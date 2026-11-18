use chrono::Duration;
use serde::{Deserialize, Serialize};
use venti_domain::{Observation, Station, Timestamp};

/// A value the pipeline refused to pass on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rejection {
    /// The station the value belongs to, as text.
    pub station: String,
    /// The time of the observation, if the value belongs to one.
    pub time: Option<Timestamp>,
    /// The field that was refused.
    pub field: String,
    /// The refused value.
    pub value: f64,
    /// Why.
    pub reason: String,
}

/// Plausible limits for each measurement. A value outside its limits is a
/// sensor or transmission fault, not weather.
const LIMITS: [(&str, f64, f64); 7] = [
    ("wind_speed_ms", 0.0, 120.0),
    ("wind_gust_ms", 0.0, 120.0),
    ("wind_direction_deg", 0.0, 360.0),
    ("gust_direction_deg", 0.0, 360.0),
    ("temperature_c", -80.0, 60.0),
    ("humidity_pct", 0.0, 100.0),
    ("pressure_hpa", 300.0, 1100.0),
];

/// How far ahead of the run time an observation may be stamped before it is
/// treated as a clock fault.
const MAX_FUTURE: Duration = Duration::minutes(15);

/// Lowest and highest station heights accepted, in metres.
const ELEVATION_LIMITS: (f64, f64) = (-500.0, 9000.0);

/// Removes implausible measurements from an observation.
///
/// Returns what was removed. If the observation is stamped too far in the
/// future, every measurement is removed. The caller drops observations that
/// end up empty.
pub(crate) fn check_observation(obs: &mut Observation, now: Timestamp) -> Vec<Rejection> {
    let mut rejections = Vec::new();
    let station = obs.station.to_string();
    let time = obs.time;

    if time > now + MAX_FUTURE {
        for (field, slot) in obs.fields_mut() {
            if let Some(value) = slot.take() {
                rejections.push(Rejection {
                    station: station.clone(),
                    time: Some(time),
                    field: field.to_owned(),
                    value,
                    reason: "observation is stamped in the future".to_owned(),
                });
            }
        }
        return rejections;
    }

    for (field, slot) in obs.fields_mut() {
        let Some(value) = *slot else { continue };
        let (_, low, high) = LIMITS
            .iter()
            .find(|(name, _, _)| *name == field)
            .expect("every observation field has limits");
        if !value.is_finite() || value < *low || value > *high {
            *slot = None;
            rejections.push(Rejection {
                station: station.clone(),
                time: Some(time),
                field: field.to_owned(),
                value,
                reason: format!("outside {low}..={high}"),
            });
        }
    }
    rejections
}

/// Removes an implausible elevation from a station.
pub(crate) fn check_station(station: &mut Station) -> Option<Rejection> {
    let value = station.elevation_m?;
    let (low, high) = ELEVATION_LIMITS;
    if value.is_finite() && (low..=high).contains(&value) {
        return None;
    }
    station.elevation_m = None;
    Some(Rejection {
        station: station.id.to_string(),
        time: None,
        field: "elevation_m".to_owned(),
        value,
        reason: format!("outside {low}..={high}"),
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> Timestamp {
        chrono::Utc
            .with_ymd_and_hms(2026, 11, 18, 19, 0, 0)
            .unwrap()
    }

    fn observation(minutes_from_now: i64) -> Observation {
        let station = "example:1".parse().unwrap();
        Observation::empty(station, now() + Duration::minutes(minutes_from_now))
    }

    #[test]
    fn keeps_plausible_values() {
        let mut obs = observation(-10);
        obs.wind_speed_ms = Some(14.9);
        obs.wind_direction_deg = Some(360.0);
        obs.temperature_c = Some(-7.1);
        assert!(check_observation(&mut obs, now()).is_empty());
        assert_eq!(obs.wind_speed_ms, Some(14.9));
    }

    #[test]
    fn removes_only_the_implausible_field() {
        let mut obs = observation(-10);
        obs.wind_speed_ms = Some(14.9);
        obs.humidity_pct = Some(140.0);
        let rejections = check_observation(&mut obs, now());
        assert_eq!(rejections.len(), 1);
        assert_eq!(rejections[0].field, "humidity_pct");
        assert_eq!(obs.humidity_pct, None);
        assert_eq!(obs.wind_speed_ms, Some(14.9));
    }

    #[test]
    fn empties_an_observation_from_the_future() {
        let mut obs = observation(60);
        obs.wind_speed_ms = Some(3.0);
        let rejections = check_observation(&mut obs, now());
        assert_eq!(rejections.len(), 1);
        assert!(obs.is_empty());
    }

    #[test]
    fn tolerates_a_slightly_fast_station_clock() {
        let mut obs = observation(10);
        obs.wind_speed_ms = Some(3.0);
        assert!(check_observation(&mut obs, now()).is_empty());
    }
}
