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
    // Every pressure as its provider reduces it: Bolzano's by the
    // province, the three others' by GeoSphere. Innsbruck's is the
    // recorded one, 1001.7 hPa, where Venti's own reduction of the same
    // reading gave 998.2.
    for station in [&tirol.south, &tirol.north, &tauern.south, &tauern.north] {
        assert_eq!(
            station.pressure.unwrap().reduced_by,
            FoehnReducedBy::Provider,
            "{}",
            station.name
        );
    }
    assert_eq!(tirol.north.pressure.unwrap().sea_level_hpa, 1001.7);
    assert_eq!(tirol.south.pressure.unwrap().sea_level_hpa, 1007.9);
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
        let mut observation = Observation::empty(id, now() - Duration::minutes(*minutes_ago));
        observation.pressure_sea_level_hpa = Some(*hpa);
        save(&store, observation, *elevation_m).await;
    }
    let registry = adapters::registry();
    let clock = Arc::new(FixedClock(now()));
    Api::new(Arc::new(store), clock, registry.manifests(), settings()).router(&Cors::Any)
}

/// Saves one observation of a station at a height as a run of its source.
async fn save(store: &MemoryStore, observation: Observation, elevation_m: f64) {
    let id: StationId = observation.station.clone();
    let station = Station {
        id: id.clone(),
        name: id.to_string(),
        position: GeoPoint::new(47.0, 11.0).unwrap(),
        elevation_m: Some(elevation_m),
    };
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
async fn a_station_that_sends_only_its_own_pressure_is_reduced_here_and_said_so() {
    // The morning of 22 November 2026 at 09:00 UTC: Bolzano 1014.5 hPa by
    // the province; Innsbruck's university 938.7 hPa at the station and
    // 2.8 °C, first without GeoSphere's own reduction and then with it.
    for (provider_hpa, sea_level_hpa, by, level, difference_hpa) in [
        (None, 1007.8, FoehnReducedBy::Venti, FoehnLevel::Likely, 6.7),
        (
            Some(1011.5),
            1011.5,
            FoehnReducedBy::Provider,
            FoehnLevel::Possible,
            3.0,
        ),
    ] {
        let store = MemoryStore::new();
        let mut bolzano =
            Observation::empty(BOLZANO.parse().unwrap(), now() - Duration::minutes(5));
        bolzano.pressure_sea_level_hpa = Some(1014.5);
        let mut innsbruck =
            Observation::empty(INNSBRUCK.parse().unwrap(), now() - Duration::minutes(5));
        innsbruck.pressure_hpa = Some(938.7);
        innsbruck.temperature_c = Some(2.8);
        innsbruck.pressure_sea_level_hpa = provider_hpa;
        for (observation, elevation_m) in [(bolzano, 245.0), (innsbruck, 578.0)] {
            save(&store, observation, elevation_m).await;
        }
        let registry = adapters::registry();
        let clock = Arc::new(FixedClock(now()));
        let router =
            Api::new(Arc::new(store), clock, registry.manifests(), settings()).router(&Cors::Any);
        let foehn: FoehnResponse = get(&router, FOEHN_PATH).await.json();
        let pair = &foehn.pairs[0];
        let north = pair.north.pressure.unwrap();
        assert_eq!(north.reduced_by, by);
        assert_eq!(north.sea_level_hpa, sea_level_hpa);
        assert_eq!(
            pair.south.pressure.unwrap().reduced_by,
            FoehnReducedBy::Provider
        );
        // The server answers with what the pressures say either way; that
        // a number of Venti's own warns nobody is the core's and the
        // tools' to keep, from `reduced_by`.
        assert_eq!(pair.level, level);
        assert_eq!(pair.difference_hpa, Some(difference_hpa));
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
