use axum::Json;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::{ErrorCode, ErrorResponse};
use venti_store::StoreError;

/// A request that cannot be answered, on its way to becoming a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// The client sent something the API cannot read.
    pub(crate) fn bad_request(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    /// The thing asked for does not exist.
    pub(crate) fn missing(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse::new(self.code, self.message));
        let mut response = (self.status, body).into_response();
        let headers = response.headers_mut();
        // An error describes this moment; nothing may keep it.
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if self.status == StatusCode::SERVICE_UNAVAILABLE {
            headers.insert(header::RETRY_AFTER, HeaderValue::from_static("30"));
        }
        response
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        // The details are for the operator's log, not for whoever calls the API.
        tracing::error!("store: {error}");
        match error {
            StoreError::Unavailable(_) => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::Unavailable,
                "the server cannot reach its database; try again shortly",
            ),
            StoreError::Invalid(_) | StoreError::Failure(_) => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
                "the server failed to answer; the problem has been logged",
            ),
        }
    }
}

/// Answers every path the API does not have.
pub(crate) async fn not_found(uri: Uri) -> ApiError {
    ApiError::missing(
        ErrorCode::NotFound,
        format!(
            "{} is not part of the API; the endpoints are /v1/health, /v1/stations \
             and /v1/stations/{{id}}/observations",
            uri.path()
        ),
    )
}
