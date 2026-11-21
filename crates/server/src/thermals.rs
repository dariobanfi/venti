//! The thermal map: the kind of day, cell by cell, over the area the
//! server has forecasts for.
//!
//! An app works out what a day holds for one take-off from that place's
//! forecast. For a map of a region that would be some 1,800 forecasts on
//! every phone, so the server works the map out once for everyone, with
//! the rules a take-off's page uses (`venti_flight::day`), and serves it
//! as one answer (`GET /v1/thermals`). `docs/architecture.md` says why;
//! this is how.
//!
//! - **The grid.** A point every [`THERMAL_GRID_DEG`] degrees over the
//!   area the forecast source is listed for, on multiples of the step
//!   ([`grid_over`]). Each point is asked for as a place, and the model
//!   answers from its own cell nearest it.
//! - **One model.** The source's finest, [`MAP_MODEL`]. It reaches two
//!   days; ICON-EU's later days are a thing to do.
//! - **Made with each run.** When the server's regular look at the source
//!   sees a run of the model the map was not made from, the grid is
//!   fetched in requests of many places, one request at a time
//!   ([`ThermalMap::update`]). The map may use half of the source's hourly
//!   requests ([`Forecasts::places`]); when that is used up it stops and
//!   goes on at the next look, ten minutes later, and the map that was
//!   there is served until the new one is whole.
//! - **Kept per cell.** The judged daylight hours of each cell, an hour of
//!   a newer run replacing the same hour of an older one. A run says
//!   nothing of the hours before it began, so the morning of today stays
//!   the earlier run's. Hours of days before yesterday are dropped.
//! - **In memory only.** After a start the map is made again from the run
//!   at hand, and today then lacks the hours before that run began.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::Duration;
use venti_api_types::v1::{
    THERMAL_GRID_DEG, ThermalGrid, ThermalKind, ThermalMapDay, ThermalMapResponse, ThermalReason,
    ThermalVerdict,
};
use venti_domain::{BoundingBox, ForecastSeries, GeoPoint, ModelId, SourceId, Timestamp};
use venti_flight::day::{self, Kind, RatedHour, Reason, Verdict};

use crate::clock::Clock;
use crate::forecast::{Forecasts, Refusal};

/// The model the map is made of: ICON-D2, the finest the forecast source
/// has, some 2 km a cell, with a new run every three hours that reaches 48
/// hours. To do: the days after from ICON-EU.
pub const MAP_MODEL: &str = "icon-d2";

/// The grid of points every [`THERMAL_GRID_DEG`] degrees inside an area,
/// on multiples of the step. `None` for an area too small to hold a point.
#[must_use]
pub fn grid_over(area: BoundingBox) -> Option<ThermalGrid> {
    // Steps of the grid from the equator and from Greenwich. A little
    // allowance keeps 17.2 / 0.1 = 171.99999999999997 from losing a step.
    let per_degree = (1.0 / THERMAL_GRID_DEG).round();
    #[expect(clippy::cast_possible_truncation)]
    let steps = |degrees: f64, round: fn(f64) -> f64| round(degrees * per_degree) as i32;
    let up = |value: f64| (value - 1e-6).ceil();
    let down = |value: f64| (value + 1e-6).floor();
    let (south, north) = (steps(area.south(), up), steps(area.north(), down));
    let (west, east) = (steps(area.west(), up), steps(area.east(), down));
    let rows = u32::try_from(north - south + 1)
        .ok()
        .filter(|rows| *rows > 0)?;
    let columns = u32::try_from(east - west + 1)
        .ok()
        .filter(|cols| *cols > 0)?;
    Some(ThermalGrid {
        south: f64::from(south) / per_degree,
        west: f64::from(west) / per_degree,
        step_deg: THERMAL_GRID_DEG,
        rows,
        columns,
    })
}

/// The points of a grid, in the grid's order, written as people write
/// them: 47.3 and not 47.300000000000004, which is what a place is asked
/// for as and what its series names.
fn points_of(grid: &ThermalGrid) -> Vec<GeoPoint> {
    let per_degree = (1.0 / grid.step_deg).round();
    let first = |degrees: f64| (degrees * per_degree).round();
    (0..grid.cells())
        .filter_map(|cell| {
            let columns = grid.columns as usize;
            #[expect(clippy::cast_precision_loss)]
            let (row, column) = ((cell / columns) as f64, (cell % columns) as f64);
            GeoPoint::new(
                (first(grid.south) + row) / per_degree,
                (first(grid.west) + column) / per_degree,
            )
            .ok()
        })
        .collect()
}

/// What the map keeps of one cell.
#[derive(Debug, Clone, Default)]
struct Kept {
    /// The judged daylight hours by their time, each with the run it is
    /// from.
    hours: BTreeMap<Timestamp, (Timestamp, RatedHour)>,
    /// The height of the model's ground, from the newest series that had
    /// it.
    ground_m: Option<f64>,
}

impl Kept {
    /// Takes in a series of the cell: its daylight hours, judged, each
    /// replacing the same hour of an older run.
    fn take(&mut self, series: &ForecastSeries) {
        if series.cell_elevation_m.is_some() {
            self.ground_m = series.cell_elevation_m;
        }
        for hour in day::cell_hours(series) {
            let newer = self
                .hours
                .get(&hour.time)
                .is_none_or(|(run, _)| *run <= series.run);
            if newer {
                self.hours.insert(hour.time, (series.run, hour));
            }
        }
    }

    /// Drops the hours of days before yesterday, by the sun at the cell.
    fn drop_before_yesterday(&mut self, now: Timestamp, cell: GeoPoint) {
        // The sun's clock at the cell runs four minutes a degree ahead of
        // Greenwich's, as `venti_flight::day` counts its days.
        #[expect(clippy::cast_possible_truncation)]
        let ahead = Duration::seconds((cell.lon() * 240.0).round() as i64);
        let yesterday = (now + ahead).date_naive() - Duration::days(1);
        let from = day::daylight_hours(yesterday, cell)
            .first()
            .copied()
            .unwrap_or(now - Duration::days(2));
        self.hours.retain(|time, _| *time >= from);
    }
}

/// Where the making of a map stands.
#[derive(Debug, Clone)]
struct Making {
    /// The run of the model the look saw.
    run: Timestamp,
    /// The requests still to make, by their number: request `n` asks for
    /// the places `n * per_request ..`.
    left: Vec<usize>,
}

#[derive(Default)]
struct State {
    cells: Vec<Kept>,
    making: Option<Making>,
    /// The run the map was last made from, whole.
    made_from: Option<Timestamp>,
    /// The map as it is served, without the credit, which is the API's.
    map: Option<Arc<ThermalMapResponse>>,
}

/// The server's thermal map of one forecast source.
pub struct ThermalMap {
    forecasts: Arc<dyn Forecasts>,
    source: SourceId,
    model: ModelId,
    grid: ThermalGrid,
    points: Vec<GeoPoint>,
    per_request: usize,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
    /// Held while the map is being fetched: one making at a time.
    fetching: tokio::sync::Mutex<()>,
}

impl ThermalMap {
    /// The map of `source` over the area the forecast service lists it
    /// for, on the grid of [`grid_over`]. `None` if the service has no
    /// such source or cannot ask it for many places at once.
    pub fn for_source(
        forecasts: Arc<dyn Forecasts>,
        source: SourceId,
        clock: Arc<dyn Clock>,
    ) -> Option<Self> {
        let grid = grid_over(forecasts.area(&source)?)?;
        Self::on_grid(forecasts, source, grid, clock)
    }

    /// The map of `source` on a grid of its own. The development mode,
    /// which has recorded answers for a few places, uses this.
    pub fn on_grid(
        forecasts: Arc<dyn Forecasts>,
        source: SourceId,
        grid: ThermalGrid,
        clock: Arc<dyn Clock>,
    ) -> Option<Self> {
        let per_request = forecasts.places_per_request(&source);
        if per_request == 0 {
            tracing::warn!(
                source = %source,
                "this forecast source cannot be asked for many places at once: no thermal map"
            );
            return None;
        }
        let points = points_of(&grid);
        let state = State {
            cells: vec![Kept::default(); points.len()],
            ..State::default()
        };
        Some(Self {
            forecasts,
            source,
            model: ModelId::new(MAP_MODEL).expect("the model's name is an identifier"),
            grid,
            points,
            per_request,
            clock,
            state: Mutex::new(state),
            fetching: tokio::sync::Mutex::new(()),
        })
    }

    /// The source the map is made from.
    pub fn source(&self) -> &SourceId {
        &self.source
    }

    /// The model the map is made of.
    pub fn model(&self) -> &ModelId {
        &self.model
    }

    /// The map, once one was made. Its credit is empty: the API adds the
    /// source's.
    pub fn current(&self) -> Option<Arc<ThermalMapResponse>> {
        self.lock().map.clone()
    }

    /// How many cells the map has.
    pub fn cells(&self) -> usize {
        self.points.len()
    }

    /// How many requests a whole map takes.
    pub fn requests_per_map(&self) -> usize {
        self.points.len().div_ceil(self.per_request)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the server's look at the source tells the map: the run of the
    /// map's model that the source holds. A run the map was not made from
    /// starts the making of a new map; one being made goes on where it
    /// stopped. Returns when the map is whole, or when the source's hour
    /// has no more room for it or a request failed: the next look goes on
    /// from there. Does nothing while another call is fetching.
    pub async fn update(&self, seen: Timestamp) {
        let Ok(_fetching) = self.fetching.try_lock() else {
            return;
        };
        {
            let mut state = self.lock();
            let made = state.made_from.is_some_and(|run| run >= seen);
            match &state.making {
                Some(making) if making.run >= seen => {}
                _ if made => return,
                _ => {
                    state.making = Some(Making {
                        run: seen,
                        left: (0..self.requests_per_map()).collect(),
                    });
                }
            }
        }

        loop {
            let next = self
                .lock()
                .making
                .as_ref()
                .and_then(|making| making.left.first().copied());
            let Some(request) = next else { break };
            let from = request * self.per_request;
            let to = (from + self.per_request).min(self.points.len());
            let points = &self.points[from..to];
            let output = match self
                .forecasts
                .places(&self.source, &self.model, points)
                .await
            {
                Ok(output) => output,
                Err(Refusal::AskedEnough { retry_after_s }) => {
                    tracing::info!(
                        source = %self.source,
                        "the thermal map waits: it has used its share of the hour's requests, \
                         room again in {retry_after_s} s; {} of {} requests to go",
                        self.left(),
                        self.requests_per_map(),
                    );
                    return;
                }
                Err(refusal) => {
                    tracing::warn!(source = %self.source, "the thermal map waits: {refusal:?}");
                    return;
                }
            };
            if output.series.is_empty() {
                let failed = output.report.failures.first();
                tracing::warn!(
                    source = %self.source,
                    "the thermal map waits: no forecast for places {from}..{to}{}",
                    failed.map_or_else(String::new, |failure| format!(": {}", failure.error)),
                );
                return;
            }
            if output.series.len() < points.len() {
                tracing::warn!(
                    source = %self.source,
                    "the thermal map lacks {} of the places {from}..{to}; they keep what they had",
                    points.len() - output.series.len(),
                );
            }
            let mut state = self.lock();
            for series in &output.series {
                if let Some(at) = points.iter().position(|point| *point == series.point) {
                    state.cells[from + at].take(series);
                }
            }
            if let Some(making) = state.making.as_mut() {
                making.left.retain(|done| *done != request);
            }
        }

        let now = self.clock.now();
        let mut state = self.lock();
        let Some(making) = state.making.take() else {
            return;
        };
        state.made_from = Some(making.run);
        for (kept, point) in state.cells.iter_mut().zip(&self.points) {
            kept.drop_before_yesterday(now, *point);
        }
        let map = self.made(&state.cells, making.run, now);
        tracing::info!(
            source = %self.source,
            "thermal map made from the run of {}: {} cells, {} days, {} requests",
            making.run.format("%Y-%m-%d %H:%M UTC"),
            self.points.len(),
            map.days.len(),
            self.requests_per_map(),
        );
        state.map = Some(Arc::new(map));
    }

    fn left(&self) -> usize {
        self.lock()
            .making
            .as_ref()
            .map_or(0, |making| making.left.len())
    }

    /// The map the kept hours make.
    fn made(&self, cells: &[Kept], seen: Timestamp, now: Timestamp) -> ThermalMapResponse {
        let days: Vec<Vec<day::CellDay>> = cells
            .iter()
            .zip(&self.points)
            .map(|(kept, point)| {
                let hours: Vec<RatedHour> = kept.hours.values().map(|(_, hour)| *hour).collect();
                day::cell_days(&hours, *point)
            })
            .collect();
        let dates: BTreeSet<_> = days.iter().flatten().map(|day| day.date).collect();
        let days = dates
            .into_iter()
            .map(|date| {
                let of_date: Vec<Option<&day::CellDay>> = days
                    .iter()
                    .map(|cell| cell.iter().find(|day| day.date == date))
                    .collect();
                let thermals = |cell: &Option<&day::CellDay>| cell.and_then(|day| day.thermals);
                ThermalMapDay {
                    date,
                    whole: of_date.iter().flatten().all(|day| day.whole),
                    verdict: of_date
                        .iter()
                        .map(|cell| cell.map(|day| verdict(day.verdict)))
                        .collect(),
                    why: of_date
                        .iter()
                        .map(|cell| cell.and_then(|day| day.why).map(reason))
                        .collect(),
                    thermals: of_date
                        .iter()
                        .map(|cell| thermals(cell).map(|found| kind(found.kind)))
                        .collect(),
                    climb_ms: of_date
                        .iter()
                        .map(|cell| thermals(cell).map(|found| day::tenth(found.climb_ms)))
                        .collect(),
                    top_m: of_date
                        .iter()
                        .map(|cell| {
                            thermals(cell)
                                .and_then(|found| found.top_m)
                                .map(|top| metres(day::fifty(top)))
                        })
                        .collect(),
                    cloud: of_date
                        .iter()
                        .map(|cell| thermals(cell).is_some_and(|found| found.cloud))
                        .collect(),
                }
            })
            .collect();
        let run = cells
            .iter()
            .flat_map(|kept| kept.hours.values().map(|(run, _)| *run))
            .max()
            .unwrap_or(seen);
        ThermalMapResponse {
            source: self.source.clone(),
            model: self.model.clone(),
            run,
            computed_at: now,
            grid: self.grid,
            ground_m: cells.iter().map(|kept| kept.ground_m.map(metres)).collect(),
            days,
            attributions: Vec::new(),
        }
    }
}

/// A height in whole metres. Heights on the globe fit an `i32` many times
/// over.
#[expect(clippy::cast_possible_truncation)]
fn metres(value: f64) -> i32 {
    value.round() as i32
}

fn verdict(verdict: Verdict) -> ThermalVerdict {
    match verdict {
        Verdict::Flyable => ThermalVerdict::Flyable,
        Verdict::Marginal => ThermalVerdict::Marginal,
        Verdict::NotFlyable => ThermalVerdict::NotFlyable,
    }
}

fn reason(reason: Reason) -> ThermalReason {
    match reason {
        Reason::Rain => ThermalReason::Rain,
        Reason::Wind => ThermalReason::Wind,
        Reason::Gusts => ThermalReason::Gusts,
        Reason::WindAloft => ThermalReason::WindAloft,
        Reason::CloudBase => ThermalReason::CloudBase,
        Reason::Storms => ThermalReason::Storms,
    }
}

fn kind(kind: Kind) -> ThermalKind {
    match kind {
        Kind::Weak => ThermalKind::Weak,
        Kind::Local => ThermalKind::Local,
        Kind::CrossCountry => ThermalKind::CrossCountry,
        Kind::Big => ThermalKind::Big,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{TimeZone, Timelike, Utc};
    use futures::FutureExt;
    use futures::future::BoxFuture;
    use venti_domain::ForecastHour;
    use venti_sources::{ForecastRunOutput, ForecastRunReport};

    use super::*;
    use crate::clock::FixedClock;
    use crate::forecast::Forecast;

    fn at(day: u32, hour: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 7, day, hour, 0, 0).unwrap()
    }

    fn source() -> SourceId {
        SourceId::new("open-meteo").unwrap()
    }

    /// A forecast service that answers every place with a series of the
    /// run it is told, two places a request, and refuses when told to.
    struct Made {
        run: Mutex<Timestamp>,
        /// Requests answered.
        asked: AtomicUsize,
        /// Requests refused for the hour.
        refused: AtomicUsize,
        /// How many more requests it answers before it refuses; `None`
        /// for no end.
        room: Mutex<Option<usize>>,
    }

    impl Made {
        fn at(run: Timestamp) -> Arc<Self> {
            Arc::new(Self {
                run: Mutex::new(run),
                asked: AtomicUsize::new(0),
                refused: AtomicUsize::new(0),
                room: Mutex::new(None),
            })
        }

        fn series(&self, point: GeoPoint) -> ForecastSeries {
            let run = *self.run.lock().unwrap();
            let hours = (1..=48)
                .map(|hour| {
                    let mut hour = ForecastHour::empty(run + Duration::hours(hour));
                    hour.wind_speed_ms = Some(2.0);
                    hour.temperature_c = Some(20.0);
                    hour
                })
                .collect();
            ForecastSeries {
                source: source(),
                model: ModelId::new(MAP_MODEL).unwrap(),
                run,
                available_at: None,
                point,
                cell: point,
                cell_elevation_m: Some(800.0),
                hours,
            }
        }
    }

    impl Forecasts for Made {
        fn forecast(&self, _: GeoPoint) -> BoxFuture<'_, Result<Arc<Forecast>, Refusal>> {
            async { Err(Refusal::SourceFailed) }.boxed()
        }

        fn would_fetch(&self, _: GeoPoint) -> bool {
            false
        }

        fn look(&self, _: &SourceId) -> BoxFuture<'_, Option<ForecastRunOutput>> {
            async { None }.boxed()
        }

        fn area(&self, _: &SourceId) -> Option<BoundingBox> {
            BoundingBox::new(47.0, 11.0, 47.1, 11.2).ok()
        }

        fn places_per_request(&self, _: &SourceId) -> usize {
            2
        }

        fn places<'a>(
            &'a self,
            _: &'a SourceId,
            _: &'a ModelId,
            points: &'a [GeoPoint],
        ) -> BoxFuture<'a, Result<ForecastRunOutput, Refusal>> {
            async move {
                {
                    let mut room = self.room.lock().unwrap();
                    if *room == Some(0) {
                        self.refused.fetch_add(1, Ordering::SeqCst);
                        return Err(Refusal::AskedEnough { retry_after_s: 600 });
                    }
                    if let Some(left) = room.as_mut() {
                        *left -= 1;
                    }
                }
                self.asked.fetch_add(1, Ordering::SeqCst);
                let series: Vec<ForecastSeries> =
                    points.iter().map(|point| self.series(*point)).collect();
                let report = ForecastRunReport {
                    source: source(),
                    requests: 2,
                    failures: Vec::new(),
                    skipped: Vec::new(),
                    rejected: Vec::new(),
                    budget_exhausted: false,
                    series: series.len(),
                    hours: 48 * series.len(),
                    newest_run: series.first().map(|series| series.run),
                };
                Ok(ForecastRunOutput { series, report })
            }
            .boxed()
        }
    }

    fn map_of(made: &Arc<Made>, now: Timestamp) -> ThermalMap {
        ThermalMap::for_source(made.clone(), source(), Arc::new(FixedClock(now))).unwrap()
    }

    #[test]
    fn the_grid_lies_on_multiples_of_its_step_inside_the_area() {
        // The first region's box.
        let grid = grid_over(BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()).unwrap();
        assert_eq!((grid.south, grid.west), (46.3, 9.4));
        assert_eq!((grid.rows, grid.columns, grid.cells()), (23, 79, 1817));
        let points = points_of(&grid);
        assert_eq!(points.len(), 1817);
        let last = points[1816];
        assert_eq!((last.lat(), last.lon()), (48.5, 17.2));
        assert_eq!((points[80].lat(), points[80].lon()), (46.4, 9.5));
        for (cell, point) in points.iter().enumerate() {
            assert_eq!(grid.cell_at(*point), Some(cell));
        }

        // An edge between two points moves in to the next.
        let grid = grid_over(BoundingBox::new(46.31, 9.36, 46.52, 9.5).unwrap()).unwrap();
        assert_eq!(
            (grid.south, grid.west, grid.rows, grid.columns),
            (46.4, 9.4, 2, 2)
        );
        // An area between the points of the grid holds none.
        assert_eq!(
            grid_over(BoundingBox::new(46.31, 9.31, 46.39, 9.39).unwrap()),
            None
        );
    }

    #[tokio::test]
    async fn a_new_run_replaces_the_hours_it_has_and_leaves_the_morning_before_it() {
        let made = Made::at(at(12, 0));
        let map = map_of(&made, at(12, 4));
        assert_eq!(map.cells(), 6);
        assert_eq!(map.requests_per_map(), 3);
        assert!(map.current().is_none());

        map.update(at(12, 0)).await;
        let first = map.current().unwrap();
        assert_eq!(first.run, at(12, 0));
        assert_eq!(first.computed_at, at(12, 4));
        assert_eq!(made.asked.load(Ordering::SeqCst), 3);
        assert_eq!(first.ground_m, [Some(800); 6]);
        assert_eq!(first.days.len(), 2);
        assert!(first.days.iter().all(|day| day.verdict.len() == 6));

        // The same run again asks for nothing.
        map.update(at(12, 0)).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 3);

        // The run of 09 UTC: the hours from 10 UTC on are its, those of the
        // morning before stay the first run's.
        *made.run.lock().unwrap() = at(12, 9);
        map.update(at(12, 9)).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 6);
        assert_eq!(map.current().unwrap().run, at(12, 9));
        let state = map.lock();
        let runs = &state.cells[0].hours;
        assert_eq!(runs.get(&at(12, 8)).map(|(run, _)| *run), Some(at(12, 0)));
        assert_eq!(runs.get(&at(12, 9)).map(|(run, _)| *run), Some(at(12, 0)));
        assert_eq!(runs.get(&at(12, 10)).map(|(run, _)| *run), Some(at(12, 9)));
        assert_eq!(runs.get(&at(13, 10)).map(|(run, _)| *run), Some(at(12, 9)));
        // Each a daylight hour.
        assert!(runs.keys().all(|time| (4..=19).contains(&time.hour())));
    }

    #[tokio::test]
    async fn a_map_that_runs_out_of_room_goes_on_at_the_next_look() {
        let made = Made::at(at(12, 0));
        let map = map_of(&made, at(12, 4));
        *made.room.lock().unwrap() = Some(2);

        map.update(at(12, 0)).await;
        // Two requests of three, then the hour had no more room: no map
        // yet, and nothing asked after the refusal.
        assert!(map.current().is_none());
        assert_eq!(made.asked.load(Ordering::SeqCst), 2);
        assert_eq!(made.refused.load(Ordering::SeqCst), 1);
        assert_eq!(map.left(), 1);

        // The next look finds room, and only the last request is made.
        *made.room.lock().unwrap() = None;
        map.update(at(12, 0)).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 3);
        assert_eq!(map.current().unwrap().run, at(12, 0));

        // A newer run seen while a map is being made starts it again, and
        // the map that is there is served meanwhile.
        *made.run.lock().unwrap() = at(12, 3);
        *made.room.lock().unwrap() = Some(1);
        map.update(at(12, 3)).await;
        assert_eq!(map.left(), 2);
        *made.run.lock().unwrap() = at(12, 6);
        *made.room.lock().unwrap() = None;
        map.update(at(12, 6)).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 7);
        assert_eq!(map.current().unwrap().run, at(12, 6));
    }

    #[tokio::test]
    async fn days_before_yesterday_are_dropped() {
        let made = Made::at(at(12, 0));
        let early = map_of(&made, at(12, 4));
        early.update(at(12, 0)).await;
        let dates = |map: &ThermalMap| -> Vec<String> {
            let current = map.current().unwrap();
            current
                .days
                .iter()
                .map(|day| day.date.to_string())
                .collect()
        };
        assert_eq!(dates(&early), ["2026-07-12", "2026-07-13"]);

        // Three days on, a run of the 15th: the 13th is the day before
        // yesterday, the 12th and the 13th are gone.
        let map = map_of(&made, at(15, 4));
        map.update(at(12, 0)).await;
        *made.run.lock().unwrap() = at(15, 0);
        map.update(at(15, 0)).await;
        assert_eq!(dates(&map), ["2026-07-15", "2026-07-16"]);

        // On the 14th, the 13th is yesterday and stays.
        let map = map_of(&made, at(14, 4));
        *made.run.lock().unwrap() = at(12, 0);
        map.update(at(12, 0)).await;
        *made.run.lock().unwrap() = at(14, 0);
        map.update(at(14, 0)).await;
        assert_eq!(dates(&map), ["2026-07-13", "2026-07-14", "2026-07-15"]);
    }
}
