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
    /// The box holds more than one answer carries. Ask for a smaller one.
    BoxTooLarge,
    /// The station identifier in the path is not of the form `source:local`.
    InvalidStationId,
    /// `from` or `to` is not an RFC 3339 time.
    InvalidTime,
    /// `from` lies after `to`, or the window is longer than allowed.
    InvalidRange,
    /// No station has this identifier.
    StationNotFound,
    /// `lat` or `lon` is missing, or the two are not a place on the globe.
    InvalidPosition,
    /// The place lies outside the area the server answers forecasts for.
    OutsideRegion,
    /// The server has no forecast service. Asking again does not help;
    /// `forecasts` in the health answer says the same.
    NoForecasts,
    /// The server collects no rain radar. Asking again does not help.
    NoRadar,
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
    /// `sort` names no order the list has.
    InvalidSort,
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
    /// The account keeps as many flights, or as many bytes of flight
    /// files, as this server gives one account room for. Deleting flights
    /// makes room; sending the same file again does not help.
    AccountFull,
    /// Whoever runs the server has taken the flight down. It stays private
    /// until they release it; asking again does not help.
    FlightTakenDown,
    /// Whoever runs the server has suspended the account's publishing: it
    /// can make no flight public. Sharing by link still works.
    PublishingSuspended,
    /// A report names a reason the server does not know, has a text that
    /// is too long, or gives `other` as its reason without a text.
    InvalidReport,
    /// The name of a group is empty or too long.
    InvalidGroupName,
    /// The invite code is the code of no group: mistyped, replaced by a
    /// newer one, or ended when its group removed a member.
    InvalidGroupInvite,
    /// The group has as many members as a group may have.
    GroupFull,
    /// The account is in as many groups as an account may be in.
    TooManyGroups,
    /// No group has this identifier, or the caller is not a member of it.
    /// The two cannot be told apart.
    GroupNotFound,
    /// The handle is not the handle of a member of the group.
    GroupMemberNotFound,
    /// Only the keeper of the group may do this.
    NotGroupKeeper,
    /// A plan's day lies outside the days the server takes plans for, its
    /// place is empty or too long, or its note is too long.
    InvalidPlan,
    /// `from`, `to` or `date` is missing where it is needed, or is not a
    /// date written `YYYY-MM-DD`.
    InvalidDay,
    /// The group has no plan with this identifier, or none any more.
    PlanNotFound,
    /// Only the member who posted the plan may change it, and only that
    /// member and the group's keeper may delete it.
    NotPlanAuthor,
    /// The group holds as many plans as a group may hold at a time.
    GroupPlansFull,
    /// The identifier of a live track is not 26 characters of its
    /// alphabet.
    InvalidLiveTrackId,
    /// The fixes sent for a live track cannot be used: none or too many,
    /// not oldest first, or a value that cannot be a glider's.
    InvalidFixes,
    /// None of the fixes sent has a time the server takes: all lie in the
    /// future or too far in the past. The device's clock is probably
    /// wrong.
    FixesOutOfTime,
    /// No live track has this identifier, or none any more.
    LiveTrackNotFound,
    /// The identifier belongs to a live track of another account.
    LiveTrackTaken,
    /// The live track with this identifier was deleted or has been
    /// removed. An identifier is used once: begin a new track with a new
    /// one.
    LiveTrackGone,
    /// The live track holds as many fixes as a track may. Begin a new
    /// track with a new identifier.
    LiveTrackFull,
    /// What a request says about a site cannot be kept: a text too long
    /// or with what does not print, no use, a height or a wind direction
    /// that cannot be, a web address of another form, or a comment that
    /// is too long. The message names the field.
    InvalidSite,
    /// The place is in no region this server serves.
    SiteOutOfArea,
    /// No site has this identifier.
    SiteNotFound,
    /// The site was taken down. Its identifier is never used again.
    SiteGone,
    /// The site has no revision with this number.
    SiteRevisionNotFound,
    /// Another site that pilots made stands within a few metres of the
    /// place. Correct that one instead; the message names it.
    SiteTooClose,
    /// The site was changed since the revision the request names. Read it
    /// again and make the change from what it says now.
    SiteChanged,
    /// This server takes no sites from pilots.
    SitesClosed,
    /// The server hands out no map file under this path: no region with
    /// this identifier, not in this version, or no file of this name. A
    /// region whose files were renewed has a new version; the list of
    /// regions names it.
    MapNotFound,
    /// The bytes asked for with `Range` begin at or after the end of the
    /// file. `Content-Range` says how long the file is.
    RangeNotSatisfiable,
    /// Too many attempts. `Retry-After` says when to try again.
    RateLimited,
    /// The path is not part of the API.
    NotFound,
    /// The path exists but not with this method.
    MethodNotAllowed,
    /// The request carried a body larger than the server accepts.
    PayloadTooLarge,
    /// The request's body is JSON and was not sent as
    /// `Content-Type: application/json`.
    UnsupportedMediaType,
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
