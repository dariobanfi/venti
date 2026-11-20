//! Reports: how a signed-in pilot tells whoever runs the server that a
//! flight, a pilot's page or a live track should not be there.
//!
//! A report is read by the operator and by nobody else. The server does
//! nothing by itself when one arrives, and it never tells the pilot who
//! was reported that there is a report, or who made it.

use serde::{Deserialize, Serialize};

use super::text;

/// The most characters the text of a report may have.
pub const MAX_REPORT_TEXT_LENGTH: usize = 1000;

/// What a report is about. Exactly one of the three.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSubject {
    /// A flight that is public or shared by link, by its address: the
    /// `id` of the flight, which is also the end of its share link.
    Flight(String),
    /// A pilot's page, by the pilot's handle.
    Pilot(String),
    /// A live track, by its identifier, which is also the end of its link.
    LiveTrack(String),
}

/// Why something is reported.
///
/// The list is short on purpose and is the owner's to change. A server
/// refuses a reason it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    /// A name, a glider's name or another text that insults, threatens or
    /// harasses.
    Offensive,
    /// The account passes itself off as another person or as an
    /// organisation.
    Impersonation,
    /// It shows something about a person who did not agree to that: a
    /// flight that is someone else's, a name, where someone lives.
    Privacy,
    /// The flight was not flown as it is shown: a track that is made up or
    /// altered, or not this pilot's own.
    NotAFlight,
    /// Advertising, or something that has nothing to do with flying.
    Spam,
    /// Anything else. The report must then say what, in its text.
    Other,
    /// A reason this version does not know. A server refuses it; a client
    /// does not send it.
    #[serde(other)]
    Unknown,
}

impl ReportReason {
    /// The reasons a report can give, in the order a form lists them.
    pub const ALL: [Self; 6] = [
        Self::Offensive,
        Self::Impersonation,
        Self::Privacy,
        Self::NotAFlight,
        Self::Spam,
        Self::Other,
    ];

    /// The reason's name on the wire; `None` for a reason nobody knows.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Offensive => Some("offensive"),
            Self::Impersonation => Some("impersonation"),
            Self::Privacy => Some("privacy"),
            Self::NotAFlight => Some("not_a_flight"),
            Self::Spam => Some("spam"),
            Self::Other => Some("other"),
            Self::Unknown => None,
        }
    }
}

/// Why the text of a report is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReportTextError {
    /// It has more than [`MAX_REPORT_TEXT_LENGTH`] characters.
    #[error("the text of a report has at most {MAX_REPORT_TEXT_LENGTH} characters")]
    TooLong,
}

/// Makes the text of a report fit to keep: what the server stores of it,
/// and what a client may show as "this is what will be sent".
///
/// Line breaks stay, as single line feeds. Every other control character
/// is dropped, and so is what would hide text or turn it round on the
/// operator's screen. Leading and trailing white space goes. `None` when
/// nothing is left. Text is only refused for its length, counted after
/// all that: a report must not be lost over a stray character.
pub fn parse_report_text(text: &str) -> Result<Option<String>, ReportTextError> {
    let kept = text::lines(text);
    if kept.chars().count() > MAX_REPORT_TEXT_LENGTH {
        return Err(ReportTextError::TooLong);
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// The body of `POST /v1/reports`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRequest {
    /// What is reported.
    pub subject: ReportSubject,
    /// Why.
    pub reason: ReportReason,
    /// What the reporter wants to add, in their own words; see
    /// [`parse_report_text`]. May be left out, except with
    /// [`ReportReason::Other`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// The answer of `POST /v1/reports`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportResponse {
    /// `true` when this request stored the report, `false` when the
    /// account had an open report about the same thing already. Either
    /// way whoever runs the server will read one.
    pub created: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_one_subject_and_a_reason() {
        let request = ReportRequest {
            subject: ReportSubject::Flight("0123456789abcdefghjkmnpqrs".to_owned()),
            reason: ReportReason::NotAFlight,
            text: None,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "subject": { "flight": "0123456789abcdefghjkmnpqrs" },
                "reason": "not_a_flight"
            })
        );
        assert_eq!(
            serde_json::from_value::<ReportRequest>(json).unwrap(),
            request
        );

        let page = r#"{"subject":{"pilot":"demo"},"reason":"offensive","text":"The name."}"#;
        let request: ReportRequest = serde_json::from_str(page).unwrap();
        assert_eq!(request.subject, ReportSubject::Pilot("demo".to_owned()));
        assert_eq!(request.text.as_deref(), Some("The name."));
        let track = r#"{"subject":{"live_track":"11111111111111111111111111"},"reason":"spam"}"#;
        let request: ReportRequest = serde_json::from_str(track).unwrap();
        assert!(matches!(request.subject, ReportSubject::LiveTrack(_)));

        // A subject of a kind nobody knows, two subjects, or none, is no
        // report.
        for bad in [
            r#"{"subject":{"site":"osm:1"},"reason":"spam"}"#,
            r#"{"subject":{"flight":"a","pilot":"b"},"reason":"spam"}"#,
            r#"{"subject":{},"reason":"spam"}"#,
            r#"{"subject":"demo","reason":"spam"}"#,
            r#"{"reason":"spam"}"#,
            r#"{"subject":{"pilot":"demo"}}"#,
        ] {
            assert!(serde_json::from_str::<ReportRequest>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_reason_has_a_name_and_an_unknown_one_decodes() {
        for reason in ReportReason::ALL {
            let name = reason.name().unwrap();
            let written = serde_json::to_string(&reason).unwrap();
            assert_eq!(written, format!("\"{name}\""));
            assert_eq!(
                serde_json::from_str::<ReportReason>(&written).unwrap(),
                reason
            );
        }
        let unknown: ReportReason = serde_json::from_str(r#""airspace""#).unwrap();
        assert_eq!(unknown, ReportReason::Unknown);
        assert_eq!(unknown.name(), None);
        assert!(!ReportReason::ALL.contains(&ReportReason::Unknown));
    }

    #[test]
    fn the_text_of_a_report_is_cleaned_and_bounded() {
        assert_eq!(parse_report_text(""), Ok(None));
        assert_eq!(parse_report_text("  \n\t "), Ok(None));
        assert_eq!(
            parse_report_text("  Not his flight.\r\nI was there. "),
            Ok(Some("Not his flight.\nI was there.".to_owned()))
        );
        // Any script and any sign may stay.
        let words = "Das ist nicht sein Flug – «c'est le mien» 这不是他的 🪂 <b>";
        assert_eq!(parse_report_text(words), Ok(Some(words.to_owned())));
        // What a terminal would act on, or what hides or turns text round,
        // does not.
        let hostile = "a\u{1b}[2Jb\u{0}c\u{7}d\u{202e}e\u{200b}f\u{9b}g\rh\ti\u{2028}j";
        assert_eq!(
            parse_report_text(hostile),
            Ok(Some("a[2Jbcdefghij".to_owned()))
        );

        // A thousand characters, counted as characters and after cleaning.
        let long = "ä".repeat(MAX_REPORT_TEXT_LENGTH);
        assert_eq!(parse_report_text(&long), Ok(Some(long.clone())));
        let padded = format!("  {long}\u{200b}\n");
        assert_eq!(parse_report_text(&padded), Ok(Some(long.clone())));
        assert_eq!(
            parse_report_text(&format!("{long}x")),
            Err(ReportTextError::TooLong)
        );
    }
}
