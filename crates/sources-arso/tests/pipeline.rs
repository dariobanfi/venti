//! Runs the real pipeline over the file recorded from ARSO on 2026-11-22,
//! cut down to fifteen stations, and over the sample of the evening of
//! 2026-11-18, and compares the results with golden files.

use std::path::{Path, PathBuf};

use chrono::{NaiveDateTime, TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::testing::{
    FixtureFetch, MemoryArchive, assert_golden, block_on, check_adapter_does_no_io,
    check_station_adapter,
};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, RunOptions, RunOutput, run_observations,
};
use venti_sources_arso::{Arso, fixtures, latest_url};

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

/// The moment the file was recorded.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 8, 20, 7).unwrap()
}

/// The Alps' southern box: the Italian Alps east of 9.4° E, and Slovenia
/// south of 46.3° N.
fn alps_south() -> BoundingBox {
    BoundingBox::new(43.5, 9.4, 46.3, 17.2).unwrap()
}

/// The first region's area, which takes in Slovenia north of 46.3° N.
fn alps_de_at() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn run(fetch: &FixtureFetch, area: BoundingBox, now: Timestamp) -> RunOutput {
    let ctx = PlanCtx { now, area };
    block_on(run_observations(
        &Arso::new(),
        fetch,
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
    ))
}

fn with_latest(name: &str) -> FixtureFetch {
    FixtureFetch::new().with(latest_url(), fixture(name))
}

#[test]
fn the_adapter_meets_the_contract() {
    check_adapter_does_no_io(Path::new(env!("CARGO_MANIFEST_DIR")));
    check_station_adapter(
        &Arso::new(),
        &[(latest_url(), fixture("observationAms_si_latest.xml"))],
        recorded_at(),
        alps_south(),
    );
}

#[test]
fn the_plan_is_one_request_for_any_area() {
    let source = Arso::new();
    for area in [
        alps_south(),
        alps_de_at(),
        BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap(),
    ] {
        let plan = source.plan(&PlanCtx {
            now: recorded_at(),
            area,
        });
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tag, "latest");
        assert_eq!(
            plan[0].url,
            "https://meteo.arso.gov.si/uploads/probase/www/observ/surface/text/sl/observationAms_si_latest.xml"
        );
    }
    // What it costs: one request every ten minutes, six an hour, for an
    // answer of 811 kB on 2026-11-22. ARSO publishes no limit.
    let manifest = source.manifest();
    assert_eq!(manifest.cadence_seconds, 600);
    assert_eq!(manifest.requests_per_run(), None);
    assert!(manifest.max_response_bytes() > 811_155);
}

#[test]
fn the_recorded_file_matches_the_golden_file() {
    let archive = MemoryArchive::default();
    let ctx = PlanCtx {
        now: recorded_at(),
        area: alps_south(),
    };
    let output = block_on(run_observations(
        &Arso::new(),
        &with_latest("observationAms_si_latest.xml"),
        &archive,
        &ctx,
        &RunOptions::default(),
    ));

    assert_eq!(output.report.requests, 1);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 1);
    assert!(
        output.report.skipped.is_empty(),
        "{:?}",
        output.report.skipped
    );

    // Of the fifteen stations, six stand north of 46.3° N and are left out
    // without a word; nine are in the southern box.
    assert_eq!(output.stations.len(), 9);
    for north in [
        "KANIN_",
        "KREDA-ICA_",
        "ROGLA_",
        "URSLJ-GOR_",
        "RATECE_",
        "GORICKO_KRAJI-PAR_",
    ] {
        assert!(!output.stations.iter().any(|s| s.id.local() == north));
    }
    // Every one of them measured something, so each has an observation.
    assert_eq!(output.observations.len(), 9);

    let station = |code: &str| {
        output
            .stations
            .iter()
            .find(|s| s.id.local() == code)
            .unwrap()
    };
    let of = |code: &str| {
        output
            .observations
            .iter()
            .find(|o| o.station.local() == code)
            .unwrap()
    };

    // Krvavec, a take-off above Kranj at 1740 m: the means of the ten
    // minutes to 07:50 UTC, the file's own time in UTC, not its 8:50 CET.
    let krvavec = station("KRVAVEC_");
    assert_eq!(krvavec.name, "Krvavec");
    assert_eq!(krvavec.elevation_m, Some(1740.0));
    assert!((krvavec.position.lat() - 46.2973).abs() < 1e-9);
    assert!((krvavec.position.lon() - 14.5333).abs() < 1e-9);
    let k = of("KRVAVEC_");
    assert_eq!(
        k.time,
        Utc.with_ymd_and_hms(2026, 11, 22, 7, 50, 0).unwrap()
    );
    // m/s in the file, m/s in the record: nothing is converted.
    assert_eq!(k.wind_speed_ms, Some(9.58));
    assert_eq!(k.wind_gust_ms, Some(13.21));
    assert_eq!(k.wind_direction_deg, Some(260.0));
    assert_eq!(k.gust_direction_deg, Some(251.0));
    assert_eq!(k.temperature_c, Some(-0.4));
    assert_eq!(k.humidity_pct, Some(100.0));
    // Krvavec measures no pressure; Bilje does, at the station.
    assert_eq!(k.pressure_hpa, None);
    assert_eq!(of("NOVA-GOR_BILJE_").pressure_hpa, Some(1006.4));
    assert_eq!(station("NOVA-GOR_BILJE_").name, "Bilje Nova Gorica");

    // Celje is a main station: its values are of the half hour to 07:30.
    let celje = of("CELJE_MEDLOG_");
    assert_eq!(
        celje.time,
        Utc.with_ymd_and_hms(2026, 11, 22, 7, 30, 0).unwrap()
    );
    // The mean of the half hour, not the newest ten minutes (ff_val, 0.4).
    assert_eq!(celje.wind_direction_deg, Some(293.0));
    assert_eq!(celje.pressure_hpa, Some(984.9));

    // Babno Polje has no anemometer: a reading without wind.
    let babno = of("BABNO-POL_");
    assert_eq!(babno.wind_speed_ms, None);
    assert_eq!(babno.wind_gust_ms, None);
    assert_eq!(babno.wind_direction_deg, None);
    assert_eq!(babno.temperature_c, Some(-0.5));
    assert_eq!(babno.humidity_pct, Some(99.0));

    // The buoy in the Gulf of Trieste sends a gust and a direction, both
    // 0, and no mean: taken as written.
    let buoy = of("TRZAS-ZAL_");
    assert_eq!(buoy.wind_speed_ms, None);
    assert_eq!(buoy.wind_gust_ms, Some(0.0));
    assert_eq!(station("TRZAS-ZAL_").name, "Tržaški zaliv (Zarja)");

    // The port of Koper stands two metres below the sea's level by ARSO.
    assert_eq!(station("KOPER_LUKA_").elevation_m, Some(-2.0));

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_north_of_slovenia_is_in_the_first_regions_box() {
    let output = run(
        &with_latest("observationAms_si_latest.xml"),
        alps_de_at(),
        recorded_at(),
    );
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert_eq!(output.stations.len(), 6);
    // Rateče and Goričko had no wind that morning: their wind's state was
    // 2 and its values empty. Their temperature is taken.
    assert_eq!(output.observations.len(), 6);
    for silent in ["RATECE_", "GORICKO_KRAJI-PAR_"] {
        let o = output
            .observations
            .iter()
            .find(|o| o.station.local() == silent)
            .unwrap();
        assert_eq!(o.wind_speed_ms, None);
        assert!(o.temperature_c.is_some());
    }
    let kredarica = output
        .observations
        .iter()
        .find(|o| o.station.local() == "KREDA-ICA_")
        .unwrap();
    assert_eq!(kredarica.wind_speed_ms, Some(5.39));
    assert_eq!(kredarica.wind_gust_ms, Some(13.04));
    assert_eq!(kredarica.pressure_hpa, Some(741.6));
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
        Some(Utc.with_ymd_and_hms(2026, 11, 18, 18, 50, 0).unwrap())
    );

    assert_golden(&golden("evening_run.json"), &output);
}

/// The minutes that ARSO's archive numbers its values by, for a time on
/// the clock in Slovenia (it writes them in local time: the values it gives
/// for 13:00 on 2026-11-20 are those that the two-day history gives for
/// 12:00 UTC).
fn archive_key(local: &str) -> String {
    let time = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M").unwrap();
    let minutes = time.and_utc().timestamp() / 60 + 89_411_040;
    format!("_{minutes}:")
}

/// The values of one row of an archive answer, by its parameter's name.
fn archive_row(answer: &str, local: &str) -> Vec<(String, String)> {
    let key = archive_key(local);
    let start = answer.find(&key).unwrap_or_else(|| panic!("{key}")) + key.len();
    let row = &answer[start..];
    let row = &row[row.find('{').unwrap() + 1..row.find('}').unwrap()];
    row.split(", ")
        .map(|pair| {
            let (name, value) = pair.split_once(':').unwrap();
            (
                name.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            )
        })
        .collect()
}

fn element<'a>(block: &'a str, name: &str) -> &'a str {
    let open = format!("<{name}>");
    match block.find(&open) {
        Some(i) => {
            let rest = &block[i + open.len()..];
            &rest[..rest.find('<').unwrap()]
        }
        None => {
            assert!(block.contains(&format!("<{name}/>")), "{name}");
            ""
        }
    }
}

/// The evening sample is put together, so it is held against what it was
/// made from: ARSO's archive of each station for that day, recorded whole
/// in `tests/fixtures/evening`.
#[test]
fn the_evening_sample_holds_what_the_archive_says() {
    let sample = String::from_utf8(fixture("observationAms_evening.xml")).unwrap();
    let blocks: Vec<&str> = sample.split("<metData>").skip(1).collect();
    assert_eq!(blocks.len(), 3);
    for (block, archive_id, has_pressure) in [
        (blocks[0], 1820, true),
        (blocks[1], 1848, false),
        (blocks[2], 2845, false),
    ] {
        let answer = String::from_utf8(fixture(&format!("evening/{archive_id}.xml"))).unwrap();
        // The parameters asked, in the order asked.
        for (p, name) in [
            ("p0", "\"12\", name:\"p\""),
            ("p1", "\"2\", name:\"t2m_termin\""),
            ("p2", "\"4\", name:\"rh_termin\""),
            ("p3", "\"21\", name:\"veter_hitrost\""),
            ("p4", "\"23\", name:\"veter_vek_smer\""),
            ("p5", "\"24\", name:\"veter_max_hitrost\""),
        ] {
            assert!(answer.contains(&format!("{p}:{{ pid:{name}")), "{p}");
        }
        // 18:50 UTC is 19:50 on the clock in Slovenia in November.
        let row = archive_row(&answer, "2026-11-18 19:50");
        let value = |p: &str| row.iter().find(|(n, _)| n == p).unwrap().1.as_str();
        assert_eq!(element(block, "valid_UTC"), "18.11.2026 18:50 UTC");
        assert_eq!(element(block, "valid"), "18.11.2026 19:50 CET");
        assert_eq!(element(block, "t"), value("p1"), "{archive_id}");
        assert_eq!(element(block, "rh"), value("p2"), "{archive_id}");
        assert_eq!(element(block, "ffavg_val"), value("p3"), "{archive_id}");
        assert_eq!(element(block, "ddavg_val"), value("p4"), "{archive_id}");
        assert_eq!(element(block, "ffmax_val"), value("p5"), "{archive_id}");
        if has_pressure {
            assert_eq!(element(block, "p"), value("p0"), "{archive_id}");
        } else {
            assert_eq!(element(block, "p"), "");
        }
        // Nothing else measured is in the sample.
        for other in ["td", "tavg", "rhavg", "ddmax_val", "msl", "pavg", "rr_val"] {
            assert_eq!(element(block, other), "", "{archive_id} {other}");
        }
        assert!(!block.contains("<webcam"));
    }
}

#[test]
fn damaged_stations_are_reported_and_the_rest_is_kept() {
    // The evening sample with seven things changed by hand: Kredarica's
    // mean speed is written with a decimal comma; Rogla's time in UTC is
    // written in local time; Uršlja gora has no latitude; and four
    // stations are added: one whose mean speed is -3 m/s, one without a
    // height, one whose mean speed is said to be in km/h, and one without
    // an id.
    let output = run(
        &with_latest("observationAms_damaged.xml"),
        alps_de_at(),
        fixtures::evening_at(),
    );

    let report = &output.report;
    assert!(report.failures.is_empty());
    let skipped: Vec<(&str, &str)> = report
        .skipped
        .iter()
        .map(|s| (s.what.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 5, "{skipped:?}");
    assert!(skipped.contains(&("KREDA-ICA_ ffavg_val", "value cannot be read: 2,9")));
    assert!(skipped.contains(&("ROGLA_ 18.11.2026 19:50 CET", "time stamp cannot be read")));
    assert!(skipped.contains(&("URSLJ-GOR_", "a station without a position")));
    assert!(skipped.contains(&(
        "TEST-KMH_ ffavg_val",
        "the unit is km/h, not m/s: the reading is not taken"
    )));
    assert!(skipped.contains(&("NO ID", "a station without an id")));

    // The negative speed is removed by validation; the gust stays.
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "wind_speed_ms");

    // Kredarica, Rogla and the station in km/h keep their station without
    // a reading; the station without a height keeps its reading.
    assert_eq!(output.stations.len(), 5);
    assert_eq!(output.observations.len(), 2);
    let no_height = output
        .stations
        .iter()
        .find(|s| s.id.local() == "TEST-NOH_")
        .unwrap();
    assert_eq!(no_height.elevation_m, None);
    assert!(
        output
            .observations
            .iter()
            .any(|o| o.station.local() == "TEST-NOH_" && o.wind_speed_ms == Some(2.9))
    );
    let negative = output
        .observations
        .iter()
        .find(|o| o.station.local() == "TEST-NEG_")
        .unwrap();
    assert_eq!(negative.wind_speed_ms, None);
    assert_eq!(negative.wind_gust_ms, Some(8.9));

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_empty_file_is_no_failure_and_no_data() {
    let output = run(
        &with_latest("observationAms_empty.xml"),
        alps_south(),
        recorded_at(),
    );

    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.stations.is_empty());
    assert!(output.observations.is_empty());
    // A run without a reading is not healthy: the health record says so.
    assert!(!output.report.is_healthy());
}

#[test]
fn an_answer_that_is_not_the_expected_xml_fails_the_request() {
    let recorded = fixture("observationAms_si_latest.xml");
    for body in [
        b"<html><body>502 Bad Gateway</body></html>".to_vec(),
        b"<!DOCTYPE HTML PUBLIC \"-//IETF//DTD HTML 2.0//EN\"><html></html>".to_vec(),
        b"".to_vec(),
        // A file cut off on the way.
        recorded[..recorded.len() / 2].to_vec(),
        // Not UTF-8.
        b"<data>\xff</data>".to_vec(),
    ] {
        let fetch = FixtureFetch::new().with(latest_url(), body);
        let output = run(&fetch, alps_south(), recorded_at());
        assert_eq!(output.report.failures.len(), 1);
        assert_eq!(output.report.failures[0].tag, "latest");
        assert!(output.stations.is_empty());
        assert!(!output.report.is_healthy());
    }
}

#[test]
fn the_manifest_is_valid_and_names_the_source() {
    let source = Arso::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "arso-ams");
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert!(manifest.licence.attribution.starts_with("Vir: ARSO"));
    assert!(
        manifest
            .licence
            .constraints
            .iter()
            .any(|c| c.contains("must name the source"))
    );
    assert!(latest_url().starts_with("https://"));
}
