//! Help after landing: a pilot who has landed asks the pilots nearby for
//! a lift or a hand ("I've landed by the river at Schwaz, can anyone pick
//! me up?"). It goes to the phones of pilots who said they may be asked
//! and whose phone told the server a coarse position in the last hour,
//! nearest first; whoever can help says so, and only then sees where the
//! pilot is. A request ends after [`HELP_OPEN_HOURS`] hours, or when its
//! pilot closes it.
//!
//! It is not an emergency service: for an injury the pilot calls the
//! emergency number first, which every screen of it says first
//! ([`HELP_EMERGENCY_WORDS`]).

use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

use super::{GroupTextError, Pilot, text};

/// Path of the requests for help: `POST` asks the pilots nearby, `GET`
/// lists the account's own and those it was asked.
pub const HELP_PATH: &str = "/v1/help";

/// Path of one request for help: `GET` reads it, `DELETE` closes it.
#[must_use]
pub fn help_path(id: &str) -> String {
    format!("{HELP_PATH}/{id}")
}

/// Path of a pilot's answer to a request they were asked: `PUT` says
/// whether they can help.
#[must_use]
pub fn help_answer_path(id: &str) -> String {
    format!("{HELP_PATH}/{id}/answer")
}

/// The first words of every screen of it, before anything else.
pub const HELP_EMERGENCY_WORDS: &str = "Injured, or someone else is? Call 112 first. \
     This asks pilots nearby for a lift or a hand, and is no emergency service.";

/// The most characters a request's note may have.
pub const MAX_HELP_NOTE_LENGTH: usize = 280;

/// How far around the pilot others are asked, in kilometres.
pub const HELP_RADIUS_KM: u32 = 30;

/// The most pilots one request is sent to: the nearest.
pub const MAX_HELP_RECIPIENTS: usize = 30;

/// The most requests an account makes in a day.
pub const HELP_REQUESTS_PER_DAY: u32 = 3;

/// How long a request stands, in hours, unless its pilot closes it.
pub const HELP_OPEN_HOURS: i64 = 4;

/// Makes the note of a request fit to keep: line breaks stay, `None` when
/// nothing is left, refused when longer than [`MAX_HELP_NOTE_LENGTH`].
pub fn parse_help_note(text: &str) -> Result<Option<String>, GroupTextError> {
    let kept = text::lines(text);
    if kept.chars().count() > MAX_HELP_NOTE_LENGTH {
        return Err(GroupTextError::TooLong(MAX_HELP_NOTE_LENGTH));
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// The body of `POST /v1/help`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpRequestBody {
    /// Where the pilot is. Shown only to the pilots who answer that they
    /// can help.
    pub position: GeoPoint,
    /// What the pilot needs, in their own words, at most
    /// [`MAX_HELP_NOTE_LENGTH`] characters. May be left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The body of `PUT /v1/help/{id}/answer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelpAnswerBody {
    /// Whether the pilot can help. `false` takes an earlier `true` back.
    pub can_help: bool,
}

/// A request for help, as the one reading it may see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpRequest {
    /// Its identifier.
    pub id: String,
    /// The pilot who asks, by handle and display name.
    pub pilot: Pilot,
    /// What they need, if they said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// When they asked.
    pub created_at: Timestamp,
    /// When it ends by itself.
    pub ends_at: Timestamp,
    /// When its pilot closed it, if they did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<Timestamp>,
    /// Whether the one reading it is the pilot who asks.
    pub mine: bool,
    /// Where the pilot is: to the pilot, and to those who answered that
    /// they can help. Left out for everyone else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GeoPoint>,
    /// To a pilot who was asked: how far, in whole kilometres, the pilot
    /// is from the coarse position the reader's phone last told, if it
    /// still is kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<u32>,
    /// To a pilot who was asked: their own answer, `true` if they said
    /// they can help.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_help: Option<bool>,
    /// To the pilot who asks: how many pilots it was sent to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<u32>,
    /// To the pilot who asks: who answered that they can help, the first
    /// first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helpers: Vec<Pilot>,
}

/// The answer of `POST /v1/help`, of `GET /v1/help/{id}` and of
/// `PUT /v1/help/{id}/answer`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpResponse {
    /// The request.
    pub request: HelpRequest,
}

/// The answer of `GET /v1/help`: the requests that still stand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpListResponse {
    /// The account's own, the newest first.
    pub mine: Vec<HelpRequest>,
    /// Those the account was asked, the newest first.
    pub asked: Vec<HelpRequest>,
}

/// The title and line a phone shows for a request it is asked.
#[must_use]
pub fn help_request_words(display_name: &str, note: Option<&str>) -> (String, String) {
    let title = format!("{display_name} has landed nearby and asks for help");
    let body = match note {
        Some(note) => note.to_owned(),
        None => "A lift or a hand. Open to answer.".to_owned(),
    };
    (title, body)
}

/// The title and line a phone shows its pilot when someone can help.
#[must_use]
pub fn help_answer_words(display_name: &str) -> (String, String) {
    (
        format!("{display_name} can help"),
        "They now see where you are. Open to see who answered.".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_kept_short() {
        assert_eq!(parse_help_note("  ").unwrap(), None);
        assert_eq!(
            parse_help_note("By the river").unwrap().as_deref(),
            Some("By the river")
        );
        assert!(parse_help_note(&"x".repeat(MAX_HELP_NOTE_LENGTH + 1)).is_err());
    }

    #[test]
    fn the_words_name_the_pilot() {
        let (title, body) = help_request_words("Anna", None);
        assert_eq!(title, "Anna has landed nearby and asks for help");
        assert_eq!(body, "A lift or a hand. Open to answer.");
        assert!(HELP_EMERGENCY_WORDS.starts_with("Injured"));
        assert!(HELP_EMERGENCY_WORDS.contains("112"));
    }
}
