//! Runs every source at its cadence for as long as the server lives.

use std::sync::Arc;

use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use venti_domain::BoundingBox;
use venti_sources::{Archive, Fetch, Kind};
use venti_store::{SourceStatus, Store};

use super::Collector;
use super::schedule::Pace;
use crate::plan::Job;

/// Runs each job at its source's cadence until `shutdown` is cancelled.
///
/// Every source gets a task of its own that runs it, waits, and runs it
/// again. That one loop per source is what guarantees that a source never
/// runs twice at once, and that a slow or failing source holds up no other.
///
/// `jitter` returns a random number from 0 up to 1 each time it is called.
pub async fn run<S, F, A>(
    collector: Arc<Collector<S, F, A>>,
    jobs: Vec<Job>,
    jitter: fn() -> f64,
    shutdown: CancellationToken,
) where
    S: Store,
    F: Fetch + Send + 'static,
    A: Archive + Send + 'static,
{
    let mut sources = JoinSet::new();
    for job in jobs {
        sources.spawn(run_source(collector.clone(), job, jitter, shutdown.clone()));
    }
    while let Some(ended) = sources.join_next().await {
        if let Err(error) = ended {
            // The loop catches what a run can throw, so this is a bug in the
            // loop itself. The other sources carry on.
            tracing::error!("the schedule of a source stopped: {error}");
        }
    }
}

async fn run_source<S, F, A>(
    collector: Arc<Collector<S, F, A>>,
    job: Job,
    jitter: fn() -> f64,
    shutdown: CancellationToken,
) where
    S: Store,
    F: Fetch,
    A: Archive,
{
    let pace = Pace::new(collector.cadence_seconds(&job));
    let status = latest_status(&collector, &job).await;
    let since = status
        .as_ref()
        .and_then(|status| since_last_start(&collector, status));
    let short = match &status {
        Some(status) if latest_run_was_healthy(status) => falls_short(&collector, &job).await,
        _ => false,
    };
    let latest_failed = status
        .as_ref()
        .is_some_and(|status| status.last_failure.is_some() && !latest_run_was_healthy(status));
    if short && since.is_some() {
        tracing::info!(
            source = %job.source,
            "what is stored was collected over less than the plan asks for: running now"
        );
    }
    let mut next = Instant::now() + pace.first_delay(since, short, latest_failed);
    let mut failures: u32 = 0;

    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep_until(next) => {}
        }
        let started = Instant::now();
        // A run cut short by shutdown stores nothing; the next start
        // fetches the same hours again.
        let healthy = tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            healthy = collector.collect(&job) => healthy,
        };
        failures = if healthy {
            0
        } else {
            failures.saturating_add(1)
        };
        // Measured from the start of the run. When a run took longer than
        // the wait, the next one starts as soon as it has finished.
        next = started + pace.next_delay(failures, jitter());
    }
}

/// What the run history says about the job's source. `None` when the
/// history cannot be read.
async fn latest_status<S: Store, F: Fetch, A: Archive>(
    collector: &Collector<S, F, A>,
    job: &Job,
) -> Option<SourceStatus> {
    let status = collector
        .store()
        .source_status(std::slice::from_ref(&job.source))
        .await
        .ok()?;
    status.into_iter().next()
}

/// How long ago the source's latest run started. `None` when it never ran.
fn since_last_start<S: Store, F: Fetch, A: Archive>(
    collector: &Collector<S, F, A>,
    status: &SourceStatus,
) -> Option<std::time::Duration> {
    let last_started = status.last_started?;
    // A start time in the future means a clock was set back; the full
    // cadence is then the longest sensible wait.
    Some(
        (collector.now() - last_started)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO),
    )
}

/// Whether the source's latest run that finished was a healthy one. A run
/// that failed is retried by the ordinary rule, however little the stored
/// set covers: that is what keeps a server that restarts again and again
/// from asking a failing provider at every start.
fn latest_run_was_healthy(status: &SourceStatus) -> bool {
    match (status.last_success, status.last_failure) {
        (Some(success), Some(failure)) => success >= failure,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Whether the set the store holds of the job's source was collected over
/// less than the job now asks for: its area is not inside one of the boxes
/// the set says it is whole for, or, for airspace, the adapter now fetches
/// a country the set was not fetched for. Only kinds whose stored set says
/// what it covers can fall short: airspace, sites and cables. Stations,
/// forecasts and radar keep nothing that says so, and never do. A store
/// that cannot be read falls short of nothing: the ordinary rule holds.
async fn falls_short<S: Store, F: Fetch, A: Archive>(
    collector: &Collector<S, F, A>,
    job: &Job,
) -> bool {
    let store = collector.store();
    let sources = std::slice::from_ref(&job.source);
    let covers_area = |boxes: &[BoundingBox]| boxes.iter().any(|b| b.encloses(&job.area));
    match job.kind {
        Kind::Airspace => match store.airspace_sets(sources).await {
            Ok(sets) => sets.first().is_none_or(|set| {
                let countries = &job.manifest(collector.registry()).countries;
                !covers_area(&set.coverage.boxes)
                    || countries
                        .iter()
                        .any(|country| !set.coverage.countries.contains(country))
            }),
            Err(_) => false,
        },
        Kind::Sites => match store.site_sets(sources).await {
            Ok(sets) => sets.first().is_none_or(|set| !covers_area(&set.covers)),
            Err(_) => false,
        },
        Kind::Cables => match store.cable_sets(sources).await {
            Ok(sets) => sets.first().is_none_or(|set| !covers_area(&set.covers)),
            Err(_) => false,
        },
        Kind::Observations
        | Kind::Forecasts
        | Kind::Radar
        | Kind::Imagery
        | Kind::Live
        | Kind::Boundaries => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use chrono::{TimeZone, Utc};
    use venti_domain::{
        Activity, Airspace, AirspaceClass, AirspaceId, AirspaceType, BoundingBox, GeoPoint,
        Observation, Polygon, Site, SiteId, SiteUse, SourceId, Station, StationId, Timestamp,
        Validity, VerticalLimit,
    };
    use venti_sources::{
        AirspaceSource, FetchError, Fetched, Kind, Manifest, ObservationSource, Parsed,
        ParsedAirspace, ParsedSites, PlanCtx, Registry, Request, SiteSource, SourceError,
    };
    use venti_store::testing::TroubledStore;
    use venti_store::{Coverage, MemoryStore, RunRecord, SiteDelivery};

    use super::*;
    use crate::clock::Clock;
    use crate::ingest::DirArchive;

    const MINUTE: Duration = Duration::from_secs(60);

    // --- A clock that follows tokio's paused, virtual time -----------------

    struct VirtualClock {
        epoch: Timestamp,
        started: Instant,
    }

    impl VirtualClock {
        fn new() -> Self {
            Self {
                epoch: Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap(),
                started: Instant::now(),
            }
        }
    }

    impl Clock for VirtualClock {
        fn now(&self) -> Timestamp {
            self.epoch + chrono::Duration::from_std(self.started.elapsed()).unwrap()
        }
    }

    // --- An adapter with one station and one request per run ----------------

    struct TestSource {
        manifest: Manifest,
        panics: bool,
    }

    impl TestSource {
        fn new(id: &str, cadence_seconds: u32) -> Self {
            let manifest = format!(
                r#"
                id = "{id}"
                provider = "Test"
                short_name = "Test"
                kinds = ["observations", "airspace", "sites"]
                countries = ["DE"]
                cadence_seconds = {cadence_seconds}
                auth = "none"
                [licence]
                name = "CC0"
                commercial = true
                redistribution = true
                share_alike = false
                attribution = "Test"
                link = "https://example.org"
                verified_on = "2026-11-18"
                verified_from = ["https://example.org/terms"]
                "#
            );
            Self {
                manifest: Manifest::from_toml(&manifest).unwrap(),
                panics: false,
            }
        }
    }

    impl ObservationSource for TestSource {
        fn manifest(&self) -> &Manifest {
            &self.manifest
        }

        fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
            let url = format!("https://example.org/{}", self.manifest.id);
            vec![Request::new("values", url)]
        }

        fn parse(
            &self,
            _request: &Request,
            _body: &[u8],
            ctx: &PlanCtx,
        ) -> Result<Parsed, SourceError> {
            assert!(!self.panics, "this adapter has a bug");
            let station = StationId::new(self.manifest.id.clone(), "1").unwrap();
            let mut observation = Observation::empty(station.clone(), ctx.now);
            observation.wind_speed_ms = Some(3.0);
            Ok(Parsed {
                stations: vec![Station {
                    id: station,
                    name: "Test".to_owned(),
                    position: GeoPoint::new(47.0, 11.0).unwrap(),
                    elevation_m: None,
                }],
                observations: vec![observation],
                ..Parsed::default()
            })
        }
    }

    /// The same source as one of airspace: every run delivers a set of one
    /// airspace.
    impl AirspaceSource for TestSource {
        fn manifest(&self) -> &Manifest {
            &self.manifest
        }

        fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
            ObservationSource::plan(self, ctx)
        }

        fn parse(
            &self,
            _request: &Request,
            _body: &[u8],
            _ctx: &PlanCtx,
        ) -> Result<ParsedAirspace, SourceError> {
            Ok(ParsedAirspace {
                airspaces: vec![test_airspace(&self.manifest.id)],
                ..ParsedAirspace::default()
            })
        }
    }

    /// The same source as one of flying sites: every run delivers a set of
    /// one site.
    impl SiteSource for TestSource {
        fn manifest(&self) -> &Manifest {
            &self.manifest
        }

        fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
            ObservationSource::plan(self, ctx)
        }

        fn parse(
            &self,
            _request: &Request,
            _body: &[u8],
            _ctx: &PlanCtx,
        ) -> Result<ParsedSites, SourceError> {
            Ok(ParsedSites {
                sites: vec![test_site(&self.manifest.id)],
                ..ParsedSites::default()
            })
        }
    }

    fn test_airspace(source: &SourceId) -> Airspace {
        let point = |lat, lon| GeoPoint::new(lat, lon).unwrap();
        let ring = vec![
            point(47.0, 11.0),
            point(47.0, 11.2),
            point(47.1, 11.2),
            point(47.0, 11.0),
        ];
        Airspace {
            id: AirspaceId::new(source.clone(), "1").unwrap(),
            name: "Test".to_owned(),
            class: AirspaceClass::D,
            airspace_type: AirspaceType::Ctr,
            country: None,
            lower: VerticalLimit::ground(),
            upper: VerticalLimit::unlimited(),
            lower_at_least: None,
            upper_at_most: None,
            geometry: Polygon::new(ring, Vec::new()).unwrap(),
            activity: Activity::default(),
            frequencies: Vec::new(),
            remarks: None,
            validity: Validity::default(),
        }
    }

    fn test_site(source: &SourceId) -> Site {
        Site {
            id: SiteId::new(source.clone(), "1").unwrap(),
            name: Some("Test".to_owned()),
            position: GeoPoint::new(47.0, 11.0).unwrap(),
            elevation_m: None,
            uses: vec![SiteUse::Takeoff],
            wind_directions: Vec::new(),
            paragliding: Some(true),
            hang_gliding: None,
            official: None,
            discouraged: false,
            url: None,
            website: None,
            description: None,
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        }
    }

    // --- A fetcher that follows a script and notes when it was called -------

    /// A handle to one script; the collector holds a clone of it.
    #[derive(Clone)]
    struct ScriptedFetch(Arc<Script>);

    struct Script {
        began: Instant,
        /// The URL and the second of virtual time of every call.
        calls: Mutex<Vec<(String, u64)>>,
        /// What the next calls answer; when empty, every call succeeds.
        answers: Mutex<VecDeque<Result<(), FetchError>>>,
        /// How long each call takes.
        takes: Duration,
        in_flight: AtomicUsize,
        most_in_flight: AtomicUsize,
    }

    impl ScriptedFetch {
        fn new(takes: Duration, failures: usize) -> Self {
            Self::answering(takes, std::iter::repeat_n(false, failures))
        }

        /// Answers its first calls as listed, `true` for success, and every
        /// call after them with success.
        fn answering(takes: Duration, works: impl IntoIterator<Item = bool>) -> Self {
            let answer = |works| {
                if works {
                    Ok(())
                } else {
                    Err(FetchError::Status(503))
                }
            };
            Self(Arc::new(Script {
                began: Instant::now(),
                calls: Mutex::default(),
                answers: Mutex::new(works.into_iter().map(answer).collect()),
                takes,
                in_flight: AtomicUsize::new(0),
                most_in_flight: AtomicUsize::new(0),
            }))
        }

        /// Answers at once, always with success.
        fn working() -> Self {
            Self::new(Duration::ZERO, 0)
        }
    }

    impl Fetch for ScriptedFetch {
        async fn get(&self, request: &Request) -> Result<Fetched, FetchError> {
            let script = &self.0;
            let at = script.began.elapsed().as_secs();
            script.calls.lock().unwrap().push((request.url.clone(), at));
            let now = script.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            script.most_in_flight.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(script.takes).await;
            script.in_flight.fetch_sub(1, Ordering::SeqCst);
            let answer = script.answers.lock().unwrap().pop_front();
            answer.unwrap_or(Ok(())).map(|()| Fetched::default())
        }
    }

    // --- The scene ----------------------------------------------------------

    fn job(id: &str) -> Job {
        Job {
            source: SourceId::new(id).unwrap(),
            kind: Kind::Observations,
            area: BoundingBox::new(46.0, 9.0, 49.0, 18.0).unwrap(),
        }
    }

    type TestCollector<S> = Arc<Collector<S, ScriptedFetch, Option<DirArchive>>>;

    fn collector_for<S: Store>(
        sources: Vec<TestSource>,
        store: Arc<S>,
        fetch: &ScriptedFetch,
    ) -> TestCollector<S> {
        let mut registry = Registry::new();
        for source in sources {
            registry.register_observations(Box::new(source)).unwrap();
        }
        Arc::new(Collector::new(
            registry,
            store,
            fetch.clone(),
            None,
            Arc::new(VirtualClock::new()),
            Some(chrono::Duration::hours(3)),
        ))
    }

    /// Starts the schedule, lets `minutes` of virtual time pass, stops it,
    /// and returns the minute at which each request was made.
    async fn minutes_of_calls<S: Store>(
        collector: TestCollector<S>,
        jobs: Vec<Job>,
        fetch: &ScriptedFetch,
        jitter: fn() -> f64,
        minutes: u32,
    ) -> Vec<(String, f64)> {
        let shutdown = CancellationToken::new();
        let schedule = tokio::spawn(run(collector, jobs, jitter, shutdown.clone()));
        tokio::time::sleep(minutes * MINUTE).await;
        shutdown.cancel();
        schedule.await.unwrap();
        let calls = fetch.0.calls.lock().unwrap();
        calls
            .iter()
            .map(|(url, seconds)| {
                let source = url.rsplit('/').next().unwrap().to_owned();
                (source, *seconds as f64 / 60.0)
            })
            .collect()
    }

    fn of(calls: &[(String, f64)], source: &str) -> Vec<f64> {
        calls
            .iter()
            .filter(|(name, _)| name == source)
            .map(|(_, minute)| *minute)
            .collect()
    }

    fn no_jitter() -> f64 {
        0.0
    }

    // --- The tests ----------------------------------------------------------

    #[tokio::test(start_paused = true)]
    async fn each_source_runs_at_its_own_cadence_and_stores_what_it_gets() {
        let fetch = ScriptedFetch::working();
        let store = Arc::new(MemoryStore::new());
        let sources = vec![TestSource::new("ten", 600), TestSource::new("fifteen", 900)];
        let collector = collector_for(sources, store.clone(), &fetch);

        let calls = minutes_of_calls(
            collector,
            vec![job("ten"), job("fifteen")],
            &fetch,
            no_jitter,
            35,
        )
        .await;

        assert_eq!(of(&calls, "ten"), [0.0, 10.0, 20.0, 30.0]);
        assert_eq!(of(&calls, "fifteen"), [0.0, 15.0, 30.0]);
        // Every run reached the store: the newest observation is the last run's.
        let everywhere = BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap();
        let stations = store.stations_in(&everywhere, 10).await.unwrap();
        assert_eq!(stations.len(), 2);
        let status = store
            .source_status(&[SourceId::new("ten").unwrap()])
            .await
            .unwrap();
        let epoch = Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap();
        assert_eq!(
            status[0].last_success,
            Some(epoch + chrono::Duration::minutes(30))
        );
        assert_eq!(status[0].last_failure, None);
        assert_eq!(status[0].stations, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_airspace_source_runs_once_a_day_and_keeps_its_set_through_a_failed_run() {
        const HOUR: Duration = Duration::from_secs(3600);
        // The first night's run works, the second fails, the third works.
        let fetch = ScriptedFetch::answering(Duration::ZERO, [true, false, true]);
        let store = Arc::new(MemoryStore::new());
        let mut registry = Registry::new();
        registry
            .register_airspace(Box::new(TestSource::new("air", 86_400)))
            .unwrap();
        let collector = Arc::new(Collector::new(
            registry,
            store.clone(),
            fetch.clone(),
            None::<DirArchive>,
            Arc::new(VirtualClock::new()),
            Some(chrono::Duration::hours(3)),
        ));
        let air = SourceId::new("air").unwrap();
        let airspace_job = Job {
            kind: Kind::Airspace,
            ..job("air")
        };
        let epoch = Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap();
        let stored = || async {
            let sets = store.airspace_sets(std::slice::from_ref(&air)).await;
            sets.unwrap().first().map(|set| set.fetched_at)
        };

        let shutdown = CancellationToken::new();
        let schedule = tokio::spawn(run(
            collector,
            vec![airspace_job],
            no_jitter,
            shutdown.clone(),
        ));

        // The first run was at once, and its set is the stored one. It
        // says where it is whole: the job's area, for the countries of the
        // source's manifest.
        tokio::time::sleep(HOUR).await;
        assert_eq!(stored().await, Some(epoch));
        let sets = store.airspace_sets(std::slice::from_ref(&air)).await;
        let coverage = sets.unwrap().remove(0).coverage;
        assert_eq!(coverage.boxes, [job("air").area]);
        assert_eq!(coverage.countries, ["DE"]);

        // A day later the run fails. The set of the day before stays, and
        // the next try is a day after that, not sooner.
        tokio::time::sleep(46 * HOUR).await;
        assert_eq!(stored().await, Some(epoch));
        let status = store
            .source_status(std::slice::from_ref(&air))
            .await
            .unwrap();
        assert_eq!(
            status[0].last_failure,
            Some(epoch + chrono::Duration::hours(24))
        );

        tokio::time::sleep(2 * HOUR).await;
        shutdown.cancel();
        schedule.await.unwrap();
        assert_eq!(stored().await, Some(epoch + chrono::Duration::hours(48)));
        let calls = fetch.0.calls.lock().unwrap();
        let hours: Vec<u64> = calls.iter().map(|(_, seconds)| seconds / 3600).collect();
        assert_eq!(hours, [0, 24, 48]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_source_never_runs_twice_at_once() {
        // Each request takes 25 minutes, far longer than the cadence.
        let fetch = ScriptedFetch::new(25 * MINUTE, 0);
        let store = Arc::new(MemoryStore::new());
        let collector = collector_for(vec![TestSource::new("slow", 600)], store, &fetch);

        let calls = minutes_of_calls(collector, vec![job("slow")], &fetch, no_jitter, 90).await;

        // The next run starts when the previous one has finished, not before.
        assert_eq!(of(&calls, "slow"), [0.0, 25.0, 50.0, 75.0]);
        assert_eq!(fetch.0.most_in_flight.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failures_back_off_and_a_success_restores_the_cadence() {
        let fetch = ScriptedFetch::new(Duration::ZERO, 4);
        let store = Arc::new(MemoryStore::new());
        let collector = collector_for(vec![TestSource::new("flaky", 600)], store.clone(), &fetch);

        let calls = minutes_of_calls(collector, vec![job("flaky")], &fetch, no_jitter, 155).await;

        // Four failures: wait 10, 20, 40, then 60 minutes. The fifth run
        // works, and the source is back to every 10 minutes.
        assert_eq!(
            of(&calls, "flaky"),
            [0.0, 10.0, 30.0, 70.0, 130.0, 140.0, 150.0]
        );
        let status = store
            .source_status(&[SourceId::new("flaky").unwrap()])
            .await
            .unwrap();
        let epoch = Utc.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap();
        assert_eq!(
            status[0].last_failure,
            Some(epoch + chrono::Duration::minutes(70))
        );
        assert_eq!(
            status[0].last_success,
            Some(epoch + chrono::Duration::minutes(150))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn jitter_delays_a_run_and_never_brings_it_forward() {
        let fetch = ScriptedFetch::working();
        let store = Arc::new(MemoryStore::new());
        let collector = collector_for(vec![TestSource::new("ten", 600)], store, &fetch);

        let calls = minutes_of_calls(collector, vec![job("ten")], &fetch, || 0.5, 25).await;

        // Half of a tenth of the cadence: 30 seconds on top of each wait.
        assert_eq!(of(&calls, "ten"), [0.0, 10.5, 21.0]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_restarted_server_waits_out_the_cadence() {
        let fetch = ScriptedFetch::working();
        let store = Arc::new(MemoryStore::new());
        let collector = collector_for(vec![TestSource::new("ten", 600)], store.clone(), &fetch);
        // The process before this one started a run four minutes ago.
        let earlier = collector.now() - chrono::Duration::minutes(4);
        let record = RunRecord {
            source: SourceId::new("ten").unwrap(),
            started_at: earlier,
            finished_at: earlier,
            healthy: true,
            requests: 1,
            failed_requests: 0,
            skipped: 0,
            rejected: 0,
            stations: 1,
            observations: 1,
            newest_observation: None,
            airspaces: 0,
        };
        store.save_run(&record, &[], &[]).await.unwrap();

        let calls = minutes_of_calls(collector, vec![job("ten")], &fetch, no_jitter, 20).await;

        assert_eq!(of(&calls, "ten"), [6.0, 16.0]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_broken_adapter_and_a_dead_database_stop_nothing() {
        let fetch = ScriptedFetch::working();
        let mut buggy = TestSource::new("buggy", 600);
        buggy.panics = true;

        // An adapter that panics is retried with backoff like any failure,
        // and the source beside it keeps its cadence.
        let store = Arc::new(MemoryStore::new());
        let collector = collector_for(
            vec![buggy, TestSource::new("fine", 600)],
            store.clone(),
            &fetch,
        );
        let calls = minutes_of_calls(
            collector,
            vec![job("buggy"), job("fine")],
            &fetch,
            no_jitter,
            35,
        )
        .await;
        assert_eq!(of(&calls, "buggy"), [0.0, 10.0, 30.0]);
        assert_eq!(of(&calls, "fine"), [0.0, 10.0, 20.0, 30.0]);
        let status = store
            .source_status(&[SourceId::new("buggy").unwrap()])
            .await
            .unwrap();
        assert!(status[0].last_failure.is_some());
        assert_eq!(status[0].last_success, None);

        // With the database down every run counts as failed, and the
        // schedule keeps trying at the backed-off pace.
        let fetch = ScriptedFetch::working();
        let collector = collector_for(
            vec![TestSource::new("fine", 600)],
            Arc::new(TroubledStore::down()),
            &fetch,
        );
        let calls = minutes_of_calls(collector, vec![job("fine")], &fetch, no_jitter, 35).await;
        assert_eq!(of(&calls, "fine"), [0.0, 10.0, 30.0]);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_ends_the_schedule_in_the_middle_of_a_run() {
        // A request that would take a day.
        let fetch = ScriptedFetch::new(24 * 60 * MINUTE, 0);
        let store = Arc::new(MemoryStore::new());
        let collector = collector_for(vec![TestSource::new("stuck", 600)], store.clone(), &fetch);
        let shutdown = CancellationToken::new();
        let schedule = tokio::spawn(run(
            collector,
            vec![job("stuck")],
            no_jitter,
            shutdown.clone(),
        ));

        tokio::time::sleep(MINUTE).await;
        let before = Instant::now();
        shutdown.cancel();
        schedule.await.unwrap();

        assert_eq!(before.elapsed(), Duration::ZERO, "it stopped at once");
        // The run that was cut short left nothing behind.
        let status = store
            .source_status(&[SourceId::new("stuck").unwrap()])
            .await
            .unwrap();
        assert_eq!(status[0].last_started, None);
    }

    // --- A plan that grew past what is stored ------------------------------

    /// The box of the first region pack, `alps-de-at`.
    fn first_region() -> BoundingBox {
        BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
    }

    /// The box around the three packs of the Alps.
    fn three_regions() -> BoundingBox {
        BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap()
    }

    /// A collector of `air`, airspace, and `sites`, flying sites, both
    /// collected once a day.
    fn daily_collector(
        store: Arc<MemoryStore>,
        fetch: &ScriptedFetch,
    ) -> TestCollector<MemoryStore> {
        let mut registry = Registry::new();
        registry
            .register_airspace(Box::new(TestSource::new("air", 86_400)))
            .unwrap();
        registry
            .register_sites(Box::new(TestSource::new("sites", 86_400)))
            .unwrap();
        Arc::new(Collector::new(
            registry,
            store,
            fetch.clone(),
            None,
            Arc::new(VirtualClock::new()),
            Some(chrono::Duration::hours(3)),
        ))
    }

    fn daily_job(id: &str, kind: Kind, area: BoundingBox) -> Job {
        Job {
            source: SourceId::new(id).unwrap(),
            kind,
            area,
        }
    }

    /// Writes down a run of the server before this one, `hours_ago`: a
    /// healthy one stores its set as collected over `area`, for the
    /// countries `countries` (airspace only).
    async fn ran_before(
        collector: &TestCollector<MemoryStore>,
        job: &Job,
        area: BoundingBox,
        countries: &[&str],
        hours_ago: i64,
        healthy: bool,
    ) {
        let at = collector.now() - chrono::Duration::hours(hours_ago);
        let record = RunRecord {
            source: job.source.clone(),
            started_at: at,
            finished_at: at,
            healthy,
            requests: 1,
            failed_requests: u32::from(!healthy),
            skipped: 0,
            rejected: 0,
            stations: 0,
            observations: 0,
            newest_observation: None,
            airspaces: u32::from(healthy),
        };
        let store = collector.store();
        match job.kind {
            Kind::Airspace => {
                let coverage = Coverage {
                    boxes: vec![area],
                    countries: countries.iter().map(|&c| c.to_owned()).collect(),
                };
                let airspaces: Vec<Airspace> = if healthy {
                    vec![test_airspace(&job.source)]
                } else {
                    Vec::new()
                };
                store
                    .save_airspace_run(&record, &coverage, &airspaces)
                    .await
                    .unwrap();
            }
            Kind::Sites => {
                let delivery = SiteDelivery {
                    as_of: at,
                    covers: vec![area],
                };
                let sites: Vec<Site> = if healthy {
                    vec![test_site(&job.source)]
                } else {
                    Vec::new()
                };
                store
                    .save_site_run(&record, &delivery, &sites)
                    .await
                    .unwrap();
            }
            other => panic!("no stored set for {other:?}"),
        }
    }

    fn hours_of(calls: &[(String, f64)], source: &str) -> Vec<f64> {
        of(calls, source)
            .iter()
            .map(|minute| minute / 60.0)
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn a_plan_wider_than_the_stored_set_runs_at_once_and_once() {
        let fetch = ScriptedFetch::working();
        let store = Arc::new(MemoryStore::new());
        let collector = daily_collector(store.clone(), &fetch);
        // A server with the first pack collected both four hours ago.
        let narrow = [
            daily_job("air", Kind::Airspace, first_region()),
            daily_job("sites", Kind::Sites, first_region()),
        ];
        for job in &narrow {
            ran_before(&collector, job, first_region(), &["DE"], 4, true).await;
        }

        // It starts again with three packs.
        let wide = vec![
            daily_job("air", Kind::Airspace, three_regions()),
            daily_job("sites", Kind::Sites, three_regions()),
        ];
        let calls = minutes_of_calls(collector, wide, &fetch, no_jitter, 30 * 60).await;

        // Both run at once, and then at their cadence, not again early.
        assert_eq!(hours_of(&calls, "air"), [0.0, 24.0]);
        assert_eq!(hours_of(&calls, "sites"), [0.0, 24.0]);
        // What is stored now covers the plan.
        let air = SourceId::new("air").unwrap();
        let sets = store
            .airspace_sets(std::slice::from_ref(&air))
            .await
            .unwrap();
        assert_eq!(sets[0].coverage.boxes, [three_regions()]);
        let sites = SourceId::new("sites").unwrap();
        let sets = store.site_sets(std::slice::from_ref(&sites)).await.unwrap();
        assert_eq!(sets[0].covers, [three_regions()]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_same_plan_or_a_smaller_one_waits_out_the_cadence() {
        for (stored, planned) in [
            // Restarted with the same packs.
            (first_region(), first_region()),
            // Restarted with fewer: the stored set covers more than asked.
            (three_regions(), first_region()),
        ] {
            let fetch = ScriptedFetch::working();
            let collector = daily_collector(Arc::new(MemoryStore::new()), &fetch);
            let jobs = vec![
                daily_job("air", Kind::Airspace, planned),
                daily_job("sites", Kind::Sites, planned),
            ];
            for job in &jobs {
                ran_before(&collector, job, stored, &["DE"], 4, true).await;
            }

            let calls = minutes_of_calls(collector, jobs, &fetch, no_jitter, 22 * 60).await;

            assert_eq!(hours_of(&calls, "air"), [20.0]);
            assert_eq!(hours_of(&calls, "sites"), [20.0]);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn airspace_fetched_for_fewer_countries_than_the_adapter_now_fetches_runs_at_once() {
        let fetch = ScriptedFetch::working();
        let collector = daily_collector(Arc::new(MemoryStore::new()), &fetch);
        let job = daily_job("air", Kind::Airspace, first_region());
        // The same box, but the set holds no country the manifest names.
        ran_before(&collector, &job, first_region(), &["AT"], 4, true).await;

        let calls = minutes_of_calls(collector, vec![job], &fetch, no_jitter, 60).await;

        assert_eq!(hours_of(&calls, "air"), [0.0]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_early_run_is_retried_by_the_ordinary_rule() {
        // The early run fails: the next try is a cadence later, not sooner.
        let fetch = ScriptedFetch::answering(Duration::ZERO, [false]);
        let collector = daily_collector(Arc::new(MemoryStore::new()), &fetch);
        let narrow = daily_job("air", Kind::Airspace, first_region());
        ran_before(&collector, &narrow, first_region(), &["DE"], 4, true).await;
        let wide = daily_job("air", Kind::Airspace, three_regions());

        let calls =
            minutes_of_calls(collector, vec![wide.clone()], &fetch, no_jitter, 30 * 60).await;

        assert_eq!(hours_of(&calls, "air"), [0.0, 24.0]);

        // A server that starts again after such a failure does not try
        // again at once either: it waits out the cadence from the failed run.
        let fetch = ScriptedFetch::working();
        let collector = daily_collector(Arc::new(MemoryStore::new()), &fetch);
        ran_before(&collector, &narrow, first_region(), &["DE"], 30, true).await;
        ran_before(&collector, &wide, three_regions(), &["DE"], 2, false).await;

        let calls = minutes_of_calls(collector, vec![wide], &fetch, no_jitter, 23 * 60).await;

        assert_eq!(hours_of(&calls, "air"), [22.0]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_weekly_source_whose_run_failed_is_tried_again_hours_later() {
        // A source that runs once a week failed two hours before the server
        // started again. It is tried four hours on, six after the failure;
        // that try fails too and the next is six hours later still, and
        // works.
        let fetch = ScriptedFetch::answering(Duration::ZERO, [false, true]);
        let mut registry = Registry::new();
        registry
            .register_airspace(Box::new(TestSource::new("weekly", 7 * 86_400)))
            .unwrap();
        let collector = Arc::new(Collector::new(
            registry,
            Arc::new(MemoryStore::new()),
            fetch.clone(),
            None,
            Arc::new(VirtualClock::new()),
            Some(chrono::Duration::hours(3)),
        ));
        let job = daily_job("weekly", Kind::Airspace, first_region());
        ran_before(&collector, &job, first_region(), &["DE"], 2, false).await;

        let calls = minutes_of_calls(collector, vec![job], &fetch, no_jitter, 24 * 60).await;

        assert_eq!(hours_of(&calls, "weekly"), [4.0, 10.0]);
    }
}
