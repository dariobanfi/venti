//! Runs the real pipeline over answers recorded from Meteotrentino's
//! service on 2026-11-22, cut down to a handful of stations and to the
//! rows since 06:00 UTC, and over the sample of the evening of 2026-11-18,
//! and compares the results with golden files.

use std::path::{Path, PathBuf};

use chrono::{Duration, TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};
use venti_sources::testing::{
    FixtureFetch, MemoryArchive, assert_golden, block_on, check_adapter_does_no_io,
    check_station_adapter,
};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, Request, RunOptions, RunOutput, run_observations,
};
use venti_sources_meteotrentino::{
    Meteotrentino, WIND_STATIONS, fixtures, stations_url, values_url,
};

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

/// A minute after the answers were recorded.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 8, 24, 0).unwrap()
}

/// The Alps' southern box: the third region.
fn alps_south() -> BoundingBox {
    BoundingBox::new(43.5, 9.4, 46.3, 17.2).unwrap()
}

/// The first region's box.
fn alps_de_at() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

/// The Alps' western box.
fn alps_west() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 9.4).unwrap()
}

/// The box of the three regions together.
fn alps() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap()
}

fn run(fetch: &FixtureFetch, area: BoundingBox, now: Timestamp) -> RunOutput {
    let ctx = PlanCtx { now, area };
    block_on(run_observations(
        &Meteotrentino::new(),
        fetch,
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
    ))
}

/// The requests a run makes over the whole recorded station list: the list,
/// then what the list leads to.
fn requests_for(area: BoundingBox) -> Vec<Request> {
    let source = Meteotrentino::new();
    let ctx = PlanCtx {
        now: recorded_at(),
        area,
    };
    let plan = source.plan(&ctx);
    assert_eq!(plan, vec![Request::new("stations", stations_url())]);
    let parsed = source
        .parse(&plan[0], &fixture("listaStazioni.xml"), &ctx)
        .unwrap();
    // The whole list has every wind station in service and none it cannot
    // read.
    assert!(parsed.skipped.is_empty(), "{:?}", parsed.skipped);
    assert_eq!(parsed.stations.len(), parsed.follow_up.len());
    for request in &parsed.follow_up {
        let code = request.tag.strip_prefix("values/").unwrap();
        assert_eq!(request.url, values_url(code));
        assert!(WIND_STATIONS.contains(&code));
    }
    plan.into_iter().chain(parsed.follow_up).collect()
}

#[test]
fn the_plan_is_the_list_and_then_the_wind_stations_of_the_area() {
    // The list of 2026-11-22 has 208 stations, 113 in service, of which the
    // 46 wind stations are asked where they stand in the area: 33 of them
    // in the southern box and 13 north of 46.3° N, in the first region's.
    assert_eq!(requests_for(alps_south()).len(), 1 + 33);
    assert_eq!(requests_for(alps_de_at()).len(), 1 + 13);
    assert_eq!(requests_for(alps()).len(), 1 + 46);
    // The western box has no station of the province: the list alone.
    assert_eq!(requests_for(alps_west()).len(), 1);

    // What it costs: a run every quarter of an hour, within the 60
    // requests a run that Venti allows itself. The answers of a wind
    // station were 58 to 90 kB at 08:22 UTC and grow through the day.
    let source = Meteotrentino::new();
    let manifest = source.manifest();
    assert_eq!(manifest.cadence_seconds, 900);
    assert_eq!(manifest.requests_per_run(), Some(60));
    assert!(manifest.requests_per_run().unwrap() >= requests_for(alps()).len());
    assert!(manifest.max_response_bytes() > 3 * 90_424);
    assert!(manifest.max_response_bytes() > fixture("listaStazioni.xml").len());
}

/// The recorded answers with the addresses they answer: the station list
/// cut down, and the stations it holds.
fn recorded_answers() -> Vec<(String, Vec<u8>)> {
    let mut answers = vec![(stations_url(), fixture("listaStazioni_cut.xml"))];
    for code in ["T0129", "T0193", "T0368", "T0406", "T0419", "T0420"] {
        answers.push((values_url(code), fixture(&format!("{code}.xml"))));
    }
    answers
}

fn recorded() -> FixtureFetch {
    FixtureFetch::new().with_all(recorded_answers())
}

#[test]
fn the_adapter_meets_the_contract() {
    check_adapter_does_no_io(Path::new(env!("CARGO_MANIFEST_DIR")));
    check_station_adapter(
        &Meteotrentino::new(),
        &recorded_answers(),
        recorded_at(),
        alps_south(),
    );
}

#[test]
fn the_recorded_answers_match_the_golden_file() {
    let archive = MemoryArchive::default();
    let ctx = PlanCtx {
        now: recorded_at(),
        area: alps_south(),
    };
    let output = block_on(run_observations(
        &Meteotrentino::new(),
        &recorded(),
        &archive,
        &ctx,
        &RunOptions::default(),
    ));

    // The list and the six wind stations of the southern box. Of the nine
    // stations of the cut list, one is out of service since 2005, one has
    // no anemometer, and Ciampac stands north of the box.
    assert_eq!(output.report.requests, 7);
    assert!(output.report.failures.is_empty());
    assert_eq!(archive.stored().len(), 7);
    assert!(output.report.rejected.is_empty());
    assert_eq!(output.stations.len(), 6);
    assert!(
        !output
            .stations
            .iter()
            .any(|s| ["T0154", "T0401", "T0403"].contains(&s.id.local()))
    );

    // Mezzano's anemometer gave nothing after 02:45 UTC, and the cut keeps
    // the rows from 06:00 UTC: its answer has temperature and humidity and
    // no wind, which is said.
    let skipped: Vec<(&str, &str)> = output
        .report
        .skipped
        .iter()
        .map(|s| (s.what.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    assert_eq!(skipped[0].0, "T0420");
    assert!(
        skipped[0]
            .1
            .starts_with("the station's answer holds no wind")
    );
    let mezzano: Vec<_> = output
        .observations
        .iter()
        .filter(|o| o.station.local() == "T0420")
        .collect();
    assert_eq!(mezzano.len(), 9);
    assert!(mezzano.iter().all(|o| o.wind_speed_ms.is_none()
        && o.temperature_c.is_some()
        && o.humidity_pct.is_some()));

    // Paganella: name, height and position as the list has them.
    let paganella = output
        .stations
        .iter()
        .find(|s| s.id.local() == "T0406")
        .unwrap();
    assert_eq!(paganella.name, "Paganella (Malga Terlago)");
    assert_eq!(paganella.elevation_m, Some(1790.0));
    assert!((paganella.position.lat() - 46.143_008).abs() < 1e-9);
    assert!((paganella.position.lon() - 11.021_504).abs() < 1e-9);

    let at = |station: &str, hour, minute| {
        let time = Utc.with_ymd_and_hms(2026, 11, 22, hour, minute, 0).unwrap();
        output
            .observations
            .iter()
            .find(|o| o.station.local() == station && o.time == time)
            .unwrap_or_else(|| panic!("{station} at {time}"))
    };
    // Its newest row, 09:00 by the service's clock, is 08:00 UTC: 0.8 m/s
    // from 323° with gusts of 1.6, -1.5 °C and 94 %, in the units the
    // service writes and Venti keeps.
    let newest = at("T0406", 8, 0);
    assert_eq!(newest.wind_speed_ms, Some(0.8));
    assert_eq!(newest.wind_gust_ms, Some(1.6));
    assert_eq!(newest.wind_direction_deg, Some(323.0));
    assert_eq!(newest.temperature_c, Some(-1.5));
    assert_eq!(newest.humidity_pct, Some(94.0));
    assert!(newest.pressure_hpa.is_none());
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 22, 8, 0, 0).unwrap())
    );
    // A quarter hour without wind: Trento Laste at 07:00 by the service's
    // clock has a temperature and a humidity only.
    let laste = at("T0129", 6, 0);
    assert_eq!(laste.temperature_c, Some(3.5));
    assert_eq!(laste.humidity_pct, Some(87.0));
    assert!(laste.wind_speed_ms.is_none() && laste.wind_direction_deg.is_none());
    // Tonadico has no hygrometer.
    assert!(
        output
            .observations
            .iter()
            .filter(|o| o.station.local() == "T0419")
            .all(|o| o.humidity_pct.is_none() && o.temperature_c.is_some())
    );

    assert_golden(&golden("recorded_run.json"), &output);
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());

    let output = run(&fetch, alps_de_at(), fixtures::evening_at());

    assert_eq!(output.report.requests, 4);
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    assert_eq!(output.stations.len(), 3);
    // Three quarter hours each, two of them with wind, and the newest
    // one with: the development mode shows each station's newest reading.
    assert_eq!(output.observations.len(), 9);
    assert_eq!(
        output
            .observations
            .iter()
            .filter(|o| o.wind_speed_ms.is_some())
            .count(),
        6
    );
    assert!(output.report.is_healthy());
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 18, 18, 30, 0).unwrap())
    );
    for station in &output.stations {
        let newest = output
            .observations
            .iter()
            .filter(|o| o.station == station.id)
            .max_by_key(|o| o.time)
            .unwrap();
        assert!(newest.wind_speed_ms.is_some(), "{}", station.id);
    }

    assert_golden(&golden("evening_run.json"), &output);
}

/// The evening sample is put together, so it is held against what it was
/// made from: the service's answers of a week for each station, recorded
/// in `tests/fixtures/evening` for that hour, which write their times with
/// the zone. Every value of the sample is the one those answers give for
/// the same moment; this also holds the reading of the service's times as
/// one hour ahead of UTC.
#[test]
fn the_evening_sample_holds_what_the_other_answer_says() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());
    let output = run(&fetch, alps_de_at(), fixtures::evening_at());

    let mut compared = 0;
    for code in ["T0076", "T0403", "T0404"] {
        let answer: serde_json::Value =
            serde_json::from_slice(&fixture(&format!("evening/{code}.json"))).unwrap();
        let rows = answer["features"].as_array().unwrap();
        assert_eq!(rows.len(), 13);
        let value = |time: Timestamp, key: &str| -> Option<f64> {
            let local = (time + Duration::hours(1)).format("%Y-%m-%dT%H:%M:%S+01");
            let row = rows
                .iter()
                .find(|row| row["properties"]["datetime"] == local.to_string())
                .unwrap_or_else(|| panic!("{code} {local}"));
            assert_eq!(row["properties"]["code"], code);
            let text = row["properties"][key].as_str().unwrap();
            (!text.is_empty()).then(|| text.parse().unwrap())
        };
        for o in output
            .observations
            .iter()
            .filter(|o| o.station.local() == code)
        {
            assert_eq!(
                o.temperature_c,
                value(o.time, "ta(°C)"),
                "{code} {}",
                o.time
            );
            assert_eq!(
                o.humidity_pct,
                value(o.time, "umid(%)"),
                "{code} {}",
                o.time
            );
            if o.wind_speed_ms.is_some() {
                assert_eq!(o.wind_speed_ms, value(o.time, "vvmed(m/s)"));
                assert_eq!(o.wind_gust_ms, value(o.time, "vvmax(m/s)"));
                assert_eq!(o.wind_direction_deg, value(o.time, "dvmed(gN)"));
                compared += 1;
            }
        }
        // And the station is where the other answer puts it.
        let station = output
            .stations
            .iter()
            .find(|s| s.id.local() == code)
            .unwrap();
        let point = &rows[0]["geometry"]["coordinates"];
        assert_eq!(station.position.lon(), point[0].as_f64().unwrap());
        assert_eq!(station.position.lat(), point[1].as_f64().unwrap());
    }
    assert_eq!(compared, 6);
}

#[test]
fn damaged_answers_are_reported_and_the_rest_is_kept() {
    // The evening sample with these things changed by hand. In the list:
    // Rabbi's height is empty; four stations are added: T0998, put into
    // service after the list of wind stations was made; T0999, as new and
    // without a latitude; Mezzano, a wind station, with an end date; and
    // one whose code has a space. In the answers: Ciampac's is a page of
    // HTML; on the Marmolada a wind speed is "n/d", a time is not ISO 8601
    // and a humidity is in per mille; T0998's has no values at all.
    let fetch = FixtureFetch::new()
        .with(stations_url(), fixture("listaStazioni_damaged.xml"))
        .with(values_url("T0076"), fixture("T0076_evening.xml"))
        .with(values_url("T0403"), fixture("unavailable.html"))
        .with(values_url("T0404"), fixture("T0404_damaged.xml"))
        .with(values_url("T0998"), fixture("empty.xml"));

    let output = run(&fetch, alps_de_at(), fixtures::evening_at());
    let report = &output.report;

    // The list and four stations; the new one is asked too.
    assert_eq!(report.requests, 5);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].tag, "values/T0403");
    assert!(
        report.failures[0]
            .error
            .contains("is <html>, not <datiOggi>")
    );
    assert!(!report.is_healthy());

    let skipped: Vec<(&str, &str)> = report
        .skipped
        .iter()
        .map(|s| (s.what.as_str(), s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 7, "{skipped:?}");
    assert!(skipped.contains(&(
        "T0420",
        "a station of the adapter's list of wind stations that the station list has out of service since 01/11/2026"
    )));
    assert!(skipped.contains(&(
        "station \"T 0997\"",
        "a station without a code that can be asked for"
    )));
    assert!(skipped.contains(&("T0999", "station cannot be read: latitudine is \"\"")));
    assert!(skipped.contains(&(
        "T0404 vento_al_suolo 2026-11-18T19:00:00",
        "value cannot be read: v is \"n/d\""
    )));
    assert!(skipped.contains(&(
        "T0404 temperatura_aria 18/11/2026 19:15",
        "time stamp cannot be read"
    )));
    assert!(skipped.contains(&(
        "T0404 umidita_relativa 2026-11-18T19:45:00",
        "the unit of rh is \"‰\", not \"%\""
    )));
    assert!(skipped.iter().any(|(what, reason)| *what == "T0998"
        && reason.starts_with("the station's answer holds no wind")));

    // Rabbi keeps its wind without a height; the new station is a station
    // without observations; Ciampac's station stays though its answer
    // failed.
    assert_eq!(output.stations.len(), 4);
    let rabbi = output
        .stations
        .iter()
        .find(|s| s.id.local() == "T0076")
        .unwrap();
    assert_eq!(rabbi.elevation_m, None);
    assert_eq!(
        output
            .observations
            .iter()
            .filter(|o| o.station.local() == "T0076")
            .count(),
        3
    );
    // The Marmolada keeps the rest of its rows: the 19:00 row without its
    // wind, the 19:15 one without its temperature, the 19:45 one without
    // its humidity.
    let marmolada: Vec<_> = output
        .observations
        .iter()
        .filter(|o| o.station.local() == "T0404")
        .collect();
    assert_eq!(marmolada.len(), 4);
    assert!(marmolada[0].wind_speed_ms.is_none() && marmolada[0].temperature_c.is_some());
    assert!(marmolada[1].temperature_c.is_none() && marmolada[1].humidity_pct.is_some());
    assert_eq!(marmolada[2].wind_speed_ms, Some(5.1));
    assert!(marmolada[3].humidity_pct.is_none() && marmolada[3].temperature_c.is_some());

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_empty_answer_is_no_failure_and_no_data() {
    // The service answers so for a station that sends nothing: recorded
    // for Rifugio Alimonta, and the same for a code it does not know.
    let mut fetch = FixtureFetch::new().with(stations_url(), fixture("listaStazioni_evening.xml"));
    for code in ["T0076", "T0403", "T0404"] {
        fetch = fetch.with(values_url(code), fixture("empty.xml"));
    }
    let output = run(&fetch, alps_de_at(), fixtures::evening_at());

    assert!(output.report.failures.is_empty());
    assert_eq!(output.stations.len(), 3);
    assert!(output.observations.is_empty());
    // Each says that it holds no wind.
    assert_eq!(output.report.skipped.len(), 3);
    // A run without a reading is not healthy: the health record says so.
    assert!(!output.report.is_healthy());
}

#[test]
fn a_list_that_is_not_the_expected_xml_fails_the_run() {
    for body in [
        fixture("unavailable.html"),
        b"<?xml version=\"1.0\"?><ArrayOfAnagrafica><anagrafica>".to_vec(),
        fixture("empty.xml"),
        vec![0xff, 0xfe, 0x00],
    ] {
        let fetch = FixtureFetch::new().with(stations_url(), body);
        let output = run(&fetch, alps_south(), recorded_at());
        assert_eq!(output.report.requests, 1);
        assert_eq!(output.report.failures.len(), 1);
        assert_eq!(output.report.failures[0].tag, "stations");
        assert!(output.stations.is_empty());
        assert!(!output.report.is_healthy());
    }
}

#[test]
fn the_manifest_is_valid_and_open() {
    let source = Meteotrentino::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "meteotrentino");
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert_eq!(manifest.licence.name, "CC BY 4.0");
    assert!(
        manifest
            .licence
            .constraints
            .iter()
            .any(|c| c.contains("non hanno subito alcun processo di validazione"))
    );
    assert!(stations_url().starts_with("https://"));
    assert!(values_url("T0129").starts_with("https://"));
}
