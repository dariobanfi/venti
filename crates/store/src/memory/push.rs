//! Web Push subscriptions and the server's key, in the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use crate::{AddingPush, PushSubscription};

#[derive(Debug, Default)]
pub(super) struct Push {
    /// By endpoint.
    subscriptions: BTreeMap<String, PushSubscription>,
    key: Option<String>,
}

impl Push {
    pub(super) fn add(&mut self, subscription: &PushSubscription, most: usize) -> AddingPush {
        let others = self
            .subscriptions
            .values()
            .filter(|held| {
                held.account_id == subscription.account_id && held.endpoint != subscription.endpoint
            })
            .count();
        if others >= most {
            return AddingPush::Full;
        }
        self.subscriptions
            .insert(subscription.endpoint.clone(), subscription.clone());
        AddingPush::Added
    }

    pub(super) fn remove(&mut self, account_id: &str, endpoint: &str) -> bool {
        let theirs = self
            .subscriptions
            .get(endpoint)
            .is_some_and(|held| held.account_id == account_id);
        theirs && self.subscriptions.remove(endpoint).is_some()
    }

    pub(super) fn drop(&mut self, endpoint: &str) -> bool {
        self.subscriptions.remove(endpoint).is_some()
    }

    pub(super) fn of(&self, account_id: &str) -> Vec<PushSubscription> {
        let mut theirs: Vec<PushSubscription> = self
            .subscriptions
            .values()
            .filter(|held| held.account_id == account_id)
            .cloned()
            .collect();
        theirs.sort_by(|a, b| (a.created_at, &a.endpoint).cmp(&(b.created_at, &b.endpoint)));
        theirs
    }

    pub(super) fn key(&self) -> Option<String> {
        self.key.clone()
    }

    pub(super) fn keep_key(&mut self, key: &str, _now: Timestamp) -> String {
        self.key.get_or_insert_with(|| key.to_owned()).clone()
    }

    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.subscriptions
            .retain(|_, held| held.account_id != account_id);
    }
}
