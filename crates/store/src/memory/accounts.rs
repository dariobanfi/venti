//! Accounts, sessions and invites of the in-memory store.

use std::collections::BTreeMap;

use chrono::Duration;
use venti_domain::Timestamp;

use crate::flight::next_change;
use crate::{
    Account, AccountCreation, NewAccount, NewInvite, SESSION_TOUCH_MINUTES, SecretHash, Session,
};

#[derive(Debug, Default)]
pub(super) struct Accounts {
    /// By account identifier.
    accounts: BTreeMap<String, Entry>,
    /// Account identifier by handle.
    handles: BTreeMap<String, String>,
    /// By session identifier.
    sessions: BTreeMap<String, (Session, SecretHash)>,
    /// By the hash of the code.
    invites: BTreeMap<SecretHash, Invite>,
}

#[derive(Debug)]
struct Entry {
    account: Account,
    password_hash: String,
    /// The largest change number the account's flights have used.
    flight_change: i64,
}

#[derive(Debug)]
struct Invite {
    expires_at: Option<Timestamp>,
    used: bool,
}

impl Accounts {
    pub(super) fn create(&mut self, new: &NewAccount) -> AccountCreation {
        let created_at = new.account.created_at;
        if let Some(code) = &new.invite {
            let good = self.invites.get(code).is_some_and(|invite| {
                !invite.used && invite.expires_at.is_none_or(|expires| expires > created_at)
            });
            if !good {
                return AccountCreation::InviteRefused;
            }
        }
        if self.handles.contains_key(&new.account.handle) {
            return AccountCreation::HandleTaken;
        }

        if let Some(invite) = new.invite.and_then(|code| self.invites.get_mut(&code)) {
            invite.used = true;
        }
        self.handles
            .insert(new.account.handle.clone(), new.account.id.clone());
        let entry = Entry {
            // Nobody is suspended before they exist.
            account: Account {
                publishing_suspended_at: None,
                ..new.account.clone()
            },
            password_hash: new.password_hash.clone(),
            flight_change: 0,
        };
        self.accounts.insert(new.account.id.clone(), entry);
        AccountCreation::Created
    }

    pub(super) fn by_id(&self, id: &str) -> Option<&Account> {
        self.accounts.get(id).map(|entry| &entry.account)
    }

    pub(super) fn by_handle(&self, handle: &str) -> Option<&Account> {
        self.by_id(self.handles.get(handle)?)
    }

    pub(super) fn password_hash(&self, handle: &str) -> Option<(Account, String)> {
        let entry = self.accounts.get(self.handles.get(handle)?)?;
        Some((entry.account.clone(), entry.password_hash.clone()))
    }

    pub(super) fn set_password(&mut self, account_id: &str, hash: &str, keep_session: &str) {
        let Some(entry) = self.accounts.get_mut(account_id) else {
            return;
        };
        hash.clone_into(&mut entry.password_hash);
        self.sessions
            .retain(|id, (session, _)| session.account_id != account_id || id == keep_session);
    }

    /// Removes the account, its sessions and its handle. Its flights are
    /// the caller's to remove.
    pub(super) fn delete(&mut self, account_id: &str) {
        if let Some(entry) = self.accounts.remove(account_id) {
            self.handles.remove(&entry.account.handle);
        }
        self.delete_sessions(account_id);
    }

    pub(super) fn create_invite(&mut self, invite: &NewInvite) {
        let stored = Invite {
            expires_at: invite.expires_at,
            used: false,
        };
        self.invites.entry(invite.code).or_insert(stored);
    }

    /// Whether an account's publishing is suspended; `false` for an
    /// account that is not there.
    pub(super) fn is_suspended(&self, account_id: &str) -> bool {
        self.by_id(account_id)
            .is_some_and(|account| account.publishing_suspended_at.is_some())
    }

    /// Marks an account as suspended since `now`, or leaves the mark it
    /// has, and returns the moment it then carries. `None` for an account
    /// that is not there.
    pub(super) fn suspend(&mut self, account_id: &str, now: Timestamp) -> Option<Timestamp> {
        let entry = self.accounts.get_mut(account_id)?;
        Some(*entry.account.publishing_suspended_at.get_or_insert(now))
    }

    /// Takes the mark off, and says whether there was one. `None` for an
    /// account that is not there.
    pub(super) fn resume(&mut self, account_id: &str) -> Option<bool> {
        let entry = self.accounts.get_mut(account_id)?;
        Some(entry.account.publishing_suspended_at.take().is_some())
    }

    /// The latest change number of an account's flights.
    pub(super) fn flight_change(&self, account_id: &str) -> Option<i64> {
        self.accounts
            .get(account_id)
            .map(|entry| entry.flight_change)
    }

    /// Takes the next change number of an account's flights.
    pub(super) fn next_flight_change(&mut self, account_id: &str, now: Timestamp) -> Option<i64> {
        let entry = self.accounts.get_mut(account_id)?;
        entry.flight_change = next_change(entry.flight_change, now);
        Some(entry.flight_change)
    }

    pub(super) fn create_session(&mut self, session: &Session, token: &SecretHash, keep: usize) {
        self.sessions
            .insert(session.id.clone(), (session.clone(), *token));
        let mut of_account = self.sessions_of(&session.account_id);
        for old in of_account.split_off(keep.min(of_account.len())) {
            self.sessions.remove(&old.id);
        }
    }

    pub(super) fn use_session(
        &mut self,
        token: &SecretHash,
        now: Timestamp,
        idle_since: Timestamp,
    ) -> Option<(Session, Account)> {
        let (session, _) = self
            .sessions
            .values_mut()
            .find(|(_, stored)| stored == token)?;
        if session.last_used_at < idle_since {
            return None;
        }
        if session.last_used_at < now - Duration::minutes(SESSION_TOUCH_MINUTES) {
            session.last_used_at = now;
        }
        let session = session.clone();
        let account = self.by_id(&session.account_id)?.clone();
        Some((session, account))
    }

    /// An account's sessions, most recently used first.
    pub(super) fn sessions_of(&self, account_id: &str) -> Vec<Session> {
        let mut sessions: Vec<Session> = self
            .sessions
            .values()
            .filter(|(session, _)| session.account_id == account_id)
            .map(|(session, _)| session.clone())
            .collect();
        sessions.sort_by(|a, b| (b.last_used_at, &b.id).cmp(&(a.last_used_at, &a.id)));
        sessions
    }

    pub(super) fn delete_session(&mut self, account_id: &str, session_id: &str) -> bool {
        let owned = self
            .sessions
            .get(session_id)
            .is_some_and(|(session, _)| session.account_id == account_id);
        if owned {
            self.sessions.remove(session_id);
        }
        owned
    }

    pub(super) fn delete_sessions(&mut self, account_id: &str) {
        self.sessions
            .retain(|_, (session, _)| session.account_id != account_id);
    }

    pub(super) fn prune_sessions(&mut self, idle_since: Timestamp) -> u64 {
        let before = self.sessions.len();
        self.sessions
            .retain(|_, (session, _)| session.last_used_at >= idle_since);
        (before - self.sessions.len()) as u64
    }
}
