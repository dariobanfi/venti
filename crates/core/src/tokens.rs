//! The tokens a pilot gives to other programs, for a screen "Programs
//! with access" under the account.
//!
//! A pilot lets a logger, a script or another maker's app act for them on
//! Venti without telling it their password: they make a token that may do
//! what they chose and nothing else, copy its secret into the program
//! once, and withdraw it whenever they like. The server has the rules
//! (`docs/api.md`, "Tokens for other programs"); here are the words, the
//! requests and the readers of the answers, the same on every platform.
//!
//! The core does no I/O here either: it hands the shell a
//! [`ServerRequest`] and reads the answer. Every request here goes with
//! the session's token; a token for a program can do none of it. The
//! secret of a new token passes through the core once, on its way to the
//! screen, and is stored nowhere.

use chrono::DateTime;
use venti_api_types::v1::{
    self as api, AccessToken, AccessTokensResponse, CreateAccessTokenRequest,
    CreateAccessTokenResponse, MAX_TOKEN_NAME_LENGTH, Password, Scope, TokenNameError,
    parse_token_name,
};
use venti_domain::Timestamp;

use crate::account::json_request;
use crate::server::{answer, refusal};
use crate::{CoreError, ServerRequest};

/// One thing a program can be allowed, in words for a pilot.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TokenScope {
    /// The scope's name as the server knows it, `flights:read`: what
    /// [`token_create_request`] takes.
    pub scope: String,
    /// What it is, in a few words: "Flights: read".
    pub title: String,
    /// What it opens, in one line that goes on from the title: "your
    /// logbook, the private flights too".
    pub line: String,
}

/// The words of the form that makes a token.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TokenForm {
    /// The screen's title: "Programs with access".
    pub title: String,
    /// What a token is, for the head of the screen.
    pub intro: String,
    /// The label of the name: "Name of the program".
    pub name_label: String,
    /// What to put there: "My logger".
    pub name_hint: String,
    /// The most characters the name has.
    pub name_max_length: u32,
    /// The heading of the scopes: "What it may do".
    pub scopes_title: String,
    /// The scopes to choose from, in the order to list them.
    pub scopes: Vec<TokenScope>,
    /// The label of the end date: "Ends on".
    pub ends_label: String,
    /// What leaving it empty means: "Leave empty for access that does not
    /// end until you withdraw it."
    pub ends_hint: String,
    /// The label of the password: "Your password".
    pub password_label: String,
    /// Why it is asked for.
    pub password_reason: String,
    /// The button: "Give access".
    pub submit: String,
    /// What the list says when it is empty.
    pub empty: String,
    /// What no token can do, whatever it is allowed.
    pub never: String,
}

/// A token as its pilot sees it in the list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TokenView {
    /// The token's identifier, for withdrawing it. It is not the secret.
    pub id: String,
    /// What the pilot called the program.
    pub name: String,
    /// What it may do, each as a scope's title: "Flights: read".
    pub scopes: Vec<String>,
    /// The same in one line: "Flights: read · Live tracking: send".
    pub scopes_line: String,
    /// The day it was made, UTC: "21 Nov 2026". Shown after "Made".
    pub made: String,
    /// When it was last used, to the day: "on 21 Nov 2026", or "never".
    /// Shown after "Last used".
    pub last_used: String,
    /// When it ends: "on 21 Dec 2026", or "never". Shown after "Ends";
    /// after "Ended" when `has_ended`.
    pub ends: String,
    /// Whether its end has come: it opens nothing any more, and stays in
    /// the list until the pilot withdraws it.
    pub has_ended: bool,
}

/// A token that was just made, with its secret.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TokenCreated {
    /// The token as the list shows it from now on.
    pub token: TokenView,
    /// The secret the program is given. Show it, let the pilot copy it,
    /// and keep it nowhere: not in a log, not in the app's storage.
    pub secret: String,
    /// What to say beside it: "Copy it now: Venti does not show it again."
    pub shown_once: String,
    /// What the pilot does with it.
    pub how_to_use: String,
    /// The button that copies it: "Copy".
    pub copy: String,
    /// The button that leaves the screen: "I have copied it".
    pub done: String,
}

impl std::fmt::Debug for TokenCreated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenCreated")
            .field("token", &self.token)
            .field("secret", &"<hidden>")
            .finish_non_exhaustive()
    }
}

/// The words that ask before a token is withdrawn.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TokenWithdrawal {
    /// The question: "Withdraw the access of “My logger”?".
    pub title: String,
    /// What follows from it.
    pub message: String,
    /// The button that does it: "Withdraw access".
    pub confirm: String,
    /// The button that does not: "Keep access".
    pub cancel: String,
}

/// A scope's words: its title, and the line of what it opens.
fn words(scope: Scope) -> Option<(&'static str, &'static str)> {
    Some(match scope {
        Scope::AccountRead => (
            "Account: read",
            "your handle, your name and since when you are on Venti",
        ),
        Scope::FlightsRead => ("Flights: read", "your logbook, the private flights too"),
        Scope::FlightsWrite => (
            "Flights: write",
            "upload flights, change who may see them, delete one",
        ),
        Scope::LiveWrite => (
            "Live tracking: send",
            "send your position while you fly, and end a live track",
        ),
        Scope::SitesWrite => (
            "Flying sites: write",
            "add, correct and remove take-offs and landings on Venti, under your name",
        ),
        Scope::GroupsRead => ("Groups: read", "your groups, their members and their plans"),
        Scope::GroupsWrite => (
            "Groups: write",
            "everything you can do in groups: make, join and leave them, post plans, say \"I am in\"",
        ),
        Scope::ReportsWrite => (
            "Reports: send",
            "report to whoever runs Venti what another pilot shares, in your name",
        ),
        Scope::KeptSitesRead => ("Your take-offs: read", "the sites you keep as your own"),
        Scope::KeptSitesWrite => (
            "Your take-offs: write",
            "keep a site as your own, and forget one",
        ),
        Scope::AlertsRead => ("Alarms: read", "your alarms, and what they fired"),
        Scope::AlertsWrite => ("Alarms: write", "set, change and remove your alarms"),
        Scope::SocialRead => (
            "Pilots you follow: read",
            "the flights of the pilots you follow, whom you follow, and what you were told",
        ),
        Scope::SocialWrite => (
            "Pilots you follow: write",
            "follow pilots, cheer flights and write comments in your name",
        ),
        Scope::Other => return None,
    })
}

/// What a scope is called in a list. One this version does not know is
/// said to be that, and not left out: the token has it.
fn title(scope: Scope) -> String {
    words(scope).map_or_else(
        || "Something this version of the app does not know".to_owned(),
        |(title, _)| title.to_owned(),
    )
}

/// The things a program can be allowed, in the order to list them, each
/// with its words.
#[uniffi::export]
pub fn token_scopes() -> Vec<TokenScope> {
    Scope::ALL
        .into_iter()
        .filter_map(|scope| {
            let (title, line) = words(scope)?;
            Some(TokenScope {
                scope: scope.as_str().to_owned(),
                title: title.to_owned(),
                line: line.to_owned(),
            })
        })
        .collect()
}

/// The words of the screen and of the form that makes a token.
#[uniffi::export]
pub fn token_form() -> TokenForm {
    TokenForm {
        title: "Programs with access".to_owned(),
        intro: "Let a logger, a script or another app work with your Venti account \
                without telling it your password. You choose what it may do, and you can \
                withdraw its access at any time."
            .to_owned(),
        name_label: "Name of the program".to_owned(),
        name_hint: "My logger".to_owned(),
        name_max_length: u32::try_from(MAX_TOKEN_NAME_LENGTH).unwrap_or(u32::MAX),
        scopes_title: "What it may do".to_owned(),
        scopes: token_scopes(),
        ends_label: "Ends on".to_owned(),
        ends_hint: "Leave empty for access that does not end until you withdraw it.".to_owned(),
        password_label: "Your password".to_owned(),
        password_reason: "Asked for so that nobody who picks up this device can give a program \
                          access to your account."
            .to_owned(),
        submit: "Give access".to_owned(),
        empty: "No program has access to your account.".to_owned(),
        never: "No program can change your password, delete your account, see your devices \
                or give access to another program, whatever you allow it."
            .to_owned(),
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

/// Checks the name a pilot gives a token, before anything is sent, and
/// returns it as it will be kept: without the spaces around it. What the
/// server would refuse is refused here, in words.
#[uniffi::export]
pub fn token_name_check(name: String) -> Result<String, CoreError> {
    parse_token_name(&name).map_err(|error| {
        invalid(&match error {
            TokenNameError::Empty => {
                "Give the program a name, so that you know later what this access is for."
                    .to_owned()
            }
            TokenNameError::TooLong => {
                format!("The name can have at most {MAX_TOKEN_NAME_LENGTH} characters.")
            }
            TokenNameError::Character(_) => {
                "The name cannot hold line breaks or characters that do not show.".to_owned()
            }
        })
    })
}

/// The request that makes a token. `scopes` are the names of
/// [`token_scopes`] the pilot chose, `ends_unix_s` the moment from which
/// the token is no longer good, if the pilot gave an end, and `password`
/// the account's password. It goes with the session's token.
#[uniffi::export]
pub fn token_create_request(
    name: String,
    scopes: Vec<String>,
    ends_unix_s: Option<i64>,
    password: String,
) -> Result<ServerRequest, CoreError> {
    let name = token_name_check(name)?;
    let mut chosen: Vec<Scope> = Vec::new();
    for scope in &scopes {
        let scope = Scope::from_name(scope).ok_or_else(|| {
            invalid("One of the things chosen is not known to this version of the app.")
        })?;
        if !chosen.contains(&scope) {
            chosen.push(scope);
        }
    }
    if chosen.is_empty() {
        return Err(invalid("Choose at least one thing the program may do."));
    }
    chosen.sort();
    let expires_at = match ends_unix_s {
        None => None,
        Some(at) => Some(
            DateTime::from_timestamp(at, 0)
                .ok_or_else(|| invalid("The end date could not be read."))?,
        ),
    };
    if password.is_empty() {
        return Err(invalid("Enter your password."));
    }
    json_request(
        "POST",
        api::ACCOUNT_TOKENS_PATH,
        &CreateAccessTokenRequest {
            name,
            scopes: chosen
                .iter()
                .map(|scope| scope.as_str().to_owned())
                .collect(),
            expires_at,
            password: Password::new(password),
            google_id_token: None,
        },
        true,
    )
}

/// Reads the server's answer to [`token_create_request`]: the token, and
/// its secret with the words for the one moment it is shown.
/// `now_unix_s` is the current time.
#[uniffi::export]
pub fn token_created(
    status: u16,
    body: Vec<u8>,
    now_unix_s: i64,
) -> Result<TokenCreated, CoreError> {
    let response: CreateAccessTokenResponse = answer(status, &body)?;
    Ok(TokenCreated {
        token: view(&response.token, now_unix_s),
        secret: response.secret,
        shown_once: "Copy it now: Venti does not show it again.".to_owned(),
        how_to_use: "Paste it into the program where it asks for a Venti token. Whoever has \
                     it can do what you allowed, so give it to nobody else."
            .to_owned(),
        copy: "Copy".to_owned(),
        done: "I have copied it".to_owned(),
    })
}

/// The request for the account's tokens. It goes with the session's
/// token.
#[uniffi::export]
pub fn tokens_request() -> ServerRequest {
    ServerRequest {
        method: "GET".to_owned(),
        path: api::ACCOUNT_TOKENS_PATH.to_owned(),
        body: None,
        content_type: None,
        needs_token: true,
    }
}

/// Reads the server's answer to [`tokens_request`]: the tokens, the
/// newest first. `now_unix_s` is the current time, by which a token is
/// said to have ended.
#[uniffi::export]
pub fn tokens(status: u16, body: Vec<u8>, now_unix_s: i64) -> Result<Vec<TokenView>, CoreError> {
    let response: AccessTokensResponse = answer(status, &body)?;
    Ok(response
        .tokens
        .iter()
        .map(|token| view(token, now_unix_s))
        .collect())
}

/// The words that ask the pilot before a token is withdrawn. `name` is
/// the token's name.
#[uniffi::export]
pub fn token_withdrawal(name: String) -> TokenWithdrawal {
    TokenWithdrawal {
        title: format!("Withdraw the access of \u{201c}{}\u{201d}?", name.trim()),
        message: "The program can do nothing more with your account from its next request \
                  on. This cannot be undone: to let it in again you make a new token and \
                  copy it into the program."
            .to_owned(),
        confirm: "Withdraw access".to_owned(),
        cancel: "Keep access".to_owned(),
    }
}

/// The request that withdraws a token, by the identifier the list gives.
/// It goes with the session's token.
#[uniffi::export]
pub fn token_withdraw_request(id: String) -> Result<ServerRequest, CoreError> {
    // An identifier is 26 characters of the server's alphabet. Anything
    // else is not put into a path.
    if id.len() != 26 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(invalid(
            "This access could not be found. Load the list again.",
        ));
    }
    Ok(ServerRequest {
        method: "DELETE".to_owned(),
        path: api::account_token_path(&id),
        body: None,
        content_type: None,
        needs_token: true,
    })
}

/// Reads the server's answer to [`token_withdraw_request`]. A token that
/// was withdrawn already is withdrawn: that is no error.
#[uniffi::export]
pub fn token_withdrawn(status: u16, body: Vec<u8>) -> Result<(), CoreError> {
    if (200..300).contains(&status) || status == 404 {
        return Ok(());
    }
    Err(refusal(status, &body))
}

fn day(at: Timestamp) -> String {
    at.format("%-d %b %Y").to_string()
}

fn view(token: &AccessToken, now_unix_s: i64) -> TokenView {
    let scopes: Vec<String> = token.scopes.iter().map(|scope| title(*scope)).collect();
    let has_ended = token
        .expires_at
        .is_some_and(|end| end.timestamp() <= now_unix_s);
    TokenView {
        id: token.id.clone(),
        name: token.name.clone(),
        scopes_line: scopes.join(" \u{b7} "),
        scopes,
        made: day(token.created_at),
        last_used: token.last_used_on.map_or_else(
            || "never".to_owned(),
            |on| format!("on {}", on.format("%-d %b %Y")),
        ),
        ends: token
            .expires_at
            .map_or_else(|| "never".to_owned(), |end| format!("on {}", day(end))),
        has_ended,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::server::refusal;

    /// 2026-11-21 09:05 UTC.
    const NOW: i64 = 1_795_251_900;

    /// The server's own recorded answers, which its tests keep current.
    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    fn message(error: CoreError) -> String {
        match error {
            CoreError::Server { message } => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_scope_has_words_for_a_pilot() {
        let scopes = token_scopes();
        assert_eq!(scopes.len(), Scope::ALL.len());
        for (scope, known) in scopes.iter().zip(Scope::ALL) {
            assert_eq!(scope.scope, known.as_str());
            assert!(scope.title.contains(": "), "{}", scope.title);
            assert!(!scope.line.is_empty() && !scope.line.ends_with('.'));
        }
        let flights = &scopes[1];
        assert_eq!(
            (flights.title.as_str(), flights.line.as_str()),
            ("Flights: read", "your logbook, the private flights too")
        );
        let form = token_form();
        assert_eq!(form.title, "Programs with access");
        assert_eq!(form.scopes, scopes);
        assert_eq!(form.name_max_length, 60);
    }

    #[test]
    fn the_request_is_what_the_servers_example_sends() {
        let request = token_create_request(
            "  My logger ".to_owned(),
            vec![
                "live:write".to_owned(),
                "flights:write".to_owned(),
                "live:write".to_owned(),
            ],
            None,
            "demo-pilot-password".to_owned(),
        )
        .unwrap();
        assert_eq!(
            (
                request.method.as_str(),
                request.path.as_str(),
                request.needs_token
            ),
            ("POST", "/v1/account/tokens", true)
        );
        assert_eq!(request.content_type.as_deref(), Some("application/json"));
        let sent: Value = serde_json::from_slice(&request.body.unwrap()).unwrap();
        let example: Value = serde_json::from_slice(&golden("token_create_request.json")).unwrap();
        assert_eq!(sent, example);

        // With an end, the moment is written as the API writes moments.
        let ending = token_create_request(
            "Vario".to_owned(),
            vec!["flights:read".to_owned()],
            Some(NOW + 86_400),
            "pw".to_owned(),
        )
        .unwrap();
        let sent: Value = serde_json::from_slice(&ending.body.unwrap()).unwrap();
        assert_eq!(sent["expires_at"], json!("2026-11-22T09:05:00Z"));
    }

    #[test]
    fn what_the_server_would_refuse_is_refused_before_sending() {
        let make = |name: &str, scopes: &[&str], password: &str| {
            token_create_request(
                name.to_owned(),
                scopes.iter().map(|&scope| scope.to_owned()).collect(),
                None,
                password.to_owned(),
            )
            .map_err(message)
        };
        assert_eq!(
            make("  ", &["flights:read"], "pw").unwrap_err(),
            "Give the program a name, so that you know later what this access is for."
        );
        assert_eq!(
            make(&"x".repeat(61), &["flights:read"], "pw").unwrap_err(),
            "The name can have at most 60 characters."
        );
        assert_eq!(
            make("two\nlines", &["flights:read"], "pw").unwrap_err(),
            "The name cannot hold line breaks or characters that do not show."
        );
        assert_eq!(
            make("Logger", &[], "pw").unwrap_err(),
            "Choose at least one thing the program may do."
        );
        assert_eq!(
            make("Logger", &["photos:write"], "pw").unwrap_err(),
            "One of the things chosen is not known to this version of the app."
        );
        assert_eq!(
            make("Logger", &["flights:read"], "").unwrap_err(),
            "Enter your password."
        );
        assert_eq!(
            token_name_check(" My logger ".to_owned()).unwrap(),
            "My logger"
        );
    }

    #[test]
    fn a_new_token_is_read_with_its_secret_and_the_words_for_that_moment() {
        let made = token_created(201, golden("token_created.json"), NOW).unwrap();
        assert!(made.secret.starts_with("venti_pat_"));
        assert_eq!(
            made.shown_once,
            "Copy it now: Venti does not show it again."
        );
        assert_eq!(made.token.name, "My logger");
        assert_eq!(made.token.scopes, ["Flights: write", "Live tracking: send"]);
        assert_eq!(
            made.token.scopes_line,
            "Flights: write \u{b7} Live tracking: send"
        );
        assert_eq!(
            (made.token.last_used.as_str(), made.token.ends.as_str()),
            ("never", "never")
        );
        // The secret is not in what a log would print.
        let printed = format!("{made:?}");
        assert!(!printed.contains(&made.secret), "{printed}");
    }

    #[test]
    fn the_list_says_when_each_was_made_used_and_ends() {
        let listed = tokens(200, golden("tokens.json"), NOW).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Demo logger");
        assert_eq!(listed[0].scopes, ["Flights: read", "Flights: write"]);
        assert_eq!(listed[0].id, "00000000000000000000000003");

        let answer = json!({"tokens": [
            {"id": "0123456789abcdefghjkmnpqrs", "name": "Vario", "scopes": ["live:write", "photos:write"],
             "created_at": "2026-11-01T08:00:00Z", "last_used_on": "2026-11-21",
             "expires_at": "2026-12-21T00:00:00Z"},
            {"id": "1123456789abcdefghjkmnpqrs", "name": "Old script", "scopes": ["flights:read"],
             "created_at": "2026-03-09T23:30:00Z", "expires_at": "2026-11-20T00:00:00Z"}
        ]});
        let listed = tokens(200, answer.to_string().into_bytes(), NOW).unwrap();
        let vario = &listed[0];
        assert_eq!(vario.made, "1 Nov 2026");
        assert_eq!(vario.last_used, "on 21 Nov 2026");
        assert_eq!(
            (vario.ends.as_str(), vario.has_ended),
            ("on 21 Dec 2026", false)
        );
        // A scope of a newer server is named as unknown, not left out.
        assert_eq!(
            vario.scopes,
            [
                "Live tracking: send",
                "Something this version of the app does not know"
            ]
        );
        let old = &listed[1];
        assert_eq!(old.made, "9 Mar 2026");
        assert_eq!(old.last_used, "never");
        assert_eq!((old.ends.as_str(), old.has_ended), ("on 20 Nov 2026", true));

        let asked = tokens_request();
        assert_eq!(
            (
                asked.method.as_str(),
                asked.path.as_str(),
                asked.needs_token
            ),
            ("GET", "/v1/account/tokens", true)
        );
    }

    #[test]
    fn withdrawing_asks_first_and_names_the_program() {
        let question = token_withdrawal("My logger".to_owned());
        assert_eq!(
            question.title,
            "Withdraw the access of \u{201c}My logger\u{201d}?"
        );
        assert!(question.message.contains("cannot be undone"));
        assert_eq!(
            (question.confirm.as_str(), question.cancel.as_str()),
            ("Withdraw access", "Keep access")
        );

        let request = token_withdraw_request("00000000000000000000000003".to_owned()).unwrap();
        assert_eq!(
            (
                request.method.as_str(),
                request.path.as_str(),
                request.needs_token
            ),
            (
                "DELETE",
                "/v1/account/tokens/00000000000000000000000003",
                true
            )
        );
        assert!(token_withdraw_request("../sessions".to_owned()).is_err());
        assert_eq!(token_withdrawn(204, Vec::new()), Ok(()));
        // Withdrawn already, on another device: it is gone, as asked.
        let gone = br#"{"error":{"code":"token_not_found","message":"no"}}"#;
        assert_eq!(token_withdrawn(404, gone.to_vec()), Ok(()));
        let busy = token_withdrawn(503, Vec::new()).unwrap_err();
        assert!(message(busy).contains("busy"));
    }

    #[test]
    fn every_refusal_of_the_tokens_has_words_for_a_pilot() {
        let words = |status: u16, code: &str| {
            let body = json!({"error": {"code": code, "message": "for developers"}});
            message(refusal(status, body.to_string().as_bytes()))
        };
        assert_eq!(words(403, "wrong_password"), "The password is wrong.");
        assert_eq!(
            words(409, "too_many_tokens"),
            "Your account has as many programs with access as it can. Withdraw one first."
        );
        assert_eq!(
            words(400, "invalid_token_name"),
            "The server does not accept this name."
        );
        assert!(
            words(400, "invalid_scopes").starts_with("The server does not know one of the things")
        );
        assert_eq!(
            words(400, "invalid_expiry"),
            "The end date has to be a day that is still to come."
        );
        assert_eq!(
            words(404, "token_not_found"),
            "This program's access is no longer there: it was withdrawn already."
        );
        // The server's own refusal of a wrong password, as recorded, read
        // by the reader of a new token.
        let refused =
            br#"{"error":{"code":"wrong_password","message":"the password is not correct"}}"#;
        let error = token_created(403, refused.to_vec(), NOW).unwrap_err();
        assert_eq!(message(error), "The password is wrong.");
    }
}
