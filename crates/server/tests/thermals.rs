//! Tests of the thermal map's endpoint through the real router.
//!
//! The development mode makes its map from answers recorded from an
//! Open-Meteo instance for four places in one request, by the code a real
//! server runs; its answer is kept as a golden file, which is also the
//! example in `docs/api.md`. The recorded day is in November: weak
//! thermals where there are any, and that is what the map says.

use std::sync::Arc;

use axum::http::{StatusCode, header};
use venti_api_types::v1::{
    ErrorCode, THERMALS_PATH, ThermalKind, ThermalMapResponse, ThermalVerdict,
};
use venti_regions::RegionPack;
use venti_server::adapters::{self, Providers};
use venti_server::api::{Api, Cors};
use venti_server::clock::{Clock, FixedClock};
use venti_server::forecast::{self, Asking};
use venti_server::ingest::{Collector, DirArchive};
use venti_server::plan;
use venti_server::thermals::ThermalMap;
use venti_sources::testing::assert_golden;
use venti_store::MemoryStore;

mod common;

use common::{TestClock, get, get_with, golden, recorded, server, settings};

const FIRST_REGION: &str = include_str!("../../../regions/alps-de-at/region.toml");

#[tokio::test]
async fn the_development_mode_serves_the_map_of_the_recorded_places() {
    let router = recorded().await;

    let answer = get(&router, THERMALS_PATH).await;
    let map: ThermalMapResponse = answer.ok(StatusCode::OK);

    // Cacheable like every answer that is the same for everyone.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    let etag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, THERMALS_PATH, &[(header::IF_NONE_MATCH, &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    // Compressed for a client that takes it, as every answer of the API:
    // the map of the first region is some 126 kB, and 10 kB packed.
    let packed = get_with(&router, THERMALS_PATH, &[(header::ACCEPT_ENCODING, "gzip")]).await;
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");

    assert_eq!(map.source.as_str(), "open-meteo");
    assert_eq!(map.model.as_str(), "icon-d2");
    assert_eq!(map.run, adapters::thermal_map_recorded_run());
    assert_eq!(map.computed_at, adapters::thermal_map_recorded_at());
    assert_eq!(map.grid, adapters::recorded_thermal_grid());
    assert_eq!(
        map.ground_m,
        [Some(1118), Some(1046), Some(1069), Some(947)]
    );
    assert_eq!(map.attributions.len(), 1);
    assert!(map.attributions[0].text.contains("Open-Meteo.com"));

    // The run of midnight holds today and tomorrow, both whole.
    let dates: Vec<String> = map.days.iter().map(|day| day.date.to_string()).collect();
    assert_eq!(dates, ["2026-11-21", "2026-11-22"]);
    for day in &map.days {
        assert!(day.whole, "{}", day.date);
        assert_eq!(day.verdict.len(), 4);
        assert_eq!(day.thermals.len(), 4);
        assert_eq!(day.climb_ms.len(), 4);
        assert_eq!(day.top_m.len(), 4);
        assert_eq!(day.cloud.len(), 4);
        for cell in 0..4 {
            // A day that is not flyable says nothing of thermals, and a
            // November day has no thermals but weak ones.
            match day.verdict[cell] {
                Some(ThermalVerdict::NotFlyable) | None => {
                    assert_eq!(day.thermals[cell], None);
                }
                Some(_) => assert!(matches!(day.thermals[cell], Some(ThermalKind::Weak) | None)),
            }
            if let Some(climb) = day.climb_ms[cell] {
                assert_eq!(climb, (climb * 10.0).round() / 10.0);
            }
            if let Some(top) = day.top_m[cell] {
                assert_eq!(top % 50, 0);
            }
        }
    }

    assert_golden(&golden("thermals.json"), &map);
}

#[tokio::test]
async fn a_server_without_forecasts_has_no_thermal_map() {
    let (router, _) = server(settings(), &TestClock::new());

    let answer = get(&router, THERMALS_PATH).await;
    let message = answer.error(StatusCode::NOT_IMPLEMENTED, ErrorCode::NoForecasts);
    assert!(message.contains("no thermal map"), "{message}");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
}

#[tokio::test]
async fn a_server_with_forecasts_and_no_map_yet_says_when_to_ask_again() {
    let providers = Providers::recorded();
    let registry = adapters::registry_for(&providers).unwrap();
    let pack = RegionPack::from_toml(FIRST_REGION).unwrap();
    let jobs = plan::plan(&[pack], &registry).unwrap();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::forecasts_recorded_at()));
    let forecasts = forecast::service_for(
        &jobs,
        adapters::forecast_sources(&providers).unwrap(),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock.clone(),
        Asking::Live,
    )
    .unwrap();
    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let router = Api::new(Arc::new(MemoryStore::new()), clock, manifests, settings())
        .with_forecasts(forecasts)
        .router(&Cors::Origins(Vec::new()));

    let answer = get(&router, THERMALS_PATH).await;
    let message = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
    assert!(message.contains("not made yet"), "{message}");
    assert_eq!(answer.header(header::RETRY_AFTER), "60");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
}

#[tokio::test]
async fn a_look_that_sees_a_run_has_the_map_made() {
    let providers = Providers::recorded();
    let registry = adapters::registry_for(&providers).unwrap();
    let pack = RegionPack::from_toml(FIRST_REGION).unwrap();
    let jobs = plan::plan(&[pack], &registry).unwrap();
    let job = jobs
        .iter()
        .find(|job| job.kind == venti_sources::Kind::Forecasts)
        .unwrap()
        .clone();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::thermal_map_recorded_at()));
    let service = |fetch| {
        forecast::service_for(
            &jobs,
            adapters::forecast_sources(&providers).unwrap(),
            fetch,
            None::<DirArchive>,
            clock.clone(),
            Asking::Recorded(adapters::recorded_forecast_places()),
        )
        .unwrap()
    };
    // The look asks the answers for single places, the map those for
    // four: the two were recorded hours apart.
    let looked_at = service(adapters::recorded_payloads());
    let map = ThermalMap::on_grid(
        service(adapters::recorded_thermal_payloads()),
        job.source.clone(),
        adapters::recorded_thermal_grid(),
        clock.clone(),
    )
    .map(Arc::new)
    .unwrap();
    let collector = Collector::new(
        registry,
        Arc::new(MemoryStore::new()),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock,
        None,
    )
    .with_forecasts(looked_at)
    .with_thermal_map(map.clone());

    assert!(map.current().is_none());
    assert!(collector.collect(&job).await);
    // The map is made in a task of its own.
    for _ in 0..1000 {
        if map.current().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let made = map.current().expect("the map is made after the look");
    assert_eq!(made.run, adapters::thermal_map_recorded_run());
}
