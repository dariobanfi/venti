//! A store for tests of what happens when the store is in trouble.
//!
//! Code that uses a [`Store`] has to cope with a database that is down or
//! slow. Neither is easy to arrange with a real one, so this crate ships a
//! store that does nothing else, for its users' tests.

use std::time::Duration;

use chrono::NaiveDate;
use venti_domain::{
    Airspace, BoundingBox, Observation, Site, SourceId, Station, StationId, Timestamp,
};

use crate::flight::{Room, Saved};
use crate::{
    Account, AccountCreation, AirspaceLayer, AirspaceSet, Coverage, Derived, Flight, FlightChanges,
    FlightKey, ForecastModel, Group, GroupCreation, GroupJoining, GroupMembers, GroupRoom,
    KeeperAct, Listing, LiveBatch, LiveFix, LiveFixAt, LiveRemoval, LiveRoom, LiveSaved, LiveTrack,
    MemberRemoval, Month, NewAccount, NewFlight, NewGroup, NewInvite, NewPlan, NewReport,
    OutdatedFlight, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom, PlanSaving,
    PublicFlights, PublicOrder, Ranked, RemovedGroup, Report, ReportClosing, ReportKey,
    ReportListing, ReportSaved, RunRecord, SecretHash, Session, SiteDelivery, SiteLayer, SiteSet,
    SourceStatus, StationLatest, Store, StoreError, Suspension, ThermalPart, Visibility,
    VisibilityChange,
};

/// A store whose database is down, or answers with a failure after a delay.
/// Every call fails; nothing is ever stored.
#[derive(Debug, Clone, Copy)]
pub struct TroubledStore {
    delay: Option<Duration>,
}

impl TroubledStore {
    /// A store whose database cannot be reached: every call fails at once
    /// with [`StoreError::Unavailable`].
    pub fn down() -> Self {
        Self { delay: None }
    }

    /// A store that takes `delay` to fail, with [`StoreError::Failure`].
    pub fn slow(delay: Duration) -> Self {
        Self { delay: Some(delay) }
    }

    async fn fail<T>(&self) -> Result<T, StoreError> {
        match self.delay {
            Some(delay) => {
                tokio::time::sleep(delay).await;
                Err(StoreError::Failure("too late to matter".to_owned()))
            }
            None => Err(StoreError::Unavailable("connection refused".to_owned())),
        }
    }
}

impl Store for TroubledStore {
    async fn save_run(
        &self,
        _: &RunRecord,
        _: &[Station],
        _: &[Observation],
    ) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn stations_in(
        &self,
        _: &BoundingBox,
        _: usize,
    ) -> Result<Vec<StationLatest>, StoreError> {
        self.fail().await
    }

    async fn station(&self, _: &StationId) -> Result<Option<Station>, StoreError> {
        self.fail().await
    }

    async fn observations(
        &self,
        _: &StationId,
        _: Timestamp,
        _: Timestamp,
    ) -> Result<Vec<Observation>, StoreError> {
        self.fail().await
    }

    async fn source_status(&self, _: &[SourceId]) -> Result<Vec<SourceStatus>, StoreError> {
        self.fail().await
    }

    async fn prune_before(&self, _: Month) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn save_airspace_run(
        &self,
        _: &RunRecord,
        _: &Coverage,
        _: &[Airspace],
    ) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn airspaces_in(
        &self,
        _: &[SourceId],
        _: &BoundingBox,
        _: usize,
    ) -> Result<Vec<AirspaceLayer>, StoreError> {
        self.fail().await
    }

    async fn airspace_sets(&self, _: &[SourceId]) -> Result<Vec<AirspaceSet>, StoreError> {
        self.fail().await
    }

    async fn save_site_run(
        &self,
        _: &RunRecord,
        _: &SiteDelivery,
        _: &[Site],
    ) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn sites_in(
        &self,
        _: &[SourceId],
        _: &BoundingBox,
        _: usize,
    ) -> Result<Vec<SiteLayer>, StoreError> {
        self.fail().await
    }

    async fn site_sets(&self, _: &[SourceId]) -> Result<Vec<SiteSet>, StoreError> {
        self.fail().await
    }

    async fn save_forecast_check(
        &self,
        _: &RunRecord,
        _: &[ForecastModel],
    ) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn forecast_models(&self, _: &[SourceId]) -> Result<Vec<ForecastModel>, StoreError> {
        self.fail().await
    }

    async fn save_thermal_part(&self, _: &ThermalPart) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn thermal_parts(&self, _: &SourceId) -> Result<Vec<ThermalPart>, StoreError> {
        self.fail().await
    }

    async fn create_account(&self, _: &NewAccount) -> Result<AccountCreation, StoreError> {
        self.fail().await
    }

    async fn account(&self, _: &str) -> Result<Option<Account>, StoreError> {
        self.fail().await
    }

    async fn password_hash(&self, _: &str) -> Result<Option<(Account, String)>, StoreError> {
        self.fail().await
    }

    async fn set_password(&self, _: &str, _: &str, _: &str) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn delete_account(&self, _: &str) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn suspend_publishing(
        &self,
        _: &str,
        _: Timestamp,
    ) -> Result<Option<Suspension>, StoreError> {
        self.fail().await
    }

    async fn resume_publishing(&self, _: &str) -> Result<Option<bool>, StoreError> {
        self.fail().await
    }

    async fn create_invite(&self, _: &NewInvite) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn create_session(
        &self,
        _: &Session,
        _: &SecretHash,
        _: usize,
    ) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn use_session(
        &self,
        _: &SecretHash,
        _: Timestamp,
        _: Timestamp,
    ) -> Result<Option<(Session, Account)>, StoreError> {
        self.fail().await
    }

    async fn sessions(&self, _: &str) -> Result<Vec<Session>, StoreError> {
        self.fail().await
    }

    async fn delete_session(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn delete_sessions(&self, _: &str) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn prune_sessions(&self, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn save_flight(&self, _: &NewFlight, _: &Room) -> Result<Saved, StoreError> {
        self.fail().await
    }

    async fn flight(&self, _: &str) -> Result<Option<Flight>, StoreError> {
        self.fail().await
    }

    async fn flight_file(&self, _: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.fail().await
    }

    async fn flights_of(
        &self,
        _: &str,
        _: Listing,
        _: Option<&FlightKey>,
        _: usize,
    ) -> Result<Vec<Flight>, StoreError> {
        self.fail().await
    }

    async fn public_flights(
        &self,
        _: &PublicFlights,
        _: &PublicOrder,
        _: usize,
    ) -> Result<Vec<Flight>, StoreError> {
        self.fail().await
    }

    async fn ranking(
        &self,
        _: &PublicFlights,
        _: usize,
        _: usize,
    ) -> Result<Vec<Ranked>, StoreError> {
        self.fail().await
    }

    async fn flight_changes(&self, _: &str, _: i64, _: usize) -> Result<FlightChanges, StoreError> {
        self.fail().await
    }

    async fn set_visibility(
        &self,
        _: &str,
        _: &str,
        _: Visibility,
        _: Timestamp,
    ) -> Result<VisibilityChange, StoreError> {
        self.fail().await
    }

    async fn take_down_flight(&self, _: &str, _: Timestamp) -> Result<Option<Flight>, StoreError> {
        self.fail().await
    }

    async fn release_flight(&self, _: &str, _: Timestamp) -> Result<Option<Flight>, StoreError> {
        self.fail().await
    }

    async fn delete_flight(&self, _: &str, _: &str, _: Timestamp) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn outdated_flights(
        &self,
        _: u32,
        _: Option<&(u32, String)>,
        _: usize,
    ) -> Result<Vec<OutdatedFlight>, StoreError> {
        self.fail().await
    }

    async fn set_derived(&self, _: &str, _: &Derived, _: Timestamp) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn save_live_fixes(&self, _: &LiveBatch, _: &LiveRoom) -> Result<LiveSaved, StoreError> {
        self.fail().await
    }

    async fn live_track(&self, _: &str) -> Result<Option<LiveTrack>, StoreError> {
        self.fail().await
    }

    async fn live_fixes(
        &self,
        _: &str,
        _: i64,
        _: Option<Timestamp>,
        _: usize,
    ) -> Result<Vec<LiveFixAt>, StoreError> {
        self.fail().await
    }

    async fn live_tail(&self, _: &str, _: usize) -> Result<Vec<LiveFix>, StoreError> {
        self.fail().await
    }

    async fn live_tracks_in(
        &self,
        _: &BoundingBox,
        _: Timestamp,
        _: usize,
    ) -> Result<Vec<LiveTrack>, StoreError> {
        self.fail().await
    }

    async fn delete_live_track(
        &self,
        _: &str,
        _: &str,
        _: Timestamp,
        _: bool,
    ) -> Result<LiveRemoval, StoreError> {
        self.fail().await
    }

    async fn remove_live_track(&self, _: &str, _: Timestamp) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn prune_live_tracks(&self, _: Timestamp, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn create_group(&self, _: &NewGroup, _: &GroupRoom) -> Result<GroupCreation, StoreError> {
        self.fail().await
    }

    async fn join_group(
        &self,
        _: &str,
        _: &SecretHash,
        _: &GroupRoom,
        _: Timestamp,
    ) -> Result<GroupJoining, StoreError> {
        self.fail().await
    }

    async fn groups_of(&self, _: &str) -> Result<Vec<Group>, StoreError> {
        self.fail().await
    }

    async fn group(&self, _: &str, _: &str) -> Result<Option<GroupMembers>, StoreError> {
        self.fail().await
    }

    async fn rename_group(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        self.fail().await
    }

    async fn renew_group_invite(
        &self,
        _: &str,
        _: &str,
        _: &SecretHash,
    ) -> Result<KeeperAct<()>, StoreError> {
        self.fail().await
    }

    async fn hand_over_group(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        self.fail().await
    }

    async fn remove_group_member(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<MemberRemoval, StoreError> {
        self.fail().await
    }

    async fn delete_group(&self, _: &str, _: &str) -> Result<KeeperAct<()>, StoreError> {
        self.fail().await
    }

    async fn save_plan(&self, _: &NewPlan, _: &PlanRoom) -> Result<PlanSaving, StoreError> {
        self.fail().await
    }

    async fn change_plan(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &PlanDetails,
        _: Timestamp,
        _: NaiveDate,
    ) -> Result<PlanChange, StoreError> {
        self.fail().await
    }

    async fn delete_plan(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: NaiveDate,
    ) -> Result<PlanRemoval, StoreError> {
        self.fail().await
    }

    async fn set_plan_in(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: bool,
        _: Timestamp,
        _: NaiveDate,
    ) -> Result<PlanChange, StoreError> {
        self.fail().await
    }

    async fn plans_of_group(
        &self,
        _: &str,
        _: &str,
        _: NaiveDate,
        _: Option<NaiveDate>,
    ) -> Result<Option<Vec<Plan>>, StoreError> {
        self.fail().await
    }

    async fn plans_for(
        &self,
        _: &str,
        _: NaiveDate,
        _: Option<NaiveDate>,
        _: usize,
    ) -> Result<Vec<Plan>, StoreError> {
        self.fail().await
    }

    async fn prune_plans(&self, _: NaiveDate) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn any_group(&self, _: &str) -> Result<Option<GroupMembers>, StoreError> {
        self.fail().await
    }

    async fn plans_of_any_group(
        &self,
        _: &str,
        _: NaiveDate,
    ) -> Result<Option<Vec<Plan>>, StoreError> {
        self.fail().await
    }

    async fn any_plan(&self, _: &str, _: NaiveDate) -> Result<Option<Plan>, StoreError> {
        self.fail().await
    }

    async fn rename_any_group(&self, _: &str, _: &str) -> Result<Option<GroupMembers>, StoreError> {
        self.fail().await
    }

    async fn remove_plan(&self, _: &str, _: NaiveDate) -> Result<Option<Plan>, StoreError> {
        self.fail().await
    }

    async fn remove_group(
        &self,
        _: &str,
        _: NaiveDate,
    ) -> Result<Option<RemovedGroup>, StoreError> {
        self.fail().await
    }

    async fn save_report(&self, _: &NewReport) -> Result<ReportSaved, StoreError> {
        self.fail().await
    }

    async fn reports(
        &self,
        _: ReportListing,
        _: Option<&ReportKey>,
        _: usize,
    ) -> Result<Vec<Report>, StoreError> {
        self.fail().await
    }

    async fn close_report(
        &self,
        _: &str,
        _: &str,
        _: Timestamp,
    ) -> Result<ReportClosing, StoreError> {
        self.fail().await
    }

    async fn prune_reports(&self, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }
}
