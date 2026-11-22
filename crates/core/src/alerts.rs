//! The pilot's alarms, for an "Alert me" on a station's page and a
//! take-off's page, a list under the account, and what fired.
//!
//! An alarm lives on the account: the server checks it where the data
//! arrives and keeps what fired (`docs/api.md`, "Alarms"). Here are the
//! choices and the words, the check of a draft, the requests and the
//! readers of the answers, and [`AlertsSync`], which brings the list and
//! what fired to the device, so that both show without a network. The
//! core does no I/O: it hands the shell a [`ServerRequest`] and reads the
//! answer. Signed out there are no alarms.
//!
//! A shell makes an [`AlertDraft`] from what the pilot chose, shows
//! [`alert_line`] as it changes, [`alert_check`] before sending, and sends
//! [`alert_add_request`] or [`alert_change_request`].

use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use venti_api_types::v1::{
    ALERTS_PATH, Alert, AlertHours, AlertRequest, AlertResponse, AlertRule, AlertSector,
    AlertsResponse, DayAlert, DayAtLeast, FiredAlert, FiredAlertsRequest, FiredAlertsResponse,
    PILOTS_ALERT_DEFAULT_PILOTS, PILOTS_ALERT_DEFAULT_REST_HOURS, PilotsAlert, WindAlert,
    alert_path, fired_words,
};
use venti_domain::GeoPoint;

use crate::account::{Halt, asked_to_wait, failed, fetched, json_request, settled};
use crate::server::{answer, refusal};
use crate::{Core, CoreError, ServerRequest, SyncStep, SyncSummary};

/// The file the device keeps the account's alarms and what fired in.
const FILE: &str = "alerts-account.json";

/// How many of what fired the device keeps: the newest.
const FIRED_HELD: usize = 200;

/// The sixteen points of the compass, short, from the north clockwise.
const POINTS: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];

/// What an alarm waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum AlertKind {
    /// The wind at a station.
    Wind,
    /// The kind of day at a place.
    Day,
    /// Pilots in the air at a place.
    Pilots,
}

/// The least a day is to be, for a day alarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum AlertDayKind {
    /// Flyable by the air alone.
    Flyable,
    /// Flyable, with thermals to stay up in.
    Local,
    /// Flyable, a cross-country day or better.
    CrossCountry,
    /// Flyable, a big day.
    Big,
}

/// An alarm as the pilot sets it, in the choices of the form. What a kind
/// does not use is left as it is and not sent.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AlertDraft {
    /// What the pilot calls it: "Wallberg north".
    #[uniffi(default = "")]
    pub name: String,
    /// Whether it is on.
    #[uniffi(default = true)]
    pub enabled: bool,
    /// Which kind it is.
    pub kind: AlertKind,
    /// Wind: the station's identifier, as the station's page has it.
    #[uniffi(default = "")]
    pub station_id: String,
    /// Wind: the station's name.
    #[uniffi(default = "")]
    pub station_name: String,
    /// Wind: the first of the sixteen points the wind may come from, 0 for
    /// north, 1 for north-north-east, clockwise. `None` with `to_point`
    /// for any direction.
    #[uniffi(default = None)]
    pub from_point: Option<u8>,
    /// Wind: the last of them, clockwise from `from_point`; the same point
    /// for one point alone.
    #[uniffi(default = None)]
    pub to_point: Option<u8>,
    /// Wind: the least mean wind, km/h.
    #[uniffi(default = None)]
    pub min_kmh: Option<u16>,
    /// Wind: the most mean wind, km/h.
    #[uniffi(default = None)]
    pub max_kmh: Option<u16>,
    /// Wind: the strongest gust there may be, km/h.
    #[uniffi(default = None)]
    pub gusts_max_kmh: Option<u16>,
    /// Wind: only from this hour, 0 to 23, Central European Time.
    #[uniffi(default = None)]
    pub from_hour: Option<u8>,
    /// Wind: to this hour, 1 to 24.
    #[uniffi(default = None)]
    pub to_hour: Option<u8>,
    /// Day: where, latitude.
    #[uniffi(default = 0.0)]
    pub lat: f64,
    /// Day: where, longitude.
    #[uniffi(default = 0.0)]
    pub lon: f64,
    /// Day: the site, if the place is one.
    #[uniffi(default = None)]
    pub site_id: Option<String>,
    /// Day: the place's name: "Brauneck".
    #[uniffi(default = "")]
    pub place_name: String,
    /// Day: which days, 0 for today to 3.
    pub days_ahead: Vec<u8>,
    /// Day: the least the day is to be.
    pub at_least: AlertDayKind,
    /// Pilots: the fewest pilots in the air; `None` for three. The place
    /// is the day alarm's: `lat`, `lon`, `site_id`, `place_name`.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub min_pilots: Option<u16>,
    /// Pilots: for how many hours it rests after it fired; `None` for
    /// six.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub rest_hours: Option<u8>,
}

/// What is wrong with a draft, for the field it is about.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AlertFaultView {
    /// The field, as the server names it: `rule.min_kmh`.
    #[uniffi(default = "")]
    pub field: String,
    /// What is wrong, in words.
    #[uniffi(default = "")]
    pub message: String,
}

/// An alarm in the list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AlertView {
    /// Its identifier, for changing and removing it.
    #[uniffi(default = "")]
    pub id: String,
    /// What the pilot calls it.
    #[uniffi(default = "")]
    pub name: String,
    /// Whether it is on.
    #[uniffi(default = false)]
    pub enabled: bool,
    /// What it waits for, in one line.
    #[uniffi(default = "")]
    pub line: String,
    /// The alarm as a draft, for the form that changes it. `None` for a
    /// kind this version does not know: it can be removed, not changed.
    #[uniffi(default = None)]
    pub draft: Option<AlertDraft>,
}

/// One thing an alarm fired for, as a line.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FiredLine {
    /// The alarm that fired.
    #[uniffi(default = "")]
    pub alert_id: String,
    /// A title: the alarm's name.
    #[uniffi(default = "")]
    pub title: String,
    /// What happened: "Wallberg: 15 km/h from the north at 11:20, as your
    /// alarm 'Wallberg north' asks".
    #[uniffi(default = "")]
    pub line: String,
    /// When the server found it, seconds since 1970: for the order and
    /// for a notification's time.
    #[uniffi(default = 0)]
    pub fired_at_unix_s: i64,
}

/// What fired, a page of it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FiredPage {
    /// What fired, the oldest first.
    #[uniffi(default = [])]
    pub lines: Vec<FiredLine>,
    /// What to pass as `since` next time.
    #[uniffi(default = "")]
    pub cursor: String,
    /// Whether more waits: ask again at once.
    #[uniffi(default = false)]
    pub more: bool,
}

/// The sixteen points of the compass, short, from the north clockwise:
/// the choices for `from_point` and `to_point`.
#[uniffi::export]
pub fn alert_points() -> Vec<String> {
    POINTS.iter().map(|point| (*point).to_owned()).collect()
}

fn round_deg(degrees: f64) -> u16 {
    // 0 to 360, and 360 is 0.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = degrees.rem_euclid(360.0).round() as u16;
    whole % 360
}

fn point_of(degrees: f64) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let point = ((degrees.rem_euclid(360.0) / 22.5).round() as u8) % 16;
    point
}

fn sector(from: u8, to: u8) -> AlertSector {
    AlertSector {
        from_deg: round_deg(f64::from(from % 16) * 22.5 - 11.25),
        to_deg: round_deg(f64::from(to % 16) * 22.5 + 11.25),
    }
}

fn at_least(kind: AlertDayKind) -> DayAtLeast {
    match kind {
        AlertDayKind::Flyable => DayAtLeast::Flyable,
        AlertDayKind::Local => DayAtLeast::Local,
        AlertDayKind::CrossCountry => DayAtLeast::CrossCountry,
        AlertDayKind::Big => DayAtLeast::Big,
    }
}

fn day_kind(least: DayAtLeast) -> AlertDayKind {
    match least {
        DayAtLeast::Flyable => AlertDayKind::Flyable,
        DayAtLeast::Local => AlertDayKind::Local,
        DayAtLeast::CrossCountry => AlertDayKind::CrossCountry,
        DayAtLeast::Big => AlertDayKind::Big,
    }
}

/// The request a draft makes, or why it makes none.
fn request_of(draft: &AlertDraft) -> Result<AlertRequest, CoreError> {
    let invalid = |message: &str| CoreError::Server {
        message: message.to_owned(),
    };
    let rule = match draft.kind {
        AlertKind::Wind => AlertRule::Wind(WindAlert {
            station: draft
                .station_id
                .parse()
                .map_err(|_| invalid("Choose a station."))?,
            station_name: draft.station_name.trim().to_owned(),
            directions: match (draft.from_point, draft.to_point) {
                (Some(from), Some(to)) => Some(sector(from, to)),
                (Some(one), None) | (None, Some(one)) => Some(sector(one, one)),
                (None, None) => None,
            },
            min_kmh: draft.min_kmh,
            max_kmh: draft.max_kmh,
            gusts_max_kmh: draft.gusts_max_kmh,
            hours: match (draft.from_hour, draft.to_hour) {
                (None, None) => None,
                (from, to) => Some(AlertHours {
                    from_hour: from.unwrap_or(0),
                    to_hour: to.unwrap_or(24),
                }),
            },
        }),
        AlertKind::Day => AlertRule::Day(DayAlert {
            place: GeoPoint::new(draft.lat, draft.lon).map_err(|_| invalid("Choose a place."))?,
            site: draft.site_id.as_deref().and_then(|id| id.parse().ok()),
            place_name: draft.place_name.trim().to_owned(),
            days_ahead: draft.days_ahead.clone(),
            at_least: at_least(draft.at_least),
        }),
        AlertKind::Pilots => AlertRule::Pilots(PilotsAlert {
            place: GeoPoint::new(draft.lat, draft.lon).map_err(|_| invalid("Choose a place."))?,
            site: draft.site_id.as_deref().and_then(|id| id.parse().ok()),
            place_name: draft.place_name.trim().to_owned(),
            min_pilots: draft.min_pilots.unwrap_or(PILOTS_ALERT_DEFAULT_PILOTS),
            rest_hours: draft.rest_hours.unwrap_or(PILOTS_ALERT_DEFAULT_REST_HOURS),
        }),
    };
    Ok(AlertRequest {
        name: draft.name.trim().to_owned(),
        enabled: draft.enabled,
        rule,
    })
}

/// The draft an alarm of the account makes, for the form. `None` for a
/// kind this version does not know.
fn draft_of(alert: &Alert) -> Option<AlertDraft> {
    let mut draft = AlertDraft {
        name: alert.name.clone(),
        enabled: alert.enabled,
        kind: AlertKind::Wind,
        station_id: String::new(),
        station_name: String::new(),
        from_point: None,
        to_point: None,
        min_kmh: None,
        max_kmh: None,
        gusts_max_kmh: None,
        from_hour: None,
        to_hour: None,
        lat: 0.0,
        lon: 0.0,
        site_id: None,
        place_name: String::new(),
        days_ahead: Vec::new(),
        at_least: AlertDayKind::CrossCountry,
        min_pilots: None,
        rest_hours: None,
    };
    match &alert.rule {
        AlertRule::Wind(wind) => {
            draft.station_id = wind.station.to_string();
            draft.station_name.clone_from(&wind.station_name);
            if let Some(sector) = wind.directions {
                draft.from_point = Some(point_of(f64::from(sector.from_deg) + 11.25));
                draft.to_point = Some(point_of(f64::from(sector.to_deg) - 11.25));
            }
            draft.min_kmh = wind.min_kmh;
            draft.max_kmh = wind.max_kmh;
            draft.gusts_max_kmh = wind.gusts_max_kmh;
            draft.from_hour = wind.hours.map(|hours| hours.from_hour);
            draft.to_hour = wind.hours.map(|hours| hours.to_hour);
        }
        AlertRule::Day(day) => {
            draft.kind = AlertKind::Day;
            draft.lat = day.place.lat();
            draft.lon = day.place.lon();
            draft.site_id = day.site.as_ref().map(ToString::to_string);
            draft.place_name.clone_from(&day.place_name);
            draft.days_ahead.clone_from(&day.days_ahead);
            draft.at_least = day_kind(day.at_least);
        }
        AlertRule::Pilots(pilots) => {
            draft.kind = AlertKind::Pilots;
            draft.lat = pilots.place.lat();
            draft.lon = pilots.place.lon();
            draft.site_id = pilots.site.as_ref().map(ToString::to_string);
            draft.place_name.clone_from(&pilots.place_name);
            draft.min_pilots = Some(pilots.min_pilots);
            draft.rest_hours = Some(pilots.rest_hours);
        }
        AlertRule::Other => return None,
    }
    Some(draft)
}

fn sector_words(sector: AlertSector) -> String {
    let from = point_of(f64::from(sector.from_deg) + 11.25);
    let to = point_of(f64::from(sector.to_deg) - 11.25);
    if from == to {
        format!("from {}", POINTS[usize::from(from)])
    } else {
        format!(
            "from {} to {}",
            POINTS[usize::from(from)],
            POINTS[usize::from(to)]
        )
    }
}

fn day_words(days: &[u8]) -> String {
    let mut days = days.to_vec();
    days.sort_unstable();
    let names: Vec<String> = days
        .iter()
        .map(|day| match day {
            0 => "today".to_owned(),
            1 => "tomorrow".to_owned(),
            n => format!("in {n} days"),
        })
        .collect();
    match names.as_slice() {
        [] => "on no day".to_owned(),
        [one] => one.clone(),
        [first @ .., last] => format!("{} or {last}", first.join(", ")),
    }
}

fn least_words(least: DayAtLeast) -> &'static str {
    match least {
        DayAtLeast::Flyable => "A flyable day",
        DayAtLeast::Local => "A day with local thermals or better",
        DayAtLeast::CrossCountry => "A cross-country day or better",
        DayAtLeast::Big => "A big day",
    }
}

fn rule_line(rule: &AlertRule) -> String {
    match rule {
        AlertRule::Wind(wind) => {
            let mut parts = vec![format!("Wind at {}", wind.station_name)];
            if let Some(sector) = wind.directions {
                parts[0] = format!("{} {}", parts[0], sector_words(sector));
            }
            match (wind.min_kmh, wind.max_kmh) {
                (Some(min), Some(max)) => parts.push(format!("{min} to {max} km/h")),
                (Some(min), None) => parts.push(format!("at least {min} km/h")),
                (None, Some(max)) => parts.push(format!("at most {max} km/h")),
                (None, None) => {}
            }
            if let Some(gusts) = wind.gusts_max_kmh {
                let unit = if parts.len() > 1 { "" } else { " km/h" };
                parts.push(format!("gusts at most {gusts}{unit}"));
            }
            if let Some(hours) = wind.hours {
                parts.push(format!(
                    "between {:02}:00 and {:02}:00",
                    hours.from_hour, hours.to_hour
                ));
            }
            parts.join(", ")
        }
        AlertRule::Day(day) => format!(
            "{} at {}, {}",
            least_words(day.at_least),
            day.place_name,
            day_words(&day.days_ahead)
        ),
        AlertRule::Pilots(pilots) => format!(
            "{} in the air at {}, at most once in {}",
            match pilots.min_pilots {
                1 => "A pilot".to_owned(),
                n => format!("{n} pilots or more"),
            },
            pilots.place_name,
            match pilots.rest_hours {
                1 => "an hour".to_owned(),
                n => format!("{n} hours"),
            }
        ),
        AlertRule::Other => "An alarm this version of the app does not know".to_owned(),
    }
}

/// What a draft waits for, in one line: "Wind at Wallberg from N to NE,
/// 10 to 20 km/h, gusts at most 30, between 10:00 and 16:00". For a draft
/// that makes no alarm, what is missing.
#[uniffi::export]
pub fn alert_line(draft: AlertDraft) -> String {
    match request_of(&draft) {
        Ok(request) => rule_line(&request.rule),
        Err(error) => error.to_string(),
    }
}

/// Everything that is wrong with a draft, by the rules the server holds
/// to. Empty when it may be sent.
#[uniffi::export]
pub fn alert_check(draft: AlertDraft) -> Vec<AlertFaultView> {
    let request = match request_of(&draft) {
        Ok(request) => request,
        Err(error) => {
            let field = match draft.kind {
                AlertKind::Wind => "rule.station",
                AlertKind::Day | AlertKind::Pilots => "rule.place",
            };
            return vec![AlertFaultView {
                field: field.to_owned(),
                message: error.to_string(),
            }];
        }
    };
    request
        .check()
        .into_iter()
        .map(|fault| AlertFaultView {
            field: fault.field.to_owned(),
            message: format!("{}: {}", field_words(fault.field), fault.reason),
        })
        .collect()
}

/// The fields a fault can be about, as the server names them, each with
/// its name in words, in the form's order. The last is said of a field
/// this version does not know.
const FIELDS: [(&str, &str); 15] = [
    ("name", "The name"),
    ("rule.kind", "The kind of alarm"),
    ("rule.station", "The station"),
    ("rule.station_name", "The station's name"),
    ("rule.directions", "The directions"),
    ("rule.min_kmh", "The least wind"),
    ("rule.max_kmh", "The most wind"),
    ("rule.gusts_max_kmh", "The gusts"),
    ("rule.hours", "The hours"),
    ("rule.place", "The place"),
    ("rule.place_name", "The place's name"),
    ("rule.days_ahead", "The days"),
    ("rule.min_pilots", "The number of pilots"),
    ("rule.rest_hours", "The hours of quiet"),
    ("rule", "The alarm"),
];

fn field_words(field: &str) -> &'static str {
    FIELDS
        .iter()
        .find(|(name, _)| *name == field)
        .map_or("The alarm", |(_, words)| words)
}

/// The request for the account's alarms.
#[uniffi::export]
pub fn alerts_request() -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: ALERTS_PATH.to_owned(),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// The request that sets a new alarm.
#[uniffi::export]
pub fn alert_add_request(draft: AlertDraft) -> Result<ServerRequest, CoreError> {
    json_request("POST", ALERTS_PATH, &request_of(&draft)?, true)
}

/// The request that changes an alarm, by the identifier the list gives.
#[uniffi::export]
pub fn alert_change_request(id: String, draft: AlertDraft) -> Result<ServerRequest, CoreError> {
    json_request("PUT", &alert_path(&id), &request_of(&draft)?, true)
}

/// The request that removes an alarm.
#[uniffi::export]
pub fn alert_remove_request(id: String) -> ServerRequest {
    ServerRequest {
        method: "DELETE".to_owned(),
        path: alert_path(&id),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// Reads the answer to [`alert_remove_request`] and says what became of
/// the alarm. One that was gone already is gone: that is no error.
///
/// # Errors
///
/// The server's refusal, in words.
#[uniffi::export]
pub fn alert_removed(status: u16, body: Vec<u8>) -> Result<String, CoreError> {
    if (200..300).contains(&status) || status == 404 {
        Ok("The alarm is deleted.".to_owned())
    } else {
        Err(refusal(status, &body))
    }
}

// --- The words of the screens -------------------------------------------------

/// A kind of day as the form offers it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AlertDayKindChoice {
    /// The draft's value.
    pub kind: AlertDayKind,
    /// Its name: "Cross-country".
    #[uniffi(default = "")]
    pub name: String,
}

/// A field a fault can be about.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AlertFieldName {
    /// The key, as [`AlertFaultView::field`] has it: `rule.min_kmh`. It
    /// does not change; a shell puts a fault at the control of this key.
    #[uniffi(default = "")]
    pub field: String,
    /// The field in words, as a fault's message begins: "The least
    /// wind".
    #[uniffi(default = "")]
    pub name: String,
}

/// The words of "Alert me", of the form, of the list under the account
/// and of what fired: the same in the three apps. What an alarm waits
/// for, what fired and what is wrong with a draft are worded where they
/// are made ([`alert_line`], [`FiredLine`], [`alert_check`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AlertFormWords {
    /// The entry on a station's and a take-off's page, and the title of
    /// the form for a new alarm: "Alert me".
    #[uniffi(default = "")]
    pub alert_me: String,
    /// The title of the form that changes an alarm: "Change the alarm".
    #[uniffi(default = "")]
    pub change_title: String,
    /// The entry under the account and the title of its screen:
    /// "Alerts".
    #[uniffi(default = "")]
    pub title: String,
    /// The title over the list: "Your alarms".
    #[uniffi(default = "")]
    pub list_title: String,
    /// What the list says when it is empty.
    #[uniffi(default = "")]
    pub list_empty: String,
    /// The title over what fired: "What fired".
    #[uniffi(default = "")]
    pub fired_title: String,
    /// What it says when nothing fired: "Nothing has fired in the last
    /// 30 days."
    #[uniffi(default = "")]
    pub fired_empty: String,
    /// The note under what fired: where the alarms are checked and how
    /// long what fired is kept.
    #[uniffi(default = "")]
    pub fired_note: String,
    /// What "Alert me" says to a pilot signed out: "Alarms live on your
    /// account: sign in to set one."
    #[uniffi(default = "")]
    pub signed_out: String,
    /// "Name".
    #[uniffi(default = "")]
    pub name: String,
    /// "Station", over a wind alarm's station.
    #[uniffi(default = "")]
    pub station: String,
    /// "Place", over a day alarm's place.
    #[uniffi(default = "")]
    pub place: String,
    /// The alarm's switch: "On".
    #[uniffi(default = "")]
    pub on: String,
    /// The first of the points: "Wind from".
    #[uniffi(default = "")]
    pub wind_from: String,
    /// The last of them: "To".
    #[uniffi(default = "")]
    pub wind_to: String,
    /// The choice of no direction, of no bound and of no hour: "Any".
    #[uniffi(default = "")]
    pub any: String,
    /// "Least wind".
    #[uniffi(default = "")]
    pub least: String,
    /// "Most wind".
    #[uniffi(default = "")]
    pub most: String,
    /// "Gusts at most".
    #[uniffi(default = "")]
    pub gusts_at_most: String,
    /// The unit of the three: "km/h".
    #[uniffi(default = "")]
    pub kmh: String,
    /// The title over the hours, with their time zone: "Hours, Central
    /// European Time".
    #[uniffi(default = "")]
    pub hours_title: String,
    /// The first hour: "From".
    #[uniffi(default = "")]
    pub from_hour: String,
    /// The last: "To".
    #[uniffi(default = "")]
    pub to_hour: String,
    /// The title over the days: "Days".
    #[uniffi(default = "")]
    pub days: String,
    /// The names of the days, at the index of the draft's value in
    /// `days_ahead`: "Today", "Tomorrow", "In 2 days", "In 3 days".
    #[uniffi(default = [])]
    pub day_names: Vec<String>,
    /// The title over the kinds of day: "At least".
    #[uniffi(default = "")]
    pub at_least: String,
    /// The kinds of day, the least first.
    #[uniffi(default = [])]
    pub day_kinds: Vec<AlertDayKindChoice>,
    /// The choice of the kind of an alarm about pilots in the air, where
    /// a form offers the kinds: "Pilots in the air".
    #[uniffi(default = "")]
    pub pilots_kind: String,
    /// Over the number of pilots: "Pilots in the air, at least".
    #[uniffi(default = "")]
    pub pilots_at_least: String,
    /// Over the hours it rests: "Then quiet for, hours".
    #[uniffi(default = "")]
    pub pilots_rest: String,
    /// The note under the two: how far from the place a pilot counts and
    /// whose positions are counted.
    #[uniffi(default = "")]
    pub pilots_note: String,
    /// "Save".
    #[uniffi(default = "")]
    pub save: String,
    /// The button that deletes an alarm: "Delete alarm".
    #[uniffi(default = "")]
    pub delete: String,
    /// The question before it: "Delete this alarm?"
    #[uniffi(default = "")]
    pub delete_question: String,
    /// The fields a fault can be about, in the form's order, for a shell
    /// that puts a fault at its field.
    #[uniffi(default = [])]
    pub fields: Vec<AlertFieldName>,
}

/// The words of the alarms' screens.
#[uniffi::export]
pub fn alert_form_words() -> AlertFormWords {
    let text = str::to_owned;
    AlertFormWords {
        alert_me: text("Alert me"),
        change_title: text("Change the alarm"),
        title: text("Alerts"),
        list_title: text("Your alarms"),
        list_empty: text(
            "No alarms yet. Set one with \u{201c}Alert me\u{201d} on a wind station\u{2019}s \
             page or a take-off\u{2019}s page.",
        ),
        fired_title: text("What fired"),
        fired_empty: format!(
            "Nothing has fired in the last {} days.",
            venti_api_types::v1::FIRED_KEPT_DAYS
        ),
        fired_note: format!(
            "Venti checks your alarms where the data arrives, and keeps what they fired for {} \
             days.",
            venti_api_types::v1::FIRED_KEPT_DAYS
        ),
        signed_out: text("Alarms live on your account: sign in to set one."),
        name: text("Name"),
        station: text("Station"),
        place: text("Place"),
        on: text("On"),
        wind_from: text("Wind from"),
        wind_to: text("To"),
        any: text("Any"),
        least: text("Least wind"),
        most: text("Most wind"),
        gusts_at_most: text("Gusts at most"),
        kmh: text("km/h"),
        hours_title: text("Hours, Central European Time"),
        from_hour: text("From"),
        to_hour: text("To"),
        days: text("Days"),
        day_names: (0..=venti_api_types::v1::MAX_DAYS_AHEAD)
            .map(|day| match day {
                0 => text("Today"),
                1 => text("Tomorrow"),
                n => format!("In {n} days"),
            })
            .collect(),
        at_least: text("At least"),
        day_kinds: [
            (AlertDayKind::Flyable, "Flyable"),
            (AlertDayKind::Local, "Local thermals"),
            (AlertDayKind::CrossCountry, "Cross-country"),
            (AlertDayKind::Big, "Big day"),
        ]
        .into_iter()
        .map(|(kind, name)| AlertDayKindChoice {
            kind,
            name: text(name),
        })
        .collect(),
        pilots_kind: text("Pilots in the air"),
        pilots_at_least: text("Pilots in the air, at least"),
        pilots_rest: text("Then quiet for, hours"),
        pilots_note: format!(
            "Counts paragliders and hang gliders flying within {} km of the place: those the \
             Open Glider Network hears and Venti’s pilots who show their flight to \
             everyone. Venti looks once a minute.",
            venti_api_types::v1::SITE_AIR_WITHIN_M / 1000.0
        ),
        save: text("Save"),
        delete: text("Delete alarm"),
        delete_question: text("Delete this alarm?"),
        fields: FIELDS
            .iter()
            .map(|(field, name)| AlertFieldName {
                field: text(field),
                name: text(name),
            })
            .collect(),
    }
}

fn view(alert: &Alert) -> AlertView {
    AlertView {
        id: alert.id.clone(),
        name: alert.name.clone(),
        enabled: alert.enabled,
        line: rule_line(&alert.rule),
        draft: draft_of(alert),
    }
}

/// Reads the answer to [`alerts_request`]: the alarms, the oldest first.
#[uniffi::export]
pub fn alerts(status: u16, body: Vec<u8>) -> Result<Vec<AlertView>, CoreError> {
    let response: AlertsResponse = answer(status, &body)?;
    Ok(response.alerts.iter().map(view).collect())
}

/// Reads the answer to [`alert_add_request`] or [`alert_change_request`]:
/// the alarm as the account has it now.
#[uniffi::export]
pub fn alert_saved(status: u16, body: Vec<u8>) -> Result<AlertView, CoreError> {
    let response: AlertResponse = answer(status, &body)?;
    Ok(view(&response.alert))
}

/// The request for what the alarms fired since `since`, the cursor of the
/// page before; `None` for everything the server keeps.
#[uniffi::export]
pub fn alerts_fired_request(since: Option<String>) -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: FiredAlertsRequest {
            since,
            limit: Some(100),
        }
        .path_and_query(),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// One thing an alarm fired for, in words. `utc_offset_s` is how far the
/// device's clock is ahead of UTC. The words are the server's too, for
/// Web Push: `venti_api_types::v1::fired_words`.
fn fired_line(fired: &FiredAlert, utc_offset_s: i32) -> FiredLine {
    FiredLine {
        alert_id: fired.alert_id.clone(),
        title: fired.alert_name.clone(),
        line: fired_words(fired, utc_offset_s),
        fired_at_unix_s: fired.fired_at.timestamp(),
    }
}

/// Reads the answer to [`alerts_fired_request`]. `utc_offset_s` is how
/// far the device's clock is ahead of UTC, for the times in the lines.
#[uniffi::export]
pub fn alerts_fired(status: u16, body: Vec<u8>, utc_offset_s: i32) -> Result<FiredPage, CoreError> {
    let response: FiredAlertsResponse = answer(status, &body)?;
    Ok(FiredPage {
        lines: response
            .fired
            .iter()
            .map(|fired| fired_line(fired, utc_offset_s))
            .collect(),
        cursor: response.cursor,
        more: response.more,
    })
}

// --- Held on the device ------------------------------------------------------

/// What the device holds of the account's alarms, as stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Held {
    server: String,
    handle: String,
    #[serde(default)]
    alerts: Vec<Alert>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    fired: Vec<FiredAlert>,
}

fn storage_error(error: std::io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("Your alarms could not be read or written: {error}."),
    }
}

impl Core {
    /// What the device holds for the account it is signed in to, or
    /// nothing held yet. `None` signed out.
    fn held_alerts(&self) -> Result<Option<Held>, CoreError> {
        let Some((server, handle, _)) = self.signed_in_to()? else {
            return Ok(None);
        };
        let held = self
            .storage
            .read(FILE)
            .map_err(storage_error)?
            // The app's own writing; damaged, it is fetched again.
            .and_then(|bytes| serde_json::from_slice::<Held>(&bytes).ok())
            .filter(|held| held.server == server && held.handle == handle)
            .unwrap_or(Held {
                server,
                handle,
                ..Held::default()
            });
        Ok(Some(held))
    }

    /// Forgets the account's alarms on this device, at signing out.
    pub(crate) fn forget_alerts(&self) -> Result<(), CoreError> {
        self.storage.remove(FILE).map_err(storage_error)
    }

    fn store_alerts(&self, held: &Held) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(held).map_err(|error| CoreError::Storage {
            message: format!("Your alarms could not be saved: {error}."),
        })?;
        self.storage.write(FILE, &bytes).map_err(storage_error)
    }
}

#[uniffi::export]
impl Core {
    /// The account's alarms as the device last fetched them, the oldest
    /// first. Empty signed out.
    pub fn alerts_held(&self) -> Result<Vec<AlertView>, CoreError> {
        Ok(self
            .held_alerts()?
            .map(|held| held.alerts.iter().map(view).collect())
            .unwrap_or_default())
    }

    /// What the alarms fired as the device last fetched it, the newest
    /// first, of the last 30 days by `now_unix_s`. `utc_offset_s` is how
    /// far the device's clock is ahead of UTC.
    pub fn alerts_fired_held(
        &self,
        now_unix_s: i64,
        utc_offset_s: i32,
    ) -> Result<Vec<FiredLine>, CoreError> {
        let from = now_unix_s - venti_api_types::v1::FIRED_KEPT_DAYS * 86_400;
        let Some(held) = self.held_alerts()? else {
            return Ok(Vec::new());
        };
        Ok(held
            .fired
            .iter()
            .rev()
            .filter(|fired| fired.fired_at.timestamp() >= from)
            .map(|fired| fired_line(fired, utc_offset_s))
            .collect())
    }

    /// Starts fetching the account's alarms and what they fired since the
    /// last run. Run it after signing in, after changing an alarm, and
    /// from time to time: on Android from the app's periodic check, in a
    /// browser while the page is open. Each line in the summary's
    /// `fetched` is one thing an alarm fired that the device had not seen.
    pub fn alerts_sync(self: Arc<Self>) -> Arc<AlertsSync> {
        Arc::new(AlertsSync {
            core: self,
            progress: Mutex::new(Progress::default()),
        })
    }
}

/// One run of fetching the account's alarms and what they fired.
#[derive(Debug, uniffi::Object)]
pub struct AlertsSync {
    core: Arc<Core>,
    progress: Mutex<Progress>,
}

#[derive(Debug, Default)]
struct Progress {
    waiting_for: Option<Waiting>,
    summary: SyncSummary,
    /// The lines of what fired that this run fetched, for a notification.
    new: Vec<FiredAlert>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    List,
    Fired,
}

#[uniffi::export]
impl AlertsSync {
    /// Begins.
    pub fn start(&self) -> SyncStep {
        settled(self.try_start())
    }

    /// Takes the server's answer to the last request. `retry_after_s` is
    /// the answer's `Retry-After` header, if it had one.
    pub fn receive(&self, status: u16, body: Vec<u8>, retry_after_s: Option<u32>) -> SyncStep {
        settled(self.try_receive(status, &body, retry_after_s))
    }

    /// What this run fetched that had fired since the run before, as
    /// lines, the oldest first: what a notification says.
    pub fn new_lines(&self, utc_offset_s: i32) -> Vec<FiredLine> {
        self.lock()
            .new
            .iter()
            .map(|fired| fired_line(fired, utc_offset_s))
            .collect()
    }
}

impl AlertsSync {
    /// [`start`](Self::start) for a host whose storage hands a file over
    /// only after it was asked for it.
    pub fn start_when_fetched(&self) -> Result<SyncStep, CoreError> {
        fetched(self.try_start())
    }

    /// [`receive`](Self::receive) for such a host.
    pub fn receive_when_fetched(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<SyncStep, CoreError> {
        fetched(self.try_receive(status, body, retry_after_s))
    }

    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn held(&self) -> Result<Held, Halt> {
        match self.core.held_alerts().map_err(Halt::Fetch)? {
            Some(held) => Ok(held),
            None => Err(failed("This device is not signed in.", true).into()),
        }
    }

    fn try_start(&self) -> Result<SyncStep, Halt> {
        self.held()?;
        let mut progress = self.lock();
        *progress = Progress {
            waiting_for: Some(Waiting::List),
            ..Progress::default()
        };
        Ok(SyncStep::Send {
            request: alerts_request(),
        })
    }

    fn try_receive(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<SyncStep, Halt> {
        let mut held = self.held()?;
        let mut progress = self.lock();
        let Some(waiting) = progress.waiting_for else {
            return Ok(failed("Nothing was asked of the server.", false));
        };
        if status == 401 {
            progress.waiting_for = None;
            self.core.session_ended();
            return Err(failed("This device is no longer signed in. Sign in again.", true).into());
        }
        if status == 429 || status == 503 {
            progress.waiting_for = None;
            return Err(failed(&asked_to_wait(retry_after_s), false).into());
        }
        let stored = |held: &Held| {
            self.core
                .store_alerts(held)
                .map_err(|error| Halt::from(failed(&error.to_string(), false)))
        };
        match waiting {
            Waiting::List => {
                let response: AlertsResponse = answer(status, body)
                    .map_err(|error| Halt::from(failed(&error.to_string(), false)))?;
                held.alerts = response.alerts;
                stored(&held)?;
                progress.waiting_for = Some(Waiting::Fired);
                Ok(SyncStep::Send {
                    request: alerts_fired_request(held.cursor.clone()),
                })
            }
            Waiting::Fired => {
                let response: FiredAlertsResponse = answer(status, body)
                    .map_err(|error| Halt::from(failed(&error.to_string(), false)))?;
                if response.reset {
                    // What follows is everything the server keeps: what
                    // the device holds is had again.
                    let seen: Vec<(String, i64)> = held
                        .fired
                        .iter()
                        .map(|fired| (fired.alert_id.clone(), fired.fired_at.timestamp_micros()))
                        .collect();
                    held.fired.clear();
                    for fired in response.fired {
                        let key = (fired.alert_id.clone(), fired.fired_at.timestamp_micros());
                        if !seen.contains(&key) {
                            progress.new.push(fired.clone());
                        }
                        held.fired.push(fired);
                    }
                } else {
                    progress.new.extend(response.fired.iter().cloned());
                    held.fired.extend(response.fired);
                }
                let extra = held.fired.len().saturating_sub(FIRED_HELD);
                held.fired.drain(..extra);
                held.cursor = Some(response.cursor);
                stored(&held)?;
                progress.summary.fetched = u32::try_from(progress.new.len()).unwrap_or(u32::MAX);
                if response.more {
                    return Ok(SyncStep::Send {
                        request: alerts_fired_request(held.cursor.clone()),
                    });
                }
                progress.waiting_for = None;
                Ok(SyncStep::Finished {
                    summary: progress.summary,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use venti_api_types::v1::{FiredBecause, ThermalKind, ThermalVerdict};

    use super::*;

    fn wind_draft() -> AlertDraft {
        AlertDraft {
            name: "Wallberg north".to_owned(),
            enabled: true,
            kind: AlertKind::Wind,
            station_id: "dwd-10min:00001".to_owned(),
            station_name: "Wallberg".to_owned(),
            from_point: Some(0),
            to_point: Some(2),
            min_kmh: Some(10),
            max_kmh: Some(20),
            gusts_max_kmh: Some(30),
            from_hour: Some(10),
            to_hour: Some(16),
            lat: 0.0,
            lon: 0.0,
            site_id: None,
            place_name: String::new(),
            days_ahead: Vec::new(),
            at_least: AlertDayKind::CrossCountry,
            min_pilots: None,
            rest_hours: None,
        }
    }

    fn day_draft() -> AlertDraft {
        AlertDraft {
            kind: AlertKind::Day,
            name: "Brauneck XC".to_owned(),
            lat: 47.66,
            lon: 11.52,
            place_name: "Brauneck".to_owned(),
            days_ahead: vec![1, 0],
            ..wind_draft()
        }
    }

    #[test]
    fn a_draft_says_what_it_waits_for() {
        assert_eq!(
            alert_line(wind_draft()),
            "Wind at Wallberg from N to NE, 10 to 20 km/h, gusts at most 30, between 10:00 and 16:00"
        );
        let mut any = wind_draft();
        any.from_point = None;
        any.to_point = None;
        any.max_kmh = None;
        any.gusts_max_kmh = None;
        any.from_hour = None;
        any.to_hour = None;
        assert_eq!(alert_line(any), "Wind at Wallberg, at least 10 km/h");
        assert_eq!(
            alert_line(day_draft()),
            "A cross-country day or better at Brauneck, today or tomorrow"
        );
    }

    #[test]
    fn the_points_make_a_sector_half_a_point_either_side_and_back() {
        let request = request_of(&wind_draft()).unwrap();
        let AlertRule::Wind(wind) = &request.rule else {
            panic!("a wind alarm");
        };
        assert_eq!(
            wind.directions,
            Some(AlertSector {
                from_deg: 349,
                to_deg: 56
            })
        );
        let alert = Alert {
            id: "a".to_owned(),
            name: request.name.clone(),
            enabled: true,
            rule: request.rule.clone(),
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            changed_at: DateTime::from_timestamp(0, 0).unwrap(),
        };
        assert_eq!(draft_of(&alert), Some(wind_draft()));
    }

    #[test]
    fn a_check_says_in_words_what_is_wrong() {
        let mut wrong = wind_draft();
        wrong.min_kmh = Some(30);
        let faults = alert_check(wrong);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].field, "rule.max_kmh");
        assert!(
            faults[0].message.starts_with("The most wind: "),
            "{faults:?}"
        );
        let mut no_station = wind_draft();
        no_station.station_id = String::new();
        assert_eq!(alert_check(no_station)[0].message, "Choose a station.");
        assert_eq!(alert_check(day_draft()), []);
    }

    #[test]
    fn the_words_line_up_with_the_drafts_values_and_the_faults() {
        let words = alert_form_words();
        assert_eq!(
            words.day_names.len(),
            usize::from(venti_api_types::v1::MAX_DAYS_AHEAD) + 1
        );
        assert_eq!(words.day_names[2], "In 2 days");
        assert_eq!(words.day_kinds[3].kind, AlertDayKind::Big);
        assert_eq!(words.hours_title, "Hours, Central European Time");
        assert_eq!(
            words.signed_out,
            "Alarms live on your account: sign in to set one."
        );
        // Every fault the check can give is at a field the words name,
        // and its message begins with that name.
        let mut wrong = wind_draft();
        wrong.name = String::new();
        wrong.min_kmh = Some(30);
        wrong.from_hour = Some(20);
        let mut day = day_draft();
        day.days_ahead = Vec::new();
        day.place_name = String::new();
        let mut no_place = day_draft();
        no_place.lat = 200.0;
        let faults: Vec<AlertFaultView> = [wrong, day, no_place]
            .into_iter()
            .flat_map(alert_check)
            .collect();
        assert!(faults.len() >= 5, "{faults:?}");
        for fault in faults {
            let named = words
                .fields
                .iter()
                .find(|field| field.field == fault.field)
                .unwrap_or_else(|| panic!("{} is named", fault.field));
            if fault.field != "rule.station" && fault.field != "rule.place" {
                assert!(fault.message.starts_with(&named.name), "{fault:?}");
            }
        }
    }

    #[test]
    fn an_alarm_that_is_gone_is_deleted() {
        assert_eq!(
            alert_removed(204, Vec::new()).unwrap(),
            "The alarm is deleted."
        );
        assert!(alert_removed(404, Vec::new()).is_ok());
        assert!(alert_removed(401, Vec::new()).is_err());
        assert!(alert_removed(500, b"oops".to_vec()).is_err());
    }

    #[test]
    fn what_fired_is_said_in_a_line() {
        let request = request_of(&wind_draft()).unwrap();
        let fired = FiredAlert {
            alert_id: "a".to_owned(),
            alert_name: "Wallberg north".to_owned(),
            fired_at: "2026-07-04T09:21:00Z".parse().unwrap(),
            rule: request.rule,
            because: FiredBecause::Wind {
                time: "2026-07-04T09:20:00Z".parse().unwrap(),
                speed_kmh: 15,
                gust_kmh: None,
                direction_deg: Some(5),
            },
        };
        assert_eq!(
            fired_line(&fired, 7200).line,
            "Wallberg: 15 km/h from the north at 11:20, as your alarm 'Wallberg north' asks"
        );
        let day = request_of(&day_draft()).unwrap();
        let fired = FiredAlert {
            alert_id: "b".to_owned(),
            alert_name: "Brauneck XC".to_owned(),
            fired_at: "2026-07-02T07:00:00Z".parse().unwrap(),
            rule: day.rule,
            because: FiredBecause::Day {
                date: "2026-07-04".parse().unwrap(),
                verdict: ThermalVerdict::Flyable,
                thermals: Some(ThermalKind::CrossCountry),
                model: venti_domain::ModelId::new("icon-d2").unwrap(),
                run: "2026-07-02T04:00:00Z".parse().unwrap(),
            },
        };
        assert_eq!(
            fired_line(&fired, 7200).line,
            "Saturday is a cross-country day at Brauneck by the 06:00 run"
        );
    }
    #[test]
    fn a_pilots_alarm_is_set_with_a_place_and_two_numbers() {
        let alert = Alert {
            id: "a".to_owned(),
            name: "Wallberg is on".to_owned(),
            enabled: true,
            rule: serde_json::from_str(
                r#"{"kind":"pilots","place":{"lat":47.664,"lon":11.748},"place_name":"Wallberg"}"#,
            )
            .unwrap(),
            created_at: "2026-11-22T10:00:00Z".parse().unwrap(),
            changed_at: "2026-11-22T10:00:00Z".parse().unwrap(),
        };
        let shown = view(&alert);
        assert_eq!(
            shown.line,
            "3 pilots or more in the air at Wallberg, at most once in 6 hours"
        );
        let mut draft = shown.draft.unwrap();
        assert_eq!(draft.kind, AlertKind::Pilots);
        assert_eq!((draft.min_pilots, draft.rest_hours), (Some(3), Some(6)));
        assert_eq!(alert_check(draft.clone()), []);
        // The draft makes the rule it was read from.
        assert_eq!(request_of(&draft).unwrap().rule, alert.rule);
        draft.min_pilots = Some(1);
        draft.rest_hours = Some(1);
        assert_eq!(
            alert_line(draft.clone()),
            "A pilot in the air at Wallberg, at most once in an hour"
        );
        // Nothing chosen: three pilots and six hours.
        draft.min_pilots = None;
        draft.rest_hours = None;
        assert!(alert_line(draft.clone()).starts_with("3 pilots or more"));
        draft.min_pilots = Some(0);
        let faults = alert_check(draft);
        assert_eq!(faults.len(), 1);
        assert!(
            faults[0].message.starts_with("The number of pilots: "),
            "{}",
            faults[0].message
        );
        let words = alert_form_words();
        assert_eq!(words.pilots_kind, "Pilots in the air");
        assert!(words.pilots_note.contains("within 5 km"));
    }
}
