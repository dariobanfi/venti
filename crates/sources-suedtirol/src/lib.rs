//! Adapter for the weather stations of the Autonomous Province of Bolzano -
//! South Tyrol, which its Agency for Civil Protection runs.
//!
//! The province's web service has three parts. A run asks two of them, each
//! once: the list of stations, and the newest value of every sensor of
//! every station. The third, a station's time series, is not used.
//!
//! The list holds every station of the service, and more than half of them
//! measure water: river gauges, lakes and groundwater. What a station is
//! shows in the last two letters of its code. That is not in the service's
//! documentation; it was read off the answers of 2026-11-21, in which every
//! station ending in `MS`, `SF` or `WS` had a thermometer and most had an
//! anemometer, and no other had either. To verify. Should a station of
//! another ending measure wind, its values arrive without a station and the
//! pipeline reports them, run after run, so the rule cannot fail silently.
//!
//! The values come as one row per station and sensor. Three things in them
//! are as the service has them and not faults:
//!
//! - A sensor that has no value has a row without a date and a value.
//! - A sensor can have more than one row. On 2026-11-21 one thermometer
//!   was in the answer twice with the same time and value, and seven
//!   sunshine sensors, which are not read here, had a row of 30 July beside
//!   the current one. The service calls the answer the "last value" of each
//!   sensor, so the newest row is taken and any other left out.
//! - The values are of every station, and the service takes no list of
//!   stations to answer for. The values of a station outside the run's area
//!   reach the pipeline, which leaves them out and says so.
//!
//! The pressure the service gives (`LD.RED`) is reduced to sea level: on
//! 2026-11-21 it was between 1015 and 1023 hPa at every one of 59 stations
//! from 212 m to 2055 m. An observation's pressure is the pressure at the
//! station's own height, so it is not put there: it goes into the field of
//! its own, `pressure_sea_level_hpa`, which the foehn reading compares. How
//! the province reduces it is not documented: to verify.
//!
//! Times are local time with the zone written as `CET` or `CEST`. The
//! documentation calls the wind speed and direction means and does not say
//! over which interval, nor whether a time marks its start or its end. The
//! time is treated as the end, as for the other sources. To verify.

pub mod fixtures;

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

const SERVICE: &str = "https://geoservices.buergernetz.bz.it/services/meteo/v1";

/// The endings of the codes of the stations that measure weather: see the
/// crate's documentation.
const WEATHER_STATIONS: [&str; 3] = ["MS", "SF", "WS"];

/// A sensor of the service and the observation field it fills.
struct Sensor {
    /// The service's code for the sensor.
    code: &'static str,
    /// The unit the service documents for it and writes into every row.
    unit: &'static str,
    set: fn(&mut Observation, f64),
}

const SENSORS: [Sensor; 6] = [
    Sensor {
        code: "WG",
        unit: "m/s",
        set: |o, v| o.wind_speed_ms = Some(v),
    },
    Sensor {
        code: "WG.BOE",
        unit: "m/s",
        set: |o, v| o.wind_gust_ms = Some(v),
    },
    Sensor {
        code: "WR",
        unit: "°",
        set: |o, v| o.wind_direction_deg = Some(v),
    },
    Sensor {
        code: "LT",
        unit: "°C",
        set: |o, v| o.temperature_c = Some(v),
    },
    Sensor {
        code: "LF",
        unit: "%",
        set: |o, v| o.humidity_pct = Some(v),
    },
    Sensor {
        code: "LD.RED",
        unit: "hPa",
        set: |o, v| o.pressure_sea_level_hpa = Some(v),
    },
];

/// The address of the list of stations.
pub fn stations_url() -> String {
    format!("{SERVICE}/stations")
}

/// The address of the newest value of every sensor.
pub fn sensors_url() -> String {
    format!("{SERVICE}/sensors")
}

/// The weather stations of South Tyrol as an observation source.
#[derive(Debug, Clone)]
pub struct SuedtirolMeteo {
    manifest: Manifest,
}

impl SuedtirolMeteo {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-suedtirol is valid");
        Self { manifest }
    }
}

impl Default for SuedtirolMeteo {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for SuedtirolMeteo {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![
            Request::new("stations", stations_url()),
            Request::new("sensors", sensors_url()),
        ]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "stations" => parse_stations(&self.manifest.id, body, ctx),
            "sensors" => parse_sensors(&self.manifest.id, body),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

#[derive(Deserialize)]
struct StationList {
    features: Vec<serde_json::Value>,
}

/// What is read of a station. The service gives the position twice, in the
/// projection asked for and in degrees; the degrees are taken. Of the four
/// names, German, Italian, Ladin and English, the German one is taken: a
/// station has one name, and German is a language of the region the source
/// is collected for.
#[derive(Deserialize)]
struct StationProperties {
    #[serde(rename = "SCODE")]
    code: String,
    #[serde(rename = "NAME_D")]
    name: String,
    #[serde(rename = "ALT")]
    altitude: Option<f64>,
    #[serde(rename = "LONG")]
    lon: f64,
    #[serde(rename = "LAT")]
    lat: f64,
}

fn parse_stations(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let list: StationList = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the station list is not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    for feature in list.features {
        let Some(properties) = feature.get("properties") else {
            parsed.skipped.push(Skipped::new(
                &feature.to_string(),
                "a station without facts",
            ));
            continue;
        };
        let entry = match StationProperties::deserialize(properties) {
            Ok(entry) => entry,
            Err(error) => {
                parsed.skipped.push(Skipped::new(
                    &properties.to_string(),
                    format!("station cannot be read: {error}"),
                ));
                continue;
            }
        };
        if !WEATHER_STATIONS
            .iter()
            .any(|ending| entry.code.ends_with(ending))
        {
            continue;
        }
        let station = GeoPoint::new(entry.lat, entry.lon)
            .map_err(|e| e.to_string())
            .and_then(|position| {
                let id = StationId::new(source.clone(), entry.code.clone())
                    .map_err(|e| e.to_string())?;
                Ok(Station {
                    id,
                    name: entry.name.clone(),
                    position,
                    elevation_m: entry.altitude,
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

/// One sensor of one station with its newest value. A sensor that has no
/// value has neither date nor value.
#[derive(Deserialize)]
struct Row {
    #[serde(rename = "SCODE")]
    station: String,
    #[serde(rename = "UNIT", default)]
    unit: Option<String>,
    #[serde(rename = "DATE", default)]
    date: Option<String>,
    #[serde(rename = "VALUE", default)]
    value: Option<f64>,
}

fn parse_sensors(source: &SourceId, body: &[u8]) -> Result<Parsed, SourceError> {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the values are not the expected JSON: {e}"))
    })?;

    let mut parsed = Parsed::default();
    // The newest value of each sensor of each station, by station and by
    // the sensor's place in `SENSORS`.
    let mut newest: BTreeMap<(String, usize), (Timestamp, f64)> = BTreeMap::new();
    for row in rows {
        let Some(code) = row.get("TYPE").and_then(serde_json::Value::as_str) else {
            parsed
                .skipped
                .push(Skipped::new(&row.to_string(), "a row without a sensor"));
            continue;
        };
        // Water levels, rain, snow and sunshine.
        let Some(index) = SENSORS.iter().position(|sensor| sensor.code == code) else {
            continue;
        };
        let sensor = &SENSORS[index];
        let entry = match Row::deserialize(&row) {
            Ok(entry) => entry,
            Err(error) => {
                let station = row.get("SCODE").and_then(serde_json::Value::as_str);
                let what = station.map_or_else(|| row.to_string(), |s| format!("{s} {code}"));
                parsed
                    .skipped
                    .push(Skipped::new(&what, format!("row cannot be read: {error}")));
                continue;
            }
        };
        let Some(value) = entry.value else {
            continue;
        };
        let what = format!("{} {code}", entry.station);
        if entry.station.is_empty() {
            parsed
                .skipped
                .push(Skipped::new(&what, "a value without a station"));
            continue;
        }
        // A value in another unit than the documented one would be stored as
        // a wrong wind. It is left out and reported instead.
        let unit = entry.unit.as_deref().map_or("", str::trim);
        if unit != sensor.unit {
            parsed.skipped.push(Skipped::new(
                &what,
                format!("the unit is {unit:?}, not {:?}", sensor.unit),
            ));
            continue;
        }
        let Some(time) = entry.date.as_deref().and_then(parse_time) else {
            parsed.skipped.push(Skipped::new(
                &format!(
                    "{what} {}",
                    entry.date.as_deref().unwrap_or("without a date")
                ),
                "time stamp cannot be read",
            ));
            continue;
        };
        let slot = newest
            .entry((entry.station, index))
            .or_insert((time, value));
        if time > slot.0 {
            *slot = (time, value);
        }
    }

    let mut observations: BTreeMap<(StationId, Timestamp), Observation> = BTreeMap::new();
    for ((station, index), (time, value)) in newest {
        let Ok(station) = StationId::new(source.clone(), station) else {
            // Ruled out above: a row without a station is not kept.
            continue;
        };
        let observation = observations
            .entry((station.clone(), time))
            .or_insert_with(|| Observation::empty(station, time));
        (SENSORS[index].set)(observation, value);
    }
    parsed.observations.extend(observations.into_values());
    Ok(parsed)
}

/// Reads a time stamp of the service: local time with the zone's name at the
/// end, `2026-11-21T11:20:00CET` in winter and `2026-07-30T15:20:00CEST` in
/// summer.
fn parse_time(text: &str) -> Option<Timestamp> {
    let (local, hours_ahead) = if let Some(local) = text.strip_suffix("CEST") {
        (local, 2)
    } else {
        (text.strip_suffix("CET")?, 1)
    };
    let local = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(Utc.from_utc_datetime(&(local - Duration::hours(hours_ahead))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_winter_and_summer_time() {
        let at = |day, hour, minute| {
            Utc.with_ymd_and_hms(2026, if day == 30 { 7 } else { 11 }, day, hour, minute, 0)
                .unwrap()
        };
        assert_eq!(parse_time("2026-11-21T11:20:00CET"), Some(at(21, 10, 20)));
        assert_eq!(parse_time("2026-07-30T15:20:00CEST"), Some(at(30, 13, 20)));
        // Half past midnight local time is the day before in UTC.
        assert_eq!(parse_time("2026-11-21T00:30:00CET"), Some(at(20, 23, 30)));
        assert_eq!(parse_time("2026-11-21T11:20:00"), None);
        assert_eq!(parse_time("2026-11-21T11:20:00MEZ"), None);
        assert_eq!(parse_time("2026-11-21 11:20 CET"), None);
        assert_eq!(parse_time("CET"), None);
    }
}
