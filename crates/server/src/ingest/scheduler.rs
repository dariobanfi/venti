//! Runs every source at its cadence for as long as the server lives.

use std::sync::Arc;

use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use venti_sources::{Archive, Fetch};
use venti_store::Store;

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
    let mut next = Instant::now() + pace.first_delay(since_last_start(&collector, &job).await);
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

/// How long ago the source's latest run started, by the run history. `None`
/// when it never ran or the history cannot be read.
async fn since_last_start<S: Store, F: Fetch, A: Archive>(
    collector: &Collector<S, F, A>,
    job: &Job,
) -> Option<std::time::Duration> {
    let status = collector
        .store()
        .source_status(std::slice::from_ref(&job.source))
        .await
        .ok()?;
    let last_started = status.first()?.last_started?;
    // A start time in the future means a clock was set back; the full
    // cadence is then the longest sensible wait.
    Some(
        (collector.now() - last_started)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO),
    )
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
        Observation, Polygon, SourceId, Station, StationId, Timestamp, Validity, VerticalLimit,
    };
    use venti_sources::{
        AirspaceSource, FetchError, Fetched, Kind, Manifest, ObservationSource, Parsed,
        ParsedAirspace, PlanCtx, Registry, Request, SourceError,
    };
    use venti_store::testing::TroubledStore;
    use venti_store::{MemoryStore, RunRecord};

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
                kinds = ["observations", "airspace"]
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
            let point = |lat, lon| GeoPoint::new(lat, lon).unwrap();
            let ring = vec![
                point(47.0, 11.0),
                point(47.0, 11.2),
                point(47.1, 11.2),
                point(47.0, 11.0),
            ];
            Ok(ParsedAirspace {
                airspaces: vec![Airspace {
                    id: AirspaceId::new(self.manifest.id.clone(), "1").unwrap(),
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
                }],
                ..ParsedAirspace::default()
            })
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
}
