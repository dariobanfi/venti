use serde::{Deserialize, Serialize};

/// The body of every response with a 4xx or 5xx status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// What went wrong.
    pub error: ErrorBody,
}

/// What went wrong with a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// A stable code for programs.
    pub code: ErrorCode,
    /// An explanation for the person debugging the client. Not for display
    /// to pilots, and not stable.
    pub message: String,
}

/// Why a request failed, as a stable code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The request could not be read at all, for example because its query
    /// string is not valid.
    InvalidRequest,
    /// The `bbox` parameter is missing or is not four numbers that form a box.
    InvalidBbox,
    /// The station identifier in the path is not of the form `source:local`.
    InvalidStationId,
    /// `from` or `to` is not an RFC 3339 time.
    InvalidTime,
    /// `from` lies after `to`, or the window is longer than allowed.
    InvalidRange,
    /// No station has this identifier.
    StationNotFound,
    /// A handle that is not written as handles are.
    InvalidHandle,
    /// A display name that is empty, too long or holds characters that do
    /// not show.
    InvalidDisplayName,
    /// A new password that is too short or too long, or is the handle.
    InvalidPassword,
    /// The `visibility` is not one the server knows.
    InvalidVisibility,
    /// `after` or `since` is not a value the server gave out.
    InvalidCursor,
    /// `limit` is not a number from one to the largest page.
    InvalidLimit,
    /// The request needs an account and carries no valid session token:
    /// none at all, one that was signed out, or one that expired.
    Unauthorized,
    /// The handle or the password is wrong. Which of the two is not said.
    InvalidCredentials,
    /// The password sent to confirm a change to the account is wrong.
    WrongPassword,
    /// The server takes no new accounts.
    RegistrationClosed,
    /// The server asks for an invite code, and the one given is missing,
    /// wrong, used or expired.
    InvalidInvite,
    /// The handle belongs to another account or is not available.
    HandleTaken,
    /// The account has no session with this identifier.
    SessionNotFound,
    /// No flight has this address, or it is not the caller's to see.
    FlightNotFound,
    /// No pilot has this handle.
    PilotNotFound,
    /// The uploaded file is not a flight file the server can use.
    InvalidFlight,
    /// Too many attempts. `Retry-After` says when to try again.
    RateLimited,
    /// The path is not part of the API.
    NotFound,
    /// The path exists but not with this method.
    MethodNotAllowed,
    /// The request carried a body larger than the server accepts.
    PayloadTooLarge,
    /// The server could not answer in time.
    Timeout,
    /// The server cannot reach its database. Try again later.
    Unavailable,
    /// The server failed in a way it did not expect.
    Internal,
    /// A code this version of the client does not know.
    #[serde(other)]
    Unknown,
}

impl ErrorResponse {
    /// An error with a code and an explanation.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            error: ErrorBody {
                code,
                message: message.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_snake_case_on_the_wire() {
        let error = ErrorResponse::new(ErrorCode::StationNotFound, "no such station");
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"error":{"code":"station_not_found","message":"no such station"}}"#
        );
    }

    #[test]
    fn a_code_from_a_newer_server_decodes_as_unknown() {
        let text = r#"{"error":{"code":"flight_quarantined","message":"under review"}}"#;
        let error: ErrorResponse = serde_json::from_str(text).unwrap();
        assert_eq!(error.error.code, ErrorCode::Unknown);
    }
}
