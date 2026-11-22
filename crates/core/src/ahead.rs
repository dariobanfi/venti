//! The airspace ahead of the glider, seen from the side: what a straight
//! line over the ground runs under, into or over, and where the glide
//! would pass each of it.
//!
//! It is a picture to plan a glide with, not a warning: the warnings come
//! from the engine, with their margins and their sound. Where the session
//! was given terrain the picture has the ground in it, and a limit given
//! above ground stands on that ground. Where it was not, such a limit
//! cannot be placed exactly; it is then drawn as far as it may reach and
//! marked as uncertain.

use venti_airspace::{Altitudes, Policy, PressureAltitude, destination, resolve};
use venti_domain::GeoPoint;

use crate::airspace::{distance_in_words, kind_in_words, limit_in_words, policy_view};
use crate::{AirspacePolicy, AirspaceSet};

/// How far ahead the side view looks, metres.
pub(crate) const AHEAD_M: f64 = 10_000.0;

/// The ground along the line is read this far apart, metres.
pub(crate) const GROUND_STEP_M: f64 = 100.0;

/// And at this many places: from the glider to the end of the line.
pub(crate) const GROUND_PLACES: usize = 101;

/// A glider lower than this over the ground is on it, or about to be:
/// nothing is said about where its line meets the ground.
const ON_THE_GROUND_M: f64 = 30.0;

/// The side view shows at least this much height under and over the
/// glider, metres.
const ROOM_M: f64 = 500.0;

/// And never more than this much, however high or low an airspace
/// reaches: beyond it the picture would be all airspace and no glider.
const ROOM_AT_MOST_M: f64 = 3_000.0;

/// Under the glider it shows no more than this, wherever a steep line
/// ends: without terrain the picture cannot say where the ground is, and
/// a line drawn far under it would say nothing.
const ROOM_BELOW_AT_MOST_M: f64 = 1_500.0;

/// One airspace on the line ahead.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AirspaceAheadSpan {
    /// The airspace's identifier.
    pub id: String,
    /// Its name as published, or "Unnamed airspace".
    pub name: String,
    /// What it is: "Class D control zone".
    pub kind: String,
    /// Its floor as published.
    pub lower: String,
    /// Its ceiling as published.
    pub upper: String,
    /// Where on the line it begins, metres from the glider. Zero when the
    /// glider is within its outline already.
    pub from_m: f64,
    /// Where it ends, or the end of the line.
    pub to_m: f64,
    /// Its floor on the glider's height scale, metres above mean sea
    /// level: the lowest the floor can be.
    pub floor_m: f64,
    /// Its ceiling: the highest it can be. `None` when it has none.
    pub ceiling_m: Option<f64>,
    /// Whether floor or ceiling could not be placed exactly: a limit above
    /// ground without knowing the ground, or a flight level without a
    /// pressure sensor. The airspace is then drawn as far as it may
    /// reach.
    pub uncertain: bool,
    /// Whether the glider's line passes through it.
    pub in_the_way: bool,
    /// How the app treats it.
    pub policy: AirspacePolicy,
}

/// The ground at one place on the line ahead.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct AheadGround {
    /// How far from the glider, metres.
    pub distance_m: f64,
    /// The height of the ground there, metres above mean sea level.
    pub elevation_m: f64,
}

/// The airspace ahead, from the side.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AirspaceAhead {
    /// Which way the view looks, in words: "On course SW" or "Towards
    /// Sample landing field".
    pub toward: String,
    /// The direction, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far the view reaches, metres.
    pub distance_m: f64,
    /// The glider's altitude, metres above mean sea level.
    pub altitude_m: f64,
    /// The height the glider's line ends at, at the far end of the view.
    /// Equal to the altitude when there is no line to draw.
    pub line_end_m: f64,
    /// What the line is, in words: the glide of the last seconds, or the
    /// glider's own glide in the wind.
    pub line_words: String,
    /// The lowest height the picture needs to show.
    pub bottom_m: f64,
    /// The highest.
    pub top_m: f64,
    /// The airspaces on the line, in the order they begin. Airspace a
    /// paraglider needs nothing for is left out.
    pub spans: Vec<AirspaceAheadSpan>,
    /// What matters most about the airspace, in a line.
    pub words: String,
    /// The ground along the line, every hundred metres, as far as the
    /// session's terrain covers it. Empty without terrain.
    pub ground: Vec<AheadGround>,
    /// Where the glider's line meets the ground, metres from the glider.
    /// `None` when it stays clear as far as the ground is known, when
    /// there is no line, and for a glider on the ground.
    pub line_meets_ground_m: Option<f64>,
    /// What the line does over the ground, in a line: where it meets it,
    /// or by how much it stays clear. `None` without terrain on the line.
    pub ground_words: Option<String>,
}

/// Where the view looks from, and along what.
#[derive(Debug, Clone)]
pub(crate) struct Look {
    pub position: GeoPoint,
    /// The glider's altitude, the one it measures height over the
    /// terrain with, and the scale the picture is drawn on.
    pub altitude_m: f64,
    /// What an altimeter set to the QNH shows, when the session has one:
    /// limits above sea level are measured against it, as the warnings do.
    pub qnh_altitude_m: Option<f64>,
    pub pressure_altitude_m: Option<f64>,
    pub bearing_deg: f64,
    pub toward: String,
    /// Metres forward over the ground for each metre of height lost, if
    /// the glider makes way on this line.
    pub glide: Option<f64>,
    pub line_words: String,
    /// The ground along the line: [`GROUND_PLACES`] heights a
    /// [`GROUND_STEP_M`] apart from the glider on, each `None` where the
    /// terrain does not reach. Empty without terrain.
    pub ground: Vec<Option<f64>>,
}

/// The side view for one moment: over the airspace of `set`, or with
/// `None` over the ground alone. `None` when the line cannot be laid out,
/// which only happens next to a pole.
pub(crate) fn ahead(set: Option<&AirspaceSet>, look: &Look) -> Option<AirspaceAhead> {
    let end = destination(look.position, look.bearing_deg, AHEAD_M)?;
    let altitudes = |ground_elevation_m: Option<f64>| Altitudes {
        altitude_m: look.altitude_m,
        qnh_altitude_m: look.qnh_altitude_m,
        pressure: look.pressure_altitude_m.map(PressureAltitude::standard),
        ground_elevation_m,
    };
    // The lowest and the highest ground under a stretch of the line, if
    // all of it is known.
    let ground_between = |from_m: f64, to_m: f64| -> Option<(f64, f64)> {
        let first = (from_m / GROUND_STEP_M).floor() as usize;
        let last = (to_m / GROUND_STEP_M).ceil() as usize;
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for height in look.ground.get(first..=last.min(GROUND_PLACES - 1))? {
            let height = (*height)?;
            (low, high) = (low.min(height), high.max(height));
        }
        low.is_finite().then_some((low, high))
    };
    let known: Vec<AheadGround> = look
        .ground
        .iter()
        .enumerate()
        .filter_map(|(place, height)| {
            height.map(|elevation_m| AheadGround {
                distance_m: place as f64 * GROUND_STEP_M,
                elevation_m,
            })
        })
        .collect();
    let lowest_ground = known
        .iter()
        .map(|place| place.elevation_m)
        .min_by(f64::total_cmp);
    let highest_ground = known
        .iter()
        .map(|place| place.elevation_m)
        .max_by(f64::total_cmp);
    let line_end_m = look
        .glide
        .filter(|glide| glide.is_finite() && *glide > 0.0)
        .map_or(look.altitude_m, |glide| look.altitude_m - AHEAD_M / glide);
    let has_line = look.glide.is_some();
    let line_at =
        |distance_m: f64| look.altitude_m + (line_end_m - look.altitude_m) * (distance_m / AHEAD_M);

    // The picture's height: room around the glider and its line, and the
    // edges of each airspace that are known levels, within reason. A
    // floor on the ground is no such edge: it is the foot of the picture
    // wherever that is.
    let reach = |height: f64| {
        height.clamp(
            look.altitude_m - ROOM_AT_MOST_M,
            look.altitude_m + ROOM_AT_MOST_M,
        )
    };
    let mut bottom = match lowest_ground {
        // With the ground in the picture, its foot lies under the lowest
        // ground on the line.
        Some(lowest) => (lowest - 100.0).min(look.altitude_m - 100.0),
        None => (look.altitude_m - ROOM_M).min(reach(line_end_m) - 100.0),
    };
    let mut top = look.altitude_m + ROOM_M;
    if let Some(highest) = highest_ground {
        // A ridge higher than the glider is shown whole.
        top = top.max(reach(highest + 100.0));
    }

    // The line ends where it meets the ground: what lies beyond is not
    // reached on it.
    let (line_meets_ground_m, ground_words) = over_the_ground(look, &line_at, has_line);
    let reached_m = line_meets_ground_m.unwrap_or(AHEAD_M);

    let mut spans = Vec::new();
    let crossed = set.map_or_else(Vec::new, |set| {
        set.index.cross_section(&[look.position, end])
    });
    for span in crossed {
        let Some(set) = set else { break };
        let policy = set.policy.policy_for(span.airspace);
        if policy == Policy::Free {
            continue;
        }
        // A stretch that runs to the end of the line ends there exactly,
        // not a hair before it.
        let from_m = span.from_m.max(0.0);
        let to_m = if span.to_m > AHEAD_M - 1.0 {
            AHEAD_M
        } else {
            span.to_m
        };
        // A limit above ground follows the ground. Drawn as one block it
        // begins over the lowest ground under the stretch and ends over
        // the highest: larger than the airspace, never smaller.
        let under = ground_between(from_m, to_m);
        let (floor, ceiling) = (
            resolve(&span.lower, &altitudes(under.map(|(low, _)| low))),
            resolve(&span.upper, &altitudes(under.map(|(_, high)| high))),
        );
        let (floor_m, ceiling_m) = (floor.low_m, ceiling.high_m);
        let (near, far) = (line_at(from_m), line_at(to_m));
        if floor.low_m == floor.high_m {
            bottom = bottom.min(reach(floor_m - 100.0));
            // Enough of an airspace overhead to see that it is there.
            top = top.max(reach(floor_m + 200.0));
        }
        if ceiling_m.is_finite() {
            bottom = bottom.min(reach(ceiling_m - 200.0));
            top = top.max(reach(ceiling_m + 100.0));
        }
        spans.push(AirspaceAheadSpan {
            id: span.airspace.id.to_string(),
            name: if span.airspace.name.trim().is_empty() {
                "Unnamed airspace".to_owned()
            } else {
                span.airspace.name.clone()
            },
            kind: kind_in_words(span.airspace),
            lower: limit_in_words(&span.lower),
            upper: limit_in_words(&span.upper),
            from_m,
            to_m,
            floor_m,
            ceiling_m: ceiling_m.is_finite().then_some(ceiling_m),
            uncertain: floor.unknown.is_some() || ceiling.unknown.is_some(),
            // With no way made on the line nothing on it is reached,
            // except what the glider is in already.
            in_the_way: (has_line || from_m <= 0.0)
                && from_m <= reached_m
                && near.max(far) >= floor_m
                && near.min(far) <= ceiling_m,
            policy: policy_view(policy),
        });
    }
    let bottom = bottom.max(
        look.altitude_m
            - if lowest_ground.is_some() {
                ROOM_AT_MOST_M
            } else {
                ROOM_BELOW_AT_MOST_M
            },
    );
    let (bottom_m, top_m) = (
        (bottom / 100.0).floor() * 100.0,
        (top / 100.0).ceil() * 100.0,
    );

    let words = match set {
        Some(_) => in_words(&spans, &line_at, has_line, reached_m),
        // Nothing may sound like "clear" when there is nothing to look in.
        None => "No airspace in the app".to_owned(),
    };
    Some(AirspaceAhead {
        toward: look.toward.clone(),
        bearing_deg: look.bearing_deg,
        distance_m: AHEAD_M,
        altitude_m: look.altitude_m,
        line_end_m,
        line_words: look.line_words.clone(),
        bottom_m,
        top_m,
        spans,
        words,
        ground: known,
        line_meets_ground_m,
        ground_words,
    })
}

/// Where the glider's line meets the ground, and what it does over the
/// ground in a line of words.
fn over_the_ground(
    look: &Look,
    line_at: &dyn Fn(f64) -> f64,
    has_line: bool,
) -> (Option<f64>, Option<String>) {
    // How far over the ground the line is at each place where the ground
    // is known, from the glider on, up to the first place where it is not.
    let over: Vec<(f64, f64)> = look
        .ground
        .iter()
        .enumerate()
        .map_while(|(place, height)| {
            let distance_m = place as f64 * GROUND_STEP_M;
            height.map(|ground| (distance_m, line_at(distance_m) - ground))
        })
        .collect();
    let Some(&(_, here)) = over.first() else {
        return (None, None);
    };
    if !has_line || here < ON_THE_GROUND_M {
        return (None, None);
    }
    let meeting = over.windows(2).find_map(|pair| {
        let ((before_m, before), (_, after)) = (pair[0], pair[1]);
        // Between two places the line and the ground are taken as
        // straight.
        (after <= 0.0).then(|| before_m + GROUND_STEP_M * before / (before - after))
    });
    if let Some(distance_m) = meeting {
        return (
            Some(distance_m),
            Some(format!(
                "Your line meets the ground in {}",
                distance_in_words(distance_m)
            )),
        );
    }
    let least = over
        .iter()
        .map(|&(_, clear)| clear)
        .fold(f64::INFINITY, f64::min);
    let known_to_m = over.last().map_or(0.0, |&(distance_m, _)| distance_m);
    let clear = format!(
        "Your line clears the ground by {:.0} m at the least",
        (least / 10.0).floor() * 10.0
    );
    (
        None,
        Some(if known_to_m >= AHEAD_M - GROUND_STEP_M {
            clear
        } else {
            // The app's ground ends before the line does.
            format!(
                "{clear}, as far as the app has the ground: {}",
                distance_in_words(known_to_m)
            )
        }),
    )
}

/// What matters most on the line, in a line.
fn in_words(
    spans: &[AirspaceAheadSpan],
    line_at: &dyn Fn(f64) -> f64,
    has_line: bool,
    reached_m: f64,
) -> String {
    let possibly = |span: &AirspaceAheadSpan| if span.uncertain { "possibly " } else { "" };
    if let Some(span) = spans
        .iter()
        .find(|span| span.in_the_way && span.from_m <= 0.0)
    {
        return if span.uncertain {
            format!("Possibly in {} now", span.name)
        } else {
            format!("In {} now", span.name)
        };
    }
    if let Some(span) = spans.iter().find(|span| span.in_the_way) {
        return format!(
            "{} in {}, {}on your line",
            span.name,
            distance_in_words(span.from_m),
            possibly(span)
        );
    }
    let Some(span) = spans.first() else {
        return format!(
            "No airspace to avoid on this line for {:.0} km",
            AHEAD_M / 1000.0
        );
    };
    // The nearest one, which the line passes over or under.
    let place = if span.from_m <= 0.0 {
        "here".to_owned()
    } else {
        format!("in {}", distance_in_words(span.from_m))
    };
    // Without a line, and beyond where the line meets the ground, there
    // is nothing to measure the airspace against.
    if !has_line || span.from_m > reached_m {
        return format!("{} {place}", span.name);
    }
    let (near, far) = (line_at(span.from_m), line_at(span.to_m));
    let above = span.floor_m - near.max(far);
    if above > 0.0 {
        return format!(
            "{} {place}: {:.0} m above your line",
            span.name,
            round_to_ten(above)
        );
    }
    let below = near.min(far) - span.ceiling_m.unwrap_or(f64::INFINITY);
    format!(
        "{} {place}: {:.0} m below your line",
        span.name,
        round_to_ten(below)
    )
}

fn round_to_ten(metres: f64) -> f64 {
    (metres / 10.0).round() * 10.0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::Core;
    use crate::testing::TempFolder;

    const NOW: i64 = 1_795_000_000;

    /// A control zone from the ground to 2,000 m, ten kilometres across,
    /// whose western edge lies three kilometres east of the glider; a
    /// restricted area above ground beyond it; and a class E area, which a
    /// paraglider needs nothing for, over all of it.
    const FILE: &str = "\
AC D
AY CTR
AN CTR EAST
AL GND
AH 2000m AMSL
DP 47:27:00 N 011:02:23 E
DP 47:27:00 N 011:10:21 E
DP 47:33:00 N 011:10:21 E
DP 47:33:00 N 011:02:23 E

AC R
AN ED-R HIGH
AL 1500m AMSL
AH 300m AGL
DP 47:27:00 N 011:10:30 E
DP 47:27:00 N 011:20:00 E
DP 47:33:00 N 011:20:00 E
DP 47:33:00 N 011:10:30 E

AC E
AN E EVERYWHERE
AL 1000ft AGL
AH FL 100
DP 47:00:00 N 010:00:00 E
DP 47:00:00 N 012:00:00 E
DP 48:00:00 N 012:00:00 E
DP 48:00:00 N 010:00:00 E
";

    fn set(folder: &TempFolder) -> Arc<AirspaceSet> {
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        core.import_airspace("ahead.txt".to_owned(), FILE.as_bytes().to_vec(), NOW)
            .unwrap();
        core.airspace().unwrap()
    }

    fn look(altitude_m: f64, bearing_deg: f64, glide: Option<f64>) -> Look {
        Look {
            position: GeoPoint::new(47.5, 11.0).unwrap(),
            altitude_m,
            qnh_altitude_m: None,
            pressure_altitude_m: Some(altitude_m),
            bearing_deg,
            toward: "On course E".to_owned(),
            glide,
            line_words: "Your glide now, 8".to_owned(),
            ground: Vec::new(),
        }
    }

    #[test]
    fn a_glide_into_a_zone_is_said_and_one_over_it_is_measured() {
        let folder = TempFolder::new();
        let set = set(&folder);

        // East at 2,300 m with a glide of 8: 375 m are lost in the three
        // kilometres to the zone's edge, which is reached at 1,925 m,
        // under its ceiling.
        let view = ahead(Some(&set), &look(2300.0, 90.0, Some(8.0))).unwrap();
        assert_eq!(view.distance_m, 10_000.0);
        assert!((view.line_end_m - 1050.0).abs() < 1e-6);
        // The class E area is left out.
        assert_eq!(view.spans.len(), 1, "{:?}", view.spans);
        let zone = &view.spans[0];
        assert_eq!(zone.name, "CTR EAST");
        assert_eq!(zone.kind, "Class D control zone");
        assert_eq!(
            (zone.lower.as_str(), zone.upper.as_str()),
            ("GND", "2000 m AMSL")
        );
        assert!((zone.from_m - 3000.0).abs() < 60.0, "{}", zone.from_m);
        assert_eq!(zone.to_m, 10_000.0);
        assert_eq!(zone.ceiling_m, Some(2000.0));
        // The ground is somewhere under the glider: nothing is in doubt.
        assert!(!zone.uncertain);
        assert!(zone.in_the_way);
        assert_eq!(view.words, "CTR EAST in 3.0 km, on your line");
        // Room for the line's end and for the glider.
        assert!(view.bottom_m <= 900.0 && view.top_m >= 2800.0, "{view:?}");

        // From 3,400 m the same glide ends at 2,150 m: over the zone all
        // the way, by 150 m at the far end.
        let view = ahead(Some(&set), &look(3400.0, 90.0, Some(8.0))).unwrap();
        assert!(!view.spans[0].in_the_way);
        assert_eq!(view.words, "CTR EAST in 3.0 km: 150 m below your line");

        // Westwards there is nothing.
        let view = ahead(Some(&set), &look(2300.0, 270.0, Some(8.0))).unwrap();
        assert!(view.spans.is_empty());
        assert_eq!(view.words, "No airspace to avoid on this line for 10 km");
        assert_eq!((view.bottom_m, view.top_m), (900.0, 2800.0));

        // A steep line ends far under any ground: the picture does not
        // follow it down.
        let view = ahead(Some(&set), &look(2300.0, 270.0, Some(3.0))).unwrap();
        assert!(view.line_end_m < -1000.0);
        assert_eq!(view.bottom_m, 800.0);
    }

    #[test]
    fn a_glider_inside_and_a_line_that_makes_no_way() {
        let folder = TempFolder::new();
        let set = set(&folder);

        // In the zone's outline, under its ceiling.
        let inside = Look {
            position: GeoPoint::new(47.5, 11.1).unwrap(),
            ..look(1800.0, 90.0, Some(8.0))
        };
        let view = ahead(Some(&set), &inside).unwrap();
        assert_eq!(view.spans[0].from_m, 0.0);
        assert!(view.spans[0].in_the_way);
        assert_eq!(view.words, "In CTR EAST now");

        // A wind the glider makes no way against: no line, and what lies
        // ahead is named with its distance only.
        let view = ahead(Some(&set), &look(2300.0, 90.0, None)).unwrap();
        assert_eq!(view.line_end_m, 2300.0);
        assert!(!view.spans[0].in_the_way);
        assert_eq!(view.words, "CTR EAST in 3.0 km");
    }

    #[test]
    fn the_ground_is_in_the_picture_and_the_line_ends_on_it() {
        let folder = TempFolder::new();
        let set = set(&folder);

        // Flat at 800 m for four kilometres, then a slope that rises
        // 130 m every hundred metres to a ridge of 2,100 m at five, and
        // stays there. A glide of 8 from 2,300 m is at 1,712 m where it
        // meets the slope, 4.7 km out: before it would be over the ridge.
        let ground: Vec<Option<f64>> = (0..GROUND_PLACES)
            .map(|place| {
                let distance = place as f64 * GROUND_STEP_M;
                Some((800.0 + (distance - 4000.0) * 1.3).clamp(800.0, 2100.0))
            })
            .collect();
        let over_a_ridge = Look {
            ground: ground.clone(),
            ..look(2300.0, 270.0, Some(8.0))
        };
        let view = ahead(Some(&set), &over_a_ridge).unwrap();
        assert_eq!(view.ground.len(), 101);
        assert_eq!(view.ground[45].distance_m, 4500.0);
        assert!((view.ground[45].elevation_m - 1450.0).abs() < 1e-6);
        let meets = view.line_meets_ground_m.unwrap();
        assert!((meets - 4701.75).abs() < 0.5, "{meets}");
        assert_eq!(
            view.ground_words.as_deref(),
            Some("Your line meets the ground in 4.7 km")
        );
        // The picture's foot lies under the lowest ground.
        assert_eq!(view.bottom_m, 700.0);

        // From 3,400 m the same glide clears the ridge: by 675 m where the
        // ridge begins, and by less and less as it sinks over it, to 50 m
        // at the end of the line.
        let high = Look {
            ground: ground.clone(),
            ..look(3400.0, 270.0, Some(8.0))
        };
        let view = ahead(Some(&set), &high).unwrap();
        assert_eq!(view.line_meets_ground_m, None);
        assert_eq!(
            view.ground_words.as_deref(),
            Some("Your line clears the ground by 50 m at the least")
        );

        // Where the app's ground ends after three kilometres, that is
        // said.
        let short: Vec<Option<f64>> = ground
            .iter()
            .enumerate()
            .map(|(place, height)| height.filter(|_| place <= 30))
            .collect();
        let view = ahead(
            Some(&set),
            &Look {
                ground: short,
                ..look(2300.0, 270.0, Some(8.0))
            },
        )
        .unwrap();
        assert_eq!(view.ground.len(), 31);
        assert_eq!(
            view.ground_words.as_deref(),
            Some(
                "Your line clears the ground by 1120 m at the least, as far as the app has the ground: 3.0 km"
            )
        );

        // A zone beyond where the line meets the ground is named and not
        // measured against the line, and it is not in the line's way.
        let low = Look {
            ground: (0..GROUND_PLACES).map(|_| Some(800.0)).collect(),
            ..look(1000.0, 90.0, Some(8.0))
        };
        let view = ahead(Some(&set), &low).unwrap();
        assert!((view.line_meets_ground_m.unwrap() - 1600.0).abs() < 0.5);
        assert!(!view.spans[0].in_the_way);
        assert_eq!(view.words, "CTR EAST in 3.0 km");
        // The zone stands on the ground now, and nothing about it is in
        // doubt.
        assert_eq!(view.spans[0].floor_m, 800.0);

        // A glider on the ground has no line to speak of, and without
        // airspace in the app nothing sounds like "clear".
        let standing = Look {
            ground: (0..GROUND_PLACES).map(|_| Some(800.0)).collect(),
            ..look(805.0, 90.0, Some(8.0))
        };
        let view = ahead(None, &standing).unwrap();
        assert_eq!((view.line_meets_ground_m, view.ground_words), (None, None));
        assert_eq!(view.words, "No airspace in the app");
        assert!(view.spans.is_empty());
    }

    #[test]
    fn a_limit_above_ground_stands_on_the_ground_where_it_is_known() {
        let folder = TempFolder::new();
        let set = set(&folder);

        // The restricted area ends 300 m over the ground. Under it the
        // ground rises from 900 m to 1,400 m: drawn as one block, it ends
        // 300 m over the highest of it.
        let ground: Vec<Option<f64>> = (0..GROUND_PLACES)
            .map(|place| Some(900.0 + 5.0 * place as f64))
            .collect();
        let from_the_zone = Look {
            position: GeoPoint::new(47.5, 11.15).unwrap(),
            ground,
            ..look(1700.0, 90.0, Some(8.0))
        };
        let view = ahead(Some(&set), &from_the_zone).unwrap();
        let area = view
            .spans
            .iter()
            .find(|span| span.name == "ED-R HIGH")
            .unwrap();
        assert!(!area.uncertain);
        assert_eq!(area.floor_m, 1500.0);
        assert_eq!(area.ceiling_m, Some(1700.0));
    }

    #[test]
    fn a_limit_above_ground_is_drawn_as_far_as_it_may_reach() {
        let folder = TempFolder::new();
        let set = set(&folder);

        // From within the zone at 1,700 m, eastwards: the restricted area
        // begins at 1,500 m and ends 300 m over a ground the app does not
        // know, which may be as high as the glider.
        let from_the_zone = Look {
            position: GeoPoint::new(47.5, 11.15).unwrap(),
            ..look(1700.0, 90.0, Some(8.0))
        };
        let view = ahead(Some(&set), &from_the_zone).unwrap();
        let area = view
            .spans
            .iter()
            .find(|span| span.name == "ED-R HIGH")
            .unwrap();
        assert!(area.uncertain);
        assert_eq!(area.floor_m, 1500.0);
        assert_eq!(area.ceiling_m, Some(2000.0));
        assert_eq!(area.upper, "300 m AGL");
    }
}
