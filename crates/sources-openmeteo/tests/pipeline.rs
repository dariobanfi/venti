//! Runs the real pipeline over answers recorded from an Open-Meteo instance
//! on 2026-11-21 at 09:25 UTC and compares the result with golden files.
//! Damaged and partial answers are made from the recorded ones, and each
//! test says what it changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Duration, TimeZone, Utc};
use serde_json::{Value, json};
use venti_domain::{BoundingBox, ForecastHour, ForecastSeries, GeoPoint, Timestamp, WindAloft};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{ForecastRunOutput, ForecastSource, PlanCtx, RunOptions, run_forecast};
use venti_sources_openmeteo::{OpenMeteo, fixtures};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn source() -> OpenMeteo {
    OpenMeteo::new(fixtures::BASE).unwrap()
}

/// Shortly after the answers were recorded.
fn ctx() -> PlanCtx {
    PlanCtx {
        now: fixtures::recorded_at(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

/// The east shore of the Tegernsee: the first of the recorded places.
fn tegernsee() -> GeoPoint {
    GeoPoint::new(47.70, 11.80).unwrap()
}

fn at(day: u32, hour: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
}

fn run_for(point: GeoPoint, fetch: &FixtureFetch, archive: &MemoryArchive) -> ForecastRunOutput {
    block_on(run_forecast(
        &source(),
        point,
        fetch,
        archive,
        &ctx(),
        &RunOptions::default(),
    ))
}

fn recorded_run(point: GeoPoint) -> ForecastRunOutput {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    run_for(point, &fetch, &MemoryArchive::default())
}

/// The four addresses a forecast for Tegernsee asks, in the order the
/// adapter asks them: the run of ICON-D2, the run of ICON-EU, the series of
/// ICON-D2, the series of ICON-EU.
fn urls() -> [String; 4] {
    let source = source();
    let runs = source.plan(tegernsee(), &ctx());
    let series = |index: usize| {
        let held = recorded(&runs[index].url);
        let asked = source
            .parse(&runs[index], &held, tegernsee(), &ctx())
            .unwrap();
        asked.follow_up[0].url.clone()
    };
    [
        runs[0].url.clone(),
        runs[1].url.clone(),
        series(0),
        series(1),
    ]
}

/// The recorded answer to an address.
fn recorded(url: &str) -> Vec<u8> {
    fixtures::recorded()
        .into_iter()
        .find(|(recorded, _)| recorded == url)
        .map(|(_, body)| body.to_vec())
        .unwrap_or_else(|| panic!("nothing recorded for {url}"))
}

/// The recorded answer to an address, changed.
fn changed(url: &str, change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut answer: Value = serde_json::from_slice(&recorded(url)).unwrap();
    change(&mut answer);
    serde_json::to_vec(&answer).unwrap()
}

/// A run for Tegernsee in which the answers to some addresses are replaced.
fn run_with(replaced: Vec<(String, Vec<u8>)>) -> ForecastRunOutput {
    let mut fetch = FixtureFetch::new().with_all(fixtures::recorded());
    for (url, body) in replaced {
        fetch = fetch.with(url, body);
    }
    run_for(tegernsee(), &fetch, &MemoryArchive::default())
}

fn series<'a>(output: &'a ForecastRunOutput, model: &str) -> &'a ForecastSeries {
    output
        .series
        .iter()
        .find(|series| series.model.as_str() == model)
        .unwrap_or_else(|| panic!("no series of {model}"))
}

fn hour(series: &ForecastSeries, time: Timestamp) -> &ForecastHour {
    series
        .hours
        .iter()
        .find(|hour| hour.time == time)
        .unwrap_or_else(|| panic!("no hour {time}"))
}

/// An hour as one line: every value the record holds, in a fixed order.
fn line(hour: &ForecastHour) -> String {
    let mut parts: Vec<String> = hour
        .fields()
        .into_iter()
        .filter_map(|(field, value)| value.map(|value| format!("{field} {value}")))
        .collect();
    for level in &hour.winds_aloft {
        parts.push(level_text(level));
    }
    parts.join("; ")
}

fn level_text(level: &WindAloft) -> String {
    let mut text = match (level.above_ground_m, level.pressure_hpa) {
        (Some(height), None) => format!("{height} m above ground:"),
        (None, Some(pressure)) => format!("{pressure} hPa:"),
        other => panic!("a level that is {other:?}"),
    };
    let mut level = level.clone();
    for (field, value) in level.fields_mut() {
        if let Some(value) = value {
            text.push_str(&format!(" {field} {value}"));
        }
    }
    text
}

/// The run in the form the golden files keep it: each series with its
/// first `hours` hours as one line each, and the report.
fn summary(output: &ForecastRunOutput, hours: usize) -> Value {
    let series: Vec<Value> = output
        .series
        .iter()
        .map(|series| {
            let hours: BTreeMap<String, String> = series
                .hours
                .iter()
                .take(hours)
                .map(|hour| (hour.time.to_rfc3339(), line(hour)))
                .collect();
            json!({
                "source": series.source,
                "model": series.model,
                "run": series.run,
                "available_at": series.available_at,
                "point": series.point,
                "cell": series.cell,
                "cell_elevation_m": series.cell_elevation_m,
                "hours": hours,
            })
        })
        .collect();
    json!({ "series": series, "report": output.report })
}

#[test]
fn a_recorded_run_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let archive = MemoryArchive::default();

    let output = run_for(tegernsee(), &fetch, &archive);

    // Two requests per model, each archived.
    assert_eq!(output.report.requests, 4);
    assert_eq!(archive.stored().len(), 4);
    assert!(
        output.report.failures.is_empty(),
        "{:?}",
        output.report.failures
    );
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
    assert!(output.report.is_healthy());
    assert_eq!(output.report.series, 2);
    assert_eq!(output.report.newest_run, Some(at(21, 6)));

    for series in &output.series {
        assert_eq!(series.source.as_str(), "open-meteo");
        assert_eq!(series.point, tegernsee());
    }
    assert_golden(&golden("recorded_run.json"), &summary(&output, usize::MAX));
}

#[test]
fn a_series_holds_the_hours_its_run_computed_and_no_others() {
    let output = recorded_run(tegernsee());

    // ICON-D2: the run of 06 UTC of the day, ready at the instance 69
    // minutes later, reaches 48 hours. The answer began at midnight, as
    // "today" does: its first seven hours are of runs before this one and
    // are left out, and from the hour after the run began every hour to
    // the run's end, on the morning of the day after tomorrow, is in the
    // series. The answer's hours after that are empty.
    let icon_d2 = series(&output, "icon-d2");
    assert_eq!(icon_d2.run, at(21, 6));
    assert_eq!(
        icon_d2.available_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 21, 7, 9, 12).unwrap())
    );
    assert_eq!(icon_d2.hours.len(), 48);
    assert_eq!(icon_d2.hours[0].time, at(21, 7));
    assert_eq!(icon_d2.hours[47].time, at(23, 6));
    let answered: Value = serde_json::from_slice(&recorded(&urls()[2])).unwrap();
    let values = answered["hourly"]["temperature_2m"].as_array().unwrap();
    assert_eq!(values.len(), 72);
    assert!(values[..55].iter().all(|value| value.is_number()));
    assert!(values[55..].iter().all(|value| value.is_null()));

    // ICON-EU: the run of 06 UTC is a main run and reaches 78 hours hour
    // by hour, to noon of the third day after today, which is why the
    // model is asked for four days. The instance answered all 96 hours it
    // was asked for: the first seven are of runs before this one, and
    // those after the run's 78th are filled in from values three hours
    // apart, and the instance says neither. They are left out.
    let icon_eu = series(&output, "icon-eu");
    assert_eq!(icon_eu.run, at(21, 6));
    assert_eq!(
        icon_eu.available_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 21, 9, 14, 18).unwrap())
    );
    assert_eq!(icon_eu.hours.len(), 78);
    assert_eq!(icon_eu.hours[0].time, at(21, 7));
    assert_eq!(icon_eu.hours[77].time, at(24, 12));
    let answered: Value = serde_json::from_slice(&recorded(&urls()[3])).unwrap();
    let values = answered["hourly"]["temperature_2m"].as_array().unwrap();
    assert_eq!(values.len(), 96);
    assert!(values.iter().all(|value| value.is_number()));
    // What "filled in" looks like: the sky of the last evening closes from
    // 0 to 100 % between 18 and 21 UTC in steps of a third.
    let cloud = &answered["hourly"]["cloud_cover"];
    assert_eq!(
        [&cloud[90], &cloud[91], &cloud[92], &cloud[93]],
        [&json!(0), &json!(33), &json!(67), &json!(100)]
    );

    // Every hour follows the one before.
    for series in &output.series {
        for pair in series.hours.windows(2) {
            assert_eq!(pair[1].time - pair[0].time, Duration::hours(1));
        }
    }
}

#[test]
fn what_the_instance_says_arrives_in_the_record() {
    let output = recorded_run(tegernsee());
    let icon_d2 = series(&output, "icon-d2");
    let icon_eu = series(&output, "icon-eu");

    // The cell each model answered from, and the height of its ground. The
    // lake lies at 725 m; the models' ground east of it is higher.
    assert!((icon_d2.cell.lat() - 47.70034).abs() < 1e-5);
    assert!((icon_d2.cell.lon() - 11.799969).abs() < 1e-5);
    assert_eq!(icon_d2.cell_elevation_m, Some(1069.0));
    assert!(icon_d2.cell.distance_m(tegernsee()) < 100.0);
    assert_eq!(icon_eu.cell_elevation_m, Some(1100.0));
    let away = icon_eu.cell.distance_m(tegernsee());
    assert!((2000.0..3000.0).contains(&away), "{away}");

    // One hour of ICON-D2, value by value as the instance sent it: a wet
    // early afternoon under a closed sky.
    let afternoon = hour(icon_d2, at(21, 13));
    assert_eq!(afternoon.wind_speed_ms, Some(1.21));
    assert_eq!(afternoon.wind_direction_deg, Some(246.0));
    assert_eq!(afternoon.wind_gust_ms, Some(10.3));
    assert_eq!(afternoon.temperature_c, Some(1.5));
    assert_eq!(afternoon.dew_point_c, Some(0.9));
    assert_eq!(afternoon.cloud_cover_pct, Some(100.0));
    assert_eq!(afternoon.cloud_cover_low_pct, Some(92.0));
    assert_eq!(afternoon.cloud_cover_mid_pct, Some(100.0));
    assert_eq!(afternoon.cloud_cover_high_pct, Some(0.0));
    assert_eq!(afternoon.precipitation_mm, Some(0.2));
    assert_eq!(afternoon.cape_j_kg, Some(0.0));
    assert_eq!(afternoon.freezing_level_m, Some(1270.0));
    assert_eq!(afternoon.updraft_ms, Some(0.17));
    // No convective cloud at this hour: the instance wrote 0 for ICON-D2
    // and -500 for ICON-EU, and neither is a height.
    assert_eq!(afternoon.convective_cloud_base_m, None);
    assert_eq!(afternoon.convective_cloud_top_m, None);
    let afternoon_eu = hour(icon_eu, at(21, 13));
    assert_eq!(afternoon_eu.convective_cloud_base_m, None);
    assert_eq!(afternoon_eu.convective_cloud_top_m, None);
    // The heat the ground gives to the air. The model counts it towards
    // the ground: the instance wrote -13.9 for this hour of daylight and
    // 20.8 for 02 UTC of the night after, under a clear sky, when the air
    // warmed the ground. The record counts it upward.
    assert_eq!(afternoon.heat_to_air_w_m2, Some(13.9));
    let night = hour(icon_d2, at(22, 2));
    assert_eq!(night.cloud_cover_pct, Some(0.0));
    assert_eq!(night.heat_to_air_w_m2, Some(-20.8));
    assert!(
        icon_eu
            .hours
            .iter()
            .all(|hour| hour.heat_to_air_w_m2.is_some())
    );
    // ICON-EU has no updraft, and was not asked for one.
    assert!(icon_eu.hours.iter().all(|hour| hour.updraft_ms.is_none()));
    assert!(icon_d2.hours.iter().all(|hour| hour.updraft_ms.is_some()));

    // Higher up: three heights above the ground, then the pressure
    // surfaces. The surface of 950 hPa lay at 586 m, far under the cell's
    // ground at 1069 m, and is not in the record.
    let levels = &afternoon.winds_aloft;
    let places: Vec<(Option<f64>, Option<f64>)> = levels
        .iter()
        .map(|level| (level.above_ground_m, level.pressure_hpa))
        .collect();
    assert_eq!(
        places,
        [
            (Some(80.0), None),
            (Some(120.0), None),
            (Some(180.0), None),
            (None, Some(850.0)),
            (None, Some(700.0)),
            (None, Some(600.0)),
            (None, Some(500.0)),
        ]
    );
    assert_eq!(levels[0].wind_speed_ms, Some(2.8));
    assert_eq!(levels[0].wind_direction_deg, Some(272.0));
    assert_eq!(levels[0].temperature_c, Some(1.0));
    assert_eq!(levels[0].altitude_m, None);
    assert_eq!(levels[3].altitude_m, Some(1483.0));
    assert_eq!(levels[3].wind_speed_ms, Some(8.93));
    assert_eq!(levels[3].wind_direction_deg, Some(277.0));
    assert_eq!(levels[3].temperature_c, Some(-1.1));
    assert_eq!(levels[6].altitude_m, Some(5518.0));
    assert_eq!(levels[6].temperature_c, Some(-25.7));

    // An instance sometimes writes "-0.0": this recording has it for the
    // dew point of 01 UTC, an hour before the run began. Here the hour
    // looked at is given one. It arrives as plain zero.
    let [_, _, series_d2, _] = urls();
    let output = run_with(vec![(
        series_d2.clone(),
        changed(&series_d2, |answer| {
            answer["hourly"]["dew_point_2m"][13] = json!(-0.0);
        }),
    )]);
    let dew_point = hour(series(&output, "icon-d2"), at(21, 13)).dew_point_c;
    assert_eq!(dew_point, Some(0.0));
    assert!(dew_point.unwrap().is_sign_positive());

    // Convective cloud, where the model has some, comes with its heights.
    let with_cloud = icon_d2
        .hours
        .iter()
        .filter(|hour| hour.convective_cloud_base_m.is_some())
        .count();
    assert!(with_cloud > 0 && with_cloud < icon_d2.hours.len());
    for hour in icon_d2.hours.iter().chain(&icon_eu.hours) {
        if let (Some(base), Some(top)) = (hour.convective_cloud_base_m, hour.convective_cloud_top_m)
        {
            assert!(base > 0.0 && top >= base, "{base} {top}");
        }
    }
}

#[test]
fn every_recorded_place_gives_a_healthy_run() {
    for place in fixtures::places() {
        let output = recorded_run(place);
        assert!(output.report.is_healthy(), "{place:?}: {:?}", output.report);
        assert!(output.report.skipped.is_empty(), "{place:?}");
        assert!(output.report.rejected.is_empty(), "{place:?}");
        let icon_d2 = series(&output, "icon-d2");
        let icon_eu = series(&output, "icon-eu");
        assert_eq!((icon_d2.hours.len(), icon_eu.hours.len()), (48, 78));
        // The nearest cell of a model is never far: ICON-D2's cells are
        // about 2 km wide, ICON-EU's about 6.
        assert!(icon_d2.cell.distance_m(place) < 2500.0, "{place:?}");
        assert!(icon_eu.cell.distance_m(place) < 7000.0, "{place:?}");
        for series in &output.series {
            assert_eq!(series.point, place);
            assert!(series.cell_elevation_m.is_some());
        }
    }
    // A place nothing was recorded for: the instance does not answer.
    let elsewhere = GeoPoint::new(47.50, 11.10).unwrap();
    let output = recorded_run(elsewhere);
    assert!(!output.report.is_healthy());
    assert!(output.series.is_empty());
}

#[test]
fn a_model_whose_run_cannot_be_read_delivers_no_series() {
    let [run_d2, run_eu, _, series_eu] = urls();

    // The instance does not say which run of ICON-D2 it holds. Nothing of
    // ICON-D2 is asked for or delivered; ICON-EU is, and the run is not
    // healthy.
    let output = run_with(vec![(run_d2.clone(), b"{}".to_vec())]);
    assert_eq!(output.report.requests, 3);
    assert_eq!(output.series.len(), 1);
    assert_eq!(output.series[0].model.as_str(), "icon-eu");
    assert!(!output.report.is_healthy());
    let error = &output.report.failures[0].error;
    assert!(error.contains("does not say which run"), "{error}");

    // The same when the instance has no such file (without a key for
    // syncing set, an instance answers 404 here), when the file is a page
    // for people, and when the run began at no full hour.
    let failed = |body: Vec<u8>| {
        let output = run_with(vec![(run_d2.clone(), body)]);
        assert_eq!(output.series.len(), 1);
        output.report.failures[0].error.clone()
    };
    let error = failed(b"<html>404 Not Found</html>".to_vec());
    assert!(error.contains("does not say which run"), "{error}");
    let error = failed(changed(&run_d2, |meta| {
        meta["last_run_initialisation_time"] = json!(at(21, 6).timestamp() + 60);
    }));
    assert!(error.contains("cannot have begun"), "{error}");
    // An instance that keeps a value every quarter of an hour under this
    // name is not what the adapter reads.
    let error = failed(changed(&run_d2, |meta| {
        meta["temporal_resolution_seconds"] = json!(900);
    }));
    assert!(error.contains("every 900 seconds"), "{error}");

    // Without the times the instance had the run ready and the run ends,
    // the series is delivered without the first and by the reach of the run.
    let output = run_with(vec![(
        run_eu.clone(),
        changed(&run_eu, |meta| {
            let meta = meta.as_object_mut().unwrap();
            meta.remove("last_run_availability_time");
            meta.remove("data_end_time");
        }),
    )]);
    assert!(output.report.is_healthy());
    let icon_eu = series(&output, "icon-eu");
    assert_eq!(icon_eu.available_at, None);
    assert_eq!(icon_eu.hours.len(), 78);

    // An instance that says a run ends earlier than runs of its kind do is
    // believed: the run of 06 UTC, cut off at three in the afternoon.
    let output = run_with(vec![(
        run_eu.clone(),
        changed(&run_eu, |meta| {
            meta["data_end_time"] = json!(at(21, 15).timestamp());
        }),
    )]);
    assert_eq!(series(&output, "icon-eu").hours.len(), 8);

    // A run of ICON-EU between the main runs reaches 30 hours: were the
    // instance still at the run of 03 UTC, the answer's hours from 04 UTC
    // to 09 UTC of the next day would be its own, and no others, though
    // the instance answers all 96.
    let output = run_with(vec![(
        run_eu.clone(),
        changed(&run_eu, |meta| {
            meta["last_run_initialisation_time"] = json!(at(21, 3).timestamp());
            meta["data_end_time"] = json!((at(21, 3) + Duration::hours(31)).timestamp());
        }),
    )]);
    let icon_eu = series(&output, "icon-eu");
    assert_eq!(icon_eu.hours.len(), 30);
    assert_eq!(icon_eu.hours[0].time, at(21, 4));
    assert_eq!(icon_eu.hours[29].time, at(22, 9));

    // The main run of 18 UTC ends 78 hours later at midnight. Were the
    // instance at that of the evening before, the answer's four days
    // would hold all that is left of it, to its last hour: 73 hours, the
    // run's first five having been yesterday's.
    let output = run_with(vec![(
        run_eu.clone(),
        changed(&run_eu, |meta| {
            meta["last_run_initialisation_time"] = json!(at(20, 18).timestamp());
            meta["data_end_time"] = json!((at(20, 18) + Duration::hours(121)).timestamp());
        }),
    )]);
    let icon_eu = series(&output, "icon-eu");
    assert_eq!(icon_eu.hours.len(), 73);
    assert_eq!(icon_eu.hours[0].time, at(21, 0));
    assert_eq!(icon_eu.hours[72].time, at(24, 0));

    // An instance that stopped fetching runs days ago has no hour of today
    // to give: no series, and the report says why.
    let stale = at(15, 12).timestamp();
    let output = run_with(vec![
        (
            run_d2.clone(),
            changed(&run_d2, |meta| {
                meta["last_run_initialisation_time"] = json!(stale);
                meta.as_object_mut().unwrap().remove("data_end_time");
            }),
        ),
        (
            run_eu.clone(),
            changed(&run_eu, |meta| {
                meta["last_run_initialisation_time"] = json!(stale);
                meta.as_object_mut().unwrap().remove("data_end_time");
            }),
        ),
    ]);
    assert!(output.series.is_empty());
    assert!(!output.report.is_healthy());
    assert_eq!(output.report.skipped.len(), 2);
    // Each model is told by the days it was asked for.
    let reasons: Vec<&str> = output
        .report
        .skipped
        .iter()
        .map(|skipped| skipped.reason.as_str())
        .collect();
    assert_eq!(
        reasons,
        [
            "the run of 2026-11-15 12:00 UTC holds no hour of today and the 2 days after",
            "the run of 2026-11-15 12:00 UTC holds no hour of today and the 3 days after",
        ]
    );
    let _ = series_eu;
}

#[test]
fn a_damaged_answer_costs_what_cannot_be_read_and_no_more() {
    let [_, _, series_d2, _] = urls();

    // The recorded answer of ICON-D2 with seven things wrong: the wind in
    // km/h, no unit for the gusts, the dew point missing, one temperature
    // too few, cloud cover as words, no precipitation for any hour, and a
    // cloud cover of 140 % at 20 UTC.
    let damaged = changed(&series_d2, |answer| {
        answer["hourly_units"]["wind_speed_10m"] = json!("km/h");
        answer["hourly_units"]
            .as_object_mut()
            .unwrap()
            .remove("wind_gusts_10m");
        answer["hourly"]
            .as_object_mut()
            .unwrap()
            .remove("dew_point_2m");
        answer["hourly"]["temperature_2m"]
            .as_array_mut()
            .unwrap()
            .pop();
        answer["hourly"]["cloud_cover"] = json!(vec!["overcast"; 72]);
        answer["hourly"]["precipitation"] = json!(vec![Value::Null; 72]);
        answer["hourly"]["cloud_cover_low"][20] = json!(140);
    });

    let output = run_with(vec![(series_d2.clone(), damaged)]);

    // Both series are delivered, and every request worked.
    assert!(output.report.is_healthy());
    assert_eq!(output.series.len(), 2);
    let icon_d2 = series(&output, "icon-d2");
    assert_eq!(icon_d2.hours.len(), 48);
    let evening = hour(icon_d2, at(21, 20));
    assert_eq!(evening.wind_speed_ms, None);
    assert_eq!(evening.wind_gust_ms, None);
    assert_eq!(evening.dew_point_c, None);
    assert_eq!(evening.temperature_c, None);
    assert_eq!(evening.cloud_cover_pct, None);
    assert_eq!(evening.precipitation_mm, None);
    assert_eq!(evening.cloud_cover_low_pct, None);
    // What could be read is there.
    assert_eq!(evening.wind_direction_deg, Some(256.0));
    assert_eq!(evening.cloud_cover_mid_pct, Some(0.0));
    assert_eq!(evening.winds_aloft.len(), 7);
    assert_eq!(
        hour(icon_d2, at(21, 21)).cloud_cover_low_pct,
        Some(
            hour(series(&recorded_run(tegernsee()), "icon-d2"), at(21, 21))
                .cloud_cover_low_pct
                .unwrap()
        )
    );

    // Everything that was left out is in the report: six variables by the
    // adapter, one value by the pipeline.
    let skipped: BTreeMap<&str, &str> = output
        .report
        .skipped
        .iter()
        .map(|skipped| (skipped.what.as_str(), skipped.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 6, "{skipped:?}");
    assert!(skipped["wind_speed_10m"].contains("\"km/h\" where \"m/s\" was asked for"));
    assert!(skipped["wind_gusts_10m"].contains("does not say in which unit"));
    assert!(skipped["dew_point_2m"].contains("does not have it"));
    assert!(skipped["temperature_2m"].contains("71 values for 72 hours"));
    assert!(skipped["cloud_cover"].contains("not numbers"));
    assert!(skipped["precipitation"].contains("no value for any hour"));
    assert_eq!(output.report.rejected.len(), 1);
    assert_eq!(output.report.rejected[0].field, "cloud_cover_low_pct");
    assert_eq!(output.report.rejected[0].value, Some(140.0));

    // The report, and three hours of each series.
    assert_golden(&golden("damaged_run.json"), &summary(&output, 3));
}

#[test]
fn an_answer_that_is_no_forecast_delivers_no_series_of_that_model() {
    let [_, _, series_d2, _] = urls();
    let failed = |body: Vec<u8>| {
        let output = run_with(vec![(series_d2.clone(), body)]);
        // ICON-EU is untouched, and the run is not healthy.
        assert_eq!(output.series.len(), 1);
        assert_eq!(output.series[0].model.as_str(), "icon-eu");
        assert!(!output.report.is_healthy());
        assert_eq!(output.report.failures.len(), 1);
        output.report.failures[0].error.clone()
    };

    // What an instance answers when it refuses, as Open-Meteo's public API
    // answered the machine that tried to record there.
    let refused =
        br#"{"error":true,"reason":"Daily API request limit exceeded. Please try again tomorrow."}"#;
    let error = failed(refused.to_vec());
    assert!(error.contains("the instance refused"), "{error}");
    assert!(error.contains("Daily API request limit"), "{error}");

    // The connection broke in the middle of the answer.
    let whole = recorded(&series_d2);
    let error = failed(whole[..whole.len() / 2].to_vec());
    assert!(error.contains("not a forecast"), "{error}");

    // A page for people.
    let error = failed(b"<html><body>502 Bad Gateway</body></html>".to_vec());
    assert!(error.contains("not a forecast"), "{error}");

    // Times that are not seconds since 1970: the instance ignored what was
    // asked for.
    let error = failed(changed(&series_d2, |answer| {
        answer["hourly_units"]["time"] = json!("iso8601");
    }));
    assert!(error.contains("seconds since 1970"), "{error}");

    // No times at all, and times that are words.
    let error = failed(changed(&series_d2, |answer| {
        answer["hourly"].as_object_mut().unwrap().remove("time");
    }));
    assert!(error.contains("no list of times"), "{error}");
    let error = failed(changed(&series_d2, |answer| {
        answer["hourly"]["time"] = json!(vec!["2026-11-21T00:00"; 72]);
    }));
    assert!(error.contains("no list of times"), "{error}");

    // An answer that does not say which cell it is for, or names one off
    // the globe.
    let error = failed(changed(&series_d2, |answer| {
        answer.as_object_mut().unwrap().remove("latitude");
    }));
    assert!(error.contains("which cell"), "{error}");
    let error = failed(changed(&series_d2, |answer| {
        answer["latitude"] = json!(147.7);
    }));
    assert!(error.contains("which cell"), "{error}");

    // An instance that does not answer at all.
    let output = run_for(tegernsee(), &FixtureFetch::new(), &MemoryArchive::default());
    assert!(output.series.is_empty());
    assert_eq!(output.report.requests, 2);
    assert_eq!(output.report.failures[0].error, "HTTP status 404");
}

#[test]
fn an_answer_for_a_cell_far_from_the_place_is_refused_by_the_pipeline() {
    let [_, _, series_d2, _] = urls();
    // An instance that answers for a cell half a degree away: outside the
    // model's area the nearest cell can lie anywhere.
    let far = changed(&series_d2, |answer| {
        answer["latitude"] = json!(48.3);
    });
    let output = run_with(vec![(series_d2.clone(), far)]);
    assert_eq!(output.series.len(), 1);
    assert_eq!(output.report.rejected.len(), 1);
    assert_eq!(output.report.rejected[0].field, "series");
    let reason = &output.report.rejected[0].reason;
    assert!(reason.contains("km from the place"), "{reason}");

    // An answer without the height of the model's ground is delivered
    // without it, and then nothing can be said to lie under the ground:
    // the surface of 950 hPa stays.
    let no_height = changed(&series_d2, |answer| {
        answer["elevation"] = Value::Null;
    });
    let output = run_with(vec![(series_d2.clone(), no_height)]);
    let icon_d2 = series(&output, "icon-d2");
    assert_eq!(icon_d2.cell_elevation_m, None);
    assert_eq!(hour(icon_d2, at(21, 20)).winds_aloft.len(), 8);
}

#[test]
fn a_cell_of_water_has_no_height_of_the_ground() {
    let [_, _, series_d2, _] = urls();
    // What an instance answers where the nearest cell is one the model
    // counts as water: a height of 0, wherever the water lies. Seen on
    // 2026-11-21 for the cell of ICON-D2 at 47.61 N 9.50 E, over Lake
    // Constance, which lies at 395 m. Here the recorded cell beside the
    // Tegernsee is given that height, and nothing else is changed.
    let water = changed(&series_d2, |answer| {
        answer["elevation"] = json!(0.0);
    });
    let output = run_with(vec![(series_d2.clone(), water)]);
    let icon_d2 = series(&output, "icon-d2");
    assert_eq!(icon_d2.cell_elevation_m, None);
    // Its values are handed on as the model computed them, hour for hour,
    // and with no ground to lie under, the surface of 950 hPa stays.
    let recorded = recorded_run(tegernsee());
    assert_eq!(
        icon_d2.hours.len(),
        series(&recorded, "icon-d2").hours.len()
    );
    assert_eq!(
        hour(icon_d2, at(21, 12)).temperature_c,
        hour(series(&recorded, "icon-d2"), at(21, 12)).temperature_c
    );
    assert_eq!(hour(icon_d2, at(21, 20)).winds_aloft.len(), 8);
    // Nothing was refused for it: the height is not a value out of range,
    // it is one the instance does not have.
    assert!(
        output
            .report
            .rejected
            .iter()
            .all(|rejection| rejection.field != "cell_elevation_m")
    );
    // The other model's cell keeps its ground.
    assert_eq!(series(&output, "icon-eu").cell_elevation_m, Some(1100.0));
}
