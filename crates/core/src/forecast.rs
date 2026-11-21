//! The forecast for a place, from the Venti server.
//!
//! As with the wind, the core never talks to the network: one function
//! says what to ask the server for, another reads the answer into what a
//! screen shows. The shell does the request in between.
//!
//! The answer is what weather models compute, and the views keep it that
//! way: one model beside the next, never blended; every model with its
//! run, the grid cell it speaks for and how high the model thinks the
//! ground is there, because in the mountains that can be hundreds of
//! metres from the truth; magnitudes, and no word on whether a day can be
//! flown. With them stands what the app works out of a model's heat and
//! temperatures about thermals: an estimate, and named as one.

use chrono::Duration;
use venti_api_types::v1::{ForecastRequest, ForecastResponse};
use venti_domain::{ForecastHour, ForecastSeries, GeoPoint, WindAloft};
use venti_flight::convection::convection;
use venti_flight::sun;

use crate::server::answer;
use crate::wind::{credit, wind_band, wind_speeds_label};
use crate::{Coordinate, CoreError, Credit, WindBand};

/// The models the app knows, the finest first, each with the name pilots
/// know it by and the width of its grid cells. A model the app does not
/// know comes after these, under its identifier.
const MODELS: [(&str, &str, &str); 6] = [
    ("icon-d2", "ICON-D2", "2 km cells"),
    ("icon-ch1", "ICON-CH1", "1 km cells"),
    ("icon-ch2", "ICON-CH2", "2 km cells"),
    ("icon-eu", "ICON-EU", "7 km cells"),
    ("icon-global", "ICON", "13 km cells"),
    ("ecmwf-ifs", "ECMWF IFS", "25 km cells"),
];

/// A wind at one level and one hour.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ForecastWind {
    /// Its speed, metres per second.
    pub speed_ms: f64,
    /// The direction it comes from, degrees clockwise from north, if the
    /// model gives one.
    pub from_deg: Option<f64>,
    /// How strong it is, in the bands the wind map uses.
    pub band: WindBand,
    /// Its speed in whole km/h, with the gusts where they are known and
    /// differ: "12", "12/25".
    pub label: String,
}

/// One level the wind higher up is given at: a row of a table.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ForecastLevel {
    /// The level in words: "80 m above ground", "850 hPa".
    pub name: String,
    /// For a pressure surface, about how high it lies above sea level over
    /// the days shown, metres, rounded to a hundred. A surface of equal
    /// pressure rises and sinks with the weather.
    pub about_altitude_m: Option<f64>,
}

/// What a model forecasts for one hour.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ForecastHourView {
    /// The moment the values are for, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// Whether the sun is up at the place at that moment.
    pub is_daylight: bool,
    /// The wind 10 m above the model's ground, with its gusts in the label.
    pub wind: Option<ForecastWind>,
    /// The wind at each of the model's levels, in the order of
    /// [`ModelForecast::levels`]. `None` where the model has no value.
    pub aloft: Vec<Option<ForecastWind>>,
    /// Air temperature 2 m above the model's ground, °C.
    pub temperature_c: Option<f64>,
    /// Cloud cover, percent of the sky: all levels, then low, middle and
    /// high cloud.
    pub cloud_pct: Option<f64>,
    /// Low cloud.
    pub cloud_low_pct: Option<f64>,
    /// Cloud at middle levels.
    pub cloud_mid_pct: Option<f64>,
    /// High cloud.
    pub cloud_high_pct: Option<f64>,
    /// Rain or snow in the hour, millimetres of water.
    pub precipitation_mm: Option<f64>,
    /// The base of convective cloud, metres above sea level. Absent also
    /// when the model has no such cloud over the place at this hour.
    pub cloud_base_m: Option<f64>,
    /// The height of the 0 °C level, metres above sea level.
    pub freezing_level_m: Option<f64>,
    /// Convective available potential energy, J/kg. High values mean that
    /// showers and thunderstorms can grow, not that thermals are good.
    pub cape_j_kg: Option<f64>,
    /// The climb to expect of a paraglider in the hour's thermals, m/s to
    /// a tenth; zero without thermals. Not a value of the model: the app's
    /// estimate from the heat the model's ground gives to the air and from
    /// its temperatures. `None` when the model does not give those.
    pub thermal_climb_ms: Option<f64>,
    /// How high the thermals go by the same estimate, metres above sea
    /// level to the nearest fifty. `None` without thermals.
    pub thermal_top_m: Option<f64>,
    /// Whether cumulus caps them: `thermal_top_m` is then the cloud base.
    pub thermal_cloud: bool,
}

/// One day of a model's forecast, by the clock of the place's own
/// time zone as the shell gives it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ForecastDay {
    /// The day's first moment, seconds since 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// Sunrise at the place that day.
    pub sunrise_unix_s: Option<i64>,
    /// Sunset.
    pub sunset_unix_s: Option<i64>,
    /// The hours of the day the model has, oldest first.
    pub hours: Vec<ForecastHourView>,
}

/// What one weather model says about the place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ModelForecast {
    /// The model's identifier.
    pub model: String,
    /// The name pilots know it by: "ICON-D2".
    pub name: String,
    /// How fine its grid is, in words: "2 km cells". Empty for a model
    /// the app does not know.
    pub grid: String,
    /// The data source it was fetched from; matches a [`Credit`]'s
    /// `source`.
    pub source: String,
    /// When the model's newest run began, seconds since 1970-01-01 UTC.
    pub run_unix_s: i64,
    /// The centre of the model's grid cell the values are for.
    pub cell: Coordinate,
    /// How far that is from the place asked for, metres.
    pub cell_distance_m: f64,
    /// The height of the model's ground in that cell, metres above sea
    /// level. "Wind at 10 m" is the wind 10 m above this, which in the
    /// mountains can lie far below a summit or above a valley floor.
    pub cell_elevation_m: Option<f64>,
    /// The levels the wind higher up is given at, lowest first.
    pub levels: Vec<ForecastLevel>,
    /// The days it reaches over, the first one first.
    pub days: Vec<ForecastDay>,
}

/// The forecast for a place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ForecastView {
    /// The place the server answered for: the place asked for, moved to
    /// the nearest point of its grid.
    pub position: Coordinate,
    /// When the server last made sure it had the newest runs.
    pub fetched_unix_s: i64,
    /// What each model says, the finest first. Never blended.
    pub models: Vec<ModelForecast>,
    /// Whose data it is. The licences ask for this next to the data.
    pub credits: Vec<Credit>,
}

/// The path and query to `GET` from the server for the forecast of a
/// place.
#[uniffi::export]
pub fn forecast_path(lat: f64, lon: f64) -> Result<String, CoreError> {
    let point = GeoPoint::new(lat, lon).map_err(|_| CoreError::Server {
        message: "The app could not ask the server about this place.".to_owned(),
    })?;
    Ok(ForecastRequest { point }.path_and_query())
}

/// Reads the server's answer to [`forecast_path`]. `utc_offset_s` is how
/// far the clock at the place is ahead of UTC, in seconds: the days of
/// the forecast are that clock's days.
#[uniffi::export]
pub fn forecast(status: u16, body: Vec<u8>, utc_offset_s: i32) -> Result<ForecastView, CoreError> {
    let response: ForecastResponse = answer(status, &body)?;
    let mut models: Vec<ModelForecast> = response
        .layers
        .iter()
        .flat_map(|layer| &layer.series)
        .map(|series| model_forecast(series, response.point, utc_offset_s))
        .collect();
    models.sort_by_key(|model| {
        (
            MODELS
                .iter()
                .position(|(id, _, _)| *id == model.model)
                .unwrap_or(MODELS.len()),
            model.model.clone(),
        )
    });
    Ok(ForecastView {
        position: coordinate(response.point),
        fetched_unix_s: response.fetched_at.timestamp(),
        models,
        credits: response.attributions.iter().map(credit).collect(),
    })
}

/// Where a model stands among those the app knows, the finest first. A
/// model the app does not know comes after them all.
pub(crate) fn model_rank(model: &str) -> usize {
    MODELS
        .iter()
        .position(|(id, _, _)| *id == model)
        .unwrap_or(MODELS.len())
}

/// The name pilots know a model by: "ICON-D2". Its identifier in capitals
/// for one the app does not know.
pub(crate) fn model_name(model: &str) -> String {
    MODELS
        .iter()
        .find(|(id, _, _)| *id == model)
        .map_or_else(|| model.to_uppercase(), |(_, name, _)| (*name).to_owned())
}

fn coordinate(point: GeoPoint) -> Coordinate {
    Coordinate {
        lat: point.lat(),
        lon: point.lon(),
    }
}

/// What tells one level from another across the hours of a series.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LevelKey {
    AboveGround(f64),
    Pressure(f64),
}

fn level_key(wind: &WindAloft) -> Option<LevelKey> {
    match (wind.above_ground_m, wind.pressure_hpa) {
        (Some(height), _) => Some(LevelKey::AboveGround(height)),
        (None, Some(pressure)) => Some(LevelKey::Pressure(pressure)),
        (None, None) => None,
    }
}

fn model_forecast(series: &ForecastSeries, asked: GeoPoint, utc_offset_s: i32) -> ModelForecast {
    // The levels of the series, lowest first: the heights above ground,
    // then the pressure surfaces from the highest pressure down.
    let mut keys: Vec<LevelKey> = Vec::new();
    for key in series
        .hours
        .iter()
        .flat_map(|hour| &hour.winds_aloft)
        .filter_map(level_key)
    {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.sort_by(|a, b| match (a, b) {
        (LevelKey::AboveGround(a), LevelKey::AboveGround(b)) => a.total_cmp(b),
        (LevelKey::Pressure(a), LevelKey::Pressure(b)) => b.total_cmp(a),
        (LevelKey::AboveGround(_), LevelKey::Pressure(_)) => std::cmp::Ordering::Less,
        (LevelKey::Pressure(_), LevelKey::AboveGround(_)) => std::cmp::Ordering::Greater,
    });
    let levels = keys
        .iter()
        .map(|key| match *key {
            LevelKey::AboveGround(height) => ForecastLevel {
                name: format!("{height:.0} m above ground"),
                about_altitude_m: None,
            },
            LevelKey::Pressure(pressure) => {
                let heights: Vec<f64> = series
                    .hours
                    .iter()
                    .flat_map(|hour| &hour.winds_aloft)
                    .filter(|wind| level_key(wind) == Some(*key))
                    .filter_map(|wind| wind.altitude_m)
                    .collect();
                ForecastLevel {
                    name: format!("{pressure:.0} hPa"),
                    about_altitude_m: (!heights.is_empty()).then(|| {
                        let mean = heights.iter().sum::<f64>() / heights.len() as f64;
                        (mean / 100.0).round() * 100.0
                    }),
                }
            }
        })
        .collect();

    // The hours by the day of the place's own clock.
    let offset = Duration::seconds(i64::from(utc_offset_s));
    let mut days: Vec<ForecastDay> = Vec::new();
    for hour in &series.hours {
        let local_day = (hour.time + offset).date_naive();
        let start = local_day
            .and_hms_opt(0, 0, 0)
            .map(|midnight| midnight.and_utc() - offset)
            .unwrap_or(hour.time);
        if days
            .last()
            .is_none_or(|day| day.start_unix_s != start.timestamp())
        {
            // The sun's times for the day by the sun that holds the local
            // noon: that is the daylight the day's hours fall in.
            let light = sun::daylight_at(start + Duration::hours(12), series.cell);
            days.push(ForecastDay {
                start_unix_s: start.timestamp(),
                sunrise_unix_s: light.sunrise.map(|time| time.timestamp()),
                sunset_unix_s: light.sunset.map(|time| time.timestamp()),
                hours: Vec::new(),
            });
        }
        if let Some(day) = days.last_mut() {
            let is_daylight = matches!(
                (day.sunrise_unix_s, day.sunset_unix_s),
                (Some(rise), Some(set)) if (rise..=set).contains(&hour.time.timestamp())
            );
            day.hours
                .push(hour_view(hour, &keys, is_daylight, series.cell_elevation_m));
        }
    }

    let (name, grid) = MODELS
        .iter()
        .find(|(id, _, _)| *id == series.model.as_str())
        .map_or_else(
            || (series.model.as_str().to_uppercase(), String::new()),
            |(_, name, grid)| ((*name).to_owned(), (*grid).to_owned()),
        );
    ModelForecast {
        model: series.model.as_str().to_owned(),
        name,
        grid,
        source: series.source.to_string(),
        run_unix_s: series.run.timestamp(),
        cell: coordinate(series.cell),
        cell_distance_m: asked.distance_m(series.cell),
        cell_elevation_m: series.cell_elevation_m,
        levels,
        days,
    }
}

fn hour_view(
    hour: &ForecastHour,
    levels: &[LevelKey],
    is_daylight: bool,
    ground_m: Option<f64>,
) -> ForecastHourView {
    // The thermals over the model's cell, where its ground is known.
    let air = ground_m.and_then(|ground| convection(hour, ground));
    ForecastHourView {
        time_unix_s: hour.time.timestamp(),
        is_daylight,
        wind: hour
            .wind_speed_ms
            .map(|speed| wind(speed, hour.wind_gust_ms, hour.wind_direction_deg)),
        aloft: levels
            .iter()
            .map(|key| {
                let level = hour
                    .winds_aloft
                    .iter()
                    .find(|wind| level_key(wind) == Some(*key))?;
                Some(wind(level.wind_speed_ms?, None, level.wind_direction_deg))
            })
            .collect(),
        temperature_c: hour.temperature_c,
        cloud_pct: hour.cloud_cover_pct,
        cloud_low_pct: hour.cloud_cover_low_pct,
        cloud_mid_pct: hour.cloud_cover_mid_pct,
        cloud_high_pct: hour.cloud_cover_high_pct,
        precipitation_mm: hour.precipitation_mm,
        cloud_base_m: hour.convective_cloud_base_m,
        freezing_level_m: hour.freezing_level_m,
        cape_j_kg: hour.cape_j_kg,
        thermal_climb_ms: air.map(|air| crate::today::tenth(air.climb_ms)),
        thermal_top_m: air
            .filter(|air| air.depth_m > 0.0)
            .map(|air| crate::today::fifty(air.top_m)),
        thermal_cloud: air.is_some_and(|air| air.cloud_base_m.is_some()),
    }
}

pub(crate) fn wind(speed_ms: f64, gust_ms: Option<f64>, from_deg: Option<f64>) -> ForecastWind {
    ForecastWind {
        speed_ms,
        from_deg,
        band: wind_band(speed_ms),
        label: wind_speeds_label(speed_ms, gust_ms),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    fn moment(unix_s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(unix_s, 0).unwrap_or_default()
    }

    /// A summer day's answer for the Tegernsee valley: two models, the
    /// coarser one listed first by the server, three hours each around
    /// midnight by the local clock, which is two hours ahead of UTC.
    const ANSWER: &str = r#"{
        "point": {"lat": 47.7, "lon": 11.8},
        "fetched_at": "2026-07-04T09:40:00Z",
        "layers": [{
            "source": "open-meteo",
            "series": [
                {"source": "open-meteo", "model": "icon-eu", "run": "2026-07-04T06:00:00Z",
                 "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.6875, "lon": 11.8125},
                 "cell_elevation_m": 905.0,
                 "hours": [{"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 3.0}]},
                {"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T06:00:00Z",
                 "point": {"lat": 47.7, "lon": 11.8}, "cell": {"lat": 47.7, "lon": 11.8},
                 "cell_elevation_m": 812.0,
                 "hours": [
                    {"time": "2026-07-04T10:00:00Z", "wind_speed_ms": 2.5, "wind_gust_ms": 6.9,
                     "wind_direction_deg": 250.0, "temperature_c": 24.5, "cloud_cover_pct": 30.0,
                     "cloud_cover_low_pct": 20.0, "precipitation_mm": 0.0, "cape_j_kg": 180.0,
                     "freezing_level_m": 3900.0, "convective_cloud_base_m": 2450.0,
                     "winds_aloft": [
                        {"above_ground_m": 80.0, "wind_speed_ms": 3.6, "wind_direction_deg": 255.0},
                        {"pressure_hpa": 850.0, "altitude_m": 1532.0, "wind_speed_ms": 5.6, "wind_direction_deg": 270.0},
                        {"pressure_hpa": 700.0, "altitude_m": 3140.0, "wind_speed_ms": 9.4, "wind_direction_deg": 290.0}
                     ]},
                    {"time": "2026-07-04T21:00:00Z", "wind_speed_ms": 1.0,
                     "winds_aloft": [
                        {"pressure_hpa": 850.0, "altitude_m": 1548.0, "wind_speed_ms": 4.0}
                     ]},
                    {"time": "2026-07-04T22:00:00Z", "wind_speed_ms": 0.8, "wind_direction_deg": 180.0}
                 ]}
            ]
        }],
        "attributions": [{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                          "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]
    }"#;

    const SUMMER_TIME_S: i32 = 2 * 3600;

    #[test]
    fn the_forecast_of_a_place_is_asked_for() {
        assert_eq!(
            forecast_path(47.7, 11.8).unwrap(),
            "/v1/forecast?lat=47.7&lon=11.8"
        );
        assert!(forecast_path(91.0, 0.0).is_err());
    }

    #[test]
    fn each_model_is_read_by_itself_the_finest_first() {
        let view = forecast(200, ANSWER.as_bytes().to_vec(), SUMMER_TIME_S).unwrap();
        assert_eq!(
            view.position,
            Coordinate {
                lat: 47.7,
                lon: 11.8
            }
        );
        assert_eq!(view.fetched_unix_s, moment(1_783_158_000).timestamp());
        assert_eq!(view.credits[0].text, "Weather data by Open-Meteo.com");

        let names: Vec<&str> = view
            .models
            .iter()
            .map(|model| model.name.as_str())
            .collect();
        assert_eq!(names, ["ICON-D2", "ICON-EU"]);
        let fine = &view.models[0];
        assert_eq!(fine.grid, "2 km cells");
        assert_eq!(fine.source, "open-meteo");
        assert_eq!(fine.cell_elevation_m, Some(812.0));
        assert_eq!(fine.cell_distance_m, 0.0);
        // The coarser model speaks for a cell a kilometre and a half away.
        let coarse = &view.models[1];
        assert!(coarse.cell_distance_m > 1_000.0 && coarse.cell_distance_m < 2_500.0);
        assert!(coarse.levels.is_empty());
    }

    #[test]
    fn hours_fall_into_the_days_of_the_places_own_clock_with_their_light() {
        let view = forecast(200, ANSWER.as_bytes().to_vec(), SUMMER_TIME_S).unwrap();
        let days = &view.models[0].days;
        // 12:00 and 23:00 local are the fourth of July; 00:00 is the fifth.
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].hours.len(), 2);
        assert_eq!(days[1].hours.len(), 1);
        // The day begins at the local midnight, which is 22:00 UTC.
        assert_eq!(days[1].start_unix_s - days[0].start_unix_s, 86_400);
        assert_eq!(days[0].start_unix_s % 86_400, 22 * 3600);
        // Midday is in daylight, an hour before midnight is not.
        assert!(days[0].hours[0].is_daylight);
        assert!(!days[0].hours[1].is_daylight);
        let (rise, set) = (
            days[0].sunrise_unix_s.unwrap(),
            days[0].sunset_unix_s.unwrap(),
        );
        // Early July near Munich: up around 03:20 UTC, down around 19:15.
        assert!(
            (rise - days[0].start_unix_s - 5 * 3600 - 20 * 60).abs() < 20 * 60,
            "{rise}"
        );
        assert!(set - rise > 15 * 3600 && set - rise < 17 * 3600);
    }

    #[test]
    fn the_wind_is_given_at_the_ground_and_at_each_level_in_one_order() {
        let view = forecast(200, ANSWER.as_bytes().to_vec(), SUMMER_TIME_S).unwrap();
        let model = &view.models[0];
        let levels: Vec<&str> = model
            .levels
            .iter()
            .map(|level| level.name.as_str())
            .collect();
        assert_eq!(levels, ["80 m above ground", "850 hPa", "700 hPa"]);
        assert_eq!(model.levels[0].about_altitude_m, None);
        // About how high a pressure surface lies, from the model's own
        // heights for it over the hours.
        assert_eq!(model.levels[1].about_altitude_m, Some(1500.0));
        assert_eq!(model.levels[2].about_altitude_m, Some(3100.0));

        let midday = &model.days[0].hours[0];
        let ground = midday.wind.as_ref().unwrap();
        assert_eq!(ground.label, "9/25");
        assert_eq!(ground.band, WindBand::Light);
        assert_eq!(ground.from_deg, Some(250.0));
        assert_eq!(midday.aloft.len(), 3);
        let high = midday.aloft[2].as_ref().unwrap();
        assert_eq!((high.label.as_str(), high.band), ("34", WindBand::Strong));
        assert_eq!(midday.cloud_base_m, Some(2450.0));
        assert_eq!(midday.cloud_mid_pct, None);

        // An hour without a level has a gap in that row, not a shift.
        let night = &model.days[0].hours[1];
        assert_eq!(night.aloft.len(), 3);
        assert!(night.aloft[0].is_none() && night.aloft[2].is_none());
        let level = night.aloft[1].as_ref().unwrap();
        assert_eq!((level.label.as_str(), level.from_deg), ("14", None));
    }

    #[test]
    fn an_hour_carries_the_estimate_of_its_thermals_where_the_model_gives_what_it_takes() {
        // The same answer with the air that makes thermals at midday: the
        // heat from the ground, a dew point, and a temperature above it.
        let answer = ANSWER
            .replace(
                r#""cape_j_kg": 180.0,"#,
                r#""cape_j_kg": 180.0, "dew_point_c": 10.0, "heat_to_air_w_m2": 400.0,"#,
            )
            .replace(
                r#"{"above_ground_m": 80.0, "wind_speed_ms": 3.6, "wind_direction_deg": 255.0}"#,
                r#"{"above_ground_m": 80.0, "wind_speed_ms": 3.6, "wind_direction_deg": 255.0,
                    "temperature_c": 22.5}"#,
            );
        let view = forecast(200, answer.into_bytes(), SUMMER_TIME_S).unwrap();
        let hours = &view.models[0].days[0].hours;
        // 400 W/m² into the 1640 m under the model's cumulus at 2450 m.
        assert_eq!(hours[0].thermal_climb_ms, Some(1.7));
        assert_eq!(hours[0].thermal_top_m, Some(2450.0));
        assert!(hours[0].thermal_cloud);
        // The hour of the night has no heat in the answer: no estimate.
        assert_eq!(hours[1].thermal_climb_ms, None);
        assert_eq!(hours[1].thermal_top_m, None);
        assert!(!hours[1].thermal_cloud);

        // And none anywhere in an answer without the heat.
        let plain = forecast(200, ANSWER.as_bytes().to_vec(), SUMMER_TIME_S).unwrap();
        assert_eq!(plain.models[0].days[0].hours[0].thermal_climb_ms, None);
    }

    #[test]
    fn what_the_server_itself_answers_is_read() {
        // The server's own golden file of this endpoint: if the server
        // changes what it sends, this says whether the apps still read it.
        let answer = include_str!("../../server/tests/golden/forecast.json");
        let view = forecast(200, answer.as_bytes().to_vec(), 3600).unwrap();
        assert!(!view.models.is_empty());
        for model in &view.models {
            assert!(
                !model.grid.is_empty(),
                "the app does not know the model {}",
                model.model
            );
            assert!(model.cell_elevation_m.is_some());
            assert!(!model.levels.is_empty());
            let hours: Vec<_> = model.days.iter().flat_map(|day| &day.hours).collect();
            assert!(!hours.is_empty());
            assert!(hours.iter().all(|hour| hour.wind.is_some()));
            assert!(
                hours
                    .iter()
                    .all(|hour| hour.aloft.len() == model.levels.len())
            );
        }
        assert!(!view.credits.is_empty());
    }

    #[test]
    fn a_server_without_forecasts_is_said_in_words() {
        for (status, code, words) in [
            (501, "no_forecasts", "This server has no forecasts."),
            (
                404,
                "outside_region",
                "This place lies outside the area the server has forecasts for.",
            ),
        ] {
            let body = format!(r#"{{"error": {{"code": "{code}", "message": "x"}}}}"#);
            let refused = forecast(status, body.into_bytes(), 0).unwrap_err();
            assert_eq!(refused.to_string(), words);
        }
        let refused = forecast(
            503,
            br#"{"error": {"code": "forecast_unavailable", "message": "x"}}"#.to_vec(),
            0,
        )
        .unwrap_err();
        assert!(!refused.to_string().is_empty());
        let empty = forecast(
            200,
            br#"{"point": {"lat": 47.7, "lon": 11.8}, "fetched_at": "2026-07-04T09:40:00Z",
                "layers": [], "attributions": []}"#
                .to_vec(),
            0,
        )
        .unwrap();
        assert!(empty.models.is_empty());
    }
}
