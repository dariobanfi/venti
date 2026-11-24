//! Groups as communities with a chat (owner decision 26): finding public
//! groups near a place and by name, coming in, invites by handle, admins,
//! a group's picture, and each group's messages with their pictures and
//! the cards their links are shown as.
//!
//! **Who reads what.** A public group and its messages are read by every
//! pilot signed in; a private group by its members, and by a pilot invited
//! into it without its messages. To anyone else a private group answers as
//! a group that does not exist ([`super::groups::no_such_group`]).
//!
//! **Messages are conversation, not data.** The endpoints of messages and
//! of their pictures take a session of Venti's own apps only, never a
//! token for a program, and nothing of a message is in a public read, a
//! dump or the connector. A picture is served by its identifier without a
//! sign-in, so that an app can show it in an image view: the identifier
//! is 26 random characters, in no list but the chat's own.
//!
//! **Nothing of a group reaches the log**, as for the rest of groups: no
//! line names a group, a message or a picture.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use venti_api_types::v1::{
    self, DEFAULT_FOUND_GROUPS, DEFAULT_MESSAGES, ErrorCode, FlightId, GROUPS_NEAR_KM, GroupId,
    GroupMeRequest, GroupMeResponse, GroupTextError, GroupsFoundResponse, Handle, InviteRequest,
    MAX_CARD_TRACK_POINTS, MAX_FOUND_GROUPS, MAX_GROUP_INVITES, MAX_HOME_RADIUS_KM,
    MAX_MESSAGE_LINKS, MAX_MESSAGE_PICTURE_SIDE, MAX_MESSAGE_PICTURES, MAX_MESSAGES,
    MIN_HOME_RADIUS_KM, MessageRequest, MessageResponse, MessagesResponse, PICTURE_MEDIA_TYPE,
    PICTURE_MEDIA_TYPES, PlanId, SentPicture, SentPictureResponse, UNREAD_COUNTED_UP_TO,
    parse_home_place, parse_message_text, parse_position,
};
use venti_domain::{SiteId, Timestamp};
use venti_store::{
    GroupAct, GroupJoining, GroupPicture, GroupSearch, HomeArea, InviteRoom, LinkKind, MessageLink,
    MessagePage, MessagePosting, MessageRemoval, NewMessage, Store, Visibility,
};

use super::auth::{May, OwnSession, ReadGroups, Signed, WriteGroups};
use super::error::ApiError;
use super::groups::{no_such_group, text_problem};
use super::{Api, body, cache, wire};
use crate::accounts::secret::{self, random_id};
use crate::accounts::throttle::Counted;

// --- Answers that say no ------------------------------------------------------------

fn no_such_message() -> ApiError {
    ApiError::missing(
        ErrorCode::MessageNotFound,
        "the group has no message with this identifier",
    )
}

fn no_such_pilot() -> ApiError {
    ApiError::missing(
        ErrorCode::GroupMemberNotFound,
        "no pilot has this handle, or the group has no member or invite with it",
    )
}

fn not_allowed(what: &str) -> ApiError {
    ApiError::forbidden(
        ErrorCode::NotGroupKeeper,
        format!("only the owner of the group or one of its admins may {what}"),
    )
}

fn invalid_message(problem: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidMessage, problem)
}

fn invalid_group(problem: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidGroupName, problem)
}

/// What became of an act of the owner or an admin, as an answer.
fn acted<T>(act: GroupAct<T>, what: &str) -> Result<T, ApiError> {
    match act {
        GroupAct::Done(done) => Ok(done),
        GroupAct::NotAllowed => Err(not_allowed(what)),
        GroupAct::NoSuchPilot => Err(no_such_pilot()),
        GroupAct::AlreadyIn => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::AlreadyInGroup,
            "the pilot is in the group already",
        )),
        GroupAct::Full => Err(ApiError::forbidden(
            ErrorCode::GroupFull,
            format!("a group has at most {MAX_GROUP_INVITES} open invites"),
        )),
        GroupAct::NoSuchGroup => Err(no_such_group()),
    }
}

// --- Reading a request ----------------------------------------------------------------

fn group_id(text: &str) -> Result<GroupId, ApiError> {
    text.parse().map_err(|_| no_such_group())
}

/// A handle from a path; text that is none is nobody's handle.
fn handle(text: &str) -> String {
    Handle::new(text)
        .map(|handle| handle.as_str().to_owned())
        .unwrap_or_default()
}

/// A home area from a request, as it is kept.
pub(crate) fn home_area(home: Option<&v1::HomeArea>) -> Result<Option<HomeArea>, ApiError> {
    let Some(home) = home else {
        return Ok(None);
    };
    if !(MIN_HOME_RADIUS_KM..=MAX_HOME_RADIUS_KM).contains(&home.radius_km) {
        return Err(invalid_group(format!(
            "the radius of a group's home is {MIN_HOME_RADIUS_KM} to {MAX_HOME_RADIUS_KM} km"
        )));
    }
    let place = parse_home_place(home.place.as_deref().unwrap_or_default())
        .map_err(|error| invalid_group(text_problem("the place of a group's home", error)))?;
    Ok(Some(HomeArea {
        center: home.center,
        radius_km: home.radius_km,
        place,
    }))
}

/// A description from a request, as it is kept.
pub(crate) fn description(text: Option<&str>) -> Result<Option<String>, ApiError> {
    v1::parse_group_description(text.unwrap_or_default())
        .map_err(|error| invalid_group(text_problem("the description of a group", error)))
}

/// A cursor of an earlier answer.
fn cursor(text: &str, field: &str) -> Result<i64, ApiError> {
    text.parse()
        .ok()
        .filter(|number: &i64| *number >= 0)
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidCursor,
                format!("{field} must be a cursor of an earlier answer"),
            )
        })
}

/// A link of a message, checked for its form.
fn link(link: &v1::Link) -> Result<MessageLink, ApiError> {
    let bad = || invalid_message("a link names no record of its kind");
    let (kind, id) = match link.kind {
        v1::LinkKind::Flight => (
            LinkKind::Flight,
            link.id.parse::<FlightId>().map_err(|_| bad())?.to_string(),
        ),
        v1::LinkKind::Site => (
            LinkKind::Site,
            link.id.parse::<SiteId>().map_err(|_| bad())?.to_string(),
        ),
        v1::LinkKind::OpenPlan => (
            LinkKind::OpenPlan,
            link.id.parse::<PlanId>().map_err(|_| bad())?.to_string(),
        ),
        v1::LinkKind::Pilot => (
            LinkKind::Pilot,
            Handle::new(&link.id)
                .map_err(|_| bad())?
                .as_str()
                .to_owned(),
        ),
    };
    Ok(MessageLink { kind, id })
}

fn wire_link(link: &MessageLink) -> v1::Link {
    v1::Link {
        kind: match link.kind {
            LinkKind::Flight => v1::LinkKind::Flight,
            LinkKind::Site => v1::LinkKind::Site,
            LinkKind::OpenPlan => v1::LinkKind::OpenPlan,
            LinkKind::Pilot => v1::LinkKind::Pilot,
        },
        id: link.id.clone(),
    }
}

/// Every `step`th point of a track, the last one included, at most `most`.
fn thinned(points: &[[f64; 2]], most: usize) -> Vec<[f64; 2]> {
    if points.len() <= most {
        return points.to_vec();
    }
    let step = points.len().div_ceil(most - 1);
    let mut thin: Vec<[f64; 2]> = points.iter().step_by(step).copied().collect();
    if let Some(last) = points.last()
        && thin.last() != Some(last)
    {
        thin.push(*last);
    }
    thin
}

/// The picture a request carries, if it says it is one.
fn picture_body(headers: &HeaderMap) -> Result<(), ApiError> {
    let media = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase());
    if media.is_some_and(|media| PICTURE_MEDIA_TYPES.contains(&media.as_str())) {
        return Ok(());
    }
    Err(ApiError::new(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedMediaType,
        format!(
            "send the picture as one of {}, and say so in Content-Type",
            PICTURE_MEDIA_TYPES.join(", ")
        ),
    ))
}

impl<S: Store> Api<S> {
    /// A group as the account reads it, or the answer for no group.
    pub(crate) async fn group_seen_by(
        &self,
        id: &str,
        account_id: &str,
    ) -> Result<venti_store::GroupSeen, ApiError> {
        let id = group_id(id)?;
        self.store
            .group_seen(id.as_str(), account_id, UNREAD_COUNTED_UP_TO)
            .await?
            .ok_or_else(no_such_group)
    }

    /// The cards a message's links are shown as, to `reader`.
    async fn cards(&self, links: &[MessageLink], reader: &str, now: Timestamp) -> Vec<v1::Card> {
        let mut cards = Vec::with_capacity(links.len());
        for link in links {
            let card = self.card(link, reader, now).await;
            cards.push(card.unwrap_or_else(|| v1::Card::Gone {
                link: wire_link(link),
            }));
        }
        cards
    }

    async fn card(&self, link: &MessageLink, reader: &str, now: Timestamp) -> Option<v1::Card> {
        match link.kind {
            LinkKind::Flight => {
                let flight = self.store.flight(&link.id).await.ok()??;
                let shown = flight.visibility != Visibility::Private || flight.account_id == reader;
                if !shown || flight.taken_down_at.is_some() {
                    return None;
                }
                let takeoff = self
                    .shown_flight(&flight)
                    .await
                    .ok()
                    .and_then(|shown| shown.takeoff_site.map(|site| site.name));
                let derived = &flight.derived;
                let metres = derived
                    .score
                    .map_or(derived.track_length_m, |score| score.distance_m);
                Some(v1::Card::Flight(v1::FlightCard {
                    id: flight.id.parse().ok()?,
                    pilot: wire::pictured(
                        &flight.pilot_handle,
                        &flight.pilot_name,
                        flight.pilot_picture.as_deref(),
                    )
                    .ok()?,
                    takeoff,
                    distance_km: (metres / 100.0).round() / 10.0,
                    date: derived.start.date_naive(),
                    track: thinned(&derived.outline, MAX_CARD_TRACK_POINTS),
                }))
            }
            LinkKind::Site => {
                let id: SiteId = link.id.parse().ok()?;
                let site = self.site_answer(&id).await.ok()?.site;
                let kind = site.uses.first().map_or_else(
                    || "site".to_owned(),
                    |used| {
                        serde_json::to_value(used)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "site".to_owned())
                    },
                );
                Some(v1::Card::Site(v1::SiteCard {
                    id: site.id.to_string(),
                    name: site.name,
                    site_use: kind,
                    position: site.position,
                    elevation_m: site.elevation_m,
                }))
            }
            LinkKind::OpenPlan => {
                let plan = self.open_plan_as_anyone(&link.id, now).await.ok()?;
                Some(v1::Card::OpenPlan(v1::OpenPlanCard {
                    id: plan.id.clone(),
                    place: plan.details.place.clone(),
                    day: plan.details.day,
                    position: plan.details.position,
                    author: wire::pilot(&plan.author.handle, &plan.author.display_name).ok()?,
                    pilots_in: u32::try_from(plan.pilots_in.len()).unwrap_or(u32::MAX),
                    i_am_in: plan
                        .pilots_in
                        .iter()
                        .any(|pilot| pilot.account_id == reader),
                }))
            }
            LinkKind::Pilot => {
                let account = self.pilot_with_a_page(&link.id).await.ok()?;
                let pilot = wire::pictured(
                    &account.handle,
                    &account.display_name,
                    account.picture.as_deref(),
                )
                .ok()?;
                Some(v1::Card::Pilot { pilot })
            }
        }
    }

    /// Messages as a reader is shown them, with their cards and plans.
    async fn shown_messages(
        &self,
        group_id: &str,
        messages: &[venti_store::Message],
        reader: &str,
        with_cards: bool,
    ) -> Result<Vec<v1::Message>, ApiError> {
        let now = self.clock.now();
        let wants_plans = messages.iter().any(|message| message.plan_id.is_some());
        let plans = if wants_plans {
            let from = self.groups.limits.gone_before(now);
            self.store
                .plans_of_group(group_id, reader, from, None)
                .await?
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut shown = Vec::with_capacity(messages.len());
        for message in messages {
            let cards = if with_cards {
                self.cards(&message.links, reader, now).await
            } else {
                Vec::new()
            };
            let plan = message
                .plan_id
                .as_ref()
                .and_then(|id| plans.iter().find(|plan| plan.id == *id))
                .map(wire::plan)
                .transpose()?;
            shown.push(wire_message(message, cards, plan)?);
        }
        Ok(shown)
    }

    /// Tells the phones of the members who are to hear of a new message:
    /// in a private group every member who did not mute it, in a public
    /// group those it mentions. A store that cannot say who they are tells
    /// nobody; the message is posted all the same.
    async fn tell_members(&self, group: &venti_store::Group, message: &venti_store::Message) {
        let listeners = match self.store.listeners(&group.id).await {
            Ok(listeners) => listeners,
            Err(error) => {
                tracing::warn!("groups: whom to tell of a new message could not be read: {error}");
                return;
            }
        };
        let accounts = who_hears(listeners, message, group.profile.public);
        let (title, body) = v1::group_message_words(
            &group.name,
            &message.author.display_name,
            &message.text,
            message.pictures.len(),
        );
        self.phones.tell(
            accounts,
            v1::PhoneMessage {
                title,
                body,
                kind: v1::PhoneMessageKind::GroupMessage,
                id: group.id.clone(),
                sent_at: message.created_at,
            },
        );
    }

    fn count(
        &self,
        throttle: &crate::accounts::throttle::Throttle,
        signed: &Signed,
        limit: u32,
        what: &str,
    ) -> Result<(), ApiError> {
        let by_day = [Counted::new(&signed.account.id, limit)];
        throttle
            .attempt(&by_day, self.clock.now())
            .map_err(|refused| {
                ApiError::rate_limited(
                    refused,
                    &format!("this account has {what} today as one account may"),
                )
            })
    }
}

/// Whom a new message is pushed to: in a private group every member but
/// its author who did not mute the group, in a public group those of them
/// it mentions.
fn who_hears(
    listeners: Vec<venti_store::Listener>,
    message: &venti_store::Message,
    public: bool,
) -> Vec<String> {
    let mentioned = v1::mentions(&message.text);
    listeners
        .into_iter()
        .filter(|listener| listener.account_id != message.author.account_id && !listener.muted)
        .filter(|listener| !public || mentioned.contains(&listener.handle))
        .map(|listener| listener.account_id)
        .collect()
}

/// A message as the wire carries it.
pub(crate) fn wire_message(
    message: &venti_store::Message,
    cards: Vec<v1::Card>,
    plan: Option<v1::Plan>,
) -> Result<v1::Message, ApiError> {
    Ok(v1::Message {
        id: message.id.clone(),
        group: message.group_id.parse().map_err(|_| ApiError::internal())?,
        cursor: message.number.to_string(),
        author: wire::pilot(&message.author.handle, &message.author.display_name)?,
        kind: match message.kind {
            venti_store::MessageKind::Text => v1::MessageKind::Text,
            venti_store::MessageKind::Plan => v1::MessageKind::Plan,
        },
        text: message.text.clone(),
        pictures: message
            .pictures
            .iter()
            .map(|id| v1::MessagePicture {
                path: v1::group_picture_path(id),
                id: id.clone(),
            })
            .collect(),
        cards,
        plan,
        created_at: message.created_at,
        removed: message.removed.map(|(at, by)| v1::Removed {
            at,
            by: match by {
                venti_store::RemovedBy::Author => v1::RemovedBy::Author,
                venti_store::RemovedBy::Admin => v1::RemovedBy::Admin,
                venti_store::RemovedBy::Operator => v1::RemovedBy::Operator,
            },
        }),
    })
}

fn no_content() -> Response {
    cache::private(StatusCode::NO_CONTENT.into_response())
}

// --- Finding and coming in --------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct FindParams {
    lat: Option<String>,
    lon: Option<String>,
    q: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/groups/find?lat=&lon=&q=&limit=`: public groups near a place,
/// by name or place, or both.
pub(crate) async fn find<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadGroups>,
    Query(params): Query<FindParams>,
) -> Result<Response, ApiError> {
    let near = match (params.lat.as_deref(), params.lon.as_deref()) {
        (None, None) => None,
        (Some(lat), Some(lon)) => Some(parse_position(Some(lat), Some(lon)).map_err(|error| {
            ApiError::bad_request(ErrorCode::InvalidPosition, error.to_string())
        })?),
        _ => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidPosition,
                "lat and lon go together",
            ));
        }
    };
    let text = params
        .q
        .as_deref()
        .map(|q| v1::parse_group_name(q).ok())
        .unwrap_or_default();
    let limit = match params.limit.as_deref() {
        None => DEFAULT_FOUND_GROUPS,
        Some(text) => text
            .parse()
            .ok()
            .filter(|limit| (1..=MAX_FOUND_GROUPS).contains(limit))
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidLimit,
                    format!("limit is 1 to {MAX_FOUND_GROUPS}"),
                )
            })?,
    };
    let search = GroupSearch {
        near,
        within_km: GROUPS_NEAR_KM,
        text,
        limit,
    };
    let found = api.store.find_groups(&signed.account.id, &search).await?;
    let groups = found
        .iter()
        .map(|found| {
            Ok(v1::FoundGroup {
                group: wire::group(&found.group)?,
                distance_km: found.distance_km.map(|km| (km * 10.0).round() / 10.0),
                member: found.member,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(cache::private_json(
        StatusCode::OK,
        &GroupsFoundResponse { groups },
    ))
}

/// `POST /v1/groups/{id}/members`: the asker comes into a public group,
/// or into a private group they were invited into.
pub(crate) async fn enter<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let limits = &api.groups.limits;
    api.count(
        &api.groups.joins,
        &signed,
        limits.joins_per_day,
        "come into as many groups",
    )?;
    let id = group_id(&id)?;
    let joining = api
        .store
        .enter_group(
            id.as_str(),
            &signed.account.id,
            &limits.room(),
            api.clock.now(),
        )
        .await?;
    let (group, joined) = match joining {
        GroupJoining::Joined(group) => (group, true),
        GroupJoining::AlreadyIn(group) => (group, false),
        GroupJoining::NoSuchInvite => return Err(no_such_group()),
        GroupJoining::GroupFull => {
            return Err(ApiError::forbidden(
                ErrorCode::GroupFull,
                format!(
                    "the group has {} members, which is as many as a group may have",
                    limits.members_per_group
                ),
            ));
        }
        GroupJoining::TooManyGroups => {
            return Err(ApiError::forbidden(
                ErrorCode::TooManyGroups,
                format!(
                    "this account is in {} groups, which is as many as an account may be in",
                    limits.groups_per_account
                ),
            ));
        }
    };
    let status = if joined {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let answer = v1::JoinGroupResponse {
        group: wire::group(&group)?,
        joined,
    };
    Ok(cache::private_json(status, &answer))
}

/// `GET /v1/groups/invites`: the asker's invites into private groups.
pub(crate) async fn invites<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadGroups>,
) -> Result<Response, ApiError> {
    let invites = api.store.invites_of(&signed.account.id).await?;
    let invites = invites.iter().map(wire_invite).collect::<Result<_, _>>()?;
    Ok(cache::private_json(
        StatusCode::OK,
        &v1::GroupInvitesResponse { invites },
    ))
}

fn wire_invite(invite: &venti_store::GroupInvite) -> Result<v1::GroupInvite, ApiError> {
    Ok(v1::GroupInvite {
        group: wire::group(&invite.group)?,
        invited_by: wire::pilot(&invite.invited_by.handle, &invite.invited_by.display_name)?,
        invited_at: invite.invited_at,
    })
}

/// `POST /v1/groups/{id}/invites`: the owner or an admin invites a pilot
/// by handle.
pub(crate) async fn invite<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    api.count(
        &api.groups.invites,
        &signed,
        api.groups.limits.invites_per_day,
        "invited as many pilots",
    )?;
    let id = group_id(&id)?;
    let request: InviteRequest = body::json(&headers, &body)?;
    let room = InviteRoom {
        invites_per_group: MAX_GROUP_INVITES,
    };
    let invited = api
        .store
        .invite_to_group(
            id.as_str(),
            &signed.account.id,
            &handle(&request.handle),
            api.clock.now(),
            &room,
        )
        .await?;
    let invite = acted(invited, "invite pilots")?;
    let answer = v1::GroupInviteAnswer {
        invite: wire_invite(&invite)?,
    };
    Ok(cache::private_json(StatusCode::CREATED, &answer))
}

/// `DELETE /v1/groups/{id}/invites/{handle}`: the owner or an admin takes
/// an invite back, or the invited pilot declines it by their own handle.
pub(crate) async fn drop_invite<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path((id, named)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let dropped = api
        .store
        .drop_group_invite(id.as_str(), &signed.account.id, &handle(&named))
        .await?;
    acted(dropped, "take invites back")?;
    Ok(no_content())
}

/// Makes a member an admin, or no longer one.
async fn set_admin<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    (id, named): (&str, &str),
    admin: bool,
) -> Result<Response, ApiError> {
    let id = group_id(id)?;
    let set = api
        .store
        .set_group_admin(id.as_str(), &signed.account.id, &handle(named), admin)
        .await?;
    match set {
        GroupAct::NotAllowed => Err(ApiError::forbidden(
            ErrorCode::NotGroupKeeper,
            "only the owner of a group makes and unmakes its admins",
        )),
        other => acted(other, "make admins").map(|()| no_content()),
    }
}

/// `PUT /v1/groups/{id}/admins/{handle}`: the owner makes a member an
/// admin.
pub(crate) async fn make_admin<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path((id, named)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    set_admin(&api, &signed, (&id, &named), true).await
}

/// `DELETE /v1/groups/{id}/admins/{handle}`: the owner makes an admin a
/// member again.
pub(crate) async fn unmake_admin<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path((id, named)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    set_admin(&api, &signed, (&id, &named), false).await
}

// --- Pictures ----------------------------------------------------------------------------

/// Reads a picture sent to a group and keeps it; its identifier.
async fn keep_picture<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    group: &GroupId,
    body: Bytes,
    square: bool,
) -> Result<SentPicture, ApiError> {
    api.count(
        &api.groups.pictures,
        signed,
        api.groups.limits.group_pictures_per_day,
        "sent as many pictures to groups",
    )?;
    let refused = |refused: crate::accounts::picture::Refused| {
        ApiError::bad_request(ErrorCode::InvalidPicture, refused.0)
    };
    let (bytes, width, height) = if square {
        let bytes = api
            .picture_turns
            .square(body.to_vec())
            .await
            .map_err(refused)?;
        let side = v1::PICTURE_SIDE;
        (bytes, side, side)
    } else {
        api.picture_turns
            .fitted(body.to_vec(), MAX_MESSAGE_PICTURE_SIDE)
            .await
            .map_err(refused)?
    };
    let picture = GroupPicture {
        id: random_id()?,
        group_id: group.as_str().to_owned(),
        account_id: signed.account.id.clone(),
        bytes,
        width,
        height,
        created_at: api.clock.now(),
    };
    if !api.store.save_group_picture(&picture).await? {
        return Err(no_such_group());
    }
    Ok(SentPicture {
        path: v1::group_picture_path(&picture.id),
        id: picture.id,
        width,
        height,
    })
}

/// `POST /v1/groups/{id}/pictures`: a member sends a picture for a
/// message. The body is the picture, a JPEG, a PNG or a WebP.
pub(crate) async fn send_picture<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    picture_body(&headers)?;
    let signed = own.signed();
    let id = group_id(&id)?;
    let picture = keep_picture(&api, &signed, &id, body, false).await?;
    Ok(cache::private_json(
        StatusCode::CREATED,
        &SentPictureResponse { picture },
    ))
}

/// `PUT /v1/groups/{id}/picture`: the owner or an admin gives the group a
/// picture, cut square as a pilot's is.
pub(crate) async fn put_group_picture<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    picture_body(&headers)?;
    let id = group_id(&id)?;
    // Only who may change the group sends its picture.
    let seen = api.group_seen_by(id.as_str(), &signed.account.id).await?;
    if !matches!(
        seen.standing.role,
        Some(venti_store::GroupRole::Owner | venti_store::GroupRole::Admin)
    ) {
        return Err(if seen.standing.role.is_some() {
            not_allowed("change the group")
        } else {
            no_such_group()
        });
    }
    let picture = keep_picture(&api, &signed, &id, body, true).await?;
    change_profile(
        &api,
        &signed,
        &id,
        venti_store::ProfileChange {
            picture: Some(Some(picture.id)),
            ..Default::default()
        },
    )
    .await
}

/// `DELETE /v1/groups/{id}/picture`: the group has no picture from now on.
pub(crate) async fn remove_group_picture<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteGroups>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    change_profile(
        &api,
        &signed,
        &id,
        venti_store::ProfileChange {
            picture: Some(None),
            ..Default::default()
        },
    )
    .await
}

/// Changes what a group says of itself, and answers with the group as the
/// asker reads it.
pub(crate) async fn change_profile<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    id: &GroupId,
    change: venti_store::ProfileChange,
) -> Result<Response, ApiError> {
    let changed = api
        .store
        .change_group(id.as_str(), &signed.account.id, &change)
        .await?;
    match changed {
        GroupAct::NotAllowed if change.public.is_some() => {
            return Err(ApiError::forbidden(
                ErrorCode::NotGroupKeeper,
                "only the owner of a group makes it public or private",
            ));
        }
        other => {
            acted(other, "change the group")?;
        }
    }
    let seen = api.group_seen_by(id.as_str(), &signed.account.id).await?;
    Ok(cache::private_json(
        StatusCode::OK,
        &wire::group_seen(&seen)?,
    ))
}

/// What a picture's address may be kept for: an hour, by the asker's own
/// browser alone. It names one picture for good; a removed one is gone.
const GROUP_PICTURE_CACHE: &str = "private, max-age=3600";

/// `GET /v1/groups/pictures/{id}`: a picture sent to a group, by its
/// identifier.
pub(crate) async fn picture<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let missing = || {
        ApiError::missing(
            ErrorCode::PictureNotFound,
            "there is no picture with this identifier",
        )
    };
    if !secret::is_id(&id) {
        return Err(missing());
    }
    let picture = api.store.group_picture(&id).await?.ok_or_else(missing)?;
    let mut response = (StatusCode::OK, picture.bytes).into_response();
    cache::set(&mut response, header::CONTENT_TYPE, PICTURE_MEDIA_TYPE);
    cache::set(&mut response, header::CACHE_CONTROL, GROUP_PICTURE_CACHE);
    if let Ok(value) = HeaderValue::from_str(&format!("\"{id}\"")) {
        response.headers_mut().insert(header::ETAG, value);
    }
    Ok(response)
}

// --- Messages ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct MessageParams {
    before: Option<String>,
    since: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/groups/{id}/messages?before=&since=&limit=`: a page of a
/// group's messages, the newest first; or with `since`, what was posted or
/// changed after an earlier answer.
pub(crate) async fn messages<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
    Query(params): Query<MessageParams>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let limit = match params.limit.as_deref() {
        None => DEFAULT_MESSAGES,
        Some(text) => text
            .parse()
            .ok()
            .filter(|limit| (1..=MAX_MESSAGES).contains(limit))
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidLimit,
                    format!("limit is 1 to {MAX_MESSAGES}"),
                )
            })?,
    };
    let page = match (params.before.as_deref(), params.since.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidCursor,
                "ask with before or with since, not both",
            ));
        }
        (Some(before), None) => MessagePage::Before(Some(cursor(before, "before")?)),
        (None, Some(since)) => MessagePage::After(cursor(since, "since")?),
        (None, None) => MessagePage::Before(None),
    };
    let account_id = &own.account.id;
    let found = api
        .store
        .messages(id.as_str(), account_id, page, limit)
        .await?
        .ok_or_else(no_such_group)?;
    let full = found.len() == limit;
    let newest_version = found.iter().map(|message| message.version).max();
    let (older, since, more) = match page {
        MessagePage::Before(_) => (
            full.then(|| found.last().map(|message| message.number.to_string()))
                .flatten(),
            newest_version.unwrap_or(0),
            false,
        ),
        MessagePage::After(after) => (None, newest_version.unwrap_or(after), full),
    };
    let messages = api
        .shown_messages(id.as_str(), &found, account_id, true)
        .await?;
    let answer = MessagesResponse {
        messages,
        older,
        since: since.to_string(),
        more,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `POST /v1/groups/{id}/messages`: a member posts a message.
pub(crate) async fn post_message<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let signed = own.signed();
    if signed.account.publishing_suspended_at.is_some() {
        return Err(ApiError::forbidden(
            ErrorCode::PublishingSuspended,
            "this account may post nothing",
        ));
    }
    api.count(
        &api.groups.messages,
        &signed,
        api.groups.limits.messages_per_day,
        "posted as many messages",
    )?;
    let id = group_id(&id)?;
    let request: MessageRequest = body::json(&headers, &body)?;
    let text = parse_message_text(&request.text).map_err(|error| match error {
        GroupTextError::TooLong(_) | GroupTextError::Empty => {
            invalid_message(text_problem("a message", error))
        }
    })?;
    if request.pictures.len() > MAX_MESSAGE_PICTURES {
        return Err(invalid_message(format!(
            "a message has at most {MAX_MESSAGE_PICTURES} pictures"
        )));
    }
    if request.links.len() > MAX_MESSAGE_LINKS {
        return Err(invalid_message(format!(
            "a message has at most {MAX_MESSAGE_LINKS} links"
        )));
    }
    if request
        .pictures
        .iter()
        .any(|picture| !secret::is_id(picture))
    {
        return Err(invalid_message(
            "a picture names no picture sent to the group",
        ));
    }
    let links = request
        .links
        .iter()
        .map(link)
        .collect::<Result<Vec<_>, _>>()?;
    if text.is_empty() && request.pictures.is_empty() && links.is_empty() {
        return Err(invalid_message("a message has text, a picture or a link"));
    }
    let new = NewMessage {
        id: random_id()?,
        group_id: id.as_str().to_owned(),
        author_id: signed.account.id.clone(),
        text,
        pictures: request.pictures,
        links,
        created_at: api.clock.now(),
    };
    let message = match api.store.post_message(&new).await? {
        MessagePosting::Posted(message) => message,
        MessagePosting::NoSuchPicture => {
            return Err(invalid_message(
                "a picture is not one this pilot sent to the group, or another message has it",
            ));
        }
        MessagePosting::NoSuchGroup => return Err(no_such_group()),
    };
    if let Some(seen) = api
        .store
        .group_seen(id.as_str(), &signed.account.id, 0)
        .await?
    {
        api.tell_members(&seen.group.group, &message).await;
    }
    let shown = api
        .shown_messages(
            id.as_str(),
            std::slice::from_ref(&message),
            &signed.account.id,
            true,
        )
        .await?;
    let message = shown.into_iter().next().ok_or_else(ApiError::internal)?;
    Ok(cache::private_json(
        StatusCode::CREATED,
        &MessageResponse { message },
    ))
}

/// `DELETE /v1/groups/{id}/messages/{message}`: its author, the owner or
/// an admin removes a message. It shows as removed.
pub(crate) async fn remove_message<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path((id, message)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let account_id = &own.account.id;
    let removal = api
        .store
        .remove_message(id.as_str(), &message, account_id, api.clock.now())
        .await?;
    let removed = match removal {
        MessageRemoval::Removed(message) => message,
        MessageRemoval::NotAllowed => {
            return Err(ApiError::forbidden(
                ErrorCode::NotGroupKeeper,
                "only the author of a message, the owner of the group and its admins remove it",
            ));
        }
        MessageRemoval::NoSuchMessage => return Err(no_such_message()),
        MessageRemoval::NoSuchGroup => return Err(no_such_group()),
    };
    let message = wire_message(&removed, Vec::new(), None)?;
    Ok(cache::private_json(
        StatusCode::OK,
        &MessageResponse { message },
    ))
}

/// `PATCH /v1/groups/{id}/me`: a member mutes or unmutes the group, or
/// marks it read.
pub(crate) async fn me<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = group_id(&id)?;
    let request: GroupMeRequest = body::json(&headers, &body)?;
    let read_up_to = request
        .read_up_to
        .as_deref()
        .map(|text| cursor(text, "read_up_to"))
        .transpose()?;
    let standing = api
        .store
        .set_group_standing(
            id.as_str(),
            &own.account.id,
            request.muted,
            read_up_to,
            UNREAD_COUNTED_UP_TO,
        )
        .await?
        .ok_or_else(no_such_group)?;
    Ok(cache::private_json(
        StatusCode::OK,
        &GroupMeResponse {
            me: wire::group_me(&standing),
        },
    ))
}

/// The last message of each of the listed groups, without cards: what
/// `GET /v1/groups` shows of it.
pub(crate) fn last_message(message: &venti_store::Message) -> Result<v1::Message, ApiError> {
    wire_message(message, Vec::new(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_private_group_tells_every_member_and_a_public_one_those_it_mentions() {
        let listener = |handle: &str, muted: bool| venti_store::Listener {
            account_id: format!("id-{handle}"),
            handle: handle.to_owned(),
            muted,
        };
        let listeners = || {
            vec![
                listener("dario", false),
                listener("anna", false),
                listener("jonas", true),
                listener("mara", false),
            ]
        };
        let message = venti_store::Message {
            id: "m".to_owned(),
            group_id: "g".to_owned(),
            number: 1,
            version: 1,
            author: venti_store::GroupPilot {
                account_id: "id-dario".to_owned(),
                handle: "dario".to_owned(),
                display_name: "Dario".to_owned(),
            },
            kind: venti_store::MessageKind::Text,
            text: "@anna and @jonas, who drives?".to_owned(),
            pictures: Vec::new(),
            links: Vec::new(),
            plan_id: None,
            created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            removed: None,
        };
        // Private: everyone but the author and who muted it.
        assert_eq!(
            who_hears(listeners(), &message, false),
            ["id-anna", "id-mara"]
        );
        // Public: only those mentioned, and a muted one not even then.
        assert_eq!(who_hears(listeners(), &message, true), ["id-anna"]);
    }

    #[test]
    fn a_track_is_thinned_to_at_most_the_points_a_card_takes() {
        let points: Vec<[f64; 2]> = (0..500).map(|i| [f64::from(i), 0.0]).collect();
        let thin = thinned(&points, MAX_CARD_TRACK_POINTS);
        assert!(thin.len() <= MAX_CARD_TRACK_POINTS, "{}", thin.len());
        assert_eq!(thin.first(), points.first());
        assert_eq!(thin.last(), points.last());
        assert_eq!(thinned(&points[..10], 48).len(), 10);
    }
}
