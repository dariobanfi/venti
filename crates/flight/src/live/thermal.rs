//! Where the lift is in the thermal the glider is circling in.
//!
//! A thermal moves with the wind, so the circles a glider flies in it are
//! not circles over the ground: seen on a map they are loops that trail off
//! downwind. The picture here undoes that. Each place the glider flew
//! through is moved to where that air is now, which turns the loops back
//! into the circles the pilot flew, and on those circles it shows where the
//! climb was stronger.
//!
//! That thermals are followed by moving past positions with the wind is how
//! gliding computers have long done it; XCSoar's thermal locator
//! (GPL-2.0-or-later) is the one read for this. It takes the centre as the
//! average of those positions weighted by lift. This module fits the circle
//! flown and the lean of the climb around it instead, which gives the same
//! answer for whole circles and does not lean toward the glider when the
//! circles are not whole. The code is the project's own.

use venti_domain::GeoPoint;

use super::computer::TrailPoint;
use super::wind::{Wind, solve};

/// How far back the picture looks, seconds: about two circles. Older air
/// has had time to change, and an error in the wind moves a place further
/// the older it is.
pub const LOOK_BACK_S: f64 = 50.0;

/// The climb shown at a place is the mean over this many seconds before
/// and after it: a fifth of a circle in all. A vario's reading jumps about
/// from second to second, most of all one worked out from a recorder's
/// whole metres, and a dot on a map should not. Where the lift is, is
/// worked out from the readings as they came.
const SMOOTH_S: f64 = 2.0;

/// The fewest places a circle is fitted to.
const MIN_POINTS: usize = 12;

/// Seen from the middle of the fitted circle, the places are sorted into
/// this many sectors, and a centre is given only when they lie in at least
/// three quarters of them: less than that is an arc, which fits circles of
/// many sizes.
const SECTORS: usize = 12;
const SECTORS_NEEDED: usize = 9;

/// A paraglider's circles have a radius of 15 to 60 m. A fit far outside
/// that is not a circle flown in a thermal.
const RADIUS_M: std::ops::RangeInclusive<f64> = 8.0..=250.0;

/// The places may lie this far from the fitted circle, root mean square,
/// as a share of its radius. Loops left by a wrong wind, or a search
/// pattern that is no circle, lie further.
const MISFIT_SHARE: f64 = 0.35;

/// A climb that is this much stronger on one side of the circle than on
/// average, metres per second, puts the middle of the lift a full radius
/// that way. In a thermal that weakens evenly from its core the true
/// distance is the lean times the thermal's width squared, over twice its
/// strength and the circle's radius: half a radius for each metre per
/// second in a narrow strong thermal, three in a wide weak one. One radius
/// is between them and is as far as a pilot moves a circle at a time. To
/// verify in the air.
const FULL_SHIFT_MS: f64 = 1.0;

/// One place the glider flew through in the thermal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftPoint {
    /// Where the air it flew through there is now.
    pub position: GeoPoint,
    /// The climb rate around there, metres per second: the mean over the
    /// two seconds before and after.
    pub climb_ms: f64,
    /// How long ago that was, seconds.
    pub age_s: f64,
}

/// The middle of the lift, as far as the last circles show it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftCentre {
    /// Where to centre the next circle: the middle of the circles flown,
    /// moved toward the side where the climb was stronger.
    pub position: GeoPoint,
    /// The middle of the circles flown, where that air is now.
    pub circle: GeoPoint,
    /// The radius of those circles, metres.
    pub radius_m: f64,
    /// How far `position` lies from `circle`, metres. Never more than the
    /// radius.
    pub shift_m: f64,
    /// The direction from `circle` to `position`, degrees clockwise from
    /// north.
    pub toward_deg: f64,
    /// The climb averaged around the circle, metres per second.
    pub climb_ms: f64,
    /// How much stronger the climb is on the best side of the circle than
    /// on average, metres per second.
    pub stronger_by_ms: f64,
}

/// The thermal as the last circles show it.
#[derive(Debug, Clone, PartialEq)]
pub struct ThermalPicture {
    /// The places flown through, oldest first, each where its air is now.
    pub points: Vec<LiftPoint>,
    /// The middle of the lift. `None` until most of a circle is flown, and
    /// while what was flown is no circle.
    pub centre: Option<LiftCentre>,
    /// Whether the places were moved with a wind. Without one they are
    /// where the glider was over the ground.
    pub drifted: bool,
}

/// The picture at `now_s`, from the track flown since `since_s` and no
/// further back than [`LOOK_BACK_S`]. `wind` moves the places; `None` if
/// nothing was flown in that time.
///
/// The climb at a place is the climb the vario showed there. A vario runs
/// about a second behind the air, and so does a position: the two are
/// taken to cancel.
pub fn picture(
    trail: impl IntoIterator<Item = TrailPoint>,
    since_s: f64,
    now_s: f64,
    wind: Option<Wind>,
) -> Option<ThermalPicture> {
    let from = since_s.max(now_s - LOOK_BACK_S);
    let flown: Vec<TrailPoint> = trail
        .into_iter()
        .filter(|point| point.time_s >= from && point.time_s <= now_s)
        .collect();
    let map = FlatMap::around(flown.last()?.position);
    // Where the air goes, east and north, metres per second.
    let drift = wind.map_or((0.0, 0.0), |wind| {
        let (sin, cos) = wind.from_deg.to_radians().sin_cos();
        (-wind.speed_ms * sin, -wind.speed_ms * cos)
    });
    let placed: Vec<Placed> = flown
        .iter()
        .map(|point| {
            let age_s = now_s - point.time_s;
            let (east, north) = map.metres(point.position);
            Placed {
                east: east + drift.0 * age_s,
                north: north + drift.1 * age_s,
                climb_ms: point.climb_ms,
                age_s,
            }
        })
        .collect();
    Some(ThermalPicture {
        points: placed
            .iter()
            .filter_map(|place| {
                let near = || {
                    placed
                        .iter()
                        .filter(|other| (other.age_s - place.age_s).abs() <= SMOOTH_S)
                };
                Some(LiftPoint {
                    position: map.point(place.east, place.north)?,
                    climb_ms: near().map(|other| other.climb_ms).sum::<f64>()
                        / near().count() as f64,
                    age_s: place.age_s,
                })
            })
            .collect(),
        centre: centre(&map, &placed),
        drifted: wind.is_some(),
    })
}

/// A place on the flat map, metres east and north of its middle.
#[derive(Debug, Clone, Copy)]
struct Placed {
    east: f64,
    north: f64,
    climb_ms: f64,
    age_s: f64,
}

fn centre(map: &FlatMap, placed: &[Placed]) -> Option<LiftCentre> {
    if placed.len() < MIN_POINTS {
        return None;
    }

    // The circle through the places, by least squares: find D, E, F
    // minimising the sum of (x² + y² + Dx + Ey + F)², which is linear in
    // the three.
    let n = placed.len() as f64;
    let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sxz, mut syz, mut sz) = (0.0, 0.0, 0.0);
    for place in placed {
        let (x, y) = (place.east, place.north);
        let z = x * x + y * y;
        sx += x;
        sy += y;
        sxx += x * x;
        syy += y * y;
        sxy += x * y;
        sxz += x * z;
        syz += y * z;
        sz += z;
    }
    let [d, e, f] = solve(
        [[sxx, sxy, sx], [sxy, syy, sy], [sx, sy, n]],
        [-sxz, -syz, -sz],
    )?;
    let (centre_east, centre_north) = (-d / 2.0, -e / 2.0);
    let radius = (centre_east * centre_east + centre_north * centre_north - f).sqrt();
    if !RADIUS_M.contains(&radius) {
        return None;
    }

    // Each place as a direction from the middle, and how far it lies from
    // the circle.
    let mut covered = [false; SECTORS];
    let mut misfit = 0.0;
    let mut directions = Vec::with_capacity(placed.len());
    for place in placed {
        let (east, north) = (place.east - centre_east, place.north - centre_north);
        let distance = east.hypot(north);
        misfit += (distance - radius).powi(2);
        if distance > 0.0 {
            let bearing = east.atan2(north).to_degrees().rem_euclid(360.0);
            covered[(bearing / (360.0 / SECTORS as f64)) as usize % SECTORS] = true;
            directions.push((east / distance, north / distance, place.climb_ms));
        }
    }
    if (misfit / n).sqrt() > MISFIT_SHARE * radius
        || covered.iter().filter(|&&sector| sector).count() < SECTORS_NEEDED
    {
        return None;
    }

    // The climb around the circle as an average and a lean: the climb at a
    // place is taken as mean + lean · direction, and the three numbers are
    // found by least squares. The lean points to the side where the climb
    // is stronger, and its length is by how much.
    let count = directions.len() as f64;
    let (mut su, mut sv, mut suu, mut svv, mut suv) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sc, mut scu, mut scv) = (0.0, 0.0, 0.0);
    for &(u, v, climb) in &directions {
        su += u;
        sv += v;
        suu += u * u;
        svv += v * v;
        suv += u * v;
        sc += climb;
        scu += climb * u;
        scv += climb * v;
    }
    let [mean, lean_east, lean_north] = solve(
        [[count, su, sv], [su, suu, suv], [sv, suv, svv]],
        [sc, scu, scv],
    )?;
    let stronger_by_ms = lean_east.hypot(lean_north);
    let shift_m = radius * (stronger_by_ms / FULL_SHIFT_MS).min(1.0);
    let (toward_east, toward_north) = if stronger_by_ms > 0.0 {
        (lean_east / stronger_by_ms, lean_north / stronger_by_ms)
    } else {
        (0.0, 0.0)
    };

    Some(LiftCentre {
        position: map.point(
            centre_east + toward_east * shift_m,
            centre_north + toward_north * shift_m,
        )?,
        circle: map.point(centre_east, centre_north)?,
        radius_m: radius,
        shift_m,
        toward_deg: lean_east.atan2(lean_north).to_degrees().rem_euclid(360.0),
        climb_ms: mean,
        stronger_by_ms,
    })
}

/// Metres per degree of latitude on a sphere of the earth's mean radius.
const M_PER_DEG: f64 = 111_194.93;

/// The few hundred metres around one point as a flat map: east and north
/// of it in metres. Over that distance the earth's curve is millimetres.
#[derive(Debug, Clone, Copy)]
struct FlatMap {
    middle: GeoPoint,
    m_per_deg_lon: f64,
}

impl FlatMap {
    fn around(middle: GeoPoint) -> Self {
        Self {
            middle,
            m_per_deg_lon: M_PER_DEG * middle.lat().to_radians().cos(),
        }
    }

    fn metres(&self, point: GeoPoint) -> (f64, f64) {
        (
            (point.lon() - self.middle.lon()) * self.m_per_deg_lon,
            (point.lat() - self.middle.lat()) * M_PER_DEG,
        )
    }

    fn point(&self, east: f64, north: f64) -> Option<GeoPoint> {
        GeoPoint::new(
            self.middle.lat() + north / M_PER_DEG,
            self.middle.lon() + east / self.m_per_deg_lon,
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;

    const RADIUS: f64 = 40.0;
    const PERIOD_S: f64 = 25.0;

    /// A glider circling in air that moves with `wind`, one place a second
    /// from second 0 to `seconds`. The circle's middle is over the map's
    /// middle at second 0; `climb` gives the climb from the direction the
    /// glider lies in from the circle's middle, as east and north parts.
    fn circling(
        seconds: u32,
        wind: Option<Wind>,
        climb: impl Fn(f64, f64) -> f64,
    ) -> Vec<TrailPoint> {
        let map = FlatMap::around(GeoPoint::new(47.5, 11.0).unwrap());
        let drift = wind.map_or((0.0, 0.0), |wind| {
            let (sin, cos) = wind.from_deg.to_radians().sin_cos();
            (-wind.speed_ms * sin, -wind.speed_ms * cos)
        });
        (0..=seconds)
            .map(|second| {
                let time = f64::from(second);
                let (east, north) = (TAU * time / PERIOD_S).sin_cos();
                TrailPoint {
                    time_s: time,
                    position: map
                        .point(
                            drift.0 * time + RADIUS * east,
                            drift.1 * time + RADIUS * north,
                        )
                        .unwrap(),
                    altitude_m: 1500.0 + 2.0 * time,
                    climb_ms: climb(east, north),
                }
            })
            .collect()
    }

    /// Where a place lies from the map's middle, east and north in metres.
    fn metres(point: GeoPoint) -> (f64, f64) {
        FlatMap::around(GeoPoint::new(47.5, 11.0).unwrap()).metres(point)
    }

    #[test]
    fn even_lift_puts_the_centre_in_the_middle_of_the_circle() {
        let trail = circling(60, None, |_, _| 2.0);
        let picture = picture(trail, 0.0, 60.0, None).unwrap();
        assert!(!picture.drifted);
        // The last fifty seconds, one place a second.
        assert_eq!(picture.points.len(), 51);
        let centre = picture.centre.unwrap();
        assert!((centre.radius_m - RADIUS).abs() < 0.5, "{centre:?}");
        assert!(centre.shift_m < 0.5, "{centre:?}");
        assert!((centre.climb_ms - 2.0).abs() < 0.01, "{centre:?}");
        let (east, north) = metres(centre.position);
        assert!(east.hypot(north) < 1.0, "{centre:?}");
    }

    #[test]
    fn the_centre_moves_toward_the_stronger_climb() {
        // Half a metre per second more on the east side, as much less on
        // the west.
        let trail = circling(60, None, |east, _| 2.0 + 0.5 * east);
        let centre = picture(trail, 0.0, 60.0, None).unwrap().centre.unwrap();
        assert!((centre.toward_deg - 90.0).abs() < 2.0, "{centre:?}");
        assert!((centre.stronger_by_ms - 0.5).abs() < 0.02, "{centre:?}");
        assert!((centre.shift_m - RADIUS / 2.0).abs() < 1.0, "{centre:?}");
        assert!((centre.climb_ms - 2.0).abs() < 0.02, "{centre:?}");
        let (east, north) = metres(centre.position);
        assert!(
            (east - RADIUS / 2.0).abs() < 1.5 && north.abs() < 1.5,
            "{centre:?}"
        );
        let (east, north) = metres(centre.circle);
        assert!(east.hypot(north) < 1.0, "{centre:?}");
    }

    #[test]
    fn a_circle_and_a_half_does_not_pull_the_centre_toward_the_glider() {
        // Thirty-seven seconds are a circle and a half. An average of the
        // places would lie a fifth of a radius toward the half flown twice.
        let trail = circling(37, None, |_, _| 2.0);
        let centre = picture(trail, 0.0, 37.0, None).unwrap().centre.unwrap();
        let (east, north) = metres(centre.position);
        assert!(east.hypot(north) < 1.0, "{centre:?}");
    }

    #[test]
    fn the_lift_never_lies_more_than_a_radius_away() {
        let trail = circling(60, None, |_, north| 1.0 - 3.0 * north);
        let centre = picture(trail, 0.0, 60.0, None).unwrap().centre.unwrap();
        assert!((centre.toward_deg - 180.0).abs() < 2.0, "{centre:?}");
        assert!((centre.shift_m - RADIUS).abs() < 0.5, "{centre:?}");
    }

    #[test]
    fn the_wind_turns_the_loops_over_the_ground_back_into_circles() {
        // Six metres per second from the west carry the thermal 360 m east
        // in a minute, nine times the radius of the circle.
        let wind = Wind {
            from_deg: 270.0,
            speed_ms: 6.0,
        };
        let trail = circling(60, Some(wind), |_, north| 2.0 + 0.4 * north);
        let picture_ = picture(trail.clone(), 0.0, 60.0, Some(wind)).unwrap();
        assert!(picture_.drifted);
        let centre = picture_.centre.unwrap();
        assert!((centre.radius_m - RADIUS).abs() < 0.5, "{centre:?}");
        // The circle's middle is where the air it began in is now.
        let (east, north) = metres(centre.circle);
        assert!(
            (east - 360.0).abs() < 1.5 && north.abs() < 1.5,
            "{centre:?}"
        );
        assert!(
            centre.toward_deg < 2.0 || centre.toward_deg > 358.0,
            "{centre:?}"
        );
        assert!((centre.shift_m - 0.4 * RADIUS).abs() < 1.0, "{centre:?}");
        // Every place lies on that circle, the oldest too.
        for point in &picture_.points {
            let (point_east, point_north) = metres(point.position);
            let distance = (point_east - east).hypot(point_north - north);
            assert!((distance - RADIUS).abs() < 1.5, "{point:?}");
        }

        // Without the wind the same track is a line of loops, and no
        // centre is claimed for it.
        let unmoved = picture(trail, 0.0, 60.0, None).unwrap();
        assert!(unmoved.centre.is_none());
        assert_eq!(unmoved.points.len(), 51);
    }

    #[test]
    fn the_climb_shown_at_a_place_is_smoothed_and_the_centre_is_not() {
        // A vario that reads half a metre per second high and low by turns
        // around 2 m/s, with the east side really better by 0.5.
        let trail = circling(60, None, |east, _| 2.0 + 0.5 * east);
        let jumpy: Vec<TrailPoint> = trail
            .iter()
            .enumerate()
            .map(|(index, &point)| TrailPoint {
                climb_ms: point.climb_ms + if index % 2 == 0 { 0.5 } else { -0.5 },
                ..point
            })
            .collect();
        let (steady, jumpy) = (
            picture(trail, 0.0, 60.0, None).unwrap(),
            picture(jumpy, 0.0, 60.0, None).unwrap(),
        );
        // The dots hardly notice: five readings in a row hold three of one
        // kind and two of the other.
        for (calm, shown) in steady.points.iter().zip(&jumpy.points).skip(2).take(47) {
            assert!((calm.climb_ms - shown.climb_ms).abs() < 0.11, "{shown:?}");
        }
        // And the lift is found where it is.
        let centre = jumpy.centre.unwrap();
        assert!((centre.toward_deg - 90.0).abs() < 4.0, "{centre:?}");
        assert!((centre.stronger_by_ms - 0.5).abs() < 0.05, "{centre:?}");
    }

    #[test]
    fn half_a_circle_gives_places_and_no_centre() {
        let trail = circling(14, None, |_, _| 2.0);
        let picture = picture(trail, 0.0, 14.0, None).unwrap();
        assert_eq!(picture.points.len(), 15);
        assert!(picture.centre.is_none());
    }

    #[test]
    fn nothing_flown_gives_no_picture() {
        let trail = circling(30, None, |_, _| 2.0);
        // Asked two minutes later: all of it is too old.
        assert!(picture(trail, 0.0, 150.0, None).is_none());
        assert!(picture(Vec::new(), 0.0, 10.0, None).is_none());
    }

    #[test]
    fn only_what_was_flown_since_the_thermal_began_counts() {
        let trail = circling(60, None, |_, _| 2.0);
        let picture = picture(trail, 40.0, 60.0, None).unwrap();
        assert_eq!(picture.points.len(), 21);
        assert!(picture.points.iter().all(|point| point.age_s <= 20.0));
    }
}
