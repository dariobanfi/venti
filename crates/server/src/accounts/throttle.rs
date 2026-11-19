//! Counting attempts, so that guessing passwords, collecting invite codes
//! or filling the disk is slow.
//!
//! The counters live in this process. That is right while one process
//! serves the API; when several do, they move into the database.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use chrono::Duration;
use venti_domain::Timestamp;

/// How many keys a throttle watches at most. Each takes about a hundred
/// bytes, so a full table is a few megabytes.
const CAPACITY: usize = 50_000;

/// Allows each key a number of attempts within a window of time.
///
/// The window of a key opens with its first counted attempt and closes
/// `window` later, when the key is forgotten. A key that has used up its
/// attempts is refused until then.
#[derive(Debug)]
pub struct Throttle {
    limit: u32,
    window: Duration,
    capacity: usize,
    watched: Mutex<HashMap<String, Watched>>,
}

#[derive(Debug, Clone, Copy)]
struct Watched {
    /// When the window closes.
    until: Timestamp,
    count: u32,
    /// Whether a refusal in this window has been reported as the first.
    told: bool,
}

/// A key that may not try again yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    /// How long until it may.
    pub retry_after: Duration,
    /// Whether this is the first time the key is refused in its window.
    /// Whoever writes refusals to the log writes this one and no other: a
    /// client that keeps asking after it was refused must not be able to
    /// fill the disk with lines that say so.
    pub first: bool,
}

impl Throttle {
    /// A throttle that allows `limit` attempts per `window`.
    pub fn new(limit: u32, window: Duration) -> Self {
        Self::with_capacity(limit, window, CAPACITY)
    }

    fn with_capacity(limit: u32, window: Duration, capacity: usize) -> Self {
        Self {
            limit,
            window,
            capacity,
            watched: Mutex::new(HashMap::new()),
        }
    }

    /// Whether `key` may try at `now`. Counts nothing.
    ///
    /// When the table is full of keys whose windows are open, a key that is
    /// not in it is refused: counting it is impossible, and letting it pass
    /// uncounted would let anyone switch the limits off by flooding the
    /// table. That costs new sign-ins during a large attack and nothing
    /// else.
    pub fn check(&self, key: &str, now: Timestamp) -> Result<(), Refused> {
        self.check_in(&mut self.lock(), key, now)
    }

    fn check_in(
        &self,
        watched: &mut HashMap<String, Watched>,
        key: &str,
        now: Timestamp,
    ) -> Result<(), Refused> {
        if let Some(entry) = watched.get_mut(key).filter(|entry| entry.until > now) {
            if entry.count < self.limit {
                return Ok(());
            }
            let first = !entry.told;
            entry.told = true;
            return Err(Refused {
                retry_after: entry.until - now,
                first,
            });
        }
        if Self::is_full(watched, self.capacity, now) {
            // A key that has no place has no window to remember having
            // been refused in, so none of its refusals is the first.
            return Err(Refused {
                retry_after: self.window,
                first: false,
            });
        }
        Ok(())
    }

    /// Counts `weight` attempts by `key` at `now`.
    pub fn count(&self, key: &str, weight: u32, now: Timestamp) {
        self.count_in(&mut self.lock(), key, weight, now);
    }

    fn count_in(
        &self,
        watched: &mut HashMap<String, Watched>,
        key: &str,
        weight: u32,
        now: Timestamp,
    ) {
        match watched.get_mut(key).filter(|entry| entry.until > now) {
            Some(entry) => entry.count = entry.count.saturating_add(weight),
            None => {
                if Self::is_full(watched, self.capacity, now) {
                    return;
                }
                let entry = Watched {
                    until: now + self.window,
                    count: weight,
                    told: false,
                };
                watched.insert(key.to_owned(), entry);
            }
        }
    }

    /// Counts one attempt by `key` if it may try, and says whether it may.
    ///
    /// Asking and counting are one step: of any number of attempts that
    /// arrive at the same moment, no more than the limit get through.
    pub fn attempt(&self, key: &str, now: Timestamp) -> Result<(), Refused> {
        let mut watched = self.lock();
        self.check_in(&mut watched, key, now)?;
        self.count_in(&mut watched, key, 1, now);
        Ok(())
    }

    /// Whether there is no room for another key, after forgetting every key
    /// whose window has closed.
    fn is_full(watched: &mut HashMap<String, Watched>, capacity: usize, now: Timestamp) -> bool {
        if watched.len() < capacity {
            return false;
        }
        watched.retain(|_, entry| entry.until > now);
        watched.len() >= capacity
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Watched>> {
        // A panic while the lock was held leaves counts that are at worst
        // one attempt off.
        self.watched.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn at(minute: i64) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap() + Duration::minutes(minute)
    }

    #[test]
    fn a_key_gets_its_attempts_and_then_waits_for_the_window_to_close() {
        let throttle = Throttle::new(3, Duration::minutes(15));

        for minute in [0, 1, 2] {
            assert_eq!(throttle.attempt("dario", at(minute)), Ok(()));
        }
        // The window opened at minute 0 and closes at minute 15.
        assert_eq!(
            throttle.attempt("dario", at(3)),
            Err(Refused {
                retry_after: Duration::minutes(12),
                first: true,
            })
        );
        assert_eq!(
            throttle.check("dario", at(14)),
            Err(Refused {
                retry_after: Duration::minutes(1),
                first: false,
            })
        );
        // Being refused does not push the window out.
        assert_eq!(throttle.attempt("dario", at(15)), Ok(()));
        assert_eq!(throttle.attempt("dario", at(16)), Ok(()));
    }

    #[test]
    fn keys_are_counted_apart_and_checking_counts_nothing() {
        let throttle = Throttle::new(1, Duration::minutes(15));
        for _ in 0..10 {
            assert_eq!(throttle.check("dario", at(0)), Ok(()));
        }
        assert_eq!(throttle.attempt("dario", at(0)), Ok(()));
        assert!(throttle.attempt("dario", at(1)).is_err());
        assert_eq!(throttle.attempt("anna", at(1)), Ok(()));
    }

    #[test]
    fn a_heavy_attempt_counts_for_many() {
        let throttle = Throttle::new(100, Duration::hours(24));
        throttle.count("dario", 50, at(0));
        assert_eq!(throttle.check("dario", at(1)), Ok(()));
        throttle.count("dario", 50, at(1));
        assert!(throttle.check("dario", at(2)).is_err());
        // A count that overflows stays at the top instead of starting again.
        throttle.count("dario", u32::MAX, at(2));
        assert!(throttle.check("dario", at(3)).is_err());
    }

    #[test]
    fn only_the_first_refusal_of_a_window_is_marked_as_the_first() {
        let throttle = Throttle::new(2, Duration::minutes(1));
        let firsts = |key: &str, minute: i64, times: usize| {
            (0..times)
                .filter_map(|_| throttle.attempt(key, at(minute)).err())
                .filter(|refused| refused.first)
                .count()
        };

        // Two get through; of the thousand refusals after them, one is
        // the first.
        assert_eq!(firsts("203.0.113.9", 0, 1002), 1);
        // Another key has a first refusal of its own.
        assert_eq!(firsts("198.51.100.7", 0, 50), 1);
        assert_eq!(firsts("203.0.113.9", 0, 50), 0);
        // And so has the next window.
        assert_eq!(firsts("203.0.113.9", 1, 1002), 1);
    }

    #[test]
    fn a_full_table_refuses_new_keys_until_windows_close() {
        let throttle = Throttle::with_capacity(5, Duration::minutes(15), 3);
        for key in ["a", "b", "c"] {
            assert_eq!(throttle.attempt(key, at(0)), Ok(()));
        }

        // No room to count a fourth key, so it may not try.
        assert_eq!(
            throttle.attempt("d", at(1)),
            Err(Refused {
                retry_after: Duration::minutes(15),
                first: false,
            })
        );
        // Keys that are watched carry on.
        assert_eq!(throttle.attempt("a", at(1)), Ok(()));
        // When the windows have closed there is room again.
        assert_eq!(throttle.attempt("d", at(15)), Ok(()));
        assert_eq!(throttle.lock().len(), 1);
    }
}
