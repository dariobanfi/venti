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
//! colours, which a map draws pinned to the layer's four corners.
//!
//! What is said here is fixed rules and no judgement of the day: how old a
//! frame is, when the newest is too old to be the rain of now, and what
//! the steps stand for.

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
/// server calls the source behind.
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
/// layer is, in words, and whether its pictures are an analysis (rain as
/// a model puts it together from radars and gauges, a quarter of an hour
/// and more old when it arrives) and not what radars measured minutes
/// ago. A source it does not know is a radar's.
const SOURCES: [(&str, &str, bool); 2] = [
    ("dwd-radar-ry", "Germany's radars", false),
    (
        "geosphere-inca-rr",
        "Austria's analysis of radar and gauges",
        true,
    ),
];

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

/// The rain radar as a shell shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RadarView {
    /// One layer for each radar source the server has, each to be drawn
    /// as a layer of its own with its own credit, in the order to draw
    /// them: the first at the bottom, the last on top. A measured radar
    /// picture lies above an analysis.
    pub layers: Vec<RadarLayerView>,
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
    let mut layers: Vec<(bool, RadarLayerView)> = response
        .layers
        .iter()
        .take(MAX_LAYERS)
        .filter(|layer| is_sound(layer))
        .map(|layer| {
            let known = SOURCES
                .iter()
                .find(|(source, _, _)| *source == layer.source.as_str());
            let analysis = known.is_some_and(|(_, _, analysis)| *analysis);
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
                    known.map_or(SOME_RADAR, |(_, what, _)| what),
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
            (analysis, view)
        })
        .collect();
    // The order to draw them in, the first at the bottom: analyses, then
    // what radars measured, each kind in the server's order.
    layers.sort_by_key(|(analysis, _)| !*analysis);
    let any_analysis = layers.iter().any(|(analysis, _)| *analysis);
    let mut layers: Vec<RadarLayerView> = layers.into_iter().map(|(_, view)| view).collect();
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
    Ok(RadarView {
        layers,
        key: key(),
        unseen_words: "Not seen by the radar".to_owned(),
        note,
    })
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
/// its pixels stay as they are.
#[uniffi::export]
pub fn radar_frame_image(
    picture: Vec<u8>,
    rain: Vec<u32>,
    unseen: u32,
) -> Result<Vec<u8>, CoreError> {
    let unreadable = || CoreError::Server {
        message: "The radar picture could not be read.".to_owned(),
    };
    if rain.len() != RAIN_STEPS_MM_H.len() {
        return Err(CoreError::Server {
            message: format!(
                "The rain radar has {} steps of rain, and {} colours were given.",
                RAIN_STEPS_MM_H.len(),
                rain.len()
            ),
        });
    }
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if picture.len() > MAX_PICTURE_BYTES || !picture.starts_with(SIGNATURE) {
        return Err(unreadable());
    }

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
        // A chunk: its length, its name, its bytes, a checksum.
        let (length, name) = match rest {
            [a, b, c, d, n0, n1, n2, n3, ..] => (
                u32::from_be_bytes([*a, *b, *c, *d]) as usize,
                [*n0, *n1, *n2, *n3],
            ),
            _ => return Err(unreadable()),
        };
        let whole = length.checked_add(12).ok_or_else(unreadable)?;
        let chunk = rest.get(..whole).ok_or_else(unreadable)?;
        rest = &rest[whole..];
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
                if length != palette.len() {
                    return Err(unreadable());
                }
                palette_seen = true;
            }
            b"tRNS" => {}
            b"IDAT" if !palette_seen => return Err(unreadable()),
            b"IEND" => {
                out.extend_from_slice(chunk);
                ended = true;
            }
            _ => out.extend_from_slice(chunk),
        }
    }
    Ok(out)
}

/// Appends a chunk of a PNG: its length, its name, its bytes and their
/// checksum.
fn write_chunk(out: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
    // A palette of eight colours: the length fits many times over.
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&name);
    out.extend_from_slice(data);
    let checksum = crc32(&out[start..]);
    out.extend_from_slice(&checksum.to_be_bytes());
}

/// The checksum of a PNG's chunk: CRC-32 as ISO 3309 and the PNG
/// specification define it, bit by bit. It runs over a few dozen bytes.
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

    /// 21 November 2026, 11:44 UTC: the moment the development mode's
    /// radar clock stands at.
    const NOW: i64 = 1_795_261_440;

    /// The development mode's answer, as `crates/server/tests/golden/radar.json`
    /// keeps it.
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

    /// The answer of a server with the first region's two sources, as
    /// `crates/server/tests/golden/radar_two_layers.json` keeps it: both
    /// recordings, by a clock at 12:40 UTC.
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
}
