//! Runs the real pipeline over payloads recorded from the GeoSphere Austria
//! Data Hub on 2026-11-18 and compares the result with golden files.

use std::path::{Path, PathBuf};

use venti_domain::BoundingBox;
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_geosphere::{Tawes10Min, fixtures};

const METADATA: &str =
    "https://dataset.api.hub.geosphere.at/v1/station/current/tawes-v1-10min/metadata";
/// The request the adapter must build from the recorded metadata: one call
/// for the three active stations, in the order the metadata lists them.
const CURRENT: &str = "https://dataset.api.hub.geosphere.at/v1/station/current/tawes-v1-10min\
    ?parameters=FF,FFX,DD,DDX,TL,RF,P&station_ids=11126,11320,11343&output_format=geojson";

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

/// Shortly after the fixtures were recorded, over the first region's area.
fn ctx() -> PlanCtx {
    PlanCtx {
        now: fixtures::recorded_at(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive) -> RunOutput {
    let options = RunOptions::default();
    block_on(run_observations(
        &Tawes10Min::new(),
        fetch,
        archive,
        &ctx(),
        &options,
    ))
}

#[test]
fn a_recorded_run_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive);

    // Two requests in total, however many stations there are.
    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 2);
    // The inactive station in the metadata is neither listed nor requested.
    assert_eq!(output.stations.len(), 3);
    assert_eq!(output.observations.len(), 3);

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn damaged_values_are_reported_and_the_rest_is_kept() {
    // The recorded response with three kinds of damage: the newest time stamp
    // is unreadable, one value list has the wrong length, and one temperature
    // is 999 °C.
    let fetch = FixtureFetch::new()
        .with(METADATA, fixture("metadata.json"))
        .with(CURRENT, fixture("current_malformed.json"));

    let output = run(&fetch, &MemoryArchive::default());

    let report = &output.report;
    assert!(report.failures.is_empty());
    assert_eq!(report.skipped.len(), 2);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "temperature_c");
    // With the newest time stamp unreadable, no usable value is left.
    assert!(output.observations.is_empty());

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn a_response_that_is_not_json_fails_only_that_request() {
    let fetch = FixtureFetch::new()
        .with(METADATA, fixture("metadata.json"))
        .with(CURRENT, b"<html>502 Bad Gateway</html>".to_vec());

    let output = run(&fetch, &MemoryArchive::default());

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "current");
    // The stations from the metadata are still delivered.
    assert_eq!(output.stations.len(), 3);
}

#[test]
fn the_manifest_is_valid_and_keeps_runs_within_the_hourly_limit() {
    let source = Tawes10Min::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "geosphere-tawes");
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    // 240 requests an hour at one run every ten minutes.
    assert_eq!(manifest.requests_per_run(), Some(40));
}
