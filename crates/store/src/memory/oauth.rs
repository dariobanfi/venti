//! What the authorization server keeps, in the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use super::accounts::Accounts;
use crate::{
    AuthorizationCode, AuthorizationRequest, NewGrant, OauthClient, Redeemed, Refreshed,
    RequestState, Rotation, SecretHash, StoredCode,
};

#[derive(Debug, Default)]
pub(super) struct Oauth {
    /// The registered programs, by identifier.
    clients: BTreeMap<String, OauthClient>,
    /// The requests that wait for a pilot's answer or got one, by the
    /// hash of their identifier.
    requests: BTreeMap<SecretHash, RequestEntry>,
    /// The codes, by their hash.
    pub(super) codes: BTreeMap<SecretHash, CodeEntry>,
    /// The refresh tokens, used up or not, by their hash.
    refresh: BTreeMap<SecretHash, RefreshEntry>,
}

#[derive(Debug)]
pub(super) struct CodeEntry {
    pub(super) code: AuthorizationCode,
    used: bool,
    /// The token that came of its exchange.
    token_id: Option<String>,
}

#[derive(Debug)]
struct RequestEntry {
    request: AuthorizationRequest,
    answered: bool,
}

#[derive(Debug)]
struct RefreshEntry {
    token_id: String,
    expires_at: Timestamp,
    spent: bool,
}

impl Accounts {
    /// Removes the refresh tokens whose token is gone: what a database
    /// does by itself for a row that refers to another.
    pub(super) fn forget_refresh_tokens_without_a_token(&mut self) {
        let tokens = &self.tokens;
        self.oauth
            .refresh
            .retain(|_, entry| tokens.contains_key(&entry.token_id));
    }

    fn holds_a_grant(&self, client_id: &str) -> bool {
        self.tokens.values().any(|(token, _)| {
            token
                .client
                .as_ref()
                .is_some_and(|client| client.client_id == client_id)
        })
    }

    fn remove_client(&mut self, client_id: &str) {
        self.oauth.clients.remove(client_id);
        self.oauth
            .requests
            .retain(|_, entry| entry.request.client_id != client_id);
        self.oauth
            .codes
            .retain(|_, entry| entry.code.client_id != client_id);
        self.tokens.retain(|_, (token, _)| {
            token
                .client
                .as_ref()
                .is_none_or(|client| client.client_id != client_id)
        });
        self.forget_refresh_tokens_without_a_token();
    }

    pub(super) fn create_oauth_client(&mut self, client: &OauthClient, most: usize) -> bool {
        if self.oauth.clients.contains_key(&client.id) {
            return false;
        }
        if self.oauth.clients.len() >= most {
            let oldest_unused = self
                .oauth
                .clients
                .values()
                .filter(|stored| stored.last_used_at.is_none())
                .min_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)))
                .map(|stored| stored.id.clone());
            let Some(oldest_unused) = oldest_unused else {
                return false;
            };
            self.remove_client(&oldest_unused);
        }
        self.oauth.clients.insert(client.id.clone(), client.clone());
        true
    }

    pub(super) fn oauth_client(&self, client_id: &str) -> Option<OauthClient> {
        self.oauth.clients.get(client_id).cloned()
    }

    pub(super) fn create_authorization_request(
        &mut self,
        request: &SecretHash,
        record: &AuthorizationRequest,
        now: Timestamp,
        most: usize,
    ) -> bool {
        self.oauth
            .requests
            .retain(|_, entry| entry.request.expires_at > now);
        if self.oauth.requests.len() >= most
            || self.oauth.requests.contains_key(request)
            || !self.oauth.clients.contains_key(&record.client_id)
        {
            return false;
        }
        let entry = RequestEntry {
            request: record.clone(),
            answered: false,
        };
        self.oauth.requests.insert(*request, entry);
        true
    }

    pub(super) fn authorization_request(
        &self,
        request: &SecretHash,
        now: Timestamp,
    ) -> RequestState {
        match self.oauth.requests.get(request) {
            Some(entry) if entry.request.expires_at <= now => RequestState::Unknown,
            Some(entry) if entry.answered => RequestState::Answered,
            Some(entry) => RequestState::Waiting(entry.request.clone()),
            None => RequestState::Unknown,
        }
    }

    pub(super) fn answer_authorization_request(
        &mut self,
        request: &SecretHash,
        now: Timestamp,
    ) -> RequestState {
        match self.oauth.requests.get_mut(request) {
            Some(entry) if entry.request.expires_at <= now => RequestState::Unknown,
            Some(entry) if entry.answered => RequestState::Answered,
            Some(entry) => {
                entry.answered = true;
                RequestState::Waiting(entry.request.clone())
            }
            None => RequestState::Unknown,
        }
    }

    pub(super) fn create_authorization_code(
        &mut self,
        code: &SecretHash,
        record: &AuthorizationCode,
    ) -> bool {
        if !self.has(&record.account_id) || self.oauth.codes.contains_key(code) {
            return false;
        }
        let Some(client) = self.oauth.clients.get_mut(&record.client_id) else {
            return false;
        };
        client.last_used_at = Some(record.created_at);
        let entry = CodeEntry {
            code: record.clone(),
            used: false,
            token_id: None,
        };
        self.oauth.codes.insert(*code, entry);
        true
    }

    pub(super) fn authorization_code(
        &self,
        code: &SecretHash,
        now: Timestamp,
    ) -> Option<StoredCode> {
        let entry = self.oauth.codes.get(code)?;
        (entry.code.expires_at > now).then(|| StoredCode {
            code: entry.code.clone(),
            used: entry.used,
        })
    }

    pub(super) fn redeem_authorization_code(
        &mut self,
        code: &SecretHash,
        grant: &NewGrant,
        now: Timestamp,
        most: usize,
    ) -> Redeemed {
        let Some(entry) = self.oauth.codes.get(code) else {
            return Redeemed::Unknown;
        };
        if entry.code.expires_at <= now {
            return Redeemed::Unknown;
        }
        if entry.used {
            if let Some(token_id) = entry.token_id.clone() {
                self.tokens.remove(&token_id);
                self.forget_refresh_tokens_without_a_token();
            }
            return Redeemed::Spent;
        }
        let (account_id, client_id) = (entry.code.account_id.clone(), entry.code.client_id.clone());
        let bound = |client: &Option<crate::TokenClient>| {
            client
                .as_ref()
                .is_some_and(|client| client.client_id == client_id)
        };
        if grant.token.account_id != account_id || !bound(&grant.token.client) {
            return Redeemed::Unknown;
        }
        let earlier: Vec<String> = self
            .tokens
            .values()
            .filter(|(token, _)| token.account_id == account_id && bound(&token.client))
            .map(|(token, _)| token.id.clone())
            .collect();
        let held = self.access_tokens_of(&account_id).len() - earlier.len();
        if held >= most {
            return Redeemed::Full;
        }
        for token_id in earlier {
            self.tokens.remove(&token_id);
        }
        self.forget_refresh_tokens_without_a_token();
        self.tokens
            .insert(grant.token.id.clone(), (grant.token.clone(), grant.access));
        let refresh = RefreshEntry {
            token_id: grant.token.id.clone(),
            expires_at: grant.token.expires_at.unwrap_or(now),
            spent: false,
        };
        self.oauth.refresh.insert(grant.refresh, refresh);
        if let Some(entry) = self.oauth.codes.get_mut(code) {
            entry.used = true;
            entry.token_id = Some(grant.token.id.clone());
        }
        if let Some(client) = self.oauth.clients.get_mut(&client_id) {
            client.last_used_at = Some(now);
        }
        Redeemed::Granted
    }

    pub(super) fn refresh_grant(
        &mut self,
        refresh: &SecretHash,
        client_id: &str,
        next: &Rotation,
        now: Timestamp,
    ) -> Refreshed {
        let Some(entry) = self.oauth.refresh.get(refresh) else {
            return Refreshed::Unknown;
        };
        let token_id = entry.token_id.clone();
        let of_this_client = self.tokens.get(&token_id).is_some_and(|(token, _)| {
            token
                .client
                .as_ref()
                .is_some_and(|client| client.client_id == client_id)
        });
        if !of_this_client || entry.expires_at <= now {
            return Refreshed::Unknown;
        }
        if entry.spent {
            self.tokens.remove(&token_id);
            self.forget_refresh_tokens_without_a_token();
            return Refreshed::Reused;
        }
        if let Some(entry) = self.oauth.refresh.get_mut(refresh) {
            entry.spent = true;
        }
        let fresh = RefreshEntry {
            token_id: token_id.clone(),
            expires_at: next.expires_at,
            spent: false,
        };
        self.oauth.refresh.insert(next.refresh, fresh);
        let Some((token, secret)) = self.tokens.get_mut(&token_id) else {
            return Refreshed::Unknown;
        };
        *secret = next.access;
        token.expires_at = Some(next.expires_at);
        if let Some(client) = token.client.as_mut() {
            client.access_until = next.access_until;
        }
        let token = token.clone();
        if let Some(client) = self.oauth.clients.get_mut(client_id) {
            client.last_used_at = Some(now);
        }
        match self.by_id(&token.account_id).cloned() {
            Some(account) => Refreshed::Rotated(Box::new((token, account))),
            None => Refreshed::Unknown,
        }
    }

    pub(super) fn revoke_grant(&mut self, secret: &SecretHash, client_id: &str) -> bool {
        let by_refresh = self
            .oauth
            .refresh
            .get(secret)
            .filter(|entry| !entry.spent)
            .map(|entry| entry.token_id.clone());
        let by_access = self
            .tokens
            .values()
            .find(|(_, stored)| stored == secret)
            .map(|(token, _)| token.id.clone());
        let Some(token_id) = by_refresh.or(by_access) else {
            return false;
        };
        let of_this_client = self.tokens.get(&token_id).is_some_and(|(token, _)| {
            token
                .client
                .as_ref()
                .is_some_and(|client| client.client_id == client_id)
        });
        if !of_this_client {
            return false;
        }
        self.tokens.remove(&token_id);
        self.forget_refresh_tokens_without_a_token();
        true
    }

    pub(super) fn prune_oauth(&mut self, now: Timestamp, unused_since: Timestamp) -> u64 {
        self.oauth
            .requests
            .retain(|_, entry| entry.request.expires_at > now);
        self.oauth
            .codes
            .retain(|_, entry| entry.code.expires_at > now);
        let before = self.tokens.len();
        self.tokens.retain(|_, (token, _)| {
            token.client.is_none() || token.expires_at.is_none_or(|end| end > now)
        });
        let ended = before - self.tokens.len();
        self.forget_refresh_tokens_without_a_token();
        self.oauth.refresh.retain(|_, entry| entry.expires_at > now);
        let idle: Vec<String> = self
            .oauth
            .clients
            .values()
            .filter(|client| client.last_used_at.unwrap_or(client.created_at) < unused_since)
            .filter(|client| !self.holds_a_grant(&client.id))
            .map(|client| client.id.clone())
            .collect();
        for client_id in &idle {
            self.remove_client(client_id);
        }
        (ended + idle.len()) as u64
    }
}
