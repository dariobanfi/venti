//! Forecasts for a place, fetched when somebody asks.
//!
//! A point forecast is the one kind of data the server does not collect
//! ahead: the Open-Meteo instance beside it already answers for any place.
//! [`ForecastService`] asks it when a place is asked for and keeps the
//! answer for a while. `docs/architecture.md` says why; this is how.
//!
//! - **A place is a cell.** The place asked for is moved to the nearest
//!   point of a grid of [`FORECAST_GRID_DEG`] degrees, and that point is
//!   what the source is asked for. Everyone who asks inside the cell gets
//!   the same answer.
//! - **An answer is kept** for [`KEEP`]. An answer that lacks something
//!   (one model's request failed) is handed on and kept for
//!   [`KEEP_PARTIAL`] only, so that it is soon tried again.
//! - **A cell is fetched once at a time.** Whoever asks while it is being
//!   fetched waits for the same answer. The fetching goes on if they give
//!   up, and the next one to ask finds it done.
//! - **The source is asked no more than its manifest allows in an hour.**
//!   Every request to it is counted, whoever caused it. When the hour's
//!   share is used up, a place that is not at hand is refused until there
//!   is room again. A tenth of the hour is held back for the server's own
//!   regular look at the source, so that a busy hour does not make a
//!   healthy source look dead.
//! - **Only where the server's regions are.** A place outside the area a
//!   source is listed for is refused before anything is asked.
//!
//! # Where the address comes from
//!
//! This module is the one way a request to the API can make the server ask
//! a provider. What the request contributes is two numbers, checked and
//! rounded. The address that is asked is the adapter's, which is made from
//! the server's configuration when it starts; nothing a client sends is
//! ever part of a host name or a path, and the fetcher is the one every
//! source uses.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use chrono::Duration;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use tokio::sync::Semaphore;
use venti_api_types::v1::FORECAST_GRID_DEG;
use venti_domain::{BoundingBox, ForecastSeries, GeoPoint, SourceId, Timestamp};
use venti_sources::{
    Archive, ArchiveEntry, ArchiveError, Fetch, FetchError, Fetched, ForecastRunOutput,
    ForecastSource, Kind, PlanCtx, Request, RunOptions, run_forecast,
};

use crate::clock::Clock;
use crate::plan::Job;

/// How long an answer is reused. A model starts a new run every three
/// hours; half an hour is the longest a pilot looks at the run before the
/// newest without the answer saying so.
pub const KEEP: Duration = Duration::minutes(30);

/// How long an answer that lacks something is reused.
pub const KEEP_PARTIAL: Duration = Duration::minutes(2);

/// How long a request waits for a forecast that is being fetched before it
/// is told to come back. An instance that has to read a place for the
/// first time can take longer than a client should be kept waiting.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(8);

/// After how many seconds a client that was told to come back should.
const COME_BACK_S: u64 = 5;

/// How many places are fetched at the same time.
const AT_ONCE: usize = 2;

/// The part of a source's hourly requests that only the server's own look
/// at it may use: one in this many.
const HELD_BACK: usize = 10;

/// How many requests to a source one hour's memory holds at most, where
/// the manifest names no limit.
const REMEMBERED: usize = 10_000;

/// What the weather models say about one place.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// The place the forecast is for: a point of the grid.
    pub point: GeoPoint,
    /// When the server asked its sources.
    pub fetched_at: Timestamp,
    /// The series of each source that has any for the place, by source.
    pub layers: Vec<(SourceId, Vec<ForecastSeries>)>,
}

/// Why there is no forecast for a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The place lies outside the area forecasts are answered for.
    OutsideRegion,
    /// The forecast is being fetched and is not there yet.
    NotYet {
        /// Seconds after which asking again makes sense.
        retry_after_s: u64,
    },
    /// The source has been asked as often as it may be in an hour.
    AskedEnough {
        /// Seconds until it may be asked again.
        retry_after_s: u64,
    },
    /// The source did not answer, or answered nothing that can be used.
    SourceFailed,
}

/// What the API and the collector ask of the forecast service.
pub trait Forecasts: Send + Sync {
    /// The forecast for the cell a place lies in: the one at hand if it is
    /// young enough, a fetched one otherwise.
    fn forecast(&self, point: GeoPoint) -> BoxFuture<'_, Result<Arc<Forecast>, Refusal>>;

    /// Whether asking for this place now would make the service ask its
    /// sources: the place is one it answers for, no forecast for its cell
    /// is at hand that is young enough, and none is being fetched.
    ///
    /// The API asks this to count such requests per client. What the
    /// sources may be asked in an hour is shared by everyone, and without
    /// a count of its own one client asking for place after place uses
    /// it up for all.
    fn would_fetch(&self, point: GeoPoint) -> bool;

    /// The server's regular look at a source: a forecast for the middle of
    /// the area the source is listed for, fetched now whatever is at hand,
    /// and archived. `None` if the service has no such source.
    fn look(&self, source: &SourceId) -> BoxFuture<'_, Option<ForecastRunOutput>>;
}

/// The forecast service for what a server collects: `sources` are the
/// forecast adapters it has, `jobs` its plan. `None` when the plan holds no
/// forecast source, which is a server that was not told where its forecast
/// service is, or whose regions name none.
///
/// A source is asked for places in the area of its job, which is the box
/// around the regions that list it.
pub fn service_for<F, A>(
    jobs: &[Job],
    sources: Vec<Box<dyn ForecastSource>>,
    fetch: F,
    archive: A,
    clock: Arc<dyn Clock>,
    asking: Asking,
) -> Option<Arc<dyn Forecasts>>
where
    F: Fetch + Send + 'static,
    A: Archive + Send + 'static,
{
    let planned: Vec<(Box<dyn ForecastSource>, BoundingBox)> = sources
        .into_iter()
        .filter_map(|source| {
            let id = &source.manifest().id;
            let area = jobs
                .iter()
                .find(|job| job.kind == Kind::Forecasts && &job.source == id)
                .map(|job| job.area);
            if area.is_none() {
                tracing::warn!(
                    source = %id,
                    "the server knows where this forecast source is, and no region lists it: \
                     no forecasts from it"
                );
            }
            area.map(|area| (source, area))
        })
        .collect();
    if planned.is_empty() {
        return None;
    }
    Some(Arc::new(ForecastService::new(
        planned, fetch, archive, clock, asking,
    )))
}

/// A cell of the grid, by the number of grid steps from the equator and
/// from Greenwich.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Cell {
    lat: i32,
    lon: i32,
}

impl Cell {
    /// The cell a place lies in.
    fn of(point: GeoPoint) -> Self {
        // A step count fits an `i32` many times over: the globe is 9000
        // steps of 0.02° from pole to pole.
        #[expect(clippy::cast_possible_truncation)]
        let steps = |degrees: f64| (degrees / FORECAST_GRID_DEG).round() as i32;
        Self {
            lat: steps(point.lat()),
            lon: steps(point.lon()),
        }
    }

    /// The point of the grid in the middle of the cell.
    fn point(self) -> GeoPoint {
        // Dividing by the number of steps in a degree gives 47.72 where
        // multiplying by the step gives 47.720000000000006.
        let per_degree = (1.0 / FORECAST_GRID_DEG).round();
        GeoPoint::new(
            f64::from(self.lat) / per_degree,
            f64::from(self.lon) / per_degree,
        )
        .expect("a point of the grid made from a place on the globe is on the globe")
    }
}

/// The requests made to one source in the last hour.
struct Budget {
    /// Most requests in any hour. `None` counts nothing: the development
    /// mode's clock stands still, so its hour would never end.
    per_hour: Option<usize>,
    clock: Arc<dyn Clock>,
    made: Mutex<VecDeque<Timestamp>>,
}

impl Budget {
    fn new(per_hour: Option<usize>, clock: Arc<dyn Clock>) -> Self {
        Self {
            per_hour,
            clock,
            made: Mutex::new(VecDeque::new()),
        }
    }

    /// The limit for what a request to the API causes: the hour's requests
    /// without the part held back for the server's own look.
    fn for_requests(&self) -> Option<usize> {
        self.per_hour
            .map(|limit| limit - (limit / HELD_BACK).max(1).min(limit))
    }

    /// The requests of the last hour, oldest first, without older ones.
    fn recent(&self) -> MutexGuard<'_, VecDeque<Timestamp>> {
        let mut made = unpoisoned(self.made.lock());
        let hour_ago = self.clock.now() - Duration::hours(1);
        while made.front().is_some_and(|time| *time <= hour_ago) {
            made.pop_front();
        }
        made
    }

    /// Counts one request if fewer than `limit` were made in the last hour.
    fn take(&self, limit: Option<usize>) -> bool {
        let mut made = self.recent();
        if limit.is_some_and(|limit| made.len() >= limit) {
            return false;
        }
        if limit.is_some() || made.len() < REMEMBERED {
            made.push_back(self.clock.now());
        }
        true
    }

    /// For how many seconds no further request fits under `limit`: zero if
    /// one fits now.
    fn full_for_s(&self, limit: Option<usize>) -> u64 {
        let Some(limit) = limit else { return 0 };
        let made = self.recent();
        if made.len() < limit {
            return 0;
        }
        // Room appears when the request that is `limit` from the end is an
        // hour old.
        let leaves = made[made.len() - limit.max(1)] + Duration::hours(1);
        u64::try_from((leaves - self.clock.now()).num_seconds())
            .unwrap_or(0)
            .max(1)
    }
}

/// A fetcher that counts every request against a source's hour.
struct Counted<'a, F> {
    fetch: &'a F,
    budget: &'a Budget,
    limit: Option<usize>,
}

impl<F> Counted<'_, F> {
    /// Counts one request, or says that the hour's requests are used up.
    fn count(&self) -> Result<(), FetchError> {
        if self.budget.take(self.limit) {
            return Ok(());
        }
        Err(FetchError::Transport(
            "not asked: the source has been asked as often as it may be in an hour".to_owned(),
        ))
    }
}

impl<F: Fetch> Fetch for Counted<'_, F> {
    async fn get(&self, request: &Request) -> Result<Fetched, FetchError> {
        self.count()?;
        self.fetch.get(request).await
    }

    // Passed on with its bound, so that the fetcher behind this one stops
    // reading where the source's manifest says, and not where a fetcher
    // stops that was told nothing.
    async fn get_within(&self, request: &Request, max_bytes: usize) -> Result<Fetched, FetchError> {
        self.count()?;
        self.fetch.get_within(request, max_bytes).await
    }
}

/// The archive of a forecast nobody keeps the payloads of.
struct NotArchived;

impl Archive for NotArchived {
    fn store(&self, _: ArchiveEntry<'_>) -> Result<(), ArchiveError> {
        Ok(())
    }
}

/// A forecast source with where it is asked and how often it has been.
struct Covered {
    source: Box<dyn ForecastSource>,
    /// The area the source is listed for.
    area: BoundingBox,
    budget: Budget,
}

type Outcome = Result<Arc<Forecast>, Refusal>;

/// What the service holds for a cell.
enum Slot {
    /// It is being fetched.
    Fetching(Shared<BoxFuture<'static, Outcome>>),
    /// A forecast, and the moment from which it is no longer handed out.
    Kept {
        forecast: Arc<Forecast>,
        until: Timestamp,
    },
}

/// What a forecast service asks its sources.
#[derive(Debug, Clone, PartialEq)]
pub enum Asking {
    /// Live sources: any place in their areas, each source no more often in
    /// an hour than its manifest allows.
    Live,
    /// Recorded answers, which exist for these places and no others. This
    /// is the development mode: a place anywhere in a source's area is
    /// answered with the forecast of the nearest of them, and the answer
    /// says which place it is for. Nothing is counted, because that mode's
    /// clock stands still and its hour would never end.
    Recorded(Vec<GeoPoint>),
}

struct Inner<F, A> {
    sources: Vec<Covered>,
    fetch: F,
    archive: A,
    clock: Arc<dyn Clock>,
    /// The only places there are answers for, if the answers are recorded.
    recorded: Option<Vec<GeoPoint>>,
    cells: Mutex<HashMap<Cell, Slot>>,
    fetching: Semaphore,
}

/// Fetches forecasts when they are asked for and keeps them for a while.
pub struct ForecastService<F, A>(Arc<Inner<F, A>>);

/// A lock is held only to read or change a map; a panic elsewhere while it
/// was held leaves the map as it was.
fn unpoisoned<T>(lock: Result<T, PoisonError<T>>) -> T {
    lock.unwrap_or_else(PoisonError::into_inner)
}

impl<F, A> ForecastService<F, A>
where
    F: Fetch + Send + 'static,
    A: Archive + Send + 'static,
{
    /// A service over `sources`, each with the area it is listed for.
    ///
    /// `fetch` reaches the sources and `archive` keeps the payloads of the
    /// server's own looks.
    pub fn new(
        sources: Vec<(Box<dyn ForecastSource>, BoundingBox)>,
        fetch: F,
        archive: A,
        clock: Arc<dyn Clock>,
        asking: Asking,
    ) -> Self {
        let counted = asking == Asking::Live;
        let recorded = match asking {
            Asking::Live => None,
            Asking::Recorded(places) => Some(places),
        };
        let sources = sources
            .into_iter()
            .map(|(source, area)| {
                let per_hour = source
                    .manifest()
                    .rate_limit
                    .per_hour
                    .filter(|_| counted)
                    .map(|limit| usize::try_from(limit).unwrap_or(usize::MAX));
                Covered {
                    budget: Budget::new(per_hour, clock.clone()),
                    source,
                    area,
                }
            })
            .collect();
        Self(Arc::new(Inner {
            sources,
            fetch,
            archive,
            clock,
            recorded,
            cells: Mutex::new(HashMap::new()),
            fetching: Semaphore::new(AT_ONCE),
        }))
    }
}

impl<F, A> Inner<F, A>
where
    F: Fetch + Send + 'static,
    A: Archive + Send + 'static,
{
    /// The sources that are listed for a place, by their position.
    /// The cell a place is answered with. Recorded answers exist for a few
    /// places: the nearest stands in, and the answer names it.
    fn cell_of(&self, point: GeoPoint) -> Cell {
        let nearest = self.recorded.as_ref().and_then(|places| {
            places
                .iter()
                .copied()
                .min_by(|a, b| a.distance_m(point).total_cmp(&b.distance_m(point)))
        });
        Cell::of(nearest.unwrap_or(point))
    }

    fn covering(&self, point: GeoPoint) -> Vec<usize> {
        (0..self.sources.len())
            .filter(|&index| self.sources[index].area.contains(point))
            .collect()
    }

    /// Asks the sources at `asked` for a cell and keeps what they deliver.
    /// `look` names the source of the server's own look, if this is one:
    /// its payloads are archived, it may use the whole of the hour's
    /// requests, and its output is handed back.
    async fn fetch(
        &self,
        cell: Cell,
        asked: &[usize],
        look: Option<usize>,
    ) -> (Outcome, Option<ForecastRunOutput>) {
        // Never closed, so this cannot fail.
        let _turn = self.fetching.acquire().await;
        let point = cell.point();
        let started = self.clock.now();
        let options = RunOptions::default();

        let mut layers = Vec::new();
        let mut whole = true;
        let mut looked = None;
        for &index in asked {
            let covered = &self.sources[index];
            let source = covered.source.as_ref();
            let id = &source.manifest().id;
            let ctx = PlanCtx {
                now: started,
                area: covered.area,
            };
            let output = if look == Some(index) {
                let fetch = Counted {
                    fetch: &self.fetch,
                    budget: &covered.budget,
                    limit: covered.budget.per_hour,
                };
                run_forecast(source, point, &fetch, &self.archive, &ctx, &options).await
            } else {
                let fetch = Counted {
                    fetch: &self.fetch,
                    budget: &covered.budget,
                    limit: covered.budget.for_requests(),
                };
                run_forecast(source, point, &fetch, &NotArchived, &ctx, &options).await
            };
            if !output.report.is_healthy() {
                whole = false;
                let failed = output.report.failures.first();
                tracing::warn!(
                    source = %id,
                    "a forecast for {:.2} N {:.2} E is not whole: {} series, {} of {} requests failed{}",
                    point.lat(),
                    point.lon(),
                    output.report.series,
                    output.report.failures.len(),
                    output.report.requests,
                    failed.map_or_else(String::new, |failure| format!(", first: {}", failure.error)),
                );
            }
            if !output.series.is_empty() {
                layers.push((id.clone(), output.series.clone()));
            }
            if look == Some(index) {
                looked = Some(output);
            }
        }

        let forecast = (!layers.is_empty()).then(|| {
            Arc::new(Forecast {
                point,
                fetched_at: started,
                layers,
            })
        });
        // A look asks one source. Where a place has several, its answer is
        // not the place's whole forecast and is not kept as that.
        let is_the_whole_answer = asked.len() == self.covering(point).len();
        let mut cells = unpoisoned(self.cells.lock());
        let now = self.clock.now();
        cells.retain(|_, slot| match slot {
            Slot::Fetching(_) => true,
            Slot::Kept { until, .. } => now < *until,
        });
        match &forecast {
            Some(forecast) if is_the_whole_answer => {
                let keep = if whole { KEEP } else { KEEP_PARTIAL };
                let kept = Slot::Kept {
                    forecast: forecast.clone(),
                    until: started + keep,
                };
                cells.insert(cell, kept);
            }
            // A request that was waiting for this cell is answered by the
            // outcome itself; nothing stays behind.
            _ if look.is_none() => {
                cells.remove(&cell);
            }
            _ => {}
        }
        (forecast.ok_or(Refusal::SourceFailed), looked)
    }
}

impl<F, A> Forecasts for ForecastService<F, A>
where
    F: Fetch + Send + 'static,
    A: Archive + Send + 'static,
{
    fn would_fetch(&self, point: GeoPoint) -> bool {
        let inner = &self.0;
        if inner.covering(point).is_empty() {
            return false;
        }
        let now = inner.clock.now();
        let cells = unpoisoned(inner.cells.lock());
        match cells.get(&inner.cell_of(point)) {
            Some(Slot::Kept { until, .. }) => now >= *until,
            Some(Slot::Fetching(_)) => false,
            None => true,
        }
    }

    fn forecast(&self, point: GeoPoint) -> BoxFuture<'_, Result<Arc<Forecast>, Refusal>> {
        let inner = &self.0;
        async move {
            let asked = inner.covering(point);
            if asked.is_empty() {
                return Err(Refusal::OutsideRegion);
            }
            let cell = inner.cell_of(point);
            let now = inner.clock.now();

            let fetching = {
                let mut cells = unpoisoned(inner.cells.lock());
                match cells.get(&cell) {
                    Some(Slot::Kept { forecast, until }) if now < *until => {
                        return Ok(forecast.clone());
                    }
                    Some(Slot::Fetching(fetching)) => fetching.clone(),
                    _ => {
                        // Not at hand: is there room to ask? A source that
                        // is full refuses the place even if another has
                        // room, since half a forecast would be kept as one.
                        let full_for_s = asked
                            .iter()
                            .map(|&index| {
                                let budget = &inner.sources[index].budget;
                                budget.full_for_s(budget.for_requests())
                            })
                            .max()
                            .unwrap_or(0);
                        if full_for_s > 0 {
                            return Err(Refusal::AskedEnough {
                                retry_after_s: full_for_s,
                            });
                        }
                        // The fetching is a task of its own: it finishes
                        // and is kept also when whoever asked gives up.
                        let service = inner.clone();
                        let task =
                            tokio::spawn(async move { service.fetch(cell, &asked, None).await.0 });
                        let fetching = async move {
                            // A task ends without an outcome only when the
                            // adapter panicked.
                            task.await.unwrap_or(Err(Refusal::SourceFailed))
                        }
                        .boxed()
                        .shared();
                        cells.insert(cell, Slot::Fetching(fetching.clone()));
                        fetching
                    }
                }
            };

            match tokio::time::timeout(PATIENCE, fetching).await {
                Ok(outcome) => {
                    if outcome.is_err() {
                        // A task that panicked left its mark behind.
                        let mut cells = unpoisoned(inner.cells.lock());
                        if matches!(cells.get(&cell), Some(Slot::Fetching(_))) {
                            cells.remove(&cell);
                        }
                    }
                    outcome
                }
                Err(_) => Err(Refusal::NotYet {
                    retry_after_s: COME_BACK_S,
                }),
            }
        }
        .boxed()
    }

    fn look(&self, source: &SourceId) -> BoxFuture<'_, Option<ForecastRunOutput>> {
        let inner = &self.0;
        let index = inner
            .sources
            .iter()
            .position(|covered| &covered.source.manifest().id == source);
        async move {
            let index = index?;
            let area = &inner.sources[index].area;
            let middle = GeoPoint::new(
                f64::midpoint(area.south(), area.north()),
                f64::midpoint(area.west(), area.east()),
            )
            .ok()?;
            let (_, looked) = inner.fetch(Cell::of(middle), &[index], Some(index)).await;
            looked
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{TimeZone, Utc};
    use venti_sources::testing::FixtureFetch;
    use venti_sources_openmeteo::{OpenMeteo, fixtures};

    use super::*;

    /// A clock the test moves.
    struct MovedClock(Mutex<Timestamp>);

    impl MovedClock {
        fn at(time: Timestamp) -> Arc<Self> {
            Arc::new(Self(Mutex::new(time)))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    impl Clock for MovedClock {
        fn now(&self) -> Timestamp {
            *self.0.lock().unwrap()
        }
    }

    /// The recorded answers, with a count of the requests made.
    struct Recorded {
        answers: FixtureFetch,
        asked: Arc<AtomicUsize>,
    }

    impl Fetch for Recorded {
        async fn get(&self, request: &Request) -> Result<Fetched, FetchError> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            // Let other tasks run, as a network would.
            tokio::task::yield_now().await;
            self.answers.get(request).await
        }
    }

    fn first_region() -> BoundingBox {
        BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
    }

    fn tegernsee() -> GeoPoint {
        GeoPoint::new(47.70, 11.80).unwrap()
    }

    struct Setup {
        service: ForecastService<Recorded, NotArchived>,
        clock: Arc<MovedClock>,
        asked: Arc<AtomicUsize>,
    }

    fn setup(asking: Asking) -> Setup {
        let clock = MovedClock::at(fixtures::recorded_at());
        let asked = Arc::new(AtomicUsize::new(0));
        let fetch = Recorded {
            answers: FixtureFetch::new().with_all(fixtures::recorded()),
            asked: asked.clone(),
        };
        let source: Box<dyn ForecastSource> = Box::new(OpenMeteo::new(fixtures::BASE).unwrap());
        let service = ForecastService::new(
            vec![(source, first_region())],
            fetch,
            NotArchived,
            clock.clone(),
            asking,
        );
        Setup {
            service,
            clock,
            asked,
        }
    }

    #[test]
    fn a_place_is_moved_to_the_grid() {
        let cell = Cell::of(GeoPoint::new(47.6652, 11.7954).unwrap());
        assert_eq!(
            cell,
            Cell {
                lat: 2383,
                lon: 590
            }
        );
        let point = cell.point();
        assert_eq!((point.lat(), point.lon()), (47.66, 11.8));
        // Written as people write it, not with the noise of a product.
        assert_eq!(
            Cell::of(GeoPoint::new(47.715, 11.0).unwrap()).point().lat(),
            47.72
        );
        // West and south of zero, and the edges of the globe.
        let cell = Cell::of(GeoPoint::new(-33.917, -70.005).unwrap());
        assert_eq!((cell.point().lat(), cell.point().lon()), (-33.92, -70.0));
        let corner = Cell::of(GeoPoint::new(90.0, 180.0).unwrap()).point();
        assert_eq!((corner.lat(), corner.lon()), (90.0, 180.0));
    }

    #[tokio::test]
    async fn an_answer_is_fetched_once_and_reused_for_half_an_hour() {
        let Setup {
            service,
            clock,
            asked,
        } = setup(Asking::Live);

        let first = service.forecast(tegernsee()).await.unwrap();
        assert_eq!(asked.load(Ordering::SeqCst), 4);
        assert_eq!(first.point, tegernsee());
        assert_eq!(first.fetched_at, fixtures::recorded_at());
        assert_eq!(first.layers.len(), 1);
        assert_eq!(first.layers[0].0.as_str(), "open-meteo");
        let models: Vec<&str> = first.layers[0].1.iter().map(|s| s.model.as_str()).collect();
        assert_eq!(models, ["icon-d2", "icon-eu"]);

        // Anywhere in the same cell, and 29 minutes later: the same answer,
        // and the source is not asked.
        clock.advance(Duration::minutes(29));
        let nearby = GeoPoint::new(47.7049, 11.7951).unwrap();
        let again = service.forecast(nearby).await.unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(asked.load(Ordering::SeqCst), 4);

        // After half an hour it is fetched again.
        clock.advance(Duration::minutes(1));
        let later = service.forecast(tegernsee()).await.unwrap();
        assert_eq!(asked.load(Ordering::SeqCst), 8);
        assert_eq!(
            later.fetched_at,
            fixtures::recorded_at() + Duration::minutes(30)
        );
    }

    #[tokio::test]
    async fn many_who_ask_at_once_cause_one_fetch() {
        let Setup { service, asked, .. } = setup(Asking::Live);
        let service = Arc::new(service);

        let asking: Vec<_> = (0..20)
            .map(|_| {
                let service = service.clone();
                tokio::spawn(async move { service.forecast(tegernsee()).await })
            })
            .collect();
        for one in asking {
            assert!(one.await.unwrap().is_ok());
        }

        assert_eq!(asked.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn a_place_outside_the_regions_is_refused_and_nothing_is_asked() {
        let Setup { service, asked, .. } = setup(Asking::Live);
        let hamburg = GeoPoint::new(53.55, 10.0).unwrap();
        assert_eq!(service.forecast(hamburg).await, Err(Refusal::OutsideRegion));
        assert_eq!(asked.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_source_that_fails_is_not_kept_and_is_asked_again() {
        let Setup { service, asked, .. } = setup(Asking::Live);
        // Nothing was recorded for this place: the instance answers the
        // runs and fails the series.
        let elsewhere = GeoPoint::new(47.5, 11.1).unwrap();
        assert_eq!(
            service.forecast(elsewhere).await,
            Err(Refusal::SourceFailed)
        );
        assert_eq!(asked.load(Ordering::SeqCst), 4);
        assert_eq!(
            service.forecast(elsewhere).await,
            Err(Refusal::SourceFailed)
        );
        assert_eq!(asked.load(Ordering::SeqCst), 8);
    }

    #[tokio::test]
    async fn the_source_is_asked_no_more_than_its_hour_allows() {
        let Setup {
            service,
            clock,
            asked,
        } = setup(Asking::Live);
        // The manifest allows 400 requests an hour, of which a tenth is
        // held back: 360 for what requests cause, which is 90 forecasts.
        // Every one of these fails, which changes nothing about the count.
        let place = |n: u32| GeoPoint::new(46.5 + f64::from(n) * 0.02, 10.0).unwrap();
        for n in 0..90 {
            assert_eq!(service.forecast(place(n)).await, Err(Refusal::SourceFailed));
            clock.advance(Duration::seconds(10));
        }
        assert_eq!(asked.load(Ordering::SeqCst), 360);

        // The ninety-first is refused without a request, and told when the
        // first of the hour's requests is an hour old: in 45 minutes.
        let refused = service.forecast(place(90)).await;
        assert_eq!(
            refused,
            Err(Refusal::AskedEnough {
                retry_after_s: 2700
            })
        );
        assert_eq!(asked.load(Ordering::SeqCst), 360);

        // A place that is at hand is still answered.
        // (None is, here: every forecast failed. So fetch one by a look.)
        // The server's own look still gets through, on the part held back.
        let source = SourceId::new("open-meteo").unwrap();
        let looked = service.look(&source).await.unwrap();
        assert!(looked.report.is_healthy());
        assert_eq!(asked.load(Ordering::SeqCst), 364);
        let middle = GeoPoint::new(47.4, 13.3).unwrap();
        assert!(service.forecast(middle).await.is_ok());
        assert_eq!(asked.load(Ordering::SeqCst), 364);

        // When the hour has moved on there is room again.
        clock.advance(Duration::minutes(46));
        assert_eq!(
            service.forecast(place(90)).await,
            Err(Refusal::SourceFailed)
        );
        assert_eq!(asked.load(Ordering::SeqCst), 368);
    }

    #[tokio::test]
    async fn a_look_fetches_the_middle_of_the_area_and_keeps_it() {
        let Setup {
            service,
            clock,
            asked,
        } = setup(Asking::Live);
        let source = SourceId::new("open-meteo").unwrap();

        let looked = service.look(&source).await.unwrap();

        assert!(looked.report.is_healthy());
        assert_eq!(looked.report.requests, 4);
        assert_eq!(
            looked.report.newest_run,
            Some(Utc.with_ymd_and_hms(2026, 11, 20, 21, 0, 0).unwrap())
        );
        assert_eq!(looked.series[0].point, GeoPoint::new(47.4, 13.3).unwrap());
        // Whoever asks for that place gets what the look fetched.
        let middle = GeoPoint::new(47.405, 13.295).unwrap();
        assert_eq!(
            service.forecast(middle).await.unwrap().fetched_at,
            fixtures::recorded_at()
        );
        assert_eq!(asked.load(Ordering::SeqCst), 4);
        // A look always asks, also when the place is at hand.
        clock.advance(Duration::minutes(10));
        service.look(&source).await.unwrap();
        assert_eq!(asked.load(Ordering::SeqCst), 8);

        let unknown = SourceId::new("other").unwrap();
        assert!(service.look(&unknown).await.is_none());
    }

    #[tokio::test]
    async fn recorded_answers_stand_in_for_any_place_and_say_which_they_are_for() {
        // The development mode: answers exist for five places.
        let Setup { service, asked, .. } = setup(Asking::Recorded(fixtures::places()));

        // A place in the region that nothing was recorded for is answered
        // with the forecast of the nearest recorded place, which the
        // answer names: Garmisch gets the Wallberg's.
        let garmisch = GeoPoint::new(47.4917, 11.0955).unwrap();
        let answer = service.forecast(garmisch).await.unwrap();
        assert_eq!(answer.point, GeoPoint::new(47.66, 11.8).unwrap());
        for series in &answer.layers[0].1 {
            assert_eq!(series.point, answer.point);
        }
        // Vienna gets the middle of the region's, and the Tegernsee its own.
        let vienna = GeoPoint::new(48.2, 16.37).unwrap();
        let answer = service.forecast(vienna).await.unwrap();
        assert_eq!(answer.point, GeoPoint::new(47.4, 13.3).unwrap());
        let answer = service.forecast(tegernsee()).await.unwrap();
        assert_eq!(answer.point, tegernsee());
        assert_eq!(asked.load(Ordering::SeqCst), 12);

        // Outside the region there is still no forecast.
        let hamburg = GeoPoint::new(53.55, 10.0).unwrap();
        assert_eq!(service.forecast(hamburg).await, Err(Refusal::OutsideRegion));

        // Nothing is counted: that mode's clock stands still, and an hour
        // that filled would never empty. Every place is at hand after its
        // first fetch, so 500 more requests ask the source for nothing.
        for n in 0..500 {
            let place = GeoPoint::new(46.5 + f64::from(n) * 0.004, 10.0).unwrap();
            assert!(service.forecast(place).await.is_ok());
        }
        assert!(asked.load(Ordering::SeqCst) <= 20);
    }
}
