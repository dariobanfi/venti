//! The tools that read live wind.
//!
//! Their data is on a Venti server, so they are answered in two steps:
//! [`request`] says what to ask the server for, and [`result`] turns the
//! server's answer into what the model reads. Whoever offers the tools does
//! the asking in between. The app sends an HTTP request. The server's own
//! connector has no use for a path to itself: it reads a call's arguments
//! with [`StationsCall`] or [`HistoryCall`], builds from its database the
//! answer the request would have got, and hands it to [`wind_stations`] or
//! [`station_history`], the functions the app's results come from too.
//!
//! The model gets wind in km/h, as pilots in the first region speak of it,
//! and directions both in degrees and as a compass point. Every reading
//! carries its age, and one that is too old says so.

use chrono::{DateTime, Duration, Timelike, Utc};
use serde_json::{Map, Value, json};
use venti_api_types::v1::{
    Attribution, ObservationsRequest, ObservationsResponse, Reading, StationsRequest,
    StationsResponse,
};
use venti_domain::{BoundingBox, GeoPoint, StationId};

use crate::{Tool, ToolError};

/// How far around a place `wind_stations` looks when the model does not say.
const DEFAULT_RADIUS_KM: f64 = 25.0;
/// The farthest it looks whatever the model says.
const MAX_RADIUS_KM: f64 = 100.0;
/// The most stations it lists. The nearest are the ones that matter, and
/// the pilot pays for every token.
const MAX_STATIONS: usize = 15;

/// How many hours `station_history` covers when the model does not say.
const DEFAULT_HOURS: i64 = 6;
/// The most it covers: what the server keeps ready for one request.
const MAX_HOURS: i64 = 24;

/// A reading older than this many minutes says nothing about the wind now.
/// The same line the map draws.
const TOO_OLD_MIN: i64 = 60;

/// The tools that need a Venti server.
pub const SERVER_TOOLS: [Tool; 2] = [
    Tool {
        name: "wind_stations",
        description: "Live wind measured by weather stations around a place. For each station: \
            its name and id, how far and in which direction it lies from the place, its height \
            above sea level, mean wind and gusts in km/h, where the wind comes from, the \
            temperature, and how many minutes ago it was measured. Stations are listed nearest \
            first. Give the place as coordinates; you may use your own knowledge of where a \
            mountain, town or flying site is. A station measures the wind where it stands: \
            compare its height and position with the place asked about, and say how old a \
            reading is. A reading marked too_old says nothing about the wind now.",
        read_only: true,
        schema: r#"{
            "type": "object",
            "properties": {
                "latitude": {"type": "number", "minimum": -90, "maximum": 90,
                             "description": "Latitude of the place in decimal degrees, north positive."},
                "longitude": {"type": "number", "minimum": -180, "maximum": 180,
                              "description": "Longitude of the place in decimal degrees, east positive."},
                "radius_km": {"type": "number", "minimum": 1, "maximum": 100,
                              "description": "How far around the place to look, in kilometres. 25 if left out."}
            },
            "required": ["latitude", "longitude"],
            "additionalProperties": false
        }"#,
    },
    Tool {
        name: "station_history",
        description: "How the wind at one weather station developed over the last hours: for \
            each hour the mean wind and the strongest gust in km/h, where the wind came from, \
            and the temperature. Use it to see whether the wind is picking up, easing or \
            turning. Needs a station id from wind_stations.",
        read_only: true,
        schema: r#"{
            "type": "object",
            "properties": {
                "station_id": {"type": "string", "description": "The station's id, as wind_stations gives it."},
                "hours": {"type": "integer", "minimum": 1, "maximum": 24,
                          "description": "How many hours back to look. 6 if left out."}
            },
            "required": ["station_id"],
            "additionalProperties": false
        }"#,
    },
];

/// Whether a tool is one of [`SERVER_TOOLS`].
pub fn needs_server(name: &str) -> bool {
    SERVER_TOOLS.iter().any(|tool| tool.name == name)
}

/// What a tool needs from the server: the path and query to `GET`.
pub fn request(name: &str, input: &Value) -> Result<String, ToolError> {
    match name {
        "wind_stations" => Ok(StationsCall::read(input)?.request().path_and_query()),
        "station_history" => Ok(HistoryCall::read(input)?.request().path_and_query()),
        other => Err(ToolError::UnknownTool(other.to_owned())),
    }
}

/// What to tell the model when the server did not answer a tool's request:
/// its HTTP status and, if its answer had one, the code of its error.
pub fn refusal(status: u16, code: Option<&str>) -> ToolError {
    ToolError::Failed(match code {
        Some("station_not_found") => {
            "There is no station with this id. Use an id from wind_stations.".to_owned()
        }
        Some(code) => format!("The Venti server could not answer ({code})."),
        None => format!("The Venti server could not answer (status {status})."),
    })
}

/// The code of the error in a server's answer, if the answer is one of the
/// server's errors.
pub(crate) fn error_code(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|answer| {
            answer
                .get("error")?
                .get("code")?
                .as_str()
                .map(str::to_owned)
        })
}

/// The tool's result, as JSON text, from the server's answer to
/// [`request`]. `now` is the current time, which ages are measured against.
pub fn result(
    name: &str,
    input: &Value,
    status: u16,
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<String, ToolError> {
    if !(200..300).contains(&status) {
        return Err(refusal(status, error_code(body).as_deref()));
    }
    match name {
        "wind_stations" => {
            let call = StationsCall::read(input)?;
            let answer: StationsResponse = serde_json::from_slice(body).map_err(unreadable)?;
            Ok(wind_stations(&answer, call.place, call.radius_km, now).to_string())
        }
        "station_history" => {
            let call = HistoryCall::read(input)?;
            let answer: ObservationsResponse = serde_json::from_slice(body).map_err(unreadable)?;
            Ok(station_history(&answer, call.hours, now).to_string())
        }
        other => Err(ToolError::UnknownTool(other.to_owned())),
    }
}

/// What to tell the model about a server's answer that is not what the
/// tool expects.
pub(crate) fn unreadable(_: serde_json::Error) -> ToolError {
    ToolError::Failed("The Venti server's answer could not be read.".to_owned())
}

/// The stations around a place, nearest first, as the model reads them.
pub fn wind_stations(
    answer: &StationsResponse,
    place: GeoPoint,
    radius_km: f64,
    now: DateTime<Utc>,
) -> Value {
    // The server was asked for a box; the tool promises a circle.
    let mut near: Vec<(f64, &venti_api_types::v1::StationWithLatest)> = answer
        .stations
        .iter()
        .map(|entry| (place.distance_m(entry.station.position) / 1000.0, entry))
        .filter(|(distance_km, _)| *distance_km <= radius_km)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let left_out = near.len().saturating_sub(MAX_STATIONS);
    near.truncate(MAX_STATIONS);

    let stations: Vec<Value> = near
        .iter()
        .map(|(distance_km, entry)| {
            let mut station = Map::new();
            station.insert("station_id".to_owned(), json!(entry.station.id.to_string()));
            station.insert("name".to_owned(), json!(entry.station.name));
            station.insert("distance_km".to_owned(), json!(tenth(*distance_km)));
            // At the place itself there is no direction to speak of.
            if *distance_km >= 0.1 {
                station.insert(
                    "direction_from_place".to_owned(),
                    json!(compass(bearing_deg(place, entry.station.position))),
                );
            }
            if let Some(elevation) = entry.station.elevation_m {
                station.insert("elevation_m".to_owned(), json!(elevation.round()));
            }
            match &entry.latest {
                Some(reading) => station.extend(reading_fields(reading, now)),
                None => {
                    station.insert("no_reading".to_owned(), json!(true));
                }
            }
            Value::Object(station)
        })
        .collect();

    let mut result = Map::new();
    result.insert(
        "place".to_owned(),
        json!({"latitude": place.lat(), "longitude": place.lon()}),
    );
    result.insert("radius_km".to_owned(), json!(radius_km));
    result.insert("stations".to_owned(), json!(stations));
    if left_out > 0 || answer.truncated {
        result.insert(
            "note".to_owned(),
            json!("More stations lie in this area than are listed. Ask with a smaller radius to see the nearest in full."),
        );
    }
    result.insert("sources".to_owned(), sources(&answer.attributions));
    Value::Object(result)
}

/// One station's last hours, hour by hour, as the model reads them.
pub fn station_history(answer: &ObservationsResponse, hours: i64, now: DateTime<Utc>) -> Value {
    let since = now - Duration::hours(hours);
    let mut by_hour: Vec<(DateTime<Utc>, Vec<&Reading>)> = Vec::new();
    for reading in answer
        .observations
        .iter()
        .filter(|reading| reading.time > since)
    {
        let hour = reading
            .time
            .with_minute(0)
            .and_then(|time| time.with_second(0))
            .and_then(|time| time.with_nanosecond(0))
            .unwrap_or(reading.time);
        match by_hour.last_mut() {
            Some((last, readings)) if *last == hour => readings.push(reading),
            _ => by_hour.push((hour, vec![reading])),
        }
    }

    let rows: Vec<Value> = by_hour
        .iter()
        .map(|(hour, readings)| {
            let mut row = Map::new();
            row.insert(
                "hour_utc".to_owned(),
                json!(hour.format("%Y-%m-%d %H:00").to_string()),
            );
            if let Some(wind) = mean(readings.iter().filter_map(|reading| reading.wind_speed_ms)) {
                row.insert("wind_km_h".to_owned(), json!((wind * 3.6).round()));
            }
            let gusts = readings
                .iter()
                .filter_map(|reading| reading.wind_gust_ms)
                .fold(None, |strongest: Option<f64>, gust| {
                    Some(strongest.map_or(gust, |strongest| strongest.max(gust)))
                });
            if let Some(gusts) = gusts {
                row.insert(
                    "strongest_gust_km_h".to_owned(),
                    json!((gusts * 3.6).round()),
                );
            }
            if let Some(from) = mean_direction(readings) {
                row.insert("wind_from_deg".to_owned(), json!(from.round()));
                row.insert("wind_from".to_owned(), json!(compass(from)));
            }
            if let Some(temperature) =
                mean(readings.iter().filter_map(|reading| reading.temperature_c))
            {
                row.insert("temperature_c".to_owned(), json!(tenth(temperature)));
            }
            Value::Object(row)
        })
        .collect();

    let mut station = Map::new();
    station.insert(
        "station_id".to_owned(),
        json!(answer.station.id.to_string()),
    );
    station.insert("name".to_owned(), json!(answer.station.name));
    if let Some(elevation) = answer.station.elevation_m {
        station.insert("elevation_m".to_owned(), json!(elevation.round()));
    }

    let mut result = Map::new();
    result.insert("station".to_owned(), Value::Object(station));
    result.insert("hours".to_owned(), json!(rows));
    match answer.observations.last() {
        Some(latest) => {
            result.insert(
                "latest".to_owned(),
                Value::Object(reading_fields(latest, now)),
            );
        }
        None => {
            result.insert(
                "note".to_owned(),
                json!("The server has no readings from this station for the last day."),
            );
        }
    }
    result.insert("sources".to_owned(), sources(&answer.attributions));
    Value::Object(result)
}

/// A reading's values with their units in the names. What the station did
/// not measure is left out.
fn reading_fields(reading: &Reading, now: DateTime<Utc>) -> Map<String, Value> {
    let mut fields = Map::new();
    if let Some(wind) = reading.wind_speed_ms {
        fields.insert("wind_km_h".to_owned(), json!((wind * 3.6).round()));
    }
    if let Some(gusts) = reading.wind_gust_ms {
        fields.insert("gusts_km_h".to_owned(), json!((gusts * 3.6).round()));
    }
    if let Some(from) = reading.wind_direction_deg {
        fields.insert("wind_from_deg".to_owned(), json!(from.round()));
        fields.insert("wind_from".to_owned(), json!(compass(from)));
    }
    if let Some(temperature) = reading.temperature_c {
        fields.insert("temperature_c".to_owned(), json!(tenth(temperature)));
    }
    fields.insert(
        "measured_utc".to_owned(),
        json!(reading.time.format("%Y-%m-%d %H:%M").to_string()),
    );
    let age_min = (now - reading.time).num_minutes().max(0);
    fields.insert("age_min".to_owned(), json!(age_min));
    if age_min > TOO_OLD_MIN {
        fields.insert("too_old".to_owned(), json!(true));
    }
    fields
}

/// Whose data a result holds, with the licence. The model is told to name
/// the source; the licences ask for that.
pub(crate) fn sources(attributions: &[Attribution]) -> Value {
    json!(
        attributions
            .iter()
            .map(|attribution| format!("{} ({})", attribution.text, attribution.licence))
            .collect::<Vec<_>>()
    )
}

/// A call of `wind_stations`, with its arguments read and bounded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationsCall {
    /// The place asked about.
    pub place: GeoPoint,
    /// How far around it to look, in kilometres.
    pub radius_km: f64,
}

impl StationsCall {
    /// Reads the arguments the model gave.
    pub fn read(input: &Value) -> Result<Self, ToolError> {
        let (place, radius_km) = place_and_radius(input, DEFAULT_RADIUS_KM, MAX_RADIUS_KM)?;
        Ok(Self { place, radius_km })
    }

    /// What the call asks the server for: the stations of the box that
    /// holds the circle. [`wind_stations`] cuts the answer to the circle.
    pub fn request(&self) -> StationsRequest {
        StationsRequest {
            bbox: box_around(self.place, self.radius_km),
        }
    }
}

/// A call of `station_history`, with its arguments read and bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCall {
    /// The station asked about.
    pub station: StationId,
    /// How many hours back to look.
    pub hours: i64,
}

impl HistoryCall {
    /// Reads the arguments the model gave.
    pub fn read(input: &Value) -> Result<Self, ToolError> {
        let station = input["station_id"]
            .as_str()
            .ok_or_else(|| ToolError::BadArguments("station_id is required".to_owned()))?
            .parse()
            .map_err(|_| {
                ToolError::BadArguments(
                    "station_id is not a station's id; use one from wind_stations".to_owned(),
                )
            })?;
        let hours = match &input["hours"] {
            Value::Null => DEFAULT_HOURS,
            hours => hours
                .as_i64()
                .filter(|hours| *hours >= 1)
                .map(|hours| hours.min(MAX_HOURS))
                .ok_or_else(|| {
                    ToolError::BadArguments("hours must be a whole number from 1".to_owned())
                })?,
        };
        Ok(Self { station, hours })
    }

    /// What the call asks the server for: the station's last day, which is
    /// the server's default window. [`station_history`] cuts the answer to
    /// the hours asked for.
    pub fn request(&self) -> ObservationsRequest {
        ObservationsRequest {
            station: self.station.clone(),
            from: None,
            to: None,
        }
    }
}

/// Reads a place and a radius around it from a tool's arguments. A radius
/// that is left out is `default_km`, and none is larger than `max_km`.
pub(crate) fn place_and_radius(
    input: &Value,
    default_km: f64,
    max_km: f64,
) -> Result<(GeoPoint, f64), ToolError> {
    let number = |name: &str| {
        input[name]
            .as_f64()
            .ok_or_else(|| ToolError::BadArguments(format!("{name} must be a number")))
    };
    let place = GeoPoint::new(number("latitude")?, number("longitude")?).map_err(|_| {
        ToolError::BadArguments(
            "latitude must be between -90 and 90, longitude between -180 and 180".to_owned(),
        )
    })?;
    let radius_km = match &input["radius_km"] {
        Value::Null => default_km,
        radius => radius
            .as_f64()
            .filter(|radius| radius.is_finite() && *radius > 0.0)
            .map(|radius| radius.min(max_km))
            .ok_or_else(|| {
                ToolError::BadArguments("radius_km must be a number above 0".to_owned())
            })?,
    };
    Ok((place, radius_km))
}

/// The box that holds the circle of a radius around a place.
pub(crate) fn box_around(place: GeoPoint, radius_km: f64) -> BoundingBox {
    const KM_PER_DEGREE: f64 = 111.32;
    let north_south = radius_km / KM_PER_DEGREE;
    // Degrees of longitude shrink towards the poles; near them the box
    // simply spans the globe.
    let narrowing = place.lat().to_radians().cos();
    let east_west = if narrowing < 0.05 {
        360.0
    } else {
        radius_km / (KM_PER_DEGREE * narrowing)
    };
    BoundingBox::new(
        (place.lat() - north_south).max(-90.0),
        (place.lon() - east_west).max(-180.0),
        (place.lat() + north_south).min(90.0),
        (place.lon() + east_west).min(180.0),
    )
    .expect("a box around a valid point, clamped to the globe, is valid")
}

/// The direction from one point to another, degrees clockwise from north.
fn bearing_deg(from: GeoPoint, to: GeoPoint) -> f64 {
    let (lat1, lat2) = (from.lat().to_radians(), to.lat().to_radians());
    let along = (to.lon() - from.lon()).to_radians();
    let east = along.sin() * lat2.cos();
    let north = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * along.cos();
    east.atan2(north).to_degrees().rem_euclid(360.0)
}

/// A direction as one of sixteen compass points.
pub(crate) fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    POINTS[((degrees.rem_euclid(360.0) + 11.25) / 22.5) as usize % 16]
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, count) = values.fold((0.0, 0u32), |(sum, count), value| (sum + value, count + 1));
    (count > 0).then(|| sum / f64::from(count))
}

/// The direction the wind mostly came from over some readings: the
/// direction of the summed wind, so that 350 and 10 degrees average to
/// north and a strong wind counts for more than a light one.
fn mean_direction(readings: &[&Reading]) -> Option<f64> {
    let (east, north) = readings
        .iter()
        .filter_map(|reading| {
            let from = reading.wind_direction_deg?.to_radians();
            let speed = reading.wind_speed_ms.unwrap_or(1.0);
            Some((speed * from.sin(), speed * from.cos()))
        })
        .fold((0.0, 0.0), |(east, north), (e, n)| (east + e, north + n));
    // Winds that cancel out have no direction. Whole degrees, so that a
    // hair west of north is 0 and not 360.
    (east.hypot(north) > 1e-6).then(|| east.atan2(north).to_degrees().round().rem_euclid(360.0))
}

pub(crate) fn tenth(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    const STATIONS: &str = r#"{
        "stations": [
            {"id": "dwd-10min:01550", "source": "dwd-10min", "name": "Garmisch-Partenkirchen",
             "position": {"lat": 47.483, "lon": 11.0621}, "elevation_m": 719.0,
             "latest": {"time": "2026-11-18T14:00:00Z", "wind_speed_ms": 1.3, "wind_direction_deg": 350.0}},
            {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
             "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.4,
             "latest": {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7,
                        "wind_direction_deg": 260.0, "temperature_c": -7.13}},
            {"id": "geosphere-tawes:11126", "source": "geosphere-tawes", "name": "Patscherkofel",
             "position": {"lat": 47.2089, "lon": 11.4622}, "elevation_m": 2251.0},
            {"id": "geosphere-tawes:11343", "source": "geosphere-tawes", "name": "Sonnblick",
             "position": {"lat": 47.0542, "lon": 12.9575}, "elevation_m": 3109.0,
             "latest": {"time": "2026-11-18T19:00:00Z", "wind_speed_ms": 5.0}}
        ],
        "truncated": false,
        "attributions": [
            {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
             "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
        ]
    }"#;

    #[test]
    fn a_place_and_radius_become_a_box_on_the_server() {
        // Garmisch, 25 km: a quarter of a degree north and south, a third
        // east and west at this latitude.
        let path = request(
            "wind_stations",
            &json!({"latitude": 47.5, "longitude": 11.1}),
        )
        .unwrap();
        let bbox = path.strip_prefix("/v1/stations?bbox=").unwrap();
        let edges: Vec<f64> = bbox.split(',').map(|edge| edge.parse().unwrap()).collect();
        assert!((edges[0] - (11.1 - 0.3324)).abs() < 0.001, "{bbox}");
        assert!((edges[1] - (47.5 - 0.2246)).abs() < 0.001, "{bbox}");
        assert!((edges[2] - (11.1 + 0.3324)).abs() < 0.001, "{bbox}");
        assert!((edges[3] - (47.5 + 0.2246)).abs() < 0.001, "{bbox}");

        // No radius reaches further than 100 km, and the box never leaves the globe.
        let far = request(
            "wind_stations",
            &json!({"latitude": 89.9, "longitude": 179.0, "radius_km": 100000}),
        )
        .unwrap();
        assert!(far.starts_with("/v1/stations?bbox=-180,89.0"), "{far}");
        assert!(far.ends_with(",180,90"), "{far}");

        assert_eq!(
            request("station_history", &json!({"station_id": "dwd-10min:05792"})).unwrap(),
            "/v1/stations/dwd-10min:05792/observations"
        );
    }

    #[test]
    fn bad_arguments_are_explained_before_anything_is_asked() {
        for input in [
            json!({}),
            json!({"latitude": 47.5}),
            json!({"latitude": "47.5", "longitude": 11.1}),
            json!({"latitude": 95, "longitude": 11.1}),
            json!({"latitude": 47.5, "longitude": 11.1, "radius_km": 0}),
            json!({"latitude": 47.5, "longitude": 11.1, "radius_km": "far"}),
        ] {
            assert!(
                matches!(
                    request("wind_stations", &input),
                    Err(ToolError::BadArguments(_))
                ),
                "{input}"
            );
        }
        for input in [
            json!({}),
            json!({"station_id": "no colon"}),
            json!({"station_id": "dwd-10min:05792", "hours": 0}),
        ] {
            assert!(
                matches!(
                    request("station_history", &input),
                    Err(ToolError::BadArguments(_))
                ),
                "{input}"
            );
        }
        assert!(matches!(
            request("get_flight", &json!({})),
            Err(ToolError::UnknownTool(_))
        ));
        assert!(needs_server("wind_stations"));
        assert!(!needs_server("get_flight"));
    }

    #[test]
    fn a_call_can_be_read_without_asking_anything() {
        // What a host that answers from its own data needs: the arguments,
        // bounded, and the request whose answer the result is made from.
        let call = StationsCall::read(&json!({"latitude": 47.5, "longitude": 11.1})).unwrap();
        assert_eq!(call.place, GeoPoint::new(47.5, 11.1).unwrap());
        assert_eq!(call.radius_km, 25.0);
        assert!(call.request().bbox.contains(call.place));
        let far = json!({"latitude": 47.5, "longitude": 11.1, "radius_km": 5000});
        assert_eq!(StationsCall::read(&far).unwrap().radius_km, 100.0);

        let call = HistoryCall::read(&json!({"station_id": "dwd-10min:05792"})).unwrap();
        assert_eq!(call.hours, 6);
        assert_eq!(call.request().station, call.station);
        let long = json!({"station_id": "dwd-10min:05792", "hours": 500});
        assert_eq!(HistoryCall::read(&long).unwrap().hours, 24);

        // The same refusals the app's model reads.
        assert!(
            refusal(404, Some("station_not_found"))
                .to_string()
                .contains("no station with this id")
        );
        assert_eq!(
            refusal(503, Some("unavailable")).to_string(),
            "The Venti server could not answer (unavailable)."
        );
        assert_eq!(
            refusal(502, None).to_string(),
            "The Venti server could not answer (status 502)."
        );
    }

    #[test]
    fn stations_are_listed_nearest_first_with_units_ages_and_sources() {
        let text = result(
            "wind_stations",
            &json!({"latitude": 47.45, "longitude": 11.0, "radius_km": 50}),
            200,
            STATIONS.as_bytes(),
            at("2026-11-18T19:05:00Z"),
        )
        .unwrap();
        let result: Value = serde_json::from_str(&text).unwrap();

        // Sonnblick is 150 km away: inside no 50 km circle.
        let names: Vec<&str> = result["stations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|station| station["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["Zugspitze", "Garmisch-Partenkirchen", "Patscherkofel"]
        );

        assert_eq!(
            result["stations"][0],
            json!({
                "station_id": "dwd-10min:05792",
                "name": "Zugspitze",
                "distance_km": 3.4,
                "direction_from_place": "SSW",
                "elevation_m": 2956.0,
                "wind_km_h": 54.0,
                "gusts_km_h": 93.0,
                "wind_from_deg": 260.0,
                "wind_from": "W",
                "temperature_c": -7.1,
                "measured_utc": "2026-11-18 18:20",
                "age_min": 45,
            })
        );
        // Five hours old, and it says so; what it did not measure is absent.
        let valley = &result["stations"][1];
        assert_eq!(valley["age_min"], 305);
        assert_eq!(valley["too_old"], true);
        assert!(valley.get("gusts_km_h").is_none());
        assert_eq!(valley["wind_from"], "N");
        // A station that never reported is listed as such.
        assert_eq!(result["stations"][2]["no_reading"], true);
        assert!(result["stations"][2].get("age_min").is_none());

        assert_eq!(
            result["sources"],
            json!(["Quelle: Deutscher Wetterdienst (CC BY 4.0)"])
        );
        assert!(result.get("note").is_none());
    }

    #[test]
    fn a_history_is_given_hour_by_hour() {
        let body = r#"{
            "station": {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                        "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0},
            "observations": [
                {"time": "2026-11-18T12:50:00Z", "wind_speed_ms": 30.0, "wind_gust_ms": 40.0, "wind_direction_deg": 180.0},
                {"time": "2026-11-18T17:10:00Z", "wind_speed_ms": 10.0, "wind_gust_ms": 15.0, "wind_direction_deg": 350.0, "temperature_c": -6.0},
                {"time": "2026-11-18T17:50:00Z", "wind_speed_ms": 10.0, "wind_gust_ms": 22.0, "wind_direction_deg": 10.0, "temperature_c": -7.0},
                {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7, "wind_direction_deg": 260.0}
            ],
            "attributions": [
                {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
                 "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
            ]
        }"#;
        let text = result(
            "station_history",
            &json!({"station_id": "dwd-10min:05792", "hours": 3}),
            200,
            body.as_bytes(),
            at("2026-11-18T19:05:00Z"),
        )
        .unwrap();
        let result: Value = serde_json::from_str(&text).unwrap();

        // The reading from noon is outside the three hours asked for.
        assert_eq!(
            result["hours"],
            json!([
                // 350 and 10 degrees average to north, not to south.
                {"hour_utc": "2026-11-18 17:00", "wind_km_h": 36.0, "strongest_gust_km_h": 79.0,
                 "wind_from_deg": 0.0, "wind_from": "N", "temperature_c": -6.5},
                {"hour_utc": "2026-11-18 18:00", "wind_km_h": 54.0, "strongest_gust_km_h": 93.0,
                 "wind_from_deg": 260.0, "wind_from": "W"},
            ])
        );
        assert_eq!(result["station"]["name"], "Zugspitze");
        assert_eq!(result["latest"]["age_min"], 45);
        assert_eq!(
            result["sources"][0],
            "Quelle: Deutscher Wetterdienst (CC BY 4.0)"
        );
    }

    #[test]
    fn a_server_that_refuses_is_explained_to_the_model() {
        let fail = |status: u16, body: &str| {
            result(
                "station_history",
                &json!({"station_id": "dwd-10min:99999"}),
                status,
                body.as_bytes(),
                at("2026-11-18T19:05:00Z"),
            )
            .unwrap_err()
            .to_string()
        };
        assert!(
            fail(
                404,
                r#"{"error": {"code": "station_not_found", "message": "x"}}"#
            )
            .contains("no station")
        );
        assert!(
            fail(503, r#"{"error": {"code": "unavailable", "message": "x"}}"#)
                .contains("unavailable")
        );
        assert!(fail(502, "<html>").contains("502"));
        assert!(fail(200, "not json").contains("could not be read"));
    }

    #[test]
    fn directions_are_named_by_compass_point() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(11.0), "N");
        assert_eq!(compass(12.0), "NNE");
        assert_eq!(compass(247.5), "WSW");
        assert_eq!(compass(359.0), "N");
        assert_eq!(compass(-90.0), "W");
        // Due east and due south of a point.
        let here = GeoPoint::new(47.0, 11.0).unwrap();
        assert!((bearing_deg(here, GeoPoint::new(47.0, 12.0).unwrap()) - 90.0).abs() < 1.0);
        assert!((bearing_deg(here, GeoPoint::new(46.0, 11.0).unwrap()) - 180.0).abs() < 0.001);
    }
}
