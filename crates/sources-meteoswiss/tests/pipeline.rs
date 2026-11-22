//! Runs the real pipeline over the files recorded from MeteoSwiss's open
//! data on 2026-11-22, and over the sample of the evening of 2026-11-18,
//! and compares the results with golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_meteoswiss::{MeteoSwissSmn, fixtures, stations_url, values_url};

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

/// Shortly after the whole files were recorded, at 08:09:13 UTC.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 8, 10, 0).unwrap()
}

/// The whole Alps: the box the three region packs tile.
fn alps() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap()
}

/// The western Alps, the pack `alps-west`.
fn west() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 9.4).unwrap()
}

/// The first region, the pack `alps-de-at`.
fn first() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn ctx(now: Timestamp, area: BoundingBox) -> PlanCtx {
    PlanCtx { now, area }
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive, ctx: &PlanCtx) -> RunOutput {
    block_on(run_observations(
        &MeteoSwissSmn::new(),
        fetch,
        archive,
        ctx,
        &RunOptions::default(),
    ))
}

fn recorded() -> FixtureFetch {
    FixtureFetch::new()
        .with(stations_url(), fixture("ogd-smn_meta_stations.csv"))
        .with(values_url(), fixture("VQHA80.csv"))
}

fn with_wind(output: &RunOutput) -> usize {
    output
        .observations
        .iter()
        .filter(|o| o.wind_speed_ms.is_some())
        .count()
}

#[test]
fn the_whole_recorded_files_match_the_golden_file() {
    let archive = MemoryArchive::default();

    let output = run(&recorded(), &archive, &ctx(recorded_at(), alps()));

    // Two requests, however many stations there are.
    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 2);

    // Every station of the list lies in the box, and 150 of them measured
    // a wind. Of the nine without one, four measure it on a tower, which is
    // not taken, and five had none at 07:50.
    assert_eq!(output.stations.len(), 158);
    assert_eq!(with_wind(&output), 150);
    assert!(output.report.is_healthy());
    // The file has a row of dashes for MRP, a code the list does not hold.
    // It says nothing and is no observation.
    assert!(
        output.report.skipped.is_empty(),
        "{:?}",
        output.report.skipped
    );
    assert!(
        output.report.rejected.is_empty(),
        "{:?}",
        output.report.rejected
    );
    assert!(
        output
            .observations
            .iter()
            .all(|o| o.station.local() != "MRP")
    );

    // Every value is of one moment.
    assert!(
        output
            .observations
            .iter()
            .all(|o| o.time == Utc.with_ymd_and_hms(2026, 11, 22, 7, 50, 0).unwrap())
    );
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 22, 7, 50, 0).unwrap())
    );

    // MeteoSwiss gives no direction of the gust.
    assert!(
        output
            .observations
            .iter()
            .all(|o| o.gust_direction_deg.is_none())
    );

    // A reading without wind: Grächen had its temperature and humidity and
    // no anemometer's value.
    let grc = output
        .observations
        .iter()
        .find(|o| o.station.local() == "GRC")
        .unwrap();
    assert_eq!(grc.wind_speed_ms, None);
    assert_eq!(grc.wind_gust_ms, None);
    assert_eq!(grc.wind_direction_deg, None);
    assert!(grc.temperature_c.is_some());

    // The units: Aigle's 31.3 km/h with gusts of 53.6 km/h from 144°, and
    // its pressure at the station, 962.8 hPa, not the 1008.4 hPa reduced to
    // sea level.
    let aig = output
        .observations
        .iter()
        .find(|o| o.station.local() == "AIG")
        .unwrap();
    assert_eq!(aig.wind_speed_ms, Some(8.69));
    assert_eq!(aig.wind_gust_ms, Some(14.89));
    assert_eq!(aig.wind_direction_deg, Some(144.0));
    assert_eq!(aig.temperature_c, Some(7.1));
    assert_eq!(aig.humidity_pct, Some(77.7));
    assert_eq!(aig.pressure_hpa, Some(962.8));

    // The names are read from ISO 8859-1, the highest station is the
    // Jungfraujoch, and Bern's position is the list's WGS84.
    let station = |code: &str| {
        output
            .stations
            .iter()
            .find(|s| s.id.local() == code)
            .unwrap()
    };
    assert_eq!(station("AEG").name, "Oberägeri");
    assert_eq!(station("JUN").elevation_m, Some(3571.0));
    assert_eq!(station("BER").position.lat(), 46.990744);
    assert_eq!(station("BER").position.lon(), 7.464061);

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_plan_for_the_western_alps_is_two_requests() {
    let source = MeteoSwissSmn::new();
    let plan = source.plan(&ctx(recorded_at(), west()));
    let urls: Vec<&str> = plan.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://data.geo.admin.ch/ch.meteoschweiz.ogd-smn/ogd-smn_meta_stations.csv",
            "https://data.geo.admin.ch/ch.meteoschweiz.messwerte-aktuell/VQHA80.csv",
        ]
    );

    let output = run(
        &recorded(),
        &MemoryArchive::default(),
        &ctx(recorded_at(), west()),
    );
    // What it costs: two requests, 168 kB, for 134 stations, 126 with a
    // wind.
    assert_eq!(output.report.requests, 2);
    let bytes = fixture("ogd-smn_meta_stations.csv").len() + fixture("VQHA80.csv").len();
    assert_eq!(bytes, 167_989);
    assert_eq!(output.stations.len(), 134);
    assert_eq!(with_wind(&output), 126);
    // The 24 stations east of 9.4° have values in the file too, which the
    // pipeline leaves out and says so.
    assert_eq!(output.report.skipped.len(), 24);
    assert!(
        output
            .report
            .skipped
            .iter()
            .all(|s| s.reason.contains("outside the area"))
    );
}

#[test]
fn the_first_region_has_the_stations_of_graubuenden() {
    let output = run(
        &recorded(),
        &MemoryArchive::default(),
        &ctx(recorded_at(), first()),
    );
    assert_eq!(output.stations.len(), 24);
    assert_eq!(with_wind(&output), 24);
}

#[test]
fn nothing_is_asked_for_an_area_without_a_station() {
    let source = MeteoSwissSmn::new();
    // The southern Alps' pack: Switzerland ends north of 46.3° east of
    // 9.4°, but the box reaches into the coverage, so it is asked.
    let south = BoundingBox::new(43.5, 9.4, 46.3, 17.2).unwrap();
    assert_eq!(source.plan(&ctx(recorded_at(), south)).len(), 2);
    // Vienna and Provence are far from it.
    let vienna = BoundingBox::new(48.0, 16.0, 48.4, 16.6).unwrap();
    let provence = BoundingBox::new(43.5, 5.0, 44.5, 7.0).unwrap();
    assert!(source.plan(&ctx(recorded_at(), vienna)).is_empty());
    assert!(source.plan(&ctx(recorded_at(), provence)).is_empty());
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, &ctx(fixtures::evening_at(), first()));

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
/// made from: each station's own file of ten-minute values, cut to the
/// evening, in `tests/fixtures/evening`.
#[test]
fn the_evening_sample_holds_what_the_stations_files_say() {
    let text = |name: &str| String::from_utf8(fixture(name)).unwrap();
    let sample = text("VQHA80_evening.csv");
    let mut lines = sample.lines();
    let header: Vec<&str> = lines.next().unwrap().split(';').collect();
    let mut rows = 0;
    for line in lines {
        rows += 1;
        let row: Vec<&str> = line.split(';').collect();
        assert_eq!(row[1], "202611181850");
        let code = row[0];
        let series = text(&format!(
            "evening/ogd-smn_{}_t_recent.csv",
            code.to_lowercase()
        ));
        let mut series = series.lines();
        let names: Vec<&str> = series.next().unwrap().split(';').collect();
        let measured: Vec<&str> = series
            .find(|l| l.starts_with(&format!("{code};18.11.2026 18:50;")))
            .unwrap()
            .split(';')
            .collect();
        for (column, value) in header.iter().zip(&row).skip(2) {
            let theirs = names
                .iter()
                .position(|name| name == column)
                .map_or("", |index| measured[index]);
            if theirs.is_empty() {
                assert_eq!(*value, "-", "{code} {column}");
            } else {
                let theirs: f64 = theirs.parse().unwrap();
                assert_eq!(value.parse::<f64>().unwrap(), theirs, "{code} {column}");
            }
        }
    }
    assert_eq!(rows, 3);
}

#[test]
fn damaged_rows_are_reported_and_the_rest_is_kept() {
    // The evening sample with things changed by hand. In the station list:
    // Davos has no height, the Weissfluhjoch's latitude is 146.833°, and a
    // last row has two fields. In the values: Davos's wind is written with
    // a comma and its humidity is 120 %, Samedan's time is in another form,
    // and a last row has two fields.
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_damaged.csv"))
        .with(values_url(), fixture("VQHA80_damaged.csv"));

    let output = run(
        &fetch,
        &MemoryArchive::default(),
        &ctx(fixtures::evening_at(), first()),
    );

    let report = &output.report;
    assert!(report.failures.is_empty());
    let skipped: Vec<(&str, &str)> = report
        .skipped
        .iter()
        .map(|s| (s.what.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 6, "{skipped:?}");
    assert!(
        skipped
            .iter()
            .any(|(what, reason)| *what == "WFJ" && reason.contains("latitude"))
    );
    assert!(skipped.contains(&("XXX;Nowhere", "the row has 2 fields and the header 24")));
    assert!(skipped.contains(&("DAV fu3010z0 6,80", "the value cannot be read as a number")));
    assert!(skipped.contains(&("SAM 18.11.2026 18:50", "time stamp cannot be read")));
    assert!(skipped.contains(&("XYZ;202611181850", "the row has 2 fields and the header 22")));
    // The Weissfluhjoch's values have no station to belong to.
    assert!(
        skipped
            .iter()
            .any(|(what, reason)| what.ends_with(":WFJ")
                && reason.contains("not in the station list"))
    );
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "humidity_pct");

    // A station without a height is a station all the same.
    assert_eq!(output.stations.len(), 2);
    let davos = output
        .stations
        .iter()
        .find(|s| s.id.local() == "DAV")
        .unwrap();
    assert_eq!(davos.elevation_m, None);
    // Davos keeps what was good.
    assert_eq!(output.observations.len(), 1);
    let davos = &output.observations[0];
    assert_eq!(davos.station.local(), "DAV");
    assert_eq!(davos.wind_speed_ms, None);
    assert_eq!(davos.humidity_pct, None);
    assert_eq!(davos.wind_gust_ms, Some(3.31));
    assert_eq!(davos.temperature_c, Some(0.5));

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_empty_answer() {
    // The header and no row: no station has a value, nothing is wrong
    // with the file, and the run has nothing to show.
    let header = fixture("VQHA80.csv")
        .split(|&b| b == b'\n')
        .next()
        .unwrap()
        .to_vec();
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.csv"))
        .with(values_url(), header);
    let output = run(
        &fetch,
        &MemoryArchive::default(),
        &ctx(fixtures::evening_at(), first()),
    );
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert_eq!(output.stations.len(), 3);
    assert!(output.observations.is_empty());
    assert!(!output.report.is_healthy());

    // An answer of no bytes at all is not the file.
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.csv"))
        .with(values_url(), Vec::new());
    let output = run(
        &fetch,
        &MemoryArchive::default(),
        &ctx(fixtures::evening_at(), first()),
    );
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "values");
    assert!(!output.report.is_healthy());
}

#[test]
fn an_answer_that_is_not_the_file_fails_only_that_request() {
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.csv"))
        .with(values_url(), b"<html>502 Bad Gateway</html>".to_vec());

    let output = run(
        &fetch,
        &MemoryArchive::default(),
        &ctx(fixtures::evening_at(), first()),
    );

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "values");
    // The stations of the list are still delivered.
    assert_eq!(output.stations.len(), 3);
    assert!(!output.report.is_healthy());
}

#[test]
fn the_manifest_is_valid_and_keeps_runs_within_the_limit() {
    let source = MeteoSwissSmn::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "meteoswiss-smn");
    assert_eq!(manifest.countries, ["CH", "LI"]);
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert_eq!(manifest.licence.attribution, "Source: MeteoSwiss");
    // 2400 an hour at one run every ten minutes; a run asks two.
    assert_eq!(manifest.requests_per_run(), Some(400));
    // The fetcher asks nothing else.
    assert!(stations_url().starts_with("https://"));
    assert!(values_url().starts_with("https://"));
}
