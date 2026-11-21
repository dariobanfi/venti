//! Web Push: a browser's subscription on the account, so that the server
//! tells it when an alarm fires, even while no Venti page is open.
//!
//! The browser vendors' push services carry the message (RFC 8030); the
//! server encrypts it for the browser (RFC 8291) and signs its request
//! with a key pair of its own (VAPID, RFC 8292), whose public half
//! [`PUSH_KEY_PATH`] hands out for `pushManager.subscribe`. The message
//! is a [`PushMessage`]: the same words the apps show for what fired, by
//! [`fired_words`].

use chrono::Duration;
use serde::{Deserialize, Serialize};
use venti_domain::Timestamp;

use super::{AlertRule, FiredAlert, FiredBecause, ThermalKind, ThermalVerdict};
use super::{alert_clock, weekday_name};

/// Path of the account's push subscriptions: `POST` adds the browser's,
/// `DELETE` removes it.
pub const PUSH_SUBSCRIPTION_PATH: &str = "/v1/account/push";

/// Path of the server's public key for Web Push. Open to everyone.
pub const PUSH_KEY_PATH: &str = "/v1/push/key";

/// The most browsers an account has subscribed.
pub const MAX_PUSH_SUBSCRIPTIONS: usize = 10;

/// The longest endpoint the server keeps, in bytes.
pub const MAX_PUSH_ENDPOINT_LENGTH: usize = 1024;

/// How long a push service holds a message for a browser that is not
/// reachable, in seconds: six hours. What fired later than that is
/// still on the account, for the apps to fetch.
pub const PUSH_TTL_SECONDS: u32 = 6 * 3600;

/// The body of `POST /v1/account/push`: the browser's `PushSubscription`
/// as its `toJSON()` gives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushSubscriptionRequest {
    /// The address of the push service the browser was given.
    pub endpoint: String,
    /// When the subscription ends, by the browser's word; read and not
    /// kept: a push service that ends it says so with `410`.
    #[serde(
        default,
        rename = "expirationTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub expiration_time: Option<f64>,
    /// The browser's keys.
    pub keys: PushKeys,
}

/// A browser's keys for a subscription, base64url as the browser gives
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushKeys {
    /// Its P-256 public key, uncompressed (65 bytes).
    pub p256dh: String,
    /// Its authentication secret (16 bytes).
    pub auth: String,
}

/// The body of `DELETE /v1/account/push`: which subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushRemoveRequest {
    /// The subscription's endpoint.
    pub endpoint: String,
}

/// The answer of `GET /v1/push/key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushKeyResponse {
    /// The server's P-256 public key, uncompressed, base64url without
    /// padding: the `applicationServerKey` of `pushManager.subscribe`.
    pub public_key: String,
}

/// What a push message carries, as JSON, once the browser decrypted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushMessage {
    /// A title: the alarm's name.
    pub title: String,
    /// What happened, in the words of [`fired_words`].
    pub line: String,
    /// The alarm that fired.
    pub alert_id: String,
    /// When the server found it.
    pub fired_at: Timestamp,
}

/// The sixteen points of the compass in words, from the north clockwise.
const POINT_WORDS: [&str; 16] = [
    "the north",
    "the north-north-east",
    "the north-east",
    "the east-north-east",
    "the east",
    "the east-south-east",
    "the south-east",
    "the south-south-east",
    "the south",
    "the south-south-west",
    "the south-west",
    "the west-south-west",
    "the west",
    "the west-north-west",
    "the north-west",
    "the north-north-west",
];

fn point_of(degrees: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let point = ((degrees.rem_euclid(360.0) / 22.5).round() as usize) % 16;
    point
}

fn kind_words(verdict: ThermalVerdict, thermals: Option<ThermalKind>) -> &'static str {
    if verdict != ThermalVerdict::Flyable {
        return "a day to look at";
    }
    match thermals {
        Some(ThermalKind::Big) => "a big day",
        Some(ThermalKind::CrossCountry) => "a cross-country day",
        Some(ThermalKind::Local) => "a day with local thermals",
        Some(ThermalKind::Weak) | None => "a flyable day",
    }
}

/// What an alarm fired for, in words: "Wallberg: 15 km/h from the north at
/// 11:20, as your alarm 'Wallberg north' asks"; "Saturday is a
/// cross-country day at Brauneck by the 06:00 run". `utc_offset_s` is how
/// far the reader's clock is ahead of UTC, for the times.
#[must_use]
pub fn fired_words(fired: &FiredAlert, utc_offset_s: i32) -> String {
    let clock = |time: Timestamp| {
        (time + Duration::seconds(i64::from(utc_offset_s)))
            .format("%H:%M")
            .to_string()
    };
    match (&fired.because, &fired.rule) {
        (
            FiredBecause::Wind {
                time,
                speed_kmh,
                direction_deg,
                gust_kmh,
            },
            AlertRule::Wind(rule),
        ) => {
            let from = direction_deg.map_or_else(String::new, |deg| {
                format!(" from {}", POINT_WORDS[point_of(f64::from(deg))])
            });
            let gusts = match (rule.gusts_max_kmh, gust_kmh) {
                (Some(_), Some(gust)) => format!(", gusts {gust},"),
                _ => String::new(),
            };
            format!(
                "{}: {speed_kmh} km/h{from}{gusts} at {}, as your alarm '{}' asks",
                rule.station_name,
                clock(*time),
                fired.alert_name
            )
        }
        (
            FiredBecause::Day {
                date,
                verdict,
                thermals,
                run,
                ..
            },
            AlertRule::Day(rule),
        ) => format!(
            "{} is {} at {} by the {} run",
            weekday_name(*date),
            kind_words(*verdict, *thermals),
            rule.place_name,
            clock(*run)
        ),
        _ => format!(
            "Your alarm '{}' fired for something this version of the app does not know",
            fired.alert_name
        ),
    }
}

/// How far the clock of alarms ([`alert_clock`]) is ahead of UTC at a
/// moment, in seconds.
#[must_use]
pub fn alert_utc_offset_s(time: Timestamp) -> i32 {
    let ahead = (alert_clock(time) - time.naive_utc()).num_seconds();
    i32::try_from(ahead).unwrap_or(0)
}

impl PushMessage {
    /// The message for what fired, in the words of the clock of alarms.
    #[must_use]
    pub fn of(fired: &FiredAlert) -> Self {
        Self {
            title: fired.alert_name.clone(),
            line: fired_words(fired, alert_utc_offset_s(fired.fired_at)),
            alert_id: fired.alert_id.clone(),
            fired_at: fired.fired_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::GeoPoint;

    use super::super::{DayAlert, DayAtLeast, WindAlert};
    use super::*;

    fn wind_fired() -> FiredAlert {
        FiredAlert {
            alert_id: "a1".to_owned(),
            alert_name: "Wallberg north".to_owned(),
            fired_at: Utc.with_ymd_and_hms(2026, 11, 18, 10, 23, 0).unwrap(),
            rule: AlertRule::Wind(WindAlert {
                station: "dwd-10min:05792".parse().unwrap(),
                station_name: "Wallberg".to_owned(),
                directions: None,
                min_kmh: Some(10),
                max_kmh: None,
                gusts_max_kmh: Some(30),
                hours: None,
            }),
            because: FiredBecause::Wind {
                time: Utc.with_ymd_and_hms(2026, 11, 18, 10, 20, 0).unwrap(),
                speed_kmh: 15,
                gust_kmh: Some(22),
                direction_deg: Some(10),
            },
        }
    }

    #[test]
    fn a_wind_message_is_in_central_european_time() {
        let message = PushMessage::of(&wind_fired());
        assert_eq!(message.title, "Wallberg north");
        assert_eq!(
            message.line,
            "Wallberg: 15 km/h from the north, gusts 22, at 11:20, as your alarm 'Wallberg north' asks"
        );
    }

    #[test]
    fn a_day_message_names_the_day_and_the_run() {
        let fired = FiredAlert {
            alert_id: "a2".to_owned(),
            alert_name: "Brauneck XC".to_owned(),
            // Summer time: two hours ahead.
            fired_at: Utc.with_ymd_and_hms(2026, 7, 2, 5, 0, 0).unwrap(),
            rule: AlertRule::Day(DayAlert {
                place: GeoPoint::new(47.6, 11.5).unwrap(),
                site: None,
                place_name: "Brauneck".to_owned(),
                days_ahead: vec![0, 1],
                at_least: DayAtLeast::CrossCountry,
            }),
            because: FiredBecause::Day {
                date: chrono::NaiveDate::from_ymd_opt(2026, 7, 4).unwrap(),
                verdict: ThermalVerdict::Flyable,
                thermals: Some(ThermalKind::CrossCountry),
                model: venti_domain::ModelId::new("icon-d2").unwrap(),
                run: Utc.with_ymd_and_hms(2026, 7, 2, 3, 0, 0).unwrap(),
            },
        };
        assert_eq!(
            PushMessage::of(&fired).line,
            "Saturday is a cross-country day at Brauneck by the 05:00 run"
        );
    }

    #[test]
    fn the_browsers_subscription_is_read_as_it_sends_it() {
        let text = r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/abc","expirationTime":null,"keys":{"p256dh":"BCVx","auth":"BTBZ"}}"#;
        let request: PushSubscriptionRequest = serde_json::from_str(text).unwrap();
        assert_eq!(request.endpoint, "https://fcm.googleapis.com/fcm/send/abc");
        assert_eq!(request.keys.auth, "BTBZ");
        assert_eq!(request.expiration_time, None);
    }
}
