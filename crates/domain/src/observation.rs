use serde::{Deserialize, Serialize};

use crate::{StationId, Timestamp};

/// What one station measured over one interval.
///
/// Every measurement is optional because stations differ in their sensors and
/// providers publish gaps. A field is `None` when the value is missing; it is
/// never a sentinel such as `-999`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// The station that measured.
    pub station: StationId,
    /// End of the measuring interval.
    pub time: Timestamp,
    /// Mean wind speed over the interval, m/s.
    pub wind_speed_ms: Option<f64>,
    /// Strongest gust in the interval, m/s.
    pub wind_gust_ms: Option<f64>,
    /// Mean direction the wind came from, degrees.
    pub wind_direction_deg: Option<f64>,
    /// Direction the strongest gust came from, degrees.
    pub gust_direction_deg: Option<f64>,
    /// Air temperature, °C.
    pub temperature_c: Option<f64>,
    /// Relative humidity, percent.
    pub humidity_pct: Option<f64>,
    /// Air pressure at the station's own height, hPa.
    pub pressure_hpa: Option<f64>,
}

impl Observation {
    /// An observation with no measurements yet.
    pub fn empty(station: StationId, time: Timestamp) -> Self {
        Self {
            station,
            time,
            wind_speed_ms: None,
            wind_gust_ms: None,
            wind_direction_deg: None,
            gust_direction_deg: None,
            temperature_c: None,
            humidity_pct: None,
            pressure_hpa: None,
        }
    }

    /// Whether no measurement is present.
    pub fn is_empty(&self) -> bool {
        self.fields().iter().all(|(_, value)| value.is_none())
    }

    /// Fills measurements this observation lacks from `other`.
    ///
    /// Providers that publish wind, gusts and temperature in separate files
    /// produce several partial observations for the same station and time.
    /// A value already present is kept. Returns how many fields had different
    /// values on both sides, so the caller can count disagreements.
    pub fn merge(&mut self, other: &Observation) -> usize {
        let mut conflicts = 0;
        let pairs: [(&mut Option<f64>, Option<f64>); 7] = [
            (&mut self.wind_speed_ms, other.wind_speed_ms),
            (&mut self.wind_gust_ms, other.wind_gust_ms),
            (&mut self.wind_direction_deg, other.wind_direction_deg),
            (&mut self.gust_direction_deg, other.gust_direction_deg),
            (&mut self.temperature_c, other.temperature_c),
            (&mut self.humidity_pct, other.humidity_pct),
            (&mut self.pressure_hpa, other.pressure_hpa),
        ];
        for (mine, theirs) in pairs {
            match (*mine, theirs) {
                (None, Some(value)) => *mine = Some(value),
                (Some(a), Some(b)) if a != b => conflicts += 1,
                _ => {}
            }
        }
        conflicts
    }

    /// The measurements with their field names, for code that treats them uniformly.
    pub fn fields(&self) -> [(&'static str, Option<f64>); 7] {
        [
            ("wind_speed_ms", self.wind_speed_ms),
            ("wind_gust_ms", self.wind_gust_ms),
            ("wind_direction_deg", self.wind_direction_deg),
            ("gust_direction_deg", self.gust_direction_deg),
            ("temperature_c", self.temperature_c),
            ("humidity_pct", self.humidity_pct),
            ("pressure_hpa", self.pressure_hpa),
        ]
    }

    /// Mutable access to the measurements with their field names.
    pub fn fields_mut(&mut self) -> [(&'static str, &mut Option<f64>); 7] {
        [
            ("wind_speed_ms", &mut self.wind_speed_ms),
            ("wind_gust_ms", &mut self.wind_gust_ms),
            ("wind_direction_deg", &mut self.wind_direction_deg),
            ("gust_direction_deg", &mut self.gust_direction_deg),
            ("temperature_c", &mut self.temperature_c),
            ("humidity_pct", &mut self.humidity_pct),
            ("pressure_hpa", &mut self.pressure_hpa),
        ]
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at_zugspitze() -> Observation {
        let station = "dwd-10min:05792".parse().unwrap();
        let time = chrono::Utc
            .with_ymd_and_hms(2026, 11, 18, 18, 20, 0)
            .unwrap();
        Observation::empty(station, time)
    }

    #[test]
    fn merge_fills_gaps_and_keeps_existing_values() {
        let mut wind = at_zugspitze();
        wind.wind_speed_ms = Some(14.9);
        let mut gust = at_zugspitze();
        gust.wind_gust_ms = Some(25.7);
        gust.wind_speed_ms = Some(15.0);

        let conflicts = wind.merge(&gust);

        assert_eq!(wind.wind_speed_ms, Some(14.9));
        assert_eq!(wind.wind_gust_ms, Some(25.7));
        assert_eq!(conflicts, 1);
    }

    #[test]
    fn empty_observation_reports_empty() {
        let mut obs = at_zugspitze();
        assert!(obs.is_empty());
        obs.temperature_c = Some(-7.1);
        assert!(!obs.is_empty());
    }
}
