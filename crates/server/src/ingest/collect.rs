use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use chrono::Duration;
use futures::FutureExt;
use venti_domain::{Airspace, Observation, Site, SourceId, Station, Timestamp};
use venti_sources::{
    AirspaceRunReport, Archive, Fetch, ForecastRunReport, Kind, PlanCtx, RadarRunReport, Registry,
    RunOptions, RunReport, SiteRunReport, run_airspace, run_observations, run_sites,
};
use venti_store::{Coverage, ForecastModel, RunRecord, SiteDelivery, Store};

use super::{airspace_summary, forecast_summary, radar_summary, sites_summary, summary};
use crate::alerts::Watch;
use crate::clock::Clock;
use crate::forecast::Forecasts;
use crate::plan::Job;
use crate::radar::Radar;
use crate::thermals::ThermalMap;

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
    /// The forecast service, which does the looking at forecast sources.
    forecasts: Option<Arc<dyn Forecasts>>,
    /// The thermal map, which a look tells of the runs it sees.
    thermal_map: Option<Arc<ThermalMap>>,
    /// The radar service, which runs the radar sources and keeps their
    /// frames.
    radar: Option<Arc<dyn Radar>>,
    /// The alarms pilots set, checked against the readings of each run.
    alerts: Option<Arc<dyn Watch>>,
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
            forecasts: None,
            thermal_map: None,
            radar: None,
            alerts: None,
        }
    }

    /// The same collector, checking the wind alarms against the readings
    /// of each run of stations once they are stored.
    #[must_use]
    pub fn with_alerts(mut self, alerts: Arc<dyn Watch>) -> Self {
        self.alerts = Some(alerts);
        self
    }

    /// The same collector, with the radar service it runs radar sources
    /// through.
    ///
    /// Radar frames are not stored: the radar service fetches them and
    /// keeps the last ones in memory, with a clock of its own. What the
    /// collector does for a radar source is to ask the service for a run
    /// at the source's cadence and to write the run down.
    #[must_use]
    pub fn with_radar(mut self, radar: Arc<dyn Radar>) -> Self {
        self.radar = Some(radar);
        self
    }

    /// The same collector, with the forecast service it looks at forecast
    /// sources through.
    ///
    /// Forecasts are not collected: they are fetched when a pilot asks, by
    /// the forecast service. What the collector does for a forecast source
    /// is the regular look that the health report is made from, and it
    /// asks the service to do it, so that every request to the source goes
    /// one way and is counted in one place.
    #[must_use]
    pub fn with_forecasts(mut self, forecasts: Arc<dyn Forecasts>) -> Self {
        self.forecasts = Some(forecasts);
        self
    }

    /// The same collector, telling the thermal map of each run of its
    /// model that a look at its source sees. The map is then made in a
    /// task of its own, so that a look is not held up by it.
    #[must_use]
    pub fn with_thermal_map(mut self, map: Arc<ThermalMap>) -> Self {
        self.thermal_map = Some(map);
        self
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
            Kind::Sites => self.collect_sites(job).await,
            Kind::Forecasts => self.look_at_forecasts(job).await,
            Kind::Radar => self.collect_radar(job).await,
            // No plan has such a job: imagery is fetched when a client
            // asks for a tile, never on a schedule.
            Kind::Imagery => {
                tracing::error!(source = %job.source, "imagery is not collected");
                false
            }
            // Nor has any a job of live positions: the server listens to
            // such a source over a connection of its own.
            Kind::Live => {
                tracing::error!(source = %job.source, "live positions are not collected");
                false
            }
        }
    }

    /// Looks at a forecast source once: asks the forecast service for the
    /// forecast of the middle of the source's area and writes down whether
    /// that worked and which run each model is at.
    async fn look_at_forecasts(&self, job: &Job) -> bool {
        let Some(forecasts) = &self.forecasts else {
            tracing::error!(
                source = %job.source,
                "no forecast service to look at the source through"
            );
            return false;
        };
        let started_at = self.clock.now();
        let look = forecasts.look(&job.source);
        let Some(Some(output)) = unless_it_panics(&job.source, look).await else {
            let failed = self.record(&job.source, started_at, &Outcome::default());
            self.save_forecast_check(&failed, &[]).await;
            return false;
        };

        let report = &output.report;
        let healthy = report.is_healthy();
        if healthy {
            tracing::info!(source = %job.source, "{}", forecast_summary(report));
        } else {
            tracing::warn!(source = %job.source, "unhealthy look: {}", forecast_summary(report));
        }
        for failure in report.failures.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "failed {}: {}", failure.tag, failure.error);
        }
        // A value the source does not have is a gap in every pilot's
        // forecast. The look stays healthy, so the log is where it shows.
        for skipped in report.skipped.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "left out {:?}: {}", skipped.what, skipped.reason);
        }

        let models: Vec<ForecastModel> = output
            .series
            .iter()
            .map(|series| ForecastModel {
                source: job.source.clone(),
                model: series.model.clone(),
                run: series.run,
                available_at: series.available_at,
                checked_at: started_at,
            })
            .collect();
        if let Some(map) = self
            .thermal_map
            .as_ref()
            .filter(|map| map.source() == &job.source)
        {
            let of_the_map = map.models();
            let seen: Vec<_> = models
                .iter()
                .filter(|model| of_the_map.contains(&model.model))
                .map(|model| (model.model.clone(), model.run))
                .collect();
            if !seen.is_empty() {
                let map = map.clone();
                tokio::spawn(async move { map.update(&seen).await });
            }
        }
        let record = self.record(&job.source, started_at, &Outcome::of_forecasts(report));
        self.save_forecast_check(&record, &models).await && healthy
    }

    /// Runs a radar source once, through the radar service, and writes the
    /// run down. The frames stay with the service.
    async fn collect_radar(&self, job: &Job) -> bool {
        let Some(radar) = &self.radar else {
            tracing::error!(source = %job.source, "no radar service to run the source through");
            return false;
        };
        let started_at = self.clock.now();
        let run = radar.collect(&job.source);
        let Some(Some(report)) = unless_it_panics(&job.source, run).await else {
            let failed = self.record(&job.source, started_at, &Outcome::default());
            self.save(&failed, &[], &[]).await;
            return false;
        };

        let healthy = report.is_healthy();
        if healthy {
            tracing::info!(source = %job.source, "{}", radar_summary(&report));
        } else {
            tracing::warn!(source = %job.source, "unhealthy run: {}", radar_summary(&report));
        }
        for failure in report.failures.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "failed {}: {}", failure.tag, failure.error);
        }
        for rejected in report.rejected.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(
                source = %job.source,
                "rejected the frame of {}: {}",
                rejected.time.format("%H:%M UTC"),
                rejected.reason
            );
        }

        // The run history is the store's; a radar run has no records to
        // go with it.
        let record = self.record(&job.source, started_at, &Outcome::of_radar(&report));
        self.save(&record, &[], &[]).await && healthy
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
        let saved = self
            .save(&record, &output.stations, &output.observations)
            .await;
        if saved && let Some(alerts) = &self.alerts {
            alerts.readings(&output.observations).await;
        }
        saved && healthy
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

    /// Runs a source of flying sites once. As for airspace, the set it
    /// delivers replaces the stored one only if every request worked, and
    /// the store holds to that itself.
    async fn collect_sites(&self, job: &Job) -> bool {
        let source = self
            .registry
            .sites(&job.source)
            .unwrap_or_else(|| panic!("the plan names {}, which has no adapter", job.source));
        let started_at = self.clock.now();
        let ctx = PlanCtx {
            now: started_at,
            area: job.area,
        };
        let options = RunOptions::default();

        let run = run_sites(source, &self.fetch, &self.archive, &ctx, &options);
        let Some(output) = unless_it_panics(&job.source, run).await else {
            let failed = self.record(&job.source, started_at, &Outcome::default());
            let nothing = SiteDelivery {
                as_of: started_at,
                covers: Vec::new(),
            };
            self.save_sites(&failed, &nothing, &[]).await;
            return false;
        };

        let report = &output.report;
        let healthy = report.is_healthy();
        if healthy {
            tracing::info!(source = %job.source, "{}", sites_summary(report, output.as_of));
        } else {
            tracing::warn!(
                source = %job.source,
                "unhealthy run, the stored sites stay as they are: {}",
                sites_summary(report, output.as_of)
            );
        }
        for failure in report.failures.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(source = %job.source, "failed {}: {}", failure.tag, failure.error);
        }
        for rejected in report.rejected.iter().take(LOGGED_PROBLEMS) {
            tracing::warn!(
                source = %job.source,
                "rejected {} of {}: {}",
                rejected.field,
                rejected.site,
                rejected.reason
            );
        }

        // Where the set of this run is whole: the area it was collected for.
        let delivery = SiteDelivery {
            as_of: output.as_of,
            covers: vec![job.area],
        };
        let record = self.record(&job.source, started_at, &Outcome::of_sites(report));
        self.save_sites(&record, &delivery, &output.sites).await && healthy
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

    async fn save_sites(
        &self,
        record: &RunRecord,
        delivery: &SiteDelivery,
        sites: &[Site],
    ) -> bool {
        let saved = self.store.save_site_run(record, delivery, sites).await;
        stored(record, saved)
    }

    async fn save_forecast_check(&self, record: &RunRecord, models: &[ForecastModel]) -> bool {
        let saved = self.store.save_forecast_check(record, models).await;
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

    /// How many sites a run delivered is not in the run history: the set
    /// the store holds says how many there are.
    fn of_sites(report: &SiteRunReport) -> Self {
        Self {
            healthy: report.is_healthy(),
            requests: report.requests,
            failed_requests: report.failures.len(),
            skipped: report.skipped.len(),
            rejected: report.rejected.len(),
            ..Self::default()
        }
    }

    /// Which run each model is at is not in the run history: what the
    /// store holds about the source's models says so.
    fn of_forecasts(report: &ForecastRunReport) -> Self {
        Self {
            healthy: report.is_healthy(),
            requests: report.requests,
            failed_requests: report.failures.len(),
            skipped: report.skipped.len(),
            rejected: report.rejected.len(),
            ..Self::default()
        }
    }

    /// How many frames are held, and how old the newest is, is not in the
    /// run history: the radar service says so.
    fn of_radar(report: &RadarRunReport) -> Self {
        Self {
            healthy: report.is_healthy(),
            requests: report.requests,
            failed_requests: report.failures.len(),
            skipped: report.skipped.len(),
            rejected: report.rejected.len(),
            ..Self::default()
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
