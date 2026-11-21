//! Runs the real pipeline for several places at once over answers recorded
//! from an Open-Meteo instance on 2026-11-21 at 09:25 UTC, and compares the
//! result with a golden file. Damaged answers are made from the recorded
//! ones, and each test says what it changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use venti_domain::{BoundingBox, GeoPoint, ModelId, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{ForecastRunOutput, ForecastSource, PlanCtx, RunOptions, run_forecast_places};
use venti_sources_openmeteo::{OpenMeteo, PLACES_PER_REQUEST, fixtures};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn source() -> OpenMeteo {
    OpenMeteo::new(fixtures::BASE).unwrap()
}

fn ctx() -> PlanCtx {
    PlanCtx {
        now: fixtures::places_recorded_at(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

fn model(name: &str) -> ModelId {
    ModelId::new(name).unwrap()
}

fn at(day: u32, hour: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
}

fn run_with(fetch: &FixtureFetch, of: &str, points: &[GeoPoint]) -> ForecastRunOutput {
    block_on(run_forecast_places(
        &source(),
        &model(of),
        points,
        fetch,
        &MemoryArchive::default(),
        &ctx(),
        &RunOptions::default(),
    ))
}

fn recorded_run(of: &str) -> ForecastRunOutput {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded_places());
    run_with(&fetch, of, &fixtures::grid_places())
}

/// The address of the forecast of a model for the four places.
fn series_url(of: &str) -> String {
    let source = source();
    let points = fixtures::grid_places();
    let run = &source.plan_places(&model(of), &points, &ctx())[0];
    let held = recorded(&run.url);
    source
        .parse_places(run, &held, &points, &ctx())
        .unwrap()
        .follow_up[0]
        .url
        .clone()
}

fn recorded(url: &str) -> Vec<u8> {
    fixtures::recorded_places()
        .into_iter()
        .find(|(recorded, _)| recorded == url)
        .map(|(_, body)| body.to_vec())
        .unwrap_or_else(|| panic!("nothing recorded for {url}"))
}

/// A run of a model for the four places in which its forecast's answer is
/// changed.
fn changed_run(of: &str, change: impl FnOnce(&mut Value)) -> ForecastRunOutput {
    let url = series_url(of);
    let mut answer: Value = serde_json::from_slice(&recorded(&url)).unwrap();
    change(&mut answer);
    let fetch = FixtureFetch::new()
        .with_all(fixtures::recorded_places())
        .with(url, serde_json::to_vec(&answer).unwrap());
    run_with(&fetch, of, &fixtures::grid_places())
}

/// Each series with its first hours as the values of a few fields, and the
/// report: enough to see that every place got its own answer.
fn summary(output: &ForecastRunOutput) -> Value {
    let series: Vec<Value> = output
        .series
        .iter()
        .map(|series| {
            let hours: BTreeMap<String, Value> = series
                .hours
                .iter()
                .take(3)
                .map(|hour| {
                    let values = json!({
                        "temperature_c": hour.temperature_c,
                        "wind_speed_ms": hour.wind_speed_ms,
                        "heat_to_air_w_m2": hour.heat_to_air_w_m2,
                        "levels": hour.winds_aloft.len(),
                    });
                    (hour.time.to_rfc3339(), values)
                })
                .collect();
            json!({
                "model": series.model,
                "run": series.run,
                "available_at": series.available_at,
                "point": series.point,
                "cell": series.cell,
                "cell_elevation_m": series.cell_elevation_m,
                "hours": series.hours.len(),
                "first_hours": hours,
            })
        })
        .collect();
    json!({ "series": series, "report": output.report })
}

#[test]
fn a_recorded_run_for_four_places_matches_the_golden_file() {
    let icon_d2 = recorded_run("icon-d2");
    let icon_eu = recorded_run("icon-eu");

    for output in [&icon_d2, &icon_eu] {
        // Two requests for four places: the run, then the forecast.
        assert_eq!(output.report.requests, 2);
        assert!(output.report.is_healthy(), "{:?}", output.report);
        assert!(output.report.skipped.is_empty(), "{:?}", output.report);
        assert!(output.report.rejected.is_empty(), "{:?}", output.report);
        assert_eq!(output.series.len(), 4);
        // One series for each place, in the order they were asked for.
        let points: Vec<GeoPoint> = output.series.iter().map(|series| series.point).collect();
        assert_eq!(points, fixtures::grid_places());
    }

    // The instance held the runs of 06 UTC of both models. ICON-D2's
    // reaches 48 hours. ICON-EU's is a main run and reaches 78, to noon of
    // the third day after: the fourth day the model is asked for.
    for series in &icon_d2.series {
        assert_eq!(series.model.as_str(), "icon-d2");
        assert_eq!(series.run, at(21, 6));
        assert_eq!(series.hours.len(), 48);
        assert_eq!(series.hours[0].time, at(21, 7));
        assert_eq!(series.hours[47].time, at(23, 6));
    }
    for series in &icon_eu.series {
        assert_eq!(series.model.as_str(), "icon-eu");
        assert_eq!(series.run, at(21, 6));
        assert_eq!(series.hours.len(), 78);
        assert_eq!(series.hours[0].time, at(21, 7));
        assert_eq!(series.hours[77].time, at(24, 12));
    }

    // Each place has the cell nearest it and the height of the model's
    // ground there, not one cell for all.
    let grounds: Vec<Option<f64>> = icon_d2
        .series
        .iter()
        .map(|series| series.cell_elevation_m)
        .collect();
    assert_eq!(
        grounds,
        [Some(1118.0), Some(1046.0), Some(1069.0), Some(947.0)]
    );
    for series in &icon_d2.series {
        assert!(series.cell.distance_m(series.point) < 2000.0);
    }

    assert_golden(
        &golden("places_run.json"),
        &json!({ "icon-d2": summary(&icon_d2), "icon-eu": summary(&icon_eu) }),
    );
}

#[test]
fn the_request_names_every_place_and_a_height_for_each() {
    let url = series_url("icon-d2");
    assert!(url.contains(
        "?latitude=47.6000,47.6000,47.7000,47.7000&longitude=11.8000,11.9000,11.8000,11.9000&"
    ));
    // The instance refuses one `nan` for four places: "Parameter
    // 'elevation' must have the same number of elements as coordinates".
    // The choice of cell is one for all; it refuses a list.
    assert!(url.ends_with("&elevation=nan,nan,nan,nan&cell_selection=nearest"));
    // Each model is asked for the days its runs reach: the recorded
    // answers hold 72 hours a place of ICON-D2 and 96 of ICON-EU.
    assert!(url.contains("&forecast_days=3&"));
    assert!(series_url("icon-eu").contains("&forecast_days=4&"));
    for (of, hours) in [("icon-d2", 72), ("icon-eu", 96)] {
        let answer: Value = serde_json::from_slice(&recorded(&series_url(of))).unwrap();
        for place in answer.as_array().unwrap() {
            assert_eq!(place["hourly"]["time"].as_array().unwrap().len(), hours);
        }
    }
}

#[test]
fn a_model_it_does_not_know_and_too_many_places_ask_nothing() {
    let source = source();
    let points = fixtures::grid_places();
    assert_eq!(source.places_per_request(), PLACES_PER_REQUEST);
    assert!(
        source
            .plan_places(&model("gfs"), &points, &ctx())
            .is_empty()
    );
    assert!(
        source
            .plan_places(&model("icon-d2"), &[], &ctx())
            .is_empty()
    );
    let many: Vec<GeoPoint> = (0..=PLACES_PER_REQUEST)
        .map(|n| GeoPoint::new(47.0, 10.0 + n as f64 * 0.01).unwrap())
        .collect();
    assert!(
        source
            .plan_places(&model("icon-d2"), &many, &ctx())
            .is_empty()
    );

    // The pipeline asks nothing for them either.
    let fetch = FixtureFetch::new().with_all(fixtures::recorded_places());
    let output = run_with(&fetch, "icon-d2", &many);
    assert_eq!(output.report.requests, 0);
    assert!(output.series.is_empty());
}

#[test]
fn a_place_the_answer_cannot_be_read_for_costs_that_place_alone() {
    // The third place's answer has lost the cell it is for.
    let output = changed_run("icon-d2", |answer| {
        answer[2].as_object_mut().unwrap().remove("latitude");
    });
    assert_eq!(output.series.len(), 3);
    assert!(
        output
            .series
            .iter()
            .all(|series| series.point != fixtures::grid_places()[2])
    );
    assert_eq!(output.report.skipped.len(), 1);
    assert_eq!(output.report.skipped[0].what, "47.7000 N 11.8000 E");
    assert!(output.report.failures.is_empty());
}

#[test]
fn an_answer_that_does_not_fit_the_places_delivers_nothing() {
    // One answer too few.
    let output = changed_run("icon-d2", |answer| {
        answer.as_array_mut().unwrap().pop();
    });
    assert!(output.series.is_empty());
    assert!(
        output.report.failures[0]
            .error
            .contains("3 answers for 4 places")
    );

    // Answers in another order than the places.
    let output = changed_run("icon-d2", |answer| {
        answer.as_array_mut().unwrap().swap(1, 2);
    });
    assert!(output.series.is_empty());
    assert!(
        output.report.failures[0]
            .error
            .contains("says it is for place 2")
    );

    // A refusal for the whole request, as the instance writes it.
    let output = changed_run("icon-eu", |answer| {
        *answer = json!({"reason": "Parameter 'elevation' must have the same number of elements as coordinates", "error": true});
    });
    assert!(output.series.is_empty());
    assert!(
        output.report.failures[0]
            .error
            .contains("the instance refused")
    );
}
