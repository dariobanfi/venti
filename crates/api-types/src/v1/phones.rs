//! Push to the phone apps: a phone registers the token its platform's
//! push service gave it (Apple's APNs on an iPhone, Google's Firebase
//! Cloud Messaging on Android) for the signed-in account, and the server
//! tells it what concerns its pilot: an alarm that fired, an open plan
//! posted near a place the pilot chose, a pilot nearby asking for help
//! after landing, and a pilot answering the pilot's own call for help.
//!
//! What the phone is told comes as a [`PhoneMessage`]: a title and a line
//! the system shows as they are, and what it is about, for the app to
//! open when the pilot taps it.
//!
//! Two settings, both off until the pilot turns them on (owner decision
//! 15): a circle the pilot wants to hear of open plans in, and whether
//! pilots nearby may ask them for help. A pilot who may be asked tells the
//! server, now and then, a coarse position ([`coarse_position`]), kept for
//! [`POSITION_KEPT_MINUTES`] and shown to nobody.

use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

/// Path of the phones registered on the account: `POST` registers this
/// phone's token, `DELETE` removes it.
pub const DEVICES_PATH: &str = "/v1/account/devices";

/// Path of the account's settings for what pilots nearby do: open plans
/// near a place, and being asked for help.
pub const NEARBY_PATH: &str = "/v1/account/nearby";

/// Path of the pilot's coarse position, for being asked for help: `PUT`
/// tells it, `DELETE` forgets it.
pub const POSITION_PATH: &str = "/v1/account/position";

/// The most phones an account has registered.
pub const MAX_DEVICES: usize = 10;

/// The longest token the server keeps, in bytes. An APNs token is 64
/// hexadecimal characters, an FCM token some 160 characters; both
/// platforms say a token's length may change.
pub const MAX_DEVICE_TOKEN_LENGTH: usize = 512;

/// How long a push service holds a message for a phone that is off, in
/// seconds: six hours, as for Web Push.
pub const PHONE_PUSH_TTL_SECONDS: u32 = 6 * 3600;

/// How far around the pilot's circle may reach, in kilometres.
pub const MAX_NEARBY_RADIUS_KM: u32 = 100;

/// The radius an app offers first, in kilometres.
pub const DEFAULT_NEARBY_RADIUS_KM: u32 = 30;

/// The grid a pilot's position is taken to before it is kept, in degrees:
/// some 5.5 km north to south and 3.7 km east to west in the Alps.
pub const POSITION_GRID_DEG: f64 = 0.05;

/// How long the server keeps a pilot's coarse position, in minutes.
pub const POSITION_KEPT_MINUTES: i64 = 60;

/// Whose push service a phone's token is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushPlatform {
    /// Apple's push service, APNs: an iPhone.
    Apns,
    /// Google's Firebase Cloud Messaging: an Android phone.
    Fcm,
}

impl PushPlatform {
    /// The word the API and the store use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apns => "apns",
            Self::Fcm => "fcm",
        }
    }

    /// The platform of a word, if it is one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "apns" => Some(Self::Apns),
            "fcm" => Some(Self::Fcm),
            _ => None,
        }
    }
}

/// The body of `POST /v1/account/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRequest {
    /// Whose push service the token is of.
    pub platform: PushPlatform,
    /// The token the platform gave the app: on an iPhone the device token
    /// in hexadecimal, on Android the FCM registration token.
    pub token: String,
    /// An iPhone app built for development, whose token is of Apple's
    /// sandbox. Left out, or `false`, for every other.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sandbox: bool,
}

/// The body of `DELETE /v1/account/devices`: which phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRemoveRequest {
    /// Whose push service the token is of.
    pub platform: PushPlatform,
    /// The token.
    pub token: String,
}

/// A circle on the map: where, and how far around.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NearbyCircle {
    /// Its middle.
    pub position: GeoPoint,
    /// How far around, in kilometres, 1 to [`MAX_NEARBY_RADIUS_KM`].
    pub radius_km: u32,
}

/// The account's settings for what pilots nearby do: the body of
/// `PUT /v1/account/nearby`, sent whole, and its answer and that of
/// `GET`. Both are off for an account that never set them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct NearbySettings {
    /// Where the pilot wants to be told of a new open plan; `null` for
    /// nowhere.
    #[serde(default)]
    pub open_plans_near: Option<NearbyCircle>,
    /// Whether pilots nearby who have landed may ask this pilot for help:
    /// a lift, or a hand with the wing. The pilot's phone then tells the
    /// server a coarse position now and then.
    #[serde(default)]
    pub may_be_asked_for_help: bool,
}

/// The body of `PUT /v1/account/position`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PositionRequest {
    /// Where the phone is. The server takes it to [`POSITION_GRID_DEG`]
    /// before it keeps it.
    pub position: GeoPoint,
}

/// A position taken to [`POSITION_GRID_DEG`]: the middle of its square
/// of the grid, so that the kept one says no more than which square.
#[must_use]
pub fn coarse_position(position: GeoPoint) -> GeoPoint {
    let snap = |degrees: f64| ((degrees / POSITION_GRID_DEG).floor() + 0.5) * POSITION_GRID_DEG;
    let lat = snap(position.lat()).clamp(-90.0, 90.0);
    let mut lon = snap(position.lon());
    if lon > 180.0 {
        lon -= 360.0;
    }
    // Rounded to what the grid can say: no tail of binary fractions.
    let round = |degrees: f64| (degrees * 1000.0).round() / 1000.0;
    GeoPoint::new(round(lat), round(lon)).unwrap_or(position)
}

/// What a phone is told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhoneMessageKind {
    /// An alarm fired; `id` is the alarm's.
    Alert,
    /// An open plan was posted in the pilot's circle; `id` is the plan's.
    OpenPlan,
    /// A pilot nearby asks for help; `id` is the request's.
    HelpRequest,
    /// A pilot answered "I can help" on the pilot's own request; `id` is
    /// the request's.
    HelpAnswer,
    /// A new message in one of the pilot's groups; `id` is the group's.
    GroupMessage,
}

impl PhoneMessageKind {
    /// The word a message carries.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::OpenPlan => "open_plan",
            Self::HelpRequest => "help_request",
            Self::HelpAnswer => "help_answer",
            Self::GroupMessage => "group_message",
        }
    }
}

/// What a phone is told: the notification's title and line, as the system
/// shows them, and what it is about. On an iPhone the title and line are
/// the `aps.alert`, and `kind`, `id` and `sent_at` are keys of their own
/// beside `aps`; on Android they are the FCM message's `notification` and
/// its `data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneMessage {
    /// The title.
    pub title: String,
    /// The line under it.
    pub body: String,
    /// What it is about.
    pub kind: PhoneMessageKind,
    /// The identifier of what it is about.
    pub id: String,
    /// When the server sent it.
    pub sent_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_kept_as_the_middle_of_its_square() {
        let kept = coarse_position(GeoPoint::new(47.2694, 11.4041).unwrap());
        assert_eq!((kept.lat(), kept.lon()), (47.275, 11.425));
        let west = coarse_position(GeoPoint::new(-0.01, -0.01).unwrap());
        assert_eq!((west.lat(), west.lon()), (-0.025, -0.025));
        let edge = coarse_position(GeoPoint::new(89.99, 179.99).unwrap());
        assert!(edge.lat() <= 90.0 && edge.lon() <= 180.0);
    }

    #[test]
    fn a_device_is_read_as_the_apps_send_it() {
        let request: DeviceRequest =
            serde_json::from_str(r#"{"platform":"apns","token":"ab12","sandbox":true}"#).unwrap();
        assert_eq!(request.platform, PushPlatform::Apns);
        assert!(request.sandbox);
        let android: DeviceRequest =
            serde_json::from_str(r#"{"platform":"fcm","token":"x:y"}"#).unwrap();
        assert!(!android.sandbox);
        assert_eq!(
            serde_json::to_string(&android).unwrap(),
            r#"{"platform":"fcm","token":"x:y"}"#
        );
    }

    #[test]
    fn settings_left_out_are_off() {
        let settings: NearbySettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, NearbySettings::default());
    }
}
