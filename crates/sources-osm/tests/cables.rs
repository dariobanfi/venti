//! Runs the real pipeline over the cables recorded near the Tegernsee on
//! 2026-11-21, and over two tiles south of Munich of 2026-11-23, and
//! compares the results with golden files.

use std::path::Path;
use std::time::Duration;

use venti_domain::{BoundingBox, CableKind};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{CableRunOutput, PlanCtx, RunOptions, run_cables};
use venti_sources_osm::{OsmCables, fixtures};

fn recorded_run() -> CableRunOutput {
    let source = OsmCables::new(fixtures::INTERPRETER).unwrap();
    let ctx = PlanCtx {
        now: fixtures::cables_recorded_at(),
        area: fixtures::cables_area(),
    };
    let fetch = FixtureFetch::new().with_all(fixtures::cables_recorded());
    block_on(run_cables(
        &source,
        &fetch,
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
    ))
}

#[test]
fn the_recorded_cables_match_the_golden_file() {
    let output = recorded_run();
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/cables_tegernsee.json"),
        &serde_json::to_value(&output).unwrap(),
    );
}

#[test]
fn the_box_near_the_tegernsee_holds_what_was_recorded() {
    let output = recorded_run();
    assert!(output.report.is_healthy(), "{:?}", output.report);
    let count = |kind: CableKind| output.cables.iter().filter(|c| c.kind == kind).count();
    // Twelve elements: four power lines of 110 kV, two minor lines, the
    // Wallbergbahn, one material ropeway; the valley station, two T-bars
    // and a drag lift without a height are left out.
    assert_eq!(count(CableKind::PowerLine), 4);
    assert_eq!(count(CableKind::MinorPowerLine), 2);
    assert_eq!(count(CableKind::Gondola), 1);
    assert_eq!(count(CableKind::MaterialRopeway), 1);
    assert_eq!(output.cables.len(), 8);
    assert_eq!(output.report.skipped.len(), 4);
    // None of them has a height mapped.
    assert!(output.cables.iter().all(|cable| cable.height_m.is_none()));
    let line = output
        .cables
        .iter()
        .find(|cable| cable.local == "way/37016046")
        .unwrap();
    assert_eq!(line.voltage_kv, Some(110.0));
    assert_eq!(line.operator.as_deref(), Some("E.ON Netz GmbH"));
}

fn tiles_run(fetch: &FixtureFetch) -> CableRunOutput {
    let source = OsmCables::new(fixtures::INTERPRETER).unwrap();
    let ctx = PlanCtx {
        now: fixtures::cables_tiles_recorded_at(),
        area: fixtures::cables_tiles_area(),
    };
    block_on(run_cables(
        &source,
        fetch,
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
    ))
}

#[test]
fn a_line_in_the_answers_of_two_tiles_is_kept_once() {
    let fetch = FixtureFetch::new().with_all(fixtures::cables_tiles());
    let output = tiles_run(&fetch);
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/cables_tiles.json"),
        &serde_json::to_value(&output).unwrap(),
    );
    assert!(output.report.is_healthy(), "{:?}", output.report);
    assert_eq!(output.report.requests, 2);
    // Seventeen ways in the southern tile, three of them again in the
    // northern one: seventeen cables, none counted as a duplicate.
    assert_eq!(output.cables.len(), 17);
    assert_eq!(output.report.duplicates, 0);
    for shared in ["way/43400090", "way/1066909551", "way/390598434"] {
        let copies = output.cables.iter().filter(|c| c.local == shared).count();
        assert_eq!(copies, 1, "{shared}");
    }
    // In the order of their keys, as for one request.
    assert!(
        output
            .cables
            .windows(2)
            .all(|two| two[0].local < two[1].local)
    );
    // Spread out: one pause before the second tile.
    assert_eq!(fetch.pauses(), [Duration::from_secs(30)]);
}

#[test]
fn a_tile_that_keeps_failing_makes_the_whole_run_unhealthy() {
    let north = fixtures::cables_tiles()[1].0.clone();
    let fetch = FixtureFetch::new()
        .with_all(fixtures::cables_tiles())
        .refusing(&north, 502, 3);
    let output = tiles_run(&fetch);
    // The southern tile's cables are there, but the run says it is not
    // whole, and names the southern tile alone as the box it is whole for:
    // a store replaces the cables inside that tile and keeps the others
    // (the server's test
    // a_tile_that_fails_leaves_its_stored_cables_and_the_other_tile_replaces_its_own).
    assert!(!output.report.is_healthy());
    assert_eq!(
        output.covers,
        [BoundingBox::new(47.97, 11.42, 48.0, 11.6).unwrap()]
    );
    assert_eq!(output.cables.len(), 17);
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].url, north);
    assert!(
        output.report.failures[0].error.contains("3 times"),
        "{}",
        output.report.failures[0].error
    );
    // Two tiles, and the northern one asked twice more, two minutes apart.
    assert_eq!(output.report.requests, 4);
    let pauses: Vec<u64> = fetch.pauses().iter().map(Duration::as_secs).collect();
    assert_eq!(pauses, [30, 120, 30, 120, 30]);
}

#[test]
fn a_tile_that_answers_at_the_second_try_gives_a_whole_set() {
    let north = fixtures::cables_tiles()[1].0.clone();
    let fetch = FixtureFetch::new()
        .with_all(fixtures::cables_tiles())
        .refusing(&north, 504, 1);
    let output = tiles_run(&fetch);
    assert!(output.report.is_healthy(), "{:?}", output.report);
    assert_eq!(output.report.requests, 3);
    assert_eq!(output.cables.len(), 17);
    assert_eq!(output.covers.len(), 2);
}

#[test]
fn after_a_refusal_the_next_tile_waits_the_longer_pause() {
    // The southern tile is asked first and refused once: the northern one
    // waits two minutes, not half a minute, then the southern one is asked
    // again after the two minutes for that, and half a minute's spacing.
    let south = fixtures::cables_tiles()[0].0.clone();
    let fetch = FixtureFetch::new()
        .with_all(fixtures::cables_tiles())
        .refusing(&south, 429, 1);
    let output = tiles_run(&fetch);
    assert!(output.report.is_healthy(), "{:?}", output.report);
    let pauses: Vec<u64> = fetch.pauses().iter().map(Duration::as_secs).collect();
    assert_eq!(pauses, [120, 120, 30]);
}
