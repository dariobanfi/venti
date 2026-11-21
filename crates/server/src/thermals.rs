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
//! - **Two models.** The source's finest, ICON-D2, and ICON-EU for the
//!   days ICON-D2 does not have whole ([`MAP_MODELS`]). A day is one
//!   model's, never a blend: the finest model's, unless the coarser one
//!   has more of the day's hours, as a take-off's page decides it.
//! - **Made with each run.** When the server's regular look at the source
//!   sees a run of a model the map was not made from, the grid is
//!   fetched in requests of many places, one request at a time
//!   ([`ThermalMap::update`]). The map may use half of the source's hourly
//!   requests ([`Forecasts::places`]); when that is used up it stops and
//!   goes on at the next look, ten minutes later, and the map that was
//!   there is served until the new one is whole.
//! - **Kept per cell.** The judged daylight hours of each cell, an hour of
//!   a newer run replacing the same hour of an older one. A run says
//!   nothing of the hours before it began, so the morning of today stays
//!   the earlier run's. Hours of days before yesterday are dropped.
//! - **Written down.** Each time a model's part is made whole its judged
//!   hours go to the store ([`Keeper`]), and a start reads them back
//!   ([`ThermalMap::restore`]): today keeps the hours before the newest run
//!   began across a restart. Without a store, as in the development mode,
//!   the map is in memory only.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{Duration, NaiveDate};
use venti_api_types::v1::{
    THERMAL_GRID_DEG, ThermalGrid, ThermalKind, ThermalMapDay, ThermalMapResponse, ThermalReason,
    ThermalVerdict,
};
use venti_domain::{BoundingBox, ForecastSeries, GeoPoint, ModelId, SourceId, Timestamp};
use venti_flight::day::{self, Kind, RatedHour, Reason, Verdict};

use futures::FutureExt;
use futures::future::BoxFuture;
use venti_store::{Store, StoreError, ThermalPart};

use crate::clock::Clock;
use crate::forecast::{Forecasts, Refusal};

/// The models the map is made of, the finest first. ICON-D2: some 2 km a
/// cell, a new run every three hours that reaches 48 hours. ICON-EU: some
/// 5 km a cell, a run every three hours that reaches 78 hours from 00, 06,
/// 12 and 18 UTC and 30 from the runs between; the adapter asks for three
/// days, so it adds the day after tomorrow.
pub const MAP_MODELS: [&str; 2] = ["icon-d2", "icon-eu"];

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
        let yesterday = day::solar_date(now, cell) - Duration::days(1);
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

/// What the map keeps of one model.
struct Of {
    model: ModelId,
    cells: Vec<Kept>,
    making: Option<Making>,
    /// The run the map was last made from, whole.
    made_from: Option<Timestamp>,
}

struct State {
    /// By model, the finest first.
    models: Vec<Of>,
    /// The map as it is served, without the credit, which is the API's.
    map: Option<Arc<ThermalMapResponse>>,
}

/// The server's thermal map of one forecast source.
pub struct ThermalMap {
    forecasts: Arc<dyn Forecasts>,
    source: SourceId,
    grid: ThermalGrid,
    points: Vec<GeoPoint>,
    per_request: usize,
    clock: Arc<dyn Clock>,
    state: Mutex<State>,
    /// Held while the map is being fetched: one making at a time.
    fetching: tokio::sync::Mutex<()>,
    /// Where the judged hours are written down, if anywhere.
    keeper: Option<Arc<dyn Keeper>>,
}

/// Where the map writes down what it judged and reads it back after a
/// start: the server's database.
pub trait Keeper: Send + Sync {
    /// Writes down one model's part.
    fn save(&self, part: ThermalPart) -> BoxFuture<'_, Result<(), StoreError>>;

    /// What was written down for a source.
    fn load(&self, source: SourceId) -> BoxFuture<'_, Result<Vec<ThermalPart>, StoreError>>;
}

/// The map's parts in a store.
pub struct InStore<S>(pub Arc<S>);

impl<S: Store + 'static> Keeper for InStore<S> {
    fn save(&self, part: ThermalPart) -> BoxFuture<'_, Result<(), StoreError>> {
        async move { self.0.save_thermal_part(&part).await }.boxed()
    }

    fn load(&self, source: SourceId) -> BoxFuture<'_, Result<Vec<ThermalPart>, StoreError>> {
        async move { self.0.thermal_parts(&source).await }.boxed()
    }
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
            models: MAP_MODELS
                .iter()
                .map(|model| Of {
                    model: ModelId::new(*model).expect("the models' names are identifiers"),
                    cells: vec![Kept::default(); points.len()],
                    making: None,
                    made_from: None,
                })
                .collect(),
            map: None,
        };
        Some(Self {
            forecasts,
            source,
            grid,
            points,
            per_request,
            clock,
            state: Mutex::new(state),
            fetching: tokio::sync::Mutex::new(()),
            keeper: None,
        })
    }

    /// The same map, writing down what it judged in `keeper` each time it
    /// makes a model's part whole, and reading it back with
    /// [`restore`](Self::restore).
    #[must_use]
    pub fn with_keeper(mut self, keeper: Arc<dyn Keeper>) -> Self {
        self.keeper = Some(keeper);
        self
    }

    /// The source the map is made from.
    pub fn source(&self) -> &SourceId {
        &self.source
    }

    /// The models the map is made of, the finest first.
    pub fn models(&self) -> Vec<ModelId> {
        self.lock()
            .models
            .iter()
            .map(|of| of.model.clone())
            .collect()
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

    /// How many requests a whole map of one model takes.
    pub fn requests_per_map(&self) -> usize {
        self.points.len().div_ceil(self.per_request)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the server's look at the source tells the map: the run of
    /// each of the map's models that the source holds. A run the map was
    /// not made from starts the making of that model's part anew; one
    /// being made goes on where it stopped. The models are fetched one
    /// after the other, the finest first. Returns when every part is
    /// whole, or when the source's hour has no more room for the map or a
    /// request failed: the next look goes on from there. Does nothing
    /// while another call is fetching.
    pub async fn update(&self, seen: &[(ModelId, Timestamp)]) {
        let Ok(_fetching) = self.fetching.try_lock() else {
            return;
        };
        for index in 0..MAP_MODELS.len() {
            let model = self.lock().models[index].model.clone();
            let Some(run) = seen
                .iter()
                .filter(|(of, _)| *of == model)
                .map(|(_, run)| *run)
                .max()
            else {
                continue;
            };
            if !self.make(index, &model, run).await {
                return;
            }
        }
    }

    /// Makes the part of one model, from the run the look saw. Whether it
    /// is whole: `false` when the map has to wait.
    async fn make(&self, index: usize, model: &ModelId, seen: Timestamp) -> bool {
        {
            let mut state = self.lock();
            let of = &mut state.models[index];
            let made = of.made_from.is_some_and(|run| run >= seen);
            match &of.making {
                Some(making) if making.run >= seen => {}
                _ if made => return true,
                _ => {
                    of.making = Some(Making {
                        run: seen,
                        left: (0..self.requests_per_map()).collect(),
                    });
                }
            }
        }

        loop {
            let next = self.lock().models[index]
                .making
                .as_ref()
                .and_then(|making| making.left.first().copied());
            let Some(request) = next else { break };
            let from = request * self.per_request;
            let to = (from + self.per_request).min(self.points.len());
            let points = &self.points[from..to];
            let output = match self.forecasts.places(&self.source, model, points).await {
                Ok(output) => output,
                Err(Refusal::AskedEnough { retry_after_s }) => {
                    tracing::info!(
                        source = %self.source,
                        "the thermal map of {model} waits: it has used its share of the hour's \
                         requests, room again in {retry_after_s} s; {} of {} requests to go",
                        self.left(index),
                        self.requests_per_map(),
                    );
                    return false;
                }
                Err(refusal) => {
                    tracing::warn!(
                        source = %self.source,
                        "the thermal map of {model} waits: {refusal:?}"
                    );
                    return false;
                }
            };
            if output.series.is_empty() {
                let failed = output.report.failures.first();
                tracing::warn!(
                    source = %self.source,
                    "the thermal map of {model} waits: no forecast for places {from}..{to}{}",
                    failed.map_or_else(String::new, |failure| format!(": {}", failure.error)),
                );
                return false;
            }
            if output.series.len() < points.len() {
                tracing::warn!(
                    source = %self.source,
                    "the thermal map of {model} lacks {} of the places {from}..{to}; \
                     they keep what they had",
                    points.len() - output.series.len(),
                );
            }
            let mut state = self.lock();
            let of = &mut state.models[index];
            for series in &output.series {
                if let Some(at) = points.iter().position(|point| *point == series.point) {
                    of.cells[from + at].take(series);
                }
            }
            if let Some(making) = of.making.as_mut() {
                making.left.retain(|done| *done != request);
            }
        }

        let now = self.clock.now();
        let written = {
            let mut state = self.lock();
            let of = &mut state.models[index];
            let Some(making) = of.making.take() else {
                return true;
            };
            of.made_from = Some(making.run);
            for of in &mut state.models {
                for (kept, point) in of.cells.iter_mut().zip(&self.points) {
                    kept.drop_before_yesterday(now, *point);
                }
            }
            if let Some(map) = self.made(&state.models, now) {
                tracing::info!(
                    source = %self.source,
                    "thermal map made with the run of {model} of {}: {} cells, {} days, {} requests",
                    making.run.format("%Y-%m-%d %H:%M UTC"),
                    self.points.len(),
                    map.days.len(),
                    self.requests_per_map(),
                );
                state.map = Some(Arc::new(map));
            }
            self.keeper.as_ref().map(|_| {
                let of = &state.models[index];
                ThermalPart {
                    source: self.source.clone(),
                    model: of.model.clone(),
                    made_from: making.run,
                    written_at: now,
                    cells: stored::encode(&self.grid, &of.cells),
                }
            })
        };
        if let (Some(keeper), Some(part)) = (&self.keeper, written) {
            let bytes = part.cells.len();
            match keeper.save(part).await {
                Ok(()) => tracing::info!(
                    source = %self.source,
                    "thermal map: the judged hours of {model} written down, {bytes} bytes"
                ),
                // The map goes on in memory; after a start it lacks this.
                Err(error) => tracing::warn!(
                    source = %self.source,
                    "thermal map: the judged hours of {model} could not be written down: {error}"
                ),
            }
        }
        true
    }

    /// Reads back what the map wrote down before the server stopped, and
    /// makes the map from it: so that after a start the map is there at
    /// once, with the hours of today that the models' newest runs no
    /// longer hold. A part of another grid, or written in another form, is
    /// left out, and is made again from the source at the next look.
    pub async fn restore(&self) {
        let Some(keeper) = &self.keeper else { return };
        let parts = match keeper.load(self.source.clone()).await {
            Ok(parts) => parts,
            Err(error) => {
                tracing::warn!(
                    source = %self.source,
                    "thermal map: what was written down could not be read: {error}"
                );
                return;
            }
        };
        let now = self.clock.now();
        let mut state = self.lock();
        for part in parts {
            let Some(of) = state.models.iter_mut().find(|of| of.model == part.model) else {
                continue;
            };
            match stored::decode(&part.cells, &self.grid) {
                Some(cells) => {
                    of.cells = cells;
                    of.made_from = Some(part.made_from);
                    tracing::info!(
                        source = %self.source,
                        "thermal map: the judged hours of {} read back, of the run of {}",
                        part.model,
                        part.made_from.format("%Y-%m-%d %H:%M UTC"),
                    );
                }
                None => tracing::warn!(
                    source = %self.source,
                    "thermal map: the judged hours of {} written down are of another grid or \
                     form; they are made again",
                    part.model,
                ),
            }
        }
        for of in &mut state.models {
            for (kept, point) in of.cells.iter_mut().zip(&self.points) {
                kept.drop_before_yesterday(now, *point);
            }
        }
        if let Some(map) = self.made(&state.models, now) {
            state.map = Some(Arc::new(map));
        }
    }

    fn left(&self, index: usize) -> usize {
        self.lock().models[index]
            .making
            .as_ref()
            .map_or(0, |making| making.left.len())
    }

    /// The map the kept hours make: none before the finest model's part
    /// was made once.
    fn made(&self, models: &[Of], now: Timestamp) -> Option<ThermalMapResponse> {
        let finest = models.first().filter(|of| of.made_from.is_some())?;
        // Of each model, each cell's days and how many hours each has.
        let read: Vec<Read<'_>> = models
            .iter()
            .filter(|of| of.made_from.is_some())
            .map(|of| Read::of(of, &self.points))
            .collect();
        let dates: BTreeSet<NaiveDate> = read
            .iter()
            .flat_map(|read| read.days.iter().flatten().map(|day| day.date))
            .collect();
        let days = dates
            .into_iter()
            .filter_map(|date| {
                // A day is one model's, never a blend: the finest model's,
                // unless a coarser one has more of the day's hours.
                let mut chosen: Option<(&Read<'_>, usize)> = None;
                for read in &read {
                    let hours = read.hours_of(date);
                    if hours > 0 && chosen.is_none_or(|(_, most)| hours > most) {
                        chosen = Some((read, hours));
                    }
                }
                let (read, _) = chosen?;
                Some(read.day(date))
            })
            .collect();
        let run = finest
            .cells
            .iter()
            .flat_map(|kept| kept.hours.values().map(|(run, _)| *run))
            .max()
            .or(finest.made_from)?;
        Some(ThermalMapResponse {
            source: self.source.clone(),
            model: finest.model.clone(),
            run,
            computed_at: now,
            grid: self.grid,
            ground_m: finest
                .cells
                .iter()
                .map(|kept| kept.ground_m.map(metres))
                .collect(),
            days,
            attributions: Vec::new(),
        })
    }
}

/// What one model's kept hours make of each cell.
struct Read<'a> {
    of: &'a Of,
    points: &'a [GeoPoint],
    /// By cell, the days.
    days: Vec<Vec<day::CellDay>>,
}

impl<'a> Read<'a> {
    fn of(of: &'a Of, points: &'a [GeoPoint]) -> Self {
        let days = of
            .cells
            .iter()
            .zip(points)
            .map(|(kept, point)| {
                let hours: Vec<RatedHour> = kept.hours.values().map(|(_, hour)| *hour).collect();
                day::cell_days(&hours, *point)
            })
            .collect();
        Self { of, points, days }
    }

    /// How many judged hours of a day the model has, over all cells.
    fn hours_of(&self, date: NaiveDate) -> usize {
        self.of
            .cells
            .iter()
            .zip(self.points)
            .map(|(kept, point)| {
                kept.hours
                    .keys()
                    .filter(|time| day::solar_date(**time, *point) == date)
                    .count()
            })
            .sum()
    }

    /// The day as the map has it, from this model.
    fn day(&self, date: NaiveDate) -> ThermalMapDay {
        let of_date: Vec<Option<&day::CellDay>> = self
            .days
            .iter()
            .map(|cell| cell.iter().find(|day| day.date == date))
            .collect();
        let thermals = |cell: &Option<&day::CellDay>| cell.and_then(|day| day.thermals);
        let run = self
            .of
            .cells
            .iter()
            .zip(self.points)
            .flat_map(|(kept, point)| {
                kept.hours
                    .iter()
                    .filter(|(time, _)| day::solar_date(**time, *point) == date)
                    .map(|(_, (run, _))| *run)
            })
            .max();
        ThermalMapDay {
            date,
            model: Some(self.of.model.clone()),
            run,
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

/// How the judged hours of a model's cells are written down: JSON, packed
/// with gzip, in a form of the map's own. An hour is kept as it was
/// judged, so the hours read back after a start are judged by the rules
/// of the version that judged them; they are at most a day and a half old
/// by then, and the next runs replace the most of them.
mod stored {
    use std::io::Read as _;

    use chrono::DateTime;
    use flate2::Compression;
    use flate2::read::GzDecoder;
    use flate2::write::GzEncoder;
    use serde::{Deserialize, Serialize};
    use venti_api_types::v1::ThermalGrid;
    use venti_flight::convection::Convection;
    use venti_flight::day::{Against, Kind, RatedHour, Verdict};

    use super::Kept;

    /// The form's number. Raised when the form changes; a part of another
    /// form is not read.
    const FORM: u32 = 1;

    /// The most a part may unpack to: some 30 MB is a grid ten times the
    /// first region's.
    const MAX_UNPACKED: u64 = 64 * 1024 * 1024;

    #[derive(Serialize, Deserialize)]
    struct Part {
        form: u32,
        grid: ThermalGrid,
        cells: Vec<Cell>,
    }

    #[derive(Serialize, Deserialize)]
    struct Cell {
        #[serde(rename = "g")]
        ground_m: Option<f64>,
        #[serde(rename = "h")]
        hours: Vec<Hour>,
    }

    /// An hour: its time and its run in seconds since 1970, the verdict,
    /// the kind, what speaks against it and the air.
    #[derive(Serialize, Deserialize)]
    struct Hour(i64, i64, u8, Option<u8>, Option<Why>, Option<Air>);

    #[derive(Serialize, Deserialize)]
    enum Why {
        R,
        W(f64),
        G(f64),
        A(f64, f64),
        B,
        N,
        S,
    }

    /// Top, cloud base, depth, updraft, climb and wind of the air.
    #[derive(Serialize, Deserialize)]
    struct Air(f64, Option<f64>, f64, f64, f64, Option<f64>);

    const VERDICTS: [Verdict; 3] = [Verdict::Flyable, Verdict::Marginal, Verdict::NotFlyable];
    const KINDS: [Kind; 4] = [Kind::Weak, Kind::Local, Kind::CrossCountry, Kind::Big];

    fn index_of<T: PartialEq>(all: &[T], one: &T) -> u8 {
        u8::try_from(all.iter().position(|of| of == one).unwrap_or(0)).unwrap_or(0)
    }

    fn hour(run: i64, rated: &RatedHour) -> Hour {
        let why = rated.against.map(|against| match against {
            Against::Rain => Why::R,
            Against::Wind { kmh } => Why::W(kmh),
            Against::Gusts { kmh } => Why::G(kmh),
            Against::WindAloft { kmh, at_m } => Why::A(kmh, at_m),
            Against::CloudBaseBelow => Why::B,
            Against::CloudBaseNear => Why::N,
            Against::Storms => Why::S,
        });
        let air = rated.air.map(|air| {
            Air(
                air.top_m,
                air.cloud_base_m,
                air.depth_m,
                air.updraft_ms,
                air.climb_ms,
                air.wind_ms,
            )
        });
        Hour(
            rated.time.timestamp(),
            run,
            index_of(&VERDICTS, &rated.verdict),
            rated.kind.map(|kind| index_of(&KINDS, &kind)),
            why,
            air,
        )
    }

    fn rated(hour: Hour) -> Option<(i64, RatedHour)> {
        let Hour(time, run, verdict, kind, why, air) = hour;
        let against = why.map(|why| match why {
            Why::R => Against::Rain,
            Why::W(kmh) => Against::Wind { kmh },
            Why::G(kmh) => Against::Gusts { kmh },
            Why::A(kmh, at_m) => Against::WindAloft { kmh, at_m },
            Why::B => Against::CloudBaseBelow,
            Why::N => Against::CloudBaseNear,
            Why::S => Against::Storms,
        });
        let kind = match kind {
            Some(kind) => Some(*KINDS.get(usize::from(kind))?),
            None => None,
        };
        Some((
            run,
            RatedHour {
                time: DateTime::from_timestamp(time, 0)?,
                verdict: *VERDICTS.get(usize::from(verdict))?,
                against,
                air: air.map(
                    |Air(top_m, cloud_base_m, depth_m, updraft_ms, climb_ms, wind_ms)| Convection {
                        top_m,
                        cloud_base_m,
                        depth_m,
                        updraft_ms,
                        climb_ms,
                        wind_ms,
                    },
                ),
                kind,
            },
        ))
    }

    /// A model's cells, written down.
    pub(super) fn encode(grid: &ThermalGrid, cells: &[Kept]) -> Vec<u8> {
        let part = Part {
            form: FORM,
            grid: *grid,
            cells: cells
                .iter()
                .map(|kept| Cell {
                    ground_m: kept.ground_m,
                    hours: kept
                        .hours
                        .values()
                        .map(|(run, rated)| hour(run.timestamp(), rated))
                        .collect(),
                })
                .collect(),
        };
        let mut packed = GzEncoder::new(Vec::new(), Compression::default());
        // Writing to memory does not fail, nor does writing plain data as
        // JSON.
        let written = serde_json::to_writer(&mut packed, &part).is_ok();
        let packed = packed.finish().unwrap_or_default();
        if written { packed } else { Vec::new() }
    }

    /// The cells written down, if they are of this grid and of this form.
    pub(super) fn decode(bytes: &[u8], grid: &ThermalGrid) -> Option<Vec<Kept>> {
        let mut text = Vec::new();
        GzDecoder::new(bytes)
            .take(MAX_UNPACKED)
            .read_to_end(&mut text)
            .ok()?;
        let part: Part = serde_json::from_slice(&text).ok()?;
        if part.form != FORM || part.grid != *grid || part.cells.len() != grid.cells() {
            return None;
        }
        part.cells
            .into_iter()
            .map(|cell| {
                let mut kept = Kept {
                    ground_m: cell.ground_m,
                    ..Kept::default()
                };
                for hour in cell.hours {
                    let (run, rated) = rated(hour)?;
                    kept.hours
                        .insert(rated.time, (DateTime::from_timestamp(run, 0)?, rated));
                }
                Some(kept)
            })
            .collect()
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

    fn model(name: &str) -> ModelId {
        ModelId::new(name).unwrap()
    }

    /// What a look tells the map that sees this run of ICON-D2.
    fn d2(run: Timestamp) -> Vec<(ModelId, Timestamp)> {
        vec![(model("icon-d2"), run)]
    }

    /// A forecast service that answers every place with a series of the
    /// run it is told, two places a request, and refuses when told to.
    struct Made {
        run: Mutex<Timestamp>,
        /// The run of ICON-EU, which reaches 72 hours here.
        eu_run: Mutex<Option<Timestamp>>,
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
                eu_run: Mutex::new(None),
                asked: AtomicUsize::new(0),
                refused: AtomicUsize::new(0),
                room: Mutex::new(None),
            })
        }

        fn series(&self, of: &ModelId, point: GeoPoint) -> ForecastSeries {
            let (run, reach) = if of.as_str() == "icon-eu" {
                (self.eu_run.lock().unwrap().unwrap_or_default(), 72)
            } else {
                (*self.run.lock().unwrap(), 48)
            };
            let hours = (1..=reach)
                .map(|hour| {
                    let mut hour = ForecastHour::empty(run + Duration::hours(hour));
                    hour.wind_speed_ms = Some(2.0);
                    hour.temperature_c = Some(20.0);
                    hour
                })
                .collect();
            ForecastSeries {
                source: source(),
                model: of.clone(),
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
            of: &'a ModelId,
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
                    points.iter().map(|point| self.series(of, *point)).collect();
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

        map.update(&d2(at(12, 0))).await;
        let first = map.current().unwrap();
        assert_eq!(first.run, at(12, 0));
        assert_eq!(first.computed_at, at(12, 4));
        assert_eq!(made.asked.load(Ordering::SeqCst), 3);
        assert_eq!(first.ground_m, [Some(800); 6]);
        assert_eq!(first.days.len(), 2);
        assert!(first.days.iter().all(|day| day.verdict.len() == 6));

        // The same run again asks for nothing.
        map.update(&d2(at(12, 0))).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 3);

        // The run of 09 UTC: the hours from 10 UTC on are its, those of the
        // morning before stay the first run's.
        *made.run.lock().unwrap() = at(12, 9);
        map.update(&d2(at(12, 9))).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 6);
        assert_eq!(map.current().unwrap().run, at(12, 9));
        let state = map.lock();
        let runs = &state.models[0].cells[0].hours;
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

        map.update(&d2(at(12, 0))).await;
        // Two requests of three, then the hour had no more room: no map
        // yet, and nothing asked after the refusal.
        assert!(map.current().is_none());
        assert_eq!(made.asked.load(Ordering::SeqCst), 2);
        assert_eq!(made.refused.load(Ordering::SeqCst), 1);
        assert_eq!(map.left(0), 1);

        // The next look finds room, and only the last request is made.
        *made.room.lock().unwrap() = None;
        map.update(&d2(at(12, 0))).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 3);
        assert_eq!(map.current().unwrap().run, at(12, 0));

        // A newer run seen while a map is being made starts it again, and
        // the map that is there is served meanwhile.
        *made.run.lock().unwrap() = at(12, 3);
        *made.room.lock().unwrap() = Some(1);
        map.update(&d2(at(12, 3))).await;
        assert_eq!(map.left(0), 2);
        *made.run.lock().unwrap() = at(12, 6);
        *made.room.lock().unwrap() = None;
        map.update(&d2(at(12, 6))).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 7);
        assert_eq!(map.current().unwrap().run, at(12, 6));
    }

    #[tokio::test]
    async fn days_before_yesterday_are_dropped() {
        let made = Made::at(at(12, 0));
        let early = map_of(&made, at(12, 4));
        early.update(&d2(at(12, 0))).await;
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
        map.update(&d2(at(12, 0))).await;
        *made.run.lock().unwrap() = at(15, 0);
        map.update(&d2(at(15, 0))).await;
        assert_eq!(dates(&map), ["2026-07-15", "2026-07-16"]);

        // On the 14th, the 13th is yesterday and stays.
        let map = map_of(&made, at(14, 4));
        *made.run.lock().unwrap() = at(12, 0);
        map.update(&d2(at(12, 0))).await;
        *made.run.lock().unwrap() = at(14, 0);
        map.update(&d2(at(14, 0))).await;
        assert_eq!(dates(&map), ["2026-07-13", "2026-07-14", "2026-07-15"]);
    }

    #[tokio::test]
    async fn the_days_after_the_finest_model_are_the_coarser_ones() {
        let made = Made::at(at(12, 0));
        *made.eu_run.lock().unwrap() = Some(at(11, 21));
        let map = map_of(&made, at(12, 4));
        let both = vec![
            (model("icon-d2"), at(12, 0)),
            (model("icon-eu"), at(11, 21)),
        ];

        map.update(&both).await;

        // Three requests of each model.
        assert_eq!(made.asked.load(Ordering::SeqCst), 6);
        let current = map.current().unwrap();
        // The answer's model and run are the finest's.
        assert_eq!(
            (current.model.as_str(), current.run),
            ("icon-d2", at(12, 0))
        );
        let days: Vec<(String, &str, Option<Timestamp>, bool)> = current
            .days
            .iter()
            .map(|day| {
                let model = day.model.as_ref().map_or("", ModelId::as_str);
                (day.date.to_string(), model, day.run, day.whole)
            })
            .collect();
        // ICON-D2 has the 12th and the 13th whole, and nothing of the 14th
        // by daylight; ICON-EU reaches into the 14th, and has it whole. Of
        // the 11th ICON-EU has the evening, which has no daylight hour.
        assert_eq!(
            days,
            [
                ("2026-07-12".to_owned(), "icon-d2", Some(at(12, 0)), true),
                ("2026-07-13".to_owned(), "icon-d2", Some(at(12, 0)), true),
                ("2026-07-14".to_owned(), "icon-eu", Some(at(11, 21)), true),
            ]
        );

        // A newer run of ICON-D2 that reaches into the 14th, but less of
        // it than ICON-EU has: the 14th stays ICON-EU's.
        *made.run.lock().unwrap() = at(12, 15);
        map.update(&[
            (model("icon-d2"), at(12, 15)),
            (model("icon-eu"), at(11, 21)),
        ])
        .await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 9);
        let current = map.current().unwrap();
        assert_eq!(current.run, at(12, 15));
        let models: Vec<&str> = current
            .days
            .iter()
            .map(|day| day.model.as_ref().map_or("", ModelId::as_str))
            .collect();
        assert_eq!(models, ["icon-d2", "icon-d2", "icon-eu"]);
        // The 12th holds hours of both runs of ICON-D2 and names the newer.
        assert_eq!(current.days[0].run, Some(at(12, 15)));
    }

    #[tokio::test]
    async fn the_coarser_model_waits_for_room_like_the_finest() {
        let made = Made::at(at(12, 0));
        *made.eu_run.lock().unwrap() = Some(at(11, 21));
        let map = map_of(&made, at(12, 4));
        let both = vec![
            (model("icon-d2"), at(12, 0)),
            (model("icon-eu"), at(11, 21)),
        ];
        *made.room.lock().unwrap() = Some(4);

        // ICON-D2 whole, and one request of ICON-EU: the map is served with
        // ICON-D2's days alone until ICON-EU's part is whole.
        map.update(&both).await;
        let current = map.current().unwrap();
        assert_eq!(current.days.len(), 2);
        assert_eq!(map.left(1), 2);

        *made.room.lock().unwrap() = None;
        map.update(&both).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), 6);
        assert_eq!(map.current().unwrap().days.len(), 3);
    }

    #[tokio::test]
    async fn what_was_judged_is_written_down_and_read_back_exactly() {
        let made = Made::at(at(12, 0));
        *made.eu_run.lock().unwrap() = Some(at(11, 21));
        let map = map_of(&made, at(12, 4));
        map.update(&[
            (model("icon-d2"), at(12, 0)),
            (model("icon-eu"), at(11, 21)),
        ])
        .await;
        let state = map.lock();
        for of in &state.models {
            let written = stored::encode(&map.grid, &of.cells);
            let read = stored::decode(&written, &map.grid).unwrap();
            assert_eq!(read.len(), of.cells.len());
            for (read, kept) in read.iter().zip(&of.cells) {
                assert_eq!(read.hours, kept.hours);
                assert_eq!(read.ground_m, kept.ground_m);
            }
            // Of another grid, or not a part at all, nothing is read.
            let other = ThermalGrid {
                rows: 3,
                ..map.grid
            };
            assert!(stored::decode(&written, &other).is_none());
            assert!(stored::decode(b"not packed", &map.grid).is_none());
        }
    }

    #[tokio::test]
    async fn after_a_restart_the_map_has_the_morning_it_had_before() {
        let store = Arc::new(venti_store::MemoryStore::new());
        let kept_in = |made: &Arc<Made>, now: Timestamp| {
            map_of(made, now).with_keeper(Arc::new(InStore(store.clone())))
        };
        let made = Made::at(at(12, 0));
        let before = kept_in(&made, at(12, 4));
        before.update(&d2(at(12, 0))).await;
        *made.run.lock().unwrap() = at(12, 9);
        before.update(&d2(at(12, 9))).await;
        drop(before);
        let asked = made.asked.load(Ordering::SeqCst);
        let parts = store.thermal_parts(&source()).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].made_from, at(12, 9));

        // A restart: the map is there at once, with today whole, and the
        // look that sees the run it was made from asks nothing.
        let restarted = kept_in(&made, at(12, 11));
        assert!(restarted.current().is_none());
        restarted.restore().await;
        let current = restarted.current().unwrap();
        assert_eq!(current.run, at(12, 9));
        assert!(current.days[0].whole);
        assert_eq!(current.computed_at, at(12, 11));
        restarted.update(&d2(at(12, 9))).await;
        assert_eq!(made.asked.load(Ordering::SeqCst), asked);
        let morning = restarted.lock().models[0].cells[0]
            .hours
            .get(&at(12, 8))
            .map(|(run, _)| *run);
        assert_eq!(morning, Some(at(12, 0)));

        // Without it, the morning before the run of 09 UTC is missing.
        let forgetful = map_of(&made, at(12, 11));
        forgetful.update(&d2(at(12, 9))).await;
        assert!(!forgetful.current().unwrap().days[0].whole);

        // A store that is down costs the written hours, not the map.
        let troubled = map_of(&made, at(12, 11)).with_keeper(Arc::new(InStore(Arc::new(
            venti_store::testing::TroubledStore::down(),
        ))));
        troubled.restore().await;
        assert!(troubled.current().is_none());
        troubled.update(&d2(at(12, 9))).await;
        assert!(troubled.current().is_some());
    }
}
