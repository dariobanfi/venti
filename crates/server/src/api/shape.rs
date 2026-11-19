//! Two guarantees that hold for every request, whatever handler or layer
//! answers it.

use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::ErrorCode;

use super::DEADLINE;
use super::error::ApiError;

/// Gives up on a request that takes longer than the deadline.
///
/// The work behind it is dropped; a client that waits longer has usually
/// gone anyway.
pub(crate) async fn deadline(request: Request, next: Next) -> Response {
    match tokio::time::timeout(DEADLINE, next.run(request)).await {
        Ok(response) => response,
        Err(_) => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::Timeout,
            "the server could not answer in time; try again shortly",
        )
        .into_response(),
    }
}

/// Makes every error response a JSON error body.
///
/// The handlers already answer that way. This catches what the framework
/// and the layers answer on their own, as plain text or with no body at
/// all: a wrong method, a query string that cannot be decoded, a body that
/// is too large.
pub(crate) async fn uniform_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_error = status.is_client_error() || status.is_server_error();
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !is_error || is_json {
        return response;
    }

    let code = match status {
        StatusCode::NOT_FOUND => ErrorCode::NotFound,
        StatusCode::METHOD_NOT_ALLOWED => ErrorCode::MethodNotAllowed,
        StatusCode::PAYLOAD_TOO_LARGE => ErrorCode::PayloadTooLarge,
        _ if status.is_client_error() => ErrorCode::InvalidRequest,
        _ => ErrorCode::Internal,
    };
    let (mut parts, body) = response.into_parts();
    // What the framework said in plain text is the best explanation there is.
    let said = axum::body::to_bytes(body, 1024)
        .await
        .ok()
        .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
        .filter(|text| !text.trim().is_empty());
    let message = said.unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("the request failed")
            .to_owned()
    });

    let mut shaped = ApiError::new(status, code, message).into_response();
    // Keep what the original said in its headers, such as `Allow`.
    parts.headers.remove(header::CONTENT_TYPE);
    parts.headers.remove(header::CONTENT_LENGTH);
    for (name, value) in &parts.headers {
        shaped.headers_mut().entry(name).or_insert(value.clone());
    }
    shaped
}
