//! Runs the real pipeline over the answers recorded from GeoSphere
//! Austria's dataset service on 2026-11-21 and compares the result with a
//! golden file.

use std::path::{Path, PathBuf};

use venti_domain::{RADAR_UNSEEN, RadarGrid};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on, radar_golden};
use venti_sources::{PlanCtx, RadarRunOutput, RadarWanted, RunOptions, run_radar};
use venti_sources_geosphere_radar::{GeosphereIncaRr, fixtures, frame_url, list_url};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive) -> RadarRunOutput {
    let area = fixtures::area();
    let ctx = PlanCtx {
        now: fixtures::recorded_at(),
        area,
    };
    let wanted = RadarWanted {
        after: fixtures::held_until(),
        grid: RadarGrid::over(&area).unwrap(),
    };
    block_on(run_radar(
        &GeosphereIncaRr::new(),
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

    // The list, and the two runs after 12:30.
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
            ("frame/2611211245".to_owned(), 425_558),
            ("frame/2611211300".to_owned(), 426_204),
            ("list".to_owned(), 1588),
        ]
    );

    assert_eq!(output.frames.len(), 2);
    for (frame, (time, _)) in output.frames.iter().zip(fixtures::frames()) {
        assert_eq!(frame.time, time);
        assert_eq!(frame.interval_s, 900);
        assert_eq!((frame.grid.columns, frame.grid.rows), (624, 261));
        // The product has a value at every point of its grid, also over
        // Bavaria, South Tyrol and Slovenia, and the widened box that is
        // asked for covers the map to its edges: all of the first region
        // is seen.
        let unseen = frame.cells.iter().filter(|c| **c == RADAR_UNSEEN).count();
        assert_eq!(unseen, 0, "of {}", frame.cells.len());
    }
    let rain = |frame: usize, step: u8| {
        output.frames[frame]
            .cells
            .iter()
            .filter(|c| **c == step)
            .count()
    };
    assert_eq!(
        [1, 2, 3, 4, 5, 6].map(|step| rain(0, step)),
        [4470, 11_305, 3088, 36, 0, 0]
    );

    let frames: Vec<serde_json::Value> = output.frames.iter().map(radar_golden).collect();
    assert_golden(
        &golden("first_region.json"),
        &serde_json::json!({ "report": output.report, "frames": frames }),
    );
}

#[test]
fn a_run_that_arrived_in_between_is_refused_and_the_other_frame_arrives() {
    // Between the list and the frames a new run arrived, and every place
    // in the list now gives the run before: the request for 13:00 gets
    // the frame of 12:45, the request for 12:45 nothing recorded.
    let [(older, older_body), (newer, _)] = fixtures::frames();
    let area = fixtures::area();
    let fetch = FixtureFetch::new()
        .with(list_url(), fixtures::LIST)
        .with(frame_url(newer, 0, &area), older_body);

    let output = run(&fetch, &MemoryArchive::default());

    assert!(output.frames.is_empty());
    let errors: Vec<(&str, &str)> = output
        .report
        .failures
        .iter()
        .map(|failure| (failure.tag.as_str(), failure.error.as_str()))
        .collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0], ("frame/2611211245", "HTTP status 404"));
    assert_eq!(errors[1].0, "frame/2611211300");
    assert!(
        errors[1]
            .1
            .contains("was asked for as the run of 2026-11-21 13:00"),
        "{errors:?}"
    );
    // The next run asks again: this one is not healthy.
    assert!(!output.report.is_healthy());
    let _ = older;
}

#[test]
fn an_answer_that_is_no_frame_is_reported_and_delivers_nothing() {
    let [(older, body), (newer, _)] = fixtures::frames();
    let area = fixtures::area();
    let fetch = FixtureFetch::new()
        .with(list_url(), fixtures::LIST)
        .with(
            frame_url(older, 1, &area),
            "<html>429 Too Many Requests</html>",
        )
        .with(frame_url(newer, 0, &area), &body[..4096]);

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
fn a_list_that_is_no_list_is_a_failed_request() {
    let fetch = FixtureFetch::new().with(list_url(), "<html>503</html>");

    let output = run(&fetch, &MemoryArchive::default());

    assert_eq!(output.report.requests, 1);
    assert!(output.frames.is_empty());
    assert_eq!(output.report.failures.len(), 1);
    assert!(
        output.report.failures[0].error.contains("no list of runs"),
        "{:?}",
        output.report.failures
    );
    assert_eq!(output.report.newest_listed, None);
    assert!(!output.report.is_healthy());
}
