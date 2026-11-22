//! Adapter for the airfield weather reports (METAR) that the Aviation
//! Weather Center of the United States collects from the whole world and
//! publishes.
//!
//! A run is one request: the newest report of every airfield inside the
//! run's area. The answer carries each airfield's name, position and height
//! with its report, so a report is a station and an observation at once.
//!
//! What a METAR is worth to a paraglider is the wind at an airfield, which
//! lies in the flat of a valley or on the plain, and never at a take-off.
//! It is written for aircraft that land there:
//!
//! - The wind is the mean of the ten minutes before the report, in whole
//!   knots, and its direction in steps of ten degrees. A gust is reported
//!   only when it exceeds the mean by ten knots or more, so "no gust" does
//!   not mean a steady wind. (ICAO Annex 3; not read for this adapter. To
//!   verify.)
//! - The direction can be `VRB`, variable, which is no direction, and calm
//!   is written as direction 0 with speed 0, which is not north.
//! - The temperature is in whole degrees.
//! - The pressure is the altimeter setting, reduced to sea level. An
//!   observation's pressure is the pressure at the station's own height, so
//!   it is not taken. The humidity is not reported; the dew point is, and
//!   is not converted.
//! - A report comes every half hour, at a few airfields once an hour, and
//!   reached the service two to ten minutes after its time on 2026-11-18.
//!
//! The service documents `wspd` and `wgst` as knots whatever the report
//! itself is written in. No airfield in the first region reports in metres
//! per second, so that was not seen. To verify.

pub mod fixtures;

use chrono::DateTime;
use serde::Deserialize;
use venti_domain::{BoundingBox, GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_sources::{Manifest, ObservationSource, Parsed, PlanCtx, Request, Skipped, SourceError};

const SERVICE: &str = "https://aviationweather.gov/api/data/metar";

/// One knot, a nautical mile of 1852 m in an hour, in metres per second.
const KNOT_MS: f64 = 1852.0 / 3600.0;

/// The most reports the service gives in one answer.
const MOST_REPORTS: usize = 400;

/// The address of the newest reports inside `area`.
///
/// The service takes a box as south, west, north, east with at most three
/// decimals. A bound with more is moved outward, so that no airfield at the
/// edge is lost; the adapter leaves out what lies outside the area itself.
pub fn reports_url(area: &BoundingBox) -> String {
    format!(
        "{SERVICE}?bbox={},{},{},{}&format=json",
        bound(area.south(), false),
        bound(area.west(), false),
        bound(area.north(), true),
        bound(area.east(), true),
    )
}

/// A bound of the box with at most three decimals: the value itself if it
/// has no more, else the next such number below or above it.
fn bound(value: f64, up: bool) -> f64 {
    let thousandths = value * 1000.0;
    let nearest = thousandths.round();
    let thousandths = if (thousandths - nearest).abs() < 1e-6 {
        nearest
    } else if up {
        thousandths.ceil()
    } else {
        thousandths.floor()
    };
    thousandths / 1000.0
}

/// The world's airfield weather reports as an observation source.
#[derive(Debug, Clone)]
pub struct AwcMetar {
    manifest: Manifest,
}

impl AwcMetar {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-awc is valid");
        Self { manifest }
    }
}

/// The network as a build takes it in: the adapter, and the sample that
/// answers for it in the server's development mode. The server's list of
/// adapters has one line for this function and names nothing else of the
/// crate.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(AwcMetar::new())
        .with_sample(fixtures::evening_at(), fixtures::evening())
}

impl Default for AwcMetar {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationSource for AwcMetar {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("metar", reports_url(&ctx.area))]
    }

    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
        match request.tag.as_str() {
            "metar" => parse_reports(&self.manifest.id, body, ctx),
            _ => Err(SourceError::UnknownRequest(request.tag.clone())),
        }
    }
}

/// What is read of a report. `rawOb`, the report as the airfield wrote it,
/// is not read: the service has decoded it.
#[derive(Deserialize)]
struct Report {
    #[serde(rename = "icaoId")]
    airfield: String,
    name: String,
    lat: f64,
    lon: f64,
    #[serde(default)]
    elev: Option<f64>,
    /// The time of the observation, in seconds since 1970. `reportTime`,
    /// which the answer has too, is rounded to the half hour.
    #[serde(rename = "obsTime", default)]
    observed: Option<i64>,
    /// Degrees, or the word `VRB`.
    #[serde(default)]
    wdir: Option<serde_json::Value>,
    #[serde(default)]
    wspd: Option<f64>,
    #[serde(default)]
    wgst: Option<f64>,
    #[serde(default)]
    temp: Option<f64>,
}

fn parse_reports(source: &SourceId, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError> {
    let mut parsed = Parsed::default();
    // "Status code 204 is returned for valid requests with no data
    // available": no report in the area, which is an answer.
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(parsed);
    }
    let reports: Vec<serde_json::Value> = serde_json::from_slice(body)
        .map_err(|e| SourceError::Malformed(format!("answer is not the expected JSON: {e}")))?;
    if reports.len() >= MOST_REPORTS {
        parsed.skipped.push(Skipped::new(
            &format!("{} reports", reports.len()),
            "the service gives no more in one answer, so some are probably missing",
        ));
    }

    for value in reports {
        let named = value.get("icaoId").and_then(serde_json::Value::as_str);
        let what = named.map_or_else(|| value.to_string(), str::to_owned);
        let report = match Report::deserialize(&value) {
            Ok(report) => report,
            Err(error) => {
                parsed.skipped.push(Skipped::new(
                    &what,
                    format!("report cannot be read: {error}"),
                ));
                continue;
            }
        };
        let station = GeoPoint::new(report.lat, report.lon)
            .map_err(|e| e.to_string())
            .and_then(|position| {
                let id = StationId::new(source.clone(), report.airfield.clone())
                    .map_err(|e| e.to_string())?;
                Ok(Station {
                    id,
                    name: report.name.clone(),
                    position,
                    elevation_m: report.elev,
                })
            });
        let station = match station {
            Ok(station) if ctx.area.contains(station.position) => station,
            Ok(_) => continue,
            Err(reason) => {
                parsed.skipped.push(Skipped::new(&what, reason));
                continue;
            }
        };
        match report
            .observed
            .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        {
            Some(time) => parsed
                .observations
                .push(observation(&report, station.id.clone(), time)),
            None => parsed.skipped.push(Skipped::new(
                &what,
                "the time of the observation is missing or cannot be read",
            )),
        }
        parsed.stations.push(station);
    }
    Ok(parsed)
}

/// Knots as metres per second, to a hundredth: a whole knot is known to a
/// quarter of a metre per second at best.
fn metres_per_second(knots: f64) -> f64 {
    (knots * KNOT_MS * 100.0).round() / 100.0
}

fn observation(report: &Report, station: StationId, time: Timestamp) -> Observation {
    let mut observation = Observation::empty(station, time);
    observation.wind_speed_ms = report.wspd.map(metres_per_second);
    observation.wind_gust_ms = report.wgst.map(metres_per_second);
    // Calm is written as 00000KT, direction 0 with speed 0. North is 360.
    let calm = report.wspd.is_some_and(|knots| knots < 0.5);
    observation.wind_direction_deg = report
        .wdir
        .as_ref()
        .and_then(serde_json::Value::as_f64)
        .filter(|degrees| !(calm && *degrees < 0.5));
    observation.temperature_c = report.temp;
    observation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_is_asked_for_with_three_decimals_at_most() {
        let first_region = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        assert_eq!(
            reports_url(&first_region),
            "https://aviationweather.gov/api/data/metar?bbox=46.3,9.4,48.5,17.2&format=json"
        );
        // A finer bound moves outward.
        let fine = BoundingBox::new(46.30049, -9.40011, 48.50011, 17.19999).unwrap();
        assert_eq!(
            reports_url(&fine),
            "https://aviationweather.gov/api/data/metar?bbox=46.3,-9.401,48.501,17.2&format=json"
        );
    }

    #[test]
    fn a_knot_is_a_nautical_mile_an_hour() {
        assert!((10.0 * KNOT_MS - 5.144_444).abs() < 1e-6);
        assert!((metres_per_second(10.0) - 5.14).abs() < 1e-9);
        assert!((metres_per_second(17.0) - 8.75).abs() < 1e-9);
    }
}
