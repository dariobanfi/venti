//! What the API writes to the log about requests, and how much of it.
//!
//! A log line that a stranger can cause with a request is a way to fill
//! the server's disk: one address that kept asking the connector after it
//! had been refused wrote about a megabyte of warnings a second (measured
//! 2026-11-19). So nothing here is written once per request.
//!
//! - A request is a span at debug level and no line of its own. It names
//!   the method and the path, never a header or a query string.
//! - A refusal for too many attempts is written once per key and window;
//!   the throttle says which refusal is the first.
//! - A failure that can repeat with every request (the database is away,
//!   the server is busy, a request ran out of time) is written through a
//!   [`Sparse`] gate: the first at once, then one line every ten seconds
//!   that says how many more there were.
//!
//! What remains per event is what costs the sender real work: a wrong
//! password is a line, and each one took a password hash to find out.

use std::fmt::Display;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::http::Request;
use tower_http::classify::ServerErrorsFailureClass;
use tracing::Span;

/// How long a gate stays shut after it let a line through.
pub(crate) const QUIET: Duration = Duration::from_secs(10);

/// Lets one log line through per interval and counts the ones it held
/// back.
#[derive(Debug)]
pub(crate) struct Sparse {
    interval: Duration,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    /// When a line was last let through.
    last: Option<Instant>,
    held_back: u64,
}

impl Sparse {
    /// A gate that opens once per [`QUIET`].
    pub(crate) const fn new() -> Self {
        Self::every(QUIET)
    }

    /// A gate that opens once per `interval`.
    pub(crate) const fn every(interval: Duration) -> Self {
        Self {
            interval,
            state: Mutex::new(State {
                last: None,
                held_back: 0,
            }),
        }
    }

    /// Whether a line may be written now; if so, how many were held back
    /// since the last one.
    pub(crate) fn pass(&self) -> Option<u64> {
        self.pass_at(Instant::now())
    }

    fn pass_at(&self, now: Instant) -> Option<u64> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let shut = state
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < self.interval);
        if shut {
            state.held_back = state.held_back.saturating_add(1);
            return None;
        }
        state.last = Some(now);
        Some(std::mem::take(&mut state.held_back))
    }

    /// Writes `message` to the log as an error, unless the gate is shut.
    pub(crate) fn error(&self, message: impl Display) {
        match self.pass() {
            Some(0) => tracing::error!("{message}"),
            Some(more) => {
                tracing::error!("{message} ({more} more like this since the last such line)");
            }
            None => {}
        }
    }
}

/// What the request log says about a request: its method and its path.
///
/// Not its headers, which carry tokens, and not its query string either:
/// nothing secret belongs there, but a client that puts a token there by
/// mistake should not find it in the server's log. And of a path that
/// names a live track, not the track's identifier; see [`logged_path`].
pub(crate) fn request_span<B>(request: &Request<B>) -> Span {
    tracing::debug_span!(
        "request",
        method = %request.method(),
        path = %logged_path(request.uri().path()),
    )
}

/// What stands in the log for what a path names: the identifier of a live
/// track, or a group with whatever of it the path goes on to name.
const LEFT_OUT: &str = "…";

/// The beginnings of paths that go on to name something the log must not
/// hold, each with the paths under it that name nothing.
const NAMING: [(&str, &[&str]); 4] = [
    ("/v1/live/", &["positions"]),
    ("/live/", &[]),
    ("/v1/groups/", &["join", "plans"]),
    ("/v1/oauth/requests/", &[]),
];

/// A request's path as the log may hold it.
///
/// Whoever has the identifier of a live track can see where its pilot is,
/// and for a track shared by link nothing else is needed. So what follows
/// `/v1/live/` or `/live/` in a path is left out of the log, whatever it
/// is: an identifier, or something a client sent in its place.
///
/// A group is nobody's business but its members'. What follows
/// `/v1/groups/` names a group, and after it a plan or a member by their
/// handle, or is something a client sent in their place, an invite code
/// perhaps: it is left out as well.
///
/// A request for a pilot's agreement is answered by whoever holds its
/// identifier and a session. What follows `/v1/oauth/requests/` is that
/// identifier, or something a client sent in its place: it is left out.
///
/// The paths under these that name nothing stay as they are.
pub(crate) fn logged_path(path: &str) -> std::borrow::Cow<'_, str> {
    for (under, naming_nothing) in NAMING {
        if let Some(rest) = path.strip_prefix(under) {
            if rest.is_empty() || naming_nothing.contains(&rest) {
                break;
            }
            return format!("{under}{LEFT_OUT}").into();
        }
    }
    path.into()
}

/// Writes down that a request was answered with a server error, or broke
/// off: the first at once, then at most one line per [`QUIET`] and kind.
///
/// Anyone can make a busy server say "busy" a thousand times a second, so
/// this is not a line per answer.
pub(crate) fn failure(class: ServerErrorsFailureClass, latency: Duration, span: &Span) {
    static UNAVAILABLE: Sparse = Sparse::new();
    static OTHER_STATUS: Sparse = Sparse::new();
    static BROKEN: Sparse = Sparse::new();
    let gate = match &class {
        ServerErrorsFailureClass::StatusCode(status) if status.as_u16() == 503 => &UNAVAILABLE,
        ServerErrorsFailureClass::StatusCode(_) => &OTHER_STATUS,
        ServerErrorsFailureClass::Error(_) => &BROKEN,
    };
    let Some(more) = gate.pass() else {
        return;
    };
    let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
    if more == 0 {
        tracing::error!(parent: span, %class, latency_ms, "a request failed");
    } else {
        tracing::error!(
            parent: span,
            %class,
            latency_ms,
            "a request failed ({more} more like this since the last such line)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_of_a_live_track_is_left_out_of_a_logged_path() {
        let id = "0123456789abcdefghjkmnpqrs";
        for (path, logged) in [
            (format!("/v1/live/{id}"), "/v1/live/…"),
            (format!("/live/{id}"), "/live/…"),
            // Whatever follows, and whatever stands in an identifier's
            // place.
            (format!("/v1/live/{id}/fixes"), "/v1/live/…"),
            (format!("/live/{id}/preview.png"), "/live/…"),
            (format!("/v1/live/positions/{id}"), "/v1/live/…"),
            (format!("/live/positions/{id}"), "/live/…"),
            ("/v1/live/x".to_owned(), "/v1/live/…"),
            ("/live/positions".to_owned(), "/live/…"),
        ] {
            let said = logged_path(&path);
            assert_eq!(said, logged, "{path}");
            assert!(!said.contains(id));
        }
        // So is whatever a path under a group names: the group, a plan, a
        // member, or an invite code a client put where none belongs.
        let handle = "xq_member";
        for path in [
            format!("/v1/groups/{id}"),
            format!("/v1/groups/{id}/invite"),
            format!("/v1/groups/{id}/keeper"),
            format!("/v1/groups/{id}/members/{handle}"),
            format!("/v1/groups/{id}/plans"),
            format!("/v1/groups/{id}/plans/{id}/in"),
            format!("/v1/groups/join/{id}"),
            format!("/v1/groups/plans/{id}"),
            "/v1/groups/7k3m-9q2x-4r8t-1v6w".to_owned(),
            "/v1/groups/x".to_owned(),
        ] {
            let said = logged_path(&path);
            assert_eq!(said, "/v1/groups/…", "{path}");
            assert!(!said.contains(id) && !said.contains(handle));
        }
        // Everything else is logged as it came.
        for path in [
            "/v1/groups",
            "/v1/groups/",
            "/v1/groups/join",
            "/v1/groups/plans",
            "/v1/groupies/0123456789abcdefghjkmnpqrs",
            "/v1/live",
            "/v1/live/",
            "/v1/live/positions",
            "/live/",
            "/v1/health",
            "/v1/flights/0123456789abcdefghjkmnpqrs",
            "/f/0123456789abcdefghjkmnpqrs",
            "/v1/lively/0123456789abcdefghjkmnpqrs",
            "/",
        ] {
            assert_eq!(logged_path(path), path);
        }
    }

    #[test]
    fn a_gate_lets_one_line_through_per_interval_and_counts_the_rest() {
        let gate = Sparse::every(Duration::from_secs(10));
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);

        // The first is written at once.
        assert_eq!(gate.pass_at(at(0)), Some(0));
        // A thousand more in the same ten seconds are not.
        for _ in 0..1000 {
            assert_eq!(gate.pass_at(at(3)), None);
        }
        // The next line says how many there were.
        assert_eq!(gate.pass_at(at(10)), Some(1000));
        assert_eq!(gate.pass_at(at(11)), None);
        // After a quiet time a line is the first again.
        assert_eq!(gate.pass_at(at(20)), Some(1));
        assert_eq!(gate.pass_at(at(3600)), Some(0));
        // A clock that steps back opens nothing.
        assert_eq!(gate.pass_at(at(3599)), None);
    }
}
