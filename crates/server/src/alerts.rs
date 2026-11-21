//! Checking the alarms pilots set, where the data arrives.
//!
//! The rules are in `venti-api-types` ([`WindAlert::fits`],
//! [`DayAtLeast::met_by`]); this is when they are asked and what is
//! written down. Both checks are deterministic and cheap: a run of
//! stations reads the alarms of its stations by an index, a run of the
//! thermal map reads the day alarms once and groups them by the cell
//! their place lies in. Neither runs on a request: the collector and the
//! map call them after they have stored what arrived, and what goes wrong
//! in here ends as a line in the log.
//!
//! - **Wind.** Of each station in a run, its newest reading, if it is no
//!   older than [`ALERT_READING_FRESH_MINUTES`]. An alarm fires when the
//!   reading fits and the reading is at least [`ALERT_REST_HOURS`] after
//!   the one that fired it last.
//! - **A day.** For each day of the map that the alarm looks at, counted
//!   from today by [`alert_day`]: the alarm fires when the day is at least
//!   what it asks and the map had not said so since; a run that says less
//!   of the day forgets that it said so, and a later one that says so
//!   again fires again. A cell with no verdict for the day changes
//!   nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{Duration, NaiveDate};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use venti_api_types::v1::{
    ALERT_READING_FRESH_MINUTES, ALERT_REST_HOURS, AlertReading, AlertRule, DayAlert, FiredAlert,
    FiredBecause, ThermalMapResponse, alert_day,
};
use venti_domain::{Observation, StationId, Timestamp};
use venti_store::{Alert, AlertCheck, DAY_ALERT, Store};

use crate::clock::Clock;

/// What the collector and the thermal map tell of what arrived.
pub trait Watch: Send + Sync {
    /// The readings of a run of stations, once they are stored.
    fn readings<'a>(&'a self, observations: &'a [Observation]) -> BoxFuture<'a, ()>;

    /// A map the thermal map made with a new run.
    fn map(&self, map: Arc<ThermalMapResponse>) -> BoxFuture<'_, ()>;
}

/// The checks, over a store.
pub struct Checks<S> {
    store: Arc<S>,
    clock: Arc<dyn Clock>,
}

/// What the server notes of an alarm between two checks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    /// For a wind alarm: the time of the reading that fired it last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last: Option<Timestamp>,
    /// For a day alarm: the days a run said are what the alarm asks,
    /// since no run said otherwise.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    said: BTreeSet<NaiveDate>,
}

impl State {
    fn of(alert: &Alert) -> Self {
        // The server's own writing; empty after a change.
        serde_json::from_str(&alert.state).unwrap_or_default()
    }

    fn text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn whole(value: f64) -> u16 {
    // Bounded first: a reading is never this large.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = value.round().clamp(0.0, f64::from(u16::MAX)) as u16;
    whole
}

impl<S: Store> Checks<S> {
    /// The checks over `store`, by `clock`.
    pub fn new(store: Arc<S>, clock: Arc<dyn Clock>) -> Self {
        Self { store, clock }
    }

    /// Checks the wind alarms of the stations of a run against each
    /// station's newest reading. Returns how many fired.
    pub async fn after_readings(&self, observations: &[Observation]) -> usize {
        let now = self.clock.now();
        let fresh = now - Duration::minutes(ALERT_READING_FRESH_MINUTES);
        let mut newest: BTreeMap<&StationId, &Observation> = BTreeMap::new();
        for observation in observations {
            if observation.time < fresh || observation.time > now + Duration::minutes(10) {
                continue;
            }
            let held = newest.entry(&observation.station).or_insert(observation);
            if observation.time > held.time {
                *held = observation;
            }
        }
        if newest.is_empty() {
            return 0;
        }
        let ids: Vec<String> = newest.keys().map(ToString::to_string).collect();
        let alerts = match self.store.wind_alerts_at(&ids).await {
            Ok(alerts) => alerts,
            Err(error) => {
                tracing::warn!("the wind alarms could not be read: {error}");
                return 0;
            }
        };
        let mut fired = 0;
        for alert in alerts {
            let Ok(AlertRule::Wind(rule)) = serde_json::from_str::<AlertRule>(&alert.rule) else {
                continue;
            };
            let Some(reading) = newest.get(&rule.station) else {
                continue;
            };
            let mut state = State::of(&alert);
            if state
                .last
                .is_some_and(|last| reading.time < last + Duration::hours(ALERT_REST_HOURS))
            {
                continue;
            }
            let kmh = |ms: Option<f64>| ms.map(|ms| ms * 3.6);
            let read = AlertReading {
                time: reading.time,
                speed_kmh: kmh(reading.wind_speed_ms),
                gust_kmh: kmh(reading.wind_gust_ms),
                direction_deg: reading.wind_direction_deg,
            };
            if !rule.fits(&read) {
                continue;
            }
            let because = FiredBecause::Wind {
                time: reading.time,
                speed_kmh: read.speed_kmh.map_or(0, whole),
                gust_kmh: read.gust_kmh.map(whole),
                direction_deg: read
                    .direction_deg
                    .map(|deg| whole(deg.rem_euclid(360.0)) % 360),
            };
            state.last = Some(reading.time);
            if self
                .note(&alert, AlertRule::Wind(rule), &state, vec![because], now)
                .await
            {
                fired += 1;
            }
        }
        fired
    }

    /// Checks the day alarms against a map the thermal map made. Returns
    /// how many fired.
    pub async fn after_map(&self, map: &ThermalMapResponse) -> usize {
        let now = self.clock.now();
        let today = alert_day(now);
        let alerts = match self.store.alerts_of_kind(DAY_ALERT).await {
            Ok(alerts) => alerts,
            Err(error) => {
                tracing::warn!("the day alarms could not be read: {error}");
                return 0;
            }
        };
        // By the cell their place lies in: one look at a cell's days for
        // all the alarms of the cell.
        let mut by_cell: BTreeMap<usize, Vec<(Alert, DayAlert)>> = BTreeMap::new();
        for alert in alerts {
            let Ok(AlertRule::Day(rule)) = serde_json::from_str::<AlertRule>(&alert.rule) else {
                continue;
            };
            if let Some(cell) = map.grid.cell_at(rule.place) {
                by_cell.entry(cell).or_default().push((alert, rule));
            }
        }
        let mut fired = 0;
        for (cell, alarms) in by_cell {
            for (alert, rule) in alarms {
                let before = State::of(&alert);
                let mut state = before.clone();
                state.said.retain(|day| *day >= today);
                let mut because = Vec::new();
                for day in &map.days {
                    let ahead = (day.date - today).num_days();
                    let looked_at =
                        u8::try_from(ahead).is_ok_and(|ahead| rule.days_ahead.contains(&ahead));
                    if !looked_at {
                        continue;
                    }
                    let Some(verdict) = day.verdict.get(cell).copied().flatten() else {
                        continue;
                    };
                    let thermals = day.thermals.get(cell).copied().flatten();
                    if !rule.at_least.met_by(verdict, thermals) {
                        state.said.remove(&day.date);
                    } else if state.said.insert(day.date) {
                        because.push(FiredBecause::Day {
                            date: day.date,
                            verdict,
                            thermals,
                            model: day.model.clone().unwrap_or_else(|| map.model.clone()),
                            run: day.run.unwrap_or(map.run),
                        });
                    }
                }
                if state == before && because.is_empty() {
                    continue;
                }
                let count = because.len();
                if self
                    .note(&alert, AlertRule::Day(rule), &state, because, now)
                    .await
                {
                    fired += count;
                }
            }
        }
        fired
    }

    /// Writes down an alarm's new state, and what it fired. Whether it was
    /// written.
    async fn note(
        &self,
        alert: &Alert,
        rule: AlertRule,
        state: &State,
        because: Vec<FiredBecause>,
        now: Timestamp,
    ) -> bool {
        let records: Vec<String> = because
            .into_iter()
            .map(|because| {
                let fired = FiredAlert {
                    alert_id: alert.id.clone(),
                    alert_name: alert.name.clone(),
                    fired_at: now,
                    rule: rule.clone(),
                    because,
                };
                serde_json::to_string(&fired).unwrap_or_default()
            })
            .collect();
        let mut check = AlertCheck {
            account_id: alert.account_id.clone(),
            alert_id: alert.id.clone(),
            changed_at: alert.changed_at,
            state: state.text(),
            fired: None,
        };
        if records.is_empty() {
            return self.write(&check).await;
        }
        let mut written = true;
        for record in records {
            check.fired = Some((now, record));
            written &= self.write(&check).await;
        }
        written
    }

    async fn write(&self, check: &AlertCheck) -> bool {
        match self.store.note_alert_check(check).await {
            Ok(written) => written,
            Err(error) => {
                tracing::warn!("what an alarm's check found could not be written: {error}");
                false
            }
        }
    }
}

impl<S: Store + 'static> Watch for Checks<S> {
    fn readings<'a>(&'a self, observations: &'a [Observation]) -> BoxFuture<'a, ()> {
        async move {
            let fired = self.after_readings(observations).await;
            if fired > 0 {
                tracing::info!("{fired} wind alarms fired");
            }
        }
        .boxed()
    }

    fn map(&self, map: Arc<ThermalMapResponse>) -> BoxFuture<'_, ()> {
        async move {
            let fired = self.after_map(&map).await;
            if fired > 0 {
                tracing::info!("{fired} day alarms fired");
            }
        }
        .boxed()
    }
}
