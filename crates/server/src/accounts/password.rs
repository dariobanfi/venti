//! Passwords: how one is hashed, and how one is checked without telling
//! anything about the account it was checked for.
//!
//! The hash is Argon2id. Its cost is the configuration OWASP's Password
//! Storage Cheat Sheet lists as a minimum, 19 MiB of memory, two passes and
//! one lane (read 2026-11-19 at
//! <https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html>).
//! A hash is stored in the standard text form, which carries its
//! parameters and salt: when the cost is raised, old hashes still verify.
//!
//! Hashing is slow and hungry on purpose, so it never runs on the threads
//! that answer requests, only a few run at once, and only a few wait for
//! their turn: a request that finds the queue long is told at once that
//! the server is busy.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use tokio::sync::Semaphore;
use unicode_normalization::UnicodeNormalization;
use venti_api_types::v1::{Handle, Password, PasswordError};

/// How many passwords are hashed at the same time. Each takes 19 MiB and a
/// whole core for some tens of milliseconds; more requests wait their turn.
const AT_ONCE: usize = 4;

/// How many requests wait for their turn at most. With four at once and
/// some tens of milliseconds each, the last in a full queue waits under a
/// second. A request that finds the queue full is told that the server is
/// busy before it has cost anything, where it used to wait five seconds to
/// be told the same.
const MAX_WAITING: usize = 32;

/// How long a request waits for its turn before the server says it is busy.
/// With the queue bounded this is not reached unless hashing is far slower
/// than it should be.
const PATIENCE: Duration = Duration::from_secs(5);

/// What hashing a password costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    /// Memory in kibibytes.
    pub memory_kib: u32,
    /// Passes over the memory.
    pub passes: u32,
}

impl Cost {
    /// What the server uses: OWASP's minimum for Argon2id, m=19456 (19 MiB),
    /// t=2, p=1.
    pub const OWASP: Self = Self {
        memory_kib: 19_456,
        passes: 2,
    };

    /// Almost nothing. For tests that sign in many times and are not about
    /// the hash. Hashes made with it are worthless.
    pub const TESTS_ONLY: Self = Self {
        memory_kib: 8,
        passes: 1,
    };
}

/// Why a password could not be hashed or checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HashError {
    /// Too many passwords are being hashed; try again shortly.
    #[error("too many passwords are being checked at once")]
    Busy,
    /// The hashing itself failed, which no input should be able to cause.
    #[error("hashing a password failed")]
    Failed,
}

/// Why a new password is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// Too short or too long.
    #[error("{0}")]
    Length(#[from] PasswordError),
    /// It is the account's handle, the first thing anyone would try.
    #[error("the password must not be the handle")]
    IsTheHandle,
}

/// Whether `password` may become the password of the account with `handle`.
///
/// The rule is length and nothing else, as NIST SP 800-63B-4 asks: no
/// required mix of characters. What that document also asks for and this
/// does not do is a comparison with a list of passwords known to be common
/// or leaked; the handle is the one entry on that list.
pub fn check_new(password: &Password, handle: &Handle) -> Result<(), Refusal> {
    password.check()?;
    let same = normalise(password.expose())
        .trim()
        .eq_ignore_ascii_case(handle.as_str());
    if same {
        return Err(Refusal::IsTheHandle);
    }
    Ok(())
}

/// Brings a password to one spelling before it is hashed.
///
/// The same accented letter can arrive as one character or as a letter and
/// a combining mark, depending on the keyboard. Composing them (Unicode
/// form NFC, which NIST SP 800-63B-4 recommends) makes both the same
/// password. Nothing else is changed: no trimming, no case folding.
fn normalise(password: &str) -> String {
    password.nfc().collect()
}

/// Hashes and checks passwords.
#[derive(Debug, Clone)]
pub struct Hasher {
    cost: Cost,
    turns: Arc<Semaphore>,
    /// How many requests are waiting for a turn.
    waiting: Arc<AtomicUsize>,
    /// The hash of a password nobody has. Checking against it takes as long
    /// as checking against a real one.
    decoy: Arc<str>,
}

impl Hasher {
    /// A hasher that hashes at `cost`.
    ///
    /// # Panics
    ///
    /// Panics if `cost` is outside what Argon2 allows, or the operating
    /// system supplies no random bytes for a salt.
    pub fn new(cost: Cost) -> Self {
        let decoy = hash_now(cost, "the password of an account that does not exist")
            .expect("the cost is valid and the system has random bytes");
        Self {
            cost,
            turns: Arc::new(Semaphore::new(AT_ONCE)),
            waiting: Arc::new(AtomicUsize::new(0)),
            decoy: decoy.into(),
        }
    }

    /// Hashes a password for storing.
    pub async fn hash(&self, password: &Password) -> Result<String, HashError> {
        let cost = self.cost;
        let password = normalise(password.expose());
        self.off_thread(move || hash_now(cost, &password)).await?
    }

    /// Whether `password` is the one `hash` was made from.
    ///
    /// For an account that does not exist, pass `None`: the answer is
    /// `false` after the same work as for one that does. Whoever measures
    /// how long signing in takes learns nothing about which handles exist.
    pub async fn verify(&self, password: &Password, hash: Option<&str>) -> Result<bool, HashError> {
        let known = hash.is_some();
        let hash: Arc<str> = hash.map_or_else(|| self.decoy.clone(), Arc::from);
        let password = normalise(password.expose());
        let matches = self
            .off_thread(move || {
                // The cost comes from the hash itself, so hashes made at an
                // older cost still verify.
                Argon2::default()
                    .verify_password(password.as_bytes(), &*hash)
                    .is_ok()
            })
            .await?;
        Ok(known && matches)
    }

    /// Runs `work` on a thread that may block, when it is its turn.
    async fn off_thread<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, HashError> {
        let turn = {
            let _waiting = Waiting::join(&self.waiting).ok_or(HashError::Busy)?;
            tokio::time::timeout(PATIENCE, self.turns.clone().acquire_owned())
                .await
                .map_err(|_| HashError::Busy)?
                .map_err(|_| HashError::Failed)?
        };
        // The turn is given back when the work is done, also when the
        // request that asked for it has gone away in the meantime.
        tokio::task::spawn_blocking(move || {
            let result = work();
            drop(turn);
            result
        })
        .await
        .map_err(|_| HashError::Failed)
    }
}

/// A place in the queue for a turn. Dropping it leaves the queue, whether a
/// turn came, patience ran out or the request went away.
struct Waiting(Arc<AtomicUsize>);

impl Waiting {
    /// Joins the queue, unless it is as long as it may get.
    fn join(waiting: &Arc<AtomicUsize>) -> Option<Self> {
        waiting
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_WAITING).then_some(count + 1)
            })
            .ok()
            .map(|_| Self(waiting.clone()))
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
impl Hasher {
    /// Takes every turn, so that whatever asks next has to wait. The turns
    /// are free again when what this returns is dropped.
    pub(crate) fn take_every_turn(&self) -> Vec<tokio::sync::OwnedSemaphorePermit> {
        (0..AT_ONCE)
            .map(|_| self.turns.clone().try_acquire_owned().expect("a free turn"))
            .collect()
    }
}

/// Hashes on the calling thread, with a fresh random salt.
fn hash_now(cost: Cost, password: &str) -> Result<String, HashError> {
    let params =
        Params::new(cost.memory_kib, cost.passes, 1, None).map_err(|_| HashError::Failed)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| HashError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password(text: &str) -> Password {
        Password::new(text)
    }

    fn handle(text: &str) -> Handle {
        Handle::new(text).unwrap()
    }

    #[tokio::test]
    async fn a_password_verifies_against_its_own_hash_only() {
        let hasher = Hasher::new(Cost::TESTS_ONLY);
        let hash = hasher
            .hash(&password("correct horse battery"))
            .await
            .unwrap();

        let verify = |text: &str| {
            let (hasher, hash) = (hasher.clone(), hash.clone());
            let text = text.to_owned();
            async move { hasher.verify(&password(&text), Some(&hash)).await.unwrap() }
        };
        assert!(verify("correct horse battery").await);
        assert!(!verify("correct horse battery ").await);
        assert!(!verify("Correct horse battery").await);
        assert!(!verify("").await);
        // Two hashes of one password differ: each has its own salt.
        let again = hasher
            .hash(&password("correct horse battery"))
            .await
            .unwrap();
        assert_ne!(hash, again);
        assert!(!hash.contains("correct"));
    }

    #[tokio::test]
    async fn the_server_hashes_with_argon2id_at_owasps_cost() {
        let hasher = Hasher::new(Cost::OWASP);
        let hash = hasher
            .hash(&password("correct horse battery"))
            .await
            .unwrap();

        assert!(
            hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{hash}"
        );
        assert!(
            hasher
                .verify(&password("correct horse battery"), Some(&hash))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_hash_made_at_another_cost_still_verifies() {
        let old = Hasher::new(Cost::TESTS_ONLY);
        let hash = old.hash(&password("correct horse battery")).await.unwrap();
        let raised = Hasher::new(Cost {
            memory_kib: 64,
            passes: 2,
        });
        assert!(
            raised
                .verify(&password("correct horse battery"), Some(&hash))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn an_unknown_account_gets_the_same_work_and_the_answer_no() {
        let hasher = Hasher::new(Cost::TESTS_ONLY);
        assert!(!hasher.verify(&password("anything"), None).await.unwrap());
        // Not even the decoy's own password opens an account that does not
        // exist.
        let decoys = password("the password of an account that does not exist");
        assert!(!hasher.verify(&decoys, None).await.unwrap());
        // A stored hash that cannot be read is a wrong password, not a crash.
        assert!(
            !hasher
                .verify(&password("anything"), Some("not a hash"))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn the_same_letters_typed_two_ways_are_one_password() {
        let hasher = Hasher::new(Cost::TESTS_ONLY);
        // "ü" as one character, and as "u" with a combining diaeresis.
        let composed = password("gl\u{00FC}cklicher flug");
        let decomposed = password("glu\u{0308}cklicher flug");
        assert_ne!(composed, decomposed);

        let hash = hasher.hash(&composed).await.unwrap();

        assert!(hasher.verify(&decomposed, Some(&hash)).await.unwrap());
    }

    #[test]
    fn a_new_password_is_judged_by_length_and_must_not_be_the_handle() {
        let dario = handle("dario_the_pilot");
        assert_eq!(check_new(&password("fifteen chars !"), &dario), Ok(()));
        assert_eq!(check_new(&password("aaaaaaaaaaaaaaa"), &dario), Ok(()));
        assert_eq!(
            check_new(&password("too short"), &dario),
            Err(Refusal::Length(PasswordError::TooShort))
        );
        assert_eq!(
            check_new(&password(&"a".repeat(129)), &dario),
            Err(Refusal::Length(PasswordError::TooLong))
        );
        assert_eq!(
            check_new(&password("dario_the_pilot"), &dario),
            Err(Refusal::IsTheHandle)
        );
        assert_eq!(
            check_new(&password(" Dario_The_Pilot "), &dario),
            Err(Refusal::IsTheHandle)
        );
    }

    #[tokio::test]
    async fn only_a_few_passwords_are_hashed_at_once() {
        let hasher = Hasher::new(Cost::TESTS_ONLY);
        // Take every turn; the next request cannot start.
        let held = hasher.take_every_turn();
        tokio::time::pause();

        let busy = hasher.verify(&password("anything"), None).await;

        assert_eq!(busy, Err(HashError::Busy));
        drop(held);
        tokio::time::resume();
        assert_eq!(hasher.verify(&password("anything"), None).await, Ok(false));
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_that_finds_the_queue_long_is_turned_away_at_once() {
        let hasher = Hasher::new(Cost::TESTS_ONLY);
        let held = hasher.take_every_turn();
        // As many as may wait are waiting.
        let waiting: Vec<_> = (0..MAX_WAITING)
            .map(|_| {
                let hasher = hasher.clone();
                tokio::spawn(async move { hasher.verify(&password("anything"), None).await })
            })
            .collect();
        tokio::task::yield_now().await;
        assert_eq!(hasher.waiting.load(Ordering::Acquire), MAX_WAITING);

        // The next is not made to wait five seconds to hear "busy".
        let asked_at = tokio::time::Instant::now();
        let late = hasher.verify(&password("anything"), None).await;
        assert_eq!(late, Err(HashError::Busy));
        assert_eq!(asked_at.elapsed(), Duration::ZERO);

        // Those in the queue get their turn when turns are free, and leave
        // the queue behind them empty.
        drop(held);
        for waited in waiting {
            assert_eq!(waited.await.unwrap(), Ok(false));
        }
        assert_eq!(hasher.waiting.load(Ordering::Acquire), 0);
        assert_eq!(hasher.verify(&password("anything"), None).await, Ok(false));
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_that_gives_up_waiting_leaves_the_queue() {
        let hasher = Hasher::new(Cost::TESTS_ONLY);
        let held = hasher.take_every_turn();

        // Patience runs out, on the clock the test owns.
        assert_eq!(
            hasher.verify(&password("anything"), None).await,
            Err(HashError::Busy)
        );
        // And a request that goes away while it waits.
        let gone = {
            let hasher = hasher.clone();
            tokio::spawn(async move { hasher.verify(&password("anything"), None).await })
        };
        tokio::task::yield_now().await;
        assert_eq!(hasher.waiting.load(Ordering::Acquire), 1);
        gone.abort();
        let _ = gone.await;

        assert_eq!(hasher.waiting.load(Ordering::Acquire), 0);
        drop(held);
    }
}
