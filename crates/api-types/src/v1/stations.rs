use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, SourceId, StationId};

use super::{Attribution, Reading};

/// The answer of `GET /v1/stations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationsResponse {
    /// The stations inside the requested box, sorted by source and identifier.
    pub stations: Vec<StationWithLatest>,
    /// Whether the box holds more stations than one response lists. The
    /// client should ask for a smaller box.
    pub truncated: bool,
    /// The credit for every source that has a station in the list.
    pub attributions: Vec<Attribution>,
    /// The sources with a station in the list that the server has not
    /// received new data from for a while, as `/v1/health` judges it: their
    /// stations show old readings until they deliver again. A late source
    /// with no station in the list is not named; `/v1/health` names every
    /// one. Left out when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub late_sources: Vec<SourceId>,
}

/// A fixed place where weather is measured.
///
/// Each station comes from exactly one source. The same physical station
/// reported by two sources appears twice: data under different licences is
/// never merged into one record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    /// Identifier of the form `source:local`.
    pub id: StationId,
    /// The source that reports the station; its credit is in the response's
    /// attributions.
    pub source: SourceId,
    /// Name as the provider publishes it.
    pub name: String,
    /// The short name of the network that reports it, "MeteoSwiss", the
    /// same as its source's attribution's `short_name`: to tell apart two
    /// stations of the same name in a list. Left out when the source has
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// Where the station stands.
    pub position: GeoPoint,
    /// Height above mean sea level in metres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_m: Option<f64>,
}

/// A station with the newest observation the server holds for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationWithLatest {
    /// The station.
    #[serde(flatten)]
    pub station: Station,
    /// Its newest observation, however old. Left out when the server holds
    /// none. The reading carries its time; the client decides what is too
    /// old to show as current.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<Reading>,
}

impl From<&venti_domain::Station> for Station {
    fn from(station: &venti_domain::Station) -> Self {
        Self {
            id: station.id.clone(),
            source: station.id.source().clone(),
            name: station.name.clone(),
            network: None,
            position: station.position,
            elevation_m: station.elevation_m,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_station_with_its_latest_reading_is_one_flat_object() {
        let station = venti_domain::Station {
            id: "dwd-10min:05792".parse().unwrap(),
            name: "Zugspitze".to_owned(),
            position: GeoPoint::new(47.421, 10.9848).unwrap(),
            elevation_m: Some(2956.0),
        };
        let entry = StationWithLatest {
            station: Station::from(&station),
            latest: None,
        };

        let json = serde_json::to_string(&entry).unwrap();

        assert_eq!(
            json,
            r#"{"id":"dwd-10min:05792","source":"dwd-10min","name":"Zugspitze","position":{"lat":47.421,"lon":10.9848},"elevation_m":2956.0}"#
        );
        assert_eq!(
            serde_json::from_str::<StationWithLatest>(&json).unwrap(),
            entry
        );
    }
}
