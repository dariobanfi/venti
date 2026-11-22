//! Runs the real pipeline over an answer recorded from the Pioupiou /
//! OpenWindMap network on 2026-11-22, cut down to sixteen sensors, and over
//! the sample of the evening of 2026-11-18, and compares the results with
//! golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_pioupiou::{Pioupiou, fixtures, live_url};

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

/// Half a minute after the answer was recorded.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 8, 9, 30).unwrap()
}

/// The Alps' western box: the French and western Swiss Alps.
fn alps_west() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 9.4).unwrap()
}

/// The first region's area.
fn alps_de_at() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn run(fetch: &FixtureFetch, area: BoundingBox, now: Timestamp) -> RunOutput {
    let ctx = PlanCtx { now, area };
    block_on(run_observations(
        &Pioupiou::new(),
        fetch,
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
    ))
}

fn with_live(name: &str) -> FixtureFetch {
    FixtureFetch::new().with(live_url(), fixture(name))
}

#[test]
fn the_plan_is_one_request_for_any_area() {
    let source = Pioupiou::new();
    for area in [
        alps_west(),
        alps_de_at(),
        BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap(),
    ] {
        let plan = source.plan(&PlanCtx {
            now: recorded_at(),
            area,
        });
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tag, "live");
        assert_eq!(plan[0].url, "https://api.pioupiou.fr/v1/live/all");
    }
    // What it costs: one request every five minutes, twelve an hour, of
    // the sixty the network allows; the answer was 463 kB on 2026-11-22.
    let manifest = source.manifest();
    assert_eq!(manifest.cadence_seconds, 300);
    assert_eq!(manifest.requests_per_run(), Some(5));
    assert!(manifest.max_response_bytes() > 462_653);
}

#[test]
fn the_recorded_answer_matches_the_golden_file() {
    let archive = MemoryArchive::default();
    let ctx = PlanCtx {
        now: recorded_at(),
        area: alps_west(),
    };
    let output = block_on(run_observations(
        &Pioupiou::new(),
        &with_live("live_all.json"),
        &archive,
        &ctx,
        &RunOptions::default(),
    ));

    assert_eq!(output.report.requests, 1);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 1);
    assert!(output.report.rejected.is_empty());

    // Of the sixteen sensors, three stand outside the western box (one in
    // the Gers, two in Bavaria) and are left out without a word. Three are
    // reported: one whose position the network calls no longer valid, one
    // without a position and one at 0, 0.
    let skipped: Vec<(&str, &str)> = output
        .report
        .skipped
        .iter()
        .map(|s| (s.what.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 3, "{skipped:?}");
    assert!(skipped.contains(&(
        "916",
        "a sensor without a position: the network writes 0, 0 for one it does not know"
    )));
    assert!(skipped.contains(&(
        "1602",
        "the network says the sensor's last known position is no longer valid"
    )));
    assert!(skipped.contains(&("2392", "a sensor without a position")));

    // Ten stations, of which eight have a reading of wind. Two sensors in
    // the box sent none: their station is kept and has no observation.
    assert_eq!(output.stations.len(), 10);
    assert_eq!(output.observations.len(), 8);
    for silent in ["123", "1720"] {
        assert!(output.stations.iter().any(|s| s.id.local() == silent));
        assert!(
            !output
                .observations
                .iter()
                .any(|o| o.station.local() == silent)
        );
    }
    // The network has no height for any sensor, and names are trimmed.
    assert!(output.stations.iter().all(|s| s.elevation_m.is_none()));
    let saleve = output
        .stations
        .iter()
        .find(|s| s.id.local() == "70")
        .unwrap();
    assert_eq!(saleve.name, "Déco Téléphérique Salève 1086m");

    let of = |code: &str| {
        output
            .observations
            .iter()
            .find(|o| o.station.local() == code)
            .unwrap()
    };
    // The Anglettaz take-off above Annecy: 24.5 km/h mean and 57 km/h at
    // most from 180°, at 08:00:55 UTC, the reading's own time.
    let anglettaz = of("1708");
    assert!((anglettaz.wind_speed_ms.unwrap() - 24.5 / 3.6).abs() < 1e-9);
    assert!((anglettaz.wind_gust_ms.unwrap() - 57.0 / 3.6).abs() < 1e-9);
    assert_eq!(anglettaz.wind_direction_deg, Some(180.0));
    assert_eq!(
        anglettaz.time,
        Utc.with_ymd_and_hms(2026, 11, 22, 8, 0, 55).unwrap()
    );
    assert!(anglettaz.temperature_c.is_none() && anglettaz.pressure_hpa.is_none());
    // A sensor without a maximum has a mean and no gust.
    assert_eq!(of("1635").wind_gust_ms, None);
    assert!(of("1635").wind_speed_ms.is_some());
    // A sensor switched off keeps its last reading, of two weeks before,
    // with that reading's own time.
    assert_eq!(
        of("1592").time,
        Utc.with_ymd_and_hms(2026, 11, 8, 14, 45, 16).unwrap()
    );

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());

    let output = run(&fetch, alps_de_at(), fixtures::evening_at());

    assert_eq!(output.report.requests, 1);
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    assert_eq!(output.stations.len(), 3);
    assert_eq!(output.observations.len(), 3);
    assert!(output.report.is_healthy());
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 18, 19, 2, 0).unwrap())
    );

    assert_golden(&golden("evening_run.json"), &output);
}

/// The evening sample is put together, so it is held against what it was
/// made from: the network's own archive of each sensor for that evening,
/// recorded whole in `tests/fixtures/evening`.
#[test]
fn the_evening_sample_holds_what_the_archive_says() {
    let answer: serde_json::Value = serde_json::from_slice(&fixture("live_evening.json")).unwrap();
    let sensors = answer["data"].as_array().unwrap();
    assert_eq!(sensors.len(), 3);
    for sensor in sensors {
        let id = sensor["id"].as_u64().unwrap();
        let archive: serde_json::Value =
            serde_json::from_slice(&fixture(&format!("evening/{id}.json"))).unwrap();
        let legend: Vec<&str> = archive["legend"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            legend,
            [
                "time",
                "latitude",
                "longitude",
                "wind_speed_min",
                "wind_speed_avg",
                "wind_speed_max",
                "wind_heading",
                "pressure"
            ]
        );
        // The last row before the development mode's clock.
        let row = archive["data"]
            .as_array()
            .unwrap()
            .iter()
            .rfind(|row| row[0].as_str().unwrap() <= "2026-11-18T19:05:00.000Z")
            .unwrap();
        let m = &sensor["measurements"];
        let l = &sensor["location"];
        assert_eq!(m["date"], row[0], "{id}");
        assert_eq!(l["latitude"], row[1], "{id}");
        assert_eq!(l["longitude"], row[2], "{id}");
        assert_eq!(m["wind_speed_min"], row[3], "{id}");
        assert_eq!(m["wind_speed_avg"], row[4], "{id}");
        assert_eq!(m["wind_speed_max"], row[5], "{id}");
        assert_eq!(m["wind_heading"], row[6], "{id}");
    }
}

#[test]
fn damaged_sensors_are_reported_and_the_rest_is_kept() {
    // The evening sample with five things changed by hand: the
    // Kampenwand's mean speed is written as text, the Osterfelder's time
    // is not ISO 8601, the Coburgerhütte's id is not a number, and two
    // sensors are added, one whose mean speed is -20 km/h and one without
    // a latitude.
    let output = run(
        &with_live("live_damaged.json"),
        alps_de_at(),
        fixtures::evening_at(),
    );

    let report = &output.report;
    assert!(report.failures.is_empty());
    let reasons: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(report.skipped.len(), 4, "{reasons:?}");
    assert_eq!(
        reasons
            .iter()
            .filter(|r| r.starts_with("sensor cannot be read"))
            .count(),
        2,
        "{reasons:?}"
    );
    assert!(reasons.contains(&"time stamp cannot be read"));
    assert!(reasons.contains(&"a sensor without a position"));
    // The negative speed is removed by validation; its gust stays.
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "wind_speed_ms");

    // The Osterfelder keeps its station without a reading; the added
    // sensor keeps its gust.
    assert_eq!(output.stations.len(), 2);
    assert_eq!(output.observations.len(), 1);
    assert_eq!(output.observations[0].station.local(), "9001");
    assert_eq!(output.observations[0].wind_speed_ms, None);
    assert!((output.observations[0].wind_gust_ms.unwrap() - 40.0 / 3.6).abs() < 1e-9);

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_empty_answer_is_no_failure_and_no_data() {
    let output = run(&with_live("live_empty.json"), alps_west(), recorded_at());

    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.stations.is_empty());
    assert!(output.observations.is_empty());
    // A run without a reading is not healthy: the health record says so.
    assert!(!output.report.is_healthy());
}

#[test]
fn an_answer_that_is_not_the_expected_json_fails_the_request() {
    for body in [
        b"<html>502 Bad Gateway</html>".to_vec(),
        br#"{"error_code":"missing_argument","error_message":"invalid"}"#.to_vec(),
    ] {
        let fetch = FixtureFetch::new().with(live_url(), body);
        let output = run(&fetch, alps_west(), recorded_at());
        assert_eq!(output.report.failures.len(), 1);
        assert_eq!(output.report.failures[0].tag, "live");
        assert!(output.stations.is_empty());
        assert!(!output.report.is_healthy());
    }
}

#[test]
fn the_manifest_is_valid_and_carries_the_condition() {
    let source = Pioupiou::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "pioupiou");
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert_eq!(
        manifest.licence.attribution,
        "Wind data (c) contributors of the OpenWindMap wind network <https://openwindmap.org>"
    );
    assert!(
        manifest.licence.constraints.iter().any(
            |c| c.contains("not allowed to use Pioupiou Wind Data under the Community License")
        )
    );
    assert!(live_url().starts_with("https://"));
}
