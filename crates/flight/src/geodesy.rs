//! Distances and directions on the two shapes of the earth that free
//! flight measures on.
//!
//! A competition task says which one its cylinders are measured on: the
//! WGS84 ellipsoid, which CIVL's scoring rules use for every distance, or
//! the FAI sphere of 6371 km, which older instruments compute on. The two
//! differ by up to half a percent, which on a cylinder of 20 km is a
//! hundred metres: enough to miss a turnpoint by.
//!
//! On the ellipsoid these are Vincenty's formulae (T. Vincenty, "Direct
//! and inverse solutions of geodesics on the ellipsoid with application
//! of nested equations", Survey Review 176, 1975). They are good to a
//! fraction of a millimetre and fail only between points almost opposite
//! each other on the globe, which no flight spans; there the sphere's
//! answer is given instead.

use venti_domain::GeoPoint;

/// The shape of the earth a distance is measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Earth {
    /// The WGS84 ellipsoid.
    #[default]
    Wgs84,
    /// A sphere with a radius of 6371 km.
    FaiSphere,
}

/// From one point to another: how far, and which way to set out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Course {
    /// The distance along the surface, metres.
    pub distance_m: f64,
    /// The direction at the first point, degrees clockwise from north.
    /// Zero when the two points are the same.
    pub bearing_deg: f64,
}

const SPHERE_RADIUS_M: f64 = 6_371_000.0;
/// The WGS84 ellipsoid: its equatorial radius and its flattening.
const WGS84_A_M: f64 = 6_378_137.0;
const WGS84_F: f64 = 1.0 / 298.257_223_563;

/// Vincenty's iteration stops when the longitude on the auxiliary sphere
/// moves by less than this, radians: well under a tenth of a millimetre.
const CONVERGED: f64 = 1e-12;
const MAX_ROUNDS: usize = 200;

impl Earth {
    /// The distance and the first direction from one point to another.
    pub fn course(self, from: GeoPoint, to: GeoPoint) -> Course {
        match self {
            Earth::FaiSphere => sphere_course(from, to),
            Earth::Wgs84 => vincenty_inverse(from, to).unwrap_or_else(|| sphere_course(from, to)),
        }
    }

    /// The distance between two points, metres.
    pub fn distance_m(self, from: GeoPoint, to: GeoPoint) -> f64 {
        self.course(from, to).distance_m
    }

    /// The point reached by setting out from a point in a direction and
    /// going a distance along the surface.
    pub fn destination(self, from: GeoPoint, bearing_deg: f64, distance_m: f64) -> GeoPoint {
        let (lat, lon) = match self {
            Earth::FaiSphere => sphere_destination(from, bearing_deg, distance_m),
            Earth::Wgs84 => vincenty_direct(from, bearing_deg, distance_m),
        };
        // A latitude can leave its range only by rounding at a pole.
        GeoPoint::new(lat.clamp(-90.0, 90.0), wrap_lon(lon)).unwrap_or(from)
    }
}

/// A longitude brought into -180 to 180 degrees.
fn wrap_lon(lon: f64) -> f64 {
    let wrapped = (lon + 180.0).rem_euclid(360.0) - 180.0;
    // The date line may be written either way; keep the sign that was given.
    if wrapped == -180.0 && lon > 0.0 {
        180.0
    } else {
        wrapped
    }
}

fn sphere_course(from: GeoPoint, to: GeoPoint) -> Course {
    let (lat1, lat2) = (from.lat().to_radians(), to.lat().to_radians());
    let dlat = lat2 - lat1;
    let dlon = (to.lon() - from.lon()).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    let distance_m = 2.0 * SPHERE_RADIUS_M * a.sqrt().min(1.0).asin();
    let east = dlon.sin() * lat2.cos();
    let north = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    let bearing_deg = if distance_m == 0.0 {
        0.0
    } else {
        east.atan2(north).to_degrees().rem_euclid(360.0)
    };
    Course {
        distance_m,
        bearing_deg,
    }
}

fn sphere_destination(from: GeoPoint, bearing_deg: f64, distance_m: f64) -> (f64, f64) {
    let lat1 = from.lat().to_radians();
    let bearing = bearing_deg.to_radians();
    let angle = distance_m / SPHERE_RADIUS_M;
    let lat2 = (lat1.sin() * angle.cos() + lat1.cos() * angle.sin() * bearing.cos())
        .clamp(-1.0, 1.0)
        .asin();
    let dlon =
        (bearing.sin() * angle.sin() * lat1.cos()).atan2(angle.cos() - lat1.sin() * lat2.sin());
    (lat2.to_degrees(), from.lon() + dlon.to_degrees())
}

/// The two series in the square of `u` that both of Vincenty's solutions
/// use.
fn vincenty_a_b(u_squared: f64) -> (f64, f64) {
    let a = 1.0
        + u_squared / 16384.0
            * (4096.0 + u_squared * (-768.0 + u_squared * (320.0 - 175.0 * u_squared)));
    let b =
        u_squared / 1024.0 * (256.0 + u_squared * (-128.0 + u_squared * (74.0 - 47.0 * u_squared)));
    (a, b)
}

/// The correction to the arc on the auxiliary sphere.
fn delta_sigma(b: f64, sin_sigma: f64, cos_sigma: f64, cos_2sigma_m: f64) -> f64 {
    b * sin_sigma
        * (cos_2sigma_m
            + b / 4.0
                * (cos_sigma * (-1.0 + 2.0 * cos_2sigma_m * cos_2sigma_m)
                    - b / 6.0
                        * cos_2sigma_m
                        * (-3.0 + 4.0 * sin_sigma * sin_sigma)
                        * (-3.0 + 4.0 * cos_2sigma_m * cos_2sigma_m)))
}

/// Vincenty's inverse solution. `None` when it does not converge, which
/// happens only between points nearly opposite each other on the globe.
fn vincenty_inverse(from: GeoPoint, to: GeoPoint) -> Option<Course> {
    let (a, f) = (WGS84_A_M, WGS84_F);
    let b = a * (1.0 - f);
    let l = (to.lon() - from.lon()).to_radians();
    let u1 = ((1.0 - f) * from.lat().to_radians().tan()).atan();
    let u2 = ((1.0 - f) * to.lat().to_radians().tan()).atan();
    let (sin_u1, cos_u1) = u1.sin_cos();
    let (sin_u2, cos_u2) = u2.sin_cos();

    let mut lambda = l;
    for _ in 0..MAX_ROUNDS {
        let (sin_lambda, cos_lambda) = lambda.sin_cos();
        let sin_sigma = ((cos_u2 * sin_lambda).powi(2)
            + (cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_lambda).powi(2))
        .sqrt();
        if sin_sigma == 0.0 {
            // The same point.
            return Some(Course {
                distance_m: 0.0,
                bearing_deg: 0.0,
            });
        }
        let cos_sigma = sin_u1 * sin_u2 + cos_u1 * cos_u2 * cos_lambda;
        let sigma = sin_sigma.atan2(cos_sigma);
        let sin_alpha = cos_u1 * cos_u2 * sin_lambda / sin_sigma;
        let cos_sq_alpha = 1.0 - sin_alpha * sin_alpha;
        // Along the equator the term has no meaning and no weight.
        let cos_2sigma_m = if cos_sq_alpha == 0.0 {
            0.0
        } else {
            cos_sigma - 2.0 * sin_u1 * sin_u2 / cos_sq_alpha
        };
        let c = f / 16.0 * cos_sq_alpha * (4.0 + f * (4.0 - 3.0 * cos_sq_alpha));
        let next = l
            + (1.0 - c)
                * f
                * sin_alpha
                * (sigma
                    + c * sin_sigma
                        * (cos_2sigma_m
                            + c * cos_sigma * (-1.0 + 2.0 * cos_2sigma_m * cos_2sigma_m)));
        let moved = (next - lambda).abs();
        lambda = next;
        if moved < CONVERGED {
            let u_squared = cos_sq_alpha * (a * a - b * b) / (b * b);
            let (big_a, big_b) = vincenty_a_b(u_squared);
            let distance_m =
                b * big_a * (sigma - delta_sigma(big_b, sin_sigma, cos_sigma, cos_2sigma_m));
            let (sin_lambda, cos_lambda) = lambda.sin_cos();
            let bearing =
                (cos_u2 * sin_lambda).atan2(cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_lambda);
            return Some(Course {
                distance_m,
                bearing_deg: bearing.to_degrees().rem_euclid(360.0),
            });
        }
    }
    None
}

/// Vincenty's direct solution: latitude and longitude of the point
/// reached, degrees.
fn vincenty_direct(from: GeoPoint, bearing_deg: f64, distance_m: f64) -> (f64, f64) {
    let (a, f) = (WGS84_A_M, WGS84_F);
    let b = a * (1.0 - f);
    let (sin_alpha1, cos_alpha1) = bearing_deg.to_radians().sin_cos();
    let tan_u1 = (1.0 - f) * from.lat().to_radians().tan();
    let cos_u1 = 1.0 / (1.0 + tan_u1 * tan_u1).sqrt();
    let sin_u1 = tan_u1 * cos_u1;
    let sigma1 = tan_u1.atan2(cos_alpha1);
    let sin_alpha = cos_u1 * sin_alpha1;
    let cos_sq_alpha = 1.0 - sin_alpha * sin_alpha;
    let u_squared = cos_sq_alpha * (a * a - b * b) / (b * b);
    let (big_a, big_b) = vincenty_a_b(u_squared);

    let first = distance_m / (b * big_a);
    let mut sigma = first;
    let mut cos_2sigma_m = (2.0 * sigma1 + sigma).cos();
    for _ in 0..MAX_ROUNDS {
        cos_2sigma_m = (2.0 * sigma1 + sigma).cos();
        let (sin_sigma, cos_sigma) = sigma.sin_cos();
        let next = first + delta_sigma(big_b, sin_sigma, cos_sigma, cos_2sigma_m);
        let moved = (next - sigma).abs();
        sigma = next;
        if moved < CONVERGED {
            break;
        }
    }

    let (sin_sigma, cos_sigma) = sigma.sin_cos();
    let across = sin_u1 * sin_sigma - cos_u1 * cos_sigma * cos_alpha1;
    let lat2 = (sin_u1 * cos_sigma + cos_u1 * sin_sigma * cos_alpha1)
        .atan2((1.0 - f) * (sin_alpha * sin_alpha + across * across).sqrt());
    let lambda =
        (sin_sigma * sin_alpha1).atan2(cos_u1 * cos_sigma - sin_u1 * sin_sigma * cos_alpha1);
    let c = f / 16.0 * cos_sq_alpha * (4.0 + f * (4.0 - 3.0 * cos_sq_alpha));
    let l = lambda
        - (1.0 - c)
            * f
            * sin_alpha
            * (sigma
                + c * sin_sigma
                    * (cos_2sigma_m + c * cos_sigma * (-1.0 + 2.0 * cos_2sigma_m * cos_2sigma_m)));
    (lat2.to_degrees(), from.lon() + l.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    fn degrees(d: f64, m: f64, s: f64) -> f64 {
        d + m / 60.0 + s / 3600.0
    }

    #[test]
    fn a_degree_along_the_equator_is_a_degree_of_the_equatorial_radius() {
        let course = Earth::Wgs84.course(point(0.0, 0.0), point(0.0, 1.0));
        let expected = WGS84_A_M * 1.0_f64.to_radians();
        assert!(
            (course.distance_m - expected).abs() < 1e-4,
            "{}",
            course.distance_m
        );
        assert!((course.bearing_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn a_degree_up_a_meridian_from_the_equator_is_shorter_than_one_along_it() {
        // The meridian arc from the equator to 1° north on WGS84:
        // a(1 - e²) times the integral of (1 - e² sin²φ)^(-3/2).
        let course = Earth::Wgs84.course(point(0.0, 10.0), point(1.0, 10.0));
        assert!(
            (course.distance_m - 110_574.389).abs() < 0.01,
            "{}",
            course.distance_m
        );
        assert!(course.bearing_deg.abs() < 1e-9);
    }

    #[test]
    fn the_surveyors_example_from_flinders_peak_to_buninyong_comes_out_to_the_millimetre() {
        // The worked example of Geoscience Australia's GDA technical
        // manual: 54,972.271 m at an azimuth of 306° 52' 05.37". It is
        // given on GRS80, whose flattening differs from WGS84's in the
        // twelfth digit: less than a tenth of a millimetre here.
        let flinders = point(-degrees(37.0, 57.0, 3.7203), degrees(144.0, 25.0, 29.5244));
        let buninyong = point(-degrees(37.0, 39.0, 10.1561), degrees(143.0, 55.0, 35.3839));
        let course = Earth::Wgs84.course(flinders, buninyong);
        assert!(
            (course.distance_m - 54_972.271).abs() < 0.002,
            "{}",
            course.distance_m
        );
        let azimuth = degrees(306.0, 52.0, 5.37);
        assert!(
            (course.bearing_deg - azimuth).abs() < 1e-5,
            "{}",
            course.bearing_deg
        );

        // And back from the distance and the direction to the point.
        let reached = Earth::Wgs84.destination(flinders, azimuth, 54_972.271);
        assert!(Earth::Wgs84.distance_m(reached, buninyong) < 0.01);
    }

    #[test]
    fn setting_out_and_measuring_back_agree_on_both_earths() {
        let start = point(47.7, 11.8);
        for earth in [Earth::Wgs84, Earth::FaiSphere] {
            for (bearing, distance) in [
                (0.0, 400.0),
                (73.0, 5_000.0),
                (181.5, 87_300.0),
                (300.0, 250_000.0),
            ] {
                let reached = earth.destination(start, bearing, distance);
                let back = earth.course(start, reached);
                assert!(
                    (back.distance_m - distance).abs() < 1e-3,
                    "{earth:?} {distance}: {}",
                    back.distance_m
                );
                assert!(
                    (back.bearing_deg - bearing).abs() < 1e-6,
                    "{earth:?} {bearing}: {}",
                    back.bearing_deg
                );
            }
        }
    }

    #[test]
    fn the_sphere_and_the_ellipsoid_differ_most_from_east_to_west() {
        // Along a parallel in the Alps the ellipsoid is the longer by
        // about three parts in a thousand: 60 m on a cylinder of 20 km.
        let (a, b) = (point(47.0, 11.0), point(47.0, 12.0));
        let ellipsoid = Earth::Wgs84.distance_m(a, b);
        let sphere = Earth::FaiSphere.distance_m(a, b);
        assert!((sphere - a.distance_m(b)).abs() < 1e-6);
        let apart = (ellipsoid - sphere) / sphere;
        assert!(apart > 0.002 && apart < 0.004, "{apart}");

        // From south to north at this latitude the two nearly agree.
        let north = point(48.0, 11.0);
        let apart = (Earth::Wgs84.distance_m(a, north) - Earth::FaiSphere.distance_m(a, north))
            / Earth::FaiSphere.distance_m(a, north);
        assert!(apart.abs() < 0.0005, "{apart}");
    }

    #[test]
    fn a_point_is_no_distance_from_itself() {
        let here = point(47.7, 11.8);
        for earth in [Earth::Wgs84, Earth::FaiSphere] {
            let course = earth.course(here, here);
            assert_eq!(course.distance_m, 0.0);
            assert_eq!(course.bearing_deg, 0.0);
            assert!(here.distance_m(earth.destination(here, 45.0, 0.0)) < 1e-6);
        }
    }
}
