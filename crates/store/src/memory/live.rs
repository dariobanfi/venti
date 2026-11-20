//! Live tracks of the in-memory store.

use std::collections::{BTreeMap, BTreeSet};

use venti_domain::{BoundingBox, Timestamp};

use super::accounts::Accounts;
use crate::live::{Held, check, merge};
use crate::{
    LiveBatch, LiveFix, LiveFixAt, LiveKept, LiveRemoval, LiveRoom, LiveSaved, LiveTrack,
    LiveVisibility, StoreError,
};

#[derive(Debug, Default)]
pub(super) struct Live {
    /// By identifier: the order [`Store::live_tracks_in`](crate::Store::live_tracks_in)
    /// promises.
    tracks: BTreeMap<String, Entry>,
    /// The identifiers that take nothing more, each with the account that
    /// used it.
    retired: BTreeMap<String, String>,
}

#[derive(Debug)]
struct Entry {
    account_id: String,
    held: Held,
    /// In the order of arrival, which is the order of their numbers.
    fixes: Vec<LiveFixAt>,
    /// The seconds the track has a fix for.
    times: BTreeSet<Timestamp>,
}

impl Live {
    /// A stored track with its pilot as the account is now.
    fn read(id: &str, entry: &Entry, accounts: &Accounts) -> LiveTrack {
        let (handle, name) = accounts
            .by_id(&entry.account_id)
            .map_or(("", ""), |account| {
                (account.handle.as_str(), account.display_name.as_str())
            });
        entry.held.track(id, &entry.account_id, handle, name)
    }

    pub(super) fn save(
        &mut self,
        batch: &LiveBatch,
        room: &LiveRoom,
        accounts: &Accounts,
    ) -> Result<LiveSaved, StoreError> {
        check(batch, room)?;
        if accounts.by_id(&batch.account_id).is_none() {
            return Err(StoreError::Invalid(
                "live fixes for an account that does not exist".to_owned(),
            ));
        }
        // An account that may not publish shares by link, whatever it asks.
        let narrowed;
        let batch = if accounts.is_suspended(&batch.account_id) {
            narrowed = batch.for_those_with_the_link();
            &narrowed
        } else {
            batch
        };

        if let Some(entry) = self.tracks.get(&batch.id) {
            // A track that ended too long ago is gone, whether or not the
            // cleanup has come by.
            if entry.held.latest.time < room.expired_before {
                let account_id = entry.account_id.clone();
                self.tracks.remove(&batch.id);
                self.retired.insert(batch.id.clone(), account_id);
                return Ok(LiveSaved::Gone);
            }
            if entry.account_id != batch.account_id {
                return Ok(LiveSaved::Taken);
            }
        } else {
            if self.retired.contains_key(&batch.id) {
                return Ok(LiveSaved::Gone);
            }
            if !room.may_create {
                return Ok(LiveSaved::NotCreated);
            }
        }

        let existing = self.tracks.get(&batch.id);
        let known: BTreeSet<Timestamp> = batch
            .fixes
            .iter()
            .map(|fix| fix.time)
            .filter(|time| existing.is_some_and(|entry| entry.times.contains(time)))
            .collect();
        let merged = merge(existing.map(|entry| &entry.held), batch, &known, room);
        let created = existing.is_none();

        let entry = self
            .tracks
            .entry(batch.id.clone())
            .or_insert_with(|| Entry {
                account_id: batch.account_id.clone(),
                held: merged.held.clone(),
                fixes: Vec::new(),
                times: BTreeSet::new(),
            });
        entry.held = merged.held;
        entry
            .times
            .extend(merged.stored.iter().map(|at| at.fix.time));
        entry.fixes.extend(merged.stored.iter().copied());
        Ok(LiveSaved::Kept(Box::new(LiveKept {
            track: Self::read(&batch.id, entry, accounts),
            created,
            stored: u32::try_from(merged.stored.len()).unwrap_or(u32::MAX),
            known: merged.known,
            full: merged.full,
        })))
    }

    pub(super) fn get(&self, id: &str, accounts: &Accounts) -> Option<LiveTrack> {
        self.tracks
            .get(id)
            .map(|entry| Self::read(id, entry, accounts))
    }

    pub(super) fn fixes(
        &self,
        id: &str,
        since: i64,
        after: Option<Timestamp>,
        limit: usize,
    ) -> Vec<LiveFixAt> {
        let Some(entry) = self.tracks.get(id) else {
            return Vec::new();
        };
        entry
            .fixes
            .iter()
            .filter(|at| at.seq > since && after.is_none_or(|after| at.fix.time > after))
            .take(limit)
            .copied()
            .collect()
    }

    pub(super) fn tail(&self, id: &str, limit: usize) -> Vec<LiveFix> {
        let Some(entry) = self.tracks.get(id) else {
            return Vec::new();
        };
        let mut fixes: Vec<LiveFix> = entry.fixes.iter().map(|at| at.fix).collect();
        fixes.sort_by_key(|fix| fix.time);
        fixes.split_off(fixes.len().saturating_sub(limit))
    }

    pub(super) fn in_box(
        &self,
        area: &BoundingBox,
        newest_since: Timestamp,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<LiveTrack> {
        self.tracks
            .iter()
            .filter(|(_, entry)| {
                entry.held.visibility == LiveVisibility::Public
                    && entry.held.latest.time >= newest_since
                    && area.contains(entry.held.latest.position)
            })
            .take(limit)
            .map(|(id, entry)| Self::read(id, entry, accounts))
            .collect()
    }

    pub(super) fn delete(
        &mut self,
        account_id: &str,
        id: &str,
        may_retire: bool,
        accounts: &Accounts,
    ) -> Result<LiveRemoval, StoreError> {
        if id.contains('\0') || account_id.contains('\0') {
            return Err(StoreError::Invalid(
                "a live track's identifier that cannot be one".to_owned(),
            ));
        }
        if let Some(entry) = self.tracks.get(id) {
            if entry.account_id != account_id {
                return Ok(LiveRemoval::NotYours);
            }
            self.tracks.remove(id);
            self.retired.insert(id.to_owned(), account_id.to_owned());
            return Ok(LiveRemoval::Removed);
        }
        if let Some(used_by) = self.retired.get(id) {
            return Ok(if used_by == account_id {
                LiveRemoval::AlreadyRemoved
            } else {
                LiveRemoval::NotYours
            });
        }
        if !may_retire {
            return Ok(LiveRemoval::NotRetired);
        }
        if accounts.by_id(account_id).is_none() {
            return Err(StoreError::Invalid(
                "a live track's identifier taken out of use for no account".to_owned(),
            ));
        }
        self.retired.insert(id.to_owned(), account_id.to_owned());
        Ok(LiveRemoval::Retired)
    }

    /// Removes a track, whoever it belongs to, and takes its identifier out
    /// of use. Says whether there was such a track.
    pub(super) fn remove(&mut self, id: &str) -> bool {
        match self.tracks.remove(id) {
            Some(entry) => {
                self.retired.insert(id.to_owned(), entry.account_id);
                true
            }
            None => false,
        }
    }

    /// Makes every public track of an account one for those with its link,
    /// and returns how many there were.
    pub(super) fn narrow_all(&mut self, account_id: &str) -> u64 {
        let mut narrowed = 0;
        for entry in self.tracks.values_mut() {
            if entry.account_id == account_id && entry.held.visibility == LiveVisibility::Public {
                entry.held.visibility = LiveVisibility::Link;
                narrowed += 1;
            }
        }
        narrowed
    }

    pub(super) fn prune(&mut self, ended_before: Timestamp) -> u64 {
        let ended: Vec<String> = self
            .tracks
            .iter()
            .filter(|(_, entry)| entry.held.latest.time < ended_before)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ended {
            if let Some(entry) = self.tracks.remove(id) {
                self.retired.insert(id.clone(), entry.account_id);
            }
        }
        ended.len() as u64
    }

    /// Removes everything that belongs to an account.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.tracks
            .retain(|_, entry| entry.account_id != account_id);
        self.retired.retain(|_, used_by| used_by != account_id);
    }
}
