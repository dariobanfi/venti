//! Where the server keeps what it must not lose.
//!
//! [`Store`] is the interface; it has two implementations:
//!
//! - [`PgStore`] keeps everything in PostgreSQL with PostGIS. This is what
//!   the server runs on.
//! - [`MemoryStore`] keeps everything in the process. Tests and the server's
//!   development mode use it; nothing survives a restart.
//!
//! Both pass the same behavioural test suite (`tests/suite`), so code written
//! against the interface behaves the same on either.
//!
//! The store holds two kinds of things:
//!
//! - what the data sources deliver: canonical [`Station`], [`Observation`],
//!   [`Airspace`] and [`Site`] records from `venti-domain`, each belonging
//!   to exactly one source, and the history of the sources' runs. Of
//!   forecasts it keeps only which run each weather model was at
//!   ([`ForecastModel`]): the forecasts stay with the service that
//!   computes them;
//! - what pilots entrust to the server: their [`Account`], the [`Session`]s
//!   of their devices, their [`Flight`]s with the files they came from, and
//!   for a short time the [`LiveTrack`]s of flights under way;
//! - what pilots share with each other and with nobody else: the
//!   [`Group`]s they are in, and the [`Plan`]s the members of a group make
//!   for a flying day;
//! - what whoever runs the server needs to answer for what is shared on
//!   it: the [`Report`]s pilots send, and what the operator did about a
//!   flight, a live track, an account, a group or a plan.
//!
//! It knows nothing about providers, about HTTP or about how a password or
//! a flight file is read: it is handed hashes and finished records.

mod account;
mod airspace;
mod alert;
mod batch;
mod cable;
mod flight;
mod forecast;
mod group;
mod kept_site;
mod live;
mod memory;
mod month;
mod oauth;
mod pilot_site;
mod postgres;
mod push;
mod report;
mod site;
mod social;
mod takeoff;
pub mod testing;

use std::future::Future;

use chrono::NaiveDate;
use venti_domain::{
    Airspace, BoundingBox, Cable, GeoPoint, Observation, Site, SiteId, SourceId, Station,
    StationId, Timestamp,
};

pub use account::{
    AccessToken, Account, AccountCreation, GoogleLink, GoogleLinking, NewAccount, NewGoogleAccount,
    NewInvite, SESSION_TOUCH_MINUTES, SecretHash, Session, Suspension,
};
pub use airspace::{AirspaceLayer, AirspaceSet, Coverage};
pub use alert::{
    AddingAlert, Alert, AlertCheck, DAY_ALERT, FiredAlert, FiredAlerts, NewAlert, PILOTS_ALERT,
    WIND_ALERT,
};
pub use cable::{CableLayer, CableSet};
pub use flight::{
    AllFlightsSet, AllVisibilityChange, DeletedFlight, Derived, Flight, FlightChanges, FlightKey,
    Flown, FoundPilot, Kept, Listing, Near, NewFlight, OutdatedFlight, Place, PublicFlightChanges,
    PublicFlights, PublicOrder, Ranked, Room, Saved, Score, ScoreKey, ScoreKind, Visibility,
    VisibilityChange, WithdrawnFlight,
};
pub use forecast::{ForecastModel, ThermalPart};
pub use group::{
    Group, GroupCreation, GroupJoining, GroupMembers, GroupPilot, GroupRoom, KeeperAct,
    MemberRemoval, NewGroup, NewPlan, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom,
    PlanSaving, RemovedGroup,
};
pub use kept_site::{ForgottenSite, KeepingSite, KeptRoom, KeptSite, KeptSiteChanges, NewKeptSite};
pub use live::{
    LiveBatch, LiveFix, LiveFixAt, LiveKept, LiveRemoval, LiveRoom, LiveSaved, LiveTrack,
    LiveVisibility,
};
pub use memory::MemoryStore;
pub use month::Month;
pub use oauth::{
    AuthorizationCode, AuthorizationRequest, NewGrant, OauthClient, Redeemed, Refreshed,
    RequestState, Rotation, StoredCode, TokenClient,
};
pub use pilot_site::{
    Given, NewPilotSite, NewSiteConfirmation, PARTNERS_LAYER, PilotSite, PilotSiteChanges,
    RevisionKind, SiteAct, SiteConfirmation, SiteConfirmed, SiteConfirming, SitePilot,
    SiteRevising, SiteRevision, SiteVia, SiteWrite,
};
pub use postgres::PgStore;
pub use push::{AddingPush, PushSubscription};
pub use report::{
    NewReport, Report, ReportAbout, ReportClosing, ReportClosure, ReportKey, ReportListing,
    ReportSaved, ReportSubject,
};
pub use site::{SiteDelivery, SiteLayer, SiteSet};
pub use social::{
    Cheering, Comment, Commenting, FlightSocial, FollowCounts, FollowSide, Following, NewComment,
    Notice, Notices, SocialGiven, SocialKey, SocialPilot,
};
pub use takeoff::{
    CANDIDATE_ALTITUDE_STEP_M, CANDIDATE_STEPS_PER_DEGREE, CELLS_PER_DEGREE, CandidateRule,
    FlightsFrom, TakeoffCandidate, solar_day,
};

/// Why the store could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The database cannot be reached, or no connection was free in time.
    /// Trying again later can help.
    #[error("the store is unavailable: {0}")]
    Unavailable(String),
    /// The caller passed records the store does not accept.
    #[error("the store refused the records: {0}")]
    Invalid(String),
    /// The database failed, or holds something that is not a canonical
    /// record. A bug here or a damaged database.
    #[error("store failure: {0}")]
    Failure(String),
}

/// A station with the newest observation the store holds for it.
#[derive(Debug, Clone, PartialEq)]
pub struct StationLatest {
    /// The station.
    pub station: Station,
    /// Its newest observation, if it has any.
    pub latest: Option<Observation>,
}

/// What happened in one run of a source, as the store keeps it.
///
/// A source's runs are told apart by the time they started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    /// The source that ran.
    pub source: SourceId,
    /// When the run started.
    pub started_at: Timestamp,
    /// When it finished.
    pub finished_at: Timestamp,
    /// Whether the run did its job. For observations: it delivered some,
    /// and nearly every request worked. For airspace: it delivered a set
    /// that is fit to replace the one before.
    pub healthy: bool,
    /// Requests made to the provider.
    pub requests: u32,
    /// Requests that failed or whose response was unusable.
    pub failed_requests: u32,
    /// Pieces of responses the adapter could not convert.
    pub skipped: u32,
    /// Values removed by validation.
    pub rejected: u32,
    /// Stations delivered.
    pub stations: u32,
    /// Observations delivered.
    pub observations: u32,
    /// The time of the newest observation delivered.
    pub newest_observation: Option<Timestamp>,
    /// Airspaces delivered. Zero for a source of observations.
    pub airspaces: u32,
}

/// What the run history says about one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStatus {
    /// The source.
    pub source: SourceId,
    /// When its latest run started.
    pub last_started: Option<Timestamp>,
    /// When its latest healthy run finished.
    pub last_success: Option<Timestamp>,
    /// When its latest unhealthy run finished.
    pub last_failure: Option<Timestamp>,
    /// How many stations its latest healthy run delivered.
    pub stations: u32,
    /// The newest observation time any of its runs reported, taken from the
    /// latest run that delivered observations.
    pub newest_observation: Option<Timestamp>,
}

impl SourceStatus {
    /// The status of a source that has never run.
    pub fn never_ran(source: SourceId) -> Self {
        Self {
            source,
            last_started: None,
            last_success: None,
            last_failure: None,
            stations: 0,
            newest_observation: None,
        }
    }
}

/// Persistence for what the sources deliver and for what pilots keep on the
/// server.
pub trait Store: Send + Sync + 'static {
    /// Stores what one run delivered together with the run's outcome, all or
    /// nothing.
    ///
    /// - A station that is already known is updated.
    /// - An observation for a station and time that is already stored is
    ///   merged into it: a measurement in the new observation replaces the
    ///   stored one, and a measurement the new observation lacks leaves the
    ///   stored one alone. Providers publish wind and temperature at
    ///   different times, and one failed request must not erase what an
    ///   earlier run stored.
    /// - Saving the same run again changes nothing.
    ///
    /// Every observation must belong to one of `stations`, and no two may
    /// share a station and time; the pipeline's output satisfies both.
    /// Anything else is refused with [`StoreError::Invalid`] and nothing is
    /// stored.
    fn save_run(
        &self,
        run: &RunRecord,
        stations: &[Station],
        observations: &[Observation],
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The stations inside `area`, edges included, each with its newest
    /// observation, sorted by source and then by station identifier. At most
    /// `limit` are returned.
    fn stations_in(
        &self,
        area: &BoundingBox,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<StationLatest>, StoreError>> + Send;

    /// One station, if the store knows it.
    fn station(
        &self,
        id: &StationId,
    ) -> impl Future<Output = Result<Option<Station>, StoreError>> + Send;

    /// What a station measured from `from` to `to`, both included, oldest
    /// first.
    fn observations(
        &self,
        station: &StationId,
        from: Timestamp,
        to: Timestamp,
    ) -> impl Future<Output = Result<Vec<Observation>, StoreError>> + Send;

    /// What the run history says about each of `sources`, in the order given.
    /// A source that never ran gets [`SourceStatus::never_ran`].
    fn source_status(
        &self,
        sources: &[SourceId],
    ) -> impl Future<Output = Result<Vec<SourceStatus>, StoreError>> + Send;

    /// Removes everything older than the start of `first_kept`: observations
    /// measured before it, stations no run has delivered since, and the
    /// records of runs that started before it. Airspace and flying sites
    /// are not touched: a source's set stays until a newer one replaces
    /// it. Neither is what is known about the forecast models.
    ///
    /// Retention works in whole calendar months because that is the unit the
    /// database can drop cheaply.
    fn prune_before(
        &self,
        first_kept: Month,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    // --- Airspace ---------------------------------------------------------

    /// Stores the outcome of one run of an airspace source and, if the run
    /// was healthy, replaces the airspace stored for the source by what the
    /// run delivered, all or nothing.
    ///
    /// - After a healthy run the store holds exactly `airspaces` for the
    ///   source: what the set before had and this one lacks is gone. With
    ///   the set it keeps `coverage`, which says where that set is whole.
    /// - A run that was not healthy leaves the stored airspace as it is,
    ///   and neither `coverage` nor `airspaces` is looked at. Half a set is worse than
    ///   yesterday's whole one: a pilot is not warned about airspace that
    ///   is missing.
    /// - A run that started before the run whose set is stored replaces
    ///   nothing either.
    /// - Saving the same run again changes nothing.
    ///
    /// A healthy run must deliver at least one airspace, every airspace
    /// must belong to the run's source, and no two may share an
    /// identifier; the pipeline's output satisfies all three. Anything else
    /// is refused with [`StoreError::Invalid`] and nothing is stored.
    fn save_airspace_run(
        &self,
        run: &RunRecord,
        coverage: &Coverage,
        airspaces: &[Airspace],
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The stored airspace of `sources` whose bounding box touches `area`,
    /// edges included: one layer per source the store holds a set of,
    /// sorted by source, with the airspaces of each sorted by identifier.
    /// At most `limit` airspaces are returned in all: the first in that
    /// order.
    ///
    /// A source whose set has nothing in `area` is a layer without
    /// airspaces. That is an answer of its own: the set says where it is
    /// whole, and only with that can "nothing found" be told from "not
    /// held". A source the store holds no set of is not listed.
    ///
    /// An airspace is found by the smallest box around its outline, so one
    /// whose outline curves past `area` may be among the answers. None
    /// that reaches into `area` is ever missing.
    ///
    /// The answer is taken at one moment. Each source's airspace is in it
    /// as one run delivered it, never as a mixture of two.
    fn airspaces_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<AirspaceLayer>, StoreError>> + Send;

    /// What the store holds of each of `sources`, sorted by source. A
    /// source of which it holds no airspace is left out.
    fn airspace_sets(
        &self,
        sources: &[SourceId],
    ) -> impl Future<Output = Result<Vec<AirspaceSet>, StoreError>> + Send;

    // --- Flying sites -----------------------------------------------------

    /// Stores the outcome of one run of a source of flying sites and, if
    /// the run was healthy, replaces the sites stored for the source by
    /// what the run delivered, all or nothing.
    ///
    /// The rules are those of [`Store::save_airspace_run`]: after a healthy
    /// run the store holds exactly `sites` for the source, with what
    /// `delivery` says about the set; a run that was not healthy, or that
    /// started before the run whose set is stored, replaces nothing; and
    /// saving the same run again changes nothing.
    ///
    /// A healthy run must deliver at least one site, every site must
    /// belong to the run's source, and no two may share an identifier.
    /// Anything else is refused with [`StoreError::Invalid`] and nothing is
    /// stored.
    fn save_site_run(
        &self,
        run: &RunRecord,
        delivery: &SiteDelivery,
        sites: &[Site],
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The stored sites of `sources` inside `area`, edges included: one
    /// layer per source the store holds a set of, sorted by source, with
    /// the sites of each sorted by identifier. At most `limit` sites are
    /// returned in all: the first in that order.
    ///
    /// A source whose set has nothing in `area` is a layer without sites,
    /// which still says how old the set is and where it is whole. A source
    /// the store holds no set of is not listed. The answer is taken at one
    /// moment: each source's sites are in it as one run delivered them.
    fn sites_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<SiteLayer>, StoreError>> + Send;

    /// What the store holds of each of `sources`, sorted by source. A
    /// source of which it holds no sites is left out.
    fn site_sets(
        &self,
        sources: &[SourceId],
    ) -> impl Future<Output = Result<Vec<SiteSet>, StoreError>> + Send;

    /// Saves a run of a cable source, as [`Store::save_site_run`] saves one
    /// of sites: the set replaces the stored one only if the run was
    /// healthy and is not older than it. A healthy run must deliver at
    /// least one cable, each of the run's source, no two with one key.
    fn save_cable_run(
        &self,
        run: &RunRecord,
        delivery: &SiteDelivery,
        cables: &[Cable],
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The stored cables of `sources` whose line's box touches `area`: one
    /// layer per source the store holds a set of, sorted by source, the
    /// cables of each by key, at most `limit` in all.
    fn cables_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<CableLayer>, StoreError>> + Send;

    /// What the store holds of each of `sources`, sorted by source.
    fn cable_sets(
        &self,
        sources: &[SourceId],
    ) -> impl Future<Output = Result<Vec<CableSet>, StoreError>> + Send;

    /// The site with this identifier in the whole set of its source, as
    /// the set the store holds now has it. `None` if the set has none: a
    /// site of a source goes when a run no longer delivers it.
    fn set_site(
        &self,
        id: &SiteId,
    ) -> impl Future<Output = Result<Option<Site>, StoreError>> + Send;

    // --- Sites that pilots make -------------------------------------------

    /// Adds a site that a pilot made, as its first revision, unless a
    /// standing site of the layer is nearer to it than `apart_m` metres;
    /// see [`SiteWrite`], of which this answers `Written`, `TooClose` and
    /// `NoSuchAccount`. Of two calls that race for one place, one gets it.
    ///
    /// An identifier that is in use is refused with
    /// [`StoreError::Invalid`]: an identifier is never used twice, also
    /// not after its site was removed.
    fn add_pilot_site(
        &self,
        new: &NewPilotSite,
        apart_m: f64,
    ) -> impl Future<Output = Result<SiteWrite, StoreError>> + Send;

    /// Writes a revision of a site that pilots made: a change, a restore
    /// of an earlier revision, or its removal; see [`SiteWrite`]. The
    /// revisions before it stay as they are. A site that stands after the
    /// revision is held to the same distance from the others as a new one.
    fn revise_pilot_site(
        &self,
        revising: &SiteRevising,
        apart_m: f64,
    ) -> impl Future<Output = Result<SiteWrite, StoreError>> + Send;

    /// The standing sites of one layer of the server's own inside a box,
    /// edges included, sorted by identifier, `limit` at most. A removed
    /// site is not among them. The layer is the source its sites'
    /// identifiers begin with.
    fn pilot_sites_in(
        &self,
        layer: &SourceId,
        area: &BoundingBox,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<PilotSite>, StoreError>> + Send;

    /// A site that pilots made, standing or removed. `None` if no site
    /// ever had the identifier.
    fn pilot_site(
        &self,
        id: &SiteId,
    ) -> impl Future<Output = Result<Option<PilotSite>, StoreError>> + Send;

    /// A site that pilots made with one page of its revisions: those with
    /// a number above `after`, by number, `limit` at most. `None` if no
    /// site ever had the identifier.
    fn pilot_site_revisions(
        &self,
        id: &SiteId,
        after: u32,
        limit: usize,
    ) -> impl Future<Output = Result<Option<(PilotSite, Vec<SiteRevision>)>, StoreError>> + Send;

    /// Every revision of the sites named, whichever layer each is of: by
    /// site as identifiers are ordered, then by number. An identifier no
    /// site ever had adds nothing. It is for whoever writes a whole layer
    /// out (the dumps): one question for a page of sites, where
    /// [`Store::pilot_site_revisions`] is one per site. It asks about
    /// nobody's standing: what of a site may be shown is the caller's to
    /// decide from the site itself.
    fn pilot_sites_revisions(
        &self,
        ids: &[SiteId],
    ) -> impl Future<Output = Result<Vec<SiteRevision>, StoreError>> + Send;

    /// What changed among the sites of one layer of the server's own after
    /// the change number `since`: every site that took a later number, by
    /// that number, `limit` at most. A site takes a number with every
    /// revision, and with every confirmation that is given or withdrawn.
    /// A site that changed several times is listed once, as it is now.
    ///
    /// The layers share one count, so `latest` may be a number that a site
    /// of another layer took: it is still a place to ask from again.
    fn pilot_site_changes(
        &self,
        layer: &SourceId,
        since: i64,
        limit: usize,
    ) -> impl Future<Output = Result<PilotSiteChanges, StoreError>> + Send;

    /// Keeps a pilot's word that they took off at a site and that it was
    /// good: one for an account and a site, so a second one takes the
    /// place of the first. The site takes a new change number and no
    /// revision. See [`SiteConfirming`].
    fn confirm_pilot_site(
        &self,
        new: &NewSiteConfirmation,
    ) -> impl Future<Output = Result<SiteConfirming, StoreError>> + Send;

    /// Takes an account's confirmation of a site back. `None` if no site
    /// ever had the identifier; otherwise the site as it stands now and
    /// whether there was a confirmation. A site that had one takes a new
    /// change number.
    fn withdraw_site_confirmation(
        &self,
        site: &SiteId,
        account_id: &str,
    ) -> impl Future<Output = Result<Option<(PilotSite, bool)>, StoreError>> + Send;

    /// A site with the confirmations it has: the newest day first, then
    /// by handle, `limit` at most. `None` if no site ever had the
    /// identifier. A confirmation goes with its account
    /// ([`Store::delete_account`]): it is the pilot's word, not a record
    /// given to everyone.
    fn site_confirmations(
        &self,
        site: &SiteId,
        limit: usize,
    ) -> impl Future<Output = Result<Option<(PilotSite, Vec<SiteConfirmation>)>, StoreError>> + Send;

    /// What an account gave that no other call lists by account: every
    /// revision of a site it wrote, every confirmation it has, and every
    /// report it sent that is still kept. See [`Given`].
    ///
    /// This is for the account itself, when it takes what it gave with
    /// it. A revision is listed whether its site stands, is removed or
    /// was taken down: it is its author's own writing. A report is listed
    /// with what whoever runs the server did about it, which the caller
    /// keeps to itself. An account that does not exist gave nothing.
    fn given_by(&self, account_id: &str) -> impl Future<Output = Result<Given, StoreError>> + Send;

    // --- Forecasts --------------------------------------------------------

    /// Stores the outcome of one look at a forecast source and, if the look
    /// was healthy, replaces what is stored about the source's models by
    /// what it found, all or nothing.
    ///
    /// A look that was not healthy, or that started before the look whose
    /// findings are stored, replaces nothing. Saving the same look again
    /// changes nothing.
    ///
    /// A healthy look must have found at least one model, every model must
    /// belong to the run's source and carry the run's start as the time it
    /// was looked at, and no model may be listed twice. Anything else is
    /// refused with [`StoreError::Invalid`] and nothing is stored.
    fn save_forecast_check(
        &self,
        run: &RunRecord,
        models: &[ForecastModel],
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The models of `sources` as the last healthy look found them, sorted
    /// by source and then by model.
    fn forecast_models(
        &self,
        sources: &[SourceId],
    ) -> impl Future<Output = Result<Vec<ForecastModel>, StoreError>> + Send;

    /// Stores what the thermal map judged of one model of a source, in
    /// place of what is stored for that source and model. A part written
    /// before the stored one replaces nothing.
    fn save_thermal_part(
        &self,
        part: &ThermalPart,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The parts of the thermal map stored for a source, sorted by model.
    fn thermal_parts(
        &self,
        source: &SourceId,
    ) -> impl Future<Output = Result<Vec<ThermalPart>, StoreError>> + Send;

    // --- Accounts ---------------------------------------------------------

    /// Creates an account with its password, all or nothing.
    ///
    /// With an invite, the invite must be known, unused and not expired at
    /// the account's creation time, and is used up by this call. An invite
    /// can create one account, also when two requests race for it.
    fn create_account(
        &self,
        new: &NewAccount,
    ) -> impl Future<Output = Result<AccountCreation, StoreError>> + Send;

    /// The account with this handle, if there is one. Handles are stored in
    /// lower case and looked up exactly.
    fn account(
        &self,
        handle: &str,
    ) -> impl Future<Output = Result<Option<Account>, StoreError>> + Send;

    /// The account with this handle and the hash of its password.
    fn password_hash(
        &self,
        handle: &str,
    ) -> impl Future<Output = Result<Option<(Account, String)>, StoreError>> + Send;

    /// Creates an account that is signed in to with Google and has no
    /// password, all or nothing. The invite is checked first and used up
    /// as by [`create_account`](Self::create_account); then the handle;
    /// then whether another account is linked to the Google account
    /// ([`AccountCreation::GoogleTaken`]).
    fn create_google_account(
        &self,
        new: &NewGoogleAccount,
    ) -> impl Future<Output = Result<AccountCreation, StoreError>> + Send;

    /// The account linked to the Google account with this subject, if one
    /// is.
    fn google_account(
        &self,
        subject: &str,
    ) -> impl Future<Output = Result<Option<Account>, StoreError>> + Send;

    /// The Google account an account is linked to, if it is.
    fn google_link(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Option<GoogleLink>, StoreError>> + Send;

    /// Links an account to a Google account, in place of one it was linked
    /// to before. Linking it again to the one it has keeps the first
    /// moment and takes the new email.
    fn link_google(
        &self,
        account_id: &str,
        google: &GoogleLink,
    ) -> impl Future<Output = Result<GoogleLinking, StoreError>> + Send;

    /// Unlinks an account from its Google account. Whether it was linked.
    fn unlink_google(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// Whether an account has a password. One made with Google has none.
    fn has_password(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// Gives an account a new password hash and signs out every device
    /// except the one with session `keep_session`, all or nothing: whoever
    /// knew the old password is out. Does nothing for an unknown account.
    fn set_password(
        &self,
        account_id: &str,
        password_hash: &str,
        keep_session: &str,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Removes an account and everything that belongs to it: its password,
    /// its sessions, the sites it keeps as its own, its flights and their
    /// files, the record of the
    /// flights it deleted, its live tracks and the identifiers they used,
    /// the reports it made and the reports about it, with the words those
    /// kept of its plans and of groups it kept, and what it put into
    /// groups: the plans it posted and its word on the plans of others.
    /// Nothing is kept. Its handle is free again.
    ///
    /// It leaves every group it is in, as
    /// [`Store::remove_group_member`] has a member leave: a group it kept
    /// is kept by the member who has been in it longest, and a group it
    /// was alone in goes with it.
    fn delete_account(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Takes from an account the ability to publish, until
    /// [`Store::resume_publishing`] gives it back. `None` if there is no
    /// such account.
    ///
    /// From this call on the account can make no flight public
    /// ([`Store::set_visibility`]), and its live tracks are for those with
    /// their link whatever its batches say ([`Store::save_live_fixes`]).
    /// What it had made public is taken out of every list at once, all or
    /// nothing: each public flight becomes unlisted, as a change of its
    /// own, in the order of the flights' addresses, and each public live
    /// track becomes one for those with its link. Whoever has a link keeps
    /// seeing what the link shows. Nothing is deleted, and everything else
    /// about the account works as before.
    ///
    /// Suspending an account that is suspended keeps the moment of the
    /// first call.
    fn suspend_publishing(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Suspension>, StoreError>> + Send;

    /// Gives an account the ability to publish back. `None` if there is no
    /// such account; otherwise whether it was suspended.
    ///
    /// Nothing is made public by this: what the suspension made unlisted
    /// stays unlisted until its pilot publishes it again.
    fn resume_publishing(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Option<bool>, StoreError>> + Send;

    /// Names an account as trusted to add and correct flying sites of the
    /// server's own layer, until [`Store::stop_editing_sites`] withdraws
    /// it. Returns since when it is trusted: the moment of this call, or
    /// of the earlier one if it was trusted already. `None` if there is no
    /// such account.
    ///
    /// It is one fact about the account and changes nothing else. Whether
    /// a write of a site is let through is the server's to decide with it;
    /// the store writes a site for whichever account it is given.
    fn let_edit_sites(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Timestamp>, StoreError>> + Send;

    /// Withdraws it. `None` if there is no such account; otherwise whether
    /// it was trusted.
    ///
    /// Nothing else changes: every site the account added and every
    /// revision it wrote stays as it is, with the account as its author.
    fn stop_editing_sites(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Option<bool>, StoreError>> + Send;

    /// Every account that is trusted to add and correct sites, in the
    /// order of the handles.
    fn site_editors(&self) -> impl Future<Output = Result<Vec<Account>, StoreError>> + Send;

    /// Sets whether a flight the account uploads from now on is public
    /// from the start ([`Account::new_flights_public`]), and returns the
    /// account as it then is. `None` if there is no such account.
    ///
    /// It changes no flight the account has: [`Store::set_all_visibility`]
    /// does that.
    fn set_new_flights_public(
        &self,
        account_id: &str,
        public: bool,
    ) -> impl Future<Output = Result<Option<Account>, StoreError>> + Send;

    /// Stores an invite that one account can be created with.
    fn create_invite(
        &self,
        invite: &NewInvite,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    // --- Sessions ---------------------------------------------------------

    /// Stores a session under the hash of its token. If the account then
    /// has more than `keep` sessions, the ones used longest ago are removed
    /// until `keep` are left: a device that signs in again and again cannot
    /// fill the table.
    fn create_session(
        &self,
        session: &Session,
        token: &SecretHash,
        keep: usize,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The session a token belongs to, with its account, and notes that it
    /// was used at `now`.
    ///
    /// A session last used before `idle_since` is not returned: it has
    /// expired. The use is written down only when the recorded one is more
    /// than [`SESSION_TOUCH_MINUTES`] old; the session returned carries the
    /// time that is then on record.
    fn use_session(
        &self,
        token: &SecretHash,
        now: Timestamp,
        idle_since: Timestamp,
    ) -> impl Future<Output = Result<Option<(Session, Account)>, StoreError>> + Send;

    /// An account's sessions, most recently used first.
    fn sessions(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<Session>, StoreError>> + Send;

    /// Removes one session of an account. Returns whether the account had
    /// it: a session of another account is not touched.
    fn delete_session(
        &self,
        account_id: &str,
        session_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// Removes every session of an account.
    fn delete_sessions(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Removes the sessions of every account that were last used before
    /// `idle_since`, and returns how many there were.
    fn prune_sessions(
        &self,
        idle_since: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    // --- Tokens a pilot gives to another program ---------------------------

    /// Stores a token a pilot made by hand under the hash of its secret,
    /// unless its account already holds `most` tokens or is gone. Returns
    /// whether it was stored. A token that names a program
    /// ([`AccessToken::client`]) is not made this way and is not stored:
    /// it comes of [`Store::redeem_authorization_code`].
    ///
    /// Unlike a session, a token never makes room for itself by removing
    /// an older one: a program whose token went that way would stop
    /// working with nobody having decided so. Tokens whose end has come
    /// are counted until they are withdrawn.
    fn create_access_token(
        &self,
        token: &AccessToken,
        secret: &SecretHash,
        most: usize,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// The token a secret belongs to, with its account, and notes the day
    /// of `now` as the day of its latest use.
    ///
    /// A token whose end is at `now` or before it is not returned, and
    /// its use is not noted; neither is one that came of a pilot's
    /// agreement once its access token's own end has come
    /// ([`TokenClient::access_until`]). The token returned carries the day
    /// that is then on record.
    fn use_access_token(
        &self,
        secret: &SecretHash,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<(AccessToken, Account)>, StoreError>> + Send;

    /// An account's tokens, the newest first; those whose end has come
    /// among them.
    fn access_tokens(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<AccessToken>, StoreError>> + Send;

    /// Removes one token of an account. Returns whether the account had
    /// it: a token of another account is not touched.
    fn delete_access_token(
        &self,
        account_id: &str,
        token_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    // --- The authorization server -------------------------------------------

    /// Stores a program that registered itself, unless `most` are stored
    /// already and none of them can make room. Returns whether it was
    /// stored.
    ///
    /// Room is made by the program that registered longest ago among those
    /// no pilot ever agreed to: a program a pilot agreed to is never
    /// removed to make room.
    fn create_oauth_client(
        &self,
        client: &OauthClient,
        most: usize,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// The registered program with this identifier.
    fn oauth_client(
        &self,
        client_id: &str,
    ) -> impl Future<Output = Result<Option<OauthClient>, StoreError>> + Send;

    /// Keeps a request that waits for a pilot's answer, under the hash of
    /// its identifier, after removing those whose end is at `now` or
    /// before it. Returns `false`, and keeps nothing, if the program is
    /// gone or `most` requests wait already: none makes room for a new
    /// one, they end by themselves within minutes.
    fn create_authorization_request(
        &self,
        request: &SecretHash,
        record: &AuthorizationRequest,
        now: Timestamp,
        most: usize,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// What is held under the hash of a request's identifier at `now`.
    /// Looking changes nothing.
    fn authorization_request(
        &self,
        request: &SecretHash,
        now: Timestamp,
    ) -> impl Future<Output = Result<RequestState, StoreError>> + Send;

    /// Takes a request for answering it: it is marked as answered and
    /// handed out, once. Of two callers that race, one gets
    /// [`RequestState::Waiting`] with the request and the other
    /// [`RequestState::Answered`], as does every later caller until the
    /// request's end.
    fn answer_authorization_request(
        &self,
        request: &SecretHash,
        now: Timestamp,
    ) -> impl Future<Output = Result<RequestState, StoreError>> + Send;

    /// Stores the code a pilot's agreement was turned into, under its
    /// hash, and notes the agreement as the program's latest use. Returns
    /// `false`, and stores nothing, if the program or the account is gone.
    fn create_authorization_code(
        &self,
        code: &SecretHash,
        record: &AuthorizationCode,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// The code with this hash, used or not, unless its end is at `now` or
    /// before it. Looking changes nothing.
    fn authorization_code(
        &self,
        code: &SecretHash,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<StoredCode>, StoreError>> + Send;

    /// Exchanges a code for the agreement `grant`, all or nothing: the
    /// code is used up, an earlier agreement of the same account with the
    /// same program is replaced, and the new one is stored with its first
    /// access token and refresh token, unless the account then held more
    /// than `most` tokens.
    ///
    /// A code that was exchanged before is not exchanged again, and what
    /// came of the first exchange is withdrawn ([`Redeemed::Spent`]). The
    /// caller checks first that whoever asks may exchange the code at all.
    fn redeem_authorization_code(
        &self,
        code: &SecretHash,
        grant: &NewGrant,
        now: Timestamp,
        most: usize,
    ) -> impl Future<Output = Result<Redeemed, StoreError>> + Send;

    /// Replaces the secrets of the agreement a refresh token of the
    /// program `client_id` belongs to, all or nothing, and returns the
    /// agreement as it then is.
    ///
    /// A refresh token that was used before ends its agreement
    /// ([`Refreshed::Reused`]). One of another program is unknown here.
    fn refresh_grant(
        &self,
        refresh: &SecretHash,
        client_id: &str,
        next: &Rotation,
        now: Timestamp,
    ) -> impl Future<Output = Result<Refreshed, StoreError>> + Send;

    /// Withdraws the agreement that a secret of the program `client_id`
    /// belongs to: its access token, or its refresh token that is not used
    /// up. Returns whether there was one.
    fn revoke_grant(
        &self,
        secret: &SecretHash,
        client_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// Removes what the authorization server no longer needs: requests,
    /// codes and refresh tokens whose end is at `now` or before it, agreements whose
    /// end has come, and registered programs that hold no agreement and
    /// showed no sign of life since `unused_since`. Returns how many
    /// agreements and programs were removed.
    fn prune_oauth(
        &self,
        now: Timestamp,
        unused_since: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    // --- The sites a pilot keeps as their own ------------------------------

    /// Keeps a site for an account, and returns it as the account keeps
    /// it now.
    ///
    /// A site the account does not keep is kept, from the moment `new`
    /// names or from `now` if it names none, unless the account already
    /// keeps `room.sites` others. A site the account keeps is replaced
    /// only by one that was kept later; otherwise what the account has
    /// stays, takes no change number, and is what is returned. A moment
    /// after `now` counts as `now`. A site the account had forgotten is
    /// kept again.
    ///
    /// The list is private to the account: nothing reads it for anybody
    /// else, and [`Store::delete_account`] removes it.
    fn keep_site(
        &self,
        account_id: &str,
        new: &NewKeptSite,
        now: Timestamp,
        room: KeptRoom,
    ) -> impl Future<Output = Result<KeepingSite, StoreError>> + Send;

    /// Forgets a site an account keeps. Returns whether the account kept
    /// it; forgetting what is not kept changes nothing.
    ///
    /// That the site was forgotten is remembered, for the account's other
    /// devices, for the `room.forgotten` sites forgotten last; see
    /// [`KeptSiteChanges::floor`].
    fn forget_kept_site(
        &self,
        account_id: &str,
        site_id: &str,
        now: Timestamp,
        room: KeptRoom,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// What changed among an account's kept sites after change number
    /// `since`, oldest change first, at most `limit` changes. A site
    /// appears once, as it is now. The answer is taken at one moment.
    fn kept_site_changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> impl Future<Output = Result<KeptSiteChanges, StoreError>> + Send;

    // --- The alarms a pilot sets ------------------------------------------

    /// The alarms of an account, the oldest first.
    fn alerts(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<Alert>, StoreError>> + Send;

    /// Adds an alarm to an account, made and changed at `now`, unless the
    /// account has `most` alarms already.
    fn add_alert(
        &self,
        account_id: &str,
        new: &NewAlert,
        now: Timestamp,
        most: usize,
    ) -> impl Future<Output = Result<AddingAlert, StoreError>> + Send;

    /// Changes an alarm of an account, changed at `now`, and forgets what
    /// the server noted of it. `None` if the account has no such alarm.
    fn change_alert(
        &self,
        account_id: &str,
        new: &NewAlert,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Alert>, StoreError>> + Send;

    /// Removes an alarm of an account. Returns whether the account had it.
    /// What it fired stays until it is old enough to go.
    fn remove_alert(
        &self,
        account_id: &str,
        alert_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// The alarms that are on and watch one of these stations, of every
    /// account, by account and then identifier.
    fn wind_alerts_at(
        &self,
        station_ids: &[String],
    ) -> impl Future<Output = Result<Vec<Alert>, StoreError>> + Send;

    /// The alarms that are on and of a kind, of every account, by account
    /// and then identifier.
    fn alerts_of_kind(
        &self,
        kind: &str,
    ) -> impl Future<Output = Result<Vec<Alert>, StoreError>> + Send;

    /// Writes down what a check found: the alarm's new state and what it
    /// fired, together. Returns whether it was written: not when the alarm
    /// is gone or was changed after it was read for the check.
    fn note_alert_check(
        &self,
        check: &AlertCheck,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// What an account's alarms fired after number `since`, oldest first,
    /// at most `limit`.
    fn fired_alerts(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> impl Future<Output = Result<FiredAlerts, StoreError>> + Send;

    /// Removes what alarms fired before `before`. Returns how many.
    fn prune_fired_alerts(
        &self,
        before: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    // --- Pilots among each other -------------------------------------------

    /// Makes `follower_id` follow `followed_id` from `now` on, unless it follows `most`
    /// accounts already. Following an account one follows already changes nothing.
    fn follow(
        &self,
        follower_id: &str,
        followed_id: &str,
        now: Timestamp,
        most: usize,
    ) -> impl Future<Output = Result<Following, StoreError>> + Send;

    /// Ends that. Returns whether the one followed the other.
    fn unfollow(
        &self,
        follower_id: &str,
        followed_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// How many accounts follow an account, and how many it follows.
    fn follow_counts(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<FollowCounts, StoreError>> + Send;

    /// Whether `follower_id` follows `followed_id`.
    fn follows_pilot(
        &self,
        follower_id: &str,
        followed_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// One page of the accounts that follow an account, or that it follows, the newest
    /// first, behind `after`. With `with_a_page`, only accounts that have a public flight.
    fn follows(
        &self,
        account_id: &str,
        side: FollowSide,
        with_a_page: bool,
        after: Option<&SocialKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<SocialPilot>, StoreError>> + Send;

    /// One page of the public flights of the accounts an account follows, the newest
    /// first, behind `after`. No flight that is not public is among them.
    fn followed_flights(
        &self,
        account_id: &str,
        after: Option<&FlightKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Flight>, StoreError>> + Send;

    /// Makes an account cheer a public flight. Cheering it again changes nothing.
    fn cheer(
        &self,
        account_id: &str,
        flight_id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Cheering, StoreError>> + Send;

    /// Takes a cheer back, whoever may see the flight. Returns whether there was one.
    fn uncheer(
        &self,
        account_id: &str,
        flight_id: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// One page of the accounts that cheer a flight, the newest first, behind `after`.
    /// Whether the flight shows them is the caller's to ask.
    fn cheers(
        &self,
        flight_id: &str,
        after: Option<&SocialKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<SocialPilot>, StoreError>> + Send;

    /// What pilots made of some flights, in the order they were asked for, and whether
    /// `viewer` cheers each. Whether a flight shows it is the caller's to ask.
    fn flight_social(
        &self,
        flight_ids: &[String],
        viewer: Option<&str>,
    ) -> impl Future<Output = Result<Vec<FlightSocial>, StoreError>> + Send;

    /// Stores a comment on a public flight whose pilot takes comments.
    fn add_comment(
        &self,
        new: &NewComment,
    ) -> impl Future<Output = Result<Commenting, StoreError>> + Send;

    /// One page of a flight's comments, the oldest first, behind `after`. Whether the
    /// flight shows them is the caller's to ask.
    fn comments(
        &self,
        flight_id: &str,
        after: Option<&SocialKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Comment>, StoreError>> + Send;

    /// One comment by its identifier.
    fn comment(&self, id: &str)
    -> impl Future<Output = Result<Option<Comment>, StoreError>> + Send;

    /// Removes a comment, whoever asks: who may is the caller's to decide. Returns the
    /// comment as it was.
    fn remove_comment(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Comment>, StoreError>> + Send;

    /// Sets whether a flight of an account takes no more comments. Returns `false` when
    /// the account has no such flight.
    fn close_comments(
        &self,
        account_id: &str,
        flight_id: &str,
        closed: bool,
        now: Timestamp,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// The cheers and the comments an account gave, for its archive.
    fn social_given(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<SocialGiven, StoreError>> + Send;

    /// Tells an account something that `actor_id` did, under the next number of the
    /// account's notices. Returns `false` when either account is not there.
    fn add_notice(
        &self,
        account_id: &str,
        actor_id: &str,
        at: Timestamp,
        record: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// What an account was told after number `since`, oldest first, up to `limit`.
    fn notices(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> impl Future<Output = Result<Notices, StoreError>> + Send;

    /// Removes what accounts were told before `before`. Returns how many.
    fn prune_notices(
        &self,
        before: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    // --- Web Push ----------------------------------------------------------

    /// Adds a browser's subscription to an account, subscribed at `now`,
    /// unless the account has `most` already. An endpoint that is there
    /// is taken over, by this account and with these keys, and is not
    /// counted against it.
    fn add_push_subscription(
        &self,
        subscription: &PushSubscription,
        most: usize,
    ) -> impl Future<Output = Result<AddingPush, StoreError>> + Send;

    /// Removes a subscription of an account. Returns whether it had it.
    fn remove_push_subscription(
        &self,
        account_id: &str,
        endpoint: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// Removes a subscription, whoever's it is: its push service said it
    /// is gone. Returns whether it was there.
    fn drop_push_subscription(
        &self,
        endpoint: &str,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// The subscriptions of an account, the oldest first.
    fn push_subscriptions(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<PushSubscription>, StoreError>> + Send;

    /// The server's private key for Web Push, as text, if it has one.
    fn push_key(&self) -> impl Future<Output = Result<Option<String>, StoreError>> + Send;

    /// Keeps `key` as the server's key for Web Push unless it has one, and
    /// returns the one it keeps: of two servers that start at once, both
    /// end with the same key.
    fn keep_push_key(
        &self,
        key: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<String, StoreError>> + Send;

    // --- Flights ----------------------------------------------------------

    /// Stores a flight, unless its account already has a flight with the
    /// same identity or has no room for another. Says which of the three
    /// it was, with the stored flight or with what the account keeps.
    ///
    /// A new flight is public from its upload when its account's setting
    /// says so ([`Account::new_flights_public`]), as if its pilot had made
    /// it public that moment: it has its `published_at` and a number of the
    /// public count, and a withdrawal of an earlier flight at its address
    /// is no longer told. It is private when the setting is off, when the
    /// account's publishing is suspended, and when it comes back taken
    /// down; the account's row is read under its lock, so neither rule
    /// can be raced past.
    ///
    /// The flight that is already there is returned untouched: the first
    /// file received is the one that is kept. Two calls that race for the
    /// same flight create one. A flight the account deleted earlier can be
    /// stored again, and is then no longer listed as deleted.
    ///
    /// `room` is the most the account may keep, this flight included.
    /// Whether there is room is decided together with storing: of two
    /// calls that race for the last place, one gets it. A flight the
    /// account already has is always returned, also by a full account.
    ///
    /// An account that does not exist is refused with
    /// [`StoreError::Invalid`].
    fn save_flight(
        &self,
        new: &NewFlight,
        room: &Room,
    ) -> impl Future<Output = Result<Saved, StoreError>> + Send;

    /// The flight with this address, whoever it belongs to and whoever may
    /// see it. Deciding that is the caller's job.
    fn flight(&self, id: &str) -> impl Future<Output = Result<Option<Flight>, StoreError>> + Send;

    /// A flight's file, exactly as it was handed to [`Store::save_flight`].
    fn flight_file(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, StoreError>> + Send;

    /// One page of an account's flights, newest first: by start, and by
    /// address among flights that start in the same second. `after` is the
    /// [`Flight::key`] of the last flight of the page before; `None` starts
    /// at the newest. At most `limit` are returned.
    fn flights_of(
        &self,
        account_id: &str,
        listing: Listing,
        after: Option<&FlightKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Flight>, StoreError>> + Send;

    /// One page of everyone's public flights: those `which` describes, in
    /// the order `order` names, after the place it names. At most `limit`
    /// are returned.
    ///
    /// A flight that is private or unlisted is never among them, whatever
    /// `which` and `order` say: not in a box, not on a day, and not after
    /// a place that was taken from it while it was public.
    fn public_flights(
        &self,
        which: &PublicFlights,
        order: &PublicOrder,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Flight>, StoreError>> + Send;

    /// The pilots of everyone's public flights by the sum of the points of
    /// their best: of the flights `which` describes, each pilot's `counted`
    /// best count. The pilot with the most points comes first, and among
    /// pilots with the same points the handle decides. At most `limit`
    /// pilots are returned, from the top.
    ///
    /// A flight that is private or unlisted counts for nobody, and one too
    /// short to score counts for nothing: a pilot without a scored public
    /// flight among those asked for is not ranked.
    fn ranking(
        &self,
        which: &PublicFlights,
        counted: usize,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Ranked>, StoreError>> + Send;

    /// The months in which the public flights that `which` describes and
    /// that score started, by UTC, the oldest first, each with how many of
    /// them: the months a ranking of those flights has someone in.
    ///
    /// A flight that is private or unlisted is in no month's count.
    fn scored_months(
        &self,
        which: &PublicFlights,
    ) -> impl Future<Output = Result<Vec<(Month, u32)>, StoreError>> + Send;

    /// The pilots with a public flight whose handle or display name holds
    /// `text`, whatever the case of either: those of whom one begins with
    /// it first, then the pilot with the most public flights, then by
    /// handle, byte by byte. At most `limit` are returned.
    ///
    /// A pilot who has made no flight public is never found, and a flight
    /// that is private or unlisted is in no count.
    fn pilots_named(
        &self,
        text: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<FoundPilot>, StoreError>> + Send;

    /// Every public flight of an account in the few numbers a pilot's
    /// totals are made of, newest first as [`Store::flights_of`] orders
    /// them. See [`Flown`].
    ///
    /// A flight that is private or unlisted is not among them.
    fn flown_by(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<Flown>, StoreError>> + Send;

    /// What the public flights that began within `within_m` metres of a
    /// place say about it: how many, of how many pilots, the newest one's
    /// day, how many in each month of the year, and the `best` best of
    /// them by points. See [`FlightsFrom`]. A flight that began exactly
    /// that far away is one of them.
    ///
    /// A flight that is private or unlisted is in none of it, not in a
    /// count either; nor is one whose recording never shows the glider
    /// flying, which took off nowhere.
    fn flights_from(
        &self,
        place: GeoPoint,
        within_m: f64,
        best: usize,
    ) -> impl Future<Output = Result<FlightsFrom, StoreError>> + Send;

    /// The standing sites within `within_m` metres of each of `places`,
    /// that far included: those of the sets of `sources`, and those of
    /// both layers of the server's own. One list for each place, in the
    /// order of `places`; the sites of a list in no particular order.
    /// A site that pilots removed is in none.
    ///
    /// One question for all the places: a page of flights asks it for
    /// where each of them took off.
    fn sites_near(
        &self,
        sources: &[SourceId],
        places: &[GeoPoint],
        within_m: f64,
    ) -> impl Future<Output = Result<Vec<Vec<Site>>, StoreError>> + Send;

    /// The places where pilots take off and no site stands: the cells of
    /// a fixed grid ([`CELLS_PER_DEGREE`]) that `area` touches, in which
    /// the public flights of at least `rule.min_pilots` different pilots
    /// began, and from the middle of whose take-offs no standing site is
    /// within `rule.clear_m` metres: no site of the sets of `sources`, and
    /// none of either layer of the server's own. The place with the most
    /// pilots comes first, then the one with the most flights; at most
    /// `limit` are returned. See [`TakeoffCandidate`].
    ///
    /// A candidate names no pilot and no flight, and its position is
    /// rounded. A flight that is private or unlisted counts for none, and
    /// a site that pilots removed is in the way of none.
    fn takeoff_candidates(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        rule: &CandidateRule,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<TakeoffCandidate>, StoreError>> + Send;

    /// What changed among an account's flights after change number `since`,
    /// oldest change first, at most `limit` changes. See [`FlightChanges`].
    ///
    /// A flight appears once, as it is now, however often it changed. The
    /// answer is taken at one moment: no change is missed between two calls
    /// when the second passes the largest change number of the first.
    fn flight_changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> impl Future<Output = Result<FlightChanges, StoreError>> + Send;

    /// What changed in what the public sees of flights after the number
    /// `since`, oldest change first, at most `limit` changes: the flights
    /// that became public or changed while public, each once and as it is
    /// now, and the flights that are public no more. See
    /// [`PublicFlightChanges`]. With `account_id`, the changes to that
    /// account's flights only.
    ///
    /// A flight is in one of the two lists and never in both. One that is
    /// private or unlisted is never among the flights, and one that was
    /// never public is in neither list. The answer is taken at one moment:
    /// no change is missed between two calls when the second passes the
    /// largest number of the first.
    ///
    /// A flight is withdrawn by whatever takes it from the public:
    /// [`Store::set_visibility`], [`Store::take_down_flight`],
    /// [`Store::suspend_publishing`], [`Store::delete_flight`] and
    /// [`Store::delete_account`]. A withdrawal outlives the flight and the
    /// account; of an account that is gone it is listed only when no
    /// account is asked for.
    fn public_flight_changes(
        &self,
        account_id: Option<&str>,
        since: i64,
        limit: usize,
    ) -> impl Future<Output = Result<PublicFlightChanges, StoreError>> + Send;

    /// Sets who may see a flight of an account, and says what became of
    /// it; see [`VisibilityChange`]. Setting what is already set changes
    /// nothing and counts as no change.
    ///
    /// Two things only whoever runs the server can lift stand in the way.
    /// A flight that was taken down ([`Store::take_down_flight`]) stays
    /// private. And an account whose publishing is suspended
    /// ([`Store::suspend_publishing`]) can make no flight public; it can
    /// still share one with those who have its link.
    fn set_visibility(
        &self,
        account_id: &str,
        id: &str,
        visibility: Visibility,
        now: Timestamp,
    ) -> impl Future<Output = Result<VisibilityChange, StoreError>> + Send;

    /// Sets who may see every flight of an account at once, all or
    /// nothing, by the rules of [`Store::set_visibility`], and says how
    /// the account's flights then stand; see [`AllVisibilityChange`].
    ///
    /// A flight that is taken down stays private and is counted as such.
    /// An account whose publishing is suspended makes no flight public: it
    /// is refused, and nothing changes. Each flight that changes takes the
    /// account's next change number, in the order of the flights'
    /// addresses, and what the public sees follows in the same count as
    /// for one flight: one that becomes public gets the moment `now` and a
    /// number, one that stops being public is withdrawn.
    fn set_all_visibility(
        &self,
        account_id: &str,
        visibility: Visibility,
        now: Timestamp,
    ) -> impl Future<Output = Result<AllVisibilityChange, StoreError>> + Send;

    /// Makes a flight private, whoever it belongs to and whatever its
    /// pilot had set, and keeps it so until [`Store::release_flight`].
    /// Returns the flight as it then is; `None` if there is no flight with
    /// this address.
    ///
    /// The flight counts as changed, so its pilot's devices hear of it.
    /// Taking down a flight that is taken down changes nothing and keeps
    /// the moment of the first call.
    fn take_down_flight(
        &self,
        id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Flight>, StoreError>> + Send;

    /// Lets a flight that was taken down go: its pilot may share it again.
    /// It stays private until its pilot does. Returns the flight as it
    /// then is; `None` if there is no flight with this address. Releasing a
    /// flight that is not taken down changes nothing.
    fn release_flight(
        &self,
        id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Flight>, StoreError>> + Send;

    /// Removes a flight of an account with its file, and remembers that it
    /// was deleted so that the account's other devices can be told. Returns
    /// whether the account had it.
    fn delete_flight(
        &self,
        account_id: &str,
        id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// One page of the flights whose derived values come from an analysis
    /// older than `version`, each with its file: oldest analysis first,
    /// then by address. `after` is the [`OutdatedFlight::key`] of the last
    /// flight of the page before, so that a flight which cannot be brought
    /// up to date is passed over instead of being handed out for ever.
    fn outdated_flights(
        &self,
        version: u32,
        after: Option<&(u32, String)>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<OutdatedFlight>, StoreError>> + Send;

    /// Replaces a flight's derived values. The flight counts as changed.
    /// Returns whether the flight exists.
    fn set_derived(
        &self,
        id: &str,
        derived: &Derived,
        now: Timestamp,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    // --- Live tracks ------------------------------------------------------

    /// Adds fixes to a live track, and begins the track if nothing is
    /// known under the batch's identifier and `room` allows it. Says what
    /// became of the batch; see [`LiveSaved`].
    ///
    /// - A track holds one fix for a second. A fix for a second it already
    ///   has changes nothing, so the same batch handed in twice changes
    ///   nothing the second time.
    /// - Fixes may arrive in any order. Each one the track takes in gets
    ///   the track's next number, by which [`Store::live_fixes`] hands
    ///   them out.
    /// - New fixes are taken oldest first while the track has room.
    /// - Who may see the track and whether it landed is what the batch
    ///   with the newest fix says. A batch whose newest fix is the track's
    ///   newest can only narrow who sees it and only say that it landed; a
    ///   batch of older fixes changes neither. Batches therefore give the
    ///   same track in whatever order they arrive.
    /// - An identifier is used once. One whose track was deleted or
    ///   pruned, or was taken out of use, takes nothing more. A track
    ///   whose newest fix is older than `room.expired_before` is removed
    ///   by this call as [`Store::prune_live_tracks`] would.
    /// - Two calls that race to begin the same track begin one.
    /// - A batch of an account whose publishing is suspended
    ///   ([`Store::suspend_publishing`]) is taken as one for those with the
    ///   link, whatever it says: the fixes are kept, and the track is in no
    ///   list.
    ///
    /// A batch without a fix, with fixes that are not in order or not at
    /// whole seconds, or for an account that does not exist is refused
    /// with [`StoreError::Invalid`].
    fn save_live_fixes(
        &self,
        batch: &LiveBatch,
        room: &LiveRoom,
    ) -> impl Future<Output = Result<LiveSaved, StoreError>> + Send;

    /// The live track with this identifier, whoever may see it and
    /// however long ago it ended. Deciding either is the caller's job.
    fn live_track(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<LiveTrack>, StoreError>> + Send;

    /// The fixes of a live track that arrived after the fix numbered
    /// `since`, in the order of arrival, at most `limit`. With `after`,
    /// only those whose time is later than it.
    fn live_fixes(
        &self,
        id: &str,
        since: i64,
        after: Option<Timestamp>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<LiveFixAt>, StoreError>> + Send;

    /// The newest `limit` fixes of a live track by their time, oldest
    /// first.
    fn live_tail(
        &self,
        id: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<LiveFix>, StoreError>> + Send;

    /// The public live tracks whose newest fix lies inside `area`, edges
    /// included, and is not older than `newest_since`, sorted by
    /// identifier. At most `limit` are returned.
    ///
    /// A track that only those with its link may see is never among them.
    fn live_tracks_in(
        &self,
        area: &BoundingBox,
        newest_since: Timestamp,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<LiveTrack>, StoreError>> + Send;

    /// Removes a live track of an account with its fixes, and takes its
    /// identifier out of use; see [`LiveRemoval`].
    ///
    /// When nothing is known under the identifier and `may_retire` is
    /// set, the identifier is taken out of use for the account all the
    /// same: fixes that were sent before the deletion and arrive after it
    /// must not begin a track.
    fn delete_live_track(
        &self,
        account_id: &str,
        id: &str,
        now: Timestamp,
        may_retire: bool,
    ) -> impl Future<Output = Result<LiveRemoval, StoreError>> + Send;

    /// Removes a live track with its fixes, whoever it belongs to, and
    /// takes its identifier out of use, as its pilot's own deletion would.
    /// Returns whether there was such a track. An identifier nothing is
    /// known under is left alone.
    fn remove_live_track(
        &self,
        id: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<bool, StoreError>> + Send;

    /// Removes every live track whose newest fix is older than
    /// `ended_before`, with its fixes, and returns how many there were.
    /// Their identifiers stay out of use.
    fn prune_live_tracks(
        &self,
        ended_before: Timestamp,
        now: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    // --- Groups -----------------------------------------------------------

    /// Makes a group whose keeper and only member is the account that
    /// makes it, unless the account is in as many groups as `room` lets it
    /// be; see [`GroupCreation`]. Of two calls that race for an account's
    /// last place, one gets it.
    ///
    /// An account that does not exist, an identifier that is in use and an
    /// invite another group has are refused with [`StoreError::Invalid`].
    fn create_group(
        &self,
        new: &NewGroup,
        room: &GroupRoom,
    ) -> impl Future<Output = Result<GroupCreation, StoreError>> + Send;

    /// Makes an account a member of the group whose invite code hashes to
    /// `invite`, at `now`; see [`GroupJoining`].
    ///
    /// Whether the code is good is decided together with whether there is
    /// room: of two calls that race for a group's last place, one gets it,
    /// and a call that races with a new code for the group either came in
    /// under the old one before it ended or did not come in.
    ///
    /// An account that does not exist is refused with
    /// [`StoreError::Invalid`].
    fn join_group(
        &self,
        account_id: &str,
        invite: &SecretHash,
        room: &GroupRoom,
        now: Timestamp,
    ) -> impl Future<Output = Result<GroupJoining, StoreError>> + Send;

    /// The groups an account is in, the one it has been in longest first,
    /// and by identifier among those it came into at the same moment.
    fn groups_of(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<Vec<Group>, StoreError>> + Send;

    /// A group with its members, as one of its members sees it. `None` if
    /// there is no such group or the account is not in it.
    ///
    /// The answer is taken at one moment: the keeper is among the members
    /// it lists.
    fn group(
        &self,
        id: &str,
        account_id: &str,
    ) -> impl Future<Output = Result<Option<GroupMembers>, StoreError>> + Send;

    /// Gives a group a new name. Only its keeper may.
    fn rename_group(
        &self,
        id: &str,
        keeper_id: &str,
        name: &str,
    ) -> impl Future<Output = Result<KeeperAct<GroupMembers>, StoreError>> + Send;

    /// Gives a group a new invite code, by its hash. The code before it
    /// lets nobody in from this call on. Only the keeper may.
    ///
    /// An invite another group has is refused with
    /// [`StoreError::Invalid`].
    fn renew_group_invite(
        &self,
        id: &str,
        keeper_id: &str,
        invite: &SecretHash,
    ) -> impl Future<Output = Result<KeeperAct<()>, StoreError>> + Send;

    /// Makes another member, named by handle, the keeper of a group. Only
    /// the keeper may. Naming the keeper changes nothing.
    fn hand_over_group(
        &self,
        id: &str,
        keeper_id: &str,
        member_handle: &str,
    ) -> impl Future<Output = Result<KeeperAct<GroupMembers>, StoreError>> + Send;

    /// Takes the member with `member_handle` out of a group; see
    /// [`MemberRemoval`]. A member may take themselves out, which is
    /// leaving; the keeper may take anyone out.
    ///
    /// - Whoever goes takes along what they put into the group: the plans
    ///   they posted there, with who was in, and their word on the plans
    ///   of others.
    /// - When the keeper leaves, the member who has been in the group
    ///   longest keeps it from then on, and by account identifier among
    ///   those who came in at the same moment. When the last member
    ///   leaves, the group goes: a group has a keeper, and the keeper is a
    ///   member, at every moment.
    /// - When the keeper takes someone else out, the group's invite code
    ///   ends with that: whoever was removed knew it. Nobody comes in
    ///   until [`Store::renew_group_invite`] makes a new one. A member who
    ///   leaves by themselves ends no code.
    fn remove_group_member(
        &self,
        id: &str,
        by_account_id: &str,
        member_handle: &str,
    ) -> impl Future<Output = Result<MemberRemoval, StoreError>> + Send;

    /// Removes a group with its members' places in it and all its plans.
    /// Only the keeper may.
    fn delete_group(
        &self,
        id: &str,
        keeper_id: &str,
    ) -> impl Future<Output = Result<KeeperAct<()>, StoreError>> + Send;

    // --- Plans ------------------------------------------------------------

    /// Stores a plan in a group its author is in, with the author as the
    /// first who is in, unless the group holds as many plans as `room`
    /// lets it; see [`PlanSaving`]. Of two calls that race for a group's
    /// last place, one gets it. Plans that `room` counts as gone take no
    /// place.
    ///
    /// An identifier that is in use is refused with
    /// [`StoreError::Invalid`].
    fn save_plan(
        &self,
        new: &NewPlan,
        room: &PlanRoom,
    ) -> impl Future<Output = Result<PlanSaving, StoreError>> + Send;

    /// Replaces what a plan says. Only the member who posted it may; see
    /// [`PlanChange`]. Who is in stays as it is. A plan for a day before
    /// `gone_before` is not there to change.
    fn change_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        details: &PlanDetails,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> impl Future<Output = Result<PlanChange, StoreError>> + Send;

    /// Removes a plan with who was in. The member who posted it may, and
    /// the keeper of the group may; see [`PlanRemoval`].
    fn delete_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        gone_before: NaiveDate,
    ) -> impl Future<Output = Result<PlanRemoval, StoreError>> + Send;

    /// Notes that a member of the group is in on a plan (`is_in`), since
    /// `now`, or takes that back; see [`PlanChange`], of which this never
    /// answers `NotAuthor`. Saying again what is already so changes
    /// nothing, and keeps the moment of the first time.
    fn set_plan_in(
        &self,
        group_id: &str,
        plan_id: &str,
        account_id: &str,
        is_in: bool,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> impl Future<Output = Result<PlanChange, StoreError>> + Send;

    /// The plans of a group for the days from `from` to `to`, both
    /// included, in the order of [`Plan::key`]; without `to`, for every
    /// day from `from` on. `None` if there is no such group or the account
    /// is not in it.
    fn plans_of_group(
        &self,
        group_id: &str,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
    ) -> impl Future<Output = Result<Option<Vec<Plan>>, StoreError>> + Send;

    /// The plans of every group an account is in, for the same days and in
    /// the same order as [`Store::plans_of_group`] gives them. At most
    /// `limit` are returned: the first in that order.
    ///
    /// A plan of a group the account is not in is never among them.
    fn plans_for(
        &self,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Plan>, StoreError>> + Send;

    /// Removes the plans for days before `before`, with who was in, and
    /// returns how many there were.
    fn prune_plans(
        &self,
        before: NaiveDate,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;

    // --- Groups and plans, for whoever runs the server ---------------------

    /// A group with its members, whoever is in it. `None` if there is no
    /// such group.
    ///
    /// This call and the five after it are for whoever runs the server,
    /// and for nothing that a pilot's request reaches. They name no
    /// account that asks, so they answer for every group: every other call
    /// about a group does name one, and tells an account that is not in
    /// the group nothing.
    fn any_group(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<GroupMembers>, StoreError>> + Send;

    /// The plans of a group for every day from `from` on, whoever is in
    /// the group, in the order of [`Plan::key`]. `None` if there is no
    /// such group. For whoever runs the server; see [`Store::any_group`].
    fn plans_of_any_group(
        &self,
        group_id: &str,
        from: NaiveDate,
    ) -> impl Future<Output = Result<Option<Vec<Plan>>, StoreError>> + Send;

    /// The plan with this identifier, whichever group it is in. `None` if
    /// there is none, or none any more: a plan for a day before
    /// `gone_before` is gone. For whoever runs the server; see
    /// [`Store::any_group`].
    fn any_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> impl Future<Output = Result<Option<Plan>, StoreError>> + Send;

    /// Gives a group a new name, whoever keeps it. Returns the group as it
    /// then is; `None` if there is no such group. For whoever runs the
    /// server; see [`Store::any_group`].
    ///
    /// Nothing else about the group changes: who keeps it, who is in it,
    /// its invite code and its plans are as they were, and nothing records
    /// who named it. Its keeper may name it again.
    fn rename_any_group(
        &self,
        id: &str,
        name: &str,
    ) -> impl Future<Output = Result<Option<GroupMembers>, StoreError>> + Send;

    /// Removes a plan with who was in, whoever posted it, as its author's
    /// own deletion would. Returns the plan as it was; `None` if there is
    /// none, or none any more by `gone_before`. For whoever runs the
    /// server; see [`Store::any_group`].
    fn remove_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> impl Future<Output = Result<Option<Plan>, StoreError>> + Send;

    /// Removes a group with its members' places in it and all its plans,
    /// whoever keeps it, as its keeper's own deletion would. Returns what
    /// went, counting the plans for days from `gone_before` on; `None` if
    /// there is no such group. For whoever runs the server; see
    /// [`Store::any_group`].
    fn remove_group(
        &self,
        id: &str,
        gone_before: NaiveDate,
    ) -> impl Future<Output = Result<Option<RemovedGroup>, StoreError>> + Send;

    // --- Reports ----------------------------------------------------------

    /// Stores a report, unless its reporter has an open report about the
    /// same subject already; see [`ReportSaved`]. Of two calls that race
    /// with the same report, one stores it.
    ///
    /// The store does not ask whether the subject exists or may be seen by
    /// the reporter: that is the caller's to know. It does ask for both
    /// accounts. A report about a group or a plan is handed the words it
    /// is to keep ([`ReportSubject`]) and looks nothing up.
    fn save_report(
        &self,
        new: &NewReport,
    ) -> impl Future<Output = Result<ReportSaved, StoreError>> + Send;

    /// One page of the reports, oldest first: by the moment they were
    /// made, and by identifier among reports of the same moment. `after`
    /// is the [`Report::key`] of the last report of the page before;
    /// `None` starts at the oldest. At most `limit` are returned.
    ///
    /// Each report names the accounts as they are now, and a group or a
    /// plan with the words it had when it was reported. This is for
    /// whoever runs the server: nothing here is ever shown to a pilot.
    fn reports(
        &self,
        listing: ReportListing,
        after: Option<&ReportKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Report>, StoreError>> + Send;

    /// Closes a report with a note; see [`ReportClosing`]. Closing does
    /// nothing to what was reported.
    fn close_report(
        &self,
        id: &str,
        note: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<ReportClosing, StoreError>> + Send;

    /// Removes the reports that were closed before `closed_before`, and
    /// returns how many there were. An open report is never removed by
    /// this.
    fn prune_reports(
        &self,
        closed_before: Timestamp,
    ) -> impl Future<Output = Result<u64, StoreError>> + Send;
}
