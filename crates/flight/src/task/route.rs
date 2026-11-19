//! The shortest way from a point through a row of areas, touching each.
//!
//! A task's length, and how far a pilot still has to go, are both this
//! way's length: from the launch or from where the pilot is, to the edge
//! of each cylinder in turn and on to the goal. There is no formula for
//! it. As in annex A of the Sporting Code's Section 7F, the points on the
//! edges are moved one after the other, each to where it shortens the way
//! between its two neighbours most, until nothing shortens any more.
//!
//! Each point is moved on a flat map centred on its own cylinder, where
//! distances near the centre are true and those to far points nearly so;
//! the length of the result is then measured on the task's earth. The
//! annex leaves the map to the implementer, so a length found here can
//! differ from a scoring program's by metres. To verify against a scored
//! task.

use venti_domain::GeoPoint;

use crate::geodesy::Earth;

/// An area to reach.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    /// A cylinder, seen from above: reached by crossing its edge, from
    /// inside or from outside.
    Cylinder {
        /// Its centre.
        centre: GeoPoint,
        /// Its radius, metres.
        radius_m: f64,
    },
    /// A goal line: reached by crossing it, or by entering the half
    /// circle behind it.
    Line {
        /// The middle of the line.
        centre: GeoPoint,
        /// Half its length, metres.
        half_length_m: f64,
        /// The direction pilots arrive in, degrees clockwise from north.
        /// The line lies across it.
        inbound_deg: f64,
    },
    /// A waypoint of a plain route: flown to, and reached by coming near.
    Point {
        /// The waypoint.
        centre: GeoPoint,
        /// How near counts, metres.
        reach_m: f64,
    },
}

impl Zone {
    /// The centre of the area.
    pub fn centre(&self) -> GeoPoint {
        match *self {
            Zone::Cylinder { centre, .. }
            | Zone::Line { centre, .. }
            | Zone::Point { centre, .. } => centre,
        }
    }
}

/// The shortest way through some areas.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Route {
    /// Where the way touches each area, in their order.
    pub points: Vec<GeoPoint>,
    /// Its length from the starting point to the last area, metres.
    pub length_m: f64,
}

/// The way stops being shortened when a round of moves gains less than
/// this, metres. Section 7F asks for one metre; a centimetre costs little
/// more and keeps the pilot's distance to goal from jumping.
const GAIN_M: f64 = 0.01;
/// And after this many rounds at the latest, as Section 7F sets.
const MAX_ROUNDS: usize = 200;

/// Finds the shortest way from a point through the areas, in their order.
pub fn shortest(earth: Earth, from: GeoPoint, zones: &[Zone]) -> Route {
    let mut points: Vec<GeoPoint> = zones.iter().map(Zone::centre).collect();
    let mut length_m = f64::INFINITY;
    for _ in 0..MAX_ROUNDS {
        for index in 0..zones.len() {
            let before = if index == 0 { from } else { points[index - 1] };
            let after = points.get(index + 1).copied();
            points[index] = touch(earth, &zones[index], before, after);
        }
        let now = length(earth, from, &points);
        let gained = length_m - now;
        length_m = now;
        if gained < GAIN_M {
            break;
        }
    }
    Route { points, length_m }
}

fn length(earth: Earth, from: GeoPoint, points: &[GeoPoint]) -> f64 {
    let mut sum = 0.0;
    let mut last = from;
    for point in points {
        sum += earth.distance_m(last, *point);
        last = *point;
    }
    sum
}

/// A place on the flat map of an area: metres east and north of its
/// centre.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flat {
    east: f64,
    north: f64,
}

impl Flat {
    fn of(earth: Earth, centre: GeoPoint, point: GeoPoint) -> Self {
        let course = earth.course(centre, point);
        let (sin, cos) = course.bearing_deg.to_radians().sin_cos();
        Self {
            east: course.distance_m * sin,
            north: course.distance_m * cos,
        }
    }

    fn at(angle: f64, radius: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            east: radius * sin,
            north: radius * cos,
        }
    }

    fn to(self, other: Flat) -> f64 {
        (self.east - other.east).hypot(self.north - other.north)
    }

    fn length(self) -> f64 {
        self.east.hypot(self.north)
    }

    /// The direction from the centre to this place, radians clockwise
    /// from north.
    fn angle(self) -> f64 {
        self.east.atan2(self.north)
    }

    /// Back onto the earth.
    fn on(self, earth: Earth, centre: GeoPoint) -> GeoPoint {
        earth.destination(centre, self.angle().to_degrees(), self.length())
    }
}

/// Where on an area the way from one point to another, or to the area
/// alone, is shortest.
fn touch(earth: Earth, zone: &Zone, before: GeoPoint, after: Option<GeoPoint>) -> GeoPoint {
    match *zone {
        Zone::Point { centre, .. } => centre,
        Zone::Line {
            centre,
            half_length_m,
            inbound_deg,
        } => {
            // The nearest point of the line to where the pilot comes
            // from. A line is a goal, so nothing comes after it.
            let from = Flat::of(earth, centre, before);
            let along_line = Flat::at((inbound_deg + 90.0).to_radians(), 1.0);
            let offset = (from.east * along_line.east + from.north * along_line.north)
                .clamp(-half_length_m, half_length_m);
            Flat {
                east: along_line.east * offset,
                north: along_line.north * offset,
            }
            .on(earth, centre)
        }
        Zone::Cylinder { centre, radius_m } => {
            let from = Flat::of(earth, centre, before);
            let angle = match after {
                // The last cylinder: its edge where it is nearest.
                None => from.angle(),
                Some(after) => {
                    let to = Flat::of(earth, centre, after);
                    crossing(from, to, radius_m).unwrap_or_else(|| best_angle(from, to, radius_m))
                }
            };
            earth.destination(centre, angle.to_degrees(), radius_m)
        }
    }
}

/// Where the straight line between two places first crosses the circle
/// around the centre, as a direction from the centre. Then the circle
/// costs no detour at all. `None` when the line does not cross it: both
/// places inside, or the line passing outside.
fn crossing(from: Flat, to: Flat, radius: f64) -> Option<f64> {
    let (dx, dy) = (to.east - from.east, to.north - from.north);
    let a = dx * dx + dy * dy;
    if a == 0.0 {
        return None;
    }
    let b = 2.0 * (from.east * dx + from.north * dy);
    let c = from.east * from.east + from.north * from.north - radius * radius;
    let root = (b * b - 4.0 * a * c).sqrt();
    if root.is_nan() {
        return None;
    }
    [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
        .into_iter()
        .find(|along| (0.0..=1.0).contains(along))
        .map(|along| {
            Flat {
                east: from.east + along * dx,
                north: from.north + along * dy,
            }
            .angle()
        })
}

/// The direction from the centre in which a point of the circle makes the
/// way from one place over it to another shortest, when the straight line
/// between them does not cross the circle.
fn best_angle(from: Flat, to: Flat, radius: f64) -> f64 {
    let detour = |angle: f64| {
        let on_edge = Flat::at(angle, radius);
        on_edge.to(from) + on_edge.to(to)
    };
    // Look all the way round first: with both places inside the circle
    // there can be two dips.
    const STEPS: usize = 72;
    let step = std::f64::consts::TAU / STEPS as f64;
    let mut best = 0.0;
    let mut least = f64::INFINITY;
    for index in 0..STEPS {
        let angle = index as f64 * step;
        let length = detour(angle);
        if length < least {
            (best, least) = (angle, length);
        }
    }
    // Then narrow down between the neighbours of the best, by golden
    // section.
    const SHRINK: f64 = 0.618_033_988_749_894_9;
    let (mut low, mut high) = (best - step, best + step);
    let mut inner_low = high - SHRINK * (high - low);
    let mut inner_high = low + SHRINK * (high - low);
    let (mut at_low, mut at_high) = (detour(inner_low), detour(inner_high));
    // To a ten-millionth of a radian: a centimetre on a cylinder of 100 km.
    while high - low > 1e-7 {
        if at_low < at_high {
            high = inner_high;
            inner_high = inner_low;
            at_high = at_low;
            inner_low = high - SHRINK * (high - low);
            at_low = detour(inner_low);
        } else {
            low = inner_low;
            inner_low = inner_high;
            at_low = at_high;
            inner_high = low + SHRINK * (high - low);
            at_high = detour(inner_high);
        }
    }
    (low + high) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::testing::point;

    const EARTHS: [Earth; 2] = [Earth::Wgs84, Earth::FaiSphere];

    fn cylinder(lat: f64, lon: f64, radius_m: f64) -> Zone {
        Zone::Cylinder {
            centre: point(lat, lon),
            radius_m,
        }
    }

    #[test]
    fn the_way_to_one_cylinder_ends_at_its_nearest_edge() {
        for earth in EARTHS {
            let from = point(47.0, 11.0);
            let zone = cylinder(47.0, 11.5, 3_000.0);
            let route = shortest(earth, from, &[zone]);
            let to_centre = earth.distance_m(from, zone.centre());
            assert!(
                (route.length_m - (to_centre - 3_000.0)).abs() < 0.01,
                "{}",
                route.length_m
            );
            assert!((earth.distance_m(zone.centre(), route.points[0]) - 3_000.0).abs() < 0.001);
        }
    }

    #[test]
    fn from_inside_a_cylinder_the_way_leads_out_to_its_edge() {
        let earth = Earth::Wgs84;
        let centre = point(47.0, 11.0);
        // One kilometre east of the centre of a cylinder of five.
        let from = earth.destination(centre, 90.0, 1_000.0);
        let route = shortest(earth, from, &[cylinder(47.0, 11.0, 5_000.0)]);
        assert!(
            (route.length_m - 4_000.0).abs() < 0.01,
            "{}",
            route.length_m
        );
        assert!((earth.course(centre, route.points[0]).bearing_deg - 90.0).abs() < 0.01);
    }

    #[test]
    fn a_cylinder_on_the_straight_line_costs_no_detour() {
        let earth = Earth::Wgs84;
        let from = point(47.0, 11.0);
        let goal = cylinder(47.0, 12.0, 400.0);
        let direct = shortest(earth, from, &[goal]).length_m;
        // A wide cylinder halfway that the straight line runs through.
        let through = shortest(earth, from, &[cylinder(47.02, 11.5, 6_000.0), goal]);
        assert!(
            (through.length_m - direct).abs() < 0.5,
            "{} {direct}",
            through.length_m
        );
    }

    #[test]
    fn a_cylinder_beside_the_line_is_touched_where_the_detour_is_least() {
        let earth = Earth::Wgs84;
        let from = point(47.0, 11.0);
        let turn = cylinder(47.2, 11.5, 2_000.0);
        let goal = cylinder(47.0, 12.0, 1_000.0);
        let route = shortest(earth, from, &[turn, goal]);
        // The turnpoint lies north of the line, so it is touched on its
        // southern side, and the way is shorter than through its centre
        // by more than its radius.
        let touched = earth.course(turn.centre(), route.points[0]).bearing_deg;
        assert!((touched - 180.0).abs() < 15.0, "{touched}");
        let through_centre = earth.distance_m(from, turn.centre())
            + earth.distance_m(turn.centre(), goal.centre())
            - 1_000.0;
        assert!(route.length_m < through_centre - 1_500.0);
        // No other point of the edge does better.
        for bearing in (0..360).step_by(5) {
            let other = earth.destination(turn.centre(), f64::from(bearing), 2_000.0);
            let via = earth.distance_m(from, other) + earth.distance_m(other, route.points[1]);
            assert!(
                via >= route.length_m - 0.05,
                "{bearing}: {via} < {}",
                route.length_m
            );
        }
    }

    #[test]
    fn a_way_out_and_back_to_the_same_cylinder_turns_at_its_near_edge() {
        // Out to a turnpoint and back to goal at the launch: the way
        // turns at the edge that faces the launch.
        let earth = Earth::Wgs84;
        let launch = point(47.0, 11.0);
        let turn = cylinder(47.0, 11.4, 1_000.0);
        let home = cylinder(47.0, 11.0, 400.0);
        let route = shortest(earth, launch, &[turn, home]);
        let out = earth.distance_m(launch, turn.centre()) - 1_000.0;
        assert!(
            (route.length_m - (2.0 * out - 400.0)).abs() < 0.05,
            "{}",
            route.length_m
        );
    }

    #[test]
    fn a_goal_line_is_reached_at_its_nearest_point() {
        let earth = Earth::Wgs84;
        let centre = point(47.0, 11.5);
        let line = Zone::Line {
            centre,
            half_length_m: 200.0,
            inbound_deg: 90.0,
        };
        // Straight in from the west: the middle of the line.
        let west = earth.destination(centre, 270.0, 30_000.0);
        let route = shortest(earth, west, &[line]);
        assert!(earth.distance_m(route.points[0], centre) < 0.01);
        assert!((route.length_m - 30_000.0).abs() < 0.01);
        // From the north-west: its northern end, 200 m from the middle.
        let route = shortest(earth, point(47.3, 11.3), &[line]);
        let end = earth.course(centre, route.points[0]);
        assert!((end.distance_m - 200.0).abs() < 0.01, "{}", end.distance_m);
        assert!(
            end.bearing_deg < 1.0 || end.bearing_deg > 359.0,
            "{}",
            end.bearing_deg
        );
    }

    #[test]
    fn a_plain_route_runs_through_its_waypoints() {
        let earth = Earth::Wgs84;
        let (a, b, c) = (point(47.0, 11.0), point(47.2, 11.3), point(47.1, 11.6));
        let zones = [
            Zone::Point {
                centre: b,
                reach_m: 400.0,
            },
            Zone::Point {
                centre: c,
                reach_m: 400.0,
            },
        ];
        let route = shortest(earth, a, &zones);
        assert_eq!(route.points, vec![b, c]);
        assert!((route.length_m - earth.distance_m(a, b) - earth.distance_m(b, c)).abs() < 1e-6);
    }

    #[test]
    fn no_areas_are_no_way() {
        let route = shortest(Earth::Wgs84, point(47.0, 11.0), &[]);
        assert!(route.points.is_empty());
        assert_eq!(route.length_m, 0.0);
    }
}
