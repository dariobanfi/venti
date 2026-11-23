//! Open plans in the in-memory store.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use venti_domain::Timestamp;

use super::accounts::Accounts;
use crate::open_plan::check_open_details;
use crate::{
    GroupPilot, NewOpenPlan, OpenPlan, OpenPlanArea, OpenPlanChange, OpenPlanDetails,
    OpenPlanRemoval, OpenPlanRoom, OpenPlanSaving, StoreError,
};

#[derive(Debug, Default)]
pub(super) struct OpenPlans {
    /// By identifier.
    plans: BTreeMap<String, Entry>,
}

#[derive(Debug)]
struct Entry {
    author_id: String,
    details: OpenPlanDetails,
    created_at: Timestamp,
    changed_at: Timestamp,
    /// Since when each pilot is in, by account identifier.
    pilots_in: BTreeMap<String, Timestamp>,
}

/// A pilot as an open plan names them, with the account as it is now.
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
    fn read(&self, id: &str, accounts: &Accounts) -> OpenPlan {
        let mut since: Vec<(&Timestamp, &String)> = self
            .pilots_in
            .iter()
            .map(|(account, time)| (time, account))
            .collect();
        since.sort();
        OpenPlan {
            id: id.to_owned(),
            author: pilot(&self.author_id, accounts),
            details: self.details.clone(),
            created_at: self.created_at,
            changed_at: self.changed_at,
            pilots_in: since
                .into_iter()
                .map(|(_, account)| pilot(account, accounts))
                .collect(),
        }
    }
}

fn sorted(mut plans: Vec<OpenPlan>, limit: usize) -> Vec<OpenPlan> {
    plans.sort_by(|a, b| a.key().cmp(&b.key()));
    plans.truncate(limit);
    plans
}

impl OpenPlans {
    fn kept(&self, id: &str, gone_before: NaiveDate) -> Option<&Entry> {
        self.plans
            .get(id)
            .filter(|plan| plan.details.day >= gone_before)
    }

    pub(super) fn save(
        &mut self,
        new: &NewOpenPlan,
        room: &OpenPlanRoom,
        accounts: &Accounts,
    ) -> Result<OpenPlanSaving, StoreError> {
        check_open_details(&new.details)?;
        if accounts.by_id(&new.author_id).is_none() {
            return Err(StoreError::Invalid(
                "an open plan by an account that does not exist".to_owned(),
            ));
        }
        if self.plans.contains_key(&new.id) {
            return Err(StoreError::Invalid(
                "an open plan under an identifier that is in use".to_owned(),
            ));
        }
        let held = self
            .plans
            .values()
            .filter(|plan| plan.author_id == new.author_id && plan.details.day >= room.gone_before)
            .count();
        if held >= room.plans_per_account as usize {
            return Ok(OpenPlanSaving::TooMany);
        }
        let entry = Entry {
            author_id: new.author_id.clone(),
            details: new.details.clone(),
            created_at: new.created_at,
            changed_at: new.created_at,
            pilots_in: BTreeMap::from([(new.author_id.clone(), new.created_at)]),
        };
        let plan = entry.read(&new.id, accounts);
        self.plans.insert(new.id.clone(), entry);
        Ok(OpenPlanSaving::Stored(Box::new(plan)))
    }

    pub(super) fn change(
        &mut self,
        id: &str,
        by_account_id: &str,
        details: &OpenPlanDetails,
        (now, gone_before): (Timestamp, NaiveDate),
        accounts: &Accounts,
    ) -> Result<OpenPlanChange, StoreError> {
        check_open_details(details)?;
        if self.kept(id, gone_before).is_none() {
            return Ok(OpenPlanChange::NoSuchPlan);
        }
        let Some(plan) = self.plans.get_mut(id) else {
            return Ok(OpenPlanChange::NoSuchPlan);
        };
        if plan.author_id != by_account_id {
            return Ok(OpenPlanChange::NotAuthor);
        }
        plan.details = details.clone();
        plan.changed_at = now;
        Ok(OpenPlanChange::Now(Box::new(plan.read(id, accounts))))
    }

    pub(super) fn delete(
        &mut self,
        id: &str,
        by_account_id: &str,
        gone_before: NaiveDate,
    ) -> OpenPlanRemoval {
        match self.kept(id, gone_before) {
            None => OpenPlanRemoval::NoSuchPlan,
            Some(plan) if plan.author_id != by_account_id => OpenPlanRemoval::NotAuthor,
            Some(_) => {
                self.plans.remove(id);
                OpenPlanRemoval::Removed
            }
        }
    }

    pub(super) fn set_in(
        &mut self,
        id: &str,
        account_id: &str,
        (is_in, now): (bool, Timestamp),
        room: &OpenPlanRoom,
        accounts: &Accounts,
    ) -> OpenPlanChange {
        if self.kept(id, room.gone_before).is_none() || accounts.by_id(account_id).is_none() {
            return OpenPlanChange::NoSuchPlan;
        }
        let Some(plan) = self.plans.get_mut(id) else {
            return OpenPlanChange::NoSuchPlan;
        };
        if is_in {
            if !plan.pilots_in.contains_key(account_id) {
                if plan.pilots_in.len() >= room.pilots_per_plan as usize {
                    return OpenPlanChange::Full;
                }
                plan.pilots_in.insert(account_id.to_owned(), now);
            }
        } else {
            plan.pilots_in.remove(account_id);
        }
        OpenPlanChange::Now(Box::new(plan.read(id, accounts)))
    }

    pub(super) fn get(
        &self,
        id: &str,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> Option<OpenPlan> {
        self.kept(id, gone_before)
            .map(|plan| plan.read(id, accounts))
    }

    pub(super) fn near(
        &self,
        area: &OpenPlanArea,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<OpenPlan> {
        let plans = self
            .plans
            .iter()
            .map(|(id, plan)| plan.read(id, accounts))
            .filter(|plan| plan.is_near(area))
            .collect();
        sorted(plans, limit)
    }

    pub(super) fn of(
        &self,
        account_id: &str,
        from: NaiveDate,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<OpenPlan> {
        let plans = self
            .plans
            .iter()
            .filter(|(_, plan)| plan.details.day >= from)
            .filter(|(_, plan)| {
                plan.author_id == account_id || plan.pilots_in.contains_key(account_id)
            })
            .map(|(id, plan)| plan.read(id, accounts))
            .collect();
        sorted(plans, limit)
    }

    pub(super) fn remove(
        &mut self,
        id: &str,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> Option<OpenPlan> {
        let plan = self.get(id, gone_before, accounts)?;
        self.plans.remove(id);
        Some(plan)
    }

    pub(super) fn prune(&mut self, before: NaiveDate) -> u64 {
        let held = self.plans.len();
        self.plans.retain(|_, plan| plan.details.day >= before);
        (held - self.plans.len()) as u64
    }

    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.plans.retain(|_, plan| plan.author_id != account_id);
        for plan in self.plans.values_mut() {
            plan.pilots_in.remove(account_id);
        }
    }
}
