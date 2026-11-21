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
use venti_domain::{BoundingBox, GeoPoint, Observation, SourceId, Station, StationId};
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
            ("awc-metar", 3),
            ("dwd-10min", 3),
            ("dwd-radar-ry", 0),
            ("geosphere-inca-rr", 0),
            ("geosphere-tawes", 3),
            ("open-meteo", 0),
            ("openaip", 0),
            ("osm-cables", 0),
            ("osm-free-flying", 0),
            ("suedtirol-meteo", 3)
        ]
    );
    // The airspace source says how much airspace the server holds, and
    // the sources of everything else say nothing about airspace.
    let airspaces: Vec<Option<u32>> = health.sources.iter().map(|s| s.airspaces).collect();
    assert_eq!(
        airspaces,
        [
            None,
            None,
            None,
            None,
            None,
            None,
            Some(192),
            None,
            None,
            None
        ]
    );
    // The same for the flying sites.
    let sites: Vec<Option<u32>> = health.sources.iter().map(|s| s.sites).collect();
    assert_eq!(
        sites,
        [
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(502),
            None
        ]
    );
    // The forecast source says which run each of its models is at. The
    // development mode's forecasts were recorded three days after its
    // wind, so here the runs are dated after the look that found them.
    assert_eq!(
        health.forecasts,
        Some(venti_api_types::v1::ForecastsHealth::Available)
    );
    let models: Vec<(&str, String)> = health.sources[5]
        .models
        .iter()
        .map(|model| (model.model.as_str(), model.run.to_rfc3339()))
        .collect();
    assert_eq!(
        models,
        [
            ("icon-d2", "2026-11-21T06:00:00+00:00".to_owned()),
            ("icon-eu", "2026-11-21T06:00:00+00:00".to_owned())
        ]
    );
    assert!(
        health.sources[5]
            .models
            .iter()
            .all(|m| m.checked_at == health.time)
    );
    assert!(health.sources[6].models.is_empty());
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
    // Three of each of the four sources, and all twelve are handed out:
    // no two of them are one place measured by two sources. The nearest
    // such two are in Innsbruck, the airfield by its own report and
    // GeoSphere's station at the university, 3.03 km apart, and the rule
    // takes stations of two sources for one place up to 2.75 km. They are
    // two places: GeoSphere has a station on the airfield as well, which
    // these recordings do not hold. What happens where they do is tested
    // further down, with a store that has all three.
    assert_eq!(stations.stations.len(), 12);
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
    assert_eq!(credited.len(), 4, "one credit per source, not per station");
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
        "thermal_hours_wind.json",
        "route_weather.json",
        "radar.json",
        "live_others.json",
        "error_missing_bbox.json",
        "sign_in.json",
        "sessions.json",
        "account.json",
        "upload.json",
        "flight_changes.json",
        "flight.json",
        "public_flights.json",
        "public_flights_best.json",
        "public_flight_changes.json",
        "flight_track.json",
        "ranking.json",
        "pilots.json",
        "pilot_statistics.json",
        "report_request.json",
        "report_group_request.json",
        "report_plan_request.json",
        "report_site_request.json",
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
        "sites_venti.json",
        "site.json",
        "site_of_a_source.json",
        "site_history.json",
        "site_changes.json",
        "site_add_request.json",
        "site_change_request.json",
        "site_written.json",
        "site_restore_request.json",
        "tokens.json",
        "token_create_request.json",
        "token_created.json",
        "error_insufficient_scope.json",
        "site_too_close.json",
        "kept_site_changes.json",
        "kept_site_keep_request.json",
        "kept_site.json",
        "sites_venti_pilots.json",
        "site_confirm_request.json",
        "site_confirmations.json",
        "site_flights.json",
        "site_candidates.json",
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
    // The sites that pilots made: the made-up ones, the licence as the
    // server names it in one place, and the numbers that bound the layer.
    use venti_server::pilot_sites;
    for site in dev::DEMO_SITES {
        assert!(docs.contains(&format!("| `{site}` |")), "{site}");
    }
    let licence = pilot_sites::Layer::Partners.licence();
    let pilots = pilot_sites::Layer::Pilots.licence();
    for stated in [
        format!("[{}]({})", licence.name, licence.url),
        format!("provider \"{}\"", licence.provider),
        licence.notice.to_owned(),
        format!("provider \"{}\"", pilots.provider),
        pilots.notice.to_owned(),
        format!("bbox={}", dev::DEMO_SITES_BBOX),
        format!("less than {} m", pilot_sites::APART_M),
        format!(
            "adds at most {} sites a day",
            pilot_sites::Limits::default().added_per_day
        ),
        format!(
            "at most {} revisions a day",
            pilot_sites::Limits::default().revisions_per_day
        ),
        format!(
            "at most {} characters, kept with the revision",
            venti_api_types::v1::MAX_SITE_COMMENT_LENGTH
        ),
    ] {
        assert!(docs.contains(&stated), "docs/api.md does not say: {stated}");
    }
    // The numbers that bound the sites a pilot keeps.
    for stated in [
        format!(
            "An account keeps at most {} sites",
            venti_api_types::v1::MAX_KEPT_SITES
        ),
        format!(
            "A body has at most {} kB",
            venti_api_types::v1::MAX_KEPT_SITE_BODY_BYTES / 1024
        ),
        format!(
            "each text of a credit at most {} characters",
            venti_api_types::v1::MAX_KEPT_CREDIT_LENGTH
        ),
    ] {
        assert!(docs.contains(&stated), "docs/api.md does not say: {stated}");
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

const ELSEWHERE: &str = "https://elsewhere.example";

fn preflight(path: &str, origin: &str, method: &str, headers: Option<&str>) -> Request<Body> {
    let mut request = Request::builder()
        .method(Method::OPTIONS)
        .uri(path)
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, method);
    if let Some(headers) = headers {
        request = request.header(header::ACCESS_CONTROL_REQUEST_HEADERS, headers);
    }
    request.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn the_configured_web_origin_may_do_everything_from_a_browser() {
    let router = recorded().await;

    let ours = get_with(&router, "/v1/health", &[(header::ORIGIN, WEB_APP)]).await;
    assert_eq!(ours.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);

    // What a browser asks before a request with a validator.
    let asked = preflight(&first_region(), WEB_APP, "GET", Some("if-none-match"));
    let asked = send(&router, asked).await;
    assert_eq!(asked.status, StatusCode::OK);
    assert_eq!(asked.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
    assert!(
        asked
            .header(header::ACCESS_CONTROL_ALLOW_METHODS)
            .contains("GET")
    );
    assert!(
        asked
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .contains("if-none-match")
    );

    // And before one that writes for an account.
    let asked = preflight(
        "/v1/flights",
        WEB_APP,
        "POST",
        Some("authorization, content-type"),
    );
    let asked = send(&router, asked).await;
    assert_eq!(asked.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
    assert!(
        asked
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .contains("authorization")
    );
}

#[tokio::test]
async fn a_page_of_any_origin_may_read_what_anyone_may_read() {
    let router = recorded().await;

    for path in [
        "/v1/health".to_owned(),
        first_region(),
        "/v1/flights/public".to_owned(),
        "/v1/openapi.json".to_owned(),
    ] {
        let read = get_with(&router, &path, &[(header::ORIGIN, ELSEWHERE)]).await;
        assert_eq!(read.status, StatusCode::OK, "{path}");
        assert_eq!(
            read.header(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "*",
            "{path}"
        );
        // Never with the browser's own credentials.
        assert!(
            !read
                .headers
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        );
        // The headers a reader needs: to ask again only for what changed,
        // and to wait as long as it is told.
        let exposed = read.header(header::ACCESS_CONTROL_EXPOSE_HEADERS);
        assert!(exposed.contains("etag"), "{exposed}");
        assert!(exposed.contains("retry-after"), "{exposed}");
        // A cache keeps the answers to different origins apart.
        assert!(read.header_values(header::VARY).contains("origin"));
    }

    // An error is read as well: a page can tell its visitor what went wrong.
    let refused = get_with(&router, "/v1/stations", &[(header::ORIGIN, ELSEWHERE)]).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(refused.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");

    // The pages for people are not part of the API and stay as they were.
    let page = get_with(
        &router,
        "/f/0123456789abcdefghjkmnpqrs",
        &[(header::ORIGIN, ELSEWHERE)],
    )
    .await;
    assert!(
        !page
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}

#[tokio::test]
async fn a_preflight_from_any_origin_opens_reads_and_nothing_else() {
    let router = recorded().await;

    // A read with a validator: allowed, and only as a read.
    for headers in [Some("if-none-match"), Some("If-None-Match, Range"), None] {
        let asked = send(
            &router,
            preflight(&first_region(), ELSEWHERE, "GET", headers),
        )
        .await;
        assert_eq!(asked.status, StatusCode::NO_CONTENT);
        assert_eq!(asked.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");
        assert_eq!(
            asked.header(header::ACCESS_CONTROL_ALLOW_METHODS),
            "GET, HEAD"
        );
        // A browser remembers what a preflight allowed: this one must not
        // allow a token or a JSON body, or the next request of the page
        // could carry one unasked.
        assert_eq!(
            asked.header(header::ACCESS_CONTROL_ALLOW_HEADERS),
            "if-none-match, range, if-range"
        );
        assert!(
            !asked
                .headers
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        );
        assert!(asked.headers.contains_key(header::ACCESS_CONTROL_MAX_AGE));
    }

    // A read with a token, a write, a JSON body: as before, not allowed.
    for (path, method, headers) in [
        ("/v1/flights", "GET", Some("authorization")),
        ("/v1/flights", "GET", Some("if-none-match, Authorization")),
        ("/v1/sessions", "POST", Some("content-type")),
        ("/v1/sessions", "GET", Some("content-type")),
        ("/v1/flights/0123456789abcdefghjkmnpqrs", "DELETE", None),
        ("/v1/account/password", "PUT", Some("authorization")),
    ] {
        let asked = send(&router, preflight(path, ELSEWHERE, method, headers)).await;
        assert!(
            !asked
                .headers
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "{method} {path} with {headers:?}"
        );
    }
}

#[tokio::test]
async fn a_signed_request_from_another_origin_is_as_it_was() {
    let router = recorded().await;
    let token = format!("Bearer {}", venti_server::dev::DEMO_TOKEN);

    // The server answers, as it answers any program; the browser of a
    // page on another site is told nothing that lets the page read it.
    let signed = get_with(
        &router,
        "/v1/flights",
        &[
            (header::ORIGIN, ELSEWHERE),
            (header::AUTHORIZATION, token.as_str()),
        ],
    )
    .await;
    assert_eq!(signed.status, StatusCode::OK);
    assert!(
        !signed
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    signed.is_private();

    // A public read with a token on it is no open read either.
    let signed = get_with(
        &router,
        "/v1/health",
        &[
            (header::ORIGIN, ELSEWHERE),
            (header::AUTHORIZATION, token.as_str()),
        ],
    )
    .await;
    assert!(
        !signed
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );

    // Nor is a write, with or without a token.
    let write = send(
        &router,
        Request::builder()
            .method(Method::POST)
            .uri("/v1/sessions")
            .header(header::ORIGIN, ELSEWHERE)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert!(
        !write
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );

    // The web app's own origin reads its account as before.
    let ours = get_with(
        &router,
        "/v1/flights",
        &[
            (header::ORIGIN, WEB_APP),
            (header::AUTHORIZATION, token.as_str()),
        ],
    )
    .await;
    assert_eq!(ours.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
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

/// A station with its height, which the rule for one place needs.
fn station_at(text: &str, name: &str, lat: f64, lon: f64, height_m: f64) -> Station {
    Station {
        name: name.to_owned(),
        elevation_m: Some(height_m),
        ..station(text, lat, lon)
    }
}

/// A wind of 3 m/s measured so many minutes before the clock's moment.
fn wind(station: &str, minutes_ago: i64) -> Observation {
    let time = adapters::recorded_at() - Duration::minutes(minutes_ago);
    let mut observation = Observation::empty(id(station), time);
    observation.wind_speed_ms = Some(3.0);
    observation
}

/// Innsbruck as two sources gave it on 2026-11-21: GeoSphere's station on
/// the airfield and its station at the university, and the airfield's own
/// report, 0.96 km from the first and 3.03 km from the second. The
/// airfield's station was heard `airfield_minutes_ago`.
async fn innsbruck(airfield_minutes_ago: i64) -> Router {
    let store = MemoryStore::new();
    let airfield = station_at(
        "geosphere-tawes:11121",
        "INNSBRUCK-FLUGHAFEN (AUTOMAT)",
        47.26,
        11.356_667,
        578.0,
    );
    let university = station_at(
        "geosphere-tawes:11320",
        "INNSBRUCK/UNIVERSITAET",
        47.26,
        11.384_167,
        578.0,
    );
    let report = station_at(
        "awc-metar:LOWI",
        "Innsbruck Arpt, TI, AT",
        47.26,
        11.344,
        570.0,
    );
    let heard = [
        wind("geosphere-tawes:11121", airfield_minutes_ago),
        wind("geosphere-tawes:11320", 5),
    ];
    store
        .save_run(&run("geosphere-tawes"), &[airfield, university], &heard)
        .await
        .unwrap();
    store
        .save_run(&run("awc-metar"), &[report], &[wind("awc-metar:LOWI", 15)])
        .await
        .unwrap();

    // The sources of the first region, preferred in its pack's order.
    let registry = adapters::registry();
    let preferred: Vec<SourceId> = [
        "dwd-10min",
        "geosphere-tawes",
        "suedtirol-meteo",
        "awc-metar",
    ]
    .map(|source| SourceId::new(source).unwrap())
    .to_vec();
    let manifests = registry
        .manifests()
        .filter(|manifest| preferred.contains(&manifest.id));
    let clock = Arc::new(FixedClock(adapters::recorded_at()));
    Api::new(Arc::new(store), clock, manifests, settings())
        .with_station_priority(preferred)
        .router(&Cors::Any)
}

/// The identifiers of the stations listed for a box, and whose credit the
/// answer carries.
async fn listed(router: &Router, bbox: &str) -> (Vec<String>, Vec<String>) {
    let answer: StationsResponse = get(router, &format!("/v1/stations?bbox={bbox}"))
        .await
        .json();
    assert!(!answer.truncated);
    let stations = answer
        .stations
        .iter()
        .map(|entry| entry.station.id.to_string())
        .collect();
    let credited = answer
        .attributions
        .iter()
        .map(|attribution| attribution.source.to_string())
        .collect();
    (stations, credited)
}

#[tokio::test]
async fn where_two_sources_measure_at_one_place_one_station_is_listed() {
    let router = innsbruck(5).await;

    // The weather service's station on the airfield stands for the place.
    // The airfield's report is not listed and its source not credited:
    // nothing of it is in the answer. The university is another place.
    let (stations, credited) = listed(&router, "11.2,47.1,11.5,47.4").await;
    assert_eq!(stations, ["geosphere-tawes:11121", "geosphere-tawes:11320"]);
    assert_eq!(credited, ["geosphere-tawes"]);

    // The report is still collected and kept, and its own observations
    // answer by its identifier as before: a link to it, a station an app
    // remembers and the assistant's question about it keep working.
    let own = get(&router, "/v1/stations/awc-metar:LOWI/observations").await;
    assert_eq!(own.status, StatusCode::OK);
    let own: ObservationsResponse = own.json();
    assert_eq!(own.station.id, id("awc-metar:LOWI"));
    assert_eq!(own.observations.len(), 1);
    // Where a box ends changes nothing. This one holds the report's
    // position and ends 400 m west of the station that stands for it: the
    // report is not listed, and the place appears when the map moves on.
    let (stations, credited) = listed(&router, "11.2,47.1,11.35,47.4").await;
    assert!(stations.is_empty() && credited.is_empty());
}

#[tokio::test]
async fn a_station_that_fell_silent_does_not_keep_the_other_back() {
    // GeoSphere's station on the airfield was last heard two hours ago.
    // The report has a wind of the last hour, so it is the one listed.
    let router = innsbruck(120).await;

    let (stations, credited) = listed(&router, "11.2,47.1,11.5,47.4").await;
    assert_eq!(stations, ["awc-metar:LOWI", "geosphere-tawes:11320"]);
    assert_eq!(credited, ["awc-metar", "geosphere-tawes"]);
    // The silent one answers by its identifier all the same.
    let own = get(&router, "/v1/stations/geosphere-tawes:11121/observations").await;
    assert_eq!(own.status, StatusCode::OK);
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
