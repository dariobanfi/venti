//! Phones, the settings for what pilots nearby do, positions and requests
//! for help, in the in-memory store.

use std::collections::BTreeMap;

use venti_domain::{GeoPoint, Timestamp};

use super::accounts::Accounts;
use crate::{
    AddingDevice, GroupPilot, HelpAnswering, HelpCreation, HelpReach, HelpRequest, NearbySettings,
    NewHelpRequest, PhoneDevice,
};

#[derive(Debug, Default)]
pub(super) struct Phones {
    /// By platform and token.
    devices: BTreeMap<(String, String), PhoneDevice>,
    settings: BTreeMap<String, NearbySettings>,
    /// By account: where, and when it was told.
    positions: BTreeMap<String, (GeoPoint, Timestamp)>,
    /// By identifier.
    requests: BTreeMap<String, Entry>,
}

#[derive(Debug)]
struct Entry {
    new: NewHelpRequest,
    closed_at: Option<Timestamp>,
    asked: Vec<String>,
    /// Who can help, with when they said so.
    helpers: Vec<(String, Timestamp)>,
}

fn pilot(account_id: &str, accounts: &Accounts) -> GroupPilot {
    let (handle, name) = accounts.by_id(account_id).map_or(("", ""), |account| {
        (account.handle.as_str(), account.display_name.as_str())
    });
    GroupPilot {
        account_id: account_id.to_owned(),
        handle: handle.to_owned(),
        display_name: name.to_owned(),
    }
}

impl Entry {
    fn read(&self, accounts: &Accounts) -> HelpRequest {
        HelpRequest {
            id: self.new.id.clone(),
            pilot: pilot(&self.new.account_id, accounts),
            position: self.new.position,
            note: self.new.note.clone(),
            created_at: self.new.created_at,
            ends_at: self.new.ends_at,
            closed_at: self.closed_at,
            asked: self.asked.clone(),
            helpers: self
                .helpers
                .iter()
                .map(|(account, _)| pilot(account, accounts))
                .collect(),
        }
    }

    fn stands(&self, now: Timestamp) -> bool {
        self.closed_at.is_none() && self.new.ends_at > now
    }
}

fn newest_first(mut requests: Vec<HelpRequest>) -> Vec<HelpRequest> {
    requests.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
    requests
}

impl Phones {
    pub(super) fn add_device(
        &mut self,
        device: &PhoneDevice,
        most: usize,
        accounts: &Accounts,
    ) -> AddingDevice {
        if !accounts.has_session(&device.session_id) || !accounts.has(&device.account_id) {
            return AddingDevice::NoSession;
        }
        let key = (device.platform.clone(), device.token.clone());
        let others = self
            .devices
            .iter()
            .filter(|(held, kept)| {
                kept.account_id == device.account_id
                    && **held != key
                    && accounts.has_session(&kept.session_id)
            })
            .count();
        if others >= most {
            return AddingDevice::Full;
        }
        self.devices.insert(key, device.clone());
        AddingDevice::Added
    }

    pub(super) fn remove_device(&mut self, account_id: &str, platform: &str, token: &str) -> bool {
        let key = (platform.to_owned(), token.to_owned());
        let theirs = self
            .devices
            .get(&key)
            .is_some_and(|held| held.account_id == account_id);
        theirs && self.devices.remove(&key).is_some()
    }

    pub(super) fn drop_device(&mut self, platform: &str, token: &str) -> bool {
        self.devices
            .remove(&(platform.to_owned(), token.to_owned()))
            .is_some()
    }

    pub(super) fn devices_of(
        &mut self,
        account_ids: &[String],
        accounts: &Accounts,
    ) -> Vec<PhoneDevice> {
        // A session that is gone takes its phones along, as the database's
        // reference does.
        self.devices
            .retain(|_, device| accounts.has_session(&device.session_id));
        let mut theirs: Vec<PhoneDevice> = self
            .devices
            .values()
            .filter(|device| account_ids.contains(&device.account_id))
            .cloned()
            .collect();
        theirs.sort_by(|a, b| {
            (&a.account_id, a.created_at, &a.platform, &a.token).cmp(&(
                &b.account_id,
                b.created_at,
                &b.platform,
                &b.token,
            ))
        });
        theirs
    }

    pub(super) fn settings(&self, account_id: &str) -> NearbySettings {
        self.settings.get(account_id).copied().unwrap_or_default()
    }

    pub(super) fn set_settings(&mut self, account_id: &str, settings: &NearbySettings) {
        if !settings.may_be_asked_for_help {
            self.positions.remove(account_id);
        }
        self.settings.insert(account_id.to_owned(), *settings);
    }

    pub(super) fn keep_position(
        &mut self,
        account_id: &str,
        position: GeoPoint,
        now: Timestamp,
    ) -> bool {
        if !self.settings(account_id).may_be_asked_for_help {
            return false;
        }
        self.positions
            .insert(account_id.to_owned(), (position, now));
        true
    }

    pub(super) fn forget_position(&mut self, account_id: &str) {
        self.positions.remove(account_id);
    }

    pub(super) fn position_of(&self, account_id: &str, since: Timestamp) -> Option<GeoPoint> {
        self.positions
            .get(account_id)
            .filter(|(_, told)| *told >= since)
            .map(|(position, _)| *position)
    }

    pub(super) fn open_plan_watchers(
        &self,
        position: GeoPoint,
        except: &str,
        most: usize,
    ) -> Vec<String> {
        self.settings
            .iter()
            .filter(|(account, settings)| {
                account.as_str() != except
                    && settings
                        .open_plans_near
                        .is_some_and(|circle| circle.holds(position))
            })
            .map(|(account, _)| account.clone())
            .take(most)
            .collect()
    }

    pub(super) fn create_help_request(
        &mut self,
        new: &NewHelpRequest,
        reach: &HelpReach,
        accounts: &Accounts,
    ) -> HelpCreation {
        if !accounts.has(&new.account_id) {
            return HelpCreation::NoAccount;
        }
        let open = self
            .requests
            .values()
            .any(|entry| entry.new.account_id == new.account_id && entry.stands(new.created_at));
        if open {
            return HelpCreation::StillOpen;
        }
        let radius_m = f64::from(reach.radius_km) * 1000.0;
        let mut near: Vec<(f64, &String)> = self
            .positions
            .iter()
            .filter(|(account, (_, told))| {
                **account != new.account_id
                    && *told >= reach.positions_since
                    && self.settings(account).may_be_asked_for_help
            })
            .map(|(account, (position, _))| (position.distance_m(new.position), account))
            .filter(|(distance, _)| *distance <= radius_m)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        let asked: Vec<String> = near
            .into_iter()
            .take(reach.most)
            .map(|(_, account)| account.clone())
            .collect();
        let entry = Entry {
            new: new.clone(),
            closed_at: None,
            asked,
            helpers: Vec::new(),
        };
        let request = entry.read(accounts);
        self.requests.insert(new.id.clone(), entry);
        HelpCreation::Created(Box::new(request))
    }

    pub(super) fn help_request(&self, id: &str, accounts: &Accounts) -> Option<HelpRequest> {
        self.requests.get(id).map(|entry| entry.read(accounts))
    }

    pub(super) fn answer(
        &mut self,
        id: &str,
        account_id: &str,
        can_help: bool,
        now: Timestamp,
        accounts: &Accounts,
    ) -> HelpAnswering {
        let Some(entry) = self.requests.get_mut(id) else {
            return HelpAnswering::NotAsked;
        };
        if !entry.stands(now) || !entry.asked.iter().any(|asked| asked == account_id) {
            return HelpAnswering::NotAsked;
        }
        let had = entry.helpers.iter().any(|(helper, _)| helper == account_id);
        let changed = had != can_help;
        if can_help && !had {
            entry.helpers.push((account_id.to_owned(), now));
        } else if !can_help {
            entry.helpers.retain(|(helper, _)| helper != account_id);
        }
        HelpAnswering::Answered(Box::new(entry.read(accounts)), changed)
    }

    pub(super) fn close(
        &mut self,
        id: &str,
        account_id: &str,
        now: Timestamp,
        accounts: &Accounts,
    ) -> Option<HelpRequest> {
        let entry = self
            .requests
            .get_mut(id)
            .filter(|entry| entry.new.account_id == account_id && entry.stands(now))?;
        entry.closed_at = Some(now);
        Some(entry.read(accounts))
    }

    pub(super) fn requests_of(
        &self,
        account_id: &str,
        now: Timestamp,
        accounts: &Accounts,
    ) -> Vec<HelpRequest> {
        newest_first(
            self.requests
                .values()
                .filter(|entry| entry.new.account_id == account_id && entry.stands(now))
                .map(|entry| entry.read(accounts))
                .collect(),
        )
    }

    pub(super) fn requests_asked(
        &self,
        account_id: &str,
        now: Timestamp,
        accounts: &Accounts,
    ) -> Vec<HelpRequest> {
        newest_first(
            self.requests
                .values()
                .filter(|entry| {
                    entry.stands(now) && entry.asked.iter().any(|asked| asked == account_id)
                })
                .map(|entry| entry.read(accounts))
                .collect(),
        )
    }

    pub(super) fn prune(&mut self, positions_before: Timestamp, requests_before: Timestamp) -> u64 {
        let before = self.positions.len() + self.requests.len();
        self.positions
            .retain(|_, (_, told)| *told >= positions_before);
        self.requests.retain(|_, entry| {
            entry
                .closed_at
                .unwrap_or(entry.new.ends_at)
                .min(entry.new.ends_at)
                >= requests_before
        });
        let after = self.positions.len() + self.requests.len();
        u64::try_from(before - after).unwrap_or(u64::MAX)
    }

    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.devices
            .retain(|_, device| device.account_id != account_id);
        self.settings.remove(account_id);
        self.positions.remove(account_id);
        self.requests
            .retain(|_, entry| entry.new.account_id != account_id);
        for entry in self.requests.values_mut() {
            entry.asked.retain(|asked| asked != account_id);
            entry.helpers.retain(|(helper, _)| helper != account_id);
        }
    }
}
