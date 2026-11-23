//! A store for tests of what happens when the store is in trouble.
//!
//! Code that uses a [`Store`] has to cope with a database that is down or
//! slow. Neither is easy to arrange with a real one, so this crate ships a
//! store that does nothing else, for its users' tests.

use std::time::Duration;

use chrono::NaiveDate;
use venti_domain::{
    Airspace, BoundingBox, Cable, GeoPoint, Observation, Site, SiteId, SourceId, Station,
    StationId, Timestamp,
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
    VisibilityChange,
};
use crate::{GoogleLink, GoogleLinking, NewGoogleAccount};
use crate::{KeepingSite, KeptRoom, KeptSiteChanges, NewKeptSite};

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

    async fn set_site(&self, _: &SiteId) -> Result<Option<Site>, StoreError> {
        self.fail().await
    }

    async fn save_cable_run(
        &self,
        _: &RunRecord,
        _: &SiteDelivery,
        _: &[Cable],
    ) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn cables_in(
        &self,
        _: &[SourceId],
        _: &BoundingBox,
        _: usize,
    ) -> Result<Vec<CableLayer>, StoreError> {
        self.fail().await
    }

    async fn cable_sets(&self, _: &[SourceId]) -> Result<Vec<CableSet>, StoreError> {
        self.fail().await
    }

    async fn add_pilot_site(&self, _: &NewPilotSite, _: f64) -> Result<SiteWrite, StoreError> {
        self.fail().await
    }

    async fn revise_pilot_site(&self, _: &SiteRevising, _: f64) -> Result<SiteWrite, StoreError> {
        self.fail().await
    }

    async fn pilot_sites_in(
        &self,
        _: &SourceId,
        _: &BoundingBox,
        _: usize,
    ) -> Result<Vec<PilotSite>, StoreError> {
        self.fail().await
    }

    async fn pilot_site(&self, _: &SiteId) -> Result<Option<PilotSite>, StoreError> {
        self.fail().await
    }

    async fn pilot_site_revisions(
        &self,
        _: &SiteId,
        _: u32,
        _: usize,
    ) -> Result<Option<(PilotSite, Vec<SiteRevision>)>, StoreError> {
        self.fail().await
    }

    async fn pilot_sites_revisions(&self, _: &[SiteId]) -> Result<Vec<SiteRevision>, StoreError> {
        self.fail().await
    }

    async fn pilot_site_changes(
        &self,
        _: &SourceId,
        _: i64,
        _: usize,
    ) -> Result<PilotSiteChanges, StoreError> {
        self.fail().await
    }

    async fn given_by(&self, _: &str) -> Result<crate::Given, StoreError> {
        self.fail().await
    }

    async fn confirm_pilot_site(
        &self,
        _: &NewSiteConfirmation,
    ) -> Result<SiteConfirming, StoreError> {
        self.fail().await
    }

    async fn withdraw_site_confirmation(
        &self,
        _: &SiteId,
        _: &str,
    ) -> Result<Option<(PilotSite, bool)>, StoreError> {
        self.fail().await
    }

    async fn site_confirmations(
        &self,
        _: &SiteId,
        _: usize,
    ) -> Result<Option<(PilotSite, Vec<SiteConfirmation>)>, StoreError> {
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

    async fn create_google_account(
        &self,
        _: &NewGoogleAccount,
    ) -> Result<AccountCreation, StoreError> {
        self.fail().await
    }

    async fn google_account(&self, _: &str) -> Result<Option<Account>, StoreError> {
        self.fail().await
    }

    async fn google_link(&self, _: &str) -> Result<Option<GoogleLink>, StoreError> {
        self.fail().await
    }

    async fn link_google(&self, _: &str, _: &GoogleLink) -> Result<GoogleLinking, StoreError> {
        self.fail().await
    }

    async fn unlink_google(&self, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn has_password(&self, _: &str) -> Result<bool, StoreError> {
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

    async fn let_edit_sites(&self, _: &str, _: Timestamp) -> Result<Option<Timestamp>, StoreError> {
        self.fail().await
    }

    async fn stop_editing_sites(&self, _: &str) -> Result<Option<bool>, StoreError> {
        self.fail().await
    }

    async fn site_editors(&self) -> Result<Vec<Account>, StoreError> {
        self.fail().await
    }

    async fn update_account_settings(
        &self,
        _: &str,
        _: &crate::AccountSettings,
    ) -> Result<Option<Account>, StoreError> {
        self.fail().await
    }

    async fn set_picture(&self, _: &crate::NewPicture) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn remove_picture(&self, _: &str) -> Result<Option<crate::Picture>, StoreError> {
        self.fail().await
    }

    async fn picture(&self, _: &str) -> Result<Option<crate::Picture>, StoreError> {
        self.fail().await
    }

    async fn claim_google_picture(&self, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn pilots_near(
        &self,
        _: GeoPoint,
        _: f64,
        _: Timestamp,
        _: usize,
    ) -> Result<Vec<crate::NearPilot>, StoreError> {
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

    async fn create_access_token(
        &self,
        _: &AccessToken,
        _: &SecretHash,
        _: usize,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn use_access_token(
        &self,
        _: &SecretHash,
        _: Timestamp,
    ) -> Result<Option<(AccessToken, Account)>, StoreError> {
        self.fail().await
    }

    async fn access_tokens(&self, _: &str) -> Result<Vec<AccessToken>, StoreError> {
        self.fail().await
    }

    async fn delete_access_token(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn create_oauth_client(
        &self,
        _: &crate::OauthClient,
        _: usize,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn oauth_client(&self, _: &str) -> Result<Option<crate::OauthClient>, StoreError> {
        self.fail().await
    }

    async fn create_authorization_request(
        &self,
        _: &SecretHash,
        _: &crate::AuthorizationRequest,
        _: Timestamp,
        _: usize,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn authorization_request(
        &self,
        _: &SecretHash,
        _: Timestamp,
    ) -> Result<crate::RequestState, StoreError> {
        self.fail().await
    }

    async fn answer_authorization_request(
        &self,
        _: &SecretHash,
        _: Timestamp,
    ) -> Result<crate::RequestState, StoreError> {
        self.fail().await
    }

    async fn create_authorization_code(
        &self,
        _: &SecretHash,
        _: &crate::AuthorizationCode,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn authorization_code(
        &self,
        _: &SecretHash,
        _: Timestamp,
    ) -> Result<Option<crate::StoredCode>, StoreError> {
        self.fail().await
    }

    async fn redeem_authorization_code(
        &self,
        _: &SecretHash,
        _: &crate::NewGrant,
        _: Timestamp,
        _: usize,
    ) -> Result<crate::Redeemed, StoreError> {
        self.fail().await
    }

    async fn refresh_grant(
        &self,
        _: &SecretHash,
        _: &str,
        _: &crate::Rotation,
        _: Timestamp,
    ) -> Result<crate::Refreshed, StoreError> {
        self.fail().await
    }

    async fn revoke_grant(&self, _: &SecretHash, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn prune_oauth(&self, _: Timestamp, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn keep_site(
        &self,
        _: &str,
        _: &NewKeptSite,
        _: Timestamp,
        _: KeptRoom,
    ) -> Result<KeepingSite, StoreError> {
        self.fail().await
    }

    async fn forget_kept_site(
        &self,
        _: &str,
        _: &str,
        _: Timestamp,
        _: KeptRoom,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn kept_site_changes(
        &self,
        _: &str,
        _: i64,
        _: usize,
    ) -> Result<KeptSiteChanges, StoreError> {
        self.fail().await
    }

    async fn alerts(&self, _: &str) -> Result<Vec<crate::Alert>, StoreError> {
        self.fail().await
    }

    async fn add_alert(
        &self,
        _: &str,
        _: &crate::NewAlert,
        _: Timestamp,
        _: usize,
    ) -> Result<crate::AddingAlert, StoreError> {
        self.fail().await
    }

    async fn change_alert(
        &self,
        _: &str,
        _: &crate::NewAlert,
        _: Timestamp,
    ) -> Result<Option<crate::Alert>, StoreError> {
        self.fail().await
    }

    async fn remove_alert(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn wind_alerts_at(&self, _: &[String]) -> Result<Vec<crate::Alert>, StoreError> {
        self.fail().await
    }

    async fn alerts_of_kind(&self, _: &str) -> Result<Vec<crate::Alert>, StoreError> {
        self.fail().await
    }

    async fn note_alert_check(&self, _: &crate::AlertCheck) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn fired_alerts(
        &self,
        _: &str,
        _: i64,
        _: usize,
    ) -> Result<crate::FiredAlerts, StoreError> {
        self.fail().await
    }

    async fn follow(
        &self,
        _: &str,
        _: &str,
        _: Timestamp,
        _: usize,
    ) -> Result<crate::Following, StoreError> {
        self.fail().await
    }

    async fn unfollow(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn follow_counts(&self, _: &str) -> Result<crate::FollowCounts, StoreError> {
        self.fail().await
    }

    async fn follows_pilot(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn follows(
        &self,
        _: &str,
        _: crate::FollowSide,
        _: bool,
        _: Option<&crate::SocialKey>,
        _: usize,
    ) -> Result<Vec<crate::SocialPilot>, StoreError> {
        self.fail().await
    }

    async fn followed_flights(
        &self,
        _: &str,
        _: Option<&crate::FlightKey>,
        _: usize,
    ) -> Result<Vec<crate::Flight>, StoreError> {
        self.fail().await
    }

    async fn cheer(&self, _: &str, _: &str, _: Timestamp) -> Result<crate::Cheering, StoreError> {
        self.fail().await
    }

    async fn uncheer(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn cheers(
        &self,
        _: &str,
        _: Option<&crate::SocialKey>,
        _: usize,
    ) -> Result<Vec<crate::SocialPilot>, StoreError> {
        self.fail().await
    }

    async fn flight_social(
        &self,
        _: &[String],
        _: Option<&str>,
    ) -> Result<Vec<crate::FlightSocial>, StoreError> {
        self.fail().await
    }

    async fn add_comment(&self, _: &crate::NewComment) -> Result<crate::Commenting, StoreError> {
        self.fail().await
    }

    async fn comments(
        &self,
        _: &str,
        _: Option<&crate::SocialKey>,
        _: usize,
    ) -> Result<Vec<crate::Comment>, StoreError> {
        self.fail().await
    }

    async fn comment(&self, _: &str) -> Result<Option<crate::Comment>, StoreError> {
        self.fail().await
    }

    async fn remove_comment(&self, _: &str) -> Result<Option<crate::Comment>, StoreError> {
        self.fail().await
    }

    async fn close_comments(
        &self,
        _: &str,
        _: &str,
        _: bool,
        _: Timestamp,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn social_given(&self, _: &str) -> Result<crate::SocialGiven, StoreError> {
        self.fail().await
    }

    async fn add_notice(
        &self,
        _: &str,
        _: &str,
        _: Timestamp,
        _: &str,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn notices(&self, _: &str, _: i64, _: usize) -> Result<crate::Notices, StoreError> {
        self.fail().await
    }

    async fn prune_notices(&self, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn prune_fired_alerts(&self, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn add_device(
        &self,
        _: &crate::PhoneDevice,
        _: usize,
    ) -> Result<crate::AddingDevice, StoreError> {
        self.fail().await
    }

    async fn remove_device(&self, _: &str, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn drop_device(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn devices_of(&self, _: &[String]) -> Result<Vec<crate::PhoneDevice>, StoreError> {
        self.fail().await
    }

    async fn nearby_settings(&self, _: &str) -> Result<crate::NearbySettings, StoreError> {
        self.fail().await
    }

    async fn set_nearby_settings(
        &self,
        _: &str,
        _: &crate::NearbySettings,
        _: Timestamp,
    ) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn keep_position(&self, _: &str, _: GeoPoint, _: Timestamp) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn forget_position(&self, _: &str) -> Result<(), StoreError> {
        self.fail().await
    }

    async fn position_of(&self, _: &str, _: Timestamp) -> Result<Option<GeoPoint>, StoreError> {
        self.fail().await
    }

    async fn open_plan_watchers(
        &self,
        _: GeoPoint,
        _: &str,
        _: usize,
    ) -> Result<Vec<String>, StoreError> {
        self.fail().await
    }

    async fn create_help_request(
        &self,
        _: &crate::NewHelpRequest,
        _: &crate::HelpReach,
    ) -> Result<crate::HelpCreation, StoreError> {
        self.fail().await
    }

    async fn help_request(&self, _: &str) -> Result<Option<crate::HelpRequest>, StoreError> {
        self.fail().await
    }

    async fn answer_help_request(
        &self,
        _: &str,
        _: &str,
        _: bool,
        _: Timestamp,
    ) -> Result<crate::HelpAnswering, StoreError> {
        self.fail().await
    }

    async fn close_help_request(
        &self,
        _: &str,
        _: &str,
        _: Timestamp,
    ) -> Result<Option<crate::HelpRequest>, StoreError> {
        self.fail().await
    }

    async fn help_requests_of(
        &self,
        _: &str,
        _: Timestamp,
    ) -> Result<Vec<crate::HelpRequest>, StoreError> {
        self.fail().await
    }

    async fn help_requests_asked(
        &self,
        _: &str,
        _: Timestamp,
    ) -> Result<Vec<crate::HelpRequest>, StoreError> {
        self.fail().await
    }

    async fn prune_nearby(&self, _: Timestamp, _: Timestamp) -> Result<u64, StoreError> {
        self.fail().await
    }

    async fn add_push_subscription(
        &self,
        _: &crate::PushSubscription,
        _: usize,
    ) -> Result<crate::AddingPush, StoreError> {
        self.fail().await
    }

    async fn remove_push_subscription(&self, _: &str, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn drop_push_subscription(&self, _: &str) -> Result<bool, StoreError> {
        self.fail().await
    }

    async fn push_subscriptions(
        &self,
        _: &str,
    ) -> Result<Vec<crate::PushSubscription>, StoreError> {
        self.fail().await
    }

    async fn push_key(&self) -> Result<Option<String>, StoreError> {
        self.fail().await
    }

    async fn keep_push_key(&self, _: &str, _: Timestamp) -> Result<String, StoreError> {
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

    async fn flights_from(
        &self,
        _: GeoPoint,
        _: f64,
        _: usize,
    ) -> Result<crate::FlightsFrom, StoreError> {
        self.fail().await
    }

    async fn sites_near(
        &self,
        _: &[SourceId],
        _: &[GeoPoint],
        _: f64,
    ) -> Result<Vec<Vec<Site>>, StoreError> {
        self.fail().await
    }

    async fn takeoff_candidates(
        &self,
        _: &[SourceId],
        _: &BoundingBox,
        _: &crate::CandidateRule,
        _: usize,
    ) -> Result<Vec<crate::TakeoffCandidate>, StoreError> {
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

    async fn scored_months(&self, _: &PublicFlights) -> Result<Vec<(Month, u32)>, StoreError> {
        self.fail().await
    }

    async fn pilots_named(&self, _: &str, _: usize) -> Result<Vec<crate::FoundPilot>, StoreError> {
        self.fail().await
    }

    async fn flown_by(&self, _: &str, _: Listing) -> Result<Vec<crate::Flown>, StoreError> {
        self.fail().await
    }

    async fn flight_changes(&self, _: &str, _: i64, _: usize) -> Result<FlightChanges, StoreError> {
        self.fail().await
    }

    async fn public_flight_changes(
        &self,
        _: Option<&str>,
        _: i64,
        _: usize,
    ) -> Result<PublicFlightChanges, StoreError> {
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

    async fn set_all_visibility(
        &self,
        _: &str,
        _: Visibility,
        _: Timestamp,
    ) -> Result<crate::AllVisibilityChange, StoreError> {
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

    async fn set_glider_class(
        &self,
        _: &str,
        _: &str,
        _: Option<crate::GliderClass>,
        _: Timestamp,
    ) -> Result<Option<Flight>, StoreError> {
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

    async fn save_open_plan(
        &self,
        _: &crate::NewOpenPlan,
        _: &crate::OpenPlanRoom,
    ) -> Result<crate::OpenPlanSaving, StoreError> {
        self.fail().await
    }

    async fn change_open_plan(
        &self,
        _: &str,
        _: &str,
        _: &crate::OpenPlanDetails,
        _: Timestamp,
        _: NaiveDate,
    ) -> Result<crate::OpenPlanChange, StoreError> {
        self.fail().await
    }

    async fn delete_open_plan(
        &self,
        _: &str,
        _: &str,
        _: NaiveDate,
    ) -> Result<crate::OpenPlanRemoval, StoreError> {
        self.fail().await
    }

    async fn set_open_plan_in(
        &self,
        _: &str,
        _: &str,
        _: bool,
        _: Timestamp,
        _: &crate::OpenPlanRoom,
    ) -> Result<crate::OpenPlanChange, StoreError> {
        self.fail().await
    }

    async fn open_plan(
        &self,
        _: &str,
        _: NaiveDate,
    ) -> Result<Option<crate::OpenPlan>, StoreError> {
        self.fail().await
    }

    async fn open_plans_near(
        &self,
        _: &crate::OpenPlanArea,
        _: usize,
    ) -> Result<Vec<crate::OpenPlan>, StoreError> {
        self.fail().await
    }

    async fn open_plans_of(
        &self,
        _: &str,
        _: NaiveDate,
        _: usize,
    ) -> Result<Vec<crate::OpenPlan>, StoreError> {
        self.fail().await
    }

    async fn remove_open_plan(
        &self,
        _: &str,
        _: NaiveDate,
    ) -> Result<Option<crate::OpenPlan>, StoreError> {
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
