//! Live wind from the Venti server.
//!
//! The core never talks to the network. For each thing a screen wants from
//! the server there is a pair here: a function that says what to ask for,
//! and one that reads the answer into a view. The shell does the request in
//! between, against whichever server the pilot has set.
//!
//! The wire format is `venti-api-types`, the same crate the server builds
//! its answers from.

use chrono::{DateTime, Utc};
use venti_api_types::v1::{
    Attribution, HEALTH_PATH, HealthResponse, ObservationsRequest, ObservationsResponse, Reading,
    StationWithLatest, StationsRequest, StationsResponse,
};
use venti_domain::{BoundingBox, StationId};

use crate::server::answer;
use crate::{Coordinate, CoreError};

/// A reading older than this many minutes is shown as old. Ten-minute
/// stations whose provider publishes late are regularly 45 minutes behind;
/// beyond an hour a reading says little about the wind now.
const OLD_AFTER_MIN: i64 = 60;

/// One reading of a station, in the units the API uses.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindReading {
    /// End of the measuring interval, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// Mean wind speed, metres per second.
    pub speed_ms: Option<f64>,
    /// Strongest gust, metres per second.
    pub gust_ms: Option<f64>,
    /// The direction the wind comes from, degrees clockwise from north.
    pub direction_deg: Option<f64>,
    /// Air temperature, degrees Celsius.
    pub temperature_c: Option<f64>,
}

/// One station on the wind map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindStation {
    /// The station's identifier, to ask for its history.
    pub id: String,
    /// Its name. A name the provider publishes in capitals throughout is
    /// set in ordinary case; any other is as published.
    pub name: String,
    /// The data source it comes from; matches a [`Credit`]'s `source`.
    pub source: String,
    /// Where it stands.
    pub position: Coordinate,
    /// Its height above sea level, metres.
    pub elevation_m: Option<f64>,
    /// Its newest reading, if the server holds one.
    pub latest: Option<WindReading>,
    /// How old that reading is, in minutes.
    pub age_min: Option<i64>,
    /// Whether the reading is too old to stand for the wind now. The shell
    /// draws such a station greyed out.
    pub is_old: bool,
}

/// Whose data is shown. The licences require this credit next to the data.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Credit {
    /// The data source it belongs to.
    pub source: String,
    /// Who publishes the data.
    pub provider: String,
    /// The credit line, worded as the provider asks.
    pub text: String,
    /// The licence's name.
    pub licence: String,
    /// Where the credit links to.
    pub url: String,
}

/// The wind map of one area.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindMap {
    /// The stations in the area.
    pub stations: Vec<WindStation>,
    /// Whose data they are.
    pub credits: Vec<Credit>,
    /// Whether the area holds more stations than one answer lists; the
    /// pilot should zoom in.
    pub truncated: bool,
}

/// What one station measured over time.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationHistory {
    /// The station.
    pub station: WindStation,
    /// Its readings, oldest first.
    pub readings: Vec<WindReading>,
    /// Whose data they are.
    pub credits: Vec<Credit>,
}

/// How the server is doing.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ServerStatus {
    /// The server's version.
    pub version: String,
    /// The server's clock, seconds since 1970-01-01 UTC. The age of a
    /// reading is computed against this, not against the device's clock,
    /// which may be wrong.
    pub time_unix_s: i64,
    /// The providers whose data the server has not received lately. Their
    /// stations show old readings until the provider delivers again.
    pub late_providers: Vec<String>,
}

/// The path to `GET` to learn how the server is doing.
#[uniffi::export]
pub fn server_status_path() -> String {
    HEALTH_PATH.to_owned()
}

/// Reads the server's answer to [`server_status_path`].
#[uniffi::export]
pub fn server_status(status: u16, body: Vec<u8>) -> Result<ServerStatus, CoreError> {
    let response: HealthResponse = answer(status, &body)?;
    Ok(ServerStatus {
        version: response.version,
        time_unix_s: response.time.timestamp(),
        late_providers: response
            .sources
            .into_iter()
            .filter(|source| source.stale)
            .map(|source| source.provider)
            .collect(),
    })
}

/// The path and query to `GET` from the server for the wind map of a box.
#[uniffi::export]
pub fn wind_map_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, CoreError> {
    // A map can be dragged past the poles and the date line; ask for what
    // exists.
    let bbox = BoundingBox::new(
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    )
    .map_err(|_| unreadable("this part of the map"))?;
    Ok(StationsRequest { bbox }.path_and_query())
}

/// Reads the server's answer to [`wind_map_path`]. `now_unix_s` is the
/// current time by the server's clock (see [`ServerStatus`]).
#[uniffi::export]
pub fn wind_map(status: u16, body: Vec<u8>, now_unix_s: i64) -> Result<WindMap, CoreError> {
    let response: StationsResponse = answer(status, &body)?;
    Ok(WindMap {
        stations: response
            .stations
            .iter()
            .map(|station| wind_station(station, now_unix_s))
            .collect(),
        credits: response.attributions.iter().map(credit).collect(),
        truncated: response.truncated,
    })
}

/// The path and query to `GET` for a station's readings of the last 24
/// hours.
#[uniffi::export]
pub fn station_history_path(station_id: String) -> Result<String, CoreError> {
    let station: StationId = station_id.parse().map_err(|_| unreadable("this station"))?;
    Ok(ObservationsRequest {
        station,
        from: None,
        to: None,
    }
    .path_and_query())
}

/// Reads the server's answer to [`station_history_path`].
#[uniffi::export]
pub fn station_history(
    status: u16,
    body: Vec<u8>,
    now_unix_s: i64,
) -> Result<StationHistory, CoreError> {
    let response: ObservationsResponse = answer(status, &body)?;
    let latest = response.observations.last().cloned();
    Ok(StationHistory {
        station: wind_station(
            &StationWithLatest {
                station: response.station,
                latest,
            },
            now_unix_s,
        ),
        readings: response.observations.iter().map(reading).collect(),
        credits: response.attributions.iter().map(credit).collect(),
    })
}

fn unreadable(what: &str) -> CoreError {
    CoreError::Server {
        message: format!("The app could not ask the server about {what}."),
    }
}

fn reading(reading: &Reading) -> WindReading {
    WindReading {
        time_unix_s: reading.time.timestamp(),
        speed_ms: reading.wind_speed_ms,
        gust_ms: reading.wind_gust_ms,
        direction_deg: reading.wind_direction_deg,
        temperature_c: reading.temperature_c,
    }
}

fn wind_station(station: &StationWithLatest, now_unix_s: i64) -> WindStation {
    let now: DateTime<Utc> = DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default();
    let age_min = station
        .latest
        .as_ref()
        .map(|latest| (now - latest.time).num_minutes().max(0));
    WindStation {
        id: station.station.id.to_string(),
        name: readable_name(&station.station.name),
        source: station.station.source.to_string(),
        position: Coordinate {
            lat: station.station.position.lat(),
            lon: station.station.position.lon(),
        },
        elevation_m: station.station.elevation_m,
        latest: station.latest.as_ref().map(reading),
        age_min,
        is_old: age_min.is_none_or(|age| age > OLD_AFTER_MIN),
    }
}

/// Some providers publish station names in capitals throughout
/// ("PATSCHERKOFEL"), which shouts in a list. Such a name is set with a
/// capital only at the start of each word; a name that already has small
/// letters is left exactly as published.
fn readable_name(name: &str) -> String {
    if name.chars().any(char::is_lowercase) {
        return name.to_owned();
    }
    let mut readable = String::with_capacity(name.len());
    let mut in_word = false;
    for letter in name.chars() {
        if in_word {
            readable.extend(letter.to_lowercase());
        } else {
            readable.push(letter);
        }
        in_word = letter.is_alphabetic();
    }
    readable
}

fn credit(attribution: &Attribution) -> Credit {
    Credit {
        source: attribution.source.to_string(),
        provider: attribution.provider.clone(),
        text: attribution.text.clone(),
        licence: attribution.licence.clone(),
        url: attribution.url.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 18 November 2026, 19:05 UTC: the moment the server's development
    /// data was recorded.
    const NOW: i64 = 1_795_028_700;

    const STATIONS: &str = r#"{
        "stations": [
            {
                "id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0,
                "latest": {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7,
                           "wind_direction_deg": 260.0, "temperature_c": -7.1, "a_field_from_the_future": 1}
            },
            {
                "id": "geosphere-tawes:11121", "source": "geosphere-tawes", "name": "Innsbruck-Flughafen",
                "position": {"lat": 47.26, "lon": 11.357},
                "latest": {"time": "2026-11-18T16:00:00Z", "wind_speed_ms": 2.1}
            },
            {
                "id": "geosphere-tawes:11999", "source": "geosphere-tawes", "name": "Silent",
                "position": {"lat": 47.3, "lon": 11.4}
            }
        ],
        "truncated": false,
        "attributions": [
            {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "licence": "CC BY 4.0",
             "text": "Quelle: Deutscher Wetterdienst", "url": "https://www.dwd.de/"}
        ]
    }"#;

    #[test]
    fn the_wind_map_is_asked_for_by_box() {
        assert_eq!(
            wind_map_path(47.0, 10.9, 47.5, 11.5).unwrap(),
            "/v1/stations?bbox=10.9,47,11.5,47.5"
        );
        // A map dragged past the edge of the world still asks a valid question.
        assert_eq!(
            wind_map_path(80.0, 170.0, 95.0, 190.0).unwrap(),
            "/v1/stations?bbox=170,80,180,90"
        );
    }

    #[test]
    fn stations_are_read_with_their_age_and_fields_from_newer_servers_are_ignored() {
        let map = wind_map(200, STATIONS.as_bytes().to_vec(), NOW).unwrap();

        assert_eq!(map.stations.len(), 3);
        let zugspitze = &map.stations[0];
        assert_eq!(zugspitze.id, "dwd-10min:05792");
        assert_eq!(zugspitze.name, "Zugspitze");
        assert_eq!(zugspitze.elevation_m, Some(2956.0));
        let latest = zugspitze.latest.unwrap();
        assert_eq!(latest.speed_ms, Some(14.9));
        assert_eq!(latest.gust_ms, Some(25.7));
        assert_eq!(latest.direction_deg, Some(260.0));
        // Measured at 18:20, read at 19:05.
        assert_eq!(zugspitze.age_min, Some(45));
        assert!(!zugspitze.is_old);

        // Three hours old, and one that never reported: both shown as old.
        assert_eq!(map.stations[1].age_min, Some(185));
        assert!(map.stations[1].is_old);
        assert_eq!(map.stations[1].latest.unwrap().gust_ms, None);
        assert_eq!(map.stations[2].latest, None);
        assert!(map.stations[2].is_old);

        assert_eq!(map.credits.len(), 1);
        assert_eq!(map.credits[0].text, "Quelle: Deutscher Wetterdienst");
        assert!(!map.truncated);
    }

    #[test]
    fn names_in_capitals_are_set_in_ordinary_case() {
        assert_eq!(readable_name("PATSCHERKOFEL"), "Patscherkofel");
        assert_eq!(
            readable_name("INNSBRUCK/UNIVERSITAET"),
            "Innsbruck/Universitaet"
        );
        assert_eq!(readable_name("SONNBLICK - AUTOM."), "Sonnblick - Autom.");
        assert_eq!(readable_name("ST.PÖLTEN LANDHAUS"), "St.Pölten Landhaus");
        // Names that already have small letters are the provider's business.
        assert_eq!(
            readable_name("Garmisch-Partenkirchen"),
            "Garmisch-Partenkirchen"
        );
        assert_eq!(readable_name("Zugspitze (DWD)"), "Zugspitze (DWD)");
        assert_eq!(readable_name(""), "");
    }

    #[test]
    fn a_station_history_is_asked_for_and_read() {
        assert_eq!(
            station_history_path("dwd-10min:05792".to_owned()).unwrap(),
            "/v1/stations/dwd-10min:05792/observations"
        );
        assert!(station_history_path("not an id".to_owned()).is_err());

        let body = r#"{
            "station": {"id": "dwd-10min:05792", "source": "dwd-10min", "name": "Zugspitze",
                        "position": {"lat": 47.421, "lon": 10.9848}, "elevation_m": 2956.0},
            "observations": [
                {"time": "2026-11-18T18:10:00Z", "wind_speed_ms": 13.0, "wind_gust_ms": 22.0},
                {"time": "2026-11-18T18:20:00Z", "wind_speed_ms": 14.9, "wind_gust_ms": 25.7}
            ],
            "attributions": []
        }"#;
        let history = station_history(200, body.as_bytes().to_vec(), NOW).unwrap();

        assert_eq!(history.readings.len(), 2);
        assert!(history.readings[0].time_unix_s < history.readings[1].time_unix_s);
        assert_eq!(history.station.latest, Some(history.readings[1]));
        assert_eq!(history.station.age_min, Some(45));
    }

    #[test]
    fn the_servers_clock_and_late_providers_are_read() {
        assert_eq!(server_status_path(), "/v1/health");
        let body = r#"{
            "status": "degraded", "version": "0.1.0", "time": "2026-11-18T19:05:00Z",
            "sources": [
                {"source": "dwd-10min", "provider": "Deutscher Wetterdienst", "cadence_seconds": 600,
                 "stale": true, "stations": 3},
                {"source": "geosphere-tawes", "provider": "GeoSphere Austria", "cadence_seconds": 600,
                 "last_success": "2026-11-18T19:05:00Z", "stale": false, "stations": 3}
            ]
        }"#;
        let status = server_status(200, body.as_bytes().to_vec()).unwrap();

        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.time_unix_s, NOW);
        assert_eq!(status.late_providers, ["Deutscher Wetterdienst"]);
    }

    /// The server's own recorded answers, which its tests keep current.
    /// Reading them here catches a wire format that drifts away from what
    /// the apps understand.
    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    #[test]
    fn the_servers_recorded_answers_are_understood() {
        let status = server_status(200, golden("health.json")).unwrap();
        assert_eq!(status.time_unix_s, NOW);
        assert!(status.late_providers.is_empty());

        let map = wind_map(200, golden("stations.json"), status.time_unix_s).unwrap();
        assert!(!map.stations.is_empty());
        assert!(map.stations.iter().all(|station| station.latest.is_some()));
        // Recorded with every provider on time, so nothing is shown as old.
        assert!(map.stations.iter().all(|station| !station.is_old));
        // Every station's source has its credit in the same answer.
        for station in &map.stations {
            assert!(
                map.credits
                    .iter()
                    .any(|credit| credit.source == station.source),
                "{} has no credit",
                station.name
            );
        }

        let history =
            station_history(200, golden("observations.json"), status.time_unix_s).unwrap();
        assert!(history.readings.len() > 1);
        assert_eq!(history.station.latest, history.readings.last().copied());
        assert_eq!(history.credits.len(), 1);

        let refused = wind_map(400, golden("error_missing_bbox.json"), NOW).unwrap_err();
        assert!(refused.to_string().contains("invalid_bbox"));
    }

    #[test]
    fn failures_are_put_in_words_for_the_pilot() {
        let error = |status: u16, body: &str| {
            wind_map(status, body.as_bytes().to_vec(), NOW)
                .unwrap_err()
                .to_string()
        };

        let said = error(
            400,
            r#"{"error": {"code": "invalid_bbox", "message": "the bbox parameter is missing"}}"#,
        );
        assert!(said.contains("invalid_bbox"), "{said}");
        assert!(!said.contains("parameter"), "{said}");
        // A code from a newer server is passed on as it is.
        assert!(
            error(429, r#"{"error": {"code": "slow_down", "message": "x"}}"#).contains("slow_down")
        );
        assert!(error(503, "").contains("Try again"));
        assert!(error(502, "<html>Bad Gateway</html>").contains("502"));
        assert!(error(200, "not json").contains("could not be read"));
    }
}
