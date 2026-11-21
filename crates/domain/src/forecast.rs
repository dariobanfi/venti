//! The canonical point forecast.
//!
//! A [`ForecastSeries`] is what one weather model says about one place, hour
//! by hour. It holds magnitudes and no judgement: nothing here says whether
//! a day can be flown.
//!
//! Every series names the model, the run of the model, the grid cell the
//! values are for and the height of the model's ground there. A model's
//! ground is smoothed: in the mountains the cell that holds a summit can
//! lie hundreds of metres below it, and "wind at 10 m" is the wind 10 m
//! above that smoothed ground. Whoever shows a series shows the height
//! beside it.
//!
//! Two series for one place, from two models, are never blended into one.

use serde::{Deserialize, Serialize};

use crate::{GeoPoint, ModelId, SourceId, Timestamp};

/// What one weather model forecasts for one place, hour by hour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastSeries {
    /// The source the values were fetched from.
    pub source: SourceId,
    /// The model that computed them.
    pub model: ModelId,
    /// When the run of the model that computed these hours began: the time
    /// the model's computation started from. It is the newest run the
    /// source held.
    ///
    /// Every hour of the series is this run's. A source keeps one value per
    /// hour, the newest it has, so it also holds hours of earlier runs:
    /// those before `run`, and those beyond the reach of a short run. It
    /// does not say which run they are from, and an adapter leaves them
    /// out. A series can therefore end before the days asked for do.
    pub run: Timestamp,
    /// When the source had that run ready to be asked, if it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_at: Option<Timestamp>,
    /// The place the forecast was asked for.
    pub point: GeoPoint,
    /// The centre of the model's grid cell the values are for. It can lie
    /// some kilometres from `point`.
    pub cell: GeoPoint,
    /// The height of the model's ground in that cell, metres above mean sea
    /// level, if the source gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_elevation_m: Option<f64>,
    /// The hours, oldest first.
    pub hours: Vec<ForecastHour>,
}

/// What a model forecasts for one hour.
///
/// Every value is optional: models differ in what they compute, and a value
/// a source does not deliver is `None`, never a placeholder. Unless a field
/// says otherwise, a value holds at the moment `time`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastHour {
    /// The moment the values are for.
    pub time: Timestamp,
    /// Wind speed 10 m above the model's ground, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_speed_ms: Option<f64>,
    /// The direction that wind comes from, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_direction_deg: Option<f64>,
    /// Strongest gust 10 m above the model's ground in the hour before
    /// `time`, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_gust_ms: Option<f64>,
    /// Air temperature 2 m above the model's ground, °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
    /// Dew point 2 m above the model's ground, °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dew_point_c: Option<f64>,
    /// Share of the sky covered by cloud at any height, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_cover_pct: Option<f64>,
    /// Share covered by low cloud, percent. Where "low" ends is the
    /// model's own definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_cover_low_pct: Option<f64>,
    /// Share covered by cloud at middle heights, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_cover_mid_pct: Option<f64>,
    /// Share covered by high cloud, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_cover_high_pct: Option<f64>,
    /// Rain, showers and snow that fall in the hour before `time`, in
    /// millimetres of water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precipitation_mm: Option<f64>,
    /// Convective available potential energy, J/kg: the energy a rising
    /// parcel of air could gain. High values mean showers and
    /// thunderstorms can grow, not that thermals are good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cape_j_kg: Option<f64>,
    /// Height of the 0 °C level, metres above mean sea level. Where the air
    /// is below 0 °C all the way down, a model gives the height of its own
    /// ground instead: a value at or near `cell_elevation_m` means "at the
    /// ground or below it", not a level in the air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freezing_level_m: Option<f64>,
    /// The strongest rising air the model has anywhere above the cell, up
    /// to 10 km, m/s. It is what a model with cells two kilometres wide
    /// resolves, which is a shower or a storm cloud and not a thermal: no
    /// pilot climbs at this rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updraft_ms: Option<f64>,
    /// Base of convective cloud, metres above mean sea level. `None` also
    /// when the model has no convective cloud over the cell at this hour.
    /// Which convection a model means differs from model to model: one
    /// that resolves showers and storms itself reports only the shallow
    /// cloud it still has to estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convective_cloud_base_m: Option<f64>,
    /// Top of convective cloud, metres above mean sea level. `None` also
    /// when the model has no convective cloud over the cell at this hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convective_cloud_top_m: Option<f64>,
    /// The heat the ground gives to the air above it, W/m², as the mean of
    /// the hour before `time`: positive when the ground warms the air,
    /// which a sunlit slope does and which is what thermals are made of;
    /// negative when the air warms the ground, as on a clear night. It is
    /// the model's own balance for its cell, not a measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat_to_air_w_m2: Option<f64>,
    /// The wind higher up, lowest level first: first the levels at a fixed
    /// height above the ground, then the pressure surfaces from the highest
    /// pressure to the lowest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub winds_aloft: Vec<WindAloft>,
}

/// The wind at one level above the ground.
///
/// A level is given in one of two ways, and exactly one of
/// `above_ground_m` and `pressure_hpa` is set:
///
/// - a fixed height above the model's ground, such as 80 m: the nominal
///   height of one of the model's own levels;
/// - a surface of equal air pressure, such as 850 hPa. How high that
///   surface lies changes with the weather, so the model's own height for
///   it at this hour comes with it in `altitude_m`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindAloft {
    /// Height of the level above the model's ground, metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub above_ground_m: Option<f64>,
    /// Pressure of the surface, hPa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_hpa: Option<f64>,
    /// Height of a pressure surface above mean sea level at this hour,
    /// metres, as the model gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude_m: Option<f64>,
    /// Wind speed at the level, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_speed_ms: Option<f64>,
    /// The direction the wind comes from, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_direction_deg: Option<f64>,
    /// Air temperature at the level, °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
}

impl ForecastHour {
    /// An hour with no values yet.
    pub fn empty(time: Timestamp) -> Self {
        Self {
            time,
            wind_speed_ms: None,
            wind_direction_deg: None,
            wind_gust_ms: None,
            temperature_c: None,
            dew_point_c: None,
            cloud_cover_pct: None,
            cloud_cover_low_pct: None,
            cloud_cover_mid_pct: None,
            cloud_cover_high_pct: None,
            precipitation_mm: None,
            cape_j_kg: None,
            freezing_level_m: None,
            updraft_ms: None,
            convective_cloud_base_m: None,
            convective_cloud_top_m: None,
            heat_to_air_w_m2: None,
            winds_aloft: Vec::new(),
        }
    }

    /// Whether the hour holds no value at all.
    pub fn is_empty(&self) -> bool {
        self.fields().iter().all(|(_, value)| value.is_none()) && self.winds_aloft.is_empty()
    }

    /// The values at the ground with their field names, for code that
    /// treats them uniformly.
    pub fn fields(&self) -> [(&'static str, Option<f64>); 16] {
        [
            ("wind_speed_ms", self.wind_speed_ms),
            ("wind_direction_deg", self.wind_direction_deg),
            ("wind_gust_ms", self.wind_gust_ms),
            ("temperature_c", self.temperature_c),
            ("dew_point_c", self.dew_point_c),
            ("cloud_cover_pct", self.cloud_cover_pct),
            ("cloud_cover_low_pct", self.cloud_cover_low_pct),
            ("cloud_cover_mid_pct", self.cloud_cover_mid_pct),
            ("cloud_cover_high_pct", self.cloud_cover_high_pct),
            ("precipitation_mm", self.precipitation_mm),
            ("cape_j_kg", self.cape_j_kg),
            ("freezing_level_m", self.freezing_level_m),
            ("updraft_ms", self.updraft_ms),
            ("convective_cloud_base_m", self.convective_cloud_base_m),
            ("convective_cloud_top_m", self.convective_cloud_top_m),
            ("heat_to_air_w_m2", self.heat_to_air_w_m2),
        ]
    }

    /// Mutable access to the values at the ground with their field names.
    pub fn fields_mut(&mut self) -> [(&'static str, &mut Option<f64>); 16] {
        [
            ("wind_speed_ms", &mut self.wind_speed_ms),
            ("wind_direction_deg", &mut self.wind_direction_deg),
            ("wind_gust_ms", &mut self.wind_gust_ms),
            ("temperature_c", &mut self.temperature_c),
            ("dew_point_c", &mut self.dew_point_c),
            ("cloud_cover_pct", &mut self.cloud_cover_pct),
            ("cloud_cover_low_pct", &mut self.cloud_cover_low_pct),
            ("cloud_cover_mid_pct", &mut self.cloud_cover_mid_pct),
            ("cloud_cover_high_pct", &mut self.cloud_cover_high_pct),
            ("precipitation_mm", &mut self.precipitation_mm),
            ("cape_j_kg", &mut self.cape_j_kg),
            ("freezing_level_m", &mut self.freezing_level_m),
            ("updraft_ms", &mut self.updraft_ms),
            ("convective_cloud_base_m", &mut self.convective_cloud_base_m),
            ("convective_cloud_top_m", &mut self.convective_cloud_top_m),
            ("heat_to_air_w_m2", &mut self.heat_to_air_w_m2),
        ]
    }
}

impl WindAloft {
    /// A level at a fixed height above the model's ground, with no values yet.
    pub fn above_ground(metres: f64) -> Self {
        Self {
            above_ground_m: Some(metres),
            pressure_hpa: None,
            altitude_m: None,
            wind_speed_ms: None,
            wind_direction_deg: None,
            temperature_c: None,
        }
    }

    /// A surface of equal pressure, with no values yet.
    pub fn at_pressure(hpa: f64) -> Self {
        Self {
            above_ground_m: None,
            pressure_hpa: Some(hpa),
            altitude_m: None,
            wind_speed_ms: None,
            wind_direction_deg: None,
            temperature_c: None,
        }
    }

    /// Whether the level holds neither a wind nor a temperature.
    pub fn is_empty(&self) -> bool {
        self.wind_speed_ms.is_none()
            && self.wind_direction_deg.is_none()
            && self.temperature_c.is_none()
    }

    /// The values with their field names, for code that treats them
    /// uniformly.
    pub fn fields_mut(&mut self) -> [(&'static str, &mut Option<f64>); 4] {
        [
            ("altitude_m", &mut self.altitude_m),
            ("wind_speed_ms", &mut self.wind_speed_ms),
            ("wind_direction_deg", &mut self.wind_direction_deg),
            ("temperature_c", &mut self.temperature_c),
        ]
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn noon() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap()
    }

    #[test]
    fn an_hour_without_values_is_empty_and_one_value_fills_it() {
        let mut hour = ForecastHour::empty(noon());
        assert!(hour.is_empty());
        hour.winds_aloft.push(WindAloft::at_pressure(850.0));
        assert!(!hour.is_empty());

        let mut hour = ForecastHour::empty(noon());
        hour.cape_j_kg = Some(0.0);
        assert!(!hour.is_empty());
        assert_eq!(hour.fields().len(), hour.fields_mut().len());
    }

    #[test]
    fn what_a_model_does_not_compute_is_left_out_of_the_record() {
        let mut level = WindAloft::at_pressure(850.0);
        level.altitude_m = Some(1489.0);
        level.wind_speed_ms = Some(9.4);
        level.wind_direction_deg = Some(262.0);
        let mut low = WindAloft::above_ground(80.0);
        low.wind_speed_ms = Some(4.1);
        let mut hour = ForecastHour::empty(noon());
        hour.wind_speed_ms = Some(2.5);
        hour.winds_aloft = vec![low, level];
        let series = ForecastSeries {
            source: SourceId::new("open-meteo").unwrap(),
            model: ModelId::new("icon-d2").unwrap(),
            run: Utc.with_ymd_and_hms(2026, 11, 20, 9, 0, 0).unwrap(),
            available_at: None,
            point: GeoPoint::new(47.7, 11.8).unwrap(),
            cell: GeoPoint::new(47.7, 11.8).unwrap(),
            cell_elevation_m: Some(812.0),
            hours: vec![hour],
        };

        let json = serde_json::to_string(&series).unwrap();

        assert_eq!(
            json,
            concat!(
                r#"{"source":"open-meteo","model":"icon-d2","run":"2026-11-20T09:00:00Z","#,
                r#""point":{"lat":47.7,"lon":11.8},"cell":{"lat":47.7,"lon":11.8},"#,
                r#""cell_elevation_m":812.0,"hours":[{"time":"2026-11-20T12:00:00Z","#,
                r#""wind_speed_ms":2.5,"winds_aloft":[{"above_ground_m":80.0,"wind_speed_ms":4.1},"#,
                r#"{"pressure_hpa":850.0,"altitude_m":1489.0,"wind_speed_ms":9.4,"#,
                r#""wind_direction_deg":262.0}]}]}"#
            )
        );
        assert_eq!(
            serde_json::from_str::<ForecastSeries>(&json).unwrap(),
            series
        );
    }
}
