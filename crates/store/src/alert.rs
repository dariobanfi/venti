//! The alarms a pilot sets on the account, and what they fired.
//!
//! The alarms are private to the account and go with it. The store holds
//! an alarm's rule and the server's state of it as text and reads
//! neither; it keeps the station a wind alarm watches beside them, so
//! that the alarms of the stations that just reported are found by an
//! index.

use venti_domain::Timestamp;

/// The kind of an alarm that watches a station.
pub const WIND_ALERT: &str = "wind";

/// The kind of an alarm that watches the thermal map.
pub const DAY_ALERT: &str = "day";

/// The kind of an alarm that watches the pilots in the air at a place.
pub const PILOTS_ALERT: &str = "pilots";

/// An alarm an account has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// The alarm's identifier.
    pub id: String,
    /// The account it is of.
    pub account_id: String,
    /// What the pilot calls it.
    pub name: String,
    /// Whether it is checked.
    pub enabled: bool,
    /// [`WIND_ALERT`], [`DAY_ALERT`] or [`PILOTS_ALERT`], or a kind of a
    /// newer server.
    pub kind: String,
    /// For a wind alarm, the station it watches.
    pub station_id: Option<String>,
    /// The rule, as JSON text.
    pub rule: String,
    /// What the server noted of it when it checked it last, as JSON text:
    /// empty for an alarm that was made or changed since.
    pub state: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// When it was last changed by the pilot.
    pub changed_at: Timestamp,
}

/// An alarm to add, or what an alarm is to become.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAlert {
    /// The alarm's identifier: a new one to add, the alarm's to change.
    pub id: String,
    /// What the pilot calls it.
    pub name: String,
    /// Whether it is checked.
    pub enabled: bool,
    /// Its kind.
    pub kind: String,
    /// For a wind alarm, the station it watches.
    pub station_id: Option<String>,
    /// The rule, as JSON text.
    pub rule: String,
}

/// What became of adding an alarm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddingAlert {
    /// It was added.
    Added(Alert),
    /// The account has as many alarms as it may.
    Full,
    /// There is no such account.
    NoAccount,
}

/// What an alarm fired, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiredAlert {
    /// Its number: larger than every number the account's fired alarms
    /// used before.
    pub number: i64,
    /// The alarm that fired.
    pub alert_id: String,
    /// When.
    pub fired_at: Timestamp,
    /// What fired it, as JSON text.
    pub record: String,
}

/// What a check found of an alarm: its new state, and what it fired, if
/// it fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertCheck {
    /// The account the alarm is of.
    pub account_id: String,
    /// The alarm.
    pub alert_id: String,
    /// The `changed_at` the alarm had when it was read for the check. If
    /// the pilot changed it since, the check is of an alarm that is no
    /// more, and nothing is written.
    pub changed_at: Timestamp,
    /// Its new state, as JSON text.
    pub state: String,
    /// What it fired: when, and the record, as JSON text.
    pub fired: Option<(Timestamp, String)>,
}

/// What an account's alarms fired after some number.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FiredAlerts {
    /// What fired, by number.
    pub fired: Vec<FiredAlert>,
    /// Whether more is waiting beyond the limit of the request.
    pub more: bool,
    /// The largest number the account's fired alarms have used, zero if
    /// none.
    pub latest: i64,
}
