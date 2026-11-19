use axum::Json;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::{ErrorCode, ErrorResponse};
use venti_store::StoreError;

use super::log::Sparse;
use crate::accounts::password::HashError;
use crate::accounts::secret::NoRandomness;
use crate::accounts::throttle::Refused;

/// A request that cannot be answered, on its way to becoming a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
    /// Seconds after which trying again makes sense.
    retry_after: Option<u64>,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            // A server that cannot answer now usually can in half a minute.
            retry_after: (status == StatusCode::SERVICE_UNAVAILABLE).then_some(30),
        }
    }

    /// The same error, telling the client after how many seconds trying
    /// again makes sense.
    pub(crate) fn retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds.max(1));
        self
    }

    /// Whether this says that the thing asked for does not exist.
    pub(crate) fn is_missing(&self) -> bool {
        self.status == StatusCode::NOT_FOUND
    }

    /// The HTTP status the error is answered with.
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    /// The error's stable code.
    pub(crate) fn code(&self) -> ErrorCode {
        self.code
    }

    /// The client sent something the API cannot read.
    pub(crate) fn bad_request(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    /// The thing asked for does not exist.
    pub(crate) fn missing(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    /// The request needs a session token and has none that is good.
    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized, message)
    }

    /// The request is understood and refused.
    pub(crate) fn forbidden(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    /// The server failed in a way the client can do nothing about. What
    /// failed is for the operator's log, which the caller writes.
    pub(crate) fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            "the server failed to answer; the problem has been logged",
        )
    }

    /// The server is too busy to do this now.
    pub(crate) fn busy(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::Unavailable,
            message,
        )
    }

    /// Too many attempts; `refused` says for how long.
    pub(crate) fn rate_limited(refused: Refused, what: &str) -> Self {
        let seconds = u64::try_from(refused.retry_after.num_seconds())
            .unwrap_or(0)
            .max(1);
        Self {
            retry_after: Some(seconds),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::RateLimited,
                format!("{what}; try again in {}", wait(seconds)),
            )
        }
    }
}

/// A waiting time in words, rounded up.
fn wait(seconds: u64) -> String {
    match seconds {
        0..=90 => format!("{seconds} seconds"),
        91..=5400 => format!("{} minutes", seconds.div_ceil(60)),
        _ => format!("{} hours", seconds.div_ceil(3600)),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse::new(self.code, self.message));
        let mut response = (self.status, body).into_response();
        let headers = response.headers_mut();
        // An error describes this moment; nothing may keep it.
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Some(seconds) = self.retry_after {
            headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        if self.status == StatusCode::UNAUTHORIZED {
            // How the API wants to be authenticated to.
            headers.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        // The details are for the operator's log, not for whoever calls the
        // API. While the database is away every request ends here, so the
        // log gets a line now and then, not one per request.
        static UNAVAILABLE: Sparse = Sparse::new();
        static FAILED: Sparse = Sparse::new();
        let gate = match error {
            StoreError::Unavailable(_) => &UNAVAILABLE,
            StoreError::Invalid(_) | StoreError::Failure(_) => &FAILED,
        };
        gate.error(format_args!("store: {error}"));
        match error {
            StoreError::Unavailable(_) => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::Unavailable,
                "the server cannot reach its database; try again shortly",
            ),
            StoreError::Invalid(_) | StoreError::Failure(_) => Self::internal(),
        }
    }
}

impl From<HashError> for ApiError {
    fn from(error: HashError) -> Self {
        match error {
            HashError::Busy => {
                Self::busy("the server is busy checking passwords; try again shortly")
            }
            HashError::Failed => {
                static GATE: Sparse = Sparse::new();
                GATE.error("a password could not be hashed");
                Self::internal()
            }
        }
    }
}

impl From<NoRandomness> for ApiError {
    fn from(error: NoRandomness) -> Self {
        static GATE: Sparse = Sparse::new();
        GATE.error(error);
        Self::internal()
    }
}

/// Answers every path the API does not have.
///
/// The path is repeated as the log would hold it: a path under a live
/// track is repeated without the track's identifier, because an error
/// message travels further than the request that caused it.
pub(crate) async fn not_found(uri: Uri) -> ApiError {
    ApiError::missing(
        ErrorCode::NotFound,
        format!(
            "{} is not part of the API; docs/api.md lists the endpoints, \
             which all start with /v1",
            super::log::logged_path(uri.path())
        ),
    )
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn a_refusal_says_how_long_to_wait() {
        let refused = |seconds: i64| {
            let refused = Refused {
                retry_after: Duration::seconds(seconds),
                first: false,
                shared: false,
            };
            ApiError::rate_limited(refused, "too many attempts")
        };
        let error = refused(725);
        assert_eq!(error.retry_after, Some(725));
        assert_eq!(error.message, "too many attempts; try again in 13 minutes");
        assert_eq!(
            refused(45).message,
            "too many attempts; try again in 45 seconds"
        );
        assert_eq!(
            refused(80_000).message,
            "too many attempts; try again in 23 hours"
        );
        // Never "now": the client would try again at once.
        assert_eq!(refused(0).retry_after, Some(1));

        let response = refused(725).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "725");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }

    #[test]
    fn an_unauthorized_answer_names_the_scheme() {
        let response = ApiError::unauthorized("sign in").into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
    }
}
