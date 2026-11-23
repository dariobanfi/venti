//! Accounts, sessions and invites of the in-memory store.

use std::collections::BTreeMap;

use chrono::Duration;
use venti_domain::Timestamp;

use crate::flight::next_change;
use crate::{
    AccessToken, Account, AccountCreation, AccountSettings, GoogleLink, GoogleLinking, NewAccount,
    NewGoogleAccount, NewInvite, NewPicture, Picture, PictureOrigin, SESSION_TOUCH_MINUTES,
    SecretHash, Session,
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
    /// The tokens pilots gave to other programs, by token identifier.
    pub(super) tokens: BTreeMap<String, (AccessToken, SecretHash)>,
    /// What the authorization server keeps beside them.
    pub(super) oauth: super::oauth::Oauth,
    /// Account identifier by the subject of its Google account.
    google: BTreeMap<String, String>,
}

#[derive(Debug)]
struct Entry {
    account: Account,
    /// `None` for an account made with Google.
    password_hash: Option<String>,
    /// The Google account it is linked to.
    google: Option<GoogleLink>,
    /// The largest change number the account's flights have used.
    flight_change: i64,
    /// Its picture.
    picture: Option<Picture>,
    /// Whether the picture of its Google account was tried.
    google_picture_tried: bool,
}

#[derive(Debug)]
struct Invite {
    expires_at: Option<Timestamp>,
    used: bool,
}

impl Accounts {
    pub(super) fn create(&mut self, new: &NewAccount) -> AccountCreation {
        self.create_with(
            &new.account,
            Some(new.password_hash.clone()),
            None,
            new.invite,
        )
    }

    pub(super) fn create_google(&mut self, new: &NewGoogleAccount) -> AccountCreation {
        self.create_with(&new.account, None, Some(new.google.clone()), new.invite)
    }

    fn create_with(
        &mut self,
        account: &Account,
        password_hash: Option<String>,
        google: Option<GoogleLink>,
        invite: Option<SecretHash>,
    ) -> AccountCreation {
        let created_at = account.created_at;
        if let Some(code) = &invite {
            let good = self.invites.get(code).is_some_and(|invite| {
                !invite.used && invite.expires_at.is_none_or(|expires| expires > created_at)
            });
            if !good {
                return AccountCreation::InviteRefused;
            }
        }
        if self.handles.contains_key(&account.handle) {
            return AccountCreation::HandleTaken;
        }
        if let Some(google) = &google
            && self.google.contains_key(&google.subject)
        {
            return AccountCreation::GoogleTaken;
        }

        if let Some(invite) = invite.and_then(|code| self.invites.get_mut(&code)) {
            invite.used = true;
        }
        self.handles
            .insert(account.handle.clone(), account.id.clone());
        if let Some(google) = &google {
            self.google
                .insert(google.subject.clone(), account.id.clone());
        }
        let entry = Entry {
            // Nobody is suspended before they exist, and nobody trusted
            // with the sites.
            account: Account {
                publishing_suspended_at: None,
                edits_sites_since: None,
                picture: None,
                ..account.clone()
            },
            password_hash,
            google,
            flight_change: 0,
            picture: None,
            google_picture_tried: false,
        };
        self.accounts.insert(account.id.clone(), entry);
        AccountCreation::Created
    }

    pub(super) fn by_google(&self, subject: &str) -> Option<Account> {
        self.by_id(self.google.get(subject)?).cloned()
    }

    pub(super) fn google_link(&self, account_id: &str) -> Option<GoogleLink> {
        self.accounts.get(account_id)?.google.clone()
    }

    pub(super) fn link_google(&mut self, account_id: &str, google: &GoogleLink) -> GoogleLinking {
        if !self.accounts.contains_key(account_id) {
            return GoogleLinking::NoSuchAccount;
        }
        if self
            .google
            .get(&google.subject)
            .is_some_and(|linked| linked != account_id)
        {
            return GoogleLinking::TakenByAnother;
        }
        let Some(entry) = self.accounts.get_mut(account_id) else {
            return GoogleLinking::NoSuchAccount;
        };
        let linked = match entry.google.take() {
            Some(old) if old.subject == google.subject => GoogleLink {
                email: google.email.clone(),
                ..old
            },
            Some(old) => {
                self.google.remove(&old.subject);
                google.clone()
            }
            None => google.clone(),
        };
        self.google
            .insert(linked.subject.clone(), account_id.to_owned());
        entry.google = Some(linked);
        GoogleLinking::Linked
    }

    pub(super) fn unlink_google(&mut self, account_id: &str) -> bool {
        let Some(old) = self
            .accounts
            .get_mut(account_id)
            .and_then(|entry| entry.google.take())
        else {
            return false;
        };
        self.google.remove(&old.subject);
        true
    }

    pub(super) fn has_password(&self, account_id: &str) -> bool {
        self.accounts
            .get(account_id)
            .is_some_and(|entry| entry.password_hash.is_some())
    }

    pub(super) fn by_id(&self, id: &str) -> Option<&Account> {
        self.accounts.get(id).map(|entry| &entry.account)
    }

    pub(super) fn by_handle(&self, handle: &str) -> Option<&Account> {
        self.by_id(self.handles.get(handle)?)
    }

    pub(super) fn password_hash(&self, handle: &str) -> Option<(Account, String)> {
        let entry = self.accounts.get(self.handles.get(handle)?)?;
        Some((entry.account.clone(), entry.password_hash.clone()?))
    }

    pub(super) fn set_password(&mut self, account_id: &str, hash: &str, keep_session: &str) {
        let Some(entry) = self.accounts.get_mut(account_id) else {
            return;
        };
        // An account made with Google gets no password this way: the
        // credential that is changed must be there.
        if let Some(password) = &mut entry.password_hash {
            hash.clone_into(password);
        }
        self.sessions
            .retain(|id, (session, _)| session.account_id != account_id || id == keep_session);
    }

    /// Removes the account, its sessions and its handle. Its flights are
    /// the caller's to remove.
    pub(super) fn delete(&mut self, account_id: &str) {
        if let Some(entry) = self.accounts.remove(account_id) {
            self.handles.remove(&entry.account.handle);
            if let Some(google) = entry.google {
                self.google.remove(&google.subject);
            }
        }
        self.delete_sessions(account_id);
        self.tokens
            .retain(|_, (token, _)| token.account_id != account_id);
        self.oauth
            .codes
            .retain(|_, entry| entry.code.account_id != account_id);
        self.forget_refresh_tokens_without_a_token();
    }

    /// Whether there is an account with this identifier.
    pub(super) fn has(&self, account_id: &str) -> bool {
        self.accounts.contains_key(account_id)
    }

    pub(super) fn create_access_token(
        &mut self,
        token: &AccessToken,
        secret: &SecretHash,
        most: usize,
    ) -> bool {
        let held = self.access_tokens_of(&token.account_id).len();
        if held >= most || token.client.is_some() || !self.accounts.contains_key(&token.account_id)
        {
            return false;
        }
        self.tokens
            .insert(token.id.clone(), (token.clone(), *secret));
        true
    }

    pub(super) fn use_access_token(
        &mut self,
        secret: &SecretHash,
        now: Timestamp,
    ) -> Option<(AccessToken, Account)> {
        let (token, _) = self
            .tokens
            .values_mut()
            .find(|(_, stored)| stored == secret)?;
        if token.expires_at.is_some_and(|end| end <= now) {
            return None;
        }
        if token
            .client
            .as_ref()
            .is_some_and(|client| client.access_until <= now)
        {
            return None;
        }
        let today = now.date_naive();
        if token.last_used_on.is_none_or(|day| day < today) {
            token.last_used_on = Some(today);
        }
        let token = token.clone();
        let account = self.by_id(&token.account_id)?.clone();
        Some((token, account))
    }

    /// An account's tokens, the newest first.
    pub(super) fn access_tokens_of(&self, account_id: &str) -> Vec<AccessToken> {
        let mut tokens: Vec<AccessToken> = self
            .tokens
            .values()
            .filter(|(token, _)| token.account_id == account_id)
            .map(|(token, _)| token.clone())
            .collect();
        tokens.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
        tokens
    }

    pub(super) fn delete_access_token(&mut self, account_id: &str, token_id: &str) -> bool {
        let owned = self
            .tokens
            .get(token_id)
            .is_some_and(|(token, _)| token.account_id == account_id);
        if owned {
            self.tokens.remove(token_id);
            self.forget_refresh_tokens_without_a_token();
        }
        owned
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

    /// Keeps a picture for an account, in place of the one it has; one
    /// from Google only where it has none. Whether the account is there.
    pub(super) fn set_picture(&mut self, new: &NewPicture) -> bool {
        let Some(entry) = self.accounts.get_mut(&new.account_id) else {
            return false;
        };
        if new.origin == PictureOrigin::Google && entry.picture.is_some() {
            return true;
        }
        entry.account.picture = Some(new.id.clone());
        entry.picture = Some(Picture {
            id: new.id.clone(),
            account_id: new.account_id.clone(),
            bytes: new.bytes.clone(),
            origin: new.origin,
            created_at: new.created_at,
        });
        true
    }

    /// Removes an account's picture, and returns it.
    pub(super) fn remove_picture(&mut self, account_id: &str) -> Option<Picture> {
        let entry = self.accounts.get_mut(account_id)?;
        entry.account.picture = None;
        entry.picture.take()
    }

    /// The picture with this identifier.
    pub(super) fn picture(&self, id: &str) -> Option<Picture> {
        self.accounts
            .values()
            .filter_map(|entry| entry.picture.as_ref())
            .find(|picture| picture.id == id)
            .cloned()
    }

    /// Notes that the picture of an account's Google account is tried, and
    /// whether this is the time to: the first, with no picture there.
    pub(super) fn claim_google_picture(&mut self, account_id: &str) -> bool {
        let Some(entry) = self.accounts.get_mut(account_id) else {
            return false;
        };
        let first = !entry.google_picture_tried && entry.picture.is_none();
        entry.google_picture_tried |= first;
        first
    }

    /// Whether a flight an account uploads is public from the start;
    /// `false` for an account that is not there.
    pub(super) fn new_flights_public(&self, account_id: &str) -> bool {
        self.by_id(account_id)
            .is_some_and(|account| account.new_flights_public)
    }

    /// Changes the settings `settings` names, and returns the account as
    /// it then is. `None` for an account that is not there.
    pub(super) fn update_settings(
        &mut self,
        account_id: &str,
        settings: &AccountSettings,
    ) -> Option<Account> {
        let account = &mut self.accounts.get_mut(account_id)?.account;
        if let Some(public) = settings.new_flights_public {
            account.new_flights_public = public;
        }
        if let Some(country) = &settings.country {
            account.country.clone_from(country);
        }
        if let Some(class) = settings.glider_class {
            account.glider_class = class;
        }
        if let Some(shown) = settings.shown_near_places {
            account.shown_near_places = shown;
        }
        Some(account.clone())
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

    /// Marks an account as trusted with the sites since `now`, or leaves
    /// the mark it has, and returns the moment it then carries. `None` for
    /// an account that is not there.
    pub(super) fn let_edit_sites(&mut self, account_id: &str, now: Timestamp) -> Option<Timestamp> {
        let entry = self.accounts.get_mut(account_id)?;
        Some(*entry.account.edits_sites_since.get_or_insert(now))
    }

    /// Takes the mark off, and says whether there was one. `None` for an
    /// account that is not there.
    pub(super) fn stop_editing_sites(&mut self, account_id: &str) -> Option<bool> {
        let entry = self.accounts.get_mut(account_id)?;
        Some(entry.account.edits_sites_since.take().is_some())
    }

    /// The accounts that are trusted with the sites, by handle.
    pub(super) fn site_editors(&self) -> Vec<Account> {
        let mut editors: Vec<Account> = self
            .accounts
            .values()
            .map(|entry| &entry.account)
            .filter(|account| account.edits_sites_since.is_some())
            .cloned()
            .collect();
        editors.sort_by(|a, b| a.handle.cmp(&b.handle));
        editors
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
