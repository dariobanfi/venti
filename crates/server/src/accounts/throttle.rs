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

/// How many attempts the keys without a place share, as a multiple of each
/// one's own limit, while a table is full. See [`Throttle`].
const SHARED_FACTOR: u32 = 100;

/// How long a full table is left alone before it is searched again for
/// windows that have closed. The search reads every key with the lock
/// held; done for every newcomer it would be a burden of its own.
const SWEEP_PAUSE: Duration = Duration::seconds(1);

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
///
/// **The table has a size**, so that whoever sends attempts under ever new
/// keys cannot take the server's memory. Measured on 2026-11-19: a key
/// costs about 110 bytes, so a table of 100,000 keys, the size the server
/// uses, is 11 MB when full.
///
/// **When the table is full**, a key that has no place in it is counted
/// in one count that all such keys share, and is allowed a hundred times
/// its own limit there, together with all the others. Keys that have a
/// place carry on as before. Two simpler rules were worse. Refusing every
/// newcomer, which this did until the review of 2026-11-19, lets whoever
/// fills the table lock out everyone the server has not seen: 50,000
/// addresses did it in 13 seconds. Letting newcomers pass uncounted lets
/// whoever fills the table switch the limits off. With the shared count,
/// filling the table buys an attacker a hundred keys' worth of attempts
/// per window and no more, and costs everyone else nothing until those
/// are used up as well; from then on newcomers are refused until the
/// shared window closes. The table says so in the log, once per window.
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
    /// Whether it was refused for what others did: the table is full, the
    /// key has no place in it, and the attempts such keys share are used
    /// up.
    pub shared: bool,
}

/// An attempt that was counted before its outcome was known. Hand it to
/// [`Throttle::give_back`] if it turned out well; drop it and it stays
/// counted.
#[derive(Debug)]
#[must_use = "a reserved attempt stays counted unless it is given back"]
pub struct Reserved {
    /// Where it was counted, each with the end of the window it was
    /// counted in.
    slots: Vec<(Slot, Timestamp)>,
}

/// Where an attempt was counted.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    /// Under a key of its own.
    Key(String),
    /// In the count that keys without a place share.
    Shared,
}

impl Throttle {
    /// A throttle for `what` (the word the log uses for its keys, such as
    /// "addresses that ask the connector"), whose windows are `window`
    /// long and which watches `capacity` keys at most.
    pub fn new(what: &'static str, window: Duration, capacity: usize) -> Self {
        Self {
            table: Mutex::new(Table {
                what,
                window,
                capacity,
                watched: HashMap::new(),
                shared: None,
                swept_at: None,
                tidied_at: None,
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

    /// Like [`attempt`](Self::attempt), for an attempt that counts for
    /// `weight`: a request that brings several of what is counted, such as
    /// a batch of live fixes. It is let through whole while every key has
    /// an attempt left, and then counted whole, in one step. A key can so
    /// end its window up to `weight` less one over its limit, and no
    /// further, however many such requests arrive at the same moment.
    pub fn spend(&self, keys: &[Counted], weight: u32, now: Timestamp) -> Result<(), Refused> {
        let mut table = self.lock();
        table.check(keys, now)?;
        // Nothing of this is ever given back.
        let _ = table.add(keys, weight, now);
        Ok(())
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
    /// Two refusals of one attempt as one: wait for the longer; it is the
    /// first if either is, and the doing of others only if both are.
    fn and(self, other: Self) -> Self {
        Self {
            retry_after: self.retry_after.max(other.retry_after),
            first: self.first || other.first,
            shared: self.shared && other.shared,
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

impl Watched {
    /// What an attempt with the limit `limit` hears from this count.
    fn refusal(&mut self, limit: u32, shared: bool, now: Timestamp) -> Option<Refused> {
        if self.count < limit {
            return None;
        }
        let first = !self.told;
        self.told = true;
        Some(Refused {
            retry_after: self.until - now,
            first,
            shared,
        })
    }
}

/// The keys a throttle watches. Whoever holds it holds the lock.
#[derive(Debug)]
struct Table {
    what: &'static str,
    window: Duration,
    capacity: usize,
    watched: HashMap<String, Watched>,
    /// What the keys without a place have used together, while the table
    /// is full.
    shared: Option<Watched>,
    /// When the full table was last searched for closed windows.
    swept_at: Option<Timestamp>,
    /// When closed windows were last forgotten as a matter of routine.
    tidied_at: Option<Timestamp>,
}

/// Where a key is counted at this moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Under itself: it is watched, or there is room to watch it.
    Own,
    /// With the others that found the table full.
    Shared,
}

impl Table {
    /// Whether every one of `keys` has an attempt left.
    fn check(&mut self, keys: &[Counted], now: Timestamp) -> Result<(), Refused> {
        self.tidy(now);
        let mut refused: Option<Refused> = None;
        for counted in keys {
            let this = match self.place(counted.key, now) {
                Place::Own => self
                    .open(counted.key, now)
                    .and_then(|entry| entry.refusal(counted.limit, false, now)),
                Place::Shared => {
                    let limit = counted.limit.saturating_mul(SHARED_FACTOR);
                    self.shared
                        .as_mut()
                        .filter(|shared| shared.until > now)
                        .and_then(|shared| shared.refusal(limit, true, now))
                }
            };
            if let Some(this) = this {
                refused = Some(refused.map_or(this, |earlier| earlier.and(this)));
            }
        }
        refused.map_or(Ok(()), Err)
    }

    /// Counts `weight` under each of `keys`, and says where. Keys that have
    /// no place are counted together, once.
    fn add(&mut self, keys: &[Counted], weight: u32, now: Timestamp) -> Reserved {
        self.tidy(now);
        let mut slots = Vec::with_capacity(keys.len());
        for counted in keys {
            match self.place(counted.key, now) {
                Place::Own => {
                    let until = match self.open(counted.key, now) {
                        Some(entry) => {
                            entry.count = entry.count.saturating_add(weight);
                            entry.until
                        }
                        None => {
                            let until = now + self.window;
                            let entry = Watched {
                                until,
                                count: weight,
                                told: false,
                            };
                            self.watched.insert(counted.key.to_owned(), entry);
                            until
                        }
                    };
                    slots.push((Slot::Key(counted.key.to_owned()), until));
                }
                Place::Shared => {
                    if slots.iter().any(|(slot, _)| *slot == Slot::Shared) {
                        continue;
                    }
                    let until = self.add_to_shared(weight, now);
                    slots.push((Slot::Shared, until));
                }
            }
        }
        Reserved { slots }
    }

    /// Counts `weight` in the count that keys without a place share, and
    /// returns when its window closes.
    fn add_to_shared(&mut self, weight: u32, now: Timestamp) -> Timestamp {
        if let Some(shared) = self.shared.as_mut().filter(|shared| shared.until > now) {
            shared.count = shared.count.saturating_add(weight);
            return shared.until;
        }
        // The first that found no place in this window: worth a line, and
        // one line only until the window closes.
        tracing::warn!(
            "the table of {} is full: {} are being counted. Until some of their windows \
             close, every further one is counted together with the others that have no \
             place, and they share {SHARED_FACTOR} times what one is allowed",
            self.what,
            self.capacity,
        );
        let until = now + self.window;
        self.shared = Some(Watched {
            until,
            count: weight,
            told: false,
        });
        until
    }

    /// Takes one attempt back from wherever it was counted, where the
    /// window it was counted in is still the open one. A key that is left
    /// with none is forgotten: nothing has happened under it.
    fn remove(&mut self, reserved: &Reserved) {
        for (slot, until) in &reserved.slots {
            match slot {
                Slot::Key(key) => {
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
                Slot::Shared => {
                    if let Some(shared) = self.shared.as_mut().filter(|s| s.until == *until) {
                        shared.count = shared.count.saturating_sub(1);
                    }
                }
            }
        }
    }

    /// The count of `key`, if its window is open.
    fn open(&mut self, key: &str, now: Timestamp) -> Option<&mut Watched> {
        self.watched.get_mut(key).filter(|entry| entry.until > now)
    }

    /// Where `key` is counted at `now`.
    fn place(&mut self, key: &str, now: Timestamp) -> Place {
        // A key that is in the table keeps its place, also when its window
        // has closed: the next count opens a new one there.
        if self.watched.contains_key(key) || self.watched.len() < self.capacity {
            return Place::Own;
        }
        // Full. Forget the keys whose windows have closed, but look for
        // them only now and then.
        let due = self
            .swept_at
            .is_none_or(|at| now < at || now - at >= SWEEP_PAUSE);
        if due {
            self.swept_at = Some(now);
            self.watched.retain(|_, entry| entry.until > now);
        }
        if self.watched.len() < self.capacity {
            Place::Own
        } else {
            Place::Shared
        }
    }

    /// Once per window, forgets what has closed and gives memory back, so
    /// that a table that was full once does not stay large for ever.
    fn tidy(&mut self, now: Timestamp) {
        let due = self
            .tidied_at
            .is_none_or(|at| now < at || now - at >= self.window);
        if !due {
            return;
        }
        self.tidied_at = Some(now);
        self.watched.retain(|_, entry| entry.until > now);
        if self.watched.capacity() > 4 * self.watched.len().max(256) {
            self.watched.shrink_to(2 * self.watched.len());
        }
        if self.shared.is_some_and(|shared| shared.until <= now) {
            self.shared = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn at(minute: i64) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap() + Duration::minutes(minute)
    }

    /// A throttle with room for more keys than a test uses.
    fn roomy(window: Duration) -> Throttle {
        Throttle::new("test keys", window, 1000)
    }

    fn watched(throttle: &Throttle) -> usize {
        throttle.lock().watched.len()
    }

    /// One key with a limit, as most callers count.
    fn one(key: &str, limit: u32) -> [Counted<'_>; 1] {
        [Counted::new(key, limit)]
    }

    #[test]
    fn a_key_gets_its_attempts_and_then_waits_for_the_window_to_close() {
        let throttle = roomy(Duration::minutes(15));
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
                shared: false,
            })
        );
        assert_eq!(
            throttle.check(&dario, at(14)),
            Err(Refused {
                retry_after: Duration::minutes(1),
                first: false,
                shared: false,
            })
        );
        // Being refused does not push the window out.
        assert_eq!(throttle.attempt(&dario, at(15)), Ok(()));
        assert_eq!(throttle.attempt(&dario, at(16)), Ok(()));
    }

    #[test]
    fn keys_are_counted_apart_and_checking_counts_nothing() {
        let throttle = roomy(Duration::minutes(15));
        for _ in 0..10 {
            assert_eq!(throttle.check(&one("dario", 1), at(0)), Ok(()));
        }
        assert_eq!(throttle.attempt(&one("dario", 1), at(0)), Ok(()));
        assert!(throttle.attempt(&one("dario", 1), at(1)).is_err());
        assert_eq!(throttle.attempt(&one("anna", 1), at(1)), Ok(()));
    }

    #[test]
    fn a_heavy_attempt_counts_for_many() {
        let throttle = roomy(Duration::hours(24));
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
    fn a_request_that_brings_many_is_let_through_whole_or_not_at_all() {
        let throttle = roomy(Duration::minutes(1));
        let dario = one("dario", 300);

        // Batches of 120: the third is still let through, at 240 of 300,
        // and takes the minute to 360.
        for _ in 0..3 {
            assert_eq!(throttle.spend(&dario, 120, at(0)), Ok(()));
        }
        // From then on nothing is, not even a single one.
        let refused = throttle.spend(&dario, 1, at(0)).unwrap_err();
        assert_eq!(refused.retry_after, Duration::minutes(1));
        assert!(refused.first);
        assert!(throttle.spend(&dario, 120, at(0)).is_err());
        // Another key is not affected, and the next minute is a new one.
        assert_eq!(throttle.spend(&one("anna", 300), 120, at(0)), Ok(()));
        assert_eq!(throttle.spend(&dario, 120, at(1)), Ok(()));
    }

    #[test]
    fn only_the_first_refusal_of_a_window_is_marked_as_the_first() {
        let throttle = roomy(Duration::minutes(1));
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
        let throttle = roomy(Duration::hours(1));
        let both = |address: &'static str| [Counted::new("dario", 2), Counted::new(address, 5)];

        assert_eq!(throttle.attempt(&both("203.0.113.9"), at(0)), Ok(()));
        assert_eq!(throttle.attempt(&both("198.51.100.7"), at(0)), Ok(()));
        // The handle is used up, whatever address asks: the address is not
        // charged for an attempt that was not made.
        for _ in 0..20 {
            assert!(throttle.attempt(&both("192.0.2.1"), at(1)).is_err());
        }
        assert_eq!(throttle.check(&one("192.0.2.1", 1), at(1)), Ok(()));
        assert_eq!(watched(&throttle), 3);
    }

    #[test]
    fn a_reserved_attempt_counts_until_it_is_given_back() {
        let throttle = roomy(Duration::hours(1));
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
        let throttle = roomy(Duration::hours(1));
        let reserved = throttle.reserve(&one("dario", 3), at(0)).unwrap();
        assert_eq!(watched(&throttle), 1);

        throttle.give_back(reserved);

        // No window was opened: an hour of right passwords leaves nothing.
        assert!(throttle.lock().watched.is_empty());
    }

    #[test]
    fn an_attempt_is_given_back_to_the_window_it_was_counted_in_only() {
        let throttle = roomy(Duration::minutes(15));
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
        let handles = roomy(Duration::hours(1));
        let addresses = roomy(Duration::hours(1));
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
        assert_eq!(watched(&handles), 2);

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
    fn a_full_table_counts_newcomers_together_instead_of_refusing_them() {
        let throttle = Throttle::new("test keys", Duration::minutes(15), 3);
        for key in ["a", "b", "c"] {
            assert_eq!(throttle.attempt(&one(key, 2), at(0)), Ok(()));
        }

        // No room to count a fourth key by itself. It is not refused for
        // that, and neither is the fifth, or the five hundredth.
        for n in 0..199 {
            let key = format!("newcomer {n}");
            assert_eq!(throttle.attempt(&one(&key, 2), at(1)), Ok(()), "{key}");
        }
        assert_eq!(watched(&throttle), 3);
        // Keys that have a place carry on under their own limit.
        assert_eq!(throttle.attempt(&one("a", 2), at(1)), Ok(()));
        assert!(throttle.attempt(&one("a", 2), at(1)).is_err());

        // What the newcomers share is a hundred times what one may: 200
        // here. Then they are refused, and told that it is not their doing.
        assert_eq!(throttle.attempt(&one("the 200th", 2), at(2)), Ok(()));
        assert_eq!(
            throttle.attempt(&one("the 201st", 2), at(2)),
            Err(Refused {
                retry_after: Duration::minutes(14),
                first: true,
                shared: true,
            })
        );
        assert_eq!(
            throttle.attempt(&one("the 202nd", 2), at(3)),
            Err(Refused {
                retry_after: Duration::minutes(13),
                first: false,
                shared: true,
            })
        );
        // A key that is allowed more is allowed more of the shared count.
        assert_eq!(throttle.attempt(&one("patient", 3), at(3)), Ok(()));

        // When windows have closed there is room again, and the newcomer is
        // counted by itself.
        assert_eq!(throttle.attempt(&one("d", 2), at(15)), Ok(()));
        assert_eq!(throttle.attempt(&one("d", 2), at(15)), Ok(()));
        assert!(throttle.attempt(&one("d", 2), at(15)).is_err());
        assert_eq!(watched(&throttle), 1);
    }

    #[test]
    fn an_attempt_without_a_place_is_counted_once_and_can_be_given_back() {
        let throttle = Throttle::new("test keys", Duration::minutes(15), 2);
        assert_eq!(throttle.attempt(&one("a", 1), at(0)), Ok(()));
        assert_eq!(throttle.attempt(&one("b", 1), at(0)), Ok(()));
        let shared = |throttle: &Throttle| throttle.lock().shared.map(|shared| shared.count);

        // An address and its network, neither with a place: one attempt.
        let both = [Counted::new("address", 1), Counted::new("network", 4)];
        let reserved = throttle.reserve(&both, at(1)).unwrap();
        assert_eq!(shared(&throttle), Some(1));
        throttle.count(&both, 5, at(1));
        assert_eq!(shared(&throttle), Some(6));

        throttle.give_back(reserved);
        assert_eq!(shared(&throttle), Some(5));

        // One key with a place and one without: each is counted where it is.
        let mixed = [Counted::new("a", 9), Counted::new("network", 4)];
        assert_eq!(throttle.attempt(&mixed, at(2)), Ok(()));
        assert_eq!(shared(&throttle), Some(6));
        assert_eq!(throttle.lock().watched["a"].count, 2);
    }

    #[test]
    fn a_full_table_is_searched_for_closed_windows_only_now_and_then() {
        let throttle = Throttle::new("test keys", Duration::seconds(10), 2);
        let second = |tenths: i64| at(0) + Duration::milliseconds(tenths * 100);
        let has = |key: &str| throttle.lock().watched.contains_key(key);
        assert_eq!(throttle.attempt(&one("a", 1), second(0)), Ok(()));
        assert_eq!(throttle.attempt(&one("b", 1), second(50)), Ok(()));
        // The routine tidying, once per window, takes "a" away.
        assert_eq!(throttle.attempt(&one("c", 1), second(105)), Ok(()));
        assert!(has("b") && has("c") && !has("a"));

        // The first newcomer that finds the table full has it searched.
        // Nothing has closed.
        assert_eq!(throttle.attempt(&one("d", 1), second(145)), Ok(()));
        assert_eq!(throttle.lock().swept_at, Some(second(145)));
        // The window of "b" closes at 15 seconds. A newcomer just after it
        // does not have the table searched again: it was, within the second.
        assert_eq!(throttle.attempt(&one("e", 1), second(152)), Ok(()));
        assert_eq!(throttle.lock().swept_at, Some(second(145)));
        assert!(has("b") && !has("e"));
        // A second after the search the next one does, and gets a place.
        assert_eq!(throttle.attempt(&one("f", 1), second(155)), Ok(()));
        assert!(has("f") && has("c") && !has("b"));
    }

    #[test]
    fn a_table_that_was_full_gives_its_memory_back() {
        let throttle = Throttle::new("test keys", Duration::minutes(1), 50_000);
        for n in 0..50_000 {
            let key = format!("2001:db8:{:x}:{:x}::/64", n >> 16, n & 0xffff);
            assert_eq!(throttle.attempt(&one(&key, 1), at(0)), Ok(()));
        }
        assert!(throttle.lock().watched.capacity() >= 50_000);

        // The next visitor after the windows have closed tidies up.
        assert_eq!(throttle.attempt(&one("198.51.100.7", 1), at(2)), Ok(()));

        assert_eq!(watched(&throttle), 1);
        assert!(throttle.lock().watched.capacity() < 1024);
    }
}
