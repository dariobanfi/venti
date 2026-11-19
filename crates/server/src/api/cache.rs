//! What may keep an answer, and for how long.
//!
//! There are three kinds of answer.
//!
//! **The same for everyone and slow to change** (stations, observations):
//! anyone may reuse it for a minute, and it carries an `ETag`, so a client
//! that asks again with `If-None-Match` gets a bodiless `304` when nothing
//! changed. On a phone in the mountains that is the difference between a
//! few hundred bytes and tens of kilobytes.
//!
//! **Made for one account** (anything asked with a session token): nobody
//! may keep it, `no-store`, and it carries no `ETag`. A validator is a
//! fingerprint of the body, and a fingerprint of private data has no
//! business in a cache or a log along the way.
//!
//! **Shared by its pilot, and revocable** (a public or unlisted flight, a
//! pilot's page): it may be kept but must be checked with the server before
//! every reuse (`no-cache`), so that a flight made private is gone at once
//! and not when a cache gets round to it. A public one may be kept by
//! shared caches; an unlisted one, known only to whoever has its link, only
//! by the browser that asked.
//!
//! Every answer that depends on who asks says so with `Vary:
//! Authorization`.

use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::error::ApiError;

/// How long an answer that is the same for everyone may be reused without
/// asking again.
const MAX_AGE: &str = "public, max-age=60";

/// Who may keep an answer that must be checked before every reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keeper {
    /// Any cache.
    Anyone,
    /// Only the browser or app that asked.
    OnlyTheAsker,
}

impl Keeper {
    fn cache_control(self) -> &'static str {
        match self {
            Self::Anyone => "public, no-cache",
            Self::OnlyTheAsker => "private, no-cache",
        }
    }
}

fn to_json(value: &impl Serialize) -> Result<Vec<u8>, ApiError> {
    serde_json::to_vec(value).map_err(|error| {
        tracing::error!("a response could not be written as JSON: {error}");
        ApiError::internal()
    })
}

/// A body with its type, or `304` when the request's `If-None-Match` shows
/// that the client already has it, under the given `Cache-Control`.
fn validated(
    request: &HeaderMap,
    content_type: &'static str,
    body: Vec<u8>,
    cache_control: &'static str,
) -> Response {
    let tag = etag(&body);
    let mut response = if client_has(request, &tag) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        let content_type = HeaderValue::from_static(content_type);
        ([(header::CONTENT_TYPE, content_type)], body).into_response()
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    if let Ok(value) = HeaderValue::from_str(&tag) {
        headers.insert(header::ETAG, value);
    }
    response
}

/// Serialises `value` as the JSON body of an answer that is the same for
/// everyone and may be reused for a minute.
pub(crate) fn json(request: &HeaderMap, value: &impl Serialize) -> Response {
    match to_json(value) {
        Ok(body) => validated(request, "application/json", body, MAX_AGE),
        Err(error) => error.into_response(),
    }
}

/// Serialises `value` as the JSON body of an answer that its pilot shares
/// and can take back: kept by `keeper`, checked before every reuse.
pub(crate) fn shared_json(request: &HeaderMap, value: &impl Serialize, keeper: Keeper) -> Response {
    match to_json(value) {
        Ok(body) => shared(request, "application/json", body, keeper),
        Err(error) => error.into_response(),
    }
}

/// A body that its pilot shares and can take back: kept by `keeper`,
/// checked before every reuse. The answer depends on who asks.
pub(crate) fn shared(
    request: &HeaderMap,
    content_type: &'static str,
    body: Vec<u8>,
    keeper: Keeper,
) -> Response {
    let mut response = validated(request, content_type, body, keeper.cache_control());
    depends_on_who_asks(&mut response);
    response
}

/// Serialises `value` as the JSON body of an answer made for one account.
/// Nobody may keep it.
pub(crate) fn private_json(status: StatusCode, value: &impl Serialize) -> Response {
    match to_json(value) {
        Ok(body) => {
            let json = HeaderValue::from_static("application/json");
            private((status, [(header::CONTENT_TYPE, json)], body).into_response())
        }
        Err(error) => error.into_response(),
    }
}

/// Marks an answer as made for one account: nobody may keep it.
pub(crate) fn private(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.remove(header::ETAG);
    depends_on_who_asks(&mut response);
    response
}

/// Tells caches that the answer is for whoever sent this `Authorization`
/// header, or for those who sent none.
fn depends_on_who_asks(response: &mut Response) {
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("authorization"));
}

/// Sets a header whose value is fixed text.
pub(crate) fn set(response: &mut Response, name: HeaderName, value: &'static str) {
    response
        .headers_mut()
        .insert(name, HeaderValue::from_static(value));
}

/// A weak validator from the content of the body.
///
/// Weak, because the same answer may leave the server compressed or not;
/// the two differ byte for byte but mean the same.
pub(crate) fn etag(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    let hex: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("W/\"{hex}\"")
}

/// Whether `If-None-Match` names `tag`. The header holds a list of tags or
/// `*`; weak and strong forms of a tag count as the same.
pub(crate) fn client_has(request: &HeaderMap, tag: &str) -> bool {
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

    #[test]
    fn an_answer_for_one_account_is_kept_by_nobody_and_has_no_fingerprint() {
        let answer = private_json(StatusCode::OK, &[1, 2, 3]);
        assert_eq!(answer.status(), StatusCode::OK);
        assert_eq!(answer.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(answer.headers()[header::VARY], "authorization");
        assert!(!answer.headers().contains_key(header::ETAG));

        // Also when the answer was built as a shared one first.
        let shared = shared_json(&HeaderMap::new(), &[1, 2, 3], Keeper::Anyone);
        assert!(shared.headers().contains_key(header::ETAG));
        let answer = private(shared);
        assert_eq!(answer.headers()[header::CACHE_CONTROL], "no-store");
        assert!(!answer.headers().contains_key(header::ETAG));
    }

    #[test]
    fn a_shared_answer_is_checked_before_every_reuse() {
        let public = shared_json(&HeaderMap::new(), &[1, 2, 3], Keeper::Anyone);
        assert_eq!(public.headers()[header::CACHE_CONTROL], "public, no-cache");
        assert_eq!(public.headers()[header::VARY], "authorization");
        let tag = public.headers()[header::ETAG].to_str().unwrap().to_owned();

        let unlisted = shared_json(&asking_with(&tag), &[1, 2, 3], Keeper::OnlyTheAsker);
        assert_eq!(unlisted.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            unlisted.headers()[header::CACHE_CONTROL],
            "private, no-cache"
        );
    }
}
