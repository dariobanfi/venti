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
/// mistake should not find it in the server's log.
pub(crate) fn request_span<B>(request: &Request<B>) -> Span {
    tracing::debug_span!(
        "request",
        method = %request.method(),
        path = %request.uri().path(),
    )
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
