use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use chrono::Duration;
use futures::FutureExt;
use venti_domain::{SourceId, Timestamp};
use venti_sources::{
    Archive, Fetch, ObservationSource, PlanCtx, Registry, RunOptions, RunReport, run_observations,
};
use venti_store::{RunRecord, Store};

use super::summary;
use crate::clock::Clock;
use crate::plan::Job;

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

    /// The adapter for a source.
    ///
    /// # Panics
    ///
    /// Panics if the registry has no such adapter. Jobs come from region
    /// packs that were checked against the registry when the plan was made.
    pub fn adapter(&self, source: &SourceId) -> &dyn ObservationSource {
        self.registry
            .observations(source)
            .unwrap_or_else(|| panic!("the plan names {source}, which has no adapter"))
    }

    /// Runs one source once and stores the result. Returns whether the run
    /// was healthy and reached the store.
    ///
    /// Nothing that goes wrong in here escapes: a failed request, a damaged
    /// payload, a panic in an adapter or a database that is down all end as
    /// a log line and `false`, and the caller tries again later.
    pub async fn collect(&self, job: &Job) -> bool {
        let source = self.adapter(&job.source);
        let started_at = self.clock.now();
        let ctx = PlanCtx {
            now: started_at,
            area: job.area,
        };
        let options = RunOptions {
            since: self.lookback.map(|lookback| started_at - lookback),
            ..RunOptions::default()
        };

        // An adapter parses whatever a provider sends. A bug in one must not
        // take the server, or the other sources, down with it.
        let run = run_observations(source, &self.fetch, &self.archive, &ctx, &options);
        let Ok(output) = AssertUnwindSafe(run).catch_unwind().await else {
            tracing::error!(source = %job.source, "the run panicked; see the panic message above");
            let failed = self.record(&job.source, started_at, None);
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
        for failure in report.failures.iter().take(5) {
            tracing::warn!(source = %job.source, "failed {}: {}", failure.tag, failure.error);
        }

        let record = self.record(&job.source, started_at, Some(report));
        self.save(&record, &output.stations, &output.observations)
            .await
            && healthy
    }

    fn record(
        &self,
        source: &SourceId,
        started_at: Timestamp,
        report: Option<&RunReport>,
    ) -> RunRecord {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        RunRecord {
            source: source.clone(),
            started_at,
            finished_at: self.clock.now(),
            healthy: report.is_some_and(RunReport::is_healthy),
            requests: report.map_or(0, |r| count(r.requests)),
            failed_requests: report.map_or(0, |r| count(r.failures.len())),
            skipped: report.map_or(0, |r| count(r.skipped.len())),
            rejected: report.map_or(0, |r| count(r.rejected.len())),
            stations: report.map_or(0, |r| count(r.stations)),
            observations: report.map_or(0, |r| count(r.observations)),
            newest_observation: report.and_then(|r| r.newest_observation),
        }
    }

    async fn save(
        &self,
        record: &RunRecord,
        stations: &[venti_domain::Station],
        observations: &[venti_domain::Observation],
    ) -> bool {
        match self.store.save_run(record, stations, observations).await {
            Ok(()) => true,
            Err(error) => {
                tracing::error!(source = %record.source, "the run could not be stored: {error}");
                false
            }
        }
    }
}
