//! Runs the real pipeline over answers recorded from the SLF's measurement
//! service on 2026-11-22, and over the sample of the evening of 2026-11-18,
//! and compares the results with golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Observation, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_slf::{SlfImis, fixtures, measurements_url, stations_url};

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

/// Half a minute after the answers were recorded.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 8, 9, 30).unwrap()
}

/// The whole Alps: 5.0° to 17.2° E, 43.5° to 48.5° N.
fn alps() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap()
}

/// The western pack's box, `alps-west`.
fn alps_west() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 9.4).unwrap()
}

/// The first region's box, `alps-de-at`, which takes in the Engadin, Davos
/// and Liechtenstein.
fn alps_de_at() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn run(
    fetch: &FixtureFetch,
    archive: &MemoryArchive,
    now: Timestamp,
    area: BoundingBox,
) -> RunOutput {
    let ctx = PlanCtx { now, area };
    block_on(run_observations(
        &SlfImis::new(),
        fetch,
        archive,
        &ctx,
        &RunOptions::default(),
    ))
}

fn recorded() -> FixtureFetch {
    FixtureFetch::new()
        .with(stations_url(), fixture("stations.json"))
        .with(measurements_url(), fixture("measurements.json"))
}

fn of<'a>(output: &'a RunOutput, code: &str, time: Timestamp) -> &'a Observation {
    output
        .observations
        .iter()
        .find(|o| o.station.local() == code && o.time == time)
        .unwrap_or_else(|| panic!("no observation of {code} at {time}"))
}

#[test]
fn the_recorded_answers_match_the_golden_file() {
    let archive = MemoryArchive::default();

    let output = run(&recorded(), &archive, recorded_at(), alps());

    // Two requests, however many stations there are.
    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 2);
    assert!(output.report.is_healthy());

    // Every station of the list is in the Alps' box: 205 in Switzerland
    // and two in Liechtenstein.
    assert_eq!(output.stations.len(), 207);
    // The values are cut to seven stations (see the notice), with their
    // 47 or 48 half hours each. Every row holds something to take.
    assert_eq!(output.observations.len(), 333);
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 22, 8, 0, 0).unwrap())
    );

    // The service's units are the canonical ones, and its values are taken
    // as they are: the Männlichen's row of 08:00 UTC is
    // VW_30MIN_MEAN 3.39, VW_30MIN_MAX 5.223, DW_30MIN_MEAN 214.9,
    // TA_30MIN_MEAN -1.661 and RH_30MIN_MEAN 99.5.
    let at_eight = Utc.with_ymd_and_hms(2026, 11, 22, 8, 0, 0).unwrap();
    let mannlichen = of(&output, "MAN1", at_eight);
    assert_eq!(mannlichen.wind_speed_ms, Some(3.39));
    assert_eq!(mannlichen.wind_gust_ms, Some(5.223));
    assert_eq!(mannlichen.wind_direction_deg, Some(214.9));
    assert_eq!(mannlichen.temperature_c, Some(-1.661));
    assert_eq!(mannlichen.humidity_pct, Some(99.5));
    // The service has neither pressure nor the gust's direction.
    assert!(output.observations.iter().all(|o| o.pressure_hpa.is_none()));
    assert!(
        output
            .observations
            .iter()
            .all(|o| o.gust_direction_deg.is_none())
    );

    let wind = |code: &str| {
        output
            .observations
            .iter()
            .filter(|o| o.station.local() == code && o.wind_speed_ms.is_some())
            .count()
    };
    let temperature = |code: &str| {
        output
            .observations
            .iter()
            .filter(|o| o.station.local() == code && o.temperature_c.is_some())
            .count()
    };
    // The Albristhorn is a wind station on a ridge whose anemometer gave
    // nothing for the whole day, and Tscheppa a snow station without wind:
    // their temperatures are taken.
    assert_eq!((wind("FAE1"), temperature("FAE1")), (0, 47));
    assert_eq!((wind("LAG2"), temperature("LAG2")), (0, 47));
    // Guppen had wind and no temperature.
    assert_eq!((wind("GLA2"), temperature("GLA2")), (48, 0));
    let albristhorn = of(
        &output,
        "FAE1",
        Utc.with_ymd_and_hms(2026, 11, 22, 7, 30, 0).unwrap(),
    );
    assert_eq!(albristhorn.wind_gust_ms, None);
    assert_eq!(albristhorn.wind_direction_deg, None);
    assert_eq!(albristhorn.temperature_c, Some(-3.605));

    // A station of the list without values is a station all the same:
    // Hinterrugg is in the list and has no row.
    assert!(output.stations.iter().any(|s| s.id.local() == "HIN1"));
    assert!(
        !output
            .observations
            .iter()
            .any(|o| o.station.local() == "HIN1")
    );

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn stations_outside_the_area_are_left_out() {
    let output = run(
        &recorded(),
        &MemoryArchive::default(),
        recorded_at(),
        alps_de_at(),
    );

    // 44 of the 207 stations are in the first region's box, among them
    // Davos's, the Engadin's and Liechtenstein's two.
    assert_eq!(output.stations.len(), 44);
    assert!(
        output
            .stations
            .iter()
            .all(|s| alps_de_at().contains(s.position))
    );
    // Of the seven stations whose values are recorded, four are in it. The
    // other three's values are left out by the pipeline, which says so.
    let mut codes: Vec<&str> = output
        .observations
        .iter()
        .map(|o| o.station.local())
        .collect();
    codes.dedup();
    assert_eq!(codes, ["DAV1", "LAG2", "MLB1", "WFJ2"]);
    let mut skipped: Vec<&str> = output
        .report
        .skipped
        .iter()
        .map(|s| s.what.as_str())
        .collect();
    skipped.sort_unstable();
    assert_eq!(skipped, ["slf-imis:FAE1", "slf-imis:GLA2", "slf-imis:MAN1"]);

    // The western pack's box has 162 of the others; no station stands on
    // the meridian 9.4° E the two boxes share. The last one, Ganda Plana
    // in the Val Poschiavo at 46.26° N, is in the southern pack's box.
    let west = run(
        &recorded(),
        &MemoryArchive::default(),
        recorded_at(),
        alps_west(),
    );
    assert_eq!(west.stations.len(), 162);
    let south = BoundingBox::new(43.5, 9.4, 46.3, 17.2).unwrap();
    let south = run(&recorded(), &MemoryArchive::default(), recorded_at(), south);
    assert_eq!(south.stations.len(), 1);
    assert_eq!(south.stations[0].id.local(), "BRU2");
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, fixtures::evening_at(), alps_de_at());

    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    assert_eq!(output.stations.len(), 3);
    // 47 half hours of three stations.
    assert_eq!(output.observations.len(), 141);
    assert!(output.report.is_healthy());
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 18, 18, 30, 0).unwrap())
    );

    assert_golden(&golden("evening_run.json"), &output);
}

/// The evening sample is put together, so it is held against what it was
/// made from: each station's own history of seven days, recorded whole in
/// `tests/fixtures/evening`.
#[test]
fn the_evening_sample_holds_what_the_histories_say() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&fixture("measurements_evening.json")).unwrap();
    assert_eq!(rows.len(), 141);
    for code in ["DAV1", "MLB1", "WFJ2"] {
        let history: Vec<serde_json::Value> =
            serde_json::from_slice(&fixture(&format!("evening/{code}.json"))).unwrap();
        let expected: Vec<&serde_json::Value> = history
            .iter()
            .filter(|row| {
                let date = row["measure_date"].as_str().unwrap();
                ("2026-11-17T19:30:00Z"..="2026-11-18T18:30:00Z").contains(&date)
            })
            .collect();
        let sample: Vec<&serde_json::Value> = rows
            .iter()
            .filter(|row| row["station_code"] == code)
            .collect();
        assert_eq!(sample.len(), 47, "{code}");
        assert_eq!(sample, expected, "{code}");
    }
}

#[test]
fn damaged_values_are_reported_and_the_rest_is_kept() {
    // The evening sample with things changed by hand. In the station list,
    // the Weissfluhjoch's height is null and a fourth station has its
    // longitude written as text. In the values, the Chrachenhorn's row of
    // 18:30 has a date of another form and its row of 18:00 the mean wind
    // "fast", the Spitz's row of 18:30 names no station and its row of
    // 18:00 has 999 °C, and the Weissfluhjoch's row of 18:30 has no wind,
    // as a frozen anemometer's has.
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_damaged.json"))
        .with(measurements_url(), fixture("measurements_damaged.json"));

    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        alps_de_at(),
    );

    let report = &output.report;
    assert!(report.failures.is_empty());
    let reasons: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(report.skipped.len(), 4, "{reasons:?}");
    assert!(
        reasons
            .iter()
            .any(|r| r.starts_with("station cannot be read")),
        "{reasons:?}"
    );
    assert!(reasons.contains(&"time stamp cannot be read"));
    assert!(reasons.contains(&"a row without a station"));
    assert!(reasons.contains(&"value is not a number: \"fast\""));
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "temperature_c");
    assert_eq!(report.rejected[0].value, 999.0);

    // A station without a height is a station without a height.
    assert_eq!(output.stations.len(), 3);
    let weissfluhjoch = output
        .stations
        .iter()
        .find(|s| s.id.local() == "WFJ2")
        .unwrap();
    assert_eq!(weissfluhjoch.elevation_m, None);

    // Two rows are lost whole, and of two others one value each.
    assert_eq!(output.observations.len(), 139);
    let at = |hour, minute| Utc.with_ymd_and_hms(2026, 11, 18, hour, minute, 0).unwrap();
    assert!(
        !output
            .observations
            .iter()
            .any(|o| o.station.local() == "DAV1" && o.time == at(18, 30))
    );
    let chrachenhorn = of(&output, "DAV1", at(18, 0));
    assert_eq!(chrachenhorn.wind_speed_ms, None);
    assert_eq!(chrachenhorn.wind_gust_ms, Some(13.85));
    assert_eq!(chrachenhorn.wind_direction_deg, Some(251.8));
    let spitz = of(&output, "MLB1", at(18, 0));
    assert_eq!(spitz.temperature_c, None);
    assert_eq!(spitz.wind_speed_ms, Some(9.81));
    // A reading without wind keeps its temperature and humidity.
    let without_wind = of(&output, "WFJ2", at(18, 30));
    assert_eq!(without_wind.wind_speed_ms, None);
    assert_eq!(without_wind.wind_gust_ms, None);
    assert_eq!(without_wind.wind_direction_deg, None);
    assert_eq!(without_wind.temperature_c, Some(-3.139));
    assert_eq!(without_wind.humidity_pct, Some(86.0));

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_empty_answer_delivers_nothing_and_is_not_healthy() {
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.json"))
        .with(measurements_url(), b"[]".to_vec());

    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        alps_de_at(),
    );

    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert_eq!(output.stations.len(), 3);
    assert!(output.observations.is_empty());
    assert!(!output.report.is_healthy());

    // An empty list of stations leaves every value without its station.
    let fetch = FixtureFetch::new()
        .with(stations_url(), b"[]".to_vec())
        .with(measurements_url(), fixture("measurements_evening.json"));
    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        alps_de_at(),
    );
    assert!(output.stations.is_empty());
    assert!(output.observations.is_empty());
    assert_eq!(output.report.skipped.len(), 3);
    assert!(!output.report.is_healthy());
}

#[test]
fn an_answer_that_is_not_json_fails_only_that_request() {
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("stations_evening.json"))
        .with(
            measurements_url(),
            b"{\"detail\":\"Internal Server Error\"}".to_vec(),
        );

    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        alps_de_at(),
    );

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "measurements");
    // The stations of the list are still delivered.
    assert_eq!(output.stations.len(), 3);
    assert!(!output.report.is_healthy());
}

#[test]
fn the_plan_for_the_western_box_is_two_requests() {
    let source = SlfImis::new();
    let ctx = PlanCtx {
        now: recorded_at(),
        area: alps_west(),
    };
    let plan = source.plan(&ctx);
    let urls: Vec<&str> = plan.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://measurement-api.slf.ch/public/api/imis/stations",
            "https://measurement-api.slf.ch/public/api/imis/measurements",
        ]
    );
    // The same two for any box: the service takes no area.
    let whole = source.plan(&PlanCtx {
        now: recorded_at(),
        area: alps(),
    });
    assert_eq!(whole, plan);
    // What it costs: two requests a run, a run every half hour, and the
    // larger answer within what the manifest allows. Recorded whole on
    // 2026-11-22 it was 3,302,282 bytes.
    let manifest = source.manifest();
    assert_eq!(manifest.cadence_seconds, 1800);
    assert_eq!(manifest.requests_per_run(), None);
    assert!(3_302_282 < manifest.max_response_bytes());
}

/// The unit of every value taken is the one the SLF's own description of
/// the service gives, recorded beside the answers.
#[test]
fn the_units_are_those_the_slf_describes() {
    let description: serde_json::Value = serde_json::from_slice(&fixture("openapi.json")).unwrap();
    let fields = &description["components"]["schemas"]["ImisMeasurement"]["properties"];
    let says = |name: &str| fields[name]["description"].as_str().unwrap().to_owned();
    assert_eq!(
        says("VW_30MIN_MEAN"),
        "wind speed, vectorial mean over last 30 minutes, m/s"
    );
    assert_eq!(
        says("VW_30MIN_MAX"),
        "wind speed, max 5sec measurement within last 30 minutes, m/s"
    );
    assert!(says("DW_30MIN_MEAN").ends_with(", °"));
    assert!(says("TA_30MIN_MEAN").ends_with(", °C"));
    assert!(says("RH_30MIN_MEAN").ends_with(", %"));
    assert_eq!(
        fields["measure_date"]["description"],
        "timestamp of measurement in UTC"
    );
    let station = &description["components"]["schemas"]["ImisStation"]["properties"];
    assert_eq!(
        station["elevation"]["description"],
        "elevation of station in m a.s.l"
    );
    assert_eq!(
        station["lat"]["description"],
        "latitude of station coordinate in WGS84"
    );
}

#[test]
fn the_manifest_is_valid_and_the_addresses_are_encrypted() {
    let source = SlfImis::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "slf-imis");
    assert_eq!(manifest.countries, ["CH", "LI"]);
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert!(stations_url().starts_with("https://"));
    assert!(measurements_url().starts_with("https://"));
}
