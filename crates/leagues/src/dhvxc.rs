//! The DHV-XC: the upload interface it publishes for apps.
//!
//! The "DHV XC Flight Upload Interface Specification", version 2022-04-24,
//! is an annex of the contest's rules ("Wettbewerbsausschreibung
//! 2026/2027", version 01.2027, read on 2026-11-20). One request carries
//! the pilot's user name, an upload password the pilot sets in their
//! DHV-XC profile (not the password they sign in with), and the file. No
//! key is asked of the app.
//!
//! An app sends the request from the pilot's own device and keeps the
//! upload password there. This module says what to send and reads what
//! came back.

use serde::{Deserialize, Serialize};

/// Where a flight is sent. This host answers in English, as the app
/// speaks; `de.dhv-xc.de` is the same interface in German.
pub const UPLOAD_URL: &str = "https://en.dhv-xc.de/api/v1/flights";

/// The content type of [`body`]. The interface takes JSON only when the
/// request says so in this header.
pub const CONTENT_TYPE: &str = "application/json";

/// Where a pilot sets the upload password.
pub const PROFILE_URL: &str = "https://en.dhv-xc.de/myprofile#/tab/user";

/// The contest's rules, with the interface's specification as an annex.
pub const RULES_URL: &str =
    "https://www.dhv.de/media/seiten/05_wettbewerb/DHVXC/Ausschreibungen/AusschreibungDHVXC.pdf";

/// One flight to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload<'a> {
    /// The pilot's DHV-XC user name, DHV user name or e-mail address:
    /// whatever they sign in with in a browser.
    pub user: &'a str,
    /// The upload password from the pilot's profile.
    pub upload_password: &'a str,
    /// Text that matches one of the gliders the pilot has set up at the
    /// DHV-XC. Without one, or without a match, the flight is not
    /// published and waits among the pilot's uploads.
    pub glider: Option<&'a str>,
    /// The file's name.
    pub file_name: &'a str,
    /// The file's text, as the recorder wrote it.
    pub igc_text: &'a str,
    /// Whether the DHV-XC is to publish the flight at once, if it can.
    /// Otherwise it stays among the pilot's uploads, to be looked at and
    /// published there.
    pub publish: bool,
}

/// The request's parameters, by the names the specification gives them.
#[derive(Serialize)]
struct Parameters<'a> {
    user: &'a str,
    pass: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    glider: Option<&'a str>,
    igcname: &'a str,
    publish: u8,
    igccontent: &'a str,
}

/// The body to `POST` to [`UPLOAD_URL`] with [`CONTENT_TYPE`].
pub fn body(upload: &Upload<'_>) -> Vec<u8> {
    let parameters = Parameters {
        user: upload.user,
        pass: upload.upload_password,
        glider: upload.glider.filter(|glider| !glider.trim().is_empty()),
        igcname: upload.file_name,
        publish: u8::from(upload.publish),
        igccontent: upload.igc_text,
    };
    // Text and small numbers always serialise.
    serde_json::to_vec(&parameters).unwrap_or_default()
}

/// What the DHV-XC said to an upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The DHV-XC has the flight. Its message says whether it was
    /// published or waits among the pilot's uploads: both are answered
    /// alike.
    Taken {
        /// The DHV-XC's own words.
        message: String,
    },
    /// The user name or the upload password was not accepted.
    NotSignedIn {
        /// The DHV-XC's own words.
        message: String,
    },
    /// Anything else: the DHV-XC could not take the flight.
    Failed {
        /// The status of the answer.
        status: u16,
        /// The DHV-XC's own words, if it sent any.
        message: String,
    },
}

/// The answer's body: "always application/json", by the specification.
#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: String,
}

/// The longest message of the DHV-XC that is handed on, in characters.
const MAX_MESSAGE: usize = 300;

/// Reads the answer to an upload.
///
/// By the specification the DHV-XC answers 200 with `success: true` once
/// it has the flight, published or not, 401 when the user name or the
/// upload password is wrong, and 500 for anything else. An answer that
/// says 200 and is not what the specification describes is not taken for
/// a success.
pub fn read_answer(status: u16, body: &[u8]) -> Outcome {
    let answer: Option<Answer> = serde_json::from_slice(body).ok();
    let message = answer
        .as_ref()
        .map(|answer| one_line(&answer.message))
        .unwrap_or_default();
    match (status, answer) {
        (200, Some(answer)) if answer.success => Outcome::Taken { message },
        (401, _) => Outcome::NotSignedIn { message },
        _ => Outcome::Failed { status, message },
    }
}

/// Another server's words on one bounded line.
fn one_line(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_MESSAGE)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    fn upload() -> Upload<'static> {
        Upload {
            user: "mara.muster",
            upload_password: "made-up-upload-password",
            glider: Some("Atom 3"),
            file_name: "2026-07-04-XCT-4b2-01.igc",
            igc_text: "AXCT4b2\r\nHFDTE040726\r\nG5a1b2c\r\n",
            publish: true,
        }
    }

    #[test]
    fn a_request_names_its_parameters_as_the_specification_does() {
        let sent: Value = serde_json::from_slice(&body(&upload())).unwrap();
        assert_eq!(
            sent,
            json!({
                "user": "mara.muster",
                "pass": "made-up-upload-password",
                "glider": "Atom 3",
                "igcname": "2026-07-04-XCT-4b2-01.igc",
                "publish": 1,
                "igccontent": "AXCT4b2\r\nHFDTE040726\r\nG5a1b2c\r\n",
            })
        );
        // The same as the request this crate keeps as its example.
        let kept: Value = serde_json::from_slice(&fixture("upload_request.json")).unwrap();
        assert_eq!(sent, kept);

        // Not to be published yet, and no glider named: the parameter is
        // left out, not sent empty.
        for glider in [None, Some(""), Some("  ")] {
            let quiet = Upload {
                glider,
                publish: false,
                ..upload()
            };
            let sent: Value = serde_json::from_slice(&body(&quiet)).unwrap();
            assert_eq!(sent["publish"], 0);
            assert!(sent.get("glider").is_none(), "{glider:?}");
        }
        // The file goes as it is, line ends and all.
        assert_eq!(UPLOAD_URL, "https://en.dhv-xc.de/api/v1/flights");
        assert_eq!(CONTENT_TYPE, "application/json");
    }

    #[test]
    fn the_answers_of_the_specification_are_read() {
        assert_eq!(
            read_answer(200, &fixture("uploaded.json")),
            Outcome::Taken {
                message: "The flight has been published under ID 1506695".to_owned()
            }
        );
        assert_eq!(
            read_answer(401, &fixture("not_signed_in.json")),
            Outcome::NotSignedIn {
                message: "Invalid username or upload password".to_owned()
            }
        );
        // "Other errors lead to false and https status 500."
        assert_eq!(
            read_answer(500, br#"{"success": false, "message": "No igc content"}"#),
            Outcome::Failed {
                status: 500,
                message: "No igc content".to_owned()
            }
        );
    }

    #[test]
    fn an_answer_the_specification_does_not_describe_is_no_success() {
        // 200 and no word of success: a page of something in between.
        for body in [&b"<html>Dein Flug wurde eingereicht</html>"[..], b"", b"{}"] {
            assert!(
                matches!(read_answer(200, body), Outcome::Failed { status: 200, .. }),
                "{body:?}"
            );
        }
        assert_eq!(
            read_answer(200, br#"{"success": false, "message": "no"}"#),
            Outcome::Failed {
                status: 200,
                message: "no".to_owned()
            }
        );
        // A wrong password without a body is still a wrong password.
        assert_eq!(
            read_answer(401, b""),
            Outcome::NotSignedIn {
                message: String::new()
            }
        );
        assert!(matches!(
            read_answer(502, b"Bad Gateway"),
            Outcome::Failed { status: 502, .. }
        ));
        // Another server's words reach a screen on one bounded line.
        let long = format!(
            r#"{{"success": true, "message": "line one\nline two\t{}"}}"#,
            "x".repeat(1000)
        );
        let Outcome::Taken { message } = read_answer(200, long.as_bytes()) else {
            panic!("the flight was taken")
        };
        assert!(message.starts_with("line one line two x"));
        assert_eq!(message.chars().count(), MAX_MESSAGE);
    }
}
