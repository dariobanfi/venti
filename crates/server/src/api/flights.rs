//! A pilot's flights: uploading, listing, what changed, one flight and its
//! file, who may see it, deleting.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::DateTime;
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::{Semaphore, SemaphorePermit};
use venti_api_types::v1::{
    DeletedFlight, ErrorCode, FlightChangesResponse, FlightId, FlightResponse, FlightTerms,
    FlightsResponse, MAX_FLIGHT_FILE_BYTES, UpdateFlightRequest, UpdateFlightsRequest,
    UpdateFlightsResponse, UploadResponse,
};
use venti_store::{
    AllVisibilityChange, Flight, FlightKey, Kept, Listing, Room, Saved, Store, Visibility,
    VisibilityChange,
};

use super::auth::{May, ReadFlights, Viewer, Viewing, WriteFlights};
use super::budget::Reader;
use super::cache::Keeper;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, body, cache, wire};
use crate::accounts::throttle::{Counted, Refused};
use crate::flights::analyser::Failure;
use crate::flights::{new_flight, track as track_of, unpack};

/// The answer for a flight that does not exist and for one the caller may
/// not see. They must not differ in anything.
pub(crate) fn no_such_flight() -> ApiError {
    ApiError::missing(
        ErrorCode::FlightNotFound,
        "there is no flight with this address that you may see",
    )
}

/// The answer to an upload by an account that has no room for it.
fn account_full(kept: &Kept, room: &Room) -> ApiError {
    let megabytes = |bytes: u64| bytes.div_ceil(1024 * 1024);
    ApiError::forbidden(
        ErrorCode::AccountFull,
        format!(
            "the account keeps {} flights with {} MB of files, and this server gives an \
             account room for {} flights and {} MB; delete flights to make room",
            kept.flights,
            megabytes(kept.file_bytes),
            room.flights,
            megabytes(room.file_bytes),
        ),
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
        let may_see =
            flight.visibility != Visibility::Private || viewer.owns(&flight.account_id)?;
        may_see.then_some(flight).ok_or_else(no_such_flight)
    }
}

impl<S: Store> Api<S> {
    /// A public flight by its address, as `GET /v1/flights/{id}` answers it
    /// to someone without a token, for the connector's `public_flight`. A
    /// flight shared by link only is no public flight here: the connector
    /// offers what is public, and a link is its pilot's to hand out.
    pub(crate) async fn public_flight(&self, id: &str) -> Result<FlightResponse, ApiError> {
        let flight = self.visible_flight(id, &Viewer::nobody()).await?;
        if flight.visibility != Visibility::Public {
            return Err(no_such_flight());
        }
        Ok(FlightResponse {
            flight: self.shown_flight(&flight).await?,
            terms: terms_of(&flight),
        })
    }
}

/// Under what a flight is given to whoever reads it: said for a flight
/// that others than its pilot can read, and left out for a private one.
fn terms_of(flight: &Flight) -> Option<FlightTerms> {
    match flight.visibility {
        Visibility::Public | Visibility::Unlisted => Some(FlightTerms::current()),
        Visibility::Private => None,
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

/// How long a flight file may take to arrive. A file sent over a weak
/// mobile connection needs time: this lets the largest one through at
/// 40 kB a second, and a usual one of a few hundred kilobytes at a few.
const ARRIVAL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(100);

/// How long a file that is arriving may stand still. A connection that
/// sends nothing for this long has gone, or belongs to someone who is
/// holding a place and nothing else: four accounts that sent nine bytes
/// each and then nothing held every place for two minutes (the security
/// review of 2026-11-19).
const ARRIVAL_PAUSE: std::time::Duration = std::time::Duration::from_secs(10);

/// How a file failed to arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotArrived {
    /// Nothing came for [`ARRIVAL_PAUSE`], or not all of it within
    /// [`ARRIVAL_DEADLINE`].
    TooSlow,
    /// It is larger than a file may be, or the connection broke.
    TooLargeOrBroken,
}

/// Takes in a request's body piece by piece, within the time a file gets.
async fn arrival(body: Body) -> Result<Vec<u8>, NotArrived> {
    let mut pieces = body.into_data_stream();
    let reading = async {
        let mut file = Vec::new();
        loop {
            let piece = tokio::time::timeout(ARRIVAL_PAUSE, pieces.next())
                .await
                .map_err(|_| NotArrived::TooSlow)?;
            match piece {
                None => return Ok(file),
                // The layer around the handler ends a body that is too
                // large with an error, as a broken connection does.
                Some(Err(_)) => return Err(NotArrived::TooLargeOrBroken),
                Some(Ok(piece)) => {
                    if file.len() + piece.len() > MAX_UPLOAD_BYTES {
                        return Err(NotArrived::TooLargeOrBroken);
                    }
                    file.extend_from_slice(&piece);
                }
            }
        }
    };
    tokio::time::timeout(ARRIVAL_DEADLINE, reading)
        .await
        .unwrap_or(Err(NotArrived::TooSlow))
}

/// How long an analysis may take without counting for anything. A real
/// flight never takes this long: twelve hours at a fix a second are
/// analysed in about half a second (measured 2026-11-19).
const ANALYSIS_FREE: std::time::Duration = std::time::Duration::from_secs(1);

/// What an analysis that took `took` counts for among an account's uploads
/// of the day, beside the one every upload counts for. The first second is
/// free; the time beyond it counts in proportion, so that an analysis
/// that used all the time allowed counts for `slow_upload_weight`.
///
/// Without this only a file that had to be killed cost anything, and a
/// file built to take nine of the ten seconds allowed cost nothing: two
/// accounts could keep both turns at analysing busy all day.
fn analysis_weight(
    took: std::time::Duration,
    allowed: std::time::Duration,
    slow_upload_weight: u32,
) -> u32 {
    let beyond = took.saturating_sub(ANALYSIS_FREE).as_millis();
    let span = allowed.saturating_sub(ANALYSIS_FREE).as_millis().max(1);
    let weight = u128::from(slow_upload_weight) * beyond / span;
    u32::try_from(weight)
        .unwrap_or(u32::MAX)
        .min(slow_upload_weight)
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
    May(signed, ..): May<WriteFlights>,
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
    let file = match arrival(body).await {
        Ok(file) => file,
        Err(NotArrived::TooSlow) => {
            // The place was held for nothing. That is tolerated a few
            // times a day, like a file that keeps the analysis busy.
            api.uploads
                .count(&by_account, api.limits.slow_upload_weight, now);
            return Err(ApiError::new(
                StatusCode::REQUEST_TIMEOUT,
                ErrorCode::Timeout,
                format!(
                    "the file did not arrive: nothing came for {} seconds, or not all of it \
                     within {}",
                    ARRIVAL_PAUSE.as_secs(),
                    ARRIVAL_DEADLINE.as_secs()
                ),
            ));
        }
        Err(NotArrived::TooLargeOrBroken) => {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorCode::PayloadTooLarge,
                format!(
                    "the file is larger than {} MB, or its upload broke off",
                    MAX_UPLOAD_BYTES / (1024 * 1024)
                ),
            ));
        }
    };
    if file.is_empty() {
        return Err(unusable(
            "the request has no body; send the IGC flight file as the body",
        ));
    }

    let file: Arc<[u8]> = file.into();
    let (analysed, took) = api.analyser.analyse_timed(file.clone()).await;
    let slow_upload_weight = api.limits.slow_upload_weight;
    let derivation = match analysed {
        Ok(derivation) => {
            // The time the analysis took is the account's, also when all
            // went well.
            let weight = analysis_weight(took, api.analyser.time_allowed(), slow_upload_weight);
            api.uploads.count(&by_account, weight, now);
            derivation
        }
        Err(Failure::Unusable(reason)) => {
            let weight = analysis_weight(took, api.analyser.time_allowed(), slow_upload_weight);
            api.uploads.count(&by_account, weight, now);
            return Err(unusable(format!("the file cannot be used: {reason}")));
        }
        Err(failure @ (Failure::TooSlow | Failure::Crashed)) => {
            // Such a file cost the server all the time it allows. Only a
            // few a day are tolerated from one account.
            api.uploads.count(&by_account, slow_upload_weight, now);
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
    let (flight, created) = match api.store.save_flight(&new, &api.room).await? {
        Saved::Created(flight) => (flight, true),
        Saved::AlreadyThere(flight) => (flight, false),
        Saved::NoRoom(kept) => return Err(account_full(&kept, &api.room)),
    };

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let answer = UploadResponse {
        flight: api.shown_flight(&flight).await?,
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
pub(crate) fn page_token(key: &FlightKey) -> String {
    format!("{}_{}", key.0.timestamp_micros(), key.1)
}

pub(crate) fn page_key(token: &str) -> Result<FlightKey, ApiError> {
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
    May(signed, ..): May<ReadFlights>,
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
        flights: api.shown_flights(&flights).await?,
        next,
        // The account's own flights: nothing to say to their pilot.
        terms: None,
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
    May(signed, ..): May<ReadFlights>,
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
        flights: api.shown_flights(&changes.flights).await?,
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
    Viewing(viewer, ..): Viewing<ReadFlights>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let flight = api.visible_flight(&id, &viewer).await?;
    let answer = FlightResponse {
        flight: api.shown_flight(&flight).await?,
        terms: terms_of(&flight),
    };
    Ok(if viewer.is_signed() {
        // Asked with a token: made for that account, kept by nobody.
        cache::private_json(StatusCode::OK, &answer)
    } else {
        cache::shared_json(&headers, &answer, keeper(&flight))
    })
}

/// `GET /v1/flights/{id}/igc`: the flight file, exactly as it was uploaded.
pub(crate) async fn file<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Viewing(viewer, ..): Viewing<ReadFlights>,
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

    let mut response = if viewer.is_signed() {
        cache::private(file.into_response())
    } else {
        cache::shared(&headers, "application/octet-stream", file, keeper(&flight))
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

/// `GET /v1/flights/{id}/track`: the flight's track as GeoJSON, for
/// whoever may see the flight. See [`crate::flights::track`].
///
/// Open to anyone for a flight that is not private, so it is counted for
/// the address that asks, as the lists of public flights are: the track
/// is read from the file for every answer.
pub(crate) async fn track<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Viewing(viewer, ..): Viewing<ReadFlights>,
    reader: Reader,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let flight = api.visible_flight(&id, &viewer).await?;
    let address: FlightId = flight.id.parse().map_err(|_| ApiError::internal())?;
    // The flight can be deleted between the two questions.
    let packed = api
        .store
        .flight_file(&flight.id)
        .await?
        .ok_or_else(no_such_flight)?;
    let derived = flight.derived.clone();
    let terms = terms_of(&flight);
    // Unpacking, reading and writing a long flight is work: off the
    // threads that answer.
    let body = tokio::task::spawn_blocking(move || {
        let file = unpack(&packed)?;
        let mut track = track_of::track(address, &file, &derived)?;
        track.terms = terms;
        serde_json::to_vec(&track).ok()
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| {
        static GATE: Sparse = Sparse::new();
        GATE.error(format_args!(
            "the stored file of flight {} gives no track",
            flight.id
        ));
        ApiError::internal()
    })?;

    const GEO_JSON: &str = "application/geo+json";
    let mut response = if viewer.is_signed() {
        cache::private(body.into_response())
    } else {
        cache::shared(&headers, GEO_JSON, body, keeper(&flight))
    };
    cache::set(&mut response, header::CONTENT_TYPE, GEO_JSON);
    Ok(response)
}

/// `PATCH /v1/flights/{id}`: changes who may see a flight.
pub(crate) async fn update<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteFlights>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: UpdateFlightRequest = body::json(&headers, &body)?;
    let visibility = wire::stored_visibility(request.visibility).ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidVisibility,
            "visibility must be one of private, unlisted, public",
        )
    })?;
    let id: FlightId = id.parse().map_err(|_| no_such_flight())?;

    let change = api
        .store
        .set_visibility(&signed.account.id, id.as_str(), visibility, api.clock.now())
        .await?;
    let flight = match change {
        VisibilityChange::Set(flight) => *flight,
        // A flight of someone else is no flight of this account, whoever
        // may see it.
        VisibilityChange::NoSuchFlight => return Err(no_such_flight()),
        // What whoever runs the server did, only they undo. The pilot is
        // told that it was them, and not why: the server does not know.
        VisibilityChange::TakenDown => {
            return Err(ApiError::forbidden(
                ErrorCode::FlightTakenDown,
                "whoever runs this server has taken the flight down; it stays private until \
                 they release it",
            ));
        }
        VisibilityChange::PublishingSuspended => {
            return Err(ApiError::forbidden(
                ErrorCode::PublishingSuspended,
                "whoever runs this server has suspended this account's publishing: it can \
                 make no flight public; a flight can still be shared with those who have its \
                 link",
            ));
        }
    };
    let answer = FlightResponse {
        flight: api.shown_flight(&flight).await?,
        terms: terms_of(&flight),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `PATCH /v1/flights`: makes every flight of the account public, or
/// every one private, by the rules of one flight's change.
pub(crate) async fn update_all<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteFlights>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: UpdateFlightsRequest = body::json(&headers, &body)?;
    let visibility = match wire::stored_visibility(request.visibility) {
        Some(visibility @ (Visibility::Public | Visibility::Private)) => visibility,
        Some(Visibility::Unlisted) | None => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidVisibility,
                "visibility must be public or private: every flight of the account is set to it",
            ));
        }
    };
    // Counted before the store is asked, whatever becomes of it: each time
    // can change every flight of the account.
    let now = api.clock.now();
    let by_day = [Counted::new(
        &signed.account.id,
        api.limits.all_flights_changes_per_day,
    )];
    api.all_flights_changes
        .attempt(&by_day, now)
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this account has set all its flights at once as often today as one account may",
            )
        })?;
    let change = api
        .store
        .set_all_visibility(&signed.account.id, visibility, now)
        .await?;
    let set = match change {
        AllVisibilityChange::Set(set) => set,
        // The account can go between the token's check and this.
        AllVisibilityChange::NoSuchAccount => {
            return Err(ApiError::unauthorized("the account is gone"));
        }
        AllVisibilityChange::PublishingSuspended => {
            return Err(ApiError::forbidden(
                ErrorCode::PublishingSuspended,
                "whoever runs this server has suspended this account's publishing: it can \
                 make no flight public; nothing was changed",
            ));
        }
    };
    let count = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    let answer = UpdateFlightsResponse {
        visibility: wire::visibility(visibility),
        flights: count(set.flights),
        changed: count(set.changed),
        taken_down: count(set.taken_down),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/flights/{id}`: deletes a flight and its file.
pub(crate) async fn delete<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteFlights>,
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
    fn an_analysis_counts_for_the_share_of_the_time_allowed_that_it_took() {
        use std::time::Duration;
        let allowed = Duration::from_secs(10);
        let weight = |millis: u64| analysis_weight(Duration::from_millis(millis), allowed, 50);

        // A real flight takes a fraction of a second, and a slow machine
        // some more: nothing beside the one every upload counts for.
        assert_eq!(weight(0), 0);
        assert_eq!(weight(150), 0);
        assert_eq!(weight(1000), 0);
        // A file that keeps the analysis busy counts for the time beyond
        // the first second.
        assert_eq!(weight(1900), 5);
        assert_eq!(weight(5500), 25);
        assert_eq!(weight(9100), 45);
        // All the time allowed counts as a file that had to be killed, and
        // nothing counts for more.
        assert_eq!(weight(10_000), 50);
        assert_eq!(weight(60_000), 50);
        // Whatever the settings, the sum does not overflow or divide by
        // nothing.
        assert_eq!(
            analysis_weight(Duration::from_secs(5), Duration::ZERO, u32::MAX),
            u32::MAX
        );
    }

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
