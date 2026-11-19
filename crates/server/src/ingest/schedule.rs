//! When a source runs next. Arithmetic only: no clock, no sleeping, so the
//! rules can be tested as plain numbers.

use std::time::Duration;

/// The longest a failing source waits between two attempts, unless its
/// cadence is longer still.
const MAX_BACKOFF: Duration = Duration::from_secs(3600);

/// The most random delay added to a wait, as a divisor of the cadence: a
/// tenth of it.
const JITTER_DIVISOR: u32 = 10;

/// The rhythm of one source.
///
/// Every wait is measured from the start of one run to the start of the
/// next, and is never shorter than the cadence. The pipeline's request
/// budget for one run assumes at most one run per cadence interval; keeping
/// to that here is what keeps the server inside a provider's hourly limit,
/// also while the provider is failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pace {
    cadence: Duration,
}

impl Pace {
    /// The rhythm of a source that publishes every `cadence_seconds`.
    pub(crate) fn new(cadence_seconds: u32) -> Self {
        Self {
            cadence: Duration::from_secs(u64::from(cadence_seconds.max(1))),
        }
    }

    /// How long to wait from the start of a run to the start of the next.
    ///
    /// After a healthy run, and after a first failure, that is the cadence:
    /// one bad run is usually a blip. From the second failure in a row the
    /// wait doubles each time, up to an hour, so a provider in trouble is
    /// left in peace. `jitter` is a random number from 0 up to 1; it adds up
    /// to a tenth of the cadence, so that runs do not hit a provider at the
    /// same second of every interval.
    pub(crate) fn next_delay(&self, consecutive_failures: u32, jitter: f64) -> Duration {
        let doublings = consecutive_failures.saturating_sub(1).min(16);
        let backoff = self.cadence.saturating_mul(1 << doublings);
        let longest = MAX_BACKOFF.max(self.cadence);
        // Whatever the random source hands over, the wait only grows, and by
        // no more than the tenth.
        let share = if jitter.is_finite() {
            jitter.clamp(0.0, 1.0)
        } else {
            0.0
        };
        backoff.min(longest) + (self.cadence / JITTER_DIVISOR).mul_f64(share)
    }

    /// How long to wait before the first run after the server starts, given
    /// how long ago the source's latest run started, if it ever ran.
    ///
    /// A server that restarts, or keeps restarting, must not run a source
    /// more often than its cadence allows.
    pub(crate) fn first_delay(&self, since_last_start: Option<Duration>) -> Duration {
        match since_last_start {
            Some(elapsed) => self.cadence.saturating_sub(elapsed),
            None => Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn a_healthy_source_runs_at_its_cadence() {
        let pace = Pace::new(600);
        assert_eq!(pace.next_delay(0, 0.0), 10 * MINUTE);
    }

    #[test]
    fn failures_back_off_up_to_an_hour() {
        let pace = Pace::new(600);
        let minutes: Vec<u64> = (1..=6)
            .map(|failures| pace.next_delay(failures, 0.0).as_secs() / 60)
            .collect();
        assert_eq!(minutes, [10, 20, 40, 60, 60, 60]);
        assert_eq!(pace.next_delay(u32::MAX, 0.0), 60 * MINUTE);
    }

    #[test]
    fn the_wait_is_never_shorter_than_the_cadence() {
        // A source that publishes every six hours is not retried hourly.
        let slow = Pace::new(6 * 3600);
        assert_eq!(slow.next_delay(0, 0.0), 360 * MINUTE);
        assert_eq!(slow.next_delay(5, 0.0), 360 * MINUTE);
    }

    #[test]
    fn jitter_adds_at_most_a_tenth_of_the_cadence() {
        let pace = Pace::new(600);
        assert_eq!(
            pace.next_delay(0, 0.5),
            10 * MINUTE + Duration::from_secs(30)
        );
        assert!(pace.next_delay(0, 0.999) < 11 * MINUTE);
        // A broken random source cannot shorten the wait or stretch it far.
        assert_eq!(pace.next_delay(0, -3.0), 10 * MINUTE);
        assert_eq!(pace.next_delay(0, 7.0), 11 * MINUTE);
        assert_eq!(pace.next_delay(0, f64::NAN), 10 * MINUTE);
    }

    #[test]
    fn the_first_run_waits_out_what_is_left_of_the_cadence() {
        let pace = Pace::new(600);
        assert_eq!(pace.first_delay(None), Duration::ZERO);
        assert_eq!(pace.first_delay(Some(4 * MINUTE)), 6 * MINUTE);
        assert_eq!(pace.first_delay(Some(10 * MINUTE)), Duration::ZERO);
        assert_eq!(pace.first_delay(Some(3 * 3600 * MINUTE)), Duration::ZERO);
    }
}
