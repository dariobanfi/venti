//! Groups and their plans in the in-memory store.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use venti_domain::Timestamp;

use super::accounts::Accounts;
use crate::group::{check_details, check_group, check_name, check_plan};
use crate::{
    Group, GroupCreation, GroupJoining, GroupMembers, GroupPilot, GroupRoom, KeeperAct,
    MemberRemoval, NewGroup, NewPlan, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom,
    PlanSaving, RemovedGroup, SecretHash, StoreError,
};

#[derive(Debug, Default)]
pub(super) struct Groups {
    /// By identifier.
    groups: BTreeMap<String, Entry>,
    /// By identifier.
    plans: BTreeMap<String, PlanEntry>,
}

#[derive(Debug)]
struct Entry {
    name: String,
    created_at: Timestamp,
    /// Always one of `members`.
    keeper_id: String,
    /// The hash of the code that lets a pilot in, while one does.
    invite: Option<SecretHash>,
    /// When each member came in, by account identifier.
    members: BTreeMap<String, Timestamp>,
}

#[derive(Debug)]
struct PlanEntry {
    group_id: String,
    author_id: String,
    details: PlanDetails,
    created_at: Timestamp,
    changed_at: Timestamp,
    /// Since when each pilot is in, by account identifier.
    pilots_in: BTreeMap<String, Timestamp>,
}

fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

/// A member as the others see them, with the account as it is now.
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

/// Account identifiers in the order of the moments that go with them, and
/// by identifier among those of the same moment.
fn in_order(since: &BTreeMap<String, Timestamp>) -> Vec<&String> {
    let mut ordered: Vec<(&Timestamp, &String)> =
        since.iter().map(|(id, time)| (time, id)).collect();
    ordered.sort();
    ordered.into_iter().map(|(_, id)| id).collect()
}

impl Entry {
    fn read(&self, id: &str, accounts: &Accounts) -> Group {
        Group {
            id: id.to_owned(),
            name: self.name.clone(),
            created_at: self.created_at,
            keeper: pilot(&self.keeper_id, accounts),
            members: count(self.members.len()),
        }
    }

    fn read_with_members(&self, id: &str, accounts: &Accounts) -> GroupMembers {
        GroupMembers {
            group: self.read(id, accounts),
            members: in_order(&self.members)
                .into_iter()
                .map(|member| pilot(member, accounts))
                .collect(),
        }
    }

    /// The account identifier of the member with this handle.
    fn member_with(&self, handle: &str, accounts: &Accounts) -> Option<String> {
        self.members
            .keys()
            .find(|member| {
                accounts
                    .by_id(member)
                    .is_some_and(|account| account.handle == handle)
            })
            .cloned()
    }
}

impl PlanEntry {
    fn read(&self, id: &str, accounts: &Accounts) -> Plan {
        Plan {
            id: id.to_owned(),
            group_id: self.group_id.clone(),
            author: pilot(&self.author_id, accounts),
            details: self.details.clone(),
            created_at: self.created_at,
            changed_at: self.changed_at,
            pilots_in: in_order(&self.pilots_in)
                .into_iter()
                .map(|account| pilot(account, accounts))
                .collect(),
        }
    }

    fn is_for(&self, from: NaiveDate, to: Option<NaiveDate>) -> bool {
        self.details.day >= from && to.is_none_or(|to| self.details.day <= to)
    }
}

fn sorted(mut plans: Vec<Plan>) -> Vec<Plan> {
    plans.sort_by(|a, b| a.key().cmp(&b.key()));
    plans
}

impl Groups {
    /// How many groups an account is in.
    fn groups_with(&self, account_id: &str) -> usize {
        self.groups
            .values()
            .filter(|entry| entry.members.contains_key(account_id))
            .count()
    }

    /// The group, if the account is in it.
    fn as_member(&self, id: &str, account_id: &str) -> Option<&Entry> {
        self.groups
            .get(id)
            .filter(|entry| entry.members.contains_key(account_id))
    }

    fn as_member_mut(&mut self, id: &str, account_id: &str) -> Option<&mut Entry> {
        self.groups
            .get_mut(id)
            .filter(|entry| entry.members.contains_key(account_id))
    }

    /// The plan, if it belongs to the group and its day is still kept.
    fn kept_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Option<&PlanEntry> {
        self.plans
            .get(plan_id)
            .filter(|plan| plan.group_id == group_id && plan.details.day >= gone_before)
    }

    pub(super) fn create(
        &mut self,
        new: &NewGroup,
        room: &GroupRoom,
        accounts: &Accounts,
    ) -> Result<GroupCreation, StoreError> {
        check_group(new)?;
        if accounts.by_id(&new.keeper_id).is_none() {
            return Err(StoreError::Invalid(
                "a group for an account that does not exist".to_owned(),
            ));
        }
        if self.groups_with(&new.keeper_id) >= room.groups_per_account as usize {
            return Ok(GroupCreation::TooManyGroups);
        }
        let in_use = self.groups.contains_key(&new.id)
            || self
                .groups
                .values()
                .any(|entry| entry.invite == Some(new.invite));
        if in_use {
            return Err(StoreError::Invalid(
                "a group under an identifier or an invite that is in use".to_owned(),
            ));
        }
        let entry = Entry {
            name: new.name.clone(),
            created_at: new.created_at,
            keeper_id: new.keeper_id.clone(),
            invite: Some(new.invite),
            members: BTreeMap::from([(new.keeper_id.clone(), new.created_at)]),
        };
        let group = entry.read(&new.id, accounts);
        self.groups.insert(new.id.clone(), entry);
        Ok(GroupCreation::Created(group))
    }

    pub(super) fn join(
        &mut self,
        account_id: &str,
        invite: &SecretHash,
        room: &GroupRoom,
        now: Timestamp,
        accounts: &Accounts,
    ) -> Result<GroupJoining, StoreError> {
        if accounts.by_id(account_id).is_none() {
            return Err(StoreError::Invalid(
                "a place in a group for an account that does not exist".to_owned(),
            ));
        }
        let in_groups = self.groups_with(account_id);
        let found = self
            .groups
            .iter_mut()
            .find(|(_, entry)| entry.invite.as_ref() == Some(invite));
        let Some((id, entry)) = found else {
            return Ok(GroupJoining::NoSuchInvite);
        };
        if entry.members.contains_key(account_id) {
            return Ok(GroupJoining::AlreadyIn(entry.read(id, accounts)));
        }
        if in_groups >= room.groups_per_account as usize {
            return Ok(GroupJoining::TooManyGroups);
        }
        if entry.members.len() >= room.members_per_group as usize {
            return Ok(GroupJoining::GroupFull);
        }
        entry.members.insert(account_id.to_owned(), now);
        Ok(GroupJoining::Joined(entry.read(id, accounts)))
    }

    pub(super) fn of_account(&self, account_id: &str, accounts: &Accounts) -> Vec<Group> {
        let mut found: Vec<(Timestamp, &String, &Entry)> = self
            .groups
            .iter()
            .filter_map(|(id, entry)| {
                let since = entry.members.get(account_id)?;
                Some((*since, id, entry))
            })
            .collect();
        found.sort_by_key(|(since, id, _)| (*since, *id));
        found
            .into_iter()
            .map(|(_, id, entry)| entry.read(id, accounts))
            .collect()
    }

    pub(super) fn get(
        &self,
        id: &str,
        account_id: &str,
        accounts: &Accounts,
    ) -> Option<GroupMembers> {
        self.as_member(id, account_id)
            .map(|entry| entry.read_with_members(id, accounts))
    }

    /// The group for something only its keeper may do.
    fn as_keeper<T>(&mut self, id: &str, keeper_id: &str) -> Result<&mut Entry, KeeperAct<T>> {
        let entry = self
            .as_member_mut(id, keeper_id)
            .ok_or(KeeperAct::NoSuchGroup)?;
        if entry.keeper_id != keeper_id {
            return Err(KeeperAct::NotKeeper);
        }
        Ok(entry)
    }

    pub(super) fn rename(
        &mut self,
        id: &str,
        keeper_id: &str,
        name: &str,
        accounts: &Accounts,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        check_name(name)?;
        Ok(match self.as_keeper(id, keeper_id) {
            Ok(entry) => {
                name.clone_into(&mut entry.name);
                KeeperAct::Done(entry.read_with_members(id, accounts))
            }
            Err(refused) => refused,
        })
    }

    pub(super) fn renew_invite(
        &mut self,
        id: &str,
        keeper_id: &str,
        invite: &SecretHash,
    ) -> Result<KeeperAct<()>, StoreError> {
        let taken = self
            .groups
            .iter()
            .any(|(other, entry)| other != id && entry.invite.as_ref() == Some(invite));
        Ok(match self.as_keeper(id, keeper_id) {
            Ok(_) if taken => {
                return Err(StoreError::Invalid(
                    "an invite that another group has".to_owned(),
                ));
            }
            Ok(entry) => {
                entry.invite = Some(*invite);
                KeeperAct::Done(())
            }
            Err(refused) => refused,
        })
    }

    pub(super) fn hand_over(
        &mut self,
        id: &str,
        keeper_id: &str,
        member_handle: &str,
        accounts: &Accounts,
    ) -> KeeperAct<GroupMembers> {
        match self.as_keeper(id, keeper_id) {
            Ok(entry) => match entry.member_with(member_handle, accounts) {
                Some(member) => {
                    entry.keeper_id = member;
                    KeeperAct::Done(entry.read_with_members(id, accounts))
                }
                None => KeeperAct::NoSuchMember,
            },
            Err(refused) => refused,
        }
    }

    /// Takes a member out of a group with what they put into it, passes
    /// the keeping on if they kept the group, and removes the group if
    /// they were its last member. Says whether the group is gone.
    fn take_out(&mut self, id: &str, account_id: &str) -> bool {
        let Some(entry) = self.groups.get_mut(id) else {
            return false;
        };
        entry.members.remove(account_id);
        if entry.keeper_id == account_id {
            let longest = in_order(&entry.members).first().map(|id| (*id).clone());
            match longest {
                Some(longest) => entry.keeper_id = longest,
                None => {
                    self.groups.remove(id);
                    self.plans.retain(|_, plan| plan.group_id != id);
                    return true;
                }
            }
        }
        self.plans
            .retain(|_, plan| !(plan.group_id == id && plan.author_id == account_id));
        for plan in self.plans.values_mut().filter(|plan| plan.group_id == id) {
            plan.pilots_in.remove(account_id);
        }
        false
    }

    pub(super) fn remove_member(
        &mut self,
        id: &str,
        by_account_id: &str,
        member_handle: &str,
        accounts: &Accounts,
    ) -> MemberRemoval {
        let Some(entry) = self.as_member_mut(id, by_account_id) else {
            return MemberRemoval::NoSuchGroup;
        };
        let member = entry.member_with(member_handle, accounts);
        let leaving = member.as_deref() == Some(by_account_id);
        if !leaving && entry.keeper_id != by_account_id {
            return MemberRemoval::NotKeeper;
        }
        let Some(member) = member else {
            return MemberRemoval::NoSuchMember;
        };
        if !leaving {
            // Whoever is removed knew the code.
            entry.invite = None;
        }
        if self.take_out(id, &member) {
            MemberRemoval::GroupRemoved
        } else {
            MemberRemoval::Removed
        }
    }

    pub(super) fn delete(&mut self, id: &str, keeper_id: &str) -> KeeperAct<()> {
        match self.as_keeper(id, keeper_id) {
            Ok(_) => {
                self.groups.remove(id);
                self.plans.retain(|_, plan| plan.group_id != id);
                KeeperAct::Done(())
            }
            Err(refused) => refused,
        }
    }

    /// Takes an account out of every group it is in.
    pub(super) fn delete_account(&mut self, account_id: &str) {
        let its_groups: Vec<String> = self
            .groups
            .iter()
            .filter(|(_, entry)| entry.members.contains_key(account_id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in its_groups {
            self.take_out(&id, account_id);
        }
    }

    pub(super) fn save_plan(
        &mut self,
        new: &NewPlan,
        room: &PlanRoom,
        accounts: &Accounts,
    ) -> Result<PlanSaving, StoreError> {
        check_plan(new)?;
        if self.as_member(&new.group_id, &new.author_id).is_none() {
            return Ok(PlanSaving::NoSuchGroup);
        }
        let held = self
            .plans
            .values()
            .filter(|plan| plan.group_id == new.group_id && plan.details.day >= room.gone_before)
            .count();
        if held >= room.plans_per_group as usize {
            return Ok(PlanSaving::GroupFull);
        }
        if self.plans.contains_key(&new.id) {
            return Err(StoreError::Invalid(
                "a plan under an identifier that is in use".to_owned(),
            ));
        }
        let entry = PlanEntry {
            group_id: new.group_id.clone(),
            author_id: new.author_id.clone(),
            details: new.details.clone(),
            created_at: new.created_at,
            changed_at: new.created_at,
            pilots_in: BTreeMap::from([(new.author_id.clone(), new.created_at)]),
        };
        let plan = entry.read(&new.id, accounts);
        self.plans.insert(new.id.clone(), entry);
        Ok(PlanSaving::Stored(Box::new(plan)))
    }

    pub(super) fn change_plan(
        &mut self,
        (group_id, plan_id): (&str, &str),
        by_account_id: &str,
        details: &PlanDetails,
        now: Timestamp,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> Result<PlanChange, StoreError> {
        check_details(details)?;
        if self.as_member(group_id, by_account_id).is_none() {
            return Ok(PlanChange::NoSuchGroup);
        }
        if self.kept_plan(group_id, plan_id, gone_before).is_none() {
            return Ok(PlanChange::NoSuchPlan);
        }
        let Some(plan) = self.plans.get_mut(plan_id) else {
            return Ok(PlanChange::NoSuchPlan);
        };
        if plan.author_id != by_account_id {
            return Ok(PlanChange::NotAuthor);
        }
        plan.details = details.clone();
        plan.changed_at = now;
        Ok(PlanChange::Now(Box::new(plan.read(plan_id, accounts))))
    }

    pub(super) fn delete_plan(
        &mut self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        gone_before: NaiveDate,
    ) -> PlanRemoval {
        let Some(group) = self.as_member(group_id, by_account_id) else {
            return PlanRemoval::NoSuchGroup;
        };
        let Some(plan) = self.kept_plan(group_id, plan_id, gone_before) else {
            return PlanRemoval::NoSuchPlan;
        };
        if plan.author_id != by_account_id && group.keeper_id != by_account_id {
            return PlanRemoval::NotAllowed;
        }
        self.plans.remove(plan_id);
        PlanRemoval::Removed
    }

    pub(super) fn set_in(
        &mut self,
        (group_id, plan_id): (&str, &str),
        account_id: &str,
        is_in: bool,
        now: Timestamp,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> PlanChange {
        if self.as_member(group_id, account_id).is_none() {
            return PlanChange::NoSuchGroup;
        }
        if self.kept_plan(group_id, plan_id, gone_before).is_none() {
            return PlanChange::NoSuchPlan;
        }
        let Some(plan) = self.plans.get_mut(plan_id) else {
            return PlanChange::NoSuchPlan;
        };
        if is_in {
            plan.pilots_in.entry(account_id.to_owned()).or_insert(now);
        } else {
            plan.pilots_in.remove(account_id);
        }
        PlanChange::Now(Box::new(plan.read(plan_id, accounts)))
    }

    pub(super) fn plans_of_group(
        &self,
        group_id: &str,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
        accounts: &Accounts,
    ) -> Option<Vec<Plan>> {
        self.as_member(group_id, account_id)?;
        let plans = self
            .plans
            .iter()
            .filter(|(_, plan)| plan.group_id == group_id && plan.is_for(from, to))
            .map(|(id, plan)| plan.read(id, accounts))
            .collect();
        Some(sorted(plans))
    }

    pub(super) fn plans_for(
        &self,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
        limit: usize,
        accounts: &Accounts,
    ) -> Vec<Plan> {
        let plans = self
            .plans
            .iter()
            .filter(|(_, plan)| {
                plan.is_for(from, to) && self.as_member(&plan.group_id, account_id).is_some()
            })
            .map(|(id, plan)| plan.read(id, accounts))
            .collect();
        let mut plans = sorted(plans);
        plans.truncate(limit);
        plans
    }

    pub(super) fn prune_plans(&mut self, before: NaiveDate) -> u64 {
        let held = self.plans.len();
        self.plans.retain(|_, plan| plan.details.day >= before);
        (held - self.plans.len()) as u64
    }

    // What follows is for whoever runs the server: it asks for no account
    // and answers for any group.

    pub(super) fn any(&self, id: &str, accounts: &Accounts) -> Option<GroupMembers> {
        self.groups
            .get(id)
            .map(|entry| entry.read_with_members(id, accounts))
    }

    pub(super) fn plans_of_any(
        &self,
        group_id: &str,
        from: NaiveDate,
        accounts: &Accounts,
    ) -> Option<Vec<Plan>> {
        self.groups.get(group_id)?;
        let plans = self
            .plans
            .iter()
            .filter(|(_, plan)| plan.group_id == group_id && plan.is_for(from, None))
            .map(|(id, plan)| plan.read(id, accounts))
            .collect();
        Some(sorted(plans))
    }

    pub(super) fn any_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> Option<Plan> {
        self.plans
            .get(plan_id)
            .filter(|plan| plan.details.day >= gone_before)
            .map(|plan| plan.read(plan_id, accounts))
    }

    pub(super) fn rename_any(
        &mut self,
        id: &str,
        name: &str,
        accounts: &Accounts,
    ) -> Result<Option<GroupMembers>, StoreError> {
        check_name(name)?;
        Ok(self.groups.get_mut(id).map(|entry| {
            name.clone_into(&mut entry.name);
            entry.read_with_members(id, accounts)
        }))
    }

    pub(super) fn remove_plan(
        &mut self,
        plan_id: &str,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> Option<Plan> {
        let plan = self.any_plan(plan_id, gone_before, accounts)?;
        self.plans.remove(plan_id);
        Some(plan)
    }

    pub(super) fn remove(
        &mut self,
        id: &str,
        gone_before: NaiveDate,
        accounts: &Accounts,
    ) -> Option<RemovedGroup> {
        let group = self.any(id, accounts)?;
        let plans = self
            .plans
            .values()
            .filter(|plan| plan.group_id == id && plan.details.day >= gone_before)
            .count();
        self.groups.remove(id);
        self.plans.retain(|_, plan| plan.group_id != id);
        Some(RemovedGroup {
            group,
            plans: count(plans),
        })
    }
}
