//! The list of everyone's public flights, through the real router: what it
//! holds, how it is narrowed, ordered and paged, and what it must never
//! show: a flight that is private or shared by link, by any way of asking.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::{NaiveDate, TimeZone, Utc};
use venti_api_types::v1::{
    ErrorCode, Flight, FlightId, FlightSort, FlightsResponse, PUBLIC_FLIGHTS_PATH,
    PublicFlightsRequest, Visibility, flight_path,
};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::synthetic_flight;
use venti_server::api::{Api, Cors};
use venti_server::dev::{DEMO_HANDLE, DEMO_PILOTS, DEMO_TOKEN};
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;

mod common;

use common::{
    Answer, TestClock, ask, get, get_with, golden, recorded, server, set_visibility, settings,
    sign_up, upload,
};

// --- Helpers ----------------------------------------------------------------

fn open_server() -> Router {
    server(settings(), &TestClock::new()).0
}

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

fn midnight(month: u32, day: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0).unwrap()
}

/// The made-up sample flight on another day, moved `east` degrees of
/// longitude from where it takes off (the Wallberg, at 11.7967° E), and cut
/// to its first `seconds` if given. It starts at 10:30 UTC.
fn flight_file(date: NaiveDate, east: f64, seconds: Option<usize>) -> Vec<u8> {
    let original = synthetic_flight();
    let moved_by = date - original.date;
    let mut fixes = original.fixes;
    if let Some(seconds) = seconds {
        fixes.truncate(seconds + 1);
    }
    for fix in &mut fixes {
        fix.time += moved_by;
        fix.position = GeoPoint::new(fix.position.lat(), fix.position.lon() + east).unwrap();
    }
    let file = IgcFile {
        date,
        fixes,
        ..original
    };
    igc::write(&file).into_bytes()
}

/// Uploads a flight and sets who may see it.
async fn fly(router: &Router, token: &str, file: &[u8], visibility: Visibility) -> Flight {
    let flight = upload(router, token, file).await.flight;
    if visibility != Visibility::Private {
        set_visibility(router, token, &flight.id, visibility).await;
    }
    flight
}

async fn list(router: &Router, request: &PublicFlightsRequest) -> FlightsResponse {
    get(router, &request.path_and_query())
        .await
        .ok(StatusCode::OK)
}

fn everything() -> PublicFlightsRequest {
    PublicFlightsRequest::default()
}

fn ids(flights: &[Flight]) -> Vec<&FlightId> {
    flights.iter().map(|flight| &flight.id).collect()
}

/// Who flew each flight and when it started.
fn rows(flights: &[Flight]) -> Vec<(String, String)> {
    flights
        .iter()
        .map(|flight| {
            (
                flight.pilot.handle.to_string(),
                flight.start.format("%m-%d %H:%M").to_string(),
            )
        })
        .collect()
}

fn row(handle: &str, start: &str) -> (String, String) {
    (handle.to_owned(), start.to_owned())
}

/// Everything about an answer that a client can observe.
fn observable(answer: &Answer) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
    let mut headers: Vec<(String, String)> = answer
        .headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        .collect();
    headers.sort();
    (answer.status, headers, answer.body.clone())
}

/// The valley of the Tegernsee, where the sample flight is flown.
fn tegernsee() -> BoundingBox {
    BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap()
}

/// An answer with the outlines cut to two points: short enough to stand in
/// the documentation.
fn example(mut answer: FlightsResponse) -> FlightsResponse {
    for flight in &mut answer.flights {
        flight.outline.truncate(2);
    }
    answer
}

// --- The development mode -----------------------------------------------------

#[tokio::test]
async fn the_development_mode_lists_five_public_flights_of_three_pilots() {
    let router = recorded().await;
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);

    let all = list(&router, &everything()).await;
    assert_eq!(
        rows(&all.flights),
        [
            row(jonas, "09-06 13:00"),
            row(mara, "08-16 12:05"),
            row(jonas, "08-16 10:00"),
            row(mara, "08-15 11:10"),
            row(DEMO_HANDLE, "08-15 10:30"),
        ]
    );
    assert_eq!(all.next, None);
    assert_eq!(all.flights[1].pilot.display_name, "Mara Muster");
    assert_eq!(all.flights[0].pilot.display_name, "Jonas Beispiel");
    for flight in &all.flights {
        assert_eq!(flight.visibility, Visibility::Public);
        // Every one is the sample flight, from the same take-off.
        let takeoff = flight.takeoff.unwrap().position;
        assert!(tegernsee().contains(takeoff), "{takeoff:?}");
        assert!(flight.score.is_some());
        // Each row is the flight as its own address shows it.
        let one: venti_api_types::v1::FlightResponse = get(&router, &flight_path(&flight.id))
            .await
            .ok(StatusCode::OK);
        assert_eq!(&one.flight, flight);
    }
    // Nine flights exist in this mode. The demo account sees its own three
    // in its logbook, and nobody sees more than the five here.
    let mine: FlightsResponse = ask(Method::GET, "/v1/flights")
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(mine.flights.len(), 3);
    let with_a_token: FlightsResponse = ask(Method::GET, PUBLIC_FLIGHTS_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(with_a_token, all);

    // The examples of the documentation: the first page of two, and the
    // best flights of one day.
    let first = list(
        &router,
        &PublicFlightsRequest {
            limit: Some(2),
            ..everything()
        },
    )
    .await;
    assert_eq!(ids(&first.flights), ids(&all.flights[..2]));
    assert!(first.next.is_some());
    assert_golden(&golden("public_flights.json"), &example(first));

    let the_15th = PublicFlightsRequest {
        from: Some(midnight(8, 15)),
        to: Some(midnight(8, 16)),
        sort: Some(FlightSort::Score),
        ..everything()
    };
    assert_eq!(
        the_15th.path_and_query(),
        "/v1/flights/public?from=2026-08-15T00:00:00Z&to=2026-08-16T00:00:00Z&sort=score"
    );
    let best = list(&router, &the_15th).await;
    // The demo account's flight is the whole sample flight; the other is
    // its first 25 minutes.
    assert_eq!(
        rows(&best.flights),
        [row(DEMO_HANDLE, "08-15 10:30"), row(mara, "08-15 11:10")]
    );
    assert_eq!(best.next, None);
    assert_golden(&golden("public_flights_best.json"), &example(best));

    // Every flight of this mode took off in the same valley: a box either
    // holds all five or none.
    let here = PublicFlightsRequest {
        bbox: Some(tegernsee()),
        ..everything()
    };
    assert_eq!(list(&router, &here).await, all);
    let elsewhere = PublicFlightsRequest {
        bbox: Some(BoundingBox::new(46.0, 9.0, 47.0, 10.0).unwrap()),
        ..everything()
    };
    assert!(list(&router, &elsewhere).await.flights.is_empty());
}

#[tokio::test]
async fn the_list_is_read_page_by_page_in_either_order() {
    let router = recorded().await;
    let all = list(&router, &everything()).await.flights;
    assert_eq!(all.len(), 5);

    for sort in [FlightSort::Newest, FlightSort::Score] {
        let mut expected = all.clone();
        if sort == FlightSort::Score {
            // Most points first; among equal points the larger address.
            expected.sort_by(|a, b| {
                let (a, b) = (
                    (a.score.unwrap().points, &a.id),
                    (b.score.unwrap().points, &b.id),
                );
                b.partial_cmp(&a).unwrap()
            });
        }
        for size in [1, 2, 5, 100] {
            let mut request = PublicFlightsRequest {
                sort: Some(sort),
                limit: Some(size),
                ..everything()
            };
            let mut seen = Vec::new();
            loop {
                let page = list(&router, &request).await;
                assert!(page.flights.len() <= size);
                seen.extend(page.flights);
                match page.next {
                    Some(next) => request.after = Some(next),
                    None => break,
                }
            }
            assert_eq!(ids(&seen), ids(&expected), "{sort:?}, pages of {size}");
        }
    }
    // Without a limit a page holds fifty; the order is the newest first.
    let by_default = get(&router, PUBLIC_FLIGHTS_PATH).await;
    let named = get(&router, "/v1/flights/public?sort=newest&limit=50").await;
    assert_eq!(by_default.body, named.body);

    // A cursor past the end of the list gives an empty page, not an error.
    let past = "/v1/flights/public?after=0_0123456789abcdefghjkmnpqrs";
    let beyond: FlightsResponse = get(&router, past).await.ok(StatusCode::OK);
    assert!(beyond.flights.is_empty() && beyond.next.is_none());
}

// --- Narrowing the list ---------------------------------------------------------

#[tokio::test]
async fn the_list_is_narrowed_to_a_box_and_to_a_span_of_time() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let public = Visibility::Public;
    // 1 July from the Wallberg; 2 July from 0.6° further east, near Kössen;
    // 2 July from the Wallberg, by someone else; 3 July from the Wallberg.
    let first = fly(&router, &dario, &flight_file(day(7, 1), 0.0, None), public).await;
    let east = fly(
        &router,
        &dario,
        &flight_file(day(7, 2), 0.6, Some(1200)),
        public,
    )
    .await;
    let second = fly(
        &router,
        &anna,
        &flight_file(day(7, 2), 0.0, Some(900)),
        public,
    )
    .await;
    let third = fly(&router, &anna, &flight_file(day(7, 3), 0.0, None), public).await;
    // What is not public, in the same place on the same days.
    fly(
        &router,
        &anna,
        &flight_file(day(7, 2), 0.0, None),
        Visibility::Private,
    )
    .await;
    fly(
        &router,
        &dario,
        &flight_file(day(7, 3), 0.0, Some(600)),
        Visibility::Unlisted,
    )
    .await;

    let all = list(&router, &everything()).await.flights;
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].id, third.id);
    assert_eq!(all[3].id, first.id);

    // By where a flight took off.
    let here = PublicFlightsRequest {
        bbox: Some(tegernsee()),
        ..everything()
    };
    let found = list(&router, &here).await.flights;
    assert_eq!(ids(&found), [&third.id, &second.id, &first.id]);
    let near_koessen = PublicFlightsRequest {
        bbox: Some(BoundingBox::new(47.5, 12.2, 47.9, 12.6).unwrap()),
        ..everything()
    };
    assert_eq!(
        near_koessen.path_and_query(),
        "/v1/flights/public?bbox=12.2,47.5,12.6,47.9"
    );
    let found = list(&router, &near_koessen).await.flights;
    assert_eq!(ids(&found), [&east.id]);
    let takeoff = found[0].takeoff.unwrap().position;
    assert!((takeoff.lon() - 12.3967).abs() < 1e-4, "{takeoff:?}");

    // By when it started: a day is its first moment and the next day's.
    let the_2nd = PublicFlightsRequest {
        from: Some(midnight(7, 2)),
        to: Some(midnight(7, 3)),
        ..everything()
    };
    let found = list(&router, &the_2nd).await.flights;
    let mut expected = vec![&east.id, &second.id];
    expected.sort_by(|a, b| b.cmp(a));
    assert_eq!(ids(&found), expected, "both start in the same second");
    // The same day named in another offset, as a phone in Munich would.
    let local =
        "/v1/flights/public?from=2026-07-02T02:00:00%2B02:00&to=2026-07-03T02:00:00%2B02:00";
    let found: FlightsResponse = get(&router, local).await.ok(StatusCode::OK);
    assert_eq!(ids(&found.flights), expected);
    // Either end alone, and a span that ends where a flight starts.
    let since = PublicFlightsRequest {
        from: Some(midnight(7, 3)),
        ..everything()
    };
    assert_eq!(ids(&list(&router, &since).await.flights), [&third.id]);
    let until = PublicFlightsRequest {
        to: Some(third.start),
        ..everything()
    };
    assert_eq!(list(&router, &until).await.flights.len(), 3);
    let from_its_start = PublicFlightsRequest {
        from: Some(third.start),
        ..everything()
    };
    let found = list(&router, &from_its_start).await.flights;
    assert_eq!(ids(&found), [&third.id]);

    // Both together, and with the best first.
    let here_on_the_2nd = PublicFlightsRequest {
        bbox: Some(tegernsee()),
        ..the_2nd.clone()
    };
    let found = list(&router, &here_on_the_2nd).await.flights;
    assert_eq!(ids(&found), [&second.id]);
    let best_of_the_2nd = PublicFlightsRequest {
        sort: Some(FlightSort::Score),
        ..the_2nd
    };
    let found = list(&router, &best_of_the_2nd).await.flights;
    assert_eq!(found.len(), 2);
    let points: Vec<f64> = found.iter().map(|f| f.score.unwrap().points).collect();
    assert!(points[0] > points[1], "{points:?}");
    // Twenty minutes of the sample flight score more than fifteen.
    assert_eq!(ids(&found), [&east.id, &second.id]);
}

// --- What the list must never show ------------------------------------------------

/// Every way of asking for a page that these tests know, for flights
/// around the Tegernsee on 2 July 2026. `after` holds cursors to try.
fn every_question(after: &[String]) -> Vec<String> {
    let bbox = "bbox=11.6,47.55,12,47.85";
    let the_day = "from=2026-07-02T00:00:00Z&to=2026-07-03T00:00:00Z";
    let mut questions = Vec::new();
    for narrowed in ["", bbox, the_day, &format!("{bbox}&{the_day}")] {
        for sort in ["", "sort=newest", "sort=score"] {
            for limit in ["", "limit=1", "limit=100"] {
                let mut cursors = vec![String::new()];
                cursors.extend(after.iter().map(|cursor| format!("after={cursor}")));
                for cursor in cursors {
                    let parts: Vec<&str> = [narrowed, sort, limit, &cursor]
                        .into_iter()
                        .filter(|part| !part.is_empty())
                        .collect();
                    let query = match parts.is_empty() {
                        true => String::new(),
                        false => format!("?{}", parts.join("&")),
                    };
                    questions.push(format!("{PUBLIC_FLIGHTS_PATH}{query}"));
                }
            }
        }
    }
    questions
}

/// The cursors that would lead to a flight if it were in a list: the place
/// of the flight itself and the place just above it, in both orders.
fn cursors_around(flight: &Flight) -> Vec<String> {
    let micros = flight.start.timestamp_micros();
    let points = flight.score.map_or(0.0, |score| score.points);
    let above = format!("{}z", &flight.id.as_str()[..25]);
    vec![
        format!("{micros}_{}", flight.id),
        format!("{micros}_{above}"),
        format!("p{points}_{}", flight.id),
        format!("p{points}_{above}"),
        // Far above everything.
        format!("{}_{above}", micros + 1_000_000_000_000_000),
        format!("p{}_{above}", points + 1e9),
    ]
}

#[tokio::test]
async fn a_server_with_nothing_public_answers_like_a_server_with_nothing() {
    let empty = open_server();
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    // Flights that are private, and flights for those with a link: from
    // the place and on the day that will be asked for, some of them long.
    let mut hidden = Vec::new();
    for (token, seconds, visibility) in [
        (&dario, None, Visibility::Private),
        (&dario, Some(1200), Visibility::Unlisted),
        (&anna, Some(900), Visibility::Private),
        (&anna, Some(1500), Visibility::Unlisted),
    ] {
        let file = flight_file(day(7, 2), 0.0, seconds);
        hidden.push(fly(&router, token, &file, visibility).await);
    }
    // One of them was public for a moment, and its place was noted then.
    set_visibility(&router, &dario, &hidden[0].id, Visibility::Public).await;
    let noted = list(&router, &everything()).await;
    assert_eq!(ids(&noted.flights), [&hidden[0].id]);
    set_visibility(&router, &dario, &hidden[0].id, Visibility::Private).await;

    let cursors: Vec<String> = hidden.iter().flat_map(cursors_around).collect();
    let questions = every_question(&cursors);
    assert!(questions.len() > 800, "{} questions", questions.len());
    for question in &questions {
        let here = get(&router, question).await;
        let nowhere = get(&empty, question).await;
        // Status, every header and the body: nothing tells the two apart,
        // also not the validator, which is a fingerprint of the body.
        assert_eq!(observable(&here), observable(&nowhere), "{question}");
        // Nor does it help to be the pilot of the flights.
        for token in [&dario, &anna] {
            let own = ask(Method::GET, question).token(token).send(&router).await;
            assert_eq!(observable(&own), observable(&nowhere), "{question}");
        }
    }
    let nothing: FlightsResponse = get(&router, PUBLIC_FLIGHTS_PATH).await.ok(StatusCode::OK);
    assert!(nothing.flights.is_empty() && nothing.next.is_none());
    // No flight, no cursor: the terms such a list is given under, and
    // nothing else.
    let text = get(&router, PUBLIC_FLIGHTS_PATH).await;
    let text = text.text();
    assert!(text.starts_with(r#"{"flights":[],"terms":{"#), "{text}");
    assert!(!text.contains("next"), "{text}");
}

#[tokio::test]
async fn beside_a_public_flight_the_others_stay_out_of_every_answer() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    // One public flight of medium length, with flights on either side of
    // it in both orders that are not public.
    let file = |seconds| flight_file(day(7, 2), 0.0, seconds);
    let shown = fly(&router, &dario, &file(Some(1200)), Visibility::Public).await;
    let mut hidden = Vec::new();
    for (token, seconds, visibility) in [
        (&anna, None, Visibility::Private),
        (&anna, Some(1500), Visibility::Unlisted),
        (&anna, Some(900), Visibility::Private),
        (&dario, Some(600), Visibility::Unlisted),
    ] {
        hidden.push(fly(&router, token, &file(seconds), visibility).await);
    }
    let later = flight_file(day(7, 9), 0.0, None);
    hidden.push(fly(&router, &dario, &later, Visibility::Unlisted).await);

    let mut cursors: Vec<String> = hidden.iter().flat_map(cursors_around).collect();
    cursors.extend(cursors_around(&shown));
    let mut found = 0;
    for question in every_question(&cursors) {
        for token in [None, Some(&dario), Some(&anna)] {
            let mut request = ask(Method::GET, &question);
            if let Some(token) = token {
                request = request.token(token);
            }
            let answer = request.send(&router).await;
            // Nothing of a hidden flight is in any answer, also not in the
            // refusal of a cursor that names one.
            for flight in &hidden {
                assert!(!answer.text().contains(flight.id.as_str()), "{question}");
                assert!(!answer.text().contains(&flight.identity), "{question}");
            }
            if answer.status == StatusCode::BAD_REQUEST {
                // The cursor of one order, tried with the other.
                answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
                continue;
            }
            let page: FlightsResponse = answer.ok(StatusCode::OK);
            assert!(
                page.flights.iter().all(|flight| flight.id == shown.id),
                "{question} showed {:?}",
                ids(&page.flights)
            );
            // A page of one that is followed by nothing says so: a cursor
            // to a page that is not there would count what is hidden.
            assert_eq!(page.next, None, "{question}");
            found += page.flights.len();
        }
    }
    assert!(found > 0, "the public flight is found at all");
}

// --- Who may keep an answer ---------------------------------------------------------

#[tokio::test]
async fn the_list_may_be_kept_and_is_checked_before_every_reuse() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let flight = fly(
        &router,
        &dario,
        &flight_file(day(7, 1), 0.0, None),
        Visibility::Public,
    )
    .await;

    let answer = get(&router, PUBLIC_FLIGHTS_PATH).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    // The same for everyone; a cache may keep it and has to ask before it
    // hands it out again.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    assert!(!answer.headers.contains_key(header::SET_COOKIE));
    let tag = answer.header(header::ETAG).to_owned();
    let asked_again = [(header::IF_NONE_MATCH, tag.as_str())];
    let again = get_with(&router, PUBLIC_FLIGHTS_PATH, &asked_again).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    assert_eq!(again.header(header::ETAG), tag);
    assert_eq!(again.header(header::CACHE_CONTROL), "public, no-cache");
    // With a token the answer is the same in every part.
    let own = ask(Method::GET, PUBLIC_FLIGHTS_PATH)
        .token(&dario)
        .send(&router)
        .await;
    assert_eq!(observable(&own), observable(&answer));

    // A flight that is made private is gone with the next question, also
    // for whoever kept the list.
    set_visibility(&router, &dario, &flight.id, Visibility::Private).await;
    let after = get_with(&router, PUBLIC_FLIGHTS_PATH, &asked_again).await;
    let page: FlightsResponse = after.ok(StatusCode::OK);
    assert!(page.flights.is_empty());
    assert_ne!(after.header(header::ETAG), tag);
}

// --- Questions that cannot be answered ------------------------------------------------

#[tokio::test]
async fn a_question_that_cannot_be_read_is_explained() {
    let router = recorded().await;
    let refused = |query: &'static str, code: ErrorCode| {
        let router = router.clone();
        async move {
            let answer = get(&router, &format!("{PUBLIC_FLIGHTS_PATH}?{query}")).await;
            answer.error(StatusCode::BAD_REQUEST, code)
        }
    };

    for bbox in [
        "bbox=",
        "bbox=11.6,47.55,12",
        "bbox=11.6,47.55,12,north",
        "bbox=11.6,47.85,12,47.55",
        "bbox=11.6,47.55,12,91",
    ] {
        let said = refused(bbox, ErrorCode::InvalidBbox).await;
        assert!(said.contains("west,south,east,north"), "{said}");
    }
    for time in [
        "from=yesterday",
        "to=2026-08-15",
        "from=",
        "from=2026-08-15T00:00:00Z&to=1723680000",
        // A plus that was not escaped arrives as a space.
        "from=2026-08-15T00:00:00+02:00",
    ] {
        refused(time, ErrorCode::InvalidTime).await;
    }
    let said = refused("to=soon", ErrorCode::InvalidTime).await;
    assert!(said.starts_with("to: "), "{said}");
    for span in [
        "from=2026-08-16T00:00:00Z&to=2026-08-15T00:00:00Z",
        "from=2026-08-15T00:00:00Z&to=2026-08-15T00:00:00Z",
    ] {
        let said = refused(span, ErrorCode::InvalidRange).await;
        assert!(said.contains("from must lie before to"), "{said}");
    }
    for sort in ["sort=", "sort=points", "sort=Score", "sort=newest,score"] {
        let said = refused(sort, ErrorCode::InvalidSort).await;
        assert_eq!(said, "sort must be one of newest, score");
    }
    for limit in ["limit=0", "limit=101", "limit=many", "limit=-1"] {
        refused(limit, ErrorCode::InvalidLimit).await;
    }
    for cursor in [
        "after=",
        "after=next",
        "after=0_short",
        // The cursor of one order is none of the other.
        "after=p11.98_0123456789abcdefghjkmnpqrs",
        "sort=score&after=1786789800000000_0123456789abcdefghjkmnpqrs",
        "sort=score&after=pNaN_0123456789abcdefghjkmnpqrs",
        "sort=score&after=pinf_0123456789abcdefghjkmnpqrs",
        "after=0_0123456789abcdefghjkmnpqrs'%20OR%20'1'='1",
    ] {
        refused(cursor, ErrorCode::InvalidCursor).await;
    }

    // The list is read and nothing else.
    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        let answer = ask(method.clone(), PUBLIC_FLIGHTS_PATH)
            .token(DEMO_TOKEN)
            .send(&router)
            .await;
        answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
    }
    // "public" is the address of no flight.
    let one = get(&router, "/v1/flights/public/igc").await;
    one.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
}

#[tokio::test]
async fn a_database_that_is_down_is_said_to_be_down() {
    let api = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(TestClock::new()),
        std::iter::empty(),
        settings(),
    );
    let router = api.router(&Cors::Any);
    for question in [
        PUBLIC_FLIGHTS_PATH,
        "/v1/flights/public?sort=score&bbox=11.6,47.55,12,47.85",
    ] {
        let answer = get(&router, question).await;
        let said = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
        assert!(!said.contains("connection refused"), "{said}");
    }
}
