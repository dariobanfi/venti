use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use venti_domain::{BoundingBox, Observation, SourceId, Station, StationId, Timestamp};

use crate::{Month, RunRecord, SourceStatus, StationLatest, Store, StoreError, batch};

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
}
