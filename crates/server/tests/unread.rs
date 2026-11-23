//! Tests of how much is new for the signed-in pilot, through the real
//! router: what the account was told and what its alarms fired after the
//! mark the pilot moves, the same on every device.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use venti_api_types::v1::{
    ACCOUNT_NOTIFICATIONS_PATH, ACCOUNT_TOKENS_PATH, ACCOUNT_UNREAD_PATH, ALERTS_PATH,
    AlertRequest, AlertRule, CommentRequest, CreateAccessTokenRequest, CreateAccessTokenResponse,
    ErrorCode, NotificationsResponse, Password, Scope, SeenRequest, SignInResponse,
    UNREAD_COUNTED_UP_TO, UnreadResponse, Visibility, WindAlert, flight_cheer_path,
    flight_comments_path, pilot_follow_path,
};
use venti_domain::{Observation, Timestamp};
use venti_server::alerts::Checks;
use venti_server::clock::Clock;
use venti_server::dev::DEMO_TOKEN;
use venti_sources::testing::assert_golden;
use venti_store::Store;

mod common;

use common::{
    Answer, PASSWORD, TestClock, ask, get, golden, recorded, sample_file, server, set_visibility,
    settings, sign_up_private, try_sign_in, upload,
};

const STATION: &str = "dwd-10min:05792";

async fn unread(router: &Router, token: &str) -> UnreadResponse {
    let answer = ask(Method::GET, ACCOUNT_UNREAD_PATH)
        .token(token)
        .send(router)
        .await;
    answer.is_private();
    answer.ok(StatusCode::OK)
}

async fn try_seen(router: &Router, token: &str, request: Option<&SeenRequest>) -> Answer {
    let ask = ask(Method::PUT, ACCOUNT_UNREAD_PATH).token(token);
    match request {
        Some(request) => ask.json(request),
        None => ask,
    }
    .send(router)
    .await
}

async fn seen(router: &Router, token: &str, request: Option<&SeenRequest>) -> UnreadResponse {
    let answer = try_seen(router, token, request).await;
    answer.is_private();
    answer.ok(StatusCode::OK)
}

fn counts(unread: UnreadResponse) -> (u32, u32, u32) {
    (unread.notifications, unread.alarms, unread.total)
}

async fn put(router: &Router, token: &str, path: &str) {
    let answer = ask(Method::PUT, path).token(token).send(router).await;
    assert_eq!(answer.status, StatusCode::OK, "PUT {path}");
}

async fn token_with(router: &Router, session: &str, scopes: &[Scope]) -> String {
    let request = CreateAccessTokenRequest {
        name: "Badge".to_owned(),
        scopes: scopes
            .iter()
            .map(|scope| scope.as_str().to_owned())
            .collect(),
        expires_at: None,
        password: Password::new(PASSWORD),
        google_id_token: None,
    };
    ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(session)
        .json(&request)
        .send(router)
        .await
        .ok::<CreateAccessTokenResponse>(StatusCode::CREATED)
        .secret
}

/// A wind alarm on [`STATION`] for anything from the north.
async fn add_alarm(router: &Router, token: &str) {
    let request = AlertRequest {
        name: "Zugspitze north".to_owned(),
        enabled: true,
        rule: AlertRule::Wind(WindAlert {
            station: STATION.parse().unwrap(),
            station_name: "Zugspitze".to_owned(),
            directions: None,
            min_kmh: Some(10),
            max_kmh: Some(20),
            gusts_max_kmh: None,
            hours: None,
        }),
    };
    let answer = ask(Method::POST, ALERTS_PATH)
        .token(token)
        .json(&request)
        .send(router)
        .await;
    assert_eq!(answer.status, StatusCode::CREATED);
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
async fn what_is_new_counts_from_the_pilots_mark_on_every_device() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let anna = sign_up_private(&router, "anna").await.token;
    let dario = sign_up_private(&router, "dario").await.token;
    let bernd = sign_up_private(&router, "bernd").await.token;
    let flight = upload(&router, &anna, &sample_file()).await.flight.id;
    set_visibility(&router, &anna, &flight, Visibility::Public).await;

    // Signed out: nothing to count, nothing to mark.
    get(&router, ACCOUNT_UNREAD_PATH)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    ask(Method::PUT, ACCOUNT_UNREAD_PATH)
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    assert_eq!(counts(unread(&router, &anna).await), (0, 0, 0));

    // A follow, a cheer and a comment: each is one more for Anna, and
    // none for whoever did it or anybody else.
    put(&router, &dario, &pilot_follow_path("anna")).await;
    assert_eq!(counts(unread(&router, &anna).await), (1, 0, 1));
    let first: NotificationsResponse = ask(Method::GET, ACCOUNT_NOTIFICATIONS_PATH)
        .token(&anna)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    clock.advance(chrono::Duration::minutes(1));
    put(&router, &dario, &flight_cheer_path(&flight)).await;
    assert_eq!(counts(unread(&router, &anna).await), (2, 0, 2));
    clock.advance(chrono::Duration::minutes(1));
    let written = ask(Method::POST, &flight_comments_path(&flight))
        .token(&dario)
        .json(&CommentRequest {
            text: "Well flown!".to_owned(),
        })
        .send(&router)
        .await;
    assert_eq!(written.status, StatusCode::CREATED);
    assert_eq!(counts(unread(&router, &anna).await), (3, 0, 3));
    assert_eq!(counts(unread(&router, &dario).await), (0, 0, 0));
    assert_eq!(counts(unread(&router, &bernd).await), (0, 0, 0));

    // An alarm that fires is one more.
    add_alarm(&router, &anna).await;
    let checks = Checks::new(store.clone(), Arc::new(clock.clone()));
    assert_eq!(checks.after_readings(&[reading(clock.now())]).await, 1);
    assert_eq!(counts(unread(&router, &anna).await), (3, 1, 4));

    // Seen up to the first page of notifications: the two after it are
    // new still; the alarms, left out, are seen up to the newest.
    let partly = seen(
        &router,
        &anna,
        Some(&SeenRequest {
            notifications_cursor: Some(first.cursor.clone()),
            alarms_cursor: None,
        }),
    )
    .await;
    assert_eq!(counts(partly), (2, 0, 2));
    // The mark does not move back.
    let back = seen(
        &router,
        &anna,
        Some(&SeenRequest {
            notifications_cursor: Some("0".to_owned()),
            alarms_cursor: Some("0".to_owned()),
        }),
    )
    .await;
    assert_eq!(counts(back), (2, 0, 2));
    // With no body, everything up to now is seen, on every device.
    assert_eq!(counts(seen(&router, &anna, None).await), (0, 0, 0));
    let phone: SignInResponse = try_sign_in(&router, "anna", PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(counts(unread(&router, &phone.token).await), (0, 0, 0));
    // What comes after the mark counts again.
    put(&router, &bernd, &pilot_follow_path("anna")).await;
    assert_eq!(counts(unread(&router, &phone.token).await), (1, 0, 1));

    // A cursor that is no cursor, and a body that does not say it is JSON.
    try_seen(
        &router,
        &anna,
        Some(&SeenRequest {
            notifications_cursor: Some("yesterday".to_owned()),
            alarms_cursor: None,
        }),
    )
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
    let untyped = ask(Method::PUT, ACCOUNT_UNREAD_PATH)
        .token(&anna)
        .header(header::CONTENT_TYPE, "text/plain")
        .bytes("{}")
        .send(&router)
        .await;
    assert_eq!(untyped.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // A program's token counts what its scopes read, and marks with
    // `social:write` only, leaving the alarms' mark to whoever reads them.
    let reader = token_with(&router, &anna, &[Scope::SocialRead]).await;
    clock.advance(chrono::Duration::hours(4));
    assert_eq!(checks.after_readings(&[reading(clock.now())]).await, 1);
    assert_eq!(counts(unread(&router, &anna).await), (1, 1, 2));
    assert_eq!(counts(unread(&router, &reader).await), (1, 0, 1));
    try_seen(&router, &reader, None)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InsufficientScope);
    let writer = token_with(&router, &anna, &[Scope::SocialRead, Scope::SocialWrite]).await;
    assert_eq!(counts(seen(&router, &writer, None).await), (0, 0, 0));
    assert_eq!(counts(unread(&router, &anna).await), (0, 1, 1));

    // After their days, lines are new no more.
    clock.advance(chrono::Duration::days(31));
    assert_eq!(counts(unread(&router, &anna).await), (0, 0, 0));
}

#[tokio::test]
async fn the_count_stops_at_its_bound() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let anna = sign_up_private(&router, "anna").await.token;
    sign_up_private(&router, "dario").await;
    let anna_id = store.account("anna").await.unwrap().unwrap().id;
    let dario_id = store.account("dario").await.unwrap().unwrap().id;
    for _ in 0..=UNREAD_COUNTED_UP_TO {
        clock.advance(chrono::Duration::seconds(1));
        assert!(
            store
                .add_notice(&anna_id, &dario_id, clock.now(), "{}")
                .await
                .unwrap()
        );
    }
    assert_eq!(
        counts(unread(&router, &anna).await),
        (UNREAD_COUNTED_UP_TO, 0, UNREAD_COUNTED_UP_TO)
    );
}

#[tokio::test]
async fn the_development_mode_counts_what_the_demo_pilot_was_told() {
    let router = recorded().await;
    let found = unread(&router, DEMO_TOKEN).await;
    assert_golden(&golden("unread.json"), &found);
    let request = SeenRequest {
        notifications_cursor: Some("1795026000000000".to_owned()),
        alarms_cursor: None,
    };
    assert_golden(&golden("seen_request.json"), &request);
    assert_eq!(
        counts(seen(&router, DEMO_TOKEN, Some(&request)).await),
        (0, 0, 0)
    );
}
