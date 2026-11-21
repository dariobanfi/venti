//! Accounts and the sessions that sign a device in to one.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use venti_domain::Timestamp;

/// A pilot's handle: the short name an account is signed in with and found
/// by, for example `dario_b`.
///
/// Three to twenty characters: the letters `a` to `z`, digits and `_`. It
/// starts with a letter, does not end with `_` and has no two `_` in a row.
/// Upper and lower case are the same handle, and a handle is always written
/// in lower case. The narrow alphabet keeps handles safe in paths and makes
/// two different handles look different.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Handle(String);

/// Why a handle was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandleError {
    /// Fewer characters than [`Handle::MIN_LENGTH`].
    #[error("a handle has at least {} characters", Handle::MIN_LENGTH)]
    TooShort,
    /// More characters than [`Handle::MAX_LENGTH`].
    #[error("a handle has at most {} characters", Handle::MAX_LENGTH)]
    TooLong,
    /// A character outside `a` to `z`, `0` to `9` and `_`.
    #[error("a handle may only contain the letters a to z, digits and '_', not {0:?}")]
    Character(char),
    /// The first character is not a letter.
    #[error("a handle starts with a letter")]
    Start,
    /// It ends with `_` or has two in a row.
    #[error("a handle does not end with '_' and has no two '_' in a row")]
    Underscores,
}

impl Handle {
    /// The fewest characters a handle has.
    pub const MIN_LENGTH: usize = 3;
    /// The most characters a handle has.
    pub const MAX_LENGTH: usize = 20;

    /// Reads a handle as a person typed it: spaces around it and an `@` in
    /// front are dropped, and upper case becomes lower case.
    pub fn new(text: &str) -> Result<Self, HandleError> {
        let text = text.trim();
        let text = text.strip_prefix('@').unwrap_or(text);
        let handle = text.to_ascii_lowercase();

        if let Some(other) = handle
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
        {
            return Err(HandleError::Character(other));
        }
        // Only ASCII is left, so bytes are characters.
        if handle.len() < Self::MIN_LENGTH {
            return Err(HandleError::TooShort);
        }
        if handle.len() > Self::MAX_LENGTH {
            return Err(HandleError::TooLong);
        }
        if !handle.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(HandleError::Start);
        }
        if handle.ends_with('_') || handle.contains("__") {
            return Err(HandleError::Underscores);
        }
        Ok(Self(handle))
    }

    /// The handle as text, in lower case and without an `@`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Handle {
    type Err = HandleError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::new(text)
    }
}

impl TryFrom<String> for Handle {
    type Error = HandleError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(&text)
    }
}

impl From<Handle> for String {
    fn from(handle: Handle) -> Self {
        handle.0
    }
}

/// The most characters a display name has.
pub const MAX_DISPLAY_NAME_LENGTH: usize = 40;

/// Why a display name was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DisplayNameError {
    /// Nothing is left once the spaces around it are dropped.
    #[error("a display name must not be empty")]
    Empty,
    /// More characters than [`MAX_DISPLAY_NAME_LENGTH`].
    #[error("a display name has at most {MAX_DISPLAY_NAME_LENGTH} characters")]
    TooLong,
    /// A character that does not show, or that changes how the text around
    /// it is laid out.
    #[error(
        "a display name must not contain line breaks, tabs, or invisible or \
         direction-changing characters (found U+{0:04X})"
    )]
    Character(u32),
}

/// Reads the name a pilot wants to be shown by, as a person typed it. The
/// spaces around it are dropped.
///
/// Any script is welcome. What is refused is what would let one name pass
/// for another or break a page: control characters, spaces other than the
/// plain one, and characters that are invisible or reorder the text around
/// them.
pub fn parse_display_name(text: &str) -> Result<String, DisplayNameError> {
    let name = text.trim();
    if name.is_empty() {
        return Err(DisplayNameError::Empty);
    }
    if name.chars().count() > MAX_DISPLAY_NAME_LENGTH {
        return Err(DisplayNameError::TooLong);
    }
    if let Some(other) = name.chars().find(|c| unwanted_in_a_name(*c)) {
        return Err(DisplayNameError::Character(u32::from(other)));
    }
    Ok(name.to_owned())
}

/// Whether a character has no place in a name a person gives something:
/// one that does not show, or that changes how the text around it is laid
/// out.
pub(crate) fn unwanted_in_a_name(c: char) -> bool {
    c.is_control()
        || (c.is_whitespace() && c != ' ')
        || matches!(
            u32::from(c),
            // Soft hyphen, combining grapheme joiner, Arabic letter mark.
            0x00AD | 0x034F | 0x061C
            // Hangul fillers, which show as nothing.
            | 0x115F | 0x1160 | 0x3164 | 0xFFA0
            // Zero-width characters and direction marks.
            | 0x180E | 0x200B..=0x200F
            // Direction embeddings and overrides.
            | 0x202A..=0x202E
            // Word joiner, invisible operators, direction isolates.
            | 0x2060..=0x206F
            // Byte order mark, interlinear annotation, replacement.
            | 0xFEFF | 0xFFF9..=0xFFFB
            // Tag characters.
            | 0xE0000..=0xE007F
        )
}

/// A password on its way to the server.
///
/// It does not print itself: a request that holds one can be logged or shown
/// in a debugger without giving it away.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Password(String);

/// Why a new password was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PasswordError {
    /// Fewer characters than [`Password::MIN_LENGTH`].
    #[error(
        "a password has at least {} characters; a few words in a row are easy to remember",
        Password::MIN_LENGTH
    )]
    TooShort,
    /// More characters than [`Password::MAX_LENGTH`].
    #[error("a password has at most {} characters", Password::MAX_LENGTH)]
    TooLong,
}

impl Password {
    /// The fewest characters a new password has.
    ///
    /// Fifteen is what NIST SP 800-63B-4 requires of a password that is the
    /// only thing protecting an account, and what OWASP's authentication
    /// guidance repeats (both read 2026-11-19). There are no other rules:
    /// any characters, spaces included.
    pub const MIN_LENGTH: usize = 15;
    /// The most characters a password has. Long enough for any passphrase,
    /// short enough that hashing one cannot be used to keep the server busy.
    pub const MAX_LENGTH: usize = 128;

    /// Wraps a password.
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    /// The password, for hashing or for sending and nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether this may become an account's password. Each Unicode code
    /// point counts as one character.
    pub fn check(&self) -> Result<(), PasswordError> {
        match self.0.chars().count() {
            n if n < Self::MIN_LENGTH => Err(PasswordError::TooShort),
            n if n > Self::MAX_LENGTH => Err(PasswordError::TooLong),
            _ => Ok(()),
        }
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<hidden>)")
    }
}

/// A pilot's account, as its owner sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// The handle the account is signed in with and found by.
    pub handle: Handle,
    /// The name the pilot is shown by.
    pub display_name: String,
    /// When the account was created.
    pub created_at: Timestamp,
    /// Since when the account may not publish, if whoever runs the server
    /// has suspended that. Left out otherwise, which is the usual case.
    ///
    /// While it is there, the account can make no flight public (its
    /// flights can still be shared by link), and its live tracks are
    /// shown to those with their link whatever the app asks for. Only the
    /// operator can lift it. An app says so where the pilot chooses who
    /// sees a flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishing_suspended_at: Option<Timestamp>,
    /// Whether whoever runs the server named this account as one of
    /// Venti's partners for flying sites: it adds sites to the partners'
    /// layer and corrects them there. Left out otherwise, which is the
    /// usual case: every other pilot adds to the crowdsourced layer, where
    /// the server takes sites from pilots. An app offers a partner's form
    /// to an account that says so and to no other.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edits_sites: bool,
}

/// The body of `POST /v1/accounts`: creates an account and signs this
/// device in to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    /// The handle wanted; see [`Handle`].
    pub handle: String,
    /// The name to be shown by; see [`parse_display_name`].
    pub display_name: String,
    /// The password; see [`Password::check`].
    pub password: Password,
    /// An invite code, on a server that asks for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite: Option<String>,
    /// What to call this device in the list of sessions, for example
    /// `iPhone 17`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// The body of `POST /v1/sessions`: signs this device in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInRequest {
    /// The account's handle.
    pub handle: String,
    /// Its password.
    pub password: Password,
    /// What to call this device in the list of sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// The answer to creating an account and to signing in.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInResponse {
    /// The account.
    pub account: Account,
    /// The session that was created for this device.
    pub session: Session,
    /// The session's token. Send it with every request that needs the
    /// account, as `Authorization: Bearer <token>`. This is the only time
    /// the server shows it: it keeps a hash and cannot show it again.
    pub token: String,
}

impl fmt::Debug for SignInResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignInResponse")
            .field("account", &self.account)
            .field("session", &self.session)
            .field("token", &"<hidden>")
            .finish()
    }
}

/// One device signed in to an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The session's identifier, for signing it out. It is not the token
    /// and gives no access.
    pub id: String,
    /// What the device called itself when it signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// When the device signed in.
    pub created_at: Timestamp,
    /// When the session was last used, to within a few minutes.
    pub last_used_at: Timestamp,
    /// Whether this is the session the request was made with.
    pub current: bool,
}

/// The answer of `GET /v1/sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionsResponse {
    /// Every device signed in to the account, most recently used first.
    pub sessions: Vec<Session>,
}

/// The answer of `GET /v1/account`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountResponse {
    /// The account the request was made for.
    pub account: Account,
}

/// The body of `PUT /v1/account/password`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    /// The password the account has now.
    pub current_password: Password,
    /// The password it should have.
    pub new_password: Password,
}

/// The body of `DELETE /v1/account`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteAccountRequest {
    /// The account's password, as proof that its owner is asking.
    pub password: Password,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_is_read_as_a_person_types_it() {
        assert_eq!(Handle::new("dario_b").unwrap().as_str(), "dario_b");
        assert_eq!(Handle::new("  @DarioB ").unwrap().as_str(), "dariob");
        assert_eq!(Handle::new("Pilot2026").unwrap().to_string(), "pilot2026");
        assert_eq!("abc".parse::<Handle>().unwrap().as_str(), "abc");
        assert_eq!(Handle::new(&"a".repeat(20)).unwrap().as_str().len(), 20);
    }

    #[test]
    fn a_handle_outside_the_rules_is_refused_with_the_reason() {
        assert_eq!(Handle::new("ab"), Err(HandleError::TooShort));
        assert_eq!(Handle::new(""), Err(HandleError::TooShort));
        assert_eq!(Handle::new("@"), Err(HandleError::TooShort));
        assert_eq!(Handle::new(&"a".repeat(21)), Err(HandleError::TooLong));
        assert_eq!(Handle::new("2fast"), Err(HandleError::Start));
        assert_eq!(Handle::new("_abc"), Err(HandleError::Start));
        assert_eq!(Handle::new("abc_"), Err(HandleError::Underscores));
        assert_eq!(Handle::new("a__b"), Err(HandleError::Underscores));
        assert_eq!(Handle::new("dario b"), Err(HandleError::Character(' ')));
        assert_eq!(Handle::new("dario-b"), Err(HandleError::Character('-')));
        assert_eq!(Handle::new("../etc"), Err(HandleError::Character('.')));
        assert_eq!(Handle::new("a/b"), Err(HandleError::Character('/')));
        assert_eq!(Handle::new("@@abc"), Err(HandleError::Character('@')));
        // Letters that only look like a to z are not a to z.
        assert_eq!(Handle::new("dаrio"), Err(HandleError::Character('а')));
        assert_eq!(Handle::new("jürgen"), Err(HandleError::Character('ü')));
    }

    #[test]
    fn a_handle_is_a_plain_string_on_the_wire_and_is_checked_when_read() {
        let handle = Handle::new("dario_b").unwrap();
        assert_eq!(serde_json::to_string(&handle).unwrap(), r#""dario_b""#);
        assert_eq!(
            serde_json::from_str::<Handle>(r#""Dario_B""#).unwrap(),
            handle
        );
        assert!(serde_json::from_str::<Handle>(r#""a b""#).is_err());
    }

    #[test]
    fn a_display_name_may_be_in_any_script_but_not_invisible() {
        assert_eq!(parse_display_name("  Dario B. "), Ok("Dario B.".to_owned()));
        assert_eq!(parse_display_name("Jürgen Ö"), Ok("Jürgen Ö".to_owned()));
        assert_eq!(parse_display_name("山田 太郎"), Ok("山田 太郎".to_owned()));
        assert_eq!(parse_display_name("<b>&\"'"), Ok("<b>&\"'".to_owned()));
        assert_eq!(parse_display_name(&"é".repeat(40)).unwrap().len(), 80);

        assert_eq!(parse_display_name("   "), Err(DisplayNameError::Empty));
        assert_eq!(
            parse_display_name(&"a".repeat(41)),
            Err(DisplayNameError::TooLong)
        );
        for (name, code) in [
            ("two\nlines", 0x0A),
            ("tab\there", 0x09),
            ("nul\0", 0x00),
            ("zero\u{200B}width", 0x200B),
            ("right\u{202E}to left", 0x202E),
            ("isolate\u{2066}d", 0x2066),
            ("no\u{00A0}break", 0xA0),
            ("wide\u{3000}space", 0x3000),
            ("bom\u{FEFF}b", 0xFEFF),
            ("filler\u{3164}", 0x3164),
        ] {
            assert_eq!(
                parse_display_name(name),
                Err(DisplayNameError::Character(code)),
                "{name:?}"
            );
        }
    }

    #[test]
    fn a_password_is_judged_by_its_length_in_characters_and_nothing_else() {
        let check = |text: &str| Password::new(text).check();
        assert_eq!(check("fourteen chars"), Err(PasswordError::TooShort));
        assert_eq!(check("fifteen chars !"), Ok(()));
        assert_eq!(check("               "), Ok(()));
        // Fifteen characters, thirty bytes.
        assert_eq!(check(&"ü".repeat(15)), Ok(()));
        assert_eq!(check(&"a".repeat(128)), Ok(()));
        assert_eq!(check(&"a".repeat(129)), Err(PasswordError::TooLong));
    }

    #[test]
    fn passwords_and_tokens_do_not_print_themselves() {
        let request = SignInRequest {
            handle: "dario_b".to_owned(),
            password: Password::new("correct horse battery"),
            device: None,
        };
        let printed = format!("{request:?}");
        assert!(!printed.contains("horse"), "{printed}");
        assert!(printed.contains("dario_b"));
        // On the wire the password is a plain string.
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"handle":"dario_b","password":"correct horse battery"}"#
        );

        let now = chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap();
        let answer = SignInResponse {
            account: Account {
                handle: Handle::new("dario_b").unwrap(),
                display_name: "Dario".to_owned(),
                created_at: now,
                publishing_suspended_at: None,
                edits_sites: false,
            },
            session: Session {
                id: "s".to_owned(),
                device: None,
                created_at: now,
                last_used_at: now,
                current: true,
            },
            token: "venti_secret".to_owned(),
        };
        let printed = format!("{answer:?}");
        assert!(!printed.contains("venti_secret"), "{printed}");
    }

    #[test]
    fn an_account_says_that_it_may_not_publish_only_when_that_is_so() {
        let now = chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap();
        let account = Account {
            handle: Handle::new("dario_b").unwrap(),
            display_name: "Dario".to_owned(),
            created_at: now,
            publishing_suspended_at: None,
            edits_sites: false,
        };
        let json = serde_json::to_value(&account).unwrap();
        assert!(
            !json
                .as_object()
                .unwrap()
                .contains_key("publishing_suspended_at")
        );
        // An answer of a server from before the field reads as before.
        assert_eq!(serde_json::from_value::<Account>(json).unwrap(), account);

        let suspended = Account {
            publishing_suspended_at: Some(now),
            ..account
        };
        let json = serde_json::to_value(&suspended).unwrap();
        assert_eq!(json["publishing_suspended_at"], "2026-11-18T11:06:40Z");
        assert_eq!(serde_json::from_value::<Account>(json).unwrap(), suspended);
    }
}
