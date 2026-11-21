//! Live positions of a network that is not Venti's: a source that is a
//! connection which stays open, not a request with an answer.
//!
//! The adapter does no I/O here either. It says where to connect and what
//! to send first ([`LiveSource::server`], [`LiveSource::login`]), turns
//! each line that comes back into a position or nothing
//! ([`LiveSource::parse_line`]), and turns the network's directory of
//! devices, an ordinary file the server fetches once a day, into the
//! choices that the devices' owners made ([`LiveSource::parse_directory`]).
//! The server holds the connection, reconnects, and applies the choices and
//! the manifest's rules about age, the same way for every such source.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};

use crate::{Manifest, Request, SourceError};

/// What kind of aircraft a position is of, as the network says.
///
/// Paragliders and hang gliders are what Venti is for; everything else is
/// told apart so that a map can choose what it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aircraft {
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
    /// The network does not say.
    Unknown,
}

impl Aircraft {
    /// Whether this is a paraglider or a hang glider: the aircraft a map
    /// of Venti draws first.
    pub fn is_free_flight(self) -> bool {
        matches!(self, Self::Paraglider | Self::HangGlider)
    }
}

/// One position of one device, as a line of the network gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivePosition {
    /// The device's address in the network, as the source writes it. It
    /// is a key for the directory and never leaves the server.
    pub device: String,
    /// What the device is fitted to.
    pub aircraft: Aircraft,
    /// When the position was taken.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Altitude above mean sea level, metres.
    pub altitude_m: Option<f64>,
    /// Direction over the ground, degrees from true north.
    pub course_deg: Option<f64>,
    /// Speed over the ground, m/s.
    pub ground_speed_ms: Option<f64>,
    /// Climb, m/s; negative when sinking.
    pub climb_ms: Option<f64>,
}

/// What one line of a connection was.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveLine {
    /// The position of an aircraft.
    Position(LivePosition),
    /// The position of a device whose line itself says that it must not be
    /// shown. Nothing of it is kept.
    Hidden,
    /// A line from the server itself: a greeting, a keepalive, the answer
    /// to the login.
    Server,
    /// Something else, for example a receiver's own position or a weather
    /// report; or a line that cannot be read, with why.
    Other(Option<String>),
}

/// What the owner of one device chose in the network's directory.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceChoice {
    /// Whether the device may be shown at all.
    pub tracked: bool,
    /// Whether the device may be shown with what identifies it.
    pub identified: bool,
    /// The aircraft's registration, if the owner gave one.
    pub registration: Option<String>,
    /// The competition number, if the owner gave one.
    pub competition_id: Option<String>,
    /// The aircraft's model, if the owner gave one.
    pub model: Option<String>,
}

/// The choices of every device in the network's directory, by the
/// device's address as [`LivePosition::device`] writes it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceDirectory {
    /// The devices.
    pub devices: BTreeMap<String, DeviceChoice>,
}

/// What the server does with a position, by the directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// Not at all: the owner chose not to be tracked.
    Not,
    /// Without anything that identifies the aircraft.
    Anonymous,
    /// With what the owner let be shown.
    Identified(DeviceChoice),
}

impl DeviceDirectory {
    /// How a device may be shown. A device the directory does not know is
    /// shown without anything that identifies it: nobody chose for it.
    pub fn shown(&self, device: &str) -> Shown {
        match self.devices.get(device) {
            Some(choice) if !choice.tracked => Shown::Not,
            Some(choice) if choice.identified => Shown::Identified(choice.clone()),
            _ => Shown::Anonymous,
        }
    }
}

/// A source of live positions over a connection that stays open.
pub trait LiveSource: Send + Sync {
    /// The declared facts about the source.
    fn manifest(&self) -> &Manifest;
    /// The host and the port to connect to.
    fn server(&self) -> (&str, u16);
    /// The first line to send, for the positions inside `area`, with its
    /// line ending.
    fn login(&self, area: &BoundingBox) -> String;
    /// A line to send now and then so that the server keeps the connection,
    /// with its line ending.
    fn keepalive(&self) -> String;
    /// What one line that came in at `received` is. A line holds the time
    /// of day only; the date is taken from `received`.
    fn parse_line(&self, line: &str, received: Timestamp) -> LiveLine;
    /// The request for the network's directory of devices.
    fn directory(&self) -> Request;
    /// The directory, from the answer to [`directory`](Self::directory).
    fn parse_directory(&self, body: &[u8]) -> Result<DeviceDirectory, SourceError>;
}
