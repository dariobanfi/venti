//! Adapter for the IMIS stations of the WSL Institute for Snow and Avalanche
//! Research SLF: the automatic stations of the avalanche warning service in
//! Switzerland and Liechtenstein, most of them on ridges and summits above
//! 2000 m.
//!
//! The SLF's measurement service has a list of the stations and the values
//! of every station of the last 24 hours, a row a station and half hour. A
//! run asks each once. The service takes no area: the list and the values
//! are of every station, the adapter leaves out the stations outside the
//! run's area, and the pipeline leaves out their values and says so.
//!
//! What a value is, is the service's own description
//! (`https://measurement-api.slf.ch/openapi.json`, read 2026-11-22; a copy
//! is in `tests/fixtures`):
//!
//! - `VW_30MIN_MEAN`: "wind speed, vectorial mean over last 30 minutes, m/s"
//! - `VW_30MIN_MAX`: "wind speed, max 5sec measurement within last 30
//!   minutes, m/s", the gust
//! - `DW_30MIN_MEAN`: "wind direction, vectorial mean over last 30 minutes, °"
//! - `TA_30MIN_MEAN`: "air temperature, mean over last 30 minute, °C"
//! - `RH_30MIN_MEAN`: "relative humidity, mean over last 30 minutes, %"
//! - `measure_date`: "timestamp of measurement in UTC"
//!
//! Every unit is the canonical one, so no value is converted. Since every
//! value is a mean "over last 30 minutes", a row's time is the end of its
//! half hour. The service has no pressure and no direction of the gust. The
//! snow's height and temperatures, the radiation and the spread of the
//! direction are not taken.
//!
//! A station's type (`WIND`, `SNOW_FLAT`, `SNOW_SLOPE`, `FLOWCAPT`) does not
//! say what it measures: on 2026-11-22 most stations of every type had an
//! anemometer and a thermometer, and some of each had a value of one and not
//! of the other. So every station is taken, and of a row whatever it holds.

pub mod fixtures;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

const SERVICE: &str = "https://measurement-api.slf.ch/public/api/imis";

/// A value of a row and the observation field it fills. The units are the
/// service's and the canonical ones: m/s, degrees, °C and percent.
struct Field {
    /// The service's name of the value.
    name: &'static str,
    set: fn(&mut Observation, f64),
}

const FIELDS: [Field; 5] = [
    Field {
        name: "VW_30MIN_MEAN",
        set: |o, v| o.wind_speed_ms = Some(v),
    },
    Field {
        name: "VW_30MIN_MAX",
        set: |o, v| o.wind_gust_ms = Some(v),
    },
    Field {
        name: "DW_30MIN_MEAN",
        set: |o, v| o.wind_direction_deg = Some(v),
    },
    Field {
        name: "TA_30MIN_MEAN",
        set: |o, v| o.temperature_c = Some(v),
    },
    Field {
        name: "RH_30MIN_MEAN",
        set: |o, v| o.humidity_pct = Some(v),
    },
];

/// The address of the list of stations.
pub fn stations_url() -> String {
    format!("{SERVICE}/stations")
}

/// The address of every station's values of the last 24 hours.
pub fn measurements_url() -> String {
    format!("{SERVICE}/measurements")
}

/// The SLF's IMIS stations as an observation source.
#[derive(Debug, Clone)]
pub struct SlfImis {
    manifest: Manifest,
}

impl SlfImis {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-slf is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(SlfImis::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for SlfImis {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for SlfImis {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![
            Request::new("stations", stations_url()),
            Request::new("measurements", measurements_url()),
        ]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "stations" => parse_stations(&self.manifest.id, body, ctx),
            "measurements" => parse_measurements(&self.manifest.id, body),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// What is read of a station. The service documents the height as always
/// there; a station without one is taken all the same, without a height.
#[derive(Deserialize)]
struct StationEntry {
    code: String,
    label: String,
    lon: f64,
    lat: f64,
    #[serde(default)]
    elevation: Option<f64>,
}

fn parse_stations(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let entries: Vec<serde_json::Value> = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the station list is not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    for value in entries {
        let entry = match StationEntry::deserialize(&value) {
            Ok(entry) => entry,
            Err(error) => {
                let what = value
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| value.to_string(), str::to_owned);
                parsed.skipped.push(Skipped::new(
                    &what,
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
                    name: entry.label.clone(),
                    position,
                    elevation_m: entry.elevation,
                })
            });
        match station {
            Ok(station) if ctx.area.contains(station.position) => parsed.stations.push(station),
            Ok(_) => {}
            Err(reason) => parsed.skipped.push(Skipped::new(&entry.code, reason)),
        }
    }
    Ok(parsed)
}

fn parse_measurements(source: &SourceId, body: &[u8]) -> Result<Parsed, SourceError> {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the measurements are not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    for row in rows {
        let station = row
            .get("station_code")
            .and_then(serde_json::Value::as_str)
            .filter(|code| !code.is_empty())
            .and_then(|code| StationId::new(source.clone(), code).ok());
        let Some(station) = station else {
            parsed
                .skipped
                .push(Skipped::new(&row.to_string(), "a row without a station"));
            continue;
        };
        let date = row.get("measure_date").and_then(serde_json::Value::as_str);
        let Some(time) = date.and_then(parse_time) else {
            parsed.skipped.push(Skipped::new(
                &format!("{} {}", station.local(), date.unwrap_or("without a date")),
                "time stamp cannot be read",
            ));
            continue;
        };
        let mut observation = Observation::empty(station, time);
        for field in &FIELDS {
            match row.get(field.name) {
                None | Some(serde_json::Value::Null) => {}
                Some(value) => match value.as_f64() {
                    Some(number) => (field.set)(&mut observation, number),
                    // One value that cannot be read costs that value only.
                    None => parsed.skipped.push(Skipped::new(
                        &format!(
                            "{} {} {}",
                            observation.station.local(),
                            date.unwrap_or_default(),
                            field.name
                        ),
                        format!("value is not a number: {value}"),
                    )),
                },
            }
        }
        // A row that holds only the snow's values.
        if !observation.is_empty() {
            parsed.observations.push(observation);
        }
    }
    Ok(parsed)
}

/// Reads a time stamp of the service, `2026-11-22T07:30:00Z`.
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
    fn reads_the_services_time_stamps_as_utc() {
        let at = |hour, minute| Utc.with_ymd_and_hms(2026, 11, 22, hour, minute, 0).unwrap();
        assert_eq!(parse_time("2026-11-22T07:30:00Z"), Some(at(7, 30)));
        // Another offset is converted, should the service ever write one.
        assert_eq!(parse_time("2026-11-22T08:30:00+01:00"), Some(at(7, 30)));
        assert_eq!(parse_time("2026-11-22 07:30"), None);
        assert_eq!(parse_time("2026-11-22T07:30:00"), None);
        assert_eq!(parse_time(""), None);
    }
}
