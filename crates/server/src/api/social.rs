//! Pilots among each other: following a pilot, the flights of the pilots
//! one follows, cheers and comments on public flights, and what a pilot
//! is told of them.
//!
//! **What is public.** That a pilot follows another, the cheers on a
//! public flight and the comments under one, each with the handle and
//! the name of the pilot who gave it, and the counts. The lists of who
//! follows a pilot and whom a pilot follows name only pilots who have a
//! page: an account that shares no flight is counted and not named, as
//! everywhere ([`super::pilots`]).
//!
//! **What is the account's own.** Its feed, the whole list of whom it
//! follows, and what it was told: asked with a session or with a token
//! that has `social:read`.
//!
//! **A flight that is not public shows no cheer and no comment** and
//! takes none: every request about them answers as for a flight nobody
//! has. They are kept while the flight is, and shown again when it is
//! public again.
//!
//! **Who writes.** A signed-in pilot whose publishing is not suspended:
//! a follow, a cheer and a comment each put the pilot's name where
//! everyone reads it. Taking one's own back is always allowed. A comment
//! is removed by its author or by the flight's pilot, and by whoever
//! runs the server ([`crate::moderation`]); it is reported like a plan's
//! words ([`super::reports`]).
//!
//! **What a pilot is told.** A new follow, a new cheer and a comment
//! each leave one line for the pilot they concern, never for the pilot
//! who did it, kept for [`NOTIFICATIONS_KEPT_DAYS`] days. Web Push for
//! them is not built.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration};
use serde::Deserialize;
use venti_api_types::v1::{
    Cheer, CheerResponse, CheersResponse, Comment, CommentRequest, CommentResponse, CommentsOpen,
    CommentsResponse, ErrorCode, FeedFlight, FeedResponse, FlightId, FlightTerms, FollowResponse,
    FollowedPilot, FollowsResponse, Handle, MAX_FOLLOWS, NOTIFICATIONS_KEPT_DAYS, Notification,
    NotificationKind, NotificationsResponse, NotifiedFlight, parse_comment,
};
use venti_domain::Timestamp;
use venti_store::{
    Account, Cheering, Commenting, Flight, FollowSide, Following, NewComment, SocialKey,
    SocialPilot, Store, Visibility,
};

use super::auth::{May, ReadSocial, Viewing, WriteFlights, WriteSocial};
use super::budget::Reader;
use super::cache::Keeper;
use super::error::ApiError;
use super::flights::{no_such_flight, page_key, page_token};
use super::log::Sparse;
use super::pilots::no_such_pilot;
use super::{Api, body, cache, wire};
use crate::accounts::secret::{is_id, random_id};
use crate::accounts::throttle::{Counted, Throttle};

fn no_such_comment() -> ApiError {
    ApiError::missing(
        ErrorCode::CommentNotFound,
        "the flight has no comment with this identifier that is this account's to remove",
    )
}

fn suspended() -> ApiError {
    ApiError::forbidden(
        ErrorCode::PublishingSuspended,
        "whoever runs this server has suspended this account's publishing: it follows nobody \
         new, cheers no flight and writes no comment; what it gave it can take back",
    )
}

/// Where the next page of a list here starts, as text for the client to
/// hand back: the moment of the last entry in microseconds and the
/// identifier that orders entries of one moment.
fn social_token(key: &SocialKey) -> String {
    format!("{}_{}", key.0.timestamp_micros(), key.1)
}

fn social_key(token: &str) -> Result<SocialKey, ApiError> {
    token
        .split_once('_')
        .and_then(|(micros, id)| {
            let at = DateTime::from_timestamp_micros(micros.parse().ok()?)?;
            is_id(id).then(|| (at, id.to_owned()))
        })
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidCursor,
                "after must be the next of an earlier answer",
            )
        })
}

/// Cuts a list read one beyond the page to the page, and says whether
/// more follows.
fn cut<T>(mut items: Vec<T>, limit: usize) -> (Vec<T>, bool) {
    let more = items.len() > limit;
    items.truncate(limit);
    (items, more)
}

fn followed(pilot: &SocialPilot) -> Result<FollowedPilot, ApiError> {
    Ok(FollowedPilot {
        pilot: wire::pictured(&pilot.handle, &pilot.name, pilot.picture.as_deref())?,
        since: pilot.at,
    })
}

fn comment_wire(comment: &venti_store::Comment) -> Result<Comment, ApiError> {
    Ok(Comment {
        id: comment.id.clone(),
        pilot: wire::pictured(
            &comment.author_handle,
            &comment.author_name,
            comment.author_picture.as_deref(),
        )?,
        text: comment.text.clone(),
        at: comment.created_at,
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct PageParams {
    after: Option<String>,
    limit: Option<String>,
}

impl<S: Store> Api<S> {
    /// The public flight with this address. A flight that is private,
    /// shared by link only or taken down is no flight here, to its pilot
    /// as to everyone.
    async fn flight_among_pilots(&self, id: &str) -> Result<Flight, ApiError> {
        let id: FlightId = id.parse().map_err(|_| no_such_flight())?;
        self.store
            .flight(id.as_str())
            .await?
            .filter(|flight| flight.visibility == Visibility::Public)
            .ok_or_else(no_such_flight)
    }

    /// The comment a report names, as its reporter may read it: under a
    /// public flight.
    pub(crate) async fn reported_comment(
        &self,
        flight: &str,
        comment: &str,
    ) -> Result<venti_store::Comment, ApiError> {
        let flight = self.flight_among_pilots(flight).await?;
        let missing = || {
            ApiError::missing(
                ErrorCode::CommentNotFound,
                "the flight has no comment with this identifier",
            )
        };
        if !is_id(comment) {
            return Err(missing());
        }
        self.store
            .comment(comment)
            .await?
            .filter(|found| found.flight_id == flight.id)
            .ok_or_else(missing)
    }

    /// Counts something an account does that puts its name before
    /// others, and refuses an account that may not publish.
    fn counted(
        &self,
        account: &Account,
        throttle: &Throttle,
        limit: u32,
        what: &str,
    ) -> Result<(), ApiError> {
        if account.publishing_suspended_at.is_some() {
            return Err(suspended());
        }
        throttle
            .attempt(&[Counted::new(&account.id, limit)], self.clock.now())
            .map_err(|refused| ApiError::rate_limited(refused, what))
    }

    /// Tells an account what a pilot did. That it could not be told
    /// undoes nothing: the follow, the cheer or the comment stands.
    async fn tell(
        &self,
        account_id: &str,
        by: &Account,
        kind: NotificationKind,
        flight: Option<&Flight>,
        comment: Option<&str>,
        now: Timestamp,
    ) {
        if account_id == by.id {
            return;
        }
        let told = async {
            let flight = match flight {
                Some(flight) => Some(NotifiedFlight {
                    id: flight.id.parse().map_err(|_| ApiError::internal())?,
                    start: flight.derived.start,
                    takeoff_site: self
                        .shown_flight(flight)
                        .await
                        .ok()
                        .and_then(|shown| shown.takeoff_site)
                        .map(|site| site.name),
                }),
                None => None,
            };
            let notification = Notification {
                kind,
                at: now,
                pilot: wire::pilot(&by.handle, &by.display_name)?,
                flight,
                comment: comment.map(str::to_owned),
            };
            let record = serde_json::to_string(&notification).map_err(|_| ApiError::internal())?;
            self.store
                .add_notice(account_id, &by.id, now, &record)
                .await?;
            Ok::<(), ApiError>(())
        };
        if told.await.is_err() {
            static GATE: Sparse = Sparse::new();
            GATE.error("a pilot could not be told what another did on their flight or page");
        }
    }

    /// One page of a pilot's followers or of whom they follow, as the
    /// public reads it.
    async fn follows_page(
        &self,
        account: &Account,
        side: FollowSide,
        with_a_page: bool,
        params: &PageParams,
        viewer: Option<&Account>,
    ) -> Result<FollowsResponse, ApiError> {
        let limit = body::page_size(params.limit.as_deref())?;
        let after = params.after.as_deref().map(social_key).transpose()?;
        let pilots = self
            .store
            .follows(&account.id, side, with_a_page, after.as_ref(), limit + 1)
            .await?;
        let (pilots, more) = cut(pilots, limit);
        let counts = self.store.follow_counts(&account.id).await?;
        let you_follow = match viewer {
            Some(viewer) => Some(self.store.follows_pilot(&viewer.id, &account.id).await?),
            None => None,
        };
        Ok(FollowsResponse {
            next: pilots
                .last()
                .filter(|_| more)
                .map(|last| social_token(&last.key())),
            pilots: pilots.iter().map(followed).collect::<Result<_, _>>()?,
            followers: counts.followers,
            following: counts.following,
            you_follow,
        })
    }
}

/// `PUT /v1/pilots/{handle}/follow`: the account follows the pilot.
/// Following a pilot one follows already changes nothing.
pub(crate) async fn follow<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    Path(handle): Path<String>,
) -> Result<Response, ApiError> {
    let pilot = api.pilot_with_a_page(&handle).await?;
    let me = &signed.account;
    if pilot.id == me.id {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidFollow,
            "a pilot does not follow themselves",
        ));
    }
    api.counted(
        me,
        &api.follows_and_cheers,
        api.limits.follows_and_cheers_per_day,
        "this account has followed pilots and cheered flights as often today as one account \
         may",
    )?;
    let now = api.clock.now();
    match api
        .store
        .follow(&me.id, &pilot.id, now, MAX_FOLLOWS)
        .await?
    {
        Following::Follows { new: true } => {
            api.tell(&pilot.id, me, NotificationKind::Followed, None, None, now)
                .await;
        }
        Following::Follows { new: false } => {}
        Following::Full => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::TooManyFollows,
                format!(
                    "an account follows at most {MAX_FOLLOWS} pilots; stop following one first"
                ),
            ));
        }
        // Gone between the look and the write.
        Following::NoAccount => return Err(no_such_pilot()),
    }
    let answer = FollowResponse {
        following: true,
        followers: api.store.follow_counts(&pilot.id).await?.followers,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/pilots/{handle}/follow`: the account follows the pilot no
/// more. Asked of a pilot it does not follow, nothing changes.
pub(crate) async fn unfollow<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    Path(handle): Path<String>,
) -> Result<Response, ApiError> {
    // A pilot who shares nothing by now can still be let go of; a handle
    // the account never followed and that leads to no page is nobody's.
    let handle = Handle::new(&handle).map_err(|_| no_such_pilot())?;
    let pilot = api.store.account(handle.as_str()).await?;
    let pilot = pilot.ok_or_else(no_such_pilot)?;
    let was = api.store.unfollow(&signed.account.id, &pilot.id).await?;
    if !was {
        api.pilot_with_a_page(handle.as_str()).await?;
    }
    let answer = FollowResponse {
        following: false,
        followers: api.store.follow_counts(&pilot.id).await?.followers,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// A pilot's public list of one side of following.
async fn public_follows<S: Store>(
    api: &Api<S>,
    reader: &Reader,
    viewer: Option<&Account>,
    handle: &str,
    side: FollowSide,
    params: &PageParams,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(reader)?;
    let pilot = api.pilot_with_a_page(handle).await?;
    let answer = api.follows_page(&pilot, side, true, params, viewer).await?;
    Ok(match viewer {
        Some(_) => cache::private_json(StatusCode::OK, &answer),
        None => cache::shared_json(headers, &answer, Keeper::Anyone),
    })
}

/// `GET /v1/pilots/{handle}/followers`: who follows the pilot, the newest
/// first, and how many do.
pub(crate) async fn followers<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Viewing(viewer, ..): Viewing<ReadSocial>,
    Path(handle): Path<String>,
    Query(params): Query<PageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let side = FollowSide::Followers;
    public_follows(
        &api,
        &reader,
        viewer.account(),
        &handle,
        side,
        &params,
        &headers,
    )
    .await
}

/// `GET /v1/pilots/{handle}/following`: whom the pilot follows, the
/// newest first, and how many.
pub(crate) async fn following<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Viewing(viewer, ..): Viewing<ReadSocial>,
    Path(handle): Path<String>,
    Query(params): Query<PageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let side = FollowSide::Following;
    public_follows(
        &api,
        &reader,
        viewer.account(),
        &handle,
        side,
        &params,
        &headers,
    )
    .await
}

/// `GET /v1/account/following`: everyone the account follows, those
/// without a page among them.
pub(crate) async fn own_following<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadSocial>,
    Query(params): Query<PageParams>,
) -> Result<Response, ApiError> {
    let side = FollowSide::Following;
    let answer = api
        .follows_page(&signed.account, side, false, &params, None)
        .await?;
    Ok(cache::private_json(StatusCode::OK, &answer))
}

#[derive(Debug, Deserialize)]
pub(crate) struct FeedParams {
    before: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/account/feed`: the public flights of the pilots the account
/// follows, the newest first, each with what pilots made of it.
pub(crate) async fn feed<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadSocial>,
    Query(params): Query<FeedParams>,
) -> Result<Response, ApiError> {
    let limit = body::page_size(params.limit.as_deref())?;
    let before = params.before.as_deref().map(page_key).transpose()?;
    let flights = api
        .store
        .followed_flights(&signed.account.id, before.as_ref(), limit + 1)
        .await?;
    // As in the public list: a mistake in the store would show a flight
    // to someone who may not see it.
    if flights
        .iter()
        .any(|flight| flight.visibility != Visibility::Public)
    {
        static GATE: Sparse = Sparse::new();
        GATE.error("the store listed a flight that is not public in a feed");
        return Err(ApiError::internal());
    }
    let (flights, more) = cut(flights, limit);
    let next = flights
        .last()
        .filter(|_| more)
        .map(|last| page_token(&last.key()));
    let ids: Vec<String> = flights.iter().map(|flight| flight.id.clone()).collect();
    let social = api
        .store
        .flight_social(&ids, Some(&signed.account.id))
        .await?;
    let shown = api.shown_flights(&flights).await?;
    let answer = FeedResponse {
        flights: shown
            .into_iter()
            .zip(social)
            .map(|(flight, social)| FeedFlight {
                flight,
                cheers: social.cheers,
                comments: social.comments,
                cheered: social.cheered,
            })
            .collect(),
        next,
        terms: FlightTerms::current(),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `PUT /v1/flights/{id}/cheer`: the account cheers a public flight of
/// another pilot. Cheering it again changes nothing.
pub(crate) async fn cheer<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let flight = api.flight_among_pilots(&id).await?;
    let me = &signed.account;
    if flight.account_id == me.id {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidCheer,
            "a pilot does not cheer their own flight",
        ));
    }
    api.counted(
        me,
        &api.follows_and_cheers,
        api.limits.follows_and_cheers_per_day,
        "this account has followed pilots and cheered flights as often today as one account \
         may",
    )?;
    let now = api.clock.now();
    match api.store.cheer(&me.id, &flight.id, now).await? {
        Cheering::Cheers { new: true } => {
            let kind = NotificationKind::Cheered;
            api.tell(&flight.account_id, me, kind, Some(&flight), None, now)
                .await;
        }
        Cheering::Cheers { new: false } => {}
        Cheering::NoSuchFlight => return Err(no_such_flight()),
    }
    let social = api
        .store
        .flight_social(std::slice::from_ref(&flight.id), None)
        .await?;
    let answer = CheerResponse {
        cheered: true,
        cheers: social.first().map_or(0, |social| social.cheers),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/flights/{id}/cheer`: takes the account's cheer back.
pub(crate) async fn uncheer<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id: FlightId = id.parse().map_err(|_| no_such_flight())?;
    // A cheer on a flight that is public no more is still the account's
    // to take back; the answer then says nothing about the flight.
    let was = api.store.uncheer(&signed.account.id, id.as_str()).await?;
    let public = api.flight_among_pilots(id.as_str()).await;
    let cheers = match public {
        Ok(flight) => api
            .store
            .flight_social(std::slice::from_ref(&flight.id), None)
            .await?
            .first()
            .map_or(0, |social| social.cheers),
        Err(_) if was => 0,
        Err(missing) => return Err(missing),
    };
    let answer = CheerResponse {
        cheered: false,
        cheers,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `GET /v1/flights/{id}/cheers`: who cheered a public flight, the newest
/// first, and how many did.
pub(crate) async fn cheers<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Viewing(viewer, ..): Viewing<ReadSocial>,
    Path(id): Path<String>,
    Query(params): Query<PageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let flight = api.flight_among_pilots(&id).await?;
    let limit = body::page_size(params.limit.as_deref())?;
    let after = params.after.as_deref().map(social_key).transpose()?;
    let page = api
        .store
        .cheers(&flight.id, after.as_ref(), limit + 1)
        .await?;
    let (page, more) = cut(page, limit);
    let viewer = viewer.account();
    let social = api
        .store
        .flight_social(
            std::slice::from_ref(&flight.id),
            viewer.map(|account| account.id.as_str()),
        )
        .await?;
    let social = social.first().ok_or_else(ApiError::internal)?;
    let answer = CheersResponse {
        count: social.cheers,
        cheered: viewer.map(|_| social.cheered),
        next: page
            .last()
            .filter(|_| more)
            .map(|last| social_token(&last.key())),
        cheers: page
            .iter()
            .map(|pilot| {
                Ok(Cheer {
                    pilot: wire::pictured(&pilot.handle, &pilot.name, pilot.picture.as_deref())?,
                    at: pilot.at,
                })
            })
            .collect::<Result<_, ApiError>>()?,
    };
    Ok(match viewer {
        Some(_) => cache::private_json(StatusCode::OK, &answer),
        None => cache::shared_json(&headers, &answer, Keeper::Anyone),
    })
}

/// `GET /v1/flights/{id}/comments`: the comments on a public flight, the
/// oldest first. The same for everyone.
pub(crate) async fn comments<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Path(id): Path<String>,
    Query(params): Query<PageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let flight = api.flight_among_pilots(&id).await?;
    let limit = body::page_size(params.limit.as_deref())?;
    let after = params.after.as_deref().map(social_key).transpose()?;
    let page = api
        .store
        .comments(&flight.id, after.as_ref(), limit + 1)
        .await?;
    let (page, more) = cut(page, limit);
    let social = api
        .store
        .flight_social(std::slice::from_ref(&flight.id), None)
        .await?;
    let social = social.first().ok_or_else(ApiError::internal)?;
    let answer = CommentsResponse {
        count: social.comments,
        open: !social.comments_closed,
        next: page
            .last()
            .filter(|_| more)
            .map(|last| social_token(&last.key())),
        comments: page.iter().map(comment_wire).collect::<Result<_, _>>()?,
    };
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

/// `POST /v1/flights/{id}/comments`: writes a comment under a public
/// flight.
pub(crate) async fn add_comment<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    // Asked before anything is counted, as for a report.
    body::must_say_json(&headers)?;
    let flight = api.flight_among_pilots(&id).await?;
    let me = &signed.account;
    let request: CommentRequest = body::json(&headers, &body)?;
    let text = parse_comment(&request.text)
        .map_err(|error| ApiError::bad_request(ErrorCode::InvalidComment, error.to_string()))?;
    api.counted(
        me,
        &api.comments,
        api.limits.comments_per_day,
        "this account has written as many comments today as one account may",
    )?;
    let now = api.clock.now();
    let new = NewComment {
        id: random_id()?,
        flight_id: flight.id.clone(),
        author_id: me.id.clone(),
        text,
        created_at: now,
    };
    let comment = match api.store.add_comment(&new).await? {
        Commenting::Added(comment) => comment,
        Commenting::Closed => {
            return Err(ApiError::forbidden(
                ErrorCode::CommentsOff,
                "the flight's pilot takes no comments on this flight",
            ));
        }
        Commenting::NoSuchFlight => return Err(no_such_flight()),
    };
    let kind = NotificationKind::Commented;
    api.tell(
        &flight.account_id,
        me,
        kind,
        Some(&flight),
        Some(&comment.id),
        now,
    )
    .await;
    let social = api
        .store
        .flight_social(std::slice::from_ref(&flight.id), None)
        .await?;
    let answer = CommentResponse {
        comment: comment_wire(&comment)?,
        count: social.first().map_or(1, |social| social.comments),
    };
    Ok(cache::private_json(StatusCode::CREATED, &answer))
}

/// `PATCH /v1/flights/{id}/comments`: the flight's pilot says whether the
/// flight takes comments. What was written stays.
pub(crate) async fn open_comments<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteFlights>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: CommentsOpen = body::json(&headers, &body)?;
    let id: FlightId = id.parse().map_err(|_| no_such_flight())?;
    let set = api
        .store
        .close_comments(
            &signed.account.id,
            id.as_str(),
            !request.open,
            api.clock.now(),
        )
        .await?;
    if !set {
        // A flight of someone else is no flight of this account.
        return Err(no_such_flight());
    }
    Ok(cache::private_json(StatusCode::OK, &request))
}

/// `DELETE /v1/flights/{id}/comments/{comment}`: removes a comment. Its
/// author may, and the flight's pilot.
pub(crate) async fn remove_comment<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSocial>,
    Path((id, comment)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let id: FlightId = id.parse().map_err(|_| no_such_flight())?;
    if !is_id(&comment) {
        return Err(no_such_comment());
    }
    let found = api
        .store
        .comment(&comment)
        .await?
        .filter(|found| found.flight_id == id.as_str())
        .ok_or_else(no_such_comment)?;
    let me = &signed.account.id;
    let mine = found.author_id == *me;
    let on_my_flight = api
        .store
        .flight(id.as_str())
        .await?
        .is_some_and(|flight| flight.account_id == *me);
    if !mine && !on_my_flight {
        // Somebody else's comment on somebody else's flight: nothing
        // this account can remove, whether or not it can read it.
        return Err(no_such_comment());
    }
    api.store.remove_comment(&found.id).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SinceParams {
    since: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/account/notifications`: what the account was told since the
/// device last asked, of the last [`NOTIFICATIONS_KEPT_DAYS`] days.
pub(crate) async fn notifications<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadSocial>,
    Query(params): Query<SinceParams>,
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
    let mut told = api.store.notices(account_id, asked, limit).await?;
    // A cursor from beyond the latest was not given out by this database
    // as it is now: start again, and say so.
    let reset = asked > told.latest;
    let since = if reset { 0 } else { asked };
    if reset {
        told = api.store.notices(account_id, 0, limit).await?;
    }
    let kept_from = api.clock.now() - Duration::days(NOTIFICATIONS_KEPT_DAYS);
    let cursor = told.notices.last().map_or(since, |last| last.number);
    let answer = NotificationsResponse {
        notifications: told
            .notices
            .iter()
            .filter(|notice| notice.at >= kept_from)
            // The text is the server's own writing.
            .map(|notice| serde_json::from_str(&notice.record).map_err(|_| ApiError::internal()))
            .collect::<Result<_, _>>()?,
        cursor: cursor.to_string(),
        more: told.more,
        reset,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_for_a_page_reads_back_and_nothing_else_does() {
        let at = DateTime::from_timestamp_micros(1_795_000_000_123_456).unwrap();
        let key = (at, "0123456789abcdefghjkmnpqrs".to_owned());
        assert_eq!(
            social_token(&key),
            "1795000000123456_0123456789abcdefghjkmnpqrs"
        );
        assert_eq!(social_key(&social_token(&key)).unwrap(), key);
        for bad in ["", "_", "1_short", "x_0123456789abcdefghjkmnpqrs", "1795"] {
            assert!(social_key(bad).is_err(), "{bad:?}");
        }
    }
}
