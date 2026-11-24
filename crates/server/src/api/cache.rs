//! What may keep an answer, and for how long.
//!
//! There are three kinds of answer.
//!
//! **The same for everyone and slow to change** (stations, observations):
//! anyone may reuse it for a minute, and it carries an `ETag`, so a client
//! that asks again with `If-None-Match` gets a bodiless `304` when nothing
//! changed. On a phone in the mountains that is the difference between a
//! few hundred bytes and tens of kilobytes. Airspace changes once a day at
//! most and may be reused for an hour; for the airspace of a whole region
//! the `304` saves half a megabyte.
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
//! by the browser that asked. A live track is of this kind too, and is
//! asked for again every few seconds: its validator is worked out from
//! what the track is, before any of its fixes is read, so that an answer
//! of "nothing changed" costs one small read ([`unchanged`],
//! [`tagged_json`]).
//!
//! Every answer that depends on who asks says so with `Vary:
//! Authorization`.

use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::error::ApiError;
use super::log::Sparse;

/// How long an answer that is the same for everyone may be reused without
/// asking again.
const MAX_AGE: &str = "public, max-age=60";

/// The same for an answer whose data changes once a day at most. An hour
/// is how long a set that the server has just replaced may still be shown
/// as the newest; every answer says when its data was fetched.
const SLOW_MAX_AGE: &str = "public, max-age=3600";

/// The same for an answer about where aircraft are now: a few seconds,
/// so that the many who look at one part of the map share an answer.
const BRIEF_MAX_AGE: &str = "public, max-age=5";

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
        static GATE: Sparse = Sparse::new();
        GATE.error(format_args!(
            "a response could not be written as JSON: {error}"
        ));
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

/// Serialises `value` as the JSON body of an answer that is the same for
/// everyone and changes by the second: it may be reused for five seconds.
pub(crate) fn brief_json(request: &HeaderMap, value: &impl Serialize) -> Response {
    match to_json(value) {
        Ok(body) => validated(request, "application/json", body, BRIEF_MAX_AGE),
        Err(error) => error.into_response(),
    }
}

/// Serialises `value` as the JSON body of an answer that is the same for
/// everyone and changes once a day at most: it may be reused for an hour.
pub(crate) fn slow_json(request: &HeaderMap, value: &impl Serialize) -> Response {
    match to_json(value) {
        Ok(body) => validated(request, "application/json", body, SLOW_MAX_AGE),
        Err(error) => error.into_response(),
    }
}

/// A JSON body written once, of an answer that is the same for everyone
/// and changes with a release of the server at most: it may be reused for
/// an hour.
pub(crate) fn slow_json_body(request: &HeaderMap, body: Vec<u8>) -> Response {
    validated(request, "application/json", body, SLOW_MAX_AGE)
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

/// The bodiless `304` for a request whose `If-None-Match` names `tag`, the
/// validator of what the answer would be now. `None` when the client has
/// to be sent the answer.
///
/// For answers whose validator is known before their body is: the caller
/// works the tag out from little, and builds the body only when this says
/// that it is wanted.
pub(crate) fn unchanged(request: &HeaderMap, tag: &str, keeper: Keeper) -> Option<Response> {
    client_has(request, tag).then(|| {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        validators(&mut response, tag, keeper);
        response
    })
}

/// Serialises `value` as the JSON body of an answer whose validator is
/// `tag`: kept by `keeper`, checked before every reuse. The answer is the
/// same whoever asks.
pub(crate) fn tagged_json(tag: &str, keeper: Keeper, value: &impl Serialize) -> Response {
    match to_json(value) {
        Ok(body) => {
            let json = HeaderValue::from_static("application/json");
            let mut response = ([(header::CONTENT_TYPE, json)], body).into_response();
            validators(&mut response, tag, keeper);
            response
        }
        Err(error) => error.into_response(),
    }
}

/// Marks an answer as one whose validator is `tag`: kept by `keeper`,
/// checked before every reuse.
pub(crate) fn validators(response: &mut Response, tag: &str, keeper: Keeper) {
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(keeper.cache_control()),
    );
    if let Ok(value) = HeaderValue::from_str(tag) {
        headers.insert(header::ETAG, value);
    }
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

        // Data that changes once a day is validated in the same way and may
        // be reused for longer.
        let slow = slow_json(&asking_with(&tag), &[1, 2, 3]);
        assert_eq!(slow.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(slow.headers()[header::CACHE_CONTROL], SLOW_MAX_AGE);
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
    fn a_validator_known_before_the_body_saves_building_the_body() {
        let tag = etag(b"what the track is");
        // Nothing to compare with, or something else: the answer is wanted.
        assert!(unchanged(&HeaderMap::new(), &tag, Keeper::Anyone).is_none());
        assert!(unchanged(&asking_with("\"old\""), &tag, Keeper::Anyone).is_none());

        let same = unchanged(&asking_with(&tag), &tag, Keeper::OnlyTheAsker).unwrap();
        assert_eq!(same.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(same.headers()[header::ETAG], tag.as_str());
        assert_eq!(same.headers()[header::CACHE_CONTROL], "private, no-cache");

        let fresh = tagged_json(&tag, Keeper::Anyone, &[1, 2, 3]);
        assert_eq!(fresh.status(), StatusCode::OK);
        assert_eq!(fresh.headers()[header::ETAG], tag.as_str());
        assert_eq!(fresh.headers()[header::CACHE_CONTROL], "public, no-cache");
        assert_eq!(fresh.headers()[header::CONTENT_TYPE], "application/json");
        // The same for everyone: nothing varies with who asks.
        assert!(!fresh.headers().contains_key(header::VARY));
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
