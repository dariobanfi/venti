//! Live tracks: taking in the fixes an app sends, the public tracks on a
//! part of the map, one track and its fixes, deleting a track, and the page
//! a link to a track shows.
//!
//! Who may see a track is decided by two things only. A track that its
//! pilot made public is listed; any track is shown to whoever asks for it
//! by its identifier, which only its pilot and those the pilot gave the
//! link to know. A track that is shared by link is therefore in no list
//! this module answers.
//!
//! Nothing here writes a position or a track's identifier to the log, and
//! no error message repeats either. The request log leaves the identifier
//! out of the path; see [`super::log`].

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use venti_api_types::v1::{
    ErrorCode, LivePositionsRequest, LivePositionsResponse, LiveTrackId, LiveTrackResponse,
    LiveTracksResponse, MAX_LIVE_FIXES, MAX_LIVE_TRACKS, live_page_path, parse_bbox, parse_time,
};
use venti_domain::Timestamp;
use venti_store::{LiveBatch, LiveRemoval, LiveRoom, LiveSaved, LiveTrack, LiveVisibility, Store};

use super::auth::Signed;
use super::cache::Keeper;
use super::error::ApiError;
use super::{Api, body, cache, wire};
use crate::accounts::throttle::{Counted, Throttle};
use crate::live::page::{self as live_page, TRAIL_FIXES};
use crate::live::{self, Limits};

/// What the handlers keep between requests: the limits, and how much each
/// account has used of them.
#[derive(Debug)]
pub(crate) struct Live {
    pub(crate) limits: Limits,
    /// Fixes sent, by account, within a minute.
    fixes: Throttle,
    /// Tracks begun, by account, within a day.
    tracks: Throttle,
}

impl Live {
    pub(crate) fn new(limits: Limits, watched_keys: usize) -> Self {
        Self {
            limits,
            fixes: Throttle::new(
                "accounts that sent live fixes",
                chrono::Duration::minutes(1),
                watched_keys,
            ),
            tracks: Throttle::new(
                "accounts that began live tracks",
                chrono::Duration::days(1),
                watched_keys,
            ),
        }
    }
}

/// The answer for an identifier that leads to no live track: none was ever
/// there, it was deleted, or it ended long enough ago to be gone. They
/// must not differ in anything.
fn no_such_track() -> ApiError {
    ApiError::missing(
        ErrorCode::LiveTrackNotFound,
        "there is no live track with this identifier",
    )
}

const ID_HELP: &str = "a live track's identifier is 26 characters, each drawn at random from \
                       0123456789abcdefghjkmnpqrstvwxyz";

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

/// Who may keep an answer about a track.
fn keeper(track: &LiveTrack) -> Keeper {
    match track.visibility {
        LiveVisibility::Public => Keeper::Anyone,
        LiveVisibility::Link => Keeper::OnlyTheAsker,
    }
}

impl<S: Store> Api<S> {
    /// The live track with this identifier, if there is one at `now`.
    ///
    /// A track that ended longer ago than tracks are kept is not there,
    /// whether or not the cleanup has removed it yet.
    async fn live_track(&self, id: &str, now: Timestamp) -> Result<LiveTrack, ApiError> {
        // Text of another form is the identifier of no track.
        let id: LiveTrackId = id.parse().map_err(|_| no_such_track())?;
        let track = self
            .store
            .live_track(id.as_str())
            .await?
            .ok_or_else(no_such_track)?;
        let still_there = track.latest.time >= self.live.limits.expired_before(now);
        still_there.then_some(track).ok_or_else(no_such_track)
    }
}

/// `POST /v1/live/positions`: fixes for one live track of the account.
pub(crate) async fn send<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: LivePositionsRequest = body::json(&headers, &body)?;
    let now = api.clock.now();
    let limits = &api.live.limits;
    let account_id = &signed.account.id;

    // Counted before anything is looked at or stored, whatever becomes of
    // the fixes: a client that sends rubbish is slowed like any other.
    let weight = u32::try_from(request.fixes.len().max(1)).unwrap_or(u32::MAX);
    let by_minute = [Counted::new(account_id, limits.fixes_per_minute)];
    api.live
        .fixes
        .spend(&by_minute, weight, now)
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this account is sending live fixes faster than the server takes them",
            )
        })?;

    let id: LiveTrackId = request
        .id
        .parse()
        .map_err(|_| ApiError::bad_request(ErrorCode::InvalidLiveTrackId, ID_HELP))?;
    let visibility = wire::stored_live_visibility(request.visibility).ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidVisibility,
            "the visibility of a live track must be one of link, public",
        )
    })?;
    let checked = live::check(&request.fixes, now, limits)
        .map_err(|bad| ApiError::bad_request(ErrorCode::InvalidFixes, bad.to_string()))?;
    if checked.fixes.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::FixesOutOfTime,
            format!(
                "none of the fixes has a time the server takes: {} lie in the future and {} \
                 are more than {} hours old by the server's clock, which shows {}; is the \
                 device's clock right?",
                checked.future,
                checked.old,
                limits.past.num_hours(),
                now.format("%Y-%m-%dT%H:%M:%SZ"),
            ),
        ));
    }

    // Whether the account may begin another track today is asked now and
    // counted once the store says that this request did begin one.
    let by_day = [Counted::new(account_id, limits.tracks_per_day)];
    let may_begin = api.live.tracks.check(&by_day, now);
    let batch = LiveBatch {
        id: id.as_str().to_owned(),
        account_id: account_id.clone(),
        visibility,
        landed: request.landed,
        fixes: checked.fixes,
    };
    let room = LiveRoom {
        fixes_per_track: limits.fixes_per_track,
        may_create: may_begin.is_ok(),
        expired_before: limits.expired_before(now),
        now,
    };
    let kept = match api.store.save_live_fixes(&batch, &room).await? {
        LiveSaved::Kept(kept) => *kept,
        LiveSaved::NotCreated => {
            return Err(match may_begin {
                Err(refused) => ApiError::rate_limited(
                    refused,
                    "this account has begun as many live tracks today as one account may",
                ),
                // The store was told that it may begin a track.
                Ok(()) => ApiError::internal(),
            });
        }
        LiveSaved::Gone => {
            return Err(ApiError::new(
                StatusCode::GONE,
                ErrorCode::LiveTrackGone,
                "the live track with this identifier was deleted or has been removed, and an \
                 identifier is used once; begin a new track with a new identifier",
            ));
        }
        LiveSaved::Taken => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::LiveTrackTaken,
                "this identifier belongs to a live track of another account; make a new one",
            ));
        }
    };
    if kept.created {
        api.live.tracks.count(&by_day, 1, now);
    }
    if kept.stored == 0 && kept.known == 0 && kept.full > 0 {
        return Err(ApiError::forbidden(
            ErrorCode::LiveTrackFull,
            format!(
                "the live track holds {} fixes, which is as many as one track may; begin a \
                 new track with a new identifier",
                limits.fixes_per_track
            ),
        ));
    }

    let answer = LivePositionsResponse {
        track: wire::live_track(&kept.track, limits.retention)?,
        page: format!("{}{}", api.public_url.as_str(), live_page_path(&id)),
        created: kept.created,
        stored: kept.stored,
        known: kept.known + checked.repeated,
        refused_future: checked.future,
        refused_old: checked.old,
        refused_full: kept.full,
    };
    let status = if kept.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok(cache::private_json(status, &answer))
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListParams {
    bbox: Option<String>,
}

/// `GET /v1/live?bbox=west,south,east,north`: the public tracks whose
/// newest fix lies in a box and is no older than half an hour.
///
/// The answer is the same whoever asks. It is checked before every reuse:
/// a track its pilot deletes, or no longer shows to everyone, leaves the
/// list at once.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> Result<Response, ApiError> {
    let bbox = params.bbox.ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidBbox,
            format!("the bbox parameter is missing; {BBOX_HELP}"),
        )
    })?;
    let area = parse_bbox(&bbox).map_err(|error| {
        ApiError::bad_request(ErrorCode::InvalidBbox, format!("{error}; {BBOX_HELP}"))
    })?;

    let now = api.clock.now();
    let limits = &api.live.limits;
    // No older than the list holds a track, and not gone for its age: the
    // first is the later moment on every server that removes tracks.
    let newest_since = (now - limits.listed).max(limits.expired_before(now));
    // One more than an answer lists, to know whether the box holds more.
    let mut found = api
        .store
        .live_tracks_in(&area, newest_since, MAX_LIVE_TRACKS + 1)
        .await?;
    let truncated = found.len() > MAX_LIVE_TRACKS;
    found.truncate(MAX_LIVE_TRACKS);
    let tracks = found
        .iter()
        .map(|track| wire::live_track(track, limits.retention))
        .collect::<Result<Vec<_>, _>>()?;

    // The validator covers the tracks and not the clock, which is in the
    // answer too: nothing has changed while no track has.
    let tag = cache::etag(&serde_json::to_vec(&(&tracks, truncated)).unwrap_or_default());
    if let Some(same) = cache::unchanged(&headers, &tag, Keeper::Anyone) {
        return Ok(same);
    }
    let answer = LiveTracksResponse {
        time: now,
        tracks,
        truncated,
    };
    Ok(cache::tagged_json(&tag, Keeper::Anyone, &answer))
}

#[derive(Debug, Deserialize)]
pub(crate) struct TrackParams {
    since: Option<String>,
    after: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/live/{id}`: one track and the fixes asked for. Anyone who
/// knows the identifier may ask, with or without an account, and the
/// answer is the same for everyone.
///
/// A client that follows a track asks every few seconds. The validator of
/// the answer is worked out from the track's own row and the request's
/// parameters, before any fix is read, so that "nothing has changed" costs
/// one small read.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    Query(params): Query<TrackParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let asked: i64 = match params.since.as_deref() {
        None => 0,
        Some(text) => text
            .parse()
            .ok()
            .filter(|since| *since >= 0)
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidCursor,
                    "since must be the cursor of an earlier answer about this track",
                )
            })?,
    };
    let after = params
        .after
        .as_deref()
        .map(|text| {
            parse_time(text).map_err(|error| {
                ApiError::bad_request(ErrorCode::InvalidTime, format!("after: {error}"))
            })
        })
        .transpose()?;
    let limit: usize = match params.limit.as_deref() {
        None => MAX_LIVE_FIXES,
        Some(text) => text
            .parse()
            .ok()
            .filter(|limit| (1..=MAX_LIVE_FIXES).contains(limit))
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidLimit,
                    format!("limit must be a number from 1 to {MAX_LIVE_FIXES}"),
                )
            })?,
    };

    let now = api.clock.now();
    let track = api.live_track(&id, now).await?;
    // A cursor from beyond the track's last fix was not given out for this
    // track as it is now. Start again from its first fix, and say so.
    let reset = asked > track.cursor;
    let since = if reset { 0 } else { asked };

    let summary = wire::live_track(&track, api.live.limits.retention)?;
    let after_s = after.map(|after| after.timestamp_micros());
    let state = (&summary, track.cursor, since, after_s, limit, reset);
    let tag = cache::etag(&serde_json::to_vec(&state).unwrap_or_default());
    let keeper = keeper(&track);
    if let Some(same) = cache::unchanged(&headers, &tag, keeper) {
        return Ok(same);
    }

    // One more than asked for, to know whether more are waiting.
    let mut fixes = api
        .store
        .live_fixes(&track.id, since, after, limit + 1)
        .await?;
    let more = fixes.len() > limit;
    fixes.truncate(limit);
    // Where the next answer begins: after the last fix of this one. With
    // nothing to hand out, after everything that was looked at, which is
    // at least what the track held when it was read above.
    let cursor = fixes
        .last()
        .map_or_else(|| since.max(track.cursor), |last| last.seq);
    let answer = LiveTrackResponse {
        time: now,
        track: summary,
        fixes: fixes.iter().map(|at| wire::live_fix(&at.fix)).collect(),
        cursor: cursor.to_string(),
        more,
        reset,
    };
    Ok(cache::tagged_json(&tag, keeper, &answer))
}

/// `DELETE /v1/live/{id}`: removes a live track of the account and takes
/// its identifier out of use.
pub(crate) async fn delete<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id: LiveTrackId = id.parse().map_err(|_| no_such_track())?;
    let now = api.clock.now();
    let account_id = &signed.account.id;
    // An identifier nothing is known of is taken out of use, so that fixes
    // still on their way begin nothing. That leaves a row behind, and is
    // counted like a track begun.
    let by_day = [Counted::new(account_id, api.live.limits.tracks_per_day)];
    let may_retire = api.live.tracks.check(&by_day, now);
    let removal = api
        .store
        .delete_live_track(account_id, id.as_str(), now, may_retire.is_ok())
        .await?;
    match removal {
        LiveRemoval::Removed | LiveRemoval::AlreadyRemoved => {}
        LiveRemoval::Retired => api.live.tracks.count(&by_day, 1, now),
        LiveRemoval::NotRetired => {
            return Err(match may_retire {
                Err(refused) => ApiError::rate_limited(
                    refused,
                    "this account has begun as many live tracks today as one account may, \
                     and nothing is known under this identifier",
                ),
                Ok(()) => ApiError::internal(),
            });
        }
        // A track of someone else is no track of this account, whoever
        // may see it.
        LiveRemoval::NotYours => return Err(no_such_track()),
    }
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

// --- The page a link opens ----------------------------------------------------

const HTML: &str = "text/html; charset=utf-8";

/// Dresses a page about a live track, or about none.
fn html(status: StatusCode, body: String) -> Response {
    let mut response = (status, body).into_response();
    cache::set(&mut response, header::CONTENT_TYPE, HTML);
    if let Ok(policy) = HeaderValue::from_str(&live_page::CONTENT_SECURITY_POLICY) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    // The page says how old a position is as of the moment it was made:
    // nothing may hand out a copy of it later.
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    cache::set(&mut response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // The link is the key to the track. It is not passed on to other
    // sites in the Referer header.
    cache::set(&mut response, header::REFERRER_POLICY, "same-origin");
    // Where a person is right now is nothing for a search engine, also
    // when everyone may see it.
    cache::set(
        &mut response,
        HeaderName::from_static("x-robots-tag"),
        "noindex",
    );
    response
}

#[derive(Debug, Deserialize)]
pub(crate) struct PageParams {
    renew: Option<String>,
}

/// `GET /live/{id}`: the page a link to a live track shows, for people
/// without the app. Not part of the versioned API: it answers HTML, errors
/// included.
pub(crate) async fn page<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    Query(params): Query<PageParams>,
) -> Response {
    let nothing_here = || html(StatusCode::NOT_FOUND, live_page::not_found_page());
    let try_again = || {
        let mut response = html(StatusCode::SERVICE_UNAVAILABLE, live_page::trouble_page());
        cache::set(&mut response, header::RETRY_AFTER, "30");
        response
    };

    let now = api.clock.now();
    let track = match api.live_track(&id, now).await {
        Ok(track) => track,
        Err(error) if error.is_missing() => return nothing_here(),
        Err(_) => return try_again(),
    };
    // The end of the track only: the page is made anew for every visitor
    // every few seconds.
    let trail = match api.store.live_tail(&track.id, TRAIL_FIXES).await {
        Ok(trail) => trail,
        Err(error) => {
            // Logged where store errors are, once in a while.
            let _ = ApiError::from(error);
            return try_again();
        }
    };
    let renew = params.renew.as_deref() != Some("off");
    let body = live_page::page(
        &track,
        &trail,
        now,
        renew,
        &api.public_url,
        &api.live.limits,
    );
    html(StatusCode::OK, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_kept_by_nobody_and_shown_to_no_search_engine() {
        let response = html(StatusCode::OK, "<!doctype html>".to_owned());
        let headers = response.headers();
        assert_eq!(headers[header::CONTENT_TYPE], HTML);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[header::REFERRER_POLICY], "same-origin");
        assert_eq!(headers["x-robots-tag"], "noindex");
        let policy = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(policy.starts_with("default-src 'none'; style-src 'sha256-"));
    }
}
