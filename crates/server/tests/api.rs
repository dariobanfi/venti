//! Tests of the HTTP API through the real router.
//!
//! Most of them run against exactly what the development mode serves: the
//! recorded payloads of every provider, run through the real adapters into
//! the in-memory store. The answers are compared with golden files, which
//! are also the examples in `docs/api.md`. Rewrite them with
//! `VENTI_UPDATE_GOLDEN=1` and review the diff.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ErrorCode, ErrorResponse, HealthResponse, HealthStatus, MAX_STATIONS, ObservationsRequest,
    ObservationsResponse, StationsRequest, StationsResponse,
};
use venti_domain::{BoundingBox, GeoPoint, SourceId, Station, StationId};
use venti_server::adapters;
use venti_server::api::{Api, Cors};
use venti_server::clock::FixedClock;
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;
use venti_store::{MemoryStore, RunRecord, Store};

mod common;

use common::{get, get_with, golden, recorded, send, settings};

// --- Helpers ----------------------------------------------------------------

const WEB_APP: &str = common::WEB_APP;

fn first_region() -> String {
    StationsRequest {
        bbox: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
    .path_and_query()
}

fn id(text: &str) -> StationId {
    text.parse().unwrap()
}

// --- The three endpoints over the recorded data -----------------------------

#[tokio::test]
async fn health_reports_every_source_fresh() {
    let answer = get(&recorded().await, "/v1/health").await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    let health: HealthResponse = answer.json();
    assert_eq!(health.status, HealthStatus::Ok);
    assert_eq!(health.time, adapters::recorded_at());
    let stations: Vec<(&str, u32)> = health
        .sources
        .iter()
        .map(|source| (source.source.as_str(), source.stations))
        .collect();
    assert_eq!(
        stations,
        [
            ("dwd-10min", 3),
            ("geosphere-tawes", 3),
            ("open-meteo", 0),
            ("openaip", 0),
            ("osm-free-flying", 0)
        ]
    );
    // The airspace source says how much airspace the server holds, and
    // the sources of everything else say nothing about airspace.
    let airspaces: Vec<Option<u32>> = health.sources.iter().map(|s| s.airspaces).collect();
    assert_eq!(airspaces, [None, None, None, Some(192), None]);
    // The same for the flying sites.
    let sites: Vec<Option<u32>> = health.sources.iter().map(|s| s.sites).collect();
    assert_eq!(sites, [None, None, None, None, Some(502)]);
    // The forecast source says which run each of its models is at. The
    // development mode's forecasts were recorded a day after its wind, so
    // here the runs are dated after the look that found them.
    assert_eq!(
        health.forecasts,
        Some(venti_api_types::v1::ForecastsHealth::Available)
    );
    let models: Vec<(&str, String)> = health.sources[2]
        .models
        .iter()
        .map(|model| (model.model.as_str(), model.run.to_rfc3339()))
        .collect();
    assert_eq!(
        models,
        [
            ("icon-d2", "2026-11-20T21:00:00+00:00".to_owned()),
            ("icon-eu", "2026-11-20T21:00:00+00:00".to_owned())
        ]
    );
    assert!(
        health.sources[2]
            .models
            .iter()
            .all(|m| m.checked_at == health.time)
    );
    assert!(health.sources[3].models.is_empty());
    assert!(health.sources.iter().all(|source| !source.stale));
    assert_golden(&golden("health.json"), &health);
}

#[tokio::test]
async fn stations_lists_the_box_with_latest_readings_and_credits() {
    let answer = get(&recorded().await, &first_region()).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    let stations: StationsResponse = answer.json();
    assert_eq!(stations.stations.len(), 6);
    assert!(!stations.truncated);
    // Every station has a reading, and the newest one of its day.
    let zugspitze = stations
        .stations
        .iter()
        .find(|entry| entry.station.name == "Zugspitze")
        .unwrap();
    let latest = zugspitze.latest.as_ref().unwrap();
    assert!(adapters::recorded_at() - latest.time < Duration::hours(1));
    assert!(stations.stations.iter().all(|entry| entry.latest.is_some()));
    assert_golden(&golden("stations.json"), &stations);
}

#[tokio::test]
async fn every_record_names_one_source_and_the_response_credits_it() {
    let stations: StationsResponse = get(&recorded().await, &first_region()).await.json();

    let credited: Vec<&SourceId> = stations.attributions.iter().map(|a| &a.source).collect();
    assert_eq!(credited.len(), 2, "one credit per source, not per station");
    for entry in &stations.stations {
        // A record belongs to the source its identifier names, and to no other.
        assert_eq!(entry.station.id.source(), &entry.station.source);
        assert!(credited.contains(&&entry.station.source));
    }
    for attribution in &stations.attributions {
        assert!(!attribution.text.is_empty());
        assert!(!attribution.licence.is_empty());
        assert!(attribution.url.starts_with("https://"));
    }

    // A box with German stations only carries the German credit only.
    let zugspitze = "/v1/stations?bbox=10.9,47.4,11.0,47.5";
    let stations: StationsResponse = get(&recorded().await, zugspitze).await.json();
    assert_eq!(stations.stations.len(), 1);
    assert_eq!(stations.attributions.len(), 1);
    assert_eq!(
        stations.attributions[0].text,
        "Quelle: Deutscher Wetterdienst"
    );

    // And an empty box carries none.
    let nowhere = "/v1/stations?bbox=0,0,1,1";
    let stations: StationsResponse = get(&recorded().await, nowhere).await.json();
    assert!(stations.stations.is_empty());
    assert!(stations.attributions.is_empty());
}

#[tokio::test]
async fn observations_default_to_the_last_day_oldest_first() {
    let router = recorded().await;
    let request = ObservationsRequest {
        station: id("dwd-10min:05792"),
        from: None,
        to: None,
    };

    let answer = get(&router, &request.path_and_query()).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    let day: ObservationsResponse = answer.json();
    assert_eq!(day.station.name, "Zugspitze");
    // The recording holds the day up to shortly before 19:00, one reading
    // every ten minutes.
    assert!(day.observations.len() > 100, "{}", day.observations.len());
    assert!(day.observations.is_sorted_by_key(|reading| reading.time));
    assert_eq!(day.attributions.len(), 1);
    assert_eq!(day.attributions[0].source.as_str(), "dwd-10min");

    // An explicit window, as a golden file.
    let request = ObservationsRequest {
        from: Some(adapters::recorded_at() - Duration::minutes(65)),
        to: Some(adapters::recorded_at()),
        ..request
    };
    let hour: ObservationsResponse = get(&router, &request.path_and_query()).await.json();
    assert!(hour.observations.len() < 10);
    let (first, last) = (&hour.observations[0], hour.observations.last().unwrap());
    assert!(first.time >= request.from.unwrap() && last.time <= request.to.unwrap());
    assert_golden(&golden("observations.json"), &hour);
}

#[tokio::test]
async fn a_station_identifier_may_be_percent_encoded() {
    let router = recorded().await;
    let plain: ObservationsResponse =
        get(&router, "/v1/stations/geosphere-tawes:11126/observations")
            .await
            .json();
    let encoded: ObservationsResponse =
        get(&router, "/v1/stations/geosphere-tawes%3A11126/observations")
            .await
            .json();
    assert_eq!(plain, encoded);
    assert_eq!(plain.station.name, "PATSCHERKOFEL");
    assert_eq!(plain.observations.len(), 1);
}

// --- The documentation ---------------------------------------------------------

/// `docs/api.md` says that its examples are the development mode's answers.
/// This is what holds it to that.
#[test]
fn the_documentation_shows_the_answers_as_they_are() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api.md");
    let docs = std::fs::read_to_string(path).unwrap();
    for name in [
        "health.json",
        "observations.json",
        "airspace.json",
        "airspace_none.json",
        "sites.json",
        "forecast.json",
        "thermals.json",
        "error_missing_bbox.json",
        "sign_in.json",
        "sessions.json",
        "account.json",
        "upload.json",
        "flight_changes.json",
        "flight.json",
        "public_flights.json",
        "public_flights_best.json",
        "ranking.json",
        "report_request.json",
        "report.json",
        "flight_taken_down.json",
        "account_suspended.json",
        "groups.json",
        "group.json",
        "group_create_request.json",
        "group_created.json",
        "group_join_request.json",
        "group_joined.json",
        "group_invite.json",
        "plans.json",
        "plan_board.json",
        "plan_request.json",
        "plan.json",
        "maps.json",
    ] {
        let answer = std::fs::read_to_string(golden(name)).unwrap();
        assert!(
            docs.contains(answer.trim_end()),
            "docs/api.md does not show {name} as the tests keep it; \
             copy the file from crates/server/tests/golden into the document"
        );
    }
    // And the demo account it describes is the one the server starts with.
    for written_down in [
        venti_server::dev::DEMO_HANDLE,
        venti_server::dev::DEMO_PASSWORD,
        venti_server::dev::DEMO_TOKEN,
    ] {
        assert!(docs.contains(written_down), "{written_down}");
    }
    // So are the two further pilots.
    for pilot in venti_server::dev::DEMO_PILOTS {
        let row = format!("| `{}` | {} |", pilot.handle, pilot.name);
        assert!(docs.contains(&row), "{row}");
    }
    // And the two groups, with their invite codes and their plans.
    use venti_server::dev;
    for (id, name, code) in [
        (dev::DEMO_GROUP, dev::DEMO_GROUP_NAME, dev::DEMO_GROUP_CODE),
        (
            dev::DEMO_OTHER_GROUP,
            dev::DEMO_OTHER_GROUP_NAME,
            dev::DEMO_OTHER_GROUP_CODE,
        ),
    ] {
        let row = format!("| `{id}` | {name} | `{code}` |");
        assert!(docs.contains(&row), "{row}");
    }
    for plan in dev::DEMO_PLANS.iter().chain([&dev::DEMO_OTHER_PLAN]) {
        assert!(docs.contains(&format!("| `{plan}` |")), "{plan}");
    }
    // The numbers that bound groups are the ones the document states.
    let limits = venti_server::groups::Limits::default();
    for stated in [
        format!("in at most {} groups", limits.groups_per_account),
        format!("at most {} members", limits.members_per_group),
        format!("at most {} plans at a time", limits.plans_per_group),
        format!("makes at most {} groups a day", limits.groups_made_per_day),
        format!("tries at most {} invite codes a day", limits.joins_per_day),
        format!(
            "posts or changes at most {} plans a day",
            limits.plans_posted_per_day
        ),
        format!("for {} days after its day", limits.plan_days),
    ] {
        assert!(docs.contains(&stated), "docs/api.md does not say: {stated}");
    }
}

// --- Caching, compression, CORS ---------------------------------------------

#[tokio::test]
async fn an_unchanged_answer_is_not_sent_again() {
    let router = recorded().await;
    let first = get(&router, &first_region()).await;
    let tag = first.header(header::ETAG).to_owned();

    let again = get_with(&router, &first_region(), &[(header::IF_NONE_MATCH, &tag)]).await;

    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    assert_eq!(again.header(header::ETAG), tag);
    // A different question has a different answer.
    let other = "/v1/stations?bbox=10.9,47.4,11.0,47.5";
    let other = get_with(&router, other, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(other.status, StatusCode::OK);
}

#[tokio::test]
async fn answers_are_compressed_when_the_client_accepts_it() {
    let router = recorded().await;
    let plain = get(&router, &first_region()).await;
    assert!(!plain.headers.contains_key(header::CONTENT_ENCODING));

    let packed = get_with(
        &router,
        &first_region(),
        &[(header::ACCEPT_ENCODING, "gzip")],
    )
    .await;

    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");
    // Caches must keep the two forms apart. CORS adds a Vary header of its own.
    let vary: Vec<&str> = packed
        .headers
        .get_all(header::VARY)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();
    assert!(vary.join(",").contains("accept-encoding"), "{vary:?}");
    assert_eq!(&packed.body[..2], [0x1f, 0x8b], "the gzip signature");
    assert!(packed.body.len() * 2 < plain.body.len());
    // The validator is the same for both forms.
    assert_eq!(packed.header(header::ETAG), plain.header(header::ETAG));
}

#[tokio::test]
async fn only_the_configured_web_origin_may_read_the_api() {
    let router = recorded().await;

    let ours = get_with(&router, "/v1/health", &[(header::ORIGIN, WEB_APP)]).await;
    assert_eq!(ours.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);

    let theirs = get_with(
        &router,
        "/v1/health",
        &[(header::ORIGIN, "https://elsewhere.example")],
    )
    .await;
    assert_eq!(
        theirs.status,
        StatusCode::OK,
        "CORS is enforced by the browser"
    );
    assert!(
        !theirs
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );

    // What a browser asks before a request with a validator.
    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri(first_region())
        .header(header::ORIGIN, WEB_APP)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "if-none-match")
        .body(Body::empty())
        .unwrap();
    let preflight = send(&router, preflight).await;
    assert_eq!(preflight.status, StatusCode::OK);
    assert_eq!(
        preflight.header(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        WEB_APP
    );
    assert!(
        preflight
            .header(header::ACCESS_CONTROL_ALLOW_METHODS)
            .contains("GET")
    );
    assert!(
        preflight
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .contains("if-none-match")
    );
}

// --- Requests that cannot be answered ---------------------------------------

#[tokio::test]
async fn a_bad_box_is_explained() {
    let router = recorded().await;
    let bad_request = StatusCode::BAD_REQUEST;

    let missing = get(&router, "/v1/stations").await;
    let message = missing.error(bad_request, ErrorCode::InvalidBbox);
    assert!(message.contains("bbox=10.5,47.2,11.9,47.8"), "{message}");
    assert_golden(
        &golden("error_missing_bbox.json"),
        &missing.json::<ErrorResponse>(),
    );

    for (bbox, expected) in [
        ("10.5,47.2,11.9", "four numbers"),
        ("10.5,47.2,11.9,north", "\"north\" is not a number"),
        ("10.5,47.8,11.9,47.2", "wrong order"),
        ("11.9,47.2,10.5,47.8", "wrong order"),
        ("10.5,47.2,11.9,97.8", "latitude 97.8"),
        ("10.5,47.2,211.9,47.8", "longitude 211.9"),
        ("10.5,47.2,11.9,NaN", "latitude NaN"),
        ("", "four numbers"),
    ] {
        let answer = get(&router, &format!("/v1/stations?bbox={bbox}")).await;
        let message = answer.error(bad_request, ErrorCode::InvalidBbox);
        assert!(message.contains(expected), "{bbox}: {message}");
    }
}

#[tokio::test]
async fn a_bad_station_or_window_is_explained() {
    let router = recorded().await;
    let station = "/v1/stations/dwd-10min:05792/observations";
    let error = |path: String| {
        let router = router.clone();
        async move { get(&router, &path).await }
    };

    let answer = error("/v1/stations/05792/observations".to_owned()).await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidStationId);

    let answer = error("/v1/stations/dwd-10min:99999/observations".to_owned()).await;
    let message = answer.error(StatusCode::NOT_FOUND, ErrorCode::StationNotFound);
    assert_eq!(message, "there is no station dwd-10min:99999");

    let answer = error(format!("{station}?from=yesterday")).await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidTime);
    assert!(message.starts_with("from: \"yesterday\" is not an RFC 3339 time"));

    // An offset written with a bare plus sign arrives as a space.
    let answer = error(format!("{station}?to=2026-11-18T19:00:00+01:00")).await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidTime);
    assert!(message.contains("%2B"), "{message}");
    let answer = error(format!("{station}?to=2026-11-18T19:00:00%2B01:00")).await;
    assert_eq!(answer.status, StatusCode::OK);

    let answer = error(format!(
        "{station}?from=2026-11-18T12:00:00Z&to=2026-11-18T06:00:00Z"
    ))
    .await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRange);

    let answer = error(format!(
        "{station}?from=2026-11-01T00:00:00Z&to=2026-11-18T00:00:00Z"
    ))
    .await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRange);
    assert!(message.contains("longer than 7 days"), "{message}");

    // A known station with nothing in the window is not an error.
    let answer = error(format!(
        "{station}?from=2026-11-01T00:00:00Z&to=2026-11-02T00:00:00Z"
    ))
    .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(
        answer
            .json::<ObservationsResponse>()
            .observations
            .is_empty()
    );
}

#[tokio::test]
async fn whatever_goes_wrong_the_error_has_the_same_shape() {
    let router = recorded().await;

    let answer = get(&router, "/v2/stations").await;
    let message = answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    assert!(message.contains("/v2/stations is not part of the API"));
    get(&router, "/")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);

    // The framework answers these itself; they are reshaped on the way out.
    let post = Request::builder()
        .method(Method::POST)
        .uri(first_region())
        .body(Body::empty())
        .unwrap();
    let answer = send(&router, post).await;
    answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
    assert!(answer.header(header::ALLOW).contains("GET"));

    let large = Request::builder()
        .method(Method::GET)
        .uri("/v1/health")
        .header(header::CONTENT_LENGTH, 64 * 1024)
        .body(Body::from(vec![b'x'; 64 * 1024]))
        .unwrap();
    let answer = send(&router, large).await;
    answer.error(StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge);

    let answer = get(&router, "/v1/stations/%FF%FE/observations").await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
}

// --- Stores other than the recorded one -------------------------------------

fn station(text: &str, lat: f64, lon: f64) -> Station {
    Station {
        id: id(text),
        name: text.to_owned(),
        position: GeoPoint::new(lat, lon).unwrap(),
        elevation_m: None,
    }
}

fn run(source: &str) -> RunRecord {
    RunRecord {
        source: SourceId::new(source).unwrap(),
        started_at: adapters::recorded_at(),
        finished_at: adapters::recorded_at(),
        healthy: true,
        requests: 1,
        failed_requests: 0,
        skipped: 0,
        rejected: 0,
        stations: 0,
        observations: 0,
        newest_observation: None,
        airspaces: 0,
    }
}

/// An API that serves DWD only, over the given store.
fn dwd_only<S: Store>(store: S) -> Router {
    let registry = adapters::registry();
    let dwd = SourceId::new("dwd-10min").unwrap();
    let manifests = registry.manifests().filter(|manifest| manifest.id == dwd);
    let clock = Arc::new(FixedClock(adapters::recorded_at()));
    Api::new(Arc::new(store), clock, manifests, settings()).router(&Cors::Any)
}

#[tokio::test]
async fn data_of_a_source_the_server_does_not_serve_stays_inside() {
    // The store still holds stations of a source that was taken out of the
    // configuration, for example because its licence changed.
    let store = MemoryStore::new();
    store
        .save_run(
            &run("dwd-10min"),
            &[station("dwd-10min:05792", 47.421, 10.9848)],
            &[],
        )
        .await
        .unwrap();
    store
        .save_run(
            &run("geosphere-tawes"),
            &[station("geosphere-tawes:11126", 47.2089, 11.4622)],
            &[],
        )
        .await
        .unwrap();
    let router = dwd_only(store);

    let stations: StationsResponse = get(&router, &first_region()).await.json();
    let ids: Vec<String> = stations
        .stations
        .iter()
        .map(|entry| entry.station.id.to_string())
        .collect();
    assert_eq!(ids, ["dwd-10min:05792"]);
    assert_eq!(stations.attributions.len(), 1);

    let hidden = get(&router, "/v1/stations/geosphere-tawes:11126/observations").await;
    hidden.error(StatusCode::NOT_FOUND, ErrorCode::StationNotFound);

    // A station without observations is listed without a reading.
    assert!(stations.stations[0].latest.is_none());
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert_eq!(health.sources.len(), 1);
}

#[tokio::test]
async fn a_box_with_too_many_stations_is_cut_and_says_so() {
    let store = MemoryStore::new();
    let many: Vec<Station> = (0..=MAX_STATIONS)
        .map(|n| station(&format!("dwd-10min:{n:05}"), 47.0, 11.0))
        .collect();
    store.save_run(&run("dwd-10min"), &many, &[]).await.unwrap();
    let router = dwd_only(store);

    let stations: StationsResponse = get(&router, &first_region()).await.json();

    assert!(stations.truncated);
    assert_eq!(stations.stations.len(), MAX_STATIONS);
    assert_eq!(stations.stations[0].station.id, id("dwd-10min:00000"));
}

#[tokio::test]
async fn a_stale_source_degrades_health_without_failing_it() {
    // The only run is a day old.
    let store = MemoryStore::new();
    let yesterday = adapters::recorded_at() - Duration::days(1);
    let old = RunRecord {
        started_at: yesterday,
        finished_at: yesterday,
        ..run("dwd-10min")
    };
    store.save_run(&old, &[], &[]).await.unwrap();

    let answer = get(&dwd_only(store), "/v1/health").await;

    assert_eq!(answer.status, StatusCode::OK);
    let health: HealthResponse = answer.json();
    assert_eq!(health.status, HealthStatus::Degraded);
    assert!(health.sources[0].stale);
    assert_eq!(health.sources[0].last_success, Some(yesterday));
}

#[tokio::test]
async fn a_database_that_is_down_is_a_503_that_says_try_again() {
    let router = dwd_only(TroubledStore::down());

    for path in [
        "/v1/health".to_owned(),
        first_region(),
        "/v1/stations/dwd-10min:05792/observations".to_owned(),
    ] {
        let answer = get(&router, &path).await;
        let message = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
        // What exactly failed is the operator's business.
        assert!(!message.contains("connection refused"), "{message}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_request_that_takes_too_long_is_given_up() {
    let router = dwd_only(TroubledStore::slow(std::time::Duration::from_secs(3600)));
    let began = tokio::time::Instant::now();

    let answer = get(&router, &first_region()).await;

    answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Timeout);
    assert_eq!(began.elapsed(), std::time::Duration::from_secs(10));
}
