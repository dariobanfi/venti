//! Flights of the in-memory store.

use std::collections::BTreeMap;

use venti_domain::{BoundingBox, GeoPoint, Timestamp};

use super::accounts::Accounts;
use crate::flight::{
    Kept, PublicEffect, Room, Saved, cut, cut_public, holds_text, may_be_set, next_public_change,
    ranked_order, score_order, sought, uploaded_visibility,
};
use crate::takeoff::{Cell, Tally, cell_of, cells_of};
use crate::{
    AllFlightsSet, AllVisibilityChange, DeletedFlight, Derived, Flight, FlightChanges, FlightKey,
    FlightsFrom, Flown, FoundPilot, GliderClass, Listing, Month, NewFlight, OutdatedFlight, Place,
    PublicFlightChanges, PublicFlights, PublicOrder, Ranked, ScoreKey, StoreError, Visibility,
    VisibilityChange, WithdrawnFlight,
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
    /// What the public sees of flights: the count of its changes, and the
    /// flights that are public no more.
    public: Public,
}

/// The count of changes to what the public sees of flights, and the
/// flights that were public and are not.
#[derive(Debug, Default)]
struct Public {
    /// The largest number in use.
    latest: i64,
    /// By the flight's address, each with the account it was of while
    /// that account is there.
    withdrawn: BTreeMap<String, (Option<String>, WithdrawnFlight)>,
}

impl Public {
    /// Takes the next number.
    fn take(&mut self, now: Option<Timestamp>) -> i64 {
        self.latest = next_public_change(self.latest, now);
        self.latest
    }

    /// Makes a flight public from `now` on.
    fn publish(&mut self, flight: &mut Flight, now: Timestamp) {
        flight.visibility = Visibility::Public;
        flight.published_at = Some(now);
        flight.public_change = Some(self.take(Some(now)));
        self.withdrawn.remove(&flight.id);
    }

    /// Takes a public flight from the public and makes it visible as
    /// `visibility` says. `now` is missing when the flight goes with its
    /// account, which is then not kept either.
    fn withdraw(&mut self, flight: &mut Flight, visibility: Visibility, now: Option<Timestamp>) {
        flight.visibility = visibility;
        flight.published_at = None;
        flight.public_change = None;
        let withdrawn = WithdrawnFlight {
            id: flight.id.clone(),
            change: self.take(now),
            withdrawn_at: now,
        };
        let account = now.map(|_| flight.account_id.clone());
        self.withdrawn
            .insert(flight.id.clone(), (account, withdrawn));
    }

    /// Sets who may see a flight at `now`, and keeps what the public sees
    /// in step.
    fn set(&mut self, flight: &mut Flight, visibility: Visibility, now: Timestamp) {
        match PublicEffect::of(flight.visibility, visibility) {
            PublicEffect::Published => self.publish(flight, now),
            PublicEffect::Withdrawn => self.withdraw(flight, visibility, Some(now)),
            PublicEffect::Nothing => flight.visibility = visibility,
        }
    }
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
            flight.pilot_country.clone_from(&account.country);
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
        // Deleting a flight and uploading it again does not undo that it
        // was taken down.
        let taken_down_at = self.deleted_taken_down.remove(&place);
        let mut entry = Entry {
            flight: Flight {
                id: new.id.clone(),
                account_id: new.account_id.clone(),
                pilot_handle: String::new(),
                pilot_name: String::new(),
                pilot_country: None,
                identity: new.identity.clone(),
                visibility: Visibility::Private,
                uploaded_at: new.uploaded_at,
                file_bytes: new.file_bytes,
                derived: new.derived.clone(),
                change,
                taken_down_at,
                published_at: None,
                public_change: None,
                glider_class: new.glider_class,
            },
            file: new.file.clone(),
        };
        let visibility = uploaded_visibility(
            accounts.new_flights_public(&new.account_id),
            accounts.is_suspended(&new.account_id),
            taken_down_at.is_some(),
        );
        if visibility == Visibility::Public {
            self.public.publish(&mut entry.flight, new.uploaded_at);
        }
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

    /// Whether there is a public flight with this address.
    pub(super) fn is_public(&self, id: &str) -> bool {
        self.flights
            .get(id)
            .is_some_and(|entry| entry.flight.visibility == Visibility::Public)
    }

    /// The account a flight is of.
    pub(super) fn owner_of(&self, id: &str) -> Option<&str> {
        self.flights
            .get(id)
            .map(|entry| entry.flight.account_id.as_str())
    }

    /// Whether an account has a public flight.
    pub(super) fn has_public(&self, account_id: &str) -> bool {
        self.flights.values().any(|entry| {
            entry.flight.account_id == account_id && entry.flight.visibility == Visibility::Public
        })
    }

    /// The addresses of an account's flights.
    pub(super) fn ids_of(&self, account_id: &str) -> Vec<String> {
        self.flights
            .values()
            .filter(|entry| entry.flight.account_id == account_id)
            .map(|entry| entry.flight.id.clone())
            .collect()
    }

    /// The public flights of some accounts, newest first.
    pub(super) fn public_of(
        &self,
        pilots: &std::collections::BTreeSet<String>,
        after: Option<&FlightKey>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Flight> {
        let mut flights: Vec<Flight> = self
            .flights
            .values()
            .filter(|entry| entry.flight.visibility == Visibility::Public)
            .filter(|entry| pilots.contains(&entry.flight.account_id))
            .filter(|entry| after.is_none_or(|after| &entry.flight.key() < after))
            .map(|entry| Self::read(entry, accounts))
            .collect();
        flights.sort_by_key(|flight| std::cmp::Reverse(flight.key()));
        flights.truncate(limit);
        flights
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

    pub(super) fn ranking(
        &self,
        which: &PublicFlights,
        counted: usize,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Ranked> {
        // Each pilot's scored public flights among those asked for.
        let mut by_pilot: BTreeMap<String, Vec<(ScoreKey, Flight)>> = BTreeMap::new();
        for entry in self.flights.values() {
            if entry.flight.visibility != Visibility::Public || !which.holds(&entry.flight) {
                continue;
            }
            let flight = Self::read(entry, accounts);
            if let Some(key) = flight.score_key() {
                by_pilot
                    .entry(flight.account_id.clone())
                    .or_default()
                    .push((key, flight));
            }
        }
        let mut ranked: Vec<Ranked> = by_pilot
            .into_values()
            .filter_map(|mut scored| {
                let all = u32::try_from(scored.len()).unwrap_or(u32::MAX);
                scored.sort_by(|a, b| score_order(&b.0, &a.0));
                scored.truncate(counted);
                Ranked::of(scored.into_iter().map(|(_, flight)| flight).collect(), all)
            })
            .collect();
        ranked.sort_by(ranked_order);
        ranked.truncate(limit);
        ranked
    }

    pub(super) fn scored_months(&self, which: &PublicFlights) -> Vec<(Month, u32)> {
        let mut months: BTreeMap<Month, u32> = BTreeMap::new();
        for entry in self.flights.values() {
            let flight = &entry.flight;
            if flight.visibility == Visibility::Public
                && which.holds(flight)
                && flight.derived.score.is_some()
            {
                *months.entry(Month::of(flight.derived.start)).or_default() += 1;
            }
        }
        months.into_iter().collect()
    }

    pub(super) fn pilots_named(
        &self,
        text: &str,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<FoundPilot> {
        let sought = sought(text);
        let mut public: BTreeMap<&str, u32> = BTreeMap::new();
        for entry in self.flights.values() {
            if entry.flight.visibility == Visibility::Public {
                *public.entry(entry.flight.account_id.as_str()).or_default() += 1;
            }
        }
        let mut found: Vec<(bool, FoundPilot)> = public
            .into_iter()
            .filter_map(|(account_id, flights)| {
                let account = accounts.by_id(account_id)?;
                let begins = holds_text(&sought, &account.handle, &account.display_name)?;
                Some((
                    begins,
                    FoundPilot {
                        handle: account.handle.clone(),
                        name: account.display_name.clone(),
                        flights,
                    },
                ))
            })
            .collect();
        found.sort_by(|(a_begins, a), (b_begins, b)| {
            b_begins
                .cmp(a_begins)
                .then(b.flights.cmp(&a.flights))
                .then_with(|| a.handle.cmp(&b.handle))
        });
        found.truncate(limit);
        found.into_iter().map(|(_, pilot)| pilot).collect()
    }

    pub(super) fn flown_by(&self, account_id: &str, listing: Listing) -> Vec<Flown> {
        let mut flights: Vec<&Flight> = self
            .flights
            .values()
            .map(|entry| &entry.flight)
            .filter(|flight| {
                flight.account_id == account_id
                    && (listing == Listing::All || flight.visibility == Visibility::Public)
            })
            .collect();
        flights.sort_by_key(|flight| std::cmp::Reverse(flight.key()));
        flights.into_iter().map(Flown::of).collect()
    }

    /// The public flights that took off somewhere, each with where. A
    /// flight that is not public is not among them: everything that
    /// counts take-offs begins here.
    fn public_takeoffs(&self) -> impl Iterator<Item = (&Entry, &Place)> {
        self.flights
            .values()
            .filter(|entry| entry.flight.visibility == Visibility::Public)
            .filter_map(|entry| Some((entry, entry.flight.derived.takeoff.as_ref()?)))
    }

    pub(super) fn began_at(
        &self,
        place: GeoPoint,
        within_m: f64,
        best: usize,
        accounts: &Accounts,
    ) -> FlightsFrom {
        let mut tally = Tally::default();
        let mut scored: Vec<(ScoreKey, &Entry)> = Vec::new();
        for (entry, takeoff) in self.public_takeoffs() {
            if place.distance_m(takeoff.position) > within_m {
                continue;
            }
            let flight = &entry.flight;
            tally.add(&flight.account_id, flight.derived.start, takeoff);
            if let Some(key) = flight.score_key() {
                scored.push((key, entry));
            }
        }
        scored.sort_by(|a, b| score_order(&b.0, &a.0));
        let best = scored
            .into_iter()
            .take(best)
            .map(|(_, entry)| Self::read(entry, accounts))
            .collect();
        tally.at_a_place(best)
    }

    /// The public flights that began in the cells a box touches, added
    /// up cell by cell.
    pub(super) fn takeoff_cells(&self, area: &BoundingBox) -> Vec<Cell> {
        let [south, north, west, east] = cells_of(area);
        let mut cells: BTreeMap<(i32, i32), Tally> = BTreeMap::new();
        for (entry, takeoff) in self.public_takeoffs() {
            let at = cell_of(takeoff.position);
            if (south..=north).contains(&at.0) && (west..=east).contains(&at.1) {
                let flight = &entry.flight;
                cells
                    .entry(at)
                    .or_default()
                    .add(&flight.account_id, flight.derived.start, takeoff);
            }
        }
        cells
            .into_iter()
            .filter_map(|(at, tally)| tally.of_cell(at))
            .collect()
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

    pub(super) fn public_changes(
        &self,
        account_id: Option<&str>,
        since: i64,
        limit: usize,
        accounts: &Accounts,
    ) -> PublicFlightChanges {
        let of_account = |owner: Option<&str>| account_id.is_none_or(|id| owner == Some(id));
        let mut flights: Vec<Flight> = self
            .flights
            .values()
            .filter(|entry| entry.flight.visibility == Visibility::Public)
            .filter(|entry| {
                entry
                    .flight
                    .public_change
                    .is_some_and(|change| change > since)
            })
            .filter(|entry| of_account(Some(&entry.flight.account_id)))
            .map(|entry| Self::read(entry, accounts))
            .collect();
        flights.sort_by_key(|flight| flight.public_change);
        let mut withdrawn: Vec<WithdrawnFlight> = self
            .public
            .withdrawn
            .values()
            .filter(|(owner, withdrawn)| withdrawn.change > since && of_account(owner.as_deref()))
            .map(|(_, withdrawn)| withdrawn.clone())
            .collect();
        withdrawn.sort_by_key(|withdrawn| withdrawn.change);

        let more = cut_public(&mut flights, &mut withdrawn, limit);
        PublicFlightChanges {
            flights,
            withdrawn,
            more,
            latest: self.public.latest,
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
            self.public.set(&mut entry.flight, visibility, now);
            entry.flight.change = change;
        }
        VisibilityChange::Set(Box::new(Self::read(entry, accounts)))
    }

    /// Sets who may see every flight of an account, each that changes as a
    /// change of its own in the order of the flights' addresses.
    pub(super) fn set_all_visibility(
        &mut self,
        account_id: &str,
        visibility: Visibility,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> AllVisibilityChange {
        if !accounts.has(account_id) {
            return AllVisibilityChange::NoSuchAccount;
        }
        if may_be_set(false, accounts.is_suspended(account_id), visibility).is_err() {
            return AllVisibilityChange::PublishingSuspended;
        }
        let mut set = AllFlightsSet::default();
        // The map is sorted by address.
        for entry in self.flights.values_mut() {
            let flight = &mut entry.flight;
            if flight.account_id != account_id {
                continue;
            }
            let taken_down = flight.taken_down_at.is_some();
            if !set.count(flight.visibility, taken_down, visibility) {
                continue;
            }
            let Some(change) = accounts.next_flight_change(account_id, now) else {
                break;
            };
            self.public.set(flight, visibility, now);
            flight.change = change;
        }
        AllVisibilityChange::Set(set)
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
            self.public.set(&mut entry.flight, Visibility::Private, now);
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
            self.public
                .set(&mut entry.flight, Visibility::Unlisted, now);
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
        let mut entry = self.flights.remove(id).expect("the flight was found above");
        // A public flight that is deleted is withdrawn; what it is
        // withdrawn to no longer matters.
        self.public.set(&mut entry.flight, Visibility::Private, now);
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
    ///
    /// Its public flights are withdrawn, in the order of their addresses,
    /// and every withdrawal of the account forgets whose it was.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        // The map is sorted by address.
        for entry in self.flights.values_mut() {
            let public = entry.flight.account_id == account_id
                && entry.flight.visibility == Visibility::Public;
            if public {
                self.public
                    .withdraw(&mut entry.flight, Visibility::Private, None);
            }
        }
        for (owner, _) in self.public.withdrawn.values_mut() {
            if owner.as_deref() == Some(account_id) {
                *owner = None;
            }
        }
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

    pub(super) fn set_glider_class(
        &mut self,
        account_id: &str,
        id: &str,
        class: Option<GliderClass>,
        now: Timestamp,
        accounts: &mut Accounts,
    ) -> Option<Flight> {
        let entry = self
            .flights
            .get_mut(id)
            .filter(|entry| entry.flight.account_id == account_id)?;
        let change = accounts.next_flight_change(account_id, now)?;
        entry.flight.glider_class = class;
        entry.flight.change = change;
        // What the public sees of a public flight has changed with it.
        if entry.flight.visibility == Visibility::Public {
            entry.flight.public_change = Some(self.public.take(Some(now)));
        }
        let entry = &self.flights[id];
        Some(Self::read(entry, accounts))
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
        // What the public sees of a public flight has changed with it.
        if entry.flight.visibility == Visibility::Public {
            entry.flight.public_change = Some(self.public.take(Some(now)));
        }
        true
    }
}
