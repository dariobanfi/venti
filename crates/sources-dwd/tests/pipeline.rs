//! Runs the real pipeline over payloads recorded from DWD's server on
//! 2026-11-18 and compares the result with golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::BoundingBox;
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_dwd::{Dwd10Min, fixtures};

const BASE: &str =
    "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/10_minutes";

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

/// Keeps the golden files short: the recorded files hold the whole day.
fn last_hour() -> RunOptions {
    let since = Utc.with_ymd_and_hms(2026, 11, 18, 17, 30, 0).unwrap();
    RunOptions {
        since: Some(since),
        ..RunOptions::default()
    }
}

fn with_station_lists(fetch: FixtureFetch) -> FixtureFetch {
    [
        ("wind", "zehn_now_ff_Beschreibung_Stationen.txt"),
        ("extreme_wind", "zehn_now_fx_Beschreibung_Stationen.txt"),
        ("air_temperature", "zehn_now_tu_Beschreibung_Stationen.txt"),
    ]
    .into_iter()
    .fold(fetch, |fetch, (dir, file)| {
        fetch.with(format!("{BASE}/{dir}/now/{file}"), fixture(file))
    })
}

fn with_data(fetch: FixtureFetch, dir: &str, file: &str, fixture_name: &str) -> FixtureFetch {
    fetch.with(format!("{BASE}/{dir}/now/{file}"), fixture(fixture_name))
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive, ctx: &PlanCtx) -> RunOutput {
    block_on(run_observations(
        &Dwd10Min::new(),
        fetch,
        archive,
        ctx,
        &last_hour(),
    ))
}

#[test]
fn a_recorded_run_matches_the_golden_file() {
    // Zugspitze and Hohenpeißenberg have all three files. Garmisch-Partenkirchen
    // has no temperature file here, as happens when a station's file is late.
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, &ctx());

    // Three station lists and nine station files were requested; one was missing.
    assert_eq!(output.report.requests, 12);
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "data/air/01550");
    assert_eq!(archive.stored().len(), 11);
    // The missing file costs Garmisch its temperature, not its wind.
    let garmisch = output
        .observations
        .iter()
        .rfind(|o| o.station.local() == "01550")
        .unwrap();
    assert!(garmisch.wind_speed_ms.is_some());
    assert!(garmisch.temperature_c.is_none());

    assert_golden(&golden("recorded_run.json"), &output);
}

/// An archive as `unpacks_to_16_mb.zip` is, except that both of its headers
/// say the table inside is a hundred bytes long.
fn saying_it_is_small(mut archive: Vec<u8>) -> Vec<u8> {
    let central = archive
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .expect("the archive has a directory");
    assert_eq!(&archive[..4], b"PK\x03\x04");
    let hundred = 100u32.to_le_bytes();
    // The size after unpacking: at byte 22 of the file's own header, and
    // at byte 24 of its entry in the directory.
    archive[22..26].copy_from_slice(&hundred);
    archive[central + 24..central + 28].copy_from_slice(&hundred);
    archive
}

#[test]
fn an_archive_is_not_unpacked_beyond_what_a_days_table_can_be() {
    // 49 kB that unpack to 16 MB of well-formed rows: made for this test
    // (the zip format packs a repeated row about 340 to 1). A station's
    // real file is under two kilobytes and unpacks to about ten.
    let bomb = fixture("unpacks_to_16_mb.zip");
    assert!(bomb.len() < 64 * 1024);
    let summit = PlanCtx {
        area: BoundingBox::new(47.40, 10.95, 47.45, 11.00).unwrap(),
        ..ctx()
    };
    let wind = format!("{BASE}/wind/now/10minutenwerte_wind_05792_now.zip");

    // The second says of itself that it is small. The archive reader
    // stops at the size an archive states, so that one fails in the
    // reader's words and not in the adapter's; it fails all the same.
    let cases = [
        (bomb.clone(), "unpacks to more than 1024 kB"),
        (saying_it_is_small(bomb), "malformed"),
    ];
    for (archive, reason) in cases {
        let fetch = with_station_lists(FixtureFetch::new());
        let fetch = with_data(
            fetch,
            "air_temperature",
            "10minutenwerte_TU_05792_now.zip",
            "10minutenwerte_TU_05792_now.zip",
        );
        let fetch = fetch.with(wind.clone(), archive);

        let output = run(&fetch, &MemoryArchive::default(), &summit);

        // The wind file is a failed request, and says why; nothing of its
        // 300,000 rows was read.
        let failed: Vec<_> = output
            .report
            .failures
            .iter()
            .filter(|failure| failure.tag == "data/wind/05792")
            .collect();
        assert_eq!(failed.len(), 1, "{:?}", output.report.failures);
        assert!(failed[0].error.contains(reason), "{}", failed[0].error);
        assert!(
            output
                .observations
                .iter()
                .all(|o| o.wind_speed_ms.is_none())
        );
        // The rest of the run is not affected.
        assert!(
            output
                .observations
                .iter()
                .any(|o| o.temperature_c.is_some())
        );
    }
}

#[test]
fn damaged_payloads_are_reported_and_do_not_stop_the_run() {
    let fetch = with_station_lists(FixtureFetch::new());
    // The wind file has damaged rows, the extremes "file" is an error page,
    // the temperature file is intact.
    let fetch = with_data(
        fetch,
        "wind",
        "10minutenwerte_wind_05792_now.zip",
        "malformed_wind_05792.zip",
    );
    let fetch = with_data(
        fetch,
        "extreme_wind",
        "10minutenwerte_extrema_wind_05792_now.zip",
        "not_a_zip.html",
    );
    let fetch = with_data(
        fetch,
        "air_temperature",
        "10minutenwerte_TU_05792_now.zip",
        "10minutenwerte_TU_05792_now.zip",
    );

    // A box around the Zugspitze alone, so only its files are requested.
    let summit = PlanCtx {
        area: BoundingBox::new(47.40, 10.95, 47.45, 11.00).unwrap(),
        ..ctx()
    };

    let output = run(&fetch, &MemoryArchive::default(), &summit);

    let report = &output.report;
    assert_eq!(report.requests, 6);
    // Three damaged rows: a word for a number, a row cut short, an impossible date.
    assert_eq!(report.skipped.len(), 3);
    // One wind speed of 999 m/s is removed by validation.
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "wind_speed_ms");
    // The error page is a failure of that one request and of nothing else.
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].tag, "data/extremes/05792");
    // Temperature from the intact file still arrives.
    assert!(
        output
            .observations
            .iter()
            .any(|o| o.temperature_c.is_some())
    );

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn only_current_stations_inside_the_area_are_requested() {
    let source = Dwd10Min::new();
    let request = &source.plan(&ctx())[0];
    assert_eq!(request.tag, "stations/wind");
    let body = fixture("zehn_now_ff_Beschreibung_Stationen.txt");

    // The first region: Neuruppin lies outside it.
    let parsed = source.parse(request, &body, &ctx()).unwrap();
    let tags: Vec<&str> = parsed.follow_up.iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(
        tags,
        ["data/wind/01550", "data/wind/02290", "data/wind/05792"]
    );

    // All of Germany: Neuruppin is in, and the one station in the list whose
    // data ended long ago is still left out.
    let germany = PlanCtx {
        area: BoundingBox::new(47.0, 5.5, 55.5, 15.5).unwrap(),
        ..ctx()
    };
    let parsed = source.parse(request, &body, &germany).unwrap();
    let tags: Vec<&str> = parsed.follow_up.iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(
        tags,
        [
            "data/wind/00096",
            "data/wind/01550",
            "data/wind/02290",
            "data/wind/05792"
        ]
    );
    assert_eq!(parsed.stations.len(), 4);
    assert!(parsed.skipped.is_empty());
}

#[test]
fn station_names_keep_their_non_ascii_letters() {
    let source = Dwd10Min::new();
    let request = &source.plan(&ctx())[0];
    let body = fixture("zehn_now_ff_Beschreibung_Stationen.txt");
    let parsed = source.parse(request, &body, &ctx()).unwrap();
    assert!(parsed.stations.iter().any(|s| s.name == "Hohenpeißenberg"));
}

#[test]
fn the_manifest_is_valid_and_openly_licensed() {
    let source = Dwd10Min::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "dwd-10min");
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert_eq!(
        manifest.licence.attribution,
        "Quelle: Deutscher Wetterdienst"
    );
}
