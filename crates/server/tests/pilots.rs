//! The search for pilots and what a pilot's public flights add up to,
//! through the real router: who is found, what is counted, and what must
//! never be: a pilot who shares nothing, a flight that is private or
//! shared by link.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::NaiveDate;
use venti_api_types::v1::{
    ErrorCode, Flight, Handle, PILOTS_PATH, PilotStatisticsResponse, PilotsRequest, PilotsResponse,
    RankingRequest, RankingResponse, Visibility, pilot_statistics_path,
};
use venti_domain::GeoPoint;
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::synthetic_flight;
use venti_server::api::{Api, Cors};
use venti_server::dev::{DEMO_HANDLE, DEMO_TAKEOFF_PILOTS, DEMO_TOKEN};
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;

mod common;

use common::{
    TestClock, ask, get, get_with, golden, recorded, server, set_visibility, settings, sign_up,
    upload,
};

// --- Helpers ----------------------------------------------------------------

fn open_server() -> Router {
    server(settings(), &TestClock::new()).0
}

fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

/// The made-up sample flight on another day, moved `east` degrees of
/// longitude and cut to its first `seconds`.
fn flight_file(date: NaiveDate, east: f64, seconds: usize) -> Vec<u8> {
    let original = synthetic_flight();
    let moved_by = date - original.date;
    let mut fixes = original.fixes;
    fixes.truncate(seconds + 1);
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

async fn fly(router: &Router, token: &str, file: &[u8], visibility: Visibility) -> Flight {
    let flight = upload(router, token, file).await.flight;
    if visibility != Visibility::Private {
        set_visibility(router, token, &flight.id, visibility).await;
    }
    flight
}

fn search(q: &str) -> String {
    PilotsRequest {
        q: q.to_owned(),
        limit: None,
    }
    .path_and_query()
}

async fn found(router: &Router, q: &str) -> Vec<(String, u32)> {
    let answer: PilotsResponse = get(router, &search(q)).await.ok(StatusCode::OK);
    answer
        .pilots
        .into_iter()
        .map(|found| (found.pilot.handle.to_string(), found.flights))
        .collect()
}

fn statistics_path(handle: &str) -> String {
    pilot_statistics_path(&Handle::new(handle).unwrap())
}

async fn statistics(router: &Router, handle: &str) -> PilotStatisticsResponse {
    get(router, &statistics_path(handle))
        .await
        .ok(StatusCode::OK)
}

// --- The development mode -----------------------------------------------------

#[tokio::test]
async fn the_development_mode_finds_its_pilots_by_name() {
    let router = recorded().await;
    let [lena, timo, ines] = DEMO_TAKEOFF_PILOTS.map(|pilot| pilot.handle);
    let pilot = |handle: &str, flights: u32| (handle.to_owned(), flights);

    // The example of the documentation: a handle and a name that begin
    // with the text.
    assert_eq!(search("le"), "/v1/pilots?q=le");
    let answer = get(&router, &search("le")).await;
    let said: PilotsResponse = answer.ok(StatusCode::OK);
    assert_eq!(said.pilots[0].pilot.display_name, "Lena Probe");
    assert_golden(&golden("pilots.json"), &said);
    assert_eq!(found(&router, "le").await[0], pilot(lena, 3));

    // By a part of the name, whatever its case; the one that begins with
    // the text before the one that holds it.
    assert_eq!(found(&router, "PROBE").await, [pilot(lena, 3)]);
    assert_eq!(found(&router, "  vorl ").await, [pilot(ines, 1)]);
    assert_eq!(
        found(&router, "ti").await,
        [pilot(timo, 2)],
        "Timo by handle and name"
    );
    // The demo account has three flights and one that is public.
    assert_eq!(found(&router, "demo").await, [pilot(DEMO_HANDLE, 1)]);
    assert!(found(&router, "nobody").await.is_empty());
    // With a token the answer is the same.
    let with_a_token: PilotsResponse = ask(Method::GET, &search("le"))
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(with_a_token, said);
}

#[tokio::test]
async fn the_development_mode_adds_up_a_pilots_public_flights() {
    let router = recorded().await;
    let [lena, timo, ines] = DEMO_TAKEOFF_PILOTS.map(|pilot| pilot.handle);

    // The example of the documentation: Lena's three flights of 2026,
    // two of them from the Musterkogel.
    let said = statistics(&router, lena).await;
    assert_golden(&golden("pilot_statistics.json"), &said);
    assert_eq!(said.counted, 3);
    assert_eq!(said.overall.flights, 3);
    assert_eq!(said.seasons.len(), 1);
    assert_eq!(said.seasons[0].year, 2026);
    assert_eq!(said.seasons[0].totals, said.overall);
    assert_eq!(said.overall.by_month, [0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0]);
    let sites: Vec<(&str, u32)> = said
        .overall
        .sites
        .iter()
        .map(|flown| (flown.site.name.as_str(), flown.flights))
        .collect();
    assert_eq!(sites, [("Musterkogel West", 2)]);
    // The points are the ranking's.
    let ranking: RankingResponse = get(&router, &RankingRequest::default().path_and_query())
        .await
        .ok(StatusCode::OK);
    for ranked in &ranking.pilots {
        let theirs = statistics(&router, ranked.pilot.handle.as_str()).await;
        assert_eq!(theirs.pilot, ranked.pilot);
        assert_eq!(theirs.overall.points, ranked.points);
        assert_eq!(theirs.overall.scored, ranked.scored);
    }

    // Timo flew in two years, the newer first; each adds up to the whole.
    let timo = statistics(&router, timo).await;
    let years: Vec<(i32, u32)> = timo
        .seasons
        .iter()
        .map(|season| (season.year, season.totals.flights))
        .collect();
    assert_eq!(years, [(2026, 1), (2025, 1)]);
    let airtime: i64 = timo
        .seasons
        .iter()
        .map(|season| season.totals.airtime_s)
        .sum();
    assert_eq!(airtime, timo.overall.airtime_s);
    // Ines has a public flight and a private one: one flight counts.
    assert_eq!(statistics(&router, ines).await.overall.flights, 1);
    // The demo account has one public flight of three.
    let demo = statistics(&router, DEMO_HANDLE).await;
    assert_eq!(demo.overall.flights, 1);
    // With a token the answer is the same: a pilot's own private flights
    // are not in their own totals here.
    let with_a_token: PilotStatisticsResponse = ask(Method::GET, &statistics_path(DEMO_HANDLE))
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(with_a_token, demo);
}

// --- What never counts ----------------------------------------------------------

#[tokio::test]
async fn what_is_not_public_is_in_no_search_and_no_total() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let whole = synthetic_flight().fixes.len();

    // Nobody shares anything: nobody is found and nobody has totals.
    let kept = fly(
        &router,
        &dario,
        &flight_file(day(2026, 7, 1), 0.0, whole),
        Visibility::Private,
    )
    .await;
    fly(
        &router,
        &anna,
        &flight_file(day(2026, 7, 2), 0.0, whole),
        Visibility::Unlisted,
    )
    .await;
    assert!(found(&router, "dario").await.is_empty());
    assert!(found(&router, "pilot").await.is_empty());
    for handle in ["dario", "anna", "nobody", "No%20Handle"] {
        let answer = get(&router, &format!("/v1/pilots/{handle}/statistics")).await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    }

    // Dario makes two flights public, in two years, the older one short.
    let long = fly(
        &router,
        &dario,
        &flight_file(day(2026, 8, 3), 0.0, whole),
        Visibility::Public,
    )
    .await;
    let short = fly(
        &router,
        &dario,
        &flight_file(day(2025, 6, 3), 0.0, 900),
        Visibility::Public,
    )
    .await;
    assert_eq!(found(&router, "dar").await, [("dario".to_owned(), 2)]);
    assert!(found(&router, "ann").await.is_empty());

    let said = statistics(&router, "dario").await;
    let score = |flight: &Flight| flight.score.expect("the flight scores");
    assert_eq!(said.overall.flights, 2);
    assert_eq!(said.overall.scored, 2);
    assert_eq!(
        said.overall.airtime_s,
        long.airtime_s + short.airtime_s,
        "the private flight's airtime is not in it"
    );
    let distance = ((score(&long).distance_m + score(&short).distance_m) * 10.0).round() / 10.0;
    assert_eq!(said.overall.distance_m, distance);
    let points = ((score(&long).points + score(&short).points) * 100.0).round() / 100.0;
    assert_eq!(said.overall.points, points);
    let longest = said.overall.longest.as_ref().unwrap();
    assert_eq!(longest.id, long.id);
    assert_eq!(longest.distance_m, score(&long).distance_m);
    let highest = said.overall.highest.as_ref().unwrap();
    assert_eq!(
        highest.max_altitude_m,
        long.max_altitude_m.max(short.max_altitude_m)
    );
    assert_eq!(said.overall.by_month, [0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0]);
    // No site stands on this server: the flights took off from none.
    assert!(said.overall.sites.is_empty());
    let years: Vec<(i32, u32, f64)> = said
        .seasons
        .iter()
        .map(|season| (season.year, season.totals.flights, season.totals.points))
        .collect();
    assert_eq!(
        years,
        [
            (2026, 1, score(&long).points),
            (2025, 1, score(&short).points)
        ]
    );
    assert_eq!(
        said.seasons[1].totals.longest.as_ref().unwrap().id,
        short.id
    );
    assert_ne!(longest.id, kept.id);

    // Who may keep an answer: anyone, and it is checked before every
    // reuse. A flight that is withdrawn is in no number of the next.
    for path in [statistics_path("dario"), search("dar")] {
        let answer = get(&router, &path).await;
        assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
        let tag = answer.header(header::ETAG).to_owned();
        let again = get_with(&router, &path, &[(header::IF_NONE_MATCH, tag.as_str())]).await;
        assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    }
    set_visibility(&router, &dario, &long.id, Visibility::Unlisted).await;
    let said = statistics(&router, "dario").await;
    assert_eq!(said.overall.flights, 1);
    assert_eq!(said.seasons.len(), 1);
    assert_eq!(found(&router, "dar").await, [("dario".to_owned(), 1)]);
    set_visibility(&router, &dario, &short.id, Visibility::Private).await;
    let answer = get(&router, &statistics_path("dario")).await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    assert!(found(&router, "dar").await.is_empty());
}

// --- Questions that cannot be answered ------------------------------------------------

#[tokio::test]
async fn a_search_that_cannot_be_read_is_explained() {
    let router = recorded().await;
    let too_long = format!("q={}", "a".repeat(61));
    for query in ["", "q=", "q=l", "q=%20l%20", too_long.as_str()] {
        let answer = get(&router, &format!("{PILOTS_PATH}?{query}")).await;
        let said = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
        assert!(said.contains("2 to 60 characters"), "{said}");
    }
    for limit in ["limit=0", "limit=101", "limit=many"] {
        let answer = get(&router, &format!("{PILOTS_PATH}?q=le&{limit}")).await;
        answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidLimit);
    }
    let one = PilotsRequest {
        q: "pro".to_owned(),
        limit: Some(1),
    };
    assert_eq!(one.path_and_query(), "/v1/pilots?q=pro&limit=1");
    let answer: PilotsResponse = get(&router, &one.path_and_query()).await.ok(StatusCode::OK);
    assert_eq!(answer.pilots.len(), 1);
    // Both are read and nothing else.
    for path in [search("le"), statistics_path(DEMO_HANDLE)] {
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            let answer = ask(method.clone(), &path)
                .token(DEMO_TOKEN)
                .send(&router)
                .await;
            answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
        }
    }
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
    for question in [search("le"), statistics_path("lena")] {
        let answer = get(&router, &question).await;
        let said = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
        assert!(!said.contains("connection refused"), "{said}");
    }
}
