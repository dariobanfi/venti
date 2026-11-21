use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use venti_api_types::v1::{ErrorCode, FlightTerms, Handle, PilotResponse};
use venti_store::{Account, Listing, Store};

use super::cache::Keeper;
use super::error::ApiError;
use super::flights::PageParams;
use super::{Api, cache, wire};

/// The answer for a handle that leads to no page: nobody has it, or its
/// pilot shares nothing publicly. The two must not differ in anything.
pub(crate) fn no_such_pilot() -> ApiError {
    ApiError::missing(
        ErrorCode::PilotNotFound,
        "there is no pilot with this handle",
    )
}

impl<S: Store> Api<S> {
    /// The account behind a handle, whether or not it has a page.
    async fn account_of(&self, handle: &str) -> Result<Account, ApiError> {
        // Text that cannot be a handle is the handle of no pilot.
        let handle = Handle::new(handle).map_err(|_| no_such_pilot())?;
        self.store
            .account(handle.as_str())
            .await?
            .ok_or_else(no_such_pilot)
    }

    /// Whether an account has a page: whether it has made a flight public.
    async fn has_a_page(&self, account: &Account) -> Result<bool, ApiError> {
        let any = self
            .store
            .flights_of(&account.id, Listing::Public, None, 1)
            .await?;
        Ok(!any.is_empty())
    }

    /// The account whose page a handle leads to.
    ///
    /// A pilot has a page once they have made a flight public. One who has
    /// not gets the answer for a handle nobody has.
    pub(crate) async fn pilot_with_a_page(&self, handle: &str) -> Result<Account, ApiError> {
        let account = self.account_of(handle).await?;
        if !self.has_a_page(&account).await? {
            return Err(no_such_pilot());
        }
        Ok(account)
    }
}

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
    let account = api.account_of(&handle).await?;

    let (flights, next) = api
        .flight_page(
            &account.id,
            Listing::Public,
            params.after.as_deref(),
            params.limit.as_deref(),
        )
        .await?;
    // An empty page is the end of the list only if there is a list.
    if flights.is_empty() && !api.has_a_page(&account).await? {
        return Err(no_such_pilot());
    }
    let answer = PilotResponse {
        pilot: wire::pilot(&account.handle, &account.display_name)?,
        flights: wire::flights(&flights)?,
        next,
        terms: Some(FlightTerms::current()),
    };
    // Checked before every reuse: a flight made private leaves this list
    // at once.
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}
