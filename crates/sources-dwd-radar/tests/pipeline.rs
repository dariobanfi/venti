//! Runs the real pipeline over the answers recorded from DWD's open-data
//! server on 2026-11-21 and compares the result with a golden file.

use std::path::{Path, PathBuf};

use venti_domain::{BoundingBox, RADAR_UNSEEN, RadarGrid};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on, radar_golden};
use venti_sources::{PlanCtx, RadarRunOutput, RadarWanted, RunOptions, run_radar};
use venti_sources_dwd_radar::{DwdRadarRy, fixtures, frame_url, list_url};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// The first region's area.
fn first_region() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive) -> RadarRunOutput {
    let area = first_region();
    let ctx = PlanCtx {
        now: fixtures::recorded_at(),
        area,
    };
    let wanted = RadarWanted {
        after: fixtures::held_until(),
        grid: RadarGrid::over(&area).unwrap(),
    };
    block_on(run_radar(
        &DwdRadarRy::new(),
        fetch,
        archive,
        &ctx,
        &RunOptions::default(),
        &wanted,
    ))
}

#[test]
fn the_recorded_answers_match_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive);

    // The list, and the two frames after 11:30.
    assert_eq!(output.report.requests, 3);
    assert!(
        output.report.failures.is_empty(),
        "{:?}",
        output.report.failures
    );
    assert!(output.report.skipped.is_empty() && output.report.rejected.is_empty());
    assert!(output.report.is_healthy());
    assert_eq!(output.report.newest_listed, Some(fixtures::frames()[1].0));
    assert_eq!(
        archive.stored(),
        [
            ("frame/2611211135".to_owned(), 56_477),
            ("frame/2611211140".to_owned(), 57_348),
            ("list".to_owned(), 85_577),
        ]
    );

    assert_eq!(output.frames.len(), 2);
    for (frame, (time, _)) in output.frames.iter().zip(fixtures::frames()) {
        assert_eq!(frame.time, time);
        assert_eq!(frame.interval_s, 300);
        assert_eq!((frame.grid.columns, frame.grid.rows), (624, 261));
        // More than half of the first region no German radar sees: all
        // of it east of Passau and south of the main ridge.
        let unseen = frame.cells.iter().filter(|c| **c == RADAR_UNSEEN).count();
        assert_eq!(unseen, 92_979, "of {}", frame.cells.len());
    }
    // It rained lightly that morning: some 6,800 cells of rain, a few of
    // them heavy, none in the two highest steps.
    let rain = |step: u8| {
        output.frames[1]
            .cells
            .iter()
            .filter(|c| **c == step)
            .count()
    };
    assert_eq!(
        [rain(1), rain(2), rain(3), rain(4), rain(5), rain(6)],
        [1688, 4278, 897, 18, 0, 0]
    );

    let frames: Vec<serde_json::Value> = output.frames.iter().map(radar_golden).collect();
    assert_golden(
        &golden("first_region.json"),
        &serde_json::json!({ "report": output.report, "frames": frames }),
    );
}

#[test]
fn a_frame_the_server_does_not_have_is_a_failed_request_and_the_other_arrives() {
    // The list and the older frame; the newer one is "not found".
    let (older, body) = fixtures::frames()[0];
    let fetch = FixtureFetch::new()
        .with(list_url(), fixtures::LIST)
        .with(frame_url(older), body);

    let output = run(&fetch, &MemoryArchive::default());

    assert_eq!(output.frames.len(), 1);
    assert_eq!(output.frames[0].time, older);
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "frame/2611211140");
    assert_eq!(output.report.failures[0].error, "HTTP status 404");
    // A frame that could not be fetched is asked for again by the next
    // run: this one is not healthy.
    assert!(!output.report.is_healthy());
}

#[test]
fn an_answer_that_is_no_frame_is_reported_and_delivers_nothing() {
    let [(older, _), (newer, _)] = fixtures::frames();
    let fetch = FixtureFetch::new()
        .with(list_url(), fixtures::LIST)
        .with(frame_url(older), fixtures::NOT_A_FRAME)
        .with(frame_url(newer), fixtures::CUT_SHORT);

    let output = run(&fetch, &MemoryArchive::default());

    assert!(output.frames.is_empty());
    let errors: Vec<&str> = output
        .report
        .failures
        .iter()
        .map(|failure| failure.error.as_str())
        .collect();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].contains("not an HDF5 file"), "{errors:?}");
    assert!(errors[1].contains("cut short"), "{errors:?}");
    assert!(!output.report.is_healthy());
}

#[test]
fn a_list_that_is_no_list_asks_for_nothing_and_is_not_healthy() {
    let fetch = FixtureFetch::new().with(list_url(), fixtures::NOT_A_FRAME);

    let output = run(&fetch, &MemoryArchive::default());

    assert_eq!(output.report.requests, 1);
    assert!(output.frames.is_empty());
    // Nothing failed, and nothing says the provider is publishing.
    assert!(output.report.failures.is_empty());
    assert_eq!(output.report.newest_listed, None);
    assert!(!output.report.is_healthy());
}
