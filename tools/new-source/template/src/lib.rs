//! Adapter for the stations of `__ID__`.
//!
//! To write, in the manner of the other adapters: whose stations these
//! are; which addresses a run asks and what each answers; and for every
//! value that is read, the provider's own words for it and for its unit,
//! with the page and the day it was read. What the provider does not say is
//! written down as "to verify", not guessed.
//!
//! The skeleton below reads an imagined answer: one JSON list with a
//! station and its newest reading in each entry. Replace it with the
//! provider's own form. What stays is the manner:
//!
//! - an adapter opens no connection and reads no clock: it names addresses
//!   in [`ObservationSource::plan`] and is handed bytes in
//!   [`ObservationSource::parse`];
//! - one entry that cannot be read costs that entry, with a line in
//!   `skipped`, and never the whole answer;
//! - every value is converted to the canonical unit here (m/s, degrees the
//!   wind comes from, °C, percent, hPa at the station), by a function with
//!   a test of its own;
//! - a station outside the run's area is left out; a height the provider
//!   does not give is left empty, never zero.

pub mod fixtures;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

/// The address of the newest reading of every station.
pub fn stations_url() -> String {
    "https://example.org/api/stations".to_owned()
}

/// Converts a speed in km/h to m/s. Keep, change or remove: whatever the
/// provider's unit is, its conversion is a function with a test.
pub fn kmh_to_ms(kmh: f64) -> f64 {
    kmh / 3.6
}

/// The stations of `__ID__` as an observation source.
#[derive(Debug, Clone)]
pub struct __TYPE__ {
    manifest: Manifest,
}

impl __TYPE__ {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-__NAME__ is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in. The server's list of adapters
/// (`crates/server/src/adapters.rs`, `station_networks`) has one line for
/// this function and names nothing else of the crate.
///
/// Without a sample, as here, the network is in no run of the development
/// mode, and no test of an app changes. A network that joins the first
/// region needs one: see [`fixtures`].
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(__TYPE__::new())
}

impl Default for __TYPE__ {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for __TYPE__ {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("stations", stations_url())]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "stations" => parse_stations(&self.manifest.id, body, ctx),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// What is read of one entry of the imagined answer.
#[derive(Deserialize)]
struct Entry {
    code: String,
    name: String,
    lat: f64,
    lon: f64,
    #[serde(default)]
    elevation: Option<f64>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    wind_kmh: Option<f64>,
    #[serde(default)]
    gust_kmh: Option<f64>,
    #[serde(default)]
    wind_from_deg: Option<f64>,
    #[serde(default)]
    temperature_c: Option<f64>,
}

fn parse_stations(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let entries: Vec<serde_json::Value> = serde_json::from_slice(body)
        .map_err(|e| SourceError::Malformed(format!("the answer is not the expected JSON: {e}")))?;

    let mut parsed = Parsed::default();
    for value in entries {
        let entry = match Entry::deserialize(&value) {
            Ok(entry) => entry,
            Err(error) => {
                parsed.skipped.push(Skipped::new(
                    &value.to_string(),
                    format!("station cannot be read: {error}"),
                ));
                continue;
            }
        };
        let station = GeoPoint::new(entry.lat, entry.lon)
            .map_err(|e| e.to_string())
            .and_then(|position| {
                let id = StationId::new(source.clone(), entry.code.clone())
                    .map_err(|e| e.to_string())?;
                Ok(Station {
                    id,
                    name: entry.name.trim().to_owned(),
                    position,
                    elevation_m: entry.elevation,
                })
            });
        let station = match station {
            Ok(station) => station,
            Err(reason) => {
                parsed.skipped.push(Skipped::new(&entry.code, reason));
                continue;
            }
        };
        if !ctx.area.contains(station.position) {
            continue;
        }
        if let Some(text) = &entry.time {
            match parse_time(text) {
                Some(time) => {
                    let mut observation = Observation::empty(station.id.clone(), time);
                    observation.wind_speed_ms = entry.wind_kmh.map(kmh_to_ms);
                    observation.wind_gust_ms = entry.gust_kmh.map(kmh_to_ms);
                    observation.wind_direction_deg = entry.wind_from_deg;
                    observation.temperature_c = entry.temperature_c;
                    if !observation.is_empty() {
                        parsed.observations.push(observation);
                    }
                }
                None => parsed.skipped.push(Skipped::new(
                    &format!("{} {text}", entry.code),
                    "time stamp cannot be read",
                )),
            }
        }
        parsed.stations.push(station);
    }
    Ok(parsed)
}

/// Reads a time stamp with its offset, `2026-11-22T07:30:00Z`. A provider
/// that writes local time without an offset needs its own reading, and a
/// test that crosses the change of the clocks.
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
    }

    #[test]
    fn reads_the_time_stamps_as_utc() {
        let at = Utc.with_ymd_and_hms(2026, 11, 22, 7, 30, 0).unwrap();
        assert_eq!(parse_time("2026-11-22T07:30:00Z"), Some(at));
        assert_eq!(parse_time("2026-11-22T08:30:00+01:00"), Some(at));
        assert_eq!(parse_time("2026-11-22 07:30"), None);
    }
}
