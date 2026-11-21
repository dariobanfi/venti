//! Tests of Web Push: a browser's subscription through the real router,
//! and the messages the server sends when an alarm fires, to a push
//! service of the test's own.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aes_gcm::Aes128Gcm;
use aes_gcm::aead::{Aead, KeyInit};
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::post;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hkdf::Hkdf;
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use sha2::Sha256;
use tokio::sync::mpsc;
use venti_api_types::v1::{
    ALERTS_PATH, AlertRequest, AlertRule, ErrorCode, MAX_PUSH_SUBSCRIPTIONS, PUSH_KEY_PATH,
    PUSH_SUBSCRIPTION_PATH, PushKeyResponse, PushKeys, PushMessage, PushRemoveRequest,
    PushSubscriptionRequest, WindAlert,
};
use venti_domain::{Observation, Timestamp};
use venti_server::alerts::Checks;
use venti_server::api::{Api, Cors};
use venti_server::clock::Clock;
use venti_server::push::{PushSettings, Pusher, ServerKey};
use venti_sources::testing::assert_golden;
use venti_store::{MemoryStore, PushSubscription, Store};

mod common;

use common::{TestClock, ask, golden, server, settings, sign_up};

const STATION: &str = "dwd-10min:05792";

/// The sender's key of RFC 8291's example, so that the key's answer is
/// the same every time.
const SERVER_KEY: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";

/// A browser: its key pair and authentication secret.
struct Browser {
    secret: SecretKey,
    auth: [u8; 16],
}

impl Browser {
    fn new(seed: u8) -> Self {
        let mut bytes = [seed; 32];
        bytes[0] = 1;
        Self {
            secret: SecretKey::from_slice(&bytes).unwrap(),
            auth: [seed; 16],
        }
    }

    fn public(&self) -> Vec<u8> {
        self.secret
            .public_key()
            .to_sec1_point(false)
            .as_bytes()
            .to_vec()
    }

    fn keys(&self) -> PushKeys {
        PushKeys {
            p256dh: URL_SAFE_NO_PAD.encode(self.public()),
            auth: URL_SAFE_NO_PAD.encode(self.auth),
        }
    }

    /// What a browser does with a message: RFC 8291, the other way.
    fn decrypt(&self, body: &[u8]) -> Vec<u8> {
        let (salt, rest) = body.split_at(16);
        assert_eq!(
            &rest[..5],
            &[0, 0, 16, 0, 65],
            "record size 4096, a key id of 65"
        );
        let sender = PublicKey::from_sec1_bytes(&rest[5..70]).unwrap();
        let sealed = &rest[70..];
        let shared =
            p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), sender.as_affine());
        let mut key_info = b"WebPush: info\0".to_vec();
        key_info.extend_from_slice(&self.public());
        key_info.extend_from_slice(&rest[5..70]);
        let mut ikm = [0; 32];
        Hkdf::<Sha256>::new(Some(&self.auth), shared.raw_secret_bytes())
            .expand(&key_info, &mut ikm)
            .unwrap();
        let content = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let (mut key, mut nonce) = ([0; 16], [0; 12]);
        content
            .expand(b"Content-Encoding: aes128gcm\0", &mut key)
            .unwrap();
        content
            .expand(b"Content-Encoding: nonce\0", &mut nonce)
            .unwrap();
        let mut plain = Aes128Gcm::new_from_slice(&key)
            .unwrap()
            .decrypt(&nonce.into(), sealed)
            .unwrap();
        assert_eq!(plain.pop(), Some(2), "the delimiter of the last record");
        plain
    }
}

fn subscribe_request(endpoint: &str, browser: &Browser) -> PushSubscriptionRequest {
    PushSubscriptionRequest {
        endpoint: endpoint.to_owned(),
        expiration_time: None,
        keys: browser.keys(),
    }
}

#[tokio::test]
async fn a_browser_subscribes_on_the_account_and_is_removed() {
    let clock = TestClock::new();
    let store = Arc::new(MemoryStore::new());
    let key = ServerKey::from_text(SERVER_KEY).unwrap();
    let router = Api::new(store.clone(), Arc::new(clock.clone()), [], settings())
        .with_push_key(&key)
        .router(&Cors::Origins(Vec::new()));

    // The key is open to everyone.
    let answer: PushKeyResponse = ask(Method::GET, PUSH_KEY_PATH)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(answer.public_key, key.public_text());
    assert_golden(&golden("push_key.json"), &answer);

    let pilot = sign_up(&router, "dario").await.token;
    let chrome = "https://fcm.googleapis.com/fcm/send/dXJxQ:APA91bE";
    let request = subscribe_request(chrome, &Browser::new(7));
    assert_golden(&golden("push_subscribe_request.json"), &request);
    ask(Method::POST, PUSH_SUBSCRIPTION_PATH)
        .json(&request)
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    let answer = ask(Method::POST, PUSH_SUBSCRIPTION_PATH)
        .token(&pilot)
        .json(&request)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    answer.is_private();
    let account = &store
        .push_subscriptions(&pilot_id(&store, &pilot).await)
        .await
        .unwrap();
    assert_eq!(account.len(), 1);
    assert_eq!(account[0].endpoint, chrome);

    // An address that is not of a push service, and keys that are not.
    for (endpoint, keys, field) in [
        (
            "https://example.com/push",
            Browser::new(7).keys(),
            "endpoint",
        ),
        (
            "http://fcm.googleapis.com/fcm/send/x",
            Browser::new(7).keys(),
            "endpoint",
        ),
        (
            chrome,
            PushKeys {
                p256dh: "BAAA".to_owned(),
                auth: Browser::new(7).keys().auth,
            },
            "keys.p256dh",
        ),
        (
            chrome,
            PushKeys {
                p256dh: Browser::new(7).keys().p256dh,
                auth: "AAAA".to_owned(),
            },
            "keys.auth",
        ),
    ] {
        let request = PushSubscriptionRequest {
            endpoint: endpoint.to_owned(),
            expiration_time: None,
            keys,
        };
        let message = ask(Method::POST, PUSH_SUBSCRIPTION_PATH)
            .token(&pilot)
            .json(&request)
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPushSubscription);
        assert!(message.starts_with(field), "{message}");
    }

    // Bounded; the same browser again is not another.
    for n in 1..MAX_PUSH_SUBSCRIPTIONS {
        let request = subscribe_request(&format!("{chrome}{n}"), &Browser::new(7));
        let answer = ask(Method::POST, PUSH_SUBSCRIPTION_PATH)
            .token(&pilot)
            .json(&request)
            .send(&router)
            .await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
    }
    ask(Method::POST, PUSH_SUBSCRIPTION_PATH)
        .token(&pilot)
        .json(&subscribe_request(&format!("{chrome}x"), &Browser::new(7)))
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::TooManyPushSubscriptions);
    let again = ask(Method::POST, PUSH_SUBSCRIPTION_PATH)
        .token(&pilot)
        .json(&subscribe_request(chrome, &Browser::new(8)))
        .send(&router)
        .await;
    assert_eq!(again.status, StatusCode::NO_CONTENT);

    let remove = PushRemoveRequest {
        endpoint: chrome.to_owned(),
    };
    assert_golden(&golden("push_remove_request.json"), &remove);
    for _ in 0..2 {
        let answer = ask(Method::DELETE, PUSH_SUBSCRIPTION_PATH)
            .token(&pilot)
            .json(&remove)
            .send(&router)
            .await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
    }
    let left = store
        .push_subscriptions(&pilot_id(&store, &pilot).await)
        .await
        .unwrap();
    assert_eq!(left.len(), MAX_PUSH_SUBSCRIPTIONS - 1);
    assert!(left.iter().all(|held| held.endpoint != chrome));
}

async fn pilot_id(store: &MemoryStore, _token: &str) -> String {
    store.account("dario").await.unwrap().unwrap().id
}

/// What the test's push service heard: the headers and the body of each
/// request.
type Heard = (HeaderMap, Bytes);

/// What the test's push service holds: where it tells what it heard, and
/// the statuses it is yet to answer with.
type Service = (
    mpsc::UnboundedSender<Heard>,
    Arc<Mutex<VecDeque<StatusCode>>>,
);

/// A push service on the loopback address that answers with the statuses
/// it is given, one per request, and `201` when they run out.
async fn push_service(answers: Vec<StatusCode>) -> (String, mpsc::UnboundedReceiver<Heard>) {
    let (tell, heard) = mpsc::unbounded_channel();
    let answers = Arc::new(Mutex::new(VecDeque::from(answers)));
    let service = Router::new()
        .route(
            "/push/{id}",
            post(
                |State((tell, answers)): State<Service>,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    tell.send((headers, body)).unwrap();
                    answers
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or(StatusCode::CREATED)
                },
            ),
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
        .expect("the push service heard nothing")
        .unwrap()
}

fn reading(at: Timestamp) -> Observation {
    Observation {
        wind_speed_ms: Some(4.2),
        wind_gust_ms: Some(6.1),
        wind_direction_deg: Some(10.0),
        ..Observation::empty(STATION.parse().unwrap(), at)
    }
}

#[tokio::test]
async fn a_fired_alarm_reaches_the_browser_encrypted_and_a_gone_one_is_removed() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;
    let alarm = AlertRequest {
        name: "Zugspitze north".to_owned(),
        enabled: true,
        rule: AlertRule::Wind(WindAlert {
            station: STATION.parse().unwrap(),
            station_name: "Zugspitze".to_owned(),
            directions: None,
            min_kmh: Some(10),
            max_kmh: None,
            gusts_max_kmh: Some(30),
            hours: None,
        }),
    };
    let added = ask(Method::POST, ALERTS_PATH)
        .token(&pilot)
        .json(&alarm)
        .send(&router)
        .await;
    assert_eq!(added.status, StatusCode::CREATED);

    // The push service is unwell once, then takes the first message and
    // says the subscription is gone with the second.
    let (origin, mut heard) = push_service(vec![
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::CREATED,
        StatusCode::GONE,
    ])
    .await;
    let browser = Browser::new(9);
    let account_id = pilot_id(&store, &pilot).await;
    let keys = browser.keys();
    store
        .add_push_subscription(
            &PushSubscription {
                account_id: account_id.clone(),
                endpoint: format!("{origin}/push/1"),
                p256dh: keys.p256dh,
                auth: keys.auth,
                created_at: clock.now(),
            },
            10,
        )
        .await
        .unwrap();
    let key = ServerKey::generate().unwrap();
    let mut push = PushSettings::new(
        "venti-server/test",
        Some("https://venti.example".to_owned()),
    );
    push.retries = vec![Duration::from_millis(10)];
    push.plain_http_to = vec![origin.clone()];
    let pusher = Pusher::start(store.clone(), key.clone(), Arc::new(clock.clone()), push).unwrap();
    let checks = Checks::new(store.clone(), Arc::new(clock.clone())).with_push(pusher);

    assert_eq!(checks.after_readings(&[reading(clock.now())]).await, 1);
    // Refused once, and tried again.
    let (_, first) = next(&mut heard).await;
    let (headers, body) = next(&mut heard).await;
    assert_ne!(first, body, "each try is encrypted anew");
    let header = |name: &str| headers.get(name).unwrap().to_str().unwrap().to_owned();
    assert_eq!(header("ttl"), "21600");
    assert_eq!(header("urgency"), "normal");
    assert_eq!(header("content-encoding"), "aes128gcm");
    let authorization = header("authorization");
    assert!(authorization.starts_with("vapid t="), "{authorization}");
    assert!(authorization.ends_with(&format!(", k={}", key.public_text())));
    let message: PushMessage = serde_json::from_slice(&browser.decrypt(&body)).unwrap();
    assert_eq!(message.title, "Zugspitze north");
    assert_eq!(
        message.line,
        "Zugspitze: 15 km/h from the north, gusts 22, at 11:20, as your alarm 'Zugspitze north' asks"
            .replace("11:20", &venti_api_types::v1::alert_clock(clock.now()).format("%H:%M").to_string())
    );
    assert_eq!(message.fired_at, clock.now());

    // Three hours on it fires again, and the push service says gone.
    clock.advance(chrono::Duration::hours(3));
    assert_eq!(checks.after_readings(&[reading(clock.now())]).await, 1);
    next(&mut heard).await;
    for _ in 0..100 {
        if store
            .push_subscriptions(&account_id)
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the gone subscription was not removed");
}
