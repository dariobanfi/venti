//! Tokens a pilot gives to another program, and what such a token may do.
//!
//! A session is what Venti's own apps get by signing in, and it may do
//! everything the pilot may. A token is made by a signed-in pilot for one
//! program they name: a logger, a script, another maker's app. It opens
//! only what its scopes say, the pilot can withdraw it at any time, and
//! the program never learns the pilot's password.

use std::fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::Timestamp;

use super::account::{Password, unwanted_in_a_name};

/// One thing a token may do, named for what it opens.
///
/// Reading what anyone may read needs no token and no scope. A scope opens
/// what is the pilot's own, for the one account the token was made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    /// Reading the account: its handle, the name it is shown by, since
    /// when it exists.
    #[serde(rename = "account:read")]
    AccountRead,
    /// Reading the pilot's flights, the private ones among them: the
    /// list, what changed since, each flight and its file.
    #[serde(rename = "flights:read")]
    FlightsRead,
    /// Uploading flights, changing who may see one, deleting one.
    #[serde(rename = "flights:write")]
    FlightsWrite,
    /// Sending the positions of a live track, and ending one.
    #[serde(rename = "live:write")]
    LiveWrite,
    /// Adding, correcting, removing and restoring flying sites.
    #[serde(rename = "sites:write")]
    SitesWrite,
    /// Reading the pilot's groups, their members and their plans.
    #[serde(rename = "groups:read")]
    GroupsRead,
    /// Everything a pilot writes in groups: making and keeping a group,
    /// joining and leaving, plans, and saying "I am in".
    #[serde(rename = "groups:write")]
    GroupsWrite,
    /// Sending a report about what another pilot shares.
    #[serde(rename = "reports:write")]
    ReportsWrite,
    /// Reading the sites the pilot keeps as their own: the list, and what
    /// changed since.
    #[serde(rename = "kept-sites:read")]
    KeptSitesRead,
    /// Keeping a site as the pilot's own, and forgetting one.
    #[serde(rename = "kept-sites:write")]
    KeptSitesWrite,
    /// Reading the pilot's alarms and what they fired.
    #[serde(rename = "alerts:read")]
    AlertsRead,
    /// Setting, changing and removing the pilot's alarms.
    #[serde(rename = "alerts:write")]
    AlertsWrite,
    /// Reading what is the pilot's own among pilots: the flights of the
    /// pilots they follow, whom they follow, and what they were told.
    #[serde(rename = "social:read")]
    SocialRead,
    /// Following a pilot, cheering a flight, and writing and removing a
    /// comment, in the pilot's name.
    #[serde(rename = "social:write")]
    SocialWrite,
    /// A scope this version of the code does not know.
    #[serde(other)]
    Other,
}

impl Scope {
    /// Every scope there is, in the order the documents list them.
    pub const ALL: [Self; 14] = [
        Self::AccountRead,
        Self::FlightsRead,
        Self::FlightsWrite,
        Self::LiveWrite,
        Self::SitesWrite,
        Self::GroupsRead,
        Self::GroupsWrite,
        Self::ReportsWrite,
        Self::KeptSitesRead,
        Self::KeptSitesWrite,
        Self::AlertsRead,
        Self::AlertsWrite,
        Self::SocialRead,
        Self::SocialWrite,
    ];

    /// The scope's name on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccountRead => "account:read",
            Self::FlightsRead => "flights:read",
            Self::FlightsWrite => "flights:write",
            Self::LiveWrite => "live:write",
            Self::SitesWrite => "sites:write",
            Self::GroupsRead => "groups:read",
            Self::GroupsWrite => "groups:write",
            Self::ReportsWrite => "reports:write",
            Self::KeptSitesRead => "kept-sites:read",
            Self::KeptSitesWrite => "kept-sites:write",
            Self::AlertsRead => "alerts:read",
            Self::AlertsWrite => "alerts:write",
            Self::SocialRead => "social:read",
            Self::SocialWrite => "social:write",
            Self::Other => "other",
        }
    }

    /// The scope a name stands for. `None` for a name that is no scope's.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.as_str() == name)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What every token for a program starts with, so that one found in a log
/// or a repository can be recognised for what it is, and told from a
/// session's token, which starts with `venti_` alone.
pub const ACCESS_TOKEN_PREFIX: &str = "venti_pat_";

/// The most characters the name of a token has.
pub const MAX_TOKEN_NAME_LENGTH: usize = 60;

/// Why the name of a token was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenNameError {
    /// Nothing is left once the spaces around it are dropped.
    #[error("a token's name must not be empty")]
    Empty,
    /// More characters than [`MAX_TOKEN_NAME_LENGTH`].
    #[error("a token's name has at most {MAX_TOKEN_NAME_LENGTH} characters")]
    TooLong,
    /// A character that does not show, or that changes how the text around
    /// it is laid out.
    #[error(
        "a token's name must not contain line breaks, tabs, or invisible or \
         direction-changing characters (found U+{0:04X})"
    )]
    Character(u32),
}

/// Reads the name a pilot gives a token, as a person typed it: what the
/// program is that the token is for, such as "My logger". The spaces
/// around it are dropped. What a display name may not hold, this may not
/// hold either.
pub fn parse_token_name(text: &str) -> Result<String, TokenNameError> {
    let name = text.trim();
    if name.is_empty() {
        return Err(TokenNameError::Empty);
    }
    if name.chars().count() > MAX_TOKEN_NAME_LENGTH {
        return Err(TokenNameError::TooLong);
    }
    if let Some(other) = name.chars().find(|c| unwanted_in_a_name(*c)) {
        return Err(TokenNameError::Character(u32::from(other)));
    }
    Ok(name.to_owned())
}

/// A token a pilot gave to another program, as its pilot sees it. The
/// secret is not in it: the server shows that once, when the token is
/// made, and keeps only a hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessToken {
    /// The token's identifier, for withdrawing it. It is not the secret
    /// and gives no access.
    pub id: String,
    /// What the pilot called the program the token is for.
    pub name: String,
    /// What the token may do.
    pub scopes: Vec<Scope>,
    /// When the pilot made it.
    pub created_at: Timestamp,
    /// The day, in UTC, on which it was last used. Absent until it is
    /// used. The server keeps the day and nothing finer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_on: Option<NaiveDate>,
    /// The moment from which it is no longer good. Absent for a token
    /// that never ends. A token whose end has come stays in the list
    /// until the pilot withdraws it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// Present on a token the pilot did not make by hand: it came of the
    /// pilot's agreement on the server's own page, with a program that
    /// asked there (an assistant, above all). For such a token `name` is
    /// what the program calls itself, which nobody checked, and
    /// `expires_at` is the moment it ends unless the program uses it
    /// before: each use of its refresh token moves the end on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<TokenClient>,
}

/// The program a pilot agreed to, on a token that came of that agreement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenClient {
    /// The identifier the server gave the program when it registered.
    pub id: String,
    /// The address the token is good at and nowhere else: the server's
    /// connector.
    pub resource: String,
}

/// The body of `POST /v1/account/tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAccessTokenRequest {
    /// What the program is that the token is for, in the pilot's words.
    pub name: String,
    /// What the token may do, as the names of scopes: at least one, each
    /// once. They are text here and not [`Scope`], so that the server can
    /// say which name it does not know.
    pub scopes: Vec<String>,
    /// The moment from which the token is no longer good. Absent for a
    /// token that never ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// The account's password, as proof that its owner is asking. Left out
    /// by an account that is signed in to with Google, which sends
    /// `google_id_token` instead.
    #[serde(default)]
    pub password: Password,
    /// A fresh ID token of the Google account the account is linked to,
    /// in place of the password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_id_token: Option<String>,
}

/// The answer of `POST /v1/account/tokens`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAccessTokenResponse {
    /// The token as the list shows it from now on.
    pub token: AccessToken,
    /// The token's secret: what the program sends with every request, as
    /// `Authorization: Bearer <secret>`. This is the only time the server
    /// shows it: it keeps a hash and cannot show it again.
    pub secret: String,
}

impl fmt::Debug for CreateAccessTokenResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateAccessTokenResponse")
            .field("token", &self.token)
            .field("secret", &"<hidden>")
            .finish()
    }
}

/// The answer of `GET /v1/account/tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessTokensResponse {
    /// Every token the account holds, the newest first.
    pub tokens: Vec<AccessToken>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_is_written_as_what_it_opens_and_how() {
        for scope in Scope::ALL {
            let name = scope.as_str();
            assert_eq!(
                serde_json::to_string(&scope).unwrap(),
                format!("\"{name}\"")
            );
            assert_eq!(Scope::from_name(name), Some(scope));
            let (noun, verb) = name.split_once(':').unwrap();
            assert!(
                !noun.is_empty() && matches!(verb, "read" | "write"),
                "{name}"
            );
        }
        // The word that stands for a scope of a newer server is no scope
        // a pilot can ask for.
        assert_eq!(Scope::from_name("other"), None);
        assert_eq!(Scope::from_name("flights"), None);
        assert_eq!(Scope::from_name("Flights:Read"), None);
    }

    #[test]
    fn a_scope_of_a_newer_server_does_not_break_the_list() {
        let text = r#"{"id":"0123456789abcdefghjkmnpqrs","name":"Logger",
            "scopes":["flights:read","photos:write"],
            "created_at":"2026-11-21T08:00:00Z"}"#;
        let token: AccessToken = serde_json::from_str(text).unwrap();
        assert_eq!(token.scopes, [Scope::FlightsRead, Scope::Other]);
        assert_eq!(token.last_used_on, None);
        assert_eq!(token.expires_at, None);
    }

    #[test]
    fn a_tokens_name_is_trimmed_and_bounded() {
        assert_eq!(parse_token_name("  My logger ").unwrap(), "My logger");
        assert_eq!(
            parse_token_name("Flugbuch für Ölberg").unwrap(),
            "Flugbuch für Ölberg"
        );
        assert_eq!(parse_token_name("   "), Err(TokenNameError::Empty));
        assert_eq!(parse_token_name(&"x".repeat(60)).unwrap().len(), 60);
        assert_eq!(
            parse_token_name(&"x".repeat(61)),
            Err(TokenNameError::TooLong)
        );
        assert_eq!(
            parse_token_name("two\nlines"),
            Err(TokenNameError::Character(0x0A))
        );
        assert_eq!(
            parse_token_name("hid\u{200B}den"),
            Err(TokenNameError::Character(0x200B))
        );
    }

    #[test]
    fn the_answer_that_shows_a_secret_does_not_print_it() {
        let answer = CreateAccessTokenResponse {
            token: AccessToken {
                id: "0123456789abcdefghjkmnpqrs".to_owned(),
                name: "Logger".to_owned(),
                scopes: vec![Scope::FlightsWrite],
                created_at: "2026-11-21T08:00:00Z".parse().unwrap(),
                last_used_on: None,
                expires_at: None,
                client: None,
            },
            secret: "venti_pat_secret".to_owned(),
        };
        let printed = format!("{answer:?}");
        assert!(!printed.contains("venti_pat_secret"), "{printed}");
        assert!(printed.contains("<hidden>"));
    }
}
