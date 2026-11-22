use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, SourceId, StationId, Timestamp};

use super::Attribution;

/// The path of the foehn reading.
pub const FOEHN_PATH: &str = "/v1/foehn";

/// The answer of `GET /v1/foehn`: the pressure difference across the Alps'
/// main ridge for each pair of stations, and what it says about foehn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoehnResponse {
    /// The server's time when it answered: the moment the levels are of.
    pub time: Timestamp,
    /// From this difference, in whole hectopascals, a pair's level is
    /// `possible`.
    pub possible_from_hpa: f64,
    /// From this difference, in whole hectopascals, a pair's level is
    /// `likely`.
    pub likely_from_hpa: f64,
    /// How old a reading may be to count, seconds. A pair with an older or
    /// a missing reading has the level `unknown`.
    pub fresh_for_s: i64,
    /// The pairs, in a fixed order.
    pub pairs: Vec<FoehnPair>,
    /// What the reading does not say, to show wherever a level is shown.
    pub caution: String,
    /// The credit for every source that has a pressure in the answer.
    pub attributions: Vec<Attribution>,
}

/// What a pressure difference says about foehn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoehnLevel {
    /// Nothing is known: a reading is missing or too old. Not "no foehn".
    Unknown,
    /// No sign of foehn in the pressure. Not "no foehn" either.
    None,
    /// Foehn is possible.
    Possible,
    /// Foehn is likely.
    Likely,
}

/// A side of the main ridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoehnSide {
    /// South of the ridge.
    South,
    /// North of the ridge.
    North,
}

/// Why a pair's level is `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoehnUnknown {
    /// The southern station has no pressure that can be compared.
    SouthMissing,
    /// The northern station has no pressure that can be compared.
    NorthMissing,
    /// The southern station's newest pressure is too old.
    SouthOld,
    /// The northern station's newest pressure is too old.
    NorthOld,
    /// The two newest readings are too far apart in time to be compared.
    Apart,
}

/// Two stations on either side of the main ridge and what their pressures
/// say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoehnPair {
    /// A stable name for the pair, for example `bolzano-innsbruck`.
    pub id: String,
    /// The station south of the ridge.
    pub south: FoehnStation,
    /// The station north of the ridge.
    pub north: FoehnStation,
    /// The level at the answer's `time`.
    pub level: FoehnLevel,
    /// Why the level is `unknown`. Left out otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_because: Option<FoehnUnknown>,
    /// The pressure in the south less the pressure in the north, hPa, to a
    /// tenth. Left out when the level is `unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difference_hpa: Option<f64>,
    /// The time of the older of the two readings the difference is of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difference_time: Option<Timestamp>,
    /// The side with the higher pressure, by the difference in whole
    /// hectopascals. Left out when that is zero or the level `unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub higher: Option<FoehnSide>,
    /// Which foehn the level speaks of: `south` foehn with the higher
    /// pressure in the south, which blows north of the ridge, and `north`
    /// foehn the other way. Only with the levels `possible` and `likely`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foehn: Option<FoehnSide>,
    /// The differences of the last 24 hours, oldest first.
    pub series: Vec<FoehnPoint>,
    /// Whether the pair or its limits are not from a weather service's own
    /// page and wait for pilots to confirm them.
    pub to_verify: bool,
    /// Where the pair and its limits are from, in a sentence.
    pub basis: String,
}

/// A station of a pair with its newest pressure at sea level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoehnStation {
    /// The station's identifier; its readings are at
    /// `/v1/stations/{id}/observations`.
    pub id: StationId,
    /// The source that reports the station.
    pub source: SourceId,
    /// The place's name as pilots say it.
    pub name: String,
    /// Where it stands.
    pub position: GeoPoint,
    /// Its height above mean sea level in metres, if the server knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_m: Option<f64>,
    /// Its newest pressure at sea level, however old. Left out when the
    /// server holds none of the last 24 hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<FoehnPressure>,
}

/// A pressure at sea level.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FoehnPressure {
    /// End of the measuring interval.
    pub time: Timestamp,
    /// The pressure at sea level, hPa, to a tenth.
    pub sea_level_hpa: f64,
    /// Who reduced it to sea level.
    pub reduced_by: FoehnReducedBy,
}

/// Who reduced a pressure to sea level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoehnReducedBy {
    /// The provider sends it so.
    Provider,
    /// Venti, from the pressure at the station, its temperature and the
    /// height the provider lists for it, by the barometric formula, because
    /// the provider sent no reduced pressure. The height need not be the
    /// barometer's, so the value may be off by several hectopascals: a
    /// difference with it on either side is not sure, and no ground for a
    /// warning.
    Venti,
}

/// One difference of a pair's last 24 hours.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FoehnPoint {
    /// The time of the older of the two readings.
    pub time: Timestamp,
    /// South less north, hPa, to a tenth.
    pub difference_hpa: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_pair_names_its_reason_and_no_difference() {
        let text = r#"{
            "id": "bolzano-innsbruck",
            "south": {"id": "suedtirol-meteo:83200MS", "source": "suedtirol-meteo",
                      "name": "Bolzano", "position": {"lat": 46.4977, "lon": 11.3128}},
            "north": {"id": "geosphere-tawes:11320", "source": "geosphere-tawes",
                      "name": "Innsbruck", "position": {"lat": 47.26, "lon": 11.3842},
                      "pressure": {"time": "2026-11-18T19:00:00Z", "sea_level_hpa": 998.2,
                                   "reduced_by": "venti"}},
            "level": "unknown", "unknown_because": "south_missing",
            "series": [], "to_verify": false, "basis": "…"
        }"#;
        let pair: FoehnPair = serde_json::from_str(text).unwrap();
        assert_eq!(pair.level, FoehnLevel::Unknown);
        assert_eq!(pair.unknown_because, Some(FoehnUnknown::SouthMissing));
        assert_eq!(pair.difference_hpa, None);
        assert_eq!(
            pair.north.pressure.unwrap().reduced_by,
            FoehnReducedBy::Venti
        );
        let again = serde_json::to_value(&pair).unwrap();
        assert!(again.get("difference_hpa").is_none());
        assert!(again.get("foehn").is_none());
    }
}
