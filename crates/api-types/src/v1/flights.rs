//! A pilot's flights as the server keeps them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

use super::Handle;

/// The largest flight file the server accepts, in bytes.
///
/// A fix is a line of 37 to about 55 bytes, so twelve hours at one fix a
/// second, longer than any paraglider has stayed up, come to between 1.6
/// and 2.4 MB. Four are room for that and for whatever else a recorder
/// writes, and little enough that reading one cannot strain the server.
pub const MAX_FLIGHT_FILE_BYTES: usize = 4 * 1024 * 1024;

/// How many flights a list holds when the request names no limit.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// The most flights one request may ask for.
pub const MAX_PAGE_SIZE: usize = 100;

/// The address of one flight on the server: 26 lower-case letters and
/// digits, as in `/v1/flights/{id}` and in a share link.
///
/// It is not the flight's identity. Two accounts that upload the same
/// recording hold two flights with two addresses, and an address cannot be
/// worked out from the file, so a link to an unlisted flight is known only
/// to those it was given to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FlightId(String);

/// Text that does not have the form of a flight's address.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a flight's address is 26 lower-case letters and digits")]
pub struct FlightIdError;

impl FlightId {
    /// How many characters an address has.
    pub const LENGTH: usize = 26;
    /// The characters an address is written in: the digits and the
    /// alphabet without `i`, `l`, `o` and `u`, which are easily misread.
    pub const ALPHABET: &'static str = "0123456789abcdefghjkmnpqrstvwxyz";

    /// The address as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FlightId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for FlightId {
    type Err = FlightIdError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let well_formed =
            text.len() == Self::LENGTH && text.chars().all(|c| Self::ALPHABET.contains(c));
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(FlightIdError)
        }
    }
}

impl TryFrom<String> for FlightId {
    type Error = FlightIdError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<FlightId> for String {
    fn from(id: FlightId) -> Self {
        id.0
    }
}

/// Who may see a flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Only its pilot. To everyone else the flight does not exist. Every
    /// flight starts like this.
    Private,
    /// Anyone who has its link. It is not listed anywhere.
    Unlisted,
    /// Anyone. It is listed on its pilot's page.
    Public,
    /// A setting this version of the client does not know. Treat the flight
    /// as visible to more people than its pilot, and do not send this back.
    #[serde(other)]
    Unknown,
}

/// The pilot a flight belongs to, as everyone may see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pilot {
    /// The pilot's handle.
    pub handle: Handle,
    /// The name the pilot is shown by.
    pub display_name: String,
}

/// One flight.
///
/// The server keeps the flight file exactly as it was uploaded and works
/// everything else here out from it, with the code the apps use on the
/// device. Like there, the numbers describe the flown part of the
/// recording: from take-off to landing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// The flight's address on the server.
    pub id: FlightId,
    /// The flight's identity: a hash of its fixes, 32 hexadecimal
    /// characters. It is the identifier the apps' logbooks use, so a device
    /// matches the server's flights with its own by this value.
    pub identity: String,
    /// The pilot the flight belongs to.
    pub pilot: Pilot,
    /// Who may see the flight.
    pub visibility: Visibility,
    /// When the server first received the flight.
    pub uploaded_at: Timestamp,
    /// When the flown part begins: the take-off, or the first fix of a
    /// recording that never shows the glider flying.
    pub start: Timestamp,
    /// Seconds from take-off to landing, or to the end of a recording that
    /// stops in the air. For a recording that never shows the glider
    /// flying, the length of the recording.
    pub airtime_s: i64,
    /// Where and when the glider took off. Left out when the recording
    /// never shows it flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff: Option<FlightPlace>,
    /// Where and when it landed. Left out when the recording ends in the
    /// air or never shows the glider flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landing: Option<FlightPlace>,
    /// Distance along the track, metres.
    pub track_length_m: f64,
    /// Highest satellite altitude, metres above mean sea level.
    pub max_altitude_m: i32,
    /// Best climb averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// How the flight scores as a cross-country flight. Left out for a
    /// track too short to score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<FlightScore>,
    /// The glider, as the flight file names it. Text from the file: show it
    /// as text and never as markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glider: Option<String>,
    /// The shape of the track for a thumbnail: at most 96 points, each a
    /// latitude and a longitude in degrees. Circles in thermals are
    /// smoothed away; nothing may be measured on it.
    pub outline: Vec<[f64; 2]>,
    /// The size of the flight file in bytes.
    pub file_bytes: u32,
    /// Which version of the server's analysis produced these numbers. The
    /// server works them out again when its analysis improves; the flight
    /// then shows up as changed.
    pub analysis_version: u32,
    /// Since when the flight is taken down, if whoever runs the server has
    /// done that. Left out otherwise, which is the usual case.
    ///
    /// A flight that is taken down is private, and only its pilot is ever
    /// shown this. While it is there the flight cannot be shared: asking
    /// for `unlisted` or `public` is refused, and deleting the flight and
    /// uploading its file again brings it back taken down. Only the
    /// operator can lift it. An app says so on the flight, in place of the
    /// choice of who sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_down_at: Option<Timestamp>,
}

/// A moment at one end of a flight.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightPlace {
    /// When.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Satellite altitude, metres above mean sea level.
    pub altitude_m: i32,
}

/// How a flight scores under the World XContest's rules, as this project
/// reads them. Until a flight has been compared with XContest's own result
/// the points are ours, not the league's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightScore {
    /// What the flight is scored as.
    pub kind: RouteKind,
    /// For a triangle, whether start and finish lie within 5% of its
    /// perimeter of each other. Always `false` for a free flight.
    pub closed: bool,
    /// The scoring distance, metres.
    pub distance_m: f64,
    /// The points.
    pub points: f64,
}

/// What a flight is scored as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// The longest distance through up to three turnpoints.
    FreeFlight,
    /// A triangle that is not an FAI triangle.
    FreeTriangle,
    /// A triangle whose shortest leg is at least 28% of its perimeter.
    FaiTriangle,
    /// A kind this version of the client does not know.
    #[serde(other)]
    Unknown,
}

/// The answer of `GET /v1/flights/{id}` and of `PATCH /v1/flights/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightResponse {
    /// The flight.
    pub flight: Flight,
}

/// The answer of `POST /v1/flights`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadResponse {
    /// The flight as the server now holds it.
    pub flight: Flight,
    /// `true` when this upload created the flight, `false` when the account
    /// already had it. Either way the upload succeeded.
    pub created: bool,
}

/// The answer of `GET /v1/flights`, one page of the account's flights, and
/// of `GET /v1/flights/public`, one page of everyone's public flights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightsResponse {
    /// The flights of this page, in the order that was asked for: newest
    /// first unless the request said otherwise.
    pub flights: Vec<Flight>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// How many of a pilot's flights count in a ranking: the best three.
pub const RANKING_COUNTED: usize = 3;

/// The answer of `GET /v1/ranking`: the pilots of everyone's public
/// flights by the sum of the points of their best flights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankingResponse {
    /// How many of a pilot's flights count at most:
    /// [`RANKING_COUNTED`], said in the answer so that a client words the
    /// rule from what the server did.
    pub counted: u32,
    /// The pilots, the one with the most points first, and by handle among
    /// pilots with the same points.
    pub pilots: Vec<RankedPilot>,
}

/// A pilot's place in a ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedPilot {
    /// The pilot.
    pub pilot: Pilot,
    /// The sum of the points of the flights that count, rounded to
    /// hundredths as a flight's own points are.
    pub points: f64,
    /// How many of the pilot's public flights among those asked for scored
    /// at all, those that count among them.
    pub scored: u32,
    /// The flights that count, the best first: no more than `counted`.
    pub flights: Vec<Flight>,
}

/// The body of `PATCH /v1/flights/{id}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateFlightRequest {
    /// Who may see the flight from now on.
    pub visibility: Visibility,
}

/// The answer of `GET /v1/flights/changes`: what happened to the account's
/// flights since the device last asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightChangesResponse {
    /// Flights that are new or changed, each as it is now.
    pub flights: Vec<Flight>,
    /// Flights that were deleted.
    pub deleted: Vec<DeletedFlight>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more changes are waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server did not recognise the request's `since`, for
    /// example after it was restored from a backup, and therefore started
    /// again from the beginning. The device should then compare its whole
    /// logbook with the server's flights instead of applying changes only.
    pub reset: bool,
}

/// A flight that is no longer on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedFlight {
    /// The address the flight had.
    pub id: FlightId,
    /// The identity of the flight, to find it in the device's logbook.
    pub identity: String,
}

/// The answer of `GET /v1/pilots/{handle}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotResponse {
    /// The pilot.
    pub pilot: Pilot,
    /// One page of the pilot's public flights, newest first.
    pub flights: Vec<Flight>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flight_address_is_26_characters_of_its_alphabet() {
        let id = "0123456789abcdefghjkmnpqrs";
        assert_eq!(id.parse::<FlightId>().unwrap().as_str(), id);
        assert_eq!(FlightId::ALPHABET.len(), 32);

        for bad in [
            "",
            "0123456789abcdefghjkmnpqr",   // one short
            "0123456789abcdefghjkmnpqrst", // one long
            "0123456789ABCDEFGHJKMNPQRS",  // upper case
            "0123456789abcdefghijkmnpqr",  // an i
            "../../../../../etc/passwd/",
            "0123456789abcdefghjkmnpq%2f",
            "0123456789abcdefghjkmnpqrś",
        ] {
            assert_eq!(bad.parse::<FlightId>(), Err(FlightIdError), "{bad:?}");
        }
        assert!(serde_json::from_str::<FlightId>(r#""changes""#).is_err());
    }

    #[test]
    fn settings_a_client_does_not_know_still_decode() {
        let visibility: Visibility = serde_json::from_str(r#""followers""#).unwrap();
        assert_eq!(visibility, Visibility::Unknown);
        let kind: RouteKind = serde_json::from_str(r#""flat_triangle""#).unwrap();
        assert_eq!(kind, RouteKind::Unknown);
        assert_eq!(
            serde_json::to_string(&Visibility::Unlisted).unwrap(),
            r#""unlisted""#
        );
        assert_eq!(
            serde_json::to_string(&RouteKind::FaiTriangle).unwrap(),
            r#""fai_triangle""#
        );
    }

    #[test]
    fn what_is_not_known_about_a_flight_is_left_out() {
        let start = chrono::DateTime::from_timestamp(1_783_161_000, 0).unwrap();
        let flight = Flight {
            id: "0123456789abcdefghjkmnpqrs".parse().unwrap(),
            identity: "00112233445566778899aabbccddeeff".to_owned(),
            pilot: Pilot {
                handle: Handle::new("demo").unwrap(),
                display_name: "Demo Pilot".to_owned(),
            },
            visibility: Visibility::Private,
            uploaded_at: start,
            start,
            airtime_s: 60,
            takeoff: None,
            landing: None,
            track_length_m: 600.0,
            max_altitude_m: 1620,
            max_climb_ms: 0.0,
            score: None,
            glider: None,
            outline: vec![[47.6642, 11.7967], [47.67, 11.79]],
            file_bytes: 2300,
            analysis_version: 1,
            taken_down_at: None,
        };

        let json = serde_json::to_value(&flight).unwrap();
        let object = json.as_object().unwrap();

        for absent in ["takeoff", "landing", "score", "glider", "taken_down_at"] {
            assert!(!object.contains_key(absent), "{absent}");
        }
        assert_eq!(json["outline"][0][0], 47.6642);
        assert_eq!(json["pilot"]["handle"], "demo");
        assert_eq!(serde_json::from_value::<Flight>(json).unwrap(), flight);
    }
}
