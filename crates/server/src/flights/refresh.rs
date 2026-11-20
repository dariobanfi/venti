//! Works stored flights out again after the analysis changed.
//!
//! What the server says about a flight is worked out from its file by
//! [`super::derive`], and stored beside it with the version of the analysis
//! that produced it. When that version goes up, the stored values are out
//! of date. The server then goes through the flights once after it starts,
//! in the background, and brings each up to date. A flight that changes
//! this way shows up as changed to its pilot's devices.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use venti_store::Store;

use super::analyser::Analyser;
use super::derive::ANALYSIS_VERSION;
use super::unpack;
use crate::clock::Clock;

/// How many flights are read from the store at a time, files included.
const BATCH: usize = 16;

/// What one pass over the store did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Flights brought up to date.
    pub refreshed: usize,
    /// Flights that could not be: their file no longer analyses. They keep
    /// their old values and are tried again after the next start.
    pub failed: usize,
}

/// Brings every flight whose derived values are older than
/// [`ANALYSIS_VERSION`] up to date, until done or `shutdown` is cancelled.
pub async fn run<S: Store>(
    store: Arc<S>,
    analyser: Analyser,
    clock: Arc<dyn Clock>,
    shutdown: CancellationToken,
) -> Outcome {
    let mut outcome = Outcome::default();
    let mut after = None;
    loop {
        let batch = match store
            .outdated_flights(ANALYSIS_VERSION, after.as_ref(), BATCH)
            .await
        {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!("flights could not be read for a new analysis: {error}");
                break;
            }
        };
        let Some(last) = batch.last() else {
            break;
        };
        after = Some(last.key());

        for flight in batch {
            if shutdown.is_cancelled() {
                return outcome;
            }
            let derivation = match unpack(&flight.file) {
                Some(file) => analyser.analyse(file.into()).await.ok(),
                None => None,
            };
            let stored = match derivation {
                Some(derivation) => store
                    .set_derived(&flight.id, &derivation.derived, clock.now())
                    .await
                    .is_ok(),
                None => false,
            };
            if stored {
                outcome.refreshed += 1;
            } else {
                outcome.failed += 1;
                tracing::warn!(
                    "flight {} could not be analysed again and keeps the values of version {}",
                    flight.id,
                    flight.version
                );
            }
        }
    }
    if outcome != Outcome::default() {
        tracing::info!(
            "analysis version {ANALYSIS_VERSION}: {} flights brought up to date, {} could not be",
            outcome.refreshed,
            outcome.failed
        );
    }
    outcome
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;
    use venti_store::{Account, AccountCreation, Derived, MemoryStore, NewAccount, NewFlight};

    use super::*;
    use crate::clock::FixedClock;
    use crate::flights::analyser::Where;
    use crate::flights::derive::derive;
    use crate::flights::{new_flight, pack};

    #[tokio::test]
    async fn flights_from_an_older_analysis_are_worked_out_again() {
        let uploaded = Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap();
        let later = Utc.with_ymd_and_hms(2026, 12, 1, 8, 0, 0).unwrap();
        let store = Arc::new(MemoryStore::new());
        let account = Account {
            id: "0123456789abcdefghjkmnpqrs".to_owned(),
            handle: "dario".to_owned(),
            display_name: "Dario".to_owned(),
            created_at: uploaded,
            publishing_suspended_at: None,
        };
        let new = NewAccount {
            account: account.clone(),
            password_hash: "$argon2id$x".to_owned(),
            invite: None,
        };
        assert_eq!(
            store.create_account(&new).await.unwrap(),
            AccountCreation::Created
        );

        let sample = igc::write(&synthetic_flight()).into_bytes();
        let current = derive(&sample).unwrap();
        // One flight as an older analysis left it, with values that are
        // plainly not this analysis's.
        let stale = NewFlight {
            derived: Derived {
                version: ANALYSIS_VERSION - 1,
                airtime_s: 1,
                glider: None,
                ..current.derived.clone()
            },
            ..new_flight(&account.id, &sample, current.clone(), uploaded)
        };
        // One whose stored file cannot be read any more.
        let broken = NewFlight {
            id: "broken0000000000000000000a".to_owned(),
            identity: "f".repeat(32),
            file: b"not a packed file".to_vec(),
            ..stale.clone()
        };
        // One that is up to date.
        let other = igc::write(&synthetic_flight()).replace("040726", "040725");
        let fresh = new_flight(
            &account.id,
            other.as_bytes(),
            derive(other.as_bytes()).unwrap(),
            uploaded,
        );
        for flight in [&stale, &broken, &fresh] {
            store
                .save_flight(flight, &venti_store::Room::UNLIMITED)
                .await
                .unwrap();
        }
        let before = store.flight(&fresh.id).await.unwrap();

        let outcome = run(
            store.clone(),
            Analyser::new(Where::Thread),
            Arc::new(FixedClock(later)),
            CancellationToken::new(),
        )
        .await;

        assert_eq!(
            outcome,
            Outcome {
                refreshed: 1,
                failed: 1
            }
        );
        let refreshed = store.flight(&stale.id).await.unwrap().unwrap();
        assert_eq!(refreshed.derived, current.derived);
        assert_eq!(refreshed.change, later.timestamp_micros());
        // The file is the one that was uploaded.
        assert_eq!(
            store.flight_file(&stale.id).await.unwrap(),
            Some(pack(&sample))
        );
        // What was up to date was not touched, and what cannot be read
        // keeps what it had.
        assert_eq!(store.flight(&fresh.id).await.unwrap(), before);
        let kept = store.flight(&broken.id).await.unwrap().unwrap();
        assert_eq!(kept.derived.version, ANALYSIS_VERSION - 1);

        // A second pass finds only the one it cannot help, once.
        let again = run(
            store.clone(),
            Analyser::new(Where::Thread),
            Arc::new(FixedClock(later)),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(
            again,
            Outcome {
                refreshed: 0,
                failed: 1
            }
        );
    }
}
