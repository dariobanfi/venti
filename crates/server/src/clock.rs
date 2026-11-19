//! The server's source of the current time.
//!
//! Nothing in the server calls `Utc::now()` directly. Code that needs the
//! time is handed a [`Clock`], so tests and the development mode can decide
//! what time it is.

use chrono::{SubsecRound, Utc};
use venti_domain::Timestamp;

/// Tells the current time.
pub trait Clock: Send + Sync + 'static {
    /// The current time.
    fn now(&self) -> Timestamp;
}

/// The computer's clock, read in whole seconds.
///
/// Nothing the server does is finer than a second, and the database keeps
/// less precision than the operating system gives. Whole seconds mean a time
/// read back from the store equals the time that was written, and the API
/// shows `12:33:28Z` instead of nine digits of fraction.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now().trunc_subsecs(0)
    }
}

/// A clock that stands still.
///
/// The development mode serves recorded data and stops the clock at the
/// moment of the recording, so the data never ages.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use chrono::Timelike;

    use super::*;

    #[test]
    fn the_system_clock_ticks_in_whole_seconds() {
        assert_eq!(SystemClock.now().nanosecond(), 0);
    }
}
