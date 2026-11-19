use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use venti_api_types::v1::{ErrorCode, Handle, PilotResponse};
use venti_store::{Listing, Store};

use super::cache::Keeper;
use super::error::ApiError;
use super::flights::PageParams;
use super::{Api, cache, wire};

/// `GET /v1/pilots/{handle}`: a pilot and their public flights, newest
/// first.
///
/// The answer is the same whoever asks, signed in or not: a pilot looking
/// at their own page sees what everyone sees.
///
/// A pilot who has made no flight public has no page: the answer is the
/// one for a handle nobody has. An account that shares nothing stays
/// invisible, and this endpoint is no way to find out which handles exist.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(handle): Path<String>,
    Query(params): Query<PageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let unknown = || {
        ApiError::missing(
            ErrorCode::PilotNotFound,
            "there is no pilot with this handle",
        )
    };
    // Text that cannot be a handle is the handle of no pilot.
    let handle = Handle::new(&handle).map_err(|_| unknown())?;
    let account = api
        .store
        .account(handle.as_str())
        .await?
        .ok_or_else(unknown)?;

    let (flights, next) = api
        .flight_page(
            &account.id,
            Listing::Public,
            params.after.as_deref(),
            params.limit.as_deref(),
        )
        .await?;
    // An empty page is the end of the list only if there is a list.
    if flights.is_empty() {
        let any = api
            .store
            .flights_of(&account.id, Listing::Public, None, 1)
            .await?;
        if any.is_empty() {
            return Err(unknown());
        }
    }
    let answer = PilotResponse {
        pilot: wire::pilot(&account.handle, &account.display_name)?,
        flights: wire::flights(&flights)?,
        next,
    };
    // Checked before every reuse: a flight made private leaves this list
    // at once.
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}
