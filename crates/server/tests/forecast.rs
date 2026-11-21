//! Tests of the forecast endpoint through the real router.
//!
//! Most run against what the development mode serves: answers recorded
//! from an Open-Meteo instance for five places, read by the real adapter.
//! One answer, cut short, is kept as a golden file, which is also the
//! example in `docs/api.md`. The others give the forecast service a source
//! that answers less, to see what a pilot is told then.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::{Duration, TimeZone, Utc};
use venti_api_types::v1::{
    ErrorCode, ForecastRequest, ForecastResponse, ForecastsHealth, HealthResponse,
};
use venti_domain::GeoPoint;
use venti_regions::RegionPack;
use venti_server::accounts::Limits;
use venti_server::adapters::{self, Providers};
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::{Clock, FixedClock};
use venti_server::forecast::{self, Asking};
use venti_server::ingest::DirArchive;
use venti_server::plan;
use venti_sources::testing::{FixtureFetch, assert_golden};
use venti_store::MemoryStore;

mod common;

use common::{TestClock, ask, get, get_with, golden, recorded, server, settings};

// --- Helpers ----------------------------------------------------------------

const FIRST_REGION: &str = include_str!("../../../regions/alps-de-at/region.toml");

/// The east shore of the Tegernsee: one of the recorded places.
const TEGERNSEE: &str = "/v1/forecast?lat=47.7&lon=11.8";

fn place(lat: f64, lon: f64) -> String {
    ForecastRequest {
        point: GeoPoint::new(lat, lon).unwrap(),
    }
    .path_and_query()
}

/// A server whose forecast service asks `fetch` and nothing else, as a
/// live one: any place of the first region, each asked for as it is.
fn server_asking(fetch: FixtureFetch) -> Router {
    let clock = Arc::new(FixedClock(adapters::forecasts_recorded_at()));
    server_asking_with(fetch, settings(), clock)
}

/// The same with settings and a clock of the test's choosing.
fn server_asking_with(fetch: FixtureFetch, settings: Settings, clock: Arc<dyn Clock>) -> Router {
    let providers = Providers::recorded();
    let registry = adapters::registry_for(&providers).unwrap();
    let pack = RegionPack::from_toml(FIRST_REGION).unwrap();
    let jobs = plan::plan(&[pack], &registry).unwrap();
    let forecasts = forecast::service_for(
        &jobs,
        adapters::forecast_sources(&providers).unwrap(),
        fetch,
        None::<DirArchive>,
        clock.clone(),
        Asking::Live,
    )
    .unwrap();
    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    Api::new(Arc::new(MemoryStore::new()), clock, manifests, settings)
        .with_forecasts(forecasts)
        .router(&Cors::Origins(Vec::new()))
}

/// The recorded answers without those whose address contains `part`.
fn recorded_without(part: &str) -> FixtureFetch {
    let kept = venti_sources_openmeteo::fixtures::recorded()
        .into_iter()
        .filter(|(url, _)| !url.contains(part));
    FixtureFetch::new().with_all(kept)
}

/// The answer with every series cut to its first hours: the form the
/// golden file and the documentation show it in.
fn cut(mut answer: ForecastResponse, hours: usize) -> ForecastResponse {
    for layer in &mut answer.layers {
        for series in &mut layer.series {
            series.hours.truncate(hours);
        }
    }
    answer
}

// --- The endpoint over the recorded answers ----------------------------------

#[tokio::test]
async fn the_forecast_of_a_place_is_one_series_per_model_with_its_run_and_its_credit() {
    let answer = get(&recorded().await, TEGERNSEE).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    // A whole answer: two models, 74 hours, under 100 kB before compression.
    assert!(answer.body.len() < 100_000, "{} bytes", answer.body.len());
    let forecast: ForecastResponse = answer.json();

    assert_eq!(forecast.point, GeoPoint::new(47.7, 11.8).unwrap());
    // The forecasts were recorded some days after the wind; their clock
    // stands at that moment.
    assert_eq!(forecast.fetched_at, adapters::forecasts_recorded_at());
    assert_eq!(forecast.layers.len(), 1);
    let layer = &forecast.layers[0];
    assert_eq!(layer.source.as_str(), "open-meteo");
    let series: Vec<(&str, String, usize)> = layer
        .series
        .iter()
        .map(|series| {
            (
                series.model.as_str(),
                series.run.to_rfc3339(),
                series.hours.len(),
            )
        })
        .collect();
    assert_eq!(
        series,
        [
            ("icon-d2", "2026-11-20T21:00:00+00:00".to_owned(), 46),
            ("icon-eu", "2026-11-20T21:00:00+00:00".to_owned(), 28),
        ]
    );
    for series in &layer.series {
        // Every series says where its values are from, and all of its hours
        // are its run's.
        assert_eq!(series.source, layer.source);
        assert_eq!(series.point, forecast.point);
        assert!(series.cell_elevation_m.is_some());
        assert!(series.available_at.unwrap() > series.run);
        assert!(series.hours.iter().all(|hour| hour.time > series.run));
        assert!(series.hours[0].wind_speed_ms.is_some());
        assert!(!series.hours[0].winds_aloft.is_empty());
    }

    // The credit names the service and the weather service whose models
    // these are, under an open licence.
    assert_eq!(forecast.attributions.len(), 1);
    let credit = &forecast.attributions[0];
    assert_eq!(credit.source, layer.source);
    assert_eq!(credit.licence, "CC BY 4.0");
    assert!(credit.text.contains("Open-Meteo.com"));
    assert!(credit.text.contains("Deutscher Wetterdienst"));
    assert_eq!(credit.url, "https://open-meteo.com/");

    // Nothing in the answer is a verdict, and nothing is null.
    let text = String::from_utf8(answer.body.clone()).unwrap();
    assert!(!text.contains("null"));
    assert!(!text.contains("flyable"));

    assert_golden(&golden("forecast.json"), &cut(forecast, 1));
}

#[tokio::test]
async fn an_answer_is_validated_like_every_other() {
    let router = recorded().await;
    let first = get(&router, TEGERNSEE).await;
    let tag = first.header(header::ETAG).to_owned();

    let again = get_with(&router, TEGERNSEE, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());

    // Any place in the same cell of the grid is the same answer.
    let nearby = get(&router, &place(47.7049, 11.7951)).await;
    assert_eq!(nearby.header(header::ETAG), tag);
}

#[tokio::test]
async fn the_development_mode_answers_any_place_of_the_region_with_the_nearest_recording() {
    let router = recorded().await;

    // Each recorded place is answered for itself: the take-off on the
    // Wallberg, by its own position.
    let wallberg: ForecastResponse = get(&router, &place(47.6652, 11.7954)).await.json();
    assert_eq!(wallberg.point, GeoPoint::new(47.66, 11.8).unwrap());
    let icon_d2 = &wallberg.layers[0].series[0];
    assert_eq!(icon_d2.cell_elevation_m, Some(1266.0));

    // A place nothing was recorded for gets the forecast of the nearest
    // recorded place, and the answer says which place that is: the
    // Zugspitze gets the Wallberg's.
    let zugspitze: ForecastResponse = get(&router, &place(47.4211, 10.9853)).await.json();
    assert_eq!(zugspitze.point, wallberg.point);
    assert_eq!(zugspitze.layers, wallberg.layers);
    // And Vienna the one of the middle of the region.
    let vienna: ForecastResponse = get(&router, &place(48.2, 16.37)).await.json();
    assert_eq!(vienna.point, GeoPoint::new(47.4, 13.3).unwrap());

    // The hours a client finds: none of 18 November, the day the wind of
    // this mode is of. All of the 21st, and of the 22nd what ICON-D2's
    // run of the evening of the 20th reaches.
    let at = |day, hour| Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap();
    assert_eq!(icon_d2.hours.first().unwrap().time, at(21, 0));
    assert_eq!(icon_d2.hours.last().unwrap().time, at(22, 21));
    let noon = icon_d2.hours.iter().find(|hour| hour.time == at(21, 11));
    assert!(noon.is_some_and(|hour| hour.temperature_c.is_some()));
}

#[tokio::test]
async fn a_place_that_is_none_or_lies_outside_the_regions_is_refused() {
    let router = recorded().await;
    let refused = |path: &'static str| {
        let router = router.clone();
        async move {
            let answer = get(&router, path).await;
            assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
            answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPosition)
        }
    };

    let message = refused("/v1/forecast").await;
    assert!(message.contains("lat parameter is missing"), "{message}");
    assert!(message.contains("lat=47.7&lon=11.8"), "{message}");
    let message = refused("/v1/forecast?lat=47.7").await;
    assert!(message.contains("lon parameter is missing"), "{message}");
    let message = refused("/v1/forecast?lat=north&lon=11.8").await;
    assert!(message.contains("\"north\" is not a number"), "{message}");
    refused("/v1/forecast?lat=147.7&lon=11.8").await;
    refused("/v1/forecast?lat=NaN&lon=11.8").await;
    refused("/v1/forecast?lat=47.7&lon=inf").await;
    // An address where a number belongs is one more thing that is not a
    // number. It is not asked, and not repeated back at length.
    let metadata = "/v1/forecast?lat=http://169.254.169.254/latest/meta-data/iam/security-credentials/&lon=11.8";
    let message = refused(metadata).await;
    assert!(!message.contains("security-credentials"), "{message}");

    // Hamburg is on the globe and in no region of this server.
    let answer = get(&router, &place(53.55, 10.0)).await;
    let message = answer.error(StatusCode::NOT_FOUND, ErrorCode::OutsideRegion);
    assert!(message.contains("outside the area"), "{message}");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
}

// --- Servers with less ---------------------------------------------------------

#[tokio::test]
async fn a_server_that_was_not_told_where_its_forecast_service_is_has_no_forecasts() {
    // A server with no forecast source: the health report and the endpoint
    // say the same, and nobody else is asked.
    let (router, _) = server(settings(), &TestClock::new());

    let answer = get(&router, TEGERNSEE).await;
    let message = answer.error(StatusCode::NOT_IMPLEMENTED, ErrorCode::NoForecasts);
    assert!(message.contains("no forecast service"), "{message}");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    assert!(answer.headers.get(header::RETRY_AFTER).is_none());

    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert_eq!(health.forecasts, Some(ForecastsHealth::NotConfigured));

    // The same for a server that has the service and serves no source of
    // forecasts: without a manifest there is no licence to serve under.
    let registry = adapters::registry();
    let pack = RegionPack::from_toml(FIRST_REGION).unwrap();
    let with_forecasts = plan::plan(
        std::slice::from_ref(&pack),
        &adapters::registry_for(&Providers::recorded()).unwrap(),
    )
    .unwrap();
    let without = plan::plan(std::slice::from_ref(&pack), &registry).unwrap();
    assert_eq!(with_forecasts.len(), without.len() + 1);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::forecasts_recorded_at()));
    let service = forecast::service_for(
        &with_forecasts,
        adapters::forecast_sources(&Providers::recorded()).unwrap(),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock.clone(),
        Asking::Live,
    )
    .unwrap();
    let manifests = without.iter().map(|job| job.manifest(&registry));
    let router = Api::new(Arc::new(MemoryStore::new()), clock, manifests, settings())
        .with_forecasts(service)
        .router(&Cors::Origins(Vec::new()));
    get(&router, TEGERNSEE)
        .await
        .error(StatusCode::NOT_IMPLEMENTED, ErrorCode::NoForecasts);
}

#[tokio::test]
async fn one_client_cannot_use_up_what_the_forecast_service_may_be_asked() {
    // A clock the test moves, set to when the forecasts were recorded.
    let clock = TestClock::new();
    clock.advance(adapters::forecasts_recorded_at() - adapters::recorded_at());
    let settings = Settings {
        limits: Limits {
            forecast_fetches_per_address: 2,
            forecast_fetches_per_network: 3,
            ..Limits::default()
        },
        ..settings()
    };
    let fetch = FixtureFetch::new().with_all(venti_sources_openmeteo::fixtures::recorded());
    let router = server_asking_with(fetch, settings, Arc::new(clock.clone()));
    let ask_from = |peer: &'static str, lat: f64, lon: f64| {
        let router = router.clone();
        async move {
            ask(Method::GET, &place(lat, lon))
                .from(peer)
                .send(&router)
                .await
        }
    };
    // The five places the recorded service answers for.
    let (tegernsee, wallberg) = ((47.70, 11.80), (47.66, 11.80));
    let (takeoff, landing, middle) = ((47.68, 11.90), (47.70, 11.88), (47.40, 13.30));
    const PILOT: &str = "198.51.100.7";

    // Two places nobody has asked for: the server fetches them for this
    // client, and that is as many as one client may have fetched.
    for (lat, lon) in [tegernsee, wallberg] {
        assert_eq!(ask_from(PILOT, lat, lon).await.status, StatusCode::OK);
    }
    let answer = ask_from(PILOT, takeoff.0, takeoff.1).await;
    let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("new places"), "{message}");
    assert_eq!(answer.header(header::RETRY_AFTER), "3600");

    // What is at hand costs nothing, and the same client gets it as often
    // as it likes: the places it asked for, and anywhere in their two
    // kilometres.
    for _ in 0..20 {
        let answer = ask_from(PILOT, tegernsee.0, tegernsee.1).await;
        assert_eq!(answer.status, StatusCode::OK);
    }
    assert_eq!(ask_from(PILOT, 47.703, 11.797).await.status, StatusCode::OK);
    // Asking for what the server does not answer at all is not counted
    // either, and stays what it was.
    let answer = ask_from(PILOT, 52.5, 13.4).await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::OutsideRegion);

    // Somebody else asks for the third place. It is fetched for them, and
    // then it is at hand for the first client too.
    assert_eq!(
        ask_from("203.0.113.50", takeoff.0, takeoff.1).await.status,
        StatusCode::OK
    );
    assert_eq!(
        ask_from(PILOT, takeoff.0, takeoff.1).await.status,
        StatusCode::OK
    );
    // A fourth place is still one too many for the first.
    let answer = ask_from(PILOT, landing.0, landing.1).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);

    // An hour later the count starts again. By then nothing is at hand
    // any more, so every place is a new one.
    clock.advance(Duration::hours(1));
    assert_eq!(
        ask_from(PILOT, landing.0, landing.1).await.status,
        StatusCode::OK
    );

    // Whoever has many IPv6 networks is one client here as elsewhere:
    // three places for all of a /48, whichever of its networks asks.
    let networks = ["2001:db8:7:1::1", "2001:db8:7:2::1", "2001:db8:7:3::1"];
    for (peer, (lat, lon)) in networks.into_iter().zip([tegernsee, wallberg, takeoff]) {
        assert_eq!(ask_from(peer, lat, lon).await.status, StatusCode::OK);
    }
    let answer = ask_from("2001:db8:7:4::1", middle.0, middle.1).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    let answer = ask_from("2001:db8:8:1::1", middle.0, middle.1).await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn a_forecast_service_that_does_not_answer_is_said_and_asked_again_later() {
    // The instance is down: every request fails.
    let router = server_asking(FixtureFetch::new());

    let answer = get(&router, TEGERNSEE).await;
    let message = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
    assert!(message.contains("did not answer"), "{message}");
    assert_eq!(answer.header(header::RETRY_AFTER), "30");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");

    // A live service has answers for the places it is asked for and no
    // others: a place nothing was recorded for is not given a neighbour's.
    let router =
        server_asking(FixtureFetch::new().with_all(venti_sources_openmeteo::fixtures::recorded()));
    let answer = get(&router, &place(47.4211, 10.9853)).await;
    answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
    let whole: ForecastResponse = get(&router, TEGERNSEE).await.json();
    assert_eq!(whole.layers[0].series.len(), 2);
}

#[tokio::test]
async fn a_model_the_service_cannot_date_is_left_out_and_the_other_is_served() {
    // The instance does not say which run of ICON-EU it holds.
    let router = server_asking(recorded_without("dwd_icon_eu_native/static/meta.json"));

    let forecast: ForecastResponse = get(&router, TEGERNSEE).await.json();

    let models: Vec<&str> = forecast.layers[0]
        .series
        .iter()
        .map(|series| series.model.as_str())
        .collect();
    assert_eq!(models, ["icon-d2"]);
    assert_eq!(forecast.attributions.len(), 1);
    // Such an answer is handed on and not kept for long; the service's own
    // tests hold it to that.
    assert!(forecast::KEEP_PARTIAL < Duration::minutes(5));
    assert!(forecast::KEEP >= Duration::minutes(30));
}
