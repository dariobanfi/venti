//! Tests of push to the phone apps and of help after landing: phones and
//! settings through the real router, and what the server sends to Apple's
//! and Google's push services, which are a service of the test's own.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use p256::SecretKey;
use p256::pkcs8::EncodePrivateKey;
use tokio::sync::mpsc;
use venti_api_types::v1::{
    DEVICES_PATH, DeviceRemoveRequest, DeviceRequest, ErrorCode, HELP_PATH, HealthResponse,
    HelpAnswerBody, HelpListResponse, HelpRequestBody, HelpResponse, MAX_HELP_NOTE_LENGTH,
    NEARBY_PATH, NearbyCircle, NearbySettings, OPEN_PLANS_PATH, OpenPlanRequest, POSITION_PATH,
    PositionRequest, PushPlatform, REPORTS_PATH, ReportReason, ReportRequest, ReportSubject,
    help_answer_path, help_path,
};
use venti_domain::GeoPoint;
use venti_server::accounts::google::dev;
use venti_server::api::{Api, Cors};
use venti_server::clock::Clock;
use venti_server::push::apns::ApnsKey;
use venti_server::push::fcm::ServiceAccount;
use venti_server::push::phones::{PhoneKeys, PhoneSettings, Phones};
use venti_sources::testing::assert_golden;
use venti_store::{MemoryStore, Store};

mod common;

use common::{TestClock, ask, golden, settings, sign_up};

/// An APNs token as Apple gives one: 32 bytes in hexadecimal.
const IPHONE: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

/// An FCM registration token of the form Google gives.
const ANDROID: &str = "dXJxQ8sKT0e:APA91bF-test_token";

/// The identifier the golden answers show in place of the random one.
const EXAMPLE_ID: &str = "4bq6x0vd5cwm2b9r8y1hzk3tns";

/// An answer with [`EXAMPLE_ID`] for its request's identifier.
fn example(mut answer: HelpResponse) -> HelpResponse {
    answer.request.id = EXAMPLE_ID.to_owned();
    answer
}

/// What the test's push services heard: the path, the headers and the
/// body of each request.
type Heard = (String, HeaderMap, Bytes);

/// What the services hold: where they tell what they heard, and the
/// statuses they are yet to answer messages with.
type Services = (
    mpsc::UnboundedSender<Heard>,
    Arc<Mutex<VecDeque<(StatusCode, &'static str)>>>,
);

async fn heard_message(
    State((tell, answers)): State<Services>,
    path: String,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    tell.send((path, headers, body)).unwrap();
    let (status, body) = answers
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or((StatusCode::OK, "{}"));
    (status, body.to_owned()).into_response()
}

/// APNs, FCM and Google's token endpoint on the loopback address. Messages
/// are answered with the statuses given, one per message, and `200` when
/// they run out; the token endpoint always grants one.
async fn push_services(
    answers: Vec<(StatusCode, &'static str)>,
) -> (String, mpsc::UnboundedReceiver<Heard>) {
    let (tell, heard) = mpsc::unbounded_channel();
    let answers = Arc::new(Mutex::new(VecDeque::from(answers)));
    let service = Router::new()
        .route(
            "/3/device/{token}",
            post(
                |state: State<Services>,
                 Path(token): Path<String>,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    heard_message(state, format!("/3/device/{token}"), headers, body).await
                },
            ),
        )
        .route(
            "/v1/projects/{project}/messages:send",
            post(
                |state: State<Services>,
                 Path(project): Path<String>,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    heard_message(
                        state,
                        format!("/v1/projects/{project}/messages:send"),
                        headers,
                        body,
                    )
                    .await
                },
            ),
        )
        .route(
            "/token",
            post(|body: String| async move {
                assert!(body.starts_with(
                    "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion="
                ));
                r#"{"access_token":"granted-access","expires_in":3600,"token_type":"Bearer"}"#
            }),
        )
        .with_state((tell, answers));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
    (origin, heard)
}

async fn next(heard: &mut mpsc::UnboundedReceiver<Heard>) -> Heard {
    tokio::time::timeout(Duration::from_secs(10), heard.recv())
        .await
        .expect("the push services heard nothing")
        .unwrap()
}

fn keys(origin: &str) -> PhoneKeys {
    let pem = SecretKey::from_slice(&[7; 32])
        .unwrap()
        .to_pkcs8_pem(Default::default())
        .unwrap();
    let account = serde_json::json!({
        "type": "service_account",
        "project_id": "venti-test",
        "client_email": "push@venti-test.iam.gserviceaccount.com",
        "private_key": dev::test_private_key_pem(),
        "token_uri": format!("{origin}/token"),
    });
    PhoneKeys {
        apns: Some(ApnsKey::from_pem(&pem, "ABC123DEFG", "TEAM456789", "fly.venti.app").unwrap()),
        fcm: Some(ServiceAccount::from_json(&account.to_string()).unwrap()),
    }
}

/// A server whose phones are told through the test's push services.
fn server_with_phones(clock: &TestClock, origin: &str) -> (Router, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let mut phone_settings = PhoneSettings::new("venti-server/test");
    phone_settings.retries = vec![Duration::from_millis(10)];
    phone_settings.apns_production = origin.to_owned();
    phone_settings.apns_sandbox = format!("{origin}/");
    phone_settings.fcm = origin.to_owned();
    phone_settings.plain_http = true;
    let phones = Phones::start(
        store.clone(),
        Arc::new(clock.clone()),
        keys(origin),
        phone_settings,
    )
    .unwrap();
    let api = Api::new(store.clone(), Arc::new(clock.clone()), [], settings()).with_phones(phones);
    (api.router(&Cors::Origins(Vec::new())), store)
}

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

async fn register(router: &Router, token: &str, platform: PushPlatform, device: &str) {
    let answer = ask(Method::POST, DEVICES_PATH)
        .token(token)
        .json(&DeviceRequest {
            platform,
            token: device.to_owned(),
            sandbox: false,
        })
        .send(router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
}

async fn may_be_asked_at(router: &Router, token: &str, at: GeoPoint) {
    let settings = NearbySettings {
        open_plans_near: None,
        may_be_asked_for_help: true,
    };
    let _: NearbySettings = ask(Method::PUT, NEARBY_PATH)
        .token(token)
        .json(&settings)
        .send(router)
        .await
        .ok(StatusCode::OK);
    let told = ask(Method::PUT, POSITION_PATH)
        .token(token)
        .json(&PositionRequest { position: at })
        .send(router)
        .await;
    assert_eq!(told.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_phone_registers_and_the_settings_start_off() {
    let clock = TestClock::new();
    let (router, store) = common::server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;

    // Without a token, and for a token of the wrong form, no.
    let request = DeviceRequest {
        platform: PushPlatform::Apns,
        token: IPHONE.to_owned(),
        sandbox: true,
    };
    assert_golden(&golden("device_request.json"), &request);
    ask(Method::POST, DEVICES_PATH)
        .json(&request)
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    for (platform, token, sandbox) in [
        (PushPlatform::Apns, "not-hex", false),
        (PushPlatform::Fcm, "a/b", false),
        (PushPlatform::Fcm, ANDROID, true),
        (PushPlatform::Apns, "", false),
    ] {
        ask(Method::POST, DEVICES_PATH)
            .token(&pilot)
            .json(&DeviceRequest {
                platform,
                token: token.to_owned(),
                sandbox,
            })
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidDevice);
    }
    for _ in 0..2 {
        let answer = ask(Method::POST, DEVICES_PATH)
            .token(&pilot)
            .json(&request)
            .send(&router)
            .await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
    }
    register(&router, &pilot, PushPlatform::Fcm, ANDROID).await;
    let id = store.account("dario").await.unwrap().unwrap().id;
    let held = store.devices_of(std::slice::from_ref(&id)).await.unwrap();
    assert_eq!(held.len(), 2);
    assert!(held.iter().any(|device| device.sandbox));

    let remove = DeviceRemoveRequest {
        platform: PushPlatform::Fcm,
        token: ANDROID.to_owned(),
    };
    assert_golden(&golden("device_remove_request.json"), &remove);
    let removed = ask(Method::DELETE, DEVICES_PATH)
        .token(&pilot)
        .json(&remove)
        .send(&router)
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(store.devices_of(&[id]).await.unwrap().len(), 1);

    // Both settings are off until the pilot turns them on; a position is
    // kept only while being asked is on.
    let off: NearbySettings = ask(Method::GET, NEARBY_PATH)
        .token(&pilot)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(off, NearbySettings::default());
    let position = PositionRequest {
        position: point(47.2694, 11.4041),
    };
    assert_golden(&golden("position_request.json"), &position);
    ask(Method::PUT, POSITION_PATH)
        .token(&pilot)
        .json(&position)
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::HelpNotOffered);
    let too_far = NearbySettings {
        open_plans_near: Some(NearbyCircle {
            position: point(47.6, 11.7),
            radius_km: 101,
        }),
        may_be_asked_for_help: true,
    };
    ask(Method::PUT, NEARBY_PATH)
        .token(&pilot)
        .json(&too_far)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidNearby);
    let on = NearbySettings {
        open_plans_near: Some(NearbyCircle {
            position: point(47.6, 11.7),
            radius_km: 30,
        }),
        may_be_asked_for_help: true,
    };
    let kept: NearbySettings = ask(Method::PUT, NEARBY_PATH)
        .token(&pilot)
        .json(&on)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(kept, on);
    assert_golden(&golden("nearby_settings.json"), &kept);
    let told = ask(Method::PUT, POSITION_PATH)
        .token(&pilot)
        .json(&position)
        .send(&router)
        .await;
    assert_eq!(told.status, StatusCode::NO_CONTENT);
    let account = store.account("dario").await.unwrap().unwrap().id;
    // Kept coarse, as the middle of its square of the grid.
    let kept = store
        .position_of(&account, clock.now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!((kept.lat(), kept.lon()), (47.275, 11.425));
    let forgot = ask(Method::DELETE, POSITION_PATH)
        .token(&pilot)
        .send(&router)
        .await;
    assert_eq!(forgot.status, StatusCode::NO_CONTENT);
    assert_eq!(
        store.position_of(&account, clock.now()).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn help_goes_to_pilots_nearby_and_only_helpers_see_where() {
    let clock = TestClock::new();
    let (origin, mut heard) = push_services(Vec::new()).await;
    let (router, store) = server_with_phones(&clock, &origin);
    let asker = sign_up(&router, "anna").await.token;
    let helper = sign_up(&router, "bert").await.token;
    let other = sign_up(&router, "cleo").await.token;
    let far = sign_up(&router, "dora").await.token;
    let stranger = sign_up(&router, "emil").await.token;

    // The health says to which phones the server sends.
    let health: HealthResponse = ask(Method::GET, "/v1/health")
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(health.phone_push.apns && health.phone_push.fcm);

    register(&router, &asker, PushPlatform::Fcm, ANDROID).await;
    register(&router, &helper, PushPlatform::Apns, IPHONE).await;
    may_be_asked_at(&router, &helper, point(47.30, 11.45)).await;
    may_be_asked_at(&router, &other, point(47.28, 11.40)).await;
    may_be_asked_at(&router, &far, point(48.14, 11.58)).await;

    let request = HelpRequestBody {
        position: point(47.2731, 11.3962),
        note: Some("Landed by the Inn at Völs, anyone driving to Innsbruck?".to_owned()),
    };
    assert_golden(&golden("help_ask_request.json"), &request);
    let too_long = HelpRequestBody {
        note: Some("x".repeat(MAX_HELP_NOTE_LENGTH + 1)),
        ..request.clone()
    };
    ask(Method::POST, HELP_PATH)
        .token(&asker)
        .json(&too_long)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidHelpRequest);
    let asked: HelpResponse = ask(Method::POST, HELP_PATH)
        .token(&asker)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let mine = &asked.request;
    assert!(mine.mine);
    assert_eq!(
        mine.asked,
        Some(2),
        "the two within reach, not the one in Munich"
    );
    assert_eq!(mine.position, Some(request.position));
    assert_golden(&golden("help_asked.json"), &example(asked.clone()));
    ask(Method::POST, HELP_PATH)
        .token(&asker)
        .json(&request)
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::HelpRequestOpen);

    // The helper's iPhone is told, through APNs, signed and addressed.
    let (path, headers, body) = next(&mut heard).await;
    assert_eq!(path, format!("/3/device/{IPHONE}"));
    let header = |name: &str| headers.get(name).unwrap().to_str().unwrap().to_owned();
    assert!(header("authorization").starts_with("bearer ey"));
    assert_eq!(header("apns-topic"), "fly.venti.app");
    assert_eq!(header("apns-push-type"), "alert");
    assert_eq!(header("apns-priority"), "10");
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload["aps"]["alert"]["title"],
        "Pilot anna has landed nearby and asks for help"
    );
    assert_eq!(payload["kind"], "help_request");
    assert_eq!(payload["id"], mine.id.as_str());

    // One who was asked reads it without where; a stranger not at all.
    let id = mine.id.clone();
    let seen: HelpResponse = ask(Method::GET, &help_path(&id))
        .token(&other)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(seen.request.position, None);
    assert_eq!(seen.request.can_help, Some(false));
    assert_eq!(seen.request.distance_km, Some(2));
    assert!(seen.request.helpers.is_empty());
    assert_golden(&golden("help_seen.json"), &example(seen.clone()));
    for reader in [&stranger, &far] {
        ask(Method::GET, &help_path(&id))
            .token(reader)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::HelpRequestNotFound);
    }
    ask(Method::PUT, &help_answer_path(&id))
        .token(&stranger)
        .json(&HelpAnswerBody { can_help: true })
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::HelpRequestNotFound);

    // "I can help": the helper sees where, and the asker's Android phone
    // is told, through FCM with the access token Google's endpoint gave.
    let answer = HelpAnswerBody { can_help: true };
    assert_golden(&golden("help_answer_request.json"), &answer);
    let helping: HelpResponse = ask(Method::PUT, &help_answer_path(&id))
        .token(&helper)
        .json(&answer)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(helping.request.position, Some(request.position));
    assert_eq!(helping.request.can_help, Some(true));
    let (path, headers, body) = next(&mut heard).await;
    assert_eq!(path, "/v1/projects/venti-test/messages:send");
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer granted-access"
    );
    let message: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(message["message"]["token"], ANDROID);
    assert_eq!(
        message["message"]["notification"]["title"],
        "Pilot bert can help"
    );
    assert_eq!(message["message"]["data"]["kind"], "help_answer");
    assert_eq!(message["message"]["android"]["priority"], "HIGH");

    let list: HelpListResponse = ask(Method::GET, HELP_PATH)
        .token(&asker)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(list.mine.len(), 1);
    assert_eq!(list.mine[0].helpers[0].handle.as_str(), "bert");
    assert!(list.asked.is_empty());
    let asked_list: HelpListResponse = ask(Method::GET, HELP_PATH)
        .token(&other)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(asked_list.asked.len(), 1);

    // One who was asked may report it; the note goes along.
    let report = ReportRequest {
        subject: ReportSubject::HelpRequest(id.clone()),
        reason: ReportReason::Offensive,
        text: None,
    };
    assert_golden(
        &golden("report_help_request.json"),
        &ReportRequest {
            subject: ReportSubject::HelpRequest(EXAMPLE_ID.to_owned()),
            ..report.clone()
        },
    );
    let reported = ask(Method::POST, REPORTS_PATH)
        .token(&other)
        .json(&report)
        .send(&router)
        .await;
    assert_eq!(reported.status, StatusCode::CREATED, "{:?}", reported.body);
    ask(Method::POST, REPORTS_PATH)
        .token(&stranger)
        .json(&report)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::HelpRequestNotFound);

    // Closed by its pilot, it takes no more answers and shows where to
    // nobody but its pilot.
    ask(Method::DELETE, &help_path(&id))
        .token(&helper)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::HelpRequestNotFound);
    let closed = ask(Method::DELETE, &help_path(&id))
        .token(&asker)
        .send(&router)
        .await;
    assert_eq!(closed.status, StatusCode::NO_CONTENT);
    ask(Method::PUT, &help_answer_path(&id))
        .token(&other)
        .json(&HelpAnswerBody { can_help: true })
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::HelpRequestEnded);
    let after: HelpResponse = ask(Method::GET, &help_path(&id))
        .token(&helper)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(after.request.position, None);
    assert!(after.request.closed_at.is_some());

    // Three a day at most.
    for _ in 0..2 {
        let again: HelpResponse = ask(Method::POST, HELP_PATH)
            .token(&asker)
            .json(&request)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        let closed = ask(Method::DELETE, &help_path(&again.request.id))
            .token(&asker)
            .send(&router)
            .await;
        assert_eq!(closed.status, StatusCode::NO_CONTENT);
    }
    ask(Method::POST, HELP_PATH)
        .token(&asker)
        .json(&request)
        .send(&router)
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    drop(store);
}

#[tokio::test]
async fn a_new_open_plan_tells_the_pilots_whose_circle_holds_it_and_a_gone_phone_goes() {
    let clock = TestClock::new();
    // FCM says the first phone it is sent to is gone.
    let (origin, mut heard) = push_services(vec![(
        StatusCode::NOT_FOUND,
        r#"{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#,
    )])
    .await;
    let (router, store) = server_with_phones(&clock, &origin);
    let author = sign_up(&router, "anna").await.token;
    let watcher = sign_up(&router, "bert").await.token;
    let elsewhere = sign_up(&router, "cleo").await.token;
    register(&router, &watcher, PushPlatform::Fcm, ANDROID).await;
    register(&router, &elsewhere, PushPlatform::Apns, IPHONE).await;
    register(
        &router,
        &author,
        PushPlatform::Apns,
        &IPHONE.replace('a', "b"),
    )
    .await;
    for (token, at) in [
        (&watcher, point(47.6, 11.7)),
        (&elsewhere, point(46.5, 7.9)),
        (&author, point(47.6, 11.7)),
    ] {
        let settings = NearbySettings {
            open_plans_near: Some(NearbyCircle {
                position: at,
                radius_km: 30,
            }),
            may_be_asked_for_help: false,
        };
        let _: NearbySettings = ask(Method::PUT, NEARBY_PATH)
            .token(token)
            .json(&settings)
            .send(&router)
            .await
            .ok(StatusCode::OK);
    }
    let plan = OpenPlanRequest {
        day: chrono::NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(),
        place: "Wallberg".to_owned(),
        position: point(47.65, 11.79),
        meet_at: None,
        seats: Some(2),
        note: None,
    };
    let posted = ask(Method::POST, OPEN_PLANS_PATH)
        .token(&author)
        .json(&plan)
        .send(&router)
        .await;
    assert_eq!(
        posted.status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&posted.body)
    );

    // Only the watcher's phone: not the author's, not the one in the
    // Bernese Oberland.
    let (path, _, body) = next(&mut heard).await;
    assert_eq!(path, "/v1/projects/venti-test/messages:send");
    let message: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        message["message"]["notification"]["title"],
        "Open plan: Saturday 21 November at Wallberg"
    );
    assert_eq!(
        message["message"]["notification"]["body"],
        "Pilot anna has 2 seats free"
    );
    assert_eq!(message["message"]["data"]["kind"], "open_plan");
    assert_eq!(message["message"]["android"]["priority"], "NORMAL");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(heard.try_recv().is_err(), "nobody else was told");

    // FCM said the phone is gone: it is removed.
    let id = store.account("bert").await.unwrap().unwrap().id;
    for _ in 0..100 {
        if store
            .devices_of(std::slice::from_ref(&id))
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the gone phone was not removed");
}
