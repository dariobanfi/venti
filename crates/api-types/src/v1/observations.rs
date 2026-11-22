use serde::{Deserialize, Serialize};
use venti_domain::{Observation, Timestamp};

use super::{Attribution, Station};

/// The answer of `GET /v1/stations/{id}/observations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationsResponse {
    /// The station that measured.
    pub station: Station,
    /// What it measured in the requested window, oldest first.
    pub observations: Vec<Reading>,
    /// The credit for the station's source.
    pub attributions: Vec<Attribution>,
}

/// What a station measured over one interval.
///
/// This is an observation without the station's identifier, which the
/// surrounding record already names. A measurement the station did not
/// deliver is left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// End of the measuring interval.
    pub time: Timestamp,
    /// Mean wind speed over the interval, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_speed_ms: Option<f64>,
    /// Strongest gust in the interval, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_gust_ms: Option<f64>,
    /// Mean direction the wind came from, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_direction_deg: Option<f64>,
    /// Direction the strongest gust came from, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gust_direction_deg: Option<f64>,
    /// Air temperature, °C.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f64>,
    /// Relative humidity, percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub humidity_pct: Option<f64>,
    /// Air pressure at the station's own height, hPa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_hpa: Option<f64>,
    /// Air pressure reduced to sea level, hPa, by the provider's own
    /// reduction. Only from a station whose provider sends it so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_sea_level_hpa: Option<f64>,
}

impl From<&Observation> for Reading {
    fn from(observation: &Observation) -> Self {
        Self {
            time: observation.time,
            wind_speed_ms: observation.wind_speed_ms,
            wind_gust_ms: observation.wind_gust_ms,
            wind_direction_deg: observation.wind_direction_deg,
            gust_direction_deg: observation.gust_direction_deg,
            temperature_c: observation.temperature_c,
            humidity_pct: observation.humidity_pct,
            pressure_hpa: observation.pressure_hpa,
            pressure_sea_level_hpa: observation.pressure_sea_level_hpa,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn a_missing_measurement_is_left_out_and_reads_back_as_none() {
        let station = "dwd-10min:05792".parse().unwrap();
        let time = chrono::Utc
            .with_ymd_and_hms(2026, 11, 18, 18, 20, 0)
            .unwrap();
        let mut observation = Observation::empty(station, time);
        observation.wind_speed_ms = Some(14.9);

        let reading = Reading::from(&observation);
        let json = serde_json::to_string(&reading).unwrap();

        assert_eq!(
            json,
            r#"{"time":"2026-11-18T18:20:00Z","wind_speed_ms":14.9}"#
        );
        assert_eq!(serde_json::from_str::<Reading>(&json).unwrap(), reading);
    }
}
