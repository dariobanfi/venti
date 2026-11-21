//! Runs the real pipeline over answers recorded from the Aviation Weather
//! Center on 2026-11-21 and compares the results with golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_awc::{AwcMetar, fixtures, reports_url};

/// The request the adapter makes for the first region.
const REPORTS: &str =
    "https://aviationweather.gov/api/data/metar?bbox=46.3,9.4,48.5,17.2&format=json";

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

/// Half a minute after the whole answer was recorded.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 10, 38, 0).unwrap()
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive, now: Timestamp) -> RunOutput {
    let ctx = PlanCtx {
        now,
        area: fixtures::area(),
    };
    block_on(run_observations(
        &AwcMetar::new(),
        fetch,
        archive,
        &ctx,
        &RunOptions::default(),
    ))
}

#[test]
fn the_whole_recorded_answer_matches_the_golden_file() {
    assert_eq!(reports_url(&fixtures::area()), REPORTS);
    let fetch = FixtureFetch::new().with(REPORTS, fixture("metar.json"));
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, recorded_at());

    // One request, however many airfields there are.
    assert_eq!(output.report.requests, 1);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 1);
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    // Every report is a station and an observation.
    assert_eq!(output.stations.len(), 22);
    assert_eq!(output.observations.len(), 22);

    let of = |airfield: &str| {
        output
            .observations
            .iter()
            .find(|o| o.station.local() == airfield)
            .unwrap()
    };
    // METAR LOAN 211000Z 32010G20KT: ten knots, gusts of twenty, from 320°.
    let neustadt = of("LOAN");
    assert_eq!(
        neustadt.time,
        Utc.with_ymd_and_hms(2026, 11, 21, 10, 0, 0).unwrap()
    );
    assert_eq!(neustadt.wind_speed_ms, Some(5.14));
    assert_eq!(neustadt.wind_gust_ms, Some(10.29));
    assert_eq!(neustadt.wind_direction_deg, Some(320.0));
    // It reported no temperature.
    assert_eq!(neustadt.temperature_c, None);
    // METAR LOWI 211020Z AUTO VRB01KT: a wind without a direction.
    let innsbruck = of("LOWI");
    assert!(innsbruck.wind_speed_ms.is_some());
    assert_eq!(innsbruck.wind_direction_deg, None);
    assert_eq!(innsbruck.temperature_c, Some(7.0));
    // METAR LIPB 210950Z: the time of the observation, where the answer's
    // "reportTime" says 10:00.
    assert_eq!(
        of("LIPB").time,
        Utc.with_ymd_and_hms(2026, 11, 21, 9, 50, 0).unwrap()
    );
    // The pressure of a METAR is reduced to sea level and is not taken;
    // humidity and the gust's direction are not reported.
    for observation in &output.observations {
        assert_eq!(observation.pressure_hpa, None);
        assert_eq!(observation.humidity_pct, None);
        assert_eq!(observation.gust_direction_deg, None);
    }

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());

    let output = run(&fetch, &MemoryArchive::default(), fixtures::evening_at());

    assert_eq!(output.report.requests, 1);
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

/// The evening sample is cut from an answer, so it is held against it:
/// every report of the sample is one of the answer, value for value.
#[test]
fn the_evening_sample_is_cut_from_the_answer_for_that_evening() {
    let reports =
        |name: &str| -> Vec<serde_json::Value> { serde_json::from_slice(&fixture(name)).unwrap() };
    let whole = reports("metar_2026-11-18.json");
    let sample = reports("metar_evening.json");
    assert_eq!(whole.len(), 20);
    assert_eq!(sample.len(), 3);
    for report in sample {
        assert!(whole.contains(&report), "{report}");
    }
}

#[test]
fn damaged_reports_are_reported_and_the_rest_is_kept() {
    // Six reports of the evening of 2026-11-18, five of them changed by
    // hand: Memmingen's wind speed is written as text, Innsbruck's report
    // has no time, Samedan's temperature is 999 °C, Salzburg lies at
    // 147° N, and Graz reports calm. The sixth entry is no report at all.
    let fetch = FixtureFetch::new().with(REPORTS, fixture("metar_damaged.json"));

    let output = run(&fetch, &MemoryArchive::default(), fixtures::evening_at());

    let report = &output.report;
    assert!(report.failures.is_empty());
    let skipped: Vec<(&str, &str)> = report
        .skipped
        .iter()
        .map(|s| (s.what.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 4, "{skipped:?}");
    assert!(skipped[0].0 == "EDJA" && skipped[0].1.starts_with("report cannot be read"));
    assert_eq!(
        skipped[1],
        (
            "LOWI",
            "the time of the observation is missing or cannot be read"
        )
    );
    assert_eq!(skipped[2].0, "LOWS");
    assert!(skipped[3].1.starts_with("report cannot be read"));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "temperature_c");

    // Innsbruck is still a station, without a value. Samedan keeps its
    // wind. Calm is a wind of no speed and no direction.
    let stations: Vec<&str> = output.stations.iter().map(|s| s.id.local()).collect();
    assert_eq!(stations, ["LOWG", "LOWI", "LSZS"]);
    let of = |airfield: &str| {
        output
            .observations
            .iter()
            .find(|o| o.station.local() == airfield)
    };
    assert!(of("LOWI").is_none());
    assert!(of("LSZS").unwrap().wind_speed_ms.is_some());
    assert_eq!(of("LSZS").unwrap().temperature_c, None);
    assert_eq!(of("LOWG").unwrap().wind_speed_ms, Some(0.0));
    assert_eq!(of("LOWG").unwrap().wind_direction_deg, None);

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_answer_that_is_not_json_fails_the_run() {
    let fetch = FixtureFetch::new().with(REPORTS, b"<html>502 Bad Gateway</html>".to_vec());

    let output = run(&fetch, &MemoryArchive::default(), fixtures::evening_at());

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "metar");
    assert!(!output.report.is_healthy());
}

#[test]
fn an_empty_answer_is_no_reports_and_no_failure() {
    // What the service sends with status 204 when an area has no report.
    let fetch = FixtureFetch::new().with(REPORTS, Vec::new());

    let output = run(&fetch, &MemoryArchive::default(), fixtures::evening_at());

    assert!(output.report.failures.is_empty());
    assert!(output.stations.is_empty());
    // A run that delivers nothing is still not a healthy one.
    assert!(!output.report.is_healthy());
}

#[test]
fn the_manifest_is_valid_and_keeps_runs_within_the_limit() {
    let source = AwcMetar::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "awc-metar");
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    // 100 requests a minute at one run every ten minutes. A run makes one.
    assert_eq!(manifest.requests_per_run(), Some(1000));
    assert!(REPORTS.starts_with("https://"));
}
