//! Groups of pilots, and the plans their members make for a flying day.
//!
//! A group is seen by its members and by nobody else: to everyone else it
//! answers exactly as a group that does not exist. A pilot comes into one
//! with an invite code that its keeper hands out. What members share in a
//! group is plans: who means to fly where on which day.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

use super::{Pilot, text};

/// The most characters the name of a group may have.
pub const MAX_GROUP_NAME_LENGTH: usize = 60;

/// The most characters the place of a plan may have.
pub const MAX_PLAN_PLACE_LENGTH: usize = 80;

/// The most characters the note of a plan may have.
pub const MAX_PLAN_NOTE_LENGTH: usize = 500;

/// The most plans the answer about all of a pilot's groups carries. When
/// the groups hold more, the answer has the first ones by day and
/// [`PlanBoardResponse::truncated`] is set; a narrower span of days gets
/// the rest.
pub const MAX_BOARD_PLANS: usize = 200;

/// Defines an identifier the server makes: 26 lower-case letters and
/// digits, like the address of a flight.
macro_rules! identifier {
    ($(#[$doc:meta])* $name:ident, $error:ident, $what:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        #[doc = concat!("Text that does not have the form of ", $what, ".")]
        #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
        #[error("{} is 26 lower-case letters and digits", $what)]
        pub struct $error;

        impl $name {
            /// How many characters the identifier has.
            pub const LENGTH: usize = 26;
            /// The characters it is written in: the digits and the alphabet
            /// without `i`, `l`, `o` and `u`.
            pub const ALPHABET: &'static str = "0123456789abcdefghjkmnpqrstvwxyz";

            /// The identifier as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = $error;
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let well_formed =
                    text.len() == Self::LENGTH && text.chars().all(|c| Self::ALPHABET.contains(c));
                if well_formed {
                    Ok(Self(text.to_owned()))
                } else {
                    Err($error)
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = $error;
            fn try_from(text: String) -> Result<Self, Self::Error> {
                text.parse()
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

identifier!(
    /// The identifier of a group, as in `/v1/groups/{id}`. It names the
    /// group to its members and gives nobody else anything: a pilot who is
    /// not in the group gets the answer for an identifier nobody has.
    GroupId,
    GroupIdError,
    "a group's identifier"
);

identifier!(
    /// The identifier of a plan, as in `/v1/groups/{id}/plans/{plan}`.
    PlanId,
    PlanIdError,
    "a plan's identifier"
);

/// The invite code of a group: sixteen characters in four groups, such as
/// `7k3m-9q2x-4r8t-1v6w`. Whoever has it and an account can come into the
/// group, until the keeper makes a new code or removes a member.
///
/// It does not print itself: an answer that holds one can be logged or
/// shown in a debugger without giving it away.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupInviteCode(String);

impl GroupInviteCode {
    /// Wraps a code.
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// The code, for showing to the keeper, for handing on and for
    /// sending, and for nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GroupInviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GroupInviteCode(<hidden>)")
    }
}

/// Why a name, a place or a note was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GroupTextError {
    /// Nothing is left of it once it is cleaned.
    #[error("it must not be empty")]
    Empty,
    /// It has more characters than its kind of text may have.
    #[error("it has at most {0} characters")]
    TooLong(usize),
}

/// One line of text, cleaned, that must not be empty or longer than `most`
/// characters.
fn one_line(text: &str, most: usize) -> Result<String, GroupTextError> {
    let kept = text::line(text);
    if kept.is_empty() {
        return Err(GroupTextError::Empty);
    }
    if kept.chars().count() > most {
        return Err(GroupTextError::TooLong(most));
    }
    Ok(kept)
}

/// Makes the name of a group fit to keep: what the server stores of it.
///
/// A name is one line. A line break or a tab in it becomes a space and
/// several spaces become one; control characters go, and so does what
/// would hide text or turn it round. It is refused only when nothing is
/// left, or when more than [`MAX_GROUP_NAME_LENGTH`] characters are.
pub fn parse_group_name(text: &str) -> Result<String, GroupTextError> {
    one_line(text, MAX_GROUP_NAME_LENGTH)
}

/// Makes the place of a plan fit to keep, as [`parse_group_name`] does for
/// a name: one line of at most [`MAX_PLAN_PLACE_LENGTH`] characters.
///
/// The place is what the pilot typed and nothing else. It is not looked up
/// anywhere and names no record of a flying site.
pub fn parse_plan_place(text: &str) -> Result<String, GroupTextError> {
    one_line(text, MAX_PLAN_PLACE_LENGTH)
}

/// Makes the note of a plan fit to keep, as
/// [`parse_report_text`](super::parse_report_text) does for the text of a
/// report: line breaks stay, and `None` when nothing is left. It is
/// refused only for more than [`MAX_PLAN_NOTE_LENGTH`] characters.
pub fn parse_plan_note(text: &str) -> Result<Option<String>, GroupTextError> {
    let kept = text::lines(text);
    if kept.chars().count() > MAX_PLAN_NOTE_LENGTH {
        return Err(GroupTextError::TooLong(MAX_PLAN_NOTE_LENGTH));
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// A time of day as a clock at the place shows it, to the minute: `09:30`.
/// It has no time zone and no date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TimeOfDay {
    hour: u8,
    minute: u8,
}

/// Text that is no time of day.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a time of day is written HH:MM on a 24-hour clock, such as 09:30")]
pub struct TimeOfDayError;

impl TimeOfDay {
    /// The time `hour:minute`; `None` unless the hour is 0 to 23 and the
    /// minute 0 to 59.
    pub fn new(hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then_some(Self { hour, minute })
    }

    /// The hour, 0 to 23.
    pub fn hour(self) -> u8 {
        self.hour
    }

    /// The minute, 0 to 59.
    pub fn minute(self) -> u8 {
        self.minute
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

impl FromStr for TimeOfDay {
    type Err = TimeOfDayError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // Exactly two digits, a colon and two digits: nothing a person
        // could read as another time.
        let &[h1, h2, b':', m1, m2] = text.as_bytes() else {
            return Err(TimeOfDayError);
        };
        let number = |tens: u8, ones: u8| {
            (tens.is_ascii_digit() && ones.is_ascii_digit())
                .then(|| (tens - b'0') * 10 + (ones - b'0'))
        };
        number(h1, h2)
            .zip(number(m1, m2))
            .and_then(|(hour, minute)| Self::new(hour, minute))
            .ok_or(TimeOfDayError)
    }
}

impl TryFrom<String> for TimeOfDay {
    type Error = TimeOfDayError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<TimeOfDay> for String {
    fn from(time: TimeOfDay) -> Self {
        time.to_string()
    }
}

/// A group, as each of its members sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// The group's identifier.
    pub id: GroupId,
    /// Its name. Text a pilot typed: show it as text and never as markup.
    pub name: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The member who keeps the group: who may rename it, make a new
    /// invite code, remove a member, hand the keeping on and delete it.
    pub keeper: Pilot,
    /// How many members it has, the keeper included.
    pub member_count: u32,
}

/// The body of `POST /v1/groups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    /// The name of the group; see [`parse_group_name`].
    pub name: String,
}

/// The answer of `POST /v1/groups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateGroupResponse {
    /// The group, whose keeper and only member is the pilot who made it.
    pub group: Group,
    /// The code that lets others in. This is the only time the server
    /// shows it: it keeps a hash and cannot show it again.
    pub invite_code: GroupInviteCode,
}

/// The answer of `GET /v1/groups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupsResponse {
    /// The groups the pilot is in, the one they have been in longest
    /// first.
    pub groups: Vec<Group>,
}

/// The answer of `GET /v1/groups/{id}`, of `PATCH /v1/groups/{id}` and of
/// `PUT /v1/groups/{id}/keeper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupResponse {
    /// The group.
    pub group: Group,
    /// Its members, the one who has been in it longest first. When the
    /// keeper leaves, the first of the others is the next keeper.
    pub members: Vec<Pilot>,
}

/// The body of `PATCH /v1/groups/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    /// The group's new name; see [`parse_group_name`].
    pub name: String,
}

/// The body of `PUT /v1/groups/{id}/keeper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandOverGroupRequest {
    /// The handle of the member who keeps the group from now on.
    pub handle: String,
}

/// The answer of `POST /v1/groups/{id}/invite`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupInviteResponse {
    /// The group's new invite code. The one before it has stopped
    /// working. The server does not show this one again either.
    pub invite_code: GroupInviteCode,
}

/// The body of `POST /v1/groups/join`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinGroupRequest {
    /// The invite code, as the keeper handed it on. Hyphens, spaces and
    /// upper case do not matter.
    pub code: GroupInviteCode,
}

/// The answer of `POST /v1/groups/join`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinGroupResponse {
    /// The group the code belongs to.
    pub group: Group,
    /// `true` when this request made the pilot a member, `false` when
    /// they were one already.
    pub joined: bool,
}

/// What a plan says: the body of `POST /v1/groups/{id}/plans` and of
/// `PUT /v1/groups/{id}/plans/{plan}`.
///
/// A plan is sent whole. What a change leaves out, the plan no longer has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRequest {
    /// The day, as a calendar at the place shows it.
    pub day: NaiveDate,
    /// The place, in the pilot's own words; see [`parse_plan_place`].
    pub place: String,
    /// Where that is, if the pilot marked it on a map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GeoPoint>,
    /// When to meet or to start, as a clock at the place shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meet_at: Option<TimeOfDay>,
    /// What else there is to say; see [`parse_plan_note`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A plan for a flying day: one member's word to the group about where
/// they mean to fly, and who of the group is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// The plan's identifier.
    pub id: PlanId,
    /// The group it was posted to. A plan belongs to one group.
    pub group: GroupId,
    /// The day it is for: a calendar date as it is at the place. The
    /// server keeps the date as it was sent and attaches no time zone.
    pub day: NaiveDate,
    /// The place, as its author typed it. Show it as text and never as
    /// markup. It names no record of a flying site.
    pub place: String,
    /// Where the place is, if the author marked it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GeoPoint>,
    /// When to meet or to start, as a clock at the place shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meet_at: Option<TimeOfDay>,
    /// What else the author had to say. Text a pilot typed, with line
    /// breaks: show it as text and never as markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The member who posted it.
    pub author: Pilot,
    /// When it was posted.
    pub created_at: Timestamp,
    /// When its author last changed it; the moment it was posted for a
    /// plan that was never changed.
    pub changed_at: Timestamp,
    /// The members who said that they are in, in the order in which they
    /// said so. The author is in from the start and can take that back
    /// like anyone else.
    #[serde(rename = "in")]
    pub pilots_in: Vec<Pilot>,
}

/// The answer of every request that makes or changes one plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanResponse {
    /// The plan as it now is.
    pub plan: Plan,
}

/// The answer of `GET /v1/groups/{id}/plans`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlansResponse {
    /// The group's plans for the days asked for: by day, within a day by
    /// the time to meet with the plans that name none last, and then in
    /// the order in which they were posted.
    pub plans: Vec<Plan>,
}

/// The answer of `GET /v1/groups/plans`: the plans of every group the
/// pilot is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanBoardResponse {
    /// The groups the pilot is in, as `GET /v1/groups` lists them: every
    /// one, also those without a plan in this answer.
    pub groups: Vec<Group>,
    /// The plans of those groups for the days asked for, in the order of
    /// [`PlansResponse::plans`].
    pub plans: Vec<Plan>,
    /// `true` when the groups hold more plans for those days than
    /// [`MAX_BOARD_PLANS`] and the answer was cut. Ask for fewer days.
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::Handle;

    #[test]
    fn an_identifier_is_26_characters_of_its_alphabet() {
        let id = "0123456789abcdefghjkmnpqrs";
        assert_eq!(id.parse::<GroupId>().unwrap().as_str(), id);
        assert_eq!(id.parse::<PlanId>().unwrap().to_string(), id);
        for bad in [
            "",
            "0123456789abcdefghjkmnpqr",
            "0123456789abcdefghjkmnpqrst",
            "0123456789ABCDEFGHJKMNPQRS",
            "0123456789abcdefghijkmnpqr",
            "join",
            "plans",
            "../../../../../etc/passwd/",
            "0123456789abcdefghjkmnpqrś",
        ] {
            assert_eq!(bad.parse::<GroupId>(), Err(GroupIdError), "{bad:?}");
            assert_eq!(bad.parse::<PlanId>(), Err(PlanIdError), "{bad:?}");
        }
        assert!(serde_json::from_str::<GroupId>(r#""join""#).is_err());
    }

    #[test]
    fn an_invite_code_does_not_print_itself() {
        let request: JoinGroupRequest =
            serde_json::from_str(r#"{"code":"7k3m-9q2x-4r8t-1v6w"}"#).unwrap();
        assert_eq!(request.code.expose(), "7k3m-9q2x-4r8t-1v6w");
        let shown = format!("{request:?}");
        assert!(!shown.contains("7k3m"), "{shown}");
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"code":"7k3m-9q2x-4r8t-1v6w"}"#
        );
    }

    #[test]
    fn names_and_places_are_one_cleaned_line_of_bounded_length() {
        assert_eq!(
            parse_group_name("  Mustergruppe\n Tegernsee "),
            Ok("Mustergruppe Tegernsee".to_owned())
        );
        assert_eq!(parse_group_name(" \u{200b}\n"), Err(GroupTextError::Empty));
        let long = "ä".repeat(MAX_GROUP_NAME_LENGTH);
        assert_eq!(parse_group_name(&long), Ok(long.clone()));
        assert_eq!(
            parse_group_name(&format!("{long}x")),
            Err(GroupTextError::TooLong(MAX_GROUP_NAME_LENGTH))
        );
        // Counted after cleaning.
        assert_eq!(parse_group_name(&format!(" {long}\u{202e}\t")), Ok(long));

        assert_eq!(
            parse_plan_place("Musterberg,\teast\u{1b} take-off"),
            Ok("Musterberg, east take-off".to_owned())
        );
        assert_eq!(parse_plan_place(""), Err(GroupTextError::Empty));
        assert_eq!(
            parse_plan_place(&"x".repeat(MAX_PLAN_PLACE_LENGTH + 1)),
            Err(GroupTextError::TooLong(MAX_PLAN_PLACE_LENGTH))
        );
    }

    #[test]
    fn a_note_keeps_its_lines_and_may_be_empty() {
        assert_eq!(parse_plan_note("  \n "), Ok(None));
        assert_eq!(
            parse_plan_note(" Bring a radio.\r\nLift opens at nine.\u{7} "),
            Ok(Some("Bring a radio.\nLift opens at nine.".to_owned()))
        );
        let long = "ä".repeat(MAX_PLAN_NOTE_LENGTH);
        assert_eq!(parse_plan_note(&long), Ok(Some(long.clone())));
        assert_eq!(
            parse_plan_note(&format!("{long}x")),
            Err(GroupTextError::TooLong(MAX_PLAN_NOTE_LENGTH))
        );
    }

    #[test]
    fn a_time_of_day_is_two_digits_a_colon_and_two_digits() {
        let time: TimeOfDay = "09:30".parse().unwrap();
        assert_eq!((time.hour(), time.minute()), (9, 30));
        assert_eq!(time.to_string(), "09:30");
        assert_eq!("00:00".parse::<TimeOfDay>().unwrap().to_string(), "00:00");
        assert_eq!("23:59".parse::<TimeOfDay>().unwrap().to_string(), "23:59");
        assert!(time < "10:00".parse().unwrap());
        for bad in [
            "",
            "9:30",
            "09:3",
            "24:00",
            "09:60",
            "09.30",
            "0930",
            "09:30:00",
            " 09:30",
            "ab:cd",
            "+9:30",
            "٠٩:٣٠",
        ] {
            assert_eq!(bad.parse::<TimeOfDay>(), Err(TimeOfDayError), "{bad:?}");
        }
        assert_eq!(serde_json::to_string(&time).unwrap(), r#""09:30""#);
        assert!(serde_json::from_str::<TimeOfDay>(r#""9.30""#).is_err());
        assert_eq!(TimeOfDay::new(24, 0), None);
    }

    #[test]
    fn what_a_plan_does_not_say_is_left_out() {
        let posted = chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap();
        let pilot = |handle: &str| Pilot {
            handle: Handle::new(handle).unwrap(),
            display_name: handle.to_owned(),
            country: None,
            picture: None,
        };
        let plan = Plan {
            id: "0123456789abcdefghjkmnpqrs".parse().unwrap(),
            group: "00000000000000000000000001".parse().unwrap(),
            day: NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
            place: "Musterberg".to_owned(),
            position: None,
            meet_at: None,
            note: None,
            author: pilot("mara"),
            created_at: posted,
            changed_at: posted,
            pilots_in: vec![pilot("mara"), pilot("jonas")],
        };

        let json = serde_json::to_value(&plan).unwrap();
        let object = json.as_object().unwrap();

        for absent in ["position", "meet_at", "note"] {
            assert!(!object.contains_key(absent), "{absent}");
        }
        assert_eq!(json["day"], "2026-11-20");
        assert_eq!(json["in"][1]["handle"], "jonas");
        assert_eq!(serde_json::from_value::<Plan>(json).unwrap(), plan);

        // A request is read the same way, and a day is a date and nothing
        // else.
        let request: PlanRequest = serde_json::from_str(
            r#"{"day":"2026-11-20","place":"Musterberg","meet_at":"09:30",
                "position":{"lat":47.6642,"lon":11.7967}}"#,
        )
        .unwrap();
        assert_eq!(request.meet_at, TimeOfDay::new(9, 30));
        assert_eq!(request.note, None);
        for bad in [
            r#"{"day":"2026-11-20T09:30:00Z","place":"x"}"#,
            r#"{"day":"20.11.2026","place":"x"}"#,
            r#"{"day":"2026-11-31","place":"x"}"#,
            r#"{"day":"2026-11-20","place":"x","meet_at":"9"}"#,
            r#"{"day":"2026-11-20","place":"x","position":{"lat":147.6,"lon":11.8}}"#,
            r#"{"day":"2026-11-20"}"#,
            r#"{"place":"x"}"#,
        ] {
            assert!(serde_json::from_str::<PlanRequest>(bad).is_err(), "{bad}");
        }
    }
}
