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
        (_, Some("box_too_large")) => {
            "The server holds more airspace around this place than one answer carries.".to_owned()
        }
        (503, _) => "The server is busy or being restarted. Try again in a moment.".to_owned(),
        (_, Some(code)) => format!("The server could not answer ({code})."),
        _ => format!("The server could not answer (status {status})."),
    };
    CoreError::Server { message }
}
