use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, ModelId, SourceId, Timestamp};

use super::{Attribution, ThermalKind, ThermalReason, ThermalVerdict, format_day};

/// Path of the forecast along a route.
pub const ROUTE_WEATHER_PATH: &str = "/v1/route/weather";

/// The most points one request of `GET /v1/route/weather` may name: a
/// route a pilot draws has a take-off, a few turn points and a goal, and
/// twenty-five of them written as the request writes them are some
/// 500 characters of a query string. Not a setting.
pub const ROUTE_POINTS_AT_MOST: usize = 25;

/// A request for the forecast along a route: its points in order and a
/// day of the thermal map.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteWeatherRequest {
    /// The route's points, in the order they are flown.
    pub points: Vec<GeoPoint>,
    /// The day, as the thermal map names it (`days[].date` of
    /// `GET /v1/thermals`).
    pub date: NaiveDate,
}

impl RouteWeatherRequest {
    /// The path and query string to GET: the points as `lat,lon` to five
    /// decimals, about a metre, joined by `;`.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        let points: Vec<String> = self
            .points
            .iter()
            .map(|point| format!("{},{}", decimals(point.lat()), decimals(point.lon())))
            .collect();
        format!(
            "{ROUTE_WEATHER_PATH}?date={}&points={}",
            format_day(self.date),
            points.join(";")
        )
    }
}

/// A coordinate to five decimals, without the zeros at its end.
fn decimals(degrees: f64) -> String {
    let text = format!("{degrees:.5}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Why the `points` of a route could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoutePointsError {
    /// No point at all.
    #[error("points is empty; give them as lat,lon;lat,lon")]
    Empty,
    /// More than [`ROUTE_POINTS_AT_MOST`].
    #[error("a route has at most {ROUTE_POINTS_AT_MOST} points; this one has {0}")]
    TooMany(usize),
    /// A point that is not `lat,lon` on the globe, by its number from 1.
    #[error("point {0} is not lat,lon in degrees on the globe")]
    NotAPoint(usize),
}

/// Reads the `points` of a request: `lat,lon` pairs joined by `;`.
pub fn parse_route_points(text: &str) -> Result<Vec<GeoPoint>, RoutePointsError> {
    if text.trim().is_empty() {
        return Err(RoutePointsError::Empty);
    }
    let parts: Vec<&str> = text.split(';').collect();
    if parts.len() > ROUTE_POINTS_AT_MOST {
        return Err(RoutePointsError::TooMany(parts.len()));
    }
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let (lat, lon) = part
                .split_once(',')
                .ok_or(RoutePointsError::NotAPoint(index + 1))?;
            let read = |value: &str| value.trim().parse::<f64>().ok();
            read(lat)
                .zip(read(lon))
                .and_then(|(lat, lon)| GeoPoint::new(lat, lon).ok())
                .ok_or(RoutePointsError::NotAPoint(index + 1))
        })
        .collect()
}

/// The answer of `GET /v1/route/weather`.
///
/// For each point of a route, and each daylight hour of one day, what the
/// server's thermal map holds for the cell the point lies in: the hour's
/// verdict and thermals, the wind at the ground with its gusts and at the
/// heights the map keeps, cloud and rain. It is read from the hours the
/// thermal map serves (`GET /v1/thermals/hours`), so it says the same as
/// the map and costs no request to the forecast's source.
///
/// Like the map it is a reading of a model for a square some ten
/// kilometres wide, one of the model's cells out of the twenty or so in
/// it: a valley's own wind is not in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteWeatherResponse {
    /// The source the model's values were fetched from. Its credit is in
    /// `attributions`.
    pub source: SourceId,
    /// The day, by the sun at the cells.
    pub date: NaiveDate,
    /// The weather model the day is read from, as the day of
    /// `GET /v1/thermals` names it.
    pub model: ModelId,
    /// When the newest run of that model began that the day holds hours
    /// of. Earlier hours of the day can be an earlier run's.
    pub run: Timestamp,
    /// Whether every cell has all of the day's daylight hours, as
    /// `days[].whole` of `GET /v1/thermals`.
    pub whole: bool,
    /// When the server worked the map out.
    pub computed_at: Timestamp,
    /// The heights above mean sea level the map keeps the wind at,
    /// metres, the lowest first.
    pub heights_m: Vec<i32>,
    /// The route's points, in the order they were asked for.
    pub points: Vec<RouteWeatherPoint>,
    /// The credit for the source, with what its licence asks a client to
    /// show.
    pub attributions: Vec<Attribution>,
}

/// What the map holds for one point of a route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteWeatherPoint {
    /// The point, as it was asked for.
    pub at: GeoPoint,
    /// The middle of the map's cell the point lies in. Left out for a
    /// point outside the map, which has no hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<GeoPoint>,
    /// The height of the day's model's ground in that cell, metres above
    /// mean sea level: the height its hours are judged at. Left out where
    /// the model gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_m: Option<i32>,
    /// The cell's daylight hours of the day that the map holds, oldest
    /// first. Empty outside the map.
    pub hours: Vec<RouteWeatherHour>,
}

/// One hour at one point of a route: what every layer of the map's hours
/// has for its cell. A value the model does not give is left out.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RouteWeatherHour {
    /// The moment the hour's values are for. Rain and gusts are those of
    /// the hour before it, as the model gives them.
    pub time: Timestamp,
    /// Whether the hour can be flown by the air alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<ThermalVerdict>,
    /// The worst thing the air has against the hour. Left out in a
    /// flyable hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<ThermalReason>,
    /// What the hour's thermals count as. Left out in an hour that is not
    /// flyable, and where there is no estimate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermals: Option<ThermalKind>,
    /// The climb to expect in the hour's thermals, m/s to a tenth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climb_ms: Option<f64>,
    /// How high the thermals go, metres above mean sea level to the
    /// nearest fifty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_m: Option<i32>,
    /// Whether that height is a base of cumulus; `false` in the blue and
    /// without thermals.
    #[serde(default)]
    pub cloud: bool,
    /// The wind's mean speed 10 m above the model's ground, whole km/h.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_kmh: Option<u16>,
    /// The direction it comes from, degrees to the nearest ten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_from_deg: Option<u16>,
    /// The strongest gust there in the hour, whole km/h.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gust_kmh: Option<u16>,
    /// The wind at those of the answer's `heights_m` that have one in this
    /// hour, the lowest first: a height under the model's ground or above
    /// its highest level has none and is left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aloft: Vec<RouteWindAloft>,
    /// The share of the sky covered by cloud at any height, whole percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_cover_pct: Option<u8>,
    /// The rain, showers and snow of the hour, millimetres of water to a
    /// tenth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precipitation_mm: Option<f64>,
}

/// The wind at one height above mean sea level, read between the model's
/// levels as the thermal map reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteWindAloft {
    /// The height, metres above mean sea level: one of `heights_m`.
    pub height_m: i32,
    /// The wind's mean speed, whole km/h.
    pub kmh: u16,
    /// The direction it comes from, degrees to the nearest ten. Left out
    /// where the model gives none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_deg: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    #[test]
    fn the_points_are_written_to_a_metre_and_read_back() {
        let request = RouteWeatherRequest {
            points: vec![
                point(47.6642, 11.7967),
                point(47.7, -0.000_001),
                point(-3.5, 11.9),
            ],
            date: NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(),
        };
        let path = request.path_and_query();
        assert_eq!(
            path,
            "/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.7,0;-3.5,11.9"
        );
        let points = path.split("points=").nth(1).unwrap();
        let read = parse_route_points(points).unwrap();
        assert_eq!(read.len(), 3);
        assert!((read[0].lat() - 47.6642).abs() < 1e-9);
    }

    #[test]
    fn points_that_are_none_are_refused_by_their_number() {
        assert_eq!(parse_route_points(""), Err(RoutePointsError::Empty));
        assert_eq!(
            parse_route_points("47.6,11.8;95,11.8"),
            Err(RoutePointsError::NotAPoint(2))
        );
        assert_eq!(
            parse_route_points("47.6;11.8"),
            Err(RoutePointsError::NotAPoint(1))
        );
        let many = vec!["47.6,11.8"; ROUTE_POINTS_AT_MOST + 1].join(";");
        assert_eq!(
            parse_route_points(&many),
            Err(RoutePointsError::TooMany(ROUTE_POINTS_AT_MOST + 1))
        );
        let enough = vec!["47.6,11.8"; ROUTE_POINTS_AT_MOST].join(";");
        assert_eq!(
            parse_route_points(&enough).unwrap().len(),
            ROUTE_POINTS_AT_MOST
        );
    }

    #[test]
    fn an_hour_leaves_out_what_the_model_does_not_give() {
        let text = r#"{
            "time": "2026-11-22T11:00:00Z", "verdict": "flyable",
            "thermals": "weak", "climb_ms": 0.4, "top_m": 1450, "cloud": false,
            "wind_kmh": 6, "wind_from_deg": 250, "gust_kmh": 14,
            "aloft": [{"height_m": 1500, "kmh": 11, "from_deg": 260}],
            "cloud_cover_pct": 35, "precipitation_mm": 0.0
        }"#;
        let hour: RouteWeatherHour = serde_json::from_str(text).unwrap();
        assert_eq!(hour.why, None);
        assert_eq!(hour.aloft[0].kmh, 11);
        assert_eq!(
            serde_json::to_value(&hour).unwrap(),
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );
    }
}
