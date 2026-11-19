//! Where to fly for an FAI triangle.
//!
//! A triangle is an FAI triangle when its shortest side is at least 28% of
//! its perimeter; cross-country leagues score it higher than any other
//! route. With one side flown, the pilot wants to see where the third
//! corner must lie: the two areas, one on each side of that side, in
//! which a corner makes the triangle an FAI triangle. XCTrack draws them
//! as "FAI triangle areas".
//!
//! The areas are worked out on the FAI sphere like every other distance
//! of the project, and laid out with the plane's law of cosines, which is
//! good to a fraction of a percent at the size of a paraglider's triangle.
//! They are for drawing; whether a flown triangle is an FAI triangle is
//! the scoring's decision, from the distances themselves.

use venti_domain::GeoPoint;

use crate::live::navigation::bearing_deg;

/// The least share of its perimeter that the shortest side of an FAI
/// triangle has.
pub const SHORTEST_SHARE: f64 = 0.28;

/// The largest share a side can then have: the other two at their least.
pub const LONGEST_SHARE: f64 = 1.0 - 2.0 * SHORTEST_SHARE;

/// How many corner points each of an area's three edges is drawn with.
const POINTS_PER_EDGE: usize = 12;

/// The radius of the FAI sphere, metres.
const SPHERE_RADIUS_M: f64 = 6_371_000.0;

/// The areas for one flown side.
#[derive(Debug, Clone, PartialEq)]
pub struct Areas {
    /// The outline of each area, closed: one to the left of the side seen
    /// from its first point to its second, one to the right.
    pub outlines: [Vec<GeoPoint>; 2],
    /// The perimeter of the smallest FAI triangle on this side, metres:
    /// the side is then its longest.
    pub smallest_m: f64,
    /// The perimeter of the largest: the side is then its shortest.
    pub largest_m: f64,
}

/// Whether three corners make an FAI triangle.
pub fn is_fai(a: GeoPoint, b: GeoPoint, c: GeoPoint) -> bool {
    let sides = [a.distance_m(b), b.distance_m(c), c.distance_m(a)];
    let perimeter: f64 = sides.iter().sum();
    let shortest = sides.iter().copied().fold(f64::INFINITY, f64::min);
    perimeter > 0.0 && shortest >= SHORTEST_SHARE * perimeter
}

/// The areas in which a third corner makes an FAI triangle of the side
/// from `a` to `b`. `None` when the two are the same place or an outline
/// cannot be placed.
pub fn areas(a: GeoPoint, b: GeoPoint) -> Option<Areas> {
    let side = a.distance_m(b);
    if !side.is_finite() || side < 1.0 {
        return None;
    }
    let towards_b = bearing_deg(a, b);

    // The corner for given shares of the perimeter: `side_share` for the
    // flown side, `from_a_share` for the side that starts at `a`. The
    // third side has what is left.
    let corner = |side_share: f64, from_a_share: f64, turn: f64| -> Option<GeoPoint> {
        let perimeter = side / side_share;
        let from_a = from_a_share * perimeter;
        let from_b = (1.0 - side_share - from_a_share) * perimeter;
        let cos = (from_a * from_a + side * side - from_b * from_b) / (2.0 * from_a * side);
        let angle = cos.clamp(-1.0, 1.0).acos().to_degrees();
        destination(a, towards_b + turn * angle, from_a)
    };

    let outline = |turn: f64| -> Option<Vec<GeoPoint>> {
        let steps = POINTS_PER_EDGE as f64;
        let between = |from: f64, to: f64, step: usize| from + (to - from) * step as f64 / steps;
        let mut ring = Vec::with_capacity(3 * POINTS_PER_EDGE + 1);
        // Along the edge where the side from `a` is the shortest, from the
        // smallest triangle outwards to the largest.
        for step in 0..POINTS_PER_EDGE {
            let side_share = between(LONGEST_SHARE, SHORTEST_SHARE, step);
            ring.push(corner(side_share, SHORTEST_SHARE, turn)?);
        }
        // Along the far edge, where the flown side is the shortest.
        for step in 0..POINTS_PER_EDGE {
            let from_a_share = between(SHORTEST_SHARE, LONGEST_SHARE, step);
            ring.push(corner(SHORTEST_SHARE, from_a_share, turn)?);
        }
        // And back along the edge where the side from `b` is the shortest.
        for step in 0..POINTS_PER_EDGE {
            let side_share = between(SHORTEST_SHARE, LONGEST_SHARE, step);
            let from_a_share = 1.0 - side_share - SHORTEST_SHARE;
            ring.push(corner(side_share, from_a_share, turn)?);
        }
        ring.push(ring[0]);
        Some(ring)
    };

    Some(Areas {
        outlines: [outline(-1.0)?, outline(1.0)?],
        smallest_m: side / LONGEST_SHARE,
        largest_m: side / SHORTEST_SHARE,
    })
}

/// The point reached from `from` after `distance_m` on the bearing
/// `bearing_deg`, on the FAI sphere.
fn destination(from: GeoPoint, bearing_deg: f64, distance_m: f64) -> Option<GeoPoint> {
    let (lat, lon) = (from.lat().to_radians(), from.lon().to_radians());
    let (bearing, arc) = (bearing_deg.to_radians(), distance_m / SPHERE_RADIUS_M);
    let reached_lat = (lat.sin() * arc.cos() + lat.cos() * arc.sin() * bearing.cos()).asin();
    let reached_lon = lon
        + (bearing.sin() * arc.sin() * lat.cos()).atan2(arc.cos() - lat.sin() * reached_lat.sin());
    let lon_deg = (reached_lon.to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
    GeoPoint::new(reached_lat.to_degrees(), lon_deg).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    /// The shortest side's share of the perimeter.
    fn shortest_share(a: GeoPoint, b: GeoPoint, c: GeoPoint) -> f64 {
        let sides = [a.distance_m(b), b.distance_m(c), c.distance_m(a)];
        sides.iter().copied().fold(f64::INFINITY, f64::min) / sides.iter().sum::<f64>()
    }

    #[test]
    fn every_corner_of_an_outline_makes_a_triangle_that_is_just_an_fai_triangle() {
        // From the Wallberg to the Brauneck: 21 km, a first leg in the Alps.
        let (a, b) = (point(47.664, 11.797), point(47.666, 11.522));
        let side = a.distance_m(b);
        let areas = areas(a, b).unwrap();

        for outline in &areas.outlines {
            assert_eq!(outline.len(), 3 * POINTS_PER_EDGE + 1);
            assert_eq!(outline.first(), outline.last());
            for &corner in outline {
                // On the edge of the rule, to a quarter of a percent of the
                // perimeter: the plane's geometry on a sphere.
                let share = shortest_share(a, b, corner);
                assert!((share - SHORTEST_SHARE).abs() < 0.0025, "{share}");
                let perimeter = side + a.distance_m(corner) + b.distance_m(corner);
                assert!(
                    perimeter > areas.smallest_m * 0.995 && perimeter < areas.largest_m * 1.005
                );
            }
        }
        assert!((areas.smallest_m - side / 0.44).abs() < 1.0);
        assert!((areas.largest_m - side / 0.28).abs() < 1.0);
    }

    #[test]
    fn the_two_areas_lie_on_either_side_and_hold_the_triangles_that_count() {
        let (a, b) = (point(47.664, 11.797), point(47.666, 11.522));
        let areas = areas(a, b).unwrap();
        // The side runs west: the first area is to its left, the south.
        let middle = |outline: &Vec<GeoPoint>| {
            let count = (outline.len() - 1) as f64;
            let (lat, lon) = outline[..outline.len() - 1]
                .iter()
                .fold((0.0, 0.0), |(lat, lon), point| {
                    (lat + point.lat(), lon + point.lon())
                });
            point(lat / count, lon / count)
        };
        let (left, right) = (middle(&areas.outlines[0]), middle(&areas.outlines[1]));
        assert!(
            left.lat() < 47.6 && right.lat() > 47.73,
            "{left:?} {right:?}"
        );
        // A corner in the middle of an area makes an FAI triangle; one on
        // the side itself, or far beyond the area, does not.
        assert!(is_fai(a, b, left) && is_fai(a, b, right));
        assert!(!is_fai(a, b, point(47.665, 11.66)));
        assert!(!is_fai(a, b, point(46.9, 11.66)));
        // An equal-sided triangle is the plainest FAI triangle.
        let apex = destination(a, bearing_deg(a, b) + 60.0, a.distance_m(b)).unwrap();
        assert!(is_fai(a, b, apex));
        assert!((shortest_share(a, b, apex) - 1.0 / 3.0).abs() < 0.002);
    }

    #[test]
    fn a_side_without_length_has_no_areas() {
        let here = point(47.664, 11.797);
        assert_eq!(areas(here, here), None);
        assert!(!is_fai(here, here, here));
    }
}
