//! Groups as communities with a chat, in the in-memory store.

use venti_domain::Timestamp;

use super::accounts::Accounts;
use super::groups::{Entry, Groups, in_order, pilot};
use crate::group_chat::{check_change, check_message, names, sort_found};
use crate::{
    FoundGroup, GroupAct, GroupInvite, GroupJoining, GroupMembers, GroupPicture, GroupRole,
    GroupRoom, GroupSearch, GroupSeen, GroupStanding, InviteRoom, ListedGroup, Listener, Message,
    MessagePage, MessagePosting, MessageRemoval, NewMessage, ProfileChange, RemovedBy, StoreError,
    distance_km,
};

fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

impl Entry {
    fn role(&self, account_id: &str) -> Option<GroupRole> {
        if self.keeper_id == account_id {
            Some(GroupRole::Owner)
        } else if self.admins.contains(account_id) {
            Some(GroupRole::Admin)
        } else if self.members.contains_key(account_id) {
            Some(GroupRole::Member)
        } else {
            None
        }
    }

    fn may_moderate(&self, account_id: &str) -> bool {
        matches!(
            self.role(account_id),
            Some(GroupRole::Owner | GroupRole::Admin)
        )
    }
}

impl Groups {
    /// A message as it is read: its plan only while the plan is there.
    fn read_message(&self, message: &Message, accounts: &Accounts) -> Message {
        let mut read = message.clone();
        read.author = pilot(&message.author.account_id, accounts);
        read.plan_id = message
            .plan_id
            .clone()
            .filter(|plan| self.plans.contains_key(plan));
        read
    }

    fn unread(&self, id: &str, account_id: &str, most: u32) -> u32 {
        let Some(entry) = self.groups.get(id) else {
            return 0;
        };
        let read = entry.read.get(account_id).copied().unwrap_or(0);
        let found = self
            .messages
            .values()
            .filter(|message| {
                message.group_id == id
                    && message.number > read
                    && message.author.account_id != account_id
                    && message.removed.is_none()
            })
            .count();
        count(found).min(most)
    }

    fn standing(&self, id: &str, account_id: &str, most: u32) -> GroupStanding {
        let entry = self.groups.get(id);
        let role = entry.and_then(|entry| entry.role(account_id));
        GroupStanding {
            role,
            invited: role.is_none()
                && self
                    .invites
                    .contains_key(&(id.to_owned(), account_id.to_owned())),
            muted: entry.is_some_and(|entry| entry.muted.contains(account_id)),
            read_up_to: entry
                .and_then(|entry| entry.read.get(account_id).copied())
                .unwrap_or(0),
            unread: if role.is_some() {
                self.unread(id, account_id, most)
            } else {
                0
            },
        }
    }

    /// Whether an account reads a group's messages.
    fn reads(&self, id: &str, account_id: &str) -> bool {
        self.groups
            .get(id)
            .is_some_and(|entry| entry.profile.public || entry.members.contains_key(account_id))
    }

    pub(super) fn find(
        &self,
        account_id: &str,
        search: &GroupSearch,
        accounts: &Accounts,
    ) -> Vec<FoundGroup> {
        let mut found: Vec<FoundGroup> = self
            .groups
            .iter()
            .filter(|(_, entry)| entry.profile.public)
            .filter_map(|(id, entry)| {
                let group = entry.read(id, accounts);
                if search
                    .text
                    .as_ref()
                    .is_some_and(|text| !names(&group, text))
                {
                    return None;
                }
                let distance_km = search
                    .near
                    .zip(group.profile.home.as_ref())
                    .map(|(near, home)| distance_km(near, home.center));
                if search.near.is_some() && distance_km.is_none_or(|km| km > search.within_km) {
                    return None;
                }
                Some(FoundGroup {
                    member: entry.members.contains_key(account_id),
                    group,
                    distance_km,
                })
            })
            .collect();
        sort_found(&mut found);
        found.truncate(search.limit);
        found
    }

    pub(super) fn seen(
        &self,
        id: &str,
        account_id: &str,
        most: u32,
        accounts: &Accounts,
    ) -> Option<GroupSeen> {
        let entry = self.groups.get(id)?;
        let standing = self.standing(id, account_id, most);
        if !entry.profile.public && standing.role.is_none() && !standing.invited {
            return None;
        }
        let members = entry.read_with_members(id, accounts);
        let admins = in_order(&entry.members)
            .into_iter()
            .filter(|member| entry.admins.contains(*member))
            .cloned()
            .collect();
        Some(GroupSeen {
            group: members,
            admins,
            standing,
        })
    }

    pub(super) fn list(
        &self,
        account_id: &str,
        most: u32,
        accounts: &Accounts,
    ) -> Vec<ListedGroup> {
        self.of_account(account_id, accounts)
            .into_iter()
            .map(|group| {
                let last = self
                    .messages
                    .values()
                    .filter(|message| message.group_id == group.id)
                    .max_by_key(|message| message.number)
                    .map(|message| self.read_message(message, accounts));
                ListedGroup {
                    standing: self.standing(&group.id, account_id, most),
                    group,
                    last,
                }
            })
            .collect()
    }

    pub(super) fn change(
        &mut self,
        id: &str,
        by: &str,
        change: &ProfileChange,
        accounts: &Accounts,
    ) -> Result<GroupAct<GroupMembers>, StoreError> {
        check_change(change)?;
        if let Some(Some(picture)) = &change.picture {
            let ours = self
                .pictures
                .get(picture)
                .is_some_and(|kept| kept.group_id == id);
            if !ours {
                return Err(StoreError::Invalid(
                    "a group's picture that was not sent to it".to_owned(),
                ));
            }
        }
        let Some(entry) = self.as_member_mut(id, by) else {
            return Ok(GroupAct::NoSuchGroup);
        };
        let owner = entry.keeper_id == by;
        if !entry.may_moderate(by) || (change.public.is_some() && !owner) {
            return Ok(GroupAct::NotAllowed);
        }
        if let Some(name) = &change.name {
            name.clone_into(&mut entry.name);
        }
        if let Some(public) = change.public {
            entry.profile.public = public;
        }
        if let Some(description) = &change.description {
            entry.profile.description.clone_from(description);
        }
        if let Some(picture) = &change.picture {
            entry.profile.picture.clone_from(picture);
        }
        if let Some(home) = &change.home {
            entry.profile.home.clone_from(home);
        }
        Ok(GroupAct::Done(entry.read_with_members(id, accounts)))
    }

    pub(super) fn set_admin(
        &mut self,
        id: &str,
        owner_id: &str,
        handle: &str,
        admin: bool,
        accounts: &Accounts,
    ) -> GroupAct<()> {
        let Some(entry) = self.as_member_mut(id, owner_id) else {
            return GroupAct::NoSuchGroup;
        };
        if entry.keeper_id != owner_id {
            return GroupAct::NotAllowed;
        }
        let Some(member) = entry.member_with(handle, accounts) else {
            return GroupAct::NoSuchPilot;
        };
        if member == entry.keeper_id {
            // The owner is more than an admin already.
            return GroupAct::Done(());
        }
        if admin {
            entry.admins.insert(member);
        } else {
            entry.admins.remove(&member);
        }
        GroupAct::Done(())
    }

    pub(super) fn invite(
        &mut self,
        (id, by): (&str, &str),
        handle: &str,
        now: Timestamp,
        room: &InviteRoom,
        accounts: &Accounts,
    ) -> GroupAct<GroupInvite> {
        let Some(entry) = self.as_member(id, by) else {
            return GroupAct::NoSuchGroup;
        };
        if !entry.may_moderate(by) {
            return GroupAct::NotAllowed;
        }
        let Some(invited) = accounts.by_handle(handle).map(|account| account.id.clone()) else {
            return GroupAct::NoSuchPilot;
        };
        if entry.members.contains_key(&invited) {
            return GroupAct::AlreadyIn;
        }
        let group = entry.read(id, accounts);
        let key = (id.to_owned(), invited);
        if let Some((asker, at)) = self.invites.get(&key) {
            return GroupAct::Done(GroupInvite {
                group,
                invited_by: pilot(asker, accounts),
                invited_at: *at,
            });
        }
        let open = self.invites.keys().filter(|(group, _)| group == id).count();
        if open >= room.invites_per_group as usize {
            return GroupAct::Full;
        }
        self.invites.insert(key, (by.to_owned(), now));
        GroupAct::Done(GroupInvite {
            group,
            invited_by: pilot(by, accounts),
            invited_at: now,
        })
    }

    pub(super) fn drop_invite(
        &mut self,
        id: &str,
        by: &str,
        handle: &str,
        accounts: &Accounts,
    ) -> GroupAct<()> {
        let Some(invited) = accounts.by_handle(handle).map(|account| account.id.clone()) else {
            return if self.as_member(id, by).is_some() {
                GroupAct::NoSuchPilot
            } else {
                GroupAct::NoSuchGroup
            };
        };
        let key = (id.to_owned(), invited.clone());
        let declining = invited == by && self.invites.contains_key(&key);
        if !declining {
            let Some(entry) = self.as_member(id, by) else {
                return GroupAct::NoSuchGroup;
            };
            if !entry.may_moderate(by) {
                return GroupAct::NotAllowed;
            }
            if !self.invites.contains_key(&key) {
                return GroupAct::NoSuchPilot;
            }
        }
        self.invites.remove(&key);
        GroupAct::Done(())
    }

    pub(super) fn invites_for(&self, account_id: &str, accounts: &Accounts) -> Vec<GroupInvite> {
        let mut invites: Vec<GroupInvite> = self
            .invites
            .iter()
            .filter(|((_, invited), _)| invited == account_id)
            .filter_map(|((group, _), (by, at))| {
                let entry = self.groups.get(group)?;
                Some(GroupInvite {
                    group: entry.read(group, accounts),
                    invited_by: pilot(by, accounts),
                    invited_at: *at,
                })
            })
            .collect();
        invites.sort_by(|a, b| {
            b.invited_at
                .cmp(&a.invited_at)
                .then(a.group.id.cmp(&b.group.id))
        });
        invites
    }

    pub(super) fn enter(
        &mut self,
        id: &str,
        account_id: &str,
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
        let key = (id.to_owned(), account_id.to_owned());
        let invited = self.invites.contains_key(&key);
        let Some(entry) = self.groups.get_mut(id) else {
            return Ok(GroupJoining::NoSuchInvite);
        };
        if entry.members.contains_key(account_id) {
            return Ok(GroupJoining::AlreadyIn(entry.read(id, accounts)));
        }
        if !entry.profile.public && !invited {
            return Ok(GroupJoining::NoSuchInvite);
        }
        if in_groups >= room.groups_per_account as usize {
            return Ok(GroupJoining::TooManyGroups);
        }
        if entry.members.len() >= room.members_per_group as usize {
            return Ok(GroupJoining::GroupFull);
        }
        entry.members.insert(account_id.to_owned(), now);
        // A new member starts with everything before them read.
        let newest = self
            .messages
            .values()
            .filter(|message| message.group_id == id)
            .map(|message| message.number)
            .max()
            .unwrap_or(0);
        let entry = self.groups.get_mut(id).expect("the group is there");
        entry.read.insert(account_id.to_owned(), newest);
        let group = entry.read(id, accounts);
        self.invites.remove(&key);
        Ok(GroupJoining::Joined(group))
    }

    pub(super) fn post(
        &mut self,
        new: &NewMessage,
        accounts: &Accounts,
    ) -> Result<MessagePosting, StoreError> {
        check_message(new)?;
        if self.as_member(&new.group_id, &new.author_id).is_none() {
            return Ok(MessagePosting::NoSuchGroup);
        }
        if self.messages.contains_key(&new.id) {
            return Err(StoreError::Invalid(
                "a message under an identifier that is in use".to_owned(),
            ));
        }
        let pictures_fit = new.pictures.iter().all(|id| {
            self.pictures.get(id).is_some_and(|picture| {
                picture.group_id == new.group_id && picture.account_id == new.author_id
            }) && !self
                .messages
                .values()
                .any(|message| message.pictures.contains(id))
        });
        if !pictures_fit {
            return Ok(MessagePosting::NoSuchPicture);
        }
        let number = self.next_number();
        let message = Message {
            id: new.id.clone(),
            group_id: new.group_id.clone(),
            number,
            version: number,
            author: pilot(&new.author_id, accounts),
            kind: crate::MessageKind::Text,
            text: new.text.clone(),
            pictures: new.pictures.clone(),
            links: new.links.clone(),
            plan_id: None,
            created_at: new.created_at,
            removed: None,
        };
        self.messages.insert(new.id.clone(), message.clone());
        // The author has read what they wrote.
        if let Some(entry) = self.groups.get_mut(&new.group_id) {
            entry.read.insert(new.author_id.clone(), number);
        }
        Ok(MessagePosting::Posted(Box::new(message)))
    }

    pub(super) fn page(
        &self,
        group_id: &str,
        account_id: &str,
        page: MessagePage,
        limit: usize,
        accounts: &Accounts,
    ) -> Option<Vec<Message>> {
        if !self.reads(group_id, account_id) {
            return None;
        }
        let mut found: Vec<&Message> = self
            .messages
            .values()
            .filter(|message| message.group_id == group_id)
            .filter(|message| match page {
                MessagePage::Before(before) => before.is_none_or(|before| message.number < before),
                MessagePage::After(after) => message.version > after,
            })
            .collect();
        match page {
            MessagePage::Before(_) => {
                found.sort_by_key(|message| std::cmp::Reverse(message.number))
            }
            MessagePage::After(_) => found.sort_by_key(|message| message.version),
        }
        found.truncate(limit);
        Some(
            found
                .into_iter()
                .map(|message| self.read_message(message, accounts))
                .collect(),
        )
    }

    pub(super) fn message(
        &self,
        group_id: &str,
        message_id: &str,
        account_id: &str,
        accounts: &Accounts,
    ) -> Option<Message> {
        if !self.reads(group_id, account_id) {
            return None;
        }
        self.messages
            .get(message_id)
            .filter(|message| message.group_id == group_id)
            .map(|message| self.read_message(message, accounts))
    }

    pub(super) fn remove_message(
        &mut self,
        (group_id, message_id): (&str, &str),
        by: &str,
        now: Timestamp,
        accounts: &Accounts,
    ) -> MessageRemoval {
        let Some(entry) = self.as_member(group_id, by) else {
            return MessageRemoval::NoSuchGroup;
        };
        let moderates = entry.may_moderate(by);
        let Some(message) = self
            .messages
            .get(message_id)
            .filter(|message| message.group_id == group_id)
        else {
            return MessageRemoval::NoSuchMessage;
        };
        let by_author = message.author.account_id == by;
        if !by_author && !moderates {
            return MessageRemoval::NotAllowed;
        }
        if message.removed.is_some() {
            return MessageRemoval::Removed(Box::new(self.read_message(message, accounts)));
        }
        let version = self.next_number();
        let message = self.messages.get_mut(message_id).expect("found above");
        let pictures = std::mem::take(&mut message.pictures);
        message.text.clear();
        message.links.clear();
        message.version = version;
        let who = if by_author {
            RemovedBy::Author
        } else {
            RemovedBy::Admin
        };
        message.removed = Some((now, who));
        let removed = message.clone();
        for picture in pictures {
            self.pictures.remove(&picture);
        }
        MessageRemoval::Removed(Box::new(self.read_message(&removed, accounts)))
    }

    pub(super) fn set_standing(
        &mut self,
        id: &str,
        account_id: &str,
        (muted, read_up_to): (Option<bool>, Option<i64>),
        most: u32,
    ) -> Option<GroupStanding> {
        let newest = self
            .messages
            .values()
            .filter(|message| message.group_id == id)
            .map(|message| message.number)
            .max()
            .unwrap_or(0);
        let entry = self.as_member_mut(id, account_id)?;
        if let Some(muted) = muted {
            if muted {
                entry.muted.insert(account_id.to_owned());
            } else {
                entry.muted.remove(account_id);
            }
        }
        if let Some(up_to) = read_up_to {
            let mark = entry.read.entry(account_id.to_owned()).or_insert(0);
            *mark = (*mark).max(up_to.min(newest));
        }
        Some(self.standing(id, account_id, most))
    }

    pub(super) fn listeners(&self, id: &str, accounts: &Accounts) -> Vec<Listener> {
        let Some(entry) = self.groups.get(id) else {
            return Vec::new();
        };
        in_order(&entry.members)
            .into_iter()
            .map(|member| Listener {
                account_id: member.clone(),
                handle: pilot(member, accounts).handle,
                muted: entry.muted.contains(member),
            })
            .collect()
    }

    pub(super) fn save_picture(&mut self, picture: &GroupPicture) -> Result<bool, StoreError> {
        if self.pictures.contains_key(&picture.id) {
            return Err(StoreError::Invalid(
                "a picture under an identifier that is in use".to_owned(),
            ));
        }
        if self
            .as_member(&picture.group_id, &picture.account_id)
            .is_none()
        {
            return Ok(false);
        }
        self.pictures.insert(picture.id.clone(), picture.clone());
        Ok(true)
    }

    pub(super) fn picture(&self, id: &str) -> Option<GroupPicture> {
        self.pictures.get(id).cloned()
    }

    pub(super) fn unread_of(&self, account_id: &str, most: u32) -> u32 {
        let ids: Vec<&String> = self
            .groups
            .iter()
            .filter(|(_, entry)| entry.members.contains_key(account_id))
            .map(|(id, _)| id)
            .collect();
        let total: u32 = ids
            .into_iter()
            .map(|id| self.unread(id, account_id, most))
            .fold(0, u32::saturating_add);
        total.min(most)
    }
}
