//! What every conversation with the Venti server has in common: reading
//! an answer, and putting a refusal into words for the pilot.

use serde::de::DeserializeOwned;

use crate::CoreError;

/// Decodes an answer, or explains to the pilot why there is none.
pub(crate) fn answer<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, CoreError> {
    if (200..300).contains(&status) {
        return serde_json::from_slice(body).map_err(|_| CoreError::Server {
            message: "The server's answer could not be read. The app may be older than the server."
                .to_owned(),
        });
    }
    Err(refusal(status, body))
}

/// The stable code of a refusal, if the body carries one. Read as plain
/// text, so that a code this version does not know is still passed on.
pub(crate) fn code(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|answer| {
            answer
                .get("error")?
                .get("code")?
                .as_str()
                .map(str::to_owned)
        })
}

/// Whether text has the shape of the API's codes: a few lowercase words
/// joined by underscores.
fn is_a_code(text: &str) -> bool {
    (1..=40).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

/// A refusal in words for the pilot. The server's own message is written
/// for developers; the pilot gets ours, and for anything unexpected the
/// code, for a bug report.
pub(crate) fn refusal(status: u16, body: &[u8]) -> CoreError {
    let code = code(body);
    let message = match (status, code.as_deref()) {
        (_, Some("invalid_credentials")) => "The handle or the password is wrong.".to_owned(),
        (_, Some("wrong_password")) => "The password is wrong.".to_owned(),
        (_, Some("unauthorized")) => {
            "This device is no longer signed in. Sign in again.".to_owned()
        }
        (_, Some("rate_limited")) => "Too many attempts. Wait a while, then try again.".to_owned(),
        (_, Some("invalid_invite")) => {
            "The invite code is missing, mistyped, already used or expired.".to_owned()
        }
        (_, Some("registration_closed")) => "This server takes no new accounts.".to_owned(),
        (_, Some("handle_taken")) => "This handle is taken. Choose another.".to_owned(),
        (_, Some("invalid_handle")) => "The server does not accept this handle.".to_owned(),
        (_, Some("invalid_display_name")) => "The server does not accept this name.".to_owned(),
        (_, Some("invalid_password")) => {
            "The server does not accept this password. It needs at least 15 characters.".to_owned()
        }
        (_, Some("flight_not_found")) => "The server no longer has this flight.".to_owned(),
        (_, Some("account_full")) => {
            "Your account on this server is full. Delete flights you no longer need to make room; what was not sent stays on this device."
                .to_owned()
        }
        (_, Some("box_too_large")) => {
            "The server holds more airspace around this place than one answer carries.".to_owned()
        }
        (_, Some("outside_region")) => {
            "This place lies outside the area the server has forecasts for.".to_owned()
        }
        (_, Some("no_forecasts")) => "This server has no forecasts.".to_owned(),
        (503, _) => "The server is busy or being restarted. Try again in a moment.".to_owned(),
        // A code this version has no words for is shown for a bug report,
        // if it looks like one of the API's codes. The address may be any
        // server's, and what it sends is not put before the pilot as it is.
        (_, Some(code)) if is_a_code(code) => format!("The server could not answer ({code})."),
        _ => format!("The server could not answer (status {status})."),
    };
    CoreError::Server { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(status: u16, body: &str) -> String {
        match refusal(status, body.as_bytes()) {
            CoreError::Server { message } => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_code_without_words_is_shown_only_if_it_looks_like_one() {
        // One of the API's own that this version has no words for.
        assert_eq!(
            words(409, r#"{"error":{"code":"something_new"}}"#),
            "The server could not answer (something_new)."
        );
        // Anything else a server sends is not put before the pilot.
        for code in [
            "Call +49 123 456 to unlock your account",
            "<b>bold</b>",
            "",
            "a_code_that_goes_on_and_on_and_on_and_on_and_on",
        ] {
            let body = serde_json::json!({"error": {"code": code}}).to_string();
            assert_eq!(
                words(418, &body),
                "The server could not answer (status 418)."
            );
        }
        assert_eq!(
            words(500, "not json"),
            "The server could not answer (status 500)."
        );
    }

    #[test]
    fn a_full_account_is_said_in_words() {
        assert!(
            words(403, r#"{"error":{"code":"account_full","message":"x"}}"#)
                .starts_with("Your account on this server is full.")
        );
    }
}
