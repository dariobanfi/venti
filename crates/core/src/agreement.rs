//! Letting a program read the pilot's flights, for the web app's page
//! `/connect/`: the page on which a pilot who is signed in to Venti in
//! their browser answers a program's request, without typing a password.
//!
//! A program (an assistant, above all) sends the pilot's browser to the
//! Venti server, which checks the request, keeps it under an identifier
//! nobody can guess, and links to `/connect/?request=<identifier>`
//! (`docs/api.md`, "Agreeing in the web app"). The page reads the request
//! by that identifier, shows what is asked, and answers with the pilot's
//! session; the server's answer is the address the browser goes to.
//!
//! The server has the rules and its own words for what may be read. Here
//! are the requests, the readers of the answers and every word the page
//! shows, so that the page writes none of its own. The core does no I/O:
//! it hands the shell a [`ServerRequest`] and reads the answer. The
//! identifier and the address the browser is sent to pass through the
//! core once and are kept nowhere.

use venti_api_types::v1::{
    AnswerAuthorizationRequest, AuthorizationAnswerResponse, AuthorizationRequestResponse,
    is_authorization_request_id, oauth_request_path,
};

use crate::account::json_request;
use crate::server::{code, refusal};
use crate::{CoreError, ServerRequest};

/// What the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum AgreementState {
    /// The request waits and a pilot is signed in: what is asked, and the
    /// two buttons.
    Asking,
    /// The request waits and nobody is signed in: what is asked, and the
    /// button that signs in, in place of the two.
    SignInFirst,
    /// The request has ended, or there never was one: the title and the
    /// message, and nothing to press.
    Ended,
    /// The request was answered already: the title and the message, and
    /// nothing to press.
    Answered,
    /// The server could not be asked or answered otherwise: the title and
    /// the message, and loading the page again.
    Failed,
}

/// One thing the page says about a request: a label and what stands
/// under it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AgreementRow {
    /// The label: "It may not".
    pub label: String,
    /// What stands under it.
    pub text: String,
}

/// The page for a request, in the state it is in, with every word of it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AgreementPage {
    /// What to show.
    pub state: AgreementState,
    /// The page's title: "Let this program read your flights?", or what
    /// became of the request.
    pub title: String,
    /// The sentence under the title. For a request that waits it names
    /// the program by the name the program gave itself, and says that the
    /// name is the program's own claim. For the other states it says what
    /// to do.
    pub message: String,
    /// The label of the list: "It may read". Empty unless the request
    /// waits, as is everything down to `withdraw`.
    pub may_read_label: String,
    /// What the program may read, in the server's words: a list.
    pub may_read: Vec<String>,
    /// What else the page says: what the program may not do, where the
    /// pilot is sent back to, and for how long the agreement holds.
    pub rows: Vec<AgreementRow>,
    /// A warning to set off from the rest, when the pilot is sent back to
    /// a program on their own device.
    pub warning: Option<String>,
    /// Who answers: "You answer as @anna." `None` when nobody is signed
    /// in.
    pub account: Option<String>,
    /// The line that the agreement can be withdrawn, and where.
    pub withdraw: String,
    /// The moment from which the request can no longer be answered, in
    /// seconds since 1970: at it the page shows [`agreement_ended`]. Zero
    /// unless the request waits.
    pub ends_unix_s: i64,
}

/// The words of the page that do not depend on the request.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AgreementWords {
    /// What the page says while it reads the request: "Loading…".
    pub loading: String,
    /// The button that agrees: "Allow".
    pub allow: String,
    /// The button that refuses: "Don't allow".
    pub refuse: String,
    /// What a button says while its answer is on the way: "One moment…".
    pub answering: String,
    /// The button of [`AgreementState::SignInFirst`]: "Sign in". It leads
    /// to the app's own sign-in and back to this page.
    pub sign_in: String,
    /// The button of [`AgreementState::Failed`]: "Try again".
    pub retry: String,
}

/// What became of the pilot's answer.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AgreementOutcome {
    /// Where the browser goes now: the program's own address with the
    /// answer. Navigate to it at once, in place of this page, and keep it
    /// nowhere: with a code in it, it is a secret. `None` when the answer
    /// was not taken.
    pub redirect_to: Option<String>,
    /// What the page shows when the answer was not taken.
    /// [`AgreementState::Asking`] means the request still waits: the page
    /// stays as it is, shows `message`, and the pilot answers again.
    pub state: AgreementState,
    /// The title for a state that replaces the page's
    /// ([`AgreementState::Ended`], [`AgreementState::Answered`]). Empty
    /// otherwise.
    pub title: String,
    /// What to say: why the answer was not taken, in words for the pilot.
    /// Empty when it was.
    pub message: String,
}

impl std::fmt::Debug for AgreementOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgreementOutcome")
            .field(
                "redirect_to",
                &self.redirect_to.as_ref().map(|_| "<hidden>"),
            )
            .field("state", &self.state)
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

const TITLE: &str = "Let this program read your flights?";
const ENDED_TITLE: &str = "This request has ended";
const ENDED: &str = "A request to let a program read your flights is good for a quarter of an \
                     hour. This one is over, or the link to it is not complete. Go back to the \
                     program and start again.";
const ANSWERED_TITLE: &str = "Already answered";
const ANSWERED: &str = "This request was answered already, and a request is answered once. If \
                        the program is still waiting, start again there.";
const SIGN_IN_FIRST: &str = "Sign in to Venti to answer. You come back to this page.";
const FAILED_TITLE: &str = "Venti could not be asked";

fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

/// The words of the page that do not depend on the request.
#[uniffi::export]
pub fn agreement_words() -> AgreementWords {
    AgreementWords {
        loading: "Loading\u{2026}".to_owned(),
        allow: "Allow".to_owned(),
        refuse: "Don't allow".to_owned(),
        answering: "One moment\u{2026}".to_owned(),
        sign_in: "Sign in".to_owned(),
        retry: "Try again".to_owned(),
    }
}

/// A page that shows only a title and a message.
fn closed(state: AgreementState, title: &str, message: String) -> AgreementPage {
    AgreementPage {
        state,
        title: title.to_owned(),
        message,
        may_read_label: String::new(),
        may_read: Vec::new(),
        rows: Vec::new(),
        warning: None,
        account: None,
        withdraw: String::new(),
        ends_unix_s: 0,
    }
}

/// The page for a request that has ended: for a link without an
/// identifier or with one that is none, without asking the server, and
/// for the moment a page that was open reaches the request's end.
#[uniffi::export]
pub fn agreement_ended() -> AgreementPage {
    closed(AgreementState::Ended, ENDED_TITLE, ENDED.to_owned())
}

/// The request that reads a request for agreement, by the identifier in
/// the page's address (`?request=`). It needs no token. An error for text
/// that is no identifier: show [`agreement_ended`] then, and send nothing.
#[uniffi::export]
pub fn agreement_request(id: String) -> Result<ServerRequest, CoreError> {
    if !is_authorization_request_id(&id) {
        return Err(invalid(ENDED));
    }
    Ok(ServerRequest {
        method: "GET".to_owned(),
        path: oauth_request_path(&id),
        body: None,
        content_type: None,
        needs_token: false,
    })
}

/// Reads the server's answer to [`agreement_request`] into the page.
/// `handle` is the handle of the account this browser is signed in to,
/// `None` when it is signed in to none: the page then asks to sign in
/// first. It never fails: what went wrong is a page too.
#[uniffi::export]
pub fn agreement_page(status: u16, body: Vec<u8>, handle: Option<String>) -> AgreementPage {
    let asked: AuthorizationRequestResponse = match (status, code(&body).as_deref()) {
        (200, _) => match serde_json::from_slice(&body) {
            Ok(asked) => asked,
            Err(_) => {
                return closed(
                    AgreementState::Failed,
                    FAILED_TITLE,
                    "The server's answer could not be read. The app may be older than the \
                     server."
                        .to_owned(),
                );
            }
        },
        (_, Some("authorization_request_not_found")) => return agreement_ended(),
        (_, Some("authorization_request_answered")) => {
            return closed(
                AgreementState::Answered,
                ANSWERED_TITLE,
                ANSWERED.to_owned(),
            );
        }
        _ => {
            let CoreError::Server { message } = refusal(status, &body) else {
                return closed(AgreementState::Failed, FAILED_TITLE, String::new());
            };
            return closed(AgreementState::Failed, FAILED_TITLE, message);
        }
    };

    let handle = handle.filter(|handle| !handle.trim().is_empty());
    let mut message = format!(
        "A program that calls itself \u{201c}{}\u{201d} asks to read your Venti logbook. The \
         name is the program's own claim: Venti has not checked who it is.",
        asked.program.trim()
    );
    if handle.is_none() {
        message.push(' ');
        message.push_str(SIGN_IN_FIRST);
    }
    AgreementPage {
        state: if handle.is_some() {
            AgreementState::Asking
        } else {
            AgreementState::SignInFirst
        },
        title: TITLE.to_owned(),
        message,
        may_read_label: "It may read".to_owned(),
        may_read: asked.may_read,
        rows: vec![
            AgreementRow {
                label: "It may not".to_owned(),
                text: asked.may_not,
            },
            AgreementRow {
                label: "You are sent back to".to_owned(),
                text: asked.returns_to,
            },
            AgreementRow {
                label: "For how long".to_owned(),
                text: asked.lasts,
            },
        ],
        warning: asked.on_this_device.then(|| {
            "The address you are sent back to is a program on the device this browser runs \
             on, and Venti cannot tell which. Allow it only if you started this yourself a \
             moment ago."
                .to_owned()
        }),
        account: handle.map(|handle| format!("You answer as @{}.", handle.trim())),
        withdraw: "You can withdraw this at any time in Venti: under your account, in \
                   Programs with access."
            .to_owned(),
        ends_unix_s: asked.expires_at.timestamp(),
    }
}

/// The request that answers a request for agreement: `agree` is `true`
/// for "Allow" and `false` for "Don't allow". It goes with the session's
/// token and with nothing else: a token for a program cannot answer.
#[uniffi::export]
pub fn agreement_answer_request(id: String, agree: bool) -> Result<ServerRequest, CoreError> {
    if !is_authorization_request_id(&id) {
        return Err(invalid(ENDED));
    }
    json_request(
        "POST",
        &oauth_request_path(&id),
        &AnswerAuthorizationRequest { agree },
        true,
    )
}

/// Whether an address is one a browser may be sent to after an answer:
/// `https://`, or `http://` as a program on the pilot's own device has
/// it. Anything else a server sends is not navigated to.
fn is_a_way_back(address: &str) -> bool {
    let plain = address
        .chars()
        .all(|c| c.is_ascii_graphic() && !matches!(c, '"' | '<' | '>' | '\\' | '`'));
    let local = ["http://127.0.0.1", "http://[::1]", "http://localhost"]
        .iter()
        .any(|start| {
            address
                .strip_prefix(start)
                .is_some_and(|rest| rest.starts_with([':', '/', '?']))
        });
    plain && (address.starts_with("https://") || local)
}

/// Reads the server's answer to [`agreement_answer_request`]: where the
/// browser goes, or what the page shows in its place. It never fails.
#[uniffi::export]
pub fn agreement_answered(status: u16, body: Vec<u8>) -> AgreementOutcome {
    let stays = |state: AgreementState, title: &str, message: String| AgreementOutcome {
        redirect_to: None,
        state,
        title: title.to_owned(),
        message,
    };
    match (status, code(&body).as_deref()) {
        (200, _) => match serde_json::from_slice::<AuthorizationAnswerResponse>(&body) {
            Ok(answer) if is_a_way_back(&answer.redirect_to) => AgreementOutcome {
                redirect_to: Some(answer.redirect_to),
                state: AgreementState::Answered,
                title: String::new(),
                message: String::new(),
            },
            // The request is used up, and there is nowhere to go.
            _ => stays(
                AgreementState::Failed,
                FAILED_TITLE,
                "The server's answer could not be read. Go back to the program and start \
                 again."
                    .to_owned(),
            ),
        },
        (_, Some("authorization_request_not_found")) => {
            stays(AgreementState::Ended, ENDED_TITLE, ENDED.to_owned())
        }
        (_, Some("authorization_request_answered")) => stays(
            AgreementState::Answered,
            ANSWERED_TITLE,
            ANSWERED.to_owned(),
        ),
        (_, Some("unauthorized")) => stays(
            AgreementState::SignInFirst,
            "",
            "This browser is no longer signed in to Venti. Sign in again, then answer.".to_owned(),
        ),
        // The request still waits: the pilot sets it right and answers
        // again.
        (_, Some("publishing_suspended")) => stays(
            AgreementState::Asking,
            "",
            "Whoever runs Venti has stopped this account from sharing, and it can give no \
             program access. You can still choose Don't allow."
                .to_owned(),
        ),
        _ => {
            let message = match refusal(status, &body) {
                CoreError::Server { message } => message,
                _ => String::new(),
            };
            stays(AgreementState::Asking, "", message)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The server's own recorded answers, which its tests keep current.
    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    fn id() -> String {
        format!("venti_oar_{}", "0".repeat(52))
    }

    fn error(code: &str) -> Vec<u8> {
        json!({"error": {"code": code, "message": "for developers"}})
            .to_string()
            .into_bytes()
    }

    #[test]
    fn the_requests_are_the_servers_and_take_only_an_identifier() {
        let read = agreement_request(id()).unwrap();
        assert_eq!(
            (read.method.as_str(), read.path.as_str(), read.needs_token),
            (
                "GET",
                format!("/v1/oauth/requests/{}", id()).as_str(),
                false
            )
        );
        assert_eq!(read.body, None);

        let agree = agreement_answer_request(id(), true).unwrap();
        assert_eq!(
            (
                agree.method.as_str(),
                agree.path.as_str(),
                agree.needs_token
            ),
            (
                "POST",
                format!("/v1/oauth/requests/{}", id()).as_str(),
                true
            )
        );
        assert_eq!(agree.content_type.as_deref(), Some("application/json"));
        let sent: serde_json::Value = serde_json::from_slice(&agree.body.unwrap()).unwrap();
        let example: serde_json::Value =
            serde_json::from_slice(&golden("oauth_request_answer.json")).unwrap();
        assert_eq!(sent, example);
        let refuse = agreement_answer_request(id(), false).unwrap();
        assert_eq!(refuse.body.unwrap(), br#"{"agree":false}"#);

        // What a link can be made to carry is put into no path.
        for bad in [
            "",
            "venti_oar_",
            "../account",
            "venti_oar_0000000000000000000000000000000000000000000000/../x",
            "venti_oac_0000000000000000000000000000000000000000000000000000",
        ] {
            assert!(agreement_request(bad.to_owned()).is_err(), "{bad}");
            assert!(agreement_answer_request(bad.to_owned(), true).is_err());
        }
    }

    #[test]
    fn a_waiting_request_is_a_page_with_every_word() {
        let page = agreement_page(200, golden("oauth_request.json"), Some("anna".to_owned()));
        assert_eq!(page.state, AgreementState::Asking);
        assert_eq!(page.title, "Let this program read your flights?");
        assert_eq!(
            page.message,
            "A program that calls itself \u{201c}Assistant\u{201d} asks to read your Venti \
             logbook. The name is the program's own claim: Venti has not checked who it is."
        );
        assert_eq!(page.may_read_label, "It may read");
        assert_eq!(page.may_read.len(), 1);
        assert!(page.may_read[0].starts_with("Your flights, the private ones"));
        let labels: Vec<&str> = page.rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(
            labels,
            ["It may not", "You are sent back to", "For how long"]
        );
        assert_eq!(page.rows[1].text, "assistant.example");
        assert!(page.rows[2].text.contains("180 days"));
        assert_eq!(page.warning, None);
        assert_eq!(page.account.as_deref(), Some("You answer as @anna."));
        assert!(page.withdraw.contains("Programs with access"));
        // 2026-11-18 19:20 UTC, as the server's example has it.
        assert_eq!(page.ends_unix_s, 1_795_029_600);

        let words = agreement_words();
        assert_eq!(
            (words.allow.as_str(), words.refuse.as_str()),
            ("Allow", "Don't allow")
        );
        assert_eq!(words.sign_in, "Sign in");
    }

    #[test]
    fn nobody_signed_in_is_asked_to_sign_in_first() {
        for nobody in [None, Some("  ".to_owned())] {
            let page = agreement_page(200, golden("oauth_request.json"), nobody);
            assert_eq!(page.state, AgreementState::SignInFirst);
            assert!(page.message.contains("\u{201c}Assistant\u{201d}"));
            assert!(
                page.message
                    .ends_with("Sign in to Venti to answer. You come back to this page.")
            );
            assert_eq!(page.account, None);
            // What is asked is shown all the same.
            assert_eq!(page.rows.len(), 3);
        }
    }

    #[test]
    fn a_program_on_the_pilots_device_comes_with_a_warning() {
        let mut asked: serde_json::Value =
            serde_json::from_slice(&golden("oauth_request.json")).unwrap();
        asked["on_this_device"] = json!(true);
        asked["returns_to"] = json!("127.0.0.1");
        let page = agreement_page(200, asked.to_string().into_bytes(), Some("anna".to_owned()));
        assert!(
            page.warning
                .unwrap()
                .contains("only if you started this yourself")
        );
    }

    #[test]
    fn what_became_of_a_request_is_a_page_too() {
        let ended = agreement_page(404, error("authorization_request_not_found"), None);
        assert_eq!(ended, agreement_ended());
        assert_eq!(ended.state, AgreementState::Ended);
        assert_eq!(ended.title, "This request has ended");
        assert!(ended.message.contains("start again"));
        assert!(ended.rows.is_empty() && ended.may_read.is_empty());

        let answered = agreement_page(410, error("authorization_request_answered"), None);
        assert_eq!(answered.state, AgreementState::Answered);
        assert_eq!(answered.title, "Already answered");

        let busy = agreement_page(503, Vec::new(), Some("anna".to_owned()));
        assert_eq!(busy.state, AgreementState::Failed);
        assert!(busy.message.contains("busy"));
        let limited = agreement_page(429, error("rate_limited"), None);
        assert_eq!(limited.state, AgreementState::Failed);
        assert!(limited.message.starts_with("Too many attempts"));
        let unread = agreement_page(200, b"{}".to_vec(), None);
        assert_eq!(unread.state, AgreementState::Failed);
    }

    #[test]
    fn an_answer_that_was_taken_says_where_the_browser_goes() {
        let taken = agreement_answered(200, golden("oauth_request_answered.json"));
        let to = taken.redirect_to.clone().unwrap();
        assert!(to.starts_with("https://assistant.example/oauth/callback?code=venti_oac_"));
        assert!(taken.message.is_empty());
        // The address is not in what a log would print.
        assert!(!format!("{taken:?}").contains("venti_oac_"));

        // A program on the pilot's own device.
        let local = json!({"redirect_to": "http://127.0.0.1:51234/callback?code=x"});
        let taken = agreement_answered(200, local.to_string().into_bytes());
        assert!(taken.redirect_to.is_some());

        // What is no address of a program is not navigated to.
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "http://evil.example/cb?code=x",
            "http://127.0.0.1.evil.example/cb",
            "http://localhost.evil.example/cb",
            "https://assistant.example/cb?code=x y",
            "/connect/",
            "",
        ] {
            let answer = json!({"redirect_to": bad}).to_string().into_bytes();
            let outcome = agreement_answered(200, answer);
            assert_eq!(outcome.redirect_to, None, "{bad}");
            assert_eq!(outcome.state, AgreementState::Failed);
        }
    }

    #[test]
    fn an_answer_that_was_not_taken_says_why_in_words() {
        let of = |status: u16, code: &str| agreement_answered(status, error(code));
        let ended = of(404, "authorization_request_not_found");
        assert_eq!(
            (ended.state, ended.title.as_str()),
            (AgreementState::Ended, "This request has ended")
        );
        let answered = of(410, "authorization_request_answered");
        assert_eq!(
            (answered.state, answered.title.as_str()),
            (AgreementState::Answered, "Already answered")
        );
        let signed_out = of(401, "unauthorized");
        assert_eq!(signed_out.state, AgreementState::SignInFirst);
        assert!(signed_out.message.contains("Sign in again"));

        // The request still waits for these.
        let full = of(409, "too_many_tokens");
        assert_eq!(full.state, AgreementState::Asking);
        assert_eq!(
            full.message,
            "Your account has as many programs with access as it can. Withdraw one first."
        );
        let suspended = of(403, "publishing_suspended");
        assert_eq!(suspended.state, AgreementState::Asking);
        assert!(suspended.message.contains("Don't allow"));
        let busy = agreement_answered(503, Vec::new());
        assert_eq!(busy.state, AgreementState::Asking);
        assert!(busy.message.contains("busy"));
        for outcome in [ended, answered, signed_out, full, suspended, busy] {
            assert_eq!(outcome.redirect_to, None);
        }
    }
}
