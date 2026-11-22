//! Runs the real pipeline over answers recorded from the open-data portal
//! of Regione Lombardia on 2026-11-22, and over the sample of the evening
//! of 2026-11-18, and compares the results with golden files.

use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Observation, Timestamp};
use venti_sources::testing::{
    FixtureFetch, MemoryArchive, assert_golden, block_on, check_adapter_does_no_io,
    check_station_adapter,
};
use venti_sources::{
    LicenceFamily, ObservationSource, PlanCtx, Request, RunOptions, RunOutput, run_observations,
};
use venti_sources_arpalombardia::{
    ArpaLombardia, MAX_URL_BYTES, ROW_LIMIT, WINDOW_HOURS, fixtures, sensors_url,
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

/// The moment the run was recorded: the list of sensors was asked at
/// 08:33:13 UTC and the values a second later.
fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 8, 33, 12).unwrap()
}

/// The whole Alps: 5.0° to 17.2° E, 43.5° to 48.5° N.
fn alps() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap()
}

/// The southern pack's box, `alps-south`.
fn alps_south() -> BoundingBox {
    BoundingBox::new(43.5, 9.4, 46.3, 17.2).unwrap()
}

/// The western pack's box, `alps-west`.
fn alps_west() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 9.4).unwrap()
}

/// The requests of the recorded run, as they were sent: a tag and an
/// address a line.
fn recorded_requests() -> Vec<Request> {
    String::from_utf8(fixture("requests.txt"))
        .unwrap()
        .lines()
        .map(|line| {
            let (tag, url) = line.split_once(' ').unwrap();
            Request::new(tag, url)
        })
        .collect()
}

/// The recorded answers with the addresses they answer.
fn recorded_answers() -> Vec<(String, Vec<u8>)> {
    recorded_requests()
        .into_iter()
        .map(|request| {
            let file = match request.tag.as_str() {
                "sensors" => "sensors.json".to_owned(),
                tag => format!("{}.json", tag.replace('/', "_")),
            };
            (request.url, fixture(&file))
        })
        .collect()
}

fn recorded() -> FixtureFetch {
    FixtureFetch::new().with_all(recorded_answers())
}

#[test]
fn the_adapter_meets_the_contract() {
    check_adapter_does_no_io(Path::new(env!("CARGO_MANIFEST_DIR")));
    check_station_adapter(
        &ArpaLombardia::new(),
        &recorded_answers(),
        recorded_at(),
        alps(),
    );
}

fn run(
    fetch: &FixtureFetch,
    archive: &MemoryArchive,
    now: Timestamp,
    area: BoundingBox,
) -> RunOutput {
    let ctx = PlanCtx { now, area };
    block_on(run_observations(
        &ArpaLombardia::new(),
        fetch,
        archive,
        &ctx,
        &RunOptions::default(),
    ))
}

fn of<'a>(output: &'a RunOutput, code: &str, time: Timestamp) -> &'a Observation {
    output
        .observations
        .iter()
        .find(|o| o.station.local() == code && o.time == time)
        .unwrap_or_else(|| panic!("no observation of {code} at {time}"))
}

/// The requests a run plans after the list of sensors.
fn follow_up(sensors: &[u8], ctx: &PlanCtx) -> Vec<Request> {
    let source = ArpaLombardia::new();
    let request = &source.plan(ctx)[0];
    source.parse(request, sensors, ctx).unwrap().follow_up
}

#[test]
fn the_adapter_asks_what_was_recorded() {
    let ctx = PlanCtx {
        now: recorded_at(),
        area: alps(),
    };
    let mut planned = ArpaLombardia::new().plan(&ctx);
    planned.extend(follow_up(&fixture("sensors.json"), &ctx));
    assert_eq!(planned, recorded_requests());
}

#[test]
fn the_recorded_answers_match_the_golden_file() {
    let archive = MemoryArchive::default();

    let output = run(&recorded(), &archive, recorded_at(), alps());

    // The list of sensors, and the values in three groups of stations.
    assert_eq!(output.report.requests, 4);
    assert!(
        output.report.failures.is_empty(),
        "{:?}",
        output.report.failures
    );
    assert_eq!(archive.stored().len(), 4);
    assert!(output.report.is_healthy());

    // 132 stations with an anemometer, of 250 that have a sensor of the
    // four kinds: all of Lombardy's is in the Alps' box.
    assert_eq!(output.stations.len(), 132);
    // The values are cut to eight stations (see the notice).
    let mut codes: Vec<&str> = output
        .observations
        .iter()
        .map(|o| o.station.local())
        .collect();
    codes.dedup();
    assert_eq!(
        codes,
        ["1347", "1382", "14", "1650", "1680", "629", "848", "857"]
    );
    assert_eq!(output.observations.len(), 212);
    // Crema's thermometer and hygrometer gave values the agency marks as
    // not valid, "NA", for four hours: each is reported, and its wind is
    // kept.
    assert_eq!(output.report.skipped.len(), 50);
    assert!(output.report.skipped.iter().all(|s| {
        s.what.starts_with("629/") && s.reason == "the provider marks the value as not valid (NA)"
    }));
    assert!(output.report.rejected.is_empty());
    // The newest values were of 07:10 by the portal's clock, 06:10 UTC: two
    // hours and 23 minutes before the run.
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 22, 6, 10, 0).unwrap())
    );

    // A reading is put together from five rows of four sensors. Barzio
    // Piani di Bobbio at 06:40 by the portal's clock: wind speed 0.80
    // (operator 1, the mean) and 2.00 (operator 3, the maximum), direction
    // 35 and 48, temperature 0.50, humidity 74. The time is the portal's
    // less an hour, and the units are the canonical ones.
    let bobbio = of(
        &output,
        "1347",
        Utc.with_ymd_and_hms(2026, 11, 22, 5, 40, 0).unwrap(),
    );
    assert_eq!(bobbio.wind_speed_ms, Some(0.8));
    assert_eq!(bobbio.wind_gust_ms, Some(2.0));
    assert_eq!(bobbio.wind_direction_deg, Some(35.0));
    assert_eq!(bobbio.gust_direction_deg, Some(48.0));
    assert_eq!(bobbio.temperature_c, Some(0.5));
    assert_eq!(bobbio.humidity_pct, Some(74.0));
    let station = output
        .stations
        .iter()
        .find(|s| s.id.local() == "1347")
        .unwrap();
    assert_eq!(station.name, "Barzio Piani di Bobbio");
    assert_eq!(station.elevation_m, Some(1713.0));
    // The network has no pressure.
    assert!(output.observations.iter().all(|o| o.pressure_hpa.is_none()));
    // Valfurva S. Caterina has no hygrometer, and its direction sensor
    // gives the mean only.
    assert!(
        output
            .observations
            .iter()
            .filter(|o| o.station.local() == "14")
            .all(|o| o.humidity_pct.is_none() && o.gust_direction_deg.is_none())
    );

    assert_golden(&golden("recorded_run.json"), &output);
}

/// The recorded answers as a run for `area` asks for them: the list of
/// sensors, which is of the whole box, at the area's address, and all the
/// recorded values at the address of the first request of values the run
/// writes, the others answering with none.
fn recorded_for(area: BoundingBox) -> FixtureFetch {
    let ctx = PlanCtx {
        now: recorded_at(),
        area,
    };
    let sensors = fixture("sensors.json");
    let mut values: Vec<serde_json::Value> = Vec::new();
    for name in ["values_1.json", "values_2.json", "values_3.json"] {
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&fixture(name)).unwrap();
        values.extend(rows);
    }
    let mut fetch = FixtureFetch::new().with(sensors_url(&area), sensors.clone());
    for (index, request) in follow_up(&sensors, &ctx).into_iter().enumerate() {
        let body = if index == 0 {
            serde_json::to_vec(&values).unwrap()
        } else {
            b"[]".to_vec()
        };
        fetch = fetch.with(request.url, body);
    }
    fetch
}

#[test]
fn stations_outside_the_area_are_left_out() {
    let south = run(
        &recorded_for(alps_south()),
        &MemoryArchive::default(),
        recorded_at(),
        alps_south(),
    );
    assert!(
        south.report.failures.is_empty(),
        "{:?}",
        south.report.failures
    );
    assert!(
        south
            .stations
            .iter()
            .all(|s| alps_south().contains(s.position))
    );
    let west = run(
        &recorded_for(alps_west()),
        &MemoryArchive::default(),
        recorded_at(),
        alps_west(),
    );
    let first = run(
        &recorded_for(fixtures::area()),
        &MemoryArchive::default(),
        recorded_at(),
        fixtures::area(),
    );
    // 72 in the southern pack's box, 48 west of 9.4° E, and 12 in the
    // first region's, in the upper Valtellina, at Livigno and Chiavenna:
    // every one of the 132 in one box. Lecco v.Sora stands at 9.39995° E,
    // the nearest to the meridian the boxes share, in the western one.
    assert_eq!(
        (
            south.stations.len(),
            west.stations.len(),
            first.stations.len()
        ),
        (72, 48, 12)
    );
    assert!(west.stations.iter().any(|s| s.id.local() == "706"));

    // Values of stations outside the area are left out by the pipeline,
    // which says so: the run writes its values' query for the area's
    // stations only, but the recorded answers are of all.
    let mut codes: Vec<&str> = south
        .observations
        .iter()
        .map(|o| o.station.local())
        .collect();
    codes.dedup();
    // Barzio Piani di Bobbio, the Presolana and Crema. The Passo del
    // Giovà, at 44.68° N the network's southernmost, stands at 9.22° E,
    // in the western box.
    assert_eq!(codes, ["1347", "1680", "629"]);
    let mut left_out: Vec<&str> = south
        .report
        .skipped
        .iter()
        .filter(|s| !s.what.starts_with("629/"))
        .map(|s| s.what.as_str())
        .collect();
    left_out.sort_unstable();
    assert_eq!(
        left_out,
        [
            "arpa-lombardia:1382",
            "arpa-lombardia:14",
            "arpa-lombardia:1650",
            "arpa-lombardia:848",
            "arpa-lombardia:857",
        ]
    );
}

#[test]
fn the_evening_sample_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::evening());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive, fixtures::evening_at(), fixtures::area());

    assert_eq!(fixtures::evening().len(), 2);
    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert!(output.report.rejected.is_empty());
    assert_eq!(output.stations.len(), 3);
    // From 13:00 to 18:30 by the portal's clock: 34 ten minutes of three
    // stations, of which the sample has 33 of Valfurva S. Caterina.
    assert_eq!(output.observations.len(), 101);
    assert!(output.report.is_healthy());
    // The newest values the portal had at 19:05 UTC were of 17:30 UTC.
    assert_eq!(
        output.report.newest_observation,
        Some(Utc.with_ymd_and_hms(2026, 11, 18, 17, 30, 0).unwrap())
    );
    // The two hygrometers in the list gave no value in four days.
    assert!(output.observations.iter().all(|o| o.humidity_pct.is_none()));

    assert_golden(&golden("evening_run.json"), &output);
}

#[test]
fn the_evening_sample_is_of_its_evening() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&fixture("values_evening.json")).unwrap();
    assert_eq!(rows.len(), 439);
    for row in &rows {
        let date = row["data"].as_str().unwrap();
        assert!(
            ("2026-11-18T13:00:00.000"..="2026-11-18T18:30:00.000").contains(&date),
            "{date}"
        );
        let station = row["s"].as_str().unwrap().split('/').next().unwrap();
        assert!(["14", "1090", "1091"].contains(&station), "{station}");
    }
}

#[test]
fn damaged_values_are_reported_and_the_rest_is_kept() {
    // The evening sample with things changed by hand. In the list of
    // sensors, Bormio 2000's height is gone; a station has its latitude
    // written as a word; a second wind sensor of Livigno Mottolino is in
    // km/h; and a sensor names no station. In the values, Valfurva's wind
    // of 18:30 has a date of another form and that of 18:20 the value
    // "calma"; its temperature of 18:20 names no sensor; Mottolino's wind
    // of 18:10 is marked "NA" and its temperature then is -9999, the
    // portal's "missing"; its direction of 18:00 is 7777, the portal's
    // "calm", and its temperature then 999 °C; and Bormio 2000's wind and
    // direction of 18:30 are gone, as a frozen anemometer's are.
    let mut fetch = FixtureFetch::new();
    for (url, _) in fixtures::evening() {
        let body = if url.contains("nf78-nj6b") {
            fixture("sensors_damaged.json")
        } else {
            fixture("values_damaged.json")
        };
        fetch = fetch.with(url, body);
    }

    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        fixtures::area(),
    );

    let report = &output.report;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let reasons: Vec<&str> = report.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(
        reasons,
        [
            "a sensor in km/h, where m/s was expected",
            "a sensor without its id, station or kind",
            "a station without a position",
            "the provider marks the value as not valid (NA)",
            "a row without a sensor the run asked for",
            "value is not a number: calma",
            "time stamp cannot be read",
        ]
    );
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].field, "temperature_c");
    assert_eq!(report.rejected[0].value, 999.0);

    // A station without a height is a station without a height.
    assert_eq!(output.stations.len(), 3);
    let bormio = output
        .stations
        .iter()
        .find(|s| s.id.local() == "1090")
        .unwrap();
    assert_eq!(bormio.elevation_m, None);

    let at = |hour, minute| Utc.with_ymd_and_hms(2026, 11, 18, hour, minute, 0).unwrap();
    // Valfurva's reading of 17:30 UTC kept its direction and temperature,
    // that of 17:20 its direction only.
    let valfurva = of(&output, "14", at(17, 30));
    assert_eq!(valfurva.wind_speed_ms, None);
    assert_eq!(valfurva.wind_direction_deg, Some(126.0));
    assert_eq!(valfurva.temperature_c, Some(-0.45));
    let valfurva = of(&output, "14", at(17, 20));
    assert_eq!(valfurva.wind_speed_ms, None);
    assert_eq!(valfurva.temperature_c, None);
    assert_eq!(valfurva.wind_direction_deg, Some(126.0));
    // A missing value and a calm are no value, and not reported.
    let mottolino = of(&output, "1091", at(17, 10));
    assert_eq!(mottolino.wind_speed_ms, None);
    assert_eq!(mottolino.temperature_c, None);
    assert!(mottolino.wind_gust_ms.is_some());
    let mottolino = of(&output, "1091", at(17, 0));
    assert_eq!(mottolino.wind_direction_deg, None);
    assert!(mottolino.gust_direction_deg.is_some());
    assert!(mottolino.wind_speed_ms.is_some());
    // A reading without wind keeps its temperature.
    let without_wind = of(&output, "1090", at(17, 30));
    assert_eq!(without_wind.wind_speed_ms, None);
    assert_eq!(without_wind.wind_gust_ms, None);
    assert_eq!(without_wind.wind_direction_deg, None);
    assert_eq!(without_wind.temperature_c, Some(0.0));

    assert_golden(&golden("damaged_run.json"), &output);
}

#[test]
fn an_empty_answer_delivers_nothing_and_is_not_healthy() {
    let mut fetch = FixtureFetch::new();
    for (url, body) in fixtures::evening() {
        let body = if url.contains("nf78-nj6b") {
            body.to_vec()
        } else {
            b"[]\n".to_vec()
        };
        fetch = fetch.with(url, body);
    }
    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        fixtures::area(),
    );
    assert!(output.report.failures.is_empty());
    assert!(output.report.skipped.is_empty());
    assert_eq!(output.stations.len(), 3);
    assert!(output.observations.is_empty());
    assert!(!output.report.is_healthy());

    // An empty list of sensors asks for no values at all.
    let fetch = FixtureFetch::new().with(sensors_url(&fixtures::area()), b"[]\n".to_vec());
    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        fixtures::area(),
    );
    assert_eq!(output.report.requests, 1);
    assert!(output.report.failures.is_empty());
    assert!(output.stations.is_empty());
    assert!(output.observations.is_empty());
    assert!(!output.report.is_healthy());
}

#[test]
fn an_answer_that_is_not_json_fails_only_that_request() {
    let mut fetch = FixtureFetch::new();
    for (url, body) in fixtures::evening() {
        let body = if url.contains("nf78-nj6b") {
            body.to_vec()
        } else {
            b"<html><head><title>414 Request-URI Too Large</title></head></html>".to_vec()
        };
        fetch = fetch.with(url, body);
    }
    let output = run(
        &fetch,
        &MemoryArchive::default(),
        fixtures::evening_at(),
        fixtures::area(),
    );
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "values/1");
    // The stations of the list are still delivered.
    assert_eq!(output.stations.len(), 3);
    assert!(!output.report.is_healthy());
}

#[test]
fn the_plan_for_the_southern_box_and_what_it_costs() {
    let source = ArpaLombardia::new();
    let ctx = PlanCtx {
        now: recorded_at(),
        area: alps_south(),
    };
    let plan = source.plan(&ctx);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].tag, "sensors");
    // The portal is asked for the sensors in the box only.
    assert!(
        plan[0]
            .url
            .contains("lat%20between%2043.5%20and%2046.3%20AND%20lng%20between%209.4%20and%2017.2")
    );
    // The recorded list is of the whole box; read for the southern one, it
    // gives what the portal's list for that box would.
    let values = follow_up(&fixture("sensors.json"), &ctx);
    let tags: Vec<&str> = values.iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(tags, ["values/1", "values/2"]);
    assert!(values.iter().all(|r| r.url.len() <= MAX_URL_BYTES));
    // Every request asks for seven hours of values by the portal's clock:
    // 08:33 UTC is 09:33 there, and seven hours before, on the ten
    // minutes, is 02:30.
    assert_eq!(WINDOW_HOURS, 7);
    assert!(
        values
            .iter()
            .all(|r| r.url.contains("data%20%3E=%20%272026-11-22T02:30:00%27"))
    );
    assert!(
        values
            .iter()
            .all(|r| r.url.contains(&format!("LIMIT%20{ROW_LIMIT}")))
    );

    // The whole box: three requests of values. The southern box: two.
    let whole = follow_up(
        &fixture("sensors.json"),
        &PlanCtx {
            now: recorded_at(),
            area: alps(),
        },
    );
    assert_eq!(whole.len(), 3);
    // The western box: one, for the 48 stations west of 9.4° E.
    let west = follow_up(
        &fixture("sensors.json"),
        &PlanCtx {
            now: recorded_at(),
            area: alps_west(),
        },
    );
    assert_eq!(west.len(), 1);

    // What it costs: three requests a run for the southern box, a run every
    // half hour. Recorded whole for all of Lombardy, the answers were
    // 124,918 bytes for the sensors and 737,927, 801,512 and 197,948 for
    // the values, each within what the manifest allows.
    let manifest = source.manifest();
    assert_eq!(manifest.cadence_seconds, 1800);
    assert_eq!(manifest.requests_per_run(), None);
    assert!(801_512 < manifest.max_response_bytes());
}

/// The units are those of the sensors' own list: every sensor of the four
/// kinds named the canonical unit when recorded, so none was left out.
#[test]
fn every_sensor_names_the_canonical_unit() {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&fixture("sensors.json")).unwrap();
    assert_eq!(rows.len(), 683);
    for row in &rows {
        let unit = row["unit_dimisura"].as_str().unwrap();
        let expected = match row["tipologia"].as_str().unwrap() {
            "Velocità Vento" => "m/s",
            "Direzione Vento" => "°",
            "Temperatura" => "°C",
            "Umidità Relativa" => "%",
            other => panic!("a kind not asked for: {other}"),
        };
        assert_eq!(unit, expected);
    }
}

#[test]
fn the_manifest_is_valid_and_the_addresses_are_encrypted() {
    let source = ArpaLombardia::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "arpa-lombardia");
    assert_eq!(manifest.countries, ["IT"]);
    assert_eq!(manifest.licence.family(), LicenceFamily::Open);
    assert!(sensors_url(&alps()).starts_with("https://"));
    assert!(
        recorded_requests()
            .iter()
            .all(|r| r.url.starts_with("https://"))
    );
}
