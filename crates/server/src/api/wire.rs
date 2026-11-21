//! From the store's records to the API's wire format.

use chrono::Timelike;
use venti_api_types::v1;
use venti_store::{
    AccessToken, Account, Flight, Group, GroupMembers, GroupPilot, LiveFix, LiveTrack,
    LiveVisibility, Place, Plan, ScoreKind, Session, Visibility,
};

use super::error::ApiError;
use super::log::Sparse;

/// A stored value that no longer has the form it was stored in.
fn damaged(what: &str, problem: impl std::fmt::Display) -> ApiError {
    static GATE: Sparse = Sparse::new();
    GATE.error(format_args!(
        "the store holds an unusable {what}: {problem}"
    ));
    ApiError::internal()
}

fn handle(handle: &str) -> Result<v1::Handle, ApiError> {
    v1::Handle::new(handle).map_err(|e| damaged("handle", e))
}

pub(crate) fn account(account: &Account) -> Result<v1::Account, ApiError> {
    Ok(v1::Account {
        handle: handle(&account.handle)?,
        display_name: account.display_name.clone(),
        created_at: account.created_at,
        publishing_suspended_at: account.publishing_suspended_at,
    })
}

/// A session as the API shows it. `current` is the identifier of the
/// session the request was made with.
pub(crate) fn session(session: &Session, current: &str) -> v1::Session {
    v1::Session {
        id: session.id.clone(),
        device: session.device.clone(),
        created_at: session.created_at,
        last_used_at: session.last_used_at,
        current: session.id == current,
    }
}

/// A token for a program as its pilot sees it. A scope's name that this
/// server does not know opens nothing and is not shown.
pub(crate) fn access_token(token: &AccessToken) -> v1::AccessToken {
    v1::AccessToken {
        id: token.id.clone(),
        name: token.name.clone(),
        scopes: token
            .scopes
            .iter()
            .filter_map(|name| v1::Scope::from_name(name))
            .collect(),
        created_at: token.created_at,
        last_used_on: token.last_used_on,
        expires_at: token.expires_at,
    }
}

pub(crate) fn visibility(visibility: Visibility) -> v1::Visibility {
    match visibility {
        Visibility::Private => v1::Visibility::Private,
        Visibility::Unlisted => v1::Visibility::Unlisted,
        Visibility::Public => v1::Visibility::Public,
    }
}

/// The store's visibility for one a client sent. `None` for a setting this
/// server does not know.
pub(crate) fn stored_visibility(visibility: v1::Visibility) -> Option<Visibility> {
    match visibility {
        v1::Visibility::Private => Some(Visibility::Private),
        v1::Visibility::Unlisted => Some(Visibility::Unlisted),
        v1::Visibility::Public => Some(Visibility::Public),
        v1::Visibility::Unknown => None,
    }
}

fn place(place: &Place) -> v1::FlightPlace {
    v1::FlightPlace {
        time: place.time,
        position: place.position,
        altitude_m: place.altitude_m,
    }
}

pub(crate) fn pilot(handle_text: &str, display_name: &str) -> Result<v1::Pilot, ApiError> {
    Ok(v1::Pilot {
        handle: handle(handle_text)?,
        display_name: display_name.to_owned(),
    })
}

pub(crate) fn flight(flight: &Flight) -> Result<v1::Flight, ApiError> {
    let derived = &flight.derived;
    Ok(v1::Flight {
        id: flight
            .id
            .parse()
            .map_err(|e| damaged("flight address", e))?,
        identity: flight.identity.clone(),
        pilot: pilot(&flight.pilot_handle, &flight.pilot_name)?,
        visibility: visibility(flight.visibility),
        uploaded_at: flight.uploaded_at,
        start: derived.start,
        airtime_s: derived.airtime_s,
        takeoff: derived.takeoff.as_ref().map(place),
        landing: derived.landing.as_ref().map(place),
        track_length_m: derived.track_length_m,
        max_altitude_m: derived.max_altitude_m,
        max_climb_ms: derived.max_climb_ms,
        score: derived.score.map(|score| v1::FlightScore {
            kind: match score.kind {
                ScoreKind::FreeFlight => v1::RouteKind::FreeFlight,
                ScoreKind::FreeTriangle => v1::RouteKind::FreeTriangle,
                ScoreKind::FaiTriangle => v1::RouteKind::FaiTriangle,
            },
            closed: score.closed,
            distance_m: score.distance_m,
            points: score.points,
        }),
        glider: derived.glider.clone(),
        outline: derived.outline.clone(),
        file_bytes: flight.file_bytes,
        analysis_version: derived.version,
        taken_down_at: flight.taken_down_at,
    })
}

pub(crate) fn flights(flights: &[Flight]) -> Result<Vec<v1::Flight>, ApiError> {
    flights.iter().map(flight).collect()
}

fn group_pilot(pilot: &GroupPilot) -> Result<v1::Pilot, ApiError> {
    self::pilot(&pilot.handle, &pilot.display_name)
}

pub(crate) fn group(group: &Group) -> Result<v1::Group, ApiError> {
    Ok(v1::Group {
        id: group
            .id
            .parse()
            .map_err(|e| damaged("group identifier", e))?,
        name: group.name.clone(),
        created_at: group.created_at,
        keeper: group_pilot(&group.keeper)?,
        member_count: group.members,
    })
}

pub(crate) fn groups(groups: &[Group]) -> Result<Vec<v1::Group>, ApiError> {
    groups.iter().map(group).collect()
}

/// A group with its members, as each of them sees it.
pub(crate) fn group_with_members(seen: &GroupMembers) -> Result<v1::GroupResponse, ApiError> {
    Ok(v1::GroupResponse {
        group: group(&seen.group)?,
        members: seen
            .members
            .iter()
            .map(group_pilot)
            .collect::<Result<_, _>>()?,
    })
}

pub(crate) fn plan(plan: &Plan) -> Result<v1::Plan, ApiError> {
    let details = &plan.details;
    let meet_at = details
        .meet_at
        .map(|time| {
            let part = |value: u32| u8::try_from(value).ok();
            part(time.hour())
                .zip(part(time.minute()))
                .and_then(|(hour, minute)| v1::TimeOfDay::new(hour, minute))
                // Nothing of a plan is written to the log, not even this.
                .ok_or_else(|| damaged("time to meet", "it is no hour and minute"))
        })
        .transpose()?;
    Ok(v1::Plan {
        id: plan.id.parse().map_err(|e| damaged("plan identifier", e))?,
        group: plan
            .group_id
            .parse()
            .map_err(|e| damaged("group identifier", e))?,
        day: details.day,
        place: details.place.clone(),
        position: details.position,
        meet_at,
        note: details.note.clone(),
        author: group_pilot(&plan.author)?,
        created_at: plan.created_at,
        changed_at: plan.changed_at,
        pilots_in: plan
            .pilots_in
            .iter()
            .map(group_pilot)
            .collect::<Result<_, _>>()?,
    })
}

pub(crate) fn plans(plans: &[Plan]) -> Result<Vec<v1::Plan>, ApiError> {
    plans.iter().map(plan).collect()
}

pub(crate) fn live_visibility(visibility: LiveVisibility) -> v1::LiveVisibility {
    match visibility {
        LiveVisibility::Link => v1::LiveVisibility::Link,
        LiveVisibility::Public => v1::LiveVisibility::Public,
    }
}

/// The store's setting for one a client sent. `None` for a setting this
/// server does not know.
pub(crate) fn stored_live_visibility(visibility: v1::LiveVisibility) -> Option<LiveVisibility> {
    match visibility {
        v1::LiveVisibility::Link => Some(LiveVisibility::Link),
        v1::LiveVisibility::Public => Some(LiveVisibility::Public),
        v1::LiveVisibility::Unknown => None,
    }
}

pub(crate) fn live_fix(fix: &LiveFix) -> v1::LiveFix {
    v1::LiveFix {
        time: fix.time,
        position: fix.position,
        altitude_m: fix.altitude_m,
        ground_speed_ms: fix.ground_speed_ms,
        climb_ms: fix.climb_ms,
        course_deg: fix.course_deg,
    }
}

/// A live track as the API shows it, on a server that keeps one for
/// `retention` after its newest fix.
pub(crate) fn live_track(
    track: &LiveTrack,
    retention: chrono::Duration,
) -> Result<v1::LiveTrack, ApiError> {
    Ok(v1::LiveTrack {
        id: track
            .id
            .parse()
            .map_err(|e| damaged("live track identifier", e))?,
        pilot: pilot(&track.pilot_handle, &track.pilot_name)?,
        visibility: live_visibility(track.visibility),
        landed: track.landed,
        started_at: track.started_at,
        latest: live_fix(&track.latest),
        fixes: track.fixes,
        expires_at: track.latest.time + retention,
    })
}
