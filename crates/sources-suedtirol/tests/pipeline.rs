//! Runs the real pipeline over answers recorded from the web service of the
//! Autonomous Province of Bolzano - South Tyrol on 2026-11-21, and over the
//! sample of the evening of 2026-11-18, and compares the results with
//! golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_suedtirol::{SuedtirolMeteo, fixtures, sensors_url, stations_url};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Half a minute after the whole answers were recorded.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 10, 37, 0).unwrap()
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive, now: Timestamp) -> RunOutput {
    // The first region's area. Its southern edge, 46.3° N, crosses South
    // Tyrol's southern tip.
    let ctx = PlanCtx {
        now,
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    };
    block_on(run_observations(
        &SuedtirolMeteo::new(),
        fetch,
        archive,
        &ctx,
        &RunOptions::default(),
    ))
}

#[test]
fn the_whole_recorded_answers_match_the_golden_file() {
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations.json"))
        .with(sensors_url(), fixture("sensors.json"));
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, recorded_at());

    // Two requests, however many stations there are.
    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 2);

    // Of the 230 stations of the list, 97 measure weather, and one of
    // those, Salurn at 46.24° N, lies south of the area. The gauges of
    // rivers, lakes and groundwater are not stations of wind.
    assert_eq!(output.stations.len(), 96);
    let with_wind = |output: &RunOutput| {
        output
            .observations
            .iter()
            .filter(|o| o.wind_speed_ms.is_some())
            .count()
    };
    assert_eq!(with_wind(&output), 81);
    // One anemometer more is in the list and had no value: the service
    // gave the sensors of Prags Rossalm without one.
    assert!(
        !output
            .observations
            .iter()
            .any(|o| o.station.local() == "42830SF")
    );
    // Nothing was unreadable. What the pipeline left out is Salurn's
    // values, which the service sends with the rest.
    assert_eq!(output.report.skipped.len(), 1);
    // The values are raw: two hygrometers in the Pustertal read 100.3 %
    // and 100.1 %. Validation removes those two values and nothing else.
    let rejected: Vec<(&str, f64)> = output
        .report
        .rejected
        .iter()
        .map(|r| (r.field.as_str(), r.value))
        .collect();
    assert_eq!(rejected, [("humidity_pct", 100.3), ("humidity_pct", 100.1)]);
    assert!(output.report.skipped[0].what.ends_with(":88820MS"));

    // The service's pressure is reduced to sea level and is not taken, and
    // it has no direction for the gust.
    assert!(output.observations.iter().all(|o| o.pressure_hpa.is_none()));
    assert!(
        output
            .observations
            .iter()
            .all(|o| o.gust_direction_deg.is_none())
    );
    // Corvara's thermometer is in the answer twice, with the same time and
    // the same value. It is one observation.
    let corvara: Vec<_> = output
        .observations
        .iter()
        .filter(|o| o.station.local() == "61300MS")
        .collect();
    assert_eq!(corvara.len(), 1);
    assert_eq!(
        corvara[0].time,
        Utc.with_ymd_and_hms(2026, 11, 21, 10, 20, 0).unwrap()
    );
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 21, 10, 20, 0).unwrap())
    );

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, fixtures::evening_at());

    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    assert_eq!(output.stations.len(), 3);
    assert_eq!(output.observations.len(), 3);
    assert!(output.report.is_healthy());
    // Fifteen minutes before the moment the sample is for.
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 18, 18, 50, 0).unwrap())
    );

    assert_golden(&golden("evening_run.json"), &output);
}

/// The evening sample is put together, so it is held against what it was
/// made from: the service's own time series for each station and sensor,
/// recorded whole in `tests/fixtures/evening`.
#[test]
fn the_evening_sample_holds_what_the_time_series_say() {
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(rename = "SCODE")]
        station: String,
        #[serde(rename = "TYPE")]
        sensor: String,
        #[serde(rename = "DATE")]
        date: String,
        #[serde(rename = "VALUE")]
        value: f64,
    }
    #[derive(serde::Deserialize)]
    struct Measured {
        #[serde(rename = "DATE")]
        date: String,
        #[serde(rename = "VALUE")]
        value: f64,
    }

    let rows: Vec<Row> = serde_json::from_slice(&fixture("sensors_evening.json")).unwrap();
    assert_eq!(rows.len(), 15);
    for row in rows {
        assert_eq!(row.date, "2026-11-18T19:50:00CET");
        let name = format!("evening/{}_{}.json", row.station, row.sensor);
        let series: Vec<Measured> = serde_json::from_slice(&fixture(&name)).unwrap();
        let measured: Vec<f64> = series
            .iter()
            .filter(|measured| measured.date == row.date)
            .map(|measured| measured.value)
            .collect();
        assert_eq!(measured, [row.value], "{name}");
    }
}

#[test]
fn damaged_values_are_reported_and_the_rest_is_kept() {
    // The evening sample with seven things changed by hand. Five are
    // faults: Sterzing's wind speed is in km/h, its wind direction has a
    // time zone the service does not write, the Jaufenkamm's humidity is
    // written as text and its temperature is 999 °C, and Bozen's humidity
    // names no station. Two are as the service has them: Bozen's gust has a
    // second row of 30 July, as sunshine sensors had on 2026-11-21, and its
    // thermometer has no value.
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.json"))
        .with(sensors_url(), fixture("sensors_damaged.json"));

    let output = run(&fetch, &MemoryArchive::default(), fixtures::evening_at());

    let report = &output.report;
    assert!(report.failures.is_empty());
    let reasons: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(report.skipped.len(), 4, "{reasons:?}");
    assert!(
        reasons.iter().any(|r| r.contains("\"km/h\"")),
        "{reasons:?}"
    );
    assert!(reasons.contains(&"time stamp cannot be read"));
    assert!(reasons.iter().any(|r| r.starts_with("row cannot be read")));
    assert!(reasons.contains(&"a value without a station"));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "temperature_c");

    // Every station still has what was good.
    assert_eq!(output.stations.len(), 3);
    assert_eq!(output.observations.len(), 3);
    let of = |code: &str| {
        output
            .observations
            .iter()
            .find(|o| o.station.local() == code)
            .unwrap()
    };
    assert_eq!(of("37100MS").wind_speed_ms, None);
    assert_eq!(of("37100MS").wind_gust_ms, Some(3.0));
    assert_eq!(of("35100WS").wind_speed_ms, Some(4.5));
    assert_eq!(of("35100WS").temperature_c, None);
    // The current gust, not the one of July.
    assert_eq!(of("83200MS").wind_gust_ms, Some(1.7));

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_answer_that_is_not_json_fails_only_that_request() {
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.json"))
        .with(sensors_url(), b"<html>502 Bad Gateway</html>".to_vec());

    let output = run(&fetch, &MemoryArchive::default(), fixtures::evening_at());

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "sensors");
    // The stations of the list are still delivered.
    assert_eq!(output.stations.len(), 3);
    assert!(!output.report.is_healthy());
}

#[test]
fn the_manifest_is_valid_and_the_addresses_are_encrypted() {
    let source = SuedtirolMeteo::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "suedtirol-meteo");
    assert_eq!(manifest.countries, ["IT"]);
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    // The province publishes no limit on requests.
    assert_eq!(manifest.requests_per_run(), None);
    // The fetcher asks nothing else.
    assert!(stations_url().starts_with("https://"));
    assert!(sensors_url().starts_with("https://"));
}
