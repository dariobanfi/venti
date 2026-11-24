//! The thermal map: the kind of day, square by square, over the area the
//! server has forecasts for.
//!
//! The server judges one cell of a weather model for every square of a
//! grid, with the flight engine's rules ([`venti_flight::day`]), and hands
//! out the result as one answer. The shell asks for it
//! ([`thermal_map_path`]) and hands the answer here; out come the squares
//! of each day as a map draws them, with the words for the square a pilot
//! taps.
//!
//! A square is judged at the height of the model's ground, which in the
//! mountains lies below most take-offs: the thermals have further to go
//! from there, so a square can look better than a take-off in it. A
//! take-off's own page ([`crate::site_outlook`]) judges at the take-off's
//! height and knows the directions it works in. The map is where to look;
//! the page is what to read before going.
//!
//! The map of the days is one answer. A day's hours, with the model's
//! wind, cloud and rain in each, are asked for a layer at a time
//! ([`crate::thermal_hours_path`], [`crate::ThermalMapHours`]); what
//! layers and heights there are to choose from, the map of the days says
//! ([`ThermalMap::layers`], [`ThermalMap::levels`]). And a square's days
//! side by side, for the sheet of a tapped square, are read from the same
//! answer ([`thermal_map_strip`]).

use chrono::{DateTime, Duration, NaiveDate};
use venti_api_types::v1;

use crate::forecast::model_name;
use crate::server::answer;
use crate::thermal_hours::{ThermalMapLayerChoice, ThermalMapLevel, layers, levels};
use crate::wind::credit;
use crate::{CoreError, Credit, Flyability, Thermals};

/// The most cells a map may have and still be read. The first region has
/// some 1,800; a hundred times that is a server that is not ours.
pub(crate) const MAX_CELLS: usize = 200_000;

/// The steps of the design language's thermal scale the kinds of day are
/// drawn in, from no thermals to a big day. The scale has six steps and
/// is made for a climb in m/s; the kinds take five of them. No thermals
/// the palest, which is the colour of the drawn map's land and is not
/// seen on it: there is nothing to see; and a gap between local thermals
/// and a cross-country day, the difference a pilot looks for first.
/// Colours of their own for the kinds of day are the owner's to give.
const SCALE_STEPS: [u8; 5] = [1, 2, 3, 5, 6];

/// One square of the thermal map on one day.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapCell {
    /// The square's southern edge, degrees of latitude.
    pub south: f64,
    /// Its western edge, degrees of longitude.
    pub west: f64,
    /// Its northern edge.
    pub north: f64,
    /// Its eastern edge.
    pub east: f64,
    /// Whether the day can be flown there, by the air alone.
    pub verdict: Flyability,
    /// The kind of day its thermals make. None on a day that is not
    /// flyable, and where the model gives no estimate.
    pub thermals: Option<Thermals>,
    /// The step of the thermal scale to draw the square in, 1 to 6. None
    /// where there is no kind of day: draw a day that is not flyable as a
    /// quiet veil, and nothing where there is no estimate. Not in the
    /// status colour for danger, which is nearly the scale's darkest step:
    /// the best day and the worst would look alike.
    pub scale_step: Option<u8>,
    /// The verdict in words: "Flyable", "Marginal", "Not flyable".
    pub status: String,
    /// What speaks against the day there, in words: "Rain", "Gusts",
    /// "Wind higher up". What its hours mostly have against them, without
    /// a figure: the day has many hours. Empty on a flyable day, and
    /// where the server does not say.
    pub why: String,
    /// The kind of day in words: "Cross-country day". Empty without one.
    pub thermal_words: String,
    /// The thermals in a line: "1.8 m/s to a cloud base of 3250 m",
    /// "1.4 m/s to 2900 m in the blue", "0.3 m/s to 1800 m at best",
    /// "None to climb in". Empty without a kind of day.
    pub thermal_line: String,
    /// The height the square was judged at: the model's ground there,
    /// metres above mean sea level. None where the server does not say.
    pub ground_m: Option<f64>,
}

/// One day of the thermal map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapDay {
    /// The day as a date: "2026-07-12". It is the day by the sun at the
    /// squares.
    pub date: String,
    /// What to call it: "Today", "Tomorrow", else the day of the week,
    /// "Mon".
    pub name: String,
    /// Whether the day is today by the pilot's clock. A shell asks this,
    /// never whether `name` is "Today". False against an unknown clock.
    #[uniffi(default = false)]
    pub today: bool,
    /// The weather model the day is read from, by the name pilots know:
    /// "ICON-D2", or "ICON-EU" for a day the finest model does not reach.
    /// Show it with the day.
    pub model: String,
    /// When the newest run of that model began that the day holds hours
    /// of, seconds since 1970. Show it with the day.
    pub run_unix_s: i64,
    /// Whether every square was judged with all of the day's daylight
    /// hours.
    pub whole: bool,
    /// What to say under the map of this day: that it is not whole, that
    /// it is read from a coarser model than the map's finest, or both.
    /// Empty on a whole day of the finest model.
    pub note: String,
    /// The squares that have a verdict, from the south-west, row by row.
    pub cells: Vec<ThermalMapCell>,
}

/// One line of the map's key.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalLegendEntry {
    /// What the colour stands for: "Big day".
    pub words: String,
    /// The step of the thermal scale it is drawn in. None for the line of
    /// a day that is not flyable, which is drawn as a quiet veil.
    pub scale_step: Option<u8>,
}

/// The thermal map as a shell draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMap {
    /// Today and the days after it that the map has, the earliest first.
    pub days: Vec<ThermalMapDay>,
    /// The key: the kinds of day from the best down, then the day that is
    /// not flyable.
    pub legend: Vec<ThermalLegendEntry>,
    /// The weather model, by the name pilots know: "ICON-D2".
    pub model: String,
    /// When the newest run began that the map holds hours of, seconds
    /// since 1970.
    pub run_unix_s: i64,
    /// When the server worked the map out.
    pub computed_unix_s: i64,
    /// What the map is and is not, for the foot of the screen.
    pub note: String,
    /// What a day's hours can be asked for and drawn as, in the order a
    /// shell offers them: thermals, wind, gusts, cloud, rain, each with
    /// its name and its key. Empty for a server that has no hours to ask
    /// for: the map is then the days alone.
    pub layers: Vec<ThermalMapLayerChoice>,
    /// The levels the wind can be asked for at: 10 m above the model's
    /// ground first, then the heights above the sea the server keeps, the
    /// lowest first. Empty where `layers` is.
    pub levels: Vec<ThermalMapLevel>,
    /// The credits to show wherever the map is.
    pub credits: Vec<Credit>,
}

/// One day of a square of the map, for a strip of its days side by side.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalStripDay {
    /// The day as a date: "2026-07-12". The date of a [`ThermalMapDay`].
    pub date: String,
    /// What to call it: "Today", "Tomorrow", else the day of the week.
    pub name: String,
    /// Whether the day is today by the pilot's clock, as
    /// [`ThermalMapDay::today`].
    #[uniffi(default = false)]
    pub today: bool,
    /// The weather model the day is read from: "ICON-D2".
    pub model: String,
    /// Whether the square was judged with all of the day's daylight
    /// hours, as the day of the map says of all its squares.
    pub whole: bool,
    /// The square on that day: its verdict, its kind of day with the step
    /// of the scale for a mark, and the words. None where the day has no
    /// verdict for the square: show the day with nothing under it.
    pub square: Option<ThermalMapCell>,
}

/// The path to `GET` from the server for the thermal map.
#[uniffi::export]
pub fn thermal_map_path() -> String {
    v1::THERMALS_PATH.to_owned()
}

/// Reads the server's answer to [`thermal_map_path`]. `utc_offset_s` is
/// how far the pilot's clock is ahead of UTC and `now_unix_s` the moment
/// by Venti's clock: they decide which day is called today, and days
/// before it are left out.
///
/// A `now_unix_s` of zero or less, the clock not known, leaves no day out
/// and names every day by its weekday, never "Today" or "Tomorrow".
#[uniffi::export]
pub fn thermal_map(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: i64,
) -> Result<ThermalMap, CoreError> {
    let response = read(status, &body)?;
    let grid = response.grid;
    let cells = grid.cells();
    // Every list is as long as the grid has cells, or the map is not one.
    let fits = |day: &v1::ThermalMapDay| {
        [
            day.verdict.len(),
            // A server older than the reasons sends none.
            if day.why.is_empty() {
                cells
            } else {
                day.why.len()
            },
            day.thermals.len(),
            day.climb_ms.len(),
            day.top_m.len(),
            day.cloud.len(),
        ]
        .iter()
        .all(|length| *length == cells)
    };
    if !response.days.iter().all(fits) {
        return Err(unreadable());
    }

    let today = today(utc_offset_s, now_unix_s);
    let name = model_name(response.model.as_str());
    let days = response
        .days
        .iter()
        .filter(|day| day.date >= today)
        .map(|day| {
            let model = day.model.as_ref().unwrap_or(&response.model);
            let coarser = model != &response.model;
            ThermalMapDay {
                date: day.date.format("%Y-%m-%d").to_string(),
                name: day_name(day.date, today),
                today: day.date == today,
                model: model_name(model.as_str()),
                run_unix_s: day.run.unwrap_or(response.run).timestamp(),
                whole: day.whole,
                note: day_note(
                    day.whole,
                    coarser.then(|| model_name(model.as_str())),
                    &name,
                ),
                cells: (0..cells)
                    .filter_map(|cell| {
                        square(&grid, &Judged::of_day(day), &response.ground_m, cell)
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(ThermalMap {
        days,
        legend: legend(),
        // An estimate by fixed limits that knows nothing of a slope: said
        // in docs/design.md, not on the screen (owner decision 19).
        note: "Each square judged at the model's ground by the wind, the rain and the heat; a \
               take-off's own page judges at its height."
            .to_owned(),
        model: name,
        run_unix_s: response.run.timestamp(),
        computed_unix_s: response.computed_at.timestamp(),
        // A server that names no heights has no hours to ask for.
        layers: if response.heights_m.is_empty() {
            Vec::new()
        } else {
            layers()
        },
        levels: if response.heights_m.is_empty() {
            Vec::new()
        } else {
            levels(&response.heights_m, &response.heights_above_ground_m)
        },
        credits: response.attributions.iter().map(credit).collect(),
    })
}

/// A square's days side by side: every day from today on that the map
/// has, with what it says of the square a place lies in. For the sheet of
/// a tapped square, which shows them as a strip: which day, at a glance.
///
/// `status` and `body` are the server's answer to [`thermal_map_path`],
/// the same that [`thermal_map`] read: the shell keeps it. `utc_offset_s`
/// and `now_unix_s` are as there, the unknown clock too. Empty for a place
/// outside the map.
#[uniffi::export]
pub fn thermal_map_strip(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: i64,
    lat: f64,
    lon: f64,
) -> Result<Vec<ThermalStripDay>, CoreError> {
    let response = read(status, &body)?;
    let Some(cell) = cell_at(&response.grid, lat, lon) else {
        return Ok(Vec::new());
    };
    let today = today(utc_offset_s, now_unix_s);
    Ok(response
        .days
        .iter()
        .filter(|day| day.date >= today)
        .map(|day| ThermalStripDay {
            date: day.date.format("%Y-%m-%d").to_string(),
            name: day_name(day.date, today),
            today: day.date == today,
            model: model_name(day.model.as_ref().unwrap_or(&response.model).as_str()),
            whole: day.whole,
            square: square(
                &response.grid,
                &Judged::of_day(day),
                &response.ground_m,
                cell,
            ),
        })
        .collect())
}

/// The server's map, if it is one whose grid can be drawn.
fn read(status: u16, body: &[u8]) -> Result<v1::ThermalMapResponse, CoreError> {
    let response: v1::ThermalMapResponse = answer(status, body)?;
    if !grid_is_sound(&response.grid) || response.ground_m.len() != response.grid.cells() {
        return Err(unreadable());
    }
    Ok(response)
}

fn unreadable() -> CoreError {
    CoreError::Server {
        message:
            "The server's thermal map could not be read. The app may be older than the server."
                .to_owned(),
    }
}

/// Whether a grid is one the core draws: a step, and no more cells than
/// a server of ours has.
pub(crate) fn grid_is_sound(grid: &v1::ThermalGrid) -> bool {
    grid.cells() <= MAX_CELLS && grid.step_deg.is_finite() && grid.step_deg > 0.0
}

/// The number of the square a place lies in. None outside the grid, and
/// for a place that is none.
pub(crate) fn cell_at(grid: &v1::ThermalGrid, lat: f64, lon: f64) -> Option<usize> {
    venti_domain::GeoPoint::new(lat, lon)
        .ok()
        .and_then(|place| grid.cell_at(place))
}

/// The edges of a square: south, west, north, east. None for a number the
/// grid does not have.
pub(crate) fn edges(grid: &v1::ThermalGrid, cell: usize) -> Option<(f64, f64, f64, f64)> {
    let middle = grid.point(cell)?;
    let half = grid.step_deg / 2.0;
    Some((
        (middle.lat() - half).max(-90.0),
        middle.lon() - half,
        (middle.lat() + half).min(90.0),
        middle.lon() + half,
    ))
}

/// The day it is by the pilot's clock. Against an unknown clock, a
/// `now_unix_s` of zero or less, the first day there is: every day is
/// then still to come, and none is today or tomorrow.
pub(crate) fn today(utc_offset_s: i32, now_unix_s: i64) -> NaiveDate {
    if now_unix_s <= 0 {
        return NaiveDate::MIN;
    }
    (DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default()
        + Duration::seconds(i64::from(utc_offset_s)))
    .date_naive()
}

/// What to say under a day: whether it is whole, and the coarser model it
/// is read from, if it is not the map's finest, `finest`.
pub(crate) fn day_note(whole: bool, coarser: Option<String>, finest: &str) -> String {
    let mut note = Vec::new();
    if let Some(coarser) = coarser {
        note.push(format!("Read from {coarser}, coarser than {finest}."));
    }
    if !whole {
        note.push("Not every hour is forecast yet: it can look worse than it is.".to_owned());
    }
    note.join(" ")
}

/// What to call a day that is `date`, when today is `today`.
pub(crate) fn day_name(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".to_owned(),
        1 => "Tomorrow".to_owned(),
        _ => date.format("%a").to_string(),
    }
}

/// The key of the map.
pub(crate) fn legend() -> Vec<ThermalLegendEntry> {
    [
        Thermals::Big,
        Thermals::CrossCountry,
        Thermals::Local,
        Thermals::Weak,
        Thermals::NoThermals,
    ]
    .into_iter()
    .map(|kind| ThermalLegendEntry {
        words: kind.words().to_owned(),
        scale_step: Some(scale_step(kind)),
    })
    .chain([ThermalLegendEntry {
        words: Flyability::NotFlyable.words().to_owned(),
        scale_step: None,
    }])
    .collect()
}

/// The step of the thermal scale a kind of day is drawn in.
pub(crate) fn scale_step(kind: Thermals) -> u8 {
    match kind {
        Thermals::NoThermals => SCALE_STEPS[0],
        Thermals::Weak => SCALE_STEPS[1],
        Thermals::Local => SCALE_STEPS[2],
        Thermals::CrossCountry => SCALE_STEPS[3],
        Thermals::Big => SCALE_STEPS[4],
    }
}

/// What the server judged of every square, for a day or for one hour of
/// it: the lists a square is read from.
pub(crate) struct Judged<'a> {
    verdict: &'a [Option<v1::ThermalVerdict>],
    why: &'a [Option<v1::ThermalReason>],
    thermals: &'a [Option<v1::ThermalKind>],
    climb_ms: &'a [Option<f64>],
    top_m: &'a [Option<i32>],
    cloud: &'a [bool],
    /// Whether the lists are an hour's: its thermals are then its own,
    /// where a day of weak thermals names the best of its hours.
    an_hour: bool,
}

impl<'a> Judged<'a> {
    pub(crate) fn of_day(day: &'a v1::ThermalMapDay) -> Self {
        Self {
            verdict: &day.verdict,
            why: &day.why,
            thermals: &day.thermals,
            climb_ms: &day.climb_ms,
            top_m: &day.top_m,
            cloud: &day.cloud,
            an_hour: false,
        }
    }

    pub(crate) fn of_hour(hour: &'a v1::ThermalMapHour) -> Self {
        Self {
            verdict: &hour.verdict,
            why: &hour.why,
            thermals: &hour.thermals,
            climb_ms: &hour.climb_ms,
            top_m: &hour.top_m,
            cloud: &hour.cloud,
            an_hour: true,
        }
    }
}

/// One square of a day or of an hour, if there is a verdict for it and
/// the square lies on the globe.
pub(crate) fn square(
    grid: &v1::ThermalGrid,
    day: &Judged<'_>,
    ground_m: &[Option<i32>],
    cell: usize,
) -> Option<ThermalMapCell> {
    let verdict = match day.verdict.get(cell).copied()?? {
        v1::ThermalVerdict::Flyable => Flyability::Flyable,
        v1::ThermalVerdict::Marginal => Flyability::Marginal,
        v1::ThermalVerdict::NotFlyable => Flyability::NotFlyable,
    };
    let (south, west, north, east) = edges(grid, cell)?;
    // A day that is not flyable says nothing of thermals, whatever the
    // answer holds.
    let thermals = day
        .thermals
        .get(cell)
        .copied()
        .flatten()
        .filter(|_| verdict != Flyability::NotFlyable)
        .map(|kind| match kind {
            v1::ThermalKind::NoThermals => Thermals::NoThermals,
            v1::ThermalKind::Weak => Thermals::Weak,
            v1::ThermalKind::Local => Thermals::Local,
            v1::ThermalKind::CrossCountry => Thermals::CrossCountry,
            v1::ThermalKind::Big => Thermals::Big,
        });
    let climb_ms = day.climb_ms.get(cell).copied().flatten();
    let top_m = day.top_m.get(cell).copied().flatten();
    let cloud = day.cloud.get(cell).copied().unwrap_or(false);
    Some(ThermalMapCell {
        south,
        west,
        north,
        east,
        verdict,
        thermals,
        scale_step: thermals.map(scale_step),
        status: verdict.words().to_owned(),
        why: day
            .why
            .get(cell)
            .copied()
            .flatten()
            .filter(|_| verdict != Flyability::Flyable)
            .map_or_else(String::new, |reason| words_of(reason).to_owned()),
        thermal_words: thermals.map_or_else(String::new, |kind| kind.words().to_owned()),
        thermal_line: thermals.map_or_else(String::new, |kind| {
            line(kind, climb_ms, top_m, cloud, day.an_hour)
        }),
        ground_m: ground_m.get(cell).copied().flatten().map(f64::from),
    })
}

/// What speaks against a day or an hour, in words.
pub(crate) fn words_of(reason: v1::ThermalReason) -> &'static str {
    match reason {
        v1::ThermalReason::Rain => "Rain",
        v1::ThermalReason::Wind => "Wind",
        v1::ThermalReason::Gusts => "Gusts",
        v1::ThermalReason::WindAloft => "Wind higher up",
        v1::ThermalReason::CloudBase => "Cloud base near the ground",
        v1::ThermalReason::Storms => "Showers or thunderstorms can grow",
    }
}

/// A square's thermals in a line. Weak thermals are the best hour's for
/// a day, and say so; an hour's are its own.
fn line(
    kind: Thermals,
    climb_ms: Option<f64>,
    top_m: Option<i32>,
    cloud: bool,
    an_hour: bool,
) -> String {
    match (kind, climb_ms, top_m) {
        (Thermals::Weak, Some(climb), Some(top)) if climb > 0.0 && an_hour => {
            format!("{climb:.1} m/s to {top} m")
        }
        (Thermals::Weak, Some(climb), Some(top)) if climb > 0.0 => {
            format!("{climb:.1} m/s to {top} m at best")
        }
        (Thermals::Weak | Thermals::NoThermals, _, _) => "None to climb in".to_owned(),
        (_, Some(climb), Some(top)) if cloud => {
            format!("{climb:.1} m/s to a cloud base of {top} m")
        }
        (_, Some(climb), Some(top)) => format!("{climb:.1} m/s to {top} m in the blue"),
        // A kind of day without its figures: say the kind and no more.
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map of three squares in a row for two days, as a server sends it.
    fn body() -> Vec<u8> {
        br#"{
            "source": "open-meteo", "model": "icon-d2",
            "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 3},
            "ground_m": [612, 1240, null],
            "days": [
                {"date": "2026-07-11", "whole": true,
                 "verdict": ["flyable", "flyable", "flyable"],
                 "thermals": ["local", "local", "local"],
                 "climb_ms": [0.7, 0.7, 0.7], "top_m": [1800, 1800, 1800],
                 "cloud": [false, false, false]},
                {"date": "2026-07-12", "whole": false,
                 "verdict": ["flyable", "not_flyable", null],
                 "why": ["gusts", "wind_aloft", null],
                 "thermals": ["cross_country", "big", null],
                 "climb_ms": [1.8, 2.4, null], "top_m": [3250, 3400, null],
                 "cloud": [true, false, false]},
                {"date": "2026-07-13", "whole": true,
                 "verdict": ["marginal", "flyable", "flyable"],
                 "why": ["storms", null, null],
                 "thermals": ["weak", "weak", null],
                 "climb_ms": [0.3, 0.0, null], "top_m": [1800, null, null],
                 "cloud": [false, false, false]},
                {"date": "2026-07-14", "model": "icon-eu", "run": "2026-07-12T00:00:00Z",
                 "whole": true,
                 "verdict": ["flyable", "flyable", "flyable"],
                 "thermals": ["big", "local", "big"],
                 "climb_ms": [2.2, 0.6, 2.6], "top_m": [3400, 2100, 3600],
                 "cloud": [true, true, false]}
            ],
            "attributions": [{"source": "open-meteo", "provider": "Open-Meteo",
                "licence": "CC BY 4.0", "text": "Weather data by Open-Meteo.com",
                "url": "https://open-meteo.com/"}]
        }"#
        .to_vec()
    }

    /// Noon on 12 July 2026 by UTC.
    const NOON: i64 = 1_783_857_600;

    fn map() -> ThermalMap {
        thermal_map(200, body(), 7200, NOON).unwrap()
    }

    #[test]
    fn the_path_is_the_servers() {
        assert_eq!(thermal_map_path(), "/v1/thermals");
    }

    #[test]
    fn against_an_unknown_clock_every_day_is_named_by_its_weekday() {
        let map = thermal_map(200, body(), 7200, 0).unwrap();
        let names: Vec<&str> = map.days.iter().map(|day| day.name.as_str()).collect();
        assert_eq!(names, ["Sat", "Sun", "Mon", "Tue"]);
    }

    #[test]
    fn against_an_unknown_clock_a_squares_strip_has_every_day_by_its_weekday() {
        let strip = thermal_map_strip(200, body(), 7200, 0, 47.0, 11.0).unwrap();
        let names: Vec<&str> = strip.iter().map(|day| day.name.as_str()).collect();
        assert_eq!(names, ["Sat", "Sun", "Mon", "Tue"]);
        assert!(strip.iter().all(|day| !day.today));
    }

    #[test]
    fn days_are_named_by_the_pilots_clock_and_the_past_is_left_out() {
        let map = map();
        let names: Vec<&str> = map.days.iter().map(|day| day.name.as_str()).collect();
        assert_eq!(names, ["Today", "Tomorrow", "Tue"]);
        let today: Vec<bool> = map.days.iter().map(|day| day.today).collect();
        assert_eq!(today, [true, false, false]);
        assert_eq!(map.days[0].date, "2026-07-12");

        // An hour before midnight in Greenwich it is the next day already
        // two hours east of it.
        let late = thermal_map(200, body(), 7200, NOON + 11 * 3600).unwrap();
        assert_eq!(late.days[0].date, "2026-07-13");
        assert_eq!(late.days[0].name, "Today");
        assert!(late.days[0].today);
    }

    #[test]
    fn a_square_lies_half_a_step_around_its_point() {
        let map = map();
        let cell = &map.days[0].cells[0];
        assert!((cell.south - 46.95).abs() < 1e-9 && (cell.north - 47.05).abs() < 1e-9);
        assert!((cell.west - 10.95).abs() < 1e-9 && (cell.east - 11.05).abs() < 1e-9);
        assert_eq!(cell.ground_m, Some(612.0));
    }

    #[test]
    fn a_square_says_its_kind_of_day_in_words_and_in_a_step_of_the_scale() {
        let map = map();
        let today = &map.days[0].cells;
        // The third square has no verdict today: it is not drawn.
        assert_eq!(today.len(), 2);
        assert_eq!(today[0].status, "Flyable");
        assert_eq!(today[0].thermals, Some(Thermals::CrossCountry));
        assert_eq!(today[0].thermal_words, "Cross-country day");
        assert_eq!(today[0].thermal_line, "1.8 m/s to a cloud base of 3250 m");
        assert_eq!(today[0].scale_step, Some(5));

        let later = &map.days[2].cells;
        assert_eq!(later[0].thermal_line, "2.2 m/s to a cloud base of 3400 m");
        assert_eq!(later[0].scale_step, Some(6));
        assert_eq!(later[1].thermal_words, "Local thermals");
        assert_eq!(later[1].scale_step, Some(3));
        assert_eq!(later[2].thermal_line, "2.6 m/s to 3600 m in the blue");
    }

    #[test]
    fn a_day_that_is_not_flyable_names_no_thermals_whatever_the_answer_holds() {
        let map = map();
        let cell = &map.days[0].cells[1];
        assert_eq!(cell.verdict, Flyability::NotFlyable);
        assert_eq!(cell.status, "Not flyable");
        assert_eq!(cell.why, "Wind higher up");
        // A flyable day has nothing against it, whatever the answer holds.
        assert_eq!(map.days[0].cells[0].why, "");
        assert_eq!(cell.thermals, None);
        assert_eq!(cell.scale_step, None);
        assert_eq!(cell.thermal_words, "");
        assert_eq!(cell.thermal_line, "");
        assert_eq!(cell.ground_m, Some(1240.0));
    }

    #[test]
    fn weak_thermals_say_the_best_there_is_or_that_there_is_none() {
        let map = map();
        let tomorrow = &map.days[1].cells;
        assert_eq!(tomorrow[0].status, "Marginal");
        assert_eq!(tomorrow[0].why, "Showers or thunderstorms can grow");
        assert_eq!(tomorrow[0].thermal_words, "Weak thermals");
        assert_eq!(tomorrow[0].thermal_line, "0.3 m/s to 1800 m at best");
        assert_eq!(tomorrow[0].scale_step, Some(2));
        assert_eq!(tomorrow[1].thermal_line, "None to climb in");
        // Flyable, and the model gave no estimate: a verdict and no more.
        assert_eq!(tomorrow[2].status, "Flyable");
        assert_eq!(tomorrow[2].thermals, None);
        assert_eq!(tomorrow[2].scale_step, None);
    }

    #[test]
    fn a_server_that_gives_no_reasons_is_read_without_them() {
        // The last day of the sample has no list of reasons, as an older
        // server's answer has none.
        let map = map();
        assert_eq!(map.days[2].cells.len(), 3);
        assert!(map.days[2].cells.iter().all(|cell| cell.why.is_empty()));

        // A list of reasons that does not fit the grid is no map.
        let text = String::from_utf8(body()).unwrap();
        let short = text.replace(r#""why": ["storms", null, null],"#, r#""why": ["storms"],"#);
        assert!(matches!(
            thermal_map(200, short.into_bytes(), 0, NOON),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn a_day_that_is_not_whole_says_so() {
        let map = map();
        assert!(!map.days[0].whole);
        assert_eq!(
            map.days[0].note,
            "Not every hour is forecast yet: it can look worse than it is."
        );
        assert!(map.days[1].whole);
        assert_eq!(map.days[1].note, "");
    }

    #[test]
    fn a_day_names_its_model_and_says_when_it_is_the_coarser_ones() {
        let map = map();
        let days: Vec<(&str, i64)> = map
            .days
            .iter()
            .map(|day| (day.model.as_str(), day.run_unix_s))
            .collect();
        // A day that names no model is the map's finest, of its run.
        assert_eq!(
            days,
            [
                ("ICON-D2", 1_783_825_200),
                ("ICON-D2", 1_783_825_200),
                ("ICON-EU", 1_783_814_400)
            ]
        );
        assert_eq!(map.days[1].note, "");
        assert_eq!(map.days[2].note, "Read from ICON-EU, coarser than ICON-D2.");
        // Both things said of a day that is the coarser model's and not
        // whole.
        let note = day_note(false, Some("ICON-EU".to_owned()), "ICON-D2");
        assert!(note.starts_with("Read from ICON-EU"));
        assert!(note.ends_with("it can look worse than it is."));
    }

    #[test]
    fn a_fourth_day_has_its_weekday_and_says_that_it_is_coarser_and_not_whole() {
        // The map as a server sends it after a run of ICON-EU of 00, 06,
        // 12 or 18 UTC: the day after tomorrow and the day after that are
        // ICON-EU's, and the last, which such a run seldom reaches the end
        // of, is not whole.
        let text = String::from_utf8(body()).unwrap();
        let with_a_fourth = text.replace(
            r#""cloud": [true, true, false]}"#,
            r#""cloud": [true, true, false]},
                {"date": "2026-07-15", "model": "icon-eu", "run": "2026-07-12T00:00:00Z",
                 "whole": false,
                 "verdict": ["flyable", "marginal", null],
                 "why": [null, "gusts", null],
                 "thermals": ["local", "weak", null],
                 "climb_ms": [0.6, 0.2, null], "top_m": [2100, 1500, null],
                 "cloud": [false, false, false]}"#,
        );
        assert_ne!(with_a_fourth, text);
        let map = thermal_map(200, with_a_fourth.into_bytes(), 7200, NOON).unwrap();

        let names: Vec<&str> = map.days.iter().map(|day| day.name.as_str()).collect();
        assert_eq!(names, ["Today", "Tomorrow", "Tue", "Wed"]);
        let last = &map.days[3];
        assert_eq!(last.date, "2026-07-15");
        assert_eq!(
            (last.model.as_str(), last.run_unix_s),
            ("ICON-EU", 1_783_814_400)
        );
        assert!(!last.whole);
        // Both things are said of it, the model first.
        assert_eq!(
            last.note,
            "Read from ICON-EU, coarser than ICON-D2. Not every hour is forecast yet: it can \
             look worse than it is."
        );
        // It is drawn like any other day: the squares that have a verdict.
        assert_eq!(last.cells.len(), 2);
        assert_eq!(last.cells[1].status, "Marginal");
        assert_eq!(last.cells[1].why, "Gusts");
        // The day before is the coarser model's as well, and whole: it
        // says the first thing alone.
        assert!(map.days[2].whole);
        assert!(map.days[2].note.ends_with("coarser than ICON-D2."));
    }

    #[test]
    fn the_map_names_its_model_its_run_and_its_key() {
        let map = map();
        assert_eq!(map.model, "ICON-D2");
        assert_eq!(map.run_unix_s, 1_783_825_200);
        assert_eq!(
            map.note,
            "Each square judged at the model's ground by the wind, the rain and the heat; a \
             take-off's own page judges at its height."
        );
        assert_eq!(map.credits.len(), 1);
        let key: Vec<(&str, Option<u8>)> = map
            .legend
            .iter()
            .map(|entry| (entry.words.as_str(), entry.scale_step))
            .collect();
        assert_eq!(
            key,
            [
                ("Big day", Some(6)),
                ("Cross-country day", Some(5)),
                ("Local thermals", Some(3)),
                ("Weak thermals", Some(2)),
                ("No thermals", Some(1)),
                ("Not flyable", None),
            ]
        );
    }

    #[test]
    fn a_squares_days_stand_side_by_side() {
        let strip = |lat: f64, lon: f64| thermal_map_strip(200, body(), 7200, NOON, lat, lon);

        // The first square, by a place in it: today, tomorrow and Tuesday,
        // each with its verdict, its kind of day and the step for a mark.
        let days = strip(47.03, 10.97).unwrap();
        let read: Vec<(&str, &str, &str, bool)> = days
            .iter()
            .map(|day| {
                (
                    day.date.as_str(),
                    day.name.as_str(),
                    day.model.as_str(),
                    day.whole,
                )
            })
            .collect();
        assert_eq!(
            read,
            [
                ("2026-07-12", "Today", "ICON-D2", false),
                ("2026-07-13", "Tomorrow", "ICON-D2", true),
                ("2026-07-14", "Tue", "ICON-EU", true),
            ]
        );
        let squares: Vec<(Flyability, Option<Thermals>, Option<u8>, &str)> = days
            .iter()
            .map(|day| {
                let square = day.square.as_ref().unwrap();
                (
                    square.verdict,
                    square.thermals,
                    square.scale_step,
                    square.thermal_line.as_str(),
                )
            })
            .collect();
        assert_eq!(
            squares,
            [
                (
                    Flyability::Flyable,
                    Some(Thermals::CrossCountry),
                    Some(5),
                    "1.8 m/s to a cloud base of 3250 m"
                ),
                (
                    Flyability::Marginal,
                    Some(Thermals::Weak),
                    Some(2),
                    "0.3 m/s to 1800 m at best"
                ),
                (
                    Flyability::Flyable,
                    Some(Thermals::Big),
                    Some(6),
                    "2.2 m/s to a cloud base of 3400 m"
                ),
            ]
        );
        // They are the squares the map of each day has.
        let map = map();
        assert_eq!(days[0].square.as_ref(), Some(&map.days[0].cells[0]));
        assert_eq!(
            days[1].square.as_ref().unwrap().why,
            "Showers or thunderstorms can grow"
        );

        // The third square has no verdict today: the day is in the strip
        // with nothing under it.
        let days = strip(47.0, 11.2).unwrap();
        assert_eq!(days.len(), 3);
        assert_eq!(days[0].square, None);
        assert_eq!(days[2].square.as_ref().unwrap().thermal_words, "Big day");

        // By the pilot's clock, as the map: late in the evening today is
        // the next day.
        let late = thermal_map_strip(200, body(), 7200, NOON + 11 * 3600, 47.0, 11.0).unwrap();
        assert_eq!(late[0].name, "Today");
        assert_eq!(late[0].date, "2026-07-13");

        // Outside the map there is no square, and what is no place has
        // none; a refusal of the server is passed on.
        assert_eq!(strip(48.0, 11.0).unwrap(), []);
        assert_eq!(strip(f64::NAN, 11.0).unwrap(), []);
        assert!(thermal_map_strip(503, b"{}".to_vec(), 0, NOON, 47.0, 11.0).is_err());
    }

    #[test]
    fn a_map_whose_lists_do_not_fit_its_grid_is_refused() {
        let text = String::from_utf8(body()).unwrap();
        let short = text.replace(r#""ground_m": [612, 1240, null]"#, r#""ground_m": [612]"#);
        assert!(matches!(
            thermal_map(200, short.into_bytes(), 0, NOON),
            Err(CoreError::Server { .. })
        ));
        let no_step = text.replace(r#""step_deg": 0.1"#, r#""step_deg": 0.0"#);
        assert!(matches!(
            thermal_map(200, no_step.into_bytes(), 0, NOON),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn a_refusal_of_the_server_is_passed_on_in_words() {
        let body =
            br#"{"error": {"code": "not_ready", "message": "The thermal map is not made yet."}}"#;
        assert!(thermal_map(503, body.to_vec(), 0, NOON).is_err());
    }
}
