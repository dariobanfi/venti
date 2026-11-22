//! Adapter for the TAWES 10-minute station observations of GeoSphere Austria.
//!
//! The Data Hub API serves the newest values of many stations in one request.
//! A run takes two steps: the dataset's metadata, which lists the stations,
//! then one request for the newest values of every active station inside the
//! run's area. The provider allows 240 requests an hour, so the values are
//! never fetched station by station.
//!
//! The response carries one list of time stamps for all stations and, per
//! station and parameter, a list of values aligned with it. A station has
//! values only at its own newest time stamp; the other positions are null.
//!
//! The pressure comes twice: `P`, at the station's own height, and `PRED`,
//! "Reduzierter Luftdruck", which GeoSphere reduces to sea level itself.
//! `PRED` goes into a field of its own, `pressure_sea_level_hpa`, which the
//! foehn reading compares; Venti's own reduction of `P` from the height the
//! metadata lists read 3.7 hPa lower than `PRED` at Innsbruck's university
//! on 2026-11-22, because that height is not the barometer's and the
//! metadata names no other. By which method GeoSphere reduces is not on
//! the pages read that day: to verify.
//!
//! `PRED` is a pressure at sea level only at the lower stations. At the
//! higher ones it carries the height of a pressure surface instead, as a
//! weather report's group for a mountain station does: the metadata calls
//! `PGPM`, the geopotential, "gültiger Wert bei pred < 950 hPa". On
//! 2026-11-22 the 170 stations up to 890 m sent 1008.1 to 1015.7 hPa, and
//! the 68 from 909 m up sent 795.1 to 843.2, each exactly 700 (or 500)
//! plus a tenth of its `PGPM`: 840.3 beside 1403 gpm. Such a value is not
//! a pressure and is not taken ([`SEA_LEVEL_FROM_HPA`]). Two stations sent
//! `P` and no `PRED` at all.
//!
//! Whether a TAWES time stamp marks the start or the end of its interval is
//! not stated in the pages read on 2026-11-18. It is treated as the end, as
//! for the other sources. To verify.

pub mod fixtures;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

const DATASET: &str = "https://dataset.api.hub.geosphere.at/v1/station/current/tawes-v1-10min";

/// A TAWES parameter and the observation field it fills.
struct Parameter {
    name: &'static str,
    set: fn(&mut Observation, f64),
}

/// From this value a `PRED` is a pressure reduced to sea level, hPa. Below
/// it the station is a high one and the value encodes the height of a
/// pressure surface: the limit is the metadata's own, in its description
/// of `PGPM` (read 2026-11-22). No pressure at sea level in Austria has
/// been this low.
pub const SEA_LEVEL_FROM_HPA: f64 = 950.0;

const PARAMETERS: [Parameter; 8] = [
    Parameter {
        name: "FF",
        set: |o, v| o.wind_speed_ms = Some(v),
    },
    Parameter {
        name: "FFX",
        set: |o, v| o.wind_gust_ms = Some(v),
    },
    Parameter {
        name: "DD",
        set: |o, v| o.wind_direction_deg = Some(v),
    },
    Parameter {
        name: "DDX",
        set: |o, v| o.gust_direction_deg = Some(v),
    },
    Parameter {
        name: "TL",
        set: |o, v| o.temperature_c = Some(v),
    },
    Parameter {
        name: "RF",
        set: |o, v| o.humidity_pct = Some(v),
    },
    Parameter {
        name: "P",
        set: |o, v| o.pressure_hpa = Some(v),
    },
    Parameter {
        name: "PRED",
        set: |o, v| {
            if v >= SEA_LEVEL_FROM_HPA {
                o.pressure_sea_level_hpa = Some(v);
            }
        },
    },
];

/// The GeoSphere Austria TAWES observation source.
#[derive(Debug, Clone)]
pub struct Tawes10Min {
    manifest: Manifest,
}

impl Tawes10Min {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-geosphere is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(Tawes10Min::new())
        .with_sample(fixtures::recorded_at(), fixtures::recorded())
}

impl Default for Tawes10Min {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for Tawes10Min {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("metadata", format!("{DATASET}/metadata"))]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "metadata" => parse_metadata(&self.manifest.id, body, ctx),
            "current" => parse_current(&self.manifest.id, body),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

#[derive(Deserialize)]
struct Metadata {
    stations: Vec<MetadataStation>,
}

#[derive(Deserialize)]
struct MetadataStation {
    id: String,
    name: String,
    lat: f64,
    lon: f64,
    altitude: Option<f64>,
    #[serde(default)]
    is_active: bool,
}

fn parse_metadata(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let metadata: Metadata = serde_json::from_slice(body)
        .map_err(|e| SourceError::Malformed(format!("metadata is not the expected JSON: {e}")))?;

    let mut parsed = Parsed::default();
    for entry in metadata.stations {
        if !entry.is_active {
            continue;
        }
        let station = GeoPoint::new(entry.lat, entry.lon)
            .map_err(|e| e.to_string())
            .and_then(|position| {
                let id =
                    StationId::new(source.clone(), entry.id.clone()).map_err(|e| e.to_string())?;
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
            Err(reason) => parsed.skipped.push(Skipped::new(&entry.id, reason)),
        }
    }

    if !parsed.stations.is_empty() {
        let ids: Vec<&str> = parsed.stations.iter().map(|s| s.id.local()).collect();
        let parameters: Vec<&str> = PARAMETERS.iter().map(|p| p.name).collect();
        let url = format!(
            "{DATASET}?parameters={}&station_ids={}&output_format=geojson",
            parameters.join(","),
            ids.join(",")
        );
        parsed.follow_up.push(Request::new("current", url));
    }
    Ok(parsed)
}

#[derive(Deserialize)]
struct Current {
    timestamps: Vec<String>,
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    properties: Properties,
}

#[derive(Deserialize)]
struct Properties {
    station: String,
    parameters: BTreeMap<String, Series>,
}

#[derive(Deserialize)]
struct Series {
    data: Vec<Option<f64>>,
}

fn parse_current(source: &SourceId, body: &[u8]) -> Result<Parsed, SourceError> {
    let current: Current = serde_json::from_slice(body)
        .map_err(|e| SourceError::Malformed(format!("response is not the expected JSON: {e}")))?;

    let mut parsed = Parsed::default();
    let times: Vec<Option<Timestamp>> = current
        .timestamps
        .iter()
        .map(|text| {
            let time = parse_time(text);
            if time.is_none() {
                parsed
                    .skipped
                    .push(Skipped::new(text, "time stamp cannot be read"));
            }
            time
        })
        .collect();

    for feature in current.features {
        let properties = feature.properties;
        let station = match StationId::new(source.clone(), properties.station.clone()) {
            Ok(station) => station,
            Err(error) => {
                parsed
                    .skipped
                    .push(Skipped::new(&properties.station, error.to_string()));
                continue;
            }
        };

        let mut by_time: BTreeMap<Timestamp, Observation> = BTreeMap::new();
        for parameter in &PARAMETERS {
            let Some(series) = properties.parameters.get(parameter.name) else {
                continue;
            };
            if series.data.len() != times.len() {
                parsed.skipped.push(Skipped::new(
                    &format!("{station} {}", parameter.name),
                    format!(
                        "{} values for {} time stamps",
                        series.data.len(),
                        times.len()
                    ),
                ));
                continue;
            }
            for (time, value) in times.iter().zip(&series.data) {
                if let (Some(time), Some(value)) = (time, value) {
                    let observation = by_time
                        .entry(*time)
                        .or_insert_with(|| Observation::empty(station.clone(), *time));
                    (parameter.set)(observation, *value);
                }
            }
        }
        parsed.observations.extend(by_time.into_values());
    }
    Ok(parsed)
}

/// Reads a time stamp. The API writes them without seconds, for example
/// `2026-11-18T19:00+00:00`, which strict RFC 3339 parsers refuse.
fn parse_time(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(text)
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M%:z"))
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    /// A station's values at one time stamp, as the service answers.
    fn current(station: &str, parameters: &str) -> Vec<u8> {
        format!(
            r#"{{"timestamps": ["2026-11-22T09:00+00:00"], "features": [{{"properties":
                {{"station": "{station}", "parameters": {{{parameters}}}}}}}]}}"#
        )
        .into_bytes()
    }

    fn observation(station: &str, parameters: &str) -> Observation {
        let source = SourceId::new("geosphere-tawes").unwrap();
        let mut parsed = parse_current(&source, &current(station, parameters)).unwrap();
        assert!(parsed.skipped.is_empty());
        assert_eq!(parsed.observations.len(), 1);
        parsed.observations.remove(0)
    }

    #[test]
    fn the_reduced_pressure_is_taken_beside_the_stations_own() {
        // Innsbruck's university on 22 November 2026 at 09:00 UTC.
        let innsbruck = observation(
            "11320",
            r#""TL": {"data": [2.8]}, "P": {"data": [938.7]}, "PRED": {"data": [1011.5]}"#,
        );
        assert_eq!(innsbruck.pressure_hpa, Some(938.7));
        assert_eq!(innsbruck.pressure_sea_level_hpa, Some(1011.5));
    }

    #[test]
    fn a_station_without_a_pressure_at_sea_level_has_none() {
        // The Patscherkofel at 2251 m the same morning: its `PRED` is the
        // height of the 850 hPa surface, 1407 gpm, in a report's code.
        let patscherkofel = observation(
            "11126",
            r#""TL": {"data": [-3.0]}, "P": {"data": [764.8]}, "PRED": {"data": [840.7]}"#,
        );
        assert_eq!(patscherkofel.pressure_hpa, Some(764.8));
        assert_eq!(patscherkofel.pressure_sea_level_hpa, None);

        // Frohnleiten sent the pressure at the station and no reduced one
        // (at 09:10 UTC); a station may leave the parameter out altogether.
        for parameters in [
            r#""P": {"data": [961.9]}, "PRED": {"data": [null]}"#,
            r#""P": {"data": [961.9]}"#,
        ] {
            let frohnleiten = observation("11249", parameters);
            assert_eq!(frohnleiten.pressure_hpa, Some(961.9));
            assert_eq!(frohnleiten.pressure_sea_level_hpa, None);
        }
    }

    #[test]
    fn reads_time_stamps_with_and_without_seconds() {
        let expected = Utc.with_ymd_and_hms(2026, 11, 18, 19, 0, 0).unwrap();
        assert_eq!(parse_time("2026-11-18T19:00+00:00"), Some(expected));
        assert_eq!(parse_time("2026-11-18T19:00:00+00:00"), Some(expected));
        assert_eq!(parse_time("2026-11-18T20:00+01:00"), Some(expected));
        assert_eq!(parse_time("not-a-time"), None);
    }
}
