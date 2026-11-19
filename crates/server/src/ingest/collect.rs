use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use chrono::Duration;
use futures::FutureExt;
use venti_domain::{Airspace, Observation, SourceId, Station, Timestamp};
use venti_sources::{
    AirspaceRunReport, Archive, Fetch, Kind, PlanCtx, Registry, RunOptions, RunReport,
    run_airspace, run_observations,
};
use venti_store::{Coverage, RunRecord, Store};

use super::{airspace_summary, summary};
use crate::clock::Clock;
use crate::plan::Job;

/// How many of a run's problems are written to the log one by one. The run
/// history has the counts.
const LOGGED_PROBLEMS: usize = 5;

/// Runs sources through the pipeline and puts what they deliver in the store.
///
/// This is the one place where the pipeline, the archive and the store meet.
/// The scheduler calls it at each source's cadence; the development mode
/// calls it once per source over recorded payloads.
pub struct Collector<S, F, A> {
    registry: Registry,
    store: Arc<S>,
    fetch: F,
    archive: A,
    clock: Arc<dyn Clock>,
    lookback: Option<Duration>,
}

impl<S: Store, F: Fetch, A: Archive> Collector<S, F, A> {
    /// A collector for the adapters in `registry`.
    ///
    /// `lookback` says how far back from the time of a run observations are
    /// taken; older ones in a payload are dropped. Providers republish hours
    /// of data on every request. Looking back a few hours picks up values
    /// that arrive late (a provider's temperature file can trail its wind
    /// file by an hour) and heals a short outage of the database, without
    /// rewriting the whole day every ten minutes. `None` takes everything.
    /// Airspace has no such window: every run delivers the whole set.
    pub fn new(
        registry: Registry,
        store: Arc<S>,
        fetch: F,
        archive: A,
        clock: Arc<dyn Clock>,
        lookback: Option<Duration>,
    ) -> Self {
        Self {
            registry,
            store,
            fetch,
            archive,
            clock,
            lookback,
        }
    }

    /// The store runs are written to.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The current time, by the collector's clock.
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// How often a job's source is collected, in seconds.
    ///
    /// # Panics
    ///
    /// Panics if the registry has no adapter for the job. Jobs come from
    /// region packs that were checked against the registry when the plan
    /// was made.
    pub fn cadence_seconds(&self, job: &Job) -> u32 {
        job.manifest(&self.registry).cadence_seconds
    }

    /// Runs one source once and stores the result. Returns whether the run
    /// was healthy and reached the store.
    ///
    /// Nothing that goes wrong in here escapes: a failed request, a damaged
    /// payload, a panic in an adapter or a database that is down all end as
    /// a log line and `false`, and the caller tries again later.
    pub async fn collect(&self, job: &Job) -> bool {
        match job.kind {
            Kind::Observations => self.collect_observations(job).await,
            Kind::Airspace => self.collect_airspace(job).await,
            Kind::Forecasts | Kind::Sites => {
                // The framework knows these kinds; the server does not
                // collect them yet, and no plan names such a job.
                tracing::error!(source = %job.source, "{:?} are not collected yet", job.kind);
                false
            }
        }
    }

    async fn collect_observations(&self, job: &Job) -> bool {
        let source = self
            .registry
            .observations(&job.source)
            .unwrap_or_else(|| panic!("the plan names {}, which has no adapter", job.source));
        let started_at = self.clock.now();
        let ctx = PlanCtx {
            now: started_at,
            area: job.area,
        };
        let options = RunOptions {
            since: self.lookback.map(|lookback| started_at - lookback),
            ..RunOptions::default()
        };

        let run = run_observations(source, &self.fetch, &self.archive, &ctx, &options);
        let Some(output) = unless_it_panics(&job.source, run).await else {
            let failed = self.record(&job.source, started_at, &Outcome::default());
            self.save(&failed, &[], &[]).await;
            return false;
        };

        let report = &output.report;
        let healthy = report.is_healthy();
        if healthy {
            tracing::info!(source = %job.source, "{}", summary(report, started_at));
        } else {
            tracing::warn!(source = %job.source, "unhealthy run: {}", summary(report, started_at));
        }
        for failure in report.failures.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "failed {}: {}", failure.tag, failure.error);
        }

        let record = self.record(&job.source, started_at, &Outcome::of_observations(report));
        self.save(&record, &output.stations, &output.observations)
            .await
            && healthy
    }

    /// Runs an airspace source once. The set it delivers replaces the
    /// stored one only if the run is healthy, which for airspace means
    /// that every request worked: the store holds to that itself.
    async fn collect_airspace(&self, job: &Job) -> bool {
        let source = self
            .registry
            .airspace(&job.source)
            .unwrap_or_else(|| panic!("the plan names {}, which has no adapter", job.source));
        let started_at = self.clock.now();
        let ctx = PlanCtx {
            now: started_at,
            area: job.area,
        };
        let options = RunOptions::default();

        let run = run_airspace(source, &self.fetch, &self.archive, &ctx, &options);
        // Where the set of this run is whole: the area it was collected
        // for, for the countries the adapter fetches. It goes into the
        // store with the set and out with every answer.
        let coverage = Coverage {
            boxes: vec![job.area],
            countries: source.manifest().countries.clone(),
        };
        let Some(output) = unless_it_panics(&job.source, run).await else {
            let failed = self.record(&job.source, started_at, &Outcome::default());
            self.save_airspace(&failed, &coverage, &[]).await;
            return false;
        };

        let report = &output.report;
        let healthy = report.is_healthy();
        if healthy {
            tracing::info!(source = %job.source, "{}", airspace_summary(report));
        } else {
            tracing::warn!(
                source = %job.source,
                "unhealthy run, the stored airspace stays as it is: {}",
                airspace_summary(report)
            );
        }
        for failure in report.failures.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "failed {}: {}", failure.tag, failure.error);
        }
        // An airspace that was left out is one no pilot is warned about.
        // The run stays healthy, so the log is where it shows.
        for skipped in report.skipped.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "left out {:?}: {}", skipped.what, skipped.reason);
        }
        for rejected in report.rejected.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "rejected {:?}: {}", rejected.name, rejected.reason);
        }

        let record = self.record(&job.source, started_at, &Outcome::of_airspace(report));
        self.save_airspace(&record, &coverage, &output.airspaces)
            .await
            && healthy
    }

    fn record(&self, source: &SourceId, started_at: Timestamp, outcome: &Outcome) -> RunRecord {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        RunRecord {
            source: source.clone(),
            started_at,
            finished_at: self.clock.now(),
            healthy: outcome.healthy,
            requests: count(outcome.requests),
            failed_requests: count(outcome.failed_requests),
            skipped: count(outcome.skipped),
            rejected: count(outcome.rejected),
            stations: count(outcome.stations),
            observations: count(outcome.observations),
            newest_observation: outcome.newest_observation,
            airspaces: count(outcome.airspaces),
        }
    }

    async fn save(
        &self,
        record: &RunRecord,
        stations: &[Station],
        observations: &[Observation],
    ) -> bool {
        let saved = self.store.save_run(record, stations, observations).await;
        stored(record, saved)
    }

    async fn save_airspace(
        &self,
        record: &RunRecord,
        coverage: &Coverage,
        airspaces: &[Airspace],
    ) -> bool {
        let saved = self
            .store
            .save_airspace_run(record, coverage, airspaces)
            .await;
        stored(record, saved)
    }
}

/// Lets a run finish, unless it panics.
///
/// An adapter parses whatever a provider sends. A bug in one must not take
/// the server, or the other sources, down with it.
async fn unless_it_panics<T>(source: &SourceId, run: impl Future<Output = T>) -> Option<T> {
    let output = AssertUnwindSafe(run).catch_unwind().await.ok();
    if output.is_none() {
        tracing::error!(source = %source, "the run panicked; see the panic message above");
    }
    output
}

fn stored(record: &RunRecord, saved: Result<(), venti_store::StoreError>) -> bool {
    match saved {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(source = %record.source, "the run could not be stored: {error}");
            false
        }
    }
}

/// What a run of either kind did, as the run history keeps it. The default
/// is a run that did nothing at all: one that panicked.
#[derive(Debug, Default)]
struct Outcome {
    healthy: bool,
    requests: usize,
    failed_requests: usize,
    skipped: usize,
    rejected: usize,
    stations: usize,
    observations: usize,
    newest_observation: Option<Timestamp>,
    airspaces: usize,
}

impl Outcome {
    fn of_observations(report: &RunReport) -> Self {
        Self {
            healthy: report.is_healthy(),
            requests: report.requests,
            failed_requests: report.failures.len(),
            skipped: report.skipped.len(),
            rejected: report.rejected.len(),
            stations: report.stations,
            observations: report.observations,
            newest_observation: report.newest_observation,
            airspaces: 0,
        }
    }

    fn of_airspace(report: &AirspaceRunReport) -> Self {
        Self {
            healthy: report.is_healthy(),
            requests: report.requests,
            failed_requests: report.failures.len(),
            skipped: report.skipped.len(),
            rejected: report.rejected.len(),
            airspaces: report.airspaces,
            ..Self::default()
        }
    }
}
