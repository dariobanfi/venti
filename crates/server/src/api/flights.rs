//! A pilot's flights: uploading, listing, what changed, one flight and its
//! file, who may see it, deleting.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::DateTime;
use serde::Deserialize;
use tokio::sync::{Semaphore, SemaphorePermit};
use venti_api_types::v1::{
    DeletedFlight, ErrorCode, FlightChangesResponse, FlightId, FlightResponse, FlightsResponse,
    MAX_FLIGHT_FILE_BYTES, UpdateFlightRequest, UploadResponse,
};
use venti_store::{Flight, FlightKey, Listing, Store, Visibility};

use super::auth::{Signed, Viewer};
use super::cache::Keeper;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, body, cache, wire};
use crate::accounts::throttle::{Counted, Refused};
use crate::flights::analyser::Failure;
use crate::flights::{new_flight, unpack};

/// The answer for a flight that does not exist and for one the caller may
/// not see. They must not differ in anything.
pub(crate) fn no_such_flight() -> ApiError {
    ApiError::missing(
        ErrorCode::FlightNotFound,
        "there is no flight with this address that you may see",
    )
}

fn unusable(message: impl Into<String>) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::InvalidFlight,
        message,
    )
}

impl<S: Store> Api<S> {
    /// The flight with this address, if `viewer` may see it: its pilot
    /// always, anyone else unless it is private.
    pub(crate) async fn visible_flight(
        &self,
        id: &str,
        viewer: &Viewer,
    ) -> Result<Flight, ApiError> {
        // An address of another form is the address of no flight.
        let id: FlightId = id.parse().map_err(|_| no_such_flight())?;
        let flight = self
            .store
            .flight(id.as_str())
            .await?
            .ok_or_else(no_such_flight)?;
        let may_see = flight.visibility != Visibility::Private || viewer.is(&flight.account_id);
        may_see.then_some(flight).ok_or_else(no_such_flight)
    }
}

/// Who may keep an answer about a flight that the asker did not sign in
/// for.
fn keeper(flight: &Flight) -> Keeper {
    match flight.visibility {
        Visibility::Public => Keeper::Anyone,
        Visibility::Unlisted | Visibility::Private => Keeper::OnlyTheAsker,
    }
}

/// How many flight files the server takes in at the same time. A file is
/// held in memory from its first byte until it is stored, so this is what
/// bounds the memory uploads can take: a few dozen megabytes.
const UPLOADS_AT_ONCE: usize = 8;

/// How many of those may belong to one account, so that one account cannot
/// take every place.
const UPLOADS_AT_ONCE_PER_ACCOUNT: usize = 2;

/// The uploads that are under way.
#[derive(Debug)]
pub(crate) struct Uploading {
    places: Semaphore,
    by_account: Mutex<HashMap<String, usize>>,
}

/// A place among the uploads under way. Dropping it gives the place back.
#[derive(Debug)]
pub(crate) struct Place<'a> {
    uploading: &'a Uploading,
    account_id: String,
    _place: SemaphorePermit<'a>,
}

/// Why an upload may not start now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoPlace {
    /// The account is already sending as many files as one account may.
    Account,
    /// The server is taking in as many files as it does at once.
    Server,
}

impl Uploading {
    pub(crate) fn new() -> Self {
        Self {
            places: Semaphore::new(UPLOADS_AT_ONCE),
            by_account: Mutex::new(HashMap::new()),
        }
    }

    /// Takes a place for an upload of this account, without waiting.
    pub(crate) fn begin(&self, account_id: &str) -> Result<Place<'_>, NoPlace> {
        let mut by_account = self
            .by_account
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let count = by_account.entry(account_id.to_owned()).or_insert(0);
        if *count >= UPLOADS_AT_ONCE_PER_ACCOUNT {
            return Err(NoPlace::Account);
        }
        let Ok(place) = self.places.try_acquire() else {
            if *count == 0 {
                by_account.remove(account_id);
            }
            return Err(NoPlace::Server);
        };
        *count += 1;
        Ok(Place {
            uploading: self,
            account_id: account_id.to_owned(),
            _place: place,
        })
    }
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        let mut by_account = self
            .uploading
            .by_account
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = by_account.get_mut(&self.account_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                by_account.remove(&self.account_id);
            }
        }
    }
}

/// `POST /v1/flights`: stores a flight file. The body is the file.
pub(crate) async fn upload<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    body: Body,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    let account_id = &signed.account.id;
    // A place first, and only then the file: what has no place is refused
    // before a byte of it is read.
    let _place = api.uploading.begin(account_id).map_err(|none| match none {
        NoPlace::Account => ApiError::rate_limited(
            Refused {
                retry_after: chrono::Duration::seconds(5),
                first: false,
                shared: false,
            },
            "this account is already sending two flight files; send them one after another",
        ),
        NoPlace::Server => ApiError::busy(
            "the server is taking in as many flight files as it can; try again shortly",
        ),
    })?;
    // Counted before the file is looked at: looking is the expensive part.
    let by_account = [Counted::new(account_id, api.limits.uploads_per_day)];
    api.uploads.attempt(&by_account, now).map_err(|refused| {
        ApiError::rate_limited(refused, "this account has uploaded too many files today")
    })?;
    let file: Bytes = axum::body::to_bytes(body, MAX_UPLOAD_BYTES)
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorCode::PayloadTooLarge,
                format!(
                    "the file is larger than {} MB, or its upload broke off",
                    MAX_UPLOAD_BYTES / (1024 * 1024)
                ),
            )
        })?;
    if file.is_empty() {
        return Err(unusable(
            "the request has no body; send the IGC flight file as the body",
        ));
    }

    let file: Arc<[u8]> = file.to_vec().into();
    let derivation = match api.analyser.analyse(file.clone()).await {
        Ok(derivation) => derivation,
        Err(Failure::Unusable(reason)) => {
            return Err(unusable(format!("the file cannot be used: {reason}")));
        }
        Err(failure @ (Failure::TooSlow | Failure::Crashed)) => {
            // Such a file cost the server all the time it allows. Only a
            // few a day are tolerated from one account.
            let weight = api.limits.slow_upload_weight;
            api.uploads.count(&by_account, weight, now);
            tracing::warn!(account = %signed.account.handle, "upload refused: {failure}");
            return Err(unusable(
                "the file could not be analysed; if it is a flight file that other \
                 programs read, please report it",
            ));
        }
        Err(Failure::Busy) => {
            return Err(ApiError::busy(
                "the server is busy analysing flights; try again shortly",
            ));
        }
        Err(Failure::Broken) => return Err(ApiError::internal()),
    };

    // Compressing is work too; keep it off the threads that answer.
    let account = account_id.clone();
    let new = tokio::task::spawn_blocking(move || new_flight(&account, &file, derivation, now))
        .await
        .map_err(|_| ApiError::internal())?;
    let (flight, created) = api.store.save_flight(&new).await?;

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let answer = UploadResponse {
        flight: wire::flight(&flight)?,
        created,
    };
    Ok(cache::private_json(status, &answer))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PageParams {
    pub(crate) after: Option<String>,
    pub(crate) limit: Option<String>,
}

/// Where the next page of a list starts, as text for the client to hand
/// back: the start of the last flight in microseconds and its address.
fn page_token(key: &FlightKey) -> String {
    format!("{}_{}", key.0.timestamp_micros(), key.1)
}

fn page_key(token: &str) -> Result<FlightKey, ApiError> {
    token
        .split_once('_')
        .and_then(|(micros, id)| {
            let start = DateTime::from_timestamp_micros(micros.parse().ok()?)?;
            let id: FlightId = id.parse().ok()?;
            Some((start, id.as_str().to_owned()))
        })
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidCursor,
                "after must be the next of an earlier answer",
            )
        })
}

impl<S: Store> Api<S> {
    /// One page of an account's flights and the token for the next.
    pub(crate) async fn flight_page(
        &self,
        account_id: &str,
        listing: Listing,
        after: Option<&str>,
        limit: Option<&str>,
    ) -> Result<(Vec<Flight>, Option<String>), ApiError> {
        let limit = body::page_size(limit)?;
        let after = after.map(page_key).transpose()?;
        // One more than asked for, to know whether a page follows.
        let mut flights = self
            .store
            .flights_of(account_id, listing, after.as_ref(), limit + 1)
            .await?;
        let more = flights.len() > limit;
        flights.truncate(limit);
        let next = flights
            .last()
            .filter(|_| more)
            .map(|last| page_token(&last.key()));
        Ok((flights, next))
    }
}

/// `GET /v1/flights`: the account's flights, newest first.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Query(params): Query<PageParams>,
) -> Result<Response, ApiError> {
    let (flights, next) = api
        .flight_page(
            &signed.account.id,
            Listing::All,
            params.after.as_deref(),
            params.limit.as_deref(),
        )
        .await?;
    let answer = FlightsResponse {
        flights: wire::flights(&flights)?,
        next,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChangesParams {
    since: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/flights/changes`: what happened to the account's flights since
/// the device last asked.
pub(crate) async fn changes<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Query(params): Query<ChangesParams>,
) -> Result<Response, ApiError> {
    let limit = body::page_size(params.limit.as_deref())?;
    let asked: i64 = match params.since.as_deref() {
        None => 0,
        Some(text) => text
            .parse()
            .ok()
            .filter(|since| *since >= 0)
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidCursor,
                    "since must be the cursor of an earlier answer",
                )
            })?,
    };

    let account_id = &signed.account.id;
    let mut changes = api.store.flight_changes(account_id, asked, limit).await?;
    // A cursor from beyond the account's latest change was not given out by
    // this database as it is now: it was restored from a backup, or the
    // cursor belongs elsewhere. Start again from the beginning, and say so.
    let reset = asked > changes.latest;
    let since = if reset { 0 } else { asked };
    if reset {
        changes = api.store.flight_changes(account_id, 0, limit).await?;
    }

    let answer = FlightChangesResponse {
        cursor: changes.last().unwrap_or(since).to_string(),
        flights: wire::flights(&changes.flights)?,
        deleted: changes
            .deleted
            .iter()
            .map(|deleted| {
                Ok(DeletedFlight {
                    id: deleted.id.parse().map_err(|_| ApiError::internal())?,
                    identity: deleted.identity.clone(),
                })
            })
            .collect::<Result<_, ApiError>>()?,
        more: changes.more,
        reset,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `GET /v1/flights/{id}`: one flight. Its pilot sees it always; everyone
/// else unless it is private.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    viewer: Viewer,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let flight = api.visible_flight(&id, &viewer).await?;
    let answer = FlightResponse {
        flight: wire::flight(&flight)?,
    };
    Ok(match viewer {
        // Asked with a token: made for that account, kept by nobody.
        Viewer(Some(_)) => cache::private_json(StatusCode::OK, &answer),
        Viewer(None) => cache::shared_json(&headers, &answer, keeper(&flight)),
    })
}

/// `GET /v1/flights/{id}/igc`: the flight file, exactly as it was uploaded.
pub(crate) async fn file<S: Store>(
    State(api): State<Arc<Api<S>>>,
    viewer: Viewer,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let flight = api.visible_flight(&id, &viewer).await?;
    // The flight can be deleted between the two questions.
    let packed = api
        .store
        .flight_file(&flight.id)
        .await?
        .ok_or_else(no_such_flight)?;
    let file = tokio::task::spawn_blocking(move || unpack(&packed))
        .await
        .ok()
        .flatten()
        .ok_or_else(|| {
            static GATE: Sparse = Sparse::new();
            GATE.error(format_args!(
                "the stored file of flight {} cannot be unpacked",
                flight.id
            ));
            ApiError::internal()
        })?;

    let mut response = match viewer {
        Viewer(Some(_)) => cache::private(file.into_response()),
        Viewer(None) => cache::shared(&headers, "application/octet-stream", file, keeper(&flight)),
    };
    cache::set(
        &mut response,
        header::CONTENT_TYPE,
        "application/octet-stream",
    );
    cache::set(&mut response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // A name built from the date and the address: nothing a pilot or a
    // file supplied goes into a header.
    let name = format!(
        "attachment; filename=\"venti-{}-{}.igc\"",
        flight.derived.start.format("%Y-%m-%d"),
        &flight.id[..8]
    );
    if let Ok(value) = HeaderValue::from_str(&name) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

/// `PATCH /v1/flights/{id}`: changes who may see a flight.
pub(crate) async fn update<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: UpdateFlightRequest = body::json(&body)?;
    let visibility = wire::stored_visibility(request.visibility).ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidVisibility,
            "visibility must be one of private, unlisted, public",
        )
    })?;
    let id: FlightId = id.parse().map_err(|_| no_such_flight())?;

    // A flight of someone else is no flight of this account, whoever may
    // see it.
    let flight = api
        .store
        .set_visibility(&signed.account.id, id.as_str(), visibility, api.clock.now())
        .await?
        .ok_or_else(no_such_flight)?;
    let answer = FlightResponse {
        flight: wire::flight(&flight)?,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/flights/{id}`: deletes a flight and its file.
pub(crate) async fn delete<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id: FlightId = id.parse().map_err(|_| no_such_flight())?;
    let deleted = api
        .store
        .delete_flight(&signed.account.id, id.as_str(), api.clock.now())
        .await?;
    if !deleted {
        return Err(no_such_flight());
    }
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// The largest body the upload accepts.
pub(crate) const MAX_UPLOAD_BYTES: usize = MAX_FLIGHT_FILE_BYTES;

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn only_so_many_files_are_taken_in_at_once() {
        let uploading = Uploading::new();

        // One account gets two places and no third.
        let first = uploading.begin("dario").unwrap();
        let second = uploading.begin("dario").unwrap();
        assert_eq!(uploading.begin("dario").unwrap_err(), NoPlace::Account);
        // Another account is not affected, and a place given back is free.
        let annas = uploading.begin("anna").unwrap();
        drop(first);
        let third = uploading.begin("dario").unwrap();

        // All accounts together get eight.
        let mut others = Vec::new();
        for n in 0..UPLOADS_AT_ONCE - 3 {
            others.push(uploading.begin(&format!("pilot{n}")).unwrap());
        }
        assert_eq!(uploading.begin("late").unwrap_err(), NoPlace::Server);
        assert_eq!(uploading.begin("anna").unwrap_err(), NoPlace::Server);
        drop(annas);
        let late = uploading.begin("late").unwrap();

        // When everything is done, nothing is remembered.
        drop((second, third, others, late));
        assert!(uploading.by_account.lock().unwrap().is_empty());
        assert_eq!(uploading.places.available_permits(), UPLOADS_AT_ONCE);
    }

    #[test]
    fn a_page_token_names_the_last_flight_and_reads_back() {
        let start = Utc.with_ymd_and_hms(2026, 7, 4, 10, 30, 0).unwrap();
        let key = (start, "0123456789abcdefghjkmnpqrs".to_owned());

        let token = page_token(&key);

        assert_eq!(token, "1783161000000000_0123456789abcdefghjkmnpqrs");
        assert_eq!(page_key(&token).unwrap(), key);
        // A flight from before 1970 has a negative start.
        let old = (
            Utc.with_ymd_and_hms(1969, 7, 20, 20, 17, 0).unwrap(),
            key.1.clone(),
        );
        assert_eq!(page_key(&page_token(&old)).unwrap(), old);

        for bad in [
            "",
            "_",
            "1783161000000000",
            "1783161000000000_",
            "1783161000000000_short",
            "yesterday_0123456789abcdefghjkmnpqrs",
            "1783161000000000_0123456789ABCDEFGHJKMNPQRS",
            "99999999999999999999999_0123456789abcdefghjkmnpqrs",
            "1783161000000000_0123456789abcdefghjkmnpqrs' OR '1'='1",
        ] {
            assert!(page_key(bad).is_err(), "{bad:?}");
        }
    }
}
