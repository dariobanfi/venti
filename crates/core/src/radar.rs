//! The rain radar on the map: the last frames a Venti server holds, each
//! with its time and its age in words, the key of the steps of rain, and
//! what a radar does not see.
//!
//! A shell asks the server for the list of frames ([`radar_path`]) and
//! hands the answer here ([`radar`]); out come the layers, one for each
//! radar source and never merged, each with its frames, in the order to
//! draw them. A frame is a
//! picture the shell fetches by the frame's `path`. Its pixels are steps
//! of rain, not colours: the shell hands the picture and its own colours
//! to [`radar_frame_image`] and gets back the same picture in those
//! colours, which a map draws pinned to the layer's four corners. A
//! picture that lies under another goes to [`radar_frame_image_under`]
//! with the pictures above it, and comes back clear wherever one of them
//! sees: one word is shown at a place, that of the topmost picture that
//! sees it.
//!
//! Two sources have frames of different moments: one every five minutes,
//! seconds old, the other every quarter of an hour, 20 to 37 minutes old.
//! Which frame of which layer belongs on the map at a moment is said
//! here once ([`RadarView::moments`]), and a shell draws what it is told.
//!
//! What is said here is fixed rules and no judgement of the day: how old a
//! frame is, when the newest is too old to be the rain of now, which
//! frames belong to a moment, and what the steps stand for.

use std::io::{Read, Write};

use venti_api_types::v1::{self, RADAR_PATH};
use venti_domain::{RADAR_UNSEEN, RAIN_STEPS_MM_H};

use crate::server::answer;
use crate::thermal_hours::ThermalMapScale;
use crate::wind::credit;
use crate::{CoreError, Credit};

/// The most layers of an answer that are read: a server has one for each
/// radar source of its regions, which is one or two.
const MAX_LAYERS: usize = 8;

/// The most frames of a layer that are read: a server keeps 25.
const MAX_FRAMES: usize = 64;

/// The most pixels a picture's side may have and still be drawn. A server
/// of ours has at most 2048.
const MAX_SIDE: u32 = 4096;

/// The largest picture that is read, in bytes: a frame of the first
/// region is some five thousand.
const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;

/// After how many of its source's intervals the newest frame is too old
/// to be shown as the rain of now: three, a quarter of an hour for a
/// source that publishes every five minutes and three for one that
/// publishes every fifteen. It is the rule by which the
/// server calls the source behind, and the rule by which a layer's frame
/// is too old to be drawn at a moment of the time strip.
const STALE_AFTER_INTERVALS: i64 = 3;

/// What the steps of rain are drawn in, for want of colours of rain's
/// own, from the first step: the grey of `vario` for a trace; its three
/// teal steps for light rain, rain and heavy rain, from the palest to the
/// deepest, the ones the forecast map's rain is drawn in; and the two
/// deepest purples of `wind` for the cores of showers and thunderstorms.
/// A stand-in until the owner gives rain a scale.
const STAND_IN: [(ThermalMapScale, u8); 6] = [
    (ThermalMapScale::Vario, 4),
    (ThermalMapScale::Vario, 3),
    (ThermalMapScale::Vario, 2),
    (ThermalMapScale::Vario, 1),
    (ThermalMapScale::Wind, 6),
    (ThermalMapScale::Wind, 7),
];

/// What the core says of a source of the rain radar it knows: what its
/// layer is, in words; how a picture of it is spoken of where it lies
/// under another's and is of another moment ("Austria's analysis is of
/// 14:00"); and whether its pictures are an analysis (rain as a model
/// puts it together from radars and gauges, a quarter of an hour and
/// more old when it arrives) and not what radars measured minutes ago. A
/// source it does not know is a radar's.
const SOURCES: [(&str, &str, &str, bool); 2] = [
    (
        "dwd-radar-ry",
        "Germany's radars",
        "Germany's radar picture is of",
        false,
    ),
    (
        "geosphere-inca-rr",
        "Austria's analysis of radar and gauges",
        "Austria's analysis is of",
        true,
    ),
];

/// How a picture of a source the core does not know is spoken of where
/// it lies under another's and is of another moment.
const SOME_PICTURE_IS_OF: &str = "The radar picture under it is of";

/// What a layer of a source is called when the core does not know the
/// source.
const SOME_RADAR: &str = "Rain radar";

/// What each step of rain is called, from the first.
const STEP_NAMES: [&str; 6] = [
    "A trace",
    "Light rain",
    "Rain",
    "Heavy rain",
    "Very heavy rain",
    "Cloudburst",
];

/// One frame of a radar layer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarFrameView {
    /// The end of the interval the frame is of, seconds since 1970.
    pub time_unix_s: i64,
    /// The path to `GET` from the server for the frame's picture.
    pub path: String,
    /// The frame's time on the pilot's clock, for a stop of the time
    /// strip: "14:35".
    pub clock: String,
    /// The interval the frame is of: "14:30 to 14:35".
    pub interval: String,
    /// How old the frame is, in seconds.
    pub age_s: i64,
    /// The same in words: "10 minutes ago", "just now", "2 hours ago".
    pub age: String,
    /// What to write on the map while the frame is shown: "Rain radar,
    /// 14:35, 10 minutes ago", and for a frame of an analysis "Rain
    /// analysis, 14:15, 25 minutes ago".
    pub label: String,
}

/// The frames of one radar source.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarLayerView {
    /// The source, as the server names it: "dwd-radar-ry".
    pub source: String,
    /// What the layer is, for the line that names it: "Germany's radars,
    /// every 5 minutes", "Austria's analysis of radar and gauges, every 15
    /// minutes".
    pub what: String,
    /// Whether this layer's pictures are to show what the source did not
    /// see, as the quiet veil. True for the lowest layer that has a
    /// picture, false for every layer drawn above it: there, hand
    /// [`radar_frame_image`] a fully clear colour as `unseen`, or the veil
    /// of a source that sees little would hide the rain of the layer
    /// below. So a place is veiled only where no layer sees it.
    pub draws_unseen: bool,
    /// The western edge of every picture of the layer, degrees.
    pub west: f64,
    /// The southern edge.
    pub south: f64,
    /// The eastern edge.
    pub east: f64,
    /// The northern edge. Pin a picture to the map by these four: top
    /// left `west`, `north`; top right `east`, `north`; bottom right
    /// `east`, `south`; bottom left `west`, `south`.
    pub north: f64,
    /// The frames, the oldest first. Show the newest unless the pilot
    /// drags the strip.
    pub frames: Vec<RadarFrameView>,
    /// What to say of the layer in a line: the newest frame's label, or
    /// "No radar picture yet".
    pub line: String,
    /// Whether the newest frame is too old to be the rain of now, or
    /// there is none. Say `stale_words` on the map then, where the label
    /// stands.
    pub stale: bool,
    /// "The newest radar picture is 25 minutes old. It is not the rain
    /// of now." Empty unless `stale`. A source is judged by its own
    /// cadence: too old is more than three of its intervals.
    pub stale_words: String,
    /// The source's credit. Show it with the layer, at the map. None if
    /// the server sent none.
    pub credit: Option<Credit>,
}

/// One line of the radar's key.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarKey {
    /// The value the pixels of this step have in a frame's picture, from
    /// 1. The colours handed to [`radar_frame_image`] are in this order.
    pub step: u8,
    /// What it stands for: "Heavy rain, 4 to 10 mm an hour".
    pub words: String,
    /// The scale its stand-in colour is of.
    pub scale: ThermalMapScale,
    /// The step of that scale, from 1.
    pub scale_step: u8,
}

/// One layer's frame to draw at a moment.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarMomentFrame {
    /// Which layer: its place in [`RadarView::layers`], from 0.
    pub layer: u32,
    /// Which of that layer's frames: its place in the layer's `frames`,
    /// from 0.
    pub frame: u32,
    /// Whether this picture is to show what its source did not see, as
    /// the quiet veil: true for the lowest picture of the moment, false
    /// for each above it, which gets a fully clear colour as `unseen`
    /// from [`radar_frame_image_under`]. It is the layer's own
    /// `draws_unseen` unless the layer that draws the veil has no frame
    /// for this moment; then the lowest that has one draws it, so that a
    /// place nobody saw is never shown as dry. The veil stays where no
    /// picture of the moment sees: where one above does,
    /// [`radar_frame_image_under`] makes this picture clear.
    pub draws_unseen: bool,
}

/// One stop of the time strip: a moment, and what lies on the map at it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarMoment {
    /// The moment, seconds since 1970: the end of the interval of the
    /// frame the stop is of.
    pub time_unix_s: i64,
    /// The same on the pilot's clock, for the stop: "14:15".
    pub clock: String,
    /// What to write on the map while the moment is shown: the label of
    /// the topmost picture, and, where a picture under it is of another
    /// moment, that in words: "Rain radar, 14:15, 4 minutes ago.
    /// Austria's analysis is of 14:00."
    pub label: String,
    /// The pictures to draw, in the order to draw them, the first at the
    /// bottom: for each layer its newest frame whose interval had ended
    /// at this moment. A layer has none here if it has no such frame, or
    /// if that frame was older at this moment than three of the layer's
    /// own intervals: rain of then is not laid under the rain of a
    /// moment.
    ///
    /// To draw the moment, colour the picture of the frame at place i
    /// with [`radar_frame_image_under`], handing it as `above` the
    /// pictures of the frames after it (i + 1 to the last) as the server
    /// sent them; the topmost has none above. A picture comes back clear
    /// wherever one above it sees, rain or dry, so the pictures never add
    /// up and a place shows the word of the topmost picture that sees it.
    pub frames: Vec<RadarMomentFrame>,
}

/// The rain radar as a shell shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarView {
    /// One layer for each radar source the server has, each to be drawn
    /// as a layer of its own with its own credit, in the order to draw
    /// them: the first at the bottom, the last on top. A measured radar
    /// picture lies above an analysis.
    ///
    /// Each layer says for itself whether its newest frame is too old
    /// (`stale`, `stale_words`). A card that has one line for a caution
    /// shows that of the topmost layer that has frames.
    pub layers: Vec<RadarLayerView>,
    /// The stops of the time strip, the oldest first, and for each what
    /// to draw: one line of time for all layers. The stops are the
    /// moments of the frames of the layer that has frames most often
    /// (the radar's five minutes where there is a radar's layer; the
    /// analysis' quarters of an hour where it stands alone). With one
    /// layer they are that layer's frames, one for one. Empty while no
    /// layer has a frame.
    pub moments: Vec<RadarMoment>,
    /// The stop to show first, its place in `moments`: the newest. 0
    /// while there are none.
    pub first_moment: u32,
    /// The key, from the most rain down.
    pub key: Vec<RadarKey>,
    /// What to call the places a picture marks as not seen, for the key's
    /// last line: "Not seen by the radar". Draw them as a quiet veil,
    /// never as dry.
    pub unseen_words: String,
    /// What the layer shows and what it does not, for the foot of its
    /// card.
    pub note: String,
}

/// The path to `GET` from the server for the list of radar frames.
#[uniffi::export]
pub fn radar_path() -> String {
    RADAR_PATH.to_owned()
}

/// Reads the server's answer to [`radar_path`]. `utc_offset_s` is how far
/// the pilot's clock is ahead of UTC, for the times in words;
/// `now_unix_s` is the moment by the device's clock, against which the
/// frames' ages are counted. A device whose clock is behind the moment
/// the server answered at counts from that moment instead.
#[uniffi::export]
pub fn radar(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: i64,
) -> Result<RadarView, CoreError> {
    let response: v1::RadarResponse = answer(status, &body)?;
    // The steps are fixed rules on both sides. A server with other steps
    // would have its rain drawn in the wrong colours and words.
    #[allow(clippy::float_cmp)]
    if response.steps_mm_h != RAIN_STEPS_MM_H || response.unseen != RADAR_UNSEEN {
        return Err(CoreError::Server {
            message: "This server's rain radar has other steps of rain than this version of \
                      the app knows. The app may be older than the server."
                .to_owned(),
        });
    }
    let now = now_unix_s.max(response.time.timestamp());
    let mut layers: Vec<(bool, Told, RadarLayerView)> = response
        .layers
        .iter()
        .take(MAX_LAYERS)
        .filter(|layer| is_sound(layer))
        .map(|layer| {
            let known = SOURCES
                .iter()
                .find(|(source, ..)| *source == layer.source.as_str());
            let analysis = known.is_some_and(|(.., analysis)| *analysis);
            // How a picture of the layer is spoken of.
            let (name, picture) = if analysis {
                ("Rain analysis", "picture of the analysis")
            } else {
                ("Rain radar", "radar picture")
            };
            let frames: Vec<RadarFrameView> = layer
                .frames
                .iter()
                .rev()
                .take(MAX_FRAMES)
                .rev()
                .filter(|frame| is_a_frames_path(&frame.path))
                .map(|frame| frame_view(frame, name, utc_offset_s, now))
                .collect();
            // Each source by its own cadence: a quarter of an hour for a
            // radar of every five minutes, three for an analysis of every
            // fifteen, which is 22 to 37 minutes old when all is well.
            let cadence = i64::from(layer.cadence_seconds.clamp(60, 3600));
            let patience = STALE_AFTER_INTERVALS * cadence;
            let none_yet = if analysis {
                "No picture of the analysis yet"
            } else {
                "No radar picture yet"
            };
            let (line, stale, stale_words) = match frames.last() {
                None => (none_yet.to_owned(), true, none_yet.to_owned()),
                Some(newest) if newest.age_s > patience => (
                    newest.label.clone(),
                    true,
                    format!(
                        "The newest {picture} is {} old. It is not the rain of now.",
                        span(newest.age_s)
                    ),
                ),
                Some(newest) => (newest.label.clone(), false, String::new()),
            };
            let view = RadarLayerView {
                source: layer.source.to_string(),
                what: format!(
                    "{}, every {}",
                    known.map_or(SOME_RADAR, |(_, what, ..)| what),
                    span(cadence)
                ),
                draws_unseen: false,
                west: layer.west,
                south: layer.south,
                east: layer.east,
                north: layer.north,
                frames,
                line,
                stale,
                stale_words,
                credit: response
                    .attributions
                    .iter()
                    .find(|attribution| attribution.source == layer.source)
                    .map(credit),
            };
            let told = Told {
                cadence,
                is_of: known.map_or(SOME_PICTURE_IS_OF, |(_, _, is_of, _)| is_of),
            };
            (analysis, told, view)
        })
        .collect();
    // The order to draw them in, the first at the bottom: analyses, then
    // what radars measured, each kind in the server's order.
    layers.sort_by_key(|(analysis, ..)| !*analysis);
    let any_analysis = layers.iter().any(|(analysis, ..)| *analysis);
    let (told, mut layers): (Vec<Told>, Vec<RadarLayerView>) = layers
        .into_iter()
        .map(|(_, told, view)| (told, view))
        .unzip();
    // The veil of "not seen" is drawn once, by the lowest layer that has a
    // picture to draw it in.
    let veiled = layers
        .iter()
        .position(|layer| !layer.frames.is_empty())
        .unwrap_or(0);
    if let Some(layer) = layers.get_mut(veiled) {
        layer.draws_unseen = true;
    }

    let mut note = "What the weather service's radars saw of the rain in the minutes before each \
                    picture's time. It is no forecast, and says nothing of a shower that has not \
                    formed yet. A radar looks from afar: beyond its range, and behind mountains, \
                    it sees nothing, and such places are marked as not seen, which is not dry. In \
                    the Alps a radar that stands north of them sees little behind the first \
                    chains and nothing south of the main ridge."
        .to_owned();
    if any_analysis {
        note.push_str(
            " Austria's analysis is no radar picture: it is the first step of INCA, the nowcast \
             of GeoSphere Austria, which puts the rain of a quarter of an hour together from \
             radars and rain gauges on squares of a kilometre, and it arrives some 22 minutes \
             after that quarter of an hour. That this first step is an analysis of the rain \
             that fell, and not already a forecast, is still to verify with GeoSphere Austria.",
        );
    }
    if layers.len() > 1 {
        note.push_str(
            " Where two layers see a place, the radar's picture lies on top, because it is \
             minutes old and the analysis 22 to 37 minutes; a place is marked as not seen only \
             where no layer sees it.",
        );
    }
    let moments = moments(&layers, &told);
    // The newest, which is the last: the frames come the oldest first.
    let first_moment = (0..moments.len())
        .max_by_key(|at| moments[*at].time_unix_s)
        .map_or(0, place);
    Ok(RadarView {
        layers,
        moments,
        first_moment,
        key: key(),
        unseen_words: "Not seen by the radar".to_owned(),
        note,
    })
}

/// What the line of time needs to know of a layer beyond its view.
struct Told {
    /// How often the source publishes, in seconds.
    cadence: i64,
    /// How a picture of it is spoken of under another's: "Austria's
    /// analysis is of".
    is_of: &'static str,
}

/// A place in a list as a shell is given it. A view has at most
/// [`MAX_LAYERS`] layers of [`MAX_FRAMES`] frames each: the number fits.
#[allow(clippy::cast_possible_truncation)]
fn place(at: usize) -> u32 {
    at as u32
}

/// The stops of the time strip and what lies on the map at each.
///
/// The stops are the frames of the layer with the shortest cadence among
/// those that have frames; of two that publish as often, the upper. At a
/// stop each layer shows its newest frame that is not of a later moment,
/// unless that frame is then older than [`STALE_AFTER_INTERVALS`] of the
/// layer's own intervals.
fn moments(layers: &[RadarLayerView], told: &[Told]) -> Vec<RadarMoment> {
    let Some(stops) = (0..layers.len())
        .filter(|at| !layers[*at].frames.is_empty())
        .min_by_key(|at| (told[*at].cadence, std::cmp::Reverse(*at)))
    else {
        return Vec::new();
    };
    layers[stops]
        .frames
        .iter()
        .map(|stop| {
            let time = stop.time_unix_s;
            // For each layer, bottom first, the frame it shows.
            let shown: Vec<(usize, usize)> = layers
                .iter()
                .enumerate()
                .filter_map(|(layer, view)| {
                    let (frame, newest) = view
                        .frames
                        .iter()
                        .enumerate()
                        .filter(|(_, frame)| frame.time_unix_s <= time)
                        .max_by_key(|(_, frame)| frame.time_unix_s)?;
                    let patience = STALE_AFTER_INTERVALS * told[layer].cadence;
                    (time - newest.time_unix_s <= patience).then_some((layer, frame))
                })
                .collect();
            // The stop's own frame is among them: there is a topmost.
            let top = shown
                .last()
                .map_or(stop, |(layer, frame)| &layers[*layer].frames[*frame]);
            let mut label = top.label.clone();
            let mut another_moment = false;
            for (layer, frame) in shown.iter().rev().skip(1) {
                let under = &layers[*layer].frames[*frame];
                if under.time_unix_s != top.time_unix_s {
                    label.push_str(&format!(". {} {}", told[*layer].is_of, under.clock));
                    another_moment = true;
                }
            }
            if another_moment {
                label.push('.');
            }
            RadarMoment {
                time_unix_s: time,
                clock: stop.clock.clone(),
                label,
                frames: shown
                    .iter()
                    .enumerate()
                    .map(|(at, (layer, frame))| RadarMomentFrame {
                        layer: place(*layer),
                        frame: place(*frame),
                        draws_unseen: at == 0,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// The key of the steps, from the most rain down.
fn key() -> Vec<RadarKey> {
    (0..RAIN_STEPS_MM_H.len())
        .rev()
        .map(|at| {
            let from = RAIN_STEPS_MM_H[at];
            let amount = match RAIN_STEPS_MM_H.get(at + 1) {
                _ if at == 0 => format!("under {} mm an hour", RAIN_STEPS_MM_H[1]),
                Some(to) => format!("{from} to {to} mm an hour"),
                None => format!("{from} mm an hour and more"),
            };
            RadarKey {
                // Six steps: the number fits.
                #[allow(clippy::cast_possible_truncation)]
                step: at as u8 + 1,
                words: format!("{}, {amount}", STEP_NAMES[at]),
                scale: STAND_IN[at].0,
                scale_step: STAND_IN[at].1,
            }
        })
        .collect()
}

/// Whether a layer's edges and size are ones a picture can be pinned by.
fn is_sound(layer: &v1::RadarLayer) -> bool {
    let on_the_map = |lat: f64| lat.is_finite() && (-86.0..=86.0).contains(&lat);
    let around = |lon: f64| lon.is_finite() && (-180.0..=180.0).contains(&lon);
    (1..=MAX_SIDE).contains(&layer.columns)
        && (1..=MAX_SIDE).contains(&layer.rows)
        && on_the_map(layer.south)
        && on_the_map(layer.north)
        && around(layer.west)
        && around(layer.east)
        && layer.south < layer.north
        && layer.west < layer.east
}

/// Whether a path is one of a frame's picture on a Venti server: under
/// the radar's own path, and made of nothing that could lead elsewhere.
/// The address a shell asks is the server's own with this path behind it.
fn is_a_frames_path(path: &str) -> bool {
    path.len() <= 200
        && path
            .strip_prefix(RADAR_PATH)
            .is_some_and(|rest| rest.starts_with('/'))
        && !path.contains("..")
        && !path.contains("//")
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_'))
}

/// A moment on the pilot's clock: "14:35".
fn clock(unix_s: i64, utc_offset_s: i32) -> String {
    chrono::DateTime::from_timestamp(unix_s + i64::from(utc_offset_s), 0)
        .map_or_else(String::new, |time| time.format("%H:%M").to_string())
}

/// A length of time in words: "less than a minute", "1 minute", "25
/// minutes", "2 hours", "3 days".
fn span(seconds: i64) -> String {
    let count = |n: i64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match seconds {
        ..60 => "less than a minute".to_owned(),
        60..5400 => count(seconds / 60, "minute"),
        5400..172_800 => count((seconds + 1800) / 3600, "hour"),
        _ => count(seconds / 86_400, "day"),
    }
}

fn frame_view(
    frame: &v1::RadarFrameRef,
    name: &str,
    utc_offset_s: i32,
    now_unix_s: i64,
) -> RadarFrameView {
    let time = frame.time.timestamp();
    let age_s = (now_unix_s - time).max(0);
    let age = if age_s < 60 {
        "just now".to_owned()
    } else {
        format!("{} ago", span(age_s))
    };
    let at = clock(time, utc_offset_s);
    RadarFrameView {
        time_unix_s: time,
        path: frame.path.clone(),
        interval: format!(
            "{} to {at}",
            clock(time - i64::from(frame.interval_s), utc_offset_s)
        ),
        label: format!("{name}, {at}, {age}"),
        clock: at,
        age_s,
        age,
    }
}

/// A frame's picture in a shell's own colours.
///
/// `picture` is what the server sent for a frame's `path`: a PNG whose
/// pixels are steps. `rain` are the colours of the steps of rain, the
/// first step first, one for each line of [`RadarView::key`] read from
/// its end; `unseen` is the colour of what the radar did not see, a
/// quiet veil. Each is `0xRRGGBBAA`: red, green, blue and how solid. No
/// rain stays clear. The answer is the same picture with those colours,
/// a PNG any platform reads.
///
/// Nothing of the picture is unpacked: its palette is written anew and
/// its pixels stay as they are. It is [`radar_frame_image_under`] with no
/// picture above, which is how the topmost picture of a moment is
/// coloured.
#[uniffi::export]
pub fn radar_frame_image(
    picture: Vec<u8>,
    rain: Vec<u32>,
    unseen: u32,
) -> Result<Vec<u8>, CoreError> {
    radar_frame_image_under(picture, Vec::new(), rain, unseen)
}

/// A frame's picture in a shell's own colours, for a picture that lies
/// under others at a moment: the picture as [`radar_frame_image`] colours
/// it, but clear at every pixel that a picture above it sees.
///
/// `above` are the pictures drawn over this one at the moment, as the
/// server sent them: for the frame at place i of [`RadarMoment::frames`],
/// the pictures of the frames after it. Where any of them has a value
/// other than "not seen", rain or dry, this picture's pixel is made
/// clear, whatever it was, the veil of "not seen" too. So one word is
/// shown at every place: that of the topmost picture that sees it, dry
/// included, and a picture under it shows only where none above sees. Two
/// see-through pictures never add up to a colour the key does not have,
/// and rain of an older picture never shows where a newer one has just
/// measured none.
///
/// With `above` empty nothing is unpacked and the answer is
/// [`radar_frame_image`]'s. Otherwise the pictures are unpacked, which
/// asks more of them: each is eight bits a pixel with a palette, not
/// interlaced, at most 4096 pixels a side, with no pixel beyond "not
/// seen", and every picture above is as wide and as high as this one. The
/// pictures of one server's layers are, all being of its one grid.
/// Anything else is refused as a picture that could not be read.
#[uniffi::export]
pub fn radar_frame_image_under(
    picture: Vec<u8>,
    above: Vec<Vec<u8>>,
    rain: Vec<u32>,
    unseen: u32,
) -> Result<Vec<u8>, CoreError> {
    if rain.len() != RAIN_STEPS_MM_H.len() {
        return Err(CoreError::Server {
            message: format!(
                "The rain radar has {} steps of rain, and {} colours were given.",
                RAIN_STEPS_MM_H.len(),
                rain.len()
            ),
        });
    }
    if picture.len() > MAX_PICTURE_BYTES || !picture.starts_with(SIGNATURE) {
        return Err(unreadable());
    }
    // The pixels anew, where a picture lies above; none, and the
    // picture's own stay as they are.
    let pixels = if above.is_empty() {
        None
    } else {
        Some(pixels_under(&picture, &above).ok_or_else(unreadable)?)
    };

    // Clear, the steps, the veil: the order of a pixel's values.
    let colours: Vec<[u8; 4]> = std::iter::once(0)
        .chain(rain)
        .chain([unseen])
        .map(u32::to_be_bytes)
        .collect();
    let palette: Vec<u8> = colours.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    let solid: Vec<u8> = colours.iter().map(|c| c[3]).collect();

    let mut out = Vec::with_capacity(picture.len() + 64);
    out.extend_from_slice(SIGNATURE);
    let mut rest = &picture[SIGNATURE.len()..];
    let (mut head_seen, mut palette_seen, mut ended) = (false, false, false);
    while !ended {
        let (name, chunk) = next_chunk(&mut rest).ok_or_else(unreadable)?;
        match &name {
            b"IHDR" if !head_seen => {
                // Width and height, then the depth and the kind of
                // colour: eight bits, with a palette.
                if chunk.get(16..18) != Some(&[8, 3]) {
                    return Err(unreadable());
                }
                head_seen = true;
                out.extend_from_slice(chunk);
                write_chunk(&mut out, *b"PLTE", &palette);
                write_chunk(&mut out, *b"tRNS", &solid);
            }
            _ if !head_seen => return Err(unreadable()),
            b"PLTE" => {
                // The picture's own palette has a colour for each value
                // a pixel can have, as many as ours.
                if chunk.len() != palette.len() + 12 {
                    return Err(unreadable());
                }
                palette_seen = true;
            }
            b"tRNS" => {}
            b"IDAT" if !palette_seen => return Err(unreadable()),
            // The pixels are written anew, in one piece before the end.
            b"IDAT" if pixels.is_some() => {}
            b"IEND" => {
                if let Some(pixels) = &pixels {
                    write_chunk(&mut out, *b"IDAT", pixels);
                }
                out.extend_from_slice(chunk);
                ended = true;
            }
            _ => out.extend_from_slice(chunk),
        }
    }
    Ok(out)
}

/// What a PNG begins with.
const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// What is said of a picture that is not a frame of a server of ours.
fn unreadable() -> CoreError {
    CoreError::Server {
        message: "The radar picture could not be read.".to_owned(),
    }
}

/// Takes the next chunk of a PNG off `rest`: its name, and the whole of
/// it, which is its length, its name, its bytes and a checksum. None if
/// what is left is not a whole chunk.
fn next_chunk<'a>(rest: &mut &'a [u8]) -> Option<([u8; 4], &'a [u8])> {
    let (length, name) = match **rest {
        [a, b, c, d, n0, n1, n2, n3, ..] => (
            usize::try_from(u32::from_be_bytes([a, b, c, d])).ok()?,
            [n0, n1, n2, n3],
        ),
        _ => return None,
    };
    let whole = length.checked_add(12)?;
    let chunk = rest.get(..whole)?;
    *rest = rest.get(whole..)?;
    Some((name, chunk))
}

/// The pixels of `picture` with every one made clear that a picture of
/// `above` sees, packed as a PNG's `IDAT` holds them. None if a picture
/// cannot be unpacked or one above is of another size.
fn pixels_under(picture: &[u8], above: &[Vec<u8>]) -> Option<Vec<u8>> {
    // A moment has a picture of each layer at most.
    if above.len() >= MAX_LAYERS {
        return None;
    }
    let mut under = unpacked(picture)?;
    for over in above {
        // One picture above at a time: no more than two are ever
        // unpacked at once.
        let over = unpacked(over)?;
        if (over.width, over.height) != (under.width, under.height) {
            return None;
        }
        for (pixel, seen) in under.pixels.iter_mut().zip(&over.pixels) {
            if *seen != RADAR_UNSEEN {
                *pixel = 0;
            }
        }
    }
    packed(&under)
}

/// A picture unpacked: its pixels row by row from the top, a byte each.
struct Unpacked {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

/// Unpacks a frame's picture: eight bits a pixel with a palette, not
/// interlaced, at most [`MAX_SIDE`] a side, every pixel a value a frame
/// has. None for anything else. What is unpacked is never more than the
/// head says the picture has, and that is known before a byte is.
fn unpacked(picture: &[u8]) -> Option<Unpacked> {
    if picture.len() > MAX_PICTURE_BYTES {
        return None;
    }
    let mut rest = picture.strip_prefix(SIGNATURE)?;
    let (name, head) = next_chunk(&mut rest)?;
    // The head: width and height, then the depth, the kind of colour,
    // how it is packed, how its rows are filtered, and whether it is
    // interlaced.
    let [w0, w1, w2, w3, h0, h1, h2, h3, 8, 3, 0, 0, 0] = *head.get(8..head.len() - 4)? else {
        return None;
    };
    if &name != b"IHDR" {
        return None;
    }
    let side = |bytes: [u8; 4]| {
        let side = u32::from_be_bytes(bytes);
        if (1..=MAX_SIDE).contains(&side) {
            usize::try_from(side).ok()
        } else {
            None
        }
    };
    let (width, height) = (side([w0, w1, w2, w3])?, side([h0, h1, h2, h3])?);

    // The packed pixels may come in pieces.
    let mut deflated = Vec::new();
    loop {
        let (name, chunk) = next_chunk(&mut rest)?;
        match &name {
            b"IDAT" => deflated.extend_from_slice(chunk.get(8..chunk.len() - 4)?),
            b"IEND" => break,
            _ => {}
        }
    }

    // Each row is a byte that says how it is filtered, then its pixels.
    let stride = width + 1;
    let expected = stride * height;
    let mut rows = Vec::with_capacity(expected);
    flate2::read::ZlibDecoder::new(deflated.as_slice())
        .take(u64::try_from(expected).ok()? + 1)
        .read_to_end(&mut rows)
        .ok()?;
    if rows.len() != expected {
        return None;
    }

    let mut pixels = vec![0_u8; width * height];
    let nothing = vec![0_u8; width];
    let mut up = nothing.as_slice();
    for (row, now) in rows
        .chunks_exact(stride)
        .zip(pixels.chunks_exact_mut(width))
    {
        let (filter, row) = row.split_first()?;
        unfilter(*filter, row, up, now)?;
        up = now;
    }
    pixels
        .iter()
        .all(|pixel| *pixel <= RADAR_UNSEEN)
        .then_some(Unpacked {
            width,
            height,
            pixels,
        })
}

/// Undoes the filter of one row of a PNG of one byte a pixel: `row` as it
/// is stored, `up` the pixels of the row above (zeros above the first),
/// `now` where the row's pixels go. None for a filter a PNG does not
/// have.
fn unfilter(filter: u8, row: &[u8], up: &[u8], now: &mut [u8]) -> Option<()> {
    // The pixel to the left, and the one above that: zero before the
    // row's first.
    let (mut left, mut corner) = (0_u8, 0_u8);
    let pixels = now.iter_mut().zip(row).zip(up);
    match filter {
        // None: the pixels as they are.
        0 => now.copy_from_slice(row),
        // Sub: the difference to the pixel on the left.
        1 => {
            for ((pixel, stored), _) in pixels {
                left = stored.wrapping_add(left);
                *pixel = left;
            }
        }
        // Up: the difference to the pixel above.
        2 => {
            for ((pixel, stored), above) in pixels {
                *pixel = stored.wrapping_add(*above);
            }
        }
        // Average: the difference to the mean of the two, rounded down.
        3 => {
            for ((pixel, stored), above) in pixels {
                let mean = u8::midpoint(left, *above);
                left = stored.wrapping_add(mean);
                *pixel = left;
            }
        }
        // Paeth: the difference to whichever of the three neighbours is
        // nearest to left + above - corner.
        4 => {
            for ((pixel, stored), above) in pixels {
                left = stored.wrapping_add(paeth(left, *above, corner));
                corner = *above;
                *pixel = left;
            }
        }
        _ => return None,
    }
    Some(())
}

/// The Paeth predictor of the PNG specification: of the pixel on the
/// left, the one above and the one above on the left, the nearest to
/// left + above - corner; at a tie the left before the one above before
/// the corner.
fn paeth(left: u8, above: u8, corner: u8) -> u8 {
    let (a, b, c) = (i16::from(left), i16::from(above), i16::from(corner));
    let estimate = a + b - c;
    let (to_a, to_b, to_c) = (
        (estimate - a).abs(),
        (estimate - b).abs(),
        (estimate - c).abs(),
    );
    if to_a <= to_b && to_a <= to_c {
        left
    } else if to_b <= to_c {
        above
    } else {
        corner
    }
}

/// Packs a picture's pixels as a PNG's `IDAT` holds them: every row
/// unfiltered, as a server of ours writes it, and the whole deflated.
fn packed(picture: &Unpacked) -> Option<Vec<u8>> {
    let mut rows = Vec::with_capacity((picture.width + 1) * picture.height);
    for row in picture.pixels.chunks_exact(picture.width) {
        rows.push(0);
        rows.extend_from_slice(row);
    }
    let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    deflate.write_all(&rows).ok()?;
    deflate.finish().ok()
}

/// Appends a chunk of a PNG: its length, its name, its bytes and their
/// checksum.
fn write_chunk(out: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
    // A palette of eight colours, or the packed pixels of a picture of
    // at most `MAX_SIDE` a side: the length fits.
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&name);
    out.extend_from_slice(data);
    let checksum = crc32(&out[start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}

/// The checksum of a PNG's chunk: CRC-32 as ISO 3309 and the PNG
/// specification define it, bit by bit. It runs over a few dozen bytes
/// of palette, and over the few thousand of a picture's packed pixels
/// where they are written anew.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 21 November 2026, 11:44 UTC: four minutes after the newer frame of
    /// [`ANSWER`].
    const NOW: i64 = 1_795_261_440;

    /// The answer of a server that has Germany's radars alone: two frames
    /// recorded from DWD on 2026-11-21, by a clock at 11:44 UTC.
    const ANSWER: &str = r#"{
        "time": "2026-11-21T11:44:00Z",
        "steps_mm_h": [0.1, 0.2, 1.0, 4.0, 10.0, 30.0],
        "unseen": 7,
        "layers": [{
            "source": "dwd-radar-ry",
            "west": 9.4, "south": 46.291984, "east": 17.2, "north": 48.5,
            "columns": 624, "rows": 261, "cadence_seconds": 300,
            "frames": [
                {"time": "2026-11-21T11:35:00Z", "interval_s": 300,
                 "path": "/v1/radar/dwd-radar-ry/20261121T1135Z.png", "bytes": 4489},
                {"time": "2026-11-21T11:40:00Z", "interval_s": 300,
                 "path": "/v1/radar/dwd-radar-ry/20261121T1140Z.png", "bytes": 4511}
            ]
        }],
        "attributions": [{
            "source": "dwd-radar-ry", "provider": "Deutscher Wetterdienst",
            "licence": "CC BY 4.0",
            "text": "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben",
            "url": "https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html"
        }]
    }"#;

    fn read(now: i64) -> RadarView {
        // A pilot in Bavaria in winter: an hour ahead of UTC.
        radar(200, ANSWER.as_bytes().to_vec(), 3600, now).unwrap()
    }

    #[test]
    fn the_path_is_the_servers() {
        assert_eq!(radar_path(), "/v1/radar");
    }

    #[test]
    fn a_frame_has_its_time_and_its_age_in_words() {
        let view = read(NOW);

        assert_eq!(view.layers.len(), 1);
        let layer = &view.layers[0];
        assert_eq!(layer.source, "dwd-radar-ry");
        assert_eq!(
            (layer.west, layer.south, layer.east, layer.north),
            (9.4, 46.291_984, 17.2, 48.5)
        );
        let newest = &layer.frames[1];
        assert_eq!(newest.path, "/v1/radar/dwd-radar-ry/20261121T1140Z.png");
        assert_eq!(newest.clock, "12:40");
        assert_eq!(newest.interval, "12:35 to 12:40");
        assert_eq!((newest.age_s, newest.age.as_str()), (240, "4 minutes ago"));
        assert_eq!(newest.label, "Rain radar, 12:40, 4 minutes ago");
        assert_eq!(layer.frames[0].label, "Rain radar, 12:35, 9 minutes ago");
        assert_eq!(layer.line, "Rain radar, 12:40, 4 minutes ago");
        // One layer says what it is, and draws its own veil.
        assert_eq!(layer.what, "Germany's radars, every 5 minutes");
        assert!(layer.draws_unseen);
        assert!(!view.note.contains("analysis"));
        assert!(!layer.stale);
        assert_eq!(layer.stale_words, "");
        let credit = layer.credit.as_ref().unwrap();
        assert_eq!(
            credit.text,
            "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben"
        );
        assert_eq!(credit.licence, "CC BY 4.0");
    }

    /// The answer of a server with the first region's two sources whose
    /// radar source stopped an hour ago: the frames of [`ANSWER`] and two
    /// of the analysis of half an hour and more later, by a clock at
    /// 12:40 UTC.
    fn two_layers() -> serde_json::Value {
        let mut answer: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
        answer["time"] = serde_json::json!("2026-11-21T12:40:00Z");
        let mut analysis = answer["layers"][0].clone();
        analysis["source"] = serde_json::json!("geosphere-inca-rr");
        analysis["cadence_seconds"] = serde_json::json!(900);
        analysis["frames"] = serde_json::json!([
            {"time": "2026-11-21T12:00:00Z", "interval_s": 900,
             "path": "/v1/radar/geosphere-inca-rr/20261121T1200Z.png", "bytes": 5407},
            {"time": "2026-11-21T12:15:00Z", "interval_s": 900,
             "path": "/v1/radar/geosphere-inca-rr/20261121T1215Z.png", "bytes": 5200}
        ]);
        answer["layers"].as_array_mut().unwrap().push(analysis);
        answer["attributions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "source": "geosphere-inca-rr", "provider": "GeoSphere Austria",
                "licence": "CC BY 4.0",
                "text": "Data source: GeoSphere Austria - https://data.hub.geosphere.at",
                "url": "https://data.hub.geosphere.at"
            }));
        answer
    }

    #[test]
    fn two_layers_come_in_the_order_to_draw_them_and_one_draws_the_veil() {
        // 12:40 UTC, an hour after DWD's recording.
        let now = NOW + 56 * 60;
        let read = |answer: &serde_json::Value| {
            radar(200, answer.to_string().into_bytes(), 3600, now).unwrap()
        };
        let view = read(&two_layers());

        // The server sorts by source; the core gives the analysis first,
        // to be drawn at the bottom, and the radars' picture on top.
        let layers: Vec<(&str, &str, bool)> = view
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.source.as_str(),
                    layer.what.as_str(),
                    layer.draws_unseen,
                )
            })
            .collect();
        assert_eq!(
            layers,
            [
                (
                    "geosphere-inca-rr",
                    "Austria's analysis of radar and gauges, every 15 minutes",
                    true
                ),
                ("dwd-radar-ry", "Germany's radars, every 5 minutes", false)
            ]
        );
        // Each with its own credit and its own words.
        let (analysis, radars) = (&view.layers[0], &view.layers[1]);
        assert_eq!(
            analysis.credit.as_ref().unwrap().text,
            "Data source: GeoSphere Austria - https://data.hub.geosphere.at"
        );
        assert!(
            radars
                .credit
                .as_ref()
                .unwrap()
                .text
                .starts_with("Datenbasis: Deutscher")
        );
        assert_eq!(analysis.frames[1].interval, "13:00 to 13:15");
        assert_eq!(analysis.line, "Rain analysis, 13:15, 25 minutes ago");
        assert_eq!(radars.line, "Rain radar, 12:40, 60 minutes ago");

        // Each is judged by its own cadence. The analysis is 25 minutes
        // old and that is how it comes; the radars' picture is an hour
        // old, twelve of its intervals.
        assert!(!analysis.stale);
        assert!(radars.stale);
        assert_eq!(
            radars.stale_words,
            "The newest radar picture is 60 minutes old. It is not the rain of now."
        );
        // The analysis is too old after three quarters of an hour.
        let later = |minutes: i64| {
            radar(
                200,
                two_layers().to_string().into_bytes(),
                3600,
                now + minutes * 60,
            )
            .unwrap()
            .layers[0]
                .clone()
        };
        assert!(!later(20).stale);
        assert_eq!(
            later(21).stale_words,
            "The newest picture of the analysis is 46 minutes old. It is not the rain of now."
        );

        // The note says what the analysis is, what is still to verify,
        // and how the two lie on each other.
        assert!(view.note.contains("no forecast"));
        assert!(view.note.contains("Austria's analysis is no radar picture"));
        assert!(view.note.contains("still to verify with GeoSphere Austria"));
        assert!(view.note.contains("the radar's picture lies on top"));
        assert!(view.note.contains("not seen only where no layer sees it"));
        assert!(!analysis.what.contains("verify") && !analysis.line.contains("verify"));

        // A lowest layer without a picture cannot draw the veil: the
        // next one does.
        let mut empty = two_layers();
        empty["layers"][1]["frames"] = serde_json::json!([]);
        let view = read(&empty);
        assert_eq!(view.layers[0].line, "No picture of the analysis yet");
        let veils: Vec<bool> = view.layers.iter().map(|layer| layer.draws_unseen).collect();
        assert_eq!(veils, [false, true]);

        // A source the core does not know is a radar's, named plainly.
        let mut unknown = two_layers();
        unknown["layers"][1]["source"] = serde_json::json!("some-new-radar");
        let view = read(&unknown);
        assert_eq!(view.layers[1].what, "Rain radar, every 15 minutes");
        assert_eq!(view.layers[0].source, "dwd-radar-ry");
        assert!(view.layers[0].draws_unseen && !view.layers[1].draws_unseen);
    }

    /// 21 November 2026, 13:19 UTC: the moment the development mode's
    /// radar clock stands at.
    const RECORDED_AT: i64 = NOW + 95 * 60;

    /// The development mode's answer, as
    /// `crates/server/tests/golden/radar.json` keeps it: both sources,
    /// recorded within two minutes, by a clock at 13:19 UTC.
    fn recorded() -> serde_json::Value {
        let mut answer = two_layers();
        answer["time"] = serde_json::json!("2026-11-21T13:19:00Z");
        answer["layers"][0]["frames"] = serde_json::json!([
            {"time": "2026-11-21T13:10:00Z", "interval_s": 300,
             "path": "/v1/radar/dwd-radar-ry/20261121T1310Z.png", "bytes": 4572},
            {"time": "2026-11-21T13:15:00Z", "interval_s": 300,
             "path": "/v1/radar/dwd-radar-ry/20261121T1315Z.png", "bytes": 4695}
        ]);
        answer["layers"][1]["frames"] = serde_json::json!([
            {"time": "2026-11-21T12:45:00Z", "interval_s": 900,
             "path": "/v1/radar/geosphere-inca-rr/20261121T1245Z.png", "bytes": 5066},
            {"time": "2026-11-21T13:00:00Z", "interval_s": 900,
             "path": "/v1/radar/geosphere-inca-rr/20261121T1300Z.png", "bytes": 5342}
        ]);
        answer
    }

    fn view_of(answer: &serde_json::Value, now: i64) -> RadarView {
        radar(200, answer.to_string().into_bytes(), 3600, now).unwrap()
    }

    /// A moment's pictures as layer, frame and whether it draws the veil.
    fn drawn(moment: &RadarMoment) -> Vec<(u32, u32, bool)> {
        moment
            .frames
            .iter()
            .map(|frame| (frame.layer, frame.frame, frame.draws_unseen))
            .collect()
    }

    #[test]
    fn two_layers_have_one_line_of_time_with_the_radars_stops() {
        let view = view_of(&recorded(), RECORDED_AT);

        // The layers in draw order: the analysis, then the radars.
        assert_eq!(view.layers[0].source, "geosphere-inca-rr");
        assert_eq!(view.layers[1].source, "dwd-radar-ry");
        assert_eq!(view.layers[0].line, "Rain analysis, 14:00, 19 minutes ago");
        assert!(!view.layers[0].stale && !view.layers[1].stale);

        // The stops are the radars' five minutes, the oldest first, and
        // the card opens at the newest.
        let stops: Vec<(i64, &str)> = view
            .moments
            .iter()
            .map(|moment| (moment.time_unix_s, moment.clock.as_str()))
            .collect();
        assert_eq!(
            stops,
            [
                (RECORDED_AT - 9 * 60, "14:10"),
                (RECORDED_AT - 4 * 60, "14:15")
            ]
        );
        assert_eq!(view.first_moment, 1);

        // At each the analysis of 14:00 lies under the radars' picture
        // of the moment and draws the veil: its interval had ended, and
        // it was 10 and 15 minutes old then. The analysis of 13:45 is
        // on no stop.
        assert_eq!(drawn(&view.moments[0]), [(0, 1, true), (1, 0, false)]);
        assert_eq!(drawn(&view.moments[1]), [(0, 1, true), (1, 1, false)]);
        // The label is the top picture's, and says of the one under it
        // that it is of another moment.
        assert_eq!(
            view.moments[1].label,
            "Rain radar, 14:15, 4 minutes ago. Austria's analysis is of 14:00."
        );
        assert_eq!(
            view.moments[0].label,
            "Rain radar, 14:10, 9 minutes ago. Austria's analysis is of 14:00."
        );

        // A radar frame of the analysis' own moment says nothing more.
        let mut same = recorded();
        same["layers"][0]["frames"][0]["time"] = serde_json::json!("2026-11-21T13:00:00Z");
        let view = view_of(&same, RECORDED_AT);
        assert_eq!(view.moments[0].label, "Rain radar, 14:00, 19 minutes ago");
        assert_eq!(drawn(&view.moments[0]), [(0, 1, true), (1, 0, false)]);

        // A stop before the analysis' newer frame shows the older one.
        let mut earlier = recorded();
        earlier["layers"][0]["frames"][0]["time"] = serde_json::json!("2026-11-21T12:55:00Z");
        let view = view_of(&earlier, RECORDED_AT);
        assert_eq!(drawn(&view.moments[0]), [(0, 0, true), (1, 0, false)]);
        assert_eq!(
            view.moments[0].label,
            "Rain radar, 13:55, 24 minutes ago. Austria's analysis is of 13:45."
        );
    }

    #[test]
    fn one_layer_has_its_own_frames_as_the_stops() {
        // The radars alone: a moment for a frame, with the frame's own
        // time, clock and label.
        let view = read(NOW);
        let layer = &view.layers[0];
        assert_eq!(view.moments.len(), layer.frames.len());
        for (at, (moment, frame)) in view.moments.iter().zip(&layer.frames).enumerate() {
            assert_eq!(moment.time_unix_s, frame.time_unix_s);
            assert_eq!(moment.clock, frame.clock);
            assert_eq!(moment.label, frame.label);
            assert_eq!(drawn(moment), [(0, place(at), true)]);
        }
        assert_eq!(view.first_moment, 1);

        // The analysis alone: its quarters of an hour are the stops.
        let mut alone = recorded();
        alone["layers"].as_array_mut().unwrap().remove(0);
        let view = view_of(&alone, RECORDED_AT);
        assert_eq!(view.layers.len(), 1);
        let clocks: Vec<&str> = view.moments.iter().map(|m| m.clock.as_str()).collect();
        assert_eq!(clocks, ["13:45", "14:00"]);
        assert_eq!(view.first_moment, 1);
        assert_eq!(
            view.moments[1].label,
            "Rain analysis, 14:00, 19 minutes ago"
        );
        assert_eq!(drawn(&view.moments[0]), [(0, 0, true)]);
        assert_eq!(drawn(&view.moments[1]), [(0, 1, true)]);
    }

    #[test]
    fn a_layer_without_frames_is_on_no_stop_and_the_other_gives_the_stops() {
        // No radar picture yet: the analysis' frames are the stops, and
        // each names its layer by its place among both.
        let mut no_radar = recorded();
        no_radar["layers"][0]["frames"] = serde_json::json!([]);
        let view = view_of(&no_radar, RECORDED_AT);
        assert_eq!(view.layers[1].line, "No radar picture yet");
        let clocks: Vec<&str> = view.moments.iter().map(|m| m.clock.as_str()).collect();
        assert_eq!(clocks, ["13:45", "14:00"]);
        assert_eq!(drawn(&view.moments[1]), [(0, 1, true)]);
        assert_eq!(
            view.moments[1].label,
            "Rain analysis, 14:00, 19 minutes ago"
        );

        // No picture of the analysis yet: the radars' picture alone, and
        // it draws the veil, as its layer says.
        let mut no_analysis = recorded();
        no_analysis["layers"][1]["frames"] = serde_json::json!([]);
        let view = view_of(&no_analysis, RECORDED_AT);
        assert!(view.layers[1].draws_unseen);
        assert_eq!(drawn(&view.moments[1]), [(1, 1, true)]);
        assert_eq!(view.moments[1].label, "Rain radar, 14:15, 4 minutes ago");

        // No frame at all: no stop, and nothing to open at.
        let mut none = recorded();
        none["layers"][0]["frames"] = serde_json::json!([]);
        none["layers"][1]["frames"] = serde_json::json!([]);
        let view = view_of(&none, RECORDED_AT);
        assert!(view.moments.is_empty());
        assert_eq!(view.first_moment, 0);
    }

    #[test]
    fn an_analysis_too_old_for_a_moment_is_not_laid_under_it() {
        // The analysis stopped at 12:15 UTC. Three of its intervals
        // later is 13:00: a radar frame of 13:00 still has it under it,
        // one of 13:05 does not.
        let mut stopped = recorded();
        stopped["layers"][1]["frames"] = serde_json::json!([
            {"time": "2026-11-21T12:15:00Z", "interval_s": 900,
             "path": "/v1/radar/geosphere-inca-rr/20261121T1215Z.png", "bytes": 5200}
        ]);
        stopped["layers"][0]["frames"] = serde_json::json!([
            {"time": "2026-11-21T13:00:00Z", "interval_s": 300,
             "path": "/v1/radar/dwd-radar-ry/20261121T1300Z.png", "bytes": 4500},
            {"time": "2026-11-21T13:05:00Z", "interval_s": 300,
             "path": "/v1/radar/dwd-radar-ry/20261121T1305Z.png", "bytes": 4500}
        ]);
        let view = view_of(&stopped, RECORDED_AT);

        assert_eq!(drawn(&view.moments[0]), [(0, 0, true), (1, 0, false)]);
        assert_eq!(
            view.moments[0].label,
            "Rain radar, 14:00, 19 minutes ago. Austria's analysis is of 13:15."
        );
        // Without the analysis under it the radars' picture draws the
        // veil itself, though its layer does not: what nobody saw at
        // that moment is not shown as dry.
        assert!(view.layers[0].draws_unseen && !view.layers[1].draws_unseen);
        assert_eq!(drawn(&view.moments[1]), [(1, 1, true)]);
        assert_eq!(view.moments[1].label, "Rain radar, 14:05, 14 minutes ago");
        // The layer says for itself that it is behind.
        assert!(view.layers[0].stale);

        // A frame of a later moment than the stop is never under it: a
        // radar that stopped before the analysis' frames has them on no
        // stop of its own.
        let view = view_of(&two_layers(), NOW + 56 * 60);
        assert_eq!(drawn(&view.moments[1]), [(1, 1, true)]);
    }

    #[test]
    fn an_age_is_said_in_minutes_then_hours_then_days() {
        let age = |seconds: i64| read(NOW - 240 + seconds).layers[0].frames[1].age.clone();
        assert_eq!(age(240), "4 minutes ago");
        assert_eq!(age(600), "10 minutes ago");
        assert_eq!(age(89 * 60), "89 minutes ago");
        assert_eq!(age(2 * 3600), "2 hours ago");
        assert_eq!(age(3 * 86_400), "3 days ago");
        // A device whose clock is behind the server's counts from the
        // moment the server answered at: the frame is not "from the
        // future", and no younger than the server said.
        assert_eq!(age(-3600), "4 minutes ago");

        // An answer made in the frame's own minute.
        let mut fresh: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
        fresh["time"] = serde_json::json!("2026-11-21T11:40:20Z");
        let view = radar(200, fresh.to_string().into_bytes(), 3600, 0).unwrap();
        assert_eq!(
            view.layers[0].frames[1].label,
            "Rain radar, 12:40, just now"
        );
        assert_eq!(
            (span(60), span(119), span(120)),
            (
                "1 minute".to_owned(),
                "1 minute".to_owned(),
                "2 minutes".to_owned()
            )
        );
        assert_eq!(span(5400), "2 hours");
    }

    #[test]
    fn a_newest_frame_older_than_a_quarter_of_an_hour_is_not_the_rain_of_now() {
        // Fifteen minutes old: still within three intervals.
        let at = |minutes: i64| read(NOW - 240 + minutes * 60).layers[0].clone();
        assert!(!at(15).stale);
        let old = at(16);
        assert!(old.stale);
        assert_eq!(
            old.stale_words,
            "The newest radar picture is 16 minutes old. It is not the rain of now."
        );
        // The label still says which picture it is.
        assert_eq!(old.line, "Rain radar, 12:40, 16 minutes ago");

        // A layer without frames says so, and is not the rain of now.
        let mut empty: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
        empty["layers"][0]["frames"] = serde_json::json!([]);
        let view = radar(200, empty.to_string().into_bytes(), 3600, NOW).unwrap();
        assert!(view.layers[0].frames.is_empty());
        assert!(view.layers[0].stale);
        assert_eq!(view.layers[0].line, "No radar picture yet");
    }

    #[test]
    fn the_key_names_the_steps_from_the_most_rain_down() {
        let view = read(NOW);

        let key: Vec<(u8, &str, ThermalMapScale, u8)> = view
            .key
            .iter()
            .map(|line| (line.step, line.words.as_str(), line.scale, line.scale_step))
            .collect();
        assert_eq!(
            key,
            [
                (
                    6,
                    "Cloudburst, 30 mm an hour and more",
                    ThermalMapScale::Wind,
                    7
                ),
                (
                    5,
                    "Very heavy rain, 10 to 30 mm an hour",
                    ThermalMapScale::Wind,
                    6
                ),
                (
                    4,
                    "Heavy rain, 4 to 10 mm an hour",
                    ThermalMapScale::Vario,
                    1
                ),
                (3, "Rain, 1 to 4 mm an hour", ThermalMapScale::Vario, 2),
                (
                    2,
                    "Light rain, 0.2 to 1 mm an hour",
                    ThermalMapScale::Vario,
                    3
                ),
                (
                    1,
                    "A trace, under 0.2 mm an hour",
                    ThermalMapScale::Vario,
                    4
                ),
            ]
        );
        assert_eq!(view.unseen_words, "Not seen by the radar");
        // What a radar does not see, and what it does not say.
        assert!(view.note.contains("no forecast"));
        assert!(view.note.contains("a shower that has not formed yet"));
        assert!(view.note.contains("not seen, which is not dry"));
        assert!(view.note.contains("nothing south of the main ridge"));
    }

    #[test]
    fn an_answer_that_could_mislead_is_refused_or_left_out() {
        let changed = |change: fn(&mut serde_json::Value)| {
            let mut answer: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
            change(&mut answer);
            radar(200, answer.to_string().into_bytes(), 3600, NOW)
        };

        // Other steps than the app's words and colours are for.
        let other_steps = changed(|answer| answer["steps_mm_h"][2] = serde_json::json!(2.0));
        assert!(matches!(other_steps, Err(CoreError::Server { message })
            if message.contains("other steps of rain")));
        let other_mark = changed(|answer| answer["unseen"] = serde_json::json!(0));
        assert!(other_mark.is_err());

        // A frame whose path leads elsewhere is not a frame.
        for path in [
            "https://example.org/v1/radar/x.png",
            "/v1/radar/../account",
            "/v1/radar//example.org/x.png",
            "/v1/radarx/dwd/1.png",
            "/v1/radar/dwd/1.png?next=https://example.org",
            "/v1/flights",
        ] {
            let mut answer: serde_json::Value = serde_json::from_str(ANSWER).unwrap();
            answer["layers"][0]["frames"][1]["path"] = serde_json::json!(path);
            let view = radar(200, answer.to_string().into_bytes(), 3600, NOW).unwrap();
            assert_eq!(view.layers[0].frames.len(), 1, "{path}");
        }

        // A layer that cannot be pinned to a map is no layer.
        let upside_down = changed(|answer| answer["layers"][0]["south"] = serde_json::json!(49.0));
        assert!(upside_down.unwrap().layers.is_empty());
        let huge = changed(|answer| answer["layers"][0]["columns"] = serde_json::json!(100_000));
        assert!(huge.unwrap().layers.is_empty());

        // A server without radar says so in words.
        let none = radar(
            501,
            br#"{"error":{"code":"no_radar","message":"x"}}"#.to_vec(),
            3600,
            NOW,
        );
        assert!(matches!(none, Err(CoreError::Server { message })
            if message == "This server has no rain radar."));
    }

    /// A picture as a server writes one: two by two pixels with the
    /// values 0, 3, 7 and 6, a palette of eight greys, one byte a pixel,
    /// stored without packing.
    fn picture() -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        write_chunk(&mut png, *b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 3, 0, 0, 0]);
        write_chunk(&mut png, *b"PLTE", &[128; 24]);
        write_chunk(&mut png, *b"tRNS", &[0, 255, 255, 255, 255, 255, 255, 48]);
        // A zlib stream of one stored block: two rows, each a filter
        // byte and two pixels.
        let rows = [0u8, 0, 3, 0, 7, 6];
        let mut data = vec![0x78, 0x01, 0x01, 6, 0, !6, 0xff];
        data.extend(rows);
        let (mut a, mut b) = (1u32, 0u32);
        for byte in rows {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        data.extend(((b << 16) | a).to_be_bytes());
        write_chunk(&mut png, *b"IDAT", &data);
        write_chunk(&mut png, *b"IEND", &[]);
        png
    }

    /// The chunks of a PNG: each name with its bytes.
    fn chunks(png: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut rest = &png[8..];
        let mut found = Vec::new();
        while !rest.is_empty() {
            let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
            let name = String::from_utf8(rest[4..8].to_vec()).unwrap();
            let data = rest[8..8 + length].to_vec();
            let checksum = u32::from_be_bytes(rest[8 + length..12 + length].try_into().unwrap());
            assert_eq!(checksum, crc32(&rest[4..8 + length]), "{name}");
            found.push((name, data));
            rest = &rest[12 + length..];
        }
        found
    }

    const RAIN: [u32; 6] = [
        0xAAB4_B8FF,
        0x9AD8_D2FF,
        0x4DB8_ACFF,
        0x1F8A_80FF,
        0x7A4F_C9FF,
        0x4B25_9EFF,
    ];

    #[test]
    fn a_picture_gets_the_shells_colours_and_keeps_its_pixels() {
        let before = picture();
        let after = radar_frame_image(before.clone(), RAIN.to_vec(), 0x6B72_8040).unwrap();

        let (old, new) = (chunks(&before), chunks(&after));
        let names: Vec<&str> = new.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["IHDR", "PLTE", "tRNS", "IDAT", "IEND"]);
        // No rain is clear, the steps are the shell's, the veil is last.
        assert_eq!(new[1].1[..6], [0, 0, 0, 0xAA, 0xB4, 0xB8]);
        assert_eq!(new[1].1[21..], [0x6B, 0x72, 0x80]);
        assert_eq!(new[2].1, [0, 255, 255, 255, 255, 255, 255, 0x40]);
        // The head and the pixels are byte for byte what they were.
        assert_eq!(new[0], old[0]);
        assert_eq!(new[3], old[3]);
    }

    #[test]
    fn the_checksum_is_the_one_of_the_png_specification() {
        // The checksum every PNG's last chunk has, and the standard's
        // own check value.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn what_is_not_such_a_picture_is_refused_in_words() {
        let refused =
            |picture: Vec<u8>, rain: &[u32]| match radar_frame_image(picture, rain.to_vec(), 0) {
                Err(CoreError::Server { message }) => message,
                other => panic!("{other:?}"),
            };
        let unreadable = "The radar picture could not be read.";
        assert_eq!(refused(b"<html>".to_vec(), &RAIN), unreadable);
        assert_eq!(refused(Vec::new(), &RAIN), unreadable);
        // Cut off in the middle of its pixels.
        let whole = picture();
        assert_eq!(
            refused(whole[..whole.len() - 20].to_vec(), &RAIN),
            unreadable
        );
        // A photograph: eight bits of red, green and blue, no palette.
        let mut photograph = picture();
        photograph[8 + 8 + 9] = 2;
        assert_eq!(refused(photograph, &RAIN), unreadable);
        // A palette of another size than the steps have.
        let mut other = b"\x89PNG\r\n\x1a\n".to_vec();
        write_chunk(
            &mut other,
            *b"IHDR",
            &[0, 0, 0, 2, 0, 0, 0, 2, 8, 3, 0, 0, 0],
        );
        write_chunk(&mut other, *b"PLTE", &[128; 9]);
        write_chunk(&mut other, *b"IEND", &[]);
        assert_eq!(refused(other, &RAIN), unreadable);
        // Too few colours for the steps.
        assert_eq!(
            refused(picture(), &RAIN[..3]),
            "The rain radar has 6 steps of rain, and 3 colours were given."
        );
    }

    /// A picture as a server writes one, of any size: `pixels` row by
    /// row, a palette of eight greys, each row stored with the filter
    /// its place in `filters` names (the last for the rows beyond), the
    /// whole deflated.
    fn picture_of(width: u32, pixels: &[u8], filters: &[u8]) -> Vec<u8> {
        let wide = width as usize;
        let height = u32::try_from(pixels.len() / wide).unwrap();
        let mut head = [width.to_be_bytes(), height.to_be_bytes()].concat();
        head.extend([8, 3, 0, 0, 0]);
        let mut rows = Vec::new();
        let mut up = vec![0_u8; wide];
        for (at, row) in pixels.chunks(wide).enumerate() {
            let filter = filters[at.min(filters.len() - 1)];
            rows.push(filter);
            rows.extend(filtered(filter, row, &up));
            up = row.to_vec();
        }
        let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        deflate.write_all(&rows).unwrap();

        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        write_chunk(&mut png, *b"IHDR", &head);
        write_chunk(&mut png, *b"PLTE", &[128; 24]);
        write_chunk(&mut png, *b"tRNS", &[0, 255, 255, 255, 255, 255, 255, 48]);
        write_chunk(&mut png, *b"IDAT", &deflate.finish().unwrap());
        write_chunk(&mut png, *b"IEND", &[]);
        png
    }

    /// A row as a PNG stores it under a filter, by the specification's
    /// words: each pixel less what the filter guesses from the pixel on
    /// its left, the one above and the one above on the left.
    fn filtered(filter: u8, row: &[u8], up: &[u8]) -> Vec<u8> {
        (0..row.len())
            .map(|at| {
                let (a, c) = if at == 0 {
                    (0, 0)
                } else {
                    (i32::from(row[at - 1]), i32::from(up[at - 1]))
                };
                let b = i32::from(up[at]);
                let guess = match filter {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => (a + b) / 2,
                    4 => {
                        let p = a + b - c;
                        let nearest = [a, b, c]
                            .into_iter()
                            .min_by_key(|neighbour| (p - neighbour).abs());
                        nearest.unwrap()
                    }
                    other => panic!("{other}"),
                };
                u8::try_from((i32::from(row[at]) - guess).rem_euclid(256)).unwrap()
            })
            .collect()
    }

    /// What a picture the core wrote holds: its width, its height and
    /// its pixels row by row. It is eight bits a pixel with a palette,
    /// every checksum is right, and every row is stored unfiltered.
    fn pixels_of(png: &[u8]) -> (u32, u32, Vec<u8>) {
        assert_eq!(png[..8], *b"\x89PNG\r\n\x1a\n");
        let found = chunks(png);
        let head = &found[0].1;
        assert_eq!(found[0].0, "IHDR");
        assert_eq!(head[8..], [8, 3, 0, 0, 0]);
        assert_eq!(found.last().unwrap().0, "IEND");
        let width = u32::from_be_bytes(head[..4].try_into().unwrap());
        let height = u32::from_be_bytes(head[4..8].try_into().unwrap());
        let packed: Vec<u8> = found
            .iter()
            .filter(|(name, _)| name == "IDAT")
            .flat_map(|(_, data)| data.clone())
            .collect();
        let mut rows = Vec::new();
        // Read to its end, the stream's own checksum is checked too.
        flate2::read::ZlibDecoder::new(packed.as_slice())
            .read_to_end(&mut rows)
            .unwrap();
        let stride = width as usize + 1;
        assert_eq!(rows.len(), stride * height as usize);
        let pixels = rows
            .chunks(stride)
            .flat_map(|row| {
                assert_eq!(row[0], 0, "a row is stored unfiltered");
                row[1..].to_vec()
            })
            .collect();
        (width, height, pixels)
    }

    const VEIL: u32 = 0x6B72_8040;

    /// The lower picture of two, four pixels wide and three high: rain
    /// of every step, dry, and a corner it does not see.
    const LOWER: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 3, 3, 0, 7];

    /// The upper picture: it sees the left half and nothing of the right,
    /// and has rain at two places of what it sees.
    const UPPER: [u8; 12] = [0, 0, 7, 7, 5, 0, 7, 7, 0, 2, 7, 7];

    fn under(picture: Vec<u8>, above: &[Vec<u8>]) -> Result<Vec<u8>, String> {
        match radar_frame_image_under(picture, above.to_vec(), RAIN.to_vec(), VEIL) {
            Ok(png) => Ok(png),
            Err(CoreError::Server { message }) => Err(message),
            Err(other) => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_picture_under_another_is_clear_wherever_the_other_sees() {
        let lower = picture_of(4, &LOWER, &[0]);
        let upper = picture_of(4, &UPPER, &[0]);
        let drawn = under(lower.clone(), &[upper]).unwrap();
        let (width, height, pixels) = pixels_of(&drawn);
        assert_eq!((width, height), (4, 3));

        // The upper sees the place as dry: the lower's rain is not shown
        // there (a trace, heavy rain).
        assert_eq!((UPPER[1], LOWER[1], pixels[1]), (0, 1, 0));
        assert_eq!((UPPER[5], LOWER[5], pixels[5]), (0, 5, 0));
        // The upper sees rain: the lower is clear, and the two do not
        // add up.
        assert_eq!((UPPER[4], LOWER[4], pixels[4]), (5, 4, 0));
        assert_eq!((UPPER[9], LOWER[9], pixels[9]), (2, 3, 0));
        // The upper does not see the place: the lower keeps its word,
        // rain, dry, or its own veil.
        assert_eq!((UPPER[2], pixels[2]), (7, 2));
        assert_eq!((UPPER[6], pixels[6]), (7, 6));
        assert_eq!((UPPER[10], pixels[10]), (7, 0));
        assert_eq!((UPPER[11], pixels[11]), (7, 7));
        // And so for every pixel.
        assert_eq!(pixels, [0, 0, 2, 3, 0, 0, 6, 7, 0, 0, 0, 7]);
        let seen = UPPER.iter().filter(|value| **value != 7).count();
        let kept = (0..12).filter(|at| pixels[*at] == LOWER[*at]).count();
        // One of the six the upper sees was dry in the lower already.
        assert_eq!((seen, kept), (6, 7));

        // The head, the palette and how solid each colour is are those
        // of the picture coloured alone.
        let alone = radar_frame_image(lower, RAIN.to_vec(), VEIL).unwrap();
        let (alone, drawn) = (chunks(&alone), chunks(&drawn));
        let names: Vec<&str> = drawn.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["IHDR", "PLTE", "tRNS", "IDAT", "IEND"]);
        assert_eq!(drawn[..3], alone[..3]);
        assert_eq!(drawn[1].1[3..6], [0xAA, 0xB4, 0xB8]);
        assert_eq!(drawn[2].1, [0, 255, 255, 255, 255, 255, 255, 0x40]);
    }

    #[test]
    fn with_nothing_above_it_is_the_picture_coloured_alone() {
        for picture in [picture(), picture_of(4, &LOWER, &[0, 1, 4])] {
            let alone = radar_frame_image(picture.clone(), RAIN.to_vec(), VEIL).unwrap();
            assert_eq!(under(picture, &[]).unwrap(), alone);
        }
        // And under a picture that sees nothing, its pixels are what
        // they were.
        let blind = picture_of(4, &[7; 12], &[0]);
        let drawn = under(picture_of(4, &LOWER, &[0]), &[blind]).unwrap();
        assert_eq!(pixels_of(&drawn).2, LOWER);
    }

    #[test]
    fn under_two_pictures_it_is_clear_where_either_sees() {
        let middle: [u8; 12] = [7, 7, 7, 0, 7, 7, 7, 7, 7, 7, 3, 7];
        let drawn = under(
            picture_of(4, &LOWER, &[0]),
            &[picture_of(4, &middle, &[0]), picture_of(4, &UPPER, &[0])],
        )
        .unwrap();
        assert_eq!(pixels_of(&drawn).2, [0, 0, 2, 0, 0, 0, 6, 7, 0, 0, 0, 7]);
        // The middle one, coloured for its own place: under the upper
        // alone.
        let drawn = under(picture_of(4, &middle, &[0]), &[picture_of(4, &UPPER, &[0])]).unwrap();
        assert_eq!(pixels_of(&drawn).2, [0, 0, 7, 0, 0, 0, 7, 7, 0, 0, 3, 7]);
    }

    #[test]
    fn every_filter_of_a_png_is_undone() {
        // A row and the row above it with bytes of every kind, stored
        // under each filter by another hand than this crate's.
        let up = [2, 0, 255, 129, 255, 0, 200, 1, 1, 0, 127, 31];
        let row = [0, 0, 128, 2, 60, 255, 1, 129, 255, 32, 30, 254];
        let stored: [[u8; 12]; 5] = [
            [0, 0, 128, 2, 60, 255, 1, 129, 255, 32, 30, 254],
            [0, 0, 128, 130, 58, 195, 2, 128, 126, 33, 254, 224],
            [254, 0, 129, 129, 61, 255, 57, 128, 254, 32, 159, 223],
            [255, 0, 1, 130, 188, 225, 30, 128, 190, 161, 207, 224],
            [254, 0, 129, 130, 187, 255, 2, 128, 126, 33, 159, 224],
        ];
        for (filter, stored) in (0..).zip(stored) {
            assert_eq!(
                filtered(filter, &row, &up),
                stored,
                "the test's own {filter}"
            );
            let mut now = [0; 12];
            unfilter(filter, &stored, &up, &mut now).unwrap();
            assert_eq!(now, row, "filter {filter}");
        }
        assert!(unfilter(5, &row, &up, &mut [0; 12]).is_none());

        // Whole pictures: whichever filter the rows are stored with, one
        // for all or another for each, the answer is the same picture.
        let plain = under(picture_of(4, &LOWER, &[0]), &[picture_of(4, &UPPER, &[0])]).unwrap();
        for filters in [
            &[1][..],
            &[2],
            &[3],
            &[4],
            &[4, 3, 2],
            &[1, 0, 4],
            &[2, 4, 1],
        ] {
            let lower = picture_of(4, &LOWER, filters);
            let upper = picture_of(4, &UPPER, filters);
            assert_eq!(unpacked(&lower).unwrap().pixels, LOWER, "{filters:?}");
            assert_eq!(under(lower, &[upper]).unwrap(), plain, "{filters:?}");
        }
        // A filter that no PNG has.
        assert!(under(picture_of(4, &LOWER, &[0]), &[picture_of(4, &UPPER, &[0])]).is_ok());
        let mut rows = vec![5, 0, 0, 0, 0];
        rows.extend([0; 10]);
        assert!(unpacked(&with_rows(4, 3, &rows)).is_none());
    }

    /// A picture whose head says `width` by `height` and whose packed
    /// pixels unpack to `rows`, whatever they are.
    fn with_rows(width: u32, height: u32, rows: &[u8]) -> Vec<u8> {
        let mut head = [width.to_be_bytes(), height.to_be_bytes()].concat();
        head.extend([8, 3, 0, 0, 0]);
        let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        deflate.write_all(rows).unwrap();
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        write_chunk(&mut png, *b"IHDR", &head);
        write_chunk(&mut png, *b"PLTE", &[128; 24]);
        write_chunk(&mut png, *b"IDAT", &deflate.finish().unwrap());
        write_chunk(&mut png, *b"IEND", &[]);
        png
    }

    #[test]
    fn what_cannot_be_unpacked_is_refused_in_words() {
        let unreadable = Err("The radar picture could not be read.".to_owned());
        let lower = || picture_of(4, &LOWER, &[0]);
        let upper = || picture_of(4, &UPPER, &[0]);
        assert!(under(lower(), &[upper()]).is_ok());

        // What lies above is no picture, or is cut off.
        assert_eq!(under(lower(), &[Vec::new()]), unreadable);
        assert_eq!(under(lower(), &[b"<html>".to_vec()]), unreadable);
        let cut = upper();
        assert_eq!(
            under(lower(), &[cut[..cut.len() - 20].to_vec()]),
            unreadable
        );
        assert_eq!(
            under(cut[..cut.len() - 20].to_vec(), &[upper()]),
            unreadable
        );
        // A photograph above: eight bits of red, green and blue.
        let mut photograph = upper();
        photograph[8 + 8 + 9] = 2;
        assert_eq!(under(lower(), &[photograph]), unreadable);
        // Sixteen bits, or interlaced, which is another order of pixels.
        for (at, value) in [(8, 16), (12, 1)] {
            let mut other = upper();
            other[8 + 8 + at] = value;
            assert_eq!(under(lower(), &[other.clone()]), unreadable);
            assert_eq!(under(other, &[upper()]), unreadable);
        }
        // A picture above of another size: as many pixels, three wide.
        assert_eq!(under(lower(), &[picture_of(3, &UPPER, &[0])]), unreadable);
        assert_eq!(
            under(lower(), &[picture_of(4, &UPPER[..8], &[0])]),
            unreadable
        );
        // One of the pictures above is enough.
        assert_eq!(
            under(lower(), &[upper(), picture_of(3, &UPPER, &[0])]),
            unreadable
        );
        // More pictures above than a moment can have.
        assert!(under(lower(), &vec![upper(); MAX_LAYERS - 1]).is_ok());
        assert_eq!(under(lower(), &vec![upper(); MAX_LAYERS]), unreadable);

        // A head that promises more than a picture may have: nothing is
        // unpacked for it. And one within the bound whose pixels are not
        // there.
        assert_eq!(
            under(with_rows(MAX_SIDE + 1, 1, &[0; 16]), &[upper()]),
            unreadable
        );
        assert_eq!(
            under(with_rows(u32::MAX, u32::MAX, &[0; 16]), &[upper()]),
            unreadable
        );
        assert_eq!(under(with_rows(0, 3, &[0; 3]), &[upper()]), unreadable);
        assert_eq!(
            under(with_rows(MAX_SIDE, MAX_SIDE, &[0; 16]), &[upper()]),
            unreadable
        );
        // More pixels than the head says, and fewer.
        assert!(under(with_rows(4, 3, &[0; 15]), &[upper()]).is_ok());
        assert_eq!(under(with_rows(4, 3, &[0; 16]), &[upper()]), unreadable);
        assert_eq!(under(with_rows(4, 3, &[0; 14]), &[upper()]), unreadable);
        assert_eq!(under(lower(), &[with_rows(4, 3, &[0; 4096])]), unreadable);
        // A pixel that is none of the values a frame has, below or above.
        let mut odd = LOWER;
        odd[3] = 8;
        assert_eq!(under(picture_of(4, &odd, &[0]), &[upper()]), unreadable);
        assert_eq!(under(lower(), &[picture_of(4, &odd, &[0])]), unreadable);
        // Pixels that are not deflated at all.
        let mut stored = b"\x89PNG\r\n\x1a\n".to_vec();
        write_chunk(
            &mut stored,
            *b"IHDR",
            &[0, 0, 0, 4, 0, 0, 0, 3, 8, 3, 0, 0, 0],
        );
        write_chunk(&mut stored, *b"PLTE", &[128; 24]);
        write_chunk(&mut stored, *b"IDAT", &[0; 15]);
        write_chunk(&mut stored, *b"IEND", &[]);
        assert_eq!(under(lower(), &[stored.clone()]), unreadable);
        assert_eq!(under(stored, &[upper()]), unreadable);
        // Too few colours for the steps.
        assert!(matches!(
            radar_frame_image_under(lower(), vec![upper()], RAIN[..3].to_vec(), 0),
            Err(CoreError::Server { message })
                if message == "The rain radar has 6 steps of rain, and 3 colours were given."
        ));
    }

    /// Numbers that look like chance and are the same on every run.
    struct Dice(u64);

    impl Dice {
        fn roll(&mut self, below: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            usize::try_from(self.0 % below as u64).unwrap()
        }
    }

    #[test]
    fn a_damaged_picture_is_refused_or_read_and_never_a_panic() {
        // Whatever comes out of a damaged picture is a picture of the
        // size its head says, with a frame's values and nothing else.
        // (A chunk that is copied keeps its checksum, right or wrong, so
        // this reads as the core does and not as `pixels_of`.)
        let sound = |answer: Result<Vec<u8>, String>| {
            if let Ok(png) = answer {
                let again = unpacked(&png).expect("what the core wrote, it reads");
                assert_eq!(again.pixels.len(), again.width * again.height);
            }
        };
        let lower = picture_of(4, &LOWER, &[0, 4, 3]);
        let upper = picture_of(4, &UPPER, &[1, 2, 0]);
        let try_both = |damaged: Vec<u8>| {
            sound(under(damaged.clone(), std::slice::from_ref(&upper)));
            sound(under(lower.clone(), std::slice::from_ref(&damaged)));
            sound(under(damaged.clone(), &[damaged]));
        };

        for whole in [&lower, &upper] {
            // Cut off after every byte.
            for length in 0..whole.len() {
                try_both(whole[..length].to_vec());
            }
            // Every byte with every bit turned, and set to the values
            // that lengths and sizes go wrong at.
            for at in 0..whole.len() {
                for value in (0..8)
                    .map(|bit| whole[at] ^ (1 << bit))
                    .chain([0, 0x7F, 0x80, 0xFF])
                {
                    let mut damaged = whole.clone();
                    damaged[at] = value;
                    try_both(damaged);
                }
            }
        }

        // And by chance: runs of bytes overwritten, taken out, put in
        // and doubled, several at a time.
        let mut dice = Dice(0x9E37_79B9_7F4A_7C15);
        for _ in 0..20_000 {
            let mut damaged = if dice.roll(2) == 0 {
                lower.clone()
            } else {
                upper.clone()
            };
            for _ in 0..=dice.roll(3) {
                let at = dice.roll(damaged.len().max(1));
                let run = dice.roll(9).min(damaged.len() - at);
                match dice.roll(4) {
                    0 => {
                        for byte in &mut damaged[at..at + run] {
                            *byte = u8::try_from(dice.roll(256)).unwrap();
                        }
                    }
                    1 => drop(damaged.drain(at..at + run)),
                    2 => {
                        let put: Vec<u8> = (0..run)
                            .map(|_| u8::try_from(dice.roll(256)).unwrap())
                            .collect();
                        damaged.splice(at..at, put);
                    }
                    _ => {
                        let again = damaged[at..at + run].to_vec();
                        damaged.splice(at..at, again);
                    }
                }
            }
            try_both(damaged);
        }

        // Packed pixels of chance, behind a sound head.
        for _ in 0..2_000 {
            let rows: Vec<u8> = (0..dice.roll(40))
                .map(|_| u8::try_from(dice.roll(9)).unwrap())
                .collect();
            try_both(with_rows(4, 3, &rows));
            let mut raw = b"\x89PNG\r\n\x1a\n".to_vec();
            write_chunk(&mut raw, *b"IHDR", &[0, 0, 0, 4, 0, 0, 0, 3, 8, 3, 0, 0, 0]);
            write_chunk(&mut raw, *b"PLTE", &[128; 24]);
            write_chunk(&mut raw, *b"IDAT", &rows);
            write_chunk(&mut raw, *b"IEND", &[]);
            try_both(raw);
        }
    }

    /// Two pictures of the first region's size, 624 by 261, as the two
    /// sources' are: an analysis that sees every place, with bands of
    /// rain, and above it a radar that sees a disc and has showers in
    /// it.
    fn a_regions_two_pictures() -> (Vec<u8>, Vec<u8>) {
        let (width, height) = (624_i64, 261_i64);
        let (mut lower, mut upper) = (Vec::new(), Vec::new());
        for y in 0..height {
            for x in 0..width {
                // Bands with ragged edges.
                let band = (x + 2 * y + (x / 3 * 31 + y / 3 * 17) % 23) % 200;
                lower.push(u8::try_from(if band < 60 { band / 10 } else { 0 }).unwrap());
                let (dx, dy) = (x - 250, y - 130);
                // Showers of uneven strength, cell by cell.
                let shower = (x / 4 * 7919 + y / 4 * 104_729) % 97 % 7;
                upper.push(if dx * dx + dy * dy > 220 * 220 {
                    7
                } else if (x / 16 + y / 8) % 5 == 0 {
                    u8::try_from(shower).unwrap()
                } else {
                    0
                });
            }
        }
        (lower, upper)
    }

    #[test]
    fn a_regions_picture_under_another_adds_up_and_takes_milliseconds() {
        let (lower, upper) = a_regions_two_pictures();
        let (below, above) = (picture_of(624, &lower, &[0]), picture_of(624, &upper, &[0]));

        let began = std::time::Instant::now();
        let runs = 20;
        let mut drawn = Vec::new();
        for _ in 0..runs {
            drawn = under(below.clone(), std::slice::from_ref(&above)).unwrap();
        }
        let each = began.elapsed() / runs;
        println!(
            "radar_frame_image_under, 624 x 261 under one picture: {each:?} a call \
             ({} and {} bytes in, {} out)",
            below.len(),
            above.len(),
            drawn.len()
        );

        let (width, height, pixels) = pixels_of(&drawn);
        assert_eq!((width, height), (624, 261));
        assert_eq!(pixels.len(), 624 * 261);
        let count = |of: &dyn Fn(usize) -> bool| (0..pixels.len()).filter(|at| of(*at)).count();
        let seen = count(&|at| upper[at] != 7);
        let unseen = count(&|at| upper[at] == 7);
        // Clear wherever the upper sees, dry or rain; the lower's own
        // wherever it does not.
        assert_eq!(count(&|at| upper[at] != 7 && pixels[at] == 0), seen);
        assert_eq!(
            count(&|at| upper[at] == 7 && pixels[at] == lower[at]),
            unseen
        );
        assert_eq!(seen + unseen, 624 * 261);
        // Both have rain in what the upper sees and the lower has rain
        // beyond it, or this would show nothing.
        assert!(count(&|at| (1..7).contains(&upper[at]) && (1..7).contains(&lower[at])) > 1000);
        assert!(count(&|at| upper[at] == 0 && (1..7).contains(&lower[at])) > 1000);
        let rain_left = count(&|at| (1..7).contains(&pixels[at]));
        assert_eq!(
            rain_left,
            count(&|at| upper[at] == 7 && (1..7).contains(&lower[at]))
        );
        assert!(rain_left > 1000);
    }
}
