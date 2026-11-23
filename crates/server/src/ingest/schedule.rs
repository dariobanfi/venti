//! When a source runs next. Arithmetic only: no clock, no sleeping, so the
//! rules can be tested as plain numbers.

use std::time::Duration;

use venti_sources::Manifest;

/// The longest a failing source waits between two attempts, unless its
/// cadence, or the wait its manifest asks for after a failed run, is longer
/// still.
pub const MAX_BACKOFF: Duration = Duration::from_secs(3600);

/// How soon a source whose cadence is longer than [`LONG_CADENCE`] is tried
/// again after a run that failed, when its manifest says nothing
/// (`rate_limit.retry_failed_run_after_hours`), the wait doubling with each
/// further failure up to its cadence. A source that publishes once a week,
/// and whose provider was busy at that hour, would otherwise wait a week.
/// At most one run in this time keeps any source inside its hourly limit,
/// since a run's budget is at most an hour's requests
/// ([`venti_sources::Manifest::requests_per_run`]).
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(6 * 3600);

/// The cadence above which a failed run is tried again before the cadence
/// is out, when the manifest says nothing. A daily source waits its day, as
/// it always has.
pub const LONG_CADENCE: Duration = Duration::from_secs(86_400);

/// The most random delay added to a wait, as a divisor of the wait it is
/// added to before any doubling: a tenth of it.
pub const JITTER_DIVISOR: u32 = 10;

/// The rhythm of one source.
///
/// Every wait is measured from the start of one run to the start of the
/// next, and is never shorter than the cadence, except after a failed run
/// of a source that is tried again sooner: one whose manifest says so, or
/// one whose cadence is longer than [`LONG_CADENCE`]. The pipeline's
/// request budget for one run assumes at most one run per cadence
/// interval, and is never more than an hour's requests; a retry before the
/// cadence is out is never sooner than an hour (the manifest gives it in
/// whole hours). That is what keeps the server inside a provider's hourly
/// limit, also while the provider is failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    cadence: Duration,
    after_failure: Duration,
    /// Whether the wait after a failure stays the same however many runs
    /// failed in a row: so when the manifest names it (the owner,
    /// 2026-11-23: "retry once every 2 days").
    steady: bool,
}

impl Pace {
    /// The rhythm of a source that publishes every `cadence_seconds` and is
    /// tried again `retry_failed_run_after_hours` after a failed run, or by
    /// the rule of [`RETRY_AFTER_FAILURE`] when that is `None`.
    pub fn new(cadence_seconds: u32, retry_failed_run_after_hours: Option<u32>) -> Self {
        let cadence = Duration::from_secs(u64::from(cadence_seconds.max(1)));
        let after_failure = match retry_failed_run_after_hours {
            Some(hours) => Duration::from_secs(u64::from(hours.max(1)) * 3600),
            None if cadence > LONG_CADENCE => RETRY_AFTER_FAILURE,
            None => cadence,
        };
        Self {
            cadence,
            after_failure,
            steady: retry_failed_run_after_hours.is_some(),
        }
    }

    /// The rhythm the source's manifest asks for.
    pub fn of(manifest: &Manifest) -> Self {
        Self::new(
            manifest.cadence_seconds,
            manifest.rate_limit.retry_failed_run_after_hours,
        )
    }

    /// How long to wait from the start of a run to the start of the next.
    ///
    /// After a healthy run that is the cadence. After a failed run it is
    /// the wait after a failure: what the manifest asks for, else six hours
    /// for a source whose cadence is longer than [`LONG_CADENCE`], else the
    /// cadence, one bad run being usually a blip. A wait the manifest names
    /// stays the same after every failure. Otherwise, from the second
    /// failure in a row, the wait doubles each time, up to the longest of
    /// an hour, the cadence and the first wait, so a provider in trouble is
    /// left in peace.
    /// `jitter` is a random number from 0 up to 1; it adds up to a tenth of
    /// the wait before any doubling, so that runs do not hit a provider at
    /// the same second of every interval.
    pub fn next_delay(&self, consecutive_failures: u32, jitter: f64) -> Duration {
        let first = if consecutive_failures > 0 {
            self.after_failure
        } else {
            self.cadence
        };
        let doublings = if self.steady {
            0
        } else {
            consecutive_failures.saturating_sub(1).min(16)
        };
        let backoff = first.saturating_mul(1 << doublings);
        let longest = MAX_BACKOFF.max(self.cadence).max(self.after_failure);
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
    /// [`Self::next_delay`] allows after one failure: a source whose latest
    /// run failed is tried again as soon as it would have been without the
    /// restart, and no sooner.
    ///
    /// The one exception is `falls_short`: what the store holds of the
    /// source was collected over less than the plan now asks for (a region
    /// was added, a box widened), and the latest run was healthy. The run
    /// is then due at once instead of up to a cadence later. The caller
    /// says so only after a healthy latest run, so a server that keeps
    /// restarting while the provider fails waits out the cadence as
    /// before; and a healthy run stores a set that covers the plan, so the
    /// exception cannot repeat itself.
    pub fn first_delay(
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
        let pace = Pace::new(600, None);
        assert_eq!(pace.next_delay(0, 0.0), 10 * MINUTE);
    }

    #[test]
    fn failures_back_off_up_to_an_hour() {
        let pace = Pace::new(600, None);
        let minutes: Vec<u64> = (1..=6)
            .map(|failures| pace.next_delay(failures, 0.0).as_secs() / 60)
            .collect();
        assert_eq!(minutes, [10, 20, 40, 60, 60, 60]);
        assert_eq!(pace.next_delay(u32::MAX, 0.0), 60 * MINUTE);
    }

    #[test]
    fn the_wait_is_never_shorter_than_the_cadence() {
        // A source that publishes every six hours is not retried hourly.
        let slow = Pace::new(6 * 3600, None);
        assert_eq!(slow.next_delay(0, 0.0), 360 * MINUTE);
        assert_eq!(slow.next_delay(5, 0.0), 360 * MINUTE);
    }

    #[test]
    fn jitter_adds_at_most_a_tenth_of_the_cadence() {
        let pace = Pace::new(600, None);
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
        let pace = Pace::new(600, None);
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
        let daily = Pace::new(86_400, None);
        assert_eq!(daily.first_delay(Some(MINUTE), true, false), Duration::ZERO);
        assert_eq!(
            daily.first_delay(Some(MINUTE), false, false),
            86_400 * Duration::from_secs(1) - MINUTE
        );
    }

    #[test]
    fn a_source_with_a_long_cadence_is_tried_again_hours_after_a_failure() {
        const HOUR: Duration = Duration::from_secs(3600);
        let weekly = Pace::new(7 * 86_400, None);
        let hours: Vec<u64> = (0..=7)
            .map(|failures| weekly.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [168, 6, 12, 24, 48, 96, 168, 168]);
        // The jitter is a tenth of the six hours, not of the week.
        assert_eq!(weekly.next_delay(1, 0.5), 6 * HOUR + 18 * MINUTE);
        assert_eq!(weekly.next_delay(0, 0.5), 7 * 24 * HOUR + 504 * MINUTE);
        // A daily source waits its day, as it always has; one of two days
        // is tried again within them.
        let daily = Pace::new(86_400, None);
        let hours: Vec<u64> = (1..=4)
            .map(|failures| daily.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [24, 24, 24, 24]);
        let every_other_day = Pace::new(2 * 86_400, None);
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

    #[test]
    fn a_manifest_may_ask_for_a_longer_wait_after_a_failed_run() {
        const HOUR: Duration = Duration::from_secs(3600);
        // The cables of OpenStreetMap: once a week, every two days while
        // runs fail (the owner: "retry once every 2 days").
        let weekly = Pace::new(7 * 86_400, Some(48));
        let hours: Vec<u64> = (0..=4)
            .map(|failures| weekly.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [168, 48, 48, 48, 48]);
        // Its sites: once a day, and two days after a failure, also when
        // the wait would double; never at the day.
        let daily = Pace::new(86_400, Some(48));
        let hours: Vec<u64> = (0..=4)
            .map(|failures| daily.next_delay(failures, 0.0).as_secs() / 3600)
            .collect();
        assert_eq!(hours, [24, 48, 48, 48, 48]);
        assert_eq!(daily.next_delay(1, 0.5), 48 * HOUR + 144 * MINUTE);
        // A server that restarts after the failed run waits out the two
        // days; after a healthy one, the day.
        assert_eq!(daily.first_delay(Some(2 * HOUR), false, true), 46 * HOUR);
        assert_eq!(weekly.first_delay(Some(2 * HOUR), false, true), 46 * HOUR);
        assert_eq!(daily.first_delay(Some(2 * HOUR), false, false), 22 * HOUR);
        // A source of ten minutes asked to wait an hour after a failure.
        let often = Pace::new(600, Some(1));
        let minutes: Vec<u64> = (0..=3)
            .map(|failures| often.next_delay(failures, 0.0).as_secs() / 60)
            .collect();
        assert_eq!(minutes, [10, 60, 60, 60]);
    }

    #[test]
    fn the_manifests_of_openstreetmap_wait_two_days_after_a_failed_run() {
        const HOUR: Duration = Duration::from_secs(3600);
        for text in [
            include_str!("../../../sources-osm/manifest.toml"),
            include_str!("../../../sources-osm/manifest-cables.toml"),
        ] {
            let manifest = Manifest::from_toml(text).unwrap();
            assert_eq!(
                Pace::of(&manifest).next_delay(1, 0.0),
                48 * HOUR,
                "{}",
                manifest.id
            );
        }
    }
}
