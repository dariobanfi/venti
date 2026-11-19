use std::collections::{BTreeMap, BTreeSet};

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use venti_domain::{Airspace, AirspaceId, Observation, SourceId, Station, StationId, Timestamp};

use crate::validate::{check_observation, check_station};
use crate::{
    AirspaceSource, Archive, ArchiveEntry, Fetch, Fetched, Manifest, ObservationSource, PlanCtx,
    Rejection, Request, Skipped, SourceError,
};

/// How a run should behave.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// Observations older than this are dropped. Providers often republish the
    /// whole day; the caller usually only wants what is new. Airspace runs
    /// ignore it: they deliver the whole set every time.
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

impl RunReport {
    /// Whether the run did its job: it delivered observations, nearly every
    /// request worked, and it did not run out of request budget.
    ///
    /// A few stations are always briefly offline, so up to a tenth of the
    /// requests may fail. A provider-wide problem shows as many failures or
    /// as nothing delivered at all. The scheduler backs off after an
    /// unhealthy run, the health endpoint counts it as a failure, and the
    /// canary fails on it.
    pub fn is_healthy(&self) -> bool {
        let tolerated_failures = self.requests / 10;
        self.observations > 0 && self.failures.len() <= tolerated_failures && !self.budget_exhausted
    }
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

/// What the fetching part of a run did.
struct Fetching {
    requests: usize,
    budget_exhausted: bool,
    failures: Vec<Failure>,
}

/// Fetches and archives what an adapter asks for, wave by wave.
///
/// Every response is handed to `take`, which parses it, keeps what it found
/// and returns the adapter's follow-up requests. A failing request, a payload
/// the archive refuses or a body `take` cannot parse is recorded and does
/// not stop the run.
///
/// Limits enforced here, for every kind of source: the per-run request
/// budget derived from the provider's hourly limit, and a concurrency cap
/// at its per-second limit.
async fn fetch_all(
    manifest: &Manifest,
    first: Vec<Request>,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
    mut take: impl FnMut(&Request, &Fetched) -> Result<Vec<Request>, SourceError>,
) -> Fetching {
    let budget = manifest.requests_per_run().unwrap_or(options.max_requests);
    let per_second = manifest
        .rate_limit
        .per_second
        .map_or(usize::MAX, |n| n as usize);
    let concurrency = options.concurrency.min(per_second).max(1);

    let mut requests = 0;
    let mut budget_exhausted = false;
    let mut failures = Vec::new();

    let mut wave = first;
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
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    failures.push(fail(error.to_string()));
                    continue;
                }
            };
            let entry = ArchiveEntry {
                source: &manifest.id,
                request: &request,
                fetched_at: ctx.now,
                body: &response.body,
            };
            if let Err(error) = archive.store(entry) {
                failures.push(fail(error.to_string()));
            }
            match take(&request, &response) {
                Ok(follow_up) => next.extend(follow_up),
                Err(error) => failures.push(fail(error.to_string())),
            }
        }
        if budget_exhausted {
            break;
        }
        wave = next;
    }

    Fetching {
        requests,
        budget_exhausted,
        failures,
    }
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
    let mut skipped = Vec::new();
    let mut stations: BTreeMap<StationId, Station> = BTreeMap::new();
    let mut partials: Vec<Observation> = Vec::new();

    let take = |request: &Request, response: &Fetched| {
        let parsed = source.parse(request, &response.body, ctx)?;
        for station in parsed.stations {
            stations.entry(station.id.clone()).or_insert(station);
        }
        partials.extend(parsed.observations);
        skipped.extend(parsed.skipped);
        Ok(parsed.follow_up)
    };
    let fetched = fetch_all(
        manifest,
        source.plan(ctx),
        fetch,
        archive,
        ctx,
        options,
        take,
    )
    .await;

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
        requests: fetched.requests,
        failures: fetched.failures,
        skipped,
        rejected,
        merge_conflicts,
        budget_exhausted: fetched.budget_exhausted,
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

/// An airspace the pipeline refused to pass on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AirspaceRejection {
    /// The airspace's identifier, as text.
    pub airspace: String,
    /// Its name.
    pub name: String,
    /// Why.
    pub reason: String,
}

/// What happened in an airspace run. This is the source's health record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceRunReport {
    /// The source that ran.
    pub source: SourceId,
    /// Requests made.
    pub requests: usize,
    /// Requests that failed, or whose response could not be parsed or archived.
    pub failures: Vec<Failure>,
    /// Airspaces, or parts of responses, the adapter could not convert.
    pub skipped: Vec<Skipped>,
    /// Airspaces removed by validation.
    pub rejected: Vec<AirspaceRejection>,
    /// Airspaces dropped because an earlier one had the same identifier.
    pub duplicates: usize,
    /// Whether the run stopped early because it used up its request budget.
    pub budget_exhausted: bool,
    /// Airspaces delivered.
    pub airspaces: usize,
    /// When the source last changed any of the airspaces delivered, if it says.
    pub newest_change: Option<Timestamp>,
}

impl AirspaceRunReport {
    /// Whether the set this run delivered may replace the one before.
    ///
    /// Stricter than for observations: every request must have worked. A
    /// run that could not fetch one country's file would otherwise replace
    /// a complete set by one with a country missing, and a pilot is not
    /// warned about airspace that is missing. Whoever stores the result
    /// keeps the previous set after an unhealthy run.
    ///
    /// Airspaces the adapter skipped or validation rejected do not make a
    /// run unhealthy, or one bad record would freeze the whole set. They
    /// are listed in the report, and a server must make them visible.
    pub fn is_healthy(&self) -> bool {
        self.airspaces > 0 && self.failures.is_empty() && !self.budget_exhausted
    }
}

/// The canonical records an airspace run produced, and its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirspaceRunOutput {
    /// Airspaces, sorted by identifier.
    pub airspaces: Vec<Airspace>,
    /// What happened.
    pub report: AirspaceRunReport,
}

/// Runs one airspace source once: fetch, archive, parse, validate.
///
/// The result is a snapshot: every airspace the source has that reaches
/// into `ctx.area`, sorted by identifier. An airspace that fails
/// [`Airspace::validate`] is removed and reported, as is a second airspace
/// with an identifier already seen. Requests are budgeted and archived as
/// for observations. `options.since` plays no part.
///
/// Every airspace leaves with the date of the file it was read from
/// ([`Validity::file_date`](venti_domain::Validity)). Where the adapter
/// found none in the file, it is the date the provider's server put on the
/// response ([`Fetched::last_modified`](crate::Fetched)), and where the
/// server gave none, the time of the run.
pub async fn run_airspace(
    source: &dyn AirspaceSource,
    fetch: &impl Fetch,
    archive: &impl Archive,
    ctx: &PlanCtx,
    options: &RunOptions,
) -> AirspaceRunOutput {
    let manifest = source.manifest();
    let mut skipped = Vec::new();
    let mut found: Vec<Airspace> = Vec::new();

    let take = |request: &Request, response: &Fetched| {
        let parsed = source.parse(request, &response.body, ctx)?;
        // An airspace is as old as the file it was read from. An adapter
        // whose files carry their date inside sets it; for the others it is
        // what the provider's server says about the file, and failing that
        // the time of this run.
        let file_date = response.last_modified.unwrap_or(ctx.now);
        found.extend(parsed.airspaces.into_iter().map(|mut airspace| {
            airspace.validity.file_date.get_or_insert(file_date);
            airspace
        }));
        skipped.extend(parsed.skipped);
        Ok(parsed.follow_up)
    };
    let fetched = fetch_all(
        manifest,
        source.plan(ctx),
        fetch,
        archive,
        ctx,
        options,
        take,
    )
    .await;

    let mut rejected = Vec::new();
    let mut duplicates = 0;
    let mut airspaces: BTreeMap<AirspaceId, Airspace> = BTreeMap::new();
    for airspace in found {
        let bounds = airspace.bounds();
        let reaches_in = bounds.north() >= ctx.area.south()
            && bounds.south() <= ctx.area.north()
            && bounds.east() >= ctx.area.west()
            && bounds.west() <= ctx.area.east();
        if !reaches_in {
            continue;
        }
        if let Err(error) = airspace.validate() {
            rejected.push(AirspaceRejection {
                airspace: airspace.id.to_string(),
                name: airspace.name.clone(),
                reason: error.to_string(),
            });
            continue;
        }
        if airspaces.contains_key(&airspace.id) {
            duplicates += 1;
            continue;
        }
        airspaces.insert(airspace.id.clone(), airspace);
    }

    let airspaces: Vec<Airspace> = airspaces.into_values().collect();
    let report = AirspaceRunReport {
        source: manifest.id.clone(),
        requests: fetched.requests,
        failures: fetched.failures,
        skipped,
        rejected,
        duplicates,
        budget_exhausted: fetched.budget_exhausted,
        airspaces: airspaces.len(),
        newest_change: airspaces
            .iter()
            .filter_map(|airspace| airspace.validity.updated)
            .max(),
    };
    AirspaceRunOutput { airspaces, report }
}
