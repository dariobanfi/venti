//! Phones registered for push, what pilots nearby do, and help after
//! landing.
//!
//! - `POST` and `DELETE /v1/account/devices`: a phone's token from Apple's
//!   or Google's push service, of the session that registers it.
//! - `GET` and `PUT /v1/account/nearby`: a circle to be told of new open
//!   plans in, and whether pilots nearby may ask for help. Both off until
//!   the pilot turns them on (owner decision 15).
//! - `PUT` and `DELETE /v1/account/position`: a coarse position, kept for
//!   an hour while the pilot may be asked, and shown to nobody.
//! - `/v1/help`: asking the pilots nearby for a lift or a hand, reading a
//!   request, answering "I can help", closing it.
//!
//! **All of it is the pilot's own sign-in's.** A token for a program does
//! none of it: a phone and a position are the device's, and asking or
//! answering for help is the pilot's own word to people near them.
//!
//! **Who sees what.** A request is seen by its pilot and by the pilots it
//! was sent to, and by nobody else: to everyone else it is not there. Its
//! position only by its pilot and, while it stands, by those who said
//! they can help. Nothing of a request, a position or a token reaches the
//! log.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Duration;
use venti_api_types::v1::{
    self as v1, DeviceRemoveRequest, DeviceRequest, ErrorCode, HELP_OPEN_HOURS, HELP_RADIUS_KM,
    HELP_REQUESTS_PER_DAY, HelpAnswerBody, HelpListResponse, HelpRequestBody, HelpResponse,
    MAX_DEVICE_TOKEN_LENGTH, MAX_DEVICES, MAX_HELP_RECIPIENTS, MAX_NEARBY_RADIUS_KM, NearbyCircle,
    NearbySettings, POSITION_KEPT_MINUTES, PhoneMessage, PhoneMessageKind, PositionRequest,
    PushPlatform, coarse_position, help_answer_words, help_request_words, parse_help_note,
};
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{
    AddingDevice, HelpAnswering, HelpCreation, HelpReach, HelpRequest, NewHelpRequest, PhoneDevice,
    Store,
};

use super::auth::OwnSession;
use super::error::ApiError;
use super::groups::text_problem;
use super::{Api, body, cache};
use crate::accounts::secret::{is_id, random_id};
use crate::accounts::throttle::Counted;

/// How long a request that ended is still read by its pilot and those it
/// was sent to, and reported, before it is removed.
pub const HELP_KEPT_HOURS: i64 = 24;

fn invalid_device(field: &str, reason: &str) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidDevice, format!("{field} {reason}"))
}

fn still_open() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        ErrorCode::HelpRequestOpen,
        "this account's last request for help still stands; close it first",
    )
}

pub(crate) fn no_such_request() -> ApiError {
    ApiError::missing(
        ErrorCode::HelpRequestNotFound,
        "there is no request for help with this identifier that this account may read",
    )
}

/// Whether a token has the form its platform gives: hexadecimal for
/// APNs, letters, digits and `-_:` for FCM.
fn token_problem(platform: PushPlatform, token: &str) -> Option<&'static str> {
    if token.is_empty() || token.len() > MAX_DEVICE_TOKEN_LENGTH {
        return Some("is empty or longer than the server keeps");
    }
    let fits = match platform {
        PushPlatform::Apns => token.bytes().all(|byte| byte.is_ascii_hexdigit()),
        PushPlatform::Fcm => token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte)),
    };
    (!fits).then_some(match platform {
        PushPlatform::Apns => "is not an APNs device token in hexadecimal",
        PushPlatform::Fcm => "is not an FCM registration token",
    })
}

/// `POST /v1/account/devices`: registers this phone. The same token again
/// is not counted twice; a token another account registered is this
/// account's from now on.
pub(crate) async fn add_device<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: DeviceRequest = body::json(&headers, &body)?;
    if let Some(problem) = token_problem(request.platform, &request.token) {
        return Err(invalid_device("token", problem));
    }
    if request.sandbox && request.platform != PushPlatform::Apns {
        return Err(invalid_device("sandbox", "is for an iPhone's token only"));
    }
    let device = PhoneDevice {
        account_id: own.account.id.clone(),
        session_id: own.session.id.clone(),
        platform: request.platform.as_str().to_owned(),
        // APNs writes its tokens in either case; one is kept.
        token: if request.platform == PushPlatform::Apns {
            request.token.to_ascii_lowercase()
        } else {
            request.token
        },
        sandbox: request.sandbox,
        created_at: api.clock.now(),
    };
    match api.store.add_device(&device, MAX_DEVICES).await? {
        AddingDevice::Added => Ok(cache::private(StatusCode::NO_CONTENT.into_response())),
        AddingDevice::Full => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::TooManyDevices,
            format!("an account has at most {MAX_DEVICES} phones registered; remove one first"),
        )),
        AddingDevice::NoSession => Err(ApiError::unauthorized("the session is gone")),
    }
}

/// `DELETE /v1/account/devices`: removes a phone. Removing one the account
/// does not have is not an error.
pub(crate) async fn remove_device<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: DeviceRemoveRequest = body::json(&headers, &body)?;
    let token = if request.platform == PushPlatform::Apns {
        request.token.to_ascii_lowercase()
    } else {
        request.token
    };
    if token_problem(request.platform, &token).is_none() {
        api.store
            .remove_device(&own.account.id, request.platform.as_str(), &token)
            .await?;
    }
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

fn settings_wire(settings: &venti_store::NearbySettings) -> NearbySettings {
    NearbySettings {
        open_plans_near: settings.open_plans_near.map(|circle| NearbyCircle {
            position: circle.position,
            radius_km: circle.radius_km,
        }),
        may_be_asked_for_help: settings.may_be_asked_for_help,
    }
}

/// `GET /v1/account/nearby`: the account's settings.
pub(crate) async fn nearby<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
) -> Result<Response, ApiError> {
    let settings = api.store.nearby_settings(&own.account.id).await?;
    Ok(cache::private_json(
        StatusCode::OK,
        &settings_wire(&settings),
    ))
}

/// `PUT /v1/account/nearby`: sets the account's settings, whole.
pub(crate) async fn set_nearby<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: NearbySettings = body::json(&headers, &body)?;
    if let Some(circle) = request.open_plans_near
        && !(1..=MAX_NEARBY_RADIUS_KM).contains(&circle.radius_km)
    {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidNearby,
            format!("open_plans_near.radius_km is 1 to {MAX_NEARBY_RADIUS_KM}"),
        ));
    }
    let settings = venti_store::NearbySettings {
        open_plans_near: request.open_plans_near.map(|circle| venti_store::Circle {
            position: circle.position,
            radius_km: circle.radius_km,
        }),
        may_be_asked_for_help: request.may_be_asked_for_help,
    };
    if !api
        .store
        .set_nearby_settings(&own.account.id, &settings, api.clock.now())
        .await?
    {
        return Err(ApiError::unauthorized("the account is gone"));
    }
    Ok(cache::private_json(
        StatusCode::OK,
        &settings_wire(&settings),
    ))
}

/// `PUT /v1/account/position`: keeps the phone's position, coarse, while
/// the pilot may be asked for help.
pub(crate) async fn set_position<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: PositionRequest = body::json(&headers, &body)?;
    let kept = api
        .store
        .keep_position(
            &own.account.id,
            coarse_position(request.position),
            api.clock.now(),
        )
        .await?;
    if !kept {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::HelpNotOffered,
            "a position is kept only while pilots nearby may ask this pilot for help; \
             turn may_be_asked_for_help on first",
        ));
    }
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `DELETE /v1/account/position`: forgets it.
pub(crate) async fn forget_position<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
) -> Result<Response, ApiError> {
    api.store.forget_position(&own.account.id).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

impl<S: Store> Api<S> {
    /// Since when a position counts.
    fn positions_since(&self, now: Timestamp) -> Timestamp {
        now - Duration::minutes(POSITION_KEPT_MINUTES)
    }

    /// A request as `reader` may see it, or `None` if they may not.
    async fn help_view(
        &self,
        request: &HelpRequest,
        reader: &str,
        now: Timestamp,
    ) -> Result<Option<v1::HelpRequest>, ApiError> {
        let mine = request.pilot.account_id == reader;
        if !mine && !request.was_asked(reader) {
            return Ok(None);
        }
        let pilot = |pilot: &venti_store::GroupPilot| {
            super::wire::pilot(&pilot.handle, &pilot.display_name)
        };
        let can_help = request.can_help(reader);
        let (position, distance_km, asked, helpers, answer) = if mine {
            (
                Some(request.position),
                None,
                Some(u32::try_from(request.asked.len()).unwrap_or(u32::MAX)),
                request
                    .helpers
                    .iter()
                    .map(pilot)
                    .collect::<Result<_, _>>()?,
                None,
            )
        } else {
            let from = self
                .store
                .position_of(reader, self.positions_since(now))
                .await?;
            let distance = from.map(|from: GeoPoint| {
                // Whole kilometres: the reader's position is coarse.
                let km = (from.distance_m(request.position) / 1000.0).round();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let km = km.clamp(0.0, f64::from(u32::MAX)) as u32;
                km
            });
            (
                (can_help && request.stands(now)).then_some(request.position),
                distance,
                None,
                Vec::new(),
                Some(can_help),
            )
        };
        Ok(Some(v1::HelpRequest {
            id: request.id.clone(),
            pilot: pilot(&request.pilot)?,
            note: request.note.clone(),
            created_at: request.created_at,
            ends_at: request.ends_at,
            closed_at: request.closed_at,
            mine,
            position,
            distance_km,
            can_help: answer,
            asked,
            helpers,
        }))
    }

    /// A request for help by the identifier in a path, as `reader` may see
    /// it, with the stored request.
    pub(crate) async fn help_request_for(
        &self,
        id: &str,
        reader: &str,
        now: Timestamp,
    ) -> Result<(HelpRequest, v1::HelpRequest), ApiError> {
        if !is_id(id) {
            return Err(no_such_request());
        }
        let request = self
            .store
            .help_request(id)
            .await?
            .ok_or_else(no_such_request)?;
        let view = self
            .help_view(&request, reader, now)
            .await?
            .ok_or_else(no_such_request)?;
        Ok((request, view))
    }

    async fn help_answer(
        &self,
        request: &HelpRequest,
        reader: &str,
        status: StatusCode,
        now: Timestamp,
    ) -> Result<Response, ApiError> {
        let request = self
            .help_view(request, reader, now)
            .await?
            .ok_or_else(no_such_request)?;
        Ok(cache::private_json(status, &HelpResponse { request }))
    }
}

/// `POST /v1/help`: asks the pilots nearby.
pub(crate) async fn ask<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: HelpRequestBody = body::json(&headers, &body)?;
    let note = parse_help_note(request.note.as_deref().unwrap_or_default()).map_err(|error| {
        ApiError::bad_request(
            ErrorCode::InvalidHelpRequest,
            text_problem("the note of a request for help", error),
        )
    })?;
    let now = api.clock.now();
    if own.account.publishing_suspended_at.is_some() {
        return Err(ApiError::forbidden(
            ErrorCode::PublishingSuspended,
            "whoever runs this server has suspended this account's publishing: it asks no \
             pilots for help through Venti; for an injury, call 112",
        ));
    }
    // A request that still stands is said before the day's count is
    // touched; the store holds it again when it stores the new one.
    if !api
        .store
        .help_requests_of(&own.account.id, now)
        .await?
        .is_empty()
    {
        return Err(still_open());
    }
    api.help_requests
        .attempt(&[Counted::new(&own.account.id, HELP_REQUESTS_PER_DAY)], now)
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this account has asked pilots nearby for help as often today as one account may",
            )
        })?;
    let id = random_id().map_err(|_| ApiError::internal())?;
    let new = NewHelpRequest {
        id,
        account_id: own.account.id.clone(),
        position: request.position,
        note,
        created_at: now,
        ends_at: now + Duration::hours(HELP_OPEN_HOURS),
    };
    let reach = HelpReach {
        radius_km: HELP_RADIUS_KM,
        positions_since: api.positions_since(now),
        most: MAX_HELP_RECIPIENTS,
    };
    let created = match api.store.create_help_request(&new, &reach).await? {
        HelpCreation::Created(created) => created,
        HelpCreation::StillOpen => return Err(still_open()),
        HelpCreation::NoAccount => return Err(ApiError::unauthorized("the account is gone")),
    };
    let (title, line) = help_request_words(&own.account.display_name, created.note.as_deref());
    api.phones.tell(
        created.asked.clone(),
        PhoneMessage {
            title,
            body: line,
            kind: PhoneMessageKind::HelpRequest,
            id: created.id.clone(),
            sent_at: now,
        },
    );
    api.help_answer(&created, &own.account.id, StatusCode::CREATED, now)
        .await
}

/// `GET /v1/help`: the account's own requests that stand, and those it
/// was asked.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    let reader = own.account.id.as_str();
    let mut mine = Vec::new();
    for request in api.store.help_requests_of(reader, now).await? {
        mine.extend(api.help_view(&request, reader, now).await?);
    }
    let mut asked = Vec::new();
    for request in api.store.help_requests_asked(reader, now).await? {
        asked.extend(api.help_view(&request, reader, now).await?);
    }
    Ok(cache::private_json(
        StatusCode::OK,
        &HelpListResponse { mine, asked },
    ))
}

/// `GET /v1/help/{id}`: one request.
pub(crate) async fn one<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    let (_, request) = api.help_request_for(&id, &own.account.id, now).await?;
    Ok(cache::private_json(
        StatusCode::OK,
        &HelpResponse { request },
    ))
}

/// `DELETE /v1/help/{id}`: closes the account's own request.
pub(crate) async fn close<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if !is_id(&id) {
        return Err(no_such_request());
    }
    api.store
        .close_help_request(&id, &own.account.id, api.clock.now())
        .await?
        .ok_or_else(no_such_request)?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `PUT /v1/help/{id}/answer`: says whether the pilot can help. Its pilot
/// is told when someone can.
pub(crate) async fn answer<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: HelpAnswerBody = body::json(&headers, &body)?;
    if !is_id(&id) {
        return Err(no_such_request());
    }
    let now = api.clock.now();
    let answered = api
        .store
        .answer_help_request(&id, &own.account.id, request.can_help, now)
        .await?;
    let HelpAnswering::Answered(stored, changed) = answered else {
        // Not asked, or no longer standing: a request that ended is read
        // still, and answered no more.
        let (stored, _) = api.help_request_for(&id, &own.account.id, now).await?;
        return Err(if stored.stands(now) {
            no_such_request()
        } else {
            ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::HelpRequestEnded,
                "this request for help has ended or was closed: it takes no answer",
            )
        });
    };
    if changed && request.can_help {
        let (title, line) = help_answer_words(&own.account.display_name);
        api.phones.tell(
            vec![stored.pilot.account_id.clone()],
            PhoneMessage {
                title,
                body: line,
                kind: PhoneMessageKind::HelpAnswer,
                id: stored.id.clone(),
                sent_at: now,
            },
        );
    }
    api.help_answer(&stored, &own.account.id, StatusCode::OK, now)
        .await
}
