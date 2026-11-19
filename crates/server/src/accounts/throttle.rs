//! Counting attempts, so that guessing passwords, collecting invite codes
//! or filling the disk is slow.
//!
//! The counters live in this process. That is right while one process
//! serves the API; when several do, they move into the database.
//!
//! An attempt is counted under one or more keys, each with its own limit:
//! a wrong password under the handle it was for and under the address it
//! came from. It is counted under all of them or under none, in one step,
//! so that of any number of attempts that arrive at the same moment no
//! more than the limit get through.
//!
//! Where finding out whether an attempt was a good one is itself the
//! expensive part, as with a password, the attempt is *reserved* before
//! the work and given back if it turns out well. Counting afterwards would
//! let a thousand attempts in while the first is still being looked at.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::Duration;
use venti_domain::Timestamp;

/// How many keys a throttle watches at most. Each takes about a hundred
/// bytes, so a full table is a few megabytes.
const CAPACITY: usize = 50_000;

/// One thing an attempt is counted under, and how many attempts it allows
/// within the throttle's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counted<'a> {
    /// What is counted: a handle, an address, an account.
    pub key: &'a str,
    /// The attempts the key has in one window.
    pub limit: u32,
}

impl<'a> Counted<'a> {
    /// `key`, allowed `limit` attempts in a window.
    pub fn new(key: &'a str, limit: u32) -> Self {
        Self { key, limit }
    }
}

/// Allows each key a number of attempts within a window of time.
///
/// The window of a key opens with its first counted attempt and closes
/// `window` later, when the key is forgotten. A key that has used up its
/// attempts is refused until then.
#[derive(Debug)]
pub struct Throttle {
    table: Mutex<Table>,
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

/// An attempt that was counted before its outcome was known. Hand it to
/// [`Throttle::give_back`] if it turned out well; drop it and it stays
/// counted.
#[derive(Debug)]
#[must_use = "a reserved attempt stays counted unless it is given back"]
pub struct Reserved {
    /// The keys it was counted under, each with the end of the window it
    /// was counted in.
    slots: Vec<(String, Timestamp)>,
}

impl Throttle {
    /// A throttle whose windows are `window` long.
    pub fn new(window: Duration) -> Self {
        Self::with_capacity(window, CAPACITY)
    }

    fn with_capacity(window: Duration, capacity: usize) -> Self {
        Self {
            table: Mutex::new(Table {
                window,
                capacity,
                watched: HashMap::new(),
            }),
        }
    }

    /// Whether an attempt under `keys` may be made at `now`. Counts
    /// nothing.
    pub fn check(&self, keys: &[Counted], now: Timestamp) -> Result<(), Refused> {
        self.lock().check(keys, now)
    }

    /// Counts `weight` attempts under each of `keys` at `now`, whether or
    /// not they are allowed: for what has already happened.
    pub fn count(&self, keys: &[Counted], weight: u32, now: Timestamp) {
        // Nothing of this is ever given back.
        let _ = self.lock().add(keys, weight, now);
    }

    /// Counts one attempt under each of `keys` if all of them allow it, and
    /// says whether they do.
    pub fn attempt(&self, keys: &[Counted], now: Timestamp) -> Result<(), Refused> {
        self.reserve(keys, now).map(drop)
    }

    /// Like [`attempt`](Self::attempt), for an attempt whose outcome is not
    /// known yet: it can be given back.
    pub fn reserve(&self, keys: &[Counted], now: Timestamp) -> Result<Reserved, Refused> {
        let mut table = self.lock();
        table.check(keys, now)?;
        Ok(table.add(keys, 1, now))
    }

    /// Takes back an attempt that turned out well.
    pub fn give_back(&self, reserved: Reserved) {
        self.lock().remove(&reserved);
    }

    fn lock(&self) -> MutexGuard<'_, Table> {
        // A panic while the lock was held leaves counts that are at worst
        // one attempt off.
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reserves one attempt in two throttles at once: in both or in neither,
/// with both tables held for the moment it takes.
///
/// Every caller that needs two throttles goes through here, which is what
/// keeps the order of the two locks the same everywhere.
pub fn reserve_both(
    first: (&Throttle, &[Counted]),
    second: (&Throttle, &[Counted]),
    now: Timestamp,
) -> Result<(Reserved, Reserved), Refused> {
    let mut first_table = first.0.lock();
    let mut second_table = second.0.lock();
    // Ask both before answering, so that a refusal is marked as told in
    // each table that refuses.
    let asked = (
        first_table.check(first.1, now),
        second_table.check(second.1, now),
    );
    match asked {
        (Ok(()), Ok(())) => Ok((
            first_table.add(first.1, 1, now),
            second_table.add(second.1, 1, now),
        )),
        (Err(refused), Ok(())) | (Ok(()), Err(refused)) => Err(refused),
        (Err(one), Err(other)) => Err(one.and(other)),
    }
}

impl Refused {
    /// Two refusals of one attempt as one: wait for the longer, and it is
    /// the first if either is.
    fn and(self, other: Self) -> Self {
        Self {
            retry_after: self.retry_after.max(other.retry_after),
            first: self.first || other.first,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Watched {
    /// When the window closes.
    until: Timestamp,
    count: u32,
    /// Whether a refusal in this window has been reported as the first.
    told: bool,
}

/// The keys a throttle watches. Whoever holds it holds the lock.
#[derive(Debug)]
struct Table {
    window: Duration,
    capacity: usize,
    watched: HashMap<String, Watched>,
}

impl Table {
    /// Whether every one of `keys` has an attempt left.
    ///
    /// When the table is full of keys whose windows are open, a key that is
    /// not in it is refused: counting it is impossible, and letting it pass
    /// uncounted would let anyone switch the limits off by flooding the
    /// table. That costs new sign-ins during a large attack and nothing
    /// else.
    fn check(&mut self, keys: &[Counted], now: Timestamp) -> Result<(), Refused> {
        let mut refused: Option<Refused> = None;
        for counted in keys {
            let open = self
                .watched
                .get_mut(counted.key)
                .filter(|entry| entry.until > now);
            let this = if let Some(entry) = open {
                if entry.count < counted.limit {
                    continue;
                }
                let first = !entry.told;
                entry.told = true;
                Refused {
                    retry_after: entry.until - now,
                    first,
                }
            } else if self.is_full(now) {
                // A key that has no place has no window to remember having
                // been refused in: none of its refusals is the first.
                Refused {
                    retry_after: self.window,
                    first: false,
                }
            } else {
                continue;
            };
            refused = Some(refused.map_or(this, |earlier| earlier.and(this)));
        }
        refused.map_or(Ok(()), Err)
    }

    /// Counts `weight` under each of `keys`, and says where.
    fn add(&mut self, keys: &[Counted], weight: u32, now: Timestamp) -> Reserved {
        let mut slots = Vec::with_capacity(keys.len());
        for counted in keys {
            if let Some(entry) = self
                .watched
                .get_mut(counted.key)
                .filter(|entry| entry.until > now)
            {
                entry.count = entry.count.saturating_add(weight);
                slots.push((counted.key.to_owned(), entry.until));
                continue;
            }
            if self.is_full(now) {
                continue;
            }
            let until = now + self.window;
            let entry = Watched {
                until,
                count: weight,
                told: false,
            };
            self.watched.insert(counted.key.to_owned(), entry);
            slots.push((counted.key.to_owned(), until));
        }
        Reserved { slots }
    }

    /// Takes one attempt back from each key it was counted under, where the
    /// window it was counted in is still the open one. A key that is left
    /// with none is forgotten: nothing has happened under it.
    fn remove(&mut self, reserved: &Reserved) {
        for (key, until) in &reserved.slots {
            let Some(entry) = self.watched.get_mut(key) else {
                continue;
            };
            if entry.until != *until {
                continue;
            }
            entry.count = entry.count.saturating_sub(1);
            if entry.count == 0 {
                self.watched.remove(key);
            }
        }
    }

    /// Whether there is no room for another key, after forgetting every key
    /// whose window has closed.
    fn is_full(&mut self, now: Timestamp) -> bool {
        if self.watched.len() < self.capacity {
            return false;
        }
        self.watched.retain(|_, entry| entry.until > now);
        self.watched.len() >= self.capacity
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn at(minute: i64) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap() + Duration::minutes(minute)
    }

    /// One key with a limit, as most callers count.
    fn one(key: &str, limit: u32) -> [Counted<'_>; 1] {
        [Counted::new(key, limit)]
    }

    #[test]
    fn a_key_gets_its_attempts_and_then_waits_for_the_window_to_close() {
        let throttle = Throttle::new(Duration::minutes(15));
        let dario = one("dario", 3);

        for minute in [0, 1, 2] {
            assert_eq!(throttle.attempt(&dario, at(minute)), Ok(()));
        }
        // The window opened at minute 0 and closes at minute 15.
        assert_eq!(
            throttle.attempt(&dario, at(3)),
            Err(Refused {
                retry_after: Duration::minutes(12),
                first: true,
            })
        );
        assert_eq!(
            throttle.check(&dario, at(14)),
            Err(Refused {
                retry_after: Duration::minutes(1),
                first: false,
            })
        );
        // Being refused does not push the window out.
        assert_eq!(throttle.attempt(&dario, at(15)), Ok(()));
        assert_eq!(throttle.attempt(&dario, at(16)), Ok(()));
    }

    #[test]
    fn keys_are_counted_apart_and_checking_counts_nothing() {
        let throttle = Throttle::new(Duration::minutes(15));
        for _ in 0..10 {
            assert_eq!(throttle.check(&one("dario", 1), at(0)), Ok(()));
        }
        assert_eq!(throttle.attempt(&one("dario", 1), at(0)), Ok(()));
        assert!(throttle.attempt(&one("dario", 1), at(1)).is_err());
        assert_eq!(throttle.attempt(&one("anna", 1), at(1)), Ok(()));
    }

    #[test]
    fn a_heavy_attempt_counts_for_many() {
        let throttle = Throttle::new(Duration::hours(24));
        let dario = one("dario", 100);
        throttle.count(&dario, 50, at(0));
        assert_eq!(throttle.check(&dario, at(1)), Ok(()));
        throttle.count(&dario, 50, at(1));
        assert!(throttle.check(&dario, at(2)).is_err());
        // A count that overflows stays at the top instead of starting again.
        throttle.count(&dario, u32::MAX, at(2));
        assert!(throttle.check(&dario, at(3)).is_err());
    }

    #[test]
    fn only_the_first_refusal_of_a_window_is_marked_as_the_first() {
        let throttle = Throttle::new(Duration::minutes(1));
        let firsts = |key: &str, minute: i64, times: usize| {
            (0..times)
                .filter_map(|_| throttle.attempt(&one(key, 2), at(minute)).err())
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
    fn an_attempt_is_counted_under_all_of_its_keys_or_under_none() {
        let throttle = Throttle::new(Duration::hours(1));
        let both = |address: &'static str| [Counted::new("dario", 2), Counted::new(address, 5)];

        assert_eq!(throttle.attempt(&both("203.0.113.9"), at(0)), Ok(()));
        assert_eq!(throttle.attempt(&both("198.51.100.7"), at(0)), Ok(()));
        // The handle is used up, whatever address asks: the address is not
        // charged for an attempt that was not made.
        for _ in 0..20 {
            assert!(throttle.attempt(&both("192.0.2.1"), at(1)).is_err());
        }
        assert_eq!(throttle.check(&one("192.0.2.1", 1), at(1)), Ok(()));
        assert_eq!(throttle.lock().watched.len(), 3);
    }

    #[test]
    fn a_reserved_attempt_counts_until_it_is_given_back() {
        let throttle = Throttle::new(Duration::hours(1));
        let dario = one("dario", 3);

        // Three are being looked at; a fourth has to wait for their outcome.
        let reserved: Vec<Reserved> = (0..3)
            .map(|_| throttle.reserve(&dario, at(0)).unwrap())
            .collect();
        assert!(throttle.reserve(&dario, at(0)).is_err());

        // Two turn out well, one does not.
        let mut reserved = reserved.into_iter();
        throttle.give_back(reserved.next().unwrap());
        throttle.give_back(reserved.next().unwrap());
        drop(reserved);
        assert_eq!(throttle.attempt(&dario, at(1)), Ok(()));
        assert_eq!(throttle.attempt(&dario, at(1)), Ok(()));
        assert!(throttle.attempt(&dario, at(1)).is_err());
    }

    #[test]
    fn a_key_whose_attempts_were_all_given_back_is_forgotten() {
        let throttle = Throttle::new(Duration::hours(1));
        let reserved = throttle.reserve(&one("dario", 3), at(0)).unwrap();
        assert_eq!(throttle.lock().watched.len(), 1);

        throttle.give_back(reserved);

        // No window was opened: an hour of right passwords leaves nothing.
        assert!(throttle.lock().watched.is_empty());
    }

    #[test]
    fn an_attempt_is_given_back_to_the_window_it_was_counted_in_only() {
        let throttle = Throttle::new(Duration::minutes(15));
        let dario = one("dario", 2);
        let old = throttle.reserve(&dario, at(0)).unwrap();

        // The window closes and another opens before the outcome is known.
        assert_eq!(throttle.attempt(&dario, at(20)), Ok(()));
        assert_eq!(throttle.attempt(&dario, at(20)), Ok(()));
        throttle.give_back(old);

        assert!(throttle.attempt(&dario, at(21)).is_err());
    }

    #[test]
    fn two_throttles_take_an_attempt_together_or_not_at_all() {
        let handles = Throttle::new(Duration::hours(1));
        let addresses = Throttle::new(Duration::hours(1));
        let attempt = |handle: &'static str, address: &'static str| {
            reserve_both(
                (&handles, &one(handle, 2)),
                (&addresses, &one(address, 3)),
                at(0),
            )
        };

        let first = attempt("dario", "203.0.113.9").unwrap();
        let _second = attempt("dario", "203.0.113.9").unwrap();
        // The handle is used up: the address is not charged.
        assert!(attempt("dario", "203.0.113.9").is_err());
        let _third = attempt("anna", "203.0.113.9").unwrap();
        // The address is used up: the handle is not charged.
        assert!(attempt("carla", "203.0.113.9").is_err());
        assert_eq!(handles.check(&one("carla", 1), at(0)), Ok(()));
        assert_eq!(handles.lock().watched.len(), 2);

        // What is given back is given back in both.
        handles.give_back(first.0);
        addresses.give_back(first.1);
        assert!(attempt("dario", "203.0.113.9").is_ok());

        // No handle is named: the address alone is asked.
        assert!(
            reserve_both(
                (&handles, &[]),
                (&addresses, &one("198.51.100.7", 1)),
                at(0)
            )
            .is_ok()
        );
        assert!(
            reserve_both(
                (&handles, &[]),
                (&addresses, &one("198.51.100.7", 1)),
                at(0)
            )
            .is_err()
        );
    }

    #[test]
    fn a_full_table_refuses_new_keys_until_windows_close() {
        let throttle = Throttle::with_capacity(Duration::minutes(15), 3);
        for key in ["a", "b", "c"] {
            assert_eq!(throttle.attempt(&one(key, 5), at(0)), Ok(()));
        }

        // No room to count a fourth key, so it may not try.
        assert_eq!(
            throttle.attempt(&one("d", 5), at(1)),
            Err(Refused {
                retry_after: Duration::minutes(15),
                first: false,
            })
        );
        // Keys that are watched carry on.
        assert_eq!(throttle.attempt(&one("a", 5), at(1)), Ok(()));
        // When the windows have closed there is room again.
        assert_eq!(throttle.attempt(&one("d", 5), at(15)), Ok(()));
        assert_eq!(throttle.lock().watched.len(), 1);
    }
}
