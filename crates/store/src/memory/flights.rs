//! Flights of the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use super::accounts::Accounts;
use crate::flight::{Kept, Room, Saved, cut, may_be_set, score_order};
use crate::{
    DeletedFlight, Derived, Flight, FlightChanges, FlightKey, Listing, NewFlight, OutdatedFlight,
    PublicFlights, PublicOrder, ScoreKey, StoreError, Visibility, VisibilityChange,
};

#[derive(Debug, Default)]
pub(super) struct Flights {
    /// By address.
    flights: BTreeMap<String, Entry>,
    /// Deleted flights by account and address.
    deleted: BTreeMap<(String, String), DeletedFlight>,
    /// Of the deleted flights, the ones that were taken down when they
    /// were deleted, with the moment they had been taken down at: a
    /// flight that comes back under its address comes back taken down.
    deleted_taken_down: BTreeMap<(String, String), Timestamp>,
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
        room: &Room,
        accounts: &mut Accounts,
    ) -> Result<Saved, StoreError> {
        if accounts.by_id(&new.account_id).is_none() {
            return Err(StoreError::Invalid(
                "a flight for an account that does not exist".to_owned(),
            ));
        }
        let existing = self.flights.values().find(|entry| {
            entry.flight.account_id == new.account_id && entry.flight.identity == new.identity
        });
        if let Some(entry) = existing {
            return Ok(Saved::AlreadyThere(Self::read(entry, accounts)));
        }
        let kept = self.kept(&new.account_id);
        if !room.has_room_for(&kept, new.file_bytes) {
            return Ok(Saved::NoRoom(kept));
        }

        let change = accounts
            .next_flight_change(&new.account_id, new.uploaded_at)
            .expect("the account was found above");
        let place = (new.account_id.clone(), new.id.clone());
        self.deleted.remove(&place);
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
                // Deleting a flight and uploading it again does not undo
                // that it was taken down.
                taken_down_at: self.deleted_taken_down.remove(&place),
            },
            file: new.file.clone(),
        };
        let flight = Self::read(&entry, accounts);
        self.flights.insert(new.id.clone(), entry);
        Ok(Saved::Created(flight))
    }

    /// What an account keeps.
    fn kept(&self, account_id: &str) -> Kept {
        let files = self
            .flights
            .values()
            .filter(|entry| entry.flight.account_id == account_id)
            .map(|entry| u64::from(entry.flight.file_bytes));
        Kept {
            flights: files.clone().count() as u64,
            file_bytes: files.sum(),
        }
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

    pub(super) fn public(
        &self,
        which: &PublicFlights,
        order: &PublicOrder,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Flight> {
        let mut flights: Vec<Flight> = self
            .flights
            .values()
            .filter(|entry| entry.flight.visibility == Visibility::Public)
            .filter(|entry| which.holds(&entry.flight))
            .map(|entry| Self::read(entry, accounts))
            .collect();
        match order {
            PublicOrder::Newest(after) => {
                flights.retain(|flight| after.as_ref().is_none_or(|after| &flight.key() < after));
                flights.sort_by_key(|flight| std::cmp::Reverse(flight.key()));
            }
            PublicOrder::Best(after) => {
                let mut scored: Vec<(ScoreKey, Flight)> = flights
                    .into_iter()
                    .filter_map(|flight| Some((flight.score_key()?, flight)))
                    .filter(|(key, _)| {
                        after
                            .as_ref()
                            .is_none_or(|after| score_order(key, after).is_lt())
                    })
                    .collect();
                scored.sort_by(|a, b| score_order(&b.0, &a.0));
                flights = scored.into_iter().map(|(_, flight)| flight).collect();
            }
        }
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
    ) -> VisibilityChange {
        let owned = self
            .flights
            .get_mut(id)
            .filter(|entry| entry.flight.account_id == account_id);
        let Some(entry) = owned else {
            return VisibilityChange::NoSuchFlight;
        };
        let taken_down = entry.flight.taken_down_at.is_some();
        if let Err(refused) = may_be_set(taken_down, accounts.is_suspended(account_id), visibility)
        {
            return refused;
        }
        if entry.flight.visibility != visibility {
            let Some(change) = accounts.next_flight_change(account_id, now) else {
                return VisibilityChange::NoSuchFlight;
            };
            entry.flight.visibility = visibility;
            entry.flight.change = change;
        }
        VisibilityChange::Set(Box::new(Self::read(entry, accounts)))
    }

    /// Makes a flight private and keeps it so, whoever it belongs to.
    pub(super) fn take_down(
        &mut self,
        id: &str,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> Option<Flight> {
        let entry = self.flights.get_mut(id)?;
        if entry.flight.taken_down_at.is_none() {
            let change = accounts.next_flight_change(&entry.flight.account_id, now)?;
            entry.flight.visibility = Visibility::Private;
            entry.flight.taken_down_at = Some(now);
            entry.flight.change = change;
        }
        Some(Self::read(entry, accounts))
    }

    /// Lets a flight that was taken down go. It stays private.
    pub(super) fn release(
        &mut self,
        id: &str,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> Option<Flight> {
        let entry = self.flights.get_mut(id)?;
        if entry.flight.taken_down_at.is_some() {
            let change = accounts.next_flight_change(&entry.flight.account_id, now)?;
            entry.flight.taken_down_at = None;
            entry.flight.change = change;
        }
        Some(Self::read(entry, accounts))
    }

    /// Makes every public flight of an account unlisted, each as a change
    /// of its own in the order of the flights' addresses, and returns how
    /// many there were.
    pub(super) fn unlist_public(
        &mut self,
        account_id: &str,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> u64 {
        let mut unlisted = 0;
        // The map is sorted by address.
        for entry in self.flights.values_mut() {
            let public = entry.flight.account_id == account_id
                && entry.flight.visibility == Visibility::Public;
            if !public {
                continue;
            }
            let Some(change) = accounts.next_flight_change(account_id, now) else {
                break;
            };
            entry.flight.visibility = Visibility::Unlisted;
            entry.flight.change = change;
            unlisted += 1;
        }
        unlisted
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
        let place = (account_id.to_owned(), id.to_owned());
        if let Some(taken_down_at) = entry.flight.taken_down_at {
            self.deleted_taken_down.insert(place.clone(), taken_down_at);
        }
        self.deleted.insert(place, deleted);
        true
    }

    /// Removes everything that belongs to an account.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.flights
            .retain(|_, entry| entry.flight.account_id != account_id);
        self.deleted.retain(|(account, _), _| account != account_id);
        self.deleted_taken_down
            .retain(|(account, _), _| account != account_id);
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
