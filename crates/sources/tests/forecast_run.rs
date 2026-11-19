//! Runs the forecast pipeline over a stand-in adapter.
//!
//! The adapter's "provider" is asked in two steps, as a real one is: first
//! which run of each model it holds, then what the model forecasts for the
//! place. The tests are about what the pipeline does with that: when a
//! series can be dated and when it cannot, what is validated, and when a
//! run counts as healthy.

use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use venti_domain::{BoundingBox, ForecastSeries, GeoPoint, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, block_on};
use venti_sources::{
    ForecastRunOutput, ForecastSource, Kind, Manifest, ParsedForecast, PlanCtx, Registry, Request,
    RunOptions, SourceError, run_forecast,
};

const MODELS: [&str; 2] = ["fine", "coarse"];

fn run_url(model: &str) -> String {
    format!("https://weather.example/{model}/run.json")
}

fn series_url(model: &str, point: GeoPoint) -> String {
    format!(
        "https://weather.example/{model}/series.json?lat={}&lon={}",
        point.lat(),
        point.lon()
    )
}

struct Weather(Manifest);

impl Weather {
    fn new() -> Self {
        let text = r#"
            id = "weather"
            provider = "Example Weather Service"
            kinds = ["forecasts"]
            countries = ["DE", "AT"]
            cadence_seconds = 600
            auth = "none"

            [rate_limit]
            per_hour = 3600

            [licence]
            name = "CC BY 4.0"
            commercial = true
            redistribution = true
            share_alike = false
            attribution = "Example Weather Service"
            link = "https://weather.example"
            verified_on = "2026-11-19"
            verified_from = ["https://weather.example/licence"]
        "#;
        Self(Manifest::from_toml(text).unwrap())
    }
}

impl ForecastSource for Weather {
    fn manifest(&self) -> &Manifest {
        &self.0
    }

    /// First the run of each model.
    fn plan(&self, _point: GeoPoint, _ctx: &PlanCtx) -> Vec<Request> {
        MODELS
            .iter()
            .map(|model| Request::new(format!("run:{model}"), run_url(model)))
            .collect()
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        point: GeoPoint,
        _ctx: &PlanCtx,
    ) -> Result<ParsedForecast, SourceError> {
        let malformed = |e: serde_json::Error| SourceError::Malformed(e.to_string());
        let mut parsed = ParsedForecast::default();
        let parts: Vec<&str> = request.tag.split(':').collect();
        match parts[..] {
            // The run is known: now the series, which is told its run by the tag.
            ["run", model] => {
                let answer: serde_json::Value = serde_json::from_slice(body).map_err(malformed)?;
                let run = answer["run"]
                    .as_i64()
                    .ok_or_else(|| SourceError::Malformed("no run".to_owned()))?;
                parsed.follow_up.push(Request::new(
                    format!("series:{model}:{run}"),
                    series_url(model, point),
                ));
            }
            ["series", model, run] => {
                let mut series: serde_json::Value =
                    serde_json::from_slice(body).map_err(malformed)?;
                let run = Utc.timestamp_opt(run.parse().unwrap(), 0).unwrap();
                series["source"] = json!("weather");
                series["model"] = json!(model);
                series["run"] = json!(run);
                series["point"] = json!(point);
                parsed
                    .series
                    .push(serde_json::from_value(series).map_err(malformed)?);
            }
            _ => return Err(SourceError::UnknownRequest(request.tag.clone())),
        }
        Ok(parsed)
    }
}

fn now() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 19, 18, 0, 0).unwrap()
}

fn tegernsee() -> GeoPoint {
    GeoPoint::new(47.7, 11.8).unwrap()
}

fn ctx() -> PlanCtx {
    PlanCtx {
        now: now(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

fn run_answer(hours_ago: i64) -> Vec<u8> {
    let run = (now() - Duration::hours(hours_ago)).timestamp();
    serde_json::to_vec(&json!({"run": run})).unwrap()
}

/// A series of `hours` hours from now on, with a wind that grows by the hour.
fn series_answer(hours: i64) -> Vec<u8> {
    let hours: Vec<serde_json::Value> = (0..hours)
        .map(|hour| {
            json!({
                "time": now() + Duration::hours(hour),
                "wind_speed_ms": 2.0 + hour as f64,
                "wind_direction_deg": 250.0,
                "winds_aloft": [
                    {"pressure_hpa": 950.0, "altitude_m": 520.0, "wind_speed_ms": 1.0},
                    {"pressure_hpa": 850.0, "altitude_m": 1490.0, "wind_speed_ms": 9.0},
                ],
            })
        })
        .collect();
    serde_json::to_vec(&json!({
        "cell": {"lat": 47.71, "lon": 11.79},
        "cell_elevation_m": 840.0,
        "hours": hours,
    }))
    .unwrap()
}

fn whole() -> FixtureFetch {
    FixtureFetch::new()
        .with(run_url("fine"), run_answer(3))
        .with(run_url("coarse"), run_answer(6))
        .with(series_url("fine", tegernsee()), series_answer(48))
        .with(series_url("coarse", tegernsee()), series_answer(72))
}

fn run_with(fetch: &FixtureFetch, options: &RunOptions) -> ForecastRunOutput {
    let archive = MemoryArchive::default();
    let output = block_on(run_forecast(
        &Weather::new(),
        tegernsee(),
        fetch,
        &archive,
        &ctx(),
        options,
    ));
    // Whatever arrived was archived, usable or not.
    assert!(archive.stored().len() <= output.report.requests);
    output
}

fn run(fetch: &FixtureFetch) -> ForecastRunOutput {
    run_with(fetch, &RunOptions::default())
}

fn models(output: &ForecastRunOutput) -> Vec<&str> {
    output.series.iter().map(|s| s.model.as_str()).collect()
}

#[test]
fn a_run_delivers_one_series_per_model_each_with_its_run() {
    let output = run(&whole());

    // Sorted by model.
    assert_eq!(models(&output), ["coarse", "fine"]);
    let fine: &ForecastSeries = &output.series[1];
    assert_eq!(fine.run, now() - Duration::hours(3));
    assert_eq!(output.series[0].run, now() - Duration::hours(6));
    assert_eq!(fine.point, tegernsee());
    assert_eq!(fine.hours.len(), 48);
    assert_eq!(fine.hours[0].time, now());
    // The surface at 950 hPa lies under the model's ground of 840 m.
    assert_eq!(fine.hours[0].winds_aloft.len(), 1);
    assert_eq!(fine.hours[0].winds_aloft[0].pressure_hpa, Some(850.0));

    let report = &output.report;
    assert_eq!(report.requests, 4);
    assert_eq!(report.series, 2);
    assert_eq!(report.hours, 48 + 72);
    assert_eq!(report.newest_run, Some(now() - Duration::hours(3)));
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert!(report.is_healthy());
}

#[test]
fn a_model_whose_run_cannot_be_read_is_not_delivered() {
    // The service does not say which run its fine model is at.
    let mut fetch = FixtureFetch::new()
        .with(run_url("coarse"), run_answer(6))
        .with(series_url("fine", tegernsee()), series_answer(48))
        .with(series_url("coarse", tegernsee()), series_answer(72));

    let output = run(&fetch);

    // Its series was never asked for: nothing could have dated it.
    assert_eq!(models(&output), ["coarse"]);
    assert_eq!(output.report.requests, 3);
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "run:fine");
    // There is an answer, and the run is not healthy.
    assert!(!output.report.is_healthy());

    // Nothing at all is no answer.
    fetch = FixtureFetch::new();
    let output = run(&fetch);
    assert!(output.series.is_empty());
    assert_eq!(output.report.failures.len(), 2);
    assert!(!output.report.is_healthy());
}

#[test]
fn a_model_without_values_for_the_place_says_nothing() {
    // The fine model does not reach the place: every hour is empty.
    let hollow = serde_json::to_vec(&json!({
        "cell": {"lat": 47.71, "lon": 11.79},
        "hours": [{"time": now()}, {"time": now() + Duration::hours(1)}],
    }))
    .unwrap();
    let fetch = whole().with(series_url("fine", tegernsee()), hollow);

    let output = run(&fetch);

    assert_eq!(models(&output), ["coarse"]);
    assert!(output.report.rejected.is_empty());
    // Every request worked and there is a series: healthy.
    assert!(output.report.is_healthy());
}

#[test]
fn a_series_from_far_away_is_refused_and_faulty_values_are_removed() {
    let far = serde_json::to_vec(&json!({
        "cell": {"lat": 52.5, "lon": 13.4},
        "hours": [{"time": now(), "wind_speed_ms": 3.0}],
    }))
    .unwrap();
    let faulty = serde_json::to_vec(&json!({
        "cell": {"lat": 47.71, "lon": 11.79},
        "hours": [
            {"time": now(), "wind_speed_ms": 3.0, "temperature_c": 900.0},
            {"time": now() + Duration::days(400), "wind_speed_ms": 3.0},
        ],
    }))
    .unwrap();
    let fetch = whole()
        .with(series_url("fine", tegernsee()), far)
        .with(series_url("coarse", tegernsee()), faulty);

    let output = run(&fetch);

    assert_eq!(models(&output), ["coarse"]);
    assert_eq!(output.series[0].hours.len(), 1);
    assert_eq!(output.series[0].hours[0].temperature_c, None);
    assert_eq!(output.series[0].hours[0].wind_speed_ms, Some(3.0));
    let rejected: Vec<(&str, &str)> = output
        .report
        .rejected
        .iter()
        .map(|r| (r.model.as_str(), r.field.as_str()))
        .collect();
    assert_eq!(
        rejected,
        [
            ("coarse", "temperature_c"),
            ("coarse", "hour"),
            ("fine", "series")
        ]
    );
    assert!(output.report.rejected[2].reason.contains("km"));
}

#[test]
fn a_run_stays_within_its_request_budget() {
    // A manifest without an hourly limit leaves the budget to the caller.
    let mut source = Weather::new();
    source.0.rate_limit.per_hour = None;
    let options = RunOptions {
        max_requests: 3,
        ..RunOptions::default()
    };
    let archive = MemoryArchive::default();

    let output = block_on(run_forecast(
        &source,
        tegernsee(),
        &whole(),
        &archive,
        &ctx(),
        &options,
    ));

    // Two runs were asked for and one series; the second series was not.
    assert_eq!(output.report.requests, 3);
    assert!(output.report.budget_exhausted);
    assert_eq!(output.series.len(), 1);
    assert!(!output.report.is_healthy());
}

#[test]
fn a_forecast_source_registers_for_its_kind() {
    let mut registry = Registry::new();
    registry
        .register_forecasts(Box::new(Weather::new()))
        .unwrap();
    let id = Weather::new().0.id;
    assert!(registry.forecasts(&id).is_some());
    assert!(registry.sites(&id).is_none());
    assert!(registry.manifest(&id, Kind::Forecasts).is_some());
    assert!(registry.manifest(&id, Kind::Observations).is_none());
}
