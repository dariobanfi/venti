//! A store for tests of what happens when the store is in trouble.
//!
//! Code that uses a [`Store`] has to cope with a database that is down or
//! slow. Neither is easy to arrange with a real one, so this crate ships a
//! store that does nothing else, for its users' tests.

use std::time::Duration;

use venti_domain::{Airspace, BoundingBox, Observation, SourceId, Station, StationId, Timestamp};

use crate::{
    Account, AccountCreation, AirspaceLayer, AirspaceSet, Coverage, Derived, Flight, FlightChanges,
    FlightKey, Listing, Month, NewAccount, NewFlight, NewInvite, OutdatedFlight, RunRecord,
    SecretHash, Session, SourceStatus, StationLatest, Store, StoreError, Visibility,
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

    async fn save_flight(&self, _: &NewFlight) -> Result<(Flight, bool), StoreError> {
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

    async fn flight_changes(&self, _: &str, _: i64, _: usize) -> Result<FlightChanges, StoreError> {
        self.fail().await
    }

    async fn set_visibility(
        &self,
        _: &str,
        _: &str,
        _: Visibility,
        _: Timestamp,
    ) -> Result<Option<Flight>, StoreError> {
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
}
