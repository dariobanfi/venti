//! Reporting: how a pilot tells whoever runs the server that a flight, a
//! pilot's page, a live track, a group, a plan of a group or a site that
//! pilots made should not be there, or not as it is.
//!
//! A report goes to the operator and to nobody else. The server does
//! nothing by itself when one arrives, the pilot who is reported is not
//! told, nor is anyone in a group, and nothing comes back to the one who
//! reported. The operator sees who sent it, which the form has to say.
//!
//! The form is the subject's: a group and a plan are words and no flight,
//! so their forms leave the reason about a flight out and say what the
//! operator is given to read. A site that pilots made is nobody's, and
//! its form has the reasons of a site: that it is at the wrong place or
//! says something dangerous, and that it is no flying site. A shell asks
//! for the form with what is reported ([`report_form_for`]) and shows
//! what it gets.

use venti_api_types::v1::{
    self as api, GroupId, MAX_REPORT_TEXT_LENGTH, PlanId, ReportRequest, ReportResponse,
    parse_report_text,
};
use venti_domain::SiteId;

use crate::account::json_request;
use crate::server::{answer, code};
use crate::{CoreError, ServerRequest};

/// What a report is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(
    feature = "wasm",
    serde(
        tag = "kind",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum ReportSubject {
    /// A flight someone shared.
    Flight {
        /// Its address on the server.
        id: String,
    },
    /// A pilot's page.
    Pilot {
        /// The pilot's handle.
        handle: String,
    },
    /// A flight shared while it is under way.
    LiveTrack {
        /// The track's identifier.
        id: String,
    },
    /// A group the pilot is in. What is reported is its name.
    Group {
        /// The group's identifier on the server.
        id: String,
    },
    /// A plan of a group the pilot is in. What is reported is its place
    /// and its note.
    Plan {
        /// The identifier of the group the plan belongs to.
        group_id: String,
        /// The plan's identifier.
        id: String,
    },
    /// A site that pilots made on Venti, as it stands. What is reported
    /// is what it says: its place, its name and its texts. A site of
    /// another source is not reported: it is corrected at its source.
    Site {
        /// The site's identifier.
        id: String,
    },
    /// One revision of such a site, from its history: for words that
    /// stood there once and can still be read.
    SiteRevision {
        /// The site's identifier.
        id: String,
        /// The revision's number.
        revision: u32,
    },
}

/// Why something is reported. The list is the server's and is short on
/// purpose: none of the reasons is about how someone flew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum ReportReason {
    /// A name or another text that insults, threatens or harasses.
    Offensive,
    /// The account passes itself off as someone else.
    Impersonation,
    /// It shows something about a person who did not agree to that.
    Privacy,
    /// The flight was not flown as it is shown, or not by this pilot.
    NotAFlight,
    /// Advertising, or nothing to do with flying.
    Spam,
    /// Anything else. The report then has to say what.
    Other,
    /// A site is at the wrong place, or says something wrong that can
    /// hurt a pilot. Only the form for a site offers it.
    WrongOrDangerous,
    /// The place is no flying site. Only the form for a site offers it.
    NotASite,
}

/// A reason as a form offers it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ReportReasonChoice {
    /// The reason.
    pub reason: ReportReason,
    /// Its name in a list.
    pub title: String,
    /// What it means, in a line under the name.
    pub explanation: String,
}

/// What a form for a report has to work with.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ReportForm {
    /// The reasons, in the order to list them.
    pub reasons: Vec<ReportReasonChoice>,
    /// The most characters the pilot's own words may have.
    pub text_at_most: u32,
    /// What happens to a report, to show before it is sent.
    pub notice: String,
}

/// The kinds of thing a form is written for.
#[derive(Clone, Copy)]
enum Reported {
    /// A flight, a pilot's page or a live track: what a pilot shares with
    /// everyone or by a link.
    Shared,
    /// A group: its name.
    Group,
    /// A plan of a group: its place and its note.
    Plan,
    /// A site that pilots made: its place, its name and its texts.
    Site,
    /// A comment under a flight: its words.
    Comment,
}

/// What a reason is called in a form and what it means there. `None` for
/// a reason that a report about this kind of thing cannot give.
fn choice(reason: ReportReason, reported: Reported) -> Option<(&'static str, &'static str)> {
    use Reported::{Comment, Group, Plan, Shared, Site};
    Some(match (reason, reported) {
        (ReportReason::Offensive, Shared) => (
            "Insulting or threatening",
            "A name, a glider's name or another text that insults, threatens or harasses.",
        ),
        (ReportReason::Offensive, Group) => (
            "Insulting or threatening",
            "The group's name insults, threatens or harasses.",
        ),
        (ReportReason::Offensive, Plan) => (
            "Insulting or threatening",
            "The plan's place or note insults, threatens or harasses.",
        ),
        (ReportReason::WrongOrDangerous, Site) => (
            "Wrong place or dangerous",
            "The site is not where it is marked, or says something wrong that can hurt a pilot: a wind direction, a height, a landing that is none.",
        ),
        (ReportReason::NotASite, Site) => (
            "Not a flying site",
            "Nobody takes off or lands here, or nobody may.",
        ),
        // The reasons of a site are about a site.
        (
            ReportReason::WrongOrDangerous | ReportReason::NotASite,
            Shared | Group | Plan | Comment,
        ) => {
            return None;
        }
        (ReportReason::Offensive, Comment) => (
            "Insulting or threatening",
            "The comment insults, threatens or harasses.",
        ),
        (ReportReason::Impersonation, Comment) => (
            "Pretends to be someone else",
            "The comment is written as if by another person or an organisation.",
        ),
        (ReportReason::Privacy, Comment) => (
            "Shows something private",
            "The comment says something about a person who did not agree to it: a name, where someone lives.",
        ),
        (ReportReason::Offensive, Site) => (
            "Insulting or threatening",
            "The site's name or one of its texts insults, threatens or harasses.",
        ),
        (ReportReason::Privacy, Site) => (
            "Shows something private",
            "A text of the site says something about a person who did not agree to it: a name, where someone lives.",
        ),
        // A site is nobody's account and no flight.
        (ReportReason::Impersonation | ReportReason::NotAFlight, Site) => return None,
        (ReportReason::Impersonation, Shared) => (
            "Pretends to be someone else",
            "The account passes itself off as another person or as an organisation.",
        ),
        (ReportReason::Impersonation, Group) => (
            "Pretends to be someone else",
            "The name passes the group off as a club, a school or an organisation it is not.",
        ),
        (ReportReason::Impersonation, Plan) => (
            "Pretends to be someone else",
            "The plan passes itself off as another pilot's, a club's or an organisation's.",
        ),
        (ReportReason::Privacy, Shared) => (
            "Shows something private",
            "Something about a person who did not agree to it: someone else's flight, a name, where someone lives.",
        ),
        (ReportReason::Privacy, Group) => (
            "Shows something private",
            "The name says something about a person who did not agree to it.",
        ),
        (ReportReason::Privacy, Plan) => (
            "Shows something private",
            "The plan says something about a person who did not agree to it: a name, where someone lives.",
        ),
        (ReportReason::NotAFlight, Shared) => (
            "Not a real flight",
            "A track that is made up or altered, or that this pilot did not fly.",
        ),
        // A group and a plan are words and no flight.
        (ReportReason::NotAFlight, Group | Plan | Comment) => return None,
        (ReportReason::Spam, _) => (
            "Advertising",
            "Advertising, or something that has nothing to do with flying.",
        ),
        (ReportReason::Other, _) => ("Something else", "Say what in your own words."),
    })
}

/// Every reason, in the order the server lists them: the two of a site
/// first, as its form lists them, then the rest.
const REASONS: [ReportReason; 8] = [
    ReportReason::WrongOrDangerous,
    ReportReason::NotASite,
    ReportReason::Offensive,
    ReportReason::Impersonation,
    ReportReason::Privacy,
    ReportReason::NotAFlight,
    ReportReason::Spam,
    ReportReason::Other,
];

fn form(reported: Reported) -> ReportForm {
    let reasons = REASONS
        .into_iter()
        .filter_map(|reason| {
            let (title, explanation) = choice(reason, reported)?;
            Some(ReportReasonChoice {
                reason,
                title: title.to_owned(),
                explanation: explanation.to_owned(),
            })
        })
        .collect();
    let notice = match reported {
        Reported::Shared => {
            "Whoever runs Venti reads the report and sees that it came from you. The pilot you report is not told, and no answer comes back."
        }
        Reported::Group => {
            "Whoever runs Venti reads the report, with the group's name as it is now, and sees that it came from you. Nobody in the group is told, and no answer comes back."
        }
        Reported::Plan => {
            "Whoever runs Venti reads the report, with the plan's place and note as they are now, and sees that it came from you. Nobody in the group is told, and no answer comes back."
        }
        Reported::Comment => {
            "Whoever runs Venti reads the report, with the comment as it is now, and sees that it came from you. Its author is not told, and no answer comes back."
        }
        Reported::Site => {
            "Whoever runs Venti reads the report, with the site and its history, and sees that it came from you. The pilots who added or changed the site are not told, and no answer comes back. If you know what is right, correcting the site yourself is quicker."
        }
    };
    ReportForm {
        reasons,
        text_at_most: u32::try_from(MAX_REPORT_TEXT_LENGTH).unwrap_or(u32::MAX),
        notice: notice.to_owned(),
    }
}

/// The form for a report about a flight, a pilot's page or a live track:
/// the reasons such a report can give and what the pilot is told before
/// sending one. For a group or a plan, and for any subject a shell has in
/// its hand, [`report_form_for`] is the one to ask.
#[uniffi::export]
pub fn report_form() -> ReportForm {
    form(Reported::Shared)
}

/// The form for a report about `subject`: the reasons a report about such
/// a thing can give, each with a line that says what it means there, and
/// what the pilot is told before sending one. Which thing it is does not
/// matter, only what kind: the form is read before anything is sent.
#[uniffi::export]
pub fn report_form_for(subject: ReportSubject) -> ReportForm {
    form(match subject {
        ReportSubject::Flight { .. }
        | ReportSubject::Pilot { .. }
        | ReportSubject::LiveTrack { .. } => Reported::Shared,
        ReportSubject::Group { .. } => Reported::Group,
        ReportSubject::Plan { .. } => Reported::Plan,
        ReportSubject::Site { .. } | ReportSubject::SiteRevision { .. } => Reported::Site,
    })
}

/// The request that sends a report. `text` is what the pilot wrote, and
/// may be empty except with [`ReportReason::Other`].
///
/// # Errors
///
/// When the text is too long, or missing where it is needed, the subject
/// has no address, or the reason is one its form does not offer.
#[uniffi::export]
pub fn report_request(
    subject: ReportSubject,
    reason: ReportReason,
    text: String,
) -> Result<ServerRequest, CoreError> {
    let subject = match subject {
        ReportSubject::Flight { id } => api::ReportSubject::Flight(id),
        ReportSubject::Pilot { handle } => api::ReportSubject::Pilot(handle),
        ReportSubject::LiveTrack { id } => api::ReportSubject::LiveTrack(id),
        ReportSubject::Group { id } => api::ReportSubject::Group(id),
        ReportSubject::Plan { group_id, id } => api::ReportSubject::Plan(api::ReportedPlan {
            group: group_id,
            id,
        }),
        ReportSubject::Site { id } => {
            api::ReportSubject::Site(api::ReportedSite { id, revision: None })
        }
        ReportSubject::SiteRevision { id, revision } => {
            api::ReportSubject::Site(api::ReportedSite {
                id,
                revision: Some(revision),
            })
        }
    };
    request_about(subject, reason, &text)
}

/// The form for a report about a comment under a flight: the reasons
/// such a report can give, each with what it means there, and what the
/// pilot is told before sending one. A comment has a form and a request
/// of its own, [`comment_report_request`], beside [`ReportSubject`].
#[uniffi::export]
pub fn comment_report_form() -> ReportForm {
    form(Reported::Comment)
}

/// The request that reports a comment under a public flight: its words.
/// `text` is what the pilot wrote, and may be empty except with
/// [`ReportReason::Other`]. The answer is read by [`report_sent`].
///
/// # Errors
///
/// As [`report_request`].
#[uniffi::export]
pub fn comment_report_request(
    flight_id: String,
    comment_id: String,
    reason: ReportReason,
    text: String,
) -> Result<ServerRequest, CoreError> {
    let subject = api::ReportSubject::Comment(api::ReportedComment {
        flight: flight_id.trim().to_owned(),
        id: comment_id.trim().to_owned(),
    });
    request_about(subject, reason, &text)
}

/// The request that reports `subject`, checked as the server checks it.
fn request_about(
    subject: api::ReportSubject,
    reason: ReportReason,
    text: &str,
) -> Result<ServerRequest, CoreError> {
    let text = parse_report_text(text).map_err(|_| CoreError::Server {
        message: format!("A report has room for {MAX_REPORT_TEXT_LENGTH} characters. Shorten it."),
    })?;
    if reason == ReportReason::Other && text.is_none() {
        return Err(CoreError::Server {
            message: "Say in a few words what is wrong.".to_owned(),
        });
    }
    // The server's list of subjects, read to its end: a kind it gains has
    // to be given words here before the core builds again.
    let names_something = match &subject {
        api::ReportSubject::Flight(name)
        | api::ReportSubject::Pilot(name)
        | api::ReportSubject::LiveTrack(name) => !name.trim().is_empty(),
        api::ReportSubject::Group(id) => id.parse::<GroupId>().is_ok(),
        api::ReportSubject::Plan(plan) => {
            plan.group.parse::<GroupId>().is_ok() && plan.id.parse::<PlanId>().is_ok()
        }
        api::ReportSubject::Comment(comment) => {
            comment.flight.parse::<api::FlightId>().is_ok() && comment.id.len() == 26
        }
        // Only a site that pilots made is reported, as it stands or at a
        // revision it has had.
        api::ReportSubject::Site(site) => {
            site.revision != Some(0)
                && site
                    .id
                    .parse::<SiteId>()
                    .is_ok_and(|id| crate::site_edit::is_own_layer(id.source().as_str()))
        }
    };
    if !names_something {
        return Err(CoreError::Server {
            message: "There is nothing here to report.".to_owned(),
        });
    }
    let reason = match reason {
        ReportReason::Offensive => api::ReportReason::Offensive,
        ReportReason::Impersonation => api::ReportReason::Impersonation,
        ReportReason::Privacy => api::ReportReason::Privacy,
        ReportReason::NotAFlight => api::ReportReason::NotAFlight,
        ReportReason::Spam => api::ReportReason::Spam,
        ReportReason::Other => api::ReportReason::Other,
        ReportReason::WrongOrDangerous => api::ReportReason::WrongOrDangerous,
        ReportReason::NotASite => api::ReportReason::NotASite,
    };
    // A form offers the reasons of its subject, and the server would
    // refuse any other.
    if !subject.reasons().contains(&reason) {
        let message = match reason {
            api::ReportReason::NotAFlight => "This reason is about a flight. Choose another.",
            api::ReportReason::WrongOrDangerous | api::ReportReason::NotASite => {
                "This reason is about a flying site. Choose another."
            }
            _ => "This reason does not fit what you report. Choose another.",
        };
        return Err(CoreError::Server {
            message: message.to_owned(),
        });
    }
    json_request(
        "POST",
        api::REPORTS_PATH,
        &ReportRequest {
            subject,
            reason,
            text,
        },
        true,
    )
}

/// Reads the server's answer to [`report_request`] and says in a line
/// what became of the report.
///
/// # Errors
///
/// The server's refusal, in words for the pilot.
#[uniffi::export]
pub fn report_sent(status: u16, body: Vec<u8>) -> Result<String, CoreError> {
    // What is gone cannot be reported, and that is no failure of the
    // pilot's: say so plainly. So for a day's worth of reports. A group
    // and a plan that are gone have their words where every answer about
    // a group gets them.
    let message = match (status, code(&body).as_deref()) {
        (_, Some("flight_not_found")) => Some("This flight is no longer there."),
        (_, Some("pilot_not_found")) => Some("This pilot is no longer there."),
        (_, Some("live_track_not_found")) => Some("This track is no longer there."),
        (_, Some("site_not_found" | "site_gone")) => Some("This site is no longer there."),
        (_, Some("site_revision_not_found")) => Some("The site has no such revision."),
        (_, Some("comment_not_found")) => Some("This comment is no longer there."),
        // A site of a source, which the form is not offered for.
        (_, Some("invalid_report")) => Some(
            "This cannot be reported here. A site of another source is corrected at its source.",
        ),
        (429, _) => Some("You have sent many reports today. Try again tomorrow."),
        // The core put the request together, so a server that cannot read
        // it knows fewer kinds of subject than the app: it is from before
        // groups and plans could be reported.
        (_, Some("invalid_request")) => {
            Some("This server cannot take this report. It may be older than the app.")
        }
        _ => None,
    };
    if let Some(message) = message {
        return Err(CoreError::Server {
            message: message.to_owned(),
        });
    }
    let response: ReportResponse = answer(status, &body)?;
    Ok(if response.created {
        "Sent. Whoever runs Venti will read it.".to_owned()
    } else {
        "You reported this before. Your report is still waiting to be read.".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(result: Result<impl std::fmt::Debug, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_report_is_put_together_as_the_server_reads_it() {
        let request = report_request(
            ReportSubject::Flight {
                id: "fbyw03atd9qdt5bfe177ra1k94".to_owned(),
            },
            ReportReason::NotAFlight,
            "  I flew there that day.\r\nThis is my track.\u{200B}  ".to_owned(),
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/reports");
        assert_eq!(request.content_type.as_deref(), Some("application/json"));
        assert!(request.needs_token);
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "subject": { "flight": "fbyw03atd9qdt5bfe177ra1k94" },
                "reason": "not_a_flight",
                // Cleaned as the server would clean it.
                "text": "I flew there that day.\nThis is my track."
            })
        );

        // A pilot's page and a live track; without words the text is left
        // out.
        let request = report_request(
            ReportSubject::Pilot {
                handle: "mara".to_owned(),
            },
            ReportReason::Impersonation,
            "   ".to_owned(),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({ "subject": { "pilot": "mara" }, "reason": "impersonation" })
        );
        let request = report_request(
            ReportSubject::LiveTrack {
                id: "33333333333333333333333333".to_owned(),
            },
            ReportReason::Spam,
            String::new(),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            body["subject"],
            serde_json::json!({ "live_track": "33333333333333333333333333" })
        );
    }

    /// An example of the server's documentation, as its tests keep it.
    fn golden(name: &str) -> serde_json::Value {
        let path = format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    const GROUP: &str = "44444444444444444444444444";
    const PLAN: &str = "77777777777777777777777777";

    fn a_group() -> ReportSubject {
        ReportSubject::Group {
            id: GROUP.to_owned(),
        }
    }

    fn a_plan() -> ReportSubject {
        ReportSubject::Plan {
            group_id: GROUP.to_owned(),
            id: PLAN.to_owned(),
        }
    }

    #[test]
    fn a_group_and_a_plan_are_reported_as_the_server_reads_them() {
        // The two examples of the server's documentation, word for word.
        let request = report_request(
            a_group(),
            ReportReason::Impersonation,
            " This is not the club's group. The club has none here.\n".to_owned(),
        )
        .unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/reports")
        );
        assert!(request.needs_token);
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(body, golden("report_group_request.json"));
        assert_eq!(body["subject"], serde_json::json!({ "group": GROUP }));

        let request = report_request(
            a_plan(),
            ReportReason::Spam,
            "An advertisement, not a plan.".to_owned(),
        )
        .unwrap();
        // In the body, like every subject: a path is what a log names.
        assert_eq!(request.path, "/v1/reports");
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(body, golden("report_plan_request.json"));
        assert_eq!(
            body["subject"],
            serde_json::json!({ "plan": { "group": GROUP, "id": PLAN } })
        );

        // What is no identifier names nothing, and is not sent.
        for nothing in [
            ReportSubject::Group { id: String::new() },
            ReportSubject::Group {
                id: "Mustergruppe".to_owned(),
            },
            ReportSubject::Plan {
                group_id: GROUP.to_owned(),
                id: " ".to_owned(),
            },
            ReportSubject::Plan {
                group_id: "mine".to_owned(),
                id: PLAN.to_owned(),
            },
        ] {
            assert_eq!(
                words(report_request(nothing, ReportReason::Spam, String::new())),
                "There is nothing here to report."
            );
        }
        // The reason about a flight is not one a group or a plan is
        // reported for; neither form offers it.
        for subject in [a_group(), a_plan()] {
            assert_eq!(
                words(report_request(
                    subject,
                    ReportReason::NotAFlight,
                    String::new()
                )),
                "This reason is about a flight. Choose another."
            );
        }
        // The rules about the pilot's own words are those of every report.
        assert_eq!(
            words(report_request(a_plan(), ReportReason::Other, String::new())),
            "Say in a few words what is wrong."
        );
        assert_eq!(
            words(report_request(
                a_group(),
                ReportReason::Spam,
                "x".repeat(1001)
            )),
            "A report has room for 1000 characters. Shorten it."
        );
    }

    #[test]
    fn the_form_for_a_group_or_a_plan_says_what_is_read_and_leaves_flights_out() {
        let flight = ReportSubject::Flight {
            id: "fbyw03atd9qdt5bfe177ra1k94".to_owned(),
        };
        let pilot = ReportSubject::Pilot {
            handle: "mara".to_owned(),
        };
        let track = ReportSubject::LiveTrack {
            id: "33333333333333333333333333".to_owned(),
        };
        // What a pilot shares has the form it always had.
        for shared in [flight, pilot, track] {
            assert_eq!(report_form_for(shared), report_form());
        }

        let group = report_form_for(a_group());
        let plan = report_form_for(a_plan());
        for form in [&group, &plan] {
            let listed: Vec<_> = form.reasons.iter().map(|choice| choice.reason).collect();
            assert_eq!(
                listed,
                [
                    ReportReason::Offensive,
                    ReportReason::Impersonation,
                    ReportReason::Privacy,
                    ReportReason::Spam,
                    ReportReason::Other,
                ]
            );
            assert_eq!(form.text_at_most, 1000);
            // A report is not anonymous, whatever it is about.
            assert!(form.notice.contains("sees that it came from you"));
            assert!(form.notice.contains("Nobody in the group is told"));
            assert!(form.notice.ends_with("no answer comes back."));
            for choice in &form.reasons {
                assert!(!choice.title.is_empty() && choice.explanation.ends_with('.'));
                assert!(!choice.explanation.contains("flight"), "{choice:?}");
                assert!(!choice.explanation.contains("glider"), "{choice:?}");
            }
        }
        // Each says what whoever runs the server is given to read.
        assert_eq!(
            group.notice,
            "Whoever runs Venti reads the report, with the group's name as it is now, and sees that it came from you. Nobody in the group is told, and no answer comes back."
        );
        assert_eq!(
            plan.notice,
            "Whoever runs Venti reads the report, with the plan's place and note as they are now, and sees that it came from you. Nobody in the group is told, and no answer comes back."
        );
        assert_eq!(
            group.reasons[0].explanation,
            "The group's name insults, threatens or harasses."
        );
        assert_eq!(
            plan.reasons[0].explanation,
            "The plan's place or note insults, threatens or harasses."
        );
        // The names of the reasons are the same in every form.
        let titles = |form: &ReportForm| -> Vec<String> {
            form.reasons.iter().map(|c| c.title.clone()).collect()
        };
        assert_eq!(titles(&group), titles(&plan));
        let shared = titles(&report_form());
        assert!(titles(&group).iter().all(|title| shared.contains(title)));
    }

    #[test]
    fn every_form_lists_the_reasons_the_server_takes_for_its_subject() {
        let subjects = [
            (
                ReportSubject::Pilot {
                    handle: "mara".to_owned(),
                },
                api::ReportSubject::Pilot("mara".to_owned()),
            ),
            (a_group(), api::ReportSubject::Group(GROUP.to_owned())),
            (
                a_plan(),
                api::ReportSubject::Plan(api::ReportedPlan {
                    group: GROUP.to_owned(),
                    id: PLAN.to_owned(),
                }),
            ),
        ];
        for (ours, theirs) in subjects {
            let form = report_form_for(ours.clone());
            assert_eq!(form.reasons.len(), theirs.reasons().len(), "{ours:?}");
            // Each sends the name the server's list has at the same place.
            for (choice, reason) in form.reasons.iter().zip(theirs.reasons()) {
                let text = if choice.reason == ReportReason::Other {
                    "why"
                } else {
                    ""
                };
                let request = report_request(ours.clone(), choice.reason, text.to_owned()).unwrap();
                let body: serde_json::Value =
                    serde_json::from_slice(&request.body.unwrap()).unwrap();
                assert_eq!(body["reason"], reason.name().unwrap(), "{ours:?}");
            }
        }
    }

    #[test]
    fn a_report_the_server_would_refuse_is_refused_before_it_is_sent() {
        let flight = || ReportSubject::Flight {
            id: "fbyw03atd9qdt5bfe177ra1k94".to_owned(),
        };
        assert_eq!(
            words(report_request(
                flight(),
                ReportReason::Other,
                " \n ".to_owned()
            )),
            "Say in a few words what is wrong."
        );
        assert_eq!(
            words(report_request(
                flight(),
                ReportReason::Spam,
                "x".repeat(1001)
            )),
            "A report has room for 1000 characters. Shorten it."
        );
        assert!(report_request(flight(), ReportReason::Spam, "x".repeat(1000)).is_ok());
        assert_eq!(
            words(report_request(
                ReportSubject::Pilot {
                    handle: " ".to_owned()
                },
                ReportReason::Spam,
                String::new()
            )),
            "There is nothing here to report."
        );
    }

    #[test]
    fn what_became_of_a_report_is_said_in_a_line() {
        assert_eq!(
            report_sent(201, br#"{"created":true}"#.to_vec()).unwrap(),
            "Sent. Whoever runs Venti will read it."
        );
        assert_eq!(
            report_sent(200, br#"{"created":false}"#.to_vec()).unwrap(),
            "You reported this before. Your report is still waiting to be read."
        );
        assert_eq!(
            words(report_sent(
                404,
                br#"{"error":{"code":"pilot_not_found","message":"x"}}"#.to_vec()
            )),
            "This pilot is no longer there."
        );
        assert_eq!(
            words(report_sent(
                404,
                br#"{"error":{"code":"live_track_not_found","message":"x"}}"#.to_vec()
            )),
            "This track is no longer there."
        );
        assert_eq!(
            words(report_sent(
                429,
                br#"{"error":{"code":"rate_limited","message":"x"}}"#.to_vec()
            )),
            "You have sent many reports today. Try again tomorrow."
        );
        assert_eq!(
            words(report_sent(
                401,
                br#"{"error":{"code":"unauthorized","message":"x"}}"#.to_vec()
            )),
            "This device is no longer signed in. Sign in again."
        );
    }

    #[test]
    fn a_group_or_a_plan_that_is_gone_and_a_server_that_knows_neither_are_said_in_words() {
        // The pilot left the group or was removed, or the group is gone:
        // the server does not say which, and neither do the words.
        assert_eq!(
            words(report_sent(
                404,
                br#"{"error":{"code":"group_not_found","message":"x"}}"#.to_vec()
            )),
            "This group is no longer there, or you are no longer in it."
        );
        assert_eq!(
            words(report_sent(
                404,
                br#"{"error":{"code":"plan_not_found","message":"x"}}"#.to_vec()
            )),
            "This plan is no longer there."
        );
        // A server from before groups could be reported cannot read the
        // subject, and says so as of any request it cannot read.
        assert_eq!(
            words(report_sent(
                400,
                br#"{"error":{"code":"invalid_request","message":"x"}}"#.to_vec()
            )),
            "This server cannot take this report. It may be older than the app."
        );
        // Stored, a report about either is answered like any other.
        assert_eq!(
            report_sent(201, br#"{"created":true}"#.to_vec()).unwrap(),
            "Sent. Whoever runs Venti will read it."
        );
    }

    #[test]
    fn the_form_lists_every_reason_the_server_knows_in_its_order() {
        let form = report_form();
        assert_eq!(form.text_at_most, 1000);
        assert!(form.notice.contains("sees that it came from you"));
        let listed: Vec<_> = form.reasons.iter().map(|choice| choice.reason).collect();
        assert_eq!(listed.len(), api::ReportReason::ALL.len());
        // Each sends the name the server's list has at the same place.
        for (reason, theirs) in listed.into_iter().zip(api::ReportReason::ALL) {
            let text = if reason == ReportReason::Other {
                "why"
            } else {
                ""
            };
            let request = report_request(
                ReportSubject::Pilot {
                    handle: "mara".to_owned(),
                },
                reason,
                text.to_owned(),
            )
            .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
            assert_eq!(body["reason"], theirs.name().unwrap());
        }
    }

    const SITE: &str = "venti:0000000000000000000000s001";

    #[test]
    fn a_site_of_the_crowdsourced_layer_is_reported_as_a_partners_is() {
        let request = report_request(
            ReportSubject::Site {
                id: "venti-pilots:0000000000000000000000s002".to_owned(),
            },
            ReportReason::NotASite,
            String::new(),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            body["subject"]["site"]["id"],
            "venti-pilots:0000000000000000000000s002"
        );
        // A source's site still is not: it is corrected at its source.
        assert!(
            report_request(
                ReportSubject::Site {
                    id: "osm-free-flying:way/1080986173".to_owned(),
                },
                ReportReason::NotASite,
                String::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn a_site_that_pilots_made_is_reported_with_the_reasons_of_a_site() {
        let site = || ReportSubject::Site {
            id: SITE.to_owned(),
        };
        let revision = ReportSubject::SiteRevision {
            id: SITE.to_owned(),
            revision: 2,
        };
        // The example of the server's documentation, word for word.
        let request = report_request(
            site(),
            ReportReason::WrongOrDangerous,
            "The take-off is on the other side of the ridge.".to_owned(),
        )
        .unwrap();
        assert_eq!(request.path, "/v1/reports");
        assert!(request.needs_token);
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(body, golden("report_site_request.json"));
        let request = report_request(revision.clone(), ReportReason::Offensive, String::new());
        let body: serde_json::Value =
            serde_json::from_slice(&request.unwrap().body.unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "subject": { "site": { "id": SITE, "revision": 2 } },
                "reason": "offensive"
            })
        );

        // The form: the two reasons of a site first, then those about a
        // text; nothing about an account or a flight.
        let form = report_form_for(site());
        assert_eq!(form, report_form_for(revision));
        let listed: Vec<_> = form.reasons.iter().map(|choice| choice.reason).collect();
        assert_eq!(
            listed,
            [
                ReportReason::WrongOrDangerous,
                ReportReason::NotASite,
                ReportReason::Offensive,
                ReportReason::Privacy,
                ReportReason::Spam,
                ReportReason::Other,
            ]
        );
        let titles: Vec<&str> = form.reasons.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Wrong place or dangerous",
                "Not a flying site",
                "Insulting or threatening",
                "Shows something private",
                "Advertising",
                "Something else",
            ]
        );
        assert_eq!(
            form.notice,
            "Whoever runs Venti reads the report, with the site and its history, and sees that it came from you. The pilots who added or changed the site are not told, and no answer comes back. If you know what is right, correcting the site yourself is quicker."
        );
        // Each sends the name the server's list has at the same place.
        let theirs = api::ReportSubject::Site(api::ReportedSite {
            id: SITE.to_owned(),
            revision: None,
        });
        assert_eq!(form.reasons.len(), theirs.reasons().len());
        for (choice, reason) in form.reasons.iter().zip(theirs.reasons()) {
            let text = if choice.reason == ReportReason::Other {
                "why"
            } else {
                ""
            };
            let request = report_request(site(), choice.reason, text.to_owned()).unwrap();
            let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
            assert_eq!(body["reason"], reason.name().unwrap());
        }

        // The reasons of a site are no reasons about anything else, and
        // the others' own are none of a site's.
        assert_eq!(
            words(report_request(
                a_group(),
                ReportReason::NotASite,
                String::new()
            )),
            "This reason is about a flying site. Choose another."
        );
        assert_eq!(
            words(report_request(
                site(),
                ReportReason::NotAFlight,
                String::new()
            )),
            "This reason is about a flight. Choose another."
        );
        assert_eq!(
            words(report_request(
                site(),
                ReportReason::Impersonation,
                String::new()
            )),
            "This reason does not fit what you report. Choose another."
        );
        // The forms that were there offer what they offered.
        assert_eq!(report_form().reasons.len(), 6);
        assert_eq!(report_form_for(a_group()).reasons.len(), 5);
        // A source's site, what is no identifier, and a revision before
        // the first name nothing to report.
        for nothing in [
            ReportSubject::Site {
                id: "osm-free-flying:node/5964676196".to_owned(),
            },
            ReportSubject::Site { id: String::new() },
            ReportSubject::SiteRevision {
                id: SITE.to_owned(),
                revision: 0,
            },
        ] {
            assert_eq!(
                words(report_request(nothing, ReportReason::Spam, String::new())),
                "There is nothing here to report."
            );
        }
        // A site that is gone by the time the report arrives.
        for (status, code) in [(404, "site_not_found"), (410, "site_gone")] {
            let body = format!(r#"{{"error":{{"code":"{code}","message":"x"}}}}"#);
            assert_eq!(
                words(report_sent(status, body.into_bytes())),
                "This site is no longer there."
            );
        }
    }
    #[test]
    fn a_comment_is_reported_with_the_reasons_of_words() {
        let form = comment_report_form();
        let listed: Vec<_> = form.reasons.iter().map(|choice| choice.reason).collect();
        assert_eq!(
            listed,
            [
                ReportReason::Offensive,
                ReportReason::Impersonation,
                ReportReason::Privacy,
                ReportReason::Spam,
                ReportReason::Other,
            ]
        );
        assert!(form.reasons[0].explanation.contains("The comment"));
        assert!(form.notice.contains("Its author is not told"));

        let flight = "5t1eshypdtvrq7rxw39a18vsrg";
        let comment = "0000000000000000000000c001";
        let request = comment_report_request(
            format!(" {flight} "),
            comment.to_owned(),
            ReportReason::Offensive,
            String::new(),
        )
        .unwrap();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("POST", "/v1/reports")
        );
        assert!(request.needs_token);
        let body: serde_json::Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "subject": { "comment": { "flight": flight, "id": comment } },
                "reason": "offensive"
            })
        );
        // What the server would refuse is refused before it is sent.
        let refused = |flight: &str, comment: &str, reason, text: &str| {
            comment_report_request(
                flight.to_owned(),
                comment.to_owned(),
                reason,
                text.to_owned(),
            )
            .unwrap_err()
            .to_string()
        };
        assert!(refused("nothing", comment, ReportReason::Spam, "").contains("nothing here"));
        assert!(refused(flight, "short", ReportReason::Spam, "").contains("nothing here"));
        assert!(refused(flight, comment, ReportReason::NotAFlight, "").contains("about a flight"));
        assert!(refused(flight, comment, ReportReason::Other, " ").contains("a few words"));
        let gone = br#"{"error":{"code":"comment_not_found","message":"x"}}"#;
        assert_eq!(
            report_sent(404, gone.to_vec()).unwrap_err().to_string(),
            "This comment is no longer there."
        );
    }
}
