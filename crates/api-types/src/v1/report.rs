//! Reports: how a signed-in pilot tells whoever runs the server that a
//! flight, a pilot's page, a live track, a group, a plan of a group or a
//! site that pilots made should not be there, or not as it is.
//!
//! A report is read by the operator and by nobody else. The server does
//! nothing by itself when one arrives, and it never tells the pilot who
//! was reported, or the members of a group, that there is a report, or who
//! made it.

use serde::{Deserialize, Serialize};

use super::text;

/// The most characters the text of a report may have.
pub const MAX_REPORT_TEXT_LENGTH: usize = 1000;

/// What a report is about. Exactly one of the ten.
///
/// The list is closed, and a subject travels one way only, from a client
/// to the server: no answer carries one. A server that is sent a kind of
/// subject it does not know cannot read the request, answers
/// [`ErrorCode::InvalidRequest`](super::ErrorCode::InvalidRequest) and
/// stores nothing. That is what a server from before groups could be
/// reported says to a report about a group or a plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSubject {
    /// A flight that is public or shared by link, by its address: the
    /// `id` of the flight, which is also the end of its share link.
    Flight(String),
    /// A pilot's page, by the pilot's handle.
    Pilot(String),
    /// A pilot's picture, by the pilot's handle: the picture the pilot
    /// shows when the report is made. Since 2026-11-23.
    Picture(String),
    /// A live track, by its identifier, which is also the end of its link.
    LiveTrack(String),
    /// A group the reporter is in, by its identifier. What is reported is
    /// its name, which is all of a group that is words.
    Group(String),
    /// A plan of a group the reporter is in. What is reported is its
    /// place and its note.
    Plan(ReportedPlan),
    /// An open plan, by its identifier. What is reported is its place and
    /// its note. Since 2026-11-23.
    OpenPlan(String),
    /// A request for help after landing that the reporter was asked, by
    /// its identifier. What is reported is its note. Since 2026-11-23.
    HelpRequest(String),
    /// A site that pilots made on Venti, as it stands or as one of its
    /// revisions said it. What is reported is what the record says: its
    /// place, its name and its texts. A site of a source the server
    /// collects is not reported here: it is corrected at its source.
    Site(ReportedSite),
    /// A comment on a public flight. What is reported is its words.
    Comment(ReportedComment),
}

/// A comment, as a report names it: by its identifier and its flight's,
/// which is how every request about a comment names it
/// (`/v1/flights/{id}/comments/{comment}`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportedComment {
    /// The address of the flight the comment is on.
    pub flight: String,
    /// The comment's identifier.
    pub id: String,
}

/// A site that pilots made, as a report names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportedSite {
    /// The site's identifier: `venti:` and 26 characters.
    pub id: String,
    /// The number of the revision the report is about. Left out, the
    /// report is about the site as it stands, and the server notes the
    /// revision it stood at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
}

/// A plan, as a report names it: by its identifier and its group's, which
/// is how every request about a plan names it (`/v1/groups/{id}/plans/{plan}`).
/// A [`Plan`](super::Plan) carries both.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportedPlan {
    /// The identifier of the group the plan was posted to.
    pub group: String,
    /// The plan's identifier.
    pub id: String,
}

impl ReportSubject {
    /// The reasons a report about this kind of subject can give, in the
    /// order a form lists them. A server refuses any other.
    ///
    /// A group and a plan are words and no flight, so
    /// [`ReportReason::NotAFlight`] is not among theirs. A site has two
    /// reasons of its own, and is nobody's account and no flight.
    pub fn reasons(&self) -> &'static [ReportReason] {
        match self {
            Self::Flight(_) | Self::Pilot(_) | Self::LiveTrack(_) => &ReportReason::ALL,
            Self::Group(_)
            | Self::Plan(_)
            | Self::OpenPlan(_)
            | Self::HelpRequest(_)
            | Self::Comment(_)
            | Self::Picture(_) => &ReportReason::ABOUT_WORDS,
            Self::Site(_) => &ReportReason::ABOUT_A_SITE,
        }
    }
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
    /// A site is at the wrong place, or says something that is wrong in a
    /// way that can hurt a pilot: a wind direction, a height, a landing
    /// that is none. Only a report about a site gives it.
    WrongOrDangerous,
    /// The place is no flying site: nobody takes off or lands there, or
    /// nobody may. Only a report about a site gives it.
    NotASite,
    /// A reason this version does not know. A server refuses it; a client
    /// does not send it.
    #[serde(other)]
    Unknown,
}

impl ReportReason {
    /// The reasons a report about a flight, a pilot's page or a live
    /// track can give, in the order a form lists them. Which reasons a
    /// report about one kind of subject can give is said by
    /// [`ReportSubject::reasons`].
    pub const ALL: [Self; 6] = [
        Self::Offensive,
        Self::Impersonation,
        Self::Privacy,
        Self::NotAFlight,
        Self::Spam,
        Self::Other,
    ];

    /// The reasons a report about a group, a plan, a comment or a picture
    /// can give: all but the one that is about a flight.
    pub const ABOUT_WORDS: [Self; 5] = [
        Self::Offensive,
        Self::Impersonation,
        Self::Privacy,
        Self::Spam,
        Self::Other,
    ];

    /// The reasons a report about a site that pilots made can give: the
    /// two of its own first, then those about a text, without the one
    /// about an account and the one about a flight.
    pub const ABOUT_A_SITE: [Self; 6] = [
        Self::WrongOrDangerous,
        Self::NotASite,
        Self::Offensive,
        Self::Privacy,
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
            Self::WrongOrDangerous => Some("wrong_or_dangerous"),
            Self::NotASite => Some("not_a_site"),
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
    fn a_group_is_named_by_its_identifier_and_a_plan_with_its_group() {
        let group = ReportRequest {
            subject: ReportSubject::Group("44444444444444444444444444".to_owned()),
            reason: ReportReason::Offensive,
            text: None,
        };
        let json = serde_json::to_value(&group).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "subject": { "group": "44444444444444444444444444" },
                "reason": "offensive"
            })
        );
        assert_eq!(
            serde_json::from_value::<ReportRequest>(json).unwrap(),
            group
        );

        let plan = ReportRequest {
            subject: ReportSubject::Plan(ReportedPlan {
                group: "44444444444444444444444444".to_owned(),
                id: "77777777777777777777777777".to_owned(),
            }),
            reason: ReportReason::Spam,
            text: Some("An advertisement, not a plan.".to_owned()),
        };
        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "subject": { "plan": {
                    "group": "44444444444444444444444444",
                    "id": "77777777777777777777777777"
                } },
                "reason": "spam",
                "text": "An advertisement, not a plan."
            })
        );
        assert_eq!(serde_json::from_value::<ReportRequest>(json).unwrap(), plan);

        // A plan without its group, a plan by its identifier alone, and a
        // group that is more than an identifier are no subjects.
        for bad in [
            r#"{"subject":{"plan":"77777777777777777777777777"},"reason":"spam"}"#,
            r#"{"subject":{"plan":{"id":"77777777777777777777777777"}},"reason":"spam"}"#,
            r#"{"subject":{"plan":{"group":"44444444444444444444444444"}},"reason":"spam"}"#,
            r#"{"subject":{"group":{"id":"44444444444444444444444444"}},"reason":"spam"}"#,
            r#"{"subject":{"group":"a","plan":{"group":"a","id":"b"}},"reason":"spam"}"#,
        ] {
            assert!(serde_json::from_str::<ReportRequest>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_site_is_reported_by_its_identifier_with_or_without_a_revision() {
        let site = ReportRequest {
            subject: ReportSubject::Site(ReportedSite {
                id: "venti:55555555555555555555555555".to_owned(),
                revision: None,
            }),
            reason: ReportReason::WrongOrDangerous,
            text: Some("The take-off is on the other side of the ridge.".to_owned()),
        };
        let json = serde_json::to_value(&site).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "subject": { "site": { "id": "venti:55555555555555555555555555" } },
                "reason": "wrong_or_dangerous",
                "text": "The take-off is on the other side of the ridge."
            })
        );
        assert_eq!(serde_json::from_value::<ReportRequest>(json).unwrap(), site);
        let revision: ReportRequest = serde_json::from_str(
            r#"{"subject":{"site":{"id":"venti:55555555555555555555555555","revision":2}},
                "reason":"not_a_site"}"#,
        )
        .unwrap();
        assert!(matches!(
            revision.subject,
            ReportSubject::Site(ReportedSite {
                revision: Some(2),
                ..
            })
        ));
        assert_eq!(revision.reason, ReportReason::NotASite);
        // A site is nobody's account and no flight, and has two reasons
        // of its own that nothing else is reported for.
        let reasons = revision.subject.reasons();
        assert_eq!(reasons, ReportReason::ABOUT_A_SITE);
        assert!(!reasons.contains(&ReportReason::Impersonation));
        assert!(!reasons.contains(&ReportReason::NotAFlight));
        for own in [ReportReason::WrongOrDangerous, ReportReason::NotASite] {
            assert!(own.name().is_some());
            assert!(!ReportReason::ALL.contains(&own));
            assert!(!ReportReason::ABOUT_WORDS.contains(&own));
        }
    }

    #[test]
    fn a_group_and_a_plan_take_every_reason_but_the_one_about_a_flight() {
        let about = |subject: &ReportSubject| subject.reasons().to_vec();
        for subject in [
            ReportSubject::Flight("a".to_owned()),
            ReportSubject::Pilot("a".to_owned()),
            ReportSubject::LiveTrack("a".to_owned()),
        ] {
            assert_eq!(about(&subject), ReportReason::ALL);
        }
        let plan = ReportSubject::Plan(ReportedPlan {
            group: "a".to_owned(),
            id: "b".to_owned(),
        });
        for subject in [ReportSubject::Group("a".to_owned()), plan] {
            let reasons = about(&subject);
            assert_eq!(reasons, ReportReason::ABOUT_WORDS);
            assert!(!reasons.contains(&ReportReason::NotAFlight));
            assert!(!reasons.contains(&ReportReason::Unknown));
            // In the order of the whole list, and nothing that is not on it.
            let of_all: Vec<ReportReason> = ReportReason::ALL
                .into_iter()
                .filter(|reason| *reason != ReportReason::NotAFlight)
                .collect();
            assert_eq!(reasons, of_all);
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
