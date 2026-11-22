//! Alarms a pilot sets on the account: "tell me when the wind at this
//! station is from the north at 10 to 20 km/h", "tell me when Saturday
//! becomes a cross-country day at my take-off".
//!
//! An alarm is a fixed rule over data the server has. The server checks
//! it where the data arrives: a wind alarm against every reading of its
//! station, a day alarm against every run of the thermal map, a pilots
//! alarm once a minute against the live positions. What fired
//! is kept on the account for [`FIRED_KEPT_DAYS`] days, and an app fetches
//! it (`GET /v1/account/alerts/fired`); the server sends nothing by itself
//! yet.
//!
//! The alarms are the pilot's own and private: nobody but the account
//! reads them.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, ModelId, SiteId, StationId, Timestamp};

use super::account::unwanted_in_a_name;
use super::{ThermalKind, ThermalVerdict};

/// Path of the account's alarms: `GET` lists them, `POST` adds one.
pub const ALERTS_PATH: &str = "/v1/account/alerts";

/// Path of what the account's alarms fired.
pub const ALERTS_FIRED_PATH: &str = "/v1/account/alerts/fired";

/// Path of one alarm, by its identifier: `PUT` changes it, `DELETE`
/// removes it.
pub fn alert_path(id: &str) -> String {
    format!("{ALERTS_PATH}/{id}")
}

/// The most alarms an account has.
pub const MAX_ALERTS: usize = 20;

/// The most characters of the pilot's name for an alarm.
pub const MAX_ALERT_NAME_LENGTH: usize = 60;

/// The most characters of the name of an alarm's station or place.
pub const MAX_ALERT_PLACE_LENGTH: usize = 80;

/// How long a wind alarm rests after it fired: it fires again only for a
/// reading at least this many hours after the one that fired it.
pub const ALERT_REST_HOURS: i64 = 3;

/// How many days what fired is kept.
pub const FIRED_KEPT_DAYS: i64 = 30;

/// The furthest day ahead a day alarm looks at: today is 0.
pub const MAX_DAYS_AHEAD: u8 = 3;

/// The fastest wind and gust an alarm may name, km/h.
pub const MAX_ALERT_KMH: u16 = 250;

/// A reading older than this many minutes when it arrives is not checked:
/// what a source hands out again from hours back does not fire.
pub const ALERT_READING_FRESH_MINUTES: i64 = 60;

/// An alarm as the account has it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// The alarm's identifier.
    pub id: String,
    /// What the pilot calls it.
    pub name: String,
    /// Whether it is checked. One that is off fires nothing.
    pub enabled: bool,
    /// What it waits for.
    pub rule: AlertRule,
    /// When the pilot made it.
    pub created_at: Timestamp,
    /// When the pilot last changed it.
    pub changed_at: Timestamp,
}

/// The body of `POST /v1/account/alerts` and of
/// `PUT /v1/account/alerts/{id}`: an alarm as the pilot set it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertRequest {
    /// What the pilot calls it.
    pub name: String,
    /// Whether it is checked. Left out: it is.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What it waits for.
    pub rule: AlertRule,
}

fn yes() -> bool {
    true
}

/// The answer of `POST /v1/account/alerts` and of
/// `PUT /v1/account/alerts/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertResponse {
    /// The alarm as the account has it now.
    pub alert: Alert,
}

/// The answer of `GET /v1/account/alerts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertsResponse {
    /// Every alarm of the account, the oldest first.
    pub alerts: Vec<Alert>,
}

/// What an alarm waits for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertRule {
    /// The wind at a station.
    Wind(WindAlert),
    /// The kind of day the thermal map says a place has.
    Day(DayAlert),
    /// Pilots in the air at a place.
    Pilots(PilotsAlert),
    /// A kind of alarm this version of the code does not know.
    #[serde(other)]
    Other,
}

/// Directions the wind comes from, clockwise from `from_deg` to `to_deg`,
/// both included: 340 to 20 is the north.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertSector {
    /// The first direction, whole degrees, 0 to 359.
    pub from_deg: u16,
    /// The last direction, whole degrees, 0 to 359.
    pub to_deg: u16,
}

impl AlertSector {
    /// Whether a direction lies in the sector.
    #[must_use]
    pub fn holds(&self, degrees: f64) -> bool {
        let degrees = degrees.rem_euclid(360.0);
        let (from, to) = (f64::from(self.from_deg), f64::from(self.to_deg));
        if from <= to {
            degrees >= from && degrees <= to
        } else {
            degrees >= from || degrees <= to
        }
    }
}

/// Hours of the day, by the clock of [`alert_clock`]: from `from_hour`
/// o'clock to `to_hour` o'clock, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertHours {
    /// From, 0 to 23.
    pub from_hour: u8,
    /// To, 1 to 24, after `from_hour`.
    pub to_hour: u8,
}

/// The wind at a station.
///
/// Fires for a reading that fits every part that is set: the direction,
/// the mean wind between its least and its most, gusts no stronger than
/// the limit, and a time between the hours. A reading that lacks what a
/// part asks about does not fit: a gust limit needs a gust in the
/// reading, a sector a direction. Then it rests [`ALERT_REST_HOURS`]
/// hours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindAlert {
    /// The station, as `GET /v1/stations` names it.
    pub station: StationId,
    /// The station's name, for the words.
    pub station_name: String,
    /// The directions the wind is to come from. Left out: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directions: Option<AlertSector>,
    /// The least mean wind, km/h. Left out: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_kmh: Option<u16>,
    /// The most mean wind, km/h. Left out: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_kmh: Option<u16>,
    /// The strongest gust there may be, km/h. Left out: any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gusts_max_kmh: Option<u16>,
    /// Only between these hours. Left out: at any hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<AlertHours>,
}

/// What a reading of a station has that a wind alarm looks at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertReading {
    /// When it was measured.
    pub time: Timestamp,
    /// The mean wind, km/h.
    pub speed_kmh: Option<f64>,
    /// The strongest gust, km/h.
    pub gust_kmh: Option<f64>,
    /// Where the wind came from, degrees.
    pub direction_deg: Option<f64>,
}

impl WindAlert {
    /// Whether a reading fits the alarm.
    #[must_use]
    pub fn fits(&self, reading: &AlertReading) -> bool {
        let Some(speed) = reading.speed_kmh else {
            return false;
        };
        if self.min_kmh.is_some_and(|min| speed < f64::from(min))
            || self.max_kmh.is_some_and(|max| speed > f64::from(max))
        {
            return false;
        }
        if let Some(limit) = self.gusts_max_kmh {
            match reading.gust_kmh {
                Some(gust) if gust <= f64::from(limit) => {}
                _ => return false,
            }
        }
        if let Some(sector) = self.directions {
            match reading.direction_deg {
                Some(direction) if sector.holds(direction) => {}
                _ => return false,
            }
        }
        if let Some(hours) = self.hours {
            let clock = alert_clock(reading.time);
            let minute = clock.hour() * 60 + clock.minute();
            if minute < u32::from(hours.from_hour) * 60 || minute > u32::from(hours.to_hour) * 60 {
                return false;
            }
        }
        true
    }
}

/// The least a day is to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayAtLeast {
    /// Flyable by the air alone, whatever its thermals.
    Flyable,
    /// Flyable, with thermals to stay up in.
    Local,
    /// Flyable, with thermals strong and high enough to leave with.
    CrossCountry,
    /// Flyable, with strong, high thermals for most of the day.
    Big,
}

impl DayAtLeast {
    /// Whether a day the thermal map judged so is at least this.
    #[must_use]
    pub fn met_by(self, verdict: ThermalVerdict, thermals: Option<ThermalKind>) -> bool {
        if verdict != ThermalVerdict::Flyable {
            return false;
        }
        let rank = |kind: ThermalKind| match kind {
            ThermalKind::NoThermals | ThermalKind::Weak => 0,
            ThermalKind::Local => 1,
            ThermalKind::CrossCountry => 2,
            ThermalKind::Big => 3,
        };
        let needed = match self {
            Self::Flyable => return true,
            Self::Local => 1,
            Self::CrossCountry => 2,
            Self::Big => 3,
        };
        thermals.is_some_and(|kind| rank(kind) >= needed)
    }
}

/// The kind of day at a place, by the thermal map.
///
/// For each day it looks at, fires when a run of the map first says the
/// day is at least `at_least` in the cell the place lies in; again only
/// after a run stopped saying so and a later one says so again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayAlert {
    /// Where.
    pub place: GeoPoint,
    /// The site the place is of, if it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<SiteId>,
    /// The place's name, for the words.
    pub place_name: String,
    /// Which days, counted from today: 0 is today, 1 tomorrow, at most
    /// [`MAX_DAYS_AHEAD`]. Today is by the clock of [`alert_clock`].
    pub days_ahead: Vec<u8>,
    /// The least the day is to be.
    pub at_least: DayAtLeast,
}

/// How many pilots a pilots alarm waits for when it names no number.
pub const PILOTS_ALERT_DEFAULT_PILOTS: u16 = 3;

/// The most pilots a pilots alarm may wait for.
pub const MAX_PILOTS_ALERT_PILOTS: u16 = 100;

/// For how many hours a pilots alarm rests after it fired when it names
/// no number.
pub const PILOTS_ALERT_DEFAULT_REST_HOURS: u8 = 6;

/// The longest a pilots alarm may rest, hours.
pub const MAX_PILOTS_REST_HOURS: u8 = 24;

fn default_pilots() -> u16 {
    PILOTS_ALERT_DEFAULT_PILOTS
}

fn default_rest() -> u8 {
    PILOTS_ALERT_DEFAULT_REST_HOURS
}

/// Pilots in the air at a place: "a lot of people fly, let me know".
///
/// Fires when at least `min_pilots` paragliders and hang gliders are in
/// the air within [`SITE_AIR_WITHIN_M`](super::SITE_AIR_WITHIN_M) of the
/// place, by one source of live positions: a network the server listens
/// to, or Venti's own live pilots. The sources are not added up, since a
/// pilot may be in both. Then it rests `rest_hours` hours. The server
/// looks once a minute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotsAlert {
    /// Where: a flying site's place, or any other.
    pub place: GeoPoint,
    /// The site the place is of, if it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<SiteId>,
    /// The place's name, for the words.
    pub place_name: String,
    /// The fewest pilots in the air, 1 to [`MAX_PILOTS_ALERT_PILOTS`].
    /// Left out: [`PILOTS_ALERT_DEFAULT_PILOTS`].
    #[serde(default = "default_pilots")]
    pub min_pilots: u16,
    /// For how many hours it rests after it fired, 1 to
    /// [`MAX_PILOTS_REST_HOURS`]. Left out:
    /// [`PILOTS_ALERT_DEFAULT_REST_HOURS`].
    #[serde(default = "default_rest")]
    pub rest_hours: u8,
}

impl PilotsAlert {
    fn check(&self, faults: &mut Vec<AlertFault>) {
        text_faults(
            "rule.place_name",
            &self.place_name,
            MAX_ALERT_PLACE_LENGTH,
            faults,
        );
        if self.min_pilots == 0 || self.min_pilots > MAX_PILOTS_ALERT_PILOTS {
            faults.push(fault(
                "rule.min_pilots",
                format!("is a number from 1 to {MAX_PILOTS_ALERT_PILOTS}"),
            ));
        }
        if self.rest_hours == 0 || self.rest_hours > MAX_PILOTS_REST_HOURS {
            faults.push(fault(
                "rule.rest_hours",
                format!("is a number from 1 to {MAX_PILOTS_REST_HOURS}"),
            ));
        }
    }
}

/// What a check of an alarm found against it: the field, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertFault {
    /// The field, as the request names it: `rule.min_kmh`.
    pub field: &'static str,
    /// What is wrong, in words for a developer and for a pilot.
    pub reason: String,
}

fn fault(field: &'static str, reason: impl Into<String>) -> AlertFault {
    AlertFault {
        field,
        reason: reason.into(),
    }
}

fn text_faults(field: &'static str, text: &str, most: usize, faults: &mut Vec<AlertFault>) {
    let text = text.trim();
    if text.is_empty() {
        faults.push(fault(field, "must not be empty"));
    } else if text.chars().count() > most {
        faults.push(fault(field, format!("has at most {most} characters")));
    } else if text.chars().any(unwanted_in_a_name) {
        faults.push(fault(
            field,
            "must not hold line breaks, tabs, or invisible characters",
        ));
    }
}

impl AlertRequest {
    /// Everything that is wrong with the alarm, the first first. Empty
    /// when the server takes it.
    #[must_use]
    pub fn check(&self) -> Vec<AlertFault> {
        let mut faults = Vec::new();
        text_faults("name", &self.name, MAX_ALERT_NAME_LENGTH, &mut faults);
        match &self.rule {
            AlertRule::Wind(wind) => wind.check(&mut faults),
            AlertRule::Day(day) => day.check(&mut faults),
            AlertRule::Pilots(pilots) => pilots.check(&mut faults),
            AlertRule::Other => faults.push(fault(
                "rule.kind",
                "is not a kind of alarm this server knows: wind, day or pilots",
            )),
        }
        faults
    }
}

impl WindAlert {
    fn check(&self, faults: &mut Vec<AlertFault>) {
        text_faults(
            "rule.station_name",
            &self.station_name,
            MAX_ALERT_PLACE_LENGTH,
            faults,
        );
        if let Some(sector) = self.directions {
            if sector.from_deg >= 360 || sector.to_deg >= 360 {
                faults.push(fault("rule.directions", "degrees run from 0 to 359"));
            } else if sector.from_deg == sector.to_deg {
                faults.push(fault(
                    "rule.directions",
                    "from and to are the same direction; leave the directions out for any",
                ));
            }
        }
        for (field, value) in [
            ("rule.min_kmh", self.min_kmh),
            ("rule.max_kmh", self.max_kmh),
            ("rule.gusts_max_kmh", self.gusts_max_kmh),
        ] {
            if value.is_some_and(|kmh| kmh > MAX_ALERT_KMH) {
                faults.push(fault(field, format!("is at most {MAX_ALERT_KMH} km/h")));
            }
        }
        if let (Some(min), Some(max)) = (self.min_kmh, self.max_kmh)
            && min > max
        {
            faults.push(fault(
                "rule.max_kmh",
                "must not be less than the least wind",
            ));
        }
        if self.directions.is_none() && self.min_kmh.is_none() && self.max_kmh.is_none() {
            faults.push(fault(
                "rule.directions",
                "an alarm needs directions or a least or most wind, or it fires with every reading",
            ));
        }
        if let Some(hours) = self.hours
            && (hours.from_hour > 23 || hours.to_hour > 24 || hours.from_hour >= hours.to_hour)
        {
            faults.push(fault(
                "rule.hours",
                "runs from an hour of 0 to 23 to a later one, at most 24",
            ));
        }
    }
}

impl DayAlert {
    fn check(&self, faults: &mut Vec<AlertFault>) {
        text_faults(
            "rule.place_name",
            &self.place_name,
            MAX_ALERT_PLACE_LENGTH,
            faults,
        );
        if self.days_ahead.is_empty() {
            faults.push(fault("rule.days_ahead", "names at least one day"));
        }
        if self.days_ahead.iter().any(|day| *day > MAX_DAYS_AHEAD) {
            faults.push(fault(
                "rule.days_ahead",
                format!("days run from 0, today, to {MAX_DAYS_AHEAD}"),
            ));
        }
        let mut seen = self.days_ahead.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != self.days_ahead.len() {
            faults.push(fault("rule.days_ahead", "names each day once"));
        }
    }
}

/// One thing an alarm fired for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FiredAlert {
    /// The alarm that fired. It may have been removed since.
    pub alert_id: String,
    /// What the pilot called it then.
    pub alert_name: String,
    /// When the server found that it fits.
    pub fired_at: Timestamp,
    /// The alarm's rule as it was then.
    pub rule: AlertRule,
    /// What fired it.
    pub because: FiredBecause,
}

/// What fired an alarm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FiredBecause {
    /// A reading of the station.
    Wind {
        /// When it was measured.
        time: Timestamp,
        /// The mean wind, km/h, whole.
        speed_kmh: u16,
        /// The strongest gust, km/h, whole, if the reading has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gust_kmh: Option<u16>,
        /// Where the wind came from, whole degrees, if the reading has it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction_deg: Option<u16>,
    },
    /// A run of the thermal map.
    Day {
        /// The day.
        date: NaiveDate,
        /// The day's verdict in the place's cell.
        verdict: ThermalVerdict,
        /// The kind of day its thermals make there.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thermals: Option<ThermalKind>,
        /// The weather model the day is read from.
        model: ModelId,
        /// When the newest run of that model began that the day holds.
        run: Timestamp,
    },
    /// Pilots in the air at the place.
    Pilots {
        /// When the server counted.
        time: Timestamp,
        /// How many were in the air near the place, by the source of
        /// live positions that had the most.
        pilots: u32,
        /// How many of them took off there in the last hour.
        #[serde(default)]
        took_off: u32,
    },
    /// Something this version of the code does not know.
    #[serde(other)]
    Other,
}

/// The answer of `GET /v1/account/alerts/fired`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FiredAlertsResponse {
    /// What fired, the oldest first.
    pub fired: Vec<FiredAlert>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more waits. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server could not answer from the request's `since`
    /// and started again from the oldest it keeps.
    pub reset: bool,
}

/// A request for what the account's alarms fired.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FiredAlertsRequest {
    /// The `cursor` of the answer before. `None` asks for everything kept.
    pub since: Option<String>,
    /// How many to return, at most [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl FiredAlertsRequest {
    /// The path and query string to GET.
    #[must_use]
    pub fn path_and_query(&self) -> String {
        let since = self
            .since
            .as_deref()
            .filter(|since| !since.is_empty() && since.bytes().all(|byte| byte.is_ascii_digit()))
            .map(|since| format!("since={since}"));
        let limit = self.limit.map(|limit| format!("limit={limit}"));
        let query: Vec<String> = since.into_iter().chain(limit).collect();
        if query.is_empty() {
            ALERTS_FIRED_PATH.to_owned()
        } else {
            format!("{ALERTS_FIRED_PATH}?{}", query.join("&"))
        }
    }
}

/// The clock alarms are read by: Central European Time, an hour ahead of
/// UTC, and two hours ahead in summer time as the European Union keeps it,
/// from 01:00 UTC on the last Sunday of March to 01:00 UTC on the last
/// Sunday of October. The hours of a wind alarm and the "today" of a day
/// alarm are by this clock: Venti's regions lie in it. A region beyond
/// it needs a zone of its own.
#[must_use]
pub fn alert_clock(time: Timestamp) -> NaiveDateTime {
    let year = time.year();
    let switch = |month: u32| {
        // The last Sunday of the month, 01:00 UTC.
        let first_of_next = NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap_or_default();
        let last = first_of_next - Duration::days(1);
        let back = i64::from(last.weekday().num_days_from_sunday());
        let sunday = last - Duration::days(back);
        Utc.from_utc_datetime(&sunday.and_hms_opt(1, 0, 0).unwrap_or_default())
    };
    let summer = time >= switch(3) && time < switch(10);
    let hours = if summer { 2 } else { 1 };
    (time + Duration::hours(hours)).naive_utc()
}

/// The day of a moment by [`alert_clock`].
#[must_use]
pub fn alert_day(time: Timestamp) -> NaiveDate {
    alert_clock(time).date()
}

/// A weekday's name in English.
#[must_use]
pub fn weekday_name(day: NaiveDate) -> &'static str {
    match day.weekday() {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn wind() -> WindAlert {
        WindAlert {
            station: "dwd-10min:00001".parse().unwrap(),
            station_name: "Wallberg".to_owned(),
            directions: Some(AlertSector {
                from_deg: 349,
                to_deg: 56,
            }),
            min_kmh: Some(10),
            max_kmh: Some(20),
            gusts_max_kmh: Some(30),
            hours: Some(AlertHours {
                from_hour: 10,
                to_hour: 16,
            }),
        }
    }

    fn reading(time: &str, speed: f64, gust: Option<f64>, direction: Option<f64>) -> AlertReading {
        AlertReading {
            time: at(time),
            speed_kmh: Some(speed),
            gust_kmh: gust,
            direction_deg: direction,
        }
    }

    #[test]
    fn a_reading_fits_when_every_part_that_is_set_does() {
        let alarm = wind();
        // 11:20 in summer time is 09:20 UTC.
        let fits = reading("2026-07-04T09:20:00Z", 15.0, Some(22.0), Some(5.0));
        assert!(alarm.fits(&fits));
        assert!(alarm.fits(&AlertReading {
            direction_deg: Some(355.0),
            ..fits
        }));
        assert!(!alarm.fits(&AlertReading {
            direction_deg: Some(90.0),
            ..fits
        }));
        assert!(!alarm.fits(&AlertReading {
            speed_kmh: Some(21.0),
            ..fits
        }));
        assert!(!alarm.fits(&AlertReading {
            gust_kmh: Some(31.0),
            ..fits
        }));
        // No gust measured: a gust limit cannot be said to hold.
        assert!(!alarm.fits(&AlertReading {
            gust_kmh: None,
            ..fits
        }));
        // 08:20 by the clock.
        assert!(!alarm.fits(&reading(
            "2026-07-04T06:20:00Z",
            15.0,
            Some(22.0),
            Some(5.0)
        )));
        // 16:00 is still in, 16:10 is not.
        assert!(alarm.fits(&reading(
            "2026-07-04T14:00:00Z",
            15.0,
            Some(22.0),
            Some(5.0)
        )));
        assert!(!alarm.fits(&reading(
            "2026-07-04T14:10:00Z",
            15.0,
            Some(22.0),
            Some(5.0)
        )));
    }

    #[test]
    fn the_clock_keeps_european_summer_time() {
        // 2026: summer time from 29 March to 25 October.
        assert_eq!(
            alert_clock(at("2026-03-29T00:59:00Z")).to_string(),
            "2026-03-29 01:59:00"
        );
        assert_eq!(
            alert_clock(at("2026-03-29T01:00:00Z")).to_string(),
            "2026-03-29 03:00:00"
        );
        assert_eq!(
            alert_clock(at("2026-10-25T00:59:00Z")).to_string(),
            "2026-10-25 02:59:00"
        );
        assert_eq!(
            alert_clock(at("2026-10-25T01:00:00Z")).to_string(),
            "2026-10-25 02:00:00"
        );
        assert_eq!(
            alert_day(at("2026-11-21T23:30:00Z")).to_string(),
            "2026-11-22"
        );
    }

    #[test]
    fn a_day_is_at_least_what_its_verdict_and_thermals_say() {
        use ThermalKind::{Big, CrossCountry, Local};
        use ThermalVerdict::{Flyable, Marginal};
        assert!(DayAtLeast::CrossCountry.met_by(Flyable, Some(CrossCountry)));
        assert!(DayAtLeast::CrossCountry.met_by(Flyable, Some(Big)));
        assert!(!DayAtLeast::CrossCountry.met_by(Flyable, Some(Local)));
        assert!(!DayAtLeast::CrossCountry.met_by(Marginal, Some(Big)));
        assert!(DayAtLeast::Flyable.met_by(Flyable, None));
        assert!(!DayAtLeast::Local.met_by(Flyable, None));
    }

    #[test]
    fn a_check_names_every_field_that_is_wrong() {
        let good = AlertRequest {
            name: "Wallberg north".to_owned(),
            enabled: true,
            rule: AlertRule::Wind(wind()),
        };
        assert_eq!(good.check(), []);
        let mut bad = wind();
        bad.min_kmh = Some(30);
        bad.hours = Some(AlertHours {
            from_hour: 16,
            to_hour: 10,
        });
        let request = AlertRequest {
            name: " ".to_owned(),
            enabled: true,
            rule: AlertRule::Wind(bad),
        };
        let fields: Vec<&str> = request.check().iter().map(|f| f.field).collect();
        assert_eq!(fields, ["name", "rule.max_kmh", "rule.hours"]);

        let day = AlertRequest {
            name: "Brauneck XC".to_owned(),
            enabled: true,
            rule: AlertRule::Day(DayAlert {
                place: GeoPoint::new(47.66, 11.52).unwrap(),
                site: None,
                place_name: "Brauneck".to_owned(),
                days_ahead: vec![1, 1, 4],
                at_least: DayAtLeast::CrossCountry,
            }),
        };
        let fields: Vec<&str> = day.check().iter().map(|f| f.field).collect();
        assert_eq!(fields, ["rule.days_ahead", "rule.days_ahead"]);
    }

    #[test]
    fn a_pilots_alarm_has_three_pilots_and_six_hours_unless_it_says() {
        let text = r#"{"kind":"pilots","place":{"lat":47.66,"lon":11.52},"place_name":"Brauneck"}"#;
        let AlertRule::Pilots(rule) = serde_json::from_str(text).unwrap() else {
            panic!("not a pilots alarm");
        };
        assert_eq!((rule.min_pilots, rule.rest_hours), (3, 6));
        let request = |min_pilots, rest_hours| AlertRequest {
            name: "Brauneck is on".to_owned(),
            enabled: true,
            rule: AlertRule::Pilots(PilotsAlert {
                min_pilots,
                rest_hours,
                ..rule.clone()
            }),
        };
        assert_eq!(request(3, 6).check(), []);
        let fields: Vec<&str> = request(0, 25).check().iter().map(|f| f.field).collect();
        assert_eq!(fields, ["rule.min_pilots", "rule.rest_hours"]);
    }

    #[test]
    fn a_rule_of_a_newer_server_is_read_as_other() {
        let text = r#"{"kind":"lightning","radius_km":20}"#;
        let rule: AlertRule = serde_json::from_str(text).unwrap();
        assert_eq!(rule, AlertRule::Other);
    }

    #[test]
    fn what_fired_is_asked_for_with_a_cursor_of_digits_or_none() {
        assert_eq!(
            FiredAlertsRequest::default().path_and_query(),
            "/v1/account/alerts/fired"
        );
        let since = FiredAlertsRequest {
            since: Some("17".to_owned()),
            limit: Some(50),
        };
        assert_eq!(
            since.path_and_query(),
            "/v1/account/alerts/fired?since=17&limit=50"
        );
    }
}
