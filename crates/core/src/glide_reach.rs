//! What a glide reaches from a place on the map, and how steep the ground
//! is: the first pieces of hike and fly.
//!
//! The pilot long-presses a summit, or any place, and names a glide ratio
//! and, if they like, a height to start at and a wind. The core lays the
//! glide over the ground the shell loaded, in 72 directions, and gives
//! the area it reaches as a ring for the map, each direction's distance
//! and arrival height, and the answer in words
//! ([`glide_reach`]). For a map of slopes it gives each place's
//! steepness and class ([`slope_classes`]).
//!
//! **The rules.** Fixed, ours, and the owner's to change:
//!
//! - **The start.** The height asked for, or without one the ground at
//!   the place: a start on the ground, as from a summit walked up to. A
//!   height under the ground is the ground's. Held up to
//!   [`START_AT_MOST_M`].
//! - **The glide.** Straight lines every 5° at the glider's trim speed
//!   (38 km/h, as a planned route has it) with the pilot's glide ratio,
//!   held between the route's [`crate::GLIDE_RATIO`], holding each line
//!   against the wind's push ([`venti_flight::live::navigation`]). The
//!   wind is one wind, the same at every height and place.
//! - **The margin.** [`REACH_MARGIN_M`] over the ground at the end and on the
//!   way, the same as a planned route keeps
//!   ([`crate::MARGIN_M`]): a reach and a route then agree, and it leaves
//!   room for the sink a glide meets and a circuit to land. It is more
//!   than the cockpit's 100 m in flight: a plan on the ground keeps more
//!   than a pilot in the air settles for. A glide begun on the ground
//!   cannot have it at the start: over the first kilometre it grows from
//!   what the glide has to all of it
//!   ([`venti_flight::live::range::RESERVE_GROWS_OVER_M`]), so ground must
//!   fall away by some 18° for a glide of 8 to leave in that direction.
//! - **The ground.** Read every 100 m along each line from the elevation
//!   tiles the shell loaded, taken as straight between. A line that runs
//!   off them ends there and says so: the glide reaches at least that far.
//!
//! What it does not know: lift and sink on the way, a wind that changes
//! with height or valley, a lee, and ground finer than the tiles and the
//! steps. Nothing flies round a ridge: what lies behind one is outside the
//! area, although another way may reach it.

use std::sync::Arc;

use venti_domain::GeoPoint;
use venti_flight::live::Wind;
use venti_flight::live::navigation::Glider;
use venti_flight::live::range::{self, EdgeEnd};
use venti_flight::slope::{self, SlopeClass};

use crate::airspace::distance_in_words;
use crate::terrain::Terrain;
use crate::{Coordinate, GLIDE_RATIO};

/// What a glide keeps over the ground, unless asked for another, metres.
pub const REACH_MARGIN_M: f64 = 200.0;

/// A margin asked for is held between these, metres.
const MARGIN_HELD_M: (f64, f64) = (0.0, 1000.0);

/// No glide starts higher than this, metres above mean sea level.
pub const START_AT_MOST_M: f64 = 9000.0;

/// Under this the glide reaches nowhere, metres: less than one step of
/// the ground read.
const NOWHERE_M: f64 = 100.0;

/// Where the nearest reach is at least this share of the farthest, the
/// glide reaches about as far every way.
const EVERY_WAY_SHARE: f64 = 0.9;

/// What the pilot asks a glide's reach for.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GlideReachSetting {
    /// Latitude of the place to start from, WGS84 degrees.
    pub lat: f64,
    /// Its longitude.
    pub lon: f64,
    /// The glider's glide ratio in still air: the pilot's, from the app's
    /// settings.
    pub glide_ratio: f64,
    /// The height to start at, metres above mean sea level. Without it,
    /// the ground at the place: a start from a summit.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub start_height_m: Option<f64>,
    /// Where the wind comes from, degrees clockwise from north. A wind
    /// needs this and its speed; without either the air is calm.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub wind_from_deg: Option<f64>,
    /// Its speed, km/h, the same at every height.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub wind_kmh: Option<f64>,
    /// What to keep over the ground, metres. Without it
    /// [`REACH_MARGIN_M`].
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default))]
    pub margin_m: Option<f64>,
}

/// Why a glide's reach ends where it does, in one direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum ReachEnd {
    /// The glide comes down to the margin over the ground there.
    Ground,
    /// The ground loaded ends there: the glide reaches at least so far.
    NotLoaded,
    /// The wind lets the glider make no way in this direction.
    NoWay,
}

/// A glide's reach in one direction.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ReachRay {
    /// The direction, degrees clockwise from north: 0, 5, 10 and on.
    pub bearing_deg: f64,
    /// How far the glide gets, metres.
    pub distance_m: f64,
    /// Where it gets to.
    pub end_at: Coordinate,
    /// How high the glider is there, metres above mean sea level.
    pub arrival_m: f64,
    /// How high that is over the ground there, metres. Absent where the
    /// ground is not loaded.
    pub over_ground_m: Option<f64>,
    /// Why it ends there.
    pub end: ReachEnd,
}

/// What a glide reaches from a place on the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GlideReach {
    /// The area reached, as a closed ring for the map: the end of each
    /// direction's glide, clockwise from north, and the first again at
    /// the end. Empty when the ground at the place is not loaded.
    pub outline: Vec<Coordinate>,
    /// Each direction's glide, clockwise from north. Empty as the outline.
    pub rays: Vec<ReachRay>,
    /// How far the glide gets where it gets farthest, metres.
    pub farthest_m: f64,
    /// In which direction, degrees clockwise from north.
    pub farthest_bearing_deg: f64,
    /// How far it gets where it gets least, metres, among the directions
    /// whose glide ends on the ground: where the ground loaded ends the
    /// glide may get farther. Zero when none does.
    pub nearest_m: f64,
    /// In which direction, degrees.
    pub nearest_bearing_deg: f64,
    /// The height the glide starts at, metres above mean sea level.
    /// Absent when the ground at the place is not loaded.
    pub start_m: Option<f64>,
    /// The ground at the place, metres above mean sea level.
    pub ground_m: Option<f64>,
    /// The glide ratio used, after holding it within bounds.
    pub glide_ratio: f64,
    /// What the glide keeps over the ground, metres.
    pub margin_m: f64,
    /// Whether some direction runs off the ground loaded: load the tiles
    /// covering a wider box ([`Terrain::tiles_covering`]) and ask again.
    pub cut_short: bool,
    /// The answer in a sentence or two: "From 1850 m at a glide of 8 you
    /// reach 9.4 km to the north-east and 3.1 km to the south-west, 200 m
    /// over the ground. Ground not loaded beyond the edge of the map to
    /// the east."
    pub words: String,
    /// What the answer does not know, for the foot of the card.
    pub note: String,
}

/// The steepness of the ground at one place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SlopeAt {
    /// The place, as it was asked for.
    pub at: Coordinate,
    /// How steep the ground is, degrees from level. Absent where the
    /// ground around the place is not loaded.
    pub slope_deg: Option<f64>,
    /// Its class. Absent as the steepness.
    pub class: Option<SlopeBand>,
}

/// A class of steepness, as maps of slopes draw them, avalanche maps
/// among them from 30° on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum SlopeBand {
    /// Under 25°.
    Under25,
    /// 25° to under 30°.
    From25,
    /// 30° to under 35°.
    From30,
    /// 35° to under 40°.
    From35,
    /// 40° and steeper.
    From40,
}

impl From<SlopeClass> for SlopeBand {
    fn from(class: SlopeClass) -> Self {
        match class {
            SlopeClass::Under25 => Self::Under25,
            SlopeClass::From25 => Self::From25,
            SlopeClass::From30 => Self::From30,
            SlopeClass::From35 => Self::From35,
            SlopeClass::From40 => Self::From40,
        }
    }
}

/// What a straight glide reaches from a place on the map over the ground
/// in `terrain`: see the module's rules. The shell loads the ground
/// around the place first, with [`Terrain::tiles_wanted`] or, for a high
/// start, [`Terrain::tiles_covering`] a wider box.
#[uniffi::export]
pub fn glide_reach(setting: GlideReachSetting, terrain: Arc<Terrain>) -> GlideReach {
    let glide_ratio = if setting.glide_ratio.is_finite() {
        setting.glide_ratio.clamp(GLIDE_RATIO.0, GLIDE_RATIO.1)
    } else {
        Glider::default().glide_ratio
    };
    let margin_m = setting
        .margin_m
        .filter(|margin| margin.is_finite())
        .map_or(REACH_MARGIN_M, |margin| {
            margin.clamp(MARGIN_HELD_M.0, MARGIN_HELD_M.1)
        });
    let glider = Glider {
        glide_ratio,
        ..Glider::default()
    };
    let wind = match (setting.wind_from_deg, setting.wind_kmh) {
        (Some(from_deg), Some(kmh)) if from_deg.is_finite() && kmh.is_finite() && kmh > 0.0 => {
            Some(Wind {
                from_deg: from_deg.rem_euclid(360.0),
                speed_ms: kmh / 3.6,
            })
        }
        _ => None,
    };
    let start = setting
        .start_height_m
        .filter(|height| height.is_finite())
        .map(|height| height.min(START_AT_MOST_M));

    let reach = GeoPoint::new(setting.lat, setting.lon)
        .ok()
        .and_then(|place| {
            terrain.with(|ground| {
                range::reach(
                    ground,
                    place,
                    start.unwrap_or(f64::NEG_INFINITY),
                    wind,
                    &glider,
                    margin_m,
                )
            })
        });
    let note = note(margin_m);
    let Some(reach) = reach else {
        return GlideReach {
            outline: Vec::new(),
            rays: Vec::new(),
            farthest_m: 0.0,
            farthest_bearing_deg: 0.0,
            nearest_m: 0.0,
            nearest_bearing_deg: 0.0,
            start_m: None,
            ground_m: None,
            glide_ratio,
            margin_m,
            cut_short: false,
            words: "The ground here is not loaded yet: load the map around this place first."
                .to_owned(),
            note,
        };
    };

    let rays: Vec<ReachRay> = reach
        .edges
        .iter()
        .map(|edge| ReachRay {
            bearing_deg: edge.bearing_deg,
            distance_m: edge.distance_m,
            end_at: Coordinate {
                lat: edge.position.lat(),
                lon: edge.position.lon(),
            },
            arrival_m: edge.arrival_m,
            over_ground_m: edge.ground_m.map(|ground| edge.arrival_m - ground),
            end: match edge.end {
                EdgeEnd::Ground => ReachEnd::Ground,
                EdgeEnd::Unknown => ReachEnd::NotLoaded,
                EdgeEnd::NoWay => ReachEnd::NoWay,
            },
        })
        .collect();
    let mut outline: Vec<Coordinate> = rays.iter().map(|ray| ray.end_at).collect();
    outline.extend(outline.first().copied());
    let farthest = rays
        .iter()
        .max_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
    // The nearest among the directions a glide leaves in at all: those it
    // does not are named apart.
    let nearest = rays
        .iter()
        .filter(|ray| ray.end == ReachEnd::Ground && ray.distance_m >= NOWHERE_M)
        .min_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
    let (farthest_m, farthest_bearing_deg) =
        farthest.map_or((0.0, 0.0), |ray| (ray.distance_m, ray.bearing_deg));
    let (nearest_m, nearest_bearing_deg) =
        nearest.map_or((0.0, 0.0), |ray| (ray.distance_m, ray.bearing_deg));

    let words = words(
        reach.altitude_m,
        glide_ratio,
        wind,
        margin_m,
        &rays,
        (farthest_m, farthest_bearing_deg),
        nearest.map(|ray| (ray.distance_m, ray.bearing_deg)),
    );
    GlideReach {
        cut_short: rays.iter().any(|ray| ray.end == ReachEnd::NotLoaded),
        outline,
        rays,
        farthest_m,
        farthest_bearing_deg,
        nearest_m,
        nearest_bearing_deg,
        start_m: Some(reach.altitude_m),
        ground_m: Some(reach.ground_m),
        glide_ratio,
        margin_m,
        words,
        note,
    }
}

/// How steep the ground is at each place, for a map of slopes: in the
/// order asked, each with its class. A place whose ground is not loaded
/// has neither.
#[uniffi::export]
pub fn slope_classes(points: Vec<Coordinate>, terrain: Arc<Terrain>) -> Vec<SlopeAt> {
    terrain.with(|ground| {
        points
            .into_iter()
            .map(|at| {
                let slope_deg = GeoPoint::new(at.lat, at.lon)
                    .ok()
                    .and_then(|place| slope::slope_deg(ground, place));
                SlopeAt {
                    at,
                    slope_deg,
                    class: slope_deg.map(|slope| SlopeClass::of(slope).into()),
                }
            })
            .collect()
    })
}

/// The answer in words.
fn words(
    start_m: f64,
    glide_ratio: f64,
    wind: Option<Wind>,
    margin_m: f64,
    rays: &[ReachRay],
    (farthest_m, farthest_deg): (f64, f64),
    nearest: Option<(f64, f64)>,
) -> String {
    let glide = if (glide_ratio - glide_ratio.round()).abs() < 0.05 {
        format!("{glide_ratio:.0}")
    } else {
        format!("{glide_ratio:.1}")
    };
    let wind_words = wind
        .map(|wind| {
            format!(
                " in {:.0} km/h of wind from the {}",
                wind.speed_ms * 3.6,
                direction(wind.from_deg)
            )
        })
        .unwrap_or_default();
    let from = format!("From {start_m:.0} m at a glide of {glide}{wind_words}");
    let margin = format!("{margin_m:.0} m over the ground");
    // The directions a glide does not leave in: from the ground, where it
    // falls away too slowly.
    let blocked = sides(
        rays.iter()
            .filter(|ray| ray.end == ReachEnd::Ground && ray.distance_m < NOWHERE_M),
    );
    let every_way = blocked.is_none() && rays.iter().all(|ray| ray.end != ReachEnd::NoWay);
    let mut words = if farthest_m < NOWHERE_M {
        format!("{from} the ground falls away too slowly to glide off anywhere with {margin}.")
    } else {
        match nearest {
            Some((nearest_m, _)) if every_way && nearest_m >= EVERY_WAY_SHARE * farthest_m => {
                format!(
                    "{from} you reach about {} every way, {margin}.",
                    distance_in_words(farthest_m)
                )
            }
            Some((nearest_m, nearest_deg)) => format!(
                "{from} you reach {} to the {} and {} to the {}, {margin}.",
                distance_in_words(farthest_m),
                direction(farthest_deg),
                distance_in_words(nearest_m),
                direction(nearest_deg)
            ),
            None => format!(
                "{from} you reach at least {} to the {}, {margin}.",
                distance_in_words(farthest_m),
                direction(farthest_deg)
            ),
        }
    };
    if let Some(blocked) = blocked.filter(|_| farthest_m >= NOWHERE_M) {
        let mut letters = blocked.chars();
        let capital: String = letters
            .next()
            .map(|first| first.to_uppercase().collect())
            .unwrap_or_default();
        words.push_str(&format!(
            " {capital}{} the ground falls away too slowly to glide off.",
            letters.as_str()
        ));
    }
    let sides = |end: ReachEnd| sides(rays.iter().filter(|ray| ray.end == end));
    if let Some(sides) = sides(ReachEnd::NoWay) {
        words.push_str(&format!(" In this wind no way is made {sides}."));
    }
    if let Some(sides) = sides(ReachEnd::NotLoaded) {
        words.push_str(&format!(
            " Ground not loaded beyond the edge of the map {sides}."
        ));
    }
    words
}

/// What the answer does not know.
fn note(margin_m: f64) -> String {
    format!(
        "Straight glides at trim speed, in calm air or one wind the same at every height: \
         no lift, no sink, no wind shear, no lee. Nothing flies round a ridge; what lies \
         behind one is outside, though another way may reach it. The ground is read every \
         100 m from the map's elevation tiles, and a ridge narrower than that can be missed. \
         A glide begun on the ground keeps {margin_m:.0} m over it from 1 km out; before \
         that, less."
    )
}

/// The sides the directions of some rays lie to, in words: "to the east",
/// "to the east and south-east", "on every side". `None` for no rays.
fn sides<'a>(rays: impl Iterator<Item = &'a ReachRay>) -> Option<String> {
    let mut named: Vec<usize> = rays.map(|ray| point(ray.bearing_deg)).collect();
    named.sort_unstable();
    named.dedup();
    match named.as_slice() {
        [] => None,
        all if all.len() == POINTS.len() => Some("on every side".to_owned()),
        [one] => Some(format!("to the {}", POINTS[*one])),
        [first @ .., last] => Some(format!(
            "to the {} and {}",
            first
                .iter()
                .map(|index| POINTS[*index])
                .collect::<Vec<_>>()
                .join(", "),
            POINTS[*last]
        )),
    }
}

const POINTS: [&str; 8] = [
    "north",
    "north-east",
    "east",
    "south-east",
    "south",
    "south-west",
    "west",
    "north-west",
];

/// Which of the eight points of the compass a direction is nearest.
fn point(degrees: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = ((degrees.rem_euclid(360.0) + 22.5) / 45.0) as usize;
    index % POINTS.len()
}

/// A direction in words: "north-east".
fn direction(degrees: f64) -> &'static str {
    POINTS[point(degrees)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terrain of one tile of zoom 11 around the Wallberg, its pixels'
    /// heights given by column and row.
    fn terrain(height: impl Fn(u32, u32) -> f64) -> (Arc<Terrain>, Coordinate) {
        use venti_flight::terrain::TileAddress;
        let wallberg = GeoPoint::new(47.6642, 11.7967).unwrap();
        let address = TileAddress::holding(wallberg, 11).unwrap();
        let side = 256_u32;
        let mut rgba = Vec::new();
        for row in 0..side {
            for column in 0..side {
                let coded = height(column, row) + 32_768.0;
                let whole = coded.floor();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                rgba.extend([
                    (whole / 256.0).floor() as u8,
                    (whole % 256.0) as u8,
                    ((coded - whole) * 256.0) as u8,
                    255,
                ]);
            }
        }
        // As a tile server sends it: a WebP image, here a lossless one.
        let mut image = Vec::new();
        image_webp::WebPEncoder::new(&mut image)
            .encode(&rgba, side, side, image_webp::ColorType::Rgba8)
            .unwrap();
        let terrain = Terrain::new();
        terrain
            .add_tile(address.z, address.x, address.y, image)
            .unwrap();
        // The tile's middle.
        let tiles = f64::from(1_u32 << 11);
        let (x, y) = (f64::from(address.x) + 0.5, f64::from(address.y) + 0.5);
        let lat = (std::f64::consts::PI * (1.0 - 2.0 * y / tiles))
            .sinh()
            .atan()
            .to_degrees();
        (
            terrain,
            Coordinate {
                lat,
                lon: x / tiles * 360.0 - 180.0,
            },
        )
    }

    fn setting(at: Coordinate) -> GlideReachSetting {
        GlideReachSetting {
            lat: at.lat,
            lon: at.lon,
            glide_ratio: 8.0,
            start_height_m: None,
            wind_from_deg: None,
            wind_kmh: None,
            margin_m: None,
        }
    }

    #[test]
    fn from_a_height_over_flat_ground_the_glide_reaches_as_far_every_way() {
        // Flat at 800 m, from 1400 m: 8 × 400 m = 3.2 km.
        let (terrain, middle) = terrain(|_, _| 800.0);
        let reach = glide_reach(
            GlideReachSetting {
                start_height_m: Some(1400.0),
                ..setting(middle)
            },
            terrain,
        );
        assert_eq!(reach.rays.len(), 72);
        assert_eq!(reach.outline.len(), 73);
        assert_eq!(reach.outline.first(), reach.outline.last());
        assert!((reach.farthest_m - 3200.0).abs() < 1.0, "{reach:?}");
        assert!((reach.nearest_m - 3200.0).abs() < 1.0);
        assert_eq!(reach.start_m, Some(1400.0));
        assert_eq!(reach.ground_m, Some(800.0));
        assert!(!reach.cut_short);
        for ray in &reach.rays {
            assert_eq!(ray.end, ReachEnd::Ground);
            assert!((ray.over_ground_m.unwrap() - 200.0).abs() < 1.0);
            assert!((ray.arrival_m - 1000.0).abs() < 1.0);
        }
        assert_eq!(
            reach.words,
            "From 1400 m at a glide of 8 you reach about 3.2 km every way, 200 m over the ground."
        );
        assert!(reach.note.contains("no sink"), "{}", reach.note);
    }

    #[test]
    fn a_wind_and_the_edge_of_the_map_are_named() {
        // A tile of zoom 11 is some 13 km wide here. From 2400 m in a
        // west wind of 20 km/h the glide reaches farthest to the east,
        // least to the west, and off the tile's east edge.
        let (terrain, middle) = terrain(|_, _| 800.0);
        let reach = glide_reach(
            GlideReachSetting {
                start_height_m: Some(2400.0),
                wind_from_deg: Some(270.0),
                wind_kmh: Some(20.0),
                ..setting(middle)
            },
            terrain,
        );
        assert!(reach.cut_short);
        assert_eq!(reach.rays[18].end, ReachEnd::NotLoaded);
        assert_eq!(reach.rays[54].end, ReachEnd::Ground);
        assert!(reach.rays[54].distance_m < 9000.0, "{:?}", reach.rays[54]);
        assert!(
            reach.words.starts_with(
                "From 2400 m at a glide of 8 in 20 km/h of wind from the west you reach"
            ),
            "{}",
            reach.words
        );
        assert!(reach.words.contains("to the west, 200 m over the ground."));
        assert!(
            reach
                .words
                .contains("Ground not loaded beyond the edge of the map to the "),
            "{}",
            reach.words
        );
        assert!(reach.words.contains("east"), "{}", reach.words);
    }

    #[test]
    fn a_summit_start_is_the_ground_and_flat_ground_lets_no_glide_off() {
        let (terrain, middle) = terrain(|_, _| 1200.0);
        let reach = glide_reach(setting(middle), terrain.clone());
        assert_eq!(reach.start_m, Some(1200.0));
        assert!(reach.farthest_m < 1.0);
        assert_eq!(
            reach.words,
            "From 1200 m at a glide of 8 the ground falls away too slowly to glide off anywhere with 200 m over the ground."
        );

        // Nowhere loaded: nothing to draw, and words that say why.
        let nowhere = glide_reach(
            GlideReachSetting {
                lat: 46.0,
                lon: 8.0,
                ..setting(middle)
            },
            terrain,
        );
        assert!(nowhere.outline.is_empty());
        assert_eq!(nowhere.start_m, None);
        assert_eq!(
            nowhere.words,
            "The ground here is not loaded yet: load the map around this place first."
        );
    }

    #[test]
    fn settings_out_of_bounds_are_held() {
        let (terrain, middle) = terrain(|_, _| 800.0);
        let reach = glide_reach(
            GlideReachSetting {
                glide_ratio: 80.0,
                margin_m: Some(-5.0),
                start_height_m: Some(1000.0),
                ..setting(middle)
            },
            terrain,
        );
        assert_eq!(reach.glide_ratio, 20.0);
        assert_eq!(reach.margin_m, 0.0);
        // 20 × 200 m.
        assert!((reach.farthest_m - 4000.0).abs() < 1.0, "{reach:?}");
        assert!(reach.words.contains("0 m over the ground"));
    }

    #[test]
    fn slopes_are_classed_place_by_place() {
        // Rising 20 m a pixel to the east: a pixel of a 256-pixel tile at
        // zoom 11 is some 51 m wide here, so about 21°.
        let (terrain, middle) = terrain(|column, _| 500.0 + 20.0 * f64::from(column));
        let found = slope_classes(
            vec![
                middle,
                Coordinate {
                    lat: 46.0,
                    lon: 8.0,
                },
            ],
            terrain,
        );
        assert_eq!(found.len(), 2);
        let slope = found[0].slope_deg.unwrap();
        assert!(slope > 19.0 && slope < 23.0, "{slope}");
        assert_eq!(found[0].class, Some(SlopeBand::Under25));
        assert_eq!(found[1].slope_deg, None);
        assert_eq!(found[1].class, None);
    }

    #[test]
    fn from_the_wallberg_on_the_ground_the_glide_reaches_the_valley() {
        // The region that comes with the iPhone app: the ground around
        // the Tegernsee, the Wallberg's summit in it at 1722 m.
        let terrain = Terrain::new();
        let archive = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/MapDemo/terrain.pmtiles"
        ))
        .unwrap();
        terrain.add_archive(archive).unwrap();
        let summit = Coordinate {
            lat: 47.6662,
            lon: 11.7982,
        };
        let reach = glide_reach(setting(summit), terrain);
        // The summit is a point in pixels 13 m wide, which hold it a few
        // metres lower.
        let start = reach.start_m.unwrap();
        assert!((start - 1722.0).abs() < 15.0, "{start}");
        // Its slopes fall away to the north and north-west, towards the
        // lake; to the east and west the ridge runs on and no glide
        // leaves it from the ground. Never farther than the height above
        // the lake (725 m) allows.
        assert!(
            (290.0..=330.0).contains(&reach.farthest_bearing_deg),
            "{reach:?}"
        );
        assert!(reach.farthest_m > 5000.0);
        assert!(reach.farthest_m <= 8.0 * (start - 725.0 - 200.0) + 1.0);
        assert!(reach.rays[0].distance_m > 2000.0, "{:?}", reach.rays[0]);
        assert!(reach.rays[54].distance_m < 1.0, "{:?}", reach.rays[54]);
        assert!(
            reach
                .words
                .contains("the ground falls away too slowly to glide off."),
            "{}",
            reach.words
        );
        assert!(!reach.cut_short);
    }

    #[test]
    fn directions_and_sides_are_named() {
        assert_eq!(direction(44.0), "north-east");
        assert_eq!(direction(359.0), "north");
        let ray = |bearing_deg: f64| ReachRay {
            bearing_deg,
            distance_m: 0.0,
            end_at: Coordinate { lat: 0.0, lon: 0.0 },
            arrival_m: 0.0,
            over_ground_m: None,
            end: ReachEnd::NotLoaded,
        };
        let rays = [ray(85.0), ray(90.0), ray(135.0), ray(180.0)];
        assert_eq!(
            sides(rays.iter()).as_deref(),
            Some("to the east, south-east and south")
        );
        assert_eq!(sides(rays[..2].iter()).as_deref(), Some("to the east"));
        assert_eq!(sides([].iter()), None);
        let all: Vec<ReachRay> = (0..8).map(|index| ray(45.0 * f64::from(index))).collect();
        assert_eq!(sides(all.iter()).as_deref(), Some("on every side"));
    }
}
