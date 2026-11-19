//! Cache validators for responses that carry data.
//!
//! Observations change every few minutes, and the same answer goes to every
//! client. So a data response may be reused by anyone for a minute, and
//! carries an `ETag`: a client that asks again with `If-None-Match` gets a
//! bodiless `304` when nothing changed. On a phone in the mountains that is
//! the difference between a few hundred bytes and tens of kilobytes.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use sha2::{Digest, Sha256};
use venti_api_types::v1::ErrorCode;

use super::error::ApiError;

/// How long a data response may be reused without asking again.
const MAX_AGE: &str = "public, max-age=60";

/// Serialises `value` as the JSON body of a cacheable `200`, or answers
/// `304` when the request's `If-None-Match` shows the client already has it.
pub(crate) fn json(request: &HeaderMap, value: &impl Serialize) -> Response {
    let body = match serde_json::to_vec(value) {
        Ok(body) => body,
        Err(error) => {
            tracing::error!("a response could not be written as JSON: {error}");
            return ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
                "the server failed to answer; the problem has been logged",
            )
            .into_response();
        }
    };
    let tag = etag(&body);

    let mut response = if client_has(request, &tag) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let json = HeaderValue::from_static("application/json");
        ([(header::CONTENT_TYPE, json)], body).into_response()
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(MAX_AGE));
    if let Ok(value) = HeaderValue::from_str(&tag) {
        headers.insert(header::ETAG, value);
    }
    response
}

/// A weak validator from the content of the body.
///
/// Weak, because the same answer may leave the server compressed or not;
/// the two differ byte for byte but mean the same.
fn etag(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let hex: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("W/\"{hex}\"")
}

/// Whether `If-None-Match` names `tag`. The header holds a list of tags or
/// `*`; weak and strong forms of a tag count as the same.
fn client_has(request: &HeaderMap, tag: &str) -> bool {
    let opaque = |tag: &str| tag.trim().trim_start_matches("W/").to_owned();
    let wanted = opaque(tag);
    request
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|candidate| candidate.trim() == "*" || opaque(candidate) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, value.parse().unwrap());
        headers
    }

    #[test]
    fn the_tag_follows_the_content() {
        assert_eq!(etag(b"{}"), etag(b"{}"));
        assert_ne!(etag(b"{}"), etag(b"[]"));
        assert!(etag(b"{}").starts_with("W/\""));
    }

    #[test]
    fn if_none_match_accepts_lists_and_both_forms() {
        let tag = etag(b"{}");
        let strong = tag.trim_start_matches("W/");
        assert!(client_has(&asking_with(&tag), &tag));
        assert!(client_has(&asking_with(strong), &tag));
        assert!(client_has(&asking_with(&format!("\"old\", {tag}")), &tag));
        assert!(client_has(&asking_with("*"), &tag));
        assert!(!client_has(&asking_with("\"old\""), &tag));
        assert!(!client_has(&HeaderMap::new(), &tag));
    }

    #[test]
    fn a_matching_request_gets_304_with_the_same_validators() {
        let fresh = json(&HeaderMap::new(), &[1, 2, 3]);
        assert_eq!(fresh.status(), StatusCode::OK);
        let tag = fresh.headers()[header::ETAG].to_str().unwrap().to_owned();

        let again = json(&asking_with(&tag), &[1, 2, 3]);
        assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(again.headers()[header::ETAG], tag.as_str());
        assert_eq!(again.headers()[header::CACHE_CONTROL], MAX_AGE);

        let changed = json(&asking_with(&tag), &[1, 2, 4]);
        assert_eq!(changed.status(), StatusCode::OK);
    }
}
