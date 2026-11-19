//! Tests of live tracking through the real router: sending fixes, the
//! public tracks on a part of the map, one track, deleting, the page a link
//! opens, and what must not happen.
//!
//! The first few run against what the development mode serves and keep
//! its answers as golden files, which are also the examples in
//! `docs/api.md`. The others give a server of their own a clock they move.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ACCOUNT_PATH, DeleteAccountRequest, ErrorCode, LIVE_PATH, LIVE_POSITIONS_PATH, LiveFix,
    LivePositionsRequest, LivePositionsResponse, LiveTrackId, LiveTrackRequest, LiveTrackResponse,
    LiveTracksResponse, LiveVisibility, MAX_LIVE_BATCH_FIXES, MAX_LIVE_BODY_BYTES, MAX_LIVE_FIXES,
    MAX_LIVE_TRACKS, Password, format_time, live_page_path, live_track_path,
};
use venti_domain::{GeoPoint, Timestamp};
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::FixedClock;
use venti_server::dev::{
    DEMO_HANDLE, DEMO_LIVE_FLYING, DEMO_LIVE_LANDED, DEMO_LIVE_LINK, DEMO_NAME, DEMO_TOKEN,
};
use venti_server::live::Limits;
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;

mod common;

use common::{
    Answer, PASSWORD, PUBLIC_URL, TestClock, ask, assert_golden_text, get, get_with, golden,
    recorded, server, settings, sign_up,
};

// --- Helpers ----------------------------------------------------------------

/// The first region.
const ALPS: &str = "/v1/live?bbox=9.4,46.3,17.2,48.5";

/// The moment the clocks of these tests start at.
fn start() -> Timestamp {
    adapters::recorded_at()
}

/// An identifier made of one character.
fn id(character: char) -> String {
    character.to_string().repeat(26)
}

fn track(text: &str) -> LiveTrackId {
    text.parse().unwrap()
}

/// A fix `seconds` after the start of the test's clock (before it, when
/// negative): over the Tegernsee valley, a little further north with every
/// second.
fn fix(seconds: i64) -> LiveFix {
    LiveFix {
        time: start() + Duration::seconds(seconds),
        position: GeoPoint::new(47.66 + seconds as f64 * 1e-5, 11.8).unwrap(),
        altitude_m: 1600,
        ground_speed_ms: Some(9.5),
        climb_ms: Some(1.2),
        course_deg: Some(0.0),
    }
}

fn request(track: &str, visibility: LiveVisibility, seconds: &[i64]) -> LivePositionsRequest {
    LivePositionsRequest {
        id: track.to_owned(),
        visibility,
        landed: false,
        fixes: seconds.iter().map(|second| fix(*second)).collect(),
    }
}

fn public(track: &str, seconds: &[i64]) -> LivePositionsRequest {
    request(track, LiveVisibility::Public, seconds)
}

fn by_link(track: &str, seconds: &[i64]) -> LivePositionsRequest {
    request(track, LiveVisibility::Link, seconds)
}

async fn post(router: &Router, token: &str, request: &LivePositionsRequest) -> Answer {
    ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(token)
        .json(request)
        .send(router)
        .await
}

/// Sends fixes and expects the server to keep them.
async fn send(
    router: &Router,
    token: &str,
    request: &LivePositionsRequest,
) -> LivePositionsResponse {
    let answer = post(router, token, request).await;
    assert!(
        answer.status == StatusCode::CREATED || answer.status == StatusCode::OK,
        "{} {}",
        answer.status,
        String::from_utf8_lossy(&answer.body)
    );
    answer.is_private();
    answer.json()
}

async fn read(router: &Router, track: &str) -> LiveTrackResponse {
    get(router, &format!("{LIVE_PATH}/{track}"))
        .await
        .ok(StatusCode::OK)
}

async fn listed(router: &Router) -> Vec<String> {
    let list: LiveTracksResponse = get(router, ALPS).await.ok(StatusCode::OK);
    list.tracks
        .iter()
        .map(|track| track.id.as_str()[..1].to_owned())
        .collect()
}

async fn delete(router: &Router, token: &str, track: &str) -> Answer {
    ask(Method::DELETE, &format!("{LIVE_PATH}/{track}"))
        .token(token)
        .send(router)
        .await
}

/// The seconds after the clock's start of the fixes of an answer.
fn seconds(fixes: &[LiveFix]) -> Vec<i64> {
    fixes
        .iter()
        .map(|fix| (fix.time - start()).num_seconds())
        .collect()
}

/// A server as it runs for real where live tracking is concerned, with a
/// clock the test moves, and the token of a pilot on it.
async fn live_server_with(limits: Limits) -> (Router, TestClock, String) {
    let clock = TestClock::new();
    let settings = Settings {
        live: limits,
        ..settings()
    };
    let (router, _) = server(settings, &clock);
    let token = sign_up(&router, "dario").await.token;
    (router, clock, token)
}

async fn live_server() -> (Router, TestClock, String) {
    live_server_with(Limits::default()).await
}

// --- What the development mode serves ------------------------------------------

#[tokio::test]
async fn the_list_holds_the_public_tracks_of_the_box_and_no_other() {
    let router = recorded().await;

    let answer = get(&router, ALPS).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    // Checked before every reuse: a track that is deleted is gone at once.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    // The same whoever asks: it does not vary with a token.
    assert!(!answer.header_values(header::VARY).contains("authorization"));
    let list: LiveTracksResponse = answer.json();
    assert_eq!(list.time, start());
    assert!(!list.truncated);
    let ids: Vec<&str> = list.tracks.iter().map(|track| track.id.as_str()).collect();
    // The one in the air and the one that landed, sorted by identifier.
    // The third demo track is shared by link, and is in no list.
    assert_eq!(ids, [DEMO_LIVE_FLYING, DEMO_LIVE_LANDED]);
    assert!(!answer.text().contains(DEMO_LIVE_LINK));
    let (flying, landed) = (&list.tracks[0], &list.tracks[1]);
    for track in [flying, landed] {
        assert_eq!(track.pilot.handle.as_str(), DEMO_HANDLE);
        assert_eq!(track.pilot.display_name, DEMO_NAME);
        assert_eq!(track.visibility, LiveVisibility::Public);
        assert_eq!(track.expires_at, track.latest.time + Duration::hours(48));
    }
    assert!(!flying.landed);
    assert_eq!(flying.latest.time, start() - Duration::seconds(4));
    assert_eq!(flying.started_at, start() - Duration::seconds(1204));
    assert_eq!(flying.fixes, 1201);
    assert!(landed.landed);
    assert_eq!(landed.latest.time, start() - Duration::minutes(9));
    assert_eq!(landed.fixes, 660);
    // Near the sample flight: above the Tegernsee valley.
    for track in &list.tracks {
        let position = track.latest.position;
        assert!((47.6..47.75).contains(&position.lat()), "{position:?}");
        assert!((11.7..11.85).contains(&position.lon()), "{position:?}");
    }
    assert_golden(&golden("live.json"), &list);
    // The valley alone, which is the request the API document shows, holds
    // the same two.
    let valley: LiveTracksResponse = get(&router, "/v1/live?bbox=11.6,47.55,12.0,47.85")
        .await
        .ok(StatusCode::OK);
    assert_eq!(valley, list);

    // A box somewhere else holds nobody.
    let empty: LiveTracksResponse = get(&router, "/v1/live?bbox=5,44,6,45")
        .await
        .ok(StatusCode::OK);
    assert!(empty.tracks.is_empty() && !empty.truncated);
}

#[tokio::test]
async fn a_track_is_read_by_whoever_has_its_identifier() {
    let router = recorded().await;

    // All of it, in one answer: twenty minutes at a fix a second.
    let answer = get(&router, &live_track_path(&track(DEMO_LIVE_FLYING))).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    assert!(!answer.header_values(header::VARY).contains("authorization"));
    let whole: LiveTrackResponse = answer.json();
    assert_eq!(whole.time, start());
    assert_eq!(whole.fixes.len(), 1201);
    assert_eq!(whole.track.fixes, 1201);
    assert_eq!(whole.cursor, "1201");
    assert!(!whole.more && !whole.reset);
    assert!(whole.fixes.is_sorted_by_key(|fix| fix.time));
    assert_eq!(whole.fixes.last(), Some(&whole.track.latest));
    assert_eq!(whole.fixes[0].time, whole.track.started_at);
    // The first fix says only where; the others also how the glider moved.
    assert_eq!(whole.fixes[0].ground_speed_ms, None);
    assert!(
        whole.fixes[1..]
            .iter()
            .all(|fix| fix.ground_speed_ms.is_some())
    );

    // Only what lies after a moment: the example of the API document.
    let recent = LiveTrackRequest {
        id: track(DEMO_LIVE_FLYING),
        since: None,
        after: Some(start() - Duration::seconds(7)),
        limit: None,
    };
    let answer = get(&router, &recent.path_and_query()).await;
    let last: LiveTrackResponse = answer.ok(StatusCode::OK);
    assert_eq!(seconds(&last.fixes), [-6, -5, -4]);
    assert_eq!(last.cursor, "1201");
    assert_golden(&golden("live_track.json"), &last);

    // The track shared by link is read in the same way, by anyone who has
    // its identifier, and only kept by the one who asked.
    let answer = get(&router, &live_track_path(&track(DEMO_LIVE_LINK))).await;
    assert_eq!(answer.header(header::CACHE_CONTROL), "private, no-cache");
    let link: LiveTrackResponse = answer.ok(StatusCode::OK);
    assert_eq!(link.track.visibility, LiveVisibility::Link);
    assert_eq!(link.fixes.len(), 421);
    assert_eq!(link.track.latest.time, start() - Duration::seconds(20));
    // The answer is the same with a token, whoever's it is.
    let signed = ask(Method::GET, &live_track_path(&track(DEMO_LIVE_LINK)))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(signed.body, answer.body);

    // The one that landed.
    let landed = read(&router, DEMO_LIVE_LANDED).await;
    assert!(landed.track.landed);
    let last_minute = &landed.fixes[landed.fixes.len() - 60..];
    assert!(
        last_minute
            .iter()
            .all(|fix| fix.ground_speed_ms == Some(0.0))
    );
    assert!(last_minute.iter().all(|fix| fix.course_deg.is_none()));
}

#[tokio::test]
async fn sending_fixes_begins_a_track_and_says_what_was_kept() {
    let router = recorded().await;
    // What the API document shows: three fixes, ten seconds apart, of a
    // glider that has just left the take-off of the sample flight. The
    // first is without what a first fix cannot know.
    let example = |seconds: i64, lat: f64, lon: f64, altitude_m: i32| LiveFix {
        time: start() + Duration::seconds(seconds),
        position: GeoPoint::new(lat, lon).unwrap(),
        altitude_m,
        ground_speed_ms: Some(10.5),
        climb_ms: Some(-1.2),
        course_deg: Some(320.0),
    };
    let mut fixes = vec![
        example(-20, 47.6642, 11.7967, 1620),
        example(-10, 47.6649, 11.7958, 1608),
        example(0, 47.6656, 11.7949, 1596),
    ];
    fixes[0].ground_speed_ms = None;
    fixes[0].climb_ms = None;
    fixes[0].course_deg = None;
    let body = LivePositionsRequest {
        id: id('4'),
        visibility: LiveVisibility::Link,
        landed: false,
        fixes,
    };
    assert_golden(&golden("live_positions_request.json"), &body);

    let answer = post(&router, DEMO_TOKEN, &body).await;

    assert_eq!(answer.status, StatusCode::CREATED);
    answer.is_private();
    let kept: LivePositionsResponse = answer.json();
    assert!(kept.created);
    assert_eq!((kept.stored, kept.known), (3, 0));
    assert_eq!(
        (kept.refused_future, kept.refused_old, kept.refused_full),
        (0, 0, 0)
    );
    assert_eq!(kept.page, format!("{PUBLIC_URL}/live/{}", id('4')));
    assert_eq!(kept.track.id.as_str(), id('4'));
    assert_eq!(kept.track.pilot.handle.as_str(), DEMO_HANDLE);
    assert_eq!(kept.track.visibility, LiveVisibility::Link);
    assert_eq!(kept.track.fixes, 3);
    assert_eq!(kept.track.started_at, start() - Duration::seconds(20));
    assert_eq!(kept.track.latest, body.fixes[2]);
    assert_eq!(kept.track.expires_at, start() + Duration::hours(48));
    assert_golden(&golden("live_positions.json"), &kept);

    // It can be read at once, and its page is there.
    let read = read(&router, &id('4')).await;
    assert_eq!(read.fixes, body.fixes);
    let page = get(&router, &live_page_path(&track(&id('4')))).await;
    assert_eq!(page.status, StatusCode::OK);
    // It is listed nowhere.
    assert!(!get(&router, ALPS).await.text().contains(&id('4')));

    // The development mode's clock stands still, so it takes fixes of any
    // time: of a flight replayed from last summer, and of a clock that is
    // a year ahead. A real server refuses both.
    let mut replay = by_link(&id('5'), &[0, 1]);
    for (fix, days) in replay.fixes.iter_mut().zip([-137, 400]) {
        fix.time += Duration::days(days);
    }
    let kept = send(&router, DEMO_TOKEN, &replay).await;
    assert_eq!(
        (kept.stored, kept.refused_old, kept.refused_future),
        (2, 0, 0)
    );
    let more = send(&router, DEMO_TOKEN, &by_link(&id('5'), &[2])).await;
    assert_eq!(more.track.fixes, 3);
}

/// `docs/api.md` says that its examples are the development mode's answers.
/// This is what holds it to that for live tracks.
#[test]
fn the_documentation_shows_the_live_answers_as_they_are() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api.md");
    let docs = std::fs::read_to_string(path).unwrap();
    for name in [
        "live_positions_request.json",
        "live_positions.json",
        "live.json",
        "live_track.json",
    ] {
        let answer = std::fs::read_to_string(golden(name)).unwrap();
        assert!(
            docs.contains(answer.trim_end()),
            "docs/api.md does not show {name} as the tests keep it; \
             copy the file from crates/server/tests/golden into the document"
        );
    }
    // And the demo tracks it describes are the ones the server starts with.
    for written_down in [DEMO_LIVE_FLYING, DEMO_LIVE_LANDED, DEMO_LIVE_LINK] {
        assert!(docs.contains(written_down), "{written_down}");
    }
    // The numbers it gives are the server's.
    let limits = Limits::default();
    for number in [
        format!("{} fixes a minute", limits.fixes_per_minute),
        format!("{} live tracks a day", limits.tracks_per_day),
        "43,200 fixes".to_owned(),
        format!("{MAX_LIVE_BATCH_FIXES} fixes"),
        format!("{} kB", MAX_LIVE_BODY_BYTES / 1024),
        "5,000 fixes".to_owned(),
        "1,000 tracks".to_owned(),
        format!("{} hours", limits.retention.num_hours()),
        format!("{} hours old", limits.past.num_hours()),
        format!("{} minutes ahead", limits.future.num_minutes()),
    ] {
        assert!(
            docs.contains(&number),
            "docs/api.md does not say {number:?}"
        );
    }
    assert_eq!(limits.fixes_per_track, 43_200);
    assert_eq!((MAX_LIVE_FIXES, MAX_LIVE_TRACKS), (5000, 1000));
}

// --- Sending ----------------------------------------------------------------------

#[tokio::test]
async fn only_a_signed_in_pilot_sends_fixes_and_only_as_json() {
    let (router, _, token) = live_server().await;
    let body = public(&id('a'), &[-2, -1]);

    // No token, a token that was never issued.
    let anonymous = ask(Method::POST, LIVE_POSITIONS_PATH)
        .json(&body)
        .send(&router)
        .await;
    anonymous.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    let forged = format!("venti_{}", "7".repeat(52));
    let answer = post(&router, &forged, &body).await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

    // A body that does not say it is JSON: what a page on another site
    // could make a visitor's browser send.
    let untyped = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&token)
        .bytes(serde_json::to_vec(&body).unwrap())
        .send(&router)
        .await;
    untyped.error(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedMediaType,
    );
    // A body that is not what the endpoint expects.
    let malformed = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&token)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(
            r#"{"id":"x","fixes":[{"time":"now","position":{"lat":47.123456,"lon":11.654321}}]}"#,
        )
        .send(&router)
        .await;
    let message = malformed.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    assert!(!message.contains("47.123456") && !message.contains("11.654321"));
    // A place that is not on the globe.
    let nowhere = serde_json::to_string(&body)
        .unwrap()
        .replace("11.8", "211.8");
    let answer = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&token)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(nowhere)
        .send(&router)
        .await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    assert!(!message.contains("211.8"), "{message}");

    // None of this began a track, and the other methods are not there.
    assert_eq!(
        get(&router, &format!("{LIVE_PATH}/{}", id('a')))
            .await
            .status,
        404
    );
    let answer = get(&router, LIVE_POSITIONS_PATH).await;
    answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
    let answer = ask(Method::DELETE, LIVE_POSITIONS_PATH)
        .token(&token)
        .send(&router)
        .await;
    answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);

    // And with a token and the type it works.
    assert!(send(&router, &token, &body).await.created);
}

#[tokio::test]
async fn a_batch_that_is_not_what_the_api_asks_for_is_refused_whole() {
    let (router, _, token) = live_server().await;
    let refused = |body: LivePositionsRequest, code: ErrorCode| {
        let (router, token) = (router.clone(), token.clone());
        async move {
            let answer = post(&router, &token, &body).await;
            answer.error(StatusCode::BAD_REQUEST, code)
        }
    };

    // The identifier.
    for bad in [
        "",
        "short",
        &id('i'),
        &id('A'),
        &format!("{}/", &id('a')[..25]),
    ] {
        let message = refused(public(bad, &[-1]), ErrorCode::InvalidLiveTrackId).await;
        assert!(message.contains("26 characters"), "{message}");
    }
    // Who may see it: a setting this server does not know.
    let unknown = serde_json::to_string(&public(&id('a'), &[-1]))
        .unwrap()
        .replace("\"public\"", "\"followers\"");
    let answer = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&token)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(unknown)
        .send(&router)
        .await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidVisibility);
    assert!(message.contains("link, public"), "{message}");

    // The fixes: none, too many, newest first, a value no glider has.
    let message = refused(public(&id('a'), &[]), ErrorCode::InvalidFixes).await;
    assert!(message.contains("at least one"), "{message}");
    let many: Vec<i64> = (-121..0).collect();
    let message = refused(public(&id('a'), &many), ErrorCode::InvalidFixes).await;
    assert!(message.contains("121 fixes and may carry 120"), "{message}");
    let message = refused(public(&id('a'), &[-1, -2]), ErrorCode::InvalidFixes).await;
    assert!(message.contains("fix 2 is older"), "{message}");
    let mut high = public(&id('a'), &[-2, -1]);
    high.fixes[1].altitude_m = 31_337;
    let message = refused(high, ErrorCode::InvalidFixes).await;
    assert_eq!(
        message,
        "the altitude of fix 2 is not a value the server takes"
    );

    // Nothing of any of them was kept.
    assert_eq!(
        get(&router, &format!("{LIVE_PATH}/{}", id('a')))
            .await
            .status,
        404
    );
    // The largest batch is taken.
    let most: Vec<i64> = (-120..0).collect();
    let kept = send(&router, &token, &public(&id('a'), &most)).await;
    assert_eq!(kept.stored, 120);
}

#[tokio::test]
async fn a_body_larger_than_a_batch_can_be_is_not_read() {
    let (router, _, token) = live_server().await;
    // The largest batch with every value written out at length fits.
    let mut body = public(&id('a'), &(-120..0).collect::<Vec<i64>>());
    for fix in &mut body.fixes {
        fix.time += Duration::nanoseconds(123_456_789);
        fix.position = GeoPoint::new(-47.123_456_789_012_34, -111.123_456_789_012_34).unwrap();
        fix.altitude_m = -1000;
        fix.ground_speed_ms = Some(123.456_789_012_345_67);
        fix.climb_ms = Some(-12.345_678_901_234_567);
        fix.course_deg = Some(359.999_999_999_999_94);
    }
    let bytes = serde_json::to_vec(&body).unwrap();
    assert!(
        bytes.len() > 20_000 && bytes.len() < MAX_LIVE_BODY_BYTES,
        "{}",
        bytes.len()
    );
    assert_eq!(send(&router, &token, &body).await.stored, 120);

    // More than that is refused before it is looked at.
    let padded = format!(
        "{{\"padding\":\"{}\",{}",
        "x".repeat(MAX_LIVE_BODY_BYTES),
        &String::from_utf8(bytes).unwrap()[1..]
    );
    let answer = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&token)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(padded)
        .send(&router)
        .await;
    answer.error(StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge);
}

#[tokio::test]
async fn the_same_batch_sent_twice_changes_nothing() {
    let (router, _, token) = live_server().await;
    let body = public(&id('a'), &[-30, -20, -10]);

    let first = post(&router, &token, &body).await;
    assert_eq!(first.status, StatusCode::CREATED);
    let first: LivePositionsResponse = first.json();
    let before = get(&router, &format!("{LIVE_PATH}/{}", id('a'))).await;

    // The answer was lost; the app sends the batch again, twice.
    for _ in 0..2 {
        let again = post(&router, &token, &body).await;
        assert_eq!(again.status, StatusCode::OK);
        let again: LivePositionsResponse = again.json();
        assert!(!again.created);
        assert_eq!((again.stored, again.known), (0, 3));
        assert_eq!(again.track, first.track);
        assert_eq!(again.page, first.page);
    }

    // The track is byte for byte what it was, validator included.
    let after = get(&router, &format!("{LIVE_PATH}/{}", id('a'))).await;
    assert_eq!(after.body, before.body);
    assert_eq!(after.header(header::ETAG), before.header(header::ETAG));
    // A second fix for a second the track has a fix for is not taken
    // either: the first one stays.
    let mut other = body.clone();
    other.fixes[1].altitude_m = 2222;
    let kept = send(&router, &token, &other).await;
    assert_eq!((kept.stored, kept.known), (0, 3));
    let read = read(&router, &id('a')).await;
    assert_eq!(read.fixes, body.fixes);

    // Fractions of a second are dropped, and of two fixes in one second
    // the first counts.
    let mut fractions = public(&id('a'), &[-5, -5, -4]);
    fractions.fixes[0].time += Duration::milliseconds(200);
    fractions.fixes[1].time += Duration::milliseconds(900);
    fractions.fixes[2].time += Duration::milliseconds(999);
    let kept = send(&router, &token, &fractions).await;
    assert_eq!((kept.stored, kept.known), (2, 1));
    assert_eq!(kept.track.latest.time, start() - Duration::seconds(4));
}

#[tokio::test]
async fn fixes_from_the_future_or_from_long_ago_are_refused() {
    let (router, clock, token) = live_server().await;
    let hour = 3600;

    // Some of a batch: the rest is kept, and the answer says how many of
    // each kind were not.
    let mixed = public(
        &id('a'),
        &[
            -13 * hour,
            -12 * hour - 1,
            -12 * hour,
            -5,
            120,
            121,
            9 * hour,
        ],
    );
    let kept = send(&router, &token, &mixed).await;
    assert!(kept.created);
    assert_eq!(kept.stored, 3);
    assert_eq!((kept.refused_old, kept.refused_future), (2, 2));
    assert_eq!(
        seconds(&read(&router, &id('a')).await.fixes),
        [-12 * hour, -5, 120]
    );

    // All of a batch, as from a phone whose clock is a day ahead or a
    // year behind: refused, with the server's time and no position.
    for (days, future, old) in [(1, 2, 0), (-365, 0, 2)] {
        let mut wrong = public(&id('b'), &[0, 1]);
        for fix in &mut wrong.fixes {
            fix.time += Duration::days(days);
        }
        let answer = post(&router, &token, &wrong).await;
        let message = answer.error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::FixesOutOfTime);
        assert!(
            message.contains(&format!(
                "{future} lie in the future and {old} are more than 12"
            )),
            "{message}"
        );
        assert!(message.contains("2026-11-18T19:05:00Z"), "{message}");
        assert!(
            !message.contains("47.6") && !message.contains("11.8"),
            "{message}"
        );
    }
    assert_eq!(
        get(&router, &format!("{LIVE_PATH}/{}", id('b')))
            .await
            .status,
        404
    );

    // What was two minutes ahead is taken a little later, and what was
    // twelve hours old is too old then.
    clock.advance(Duration::minutes(5));
    let later = public(&id('b'), &[-12 * hour, 121, 300 + 121]);
    let kept = send(&router, &token, &later).await;
    assert_eq!(
        (kept.stored, kept.refused_old, kept.refused_future),
        (1, 1, 1)
    );
    assert_eq!(kept.track.latest.time, start() + Duration::seconds(121));
}

#[tokio::test]
async fn fixes_that_arrive_late_fill_the_gap_and_reach_those_who_follow() {
    let (router, _, token) = live_server().await;
    send(&router, &token, &public(&id('a'), &[-300, -299, -298])).await;
    // A viewer has everything so far.
    let seen = read(&router, &id('a')).await;
    assert_eq!(seconds(&seen.fixes), [-300, -299, -298]);
    assert_eq!(seen.cursor, "3");
    let follow = |cursor: String| {
        let router = router.clone();
        async move {
            let ask = LiveTrackRequest {
                id: track(&id('a')),
                since: Some(cursor),
                after: None,
                limit: None,
            };
            let answer: LiveTrackResponse =
                get(&router, &ask.path_and_query()).await.ok(StatusCode::OK);
            answer
        }
    };
    // Nothing new: the cursor stays.
    let nothing = follow(seen.cursor.clone()).await;
    assert!(nothing.fixes.is_empty() && !nothing.more && !nothing.reset);
    assert_eq!(nothing.cursor, "3");

    // The network was gone for four minutes. The app sends where the
    // glider is now first, and the viewer sees it at once.
    send(&router, &token, &public(&id('a'), &[-20, -10])).await;
    let now = follow(nothing.cursor).await;
    assert_eq!(seconds(&now.fixes), [-20, -10]);
    assert_eq!(now.track.latest, fix(-10));

    // Then it fills the gap, in two batches. The glider stays where it
    // is, and the viewer gets the gap although it lies before everything
    // the viewer has.
    let gap = send(&router, &token, &public(&id('a'), &[-200, -150])).await;
    assert_eq!(gap.stored, 2);
    assert_eq!(gap.track.latest, fix(-10));
    send(&router, &token, &public(&id('a'), &[-100, -50])).await;
    let filled = follow(now.cursor).await;
    assert_eq!(seconds(&filled.fixes), [-200, -150, -100, -50]);
    assert_eq!(filled.cursor, "9");
    assert_eq!(filled.track.fixes, 9);

    // Read from the beginning, the fixes come in the order of arrival;
    // sorted by time they are the track.
    let whole = read(&router, &id('a')).await;
    assert_eq!(
        seconds(&whole.fixes),
        [-300, -299, -298, -20, -10, -200, -150, -100, -50]
    );
    assert_eq!(whole.track.started_at, start() - Duration::seconds(300));

    // A page at a time.
    let page = |since: &str, limit: usize| {
        let ask = LiveTrackRequest {
            id: track(&id('a')),
            since: Some(since.to_owned()),
            after: None,
            limit: Some(limit),
        };
        let router = router.clone();
        async move {
            let answer: LiveTrackResponse =
                get(&router, &ask.path_and_query()).await.ok(StatusCode::OK);
            answer
        }
    };
    let first = page("0", 4).await;
    assert_eq!(seconds(&first.fixes), [-300, -299, -298, -20]);
    assert!(first.more);
    let second = page(&first.cursor, 4).await;
    assert_eq!(seconds(&second.fixes), [-10, -200, -150, -100]);
    assert!(second.more);
    let third = page(&second.cursor, 4).await;
    assert_eq!(seconds(&third.fixes), [-50]);
    assert!(!third.more);
    assert_eq!(third.cursor, "9");

    // Only the last minute and a half of the track, and then what arrives.
    let recent = LiveTrackRequest {
        id: track(&id('a')),
        since: None,
        after: Some(start() - Duration::seconds(100)),
        limit: None,
    };
    let answer: LiveTrackResponse = get(&router, &recent.path_and_query())
        .await
        .ok(StatusCode::OK);
    assert_eq!(seconds(&answer.fixes), [-20, -10, -50]);
    assert_eq!(answer.cursor, "9");

    // A cursor from beyond the track's last fix was not given out for it:
    // the answer starts again and says so.
    let lost = follow("5000".to_owned()).await;
    assert!(lost.reset);
    assert_eq!(lost.fixes.len(), 9);
    assert_eq!(lost.cursor, "9");
}

#[tokio::test]
async fn bad_parameters_of_a_track_are_explained() {
    let (router, _, token) = live_server().await;
    send(&router, &token, &public(&id('a'), &[-1])).await;
    let path = format!("{LIVE_PATH}/{}", id('a'));

    for (query, code) in [
        ("since=yesterday", ErrorCode::InvalidCursor),
        ("since=-1", ErrorCode::InvalidCursor),
        ("since=1.5", ErrorCode::InvalidCursor),
        ("since=99999999999999999999", ErrorCode::InvalidCursor),
        ("after=yesterday", ErrorCode::InvalidTime),
        ("after=2026-11-18", ErrorCode::InvalidTime),
        ("limit=0", ErrorCode::InvalidLimit),
        ("limit=5001", ErrorCode::InvalidLimit),
        ("limit=many", ErrorCode::InvalidLimit),
    ] {
        let answer = get(&router, &format!("{path}?{query}")).await;
        answer.error(StatusCode::BAD_REQUEST, code);
    }
    let after = format_time(start() - Duration::hours(1));
    let answer = get(&router, &format!("{path}?since=0&after={after}&limit=5000")).await;
    assert_eq!(answer.status, StatusCode::OK);

    // The list wants a box.
    for bad in [
        "/v1/live",
        "/v1/live?bbox=1,2,3",
        "/v1/live?bbox=12,48,11,47",
    ] {
        let message = get(&router, bad)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
        assert!(message.contains("west,south,east,north"), "{message}");
    }
}

// --- Who sees what --------------------------------------------------------------------

#[tokio::test]
async fn a_track_shared_by_link_is_in_no_list() {
    let (router, _, token) = live_server().await;
    send(&router, &token, &public(&id('a'), &[-10])).await;
    send(&router, &token, &by_link(&id('b'), &[-10])).await;

    assert_eq!(listed(&router).await, ["a"]);
    let list = get(&router, ALPS).await;
    assert!(!list.text().contains(&id('b')));
    // Not for its own pilot either: the list is the same for everyone.
    let own = ask(Method::GET, ALPS).token(&token).send(&router).await;
    assert_eq!(own.body, list.body);
    // By its identifier it is there, for anyone.
    assert_eq!(
        read(&router, &id('b')).await.track.visibility,
        LiveVisibility::Link
    );

    // The pilot opens it, and closes the other one, each with its next
    // fix; the list follows at once.
    send(&router, &token, &public(&id('b'), &[-5])).await;
    send(&router, &token, &by_link(&id('a'), &[-5])).await;
    assert_eq!(listed(&router).await, ["b"]);
    // Closing needs no new fix: the newest one again, with the new wish.
    send(&router, &token, &by_link(&id('b'), &[-5])).await;
    assert!(listed(&router).await.is_empty());
    // Opening does: the same fix again leaves the track as it is.
    let kept = send(&router, &token, &public(&id('b'), &[-5])).await;
    assert_eq!(kept.track.visibility, LiveVisibility::Link);
    assert!(listed(&router).await.is_empty());
}

#[tokio::test]
async fn the_list_holds_a_track_for_half_an_hour_after_its_newest_fix() {
    let (router, clock, token) = live_server().await;
    send(&router, &token, &public(&id('a'), &[-10, 0])).await;
    let mut landed = public(&id('b'), &[-10, 0]);
    landed.landed = true;
    send(&router, &token, &landed).await;
    // Elsewhere, and far away.
    let mut elsewhere = public(&id('c'), &[0]);
    elsewhere.fixes[0].position = GeoPoint::new(45.9, 6.9).unwrap();
    send(&router, &token, &elsewhere).await;

    // In the air or landed, both are on the map of their valley.
    assert_eq!(listed(&router).await, ["a", "b"]);
    let valley: LiveTracksResponse = get(&router, "/v1/live?bbox=11.0,47.0,12.5,48.0")
        .await
        .ok(StatusCode::OK);
    assert_eq!(valley.tracks.len(), 2);
    assert!(!valley.tracks[0].landed && valley.tracks[1].landed);
    let chamonix: LiveTracksResponse = get(&router, "/v1/live?bbox=6,45,7.5,46.5")
        .await
        .ok(StatusCode::OK);
    assert_eq!(chamonix.tracks.len(), 1);
    assert_eq!(chamonix.tracks[0].id.as_str(), id('c'));

    // Half an hour later they still are, and say how old they are by the
    // time the answer carries.
    clock.advance(Duration::minutes(30));
    let list: LiveTracksResponse = get(&router, ALPS).await.ok(StatusCode::OK);
    assert_eq!(list.tracks.len(), 2);
    assert_eq!(
        list.time - list.tracks[0].latest.time,
        Duration::minutes(30)
    );
    // A second more, and they have left the list; by link they are there.
    clock.advance(Duration::seconds(1));
    assert!(listed(&router).await.is_empty());
    assert_eq!(read(&router, &id('a')).await.track.fixes, 2);
    // A new fix brings a track back.
    send(&router, &token, &public(&id('a'), &[30 * 60])).await;
    assert_eq!(listed(&router).await, ["a"]);
}

#[tokio::test]
async fn a_list_is_cut_at_a_thousand_tracks_and_says_so() {
    let limits = Limits {
        tracks_per_day: u32::MAX,
        fixes_per_minute: u32::MAX,
        ..Limits::default()
    };
    let (router, _, token) = live_server_with(limits).await;
    // Identifiers that sort as their numbers do.
    let name = |n: usize| format!("{:0>26}", n.to_string().replace('1', "y"));
    let mut names: Vec<String> = (0..=MAX_LIVE_TRACKS).map(name).collect();
    for name in &names {
        send(&router, &token, &public(name, &[-1])).await;
    }
    names.sort();

    let answer = get(&router, ALPS).await;
    let list: LiveTracksResponse = answer.ok(StatusCode::OK);
    assert!(list.truncated);
    assert_eq!(list.tracks.len(), MAX_LIVE_TRACKS);
    let listed: Vec<&str> = list.tracks.iter().map(|track| track.id.as_str()).collect();
    assert_eq!(listed, names[..MAX_LIVE_TRACKS]);
    // An answer of a thousand gliders stays well under a megabyte.
    assert!(answer.body.len() < 600_000, "{} bytes", answer.body.len());
}

#[tokio::test]
async fn asking_again_costs_little_while_nothing_has_changed() {
    let (router, clock, token) = live_server().await;
    send(&router, &token, &public(&id('a'), &[-10])).await;
    let path = format!("{LIVE_PATH}/{}?since=1", id('a'));

    // One track.
    let first = get(&router, &path).await;
    assert_eq!(first.status, StatusCode::OK);
    let tag = first.header(header::ETAG).to_owned();
    let asked = |path: &str, tag: &str| {
        let (router, path, tag) = (router.clone(), path.to_owned(), tag.to_owned());
        async move { get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await }
    };
    // Seconds pass and nothing arrives: no body, the same validator.
    for _ in 0..3 {
        clock.advance(Duration::seconds(5));
        let again = asked(&path, &tag).await;
        assert_eq!(again.status, StatusCode::NOT_MODIFIED);
        assert!(again.body.is_empty());
        assert_eq!(again.header(header::ETAG), tag);
        assert_eq!(again.header(header::CACHE_CONTROL), "public, no-cache");
    }
    // Asked without the validator, the answer carries the time of now.
    let fresh: LiveTrackResponse = get(&router, &path).await.ok(StatusCode::OK);
    assert_eq!(fresh.time, start() + Duration::seconds(15));
    // A validator belongs to the question it answered.
    let other = asked(&format!("{LIVE_PATH}/{}?since=0", id('a')), &tag).await;
    assert_eq!(other.status, StatusCode::OK);

    // A fix arrives: the answer is new, and so is its validator.
    send(&router, &token, &public(&id('a'), &[10])).await;
    let changed = asked(&path, &tag).await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_ne!(changed.header(header::ETAG), tag);
    let changed: LiveTrackResponse = changed.json();
    assert_eq!(seconds(&changed.fixes), [10]);
    // So does a landing that brings no fix with it.
    let tag = get(&router, &path).await.header(header::ETAG).to_owned();
    let mut landed = public(&id('a'), &[10]);
    landed.landed = true;
    send(&router, &token, &landed).await;
    let changed = asked(&path, &tag).await;
    assert_eq!(changed.status, StatusCode::OK);
    assert!(changed.json::<LiveTrackResponse>().track.landed);

    // The list.
    let first = get(&router, ALPS).await;
    let tag = first.header(header::ETAG).to_owned();
    clock.advance(Duration::seconds(5));
    let again = asked(ALPS, &tag).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    send(&router, &token, &public(&id('a'), &[30])).await;
    assert_eq!(asked(ALPS, &tag).await.status, StatusCode::OK);
}

// --- Ending and deleting --------------------------------------------------------------

#[tokio::test]
async fn a_track_is_its_pilots_to_delete_and_takes_nothing_afterwards() {
    let (router, _, token) = live_server().await;
    let annas = sign_up(&router, "anna").await.token;
    send(&router, &token, &public(&id('a'), &[-20, -10])).await;
    send(&router, &token, &public(&id('b'), &[-20, -10])).await;
    let path = format!("{LIVE_PATH}/{}", id('a'));
    let page = live_page_path(&track(&id('a')));

    // Not without a token, and not for someone else who has the link: to
    // them it is no track of theirs, whoever may see it.
    let answer = ask(Method::DELETE, &path).send(&router).await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    let answer = delete(&router, &annas, &id('a')).await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    assert_eq!(read(&router, &id('a')).await.track.fixes, 2);

    let answer = delete(&router, &token, &id('a')).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert!(answer.body.is_empty());
    answer.is_private();

    // Gone at once, everywhere, and exactly like a track that never was.
    let gone = get(&router, &path).await;
    let never = get(&router, &format!("{LIVE_PATH}/{}", id('c'))).await;
    gone.error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    assert_eq!(gone.body, never.body);
    assert_eq!(listed(&router).await, ["b"]);
    let gone_page = get(&router, &page).await;
    assert_eq!(gone_page.status, StatusCode::NOT_FOUND);
    assert_eq!(
        gone_page.body,
        get(&router, &live_page_path(&track(&id('c')))).await.body
    );

    // Fixes that were still on their way begin nothing, from anyone.
    let late = post(&router, &token, &public(&id('a'), &[-5])).await;
    let message = late.error(StatusCode::GONE, ErrorCode::LiveTrackGone);
    assert!(message.contains("new identifier"), "{message}");
    assert!(!message.contains(&id('a')));
    let late = post(&router, &annas, &public(&id('a'), &[-5])).await;
    late.error(StatusCode::GONE, ErrorCode::LiveTrackGone);
    assert_eq!(get(&router, &path).await.status, StatusCode::NOT_FOUND);

    // Deleting again is fine for the pilot, as after a lost answer.
    assert_eq!(
        delete(&router, &token, &id('a')).await.status,
        StatusCode::NO_CONTENT
    );
    delete(&router, &annas, &id('a'))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    // Text that is no identifier is the identifier of nothing.
    for bad in ["x", "positions", &id('A')] {
        let answer = delete(&router, &token, bad).await;
        assert!(
            answer.status == StatusCode::NOT_FOUND
                || answer.status == StatusCode::METHOD_NOT_ALLOWED
        );
    }
}

#[tokio::test]
async fn a_track_deleted_before_its_first_fixes_arrive_never_begins() {
    let (router, _, token) = live_server().await;

    // The pilot switched live tracking off while the first batch was
    // still on its way through a bad network. The deletion arrives first.
    let answer = delete(&router, &token, &id('a')).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);

    let late = post(&router, &token, &public(&id('a'), &[-5])).await;
    late.error(StatusCode::GONE, ErrorCode::LiveTrackGone);
    assert_eq!(
        get(&router, &format!("{LIVE_PATH}/{}", id('a')))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert!(listed(&router).await.is_empty());
}

#[tokio::test]
async fn an_identifier_belongs_to_the_account_that_began_its_track() {
    let (router, _, token) = live_server().await;
    let annas = sign_up(&router, "anna").await.token;
    send(&router, &token, &by_link(&id('a'), &[-20, -10])).await;

    // Anna was given the link. She can look, and cannot send.
    assert_eq!(read(&router, &id('a')).await.track.fixes, 2);
    let answer = post(&router, &annas, &public(&id('a'), &[-5])).await;
    let message = answer.error(StatusCode::CONFLICT, ErrorCode::LiveTrackTaken);
    assert!(!message.contains(&id('a')));

    let after = read(&router, &id('a')).await;
    assert_eq!(after.track.fixes, 2);
    assert_eq!(after.track.visibility, LiveVisibility::Link);
    assert_eq!(after.track.pilot.handle.as_str(), "dario");
}

#[tokio::test]
async fn deleting_the_account_deletes_its_live_tracks() {
    let (router, _, token) = live_server().await;
    let annas = sign_up(&router, "anna").await.token;
    send(&router, &token, &public(&id('a'), &[-20, -10])).await;
    send(&router, &token, &by_link(&id('b'), &[-20, -10])).await;
    send(&router, &annas, &public(&id('c'), &[-20, -10])).await;

    let answer = ask(Method::DELETE, ACCOUNT_PATH)
        .token(&token)
        .json(&DeleteAccountRequest {
            password: Password::new(PASSWORD),
        })
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);

    for gone in [id('a'), id('b')] {
        let answer = get(&router, &format!("{LIVE_PATH}/{gone}")).await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
        let page = get(&router, &live_page_path(&track(&gone))).await;
        assert_eq!(page.status, StatusCode::NOT_FOUND);
    }
    // The other pilot's track is untouched.
    assert_eq!(listed(&router).await, ["c"]);
}

#[tokio::test]
async fn a_track_is_gone_two_days_after_its_newest_fix() {
    let (router, clock, token) = live_server().await;
    let kept = send(&router, &token, &public(&id('a'), &[-10, 0])).await;
    assert_eq!(kept.track.expires_at, start() + Duration::hours(48));
    let path = format!("{LIVE_PATH}/{}", id('a'));
    let page = live_page_path(&track(&id('a')));

    // A day and a half later a late batch arrives: fixes of the last twelve
    // hours are still taken, and move the end of the track's time.
    clock.advance(Duration::hours(36));
    let more = send(&router, &token, &public(&id('a'), &[25 * 3600])).await;
    assert_eq!(more.track.expires_at, start() + Duration::hours(25 + 48));

    // To the hour it is there, by its identifier and as a page.
    clock.advance(Duration::hours(25 + 48 - 36));
    assert_eq!(get(&router, &path).await.status, StatusCode::OK);
    assert_eq!(get(&router, &page).await.status, StatusCode::OK);
    // A second later it is not, exactly like a track that never was, and
    // before any cleanup has run.
    clock.advance(Duration::seconds(1));
    let gone = get(&router, &path).await;
    gone.error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    assert_eq!(
        gone.body,
        get(&router, &format!("{LIVE_PATH}/{}", id('c'))).await.body
    );
    assert_eq!(get(&router, &page).await.status, StatusCode::NOT_FOUND);
    // Its identifier is used up.
    let late = post(&router, &token, &public(&id('a'), &[(25 + 48) * 3600])).await;
    late.error(StatusCode::GONE, ErrorCode::LiveTrackGone);
    // Deleting it now is deleting what is gone.
    assert_eq!(
        delete(&router, &token, &id('a')).await.status,
        StatusCode::NO_CONTENT
    );
}

// --- Limits ---------------------------------------------------------------------------

#[tokio::test]
async fn an_account_sends_so_many_fixes_a_minute_and_no_more() {
    let (router, clock, token) = live_server().await;
    let batch = |from: i64| public(&id('a'), &(from..from + 120).collect::<Vec<i64>>());

    // Three batches of 120 are taken: the third at 240 of the 300.
    for n in 0..3 {
        let kept = send(&router, &token, &batch(-3600 + n * 120)).await;
        assert_eq!(kept.stored, 120);
    }
    // Then nothing is, not a single fix, also not one the track has.
    for body in [batch(-3000), public(&id('a'), &[-3600])] {
        let answer = post(&router, &token, &body).await;
        let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
        assert!(
            message.contains("faster than the server takes them"),
            "{message}"
        );
        assert_eq!(answer.header(header::RETRY_AFTER), "60");
    }
    assert_eq!(read(&router, &id('a')).await.track.fixes, 360);
    // Reading is not counted, and another account is not affected.
    let annas = sign_up(&router, "anna").await.token;
    assert!(
        send(&router, &annas, &public(&id('b'), &[-1]))
            .await
            .created
    );

    // What is not what the API asks for counts too.
    clock.advance(Duration::minutes(1));
    for _ in 0..3 {
        let mut nonsense = batch(-2000);
        nonsense.fixes[5].altitude_m = 99_999;
        let answer = post(&router, &token, &nonsense).await;
        answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidFixes);
    }
    let answer = post(&router, &token, &batch(-2000)).await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    // A minute later the account sends again.
    clock.advance(Duration::minutes(1));
    assert_eq!(send(&router, &token, &batch(-2000)).await.stored, 120);
}

#[tokio::test]
async fn an_account_begins_so_many_tracks_a_day_and_no_more() {
    let limits = Limits {
        tracks_per_day: 3,
        ..Limits::default()
    };
    let (router, clock, token) = live_server_with(limits).await;

    for name in ['a', 'b', 'c'] {
        assert!(
            send(&router, &token, &public(&id(name), &[-10]))
                .await
                .created
        );
    }
    let answer = post(&router, &token, &public(&id('d'), &[-10])).await;
    let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("as many live tracks today"), "{message}");
    assert_eq!(answer.header(header::RETRY_AFTER), "86400");
    assert_eq!(
        get(&router, &format!("{LIVE_PATH}/{}", id('d')))
            .await
            .status,
        404
    );

    // The tracks it has go on, and can be deleted.
    assert_eq!(
        send(&router, &token, &public(&id('a'), &[-5])).await.stored,
        1
    );
    assert_eq!(
        delete(&router, &token, &id('b')).await.status,
        StatusCode::NO_CONTENT
    );
    // Deleting makes no room, and an identifier nothing is known of cannot
    // be taken out of use either: that is a row in the database as well.
    post(&router, &token, &public(&id('d'), &[-10]))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    delete(&router, &token, &id('e'))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);

    // The next day there is room for three more, a deletion of nothing
    // among them.
    clock.advance(Duration::days(1));
    assert!(
        send(&router, &token, &public(&id('d'), &[86_390]))
            .await
            .created
    );
    assert_eq!(
        delete(&router, &token, &id('e')).await.status,
        StatusCode::NO_CONTENT
    );
    assert!(
        send(&router, &token, &public(&id('f'), &[86_390]))
            .await
            .created
    );
    post(&router, &token, &public(&id('g'), &[86_390]))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
}

#[tokio::test]
async fn a_track_holds_so_many_fixes_and_no_more() {
    let limits = Limits {
        fixes_per_track: 5,
        ..Limits::default()
    };
    let (router, _, token) = live_server_with(limits).await;
    send(&router, &token, &public(&id('a'), &[-50, -40, -30])).await;

    // One it has, four it has not, room for two: the answer says so.
    let answer = post(&router, &token, &public(&id('a'), &[-30, -20, -10, -5, -4])).await;
    assert_eq!(answer.status, StatusCode::OK);
    let kept: LivePositionsResponse = answer.json();
    assert_eq!((kept.stored, kept.known, kept.refused_full), (2, 1, 2));
    assert_eq!(kept.track.fixes, 5);
    assert_eq!(kept.track.latest, fix(-10));

    // Nothing more fits.
    let mut more = public(&id('a'), &[-3, -2]);
    more.landed = true;
    let answer = post(&router, &token, &more).await;
    let message = answer.error(StatusCode::FORBIDDEN, ErrorCode::LiveTrackFull);
    assert!(message.contains("holds 5 fixes"), "{message}");
    assert!(!read(&router, &id('a')).await.track.landed);
    // What the track holds can still be said again, and with its newest
    // fix the pilot can say that the glider landed, or close the track.
    let mut landed = by_link(&id('a'), &[-10]);
    landed.landed = true;
    let kept = send(&router, &token, &landed).await;
    assert_eq!((kept.stored, kept.known, kept.refused_full), (0, 1, 0));
    assert!(kept.track.landed);
    assert_eq!(kept.track.visibility, LiveVisibility::Link);
    // And the flight goes on under a new identifier.
    assert!(
        send(&router, &token, &public(&id('b'), &[-3, -2]))
            .await
            .created
    );
}

// --- The page a link opens ------------------------------------------------------------

#[tokio::test]
async fn a_link_opens_a_page_that_says_everything_in_words() {
    let router = recorded().await;

    let answer = get(&router, &live_page_path(&track(DEMO_LIVE_FLYING))).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(
        answer.header(header::CONTENT_TYPE),
        "text/html; charset=utf-8"
    );
    let html = answer.text();
    for part in [
        "<title>Live track of Demo Pilot · Venti</title>",
        "<h1>Demo Pilot is under way</h1>",
        "<b>Demo Pilot</b> @demo · live track",
        "No landing has been reported. The last position is 4 seconds old.",
        "<dt>Age of the position</dt><dd>4 <span>s</span></dd>",
        "<dt>Time of the position</dt><dd>19:04:56 UTC, 18 November 2026</dd>",
        "<dt>Landed</dt><dd>No landing reported</dd>",
        "<dt>Positions received</dt><dd>1201</dd>",
        "This page was made at 19:05:00 UTC and loads itself again every 15 seconds.",
        "<meta http-equiv=\"refresh\" content=\"15\">",
        &format!("<link rel=\"canonical\" href=\"{PUBLIC_URL}/live/{DEMO_LIVE_FLYING}\">"),
    ] {
        assert!(html.contains(part), "missing: {part}");
    }
    assert!(html.contains("<dt>Altitude</dt><dd>"));
    assert!(html.contains("<dt>Position</dt><dd>47."));
    assert!(html.contains("<polyline class=\"line\" points=\""));
    assert!(!html.contains("<script"));
    // The page is the same on every start of the development mode.
    assert_golden_text("live_page.html", html);

    // The one that landed says so, and the one shared by link has a page
    // like any other.
    let landed = get(&router, &live_page_path(&track(DEMO_LIVE_LANDED))).await;
    let html = landed.text();
    assert!(html.contains("<h1>Demo Pilot has landed</h1>"));
    assert!(html.contains("The last position is 9 minutes old."));
    assert!(html.contains("<dt>Landed</dt><dd>Yes, as the pilot&#39;s app reports</dd>"));
    assert!(html.contains("<dt>Ground speed</dt><dd>0 <span>km/h</span></dd>"));
    let link = get(&router, &live_page_path(&track(DEMO_LIVE_LINK))).await;
    assert_eq!(link.status, StatusCode::OK);
    assert!(link.text().contains("<dd>Whoever has this link</dd>"));

    // Without loading itself again, for whoever asks for that.
    let path = format!("{}?renew=off", live_page_path(&track(DEMO_LIVE_FLYING)));
    let still = get(&router, &path).await;
    assert!(!still.text().contains("http-equiv"));
    assert!(still.text().contains("stays as it is"));
}

#[tokio::test]
async fn the_page_is_kept_by_nobody_and_forbids_itself_everything() {
    let router = recorded().await;
    for shown in [DEMO_LIVE_FLYING, DEMO_LIVE_LINK] {
        let answer = get(&router, &live_page_path(&track(shown))).await;
        // Made anew for every visitor: it says how old a position is.
        assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
        assert!(!answer.headers.contains_key(header::ETAG));
        assert!(!answer.headers.contains_key(header::SET_COOKIE));
        let policy = answer.header(header::CONTENT_SECURITY_POLICY);
        assert!(
            policy.starts_with("default-src 'none'; style-src 'sha256-"),
            "{policy}"
        );
        assert!(!policy.contains("unsafe-inline") && !policy.contains("script"));
        assert!(policy.contains("frame-ancestors 'none'"));
        assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
        assert_eq!(answer.header(header::REFERRER_POLICY), "same-origin");
        // Not for search engines, also when everyone may see the track.
        assert_eq!(answer.header("x-robots-tag".parse().unwrap()), "noindex");
        assert!(
            answer
                .text()
                .contains(r#"<meta name="robots" content="noindex">"#)
        );
    }
    // Asked for with HEAD, as preview fetchers do first: the headers alone.
    let head = ask(Method::HEAD, &live_page_path(&track(DEMO_LIVE_FLYING)))
        .send(&router)
        .await;
    assert_eq!(head.status, StatusCode::OK);
    assert!(head.body.is_empty());
}

#[tokio::test]
async fn a_link_to_nothing_gets_a_page_that_says_so_and_no_more() {
    let router = recorded().await;
    let mut answers = Vec::new();
    for path in [
        format!("/live/{}", id('9')),
        "/live/x".to_owned(),
        "/live/positions".to_owned(),
        "/live/..%2F..%2Fetc%2Fpasswd".to_owned(),
        format!("/live/{}", id('A')),
    ] {
        let answer = get(&router, &path).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            answer.header(header::CONTENT_TYPE),
            "text/html; charset=utf-8"
        );
        assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
        assert_eq!(answer.header("x-robots-tag".parse().unwrap()), "noindex");
        assert!(answer.headers.contains_key(header::CONTENT_SECURITY_POLICY));
        assert!(answer.text().contains("<h1>No live track here</h1>"));
        answers.push(answer.body);
    }
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
}

#[tokio::test]
async fn the_page_follows_the_track_and_never_calls_silence_a_landing() {
    let (router, clock, token) = live_server().await;
    send(&router, &token, &by_link(&id('a'), &[-10, 0])).await;
    let page = live_page_path(&track(&id('a')));
    let shown = || {
        let (router, page) = (router.clone(), page.clone());
        async move { get(&router, &page).await.text().to_owned() }
    };

    assert!(shown().await.contains("<h1>Pilot dario is under way</h1>"));
    clock.advance(Duration::minutes(10));
    let html = shown().await;
    assert!(html.contains("The last position is 10 minutes old."));
    assert!(html.contains("<h1>Pilot dario is under way</h1>"));

    // Forty minutes without a position: said as that, and as nothing else.
    clock.advance(Duration::minutes(30));
    let html = shown().await;
    assert!(html.contains("<h1>Pilot dario was last heard of 40 minutes ago</h1>"));
    assert!(html.contains("<dt>Landed</dt><dd>No landing reported</dd>"));
    assert!(html.contains("It does not say what happened."));
    assert!(html.contains("<meta http-equiv=\"refresh\" content=\"60\">"));

    // The phone has a network again: the glider had landed.
    let mut landed = by_link(&id('a'), &[35 * 60, 36 * 60]);
    landed.landed = true;
    send(&router, &token, &landed).await;
    let html = shown().await;
    assert!(html.contains("<h1>Pilot dario has landed</h1>"));
    assert!(html.contains("The last position is 4 minutes old."));
    assert!(html.contains("<dt>Positions received</dt><dd>4</dd>"));
}

#[tokio::test]
async fn nothing_a_pilot_supplies_becomes_markup_on_the_page() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let hostile = "<img src=x onerror=alert(1)>\"'&";
    let request = venti_api_types::v1::CreateAccountRequest {
        handle: "mallory".to_owned(),
        display_name: hostile.to_owned(),
        password: Password::new(PASSWORD),
        invite: None,
        device: None,
    };
    let signed_up: venti_api_types::v1::SignInResponse =
        ask(Method::POST, venti_api_types::v1::ACCOUNTS_PATH)
            .json(&request)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
    send(&router, &signed_up.token, &public(&id('a'), &[-1])).await;

    let html = get(&router, &live_page_path(&track(&id('a')))).await;

    let html = html.text();
    assert!(!html.contains("<img"));
    assert!(!html.contains("onerror=alert(1)>"));
    assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;&quot;&#39;&amp;"));
}

// --- When the store is in trouble -------------------------------------------------------

#[tokio::test]
async fn a_database_that_is_down_is_said_as_that() {
    let api = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(FixedClock(start())),
        [],
        settings(),
    );
    let router = api.router(&Cors::Any);

    for path in [ALPS.to_owned(), format!("{LIVE_PATH}/{}", id('a'))] {
        let answer = get(&router, &path).await;
        let message = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
        assert!(!message.contains("connection refused"), "{message}");
    }
    // The page says it to people.
    let page = get(&router, &live_page_path(&track(&id('a')))).await;
    assert_eq!(page.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(page.header(header::RETRY_AFTER), "30");
    assert_eq!(page.header(header::CACHE_CONTROL), "no-store");
    assert!(page.text().contains("<h1>Try again in a moment</h1>"));
}
