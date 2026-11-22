//! A request for a pilot's agreement that a program may read for them, as
//! the web app reads it and answers it.
//!
//! A program (an assistant, above all) sends the pilot's browser to the
//! server's authorization endpoint (`docs/api.md`, "Connecting an
//! assistant to a pilot's logbook"). The server checks the request and
//! keeps it under an identifier nobody can guess. The web app, where the
//! pilot is signed in already, reads the request by that identifier, shows
//! the pilot what is asked, and answers it with the pilot's session.

use serde::{Deserialize, Serialize};
use venti_domain::Timestamp;

use super::token::Scope;

/// What every identifier of a request for agreement starts with, so that
/// one found in a log or a history can be recognised for what it is.
pub const AUTHORIZATION_REQUEST_PREFIX: &str = "venti_oar_";

/// Path under which a request for agreement is read and answered, by its
/// identifier.
pub const OAUTH_REQUESTS_PATH: &str = "/v1/oauth/requests";

/// Whether text has the form of a request's identifier: the prefix and 52
/// characters after it. Text of another form is the identifier of no
/// request and is put into no path.
pub fn is_authorization_request_id(text: &str) -> bool {
    text.strip_prefix(AUTHORIZATION_REQUEST_PREFIX)
        .is_some_and(|rest| {
            rest.len() == 52
                && rest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
        })
}

/// Path of one request for agreement. `id` is an identifier that
/// [`is_authorization_request_id`] took.
pub fn oauth_request_path(id: &str) -> String {
    format!("{OAUTH_REQUESTS_PATH}/{id}")
}

/// A request for a pilot's agreement, as the pilot must see it before
/// answering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationRequestResponse {
    /// What the program calls itself. It is the program's own claim:
    /// nobody checked who it is.
    pub program: String,
    /// The host the pilot's browser is sent back to with the answer.
    pub returns_to: String,
    /// Whether that address is a program on the device the browser runs
    /// on, which the server cannot tell from any other program there.
    pub on_this_device: bool,
    /// What is asked for, as scopes.
    pub scopes: Vec<Scope>,
    /// What the program may read once the pilot agrees, in the server's
    /// words: one sentence for each thing.
    pub may_read: Vec<String>,
    /// What it may not do, in the server's words.
    pub may_not: String,
    /// For how long the agreement holds, in the server's words.
    pub lasts: String,
    /// The moment from which the request can no longer be answered.
    pub expires_at: Timestamp,
}

/// The pilot's answer to a request for agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerAuthorizationRequest {
    /// `true` to agree, `false` to refuse.
    pub agree: bool,
}

/// Where the pilot's browser goes once the request is answered.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationAnswerResponse {
    /// The program's own address, as it registered it, with what OAuth
    /// adds: the code or the refusal, the program's `state`, and `iss`.
    /// With a code it is a secret for the two minutes the code lasts: the
    /// browser is sent there, and it is kept and logged nowhere.
    pub redirect_to: String,
}

impl std::fmt::Debug for AuthorizationAnswerResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizationAnswerResponse")
            .field("redirect_to", &"<hidden>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_is_told_by_its_form() {
        let id = format!("{AUTHORIZATION_REQUEST_PREFIX}{}", "0a".repeat(26));
        assert!(is_authorization_request_id(&id));
        assert_eq!(oauth_request_path(&id), format!("/v1/oauth/requests/{id}"));
        for bad in [
            String::new(),
            AUTHORIZATION_REQUEST_PREFIX.to_owned(),
            id[..id.len() - 1].to_owned(),
            format!("{id}0"),
            id.to_uppercase(),
            id.replace("venti_oar_", "venti_oac_"),
            format!("venti_oar_{}", "../sessions/".repeat(4) + "0000"),
            format!("venti_oar_{}", "%2".repeat(26)),
        ] {
            assert!(!is_authorization_request_id(&bad), "{bad:?}");
        }
    }

    #[test]
    fn an_answer_is_agree_or_refuse_and_nothing_else() {
        let agree: AnswerAuthorizationRequest = serde_json::from_str(r#"{"agree":true}"#).unwrap();
        assert!(agree.agree);
        for bad in ["{}", r#"{"agree":"yes"}"#, r#"{"agree":true,"scope":"x"}"#] {
            assert!(serde_json::from_str::<AnswerAuthorizationRequest>(bad).is_err());
        }
        let answer = AuthorizationAnswerResponse {
            redirect_to: "https://assistant.example/cb?code=venti_oac_x".to_owned(),
        };
        assert!(!format!("{answer:?}").contains("venti_oac_"));
    }
}
