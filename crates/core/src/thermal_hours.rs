//! The hours of a day of the thermal map, and what the weather model has
//! in each: thermals, wind, gusts, cloud and rain, square by square.
//!
//! The map of the days ([`crate::thermal_map`]) says which layers and
//! heights there are ([`crate::ThermalMap::layers`],
//! [`crate::ThermalMap::levels`]). For the day, the layer and the height a
//! pilot chose, the shell asks the server ([`thermal_hours_path`]) and
//! hands the answer to [`ThermalMapHours::read`]. What comes back holds
//! the day's hours of that layer and is asked hour by hour, as the pilot
//! drags along the line of hours: what to draw ([`ThermalMapHours::shapes`])
//! and, for a square the pilot taps, the words
//! ([`ThermalMapHours::square`]).
//!
//! **What crosses to the shell.** A layer of the first region is 1,817
//! squares in each of nine to fifteen hours. As a record with words for
//! every square, as the map of the days hands its squares out, an hour
//! would be 1,817 records with four strings each, built again at every
//! step of a drag; as bare lists of numbers it would be a few kilobytes,
//! and three shells would each build a map's shapes from them in their own
//! language. So an hour crosses as GeoJSON, one text for the squares and
//! one for the arrows, which every shell's map takes as it is: 312 kB for
//! the squares of an hour in which every square of the first region is
//! drawn and 291 kB for its arrows (the tests hold it to that), one text
//! each and nothing to build on the other side. The words cross for the
//! one square that was tapped.
//!
//! **The colours.** The design language has a scale for thermals and one
//! for wind. The kinds of an hour's thermals take the steps the kinds of
//! day take, and the wind and the gusts the steps the wind's four bands
//! are drawn in everywhere else. For cloud and for rain it has none, and
//! they stand in with tokens there are, as the kinds of day did:
//! [`CLOUD_STEPS`] and [`RAIN_STEPS`] say which and why. Colours of their
//! own are the owner's to give.

use std::fmt::Write as _;
use std::sync::Arc;

use chrono::{DateTime, Duration, Timelike};
use venti_api_types::v1;

use crate::forecast::model_name;
use crate::server::answer;
use crate::sites::compass;
use crate::thermal_map::{self, Judged, cell_at, edges, grid_is_sound};
use crate::wind::{band_of_kmh, credit};
use crate::{CoreError, Credit, Flyability, ThermalMapCell, WindBand, WindMark, WindSymbol};

/// The steps of the design language's wind scale the four bands of wind
/// are drawn in, from the lightest: the second, fourth, sixth and seventh
/// of its seven, as the phones draw the arrows of the wind stations.
const WIND_STEPS: [u8; 4] = [2, 4, 6, 7];

/// From how much of the sky, in percent, a square counts as cloudy and as
/// overcast. Under the first it is not drawn. Our own limits, and the
/// owner's to change: half the sky, and what a forecaster calls eight
/// eighths.
const CLOUD_PCT: [u8; 2] = [50, 90];

/// What cloud is drawn in, for want of a scale of its own: a cloudy
/// square in the one grey the design language's scales have, the middle
/// step of `vario`, which there stands for air that neither rises nor
/// sinks; an overcast one as the quiet veil in `text-muted` that a day
/// that is not flyable has on the map of the days. Two greys, the darker
/// for the closed sky. Not a step of the wind's or the thermals' scale:
/// a pilot who switches from the wind to the cloud would read purple as
/// wind.
const CLOUD_STEPS: [(ThermalMapScale, u8); 2] =
    [(ThermalMapScale::Vario, 4), (ThermalMapScale::Veil, 0)];

/// From how many millimetres in the hour a square counts as light rain,
/// as rain and as heavy rain. The first is the flight engine's: from it
/// an hour is not for flying. The others are ours, and the owner's to
/// change. Under the first a square is not drawn.
const RAIN_MM: [f64; 3] = [0.2, 1.0, 4.0];

/// What rain is drawn in, for want of a scale of its own: the three teal
/// steps of `vario`, which there stand for sinking air, from the palest
/// for light rain to the deepest for heavy. Teal is the one colour of
/// the scales that reads as water. It is also near the colour of the
/// drawn map's lakes, which a square of light rain can be taken for:
/// one reason more for colours of rain's own.
const RAIN_STEPS: [u8; 3] = [3, 2, 1];

/// Below this mean speed, in km/h, the wind has no direction worth an
/// arrow: the half metre a second below which a wind station is drawn
/// as calm, in the whole km/h the map has.
const CALM_BELOW_KMH: u16 = 2;

/// What of a day's hours a pilot looks at on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum ThermalHoursLayer {
    /// Each hour's verdict and what its thermals count as.
    Thermals,
    /// The mean wind, 10 m above the model's ground or at a height above
    /// the sea.
    Wind,
    /// The gusts 10 m above the model's ground.
    Gusts,
    /// How much of the sky is covered.
    Cloud,
    /// The rain of the hour.
    Rain,
}

/// The scale of the design language a square is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum ThermalMapScale {
    /// `thermal-1…6`.
    Thermal,
    /// `wind-1…7`.
    Wind,
    /// `vario-1…7`.
    Vario,
    /// No scale: the quiet veil in `text-muted` that a day that is not
    /// flyable has.
    Veil,
}

/// One line of a layer's key, and how the squares it stands for are
/// drawn.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapKey {
    /// The number the squares of this kind carry in what the map is
    /// handed (`step` of a square of [`ThermalMapShapes::squares`]).
    pub step: u8,
    /// What it stands for: "Cross-country day", "20 to 30 km/h",
    /// "Overcast", "Heavy rain".
    pub words: String,
    /// Its amount apart, where the class has one beside its name: "4 mm
    /// an hour and more" for "Heavy rain". Empty where `words` is the
    /// amount itself, or there is none.
    #[uniffi(default = "")]
    pub amount: String,
    /// The scale its colour is of.
    pub scale: ThermalMapScale,
    /// The step of that scale, from 1. Zero for the veil.
    pub scale_step: u8,
}

/// A layer a shell offers, with its name and its key.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapLayerChoice {
    /// The layer.
    pub layer: ThermalHoursLayer,
    /// What to call it: "Thermals", "Wind", "Gusts", "Cloud", "Rain".
    pub name: String,
    /// Whether the layer is asked for at a level
    /// ([`crate::ThermalMap::levels`]): the wind is, the others are not.
    pub has_levels: bool,
    /// Whether the layer has arrows to draw over its squares.
    pub has_arrows: bool,
    /// The key, from the most down. A square that carries none of its
    /// steps is not drawn.
    pub legend: Vec<ThermalMapKey>,
    /// What the layer shows and what it does not, for the foot of the
    /// card.
    pub note: String,
}

/// A level the wind is asked for at.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapLevel {
    /// The height above mean sea level, metres. None for the wind 10 m
    /// above the model's ground, and for a height above the ground.
    pub height_m: Option<i32>,
    /// What to call it in a row of choices: "Ground", "250 m AGL",
    /// "2000 m".
    pub name: String,
    /// The same in full: "10 m above the model's ground", "250 m above
    /// the ground", "2000 m above the sea".
    pub words: String,
    /// The height above the model's ground, metres, for a level that is
    /// one: 250. None for the others. Asked for with
    /// [`thermal_hours_path`]'s `above_ground_m`.
    #[uniffi(default = None)]
    pub above_ground_m: Option<i32>,
}

/// The layers, in the order a shell offers them.
pub(crate) fn layers() -> Vec<ThermalMapLayerChoice> {
    let wind_key = || {
        [
            (WindBand::Strong, "30 km/h and more"),
            (WindBand::Fresh, "20 to 30 km/h"),
            (WindBand::Moderate, "10 to 20 km/h"),
            (WindBand::Light, "Under 10 km/h"),
        ]
        .into_iter()
        .map(|(band, words)| ThermalMapKey {
            step: wind_step(band),
            words: words.to_owned(),
            amount: String::new(),
            scale: ThermalMapScale::Wind,
            scale_step: wind_step(band),
        })
        .collect()
    };
    vec![
        ThermalMapLayerChoice {
            layer: ThermalHoursLayer::Thermals,
            name: "Thermals".to_owned(),
            has_levels: false,
            has_arrows: false,
            legend: thermal_map::legend()
                .into_iter()
                .map(|entry| ThermalMapKey {
                    step: entry.scale_step.unwrap_or(0),
                    words: entry.words,
                    amount: String::new(),
                    scale: match entry.scale_step {
                        Some(_) => ThermalMapScale::Thermal,
                        None => ThermalMapScale::Veil,
                    },
                    scale_step: entry.scale_step.unwrap_or(0),
                })
                .collect(),
            note: "Each hour judged at the model's ground by the wind, the rain and the heat."
                .to_owned(),
        },
        ThermalMapLayerChoice {
            layer: ThermalHoursLayer::Wind,
            name: "Wind".to_owned(),
            has_levels: true,
            has_arrows: true,
            legend: wind_key(),
            note: "The model's mean wind; a valley's own wind is not in it.".to_owned(),
        },
        ThermalMapLayerChoice {
            layer: ThermalHoursLayer::Gusts,
            name: "Gusts".to_owned(),
            has_levels: false,
            has_arrows: true,
            legend: wind_key(),
            note: "The hour's strongest gust; the arrows show the mean wind.".to_owned(),
        },
        ThermalMapLayerChoice {
            layer: ThermalHoursLayer::Cloud,
            name: "Cloud".to_owned(),
            has_levels: false,
            has_arrows: false,
            legend: vec![
                ThermalMapKey {
                    step: 2,
                    words: "Overcast".to_owned(),
                    amount: String::new(),
                    scale: CLOUD_STEPS[1].0,
                    scale_step: CLOUD_STEPS[1].1,
                },
                ThermalMapKey {
                    step: 1,
                    words: "Cloudy".to_owned(),
                    amount: String::new(),
                    scale: CLOUD_STEPS[0].0,
                    scale_step: CLOUD_STEPS[0].1,
                },
            ],
            note: "Cloud at any height: a thin high veil counts as much as a low layer.".to_owned(),
        },
        ThermalMapLayerChoice {
            layer: ThermalHoursLayer::Rain,
            name: "Rain".to_owned(),
            has_levels: false,
            has_arrows: false,
            legend: vec![
                ThermalMapKey {
                    step: 3,
                    words: "Heavy rain".to_owned(),
                    amount: "4 mm an hour and more".to_owned(),
                    scale: ThermalMapScale::Vario,
                    scale_step: RAIN_STEPS[2],
                },
                ThermalMapKey {
                    step: 2,
                    words: "Rain".to_owned(),
                    amount: "1 to 4 mm an hour".to_owned(),
                    scale: ThermalMapScale::Vario,
                    scale_step: RAIN_STEPS[1],
                },
                ThermalMapKey {
                    step: 1,
                    words: "Light rain".to_owned(),
                    amount: "0.2 to 1 mm an hour".to_owned(),
                    scale: ThermalMapScale::Vario,
                    scale_step: RAIN_STEPS[0],
                },
            ],
            note: "Rain, showers and snow in the hour before, as water.".to_owned(),
        },
    ]
}

/// The levels the wind is asked for at: the ground, the heights above it
/// the server keeps, then the heights above the sea it keeps.
pub(crate) fn levels(heights_m: &[i32], above_ground_m: &[i32]) -> Vec<ThermalMapLevel> {
    [ThermalMapLevel {
        height_m: None,
        name: "Ground".to_owned(),
        words: "10 m above the model's ground".to_owned(),
        above_ground_m: None,
    }]
    .into_iter()
    .chain(above_ground_m.iter().map(|height| ThermalMapLevel {
        height_m: None,
        name: format!("{height} m AGL"),
        words: above_the_ground(*height),
        above_ground_m: Some(*height),
    }))
    .chain(heights_m.iter().map(|height| ThermalMapLevel {
        height_m: Some(*height),
        name: format!("{height} m"),
        words: format!("{height} m above the sea"),
        above_ground_m: None,
    }))
    .collect()
}

/// A height above the ground in words: "250 m above the ground".
fn above_the_ground(height_m: i32) -> String {
    format!("{height_m} m above the ground")
}

/// The step of the wind scale a band is drawn in.
fn wind_step(band: WindBand) -> u8 {
    match band {
        WindBand::Light => WIND_STEPS[0],
        WindBand::Moderate => WIND_STEPS[1],
        WindBand::Fresh => WIND_STEPS[2],
        WindBand::Strong => WIND_STEPS[3],
    }
}

/// The path and query to `GET` from the server for one layer of the hours
/// of a day of the thermal map.
///
/// `date` is the day as the map names it ([`crate::ThermalMapDay::date`]).
/// `height_m` is the level the wind is wanted at
/// ([`ThermalMapLevel::height_m`]): none for the wind at the ground. It is
/// of no account to the other layers, and the gusts are the ground's: the
/// wind at the ground and the gusts are one answer, asked for by the same
/// path. `above_ground_m` is a level above the ground in its place
/// ([`ThermalMapLevel::above_ground_m`]), and is of no account where
/// `height_m` is given.
#[uniffi::export(default(above_ground_m = None))]
pub fn thermal_hours_path(
    date: String,
    layer: ThermalHoursLayer,
    height_m: Option<i32>,
    above_ground_m: Option<i32>,
) -> Result<String, CoreError> {
    let date = v1::parse_day(&date).map_err(|_| CoreError::Server {
        message: "The app could not ask the server for this day.".to_owned(),
    })?;
    let height_m = height_m.filter(|_| layer == ThermalHoursLayer::Wind);
    Ok(v1::ThermalHoursRequest {
        date,
        layer: on_the_wire(layer),
        height_m,
        above_ground_m: above_ground_m
            .filter(|_| layer == ThermalHoursLayer::Wind && height_m.is_none()),
    }
    .path_and_query())
}

/// The layer of the server's answer a layer of the map is drawn from.
fn on_the_wire(layer: ThermalHoursLayer) -> v1::ThermalLayer {
    match layer {
        ThermalHoursLayer::Thermals => v1::ThermalLayer::Thermals,
        ThermalHoursLayer::Wind | ThermalHoursLayer::Gusts => v1::ThermalLayer::Wind,
        ThermalHoursLayer::Cloud => v1::ThermalLayer::Cloud,
        ThermalHoursLayer::Rain => v1::ThermalLayer::Rain,
    }
}

/// One hour of the line of hours a pilot drags along.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapHourMark {
    /// The moment the hour's values are for, seconds since 1970-01-01 UTC.
    pub time_unix_s: i64,
    /// The hour of the pilot's clock at that moment, 0 to 23: a short
    /// label under a tick.
    pub clock_hour: u8,
    /// The moment by the pilot's clock: "13:00".
    pub label: String,
}

/// What a card says of the hours of a day in one layer.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapHoursView {
    /// The day as a date: "2026-07-12".
    pub date: String,
    /// The layer.
    pub layer: ThermalHoursLayer,
    /// The height above the sea the wind is at, metres. None at the
    /// ground and for the other layers.
    pub height_m: Option<i32>,
    /// What is shown, in words: "Thermals", "Wind at 2000 m", "Wind at
    /// the ground", "Wind 250 m above the ground", "Gusts at the ground",
    /// "Cloud", "Rain".
    pub title: String,
    /// The day's daylight hours, the earliest first: the stops of the
    /// line of hours. The number of an hour is its place in this list,
    /// from 0.
    pub hours: Vec<ThermalMapHourMark>,
    /// The hour to show first: today the hour that has begun, on another
    /// day the one nearest one o'clock in the afternoon.
    pub first_hour: u32,
    /// The weather model the day is read from: "ICON-D2".
    pub model: String,
    /// When the newest run of that model began that the day holds hours
    /// of, seconds since 1970.
    pub run_unix_s: i64,
    /// Whether every square has all of the day's daylight hours.
    pub whole: bool,
    /// What to say under the line of hours: that some hours are missing,
    /// when the day is not whole. Empty on a whole day: the line of hours
    /// shows which hours there are.
    pub note: String,
    /// The credits to show wherever the layer is.
    pub credits: Vec<Credit>,
    /// The height above the model's ground the wind is at, metres. None
    /// at the ground, at a height above the sea and for the other layers.
    #[uniffi(default = None)]
    pub above_ground_m: Option<i32>,
}

/// What a map draws of one hour: two GeoJSON texts, each a
/// `FeatureCollection`, to hand to the map as they are.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapShapes {
    /// The squares that are drawn in this hour, each a `Polygon` with two
    /// properties: `cell`, the square's number, and `step`, which a line
    /// of the layer's key has too ([`ThermalMapKey::step`]) and which
    /// says what colour to fill it in. A square with nothing to show is
    /// not among them.
    pub squares: String,
    /// The arrows, each a `Point` in the middle of its square, with
    /// `cell`; `symbol`, "arrow" or "calm"; `band`, "light", "moderate",
    /// "fresh" or "strong", the band of the speed the layer shows;
    /// `heading`, where the arrow points, in degrees clockwise from
    /// north, the direction the wind blows towards; and `label`, the
    /// speed in whole km/h, "12", at the ground with the gusts where they
    /// differ, "12/25". An empty collection for a layer without arrows.
    pub arrows: String,
}

/// A square in one hour, for the sheet of a square the pilot tapped.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapHourSquare {
    /// The square's number in the grid.
    pub cell: u32,
    /// Its southern edge, degrees of latitude.
    pub south: f64,
    /// Its western edge, degrees of longitude.
    pub west: f64,
    /// Its northern edge.
    pub north: f64,
    /// Its eastern edge.
    pub east: f64,
    /// The hour by the pilot's clock: "13:00".
    pub hour: String,
    /// The hour's reading in the layer, in words: "12 km/h from W, gusts
    /// 25", "34 km/h from NW", "Gusts 25 km/h", "Overcast", "Light rain",
    /// "Marginal", "No wind at 1000 m here".
    pub title: String,
    /// A second line: "10 m above the model's ground at 1240 m", "At
    /// 2000 m above the sea", "The model's ground lies higher, at 1240 m.",
    /// "92 % of the sky", "0.6 mm in the hour", "Gusts. Local thermals:
    /// 0.7 m/s to 2100 m in the blue". Empty where there is nothing to
    /// add.
    pub detail: String,
    /// The step the square is drawn in, as in [`ThermalMapShapes`] and
    /// the key. None where it is not drawn in this hour.
    pub step: Option<u8>,
    /// On the layer of the thermals: the hour as a square of a day is
    /// given, with the verdict for the status and the kind for the mark.
    /// None on the other layers, and where the hour has no verdict.
    pub thermals: Option<ThermalMapCell>,
    /// On the layers of the wind and the gusts: the arrow, as a wind
    /// station has one. None on the other layers and without a wind.
    pub wind: Option<WindMark>,
    /// The height of the model's ground there, metres above mean sea
    /// level. None where the model gives none.
    pub ground_m: Option<f64>,
}

/// The forecast wind at a place in an hour of the thermal map
/// ([`ThermalMapHours::wind_at`]): what a take-off's fan is coloured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ThermalMapWind {
    /// The mean speed, whole km/h.
    pub kmh: u16,
    /// Where it comes from, degrees clockwise from north, to the nearest
    /// ten. None where the model gives no direction.
    pub from_deg: Option<u16>,
    /// The hour's strongest gust, whole km/h: at the ground only.
    pub gust_kmh: Option<u16>,
    /// The band of the mean speed, the step of the wind scale it is drawn
    /// in.
    pub band: WindBand,
}

/// One layer of the hours of a day of the thermal map: the server's
/// answer, read once and asked hour by hour. It holds the answer, some
/// hundreds of kilobytes for a region; a shell keeps one for what it
/// shows and lets the others go.
#[derive(Debug, uniffi::Object)]
pub struct ThermalMapHours {
    layer: ThermalHoursLayer,
    response: v1::ThermalHoursResponse,
    utc_offset_s: i32,
    first_hour: u32,
}

#[uniffi::export]
impl ThermalMapHours {
    /// Reads the server's answer to [`thermal_hours_path`] for `layer`.
    /// `utc_offset_s` is how far the pilot's clock is ahead of UTC, which
    /// labels the hours, and `now_unix_s` the moment by Venti's clock,
    /// which decides the hour to show first.
    ///
    /// A `now_unix_s` of zero or less, the clock not known, shows the hour
    /// nearest one o'clock in the afternoon first.
    #[uniffi::constructor]
    pub fn read(
        status: u16,
        body: Vec<u8>,
        layer: ThermalHoursLayer,
        utc_offset_s: i32,
        now_unix_s: i64,
    ) -> Result<Arc<Self>, CoreError> {
        let response: v1::ThermalHoursResponse = answer(status, &body)?;
        let unreadable = || CoreError::Server {
            message: "The hours of the server's thermal map could not be read. The app may be \
                      older than the server."
                .to_owned(),
        };
        let cells = response.grid.cells();
        // The answer is the layer that was asked for, the gusts are the
        // ground's, and every list the layer is drawn from is as long as
        // the grid has cells.
        let fits = |hour: &v1::ThermalMapHour| {
            let lengths = match layer {
                ThermalHoursLayer::Thermals => vec![
                    hour.verdict.len(),
                    hour.why.len(),
                    hour.thermals.len(),
                    hour.climb_ms.len(),
                    hour.top_m.len(),
                    hour.cloud.len(),
                ],
                ThermalHoursLayer::Wind => vec![hour.wind_kmh.len(), hour.wind_from_deg.len()],
                ThermalHoursLayer::Gusts => vec![
                    hour.wind_kmh.len(),
                    hour.wind_from_deg.len(),
                    hour.gust_kmh.len(),
                ],
                ThermalHoursLayer::Cloud => vec![hour.cloud_cover_pct.len()],
                ThermalHoursLayer::Rain => vec![hour.precipitation_mm.len()],
            };
            lengths.iter().all(|length| *length == cells)
                // At the ground the wind comes with its gusts or without.
                && (hour.gust_kmh.is_empty() || hour.gust_kmh.len() == cells)
        };
        if response.layer != on_the_wire(layer)
            || (layer == ThermalHoursLayer::Gusts
                && (response.height_m.is_some() || response.above_ground_m.is_some()))
            || !grid_is_sound(&response.grid)
            || response.ground_m.len() != cells
            || !response.hours.iter().all(fits)
        {
            return Err(unreadable());
        }

        let first_hour = first_hour(&response, utc_offset_s, now_unix_s);
        Ok(Arc::new(Self {
            layer,
            response,
            utc_offset_s,
            first_hour,
        }))
    }

    /// What a card says of the hours: the stops of the line of hours, the
    /// hour to show first, the model and its run.
    pub fn view(&self) -> ThermalMapHoursView {
        let response = &self.response;
        let level = match (response.height_m, response.above_ground_m) {
            (Some(height), _) => format!("at {height} m"),
            (None, Some(height)) => above_the_ground(height),
            (None, None) => "at the ground".to_owned(),
        };
        let note = if response.whole {
            String::new()
        } else {
            "Not every hour of this day is forecast yet.".to_owned()
        };
        ThermalMapHoursView {
            date: response.date.format("%Y-%m-%d").to_string(),
            layer: self.layer,
            height_m: response.height_m,
            title: match self.layer {
                ThermalHoursLayer::Thermals => "Thermals".to_owned(),
                ThermalHoursLayer::Wind => format!("Wind {level}"),
                ThermalHoursLayer::Gusts => format!("Gusts {level}"),
                ThermalHoursLayer::Cloud => "Cloud".to_owned(),
                ThermalHoursLayer::Rain => "Rain".to_owned(),
            },
            hours: response
                .hours
                .iter()
                .map(|hour| hour_mark(hour.time, self.utc_offset_s))
                .collect(),
            first_hour: self.first_hour,
            model: model_name(response.model.as_str()),
            run_unix_s: response.run.timestamp(),
            whole: response.whole,
            note,
            credits: response.attributions.iter().map(credit).collect(),
            above_ground_m: response.above_ground_m,
        }
    }

    /// What the map draws of one hour, by the hour's number in
    /// [`ThermalMapHoursView::hours`]. Ask when the pilot stops at an
    /// hour, and hand the two texts to the map's two sources. For an hour
    /// there is none of, two empty collections.
    pub fn shapes(&self, hour: u32) -> ThermalMapShapes {
        let mut squares = String::new();
        let mut arrows = String::new();
        if let Some(hour) = self.response.hours.get(hour as usize) {
            let grid = &self.response.grid;
            for cell in 0..grid.cells() {
                let Some(shown) = self.shown(hour, cell) else {
                    continue;
                };
                if let (Some(step), Some((south, west, north, east))) =
                    (shown.step, edges(grid, cell))
                {
                    let (south, west, north, east) =
                        (number(south), number(west), number(north), number(east));
                    // Writing to a text does not fail.
                    let _ = write!(
                        squares,
                        r#"{}{{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[[[{west},{south}],[{east},{south}],[{east},{north}],[{west},{north}],[{west},{south}]]]}},"properties":{{"cell":{cell},"step":{step}}}}}"#,
                        if squares.is_empty() { "" } else { "," },
                    );
                }
                if let (Some(mark), Some(middle)) = (&shown.wind, grid.point(cell)) {
                    let (symbol, band) = match mark.symbol {
                        WindSymbol::Arrow { band } => ("arrow", band),
                        _ => ("calm", shown.band.unwrap_or(WindBand::Light)),
                    };
                    let _ = write!(
                        arrows,
                        r#"{}{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{},{}]}},"properties":{{"cell":{cell},"symbol":"{symbol}","band":"{}","heading":{},"label":"{}"}}}}"#,
                        if arrows.is_empty() { "" } else { "," },
                        number(middle.lon()),
                        number(middle.lat()),
                        band_name(band),
                        number(mark.heading_deg),
                        mark.label,
                    );
                }
            }
        }
        let collection =
            |features: String| format!(r#"{{"type":"FeatureCollection","features":[{features}]}}"#);
        ThermalMapShapes {
            squares: collection(squares),
            arrows: collection(arrows),
        }
    }

    /// The number of the square a place lies in: what a tap on the map
    /// is turned into, also where no square is drawn. None outside the
    /// map.
    pub fn cell_at(&self, lat: f64, lon: f64) -> Option<u32> {
        cell_at(&self.response.grid, lat, lon).and_then(|cell| u32::try_from(cell).ok())
    }

    /// The forecast wind at a place in one hour, by the hour's number: the
    /// mean wind of the square the place lies in, where it comes from and
    /// the gust, as the layers of the wind and of the gusts have them
    /// (read the wind at the ground for a take-off). None outside the map,
    /// for an hour there is none of, where the model has no wind, and on
    /// the other layers, which carry none.
    pub fn wind_at(&self, lat: f64, lon: f64, hour: u32) -> Option<ThermalMapWind> {
        let cell = cell_at(&self.response.grid, lat, lon)?;
        let at = self.response.hours.get(hour as usize)?;
        let kmh = at.wind_kmh.get(cell).copied().flatten()?;
        Some(ThermalMapWind {
            kmh,
            from_deg: at.wind_from_deg.get(cell).copied().flatten(),
            gust_kmh: at.gust_kmh.get(cell).copied().flatten(),
            band: band_of_kmh(f64::from(kmh)),
        })
    }

    /// A square in one hour, in words: `cell` is the square's number
    /// ([`cell_at`](Self::cell_at), or `cell` of what the map drew) and
    /// `hour` the hour's number. None for a square or an hour there is
    /// none of.
    pub fn square(&self, cell: u32, hour: u32) -> Option<ThermalMapHourSquare> {
        let response = &self.response;
        let at = response.hours.get(hour as usize)?;
        let (south, west, north, east) = edges(&response.grid, cell as usize)?;
        let ground = response.ground_m.get(cell as usize).copied().flatten();
        let clock = at.time + Duration::seconds(i64::from(self.utc_offset_s));
        let shown = self.shown(at, cell as usize);
        let (title, detail) = match &shown {
            Some(shown) => (shown.title.clone(), shown.detail.clone()),
            None => self.nothing(ground),
        };
        Some(ThermalMapHourSquare {
            cell,
            south,
            west,
            north,
            east,
            hour: clock.format("%H:%M").to_string(),
            title,
            detail,
            step: shown.as_ref().and_then(|shown| shown.step),
            thermals: shown.as_ref().and_then(|shown| shown.thermals.clone()),
            wind: shown.and_then(|shown| shown.wind),
            ground_m: ground.map(f64::from),
        })
    }
}

/// What a square has in an hour, in the layer.
struct Shown {
    /// The step it is drawn in. None where it is not drawn.
    step: Option<u8>,
    /// The band of the speed the layer shows, on the layers of the wind.
    band: Option<WindBand>,
    title: String,
    detail: String,
    thermals: Option<ThermalMapCell>,
    wind: Option<WindMark>,
}

impl ThermalMapHours {
    /// What a square has in an hour. None where the hour holds nothing
    /// for it in this layer.
    fn shown(&self, hour: &v1::ThermalMapHour, cell: usize) -> Option<Shown> {
        let response = &self.response;
        let ground = response.ground_m.get(cell).copied().flatten();
        let at_the_ground = || match ground {
            Some(ground) => format!("10 m above the model's ground at {ground} m"),
            None => "10 m above the model's ground".to_owned(),
        };
        let plain = |step: Option<u8>, title: String, detail: String| Shown {
            step,
            band: None,
            title,
            detail,
            thermals: None,
            wind: None,
        };
        match self.layer {
            ThermalHoursLayer::Thermals => {
                let square = thermal_map::square(
                    &response.grid,
                    &Judged::of_hour(hour),
                    &response.ground_m,
                    cell,
                )?;
                let thermals = match (square.thermal_words.as_str(), square.thermal_line.as_str()) {
                    ("", _) => String::new(),
                    (words, "") => words.to_owned(),
                    (words, line) => format!("{words}: {line}"),
                };
                let detail = match (square.why.as_str(), thermals.as_str()) {
                    ("", thermals) => thermals.to_owned(),
                    (why, "") => why.to_owned(),
                    (why, thermals) => format!("{why}. {thermals}"),
                };
                Some(Shown {
                    // The kind's step; the veil where the hour is not
                    // flyable; nothing where there is no estimate.
                    step: square
                        .scale_step
                        .or((square.verdict == Flyability::NotFlyable).then_some(0)),
                    band: None,
                    title: square.status.clone(),
                    detail,
                    thermals: Some(square),
                    wind: None,
                })
            }
            ThermalHoursLayer::Wind => {
                let kmh = hour.wind_kmh.get(cell).copied().flatten()?;
                let from = hour.wind_from_deg.get(cell).copied().flatten();
                let gust = hour.gust_kmh.get(cell).copied().flatten();
                let band = band_of_kmh(f64::from(kmh));
                Some(Shown {
                    step: Some(wind_step(band)),
                    band: Some(band),
                    title: wind_in_words(kmh, from, gust),
                    detail: match (response.height_m, response.above_ground_m) {
                        (Some(height), _) => format!("At {height} m above the sea"),
                        (None, Some(height)) => {
                            match self.response.ground_m.get(cell).copied().flatten() {
                                Some(ground) => format!(
                                    "{}, over the model's ground at {ground} m",
                                    above_the_ground(height)
                                ),
                                None => above_the_ground(height),
                            }
                        }
                        (None, None) => at_the_ground(),
                    },
                    thermals: None,
                    wind: Some(mark(kmh, from, band, speeds(kmh, gust))),
                })
            }
            ThermalHoursLayer::Gusts => {
                let gust = hour.gust_kmh.get(cell).copied().flatten()?;
                let mean = hour.wind_kmh.get(cell).copied().flatten();
                let from = hour.wind_from_deg.get(cell).copied().flatten();
                let band = band_of_kmh(f64::from(gust));
                Some(Shown {
                    step: Some(wind_step(band)),
                    band: Some(band),
                    title: format!("Gusts {gust} km/h"),
                    detail: match mean {
                        Some(mean) => format!(
                            "Mean wind {}, {}",
                            lower_first(&wind_in_words(mean, from, None)),
                            at_the_ground()
                        ),
                        None => at_the_ground(),
                    },
                    thermals: None,
                    // The arrow points where the mean wind blows, in the
                    // band of the gusts.
                    wind: Some(mark(mean.unwrap_or(gust), from, band, gust.to_string())),
                })
            }
            ThermalHoursLayer::Cloud => {
                let cover = hour.cloud_cover_pct.get(cell).copied().flatten()?;
                let (step, title) = match cover {
                    cover if cover >= CLOUD_PCT[1] => (Some(2), "Overcast"),
                    cover if cover >= CLOUD_PCT[0] => (Some(1), "Cloudy"),
                    cover if cover >= 25 => (None, "Some cloud"),
                    _ => (None, "Mostly clear"),
                };
                Some(plain(
                    step,
                    title.to_owned(),
                    format!("{cover} % of the sky"),
                ))
            }
            ThermalHoursLayer::Rain => {
                let fell = hour.precipitation_mm.get(cell).copied().flatten()?;
                let (step, title) = match fell {
                    fell if fell >= RAIN_MM[2] => (Some(3), "Heavy rain"),
                    fell if fell >= RAIN_MM[1] => (Some(2), "Rain"),
                    fell if fell >= RAIN_MM[0] => (Some(1), "Light rain"),
                    _ => (None, "Dry"),
                };
                let detail = if fell > 0.0 {
                    format!("{fell:.1} mm in the hour")
                } else {
                    "No rain in the hour".to_owned()
                };
                Some(plain(step, title.to_owned(), detail))
            }
        }
    }

    /// What to say of a square that has nothing in an hour: at a height
    /// the model's ground lies above, that; else that there is nothing.
    fn nothing(&self, ground: Option<i32>) -> (String, String) {
        match (self.layer, self.response.height_m) {
            (ThermalHoursLayer::Wind, None) if self.response.above_ground_m.is_some() => (
                "No wind here".to_owned(),
                "The model gives no height of its ground here, so its levels cannot be placed."
                    .to_owned(),
            ),
            (ThermalHoursLayer::Wind, Some(height)) => (
                format!("No wind at {height} m here"),
                match ground {
                    // The server reads no wind at a height under the
                    // ground, whose height it hands out in whole metres.
                    Some(ground) if height <= ground => {
                        format!("The model's ground lies higher, at {ground} m.")
                    }
                    Some(_) => "The model has nothing to read it from in this hour.".to_owned(),
                    None => "The model gives no height of its ground here, so its levels \
                             cannot be placed."
                        .to_owned(),
                },
            ),
            _ => ("No forecast for this hour here".to_owned(), String::new()),
        }
    }
}

/// The hour to show first: today the last hour that has begun, or the
/// first while none has; on another day the hour nearest one o'clock in
/// the afternoon by the pilot's clock, when thermals are mostly at work.
/// Against an unknown clock no day is today, so it is the hour nearest one
/// o'clock in the afternoon.
fn first_hour(response: &v1::ThermalHoursResponse, utc_offset_s: i32, now_unix_s: i64) -> u32 {
    let times: Vec<DateTime<chrono::Utc>> = response.hours.iter().map(|hour| hour.time).collect();
    first_of(response.date, &times, utc_offset_s, now_unix_s)
}

/// The hour of a day's `times` to show first, by the rule of
/// [`first_hour`].
pub(crate) fn first_of(
    date: chrono::NaiveDate,
    times: &[DateTime<chrono::Utc>],
    utc_offset_s: i32,
    now_unix_s: i64,
) -> u32 {
    let offset = Duration::seconds(i64::from(utc_offset_s));
    let now = DateTime::from_timestamp(now_unix_s, 0).unwrap_or_default();
    let chosen = if thermal_map::today(utc_offset_s, now_unix_s) == date {
        times.iter().rposition(|time| *time <= now).unwrap_or(0)
    } else {
        times
            .iter()
            .enumerate()
            .min_by_key(|(_, time)| {
                let clock = **time + offset;
                (i64::from(clock.hour()) * 60 + i64::from(clock.minute()) - 13 * 60).abs()
            })
            .map_or(0, |(at, _)| at)
    };
    u32::try_from(chosen).unwrap_or(0)
}

/// An hour as a stop of the line of hours, by the pilot's clock.
pub(crate) fn hour_mark(time: DateTime<chrono::Utc>, utc_offset_s: i32) -> ThermalMapHourMark {
    let clock = time + Duration::seconds(i64::from(utc_offset_s));
    ThermalMapHourMark {
        time_unix_s: time.timestamp(),
        clock_hour: u8::try_from(clock.hour()).unwrap_or(0),
        label: clock.format("%H:%M").to_string(),
    }
}

/// A wind in words: "12 km/h from W, gusts 25", "34 km/h from NW",
/// "Calm". The gusts are named where they are stronger than the mean.
fn wind_in_words(kmh: u16, from_deg: Option<u16>, gust_kmh: Option<u16>) -> String {
    let mut words = match from_deg {
        _ if kmh < CALM_BELOW_KMH => "Calm".to_owned(),
        Some(from) => format!("{kmh} km/h from {}", compass(f64::from(from))),
        None => format!("{kmh} km/h"),
    };
    if let Some(gust) = gust_kmh.filter(|gust| *gust > kmh) {
        let _ = write!(words, ", gusts {gust}");
    }
    words
}

/// Mean wind and gusts as a map writes them: "12/25", or "12" where the
/// gusts are unknown or the same.
fn speeds(kmh: u16, gust_kmh: Option<u16>) -> String {
    match gust_kmh {
        Some(gust) if gust != kmh => format!("{kmh}/{gust}"),
        _ => kmh.to_string(),
    }
}

/// The arrow of a wind of `kmh` from `from_deg`, in a band: calm, and
/// without a direction, a wind that has none worth an arrow.
fn mark(kmh: u16, from_deg: Option<u16>, band: WindBand, label: String) -> WindMark {
    match from_deg {
        Some(from) if kmh >= CALM_BELOW_KMH => WindMark {
            symbol: WindSymbol::Arrow { band },
            heading_deg: f64::from((from + 180) % 360),
            label,
        },
        _ => WindMark {
            symbol: WindSymbol::Calm,
            heading_deg: 0.0,
            label,
        },
    }
}

/// A band as the map's shapes name it.
fn band_name(band: WindBand) -> &'static str {
    match band {
        WindBand::Light => "light",
        WindBand::Moderate => "moderate",
        WindBand::Fresh => "fresh",
        WindBand::Strong => "strong",
    }
}

/// Words with their first letter small, to go on after others.
pub(crate) fn lower_first(words: &str) -> String {
    let mut letters = words.chars();
    match letters.next() {
        Some(first) => first.to_lowercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// A number as GeoJSON has it: to five decimals of a degree, a metre on
/// the ground, without the noughts at its end.
fn number(value: f64) -> String {
    let written = format!("{value:.5}");
    let trimmed = written.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" || trimmed == "-0" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::{thermal_map, thermal_map_path};

    /// Noon on 12 July 2026 by UTC.
    const NOON: i64 = 1_783_857_600;

    /// The head of an answer for three squares in a row on 12 July 2026,
    /// with ground at 612 m, at 1240 m and, over a lake, none.
    fn answer_of(layer: &str, height_m: Option<i32>, hours: Value) -> Vec<u8> {
        let mut answer = json!({
            "source": "open-meteo", "date": "2026-07-12",
            "layer": layer, "heights_m": [1000, 1500, 2000, 2500, 3000, 4000],
            "model": "icon-d2", "run": "2026-07-12T03:00:00Z", "whole": true,
            "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 3},
            "ground_m": [612, 1240, null],
            "hours": hours,
            "attributions": [{"source": "open-meteo", "provider": "Open-Meteo",
                "licence": "CC BY 4.0", "text": "Weather data by Open-Meteo.com",
                "url": "https://open-meteo.com/"}]
        });
        if let Some(height) = height_m {
            answer["height_m"] = height.into();
        }
        serde_json::to_vec(&answer).unwrap()
    }

    fn read(layer: ThermalHoursLayer, body: Vec<u8>) -> Arc<ThermalMapHours> {
        ThermalMapHours::read(200, body, layer, 7200, NOON).unwrap()
    }

    fn thermals() -> Vec<u8> {
        answer_of(
            "thermals",
            None,
            json!([
                {"time": "2026-07-12T08:00:00Z",
                 "verdict": ["flyable", "flyable", "flyable"], "why": [null, null, null],
                 "thermals": ["weak", "weak", null], "climb_ms": [0.3, 0.0, null],
                 "top_m": [1800, null, null], "cloud": [false, false, false]},
                {"time": "2026-07-12T11:00:00Z",
                 "verdict": ["flyable", "marginal", "not_flyable"],
                 "why": [null, "gusts", "rain"],
                 "thermals": ["cross_country", "local", null], "climb_ms": [1.8, 0.7, null],
                 "top_m": [3250, 2100, null], "cloud": [true, false, false]},
                {"time": "2026-07-12T12:00:00Z",
                 "verdict": ["flyable", null, null], "why": [null, null, null],
                 "thermals": ["big", null, null], "climb_ms": [2.4, null, null],
                 "top_m": [3400, null, null], "cloud": [true, false, false]}
            ]),
        )
    }

    fn ground_wind() -> Vec<u8> {
        answer_of(
            "wind",
            None,
            json!([
                {"time": "2026-07-12T08:00:00Z",
                 "wind_kmh": [1, 9, 10], "wind_from_deg": [40, 350, 0],
                 "gust_kmh": [4, 9, 31]},
                {"time": "2026-07-12T11:00:00Z",
                 "wind_kmh": [12, 22, null], "wind_from_deg": [270, null, null],
                 "gust_kmh": [25, 41, null]}
            ]),
        )
    }

    fn wind_at_1000() -> Vec<u8> {
        answer_of(
            "wind",
            Some(1000),
            json!([
                {"time": "2026-07-12T11:00:00Z",
                 "wind_kmh": [34, null, null], "wind_from_deg": [320, null, null]}
            ]),
        )
    }

    #[test]
    fn the_wind_at_a_place_is_that_of_its_square_in_the_hour() {
        let hours = read(ThermalHoursLayer::Wind, ground_wind());
        // The second square, at eleven: 22 km/h without a direction, gusts
        // of 41.
        assert_eq!(
            hours.wind_at(47.01, 11.1, 1),
            Some(ThermalMapWind {
                kmh: 22,
                from_deg: None,
                gust_kmh: Some(41),
                band: band_of_kmh(22.0),
            })
        );
        let west = hours.wind_at(47.01, 11.0, 1).unwrap();
        assert_eq!((west.kmh, west.from_deg), (12, Some(270)));
        // No wind over the lake at eleven, nothing outside the map or the
        // day, and none on the layer of the thermals.
        assert_eq!(hours.wind_at(47.01, 11.2, 1), None);
        assert_eq!(hours.wind_at(48.0, 11.0, 1), None);
        assert_eq!(hours.wind_at(47.01, 11.0, 9), None);
        let thermals = read(ThermalHoursLayer::Thermals, thermals());
        assert_eq!(thermals.wind_at(47.01, 11.0, 1), None);
    }

    fn features(text: &str) -> Vec<Value> {
        let collection: Value = serde_json::from_str(text).unwrap();
        assert_eq!(collection["type"], "FeatureCollection");
        collection["features"].as_array().unwrap().clone()
    }

    #[test]
    fn a_days_hours_are_asked_for_by_date_layer_and_height() {
        let path = |layer, height| thermal_hours_path("2026-07-12".to_owned(), layer, height, None);
        assert_eq!(
            path(ThermalHoursLayer::Thermals, None).unwrap(),
            "/v1/thermals/hours?date=2026-07-12&layer=thermals"
        );
        assert_eq!(
            path(ThermalHoursLayer::Wind, Some(2000)).unwrap(),
            "/v1/thermals/hours?date=2026-07-12&layer=wind&height=2000"
        );
        // The wind at the ground and the gusts are one answer, and a
        // height is of no account to a layer that has none.
        let ground = path(ThermalHoursLayer::Wind, None).unwrap();
        assert_eq!(ground, "/v1/thermals/hours?date=2026-07-12&layer=wind");
        assert_eq!(path(ThermalHoursLayer::Gusts, Some(2000)).unwrap(), ground);
        assert_eq!(
            path(ThermalHoursLayer::Cloud, Some(2000)).unwrap(),
            "/v1/thermals/hours?date=2026-07-12&layer=cloud"
        );
        assert_eq!(
            path(ThermalHoursLayer::Rain, None).unwrap(),
            "/v1/thermals/hours?date=2026-07-12&layer=rain"
        );
        // 250 m above the ground, of the wind alone, and in place of no
        // height above the sea.
        let low = |layer, height| {
            thermal_hours_path("2026-07-12".to_owned(), layer, height, Some(250)).unwrap()
        };
        assert_eq!(
            low(ThermalHoursLayer::Wind, None),
            "/v1/thermals/hours?date=2026-07-12&layer=wind&above_ground=250"
        );
        assert_eq!(low(ThermalHoursLayer::Gusts, None), ground);
        assert_eq!(
            low(ThermalHoursLayer::Wind, Some(2000)),
            "/v1/thermals/hours?date=2026-07-12&layer=wind&height=2000"
        );
        // Anything that is no day of the map is not asked for.
        assert!(matches!(
            thermal_hours_path("Today".to_owned(), ThermalHoursLayer::Rain, None, None),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn the_hours_are_labelled_by_the_pilots_clock_and_one_is_shown_first() {
        let hours = read(ThermalHoursLayer::Thermals, thermals());
        let view = hours.view();
        assert_eq!(view.date, "2026-07-12");
        assert_eq!(view.layer, ThermalHoursLayer::Thermals);
        assert_eq!((view.title.as_str(), view.height_m), ("Thermals", None));
        assert_eq!(
            (view.model.as_str(), view.run_unix_s),
            ("ICON-D2", 1_783_825_200)
        );
        assert_eq!(view.credits.len(), 1);
        // Two hours east of Greenwich, 08:00 UTC is ten o'clock.
        let labels: Vec<(&str, u8)> = view
            .hours
            .iter()
            .map(|hour| (hour.label.as_str(), hour.clock_hour))
            .collect();
        assert_eq!(labels, [("10:00", 10), ("13:00", 13), ("14:00", 14)]);
        assert_eq!(view.hours[0].time_unix_s, NOON - 4 * 3600);
        // At noon UTC of the day itself the hour that has begun is the
        // last, 12:00 UTC.
        assert_eq!(view.first_hour, 2);
        assert!(view.whole);
        // The line of hours shows which hours there are: nothing to say.
        assert_eq!(view.note, "");

        let shown_at = |now: i64| {
            ThermalMapHours::read(200, thermals(), ThermalHoursLayer::Thermals, 7200, now)
                .unwrap()
                .view()
                .first_hour
        };
        // Earlier in the day: the hour that has begun, and before the
        // first the first.
        assert_eq!(shown_at(NOON - 30 * 60), 1);
        assert_eq!(shown_at(NOON - 3 * 3600), 0);
        assert_eq!(shown_at(NOON - 6 * 3600), 0);
        // The day before, the hour nearest one o'clock by the pilot's
        // clock.
        assert_eq!(shown_at(NOON - 24 * 3600), 1);

        // A day that is not whole says so.
        let text = String::from_utf8(thermals()).unwrap();
        let partly = text.replace(r#""whole":true"#, r#""whole":false"#);
        assert_ne!(partly, text);
        let view = read(ThermalHoursLayer::Thermals, partly.into_bytes()).view();
        assert!(!view.whole);
        assert_eq!(view.note, "Not every hour of this day is forecast yet.");
    }

    #[test]
    fn against_an_unknown_clock_the_early_afternoon_is_shown_first() {
        let hours = ThermalMapHours::read(200, thermals(), ThermalHoursLayer::Thermals, 7200, 0)
            .unwrap()
            .view();
        assert_eq!(hours.first_hour, 1);
        assert_eq!(hours.hours[1].label, "13:00");
    }

    #[test]
    fn an_hours_thermals_are_squares_in_the_steps_of_the_days() {
        let hours = read(ThermalHoursLayer::Thermals, thermals());

        // 11:00 UTC: a cross-country hour, a marginal one with local
        // thermals, and one that is not flyable, which is the veil.
        let shapes = hours.shapes(1);
        let squares = features(&shapes.squares);
        let drawn: Vec<(u64, u64)> = squares
            .iter()
            .map(|square| {
                let of = &square["properties"];
                (of["cell"].as_u64().unwrap(), of["step"].as_u64().unwrap())
            })
            .collect();
        assert_eq!(drawn, [(0, 5), (1, 3), (2, 0)]);
        // A square lies half a step around its point, closed, west to
        // east and south to north.
        assert_eq!(squares[0]["geometry"]["type"], "Polygon");
        assert_eq!(
            squares[0]["geometry"]["coordinates"],
            json!([[
                [10.95, 46.95],
                [11.05, 46.95],
                [11.05, 47.05],
                [10.95, 47.05],
                [10.95, 46.95]
            ]])
        );
        // No arrows on this layer.
        assert!(features(&shapes.arrows).is_empty());

        // 08:00 UTC: weak thermals twice; the third is flyable without an
        // estimate and not drawn. 12:00 UTC: one square has the hour.
        let steps = |hour: u32| -> Vec<u64> {
            features(&hours.shapes(hour).squares)
                .iter()
                .map(|square| square["properties"]["step"].as_u64().unwrap())
                .collect()
        };
        assert_eq!(steps(0), [2, 2]);
        assert_eq!(steps(2), [6]);
        // An hour there is none of draws nothing.
        assert!(features(&hours.shapes(9).squares).is_empty());

        // The steps are lines of the layer's key.
        let layers = layers();
        let key = &layers[0].legend;
        assert_eq!(layers[0].layer, ThermalHoursLayer::Thermals);
        let lines: Vec<(u8, &str, ThermalMapScale, u8)> = key
            .iter()
            .map(|line| (line.step, line.words.as_str(), line.scale, line.scale_step))
            .collect();
        assert_eq!(
            lines,
            [
                (6, "Big day", ThermalMapScale::Thermal, 6),
                (5, "Cross-country day", ThermalMapScale::Thermal, 5),
                (3, "Local thermals", ThermalMapScale::Thermal, 3),
                (2, "Weak thermals", ThermalMapScale::Thermal, 2),
                (1, "No thermals", ThermalMapScale::Thermal, 1),
                (0, "Not flyable", ThermalMapScale::Veil, 0),
            ]
        );
    }

    #[test]
    fn a_tapped_square_says_its_hours_thermals_in_words() {
        let hours = read(ThermalHoursLayer::Thermals, thermals());
        assert_eq!(hours.cell_at(47.02, 11.08), Some(1));
        assert_eq!(hours.cell_at(48.0, 11.0), None);
        assert_eq!(hours.cell_at(f64::NAN, 11.0), None);

        let square = hours.square(0, 1).unwrap();
        assert_eq!((square.cell, square.hour.as_str()), (0, "13:00"));
        assert!((square.south - 46.95).abs() < 1e-9 && (square.east - 11.05).abs() < 1e-9);
        assert_eq!(square.title, "Flyable");
        assert_eq!(
            square.detail,
            "Cross-country day: 1.8 m/s to a cloud base of 3250 m"
        );
        assert_eq!(square.step, Some(5));
        assert_eq!(square.ground_m, Some(612.0));
        let judged = square.thermals.unwrap();
        assert_eq!(judged.verdict, Flyability::Flyable);
        assert_eq!(judged.thermals, Some(crate::Thermals::CrossCountry));
        assert_eq!(judged.scale_step, Some(5));
        assert_eq!(square.wind, None);

        // What speaks against the hour comes first.
        let marginal = hours.square(1, 1).unwrap();
        assert_eq!(marginal.title, "Marginal");
        assert_eq!(
            marginal.detail,
            "Gusts. Local thermals: 0.7 m/s to 2100 m in the blue"
        );
        let wet = hours.square(2, 1).unwrap();
        assert_eq!(
            (wet.title.as_str(), wet.detail.as_str()),
            ("Not flyable", "Rain")
        );
        assert_eq!((wet.step, wet.ground_m), (Some(0), None));

        // An hour's weak thermals are its own, not the best of a day.
        let early = hours.square(0, 0).unwrap();
        assert_eq!(early.detail, "Weak thermals: 0.3 m/s to 1800 m");
        assert_eq!(
            hours.square(1, 0).unwrap().detail,
            "Weak thermals: None to climb in"
        );
        // Flyable and no estimate: the verdict and no more, and no square
        // on the map.
        let bare = hours.square(2, 0).unwrap();
        assert_eq!((bare.title.as_str(), bare.detail.as_str()), ("Flyable", ""));
        assert_eq!(bare.step, None);

        // A square without the hour says that it has none.
        let none = hours.square(1, 2).unwrap();
        assert_eq!(none.title, "No forecast for this hour here");
        assert_eq!((none.detail.as_str(), none.step), ("", None));
        assert!(none.thermals.is_none());
        // A square or an hour there is none of.
        assert!(hours.square(3, 0).is_none());
        assert!(hours.square(0, 3).is_none());
    }

    #[test]
    fn the_wind_at_the_ground_is_squares_in_its_band_and_arrows() {
        let hours = read(ThermalHoursLayer::Wind, ground_wind());
        let view = hours.view();
        assert_eq!(
            (view.title.as_str(), view.height_m),
            ("Wind at the ground", None)
        );

        let shapes = hours.shapes(0);
        // 1, 9 and 10 km/h: light, light, and moderate from 10.
        let steps: Vec<u64> = features(&shapes.squares)
            .iter()
            .map(|square| square["properties"]["step"].as_u64().unwrap())
            .collect();
        assert_eq!(steps, [2, 2, 4]);
        let arrows = features(&shapes.arrows);
        assert_eq!(arrows.len(), 3);
        assert_eq!(
            arrows[1]["geometry"],
            json!({"type": "Point", "coordinates": [11.1, 47]})
        );
        // 1 km/h is calm and has no direction worth an arrow; its gusts
        // are in its label.
        assert_eq!(
            arrows[0]["properties"],
            json!({"cell": 0, "symbol": "calm", "band": "light", "heading": 0, "label": "1/4"})
        );
        // From 350° the wind blows towards 170°; gusts that are the mean
        // are not said twice.
        assert_eq!(
            arrows[1]["properties"],
            json!({"cell": 1, "symbol": "arrow", "band": "light", "heading": 170, "label": "9"})
        );
        assert_eq!(
            arrows[2]["properties"],
            json!({"cell": 2, "symbol": "arrow", "band": "moderate", "heading": 180,
                   "label": "10/31"})
        );

        // The words of a tapped square.
        let west = hours.square(0, 1).unwrap();
        assert_eq!(west.title, "12 km/h from W, gusts 25");
        assert_eq!(west.detail, "10 m above the model's ground at 612 m");
        assert_eq!(west.step, Some(4));
        assert_eq!(
            west.wind,
            Some(WindMark {
                symbol: WindSymbol::Arrow {
                    band: WindBand::Moderate
                },
                heading_deg: 90.0,
                label: "12/25".to_owned(),
            })
        );
        assert!(west.thermals.is_none());
        assert_eq!(hours.square(0, 0).unwrap().title, "Calm, gusts 4");
        assert_eq!(hours.square(1, 0).unwrap().title, "9 km/h from N");
        // Over the lake the model names no ground.
        let lake = hours.square(2, 0).unwrap();
        assert_eq!(lake.title, "10 km/h from N, gusts 31");
        assert_eq!(lake.detail, "10 m above the model's ground");
        // A wind without a direction is a speed, drawn as a dot.
        let turning = hours.square(1, 1).unwrap();
        assert_eq!(turning.title, "22 km/h, gusts 41");
        assert_eq!(turning.step, Some(6));
        assert_eq!(turning.wind.unwrap().symbol, WindSymbol::Calm);
        // And where the hour has none, nothing is drawn.
        assert_eq!(features(&hours.shapes(1).squares).len(), 2);
        assert_eq!(
            hours.square(2, 1).unwrap().title,
            "No forecast for this hour here"
        );
    }

    #[test]
    fn the_gusts_are_the_same_answer_drawn_by_the_gusts() {
        let hours = read(ThermalHoursLayer::Gusts, ground_wind());
        assert_eq!(hours.view().title, "Gusts at the ground");

        // 25 and 41 km/h: fresh and strong.
        let shapes = hours.shapes(1);
        let steps: Vec<u64> = features(&shapes.squares)
            .iter()
            .map(|square| square["properties"]["step"].as_u64().unwrap())
            .collect();
        assert_eq!(steps, [6, 7]);
        // The arrow points where the mean wind blows, in the band of the
        // gusts, and is labelled with them.
        let arrows = features(&shapes.arrows);
        assert_eq!(
            arrows[0]["properties"],
            json!({"cell": 0, "symbol": "arrow", "band": "fresh", "heading": 90, "label": "25"})
        );
        let square = hours.square(0, 1).unwrap();
        assert_eq!(square.title, "Gusts 25 km/h");
        assert_eq!(
            square.detail,
            "Mean wind 12 km/h from W, 10 m above the model's ground at 612 m"
        );
        assert_eq!(
            hours.square(0, 0).unwrap().detail,
            "Mean wind calm, 10 m above the model's ground at 612 m"
        );

        // The wind at a height has no gusts: it is not this layer.
        assert!(matches!(
            ThermalMapHours::read(200, wind_at_1000(), ThermalHoursLayer::Gusts, 0, NOON),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn the_wind_at_a_height_says_where_the_ground_lies_above_it() {
        let hours = read(ThermalHoursLayer::Wind, wind_at_1000());
        let view = hours.view();
        assert_eq!(
            (view.title.as_str(), view.height_m),
            ("Wind at 1000 m", Some(1000))
        );

        let shapes = hours.shapes(0);
        assert_eq!(features(&shapes.squares).len(), 1);
        assert_eq!(
            features(&shapes.arrows)[0]["properties"],
            json!({"cell": 0, "symbol": "arrow", "band": "strong", "heading": 140, "label": "34"})
        );
        let over = hours.square(0, 0).unwrap();
        assert_eq!(over.title, "34 km/h from NW");
        assert_eq!(over.detail, "At 1000 m above the sea");
        assert_eq!(over.step, Some(7));

        // The second square's ground is at 1240 m.
        let under = hours.square(1, 0).unwrap();
        assert_eq!(under.title, "No wind at 1000 m here");
        assert_eq!(under.detail, "The model's ground lies higher, at 1240 m.");
        assert_eq!(
            (under.step, under.wind, under.ground_m),
            (None, None, Some(1240.0))
        );
        // Over the lake the levels cannot be placed.
        let lake = hours.square(2, 0).unwrap();
        assert_eq!(lake.title, "No wind at 1000 m here");
        assert!(
            lake.detail
                .starts_with("The model gives no height of its ground here")
        );
    }

    #[test]
    fn the_wind_250_m_above_the_ground_says_so() {
        let mut answer: Value = serde_json::from_slice(&answer_of(
            "wind",
            None,
            json!([{"time": "2026-07-12T11:00:00Z",
                    "wind_kmh": [18, 9, null], "wind_from_deg": [270, 180, null]}]),
        ))
        .unwrap();
        answer["above_ground_m"] = 250.into();
        answer["heights_above_ground_m"] = json!([250]);
        let body = serde_json::to_vec(&answer).unwrap();
        let hours = read(ThermalHoursLayer::Wind, body.clone());
        let view = hours.view();
        assert_eq!(
            (view.title.as_str(), view.height_m, view.above_ground_m),
            ("Wind 250 m above the ground", None, Some(250))
        );
        let square = hours.square(0, 0).unwrap();
        assert_eq!(square.title, "18 km/h from W");
        assert_eq!(
            square.detail,
            "250 m above the ground, over the model's ground at 612 m"
        );
        // Over the lake there is no ground to stand the height on.
        let lake = hours.square(2, 0).unwrap();
        assert_eq!(lake.title, "No wind here");
        // It has no gusts: it is not that layer.
        assert!(matches!(
            ThermalMapHours::read(200, body, ThermalHoursLayer::Gusts, 0, NOON),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn cloud_and_rain_are_squares_in_their_stand_in_steps() {
        let cloud = read(
            ThermalHoursLayer::Cloud,
            answer_of(
                "cloud",
                None,
                json!([
                    {"time": "2026-07-12T11:00:00Z", "cloud_cover_pct": [100, 50, 49]},
                    {"time": "2026-07-12T12:00:00Z", "cloud_cover_pct": [90, 24, null]}
                ]),
            ),
        );
        let drawn = |hours: &ThermalMapHours, hour: u32| -> Vec<(u64, u64)> {
            features(&hours.shapes(hour).squares)
                .iter()
                .map(|square| {
                    let of = &square["properties"];
                    (of["cell"].as_u64().unwrap(), of["step"].as_u64().unwrap())
                })
                .collect()
        };
        // Overcast from 90 %, cloudy from 50 %, and under that nothing.
        assert_eq!(drawn(&cloud, 0), [(0, 2), (1, 1)]);
        assert_eq!(drawn(&cloud, 1), [(0, 2)]);
        let words = |hours: &ThermalMapHours, cell: u32, hour: u32| {
            let square = hours.square(cell, hour).unwrap();
            (square.title, square.detail)
        };
        let says = |title: &str, detail: &str| (title.to_owned(), detail.to_owned());
        assert_eq!(words(&cloud, 0, 0), says("Overcast", "100 % of the sky"));
        assert_eq!(words(&cloud, 1, 0), says("Cloudy", "50 % of the sky"));
        assert_eq!(words(&cloud, 2, 0), says("Some cloud", "49 % of the sky"));
        assert_eq!(words(&cloud, 1, 1), says("Mostly clear", "24 % of the sky"));
        assert_eq!(
            words(&cloud, 2, 1),
            says("No forecast for this hour here", "")
        );

        let rain = read(
            ThermalHoursLayer::Rain,
            answer_of(
                "rain",
                None,
                json!([
                    {"time": "2026-07-12T11:00:00Z", "precipitation_mm": [0.0, 0.1, 0.2]},
                    {"time": "2026-07-12T12:00:00Z", "precipitation_mm": [0.9, 1.0, 4.0]}
                ]),
            ),
        );
        // Light rain from 0.2 mm, where the flight engine ends an hour.
        assert_eq!(drawn(&rain, 0), [(2, 1)]);
        assert_eq!(drawn(&rain, 1), [(0, 1), (1, 2), (2, 3)]);
        assert_eq!(words(&rain, 0, 0), says("Dry", "No rain in the hour"));
        assert_eq!(words(&rain, 1, 0), says("Dry", "0.1 mm in the hour"));
        assert_eq!(words(&rain, 2, 0), says("Light rain", "0.2 mm in the hour"));
        assert_eq!(words(&rain, 1, 1), says("Rain", "1.0 mm in the hour"));
        assert_eq!(words(&rain, 2, 1), says("Heavy rain", "4.0 mm in the hour"));
        assert!(features(&rain.shapes(1).arrows).is_empty());

        // The keys name the stand-ins: the scale's grey and the veil for
        // cloud, the scale's three teal steps for rain.
        let layers = layers();
        let key = |layer: ThermalHoursLayer| -> Vec<(u8, String, ThermalMapScale, u8)> {
            layers
                .iter()
                .find(|choice| choice.layer == layer)
                .unwrap()
                .legend
                .iter()
                .map(|line| (line.step, line.words.clone(), line.scale, line.scale_step))
                .collect()
        };
        assert_eq!(
            key(ThermalHoursLayer::Cloud),
            [
                (2, "Overcast".to_owned(), ThermalMapScale::Veil, 0),
                (1, "Cloudy".to_owned(), ThermalMapScale::Vario, 4),
            ]
        );
        assert_eq!(
            key(ThermalHoursLayer::Rain),
            [
                (3, "Heavy rain".to_owned(), ThermalMapScale::Vario, 1),
                (2, "Rain".to_owned(), ThermalMapScale::Vario, 2),
                (1, "Light rain".to_owned(), ThermalMapScale::Vario, 3),
            ]
        );
        // The amounts apart, as the radar's key has them.
        let amounts: Vec<&str> = layers
            .iter()
            .find(|choice| choice.layer == ThermalHoursLayer::Rain)
            .unwrap()
            .legend
            .iter()
            .map(|line| line.amount.as_str())
            .collect();
        assert_eq!(
            amounts,
            [
                "4 mm an hour and more",
                "1 to 4 mm an hour",
                "0.2 to 1 mm an hour"
            ]
        );
        // Each layer's note is one sentence.
        for choice in &layers {
            assert_eq!(choice.note.matches(". ").count(), 0, "{}", choice.note);
            assert!(!choice.note.contains("advice"), "{}", choice.note);
        }
    }

    #[test]
    fn the_layers_and_levels_a_shell_offers_come_with_the_map_of_the_days() {
        let layers = layers();
        let offered: Vec<(ThermalHoursLayer, &str, bool, bool)> = layers
            .iter()
            .map(|choice| {
                (
                    choice.layer,
                    choice.name.as_str(),
                    choice.has_levels,
                    choice.has_arrows,
                )
            })
            .collect();
        assert_eq!(
            offered,
            [
                (ThermalHoursLayer::Thermals, "Thermals", false, false),
                (ThermalHoursLayer::Wind, "Wind", true, true),
                (ThermalHoursLayer::Gusts, "Gusts", false, true),
                (ThermalHoursLayer::Cloud, "Cloud", false, false),
                (ThermalHoursLayer::Rain, "Rain", false, false),
            ]
        );
        // The wind and the gusts share a key: the bands of the wind in
        // the steps the wind's arrows have everywhere.
        let bands: Vec<(u8, &str, u8)> = layers[1]
            .legend
            .iter()
            .map(|line| (line.step, line.words.as_str(), line.scale_step))
            .collect();
        assert_eq!(
            bands,
            [
                (7, "30 km/h and more", 7),
                (6, "20 to 30 km/h", 6),
                (4, "10 to 20 km/h", 4),
                (2, "Under 10 km/h", 2),
            ]
        );
        assert_eq!(layers[1].legend, layers[2].legend);
        assert!(layers.iter().all(|choice| !choice.note.is_empty()));
        assert!(layers[1].note.contains("a valley's own wind is not in it"));
        // Within a layer no two lines carry the same step.
        for choice in &layers {
            let mut steps: Vec<u8> = choice.legend.iter().map(|line| line.step).collect();
            steps.sort_unstable();
            steps.dedup();
            assert_eq!(steps.len(), choice.legend.len());
        }

        let levels = levels(&[1000, 2000], &[250]);
        let named: Vec<(Option<i32>, Option<i32>, &str, &str)> = levels
            .iter()
            .map(|level| {
                (
                    level.height_m,
                    level.above_ground_m,
                    level.name.as_str(),
                    level.words.as_str(),
                )
            })
            .collect();
        assert_eq!(
            named,
            [
                (None, None, "Ground", "10 m above the model's ground"),
                (None, Some(250), "250 m AGL", "250 m above the ground"),
                (Some(1000), None, "1000 m", "1000 m above the sea"),
                (Some(2000), None, "2000 m", "2000 m above the sea"),
            ]
        );

        // The map of the days hands them out where the server names its
        // heights, and has none for a server that does not.
        let days = |heights: &str| {
            let body = format!(
                r#"{{"source": "open-meteo", "model": "icon-d2",
                    "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
                    "grid": {{"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1,
                              "columns": 1}},
                    "ground_m": [612], {heights} "days": [], "attributions": []}}"#
            );
            thermal_map(200, body.into_bytes(), 0, NOON).unwrap()
        };
        let newer = days(r#""heights_m": [1000, 1500, 2000, 2500, 3000, 4000],"#);
        assert_eq!(newer.layers, layers);
        assert_eq!(newer.levels.len(), 7);
        assert_eq!(newer.levels[3].height_m, Some(2000));
        // A server that keeps the wind 250 m above the ground names it.
        let newest = days(
            r#""heights_m": [1000, 1500, 2000, 2500, 3000, 4000],
               "heights_above_ground_m": [250],"#,
        );
        assert_eq!(newest.levels.len(), 8);
        assert_eq!(
            (newest.levels[1].above_ground_m, newest.levels[1].height_m),
            (Some(250), None)
        );
        assert_eq!(newest.levels[4].height_m, Some(2000));
        let older = days("");
        assert!(older.layers.is_empty() && older.levels.is_empty());
        assert_eq!(thermal_map_path(), "/v1/thermals");
    }

    #[test]
    fn an_answer_that_is_not_the_layer_or_does_not_fit_its_grid_is_refused() {
        let refused = |layer: ThermalHoursLayer, body: Vec<u8>| {
            matches!(
                ThermalMapHours::read(200, body, layer, 0, NOON),
                Err(CoreError::Server { .. })
            )
        };
        // Another layer's answer.
        assert!(refused(ThermalHoursLayer::Wind, thermals()));
        assert!(refused(ThermalHoursLayer::Cloud, ground_wind()));
        // A list that is shorter than the grid.
        let text = String::from_utf8(ground_wind()).unwrap();
        let short = text.replace(r#""gust_kmh":[25,41,null]"#, r#""gust_kmh":[25]"#);
        assert_ne!(short, text);
        assert!(refused(ThermalHoursLayer::Wind, short.clone().into_bytes()));
        assert!(refused(ThermalHoursLayer::Gusts, short.into_bytes()));
        // A grid without a step, and ground for fewer squares.
        let no_step = text.replace(r#""step_deg":0.1"#, r#""step_deg":0.0"#);
        assert!(refused(ThermalHoursLayer::Wind, no_step.into_bytes()));
        let no_ground = text.replace(r#""ground_m":[612,1240,null]"#, r#""ground_m":[612]"#);
        assert_ne!(no_ground, text);
        assert!(refused(ThermalHoursLayer::Wind, no_ground.into_bytes()));
        // A refusal of the server is passed on in words.
        let body = br#"{"error": {"code": "not_found", "message": "no hours"}}"#;
        assert!(refused(ThermalHoursLayer::Wind, body.to_vec()));
    }

    #[test]
    fn an_hour_of_a_region_crosses_to_the_shell_as_some_hundreds_of_kilobytes() {
        // The first region's grid, 23 rows of 79, with a wind in every
        // square: the most an hour holds.
        let cells = 23 * 79;
        let list = |value: Value| Value::Array(vec![value; cells]);
        let body = serde_json::to_vec(&json!({
            "source": "open-meteo", "date": "2026-07-12",
            "layer": "wind", "heights_m": [1000],
            "model": "icon-d2", "run": "2026-07-12T03:00:00Z", "whole": true,
            "computed_at": "2026-07-12T04:41:00Z",
            "grid": {"south": 46.3, "west": 9.4, "step_deg": 0.1, "rows": 23, "columns": 79},
            "ground_m": list(1240.into()),
            "hours": [{"time": "2026-07-12T11:00:00Z", "wind_kmh": list(22.into()),
                       "wind_from_deg": list(270.into()), "gust_kmh": list(41.into())}],
            "attributions": []
        }))
        .unwrap();
        let hours = read(ThermalHoursLayer::Wind, body);

        let shapes = hours.shapes(0);
        assert_eq!(features(&shapes.squares).len(), cells);
        assert_eq!(features(&shapes.arrows).len(), cells);
        // 172 bytes a square and 160 an arrow: 312 kB and 291 kB.
        assert!(
            (250_000..350_000).contains(&shapes.squares.len()),
            "{}",
            shapes.squares.len()
        );
        assert!(
            (230_000..320_000).contains(&shapes.arrows.len()),
            "{}",
            shapes.arrows.len()
        );
        // The last square is the grid's north-east corner, written as
        // people write it.
        let last = features(&shapes.squares).pop().unwrap();
        assert_eq!(last["geometry"]["coordinates"][0][2], json!([17.25, 48.55]));
        assert_eq!(hours.cell_at(48.5, 17.2), Some(1816));
    }

    /// The made-up day that the apps' UI tests are run against
    /// (`tools/made-up-day/serve.py`) speaks the server's wire format by
    /// hand. This reads what it writes, every layer of every day:
    ///
    /// ```text
    /// python3 tools/made-up-day/serve.py --write <folder>
    /// VENTI_MADE_UP_DAY=<folder> cargo test -p venti-core made_up -- --ignored
    /// ```
    #[test]
    #[ignore = "needs the made-up day written to the folder VENTI_MADE_UP_DAY names"]
    fn the_made_up_days_map_and_hours_are_read() {
        let folder = std::path::PathBuf::from(
            std::env::var_os("VENTI_MADE_UP_DAY").expect("VENTI_MADE_UP_DAY names a folder"),
        );
        let days = std::fs::read(folder.join("thermals.json")).unwrap();
        // The made-up map is of the day the development server's clock
        // shows, 21 November 2026, at whose noon this asks.
        let now = 1_795_262_400;
        let map = thermal_map(200, days.clone(), 3600, now).unwrap();
        assert_eq!(map.days.len(), 4);
        assert_eq!(map.layers.len(), 5);
        assert_eq!(map.levels.len(), 7);
        let strip = crate::thermal_map_strip(200, days, 3600, now, 47.7, 11.7).unwrap();
        assert_eq!(strip.len(), 4);
        assert!(strip.iter().all(|day| day.square.is_some()));

        let mut read = 0;
        for day in &map.days {
            for choice in &map.layers {
                let levels: Vec<(Option<i32>, Option<i32>)> = if choice.has_levels {
                    map.levels
                        .iter()
                        .map(|level| (level.height_m, level.above_ground_m))
                        .collect()
                } else {
                    vec![(None, None)]
                };
                for (height, above_ground) in levels {
                    let path =
                        thermal_hours_path(day.date.clone(), choice.layer, height, above_ground)
                            .unwrap();
                    // The file is named after what the path asks for.
                    let query = path.split_once('?').unwrap().1;
                    let name: Vec<&str> = query
                        .split('&')
                        .map(|pair| pair.split_once('=').unwrap().1)
                        .collect();
                    let file = folder.join(format!("hours_{}.json", name.join("_")));
                    let body = std::fs::read(&file)
                        .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
                    let hours = ThermalMapHours::read(200, body, choice.layer, 3600, now)
                        .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
                    let view = hours.view();
                    assert_eq!(view.date, day.date);
                    assert_eq!(view.whole, day.whole);
                    assert_eq!(view.hours.len(), if day.whole { 11 } else { 1 });
                    let noon = view.first_hour;
                    let shapes = hours.shapes(noon);
                    let squares = features(&shapes.squares);
                    let arrows = features(&shapes.arrows);
                    assert_eq!(arrows.is_empty(), !choice.has_arrows, "{}", file.display());
                    // Every step a square carries is a line of the key.
                    for square in &squares {
                        let step = square["properties"]["step"].as_u64().unwrap();
                        assert!(
                            choice
                                .legend
                                .iter()
                                .any(|line| u64::from(line.step) == step),
                            "{}: step {step}",
                            file.display()
                        );
                    }
                    for cell in 0..45 {
                        let square = hours.square(cell, noon).unwrap();
                        assert!(!square.title.is_empty());
                    }
                    read += 1;
                }
            }
        }
        // Four days of thermals, the wind at the ground and at six
        // heights, gusts, cloud and rain.
        assert_eq!(read, 4 * 11);
    }

    #[test]
    fn numbers_are_written_as_people_write_them() {
        assert_eq!(number(46.300_000_000_000_004), "46.3");
        assert_eq!(number(9.35), "9.35");
        assert_eq!(number(47.0), "47");
        assert_eq!(number(-0.000_001), "0");
        assert_eq!(number(-12.5), "-12.5");
        assert_eq!(number(170.0), "170");
    }
}
