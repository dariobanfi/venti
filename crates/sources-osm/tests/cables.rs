//! Runs the real pipeline over the cables recorded near the Tegernsee on
//! 2026-11-21 and compares the result with a golden file.

use std::path::Path;

use venti_domain::CableKind;
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
