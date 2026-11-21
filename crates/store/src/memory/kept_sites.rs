//! The sites pilots keep as their own, in the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use crate::flight::next_change;
use crate::kept_site::kept_from;
use crate::{ForgottenSite, KeepingSite, KeptRoom, KeptSite, KeptSiteChanges, NewKeptSite};

#[derive(Debug, Default)]
pub(super) struct KeptSites {
    /// By account identifier.
    accounts: BTreeMap<String, Of>,
}

/// The kept sites of one account.
#[derive(Debug, Default)]
struct Of {
    /// The largest change number used.
    latest: i64,
    /// The largest change number of a forgotten site no longer remembered.
    floor: i64,
    /// By site identifier.
    sites: BTreeMap<String, Row>,
}

#[derive(Debug)]
struct Row {
    source: String,
    change: i64,
    /// When it was kept, and the record. `None` once forgotten.
    kept: Option<(Timestamp, String)>,
}

impl Row {
    fn read(&self, site_id: &str) -> Option<KeptSite> {
        let (kept_at, record) = self.kept.clone()?;
        Some(KeptSite {
            site_id: site_id.to_owned(),
            source: self.source.clone(),
            kept_at,
            record,
            change: self.change,
        })
    }
}

impl KeptSites {
    /// Keeps a site for an account that exists.
    pub(super) fn keep(
        &mut self,
        account_id: &str,
        new: &NewKeptSite,
        now: Timestamp,
        room: KeptRoom,
    ) -> KeepingSite {
        let of = self.accounts.entry(account_id.to_owned()).or_default();
        let have = of
            .sites
            .get(&new.site_id)
            .and_then(|row| row.read(&new.site_id));
        let Some(kept_at) = kept_from(have.as_ref().map(|have| have.kept_at), new.kept_at, now)
        else {
            // `kept_from` leaves alone only what the account has.
            return have.map_or(KeepingSite::Full, KeepingSite::Kept);
        };
        let others = of
            .sites
            .iter()
            .filter(|(id, row)| row.kept.is_some() && **id != new.site_id)
            .count();
        if others >= room.sites {
            return KeepingSite::Full;
        }
        of.latest = next_change(of.latest, now);
        let row = Row {
            source: new.source.clone(),
            change: of.latest,
            kept: Some((kept_at, new.record.clone())),
        };
        let kept = row.read(&new.site_id);
        of.sites.insert(new.site_id.clone(), row);
        kept.map_or(KeepingSite::Full, KeepingSite::Kept)
    }

    pub(super) fn forget(
        &mut self,
        account_id: &str,
        site_id: &str,
        now: Timestamp,
        room: KeptRoom,
    ) -> bool {
        let Some(of) = self.accounts.get_mut(account_id) else {
            return false;
        };
        let Some(row) = of.sites.get_mut(site_id).filter(|row| row.kept.is_some()) else {
            return false;
        };
        of.latest = next_change(of.latest, now);
        row.kept = None;
        row.change = of.latest;

        // Of the forgotten, the newest are remembered.
        let mut forgotten: Vec<(i64, String)> = of
            .sites
            .iter()
            .filter(|(_, row)| row.kept.is_none())
            .map(|(id, row)| (row.change, id.clone()))
            .collect();
        forgotten.sort_unstable_by(|a, b| b.cmp(a));
        for (change, id) in forgotten.into_iter().skip(room.forgotten) {
            of.sites.remove(&id);
            of.floor = of.floor.max(change);
        }
        true
    }

    pub(super) fn changes(&self, account_id: &str, since: i64, limit: usize) -> KeptSiteChanges {
        let Some(of) = self.accounts.get(account_id) else {
            return KeptSiteChanges::default();
        };
        let mut rows: Vec<(&String, &Row)> = of
            .sites
            .iter()
            .filter(|(_, row)| row.change > since)
            .collect();
        rows.sort_by_key(|(_, row)| row.change);
        let more = rows.len() > limit;
        rows.truncate(limit);
        let mut changes = KeptSiteChanges {
            more,
            latest: of.latest,
            floor: of.floor,
            ..KeptSiteChanges::default()
        };
        for (id, row) in rows {
            match row.read(id) {
                Some(kept) => changes.kept.push(kept),
                None => changes.forgotten.push(ForgottenSite {
                    site_id: id.clone(),
                    change: row.change,
                }),
            }
        }
        changes
    }

    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.accounts.remove(account_id);
    }
}
