//! Running a source of point forecasts.

use std::collections::BTreeMap;

use chrono::Duration;
use serde::{Deserialize, Serialize};
use venti_domain::{
    ForecastHour, ForecastSeries, GeoPoint, ModelId, SourceId, Timestamp, WindAloft,
};

use crate::run::fetch_all;
use crate::{
    Archive, Failure, Fetch, Fetched, ForecastSource, PlanCtx, Request, RunOptions, Skipped,
};

/// Plausible limits for each value at the ground. A value outside its
/// limits is a fault of the source or of the adapter, not weather.
const LIMITS: [(&str, f64, f64); 15] = [
    ("wind_speed_ms", 0.0, 120.0),
    ("wind_direction_deg", 0.0, 360.0),
    ("wind_gust_ms", 0.0, 120.0),
    ("temperature_c", -90.0, 60.0),
    ("dew_point_c", -100.0, 60.0),
    ("cloud_cover_pct", 0.0, 100.0),
    ("cloud_cover_low_pct", 0.0, 100.0),
    ("cloud_cover_mid_pct", 0.0, 100.0),
    ("cloud_cover_high_pct", 0.0, 100.0),
    ("precipitation_mm", 0.0, 500.0),
    ("cape_j_kg", 0.0, 15_000.0),
    ("freezing_level_m", -500.0, 12_000.0),
    ("updraft_ms", 0.0, 100.0),
    ("convective_cloud_base_m", -500.0, 20_000.0),
    ("convective_cloud_top_m", -500.0, 20_000.0),
];

/// The same for the values at a level above the ground. The wind in a jet
/// stream passes 100 m/s.
const ALOFT_LIMITS: [(&str, f64, f64); 4] = [
    ("altitude_m", -500.0, 30_000.0),
    ("wind_speed_ms", 0.0, 150.0),
    ("wind_direction_deg", 0.0, 360.0),
    ("temperature_c", -110.0, 60.0),
];

/// The heights above the ground a level may have, in metres.
const ABOVE_GROUND_LIMITS: (f64, f64) = (0.0, 5000.0);

/// The pressures a level may have, in hPa.
const PRESSURE_LIMITS: (f64, f64) = (1.0, 1100.0);

/// Lowest and highest heights of a model's ground accepted, in metres.
const ELEVATION_LIMITS: (f64, f64) = (-500.0, 9000.0);

/// How far the cell a series is for may lie from the place that was asked
/// for, in metres. The widest cells of the models in use measure 13 km.
const MAX_CELL_DISTANCE_M: f64 = 30_000.0;

/// How far ahead of the run time a model run may be stamped before it is
/// treated as a clock fault.
const MAX_FUTURE: Duration = Duration::minutes(15);

/// The window around the run time an hour must lie in: a forecast reaches
/// back to the start of the day before and ahead by some days, never by
/// months.
const HOURS_BEFORE: Duration = Duration::days(3);
const HOURS_AHEAD: Duration = Duration::days(20);

/// Something in a forecast that the pipeline refused to pass on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastRejection {
    /// The model the series is of.
    pub model: String,
    /// The hour the value belongs to, if it belongs to one.
    pub time: Option<Timestamp>,
    /// What was refused: a field, or `series`, `hour` or `level` for the
    /// whole of one.
    pub field: String,
    /// The refused value, where one number says it.
    pub value: Option<f64>,
    /// Why.
    pub reason: String,
}

/// What happened in a forecast run. This is the source's health record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastRunReport {
    /// The source that ran.
    pub source: SourceId,
    /// Requests made.
    pub requests: usize,
    /// Requests that failed, or whose response could not be parsed or archived.
    pub failures: Vec<Failure>,
    /// Parts of responses the adapter could not convert.
    pub skipped: Vec<Skipped>,
    /// Series, hours, levels and values removed by validation.
    pub rejected: Vec<ForecastRejection>,
    /// Whether the run stopped early because it used up its request budget.
    pub budget_exhausted: bool,
    /// Series delivered: one per model that has values for the place.
    pub series: usize,
    /// Hours delivered, over all series.
    pub hours: usize,
    /// The newest model run among the series delivered.
    pub newest_run: Option<Timestamp>,
}

impl ForecastRunReport {
    /// Whether the run did its job: it delivered at least one series and
    /// every request worked.
    ///
    /// Every request, because a forecast is a few requests and each
    /// matters: without the one that says which run a model is at, that
    /// model's series cannot be dated and is not delivered. An answer that
    /// lacks a model is still an answer; it is for the caller to decide
    /// whether to hand it on, and not to keep it for long.
    pub fn is_healthy(&self) -> bool {
        self.series > 0 && self.failures.is_empty() && !self.budget_exhausted
    }
}

/// The canonical records a forecast run produced, and its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastRunOutput {
    /// One series per model, sorted by model.
    pub series: Vec<ForecastSeries>,
    /// What happened.
    pub report: ForecastRunReport,
}

/// Fetches the forecast of one source for one place: fetch, archive, parse,
/// validate.
///
/// A failing request or an unparseable response is recorded in the report
/// and does not stop the run. The function reads no clock; `ctx.now` is the
/// time of the run. `ctx.area` and `options.since` play no part. Requests
/// are budgeted and archived as for every other kind of source; how often a
/// forecast may be asked for is for the caller to bound.
///
/// What validation does:
///
/// - A series that names another source or another place than was asked
///   for, whose cell lies far from the place, or whose run is stamped in
///   the future is removed whole and reported. So is a second series of a
///   model already seen.
/// - An hour far from the time of the run, or a second hour for a time
///   already seen, is removed and reported.
/// - A value outside what the weather can be is removed and reported, and
///   the rest of its hour is kept.
/// - A level above the ground that is given both as a height and as a
///   pressure, or as neither, is removed and reported.
/// - A pressure surface that the model puts below its own ground at that
///   hour is dropped: the air a model writes there does not exist. This is
///   not a fault and is not reported.
/// - An hour with nothing left is dropped, and so is a series without
///   hours: a model that does not reach a place says nothing about it.
pub async fn run_forecast(
    source: &dyn ForecastSource,
    point: GeoPoint,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
) -> ForecastRunOutput {
    let manifest = source.manifest();
    let mut skipped = Vec::new();
    let mut found: Vec<ForecastSeries> = Vec::new();

    let take = |request: &Request, response: &Fetched| {
        let parsed = source.parse(request, &response.body, point, ctx)?;
        found.extend(parsed.series);
        skipped.extend(parsed.skipped);
        Ok(parsed.follow_up)
    };
    let fetched = fetch_all(
        manifest,
        source.plan(point, ctx),
        fetch,
        archive,
        ctx,
        options,
        take,
    )
    .await;

    let mut rejected = Vec::new();
    let mut kept: BTreeMap<ModelId, ForecastSeries> = BTreeMap::new();
    for mut series in found {
        if let Some(reason) = unusable(&series, &manifest.id, point, ctx.now) {
            rejected.push(whole(&series, "series", reason));
            continue;
        }
        if kept.contains_key(&series.model) {
            let reason = "a second series of a model already delivered".to_owned();
            rejected.push(whole(&series, "series", reason));
            continue;
        }
        rejected.extend(check_series(&mut series, ctx.now));
        if !series.hours.is_empty() {
            kept.insert(series.model.clone(), series);
        }
    }

    let series: Vec<ForecastSeries> = kept.into_values().collect();
    let report = ForecastRunReport {
        source: manifest.id.clone(),
        requests: fetched.requests,
        failures: fetched.failures,
        skipped,
        rejected,
        budget_exhausted: fetched.budget_exhausted,
        series: series.len(),
        hours: series.iter().map(|series| series.hours.len()).sum(),
        newest_run: series.iter().map(|series| series.run).max(),
    };
    ForecastRunOutput { series, report }
}

/// A rejection of something larger than one value.
fn whole(series: &ForecastSeries, what: &str, reason: String) -> ForecastRejection {
    ForecastRejection {
        model: series.model.to_string(),
        time: None,
        field: what.to_owned(),
        value: None,
        reason,
    }
}

/// Why a series cannot be used at all, if it cannot.
fn unusable(
    series: &ForecastSeries,
    source: &SourceId,
    point: GeoPoint,
    now: Timestamp,
) -> Option<String> {
    if &series.source != source {
        return Some(format!("the series names the source {}", series.source));
    }
    if series.point != point {
        return Some("the series is for another place than was asked for".to_owned());
    }
    let distance = series.cell.distance_m(point);
    if distance > MAX_CELL_DISTANCE_M {
        return Some(format!(
            "the model's cell lies {:.0} km from the place asked for",
            distance / 1000.0
        ));
    }
    if series.run > now + MAX_FUTURE {
        return Some(format!("the run is stamped in the future: {}", series.run));
    }
    None
}

fn within(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}

/// Removes from a series what cannot be right and puts its hours in order.
/// Returns what was removed.
fn check_series(series: &mut ForecastSeries, now: Timestamp) -> Vec<ForecastRejection> {
    let mut rejections = Vec::new();
    let model = series.model.to_string();
    let reject = |time: Option<Timestamp>, field: &str, value: Option<f64>, reason: String| {
        ForecastRejection {
            model: model.clone(),
            time,
            field: field.to_owned(),
            value,
            reason,
        }
    };

    if let Some(value) = series.cell_elevation_m {
        let (low, high) = ELEVATION_LIMITS;
        if !within(value, low, high) {
            let reason = format!("outside {low}..={high}");
            rejections.push(reject(None, "cell_elevation_m", Some(value), reason));
            series.cell_elevation_m = None;
        }
    }
    if series
        .available_at
        .is_some_and(|available| available < series.run || available > now + MAX_FUTURE)
    {
        // A run cannot be ready before it began, nor after now.
        series.available_at = None;
    }

    let ground = series.cell_elevation_m;
    let mut hours: BTreeMap<Timestamp, ForecastHour> = BTreeMap::new();
    for mut hour in std::mem::take(&mut series.hours) {
        let time = hour.time;
        if time < now - HOURS_BEFORE || time > now + HOURS_AHEAD {
            let reason = "far from the time of the run".to_owned();
            rejections.push(reject(Some(time), "hour", None, reason));
            continue;
        }
        if hours.contains_key(&time) {
            let reason = "a second hour for a time already delivered".to_owned();
            rejections.push(reject(Some(time), "hour", None, reason));
            continue;
        }

        for (field, slot) in hour.fields_mut() {
            let Some(value) = *slot else { continue };
            let (_, low, high) = LIMITS
                .iter()
                .find(|(name, _, _)| *name == field)
                .expect("every forecast field has limits");
            if !within(value, *low, *high) {
                *slot = None;
                let reason = format!("outside {low}..={high}");
                rejections.push(reject(Some(time), field, Some(value), reason));
            }
        }

        let mut levels = Vec::with_capacity(hour.winds_aloft.len());
        for mut level in std::mem::take(&mut hour.winds_aloft) {
            match check_level(&mut level, ground) {
                Level::Kept(removed) => {
                    for (field, value, reason) in removed {
                        rejections.push(reject(Some(time), field, Some(value), reason));
                    }
                    if !level.is_empty() {
                        levels.push(level);
                    }
                }
                Level::BelowGround => {}
                Level::Unusable(reason) => {
                    rejections.push(reject(Some(time), "level", None, reason));
                }
            }
        }
        hour.winds_aloft = levels;

        if !hour.is_empty() {
            hours.insert(time, hour);
        }
    }
    series.hours = hours.into_values().collect();
    rejections
}

/// What became of one level.
enum Level {
    /// It stays, without the values listed: field, value and reason.
    Kept(Vec<(&'static str, f64, String)>),
    /// It is a pressure surface under the model's ground.
    BelowGround,
    /// It does not say where it is.
    Unusable(String),
}

fn check_level(level: &mut WindAloft, ground: Option<f64>) -> Level {
    match (level.above_ground_m, level.pressure_hpa) {
        (Some(height), None) => {
            let (low, high) = ABOVE_GROUND_LIMITS;
            if !within(height, low, high) {
                return Level::Unusable(format!("a height above ground of {height} m"));
            }
            // A height above the ground needs no height above the sea.
            level.altitude_m = None;
        }
        (None, Some(pressure)) => {
            let (low, high) = PRESSURE_LIMITS;
            if !within(pressure, low, high) {
                return Level::Unusable(format!("a pressure of {pressure} hPa"));
            }
        }
        (Some(_), Some(_)) => {
            return Level::Unusable("given both as a height and as a pressure".to_owned());
        }
        (None, None) => {
            return Level::Unusable("given neither as a height nor as a pressure".to_owned());
        }
    }

    let mut removed = Vec::new();
    for (field, slot) in level.fields_mut() {
        let Some(value) = *slot else { continue };
        let (_, low, high) = ALOFT_LIMITS
            .iter()
            .find(|(name, _, _)| *name == field)
            .expect("every level field has limits");
        if !within(value, *low, *high) {
            *slot = None;
            removed.push((field, value, format!("outside {low}..={high}")));
        }
    }
    if let (Some(altitude), Some(ground)) = (level.altitude_m, ground)
        && altitude < ground
    {
        return Level::BelowGround;
    }
    Level::Kept(removed)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn now() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 19, 18, 0, 0).unwrap()
    }

    fn series() -> ForecastSeries {
        let mut hour = ForecastHour::empty(now());
        hour.wind_speed_ms = Some(3.2);
        hour.wind_direction_deg = Some(250.0);
        ForecastSeries {
            source: SourceId::new("example").unwrap(),
            model: ModelId::new("icon-d2").unwrap(),
            run: now() - Duration::hours(3),
            available_at: Some(now() - Duration::hours(2)),
            point: GeoPoint::new(47.7, 11.8).unwrap(),
            cell: GeoPoint::new(47.71, 11.79).unwrap(),
            cell_elevation_m: Some(1000.0),
            hours: vec![hour],
        }
    }

    fn level(pressure: f64, altitude: f64, speed: f64) -> WindAloft {
        WindAloft {
            altitude_m: Some(altitude),
            wind_speed_ms: Some(speed),
            wind_direction_deg: Some(270.0),
            ..WindAloft::at_pressure(pressure)
        }
    }

    #[test]
    fn a_sound_series_passes_unchanged() {
        let mut checked = series();
        assert!(check_series(&mut checked, now()).is_empty());
        assert_eq!(checked, series());
        let source = SourceId::new("example").unwrap();
        assert_eq!(unusable(&checked, &source, checked.point, now()), None);
    }

    #[test]
    fn a_series_from_elsewhere_or_from_the_future_is_unusable() {
        let source = SourceId::new("example").unwrap();
        let asked = series().point;

        let other = SourceId::new("other").unwrap();
        assert!(unusable(&series(), &other, asked, now()).is_some());

        let elsewhere = GeoPoint::new(47.72, 11.8).unwrap();
        assert!(unusable(&series(), &source, elsewhere, now()).is_some());

        let far = ForecastSeries {
            cell: GeoPoint::new(48.2, 11.8).unwrap(),
            ..series()
        };
        let reason = unusable(&far, &source, asked, now()).unwrap();
        assert!(reason.contains("56 km"), "{reason}");

        let early = ForecastSeries {
            run: now() + Duration::hours(1),
            ..series()
        };
        assert!(unusable(&early, &source, asked, now()).is_some());
    }

    #[test]
    fn an_impossible_value_costs_itself_and_not_its_hour() {
        let mut checked = series();
        checked.hours[0].temperature_c = Some(4.5);
        checked.hours[0].cloud_cover_pct = Some(140.0);
        checked.hours[0].wind_gust_ms = Some(f64::NAN);
        checked.cell_elevation_m = Some(12_000.0);

        let rejections = check_series(&mut checked, now());

        let fields: Vec<&str> = rejections.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(
            fields,
            ["cell_elevation_m", "wind_gust_ms", "cloud_cover_pct"]
        );
        assert_eq!(rejections[2].time, Some(now()));
        assert_eq!(rejections[2].value, Some(140.0));
        let hour = &checked.hours[0];
        assert_eq!(hour.cloud_cover_pct, None);
        assert_eq!(hour.wind_gust_ms, None);
        assert_eq!(hour.temperature_c, Some(4.5));
        assert_eq!(hour.wind_speed_ms, Some(3.2));
        assert_eq!(checked.cell_elevation_m, None);
    }

    #[test]
    fn hours_come_out_in_order_once_and_near_the_run() {
        let at = |hours: i64| {
            let mut hour = ForecastHour::empty(now() + Duration::hours(hours));
            hour.temperature_c = Some(1.0);
            hour
        };
        let mut checked = ForecastSeries {
            hours: vec![at(2), at(0), at(2), at(24 * 400), at(-24 * 30), at(1)],
            ..series()
        };

        let rejections = check_series(&mut checked, now());

        let times: Vec<Timestamp> = checked.hours.iter().map(|hour| hour.time).collect();
        assert_eq!(
            times,
            [
                now(),
                now() + Duration::hours(1),
                now() + Duration::hours(2)
            ]
        );
        assert_eq!(rejections.len(), 3);
        assert!(rejections.iter().all(|r| r.field == "hour"));

        // An hour without a value is no hour, and nothing to report.
        let mut hollow = ForecastSeries {
            hours: vec![ForecastHour::empty(now())],
            ..series()
        };
        assert!(check_series(&mut hollow, now()).is_empty());
        assert!(hollow.hours.is_empty());
    }

    #[test]
    fn levels_must_say_where_they_are_and_lie_above_the_ground() {
        let mut checked = series();
        let mut low = WindAloft::above_ground(80.0);
        low.wind_speed_ms = Some(5.0);
        low.altitude_m = Some(1080.0);
        let mut both = level(850.0, 1500.0, 9.0);
        both.above_ground_m = Some(80.0);
        let mut neither = level(850.0, 1500.0, 9.0);
        neither.pressure_hpa = None;
        let mut fast = level(700.0, 3010.0, 400.0);
        fast.temperature_c = Some(-4.0);
        checked.hours[0].winds_aloft = vec![
            low,
            // Under the model's ground of 1000 m: no air there.
            level(950.0, 540.0, 2.0),
            level(850.0, 1490.0, 9.0),
            both,
            neither,
            fast,
            level(0.0, 3000.0, 9.0),
            // Nothing but a height: no level.
            WindAloft {
                altitude_m: Some(4200.0),
                ..WindAloft::at_pressure(600.0)
            },
        ];

        let rejections = check_series(&mut checked, now());

        let levels = &checked.hours[0].winds_aloft;
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].above_ground_m, Some(80.0));
        assert_eq!(levels[0].altitude_m, None);
        assert_eq!(levels[1].pressure_hpa, Some(850.0));
        assert_eq!(levels[1].altitude_m, Some(1490.0));
        // The level at 700 hPa keeps its direction and temperature.
        assert_eq!(levels[2].wind_speed_ms, None);
        assert_eq!(levels[2].temperature_c, Some(-4.0));

        let fields: Vec<&str> = rejections.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(fields, ["level", "level", "wind_speed_ms", "level"]);

        // Without the height of the ground nothing can be said to lie below it.
        let mut unknown_ground = ForecastSeries {
            cell_elevation_m: None,
            ..series()
        };
        unknown_ground.hours[0].winds_aloft = vec![level(950.0, 540.0, 2.0)];
        check_series(&mut unknown_ground, now());
        assert_eq!(unknown_ground.hours[0].winds_aloft.len(), 1);
    }

    #[test]
    fn a_run_that_was_ready_before_it_began_loses_that_time() {
        let mut checked = ForecastSeries {
            available_at: Some(now() - Duration::hours(5)),
            ..series()
        };
        check_series(&mut checked, now());
        assert_eq!(checked.available_at, None);
    }
}
