//! Sites that pilots make, in the in-memory store.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use venti_domain::{BoundingBox, Site, SiteId, SourceId, Timestamp};

use super::accounts::Accounts;
use crate::pilot_site::{
    Decision, check_confirmation, check_new, check_revising, decide, nearest_within, takes_a_place,
};
use crate::{
    NewPilotSite, NewSiteConfirmation, PilotSite, PilotSiteChanges, RevisionKind, SiteAct,
    SiteConfirmation, SiteConfirmed, SiteConfirming, SitePilot, SiteRevising, SiteRevision,
    SiteVia, SiteWrite, StoreError,
};

#[derive(Debug, Default)]
pub(super) struct PilotSites {
    /// By identifier: the order [`crate::Store::pilot_sites_in`] promises.
    /// No entry is ever removed.
    sites: BTreeMap<SiteId, Entry>,
    /// The largest change number the layer has used.
    latest: i64,
}

#[derive(Debug)]
struct Entry {
    /// Since when the site is removed, if it is.
    removed_at: Option<Timestamp>,
    taken_down: bool,
    /// The change number its latest revision took.
    change: i64,
    /// Every revision, the first one first. Never empty.
    revisions: Vec<Stored>,
    /// By account: one confirmation each.
    confirmations: BTreeMap<String, Confirmed>,
}

/// One account's confirmation of a site.
#[derive(Debug)]
struct Confirmed {
    day: NaiveDate,
    note: Option<String>,
    with_flight: bool,
    at: Timestamp,
}

/// A revision as it is kept: with its author's account, not the pilot.
#[derive(Debug)]
struct Stored {
    author_id: Option<String>,
    at: Timestamp,
    via: SiteVia,
    kind: RevisionKind,
    comment: Option<String>,
    record: Site,
}

/// The pilot an account is now; `None` once the account is gone, which is
/// what a deleted account leaves of its name on a site.
fn pilot(account_id: Option<&String>, accounts: &Accounts) -> Option<SitePilot> {
    let account = accounts.by_id(account_id?)?;
    Some(SitePilot {
        account_id: account.id.clone(),
        handle: account.handle.clone(),
        display_name: account.display_name.clone(),
    })
}

fn number(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

impl Entry {
    fn read(&self, accounts: &Accounts) -> Option<PilotSite> {
        let (first, last) = (self.revisions.first()?, self.revisions.last()?);
        Some(PilotSite {
            site: last.record.clone(),
            revision: number(self.revisions.len() - 1),
            created_at: first.at,
            created_by: pilot(first.author_id.as_ref(), accounts),
            changed_at: last.at,
            changed_by: pilot(last.author_id.as_ref(), accounts),
            removed_at: self.removed_at,
            taken_down: self.taken_down,
            change: self.change,
            confirmed: SiteConfirmed {
                pilots: u32::try_from(self.confirmations.len()).unwrap_or(u32::MAX),
                latest_day: self.confirmations.values().map(|kept| kept.day).max(),
            },
        })
    }
}

impl PilotSites {
    /// The standing site that is too near to `record`, if one is.
    fn too_close(&self, record: &Site, apart_m: f64) -> Option<SiteId> {
        let standing = self
            .sites
            .values()
            .filter(|entry| entry.removed_at.is_none())
            .filter_map(|entry| entry.revisions.last())
            .map(|stored| &stored.record);
        nearest_within(record, standing, apart_m)
    }

    pub(super) fn add(
        &mut self,
        new: &NewPilotSite,
        apart_m: f64,
        accounts: &Accounts,
    ) -> Result<SiteWrite, StoreError> {
        check_new(new)?;
        if accounts.by_id(&new.author_id).is_none() {
            return Ok(SiteWrite::NoSuchAccount);
        }
        if self.sites.contains_key(&new.site.id) {
            return Err(StoreError::Invalid(
                "a site under an identifier that is in use".to_owned(),
            ));
        }
        if let Some(other) = self.too_close(&new.site, apart_m) {
            return Ok(SiteWrite::TooClose(other));
        }
        self.latest += 1;
        let entry = Entry {
            removed_at: None,
            taken_down: false,
            change: self.latest,
            confirmations: BTreeMap::new(),
            revisions: vec![Stored {
                author_id: Some(new.author_id.clone()),
                at: new.at,
                via: new.via.clone(),
                kind: RevisionKind::Created,
                comment: new.comment.clone(),
                record: new.site.clone(),
            }],
        };
        let written = entry.read(accounts);
        self.sites.insert(new.site.id.clone(), entry);
        written
            .map(|site| SiteWrite::Written(Box::new(site)))
            .ok_or_else(|| StoreError::Failure("a site without a revision".to_owned()))
    }

    pub(super) fn revise(
        &mut self,
        revising: &SiteRevising,
        apart_m: f64,
        accounts: &Accounts,
    ) -> Result<SiteWrite, StoreError> {
        check_revising(revising)?;
        if revising
            .author_id
            .as_ref()
            .is_some_and(|id| accounts.by_id(id).is_none())
        {
            return Ok(SiteWrite::NoSuchAccount);
        }
        let Some(current) = self
            .sites
            .get(&revising.id)
            .and_then(|entry| entry.read(accounts))
        else {
            return Ok(SiteWrite::NoSuchSite);
        };
        let restored = match revising.act {
            SiteAct::Restore(number) => self.sites.get(&revising.id).and_then(|entry| {
                let index = usize::try_from(number).ok()?.checked_sub(1)?;
                entry
                    .revisions
                    .get(index)
                    .map(|stored| stored.record.clone())
            }),
            SiteAct::Change(_) | SiteAct::Remove { .. } => None,
        };
        let writing = match decide(&current, revising, restored.as_ref()) {
            Decision::Answer(answer) => return Ok(answer),
            Decision::Write(writing) => *writing,
        };
        if takes_a_place(&current, &writing)
            && let Some(other) = self.too_close(&writing.record, apart_m)
        {
            return Ok(SiteWrite::TooClose(other));
        }
        self.latest += 1;
        let change = self.latest;
        let Some(entry) = self.sites.get_mut(&revising.id) else {
            return Ok(SiteWrite::NoSuchSite);
        };
        entry.removed_at = writing.removed.then_some(revising.at);
        entry.taken_down = writing.taken_down;
        entry.change = change;
        entry.revisions.push(Stored {
            author_id: revising.author_id.clone(),
            at: revising.at,
            via: revising.via.clone(),
            kind: writing.kind,
            comment: revising.comment.clone(),
            record: writing.record,
        });
        entry
            .read(accounts)
            .map(|site| SiteWrite::Written(Box::new(site)))
            .ok_or_else(|| StoreError::Failure("a site without a revision".to_owned()))
    }

    pub(super) fn in_box(
        &self,
        layer: &SourceId,
        area: &BoundingBox,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<PilotSite> {
        self.sites
            .iter()
            .filter(|(id, entry)| id.source() == layer && entry.removed_at.is_none())
            .map(|(_, entry)| entry)
            .filter_map(|entry| entry.read(accounts))
            .filter(|site| area.contains(site.site.position))
            .take(limit)
            .collect()
    }

    pub(super) fn one(&self, id: &SiteId, accounts: &Accounts) -> Option<PilotSite> {
        self.sites.get(id)?.read(accounts)
    }

    pub(super) fn revisions(
        &self,
        id: &SiteId,
        after: u32,
        limit: usize,
        accounts: &Accounts,
    ) -> Option<(PilotSite, Vec<SiteRevision>)> {
        let entry = self.sites.get(id)?;
        let revisions = entry
            .revisions
            .iter()
            .enumerate()
            .skip(usize::try_from(after).unwrap_or(usize::MAX))
            .take(limit)
            .map(|(index, stored)| SiteRevision {
                number: number(index),
                at: stored.at,
                author: pilot(stored.author_id.as_ref(), accounts),
                via: stored.via.clone(),
                kind: stored.kind,
                comment: stored.comment.clone(),
                record: stored.record.clone(),
            })
            .collect();
        Some((entry.read(accounts)?, revisions))
    }

    /// Every revision of the sites named, by site and number.
    pub(super) fn revisions_of(&self, ids: &[SiteId], accounts: &Accounts) -> Vec<SiteRevision> {
        let ids: std::collections::BTreeSet<&SiteId> = ids.iter().collect();
        let mut all = Vec::new();
        for id in ids {
            let Some(entry) = self.sites.get(id) else {
                continue;
            };
            for (index, stored) in entry.revisions.iter().enumerate() {
                all.push(SiteRevision {
                    number: number(index),
                    at: stored.at,
                    author: pilot(stored.author_id.as_ref(), accounts),
                    via: stored.via.clone(),
                    kind: stored.kind,
                    comment: stored.comment.clone(),
                    record: stored.record.clone(),
                });
            }
        }
        all
    }

    pub(super) fn changes(
        &self,
        layer: &SourceId,
        since: i64,
        limit: usize,
        accounts: &Accounts,
    ) -> PilotSiteChanges {
        let mut changed: Vec<PilotSite> = self
            .sites
            .iter()
            .filter(|(id, entry)| id.source() == layer && entry.change > since)
            .map(|(_, entry)| entry)
            .filter_map(|entry| entry.read(accounts))
            .collect();
        changed.sort_by_key(|site| site.change);
        let more = changed.len() > limit;
        changed.truncate(limit);
        PilotSiteChanges {
            sites: changed,
            more,
            latest: self.latest,
        }
    }

    pub(super) fn confirm(
        &mut self,
        new: &NewSiteConfirmation,
        accounts: &Accounts,
    ) -> Result<SiteConfirming, StoreError> {
        check_confirmation(new)?;
        if accounts.by_id(&new.account_id).is_none() {
            return Ok(SiteConfirming::NoSuchAccount);
        }
        let Some(entry) = self.sites.get_mut(&new.site) else {
            return Ok(SiteConfirming::NoSuchSite);
        };
        if entry.removed_at.is_some() {
            return Ok(SiteConfirming::Removed);
        }
        self.latest += 1;
        entry.change = self.latest;
        entry.confirmations.insert(
            new.account_id.clone(),
            Confirmed {
                day: new.day,
                note: new.note.clone(),
                with_flight: new.with_flight,
                at: new.at,
            },
        );
        entry
            .read(accounts)
            .map(|site| SiteConfirming::Confirmed(Box::new(site)))
            .ok_or_else(|| StoreError::Failure("a site without a revision".to_owned()))
    }

    pub(super) fn withdraw(
        &mut self,
        site: &SiteId,
        account_id: &str,
        accounts: &Accounts,
    ) -> Option<(PilotSite, bool)> {
        let entry = self.sites.get_mut(site)?;
        let was = entry.confirmations.remove(account_id).is_some();
        if was {
            self.latest += 1;
            entry.change = self.latest;
        }
        Some((entry.read(accounts)?, was))
    }

    pub(super) fn confirmations(
        &self,
        site: &SiteId,
        limit: usize,
        accounts: &Accounts,
    ) -> Option<(PilotSite, Vec<SiteConfirmation>)> {
        let entry = self.sites.get(site)?;
        let mut all: Vec<SiteConfirmation> = entry
            .confirmations
            .iter()
            .filter_map(|(account_id, kept)| {
                Some(SiteConfirmation {
                    pilot: pilot(Some(account_id), accounts)?,
                    day: kept.day,
                    note: kept.note.clone(),
                    with_flight: kept.with_flight,
                    at: kept.at,
                })
            })
            .collect();
        all.sort_by(|a, b| {
            b.day
                .cmp(&a.day)
                .then_with(|| a.pilot.handle.cmp(&b.pilot.handle))
        });
        all.truncate(limit);
        Some((entry.read(accounts)?, all))
    }

    /// Every revision an account wrote, by site and number.
    pub(super) fn revisions_by(&self, account_id: &str, accounts: &Accounts) -> Vec<SiteRevision> {
        // An account that is gone wrote nothing: its revisions are found
        // without their author from then on.
        if accounts.by_id(account_id).is_none() {
            return Vec::new();
        }
        let mut all = Vec::new();
        for entry in self.sites.values() {
            for (index, stored) in entry.revisions.iter().enumerate() {
                if stored.author_id.as_deref() != Some(account_id) {
                    continue;
                }
                all.push(SiteRevision {
                    number: number(index),
                    at: stored.at,
                    author: pilot(stored.author_id.as_ref(), accounts),
                    via: stored.via.clone(),
                    kind: stored.kind,
                    comment: stored.comment.clone(),
                    record: stored.record.clone(),
                });
            }
        }
        all
    }

    /// Every confirmation an account has, by site.
    pub(super) fn confirmations_by(
        &self,
        account_id: &str,
        accounts: &Accounts,
    ) -> Vec<(SiteId, SiteConfirmation)> {
        let Some(pilot) = pilot(Some(&account_id.to_owned()), accounts) else {
            return Vec::new();
        };
        self.sites
            .iter()
            .filter_map(|(id, entry)| {
                let kept = entry.confirmations.get(account_id)?;
                let confirmation = SiteConfirmation {
                    pilot: pilot.clone(),
                    day: kept.day,
                    note: kept.note.clone(),
                    with_flight: kept.with_flight,
                    at: kept.at,
                };
                Some((id.clone(), confirmation))
            })
            .collect()
    }

    /// Takes what an account said of sites with it. Its sites and
    /// revisions stay: they are found without their author from now on.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        for entry in self.sites.values_mut() {
            entry.confirmations.remove(account_id);
        }
    }
}
