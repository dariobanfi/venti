//! The alarms pilots set, in the in-memory store.

use std::collections::BTreeMap;

use venti_domain::Timestamp;

use crate::flight::next_change;
use crate::{AddingAlert, Alert, AlertCheck, FiredAlert, FiredAlerts, NewAlert};

#[derive(Debug, Default)]
pub(super) struct Alerts {
    /// By account, then identifier.
    alerts: BTreeMap<(String, String), Alert>,
    /// By account: the largest number used, and what fired by number.
    fired: BTreeMap<String, (i64, BTreeMap<i64, FiredAlert>)>,
}

impl Alerts {
    pub(super) fn of(&self, account_id: &str) -> Vec<Alert> {
        let mut alerts: Vec<Alert> = self
            .alerts
            .values()
            .filter(|alert| alert.account_id == account_id)
            .cloned()
            .collect();
        alerts.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        alerts
    }

    /// Adds an alarm to an account that exists.
    pub(super) fn add(
        &mut self,
        account_id: &str,
        new: &NewAlert,
        now: Timestamp,
        most: usize,
    ) -> AddingAlert {
        if self.of(account_id).len() >= most {
            return AddingAlert::Full;
        }
        let alert = Alert {
            id: new.id.clone(),
            account_id: account_id.to_owned(),
            name: new.name.clone(),
            enabled: new.enabled,
            kind: new.kind.clone(),
            station_id: new.station_id.clone(),
            rule: new.rule.clone(),
            state: String::new(),
            created_at: now,
            changed_at: now,
        };
        self.alerts
            .insert((account_id.to_owned(), new.id.clone()), alert.clone());
        AddingAlert::Added(alert)
    }

    pub(super) fn change(
        &mut self,
        account_id: &str,
        new: &NewAlert,
        now: Timestamp,
    ) -> Option<Alert> {
        let alert = self
            .alerts
            .get_mut(&(account_id.to_owned(), new.id.clone()))?;
        alert.name.clone_from(&new.name);
        alert.enabled = new.enabled;
        alert.kind.clone_from(&new.kind);
        alert.station_id.clone_from(&new.station_id);
        alert.rule.clone_from(&new.rule);
        alert.state = String::new();
        alert.changed_at = now;
        Some(alert.clone())
    }

    pub(super) fn remove(&mut self, account_id: &str, alert_id: &str) -> bool {
        self.alerts
            .remove(&(account_id.to_owned(), alert_id.to_owned()))
            .is_some()
    }

    pub(super) fn matching(&self, keep: impl Fn(&Alert) -> bool) -> Vec<Alert> {
        self.alerts
            .values()
            .filter(|alert| alert.enabled && keep(alert))
            .cloned()
            .collect()
    }

    pub(super) fn note(&mut self, check: &AlertCheck) -> bool {
        let key = (check.account_id.clone(), check.alert_id.clone());
        let Some(alert) = self
            .alerts
            .get_mut(&key)
            .filter(|alert| alert.changed_at == check.changed_at)
        else {
            return false;
        };
        alert.state.clone_from(&check.state);
        if let Some((fired_at, record)) = &check.fired {
            let (latest, fired) = self.fired.entry(check.account_id.clone()).or_default();
            *latest = next_change(*latest, *fired_at);
            fired.insert(
                *latest,
                FiredAlert {
                    number: *latest,
                    alert_id: check.alert_id.clone(),
                    fired_at: *fired_at,
                    record: record.clone(),
                },
            );
        }
        true
    }

    pub(super) fn fired(&self, account_id: &str, since: i64, limit: usize) -> FiredAlerts {
        let Some((latest, fired)) = self.fired.get(account_id) else {
            return FiredAlerts::default();
        };
        let mut after: Vec<FiredAlert> = fired
            .range(since.saturating_add(1)..)
            .map(|(_, fired)| fired.clone())
            .collect();
        let more = after.len() > limit;
        after.truncate(limit);
        FiredAlerts {
            fired: after,
            more,
            latest: *latest,
        }
    }

    pub(super) fn prune(&mut self, before: Timestamp) -> u64 {
        let mut removed = 0;
        for (_, fired) in self.fired.values_mut() {
            let had = fired.len();
            fired.retain(|_, fired| fired.fired_at >= before);
            removed += (had - fired.len()) as u64;
        }
        removed
    }

    pub(super) fn delete_account(&mut self, account_id: &str) {
        self.alerts.retain(|(account, _), _| account != account_id);
        self.fired.remove(account_id);
    }
}
