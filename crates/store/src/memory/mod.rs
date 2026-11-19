mod accounts;
mod flights;

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use venti_domain::{BoundingBox, Observation, SourceId, Station, StationId, Timestamp};

use crate::{
    Account, AccountCreation, Derived, Flight, FlightChanges, FlightKey, Listing, Month,
    NewAccount, NewFlight, NewInvite, OutdatedFlight, RunRecord, SecretHash, Session, SourceStatus,
    StationLatest, Store, StoreError, Visibility, batch,
};

/// A store that lives in the process and forgets everything when it ends.
///
/// Tests and the server's development mode use it. It is written for
/// clarity, not for load: every call takes one lock.
#[derive(Debug, Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Sorted by source, then by the source's own key: the order
    /// [`Store::stations_in`] promises.
    stations: BTreeMap<StationId, Entry>,
    /// Runs by source and start time.
    runs: BTreeMap<(SourceId, Timestamp), RunRecord>,
    accounts: accounts::Accounts,
    flights: flights::Flights,
}

#[derive(Debug)]
struct Entry {
    station: Station,
    /// When a run last delivered the station.
    last_seen_at: Timestamp,
    observations: BTreeMap<Timestamp, Observation>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while the lock was held cannot leave the maps half
        // changed in a way that matters to a test store; carry on.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Merges a newly delivered observation into the stored one: new
/// measurements win, missing ones leave the stored value alone.
fn merge_newer(stored: &mut Observation, newer: &Observation) {
    for ((_, slot), (_, value)) in stored.fields_mut().into_iter().zip(newer.fields()) {
        if value.is_some() {
            *slot = value;
        }
    }
}

impl Store for MemoryStore {
    async fn save_run(
        &self,
        run: &RunRecord,
        stations: &[Station],
        observations: &[Observation],
    ) -> Result<(), StoreError> {
        batch::check(run, stations, observations)?;
        let mut inner = self.lock();

        for station in stations {
            match inner.stations.get_mut(&station.id) {
                Some(entry) => {
                    entry.station = station.clone();
                    entry.last_seen_at = entry.last_seen_at.max(run.started_at);
                }
                None => {
                    let entry = Entry {
                        station: station.clone(),
                        last_seen_at: run.started_at,
                        observations: BTreeMap::new(),
                    };
                    inner.stations.insert(station.id.clone(), entry);
                }
            }
        }
        for observation in observations {
            let entry = inner
                .stations
                .get_mut(&observation.station)
                .expect("the batch check found every observation's station");
            match entry.observations.get_mut(&observation.time) {
                Some(stored) => merge_newer(stored, observation),
                None => {
                    entry
                        .observations
                        .insert(observation.time, observation.clone());
                }
            }
        }
        inner
            .runs
            .insert((run.source.clone(), run.started_at), run.clone());
        Ok(())
    }

    async fn stations_in(
        &self,
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<StationLatest>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .stations
            .values()
            .filter(|entry| area.contains(entry.station.position))
            .take(limit)
            .map(|entry| StationLatest {
                station: entry.station.clone(),
                latest: entry.observations.values().next_back().cloned(),
            })
            .collect())
    }

    async fn station(&self, id: &StationId) -> Result<Option<Station>, StoreError> {
        let inner = self.lock();
        Ok(inner.stations.get(id).map(|entry| entry.station.clone()))
    }

    async fn observations(
        &self,
        station: &StationId,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<Observation>, StoreError> {
        if from > to {
            return Ok(Vec::new());
        }
        let inner = self.lock();
        Ok(inner
            .stations
            .get(station)
            .map(|entry| entry.observations.range(from..=to))
            .into_iter()
            .flatten()
            .map(|(_, observation)| observation.clone())
            .collect())
    }

    async fn source_status(&self, sources: &[SourceId]) -> Result<Vec<SourceStatus>, StoreError> {
        let inner = self.lock();
        Ok(sources
            .iter()
            .map(|source| {
                // Newest first.
                let runs = || {
                    inner
                        .runs
                        .values()
                        .rev()
                        .filter(move |run| &run.source == source)
                };
                let last_success = runs().find(|run| run.healthy);
                SourceStatus {
                    source: source.clone(),
                    last_started: runs().next().map(|run| run.started_at),
                    last_success: last_success.map(|run| run.finished_at),
                    last_failure: runs().find(|run| !run.healthy).map(|run| run.finished_at),
                    stations: last_success.map_or(0, |run| run.stations),
                    newest_observation: runs().find_map(|run| run.newest_observation),
                }
            })
            .collect())
    }

    async fn prune_before(&self, first_kept: Month) -> Result<(), StoreError> {
        let cutoff = first_kept.start();
        let mut inner = self.lock();
        inner
            .stations
            .retain(|_, entry| entry.last_seen_at >= cutoff);
        for entry in inner.stations.values_mut() {
            entry.observations = entry.observations.split_off(&cutoff);
        }
        inner
            .runs
            .retain(|(_, started_at), _| *started_at >= cutoff);
        Ok(())
    }

    async fn create_account(&self, new: &NewAccount) -> Result<AccountCreation, StoreError> {
        Ok(self.lock().accounts.create(new))
    }

    async fn account(&self, handle: &str) -> Result<Option<Account>, StoreError> {
        Ok(self.lock().accounts.by_handle(handle).cloned())
    }

    async fn password_hash(&self, handle: &str) -> Result<Option<(Account, String)>, StoreError> {
        Ok(self.lock().accounts.password_hash(handle))
    }

    async fn set_password(
        &self,
        account_id: &str,
        password_hash: &str,
        keep_session: &str,
    ) -> Result<(), StoreError> {
        self.lock()
            .accounts
            .set_password(account_id, password_hash, keep_session);
        Ok(())
    }

    async fn delete_account(&self, account_id: &str) -> Result<(), StoreError> {
        let mut inner = self.lock();
        inner.accounts.delete(account_id);
        inner.flights.delete_account(account_id);
        Ok(())
    }

    async fn create_invite(&self, invite: &NewInvite) -> Result<(), StoreError> {
        self.lock().accounts.create_invite(invite);
        Ok(())
    }

    async fn create_session(
        &self,
        session: &Session,
        token: &SecretHash,
        keep: usize,
    ) -> Result<(), StoreError> {
        self.lock().accounts.create_session(session, token, keep);
        Ok(())
    }

    async fn use_session(
        &self,
        token: &SecretHash,
        now: Timestamp,
        idle_since: Timestamp,
    ) -> Result<Option<(Session, Account)>, StoreError> {
        Ok(self.lock().accounts.use_session(token, now, idle_since))
    }

    async fn sessions(&self, account_id: &str) -> Result<Vec<Session>, StoreError> {
        Ok(self.lock().accounts.sessions_of(account_id))
    }

    async fn delete_session(&self, account_id: &str, session_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.delete_session(account_id, session_id))
    }

    async fn delete_sessions(&self, account_id: &str) -> Result<(), StoreError> {
        self.lock().accounts.delete_sessions(account_id);
        Ok(())
    }

    async fn prune_sessions(&self, idle_since: Timestamp) -> Result<u64, StoreError> {
        Ok(self.lock().accounts.prune_sessions(idle_since))
    }

    async fn save_flight(&self, new: &NewFlight) -> Result<(Flight, bool), StoreError> {
        let inner = &mut *self.lock();
        inner.flights.save(new, &mut inner.accounts)
    }

    async fn flight(&self, id: &str) -> Result<Option<Flight>, StoreError> {
        let inner = self.lock();
        Ok(inner.flights.get(id, &inner.accounts))
    }

    async fn flight_file(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.lock().flights.file(id))
    }

    async fn flights_of(
        &self,
        account_id: &str,
        listing: Listing,
        after: Option<&FlightKey>,
        limit: usize,
    ) -> Result<Vec<Flight>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .flights
            .of_account(account_id, listing, after, limit, &inner.accounts))
    }

    async fn flight_changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<FlightChanges, StoreError> {
        let inner = self.lock();
        Ok(inner
            .flights
            .changes(account_id, since, limit, &inner.accounts))
    }

    async fn set_visibility(
        &self,
        account_id: &str,
        id: &str,
        visibility: Visibility,
        now: Timestamp,
    ) -> Result<Option<Flight>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .flights
            .set_visibility(account_id, id, visibility, now, &mut inner.accounts))
    }

    async fn delete_flight(
        &self,
        account_id: &str,
        id: &str,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .flights
            .delete(account_id, id, now, &mut inner.accounts))
    }

    async fn outdated_flights(
        &self,
        version: u32,
        after: Option<&(u32, String)>,
        limit: usize,
    ) -> Result<Vec<OutdatedFlight>, StoreError> {
        Ok(self.lock().flights.outdated(version, after, limit))
    }

    async fn set_derived(
        &self,
        id: &str,
        derived: &Derived,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .flights
            .set_derived(id, derived, now, &mut inner.accounts))
    }
}
