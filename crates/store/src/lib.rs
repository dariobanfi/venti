//! Where the server keeps what the data sources deliver.
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
//! The store holds canonical records only: [`Station`] and [`Observation`]
//! from `venti-domain`, each belonging to exactly one source. It knows
//! nothing about providers or about HTTP.

mod batch;
mod memory;
mod month;
mod postgres;

use std::future::Future;

use venti_domain::{BoundingBox, Observation, SourceId, Station, StationId, Timestamp};

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

/// Persistence for stations, observations and the sources' run history.
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
}
