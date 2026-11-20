//! Reporting: how a pilot tells whoever runs the server that a flight, a
//! pilot's page or a live track should not be there.
//!
//! A report goes to the operator and to nobody else. The server does
//! nothing by itself when one arrives, the pilot who is reported is not
//! told, and nothing comes back to the one who reported. The operator
//! sees who sent it, which the form has to say.

use venti_api_types::v1::{
    self as api, MAX_REPORT_TEXT_LENGTH, ReportRequest, ReportResponse, parse_report_text,
};

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

/// The reasons a report can give and what the pilot is told before
/// sending one.
#[uniffi::export]
pub fn report_form() -> ReportForm {
    let choice = |reason, title: &str, explanation: &str| ReportReasonChoice {
        reason,
        title: title.to_owned(),
        explanation: explanation.to_owned(),
    };
    ReportForm {
        reasons: vec![
            choice(
                ReportReason::Offensive,
                "Insulting or threatening",
                "A name, a glider's name or another text that insults, threatens or harasses.",
            ),
            choice(
                ReportReason::Impersonation,
                "Pretends to be someone else",
                "The account passes itself off as another person or as an organisation.",
            ),
            choice(
                ReportReason::Privacy,
                "Shows something private",
                "Something about a person who did not agree to it: someone else's flight, a name, where someone lives.",
            ),
            choice(
                ReportReason::NotAFlight,
                "Not a real flight",
                "A track that is made up or altered, or that this pilot did not fly.",
            ),
            choice(
                ReportReason::Spam,
                "Advertising",
                "Advertising, or something that has nothing to do with flying.",
            ),
            choice(
                ReportReason::Other,
                "Something else",
                "Say what in your own words.",
            ),
        ],
        text_at_most: u32::try_from(MAX_REPORT_TEXT_LENGTH).unwrap_or(u32::MAX),
        notice: "Whoever runs Venti reads the report and sees that it came from you. The pilot you report is not told, and no answer comes back."
            .to_owned(),
    }
}

/// The request that sends a report. `text` is what the pilot wrote, and
/// may be empty except with [`ReportReason::Other`].
///
/// # Errors
///
/// When the text is too long, or missing where it is needed, or the
/// subject has no address.
#[uniffi::export]
pub fn report_request(
    subject: ReportSubject,
    reason: ReportReason,
    text: String,
) -> Result<ServerRequest, CoreError> {
    let text = parse_report_text(&text).map_err(|_| CoreError::Server {
        message: format!("A report has room for {MAX_REPORT_TEXT_LENGTH} characters. Shorten it."),
    })?;
    if reason == ReportReason::Other && text.is_none() {
        return Err(CoreError::Server {
            message: "Say in a few words what is wrong.".to_owned(),
        });
    }
    let subject = match subject {
        ReportSubject::Flight { id } => api::ReportSubject::Flight(id),
        ReportSubject::Pilot { handle } => api::ReportSubject::Pilot(handle),
        ReportSubject::LiveTrack { id } => api::ReportSubject::LiveTrack(id),
    };
    let named = match &subject {
        api::ReportSubject::Flight(name)
        | api::ReportSubject::Pilot(name)
        | api::ReportSubject::LiveTrack(name) => name,
    };
    if named.trim().is_empty() {
        return Err(CoreError::Server {
            message: "There is nothing here to report.".to_owned(),
        });
    }
    json_request(
        "POST",
        api::REPORTS_PATH,
        &ReportRequest {
            subject,
            reason: match reason {
                ReportReason::Offensive => api::ReportReason::Offensive,
                ReportReason::Impersonation => api::ReportReason::Impersonation,
                ReportReason::Privacy => api::ReportReason::Privacy,
                ReportReason::NotAFlight => api::ReportReason::NotAFlight,
                ReportReason::Spam => api::ReportReason::Spam,
                ReportReason::Other => api::ReportReason::Other,
            },
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
    // pilot's: say so plainly. So for a day's worth of reports.
    let message = match (status, code(&body).as_deref()) {
        (_, Some("flight_not_found")) => Some("This flight is no longer there."),
        (_, Some("pilot_not_found")) => Some("This pilot is no longer there."),
        (_, Some("live_track_not_found")) => Some("This track is no longer there."),
        (429, _) => Some("You have sent many reports today. Try again tomorrow."),
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
}
