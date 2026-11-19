//! From the store's records to the API's wire format.

use venti_api_types::v1;
use venti_store::{
    Account, Flight, LiveFix, LiveTrack, LiveVisibility, Place, ScoreKind, Session, Visibility,
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
    })
}

pub(crate) fn flights(flights: &[Flight]) -> Result<Vec<v1::Flight>, ApiError> {
    flights.iter().map(flight).collect()
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
