//! Open plans: a pilot's word to everyone flying from an area that they
//! mean to fly from a place on a day, with seats free in a car or looking
//! for company.
//!
//! A plan of a group is seen by its members only. An open plan belongs to
//! no group: anyone may read the open plans near a place, and every
//! signed-in pilot may say that they are in. Its author chose that, and so
//! does whoever says they are in: both are named on it, by handle and
//! display name and nothing else.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

use super::request::with_query;
use super::{GroupTextError, Pilot, PlanId, TimeOfDay, format_day, text};

/// Path of the open plans: posting one, and those near a place.
pub const OPEN_PLANS_PATH: &str = "/v1/open-plans";

/// Path of the open plans the signed-in pilot posted or is in on.
pub const ACCOUNT_OPEN_PLANS_PATH: &str = "/v1/account/open-plans";

/// Path of one open plan: reading, changing and deleting it.
pub fn open_plan_path(plan: &PlanId) -> String {
    format!("{OPEN_PLANS_PATH}/{plan}")
}

/// Path of a pilot's word on an open plan: saying that they are in, and
/// taking that back.
pub fn open_plan_in_path(plan: &PlanId) -> String {
    format!("{OPEN_PLANS_PATH}/{plan}/in")
}

/// The most characters the note of an open plan may have: a short note,
/// shorter than a group's plan may have.
pub const MAX_OPEN_PLAN_NOTE_LENGTH: usize = 280;

/// The most seats an open plan may offer.
pub const MAX_OPEN_PLAN_SEATS: u8 = 8;

/// How far around a place open plans are looked for when a request does
/// not say, in kilometres.
pub const DEFAULT_OPEN_PLANS_RADIUS_KM: u32 = 25;

/// The farthest around a place open plans are looked for, in kilometres.
pub const MAX_OPEN_PLANS_RADIUS_KM: u32 = 100;

/// How many days, from the first, one request for open plans covers at
/// most, and covers when it names no last day.
pub const OPEN_PLANS_DAYS: u32 = 14;

/// How far ahead an open plan may be for, in days from the server's date.
pub const OPEN_PLAN_DAYS_AHEAD: u32 = 30;

/// The most open plans one answer carries.
pub const MAX_OPEN_PLANS: usize = 100;

/// Makes the note of an open plan fit to keep, as
/// [`parse_plan_note`](super::parse_plan_note) does for a group's plan,
/// with at most [`MAX_OPEN_PLAN_NOTE_LENGTH`] characters.
pub fn parse_open_plan_note(text: &str) -> Result<Option<String>, GroupTextError> {
    let kept = text::lines(text);
    if kept.chars().count() > MAX_OPEN_PLAN_NOTE_LENGTH {
        return Err(GroupTextError::TooLong(MAX_OPEN_PLAN_NOTE_LENGTH));
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// The title and line a phone shows a pilot whose circle holds a new
/// open plan: "Open plan: Saturday 28 November at Wallberg" and "Anna has
/// 2 seats free".
#[must_use]
pub fn open_plan_words(
    day: NaiveDate,
    place: &str,
    seats: Option<u8>,
    author: &str,
) -> (String, String) {
    let title = format!(
        "Open plan: {} {} at {place}",
        super::weekday_name(day),
        day.format("%-d %B")
    );
    let body = match seats {
        Some(1) => format!("{author} has 1 seat free"),
        Some(seats) => format!("{author} has {seats} seats free"),
        None => format!("{author} is looking for company"),
    };
    (title, body)
}

/// What an open plan says: the body of `POST /v1/open-plans` and of
/// `PUT /v1/open-plans/{plan}`, sent whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenPlanRequest {
    /// The day, as a calendar at the place shows it.
    pub day: NaiveDate,
    /// The place, in the pilot's own words; see
    /// [`parse_plan_place`](super::parse_plan_place).
    pub place: String,
    /// Where that is. An open plan is found by it, so it is needed.
    pub position: GeoPoint,
    /// When to meet or to start, as a clock at the place shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meet_at: Option<TimeOfDay>,
    /// The seats the pilot has free in a car, 1 to
    /// [`MAX_OPEN_PLAN_SEATS`]. Left out: the pilot is looking for
    /// company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<u8>,
    /// What else there is to say; see [`parse_open_plan_note`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An open plan: a pilot's word to everyone flying from the area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenPlan {
    /// The plan's identifier.
    pub id: PlanId,
    /// The day it is for: a calendar date as it is at the place, with no
    /// time zone.
    pub day: NaiveDate,
    /// The place, as its author typed it. Show it as text and never as
    /// markup. It names no record of a flying site.
    pub place: String,
    /// Where the place is.
    pub position: GeoPoint,
    /// When to meet or to start, as a clock at the place shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meet_at: Option<TimeOfDay>,
    /// The seats the author has free in a car. Left out: the author is
    /// looking for company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<u8>,
    /// What else the author had to say, with line breaks: show it as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The pilot who posted it: handle and display name.
    pub author: Pilot,
    /// When it was posted.
    pub created_at: Timestamp,
    /// When its author last changed what it says.
    pub changed_at: Timestamp,
    /// The pilots who said that they are in, handle and display name, in
    /// the order in which they said so. The author is in from the start.
    #[serde(rename = "in")]
    pub pilots_in: Vec<Pilot>,
}

/// The answer of every request that makes or changes one open plan, and
/// of `GET /v1/open-plans/{plan}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenPlanResponse {
    /// The plan as it now is.
    pub plan: OpenPlan,
}

/// The answer of `GET /v1/open-plans` and of
/// `GET /v1/account/open-plans`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenPlansResponse {
    /// The plans: by day, within a day by the time to meet with the plans
    /// that name none last, and then in the order in which they were
    /// posted.
    pub plans: Vec<OpenPlan>,
    /// `true` when there were more than [`MAX_OPEN_PLANS`] and the answer
    /// has the first of them. Ask for fewer days or a smaller radius.
    pub truncated: bool,
}

/// A request for the open plans near a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenPlansRequest {
    /// The place.
    pub place: GeoPoint,
    /// How far around it, in kilometres: 1 to
    /// [`MAX_OPEN_PLANS_RADIUS_KM`]. `None` asks for
    /// [`DEFAULT_OPEN_PLANS_RADIUS_KM`].
    pub radius_km: Option<u32>,
    /// The first day. `None` leaves it to the server, which starts at the
    /// date its clock shows in UTC; a client names the pilot's own today.
    pub from: Option<NaiveDate>,
    /// The last day, at most [`OPEN_PLANS_DAYS`] from the first. `None`
    /// asks for that many.
    pub to: Option<NaiveDate>,
}

impl OpenPlansRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            OPEN_PLANS_PATH.to_owned(),
            [
                ("lat", Some(self.place.lat().to_string())),
                ("lon", Some(self.place.lon().to_string())),
                ("radius_km", self.radius_km.map(|km| km.to_string())),
                ("from", self.from.map(format_day)),
                ("to", self.to.map(format_day)),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_plan_is_told_in_a_line() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 28).unwrap();
        assert_eq!(
            open_plan_words(day, "Wallberg", Some(2), "Anna"),
            (
                "Open plan: Saturday 28 November at Wallberg".to_owned(),
                "Anna has 2 seats free".to_owned()
            )
        );
        assert_eq!(
            open_plan_words(day, "W", None, "Anna").1,
            "Anna is looking for company"
        );
    }

    #[test]
    fn a_request_for_open_plans_names_the_place_and_what_it_was_given() {
        let place = GeoPoint::new(47.66, 11.8).unwrap();
        let request = OpenPlansRequest {
            place,
            radius_km: Some(30),
            from: NaiveDate::from_ymd_opt(2026, 11, 21),
            to: None,
        };
        assert_eq!(
            request.path_and_query(),
            "/v1/open-plans?lat=47.66&lon=11.8&radius_km=30&from=2026-11-21"
        );
    }

    #[test]
    fn an_open_plan_needs_its_position_and_says_nothing_it_does_not_have() {
        let request: OpenPlanRequest = serde_json::from_str(
            r#"{"day":"2026-11-21","place":"Wallberg","position":{"lat":47.66,"lon":11.8}}"#,
        )
        .unwrap();
        assert_eq!(
            (request.seats, request.meet_at, request.note.as_deref()),
            (None, None, None)
        );
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("seats").is_none());
        assert!(
            serde_json::from_str::<OpenPlanRequest>(r#"{"day":"2026-11-21","place":"Wallberg"}"#)
                .is_err()
        );
        assert_eq!(parse_open_plan_note("  "), Ok(None));
        assert_eq!(
            parse_open_plan_note(&"x".repeat(MAX_OPEN_PLAN_NOTE_LENGTH + 1)),
            Err(GroupTextError::TooLong(MAX_OPEN_PLAN_NOTE_LENGTH))
        );
    }
}
