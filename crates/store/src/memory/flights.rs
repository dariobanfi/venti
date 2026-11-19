//! Flights of the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use super::accounts::Accounts;
use crate::flight::cut;
use crate::{
    DeletedFlight, Derived, Flight, FlightChanges, FlightKey, Listing, NewFlight, OutdatedFlight,
    StoreError, Visibility,
};

#[derive(Debug, Default)]
pub(super) struct Flights {
    /// By address.
    flights: BTreeMap<String, Entry>,
    /// Deleted flights by account and address.
    deleted: BTreeMap<(String, String), DeletedFlight>,
}

#[derive(Debug)]
struct Entry {
    /// The pilot's handle and name in here are filled in when the flight is
    /// read, from the account as it is then.
    flight: Flight,
    file: Vec<u8>,
}

impl Flights {
    /// A stored flight with its pilot as the account is now.
    fn read(entry: &Entry, accounts: &Accounts) -> Flight {
        let mut flight = entry.flight.clone();
        if let Some(account) = accounts.by_id(&flight.account_id) {
            flight.pilot_handle.clone_from(&account.handle);
            flight.pilot_name.clone_from(&account.display_name);
        }
        flight
    }

    pub(super) fn save(
        &mut self,
        new: &NewFlight,
        accounts: &mut Accounts,
    ) -> Result<(Flight, bool), StoreError> {
        if accounts.by_id(&new.account_id).is_none() {
            return Err(StoreError::Invalid(
                "a flight for an account that does not exist".to_owned(),
            ));
        }
        let existing = self.flights.values().find(|entry| {
            entry.flight.account_id == new.account_id && entry.flight.identity == new.identity
        });
        if let Some(entry) = existing {
            return Ok((Self::read(entry, accounts), false));
        }

        let change = accounts
            .next_flight_change(&new.account_id, new.uploaded_at)
            .expect("the account was found above");
        let entry = Entry {
            flight: Flight {
                id: new.id.clone(),
                account_id: new.account_id.clone(),
                pilot_handle: String::new(),
                pilot_name: String::new(),
                identity: new.identity.clone(),
                visibility: Visibility::Private,
                uploaded_at: new.uploaded_at,
                file_bytes: new.file_bytes,
                derived: new.derived.clone(),
                change,
            },
            file: new.file.clone(),
        };
        let flight = Self::read(&entry, accounts);
        self.deleted
            .remove(&(new.account_id.clone(), new.id.clone()));
        self.flights.insert(new.id.clone(), entry);
        Ok((flight, true))
    }

    pub(super) fn get(&self, id: &str, accounts: &Accounts) -> Option<Flight> {
        self.flights
            .get(id)
            .map(|entry| Self::read(entry, accounts))
    }

    pub(super) fn file(&self, id: &str) -> Option<Vec<u8>> {
        self.flights.get(id).map(|entry| entry.file.clone())
    }

    pub(super) fn of_account(
        &self,
        account_id: &str,
        listing: Listing,
        after: Option<&FlightKey>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Flight> {
        let mut flights: Vec<Flight> = self
            .flights
            .values()
            .filter(|entry| entry.flight.account_id == account_id)
            .filter(|entry| {
                listing == Listing::All || entry.flight.visibility == Visibility::Public
            })
            .filter(|entry| after.is_none_or(|after| &entry.flight.key() < after))
            .map(|entry| Self::read(entry, accounts))
            .collect();
        flights.sort_by_key(|flight| std::cmp::Reverse(flight.key()));
        flights.truncate(limit);
        flights
    }

    pub(super) fn changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
        accounts: &Accounts,
    ) -> FlightChanges {
        let mut flights: Vec<Flight> = self
            .flights
            .values()
            .filter(|entry| entry.flight.account_id == account_id && entry.flight.change > since)
            .map(|entry| Self::read(entry, accounts))
            .collect();
        flights.sort_by_key(|flight| flight.change);
        let mut deleted: Vec<DeletedFlight> = self
            .deleted
            .iter()
            .filter(|((account, _), deleted)| account == account_id && deleted.change > since)
            .map(|(_, deleted)| deleted.clone())
            .collect();
        deleted.sort_by_key(|deleted| deleted.change);

        let more = cut(&mut flights, &mut deleted, limit);
        FlightChanges {
            flights,
            deleted,
            more,
            latest: accounts.flight_change(account_id).unwrap_or(0),
        }
    }

    pub(super) fn set_visibility(
        &mut self,
        account_id: &str,
        id: &str,
        visibility: Visibility,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> Option<Flight> {
        let entry = self
            .flights
            .get_mut(id)
            .filter(|entry| entry.flight.account_id == account_id)?;
        if entry.flight.visibility != visibility {
            entry.flight.visibility = visibility;
            entry.flight.change = accounts.next_flight_change(account_id, now)?;
        }
        Some(Self::read(entry, accounts))
    }

    pub(super) fn delete(
        &mut self,
        account_id: &str,
        id: &str,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> bool {
        let owned = self
            .flights
            .get(id)
            .is_some_and(|entry| entry.flight.account_id == account_id);
        if !owned {
            return false;
        }
        let Some(change) = accounts.next_flight_change(account_id, now) else {
            return false;
        };
        let entry = self.flights.remove(id).expect("the flight was found above");
        let deleted = DeletedFlight {
            id: id.to_owned(),
            identity: entry.flight.identity,
            change,
        };
        self.deleted
            .insert((account_id.to_owned(), id.to_owned()), deleted);
        true
    }

    /// Removes everything that belongs to an account.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.flights
            .retain(|_, entry| entry.flight.account_id != account_id);
        self.deleted.retain(|(account, _), _| account != account_id);
    }

    pub(super) fn outdated(
        &self,
        version: u32,
        after: Option<&(u32, String)>,
        limit: usize,
    ) -> Vec<OutdatedFlight> {
        let mut outdated: Vec<OutdatedFlight> = self
            .flights
            .iter()
            .filter(|(_, entry)| entry.flight.derived.version < version)
            .map(|(id, entry)| OutdatedFlight {
                id: id.clone(),
                version: entry.flight.derived.version,
                file: entry.file.clone(),
            })
            .filter(|flight| after.is_none_or(|after| &flight.key() > after))
            .collect();
        outdated.sort_by_key(OutdatedFlight::key);
        outdated.truncate(limit);
        outdated
    }

    pub(super) fn set_derived(
        &mut self,
        id: &str,
        derived: &Derived,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> bool {
        let Some(entry) = self.flights.get_mut(id) else {
            return false;
        };
        let Some(change) = accounts.next_flight_change(&entry.flight.account_id, now) else {
            return false;
        };
        entry.flight.derived = derived.clone();
        entry.flight.change = change;
        true
    }
}
