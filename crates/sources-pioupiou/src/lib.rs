//! Adapter for the wind sensors of the Pioupiou / OpenWindMap network: the
//! pilots' own anemometers, most of them at take-offs and landing fields.
//!
//! One request answers for every sensor of the network, wherever it
//! stands: its newest reading, its position and its name. A run asks it
//! once and keeps the sensors inside the run's area.
//!
//! What the network's documentation (the Live API's page, read 2026-11-22)
//! says of a reading, and how it is taken:
//!
//! - The speeds are in km/h: the mean, the minimum and the maximum "over the
//!   last 4 minutes before measurements.date". The mean is the wind speed
//!   and the maximum the gust, both in m/s. The minimum has no field.
//! - The heading is in degrees, the direction the wind comes from ("0°
//!   means the wind is blowing from North to Sud").
//! - `measurements.date` is the reading's own time, in UTC, and the end of
//!   its four minutes. It is the observation's time, however old: a sensor
//!   switched off or out of reach keeps its last reading in the answer for
//!   days or months, and the run's `since` leaves those out.
//! - The pressure is "null (deprecated)" and is not read. The network
//!   measures neither temperature nor humidity.
//! - A sensor's position is its "last known" one, from its own GPS, and
//!   `location.success` says whether it is "still valid". A sensor whose
//!   position is no longer valid is left out and reported, as is one without
//!   a position. The answers write an unknown position as 0, 0 as well as
//!   null; 0, 0 is read as unknown.
//! - The network has no height for its sensors. Many names carry one ("Déco
//!   Planpraz Chamonix 1958m"), but a name is not a measurement, so a
//!   station's height is left empty.
//!
//! A sensor without a reading of wind has a station and no observation.

pub mod fixtures;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

/// The newest reading of every sensor of the network.
const LIVE_ALL: &str = "https://api.pioupiou.fr/v1/live/all";

/// The address of the newest reading of every sensor.
pub fn live_url() -> String {
    LIVE_ALL.to_owned()
}

/// Converts a speed in km/h, as the network gives it, to m/s.
pub fn kmh_to_ms(kmh: f64) -> f64 {
    kmh / 3.6
}

/// The Pioupiou / OpenWindMap network as an observation source.
#[derive(Debug, Clone)]
pub struct Pioupiou {
    manifest: Manifest,
}

impl Pioupiou {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-pioupiou is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(Pioupiou::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for Pioupiou {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for Pioupiou {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        // The service answers for all sensors or for one; one request for
        // all is cheaper than one per sensor of any area.
        vec![Request::new("live", live_url())]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "live" => parse_live(&self.manifest.id, body, ctx),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    data: Vec<serde_json::Value>,
}

/// What is read of one sensor.
#[derive(Deserialize)]
struct Sensor {
    id: u64,
    meta: Meta,
    location: Location,
    #[serde(default)]
    measurements: Option<Measurements>,
}

#[derive(Deserialize)]
struct Meta {
    name: String,
}

#[derive(Deserialize)]
struct Location {
    latitude: Option<f64>,
    longitude: Option<f64>,
    #[serde(default)]
    success: Option<bool>,
}

#[derive(Deserialize)]
struct Measurements {
    date: Option<String>,
    wind_heading: Option<f64>,
    wind_speed_avg: Option<f64>,
    wind_speed_max: Option<f64>,
}

fn parse_live(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let answer: Answer = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the live answer is not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    for entry in answer.data {
        let sensor = match Sensor::deserialize(&entry) {
            Ok(sensor) => sensor,
            Err(error) => {
                let what = entry
                    .get("id")
                    .map_or_else(|| entry.to_string(), serde_json::Value::to_string);
                parsed.skipped.push(Skipped::new(
                    &what,
                    format!("sensor cannot be read: {error}"),
                ));
                continue;
            }
        };
        let code = sensor.id.to_string();
        let (Some(lat), Some(lon)) = (sensor.location.latitude, sensor.location.longitude) else {
            parsed
                .skipped
                .push(Skipped::new(&code, "a sensor without a position"));
            continue;
        };
        if lat == 0.0 && lon == 0.0 {
            parsed.skipped.push(Skipped::new(
                &code,
                "a sensor without a position: the network writes 0, 0 for one it does not know",
            ));
            continue;
        }
        let station = GeoPoint::new(lat, lon)
            .map_err(|e| e.to_string())
            .and_then(|position| {
                let id = StationId::new(source.clone(), code.clone()).map_err(|e| e.to_string())?;
                Ok(Station {
                    id,
                    name: sensor.meta.name.trim().to_owned(),
                    position,
                    elevation_m: None,
                })
            });
        let station = match station {
            Ok(station) => station,
            Err(reason) => {
                parsed.skipped.push(Skipped::new(&code, reason));
                continue;
            }
        };
        if !ctx.area.contains(station.position) {
            continue;
        }
        if sensor.location.success == Some(false) {
            parsed.skipped.push(Skipped::new(
                &code,
                "the network says the sensor's last known position is no longer valid",
            ));
            continue;
        }

        let reading = sensor.measurements.and_then(|m| {
            let date = m.date.clone()?;
            Some((date, m))
        });
        if let Some((date, measurements)) = reading {
            match parse_time(&date) {
                Some(time) => {
                    let mut observation = Observation::empty(station.id.clone(), time);
                    observation.wind_speed_ms = measurements.wind_speed_avg.map(kmh_to_ms);
                    observation.wind_gust_ms = measurements.wind_speed_max.map(kmh_to_ms);
                    observation.wind_direction_deg = measurements.wind_heading;
                    if !observation.is_empty() {
                        parsed.observations.push(observation);
                    }
                }
                None => parsed.skipped.push(Skipped::new(
                    &format!("{code} {date}"),
                    "time stamp cannot be read",
                )),
            }
        }
        parsed.stations.push(station);
    }
    Ok(parsed)
}

/// Reads a time stamp of the network: UTC in ISO 8601 with milliseconds,
/// `2026-11-22T08:07:04.000Z`.
fn parse_time(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn converts_kilometres_an_hour_to_metres_a_second() {
        assert!((kmh_to_ms(36.0) - 10.0).abs() < 1e-12);
        assert!((kmh_to_ms(3.6) - 1.0).abs() < 1e-12);
        assert!((kmh_to_ms(21.5) - 5.972_222_222_222_222).abs() < 1e-12);
        assert!(kmh_to_ms(0.0).abs() < f64::EPSILON);
        // 1 kn is 1.852 km/h and 0.514444 m/s.
        assert!((kmh_to_ms(1.852) - 0.514_444).abs() < 1e-6);
    }

    #[test]
    fn reads_the_networks_time_stamps() {
        let at = Utc.with_ymd_and_hms(2026, 11, 22, 8, 7, 4).unwrap();
        assert_eq!(parse_time("2026-11-22T08:07:04.000Z"), Some(at));
        assert_eq!(parse_time("2026-11-22T08:07:04Z"), Some(at));
        assert_eq!(
            parse_time("2026-11-09T03:20:19.387Z"),
            Some(
                Utc.with_ymd_and_hms(2026, 11, 9, 3, 20, 19).unwrap()
                    + chrono::Duration::milliseconds(387)
            )
        );
        assert_eq!(parse_time("2026-11-22T09:07:04.000+01:00"), Some(at));
        assert_eq!(parse_time("2026-11-22 08:07"), None);
        assert_eq!(parse_time("2026-11-22T08:07:04"), None);
        assert_eq!(parse_time(""), None);
    }
}
