//! `GET /v1/foehn` through the real router: the development mode's made-up
//! pressures, and stores of the test's own for what must not happen: a
//! level from a stale reading, and a level where nothing is known.

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use chrono::Duration;
use venti_api_types::v1::{
    FOEHN_PATH, FoehnLevel, FoehnReducedBy, FoehnResponse, FoehnSide, FoehnUnknown,
};
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_server::adapters;
use venti_server::api::{Api, Cors};
use venti_server::clock::FixedClock;
use venti_sources::testing::assert_golden;
use venti_store::{MemoryStore, RunRecord, Store};

mod common;

use common::{get, golden, recorded, settings};

#[tokio::test]
async fn the_development_mode_shows_one_pair_at_likely_and_one_at_none() {
    let answer = get(&recorded().await, FOEHN_PATH).await;
    assert_eq!(answer.status, StatusCode::OK);
    let foehn: FoehnResponse = answer.json();
    assert_golden(&golden("foehn.json"), &foehn);
    let [tirol, tauern] = &foehn.pairs[..] else {
        panic!("two pairs");
    };
    assert_eq!(tirol.id, "bolzano-innsbruck");
    assert_eq!(tirol.level, FoehnLevel::Likely);
    assert_eq!(tirol.foehn, Some(FoehnSide::South));
    assert_eq!(tirol.difference_hpa, Some(6.2));
    // Bolzano as the province reduces it, Innsbruck reduced here.
    assert_eq!(
        tirol.south.pressure.unwrap().reduced_by,
        FoehnReducedBy::Provider
    );
    assert_eq!(
        tirol.north.pressure.unwrap().reduced_by,
        FoehnReducedBy::Venti
    );
    assert_eq!(tirol.north.pressure.unwrap().sea_level_hpa, 998.2);
    assert!(tirol.series.len() > 6);

    assert_eq!(tauern.level, FoehnLevel::None);
    assert_eq!(tauern.foehn, None);
    assert!(
        tauern
            .difference_hpa
            .is_some_and(|hpa| (0.5..1.2).contains(&hpa))
    );
    assert!(tauern.to_verify);

    let sources: Vec<&str> = foehn
        .attributions
        .iter()
        .map(|credit| credit.source.as_str())
        .collect();
    assert_eq!(sources, ["geosphere-tawes", "suedtirol-meteo"]);
    assert!(foehn.caution.contains("not a measurement"));
}

fn now() -> Timestamp {
    adapters::recorded_at()
}

/// A server over a store that holds the given pressures: each a station,
/// its height, minutes before now, and the pressure at sea level as the
/// provider would send it.
async fn server_with(readings: &[(&str, f64, i64, f64)]) -> Router {
    let store = MemoryStore::new();
    for (id, elevation_m, minutes_ago, hpa) in readings {
        let id: StationId = id.parse().unwrap();
        let station = Station {
            id: id.clone(),
            name: id.to_string(),
            position: GeoPoint::new(47.0, 11.0).unwrap(),
            elevation_m: Some(*elevation_m),
        };
        let mut observation =
            Observation::empty(id.clone(), now() - Duration::minutes(*minutes_ago));
        observation.pressure_sea_level_hpa = Some(*hpa);
        let run = RunRecord {
            source: SourceId::new(id.source().as_str()).unwrap(),
            started_at: now(),
            finished_at: now(),
            healthy: true,
            requests: 1,
            failed_requests: 0,
            skipped: 0,
            rejected: 0,
            stations: 1,
            observations: 1,
            newest_observation: Some(observation.time),
            airspaces: 0,
        };
        store
            .save_run(&run, &[station], &[observation])
            .await
            .unwrap();
    }
    let registry = adapters::registry();
    let clock = Arc::new(FixedClock(now()));
    Api::new(Arc::new(store), clock, registry.manifests(), settings()).router(&Cors::Any)
}

const BOLZANO: &str = "suedtirol-meteo:83200MS";
const INNSBRUCK: &str = "geosphere-tawes:11320";

#[tokio::test]
async fn a_stale_reading_gives_no_level_however_large_the_difference() {
    // Nine hectopascals would be "likely": Bolzano's reading is 70 minutes
    // old.
    let router = server_with(&[(BOLZANO, 245.0, 70, 1024.0), (INNSBRUCK, 578.0, 5, 1015.0)]).await;
    let foehn: FoehnResponse = get(&router, FOEHN_PATH).await.json();
    let pair = &foehn.pairs[0];
    assert_eq!(pair.level, FoehnLevel::Unknown);
    assert_eq!(pair.unknown_because, Some(FoehnUnknown::SouthOld));
    assert_eq!(pair.difference_hpa, None);
    assert_eq!(pair.foehn, None);
    // The old reading is still shown as what it is, with its time.
    assert_eq!(pair.south.pressure.unwrap().sea_level_hpa, 1024.0);
}

#[tokio::test]
async fn where_nothing_is_known_the_level_is_unknown_and_never_none() {
    // A server that holds one of the four stations, and one that holds
    // nothing.
    for readings in [&[(INNSBRUCK, 578.0, 5, 1015.0)][..], &[]] {
        let router = server_with(readings).await;
        let answer = get(&router, FOEHN_PATH).await;
        assert_eq!(answer.status, StatusCode::OK);
        let foehn: FoehnResponse = answer.json();
        assert_eq!(foehn.pairs.len(), 2);
        for pair in &foehn.pairs {
            assert_eq!(pair.level, FoehnLevel::Unknown, "{}", pair.id);
            assert!(pair.unknown_because.is_some());
            assert!(pair.series.is_empty());
        }
        assert_eq!(
            foehn.pairs[0].unknown_because,
            Some(FoehnUnknown::SouthMissing)
        );
    }
}

#[tokio::test]
async fn north_foehn_is_the_higher_pressure_in_the_north() {
    let router = server_with(&[(BOLZANO, 245.0, 10, 1011.0), (INNSBRUCK, 578.0, 5, 1015.0)]).await;
    let foehn: FoehnResponse = get(&router, FOEHN_PATH).await.json();
    let pair = &foehn.pairs[0];
    assert_eq!(pair.level, FoehnLevel::Possible);
    assert_eq!(pair.difference_hpa, Some(-4.0));
    assert_eq!(pair.higher, Some(FoehnSide::North));
    assert_eq!(pair.foehn, Some(FoehnSide::North));
}
