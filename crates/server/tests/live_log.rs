//! What the server writes to its log while live tracks are sent and read,
//! and what it must never write there: where a glider is, or the
//! identifier of a track, which is all it takes to see where its pilot is.
//!
//! Like `log.rs`, this test has a file, and so a process, to itself: it
//! collects the log of its own thread, and the logging library decides once
//! per log statement whether anyone listens.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ErrorCode, LIVE_PATH, LIVE_POSITIONS_PATH, LiveFix, LivePositionsRequest, LiveVisibility,
};
use venti_domain::GeoPoint;
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::FixedClock;
use venti_server::live::Limits;
use venti_store::testing::TroubledStore;

mod common;

use common::{TestClock, ask, get, get_with, server, settings, sign_up};

/// Collects what the server logs while a test holds it.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A place and a height that stand nowhere else in these tests.
const LAT: &str = "47.123456";
const LON: &str = "11.654321";
const ALTITUDE_M: i32 = 1777;

fn batch(track: &str, visibility: LiveVisibility, seconds: &[i64]) -> LivePositionsRequest {
    let fixes = seconds
        .iter()
        .map(|second| LiveFix {
            time: adapters::recorded_at() + Duration::seconds(*second),
            position: GeoPoint::new(LAT.parse().unwrap(), LON.parse().unwrap()).unwrap(),
            altitude_m: ALTITUDE_M,
            ground_speed_ms: Some(9.5),
            climb_ms: Some(1.2),
            course_deg: Some(123.0),
        })
        .collect();
    LivePositionsRequest {
        id: track.to_owned(),
        visibility,
        landed: false,
        fixes,
    }
}

#[tokio::test]
async fn no_position_and_no_track_identifier_reaches_the_log() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        // Without the time of each line: its digits could by chance be
        // those of an altitude this test looks for.
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // This test runs on one thread, so everything the router does while
    // the guard lives is logged here, at the most talkative level.
    let _guard = tracing::subscriber::set_default(subscriber);

    let clock = TestClock::new();
    let live = Settings {
        live: Limits::default(),
        ..settings()
    };
    let (router, _) = server(live, &clock);
    let token = sign_up(&router, "dario").await.token;
    let annas = sign_up(&router, "anna").await.token;
    let (public, link, never) = ("a".repeat(26), "b".repeat(26), "c".repeat(26));
    let post = |token: &str, body: LivePositionsRequest| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, LIVE_POSITIONS_PATH)
                .token(&token)
                .json(&body)
                .send(&router)
                .await
        }
    };

    // Fixes for a track everyone may see and for one shared by link; the
    // same again; fixes that fill a gap.
    for (track, visibility) in [
        (&public, LiveVisibility::Public),
        (&link, LiveVisibility::Link),
    ] {
        let begun = post(&token, batch(track, visibility, &[-30, -20])).await;
        assert_eq!(begun.status, StatusCode::CREATED);
        let again = post(&token, batch(track, visibility, &[-30, -20])).await;
        assert_eq!(again.status, StatusCode::OK);
        let late = post(&token, batch(track, visibility, &[-300, -200])).await;
        assert_eq!(late.status, StatusCode::OK);
    }

    // What goes wrong when sending: a value no glider has, a clock that is
    // a day ahead, an identifier of another account's track, a body that
    // is not what the endpoint expects, no token.
    let mut nonsense = batch(&public, LiveVisibility::Public, &[-10]);
    nonsense.fixes[0].altitude_m = 31_337;
    post(&token, nonsense)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidFixes);
    post(&token, batch(&public, LiveVisibility::Public, &[86_400]))
        .await
        .error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::FixesOutOfTime);
    post(&annas, batch(&link, LiveVisibility::Public, &[-5]))
        .await
        .error(StatusCode::CONFLICT, ErrorCode::LiveTrackTaken);
    let malformed = format!(r#"{{"id":"{link}","fixes":[{{"position":{{"lat":{LAT},"lon":"#);
    let answer = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&token)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(malformed)
        .send(&router)
        .await;
    let said = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    assert!(!said.contains(&link) && !said.contains(LAT), "{said}");
    let answer = ask(Method::POST, LIVE_POSITIONS_PATH)
        .json(&batch(&never, LiveVisibility::Link, &[-5]))
        .send(&router)
        .await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

    // Reading: the list, each track, again with a validator, a track that
    // is not there, the pages, and paths that are no part of the API.
    let list = get(&router, "/v1/live?bbox=11,47,12,48").await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(list.text().contains(&public) && list.text().contains(LAT));
    for track in [&public, &link] {
        let path = format!("{LIVE_PATH}/{track}");
        let answer = get(&router, &path).await;
        assert_eq!(answer.status, StatusCode::OK);
        let tag = answer.header(header::ETAG).to_owned();
        let again = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
        assert_eq!(again.status, StatusCode::NOT_MODIFIED);
        let page = get(&router, &format!("/live/{track}")).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.text().contains("47.1235° N"));
        // A path under a track that the API does not have: the answer
        // repeats the path without the identifier.
        for stray in [
            format!("{path}/fixes"),
            format!("/v1/live/positions/{track}"),
        ] {
            let answer = get(&router, &stray).await;
            let said = answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
            assert!(said.contains("/v1/live/… is not part of the API"), "{said}");
            assert!(!said.contains(track.as_str()), "{said}");
        }
        assert_eq!(
            get(&router, &format!("/live/{track}/preview.png"))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    get(&router, &format!("{LIVE_PATH}/{never}"))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    assert_eq!(
        get(&router, &format!("/live/{never}")).await.status,
        StatusCode::NOT_FOUND
    );

    // More than an account may send in a minute.
    let many: Vec<i64> = (-3000..-2880).collect();
    let mut refused = 0;
    for _ in 0..4 {
        let answer = post(&token, batch(&public, LiveVisibility::Public, &many)).await;
        refused += usize::from(answer.status == StatusCode::TOO_MANY_REQUESTS);
    }
    assert!(refused >= 1);

    // Deleting: someone else's, one's own, again, and one that never
    // began; fixes for what is gone.
    let delete = |token: &str, track: &str| {
        let (router, token, path) = (
            router.clone(),
            token.to_owned(),
            format!("{LIVE_PATH}/{track}"),
        );
        async move {
            ask(Method::DELETE, &path)
                .token(&token)
                .send(&router)
                .await
                .status
        }
    };
    assert_eq!(delete(&annas, &link).await, StatusCode::NOT_FOUND);
    assert_eq!(delete(&token, &link).await, StatusCode::NO_CONTENT);
    assert_eq!(delete(&token, &link).await, StatusCode::NO_CONTENT);
    assert_eq!(delete(&token, &never).await, StatusCode::NO_CONTENT);
    clock.advance(Duration::minutes(2));
    post(&token, batch(&link, LiveVisibility::Link, &[60]))
        .await
        .error(StatusCode::GONE, ErrorCode::LiveTrackGone);

    // A track that ended two days ago, asked for and sent to.
    clock.advance(Duration::hours(49));
    get(&router, &format!("{LIVE_PATH}/{public}"))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    let seconds = 49 * 3600 + 60;
    post(&token, batch(&public, LiveVisibility::Public, &[seconds]))
        .await
        .error(StatusCode::GONE, ErrorCode::LiveTrackGone);

    // And a database that is away, which the log does hear about.
    let api = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(FixedClock(adapters::recorded_at())),
        [],
        settings(),
    );
    let troubled = api.router(&Cors::Any);
    for path in [
        format!("{LIVE_PATH}/{public}"),
        format!("/live/{public}"),
        "/v1/live?bbox=11,47,12,48".to_owned(),
    ] {
        let answer = get(&troubled, &path).await;
        assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    // The log did record all this: the requests by their paths, with the
    // identifier left out, and the trouble with the database.
    for expected in [
        "/v1/live/positions",
        "path=/v1/live/…}",
        "path=/live/…}",
        "path=/v1/live}",
        "the store is unavailable",
    ] {
        assert!(log.contains(expected), "the log lacks {expected:?}:\n{log}");
    }
    // It holds no identifier of a track and nothing of a position.
    for (what, secret) in [
        ("the identifier of a public track", public.as_str()),
        ("the identifier of a track shared by link", link.as_str()),
        ("an identifier that was asked for in vain", never.as_str()),
        ("a latitude", LAT),
        ("a longitude", LON),
        ("a latitude as a page shows it", "47.1235"),
        ("an altitude", "1777"),
        ("an altitude that was refused", "31337"),
        ("a course", "123.0"),
    ] {
        assert!(!log.contains(secret), "{what} is in the log:\n{log}");
    }
    // Nor the query string of the list, which says where someone looks.
    assert!(!log.contains("bbox"), "{log}");
}
