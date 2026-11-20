//! The airspace ahead of the glider, seen from the side: what a straight
//! line over the ground runs under, into or over, and where the glide
//! would pass each of it.
//!
//! It is a picture to plan a glide with, not a warning: the warnings come
//! from the engine, with their margins and their sound. Like them it has
//! no terrain, so a limit given above ground cannot be placed exactly; it
//! is then drawn as far as it may reach and marked as uncertain.

use venti_airspace::{Altitudes, Policy, PressureAltitude, destination, resolve};
use venti_domain::GeoPoint;

use crate::airspace::{distance_in_words, kind_in_words, limit_in_words, policy_view};
use crate::{AirspacePolicy, AirspaceSet};

/// How far ahead the side view looks, metres.
pub(crate) const AHEAD_M: f64 = 10_000.0;

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
    /// What matters most, in a line.
    pub words: String,
}

/// Where the view looks from, and along what.
#[derive(Debug, Clone)]
pub(crate) struct Look {
    pub position: GeoPoint,
    pub altitude_m: f64,
    pub pressure_altitude_m: Option<f64>,
    pub bearing_deg: f64,
    pub toward: String,
    /// Metres forward over the ground for each metre of height lost, if
    /// the glider makes way on this line.
    pub glide: Option<f64>,
    pub line_words: String,
}

/// The side view for one moment. `None` when the line cannot be laid out,
/// which only happens next to a pole.
pub(crate) fn ahead(set: &AirspaceSet, look: &Look) -> Option<AirspaceAhead> {
    let end = destination(look.position, look.bearing_deg, AHEAD_M)?;
    let altitudes = Altitudes {
        altitude_m: look.altitude_m,
        pressure: look.pressure_altitude_m.map(PressureAltitude::standard),
        // No terrain model yet.
        ground_elevation_m: None,
    };
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
    let mut bottom = (look.altitude_m - ROOM_M).min(reach(line_end_m) - 100.0);
    let mut top = look.altitude_m + ROOM_M;

    let mut spans = Vec::new();
    for span in set.index.cross_section(&[look.position, end]) {
        let policy = set.policy.policy_for(span.airspace);
        if policy == Policy::Free {
            continue;
        }
        let (floor, ceiling) = (
            resolve(&span.lower, &altitudes),
            resolve(&span.upper, &altitudes),
        );
        let (floor_m, ceiling_m) = (floor.low_m, ceiling.high_m);
        // A stretch that runs to the end of the line ends there exactly,
        // not a hair before it.
        let from_m = span.from_m.max(0.0);
        let to_m = if span.to_m > AHEAD_M - 1.0 {
            AHEAD_M
        } else {
            span.to_m
        };
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
                && near.max(far) >= floor_m
                && near.min(far) <= ceiling_m,
            policy: policy_view(policy),
        });
    }
    let bottom = bottom.max(look.altitude_m - ROOM_BELOW_AT_MOST_M);
    let (bottom_m, top_m) = (
        (bottom / 100.0).floor() * 100.0,
        (top / 100.0).ceil() * 100.0,
    );

    let words = in_words(&spans, &line_at, has_line);
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
    })
}

/// What matters most on the line, in a line.
fn in_words(spans: &[AirspaceAheadSpan], line_at: &dyn Fn(f64) -> f64, has_line: bool) -> String {
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
    if !has_line {
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
            pressure_altitude_m: Some(altitude_m),
            bearing_deg,
            toward: "On course E".to_owned(),
            glide,
            line_words: "Your glide now, 8".to_owned(),
        }
    }

    #[test]
    fn a_glide_into_a_zone_is_said_and_one_over_it_is_measured() {
        let folder = TempFolder::new();
        let set = set(&folder);

        // East at 2,300 m with a glide of 8: 375 m are lost in the three
        // kilometres to the zone's edge, which is reached at 1,925 m,
        // under its ceiling.
        let view = ahead(&set, &look(2300.0, 90.0, Some(8.0))).unwrap();
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
        let view = ahead(&set, &look(3400.0, 90.0, Some(8.0))).unwrap();
        assert!(!view.spans[0].in_the_way);
        assert_eq!(view.words, "CTR EAST in 3.0 km: 150 m below your line");

        // Westwards there is nothing.
        let view = ahead(&set, &look(2300.0, 270.0, Some(8.0))).unwrap();
        assert!(view.spans.is_empty());
        assert_eq!(view.words, "No airspace to avoid on this line for 10 km");
        assert_eq!((view.bottom_m, view.top_m), (900.0, 2800.0));

        // A steep line ends far under any ground: the picture does not
        // follow it down.
        let view = ahead(&set, &look(2300.0, 270.0, Some(3.0))).unwrap();
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
        let view = ahead(&set, &inside).unwrap();
        assert_eq!(view.spans[0].from_m, 0.0);
        assert!(view.spans[0].in_the_way);
        assert_eq!(view.words, "In CTR EAST now");

        // A wind the glider makes no way against: no line, and what lies
        // ahead is named with its distance only.
        let view = ahead(&set, &look(2300.0, 90.0, None)).unwrap();
        assert_eq!(view.line_end_m, 2300.0);
        assert!(!view.spans[0].in_the_way);
        assert_eq!(view.words, "CTR EAST in 3.0 km");
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
        let view = ahead(&set, &from_the_zone).unwrap();
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
