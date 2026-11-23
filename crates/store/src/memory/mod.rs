mod accounts;
mod alerts;
mod flights;
mod groups;
mod kept_sites;
mod live;
mod oauth;
mod open_plans;
mod phones;
mod pilot_sites;
mod push;
mod reports;
mod social;

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use chrono::NaiveDate;
use venti_domain::{
    Airspace, AirspaceId, BoundingBox, Cable, GeoPoint, ModelId, Observation, Site, SiteId,
    SourceId, Station, StationId, Timestamp,
};

use crate::flight::{Room, Saved};
use crate::{
    AccessToken, Account, AccountCreation, AirspaceLayer, AirspaceSet, CableLayer, CableSet,
    Coverage, Derived, Flight, FlightChanges, FlightKey, ForecastModel, Group, GroupCreation,
    GroupJoining, GroupMembers, GroupRoom, KeeperAct, Listing, LiveBatch, LiveFix, LiveFixAt,
    LiveRemoval, LiveRoom, LiveSaved, LiveTrack, MemberRemoval, Month, NewAccount, NewFlight,
    NewGroup, NewInvite, NewPilotSite, NewPlan, NewReport, NewSiteConfirmation, OutdatedFlight,
    PilotSite, PilotSiteChanges, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom, PlanSaving,
    PublicFlightChanges, PublicFlights, PublicOrder, Ranked, RemovedGroup, Report, ReportClosing,
    ReportKey, ReportListing, ReportSaved, RunRecord, SecretHash, Session, SiteConfirmation,
    SiteConfirming, SiteDelivery, SiteLayer, SiteRevising, SiteRevision, SiteSet, SiteWrite,
    SourceStatus, StationLatest, Store, StoreError, Suspension, ThermalPart, Visibility,
    VisibilityChange, airspace, batch, cable, forecast, site, takeoff,
};
use crate::{
    AccountSettings, AllVisibilityChange, GliderClass, GoogleLink, GoogleLinking, NewGoogleAccount,
};
use crate::{CandidateRule, FlightsFrom, Flown, FoundPilot, TakeoffCandidate};
use crate::{Given, KeepingSite, KeptRoom, KeptSiteChanges, NewKeptSite};
use crate::{NearPilot, NewPicture, Picture};
use crate::{
    NewOpenPlan, OpenPlan, OpenPlanArea, OpenPlanChange, OpenPlanDetails, OpenPlanRemoval,
    OpenPlanRoom, OpenPlanSaving,
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
    cables: BTreeMap<SourceId, (CableSet, Vec<Cable>)>,
    /// The models of each forecast source, as the latest healthy look
    /// found them, sorted by model.
    forecast_models: BTreeMap<SourceId, Vec<ForecastModel>>,
    /// The parts of the thermal map, by source and model.
    thermal_parts: BTreeMap<(SourceId, ModelId), ThermalPart>,
    accounts: accounts::Accounts,
    flights: flights::Flights,
    live: live::Live,
    groups: groups::Groups,
    open_plans: open_plans::OpenPlans,
    reports: reports::Reports,
    pilot_sites: pilot_sites::PilotSites,
    kept_sites: kept_sites::KeptSites,
    alerts: alerts::Alerts,
    push: push::Push,
    phones: phones::Phones,
    social: social::Social,
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

    async fn save_cable_run(
        &self,
        run: &RunRecord,
        delivery: &SiteDelivery,
        cables: &[Cable],
    ) -> Result<(), StoreError> {
        cable::check(run, delivery, cables)?;
        let mut inner = self.lock();
        inner
            .runs
            .insert((run.source.clone(), run.started_at), run.clone());
        let newer_is_stored = inner
            .cables
            .get(&run.source)
            .is_some_and(|(set, _)| set.fetched_at > run.started_at);
        if newer_is_stored {
            return Ok(());
        }
        match cable::Replacing::of(run, delivery) {
            cable::Replacing::Whole => {
                let mut sorted = cables.to_vec();
                sorted.sort_by(|one, two| one.local.cmp(&two.local));
                inner.cables.insert(
                    run.source.clone(),
                    (CableSet::of(run, delivery, cables), sorted),
                );
            }
            cable::Replacing::Inside => {
                let before = inner.cables.remove(&run.source);
                let mut kept: BTreeMap<String, Cable> = BTreeMap::new();
                if let Some((_, stored)) = &before {
                    for cable in stored {
                        if !cable
                            .bounds()
                            .is_some_and(|bounds| cable::goes(&bounds, &delivery.covers))
                        {
                            kept.insert(cable.local.clone(), cable.clone());
                        }
                    }
                }
                for cable in cables {
                    kept.insert(cable.local.clone(), cable.clone());
                }
                let set = cable::partly_replaced(
                    before.as_ref().map(|(set, _)| set),
                    run,
                    delivery,
                    kept.len(),
                );
                inner
                    .cables
                    .insert(run.source.clone(), (set, kept.into_values().collect()));
            }
            cable::Replacing::Nothing => {}
        }
        Ok(())
    }

    async fn cables_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<CableLayer>, StoreError> {
        let inner = self.lock();
        let mut left = limit;
        let mut layers = Vec::new();
        for (source, (set, cables)) in &inner.cables {
            if !sources.contains(source) {
                continue;
            }
            let found: Vec<Cable> = cables
                .iter()
                .filter(|cable| cable.touches(area))
                .take(left)
                .cloned()
                .collect();
            left -= found.len();
            layers.push(CableLayer {
                set: set.clone(),
                cables: found,
            });
        }
        Ok(layers)
    }

    async fn cable_sets(&self, sources: &[SourceId]) -> Result<Vec<CableSet>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .cables
            .iter()
            .filter(|(source, _)| sources.contains(source))
            .map(|(_, (set, _))| set.clone())
            .collect())
    }

    async fn set_site(&self, id: &SiteId) -> Result<Option<Site>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .sites
            .get(id.source())
            .and_then(|stored| stored.sites.get(id))
            .cloned())
    }

    async fn add_pilot_site(
        &self,
        new: &NewPilotSite,
        apart_m: f64,
    ) -> Result<SiteWrite, StoreError> {
        let inner = &mut *self.lock();
        inner.pilot_sites.add(new, apart_m, &inner.accounts)
    }

    async fn revise_pilot_site(
        &self,
        revising: &SiteRevising,
        apart_m: f64,
    ) -> Result<SiteWrite, StoreError> {
        let inner = &mut *self.lock();
        inner.pilot_sites.revise(revising, apart_m, &inner.accounts)
    }

    async fn pilot_sites_in(
        &self,
        layer: &SourceId,
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<PilotSite>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .pilot_sites
            .in_box(layer, area, limit, &inner.accounts))
    }

    async fn pilot_site(&self, id: &SiteId) -> Result<Option<PilotSite>, StoreError> {
        let inner = self.lock();
        Ok(inner.pilot_sites.one(id, &inner.accounts))
    }

    async fn pilot_site_revisions(
        &self,
        id: &SiteId,
        after: u32,
        limit: usize,
    ) -> Result<Option<(PilotSite, Vec<SiteRevision>)>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .pilot_sites
            .revisions(id, after, limit, &inner.accounts))
    }

    async fn pilot_sites_revisions(&self, ids: &[SiteId]) -> Result<Vec<SiteRevision>, StoreError> {
        let inner = self.lock();
        Ok(inner.pilot_sites.revisions_of(ids, &inner.accounts))
    }

    async fn pilot_site_changes(
        &self,
        layer: &SourceId,
        since: i64,
        limit: usize,
    ) -> Result<PilotSiteChanges, StoreError> {
        let inner = self.lock();
        Ok(inner
            .pilot_sites
            .changes(layer, since, limit, &inner.accounts))
    }

    async fn given_by(&self, account_id: &str) -> Result<Given, StoreError> {
        let inner = self.lock();
        Ok(Given {
            revisions: inner.pilot_sites.revisions_by(account_id, &inner.accounts),
            confirmations: inner
                .pilot_sites
                .confirmations_by(account_id, &inner.accounts),
            reports: inner.reports.sent_by(account_id, &inner.accounts),
        })
    }

    async fn confirm_pilot_site(
        &self,
        new: &NewSiteConfirmation,
    ) -> Result<SiteConfirming, StoreError> {
        let inner = &mut *self.lock();
        inner.pilot_sites.confirm(new, &inner.accounts)
    }

    async fn withdraw_site_confirmation(
        &self,
        site: &SiteId,
        account_id: &str,
    ) -> Result<Option<(PilotSite, bool)>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .pilot_sites
            .withdraw(site, account_id, &inner.accounts))
    }

    async fn site_confirmations(
        &self,
        site: &SiteId,
        limit: usize,
    ) -> Result<Option<(PilotSite, Vec<SiteConfirmation>)>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .pilot_sites
            .confirmations(site, limit, &inner.accounts))
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

    async fn save_thermal_part(&self, part: &ThermalPart) -> Result<(), StoreError> {
        let mut inner = self.lock();
        let key = (part.source.clone(), part.model.clone());
        let newer_is_stored = inner
            .thermal_parts
            .get(&key)
            .is_some_and(|stored| stored.written_at > part.written_at);
        if !newer_is_stored {
            inner.thermal_parts.insert(key, part.clone());
        }
        Ok(())
    }

    async fn thermal_parts(&self, source: &SourceId) -> Result<Vec<ThermalPart>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .thermal_parts
            .iter()
            .filter(|((of, _), _)| of == source)
            .map(|(_, part)| part.clone())
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

    async fn create_google_account(
        &self,
        new: &NewGoogleAccount,
    ) -> Result<AccountCreation, StoreError> {
        Ok(self.lock().accounts.create_google(new))
    }

    async fn google_account(&self, subject: &str) -> Result<Option<Account>, StoreError> {
        Ok(self.lock().accounts.by_google(subject))
    }

    async fn google_link(&self, account_id: &str) -> Result<Option<GoogleLink>, StoreError> {
        Ok(self.lock().accounts.google_link(account_id))
    }

    async fn link_google(
        &self,
        account_id: &str,
        google: &GoogleLink,
    ) -> Result<GoogleLinking, StoreError> {
        Ok(self.lock().accounts.link_google(account_id, google))
    }

    async fn unlink_google(&self, account_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.unlink_google(account_id))
    }

    async fn has_password(&self, account_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.has_password(account_id))
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
        inner.pilot_sites.delete_account(account_id);
        let its_flights = inner.flights.ids_of(account_id);
        inner.social.delete_account(account_id, &its_flights);
        inner.flights.delete_account(account_id);
        inner.live.delete_account(account_id);
        inner.groups.delete_account(account_id);
        inner.open_plans.delete_account(account_id);
        inner.reports.delete_account(account_id);
        inner.kept_sites.delete_account(account_id);
        inner.alerts.delete_account(account_id);
        inner.push.delete_account(account_id);
        inner.phones.delete_account(account_id);
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

    async fn let_edit_sites(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Option<Timestamp>, StoreError> {
        Ok(self.lock().accounts.let_edit_sites(account_id, now))
    }

    async fn stop_editing_sites(&self, account_id: &str) -> Result<Option<bool>, StoreError> {
        Ok(self.lock().accounts.stop_editing_sites(account_id))
    }

    async fn site_editors(&self) -> Result<Vec<Account>, StoreError> {
        Ok(self.lock().accounts.site_editors())
    }

    async fn update_account_settings(
        &self,
        account_id: &str,
        settings: &AccountSettings,
    ) -> Result<Option<Account>, StoreError> {
        Ok(self.lock().accounts.update_settings(account_id, settings))
    }

    async fn set_picture(&self, picture: &NewPicture) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.set_picture(picture))
    }

    async fn remove_picture(&self, account_id: &str) -> Result<Option<Picture>, StoreError> {
        Ok(self.lock().accounts.remove_picture(account_id))
    }

    async fn picture(&self, id: &str) -> Result<Option<Picture>, StoreError> {
        Ok(self.lock().accounts.picture(id))
    }

    async fn claim_google_picture(&self, account_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.claim_google_picture(account_id))
    }

    async fn pilots_near(
        &self,
        place: GeoPoint,
        within_m: f64,
        since: Timestamp,
        limit: usize,
    ) -> Result<Vec<NearPilot>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .flights
            .pilots_near(place, within_m, since, limit, &inner.accounts))
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

    async fn create_access_token(
        &self,
        token: &AccessToken,
        secret: &SecretHash,
        most: usize,
    ) -> Result<bool, StoreError> {
        Ok(self
            .lock()
            .accounts
            .create_access_token(token, secret, most))
    }

    async fn use_access_token(
        &self,
        secret: &SecretHash,
        now: Timestamp,
    ) -> Result<Option<(AccessToken, Account)>, StoreError> {
        Ok(self.lock().accounts.use_access_token(secret, now))
    }

    async fn access_tokens(&self, account_id: &str) -> Result<Vec<AccessToken>, StoreError> {
        Ok(self.lock().accounts.access_tokens_of(account_id))
    }

    async fn delete_access_token(
        &self,
        account_id: &str,
        token_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(self
            .lock()
            .accounts
            .delete_access_token(account_id, token_id))
    }

    async fn create_oauth_client(
        &self,
        client: &crate::OauthClient,
        most: usize,
    ) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.create_oauth_client(client, most))
    }

    async fn oauth_client(
        &self,
        client_id: &str,
    ) -> Result<Option<crate::OauthClient>, StoreError> {
        Ok(self.lock().accounts.oauth_client(client_id))
    }

    async fn create_authorization_request(
        &self,
        request: &SecretHash,
        record: &crate::AuthorizationRequest,
        now: Timestamp,
        most: usize,
    ) -> Result<bool, StoreError> {
        Ok(self
            .lock()
            .accounts
            .create_authorization_request(request, record, now, most))
    }

    async fn authorization_request(
        &self,
        request: &SecretHash,
        now: Timestamp,
    ) -> Result<crate::RequestState, StoreError> {
        Ok(self.lock().accounts.authorization_request(request, now))
    }

    async fn answer_authorization_request(
        &self,
        request: &SecretHash,
        now: Timestamp,
    ) -> Result<crate::RequestState, StoreError> {
        Ok(self
            .lock()
            .accounts
            .answer_authorization_request(request, now))
    }

    async fn create_authorization_code(
        &self,
        code: &SecretHash,
        record: &crate::AuthorizationCode,
    ) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.create_authorization_code(code, record))
    }

    async fn authorization_code(
        &self,
        code: &SecretHash,
        now: Timestamp,
    ) -> Result<Option<crate::StoredCode>, StoreError> {
        Ok(self.lock().accounts.authorization_code(code, now))
    }

    async fn redeem_authorization_code(
        &self,
        code: &SecretHash,
        grant: &crate::NewGrant,
        now: Timestamp,
        most: usize,
    ) -> Result<crate::Redeemed, StoreError> {
        Ok(self
            .lock()
            .accounts
            .redeem_authorization_code(code, grant, now, most))
    }

    async fn refresh_grant(
        &self,
        refresh: &SecretHash,
        client_id: &str,
        next: &crate::Rotation,
        now: Timestamp,
    ) -> Result<crate::Refreshed, StoreError> {
        Ok(self
            .lock()
            .accounts
            .refresh_grant(refresh, client_id, next, now))
    }

    async fn revoke_grant(&self, secret: &SecretHash, client_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().accounts.revoke_grant(secret, client_id))
    }

    async fn prune_oauth(
        &self,
        now: Timestamp,
        unused_since: Timestamp,
    ) -> Result<u64, StoreError> {
        Ok(self.lock().accounts.prune_oauth(now, unused_since))
    }

    async fn keep_site(
        &self,
        account_id: &str,
        new: &NewKeptSite,
        now: Timestamp,
        room: KeptRoom,
    ) -> Result<KeepingSite, StoreError> {
        let inner = &mut *self.lock();
        if inner.accounts.flight_change(account_id).is_none() {
            return Ok(KeepingSite::NoAccount);
        }
        Ok(inner.kept_sites.keep(account_id, new, now, room))
    }

    async fn forget_kept_site(
        &self,
        account_id: &str,
        site_id: &str,
        now: Timestamp,
        room: KeptRoom,
    ) -> Result<bool, StoreError> {
        Ok(self
            .lock()
            .kept_sites
            .forget(account_id, site_id, now, room))
    }

    async fn kept_site_changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<KeptSiteChanges, StoreError> {
        Ok(self.lock().kept_sites.changes(account_id, since, limit))
    }

    async fn alerts(&self, account_id: &str) -> Result<Vec<crate::Alert>, StoreError> {
        Ok(self.lock().alerts.of(account_id))
    }

    async fn add_alert(
        &self,
        account_id: &str,
        new: &crate::NewAlert,
        now: Timestamp,
        most: usize,
    ) -> Result<crate::AddingAlert, StoreError> {
        let inner = &mut *self.lock();
        if inner.accounts.flight_change(account_id).is_none() {
            return Ok(crate::AddingAlert::NoAccount);
        }
        Ok(inner.alerts.add(account_id, new, now, most))
    }

    async fn change_alert(
        &self,
        account_id: &str,
        new: &crate::NewAlert,
        now: Timestamp,
    ) -> Result<Option<crate::Alert>, StoreError> {
        Ok(self.lock().alerts.change(account_id, new, now))
    }

    async fn remove_alert(&self, account_id: &str, alert_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().alerts.remove(account_id, alert_id))
    }

    async fn wind_alerts_at(
        &self,
        station_ids: &[String],
    ) -> Result<Vec<crate::Alert>, StoreError> {
        Ok(self.lock().alerts.matching(|alert| {
            alert
                .station_id
                .as_ref()
                .is_some_and(|station| station_ids.contains(station))
        }))
    }

    async fn alerts_of_kind(&self, kind: &str) -> Result<Vec<crate::Alert>, StoreError> {
        Ok(self.lock().alerts.matching(|alert| alert.kind == kind))
    }

    async fn note_alert_check(&self, check: &crate::AlertCheck) -> Result<bool, StoreError> {
        Ok(self.lock().alerts.note(check))
    }

    async fn fired_alerts(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<crate::FiredAlerts, StoreError> {
        Ok(self.lock().alerts.fired(account_id, since, limit))
    }

    async fn follow(
        &self,
        follower_id: &str,
        followed_id: &str,
        now: Timestamp,
        most: usize,
    ) -> Result<crate::Following, StoreError> {
        Ok({
            let inner = &mut *self.lock();
            inner
                .social
                .follow(follower_id, followed_id, now, most, &inner.accounts)
        })
    }

    async fn unfollow(&self, follower_id: &str, followed_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().social.unfollow(follower_id, followed_id))
    }

    async fn follow_counts(&self, account_id: &str) -> Result<crate::FollowCounts, StoreError> {
        Ok(self.lock().social.counts(account_id))
    }

    async fn follows_pilot(
        &self,
        follower_id: &str,
        followed_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(self.lock().social.follows_pilot(follower_id, followed_id))
    }

    async fn follows(
        &self,
        account_id: &str,
        side: crate::FollowSide,
        with_a_page: bool,
        after: Option<&crate::SocialKey>,
        limit: usize,
    ) -> Result<Vec<crate::SocialPilot>, StoreError> {
        Ok({
            let inner = &*self.lock();
            inner.social.follows(
                account_id,
                side,
                |other| !with_a_page || inner.flights.has_public(other),
                after,
                limit,
                &inner.accounts,
            )
        })
    }

    async fn followed_flights(
        &self,
        account_id: &str,
        after: Option<&crate::FlightKey>,
        limit: usize,
    ) -> Result<Vec<crate::Flight>, StoreError> {
        Ok({
            let inner = &*self.lock();
            let pilots = inner.social.followed_by(account_id);
            inner
                .flights
                .public_of(&pilots, after, limit, &inner.accounts)
        })
    }

    async fn cheer(
        &self,
        account_id: &str,
        flight_id: &str,
        now: Timestamp,
    ) -> Result<crate::Cheering, StoreError> {
        Ok({
            let inner = &mut *self.lock();
            inner
                .social
                .cheer(account_id, flight_id, now, &inner.accounts, &inner.flights)
        })
    }

    async fn uncheer(&self, account_id: &str, flight_id: &str) -> Result<bool, StoreError> {
        Ok(self.lock().social.uncheer(account_id, flight_id))
    }

    async fn cheers(
        &self,
        flight_id: &str,
        after: Option<&crate::SocialKey>,
        limit: usize,
    ) -> Result<Vec<crate::SocialPilot>, StoreError> {
        Ok({
            let inner = &*self.lock();
            inner
                .social
                .cheers(flight_id, after, limit, &inner.accounts)
        })
    }

    async fn flight_social(
        &self,
        flight_ids: &[String],
        viewer: Option<&str>,
    ) -> Result<Vec<crate::FlightSocial>, StoreError> {
        Ok(self.lock().social.of_flights(flight_ids, viewer))
    }

    async fn add_comment(&self, new: &crate::NewComment) -> Result<crate::Commenting, StoreError> {
        Ok({
            let inner = &mut *self.lock();
            inner
                .social
                .add_comment(new, &inner.accounts, &inner.flights)
        })
    }

    async fn comments(
        &self,
        flight_id: &str,
        after: Option<&crate::SocialKey>,
        limit: usize,
    ) -> Result<Vec<crate::Comment>, StoreError> {
        Ok({
            let inner = &*self.lock();
            inner
                .social
                .comments(flight_id, after, limit, &inner.accounts)
        })
    }

    async fn comment(&self, id: &str) -> Result<Option<crate::Comment>, StoreError> {
        Ok({
            let inner = &*self.lock();
            inner.social.comment(id, &inner.accounts)
        })
    }

    async fn remove_comment(&self, id: &str) -> Result<Option<crate::Comment>, StoreError> {
        Ok({
            let inner = &mut *self.lock();
            inner.social.remove_comment(id, &inner.accounts)
        })
    }

    async fn close_comments(
        &self,
        account_id: &str,
        flight_id: &str,
        closed: bool,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        Ok({
            let _ = now;
            let inner = &mut *self.lock();
            inner
                .social
                .close_comments(account_id, flight_id, closed, &inner.flights)
        })
    }

    async fn social_given(&self, account_id: &str) -> Result<crate::SocialGiven, StoreError> {
        Ok({
            let inner = &*self.lock();
            inner.social.given(account_id, &inner.accounts)
        })
    }

    async fn add_notice(
        &self,
        account_id: &str,
        actor_id: &str,
        at: Timestamp,
        record: &str,
    ) -> Result<bool, StoreError> {
        Ok({
            let inner = &mut *self.lock();
            inner
                .social
                .add_notice(account_id, actor_id, at, record, &inner.accounts)
        })
    }

    async fn notices(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<crate::Notices, StoreError> {
        Ok(self.lock().social.notices(account_id, since, limit))
    }

    async fn prune_notices(&self, before: Timestamp) -> Result<u64, StoreError> {
        Ok(self.lock().social.prune_notices(before))
    }

    async fn prune_fired_alerts(&self, before: Timestamp) -> Result<u64, StoreError> {
        Ok(self.lock().alerts.prune(before))
    }

    async fn add_push_subscription(
        &self,
        subscription: &crate::PushSubscription,
        most: usize,
    ) -> Result<crate::AddingPush, StoreError> {
        let inner = &mut *self.lock();
        if inner
            .accounts
            .flight_change(&subscription.account_id)
            .is_none()
        {
            return Ok(crate::AddingPush::NoAccount);
        }
        Ok(inner.push.add(subscription, most))
    }

    async fn remove_push_subscription(
        &self,
        account_id: &str,
        endpoint: &str,
    ) -> Result<bool, StoreError> {
        Ok(self.lock().push.remove(account_id, endpoint))
    }

    async fn drop_push_subscription(&self, endpoint: &str) -> Result<bool, StoreError> {
        Ok(self.lock().push.drop(endpoint))
    }

    async fn push_subscriptions(
        &self,
        account_id: &str,
    ) -> Result<Vec<crate::PushSubscription>, StoreError> {
        Ok(self.lock().push.of(account_id))
    }

    async fn push_key(&self) -> Result<Option<String>, StoreError> {
        Ok(self.lock().push.key())
    }

    async fn keep_push_key(&self, key: &str, now: Timestamp) -> Result<String, StoreError> {
        Ok(self.lock().push.keep_key(key, now))
    }

    async fn add_device(
        &self,
        device: &crate::PhoneDevice,
        most: usize,
    ) -> Result<crate::AddingDevice, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.phones.add_device(device, most, &inner.accounts))
    }

    async fn remove_device(
        &self,
        account_id: &str,
        platform: &str,
        token: &str,
    ) -> Result<bool, StoreError> {
        Ok(self
            .lock()
            .phones
            .remove_device(account_id, platform, token))
    }

    async fn drop_device(&self, platform: &str, token: &str) -> Result<bool, StoreError> {
        Ok(self.lock().phones.drop_device(platform, token))
    }

    async fn devices_of(
        &self,
        account_ids: &[String],
    ) -> Result<Vec<crate::PhoneDevice>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.phones.devices_of(account_ids, &inner.accounts))
    }

    async fn nearby_settings(&self, account_id: &str) -> Result<crate::NearbySettings, StoreError> {
        Ok(self.lock().phones.settings(account_id))
    }

    async fn set_nearby_settings(
        &self,
        account_id: &str,
        settings: &crate::NearbySettings,
        _now: Timestamp,
    ) -> Result<bool, StoreError> {
        let inner = &mut *self.lock();
        if !inner.accounts.has(account_id) {
            return Ok(false);
        }
        inner.phones.set_settings(account_id, settings);
        Ok(true)
    }

    async fn keep_position(
        &self,
        account_id: &str,
        position: GeoPoint,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        Ok(self.lock().phones.keep_position(account_id, position, now))
    }

    async fn forget_position(&self, account_id: &str) -> Result<(), StoreError> {
        self.lock().phones.forget_position(account_id);
        Ok(())
    }

    async fn position_of(
        &self,
        account_id: &str,
        since: Timestamp,
    ) -> Result<Option<GeoPoint>, StoreError> {
        Ok(self.lock().phones.position_of(account_id, since))
    }

    async fn open_plan_watchers(
        &self,
        position: GeoPoint,
        except: &str,
        most: usize,
    ) -> Result<Vec<String>, StoreError> {
        Ok(self
            .lock()
            .phones
            .open_plan_watchers(position, except, most))
    }

    async fn create_help_request(
        &self,
        new: &crate::NewHelpRequest,
        reach: &crate::HelpReach,
    ) -> Result<crate::HelpCreation, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .phones
            .create_help_request(new, reach, &inner.accounts))
    }

    async fn help_request(&self, id: &str) -> Result<Option<crate::HelpRequest>, StoreError> {
        let inner = &*self.lock();
        Ok(inner.phones.help_request(id, &inner.accounts))
    }

    async fn answer_help_request(
        &self,
        id: &str,
        account_id: &str,
        can_help: bool,
        now: Timestamp,
    ) -> Result<crate::HelpAnswering, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .phones
            .answer(id, account_id, can_help, now, &inner.accounts))
    }

    async fn close_help_request(
        &self,
        id: &str,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Option<crate::HelpRequest>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.phones.close(id, account_id, now, &inner.accounts))
    }

    async fn help_requests_of(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Vec<crate::HelpRequest>, StoreError> {
        let inner = &*self.lock();
        Ok(inner.phones.requests_of(account_id, now, &inner.accounts))
    }

    async fn help_requests_asked(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Vec<crate::HelpRequest>, StoreError> {
        let inner = &*self.lock();
        Ok(inner
            .phones
            .requests_asked(account_id, now, &inner.accounts))
    }

    async fn prune_nearby(
        &self,
        positions_before: Timestamp,
        requests_before: Timestamp,
    ) -> Result<u64, StoreError> {
        Ok(self.lock().phones.prune(positions_before, requests_before))
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

    async fn flights_from(
        &self,
        place: GeoPoint,
        within_m: f64,
        best: usize,
    ) -> Result<FlightsFrom, StoreError> {
        let inner = self.lock();
        Ok(inner
            .flights
            .began_at(place, within_m, best, &inner.accounts))
    }

    async fn sites_near(
        &self,
        sources: &[SourceId],
        places: &[GeoPoint],
        within_m: f64,
    ) -> Result<Vec<Vec<Site>>, StoreError> {
        let inner = self.lock();
        let sites: Vec<&Site> = inner
            .sites
            .iter()
            .filter(|(source, _)| sources.contains(source))
            .flat_map(|(_, stored)| stored.sites.values())
            .chain(inner.pilot_sites.standing())
            .collect();
        Ok(places
            .iter()
            .map(|&place| {
                sites
                    .iter()
                    .filter(|site| site.position.distance_m(place) <= within_m)
                    .map(|&site| site.clone())
                    .collect()
            })
            .collect())
    }

    async fn takeoff_candidates(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        rule: &CandidateRule,
        limit: usize,
    ) -> Result<Vec<TakeoffCandidate>, StoreError> {
        let inner = self.lock();
        let cells = inner.flights.takeoff_cells(area);
        let sites: Vec<GeoPoint> = inner
            .sites
            .iter()
            .filter(|(source, _)| sources.contains(source))
            .flat_map(|(_, stored)| stored.sites.values())
            .map(|site| site.position)
            .chain(inner.pilot_sites.standing_positions())
            .collect();
        Ok(takeoff::candidates(cells, &sites, rule, limit))
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

    async fn scored_months(&self, which: &PublicFlights) -> Result<Vec<(Month, u32)>, StoreError> {
        Ok(self.lock().flights.scored_months(which))
    }

    async fn pilots_named(&self, text: &str, limit: usize) -> Result<Vec<FoundPilot>, StoreError> {
        let inner = self.lock();
        Ok(inner.flights.pilots_named(text, limit, &inner.accounts))
    }

    async fn flown_by(&self, account_id: &str, listing: Listing) -> Result<Vec<Flown>, StoreError> {
        Ok(self.lock().flights.flown_by(account_id, listing))
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

    async fn public_flight_changes(
        &self,
        account_id: Option<&str>,
        since: i64,
        limit: usize,
    ) -> Result<PublicFlightChanges, StoreError> {
        let inner = self.lock();
        Ok(inner
            .flights
            .public_changes(account_id, since, limit, &inner.accounts))
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

    async fn set_all_visibility(
        &self,
        account_id: &str,
        visibility: Visibility,
        now: Timestamp,
    ) -> Result<AllVisibilityChange, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .flights
            .set_all_visibility(account_id, visibility, now, &mut inner.accounts))
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
        let deleted = inner
            .flights
            .delete(account_id, id, now, &mut inner.accounts);
        if deleted {
            inner.social.delete_flight(id);
        }
        Ok(deleted)
    }

    async fn outdated_flights(
        &self,
        version: u32,
        after: Option<&(u32, String)>,
        limit: usize,
    ) -> Result<Vec<OutdatedFlight>, StoreError> {
        Ok(self.lock().flights.outdated(version, after, limit))
    }

    async fn set_glider_class(
        &self,
        account_id: &str,
        id: &str,
        class: Option<GliderClass>,
        now: Timestamp,
    ) -> Result<Option<Flight>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .flights
            .set_glider_class(account_id, id, class, now, &mut inner.accounts))
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
        let mut inner = self.lock();
        Ok(inner.groups.prune_plans(before) + inner.open_plans.prune(before))
    }

    async fn save_open_plan(
        &self,
        new: &NewOpenPlan,
        room: &OpenPlanRoom,
    ) -> Result<OpenPlanSaving, StoreError> {
        let inner = &mut *self.lock();
        inner.open_plans.save(new, room, &inner.accounts)
    }

    async fn change_open_plan(
        &self,
        plan_id: &str,
        by_account_id: &str,
        details: &OpenPlanDetails,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> Result<OpenPlanChange, StoreError> {
        let inner = &mut *self.lock();
        inner.open_plans.change(
            plan_id,
            by_account_id,
            details,
            (now, gone_before),
            &inner.accounts,
        )
    }

    async fn delete_open_plan(
        &self,
        plan_id: &str,
        by_account_id: &str,
        gone_before: NaiveDate,
    ) -> Result<OpenPlanRemoval, StoreError> {
        Ok(self
            .lock()
            .open_plans
            .delete(plan_id, by_account_id, gone_before))
    }

    async fn set_open_plan_in(
        &self,
        plan_id: &str,
        account_id: &str,
        is_in: bool,
        now: Timestamp,
        room: &OpenPlanRoom,
    ) -> Result<OpenPlanChange, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .open_plans
            .set_in(plan_id, account_id, (is_in, now), room, &inner.accounts))
    }

    async fn open_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<OpenPlan>, StoreError> {
        let inner = self.lock();
        Ok(inner.open_plans.get(plan_id, gone_before, &inner.accounts))
    }

    async fn open_plans_near(
        &self,
        area: &OpenPlanArea,
        limit: usize,
    ) -> Result<Vec<OpenPlan>, StoreError> {
        let inner = self.lock();
        Ok(inner.open_plans.near(area, limit, &inner.accounts))
    }

    async fn open_plans_of(
        &self,
        account_id: &str,
        from: NaiveDate,
        limit: usize,
    ) -> Result<Vec<OpenPlan>, StoreError> {
        let inner = self.lock();
        Ok(inner
            .open_plans
            .of(account_id, from, limit, &inner.accounts))
    }

    async fn remove_open_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<OpenPlan>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .open_plans
            .remove(plan_id, gone_before, &inner.accounts))
    }

    async fn any_group(&self, id: &str) -> Result<Option<GroupMembers>, StoreError> {
        let inner = self.lock();
        Ok(inner.groups.any(id, &inner.accounts))
    }

    async fn plans_of_any_group(
        &self,
        group_id: &str,
        from: NaiveDate,
    ) -> Result<Option<Vec<Plan>>, StoreError> {
        let inner = self.lock();
        Ok(inner.groups.plans_of_any(group_id, from, &inner.accounts))
    }

    async fn any_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<Plan>, StoreError> {
        let inner = self.lock();
        Ok(inner.groups.any_plan(plan_id, gone_before, &inner.accounts))
    }

    async fn rename_any_group(
        &self,
        id: &str,
        name: &str,
    ) -> Result<Option<GroupMembers>, StoreError> {
        let inner = &mut *self.lock();
        inner.groups.rename_any(id, name, &inner.accounts)
    }

    async fn remove_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<Plan>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner
            .groups
            .remove_plan(plan_id, gone_before, &inner.accounts))
    }

    async fn remove_group(
        &self,
        id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<RemovedGroup>, StoreError> {
        let inner = &mut *self.lock();
        Ok(inner.groups.remove(id, gone_before, &inner.accounts))
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
