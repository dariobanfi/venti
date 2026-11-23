//! Live tracks: where a glider is while it flies, for those its pilot lets
//! see it.
//!
//! A live track is not a flight. It is made of the positions an app sends
//! during one flight, it is kept for a short time after the flight and then
//! removed, and it never reaches the logbook by itself: the logbook's
//! flight is the file the recorder wrote.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

use super::Pilot;

/// The most fixes one request may send. Two minutes at a fix a second:
/// what gathers while a phone waits for the network to come back is sent
/// in several requests.
pub const MAX_LIVE_BATCH_FIXES: usize = 120;

/// The largest body the server reads from a request that sends fixes, in
/// bytes. The largest batch, with every value written to full length, is
/// about 26 kB.
pub const MAX_LIVE_BODY_BYTES: usize = 32 * 1024;

/// The most fixes one answer about a track carries. A track that holds
/// more is read in several answers; see [`LiveTrackResponse::more`].
pub const MAX_LIVE_FIXES: usize = 5000;

/// The most tracks one list carries. A box that holds more gets the first
/// ones by identifier, and [`LiveTracksResponse::truncated`] is set.
pub const MAX_LIVE_TRACKS: usize = 1000;

/// For how many minutes after its newest fix a public track is listed.
/// A client that draws a track uses the same number to tell a glider that
/// is being followed from one that has not been heard of for a while.
pub const LIVE_LISTED_MINUTES: i64 = 30;

/// The identifier of one live track: 26 lower-case letters and digits.
///
/// The app makes it itself, before it sends the first fix, by drawing each
/// of the 26 characters at random from [`LiveTrackId::ALPHABET`] with the
/// system's source of random numbers for secrets. That is 130 random bits:
///
/// - sending a batch again after a lost answer adds nothing twice, because
///   the server knows the track by this identifier;
/// - nobody can guess it, and whoever has it can see the track. For a
///   track shared by link it is the only key. Treat it like one: it
///   belongs in no log.
///
/// An identifier is used for one track and never again.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LiveTrackId(String);

/// Text that does not have the form of a live track's identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a live track's identifier is 26 lower-case letters and digits")]
pub struct LiveTrackIdError;

impl LiveTrackId {
    /// How many characters an identifier has.
    pub const LENGTH: usize = 26;
    /// The characters an identifier is written in: the digits and the
    /// alphabet without `i`, `l`, `o` and `u`, which are easily misread.
    /// Five bits to a character.
    pub const ALPHABET: &'static str = "0123456789abcdefghjkmnpqrstvwxyz";

    /// Makes an identifier from random bytes, five bits of each. The bytes
    /// must come from the system's source of random numbers for secrets;
    /// this crate has none of its own.
    pub fn from_random(bytes: &[u8; Self::LENGTH]) -> Self {
        let alphabet = Self::ALPHABET.as_bytes();
        let text = bytes
            .iter()
            .map(|byte| char::from(alphabet[usize::from(byte & 31)]))
            .collect();
        Self(text)
    }

    /// The identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LiveTrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for LiveTrackId {
    type Err = LiveTrackIdError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let well_formed =
            text.len() == Self::LENGTH && text.chars().all(|c| Self::ALPHABET.contains(c));
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(LiveTrackIdError)
        }
    }
}

impl TryFrom<String> for LiveTrackId {
    type Error = LiveTrackIdError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<LiveTrackId> for String {
    fn from(id: LiveTrackId) -> Self {
        id.0
    }
}

/// Who may see a live track.
///
/// There is no setting for "nobody": a track that nobody may see is one
/// the app does not send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveVisibility {
    /// Whoever has the track's identifier, which is to say its link. The
    /// track is in no list.
    Link,
    /// Everyone. The track is listed for the part of the map it is in, with
    /// its pilot's handle and name.
    Public,
    /// A setting this version of the client does not know. Treat the track
    /// as visible to others, and do not send this.
    #[serde(other)]
    Unknown,
}

/// Where a glider was at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LiveFix {
    /// When, in whole seconds: the server drops the fraction of a second.
    /// A track holds one fix for a second.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Altitude in metres above mean sea level, as the cockpit shows it.
    pub altitude_m: i32,
    /// Speed over the ground, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_speed_ms: Option<f64>,
    /// Climb rate, m/s; negative when sinking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climb_ms: Option<f64>,
    /// The direction the glider moves in over the ground, degrees
    /// clockwise from true north.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_deg: Option<f64>,
}

/// The body of `POST /v1/live/positions`: fixes for one live track.
///
/// The first request for an identifier begins the track. Sending the same
/// request again changes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivePositionsRequest {
    /// The track's identifier, which the app made; see [`LiveTrackId`].
    pub id: String,
    /// Who may see the track.
    pub visibility: LiveVisibility,
    /// Whether the glider has landed: the track has ended. Left out means
    /// `false`.
    #[serde(default)]
    pub landed: bool,
    /// The fixes, oldest first: at least one, at most
    /// [`MAX_LIVE_BATCH_FIXES`].
    pub fixes: Vec<LiveFix>,
}

/// A live track without its fixes: who, where now, and whether it has
/// ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTrack {
    /// The track's identifier.
    pub id: LiveTrackId,
    /// The pilot, as on a shared flight.
    pub pilot: Pilot,
    /// Who may see the track.
    pub visibility: LiveVisibility,
    /// Whether the app has said that the glider landed.
    pub landed: bool,
    /// When the track began: the time of its oldest fix.
    pub started_at: Timestamp,
    /// The newest fix: where the glider is, or was last heard of.
    pub latest: LiveFix,
    /// How many fixes the track holds.
    pub fixes: u32,
    /// When the server removes the track, unless a newer fix arrives
    /// first.
    pub expires_at: Timestamp,
}

/// The answer of `POST /v1/live/positions`: what the server kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivePositionsResponse {
    /// The track as it now is.
    pub track: LiveTrack,
    /// The address of the page to share: what a link to the track opens,
    /// for people without the app.
    pub page: String,
    /// `true` when this request began the track.
    pub created: bool,
    /// Fixes this request added to the track.
    pub stored: u32,
    /// Fixes the track already held: a fix for the same second. Nothing
    /// changed for them.
    pub known: u32,
    /// Fixes refused because their time lies in the future by the
    /// server's clock.
    pub refused_future: u32,
    /// Fixes refused because they are older than the server takes.
    pub refused_old: u32,
    /// Fixes refused because the track holds as many as a track may.
    pub refused_full: u32,
}

/// The answer of `GET /v1/live`: the public tracks in a box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTracksResponse {
    /// The server's clock when it wrote this answer. Work out the age of a
    /// position against this, not against the device's clock. An answer of
    /// `304` does not renew it.
    pub time: Timestamp,
    /// The public tracks whose newest fix lies in the box and is no older
    /// than [`LIVE_LISTED_MINUTES`], sorted by identifier.
    pub tracks: Vec<LiveTrack>,
    /// Whether the box holds more tracks than one answer lists. The client
    /// should ask for a smaller box.
    pub truncated: bool,
}

/// The answer of `GET /v1/live/{id}`: one track and some of its fixes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTrackResponse {
    /// The server's clock when it wrote this answer; see
    /// [`LiveTracksResponse::time`].
    pub time: Timestamp,
    /// The track.
    pub track: LiveTrack,
    /// The fixes asked for, in the order in which the server received
    /// them. That is the order of their times, except where fixes arrived
    /// late: sort by `time` before drawing.
    pub fixes: Vec<LiveFix>,
    /// Pass this as `since` next time, to get only what arrived after this
    /// answer.
    pub cursor: String,
    /// Whether more fixes are waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server did not know the request's `since` and
    /// therefore started from the track's first fix. Drop the fixes held
    /// for the track and keep these.
    pub reset: bool,
}

// --- Aircraft that other networks see -----------------------------------------

/// For how many minutes after it was heard an aircraft of another network
/// is listed.
pub const LIVE_OTHERS_LISTED_MINUTES: i64 = 10;

/// The most aircraft one layer of [`LiveOthersResponse`] carries.
/// Paragliders and hang gliders come first, so a box with more loses
/// aircraft of other kinds first.
pub const MAX_LIVE_OTHERS: usize = 1000;

/// The widest box `GET /v1/live/others` answers for, in degrees, from west
/// to east and from south to north alike.
pub const MAX_LIVE_OTHERS_BOX_DEG: f64 = 10.0;

/// What kind of aircraft another network says a position is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AircraftKind {
    /// A paraglider.
    Paraglider,
    /// A hang glider.
    HangGlider,
    /// A glider or motor glider.
    Glider,
    /// A tow plane.
    TowPlane,
    /// A helicopter or gyrocopter.
    Helicopter,
    /// A skydiver or a parachute.
    Parachute,
    /// A plane that drops skydivers.
    DropPlane,
    /// An aeroplane with a piston engine.
    PoweredAircraft,
    /// An aeroplane with a jet or turboprop engine.
    JetAircraft,
    /// A balloon.
    Balloon,
    /// An airship.
    Airship,
    /// A drone.
    Drone,
    /// An obstacle that sends its position.
    Obstacle,
    /// The network does not say, or this version of the client does not
    /// know the word.
    #[serde(other)]
    Unknown,
}

impl AircraftKind {
    /// Whether this is a paraglider or a hang glider.
    pub fn is_free_flight(self) -> bool {
        matches!(self, Self::Paraglider | Self::HangGlider)
    }
}

/// An aircraft another network saw: where it was when it was last heard.
///
/// It is not a pilot of Venti's and never one record with one: the two
/// come from sources of different licences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OtherAircraft {
    /// A name for the aircraft that stays the same while the server runs,
    /// so that a map can move its mark. It is not the device's address and
    /// says nothing about the aircraft.
    pub id: String,
    /// What kind of aircraft.
    pub aircraft: AircraftKind,
    /// When it was last heard.
    pub time: Timestamp,
    /// How long before the answer's `time` that was, in seconds.
    pub age_s: u32,
    /// Where.
    pub position: GeoPoint,
    /// Altitude in metres above mean sea level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude_m: Option<f64>,
    /// Speed over the ground, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_speed_ms: Option<f64>,
    /// Climb, m/s; negative when sinking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climb_ms: Option<f64>,
    /// Direction over the ground, degrees clockwise from true north.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_deg: Option<f64>,
    /// The registration, where the owner let the aircraft be identified
    /// and gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration: Option<String>,
    /// The competition number, on the same terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub competition_id: Option<String>,
    /// The aircraft's model, on the same terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// The aircraft of one network in a box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveOthersLayer {
    /// The network, by its source's identifier: its credit is the
    /// attribution of the same source.
    pub source: venti_domain::SourceId,
    /// Whether the server is connected to the network now. When it is not,
    /// the list holds what was heard before, and ages.
    pub connected: bool,
    /// The aircraft heard in the last [`LIVE_OTHERS_LISTED_MINUTES`]:
    /// paragliders and hang gliders first, then the others, each part the
    /// most recently heard first.
    pub aircraft: Vec<OtherAircraft>,
    /// Whether the box holds more than [`MAX_LIVE_OTHERS`].
    pub truncated: bool,
}

/// The answer of `GET /v1/live/others`: the aircraft that other networks
/// saw in a box, one layer per network, and the credit of each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveOthersResponse {
    /// The server's clock when it wrote this answer.
    pub time: Timestamp,
    /// For how many minutes after it was heard an aircraft is listed.
    pub listed_minutes: i64,
    /// One layer per network the server listens to, sorted by source.
    /// Empty on a server that listens to none.
    pub layers: Vec<LiveOthersLayer>,
    /// The credit of every network in `layers`.
    pub attributions: Vec<super::Attribution>,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::super::Handle;
    use super::*;

    #[test]
    fn a_kind_of_aircraft_this_version_does_not_know_is_unknown() {
        let kind: AircraftKind = serde_json::from_str(r#""hot_air_sofa""#).unwrap();
        assert_eq!(kind, AircraftKind::Unknown);
        assert_eq!(
            serde_json::to_string(&AircraftKind::HangGlider).unwrap(),
            r#""hang_glider""#
        );
        assert!(AircraftKind::Paraglider.is_free_flight());
        assert!(!AircraftKind::Glider.is_free_flight());
    }

    #[test]
    fn a_live_track_identifier_is_26_characters_of_its_alphabet() {
        let id = "0123456789abcdefghjkmnpqrs";
        assert_eq!(id.parse::<LiveTrackId>().unwrap().as_str(), id);
        assert_eq!(LiveTrackId::ALPHABET.len(), 32);

        for bad in [
            "",
            "positions",
            "0123456789abcdefghjkmnpqr",   // one short
            "0123456789abcdefghjkmnpqrst", // one long
            "0123456789ABCDEFGHJKMNPQRS",  // upper case
            "0123456789abcdefghijkmnpqr",  // an i
            "../../../../../etc/passwd/",
            "0123456789abcdefghjkmnpq%2f",
            "0123456789abcdefghjkmnpqrś",
            "0123456789abcdefghjkmnpqr\0",
        ] {
            assert_eq!(bad.parse::<LiveTrackId>(), Err(LiveTrackIdError), "{bad:?}");
        }
    }

    #[test]
    fn an_identifier_is_made_from_five_bits_of_every_random_byte() {
        let id = LiveTrackId::from_random(&[0; 26]);
        assert_eq!(id.as_str(), "0".repeat(26));
        let id = LiveTrackId::from_random(&[0xff; 26]);
        assert_eq!(id.as_str(), "z".repeat(26));
        let mut bytes = [0_u8; 26];
        for (index, byte) in bytes.iter_mut().enumerate() {
            // The upper three bits do not matter.
            *byte = (index as u8) | 0b1110_0000;
        }
        let id = LiveTrackId::from_random(&bytes);
        assert_eq!(id.as_str(), "0123456789abcdefghjkmnpqrs");
        assert_eq!(id.as_str().parse::<LiveTrackId>(), Ok(id));
    }

    fn fix() -> LiveFix {
        LiveFix {
            time: Utc.with_ymd_and_hms(2026, 11, 18, 19, 4, 56).unwrap(),
            position: GeoPoint::new(47.6642, 11.7967).unwrap(),
            altitude_m: 1620,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        }
    }

    #[test]
    fn what_a_fix_does_not_say_is_left_out() {
        let json = serde_json::to_value(fix()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "time": "2026-11-18T19:04:56Z",
                "position": { "lat": 47.6642, "lon": 11.7967 },
                "altitude_m": 1620
            })
        );
        assert_eq!(serde_json::from_value::<LiveFix>(json).unwrap(), fix());

        let moving = LiveFix {
            ground_speed_ms: Some(9.4),
            climb_ms: Some(-1.2),
            course_deg: Some(320.0),
            ..fix()
        };
        let json = serde_json::to_value(moving).unwrap();
        assert_eq!(json["ground_speed_ms"], 9.4);
        assert_eq!(json["climb_ms"], -1.2);
        assert_eq!(json["course_deg"], 320.0);
    }

    #[test]
    fn a_fix_that_is_no_place_on_the_globe_does_not_decode() {
        let text = r#"{"time":"2026-11-18T19:04:56Z","position":{"lat":147.6,"lon":11.7},"altitude_m":1620}"#;
        assert!(serde_json::from_str::<LiveFix>(text).is_err());
        // An altitude is a whole number of metres.
        let text = r#"{"time":"2026-11-18T19:04:56Z","position":{"lat":47.6,"lon":11.7},"altitude_m":1620.5}"#;
        assert!(serde_json::from_str::<LiveFix>(text).is_err());
    }

    #[test]
    fn a_request_says_landed_only_when_it_means_it() {
        let text = r#"{"id":"0123456789abcdefghjkmnpqrs","visibility":"link","fixes":[]}"#;
        let request: LivePositionsRequest = serde_json::from_str(text).unwrap();
        assert!(!request.landed);
        assert_eq!(request.visibility, LiveVisibility::Link);

        let visibility: LiveVisibility = serde_json::from_str(r#""followers""#).unwrap();
        assert_eq!(visibility, LiveVisibility::Unknown);
        assert_eq!(
            serde_json::to_string(&LiveVisibility::Public).unwrap(),
            r#""public""#
        );
    }

    #[test]
    fn a_track_reads_back_as_it_was_written() {
        let track = LiveTrack {
            id: "0123456789abcdefghjkmnpqrs".parse().unwrap(),
            pilot: Pilot {
                handle: Handle::new("demo").unwrap(),
                display_name: "Demo Pilot".to_owned(),
                country: None,
            },
            visibility: LiveVisibility::Public,
            landed: false,
            started_at: fix().time,
            latest: fix(),
            fixes: 1,
            expires_at: fix().time + chrono::Duration::hours(48),
        };
        let json = serde_json::to_value(&track).unwrap();
        assert_eq!(json["pilot"]["handle"], "demo");
        assert_eq!(json["expires_at"], "2026-11-20T19:04:56Z");
        assert_eq!(serde_json::from_value::<LiveTrack>(json).unwrap(), track);
    }
}
