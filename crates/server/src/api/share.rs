//! The page a link to a flight shows, and its preview picture.
//!
//! These two are not part of the versioned API. They answer a browser or a
//! chat app's preview fetcher, neither of which sends a session token, so
//! they show a flight only if its pilot made it public or unlisted. A
//! private flight and a flight that does not exist get the same answer.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use venti_store::{Flight, Store, Visibility};

use super::Api;
use super::auth::Viewer;
use super::cache::{self, Keeper};
use crate::flights::{picture, share};

const HTML: &str = "text/html; charset=utf-8";

/// What every answer about a shared flight carries, page or picture.
fn dress(response: &mut Response, flight: Option<&Flight>) {
    cache::set(response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // A link to an unlisted flight is the key to it. It is not passed on
    // to other sites in the Referer header.
    cache::set(response, header::REFERRER_POLICY, "same-origin");
    // Search engines may list what is public and nothing else.
    if flight.is_none_or(|flight| flight.visibility != Visibility::Public) {
        cache::set(response, HeaderName::from_static("x-robots-tag"), "noindex");
    }
}

fn html(mut response: Response) -> Response {
    cache::set(&mut response, header::CONTENT_TYPE, HTML);
    if let Ok(policy) = HeaderValue::from_str(&share::CONTENT_SECURITY_POLICY) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

/// The answer for a link that leads to nothing anyone may see.
fn nothing_here() -> Response {
    let mut response = html((StatusCode::NOT_FOUND, share::not_found_page()).into_response());
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    dress(&mut response, None);
    response
}

fn keeper(flight: &Flight) -> Keeper {
    match flight.visibility {
        Visibility::Public => Keeper::Anyone,
        Visibility::Unlisted | Visibility::Private => Keeper::OnlyTheAsker,
    }
}

/// The answer when the server cannot look the flight up right now.
fn try_again() -> Response {
    let mut response =
        html((StatusCode::SERVICE_UNAVAILABLE, share::trouble_page()).into_response());
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    cache::set(&mut response, header::RETRY_AFTER, "30");
    dress(&mut response, None);
    response
}

impl<S: Store> Api<S> {
    /// The flight a share link names, if anyone who has the link may see
    /// it; otherwise the answer to give instead.
    async fn shared_flight(&self, id: &str) -> Result<Flight, Response> {
        // Nobody is signed in here.
        self.visible_flight(id, &Viewer(None))
            .await
            .map_err(|error| match error.is_missing() {
                true => nothing_here(),
                false => try_again(),
            })
    }
}

/// `GET /f/{id}`: the page about one flight.
pub(crate) async fn page<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let flight = match api.shared_flight(&id).await {
        Ok(flight) => flight,
        Err(answer) => return answer,
    };
    let body = share::page(&flight, &api.public_url).into_bytes();
    let mut response = html(cache::shared(&headers, HTML, body, keeper(&flight)));
    dress(&mut response, Some(&flight));
    response
}

/// `GET /f/{id}/preview.png`: the picture that goes with the page.
pub(crate) async fn preview<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let flight = match api.shared_flight(&id).await {
        Ok(flight) => flight,
        Err(answer) => return answer,
    };
    // The picture depends on the flight's derived values alone, and those
    // change only together with its change number. So whether the asker's
    // copy is still good can be told without drawing anything.
    let drawn_from = format!("{} {} {}", picture::DESIGN, flight.id, flight.change);
    let tag = cache::etag(drawn_from.as_bytes());
    let mut response = if cache::client_has(&headers, &tag) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        // Drawing takes some milliseconds of a whole core: a few at a time,
        // and not on the threads that answer requests.
        let Ok(_turn) = api.pictures.acquire().await else {
            return try_again();
        };
        let drawn = {
            let flight = flight.clone();
            tokio::task::spawn_blocking(move || picture::render(&flight)).await
        };
        match drawn {
            Ok(Ok(png)) => ([(header::CONTENT_TYPE, "image/png")], png).into_response(),
            failed => {
                tracing::error!("the preview picture of a flight could not be drawn: {failed:?}");
                let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
                cache::set(&mut response, header::CACHE_CONTROL, "no-store");
                return response;
            }
        }
    };
    let cache_control = match keeper(&flight) {
        Keeper::Anyone => "public, no-cache",
        Keeper::OnlyTheAsker => "private, no-cache",
    };
    cache::set(&mut response, header::CACHE_CONTROL, cache_control);
    if let Ok(value) = HeaderValue::from_str(&tag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    dress(&mut response, Some(&flight));
    response
}
