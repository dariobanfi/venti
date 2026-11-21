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
//!   days ICON-D2 does not have whole ([`MAP_MODELS`]): the day after
//!   tomorrow, and from a main run of ICON-EU the day after that. A day
//!   is one model's, never a blend: the finest model's, unless the coarser
//!   one has more of the day's hours, as a take-off's page decides it.
//! - **Made with each run.** When the server's regular look at the source
//!   sees a run of a model the map was not made from, the grid is
//!   fetched in requests of many places, one request at a time
//!   ([`ThermalMap::update`]). The map may use half of the source's hourly
//!   requests ([`Forecasts::places`]); when that is used up it stops and
//!   goes on at the next look, ten minutes later, and the map that was
//!   there is served until the new one is whole.
//! - **Kept per cell.** The judged daylight hours of each cell, each with
//!   what the model has in that hour of wind, cloud and rain ([`Weather`]):
//!   the wind 10 m above its ground with the gusts, and the wind at the
//!   heights above the sea of [`MAP_HEIGHTS_M`], read between the model's
//!   levels (`venti_flight::aloft`). An hour of a newer run replaces the
//!   same hour of an older one, verdict and weather together. A run says
//!   nothing of the hours before it began, so the morning of today stays
//!   the earlier run's. Hours of days before yesterday are dropped.
//! - **Served by the day and by the hour.** The days as one answer
//!   (`GET /v1/thermals`), and of each day its hours a layer at a time
//!   (`GET /v1/thermals/hours`, [`ThermalMap::hours_of`], [`DayHours`]):
//!   a day of a region with everything in it is half a megabyte packed,
//!   a layer of it some tens of kilobytes. The days and their hours are
//!   made together when a model's part is whole, so that they say the
//!   same.
//! - **Read along a route.** For the points of a route and a day, every
//!   layer of the hours of the cell each point lies in
//!   (`GET /v1/route/weather`, [`DayHours::along`]): read from the hours
//!   served, at no request to the source.
//! - **Read for a place.** A forecast for a place goes out with the
//!   strongest wind and gust among the cells within
//!   `venti_flight::nearby::NEARBY_KM` of it, hour by hour
//!   ([`ThermalMap::around`]): read from the hours served, at no request
//!   to the source.
//! - **Written down.** Each time a model's part is made whole its kept
//!   hours go to the store ([`Keeper`]), and a start reads them back
//!   ([`ThermalMap::restore`]): today keeps the hours before the newest run
//!   began across a restart. Without a store, as in the development mode,
//!   the map is in memory only.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{Duration, NaiveDate};
use venti_api_types::v1::{
    ForecastNearby, NearbyHour, NearbyStrongest, RouteWeatherHour, RouteWeatherPoint,
    RouteWeatherResponse, RouteWindAloft, THERMAL_GRID_DEG, ThermalGrid, ThermalHoursResponse,
    ThermalKind, ThermalLayer, ThermalMapDay, ThermalMapHour, ThermalMapResponse, ThermalReason,
    ThermalVerdict,
};
use venti_domain::{
    BoundingBox, ForecastHour, ForecastSeries, GeoPoint, ModelId, SourceId, Timestamp,
};
use venti_flight::aloft::wind_at_height;
use venti_flight::day::{self, Kind, RatedHour, Reason, Verdict};
use venti_flight::nearby;

use futures::FutureExt;
use futures::future::BoxFuture;
use venti_store::{Store, StoreError, ThermalPart};

use crate::clock::Clock;
use crate::forecast::{Forecasts, Refusal};

/// The models the map is made of, the finest first. ICON-D2: some 2 km a
/// cell, a new run every three hours that reaches 48 hours. ICON-EU: some
/// 7 km a cell, a run every three hours that reaches 78 hours from 00, 06,
/// 12 and 18 UTC and 30 from the runs between. The adapter asks ICON-D2
/// for three days and ICON-EU for four, today being the first, which is
/// as far as their runs reach hour by hour. So ICON-EU adds the day after
/// tomorrow, and from a main run the day after that, as far as the run
/// reaches: the runs of 00, 06 and 12 UTC end on it at 06, 12 and 18 UTC,
/// and the run of 18 UTC with it, at midnight.
pub const MAP_MODELS: [&str; 2] = ["icon-d2", "icon-eu"];

/// The heights above mean sea level the map keeps the wind at, metres:
/// where paragliders fly in the Alps, 500 m apart up to 3000 m and one
/// above. The models' levels lie around them: three within 180 m of the
/// ground, then the surfaces of 950, 850, 700, 600 and 500 hPa, at some
/// 500, 1500, 3000, 4200 and 5600 m. The set is ours, and the owner's to
/// change; a part written down with another set is made again.
pub const MAP_HEIGHTS_M: [u16; 6] = [1000, 1500, 2000, 2500, 3000, 4000];

/// What the map keeps of an hour's wind, cloud and rain, in the whole
/// numbers it is served in: a map shows no more, and a region's hours in
/// floats would be several times the size, in memory, in the store and on
/// the way to a phone. Each value is `None` where the model gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Weather {
    /// The mean wind 10 m above the model's ground, km/h.
    wind_kmh: Option<u16>,
    /// The direction it comes from, in tens of degrees: 0 to 35.
    wind_from: Option<u8>,
    /// The strongest gust there in the hour, km/h.
    gust_kmh: Option<u16>,
    /// The wind at each height of [`MAP_HEIGHTS_M`], km/h. `None` also
    /// where the height lies under the model's ground or above its
    /// highest level.
    aloft_kmh: [Option<u16>; MAP_HEIGHTS_M.len()],
    /// The direction of each, in tens of degrees.
    aloft_from: [Option<u8>; MAP_HEIGHTS_M.len()],
    /// The share of the sky covered by cloud at any height, percent.
    cloud_pct: Option<u8>,
    /// The rain, showers and snow of the hour, tenths of a millimetre.
    rain_tenths: Option<u16>,
}

impl Weather {
    /// What an hour of a model has, over ground at this height.
    fn of(hour: &ForecastHour, ground_m: Option<f64>) -> Self {
        let mut aloft_kmh = [None; MAP_HEIGHTS_M.len()];
        let mut aloft_from = [None; MAP_HEIGHTS_M.len()];
        for (at, height) in MAP_HEIGHTS_M.iter().enumerate() {
            if let Some(wind) = wind_at_height(hour, ground_m, f64::from(*height)).wind() {
                aloft_kmh[at] = kmh(wind.speed_ms);
                aloft_from[at] = wind.from_deg.and_then(tens_of_degrees);
            }
        }
        Self {
            wind_kmh: hour.wind_speed_ms.and_then(kmh),
            wind_from: hour.wind_direction_deg.and_then(tens_of_degrees),
            gust_kmh: hour.wind_gust_ms.and_then(kmh),
            aloft_kmh,
            aloft_from,
            cloud_pct: hour.cloud_cover_pct.and_then(percent),
            rain_tenths: hour
                .precipitation_mm
                .and_then(|mm| whole(mm * 10.0, f64::from(u16::MAX)))
                .and_then(|tenths| u16::try_from(tenths).ok()),
        }
    }
}

/// The heights the map keeps the wind at, as an answer lists them.
fn heights() -> Vec<i32> {
    MAP_HEIGHTS_M
        .iter()
        .map(|height| i32::from(*height))
        .collect()
}

/// A value to the nearest whole number, if it is one from zero to `most`.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(value: f64, most: f64) -> Option<u32> {
    let rounded = value.round();
    (rounded >= 0.0 && rounded <= most).then_some(rounded as u32)
}

/// A speed in whole km/h.
fn kmh(speed_ms: f64) -> Option<u16> {
    whole(speed_ms * 3.6, f64::from(u16::MAX)).and_then(|kmh| u16::try_from(kmh).ok())
}

/// A direction to the nearest ten degrees, in tens: 0 to 35, and 356° is
/// north again.
fn tens_of_degrees(degrees: f64) -> Option<u8> {
    whole(degrees.rem_euclid(360.0) / 10.0, 36.0).and_then(|tens| u8::try_from(tens % 36).ok())
}

/// A share in whole percent.
fn percent(share: f64) -> Option<u8> {
    whole(share, 100.0).and_then(|percent| u8::try_from(percent).ok())
}

/// An hour as the map keeps it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct KeptHour {
    /// The run of the model the hour is from.
    run: Timestamp,
    /// The hour as it was judged.
    rated: RatedHour,
    /// Its wind, cloud and rain.
    weather: Weather,
}

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
    /// from and its weather.
    hours: BTreeMap<Timestamp, KeptHour>,
    /// The height of the model's ground, as the series taken last says
    /// it: the height its hours were judged at. None where that series has
    /// none, as for a cell of water, whose hours are judged without one.
    ground_m: Option<f64>,
}

impl Kept {
    /// Takes in a series of the cell: its daylight hours, judged and with
    /// their weather, each replacing the same hour of an older run.
    fn take(&mut self, series: &ForecastSeries) {
        self.ground_m = series.cell_elevation_m;
        // The judged hours are the series' daylight hours, in its order.
        let mut hours = series.hours.iter();
        for rated in day::cell_hours(series) {
            let newer = self
                .hours
                .get(&rated.time)
                .is_none_or(|kept| kept.run <= series.run);
            if !newer {
                continue;
            }
            let weather = hours
                .find(|hour| hour.time == rated.time)
                .map(|hour| Weather::of(hour, series.cell_elevation_m))
                .unwrap_or_default();
            self.hours.insert(
                rated.time,
                KeptHour {
                    run: series.run,
                    rated,
                    weather,
                },
            );
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
    /// The map as it is served.
    served: Option<Served>,
}

/// The map as it is served, without the credit, which is the API's: the
/// days, and the hours of each day. Made together from the same kept
/// hours, so that a day and its hours say the same.
struct Served {
    map: Arc<ThermalMapResponse>,
    hours: BTreeMap<NaiveDate, Arc<DayHours>>,
}

/// The hours of one day of the map with every layer of them, as the
/// server holds them to answer from: [`layer`](Self::layer) is what goes
/// out.
#[derive(Debug, Clone, PartialEq)]
pub struct DayHours {
    source: SourceId,
    date: NaiveDate,
    model: ModelId,
    run: Timestamp,
    whole: bool,
    computed_at: Timestamp,
    grid: ThermalGrid,
    /// The height of the day's model's ground at each cell.
    ground_m: Vec<Option<i32>>,
    /// The day's daylight hours that any cell has, oldest first.
    hours: Vec<HourOfDay>,
}

/// One hour of a day: every list is as long as the grid has cells.
#[derive(Debug, Clone, PartialEq)]
struct HourOfDay {
    time: Timestamp,
    verdict: Vec<Option<ThermalVerdict>>,
    why: Vec<Option<ThermalReason>>,
    thermals: Vec<Option<ThermalKind>>,
    climb_ms: Vec<Option<f64>>,
    top_m: Vec<Option<i32>>,
    cloud: Vec<bool>,
    wind_kmh: Vec<Option<u16>>,
    wind_from_deg: Vec<Option<u16>>,
    gust_kmh: Vec<Option<u16>>,
    /// The wind at each height of [`MAP_HEIGHTS_M`].
    aloft: Vec<WindAtHeight>,
    cloud_cover_pct: Vec<Option<u8>>,
    precipitation_mm: Vec<Option<f64>>,
}

/// The wind at one height in one hour, cell by cell.
#[derive(Debug, Clone, PartialEq)]
struct WindAtHeight {
    /// Its speed, whole km/h.
    kmh: Vec<Option<u16>>,
    /// The direction it comes from, degrees to the nearest ten.
    from_deg: Vec<Option<u16>>,
}

impl DayHours {
    /// The strongest wind and gust of each hour from `from` on among the
    /// cells `near`, which are listed the nearest first; `points` are the
    /// centres of the grid's cells. An hour in which none of them has a
    /// wind or a gust is left out.
    fn strongest_among(
        &self,
        near: &[usize],
        points: &[GeoPoint],
        from: Timestamp,
    ) -> Vec<NearbyHour> {
        self.hours
            .iter()
            .filter(|hour| hour.time >= from)
            .filter_map(|hour| {
                let strongest = |values: &[Option<u16>]| {
                    let (found, kmh) = nearby::strongest(near.iter().map(|cell| values[*cell]))?;
                    let cell = near[found];
                    Some(NearbyStrongest {
                        kmh,
                        at: *points.get(cell)?,
                        from_deg: hour.wind_from_deg[cell],
                        ground_m: self.ground_m[cell],
                    })
                };
                let (wind, gust) = (strongest(&hour.wind_kmh), strongest(&hour.gust_kmh));
                (wind.is_some() || gust.is_some()).then(|| NearbyHour {
                    time: hour.time,
                    model: self.model.clone(),
                    run: self.run,
                    wind,
                    gust,
                })
            })
            .collect()
    }

    /// One layer of the day as the API answers it, without the credit,
    /// which is the API's. `height_m` chooses the wind at a height above
    /// the sea in place of the wind at the ground, and is of no account
    /// to another layer. `None` for a height the map does not keep.
    pub fn layer(
        &self,
        layer: ThermalLayer,
        height_m: Option<i32>,
    ) -> Option<ThermalHoursResponse> {
        let height_m = height_m.filter(|_| layer == ThermalLayer::Wind);
        let height = match height_m {
            Some(asked) => Some(
                MAP_HEIGHTS_M
                    .iter()
                    .position(|kept| i32::from(*kept) == asked)?,
            ),
            None => None,
        };
        let hours = self
            .hours
            .iter()
            .map(|hour| {
                let time = hour.time;
                match (layer, height) {
                    (ThermalLayer::Thermals, _) => ThermalMapHour {
                        time,
                        verdict: hour.verdict.clone(),
                        why: hour.why.clone(),
                        thermals: hour.thermals.clone(),
                        climb_ms: hour.climb_ms.clone(),
                        top_m: hour.top_m.clone(),
                        cloud: hour.cloud.clone(),
                        ..ThermalMapHour::default()
                    },
                    (ThermalLayer::Wind, None) => ThermalMapHour {
                        time,
                        wind_kmh: hour.wind_kmh.clone(),
                        wind_from_deg: hour.wind_from_deg.clone(),
                        gust_kmh: hour.gust_kmh.clone(),
                        ..ThermalMapHour::default()
                    },
                    (ThermalLayer::Wind, Some(height)) => ThermalMapHour {
                        time,
                        wind_kmh: hour.aloft[height].kmh.clone(),
                        wind_from_deg: hour.aloft[height].from_deg.clone(),
                        ..ThermalMapHour::default()
                    },
                    (ThermalLayer::Cloud, _) => ThermalMapHour {
                        time,
                        cloud_cover_pct: hour.cloud_cover_pct.clone(),
                        ..ThermalMapHour::default()
                    },
                    (ThermalLayer::Rain, _) => ThermalMapHour {
                        time,
                        precipitation_mm: hour.precipitation_mm.clone(),
                        ..ThermalMapHour::default()
                    },
                }
            })
            .collect();
        Some(ThermalHoursResponse {
            source: self.source.clone(),
            date: self.date,
            layer,
            height_m,
            heights_m: heights(),
            model: self.model.clone(),
            run: self.run,
            whole: self.whole,
            computed_at: self.computed_at,
            grid: self.grid,
            ground_m: self.ground_m.clone(),
            hours,
            attributions: Vec::new(),
        })
    }

    /// The day at each point of a route as the API answers it, without
    /// the credit, which is the API's: for the cell each point lies in,
    /// every layer of each of the cell's hours. An hour the cell has no
    /// verdict in is none of its daylight hours, or one the runs at hand do
    /// not hold, and is left out; a point outside the grid has no hours.
    pub fn along(&self, points: &[GeoPoint]) -> RouteWeatherResponse {
        let heights_m = heights();
        let points = points
            .iter()
            .map(|at| {
                let Some(cell) = self.grid.cell_at(*at) else {
                    return RouteWeatherPoint {
                        at: *at,
                        cell: None,
                        ground_m: None,
                        hours: Vec::new(),
                    };
                };
                let hours = self
                    .hours
                    .iter()
                    .filter(|hour| hour.verdict[cell].is_some())
                    .map(|hour| RouteWeatherHour {
                        time: hour.time,
                        verdict: hour.verdict[cell],
                        why: hour.why[cell],
                        thermals: hour.thermals[cell],
                        climb_ms: hour.climb_ms[cell],
                        top_m: hour.top_m[cell],
                        cloud: hour.cloud[cell],
                        wind_kmh: hour.wind_kmh[cell],
                        wind_from_deg: hour.wind_from_deg[cell],
                        gust_kmh: hour.gust_kmh[cell],
                        aloft: hour
                            .aloft
                            .iter()
                            .zip(&heights_m)
                            .filter_map(|(wind, height_m)| {
                                Some(RouteWindAloft {
                                    height_m: *height_m,
                                    kmh: wind.kmh[cell]?,
                                    from_deg: wind.from_deg[cell],
                                })
                            })
                            .collect(),
                        cloud_cover_pct: hour.cloud_cover_pct[cell],
                        precipitation_mm: hour.precipitation_mm[cell],
                    })
                    .collect();
                RouteWeatherPoint {
                    at: *at,
                    cell: self.grid.point(cell),
                    ground_m: self.ground_m[cell],
                    hours,
                }
            })
            .collect();
        RouteWeatherResponse {
            source: self.source.clone(),
            date: self.date,
            model: self.model.clone(),
            run: self.run,
            whole: self.whole,
            computed_at: self.computed_at,
            heights_m,
            points,
            attributions: Vec::new(),
        }
    }
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
    /// The alarms pilots set, told of each map made with a new run.
    alerts: Option<Arc<dyn crate::alerts::Watch>>,
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
            served: None,
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
            alerts: None,
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

    /// The same map, telling `alerts` of each map it makes with a new
    /// run, once the map is served.
    #[must_use]
    pub fn with_alerts(mut self, alerts: Arc<dyn crate::alerts::Watch>) -> Self {
        self.alerts = Some(alerts);
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
        self.lock().served.as_ref().map(|served| served.map.clone())
    }

    /// The hours of one day of the map that [`current`](Self::current)
    /// gives, by the day's date. `None` before a map was made and for a
    /// day the map does not have.
    pub fn hours_of(&self, date: NaiveDate) -> Option<Arc<DayHours>> {
        self.lock()
            .served
            .as_ref()
            .and_then(|served| served.hours.get(&date).cloned())
    }

    /// The strongest wind and the strongest gust among the cells of the
    /// map near a place, for each kept hour from `from` on: what a
    /// forecast for the place is handed out with
    /// ([`venti_flight::nearby`] says which cells are near). Read from
    /// the hours the map serves, so the forecast service is not asked.
    /// `None` before a map was made, and for a place with no cell near.
    pub fn around(&self, place: GeoPoint, from: Timestamp) -> Option<ForecastNearby> {
        let near = nearby::cells_within(place, &self.points);
        if near.is_empty() {
            return None;
        }
        let days: Vec<Arc<DayHours>> = self
            .lock()
            .served
            .as_ref()
            .map(|served| served.hours.values().cloned().collect())?;
        let hours = days
            .iter()
            .flat_map(|day| day.strongest_among(&near, &self.points, from))
            .collect();
        Some(ForecastNearby {
            source: self.source.clone(),
            radius_km: nearby::NEARBY_KM,
            cells: u32::try_from(near.len()).unwrap_or(u32::MAX),
            hours,
        })
    }

    /// Where the map stands, for the server's health answer: when the map
    /// it serves was worked out, if there is one, and of each model, the
    /// finest first, the run its part was last made from whole.
    pub fn standing(&self) -> (Option<Timestamp>, Vec<(ModelId, Option<Timestamp>)>) {
        let state = self.lock();
        (
            state.served.as_ref().map(|served| served.map.computed_at),
            state
                .models
                .iter()
                .map(|of| (of.model.clone(), of.made_from))
                .collect(),
        )
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
        let mut made = None;
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
            if let Some(served) = self.made(&state.models, now) {
                tracing::info!(
                    source = %self.source,
                    "thermal map made with the run of {model} of {}: {} cells, {} days, {} requests",
                    making.run.format("%Y-%m-%d %H:%M UTC"),
                    self.points.len(),
                    served.map.days.len(),
                    self.requests_per_map(),
                );
                made = Some(served.map.clone());
                state.served = Some(served);
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
                    "thermal map: the kept hours of {model} written down, {bytes} bytes"
                ),
                // The map goes on in memory; after a start it lacks this.
                Err(error) => tracing::warn!(
                    source = %self.source,
                    "thermal map: the kept hours of {model} could not be written down: {error}"
                ),
            }
        }
        if let (Some(alerts), Some(map)) = (&self.alerts, made) {
            alerts.map(map).await;
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
                        "thermal map: the kept hours of {} read back, of the run of {}",
                        part.model,
                        part.made_from.format("%Y-%m-%d %H:%M UTC"),
                    );
                }
                None => tracing::warn!(
                    source = %self.source,
                    "thermal map: the kept hours of {} written down are of another grid or \
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
        if let Some(served) = self.made(&state.models, now) {
            state.served = Some(served);
        }
    }

    fn left(&self, index: usize) -> usize {
        self.lock().models[index]
            .making
            .as_ref()
            .map_or(0, |making| making.left.len())
    }

    /// The map the kept hours make, with the hours of each of its days:
    /// none before the finest model's part was made once.
    fn made(&self, models: &[Of], now: Timestamp) -> Option<Served> {
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
        let mut hours = BTreeMap::new();
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
                let day = read.day(date);
                let of_day = read.hours(&day, &self.source, self.grid, now);
                hours.insert(date, Arc::new(of_day));
                Some(day)
            })
            .collect();
        let run = finest
            .cells
            .iter()
            .flat_map(|kept| kept.hours.values().map(|hour| hour.run))
            .max()
            .or(finest.made_from)?;
        let map = ThermalMapResponse {
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
            heights_m: heights(),
            days,
            attributions: Vec::new(),
        };
        Some(Served {
            map: Arc::new(map),
            hours,
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
                let hours: Vec<RatedHour> = kept.hours.values().map(|hour| hour.rated).collect();
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
                    .map(|(_, hour)| hour.run)
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

    /// The hours of a day as the map has them from this model: `day` is
    /// the day as [`day`](Self::day) made it.
    fn hours(
        &self,
        day: &ThermalMapDay,
        source: &SourceId,
        grid: ThermalGrid,
        now: Timestamp,
    ) -> DayHours {
        let date = day.date;
        let cells = || self.of.cells.iter().zip(self.points);
        // The day's hours of any cell. Daylight at the cells of a region
        // differs by minutes, so an hour at the day's edge is one cell's
        // and not another's.
        let times: BTreeSet<Timestamp> = cells()
            .flat_map(|(kept, point)| {
                kept.hours
                    .keys()
                    .filter(|time| day::solar_date(**time, *point) == date)
                    .copied()
            })
            .collect();
        let hours = times
            .into_iter()
            .map(|time| {
                let at: Vec<Option<&KeptHour>> = cells()
                    .map(|(kept, point)| {
                        kept.hours
                            .get(&time)
                            .filter(|_| day::solar_date(time, *point) == date)
                    })
                    .collect();
                // An hour's thermals, where it has a kind: an hour that is
                // not flyable says nothing of them.
                let air = |hour: &KeptHour| hour.rated.kind.and(hour.rated.air);
                let rising = |hour: &KeptHour| air(hour).filter(|air| air.depth_m > 0.0);
                let weather = |pick: fn(&Weather) -> Option<u16>| -> Vec<Option<u16>> {
                    at.iter()
                        .map(|hour| hour.and_then(|hour| pick(&hour.weather)))
                        .collect()
                };
                HourOfDay {
                    time,
                    verdict: at
                        .iter()
                        .map(|hour| hour.map(|hour| verdict(hour.rated.verdict)))
                        .collect(),
                    why: at
                        .iter()
                        .map(|hour| {
                            hour.and_then(|hour| hour.rated.against)
                                .map(|against| reason(against.reason()))
                        })
                        .collect(),
                    thermals: at
                        .iter()
                        .map(|hour| hour.and_then(|hour| hour.rated.kind).map(kind))
                        .collect(),
                    climb_ms: at
                        .iter()
                        .map(|hour| hour.and_then(air).map(|air| day::tenth(air.climb_ms)))
                        .collect(),
                    top_m: at
                        .iter()
                        .map(|hour| {
                            hour.and_then(rising)
                                .map(|air| metres(day::fifty(air.top_m)))
                        })
                        .collect(),
                    cloud: at
                        .iter()
                        .map(|hour| {
                            hour.and_then(rising)
                                .is_some_and(|air| air.cloud_base_m.is_some())
                        })
                        .collect(),
                    wind_kmh: weather(|weather| weather.wind_kmh),
                    wind_from_deg: weather(|weather| weather.wind_from.map(degrees)),
                    gust_kmh: weather(|weather| weather.gust_kmh),
                    aloft: (0..MAP_HEIGHTS_M.len())
                        .map(|height| WindAtHeight {
                            kmh: at
                                .iter()
                                .map(|hour| hour.and_then(|hour| hour.weather.aloft_kmh[height]))
                                .collect(),
                            from_deg: at
                                .iter()
                                .map(|hour| {
                                    hour.and_then(|hour| hour.weather.aloft_from[height])
                                        .map(degrees)
                                })
                                .collect(),
                        })
                        .collect(),
                    cloud_cover_pct: at
                        .iter()
                        .map(|hour| hour.and_then(|hour| hour.weather.cloud_pct))
                        .collect(),
                    precipitation_mm: at
                        .iter()
                        .map(|hour| {
                            hour.and_then(|hour| hour.weather.rain_tenths)
                                .map(|tenths| f64::from(tenths) / 10.0)
                        })
                        .collect(),
                }
            })
            .collect();
        DayHours {
            source: source.clone(),
            date,
            model: self.of.model.clone(),
            run: day.run.or(self.of.made_from).unwrap_or(now),
            whole: day.whole,
            computed_at: now,
            grid,
            ground_m: self
                .of
                .cells
                .iter()
                .map(|kept| kept.ground_m.map(metres))
                .collect(),
            hours,
        }
    }
}

/// A direction kept in tens of degrees, in degrees.
fn degrees(tens: u8) -> u16 {
    u16::from(tens) * 10
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

/// How the kept hours of a model's cells are written down: JSON, packed
/// with gzip, in a form of the map's own. An hour is kept as it was
/// judged, so the hours read back after a start are judged by the rules
/// of the version that judged them; they are at most a day and a half old
/// by then, and the next runs replace the most of them. Its weather is
/// written as it is kept, in whole numbers.
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

    use super::{Kept, KeptHour, MAP_HEIGHTS_M, Weather};

    /// The form's number. Raised when the form changes; a part of another
    /// form is not read. 2 since an hour has its weather; 1 was the judged
    /// hour alone.
    const FORM: u32 = 2;

    /// The most a part may unpack to. ICON-EU's part of the first region
    /// in midsummer, five days of fifteen daylight hours, is some 35 MB of
    /// JSON (reckoned from the 260 bytes an hour takes unpacked): this is
    /// a grid six times that.
    const MAX_UNPACKED: u64 = 256 * 1024 * 1024;

    #[derive(Serialize, Deserialize)]
    struct Part {
        form: u32,
        grid: ThermalGrid,
        /// The heights the hours' wind higher up is at: a part of other
        /// heights is not read.
        heights: Vec<u16>,
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
    /// the kind, what speaks against it, the air and the weather.
    #[derive(Serialize, Deserialize)]
    struct Hour(i64, i64, u8, Option<u8>, Option<Why>, Option<Air>, Sky);

    /// The weather of an hour, as [`Weather`] has it: the wind at the
    /// ground, its direction and the gusts; the wind at each height and
    /// the direction of each; the cloud and the rain.
    #[derive(Serialize, Deserialize)]
    struct Sky(
        Option<u16>,
        Option<u8>,
        Option<u16>,
        [Option<u16>; MAP_HEIGHTS_M.len()],
        [Option<u8>; MAP_HEIGHTS_M.len()],
        Option<u8>,
        Option<u16>,
    );

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

    fn hour(kept: &KeptHour) -> Hour {
        let (run, rated, weather) = (kept.run.timestamp(), &kept.rated, &kept.weather);
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
            Sky(
                weather.wind_kmh,
                weather.wind_from,
                weather.gust_kmh,
                weather.aloft_kmh,
                weather.aloft_from,
                weather.cloud_pct,
                weather.rain_tenths,
            ),
        )
    }

    fn kept(hour: Hour) -> Option<KeptHour> {
        let Hour(time, run, verdict, kind, why, air, sky) = hour;
        let Sky(wind_kmh, wind_from, gust_kmh, aloft_kmh, aloft_from, cloud_pct, rain_tenths) = sky;
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
        Some(KeptHour {
            run: DateTime::from_timestamp(run, 0)?,
            weather: Weather {
                wind_kmh,
                wind_from,
                gust_kmh,
                aloft_kmh,
                aloft_from,
                cloud_pct,
                rain_tenths,
            },
            rated: RatedHour {
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
        })
    }

    /// A model's cells, written down.
    pub(super) fn encode(grid: &ThermalGrid, cells: &[Kept]) -> Vec<u8> {
        let part = Part {
            form: FORM,
            grid: *grid,
            heights: MAP_HEIGHTS_M.to_vec(),
            cells: cells
                .iter()
                .map(|kept| Cell {
                    ground_m: kept.ground_m,
                    hours: kept.hours.values().map(hour).collect(),
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

    /// The cells written down, if they are of this grid, of this form and
    /// of these heights.
    pub(super) fn decode(bytes: &[u8], grid: &ThermalGrid) -> Option<Vec<Kept>> {
        let mut text = Vec::new();
        GzDecoder::new(bytes)
            .take(MAX_UNPACKED)
            .read_to_end(&mut text)
            .ok()?;
        let part: Part = serde_json::from_slice(&text).ok()?;
        if part.form != FORM
            || part.grid != *grid
            || part.heights != MAP_HEIGHTS_M
            || part.cells.len() != grid.cells()
        {
            return None;
        }
        part.cells
            .into_iter()
            .map(|cell| {
                let mut of_cell = Kept {
                    ground_m: cell.ground_m,
                    ..Kept::default()
                };
                for hour in cell.hours {
                    let hour = kept(hour)?;
                    of_cell.hours.insert(hour.rated.time, hour);
                }
                Some(of_cell)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{Datelike, TimeZone, Timelike, Utc};
    use futures::FutureExt;
    use futures::future::BoxFuture;
    use venti_domain::{ForecastHour, WindAloft};
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
        /// The run of ICON-EU, which reaches as far as the model's runs
        /// do: 78 hours from 00, 06, 12 and 18 UTC, 30 from one between.
        eu_run: Mutex<Option<Timestamp>>,
        /// Requests answered.
        asked: AtomicUsize,
        /// Requests refused for the hour.
        refused: AtomicUsize,
        /// How many more requests it answers before it refuses; `None`
        /// for no end.
        room: Mutex<Option<usize>>,
        /// The places whose cell the model counts as water: their series
        /// say nothing of the height of the ground.
        water: Mutex<Vec<GeoPoint>>,
        /// The places whose ground the model has at 2600 m, where every
        /// other place's is at 800 m.
        high: Mutex<Vec<GeoPoint>>,
        /// The places where the ground warms the air: their hours have
        /// thermals, to 1870 m in the blue.
        thermic: Mutex<Vec<GeoPoint>>,
    }

    impl Made {
        fn at(run: Timestamp) -> Arc<Self> {
            Arc::new(Self {
                run: Mutex::new(run),
                eu_run: Mutex::new(None),
                asked: AtomicUsize::new(0),
                refused: AtomicUsize::new(0),
                room: Mutex::new(None),
                water: Mutex::new(Vec::new()),
                high: Mutex::new(Vec::new()),
                thermic: Mutex::new(Vec::new()),
            })
        }

        fn series(&self, of: &ModelId, point: GeoPoint) -> ForecastSeries {
            let (run, reach) = if of.as_str() == "icon-eu" {
                let run = self.eu_run.lock().unwrap().unwrap_or_default();
                (run, if run.hour().is_multiple_of(6) { 78 } else { 30 })
            } else {
                (*self.run.lock().unwrap(), 48)
            };
            let thermic = self.thermic.lock().unwrap().contains(&point);
            // A level with its wind and its temperature.
            let level = |mut level: WindAloft, speed: f64, from: f64, warmth: f64| {
                level.wind_speed_ms = Some(speed);
                level.wind_direction_deg = Some(from);
                level.temperature_c = thermic.then_some(warmth);
                level
            };
            let surface = |hpa: f64, altitude: f64| {
                let mut level = WindAloft::at_pressure(hpa);
                level.altitude_m = Some(altitude);
                level
            };
            let hours = (1..=reach)
                .map(|hour| {
                    let mut hour = ForecastHour::empty(run + Duration::hours(hour));
                    // 7 km/h, gusts 14, and a wind that says which run it
                    // is of: the run of 09 UTC blows from 90°.
                    hour.wind_speed_ms = Some(2.0);
                    hour.wind_direction_deg = Some(f64::from(run.hour()) * 10.0);
                    hour.wind_gust_ms = Some(4.0);
                    hour.temperature_c = Some(20.0);
                    hour.cloud_cover_pct = Some(40.4);
                    hour.precipitation_mm = Some(0.04);
                    if thermic {
                        hour.heat_to_air_w_m2 = Some(300.0);
                        hour.dew_point_c = Some(10.0);
                    }
                    // 80 m above the ground, and the surfaces of 850 hPa
                    // at 1500 m and of 700 hPa at 3000 m.
                    hour.winds_aloft = vec![
                        level(WindAloft::above_ground(80.0), 3.0, 280.0, 18.5),
                        level(surface(850.0, 1500.0), 5.0, 300.0, 12.0),
                        level(surface(700.0, 3000.0), 10.0, 330.0, 2.0),
                    ];
                    hour
                })
                .collect();
            let ground = if self.water.lock().unwrap().contains(&point) {
                None
            } else if self.high.lock().unwrap().contains(&point) {
                Some(2600.0)
            } else {
                Some(800.0)
            };
            ForecastSeries {
                source: source(),
                model: of.clone(),
                run,
                available_at: None,
                point,
                cell: point,
                cell_elevation_m: ground,
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
        // The map names the heights its hours have the wind at.
        assert_eq!(first.heights_m, [1000, 1500, 2000, 2500, 3000, 4000]);
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
        assert_eq!(runs.get(&at(12, 8)).map(|kept| kept.run), Some(at(12, 0)));
        assert_eq!(runs.get(&at(12, 9)).map(|kept| kept.run), Some(at(12, 0)));
        assert_eq!(runs.get(&at(12, 10)).map(|kept| kept.run), Some(at(12, 9)));
        assert_eq!(runs.get(&at(13, 10)).map(|kept| kept.run), Some(at(12, 9)));
        // Each a daylight hour.
        assert!(runs.keys().all(|time| (4..=19).contains(&time.hour())));
        // An hour's weather is its run's, like its verdict: the made-up
        // run of 00 UTC blows from the north, the one of 09 UTC from 90°.
        let from = |hour| runs.get(&hour).and_then(|kept| kept.weather.wind_from);
        assert_eq!(from(at(12, 8)), Some(0));
        assert_eq!(from(at(12, 9)), Some(0));
        assert_eq!(from(at(12, 10)), Some(9));
        assert_eq!(from(at(13, 10)), Some(9));
    }

    #[test]
    fn an_hour_keeps_its_wind_cloud_and_rain_in_whole_numbers() {
        let mut hour = ForecastHour::empty(at(12, 12));
        hour.wind_speed_ms = Some(3.4);
        hour.wind_direction_deg = Some(356.0);
        hour.wind_gust_ms = Some(7.0);
        hour.cloud_cover_pct = Some(99.6);
        hour.precipitation_mm = Some(0.26);
        let mut top = WindAloft::at_pressure(700.0);
        top.altitude_m = Some(3000.0);
        top.wind_speed_ms = Some(10.0);
        top.wind_direction_deg = Some(284.9);
        hour.winds_aloft = vec![top];

        // Over ground at 1200 m: 1000 m lies under it, 4000 m above the
        // highest level, and the heights between are read between the
        // wind at the ground, at 1210 m, and the surface at 3000 m.
        let weather = Weather::of(&hour, Some(1200.0));
        assert_eq!(weather.wind_kmh, Some(12));
        // 356° is north to the nearest ten.
        assert_eq!(weather.wind_from, Some(0));
        assert_eq!(weather.gust_kmh, Some(25));
        assert_eq!(weather.cloud_pct, Some(100));
        assert_eq!(weather.rain_tenths, Some(3));
        assert_eq!(
            weather.aloft_kmh,
            [None, Some(16), Some(23), Some(29), Some(36), None]
        );
        // From 356° at the ground the shorter way round to 285°.
        assert_eq!(
            weather.aloft_from,
            [None, Some(34), Some(32), Some(30), Some(28), None]
        );

        // Without a height of the ground the wind higher up cannot be
        // placed; what the model has at the ground stays.
        let afloat = Weather::of(&hour, None);
        assert_eq!(afloat.aloft_kmh, [None; 6]);
        assert_eq!(afloat.aloft_from, [None; 6]);
        assert_eq!((afloat.wind_kmh, afloat.gust_kmh), (Some(12), Some(25)));

        // What the model does not give is not made up, and a value that
        // is no number is none.
        let mut bare = ForecastHour::empty(at(12, 12));
        assert_eq!(Weather::of(&bare, Some(800.0)), Weather::default());
        bare.wind_speed_ms = Some(f64::NAN);
        bare.cloud_cover_pct = Some(-3.0);
        bare.precipitation_mm = Some(f64::INFINITY);
        assert_eq!(Weather::of(&bare, Some(800.0)), Weather::default());
    }

    #[tokio::test]
    async fn the_hours_of_a_day_are_served_with_what_the_model_has_in_each() {
        let date = |day: u32| NaiveDate::from_ymd_opt(2026, 7, day).unwrap();
        let made = Made::at(at(12, 0));
        // Of the six cells the second lies high, the third is a lake and
        // the fourth has thermals.
        *made.high.lock().unwrap() = vec![GeoPoint::new(47.0, 11.1).unwrap()];
        *made.water.lock().unwrap() = vec![GeoPoint::new(47.0, 11.2).unwrap()];
        *made.thermic.lock().unwrap() = vec![GeoPoint::new(47.1, 11.0).unwrap()];
        let map = map_of(&made, at(12, 4));
        assert!(map.hours_of(date(12)).is_none());
        map.update(&d2(at(12, 0))).await;
        *made.run.lock().unwrap() = at(12, 9);
        map.update(&d2(at(12, 9))).await;

        let today = map.hours_of(date(12)).unwrap();
        assert_eq!(
            (today.model.as_str(), today.run, today.whole, today.date),
            ("icon-d2", at(12, 9), true, date(12))
        );
        assert_eq!(today.computed_at, at(12, 4));
        assert_eq!(today.grid, map.grid);
        assert_eq!(
            today.ground_m,
            [Some(800), Some(2600), None, Some(800), Some(800), Some(800)]
        );
        // The day's daylight hours, from an hour after sunrise to an hour
        // before sunset, and every list as long as the grid.
        let times: Vec<u32> = today.hours.iter().map(|hour| hour.time.hour()).collect();
        assert_eq!(times, (5..=18).collect::<Vec<u32>>());
        for hour in &today.hours {
            let lengths = [
                hour.verdict.len(),
                hour.why.len(),
                hour.thermals.len(),
                hour.climb_ms.len(),
                hour.top_m.len(),
                hour.cloud.len(),
                hour.wind_kmh.len(),
                hour.wind_from_deg.len(),
                hour.gust_kmh.len(),
                hour.cloud_cover_pct.len(),
                hour.precipitation_mm.len(),
            ];
            assert_eq!(lengths, [6; 11]);
            assert_eq!(hour.aloft.len(), 6);
            for aloft in &hour.aloft {
                assert_eq!((aloft.kmh.len(), aloft.from_deg.len()), (6, 6));
            }
            // What the model has at the ground, in whole numbers, lake or
            // not.
            assert_eq!(hour.wind_kmh, [Some(7); 6]);
            assert_eq!(hour.gust_kmh, [Some(14); 6]);
            assert_eq!(hour.cloud_cover_pct, [Some(40); 6]);
            assert_eq!(hour.precipitation_mm, [Some(0.0); 6]);
        }
        // The morning is the run of 00 UTC's and the afternoon the run of
        // 09 UTC's, weather as verdict.
        let at_hour = |hour: u32| &today.hours[(hour - 5) as usize];
        assert_eq!(at_hour(8).wind_from_deg, [Some(0); 6]);
        assert_eq!(at_hour(9).wind_from_deg, [Some(0); 6]);
        assert_eq!(at_hour(10).wind_from_deg, [Some(90); 6]);

        // The wind higher up over ground at 800 m: between the level 80 m
        // above the ground and the surface at 1500 m, at it, between it
        // and the surface at 3000 m, at that, and nothing above it.
        let noon = at_hour(12);
        let over = |cell: usize| -> (Vec<Option<u16>>, Vec<Option<u16>>) {
            (
                noon.aloft.iter().map(|aloft| aloft.kmh[cell]).collect(),
                noon.aloft
                    .iter()
                    .map(|aloft| aloft.from_deg[cell])
                    .collect(),
            )
        };
        assert_eq!(
            over(0),
            (
                vec![Some(12), Some(18), Some(24), Some(30), Some(36), None],
                vec![Some(280), Some(300), Some(310), Some(320), Some(330), None],
            )
        );
        // Over ground at 2600 m the heights below it have no wind, and
        // the surface of 850 hPa, which lies under it, lends them none.
        assert_eq!(
            over(1),
            (
                vec![None, None, None, None, Some(36), None],
                vec![None, None, None, None, Some(330), None],
            )
        );
        // Over the lake nothing can be placed.
        assert_eq!(over(2), (vec![None; 6], vec![None; 6]));

        // An hour's verdict, with the worst thing against it: the high
        // cell has 36 km/h within reach above it.
        assert_eq!(noon.verdict[0], Some(ThermalVerdict::Flyable));
        assert_eq!(noon.why[0], None);
        assert_eq!(noon.verdict[1], Some(ThermalVerdict::Marginal));
        assert_eq!(noon.why[1], Some(ThermalReason::WindAloft));
        // No heat from the ground, no estimate of thermals; with it, the
        // hour's own: what they count as, the climb to a tenth and the
        // top to the nearest fifty, in the blue.
        assert_eq!(
            (noon.thermals[0], noon.climb_ms[0], noon.top_m[0]),
            (None, None, None)
        );
        assert_eq!(noon.thermals[3], Some(ThermalKind::CrossCountry));
        assert_eq!(noon.climb_ms[3], Some(1.1));
        assert_eq!(noon.top_m[3], Some(1850));
        assert_eq!(noon.cloud, [false; 6]);

        // Every day of the map has its hours, of the day's model and run,
        // and a day the map does not have has none.
        // The run of 09 UTC reaches into the morning of the 14th.
        let current = map.current().unwrap();
        assert_eq!(current.days.len(), 3);
        for day in &current.days {
            let hours = map.hours_of(day.date).unwrap();
            assert_eq!(Some(&hours.model), day.model.as_ref());
            assert_eq!(Some(hours.run), day.run);
            assert_eq!(hours.whole, day.whole);
            assert_eq!(hours.computed_at, current.computed_at);
            assert!(!hours.hours.is_empty());
        }
        assert!(!map.hours_of(date(14)).unwrap().whole);
        assert!(map.hours_of(date(11)).is_none());
        assert!(map.hours_of(date(15)).is_none());

        // What goes out is a layer of the day: its lists and no others,
        // under the same head.
        let layer = |layer, height| today.layer(layer, height).unwrap();
        let judged = layer(ThermalLayer::Thermals, None);
        assert_eq!(
            (judged.layer, judged.height_m),
            (ThermalLayer::Thermals, None)
        );
        assert_eq!(
            (judged.date, judged.model.as_str(), judged.run, judged.whole),
            (date(12), "icon-d2", at(12, 9), true)
        );
        assert_eq!(judged.heights_m, [1000, 1500, 2000, 2500, 3000, 4000]);
        assert_eq!(judged.ground_m, today.ground_m);
        assert_eq!(judged.hours.len(), 14);
        let hour = &judged.hours[7];
        assert_eq!(hour.time, at(12, 12));
        assert_eq!(hour.verdict, noon.verdict);
        assert_eq!(hour.thermals, noon.thermals);
        assert_eq!(
            (&hour.why, &hour.climb_ms, &hour.top_m, &hour.cloud),
            (&noon.why, &noon.climb_ms, &noon.top_m, &noon.cloud)
        );
        assert!(hour.wind_kmh.is_empty() && hour.cloud_cover_pct.is_empty());
        // The wind at the ground has its gusts; at a height it has none.
        let ground = layer(ThermalLayer::Wind, None);
        assert_eq!(ground.height_m, None);
        assert_eq!(ground.hours[7].wind_kmh, [Some(7); 6]);
        assert_eq!(ground.hours[7].wind_from_deg, [Some(90); 6]);
        assert_eq!(ground.hours[7].gust_kmh, [Some(14); 6]);
        assert!(ground.hours[7].verdict.is_empty());
        let high = layer(ThermalLayer::Wind, Some(2000));
        assert_eq!(high.height_m, Some(2000));
        assert_eq!(
            high.hours[7].wind_kmh,
            [Some(24), None, None, Some(24), Some(24), Some(24)]
        );
        assert_eq!(high.hours[7].wind_from_deg[0], Some(310));
        assert!(high.hours[7].gust_kmh.is_empty());
        // A height the map does not keep is none of its layers.
        assert!(today.layer(ThermalLayer::Wind, Some(1800)).is_none());
        let cloud = layer(ThermalLayer::Cloud, None);
        assert_eq!(cloud.hours[7].cloud_cover_pct, [Some(40); 6]);
        assert!(cloud.hours[7].precipitation_mm.is_empty());
        let rain = layer(ThermalLayer::Rain, None);
        assert_eq!(rain.hours[7].precipitation_mm, [Some(0.0); 6]);
        assert!(rain.hours[7].cloud_cover_pct.is_empty());
        // A height asked of a layer that has none is of no account.
        assert_eq!(today.layer(ThermalLayer::Rain, Some(2000)), Some(rain));
    }

    #[tokio::test]
    async fn a_cell_of_water_has_no_ground_and_keeps_its_verdict() {
        // A first run that gives every cell a ground, as the source did
        // while it read a lake's cell as lying at the level of the sea.
        let made = Made::at(at(12, 0));
        let map = map_of(&made, at(12, 4));
        map.update(&d2(at(12, 0))).await;
        assert_eq!(map.current().unwrap().ground_m, [Some(800); 6]);

        // The next run says of the first cell that it is water. The map
        // does not go on naming the height it had: the cell's new hours
        // are judged without one.
        let lake = GeoPoint::new(47.0, 11.0).unwrap();
        *made.water.lock().unwrap() = vec![lake];
        *made.run.lock().unwrap() = at(12, 9);
        map.update(&d2(at(12, 9))).await;
        let current = map.current().unwrap();
        assert_eq!(current.ground_m[0], None);
        assert_eq!(current.ground_m[1..], [Some(800); 5]);
        // By the wind and the rain it still has a verdict on every day.
        assert!(current.days.iter().all(|day| day.verdict[0].is_some()));

        // Written down and read back, it still has none.
        let state = map.lock();
        let read = stored::decode(
            &stored::encode(&map.grid, &state.models[0].cells),
            &map.grid,
        );
        assert_eq!(read.unwrap()[0].ground_m, None);
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
        // Their hours went with them.
        let date = |day: u32| NaiveDate::from_ymd_opt(2026, 7, day).unwrap();
        assert!(early.hours_of(date(12)).is_some());
        assert!(map.hours_of(date(13)).is_none());
        assert!(map.hours_of(date(15)).is_some());

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
        *made.eu_run.lock().unwrap() = Some(at(11, 18));
        let map = map_of(&made, at(12, 4));
        let both = vec![
            (model("icon-d2"), at(12, 0)),
            (model("icon-eu"), at(11, 18)),
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
        // by daylight; ICON-EU's main run of 18 UTC the evening before
        // reaches 78 hours, to the midnight that ends the 14th, and has
        // that day whole. Of the 11th it has the evening, which has no
        // daylight hour.
        assert_eq!(
            days,
            [
                ("2026-07-12".to_owned(), "icon-d2", Some(at(12, 0)), true),
                ("2026-07-13".to_owned(), "icon-d2", Some(at(12, 0)), true),
                ("2026-07-14".to_owned(), "icon-eu", Some(at(11, 18)), true),
            ]
        );

        // The hours of a day are its model's: the 14th's are ICON-EU's, of
        // its run.
        let date = |day: u32| NaiveDate::from_ymd_opt(2026, 7, day).unwrap();
        let of_the_14th = map.hours_of(date(14)).unwrap();
        assert_eq!(
            (of_the_14th.model.as_str(), of_the_14th.run),
            ("icon-eu", at(11, 18))
        );
        assert_eq!(of_the_14th.hours[0].wind_from_deg[0], Some(180));
        assert_eq!(map.hours_of(date(13)).unwrap().model.as_str(), "icon-d2");

        // A newer run of ICON-D2 that reaches into the 14th, but less of
        // it than ICON-EU has: the 14th stays ICON-EU's.
        *made.run.lock().unwrap() = at(12, 15);
        map.update(&[
            (model("icon-d2"), at(12, 15)),
            (model("icon-eu"), at(11, 18)),
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
    async fn a_main_run_of_the_coarser_model_reaches_a_fourth_day_that_is_not_whole() {
        // Both models' runs of 06 UTC, as a forecast service holds them in
        // the late morning. ICON-D2's reaches 48 hours, to 06 UTC of the
        // 14th. ICON-EU's is a main run and reaches 78 hours, to noon UTC
        // of the 15th: the fourth day the source is asked for.
        let made = Made::at(at(12, 6));
        *made.eu_run.lock().unwrap() = Some(at(12, 6));
        let map = map_of(&made, at(12, 10));
        map.update(&[(model("icon-d2"), at(12, 6)), (model("icon-eu"), at(12, 6))])
            .await;

        let days = |map: &ThermalMap| -> Vec<(String, String, Option<Timestamp>, bool)> {
            let current = map.current().unwrap();
            current
                .days
                .iter()
                .map(|day| {
                    let model = day.model.as_ref().map_or("", ModelId::as_str);
                    (day.date.to_string(), model.to_owned(), day.run, day.whole)
                })
                .collect()
        };
        let day = |date: &str, model: &str, run, whole| {
            (date.to_owned(), model.to_owned(), Some(run), whole)
        };
        // Daylight is from 05 to 18 UTC here. Today lacks the two hours
        // before the runs began, in both models, and stays the finer
        // one's. ICON-D2 has tomorrow whole and two hours of the 14th;
        // ICON-EU has the 14th whole, and of the 15th the hours to noon.
        assert_eq!(
            days(&map),
            [
                day("2026-07-12", "icon-d2", at(12, 6), false),
                day("2026-07-13", "icon-d2", at(12, 6), true),
                day("2026-07-14", "icon-eu", at(12, 6), true),
                day("2026-07-15", "icon-eu", at(12, 6), false),
            ]
        );
        // A day that is not whole is judged by the hours it has: every
        // cell has a verdict, from the eight hours to noon.
        let current = map.current().unwrap();
        assert!(current.days[3].verdict.iter().all(Option::is_some));
        {
            let state = map.lock();
            let of_the_15th: Vec<u32> = state.models[1].cells[0]
                .hours
                .keys()
                .filter(|time| time.day() == 15)
                .map(Timelike::hour)
                .collect();
            assert_eq!(of_the_15th, [5, 6, 7, 8, 9, 10, 11, 12]);
        }

        // Three hours later ICON-EU's run of 09 UTC is there, one between
        // the main runs, which reaches 30 hours, into the afternoon of the
        // 13th. The map keeps the days it had from the main run: they are
        // still its, and the fourth day is as far as it was.
        *made.eu_run.lock().unwrap() = Some(at(12, 9));
        map.update(&[(model("icon-d2"), at(12, 6)), (model("icon-eu"), at(12, 9))])
            .await;
        assert_eq!(
            days(&map),
            [
                day("2026-07-12", "icon-d2", at(12, 6), false),
                day("2026-07-13", "icon-d2", at(12, 6), true),
                day("2026-07-14", "icon-eu", at(12, 6), true),
                day("2026-07-15", "icon-eu", at(12, 6), false),
            ]
        );
    }

    #[tokio::test]
    async fn the_coarser_model_waits_for_room_like_the_finest() {
        let made = Made::at(at(12, 0));
        *made.eu_run.lock().unwrap() = Some(at(11, 18));
        let map = map_of(&made, at(12, 4));
        let both = vec![
            (model("icon-d2"), at(12, 0)),
            (model("icon-eu"), at(11, 18)),
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
        *made.eu_run.lock().unwrap() = Some(at(11, 18));
        let map = map_of(&made, at(12, 4));
        map.update(&[
            (model("icon-d2"), at(12, 0)),
            (model("icon-eu"), at(11, 18)),
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
            // The weather is there to be read back.
            assert!(read.iter().all(|kept| {
                kept.hours
                    .values()
                    .all(|hour| hour.weather.aloft_kmh[1] == Some(18))
            }));
            // Of another grid, or not a part at all, nothing is read.
            let other = ThermalGrid {
                rows: 3,
                ..map.grid
            };
            assert!(stored::decode(&written, &other).is_none());
            assert!(stored::decode(b"not packed", &map.grid).is_none());
        }
    }

    /// A forecast service over the first region's box that makes up a
    /// midsummer's days for every place: for measuring what a region's
    /// hours weigh, which the four recorded places cannot show.
    ///
    /// `smooth` decides how: fields that change slowly from place to place
    /// and from hour to hour, as the wind high above the mountains does,
    /// or every value drawn by itself from its range, as no weather is.
    /// What a model has lies between the two, the wind at the ground
    /// nearer the second.
    struct Region {
        run: Timestamp,
        smooth: bool,
    }

    impl Region {
        fn series(&self, of: &ModelId, point: GeoPoint) -> ForecastSeries {
            use std::f64::consts::PI;

            let (lat, lon) = (point.lat(), point.lon());
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let seed = ((lat * 1000.0) as u64) << 20 | (lon * 1000.0) as u64;
            let mut rng = fastrand::Rng::with_seed(seed);
            // High ground along 47° N, falling to the north, the south and
            // the east, and rough from cell to cell.
            let ridge = (1.0 - (lat - 47.0).abs()).clamp(0.0, 1.0) * (1.0 - (lon - 9.4) / 12.0);
            let ground = 350.0 + 1900.0 * ridge + rng.f64() * 500.0;
            let reach = if of.as_str() == "icon-eu" { 78 } else { 48 };
            let smooth = self.smooth;
            let hours = (1..=reach)
                .map(|ahead| {
                    let time = self.run + Duration::hours(ahead);
                    let clock = f64::from(time.hour());
                    let ahead = f64::from(u32::try_from(ahead).unwrap_or(0));
                    let wave = |a: f64, b: f64, c: f64| (lat * a + lon * b + ahead * c).sin();
                    // A value between `low` and `high`: near `shape`, or
                    // anywhere.
                    let mut draw = |shape: f64, low: f64, high: f64| {
                        if smooth {
                            (shape + (rng.f64() - 0.5) * (high - low) * 0.05).clamp(low, high)
                        } else {
                            low + rng.f64() * (high - low)
                        }
                    };
                    let mut hour = ForecastHour::empty(time);
                    let speed = draw(3.0 + 2.0 * wave(2.0, 1.0, 0.2), 0.0, 9.0);
                    let from = draw(200.0 + 90.0 * wave(1.0, 0.7, 0.15), 0.0, 359.9);
                    let warmth = 28.0 - 0.0065 * ground + 4.0 * (PI * (clock - 4.0) / 14.0).sin();
                    hour.wind_speed_ms = Some(speed);
                    hour.wind_direction_deg = Some(from);
                    hour.wind_gust_ms = Some(speed * draw(1.8, 1.3, 2.4));
                    hour.temperature_c = Some(warmth);
                    hour.dew_point_c =
                        Some(warmth - draw(9.0 + 4.0 * wave(0.8, 0.6, 0.1), 3.0, 16.0));
                    hour.cloud_cover_pct =
                        Some(draw(50.0 + 70.0 * wave(1.5, 0.9, 0.1), 0.0, 100.0));
                    let wet = draw(0.5 + 0.5 * wave(1.1, 1.3, 0.12), 0.0, 1.0);
                    hour.precipitation_mm = Some(((wet - 0.85) * 20.0).max(0.0));
                    hour.cape_j_kg = Some(draw(500.0 + 500.0 * wave(0.9, 1.1, 0.08), 0.0, 1200.0));
                    hour.heat_to_air_w_m2 = Some(
                        380.0
                            * (PI * (clock - 4.0) / 15.0).sin()
                            * draw(0.75 + 0.2 * wave(1.3, 0.4, 0.0), 0.4, 1.0),
                    );
                    let mut levels = Vec::new();
                    for (above, more) in [(80.0, 1.2), (120.0, 1.3), (180.0, 1.45)] {
                        let mut level = WindAloft::above_ground(above);
                        level.wind_speed_ms = Some(speed * more + draw(0.2, 0.0, 0.5));
                        level.wind_direction_deg = Some((from + above / 15.0) % 360.0);
                        level.temperature_c = Some(warmth - 0.008 * above - draw(0.2, 0.0, 0.6));
                        levels.push(level);
                    }
                    for (hpa, about) in [
                        (950.0, 540.0),
                        (850.0, 1480.0),
                        (700.0, 3050.0),
                        (600.0, 4250.0),
                        (500.0, 5650.0),
                    ] {
                        let altitude: f64 = about + 30.0 * wave(0.5, 0.3, 0.05);
                        let mut level = WindAloft::at_pressure(hpa);
                        level.altitude_m = Some(altitude.round());
                        level.wind_speed_ms = Some(draw(
                            4.0 + altitude / 400.0 + 3.0 * wave(0.7, 0.5, 0.1),
                            0.0,
                            8.0 + altitude / 300.0,
                        ));
                        level.wind_direction_deg =
                            Some(draw(260.0 + 50.0 * wave(0.4, 0.3, 0.07), 0.0, 359.9));
                        level.temperature_c = Some(25.0 - 0.0068 * altitude + draw(0.0, -2.0, 2.0));
                        levels.push(level);
                    }
                    hour.winds_aloft = levels;
                    hour
                })
                .collect();
            ForecastSeries {
                source: source(),
                model: of.clone(),
                run: self.run,
                available_at: None,
                point,
                cell: point,
                cell_elevation_m: Some(ground.round()),
                hours,
            }
        }
    }

    impl Forecasts for Region {
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
            BoundingBox::new(46.3, 9.4, 48.5, 17.2).ok()
        }

        fn places_per_request(&self, _: &SourceId) -> usize {
            80
        }

        fn places<'a>(
            &'a self,
            _: &'a SourceId,
            of: &'a ModelId,
            points: &'a [GeoPoint],
        ) -> BoxFuture<'a, Result<ForecastRunOutput, Refusal>> {
            async move {
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
                    hours: series.iter().map(|series| series.hours.len()).sum(),
                    newest_run: Some(self.run),
                };
                Ok(ForecastRunOutput { series, report })
            }
            .boxed()
        }
    }

    /// Packed as the API packs an answer for a client that takes gzip.
    fn gzip(bytes: &[u8]) -> usize {
        use std::io::Write as _;

        let mut packed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        packed.write_all(bytes).unwrap();
        packed.finish().unwrap().len()
    }

    /// What a region's hours weigh: in the store, in memory and on the
    /// way to a phone, whole and layer by layer. A measurement over
    /// made-up weather, printed and not checked: `docs/architecture.md`
    /// has the figures and says how they were got.
    ///
    /// `cargo test -p venti-server --lib a_regions_hours_weighed -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "a measurement that prints what it finds"]
    async fn a_regions_hours_weighed() {
        let kb = |bytes: usize| format!("{:.0} kB", bytes as f64 / 1000.0);
        println!(
            "in memory: an hour kept {} bytes (judged {}, weather {}), beside its key in the map",
            size_of::<KeptHour>(),
            size_of::<RatedHour>(),
            size_of::<Weather>(),
        );
        for smooth in [true, false] {
            println!(
                "\n== made-up weather, {} ==",
                if smooth {
                    "smooth fields"
                } else {
                    "every value by itself"
                }
            );
            let region = Arc::new(Region {
                run: at(12, 0),
                smooth,
            });
            let map =
                ThermalMap::for_source(region, source(), Arc::new(FixedClock(at(12, 4)))).unwrap();
            map.update(&[(model("icon-d2"), at(12, 0)), (model("icon-eu"), at(12, 0))])
                .await;
            assert_eq!(map.cells(), 1817);

            {
                let state = map.lock();
                for of in &state.models {
                    let hours: usize = of.cells.iter().map(|kept| kept.hours.len()).sum();
                    let part = stored::encode(&map.grid, &of.cells);
                    // The same hours as they were written before they had
                    // their weather, near enough: with none.
                    let bare: Vec<Kept> = of
                        .cells
                        .iter()
                        .map(|kept| Kept {
                            ground_m: kept.ground_m,
                            hours: kept
                                .hours
                                .iter()
                                .map(|(time, hour)| {
                                    let bare = KeptHour {
                                        weather: Weather::default(),
                                        ..*hour
                                    };
                                    (*time, bare)
                                })
                                .collect(),
                        })
                        .collect();
                    let before = stored::encode(&map.grid, &bare);
                    println!(
                        "{}: {hours} hours kept ({:.1} a cell); the part {} ({:.1} bytes an hour), \
                         without the weather {} ({:.1})",
                        of.model,
                        hours as f64 / 1817.0,
                        kb(part.len()),
                        part.len() as f64 / hours as f64,
                        kb(before.len()),
                        before.len() as f64 / hours as f64,
                    );
                }
            }

            let days = map.current().unwrap();
            println!(
                "GET /v1/thermals: {} days, {} plain, {} gzip",
                days.days.len(),
                kb(serde_json::to_vec(&*days).unwrap().len()),
                kb(gzip(&serde_json::to_vec(&*days).unwrap())),
            );
            let date = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
            let day = map.hours_of(date).unwrap();
            let weigh = |what: &str, value: &serde_json::Value| {
                let plain = serde_json::to_vec(value).unwrap();
                println!(
                    "  {what}: {} plain, {} gzip",
                    kb(plain.len()),
                    kb(gzip(&plain))
                );
                (plain.len(), gzip(&plain))
            };
            println!(
                "GET /v1/thermals/hours for {date}, {} of {} hours, a layer an answer:",
                day.model,
                day.hours.len()
            );
            let layer =
                |layer, height| serde_json::to_value(day.layer(layer, height).unwrap()).unwrap();
            weigh("layer=thermals", &layer(ThermalLayer::Thermals, None));
            weigh(
                "layer=wind (at the ground, with gusts)",
                &layer(ThermalLayer::Wind, None),
            );
            for height in MAP_HEIGHTS_M {
                weigh(
                    &format!("layer=wind&height={height}"),
                    &layer(ThermalLayer::Wind, Some(i32::from(height))),
                );
            }
            weigh("layer=cloud", &layer(ThermalLayer::Cloud, None));
            weigh("layer=rain", &layer(ThermalLayer::Rain, None));

            // The other choice: one answer for the day with every list
            // in it, the wind at the heights as a list of pairs of lists.
            let mut whole = layer(ThermalLayer::Thermals, None);
            let others = [
                layer(ThermalLayer::Wind, None),
                layer(ThermalLayer::Cloud, None),
                layer(ThermalLayer::Rain, None),
            ];
            let heights: Vec<serde_json::Value> = MAP_HEIGHTS_M
                .iter()
                .map(|height| layer(ThermalLayer::Wind, Some(i32::from(*height))))
                .collect();
            for (at, hour) in whole["hours"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                let lists = hour.as_object_mut().unwrap();
                for other in &others {
                    for (name, list) in other["hours"][at].as_object().unwrap() {
                        lists.insert(name.clone(), list.clone());
                    }
                }
                let aloft: Vec<serde_json::Value> = heights
                    .iter()
                    .map(|height| {
                        serde_json::json!({
                            "kmh": height["hours"][at]["wind_kmh"],
                            "from_deg": height["hours"][at]["wind_from_deg"],
                        })
                    })
                    .collect();
                lists.insert("aloft".to_owned(), aloft.into());
            }
            weigh("everything in one answer for the day", &whole);
            // And the layers with their directions in tens, 0 to 35, in
            // place of degrees.
            let mut tens = layer(ThermalLayer::Wind, Some(2000));
            for hour in tens["hours"].as_array_mut().unwrap() {
                for value in hour["wind_from_deg"].as_array_mut().unwrap() {
                    if let Some(degrees) = value.as_u64() {
                        *value = (degrees / 10).into();
                    }
                }
            }
            weigh(
                "layer=wind&height=2000, directions in tens of degrees",
                &tens,
            );
        }
    }

    #[test]
    fn a_part_written_before_the_hours_had_their_weather_is_not_read() {
        use std::io::Write as _;

        let grid = grid_over(BoundingBox::new(47.0, 11.0, 47.0, 11.0).unwrap()).unwrap();
        let packed = |text: &str| {
            let mut packed =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            packed.write_all(text.as_bytes()).unwrap();
            packed.finish().unwrap()
        };
        let grid_text = serde_json::to_string(&grid).unwrap();
        // The form before: an hour is its time, its run, the verdict, the
        // kind, what speaks against it and the air.
        let before = format!(
            r#"{{"form":1,"grid":{grid_text},"cells":[{{"g":800.0,"h":[[1783857600,1783814400,0,null,null,null]]}}]}}"#
        );
        assert!(stored::decode(&packed(&before), &grid).is_none());
        // This form, of the same hour, is read.
        let now = format!(
            r#"{{"form":2,"grid":{grid_text},"heights":[1000,1500,2000,2500,3000,4000],"cells":[{{"g":800.0,"h":[[1783857600,1783814400,0,null,null,null,[7,27,14,[12,18,24,30,36,null],[28,30,31,32,33,null],40,0]]]}}]}}"#
        );
        let read = stored::decode(&packed(&now), &grid).unwrap();
        let hour = read[0].hours.values().next().unwrap();
        assert_eq!(hour.weather.wind_from, Some(27));
        assert_eq!(hour.weather.aloft_kmh[4], Some(36));
        // Written for other heights, it is made again.
        let other_heights = now.replace("[1000,1500,", "[1200,1500,");
        assert!(stored::decode(&packed(&other_heights), &grid).is_none());
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
            .map(|kept| kept.run);
        assert_eq!(morning, Some(at(12, 0)));
        // With its weather, in the answer for the day's hours.
        let date = NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        let hours = restarted.hours_of(date).unwrap();
        assert_eq!(hours.hours[0].time, at(12, 5));
        assert_eq!(hours.hours[3].wind_from_deg[0], Some(0));
        assert_eq!(hours.hours[5].wind_from_deg[0], Some(90));
        assert_eq!(hours.hours[5].aloft[2].kmh[0], Some(24));

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
