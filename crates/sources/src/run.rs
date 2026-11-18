use std::collections::{BTreeMap, BTreeSet};

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use venti_domain::{Observation, SourceId, Station, StationId, Timestamp};

use crate::validate::{check_observation, check_station};
use crate::{
    Archive, ArchiveEntry, Fetch, ObservationSource, PlanCtx, Rejection, Request, Skipped,
};

/// How a run should behave.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// Observations older than this are dropped. Providers often republish the
    /// whole day; the caller usually only wants what is new.
    pub since: Option<Timestamp>,
    /// Requests made at the same time. Capped at the provider's per-second
    /// limit when it has one.
    pub concurrency: usize,
    /// Most requests the run may make when the provider publishes no hourly
    /// limit. When it does, the manifest's own budget applies instead.
    pub max_requests: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            since: None,
            concurrency: 8,
            max_requests: 2000,
        }
    }
}

/// A request that produced nothing usable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// The adapter's label for the request.
    pub tag: String,
    /// The URL.
    pub url: String,
    /// What went wrong.
    pub error: String,
}

/// What happened in a run. This is the source's health record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunReport {
    /// The source that ran.
    pub source: SourceId,
    /// Requests made.
    pub requests: usize,
    /// Requests that failed, or whose response could not be parsed or archived.
    pub failures: Vec<Failure>,
    /// Pieces of responses the adapter could not convert.
    pub skipped: Vec<Skipped>,
    /// Values removed by validation.
    pub rejected: Vec<Rejection>,
    /// Fields on which two partial observations disagreed.
    pub merge_conflicts: usize,
    /// Whether the run stopped early because it used up its request budget.
    pub budget_exhausted: bool,
    /// Stations delivered.
    pub stations: usize,
    /// Observations delivered.
    pub observations: usize,
    /// The time of the newest observation delivered.
    pub newest_observation: Option<Timestamp>,
}

/// The canonical records a run produced, and its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutput {
    /// Stations, sorted by identifier.
    pub stations: Vec<Station>,
    /// Observations, sorted by station and time.
    pub observations: Vec<Observation>,
    /// What happened.
    pub report: RunReport,
}

/// Runs one observation source once: fetch, archive, parse, merge, validate.
///
/// A failing request or an unparseable response is recorded in the report and
/// does not stop the run. The function reads no clock; `ctx.now` is the time
/// of the run.
///
/// Limits enforced here: the per-run request budget derived from the
/// provider's hourly limit, and a concurrency cap at its per-second limit.
/// Spacing runs by the source's cadence is the scheduler's job.
pub async fn run_observations(
    source: &dyn ObservationSource,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
) -> RunOutput {
    let manifest = source.manifest();
    let budget = manifest.requests_per_run().unwrap_or(options.max_requests);
    let per_second = manifest
        .rate_limit
        .per_second
        .map_or(usize::MAX, |n| n as usize);
    let concurrency = options.concurrency.min(per_second).max(1);

    let mut requests = 0;
    let mut budget_exhausted = false;
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    let mut stations: BTreeMap<StationId, Station> = BTreeMap::new();
    let mut partials: Vec<Observation> = Vec::new();

    let mut wave = source.plan(ctx);
    while !wave.is_empty() {
        let allowed = budget - requests;
        if wave.len() > allowed {
            wave.truncate(allowed);
            budget_exhausted = true;
        }
        requests += wave.len();

        let mut responses: Vec<(Request, _)> = stream::iter(wave)
            .map(|request| async move {
                let result = fetch.get(&request).await;
                (request, result)
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;
        // Responses arrive in any order; sort so a run is reproducible.
        responses.sort_by(|a, b| a.0.cmp(&b.0));

        let mut next = Vec::new();
        for (request, result) in responses {
            let fail = |error: String| Failure {
                tag: request.tag.clone(),
                url: request.url.clone(),
                error,
            };
            let body = match result {
                Ok(body) => body,
                Err(error) => {
                    failures.push(fail(error.to_string()));
                    continue;
                }
            };
            let entry = ArchiveEntry {
                source: &manifest.id,
                request: &request,
                fetched_at: ctx.now,
                body: &body,
            };
            if let Err(error) = archive.store(entry) {
                failures.push(fail(error.to_string()));
            }
            match source.parse(&request, &body, ctx) {
                Ok(parsed) => {
                    for station in parsed.stations {
                        stations.entry(station.id.clone()).or_insert(station);
                    }
                    partials.extend(parsed.observations);
                    skipped.extend(parsed.skipped);
                    next.extend(parsed.follow_up);
                }
                Err(error) => failures.push(fail(error.to_string())),
            }
        }
        if budget_exhausted {
            break;
        }
        wave = next;
    }

    let mut rejected = Vec::new();
    stations.retain(|_, station| ctx.area.contains(station.position));
    for station in stations.values_mut() {
        rejected.extend(check_station(station));
    }

    let known: BTreeSet<&StationId> = stations.keys().collect();
    let mut merged: BTreeMap<(StationId, Timestamp), Observation> = BTreeMap::new();
    let mut merge_conflicts = 0;
    let mut orphans: BTreeSet<StationId> = BTreeSet::new();
    for partial in partials {
        if !known.contains(&partial.station) {
            orphans.insert(partial.station);
            continue;
        }
        if options.since.is_some_and(|since| partial.time < since) {
            continue;
        }
        let key = (partial.station.clone(), partial.time);
        match merged.get_mut(&key) {
            Some(existing) => merge_conflicts += existing.merge(&partial),
            None => {
                merged.insert(key, partial);
            }
        }
    }
    for station in orphans {
        skipped.push(Skipped::new(
            &station.to_string(),
            "observations for a station that is not in the station list or lies outside the area",
        ));
    }

    let mut observations = Vec::with_capacity(merged.len());
    for mut observation in merged.into_values() {
        rejected.extend(check_observation(&mut observation, ctx.now));
        if !observation.is_empty() {
            observations.push(observation);
        }
    }

    let stations: Vec<Station> = stations.into_values().collect();
    let report = RunReport {
        source: manifest.id.clone(),
        requests,
        failures,
        skipped,
        rejected,
        merge_conflicts,
        budget_exhausted,
        stations: stations.len(),
        observations: observations.len(),
        newest_observation: observations.iter().map(|o| o.time).max(),
    };
    RunOutput {
        stations,
        observations,
        report,
    }
}
