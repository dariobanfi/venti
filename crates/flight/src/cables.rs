//! A cable ahead: whether a power line or a ropeway lies on the course of
//! the next half minute, near enough and low enough to fly into.
//!
//! # The rule
//!
//! A cable is ahead when both hold:
//!
//! - **Across.** Some part of it lies within [`MARGIN_ACROSS_M`] of the
//!   course the glider makes good over the ground, from where it is to
//!   where its track and ground speed take it in [`LOOK_AHEAD_S`]. The
//!   course is a straight line: a glider that circles has the cables
//!   around it within the margin of its short course, and no more.
//! - **Up and down.** The glider is less than [`MARGIN_ABOVE_M`] above the
//!   cable's height above the ground, or below it. The cable's height is
//!   the one its source maps; where none is mapped it is the height
//!   [`assumed_height_m`] gives its kind.
//!
//! The distance is how far along the course the glider first comes within
//! the margin of the cable, measured in steps of [`STEP_M`]: zero when it
//! is within it now.
//!
//! # Its limits
//!
//! - **The cable is taken to follow the ground.** A height above the
//!   ground is compared with the glider's height above the ground beneath
//!   the glider, not beneath the cable. A span across a valley hangs far
//!   higher above the valley floor than it does at its pylons, and a cable
//!   along a slope may be higher or lower beneath the glider than at the
//!   place where its height was mapped. The margin of 100 m is meant to
//!   take some of that in; it cannot take in all of it.
//! - **Where the glider's height above the ground is not known** (no
//!   terrain under it), every cable across the course counts, and the
//!   answer says that the height played no part.
//! - **The data is incomplete.** A cable that the source does not have is
//!   never ahead. No warning is no promise.
//!
//! The numbers are the project's own, chosen to warn half a minute ahead
//! at the speed of a paraglider and to stay quiet above the cables of a
//! valley the glider crosses high; *to verify* in the air.
//!
//! Pure: no clock and no I/O. The same input gives the same answer.

use venti_domain::{CableKind, GeoPoint};

/// How far across the course a cable counts, in metres.
pub const MARGIN_ACROSS_M: f64 = 150.0;

/// How far above a cable's height the glider may be and the cable still
/// counts, in metres.
pub const MARGIN_ABOVE_M: f64 = 100.0;

/// How far ahead the course reaches, in seconds at the present ground
/// speed.
pub const LOOK_AHEAD_S: f64 = 30.0;

/// The steps in which the course is walked, in metres.
pub const STEP_M: f64 = 25.0;

/// Below this ground speed, in metres per second, the glider is taken to
/// stand: only the cables within the margin of where it is count.
const STANDING_M_S: f64 = 1.0;

/// Metres per degree of latitude, for the flat sheet the rule is worked
/// out on: the course and the cables near it lie within a few kilometres.
const METRES_PER_DEGREE: f64 = 111_320.0;

/// How high a cable of a kind is taken to hang above the ground, in
/// metres, where its source maps no height. Each is a common height for
/// the kind, on the high side, so that a glider is warned rather than not:
/// a power line of 110 kV hangs 20 to 40 m up, a material ropeway across a
/// valley far higher than any default. *To verify* against the DFS's
/// obstacle data and Tirol's air-sport cables.
pub fn assumed_height_m(kind: CableKind) -> f64 {
    match kind {
        CableKind::PowerLine => 40.0,
        CableKind::MinorPowerLine => 15.0,
        CableKind::CableCar | CableKind::MaterialRopeway => 100.0,
        CableKind::Gondola | CableKind::ZipLine | CableKind::Other => 50.0,
        CableKind::ChairLift => 20.0,
        CableKind::DragLift => 10.0,
    }
}

/// One cable as the rule reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wire<'a> {
    /// Its course, from pylon to pylon.
    pub line: &'a [GeoPoint],
    /// What it is.
    pub kind: CableKind,
    /// Its height above the ground, where the source maps one.
    pub height_m: Option<f64>,
}

impl Wire<'_> {
    /// The height the rule takes: the mapped one, else the kind's.
    pub fn height_m(&self) -> f64 {
        self.height_m.unwrap_or_else(|| assumed_height_m(self.kind))
    }
}

/// Where the glider is and where it goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glider {
    /// Its position.
    pub position: GeoPoint,
    /// Its height above the ground beneath it, in metres, if known.
    pub height_above_ground_m: Option<f64>,
    /// The direction of its course over the ground, degrees clockwise
    /// from north.
    pub track_deg: f64,
    /// Its speed over the ground, in metres per second.
    pub ground_speed_m_s: f64,
}

/// A cable that lies ahead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CableAhead {
    /// Which of the wires it is: its place in the list given.
    pub index: usize,
    /// How far along the course the glider comes within the margin of it,
    /// in metres; zero when it is within it now.
    pub distance_m: f64,
    /// How far the glider is above the height the rule takes for the
    /// cable, in metres: negative when it is below. `None` when the
    /// glider's height above the ground is not known.
    pub above_m: Option<f64>,
    /// Whether the cable's height is its source's, not the kind's.
    pub height_mapped: bool,
}

/// The nearest cable ahead of the glider by the rule, if any: the one it
/// comes within the margin of first, and of two at the same distance the
/// one it is lower above.
pub fn cable_ahead(glider: &Glider, wires: &[Wire<'_>]) -> Option<CableAhead> {
    if !glider.position.lat().is_finite() {
        return None;
    }
    let reach_m = if glider.ground_speed_m_s.is_finite() && glider.ground_speed_m_s >= STANDING_M_S
    {
        glider.ground_speed_m_s * LOOK_AHEAD_S
    } else {
        0.0
    };
    let track = glider.track_deg.to_radians();
    let (east, north) = (track.sin(), track.cos());
    let sheet = Sheet::at(glider.position);
    let steps = (reach_m / STEP_M).ceil() as usize;
    let course: Vec<(f64, (f64, f64))> = (0..=steps)
        .map(|step| {
            let along = (step as f64 * STEP_M).min(reach_m);
            (along, (east * along, north * along))
        })
        .collect();
    // A cable whose every point is farther than this from the glider
    // cannot come within the margin of the course.
    let far_m = reach_m + MARGIN_ACROSS_M;

    let mut nearest: Option<CableAhead> = None;
    for (index, wire) in wires.iter().enumerate() {
        if wire.line.len() < 2 {
            continue;
        }
        let above_m = glider
            .height_above_ground_m
            .map(|height| height - wire.height_m());
        if above_m.is_some_and(|above| above >= MARGIN_ABOVE_M) {
            continue;
        }
        let points: Vec<(f64, f64)> = wire.line.iter().map(|p| sheet.flat(*p)).collect();
        let near = points
            .windows(2)
            .any(|span| distance_to_span((0.0, 0.0), span[0], span[1]) <= far_m);
        if !near {
            continue;
        }
        let first = course.iter().find(|(_, at)| {
            points
                .windows(2)
                .any(|span| distance_to_span(*at, span[0], span[1]) <= MARGIN_ACROSS_M)
        });
        let Some((distance_m, _)) = first else {
            continue;
        };
        let found = CableAhead {
            index,
            distance_m: *distance_m,
            above_m,
            height_mapped: wire.height_m.is_some(),
        };
        let closer = nearest.is_none_or(|best| {
            found.distance_m < best.distance_m
                || (found.distance_m == best.distance_m
                    && found.above_m.unwrap_or(f64::MIN) < best.above_m.unwrap_or(f64::MIN))
        });
        if closer {
            nearest = Some(found);
        }
    }
    nearest
}

/// A flat sheet in metres east and north of a point.
struct Sheet {
    origin: GeoPoint,
    east_per_degree: f64,
}

impl Sheet {
    fn at(origin: GeoPoint) -> Self {
        Self {
            origin,
            east_per_degree: origin.lat().to_radians().cos() * METRES_PER_DEGREE,
        }
    }

    fn flat(&self, point: GeoPoint) -> (f64, f64) {
        (
            (point.lon() - self.origin.lon()) * self.east_per_degree,
            (point.lat() - self.origin.lat()) * METRES_PER_DEGREE,
        )
    }
}

/// How far a point lies from a straight span between two others.
fn distance_to_span(point: (f64, f64), from: (f64, f64), to: (f64, f64)) -> f64 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length2 = dx * dx + dy * dy;
    let along = if length2 == 0.0 {
        0.0
    } else {
        (((point.0 - from.0) * dx + (point.1 - from.1) * dy) / length2).clamp(0.0, 1.0)
    };
    (point.0 - from.0 - along * dx).hypot(point.1 - from.1 - along * dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAT: f64 = 47.7;
    const LON: f64 = 11.75;

    /// A point so many metres east and north of the test's origin.
    fn at(east_m: f64, north_m: f64) -> GeoPoint {
        let east_per_degree = LAT.to_radians().cos() * METRES_PER_DEGREE;
        GeoPoint::new(
            LAT + north_m / METRES_PER_DEGREE,
            LON + east_m / east_per_degree,
        )
        .unwrap()
    }

    /// A glider at the origin flying north at 10 m/s, 80 m above the
    /// ground: it looks 300 m ahead.
    fn glider() -> Glider {
        Glider {
            position: at(0.0, 0.0),
            height_above_ground_m: Some(80.0),
            track_deg: 0.0,
            ground_speed_m_s: 10.0,
        }
    }

    /// A power line from west to east, so many metres north.
    fn across(north_m: f64) -> [GeoPoint; 2] {
        [at(-2000.0, north_m), at(2000.0, north_m)]
    }

    fn wire(line: &[GeoPoint], height_m: Option<f64>) -> Wire<'_> {
        Wire {
            line,
            kind: CableKind::PowerLine,
            height_m,
        }
    }

    #[test]
    fn a_line_across_the_course_is_ahead_at_its_distance() {
        let line = across(400.0);
        let found = cable_ahead(&glider(), &[wire(&line, None)]).unwrap();
        // The course reaches 300 m, the margin 150 more: the glider comes
        // within 150 m of the line after 250 m.
        assert_eq!(found.distance_m, 250.0);
        // 80 m above the ground, the line taken at 40 m.
        assert_eq!(found.above_m, Some(40.0));
        assert!(!found.height_mapped);
    }

    #[test]
    fn a_line_beyond_the_half_minute_is_not_ahead() {
        let line = across(460.0);
        assert_eq!(cable_ahead(&glider(), &[wire(&line, None)]), None);
    }

    #[test]
    fn a_line_far_below_is_not_ahead_and_a_line_above_is() {
        let line = across(200.0);
        // 80 m above the ground and the line at 40 m: 40 m above it.
        // Higher by 100 m or more above the line and it does not count.
        let high = Glider {
            height_above_ground_m: Some(140.0),
            ..glider()
        };
        assert_eq!(cable_ahead(&high, &[wire(&line, None)]), None);
        let just = Glider {
            height_above_ground_m: Some(139.0),
            ..glider()
        };
        assert!(cable_ahead(&just, &[wire(&line, None)]).is_some());
        // A ropeway mapped 150 m up, with the glider below it.
        let found = cable_ahead(&glider(), &[wire(&line, Some(150.0))]).unwrap();
        assert_eq!(found.above_m, Some(-70.0));
        assert!(found.height_mapped);
    }

    #[test]
    fn a_line_beside_the_course_counts_within_the_margin() {
        // A line along the course, 140 m and 160 m to the east.
        let near = [at(140.0, -500.0), at(140.0, 500.0)];
        let far = [at(160.0, -500.0), at(160.0, 500.0)];
        let found = cable_ahead(&glider(), &[wire(&near, None)]).unwrap();
        assert_eq!(found.distance_m, 0.0);
        assert_eq!(cable_ahead(&glider(), &[wire(&far, None)]), None);
    }

    #[test]
    fn without_the_height_above_the_ground_every_line_on_the_course_counts() {
        let line = across(200.0);
        let blind = Glider {
            height_above_ground_m: None,
            ..glider()
        };
        let found = cable_ahead(&blind, &[wire(&line, None)]).unwrap();
        assert_eq!(found.above_m, None);
    }

    #[test]
    fn the_nearest_of_several_is_the_one() {
        let one = across(380.0);
        let two = across(190.0);
        let found = cable_ahead(&glider(), &[wire(&one, None), wire(&two, None)]).unwrap();
        assert_eq!(found.index, 1);
        assert_eq!(found.distance_m, 50.0);
    }

    #[test]
    fn a_glider_that_stands_sees_only_what_is_around_it() {
        let line = across(200.0);
        let standing = Glider {
            ground_speed_m_s: 0.5,
            ..glider()
        };
        assert_eq!(cable_ahead(&standing, &[wire(&line, None)]), None);
        let close = across(100.0);
        assert_eq!(
            cable_ahead(&standing, &[wire(&close, None)]).map(|found| found.distance_m),
            Some(0.0)
        );
    }

    #[test]
    fn the_course_follows_the_track() {
        // A line running north to south, 300 m east. Flying east it is
        // ahead; flying west it is not.
        let line = [at(300.0, -1000.0), at(300.0, 1000.0)];
        let east = Glider {
            track_deg: 90.0,
            ..glider()
        };
        let west = Glider {
            track_deg: 270.0,
            ..glider()
        };
        assert_eq!(
            cable_ahead(&east, &[wire(&line, None)]).map(|found| found.distance_m),
            Some(150.0)
        );
        assert_eq!(cable_ahead(&west, &[wire(&line, None)]), None);
    }

    #[test]
    fn each_kind_has_a_height() {
        assert_eq!(assumed_height_m(CableKind::PowerLine), 40.0);
        assert_eq!(assumed_height_m(CableKind::MinorPowerLine), 15.0);
        assert_eq!(assumed_height_m(CableKind::MaterialRopeway), 100.0);
    }
}
