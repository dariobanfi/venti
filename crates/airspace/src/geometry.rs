//! Geometry on the globe and in the plane of longitude and latitude.
//!
//! Two models are used, each where it is exact enough:
//!
//! - **Edges are straight in longitude and latitude.** That is how the
//!   GeoJSON standard (RFC 7946, section 3.1.1) defines the line between two
//!   positions, and it is what a map draws. Whether a point lies inside a
//!   ring, and where a path crosses one, is decided in that plane with no
//!   projection at all.
//! - **Distances are measured on the FAI sphere**, like everywhere else in
//!   this project. The nearest point of a boundary is found in a flat map
//!   laid out around the pilot, and the distance to it is then taken on the
//!   sphere. Within 50 km of the pilot the flat map distorts by less than
//!   half a percent.

use venti_domain::{GeoPoint, Polygon};

/// Radius of the FAI sphere in metres, as used by [`GeoPoint::distance_m`].
pub(crate) const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Metres in one nautical mile, exactly. OpenAir gives radii in nautical miles.
pub const METRES_PER_NAUTICAL_MILE: f64 = 1852.0;

/// Metres per degree of latitude on the FAI sphere.
pub(crate) const M_PER_DEG: f64 = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

/// The farthest a polygon made from a circle or an arc may lie from the true
/// curve, in metres. The error is always on the side that makes the airspace
/// larger: see [`circle_ring`].
pub const ARC_MAX_ERROR_M: f64 = 10.0;

/// The point reached from `from` after `distance_m` along the great circle
/// that leaves it on `bearing_deg`.
///
/// `None` when the inputs are not finite. Near a pole the longitude of the
/// result is arbitrary but valid.
pub fn destination(from: GeoPoint, bearing_deg: f64, distance_m: f64) -> Option<GeoPoint> {
    if !bearing_deg.is_finite() || !distance_m.is_finite() {
        return None;
    }
    let angular = distance_m / EARTH_RADIUS_M;
    let bearing = bearing_deg.to_radians();
    let lat1 = from.lat().to_radians();
    let sin_lat2 = lat1.sin() * angular.cos() + lat1.cos() * angular.sin() * bearing.cos();
    let lat2 = sin_lat2.clamp(-1.0, 1.0).asin();
    let lon2 = from.lon().to_radians()
        + (bearing.sin() * angular.sin() * lat1.cos())
            .atan2(angular.cos() - lat1.sin() * lat2.sin());
    let lon = (lon2.to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
    GeoPoint::new(lat2.to_degrees().clamp(-90.0, 90.0), lon).ok()
}

/// The direction in which the great circle from `from` to `to` sets out, in
/// degrees clockwise from true north.
pub fn bearing_deg(from: GeoPoint, to: GeoPoint) -> f64 {
    let (lat1, lat2) = (from.lat().to_radians(), to.lat().to_radians());
    let dlon = (to.lon() - from.lon()).to_radians();
    let east = dlon.sin() * lat2.cos();
    let north = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    east.atan2(north).to_degrees().rem_euclid(360.0)
}

/// How many straight pieces a sweep of `sweep_deg` around a circle of
/// `radius_m` needs so that neither a polygon through points on the circle
/// nor one touching it from outside strays more than [`ARC_MAX_ERROR_M`].
fn arc_steps(radius_m: f64, sweep_deg: f64) -> usize {
    // A polygon touching the circle from outside has its corners at
    // radius / cos(step / 2). Keeping them within the allowed error bounds
    // the step; the same step keeps a polygon through points on the circle
    // within it too.
    let half_step = (radius_m / (radius_m + ARC_MAX_ERROR_M)).acos();
    let largest_step = (2.0 * half_step.to_degrees()).min(45.0);
    ((sweep_deg.abs() / largest_step).ceil() as usize).max(1)
}

/// A closed ring around a circle.
///
/// The ring touches the circle from outside: every point of the circle lies
/// inside it, and no point of the ring is more than [`ARC_MAX_ERROR_M`]
/// outside the circle. A pilot the ring reports as outside is therefore
/// outside the true circle.
///
/// `None` when the radius is not a positive finite number or a point of the
/// ring cannot be placed.
pub fn circle_ring(centre: GeoPoint, radius_m: f64) -> Option<Vec<GeoPoint>> {
    if !radius_m.is_finite() || radius_m <= 0.0 {
        return None;
    }
    let steps = arc_steps(radius_m, 360.0).max(8);
    let step = 360.0 / steps as f64;
    let corner_radius = radius_m / (step / 2.0).to_radians().cos();
    let mut ring = Vec::with_capacity(steps + 1);
    for index in 0..steps {
        ring.push(destination(centre, index as f64 * step, corner_radius)?);
    }
    ring.push(ring[0]);
    Some(ring)
}

/// One arc of a boundary: around `centre`, from the bearing `start_deg`
/// through `sweep_deg` degrees, clockwise when the sweep is positive. The
/// radius goes evenly from `start_radius_m` to `end_radius_m`, which differ
/// when a file gives an arc by two end points that are not quite the same
/// distance from its centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Arc {
    pub centre: GeoPoint,
    pub start_deg: f64,
    pub sweep_deg: f64,
    pub start_radius_m: f64,
    pub end_radius_m: f64,
}

impl Arc {
    /// The points strictly between the two ends of the arc.
    ///
    /// With `outside` they are the corners of a polygon that touches the arc
    /// from outside its circle; without, they lie on the arc, so the straight
    /// pieces between them run inside the circle. The caller picks whichever
    /// makes the airspace larger. Either way the curve is at most
    /// [`ARC_MAX_ERROR_M`] away.
    pub(crate) fn between(&self, outside: bool) -> Option<Vec<GeoPoint>> {
        let radius = self.start_radius_m.max(self.end_radius_m);
        let steps = arc_steps(radius, self.sweep_deg);
        let step = self.sweep_deg / steps as f64;
        let radius_at = |fraction: f64| {
            self.start_radius_m + (self.end_radius_m - self.start_radius_m) * fraction
        };
        let mut points = Vec::with_capacity(steps);
        if outside {
            let stretch = 1.0 / (step.abs() / 2.0).to_radians().cos();
            for index in 0..steps {
                let along = (index as f64 + 0.5) / steps as f64;
                points.push(destination(
                    self.centre,
                    self.start_deg + along * self.sweep_deg,
                    radius_at(along) * stretch,
                )?);
            }
        } else {
            for index in 1..steps {
                let along = index as f64 / steps as f64;
                points.push(destination(
                    self.centre,
                    self.start_deg + along * self.sweep_deg,
                    radius_at(along),
                )?);
            }
        }
        Some(points)
    }

    /// The most points [`Arc::between`] can return for this arc.
    pub(crate) fn most_points(&self) -> usize {
        arc_steps(self.start_radius_m.max(self.end_radius_m), self.sweep_deg)
    }
}

/// Twice the area a ring encloses, in square degrees: positive when the
/// ring runs counter-clockwise seen from above, negative when clockwise.
pub(crate) fn doubled_signed_area(ring: &[GeoPoint]) -> f64 {
    let Some(origin) = ring.first() else {
        return 0.0;
    };
    ring.windows(2)
        .map(|pair| {
            let (ax, ay) = (pair[0].lon() - origin.lon(), pair[0].lat() - origin.lat());
            let (bx, by) = (pair[1].lon() - origin.lon(), pair[1].lat() - origin.lat());
            ax * by - bx * ay
        })
        .sum()
}

/// Where a point lies in relation to a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RingPosition {
    Inside,
    /// On an edge or a corner, within a tenth of a millimetre.
    Boundary,
    Outside,
}

/// Decides where a point lies in relation to a closed ring, in the plane of
/// longitude and latitude.
pub(crate) fn ring_position(ring: &[GeoPoint], point: GeoPoint) -> RingPosition {
    // About a tenth of a millimetre, in degrees.
    const ON_EDGE_DEG: f64 = 1e-9;
    let (x, y) = (point.lon(), point.lat());
    let mut inside = false;
    for pair in ring.windows(2) {
        let (x1, y1) = (pair[0].lon(), pair[0].lat());
        let (x2, y2) = (pair[1].lon(), pair[1].lat());

        // On the edge: within its box and no farther from its line than the tolerance.
        if x >= x1.min(x2) - ON_EDGE_DEG
            && x <= x1.max(x2) + ON_EDGE_DEG
            && y >= y1.min(y2) - ON_EDGE_DEG
            && y <= y1.max(y2) + ON_EDGE_DEG
        {
            let (dx, dy) = (x2 - x1, y2 - y1);
            let length = dx.hypot(dy);
            let off_line = if length > 0.0 {
                ((x - x1) * dy - (y - y1) * dx).abs() / length
            } else {
                (x - x1).hypot(y - y1)
            };
            if off_line <= ON_EDGE_DEG {
                return RingPosition::Boundary;
            }
        }

        // A ray from the point toward the east crosses this edge.
        if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
            inside = !inside;
        }
    }
    if inside {
        RingPosition::Inside
    } else {
        RingPosition::Outside
    }
}

/// Whether a point lies in a polygon. A point on the boundary, of the outer
/// ring or of a hole, counts as in: the doubtful case goes to the airspace.
pub(crate) fn contains(polygon: &Polygon, point: GeoPoint) -> bool {
    if ring_position(polygon.exterior(), point) == RingPosition::Outside {
        return false;
    }
    polygon
        .holes()
        .iter()
        .all(|hole| ring_position(hole, point) != RingPosition::Inside)
}

/// A flat map laid out around one point: metres east and north of it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalPlane {
    origin: GeoPoint,
    metres_per_deg_lon: f64,
}

impl LocalPlane {
    pub(crate) fn around(origin: GeoPoint) -> Self {
        Self {
            origin,
            metres_per_deg_lon: M_PER_DEG * origin.lat().to_radians().cos(),
        }
    }

    /// Metres east and north of the origin.
    pub(crate) fn project(&self, point: GeoPoint) -> (f64, f64) {
        (
            (point.lon() - self.origin.lon()) * self.metres_per_deg_lon,
            (point.lat() - self.origin.lat()) * M_PER_DEG,
        )
    }

    /// The point that lies `east` and `north` metres from the origin.
    pub(crate) fn unproject(&self, east: f64, north: f64) -> Option<GeoPoint> {
        let lat = self.origin.lat() + north / M_PER_DEG;
        let lon = if self.metres_per_deg_lon > 0.0 {
            self.origin.lon() + east / self.metres_per_deg_lon
        } else {
            self.origin.lon()
        };
        GeoPoint::new(lat.clamp(-90.0, 90.0), lon.clamp(-180.0, 180.0)).ok()
    }
}

/// The point of a polygon's boundary nearest to `from`, the distance to it
/// on the sphere in metres, and the direction to it in degrees.
///
/// Every ring counts, holes included: the edge of a hole is a way out too.
/// `None` only for a polygon without edges, which the domain type rules out.
pub(crate) fn nearest_on_boundary(
    polygon: &Polygon,
    from: GeoPoint,
) -> Option<(GeoPoint, f64, f64)> {
    let plane = LocalPlane::around(from);
    let mut best: Option<(f64, f64, f64)> = None;
    let rings =
        std::iter::once(polygon.exterior()).chain(polygon.holes().iter().map(Vec::as_slice));
    for ring in rings {
        for pair in ring.windows(2) {
            let (ax, ay) = plane.project(pair[0]);
            let (bx, by) = plane.project(pair[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let length_squared = dx * dx + dy * dy;
            // How far along the edge the nearest point lies, from 0 to 1.
            let along = if length_squared > 0.0 {
                (-(ax * dx + ay * dy) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (x, y) = (ax + along * dx, ay + along * dy);
            let distance_squared = x * x + y * y;
            if best.is_none_or(|(_, _, nearest)| distance_squared < nearest) {
                best = Some((x, y, distance_squared));
            }
        }
    }
    let (east, north, _) = best?;
    let nearest = plane.unproject(east, north)?;
    let bearing = if east == 0.0 && north == 0.0 {
        0.0
    } else {
        east.atan2(north).to_degrees().rem_euclid(360.0)
    };
    Some((nearest, from.distance_m(nearest), bearing))
}

/// The stretches of the straight path from `a` to `b` that lie in a polygon,
/// as fractions of the path from 0 to 1, in order.
///
/// The path is straight in longitude and latitude, like the polygon's edges.
pub(crate) fn inside_stretches(polygon: &Polygon, a: GeoPoint, b: GeoPoint) -> Vec<(f64, f64)> {
    let (ax, ay) = (a.lon(), a.lat());
    let (dx, dy) = (b.lon() - ax, b.lat() - ay);
    if dx == 0.0 && dy == 0.0 {
        return if contains(polygon, a) {
            vec![(0.0, 1.0)]
        } else {
            Vec::new()
        };
    }

    // Every place the path crosses an edge, as a fraction of the path.
    let mut cuts = vec![0.0, 1.0];
    let rings =
        std::iter::once(polygon.exterior()).chain(polygon.holes().iter().map(Vec::as_slice));
    for ring in rings {
        for pair in ring.windows(2) {
            let (px, py) = (pair[0].lon(), pair[0].lat());
            let (ex, ey) = (pair[1].lon() - px, pair[1].lat() - py);
            let denominator = dx * ey - dy * ex;
            if denominator == 0.0 {
                // Parallel. A path that runs along an edge is settled by
                // testing the stretches between the other crossings.
                continue;
            }
            let along_path = ((px - ax) * ey - (py - ay) * ex) / denominator;
            let along_edge = ((px - ax) * dy - (py - ay) * dx) / denominator;
            if (0.0..=1.0).contains(&along_path) && (0.0..=1.0).contains(&along_edge) {
                cuts.push(along_path);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);

    // Between two neighbouring crossings the path is wholly in or wholly
    // out; its middle tells which.
    let mut stretches: Vec<(f64, f64)> = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if to <= from {
            continue;
        }
        let middle = (from + to) / 2.0;
        let Ok(point) = GeoPoint::new(ay + middle * dy, ax + middle * dx) else {
            continue;
        };
        if contains(polygon, point) {
            match stretches.last_mut() {
                Some(last) if last.1 >= from => last.1 = to,
                _ => stretches.push((from, to)),
            }
        }
    }
    stretches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    fn ring(points: &[(f64, f64)]) -> Vec<GeoPoint> {
        let mut ring: Vec<GeoPoint> = points.iter().map(|&(lat, lon)| point(lat, lon)).collect();
        ring.push(ring[0]);
        ring
    }

    fn square() -> Polygon {
        Polygon::new(
            ring(&[(47.0, 11.0), (47.0, 11.2), (47.1, 11.2), (47.1, 11.0)]),
            Vec::new(),
        )
        .unwrap()
    }

    #[test]
    fn destination_and_bearing_agree_with_distance() {
        let munich = point(48.137, 11.575);
        for bearing in [0.0, 37.0, 90.0, 180.0, 271.5] {
            let there = destination(munich, bearing, 25_000.0).unwrap();
            assert!((munich.distance_m(there) - 25_000.0).abs() < 0.01);
            assert!((bearing_deg(munich, there) - bearing).abs() < 1e-6);
        }
        // One degree of latitude due north.
        let north = destination(point(47.0, 11.0), 0.0, M_PER_DEG).unwrap();
        assert!((north.lat() - 48.0).abs() < 1e-9 && (north.lon() - 11.0).abs() < 1e-9);
    }

    #[test]
    fn destination_survives_the_poles_and_the_antimeridian() {
        let pole = point(90.0, 0.0);
        assert!(destination(pole, 45.0, 1000.0).is_some());
        let east = destination(point(0.0, 179.99), 90.0, 5000.0).unwrap();
        assert!(east.lon() < -179.9, "{}", east.lon());
        assert!(destination(pole, f64::NAN, 1.0).is_none());
        assert!(destination(pole, 0.0, f64::INFINITY).is_none());
    }

    #[test]
    fn a_circle_ring_contains_its_circle_and_stays_within_the_error() {
        let centre = point(47.5, 11.5);
        for radius in [5.0, 300.0, 2.0 * 1852.0, 5.0 * 1852.0, 60.0 * 1852.0] {
            let ring = circle_ring(centre, radius).unwrap();
            assert_eq!(ring.first(), ring.last());
            let polygon = Polygon::new(ring.clone(), Vec::new()).unwrap();
            // Corners are outside the circle by no more than the error.
            for corner in &ring {
                let distance = centre.distance_m(*corner);
                assert!(distance >= radius - 1e-6, "{distance} < {radius}");
                assert!(distance <= radius + ARC_MAX_ERROR_M + 1e-6, "{distance}");
            }
            // Every point of the true circle is inside the ring.
            for tenth_degree in 0..3600 {
                let on_circle =
                    destination(centre, f64::from(tenth_degree) / 10.0, radius * 0.999_99).unwrap();
                assert!(
                    contains(&polygon, on_circle),
                    "radius {radius} at {tenth_degree}"
                );
            }
            // And nothing farther out than the error is.
            for degree in 0..360 {
                let beyond =
                    destination(centre, f64::from(degree), radius + ARC_MAX_ERROR_M + 0.5).unwrap();
                assert!(!contains(&polygon, beyond), "radius {radius} at {degree}");
            }
        }
        assert!(circle_ring(centre, 0.0).is_none());
        assert!(circle_ring(centre, f64::NAN).is_none());
    }

    #[test]
    fn a_five_mile_circle_needs_under_a_hundred_points() {
        let ring = circle_ring(point(47.5, 11.5), 5.0 * METRES_PER_NAUTICAL_MILE).unwrap();
        assert!((60..100).contains(&ring.len()), "{}", ring.len());
    }

    #[test]
    fn arc_points_lie_on_the_side_asked_for() {
        let arc = Arc {
            centre: point(47.5, 11.5),
            start_deg: 20.0,
            sweep_deg: -140.0,
            start_radius_m: 8000.0,
            end_radius_m: 8000.0,
        };
        let outside = arc.between(true).unwrap();
        let on_arc = arc.between(false).unwrap();
        assert!(outside.len() <= arc.most_points() && on_arc.len() < arc.most_points());
        for corner in &outside {
            let distance = arc.centre.distance_m(*corner);
            assert!(
                (8000.0..=8000.0 + ARC_MAX_ERROR_M).contains(&distance),
                "{distance}"
            );
        }
        for corner in &on_arc {
            assert!((arc.centre.distance_m(*corner) - 8000.0).abs() < 0.01);
        }
        // The first piece leaves the start of the arc along its tangent, so
        // it never cuts inside the circle.
        let start = destination(arc.centre, 20.0, 8000.0).unwrap();
        let middle = point(
            (start.lat() + outside[0].lat()) / 2.0,
            (start.lon() + outside[0].lon()) / 2.0,
        );
        assert!(arc.centre.distance_m(middle) >= 8000.0 - 0.01);
    }

    #[test]
    fn points_are_placed_in_out_or_on_the_boundary() {
        let square = square();
        let exterior = square.exterior();
        assert_eq!(
            ring_position(exterior, point(47.05, 11.1)),
            RingPosition::Inside
        );
        assert_eq!(
            ring_position(exterior, point(47.2, 11.1)),
            RingPosition::Outside
        );
        assert_eq!(
            ring_position(exterior, point(47.05, 10.9)),
            RingPosition::Outside
        );
        // Corners and edges.
        assert_eq!(
            ring_position(exterior, point(47.0, 11.0)),
            RingPosition::Boundary
        );
        assert_eq!(
            ring_position(exterior, point(47.1, 11.1)),
            RingPosition::Boundary
        );
        assert_eq!(
            ring_position(exterior, point(47.05, 11.2)),
            RingPosition::Boundary
        );
        assert!(contains(&square, point(47.05, 11.2)));
    }

    #[test]
    fn a_hole_is_outside_and_its_edge_is_inside() {
        let hole = ring(&[
            (47.04, 11.08),
            (47.04, 11.12),
            (47.06, 11.12),
            (47.06, 11.08),
        ]);
        let polygon = Polygon::new(square().exterior().to_vec(), vec![hole]).unwrap();
        assert!(!contains(&polygon, point(47.05, 11.1)));
        assert!(contains(&polygon, point(47.02, 11.1)));
        assert!(contains(&polygon, point(47.04, 11.1)));
    }

    #[test]
    fn the_nearest_boundary_point_is_found_with_distance_and_direction() {
        let square = square();
        // 0.05 degrees of latitude north of the northern edge.
        let (nearest, distance, bearing) =
            nearest_on_boundary(&square, point(47.15, 11.1)).unwrap();
        assert!((nearest.lat() - 47.1).abs() < 1e-9 && (nearest.lon() - 11.1).abs() < 1e-9);
        assert!((distance - 0.05 * M_PER_DEG).abs() < 0.01);
        assert!((bearing - 180.0).abs() < 1e-6);

        // From inside, the nearest edge is the way out.
        let (_, distance, bearing) = nearest_on_boundary(&square, point(47.09, 11.1)).unwrap();
        assert!((distance - 0.01 * M_PER_DEG).abs() < 0.01);
        assert!(bearing.abs() < 1e-6);

        // Beyond a corner the corner is nearest.
        let (nearest, _, bearing) = nearest_on_boundary(&square, point(47.2, 11.3)).unwrap();
        assert_eq!(nearest, point(47.1, 11.2));
        assert!((180.0..270.0).contains(&bearing));

        // On the boundary the distance is zero.
        let (_, distance, _) = nearest_on_boundary(&square, point(47.1, 11.05)).unwrap();
        assert!(distance < 1e-6);
    }

    #[test]
    fn a_path_is_cut_into_the_stretches_inside() {
        let square = square();
        // West to east straight through the middle: in from a quarter to three quarters.
        let through = inside_stretches(&square, point(47.05, 10.9), point(47.05, 11.3));
        assert_eq!(through.len(), 1);
        assert!((through[0].0 - 0.25).abs() < 1e-9 && (through[0].1 - 0.75).abs() < 1e-9);

        // Starting inside.
        let leaving = inside_stretches(&square, point(47.05, 11.1), point(47.05, 11.3));
        assert_eq!(leaving.len(), 1);
        assert!(leaving[0].0 == 0.0 && (leaving[0].1 - 0.5).abs() < 1e-9);

        // Missing it altogether, and not moving at all.
        assert!(inside_stretches(&square, point(47.2, 10.9), point(47.2, 11.3)).is_empty());
        let standing = inside_stretches(&square, point(47.05, 11.1), point(47.05, 11.1));
        assert_eq!(standing, vec![(0.0, 1.0)]);
        assert!(inside_stretches(&square, point(47.5, 11.1), point(47.5, 11.1)).is_empty());

        // Across a hole the path is in, out and in again.
        let hole = ring(&[
            (47.04, 11.08),
            (47.04, 11.12),
            (47.06, 11.12),
            (47.06, 11.08),
        ]);
        let holed = Polygon::new(square.exterior().to_vec(), vec![hole]).unwrap();
        let across = inside_stretches(&holed, point(47.05, 10.9), point(47.05, 11.3));
        assert_eq!(across.len(), 2);
    }
}
