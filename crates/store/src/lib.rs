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
//! - what the data sources deliver: canonical [`Station`] and
//!   [`Observation`] records from `venti-domain`, each belonging to exactly
//!   one source, and the history of the sources' runs;
//! - what pilots entrust to the server: their [`Account`], the [`Session`]s
//!   of their devices, and their [`Flight`]s with the files they came from.
//!
//! It knows nothing about providers, about HTTP or about how a password or
//! a flight file is read: it is handed hashes and finished records.

mod account;
mod batch;
mod flight;
mod memory;
mod month;
mod postgres;
pub mod testing;

use std::future::Future;

use venti_domain::{BoundingBox, Observation, SourceId, Station, StationId, Timestamp};

pub use account::{
    Account, AccountCreation, NewAccount, NewInvite, SESSION_TOUCH_MINUTES, SecretHash, Session,
};
pub use flight::{
    DeletedFlight, Derived, Flight, FlightChanges, FlightKey, Listing, NewFlight, OutdatedFlight,
    Place, Score, ScoreKind, Visibility,
};
pub use memory::MemoryStore;
pub use month::Month;
pub use postgres::PgStore;

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
    /// Whether the run delivered observations without trouble.
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
    /// records of runs that started before it.
    ///
    /// Retention works in whole calendar months because that is the unit the
    /// database can drop cheaply.
    fn prune_before(
        &self,
        first_kept: Month,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

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
    /// its sessions, its flights and their files, and the record of the
    /// flights it deleted. Nothing is kept. Its handle is free again.
    fn delete_account(
        &self,
        account_id: &str,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

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

    // --- Flights ----------------------------------------------------------

    /// Stores a flight, private, unless its account already has a flight
    /// with the same identity. Returns the stored flight and whether this
    /// call created it.
    ///
    /// The flight that is already there is returned untouched: the first
    /// file received is the one that is kept. Two calls that race for the
    /// same flight create one. A flight the account deleted earlier can be
    /// stored again, and is then no longer listed as deleted.
    ///
    /// An account that does not exist is refused with
    /// [`StoreError::Invalid`].
    fn save_flight(
        &self,
        new: &NewFlight,
    ) -> impl Future<Output = Result<(Flight, bool), StoreError>> + Send;

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

    /// Sets who may see a flight of an account, and returns the flight as it
    /// then is. `None` if the account has no such flight. Setting what is
    /// already set changes nothing and counts as no change.
    fn set_visibility(
        &self,
        account_id: &str,
        id: &str,
        visibility: Visibility,
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
}
