//! A pilot's picture, and the pilots who fly near a place.
//!
//! **A picture is the server's own copy.** What a pilot sends, and what
//! Google shows for the Google account they sign in with, is read and
//! written anew as a square WebP ([`crate::accounts::picture`]); the
//! original is not kept and nothing of its metadata goes along. A pilot's
//! page never makes its readers' browsers ask Google for anything.
//!
//! **Google's picture is taken once**: at the first sign-in with Google,
//! or the first linking of a Google account, that finds the account
//! without a picture. A pilot who removes it does not get it back.
//!
//! **A picture is public**: it stands on the pilot's page and beside every
//! flight they share. Its address names one picture for good, so every
//! cache may keep it; a new picture has a new address, and the old one is
//! gone with it.
//!
//! **The pilots near a place** are made of public flights only (owner
//! decision 16): who took off within the radius in the last
//! [`PILOTS_NEAR_MONTHS`] months, by their flights, never by where their
//! phone is or where they live. A pilot can leave the list with a setting
//! of the account ([`UpdateAccountRequest::shown_near_places`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, Months};
use serde::Deserialize;
use venti_api_types::v1::{
    AccountResponse, DEFAULT_PILOTS_NEAR, DEFAULT_PILOTS_NEAR_RADIUS_KM, ErrorCode,
    MAX_PILOTS_NEAR, MAX_PILOTS_NEAR_RADIUS_KM, NearPilot, PICTURE_MEDIA_TYPE, PICTURE_MEDIA_TYPES,
    PILOTS_NEAR_MONTHS, PILOTS_NEAR_TOP_SITES, PilotSeason, PilotsNearResponse, parse_position,
};
use venti_domain::GeoPoint;
use venti_store::{Account, Flown, Listing, NewPicture, PictureOrigin, Store};

use super::auth::OwnSession;
use super::budget::Reader;
use super::cache::Keeper;
use super::error::ApiError;
use super::pilots::{Placed, totals};
use super::{Api, cache, wire};
use crate::accounts::google::Identity;
use crate::accounts::secret::{self, random_id};
use crate::accounts::throttle::Counted;

/// The answer for a picture that is not there.
pub(crate) fn no_such_picture() -> ApiError {
    ApiError::missing(
        ErrorCode::PictureNotFound,
        "there is no picture with this identifier",
    )
}

fn invalid(why: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidPicture, why)
}

/// How long an answer about the pilots near a place is kept and handed out
/// again. They change with the public flights, a few a day in a valley.
const NEAR_KEPT: Duration = Duration::from_secs(10 * 60);

/// How many answers about pilots near a place are kept.
const NEAR_KEPT_ANSWERS: usize = 512;

/// The step a place asked about is taken to, in degrees: about a
/// kilometre. Places a few hundred metres apart are one answer, and the
/// server works with no place finer than this.
const NEAR_STEP_DEG: f64 = 0.01;

/// The answers about pilots near a place, as they were made last.
#[derive(Debug, Default)]
pub(crate) struct NearAnswers(Mutex<BTreeMap<NearKey, (Instant, PilotsNearResponse)>>);

/// What an answer about pilots near a place is kept by: the place on the
/// grid of [`NEAR_STEP_DEG`], the radius and the limit.
type NearKey = (i64, i64, u32, usize);

impl NearAnswers {
    fn get(&self, key: NearKey) -> Option<PilotsNearResponse> {
        let kept = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.get(&key)
            .filter(|(made, _)| made.elapsed() < NEAR_KEPT)
            .map(|(_, answer)| answer.clone())
    }

    fn keep(&self, key: NearKey, answer: &PilotsNearResponse) {
        let mut kept = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        kept.retain(|_, (made, _)| made.elapsed() < NEAR_KEPT);
        if kept.len() >= NEAR_KEPT_ANSWERS {
            return;
        }
        kept.insert(key, (Instant::now(), answer.clone()));
    }
}

impl<S: Store> Api<S> {
    /// The account as `GET /v1/account` shows it.
    async fn account_answer(&self, account: &Account) -> Result<Response, ApiError> {
        let google = self.store.google_link(&account.id).await?;
        let account = wire::account(account, google.as_ref())?;
        Ok(cache::private_json(
            StatusCode::OK,
            &AccountResponse { account },
        ))
    }

    /// The account as it is now, after a change to it.
    async fn account_now(&self, handle: &str) -> Result<Account, ApiError> {
        self.store
            .account(handle)
            .await?
            .ok_or_else(|| ApiError::unauthorized("the account is gone"))
    }

    /// Takes the picture of the account's Google account, if this is the
    /// one time to: the first sign-in or linking with Google that finds
    /// the account without a picture. Whatever goes wrong only means that
    /// the account has no picture; the sign-in goes on.
    pub(crate) async fn take_google_picture(&self, account: &Account, identity: &Identity) {
        let Some(address) = &identity.picture else {
            return;
        };
        let Some(google) = self.google.as_deref() else {
            return;
        };
        match self.store.claim_google_picture(&account.id).await {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                tracing::warn!(%error, "whether to take a picture from Google could not be noted");
                return;
            }
        }
        let made = async {
            let bytes = google.fetch_picture(address).await?;
            self.picture_turns
                .square(bytes)
                .await
                .map_err(|refused| refused.0)
        };
        let bytes = match made.await {
            Ok(bytes) => bytes,
            Err(why) => {
                tracing::info!(account = %account.handle, %why, "no picture taken from Google");
                return;
            }
        };
        let Ok(id) = random_id() else {
            return;
        };
        let new = NewPicture {
            id,
            account_id: account.id.clone(),
            bytes,
            origin: PictureOrigin::Google,
            created_at: self.clock.now(),
        };
        match self.store.set_picture(&new).await {
            Ok(_) => tracing::info!(account = %account.handle, "picture taken from Google"),
            Err(error) => tracing::warn!(%error, "a picture from Google could not be kept"),
        }
    }
}

/// `PUT /v1/account/picture`: the pilot's own picture, in place of the
/// one the account has. The body is the picture, a JPEG, a PNG or a WebP;
/// the answer is the account, with the new picture's address.
///
/// The pilot's own sign-in only: a picture stands on the pilot's public
/// page, and no scope of a token for a program opens that.
pub(crate) async fn put<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let media = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase());
    if !media.is_some_and(|media| PICTURE_MEDIA_TYPES.contains(&media.as_str())) {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ErrorCode::UnsupportedMediaType,
            format!(
                "send the picture as one of {}, and say so in Content-Type",
                PICTURE_MEDIA_TYPES.join(", ")
            ),
        ));
    }
    let account = &signed.account;
    if account.publishing_suspended_at.is_some() {
        return Err(ApiError::forbidden(
            ErrorCode::PublishingSuspended,
            "this account may publish nothing, and a picture is public",
        ));
    }
    let now = api.clock.now();
    // Counted before the picture is read: reading it is the expensive part.
    let by_day = [Counted::new(&account.id, api.limits.pictures_per_day)];
    api.picture_uploads
        .attempt(&by_day, now)
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this account has sent as many pictures today as one account may",
            )
        })?;
    let bytes = api
        .picture_turns
        .square(body.to_vec())
        .await
        .map_err(|refused| invalid(refused.0))?;
    let new = NewPicture {
        id: random_id()?,
        account_id: account.id.clone(),
        bytes,
        origin: PictureOrigin::Upload,
        created_at: now,
    };
    if !api.store.set_picture(&new).await? {
        return Err(ApiError::unauthorized("the account is gone"));
    }
    tracing::info!(account = %account.handle, "picture sent");
    let account = api.account_now(&account.handle).await?;
    api.account_answer(&account).await
}

/// `DELETE /v1/account/picture`: the account has no picture from now on.
/// The answer is the account, as for a picture that was not there.
pub(crate) async fn remove<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
) -> Result<Response, ApiError> {
    if api
        .store
        .remove_picture(&signed.account.id)
        .await?
        .is_some()
    {
        tracing::info!(account = %signed.account.handle, "picture removed");
    }
    let account = api.account_now(&signed.account.handle).await?;
    api.account_answer(&account).await
}

/// What a picture's address may be kept for: a year, and never asked
/// again. The address names one picture for good.
const PICTURE_CACHE: &str = "public, max-age=31536000, immutable";

/// `GET /v1/pictures/{id}`: a pilot's picture. Anyone may ask.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !secret::is_id(&id) {
        return Err(no_such_picture());
    }
    let tag = format!("\"{id}\"");
    if cache::client_has(&headers, &tag) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        cache::set(&mut response, header::CACHE_CONTROL, PICTURE_CACHE);
        if let Ok(value) = HeaderValue::from_str(&tag) {
            response.headers_mut().insert(header::ETAG, value);
        }
        return Ok(response);
    }
    let picture = api.store.picture(&id).await?.ok_or_else(no_such_picture)?;
    let mut response = (StatusCode::OK, picture.bytes).into_response();
    cache::set(&mut response, header::CONTENT_TYPE, PICTURE_MEDIA_TYPE);
    cache::set(&mut response, header::CACHE_CONTROL, PICTURE_CACHE);
    if let Ok(value) = HeaderValue::from_str(&tag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    Ok(response)
}

#[derive(Debug, Deserialize)]
pub(crate) struct NearParams {
    lat: Option<String>,
    lon: Option<String>,
    radius_km: Option<String>,
    limit: Option<String>,
}

/// A whole number from `low` to `high`, or the default.
fn bounded(
    text: Option<&str>,
    low: u64,
    high: u64,
    default: u64,
    what: &str,
) -> Result<u64, ApiError> {
    let Some(text) = text else {
        return Ok(default);
    };
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|value| (low..=high).contains(value))
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidRequest,
                format!("{what} is a whole number from {low} to {high}"),
            )
        })
}

/// `GET /v1/pilots/near?lat=&lon=&radius_km=&limit=`: the pilots whose
/// public flights took off near a place in the last months, the one with
/// the most first.
///
/// Open to anyone and counted as a read of public flights is. The place
/// is taken to [`NEAR_STEP_DEG`], and an answer is kept for
/// [`NEAR_KEPT`]: a pilot who leaves the list, or a flight made private,
/// is out of it within that time.
pub(crate) async fn near<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Query(params): Query<NearParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let place = parse_position(params.lat.as_deref(), params.lon.as_deref()).map_err(|error| {
        ApiError::bad_request(
            ErrorCode::InvalidPosition,
            format!(
                "{error}; give the place as lat and lon in degrees, for example lat=47.7&lon=11.8"
            ),
        )
    })?;
    let radius_km = bounded(
        params.radius_km.as_deref(),
        1,
        u64::from(MAX_PILOTS_NEAR_RADIUS_KM),
        u64::from(DEFAULT_PILOTS_NEAR_RADIUS_KM),
        "radius_km",
    )? as u32;
    let limit = bounded(
        params.limit.as_deref(),
        1,
        MAX_PILOTS_NEAR as u64,
        DEFAULT_PILOTS_NEAR as u64,
        "limit",
    )? as usize;
    let step = |degrees: f64| (degrees / NEAR_STEP_DEG).round() as i64;
    let key = (step(place.lat()), step(place.lon()), radius_km, limit);
    let answer = match api.near_answers.get(key) {
        Some(answer) => answer,
        None => {
            let at = GeoPoint::new(key.0 as f64 * NEAR_STEP_DEG, key.1 as f64 * NEAR_STEP_DEG)
                .unwrap_or(place);
            let answer = api.pilots_near(at, radius_km, limit).await?;
            api.near_answers.keep(key, &answer);
            answer
        }
    };
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

impl<S: Store> Api<S> {
    /// The pilots near a place, as `GET /v1/pilots/near` answers.
    pub(crate) async fn pilots_near(
        &self,
        place: GeoPoint,
        radius_km: u32,
        limit: usize,
    ) -> Result<PilotsNearResponse, ApiError> {
        let now = self.clock.now();
        let since = now
            .checked_sub_months(Months::new(PILOTS_NEAR_MONTHS))
            .unwrap_or(now);
        let within_m = f64::from(radius_km) * 1000.0;
        let found = self
            .store
            .pilots_near(place, within_m, since, limit)
            .await?;
        let mut pilots = Vec::with_capacity(found.len());
        for pilot in found {
            // Every public flight of the pilot, newest first: those from
            // near the place, and those of this year anywhere.
            let flown = self
                .store
                .flown_by(&pilot.account_id, Listing::Public)
                .await?;
            let near_or_this_year: Vec<Flown> = flown
                .into_iter()
                .filter(|flown| {
                    flown.start.year() == now.year() || is_near(flown, place, within_m, since)
                })
                .collect();
            let sites = self.sites_flown_from(&near_or_this_year).await?;
            let with_sites: Vec<(&Flown, Option<&Placed>)> = near_or_this_year
                .iter()
                .zip(&sites)
                .map(|(flown, site)| (flown, site.as_ref()))
                .collect();
            let near: Vec<(&Flown, Option<&Placed>)> = with_sites
                .iter()
                .copied()
                .filter(|(flown, _)| is_near(flown, place, within_m, since))
                .collect();
            let this_year: Vec<(&Flown, Option<&Placed>)> = with_sites
                .iter()
                .copied()
                .filter(|(flown, _)| flown.start.year() == now.year())
                .collect();
            let mut takeoffs = totals(&near)?.sites;
            takeoffs.truncate(PILOTS_NEAR_TOP_SITES);
            let season = if this_year.is_empty() {
                None
            } else {
                Some(PilotSeason {
                    year: now.year(),
                    totals: totals(&this_year)?,
                })
            };
            pilots.push(NearPilot {
                pilot: wire::pictured(&pilot.handle, &pilot.name, pilot.picture.as_deref())?,
                flights: pilot.flights,
                takeoffs,
                season,
            });
        }
        Ok(PilotsNearResponse {
            pilots,
            radius_km,
            months: PILOTS_NEAR_MONTHS,
        })
    }
}

/// Whether a flight took off within `within_m` metres of a place, at
/// `since` or later: as the store counts it.
fn is_near(flown: &Flown, place: GeoPoint, within_m: f64, since: venti_domain::Timestamp) -> bool {
    flown.start >= since
        && flown
            .takeoff
            .is_some_and(|takeoff| takeoff.distance_m(place) <= within_m)
}
