//! When a source runs next. Arithmetic only: no clock, no sleeping, so the
//! rules can be tested as plain numbers.

use std::time::Duration;

/// The longest a failing source waits between two attempts, unless its
/// cadence is longer still.
const MAX_BACKOFF: Duration = Duration::from_secs(3600);

/// How soon a source whose cadence is longer than [`LONG_CADENCE`] is tried
/// again after a run that failed, the wait doubling with each further
/// failure up to its cadence. A source that publishes once a week, and whose
/// provider was busy at that hour, would otherwise wait a week. At most one
/// run in this time keeps any source inside its hourly limit, since a run's
/// budget is at most an hour's requests
/// ([`venti_sources::Manifest::requests_per_run`]).
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(6 * 3600);

/// The cadence above which a failed run is tried again before the cadence
/// is out. A daily source waits its day, as it always has.
const LONG_CADENCE: Duration = Duration::from_secs(86_400);

/// The most random delay added to a wait, as a divisor of the wait it is
/// added to before any doubling: a tenth of it.
const JITTER_DIVISOR: u32 = 10;

/// The rhythm of one source.
///
/// Every wait is measured from the start of one run to the start of the
/// next, and is never shorter than the cadence, except after a failed run of
/// a source whose cadence is longer than [`LONG_CADENCE`]. The
/// pipeline's request budget for one run assumes at most one run per
/// cadence interval, and is never more than an hour's requests; keeping to
/// that here is what keeps the server inside a provider's hourly limit,
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
    /// After a healthy run that is the cadence. After a first failure it is
    /// the cadence too, one bad run being usually a blip; from the second
    /// failure in a row the wait doubles each time, up to an hour, so a
    /// provider in trouble is left in peace. A source whose cadence is
    /// longer than [`LONG_CADENCE`] is tried again [`RETRY_AFTER_FAILURE`]
    /// after a failure instead, the wait doubling from there up to its
    /// cadence.
    /// `jitter` is a random number from 0 up to 1; it adds up to a tenth of
    /// the wait before any doubling, so that runs do not hit a provider at
    /// the same second of every interval.
    pub(crate) fn next_delay(&self, consecutive_failures: u32, jitter: f64) -> Duration {
        let first = if consecutive_failures > 0 && self.cadence > LONG_CADENCE {
            RETRY_AFTER_FAILURE
        } else {
            self.cadence
        };
        let doublings = consecutive_failures.saturating_sub(1).min(16);
        let backoff = first.saturating_mul(1 << doublings);
        let longest = MAX_BACKOFF.max(self.cadence);
        // Whatever the random source hands over, the wait only grows, and by
        // no more than the tenth.
        let share = if jitter.is_finite() {
            jitter.clamp(0.0, 1.0)
        } else {
            0.0
        };
        backoff.min(longest) + (first / JITTER_DIVISOR).mul_f64(share)
    }

    /// How long to wait before the first run after the server starts, given
    /// how long ago the source's latest run started, if it ever ran, and
    /// whether that run failed.
    ///
    /// A server that restarts, or keeps restarting, must not run a source
    /// more often than its cadence allows, or, after a failed run, than
    /// [`Self::next_delay`] allows after one failure: a source with a long
    /// cadence whose latest run failed is tried again as soon as it would
    /// have been without the restart.
    ///
    /// The one exception is `falls_short`: what the store holds of the
    /// source was collected over less than the plan now asks for (a region
    /// was added, a box widened), and the latest run was healthy. The run
    /// is then due at once instead of up to a cadence later. The caller
    /// says so only after a healthy latest run, so a server that keeps
    /// restarting while the provider fails waits out the cadence as
    /// before; and a healthy run stores a set that covers the plan, so the
    /// exception cannot repeat itself.
    pub(crate) fn first_delay(
        &self,
        since_last_start: Option<Duration>,
        falls_short: bool,
        latest_failed: bool,
    ) -> Duration {
        match since_last_start {
            Some(_) if falls_short => Duration::ZERO,
            Some(elapsed) => self
                .next_delay(u32::from(latest_failed), 0.0)
                .saturating_sub(elapsed),
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
        assert_eq!(pace.first_delay(None, false, false), Duration::ZERO);
        assert_eq!(pace.first_delay(Some(4 * MINUTE), false, false), 6 * MINUTE);
        assert_eq!(pace.first_delay(Some(4 * MINUTE), false, true), 6 * MINUTE);
        assert_eq!(
            pace.first_delay(Some(10 * MINUTE), false, false),
            Duration::ZERO
        );
        assert_eq!(
            pace.first_delay(Some(3 * 3600 * MINUTE), false, false),
            Duration::ZERO
        );
    }

    #[test]
    fn a_stored_set_that_falls_short_of_the_plan_is_due_at_once() {
        let daily = Pace::new(86_400);
        assert_eq!(daily.first_delay(Some(MINUTE), true, false), Duration::ZERO);
        assert_eq!(
            daily.first_delay(Some(MINUTE), false, false),
            86_400 * Duration::from_secs(1) - MINUTE
        );
    }

    #[test]
    fn a_source_with_a_long_cadence_is_tried_again_hours_after_a_failure() {
        const HOUR: Duration = Duration::from_secs(3600);
        let weekly = Pace::new(7 * 86_400);
        let hours: Vec<u64> = (0..=7)
            .map(|failures| weekly.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [168, 6, 12, 24, 48, 96, 168, 168]);
        // The jitter is a tenth of the six hours, not of the week.
        assert_eq!(weekly.next_delay(1, 0.5), 6 * HOUR + 18 * MINUTE);
        assert_eq!(weekly.next_delay(0, 0.5), 7 * 24 * HOUR + 504 * MINUTE);
        // A daily source waits its day, as it always has; one of two days
        // is tried again within them.
        let daily = Pace::new(86_400);
        let hours: Vec<u64> = (1..=4)
            .map(|failures| daily.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [24, 24, 24, 24]);
        let every_other_day = Pace::new(2 * 86_400);
        let hours: Vec<u64> = (1..=5)
            .map(|failures| every_other_day.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [6, 12, 24, 48, 48]);
        // A server that restarts after a failed run waits out the six hours,
        // not the week; after a healthy one, the week.
        assert_eq!(weekly.first_delay(Some(2 * HOUR), false, true), 4 * HOUR);
        assert_eq!(weekly.first_delay(Some(2 * HOUR), false, false), 166 * HOUR);
        assert_eq!(
            weekly.first_delay(Some(7 * HOUR), false, true),
            Duration::ZERO
        );
    }
}
