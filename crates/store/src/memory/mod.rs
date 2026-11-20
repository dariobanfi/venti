mod accounts;
mod flights;
mod groups;
mod live;
mod reports;

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use chrono::NaiveDate;
use venti_domain::{
    Airspace, AirspaceId, BoundingBox, Observation, Site, SiteId, SourceId, Station, StationId,
    Timestamp,
};

use crate::flight::{Room, Saved};
use crate::{
    Account, AccountCreation, AirspaceLayer, AirspaceSet, Coverage, Derived, Flight, FlightChanges,
    FlightKey, ForecastModel, Group, GroupCreation, GroupJoining, GroupMembers, GroupRoom,
    KeeperAct, Listing, LiveBatch, LiveFix, LiveFixAt, LiveRemoval, LiveRoom, LiveSaved, LiveTrack,
    MemberRemoval, Month, NewAccount, NewFlight, NewGroup, NewInvite, NewPlan, NewReport,
    OutdatedFlight, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom, PlanSaving,
    PublicFlights, PublicOrder, Ranked, Report, ReportClosing, ReportKey, ReportListing,
    ReportSaved, RunRecord, SecretHash, Session, SiteDelivery, SiteLayer, SiteSet, SourceStatus,
    StationLatest, Store, StoreError, Suspension, Visibility, VisibilityChange, airspace, batch,
    forecast, site,
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
    /// The airspace of each source, as its latest healthy run delivered it.
    airspace: BTreeMap<SourceId, StoredSet>,
    /// The flying sites of each source, in the same way.
    sites: BTreeMap<SourceId, StoredSites>,
    /// The models of each forecast source, as the latest healthy look
    /// found them, sorted by model.
    forecast_models: BTreeMap<SourceId, Vec<ForecastModel>>,
    accounts: accounts::Accounts,
    flights: flights::Flights,
    live: live::Live,
    groups: groups::Groups,
    reports: reports::Reports,
}

#[derive(Debug)]
struct Entry {
    station: Station,
    /// When a run last delivered the station.
    last_seen_at: Timestamp,
    observations: BTreeMap<Timestamp, Observation>,
}

#[derive(Debug)]
struct StoredSet {
    set: AirspaceSet,
    /// Sorted by identifier: the order [`Store::airspaces_in`] promises.
    airspaces: BTreeMap<AirspaceId, Airspace>,
}

#[derive(Debug)]
struct StoredSites {
    set: SiteSet,
    /// Sorted by identifier: the order [`Store::sites_in`] promises.
    sites: BTreeMap<SiteId, Site>,
}

/// Whether two boxes share at least a point, edges included.
fn touch(a: &BoundingBox, b: &BoundingBox) -> bool {
    a.west() <= b.east() && a.east() >= b.west() && a.south() <= b.north() && a.north() >= b.south()
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

    async fn save_airspace_run(
        &self,
        run: &RunRecord,
        coverage: &Coverage,
        airspaces: &[Airspace],
    ) -> Result<(), StoreError> {
        airspace::check(run, airspaces)?;
        let mut inner = self.lock();
        inner
            .runs
            .insert((run.source.clone(), run.started_at), run.clone());
        let newer_is_stored = inner
            .airspace
            .get(&run.source)
            .is_some_and(|stored| stored.set.fetched_at > run.started_at);
        if run.healthy && !newer_is_stored {
            let stored = StoredSet {
                set: AirspaceSet::of(run, coverage, airspaces),
                airspaces: airspaces
                    .iter()
                    .map(|airspace| (airspace.id.clone(), airspace.clone()))
                    .collect(),
            };
            inner.airspace.insert(run.source.clone(), stored);
        }
        Ok(())
    }

    async fn airspaces_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<AirspaceLayer>, StoreError> {
        let inner = self.lock();
        let mut left = limit;
        let mut layers = Vec::new();
        for (source, stored) in &inner.airspace {
            if !sources.contains(source) {
                continue;
            }
            let found: Vec<Airspace> = stored
                .airspaces
                .values()
                .filter(|airspace| touch(&airspace.bounds(), area))
                .take(left)
                .cloned()
                .collect();
            left -= found.len();
            layers.push(AirspaceLayer {
                set: stored.set.clone(),
                airspaces: found,
            });
        }
        Ok(layers)
    }

    async fn airspace_sets(&self, sources: &[SourceId]) -> Result<Vec<AirspaceSet>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .airspace
            .iter()
            .filter(|(source, _)| sources.contains(source))
            .map(|(_, stored)| stored.set.clone())
            .collect())
    }

    async fn save_site_run(
        &self,
        run: &RunRecord,
        delivery: &SiteDelivery,
        sites: &[Site],
    ) -> Result<(), StoreError> {
        site::check(run, sites)?;
        let mut inner = self.lock();
        inner
            .runs
            .insert((run.source.clone(), run.started_at), run.clone());
        let newer_is_stored = inner
            .sites
            .get(&run.source)
            .is_some_and(|stored| stored.set.fetched_at > run.started_at);
        if run.healthy && !newer_is_stored {
            let stored = StoredSites {
                set: SiteSet::of(run, delivery, sites),
                sites: sites
                    .iter()
                    .map(|site| (site.id.clone(), site.clone()))
                    .collect(),
            };
            inner.sites.insert(run.source.clone(), stored);
        }
        Ok(())
    }

    async fn sites_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<SiteLayer>, StoreError> {
        let inner = self.lock();
        let mut left = limit;
        let mut layers = Vec::new();
        for (source, stored) in &inner.sites {
            if !sources.contains(source) {
                continue;
            }
            let found: Vec<Site> = stored
                .sites
                .values()
                .filter(|site| area.contains(site.position))
                .take(left)
                .cloned()
                .collect();
            left -= found.len();
            layers.push(SiteLayer {
                set: stored.set.clone(),
                sites: found,
            });
        }
        Ok(layers)
    }

    async fn site_sets(&self, sources: &[SourceId]) -> Result<Vec<SiteSet>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .sites
            .iter()
            .filter(|(source, _)| sources.contains(source))
            .map(|(_, stored)| stored.set.clone())
            .collect())
    }

    async fn save_forecast_check(
        &self,
        run: &RunRecord,
        models: &[ForecastModel],
    ) -> Result<(), StoreError> {
        forecast::check(run, models)?;
        let mut inner = self.lock();
        inner
            .runs
            .insert((run.source.clone(), run.started_at), run.clone());
        let newer_is_stored = inner
            .forecast_models
            .get(&run.source)
            .and_then(|models| models.first())
            .is_some_and(|stored| stored.checked_at > run.started_at);
        if run.healthy && !newer_is_stored {
            let mut models = models.to_vec();
            models.sort_by(|a, b| a.model.cmp(&b.model));
            inner.forecast_models.insert(run.source.clone(), models);
        }
        Ok(())
    }

    async fn forecast_models(
        &self,
        sources: &[SourceId],
    ) -> Result<Vec<ForecastModel>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .forecast_models
            .iter()
            .filter(|(source, _)| sources.contains(source))
            .flat_map(|(_, models)| models.iter().cloned())
            .collect())
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
        inner.live.delete_account(account_id);
        inner.groups.delete_account(account_id);
        inner.reports.delete_account(account_id);
        Ok(())
    }

    async fn suspend_publishing(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Option<Suspension>, StoreError> {
        let inner = &mut *self.lock();
        let Some(since) = inner.accounts.suspend(account_id, now) else {
            return Ok(None);
        };
        Ok(Some(Suspension {
            since,
            flights: inner
                .flights
                .unlist_public(account_id, now, &mut inner.accounts),
            live_tracks: inner.live.narrow_all(account_id),
        }))
    }

    async fn resume_publishing(&self, account_id: &str) -> Result<Option<bool>, StoreError> {
        Ok(self.lock().accounts.resume(account_id))
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

    async fn save_flight(&self, new: &NewFlight, room: &Room) -> Result<Saved, StoreError> {
        let inner = &mut *self.lock();
        inner.flights.save(new, room, &mut inner.accounts)
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

    async fn public_flights(
        &self,
        which: &PublicFlights,
        order: &PublicOrder,
        limit: usize,
    ) -> Result<Vec<Flight>, StoreError> {
        let inner = self.lock();
        Ok(inner.flights.public(which, order, limit, &inner.accounts))
    }

    async fn ranking(
        &self,
        which: &PublicFlights,
        counted: usize,
        limit: usize,
    ) -> Result<Vec<Ranked>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .flights
            .ranking(which, counted, limit, &inner.accounts))
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
    ) -> Result<VisibilityChange, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .flights
            .set_visibility(account_id, id, visibility, now, &mut inner.accounts))
    }

    async fn take_down_flight(
        &self,
        id: &str,
        now: Timestamp,
    ) -> Result<Option<Flight>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.flights.take_down(id, now, &mut inner.accounts))
    }

    async fn release_flight(&self, id: &str, now: Timestamp) -> Result<Option<Flight>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.flights.release(id, now, &mut inner.accounts))
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

    async fn save_live_fixes(
        &self,
        batch: &LiveBatch,
        room: &LiveRoom,
    ) -> Result<LiveSaved, StoreError> {
        let inner = &mut *self.lock();
        inner.live.save(batch, room, &inner.accounts)
    }

    async fn live_track(&self, id: &str) -> Result<Option<LiveTrack>, StoreError> {
        let inner = self.lock();
        Ok(inner.live.get(id, &inner.accounts))
    }

    async fn live_fixes(
        &self,
        id: &str,
        since: i64,
        after: Option<Timestamp>,
        limit: usize,
    ) -> Result<Vec<LiveFixAt>, StoreError> {
        Ok(self.lock().live.fixes(id, since, after, limit))
    }

    async fn live_tail(&self, id: &str, limit: usize) -> Result<Vec<LiveFix>, StoreError> {
        Ok(self.lock().live.tail(id, limit))
    }

    async fn live_tracks_in(
        &self,
        area: &BoundingBox,
        newest_since: Timestamp,
        limit: usize,
    ) -> Result<Vec<LiveTrack>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .live
            .in_box(area, newest_since, limit, &inner.accounts))
    }

    async fn delete_live_track(
        &self,
        account_id: &str,
        id: &str,
        _now: Timestamp,
        may_retire: bool,
    ) -> Result<LiveRemoval, StoreError> {
        let inner = &mut *self.lock();
        inner
            .live
            .delete(account_id, id, may_retire, &inner.accounts)
    }

    async fn remove_live_track(&self, id: &str, _now: Timestamp) -> Result<bool, StoreError> {
        Ok(self.lock().live.remove(id))
    }

    async fn prune_live_tracks(
        &self,
        ended_before: Timestamp,
        _now: Timestamp,
    ) -> Result<u64, StoreError> {
        Ok(self.lock().live.prune(ended_before))
    }

    async fn create_group(
        &self,
        new: &NewGroup,
        room: &GroupRoom,
    ) -> Result<GroupCreation, StoreError> {
        let inner = &mut *self.lock();
        inner.groups.create(new, room, &inner.accounts)
    }

    async fn join_group(
        &self,
        account_id: &str,
        invite: &SecretHash,
        room: &GroupRoom,
        now: Timestamp,
    ) -> Result<GroupJoining, StoreError> {
        let inner = &mut *self.lock();
        inner
            .groups
            .join(account_id, invite, room, now, &inner.accounts)
    }

    async fn groups_of(&self, account_id: &str) -> Result<Vec<Group>, StoreError> {
        let inner = self.lock();
        Ok(inner.groups.of_account(account_id, &inner.accounts))
    }

    async fn group(&self, id: &str, account_id: &str) -> Result<Option<GroupMembers>, StoreError> {
        let inner = self.lock();
        Ok(inner.groups.get(id, account_id, &inner.accounts))
    }

    async fn rename_group(
        &self,
        id: &str,
        keeper_id: &str,
        name: &str,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        let inner = &mut *self.lock();
        inner.groups.rename(id, keeper_id, name, &inner.accounts)
    }

    async fn renew_group_invite(
        &self,
        id: &str,
        keeper_id: &str,
        invite: &SecretHash,
    ) -> Result<KeeperAct<()>, StoreError> {
        self.lock().groups.renew_invite(id, keeper_id, invite)
    }

    async fn hand_over_group(
        &self,
        id: &str,
        keeper_id: &str,
        member_handle: &str,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .groups
            .hand_over(id, keeper_id, member_handle, &inner.accounts))
    }

    async fn remove_group_member(
        &self,
        id: &str,
        by_account_id: &str,
        member_handle: &str,
    ) -> Result<MemberRemoval, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .groups
            .remove_member(id, by_account_id, member_handle, &inner.accounts))
    }

    async fn delete_group(&self, id: &str, keeper_id: &str) -> Result<KeeperAct<()>, StoreError> {
        Ok(self.lock().groups.delete(id, keeper_id))
    }

    async fn save_plan(&self, new: &NewPlan, room: &PlanRoom) -> Result<PlanSaving, StoreError> {
        let inner = &mut *self.lock();
        inner.groups.save_plan(new, room, &inner.accounts)
    }

    async fn change_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        details: &PlanDetails,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> Result<PlanChange, StoreError> {
        let inner = &mut *self.lock();
        inner.groups.change_plan(
            (group_id, plan_id),
            by_account_id,
            details,
            now,
            gone_before,
            &inner.accounts,
        )
    }

    async fn delete_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        gone_before: NaiveDate,
    ) -> Result<PlanRemoval, StoreError> {
        Ok(self
            .lock()
            .groups
            .delete_plan(group_id, plan_id, by_account_id, gone_before))
    }

    async fn set_plan_in(
        &self,
        group_id: &str,
        plan_id: &str,
        account_id: &str,
        is_in: bool,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> Result<PlanChange, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.groups.set_in(
            (group_id, plan_id),
            account_id,
            is_in,
            now,
            gone_before,
            &inner.accounts,
        ))
    }

    async fn plans_of_group(
        &self,
        group_id: &str,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
    ) -> Result<Option<Vec<Plan>>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .groups
            .plans_of_group(group_id, account_id, from, to, &inner.accounts))
    }

    async fn plans_for(
        &self,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
        limit: usize,
    ) -> Result<Vec<Plan>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .groups
            .plans_for(account_id, from, to, limit, &inner.accounts))
    }

    async fn prune_plans(&self, before: NaiveDate) -> Result<u64, StoreError> {
        Ok(self.lock().groups.prune_plans(before))
    }

    async fn save_report(&self, new: &NewReport) -> Result<ReportSaved, StoreError> {
        let inner = &mut *self.lock();
        inner.reports.save(new, &inner.accounts)
    }

    async fn reports(
        &self,
        listing: ReportListing,
        after: Option<&ReportKey>,
        limit: usize,
    ) -> Result<Vec<Report>, StoreError> {
        let inner = self.lock();
        Ok(inner.reports.list(listing, after, limit, &inner.accounts))
    }

    async fn close_report(
        &self,
        id: &str,
        note: &str,
        now: Timestamp,
    ) -> Result<ReportClosing, StoreError> {
        self.lock().reports.close(id, note, now)
    }

    async fn prune_reports(&self, closed_before: Timestamp) -> Result<u64, StoreError> {
        Ok(self.lock().reports.prune(closed_before))
    }
}
