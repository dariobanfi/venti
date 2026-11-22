//! Runs the real pipeline over answers recorded from the provider and
//! compares the results with golden files.
//!
//! The recordings are files in `tests/fixtures`, as the provider sent them
//! (cut down to a handful of stations where an answer is large), and
//! `tests/fixtures/NOTICE.txt` says where each is from, when it was
//! recorded, under which licence, and what was cut.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::testing::{
    FixtureFetch, MemoryArchive, assert_golden, block_on, check_adapter_does_no_io,
    check_station_adapter,
};
use venti_sources::{PlanCtx, RunOptions, RunOutput, run_observations};
use venti_sources___SNAKE__::{__TYPE__, stations_url};

/// A recorded answer. The first thing to do in a new adapter: record one.
fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "record a fixture: save the provider's answer as {} and say in \
             tests/fixtures/NOTICE.txt where it is from ({e})",
            path.display()
        )
    })
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Shortly after the answers were recorded: the time of a run over them.
/// Set it to the minute of the recording.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
}

/// The area of the run: the region the network is for.
fn area() -> BoundingBox {
    BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap()
}

fn run(fetch: &FixtureFetch) -> RunOutput {
    let ctx = PlanCtx {
        now: recorded_at(),
        area: area(),
    };
    block_on(run_observations(
        &__TYPE__::new(),
        fetch,
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
    ))
}

#[test]
fn the_manifest_is_filled_in() {
    let text = include_str!("../manifest.toml");
    assert!(
        !text.contains("TO FILL IN"),
        "the manifest has lines that are not filled in yet"
    );
    assert!(
        !text.contains("example.org"),
        "the manifest names example.org where the provider's own pages belong"
    );
}

#[test]
fn the_recorded_answer_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with(stations_url(), fixture("stations.json"));
    let output = run(&fetch);

    assert!(output.report.is_healthy(), "{:?}", output.report);
    // The first run writes the file: VENTI_UPDATE_GOLDEN=1. Then read it
    // beside the provider's own page for the same hour, station by
    // station, before it is committed: it is what the adapter is held to
    // from then on.
    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_adapter_meets_the_contract() {
    check_adapter_does_no_io(Path::new(env!("CARGO_MANIFEST_DIR")));
    check_station_adapter(
        &__TYPE__::new(),
        &[(stations_url(), fixture("stations.json"))],
        recorded_at(),
        area(),
    );
}

#[test]
fn a_damaged_answer_costs_only_what_is_damaged() {
    // The recorded answer with entries damaged by hand: a station without
    // a position, a value that is no number, a time that cannot be read.
    // NOTICE.txt lists what was done to which entry.
    let fetch = FixtureFetch::new().with(stations_url(), fixture("stations_damaged.json"));
    let output = run(&fetch);

    assert!(!output.report.skipped.is_empty());
    assert!(!output.stations.is_empty());
    assert_golden(&golden("damaged_run.json"), &output);
}
