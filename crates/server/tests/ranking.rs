//! The ranking of pilots, through the real router: who is in it and in
//! what order, how it is narrowed, and what must never count: a flight
//! that is private or shared by link.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::{NaiveDate, TimeZone, Utc};
use venti_api_types::v1::{
    ErrorCode, Flight, RANKING_PATH, RankedPilot, RankingRequest, RankingResponse, Visibility,
};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::synthetic_flight;
use venti_server::api::{Api, Cors};
use venti_server::dev::{DEMO_HANDLE, DEMO_PILOTS, DEMO_TAKEOFF_PILOTS, DEMO_TOKEN};
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

fn day(month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, month, day).unwrap()
}

fn midnight(month: u32, day: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0).unwrap()
}

/// The made-up sample flight on another day, moved `east` degrees of
/// longitude from where it takes off (the Wallberg, at 11.7967° E), and cut
/// to its first `seconds`. It starts at 10:30 UTC. The longer the cut, the
/// more points.
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

/// Uploads a flight and sets who may see it.
async fn fly(router: &Router, token: &str, file: &[u8], visibility: Visibility) -> Flight {
    let flight = upload(router, token, file).await.flight;
    if visibility != Visibility::Private {
        set_visibility(router, token, &flight.id, visibility).await;
    }
    flight
}

async fn ranking(router: &Router, request: &RankingRequest) -> RankingResponse {
    get(router, &request.path_and_query())
        .await
        .ok(StatusCode::OK)
}

fn everything() -> RankingRequest {
    RankingRequest::default()
}

fn handles(pilots: &[RankedPilot]) -> Vec<String> {
    pilots
        .iter()
        .map(|pilot| pilot.pilot.handle.to_string())
        .collect()
}

fn points(flight: &Flight) -> f64 {
    flight.score.expect("the flight scores").points
}

/// What the flights that count add up to, in hundredths as the server
/// hands a sum out.
fn sum(flights: &[Flight]) -> f64 {
    (flights.iter().map(points).sum::<f64>() * 100.0).round() / 100.0
}

/// The valley of the Tegernsee, where the sample flight is flown.
fn tegernsee() -> BoundingBox {
    BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap()
}

/// The box of the development mode's made-up sites
/// ([`venti_server::dev::DEMO_SITES_BBOX`]), from which the flights that
/// say something about take-offs are flown.
fn made_up_sites() -> BoundingBox {
    BoundingBox::new(47.25, 10.85, 47.32, 10.95).unwrap()
}

/// An answer with the outlines cut to two points: short enough to stand in
/// the documentation.
fn example(mut answer: RankingResponse) -> RankingResponse {
    for flight in answer
        .pilots
        .iter_mut()
        .flat_map(|pilot| &mut pilot.flights)
    {
        flight.outline.truncate(2);
    }
    answer
}

// --- The development mode -----------------------------------------------------

#[tokio::test]
async fn the_development_mode_ranks_its_six_pilots() {
    let router = recorded().await;
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);
    let [lena, timo, ines] = DEMO_TAKEOFF_PILOTS.map(|pilot| pilot.handle);

    let all = ranking(&router, &everything()).await;
    assert_eq!(all.counted, 3);
    // Eleven public flights: three of Lena, two each of Mara, Jonas and
    // Timo, one each of the demo account and Ines. The demo account's
    // other two are not public, nor is one of Ines's. By the sum of their
    // points, the most first: Lena, whose three all count, before the
    // pilots of two flights, and the demo account and Ines, of one each,
    // last.
    assert_eq!(
        handles(&all.pilots),
        [lena, mara, jonas, timo, DEMO_HANDLE, ines].map(str::to_owned)
    );
    let public_flights = |handle: &str| match handle {
        DEMO_HANDLE => 1,
        handle if handle == ines => 1,
        handle if handle == lena => 3,
        _ => 2,
    };
    for pilot in &all.pilots {
        let flights = public_flights(pilot.pilot.handle.as_str());
        assert_eq!(pilot.flights.len(), flights, "{}", pilot.pilot.handle);
        assert_eq!(pilot.scored, u32::try_from(flights).unwrap());
        assert_eq!(pilot.points, sum(&pilot.flights));
        // The best first, each public and its pilot's own.
        assert!(
            pilot
                .flights
                .windows(2)
                .all(|pair| points(&pair[0]) >= points(&pair[1]))
        );
        for flight in &pilot.flights {
            assert_eq!(flight.visibility, Visibility::Public);
            assert_eq!(flight.pilot, pilot.pilot);
        }
    }
    // The most points first.
    assert!(
        all.pilots
            .windows(2)
            .all(|pair| pair[0].points >= pair[1].points)
    );
    // With a token the answer is the same.
    let with_a_token: RankingResponse = ask(Method::GET, RANKING_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(with_a_token, all);

    // The example of the documentation: the two pilots at the top of one
    // month.
    let august = RankingRequest {
        from: Some(midnight(8, 1)),
        to: Some(midnight(9, 1)),
        limit: Some(2),
        ..everything()
    };
    assert_eq!(
        august.path_and_query(),
        "/v1/ranking?from=2026-08-01T00:00:00Z&to=2026-09-01T00:00:00Z&limit=2"
    );
    let top = ranking(&router, &august).await;
    assert_eq!(top.pilots.len(), 2);
    assert_golden(&golden("ranking.json"), &example(top));

    // The flights of this mode took off in two places: a box ranks the
    // three pilots who flew from the one, the three who flew from the
    // other, or nobody.
    let here = RankingRequest {
        bbox: Some(tegernsee()),
        ..everything()
    };
    let mut theirs = handles(&ranking(&router, &here).await.pilots);
    theirs.sort();
    let mut expected = [DEMO_HANDLE, mara, jonas].map(str::to_owned);
    expected.sort();
    assert_eq!(theirs, expected);
    let there = RankingRequest {
        bbox: Some(made_up_sites()),
        ..everything()
    };
    let mut theirs = handles(&ranking(&router, &there).await.pilots);
    theirs.sort();
    let mut expected = [lena, timo, ines].map(str::to_owned);
    expected.sort();
    assert_eq!(theirs, expected);
    let elsewhere = RankingRequest {
        bbox: Some(BoundingBox::new(46.0, 9.0, 47.0, 10.0).unwrap()),
        ..everything()
    };
    assert!(ranking(&router, &elsewhere).await.pilots.is_empty());
}

// --- Who is ranked, and by what ---------------------------------------------------

#[tokio::test]
async fn the_best_three_public_flights_of_a_pilot_count_and_no_others() {
    let router = open_server();
    assert!(ranking(&router, &everything()).await.pilots.is_empty());

    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let bernd = sign_up(&router, "bernd").await.token;

    // Dario flies four times, each flight longer than the one before.
    let mut darios = Vec::new();
    for (n, seconds) in [(1, 900), (2, 1100), (3, 1300), (4, 1500)] {
        let file = flight_file(day(7, n), 0.0, seconds);
        darios.push(fly(&router, &dario, &file, Visibility::Public).await);
    }
    // Anna's one public flight is the whole sample flight. Her better ones
    // are not for everyone: they count for nobody.
    let whole = synthetic_flight().fixes.len();
    let annas = fly(
        &router,
        &anna,
        &flight_file(day(7, 5), 0.0, whole),
        Visibility::Public,
    )
    .await;
    fly(
        &router,
        &anna,
        &flight_file(day(7, 6), 0.0, whole),
        Visibility::Private,
    )
    .await;
    fly(
        &router,
        &anna,
        &flight_file(day(7, 7), 0.0, whole),
        Visibility::Unlisted,
    )
    .await;
    // Bernd keeps his only flight to himself: he is not ranked.
    fly(
        &router,
        &bernd,
        &flight_file(day(7, 8), 0.0, whole),
        Visibility::Private,
    )
    .await;

    let all = ranking(&router, &everything()).await;
    assert_eq!(handles(&all.pilots), ["dario", "anna"]);
    let [first, second] = &all.pilots[..] else {
        panic!("two pilots are ranked")
    };
    // Dario's three longest count, the best first; all four scored.
    let counted: Vec<Flight> = darios.iter().rev().take(3).cloned().collect();
    assert!(points(&counted[0]) > points(&counted[1]));
    assert_eq!(
        first.flights.iter().map(|f| &f.id).collect::<Vec<_>>(),
        counted.iter().map(|f| &f.id).collect::<Vec<_>>()
    );
    assert_eq!(first.scored, 4);
    assert_eq!(first.points, sum(&counted));
    assert_eq!(first.pilot.display_name, "Pilot dario");
    // Anna's one.
    assert_eq!(
        second.flights.iter().map(|f| &f.id).collect::<Vec<_>>(),
        [&annas.id]
    );
    assert_eq!(second.scored, 1);
    assert_eq!(second.points, points(&annas));
    assert!(first.points > second.points);

    // From the top, as many as asked for.
    let top = RankingRequest {
        limit: Some(1),
        ..everything()
    };
    assert_eq!(handles(&ranking(&router, &top).await.pilots), ["dario"]);

    // A span of time: of Dario's flights the first two, which is less than
    // Anna's one of the 5th if her day is asked for too.
    let first_days = RankingRequest {
        from: Some(midnight(7, 1)),
        to: Some(midnight(7, 3)),
        ..everything()
    };
    let early = ranking(&router, &first_days).await;
    assert_eq!(handles(&early.pilots), ["dario"]);
    assert_eq!(early.pilots[0].scored, 2);
    assert_eq!(early.pilots[0].points, sum(&darios[..2]));
    let after_them = RankingRequest {
        from: Some(midnight(7, 5)),
        ..everything()
    };
    assert_eq!(
        handles(&ranking(&router, &after_them).await.pilots),
        ["anna"]
    );

    // A flight counts while it is public and no longer.
    set_visibility(&router, &dario, &darios[3].id, Visibility::Private).await;
    let without = ranking(&router, &everything()).await;
    let dario_now = without
        .pilots
        .iter()
        .find(|pilot| pilot.pilot.handle.as_str() == "dario")
        .unwrap();
    assert_eq!(dario_now.scored, 3);
    assert_eq!(dario_now.points, sum(&darios[..3]));
}

#[tokio::test]
async fn a_ranking_of_a_part_of_the_map_counts_the_flights_that_took_off_there() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let whole = synthetic_flight().fixes.len();
    // Dario flies at the Tegernsee, and once from a mountain 0.6 degrees
    // east of it; Anna only there.
    fly(
        &router,
        &dario,
        &flight_file(day(7, 1), 0.0, 1200),
        Visibility::Public,
    )
    .await;
    fly(
        &router,
        &dario,
        &flight_file(day(7, 2), 0.6, whole),
        Visibility::Public,
    )
    .await;
    fly(
        &router,
        &anna,
        &flight_file(day(7, 3), 0.6, whole),
        Visibility::Public,
    )
    .await;

    let here = RankingRequest {
        bbox: Some(tegernsee()),
        ..everything()
    };
    let at_the_lake = ranking(&router, &here).await;
    assert_eq!(handles(&at_the_lake.pilots), ["dario"]);
    assert_eq!(at_the_lake.pilots[0].scored, 1);

    let east = RankingRequest {
        bbox: Some(BoundingBox::new(47.55, 12.2, 47.85, 12.6).unwrap()),
        ..everything()
    };
    // Both flew the same flight there: the same points, and the handle
    // decides.
    let there = ranking(&router, &east).await;
    assert_eq!(handles(&there.pilots), ["anna", "dario"]);
    assert_eq!(there.pilots[0].points, there.pilots[1].points);
}

// --- Who may keep an answer ---------------------------------------------------------

#[tokio::test]
async fn the_ranking_may_be_kept_and_is_checked_before_every_reuse() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let whole = synthetic_flight().fixes.len();
    let flight = fly(
        &router,
        &dario,
        &flight_file(day(7, 1), 0.0, whole),
        Visibility::Public,
    )
    .await;

    let answer = get(&router, RANKING_PATH).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    assert!(!answer.headers.contains_key(header::SET_COOKIE));
    let tag = answer.header(header::ETAG).to_owned();
    let asked_again = [(header::IF_NONE_MATCH, tag.as_str())];
    let again = get_with(&router, RANKING_PATH, &asked_again).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());

    // A flight that is made private counts no longer with the next
    // question, also for whoever kept the ranking.
    set_visibility(&router, &dario, &flight.id, Visibility::Private).await;
    let after = get_with(&router, RANKING_PATH, &asked_again).await;
    let now: RankingResponse = after.ok(StatusCode::OK);
    assert!(now.pilots.is_empty());
    assert_ne!(after.header(header::ETAG), tag);
}

// --- Questions that cannot be answered ------------------------------------------------

#[tokio::test]
async fn a_question_that_cannot_be_read_is_explained() {
    let router = recorded().await;
    let refused = |query: &'static str, code: ErrorCode| {
        let router = router.clone();
        async move {
            let answer = get(&router, &format!("{RANKING_PATH}?{query}")).await;
            answer.error(StatusCode::BAD_REQUEST, code)
        }
    };

    for bbox in ["bbox=", "bbox=11.6,47.55,12", "bbox=11.6,47.85,12,47.55"] {
        let said = refused(bbox, ErrorCode::InvalidBbox).await;
        assert!(said.contains("west,south,east,north"), "{said}");
    }
    for time in ["from=yesterday", "to=2026-08-15", "from="] {
        refused(time, ErrorCode::InvalidTime).await;
    }
    for span in [
        "from=2026-08-16T00:00:00Z&to=2026-08-15T00:00:00Z",
        "from=2026-08-15T00:00:00Z&to=2026-08-15T00:00:00Z",
    ] {
        let said = refused(span, ErrorCode::InvalidRange).await;
        assert!(said.contains("from must lie before to"), "{said}");
    }
    for limit in ["limit=0", "limit=101", "limit=many", "limit=-1"] {
        refused(limit, ErrorCode::InvalidLimit).await;
    }

    // The ranking is read and nothing else.
    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        let answer = ask(method.clone(), RANKING_PATH)
            .token(DEMO_TOKEN)
            .send(&router)
            .await;
        answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
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
    for question in [RANKING_PATH, "/v1/ranking?bbox=11.6,47.55,12,47.85"] {
        let answer = get(&router, question).await;
        let said = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
        assert!(!said.contains("connection refused"), "{said}");
    }
}
