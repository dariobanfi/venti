//! The projection INCA's grid is drawn in: EPSG:31287, "MGI / Austria
//! Lambert".
//!
//! A recorded file says it itself, in the `crs_wkt` of its variable
//! `lambert_conformal_conic` (read on 2026-11-21):
//!
//! ```text
//! PROJCS["MGI / Austria Lambert", GEOGCS["MGI", …
//!   SPHEROID["Bessel 1841",6377397.155,299.1528128],
//!   TOWGS84[577.326,90.129,463.919,5.137,1.474,5.297,2.4232]],
//!   PROJECTION["Lambert_Conformal_Conic_2SP"],
//!   standard_parallel_1 49, standard_parallel_2 46, latitude_of_origin 47.5,
//!   central_meridian 13.33333333333333, false_easting 400000,
//!   false_northing 400000, UNIT["metre",1]]
//! ```
//!
//! a conformal conic projection of Bessel's ellipsoid with two standard
//! parallels, on the datum of the Militärgeographisches Institut. That
//! datum is not the map's: a place has other degrees on it than on WGS84,
//! 54 to 115 m apart in the first region. So a place of the map is first moved from
//! WGS84 to MGI, by the seven numbers the file gives after `TOWGS84` (three
//! shifts in metres, three turns in seconds of arc, a scale in millionths;
//! the "position vector" reading, which is the one that fits), and then
//! projected. The formulas are Snyder's for the conformal conic projection
//! of an ellipsoid ("Map Projections: A Working Manual", USGS 1987).
//!
//! The adapter writes these numbers down and does not read them from each
//! file: the text above is a long string of another grammar, and the one
//! number a file gives in a form of its own, the central meridian, it gives
//! rounded to 13.33333. Instead every frame is held against the degrees
//! the file carries for each of its points (`lat`, `lon`): the four corners
//! must lie where this projection puts them, or the frame is refused.
//!
//! How well it fits was measured on the recorded frames (2026-11-21), over
//! all 151,200 points of each: with the datum's shift the file's own
//! degrees land within 0.95 m of the file's own metres, a thousandth of a
//! cell; without it they are 54 to 115 m off, up to a ninth of a cell and
//! all to one side. A test of this crate holds both numbers.

use std::f64::consts::FRAC_PI_4;

/// Bessel's ellipsoid of 1841: its equatorial radius in metres and its
/// flattening.
const BESSEL_A: f64 = 6_377_397.155;
const BESSEL_F: f64 = 1.0 / 299.152_812_8;

/// The ellipsoid of WGS84.
const WGS84_A: f64 = 6_378_137.0;
const WGS84_F: f64 = 1.0 / 298.257_223_563;

/// The two standard parallels, the latitude and the meridian of the
/// origin, in degrees, and what is added to an easting and a northing.
const STANDARD_PARALLELS: (f64, f64) = (49.0, 46.0);
const ORIGIN: (f64, f64) = (13.0 + 1.0 / 3.0, 47.5);
const FALSE_EASTING: f64 = 400_000.0;
const FALSE_NORTHING: f64 = 400_000.0;

/// From MGI to WGS84: shifts in metres, turns in seconds of arc, a scale
/// in millionths.
const TO_WGS84: [f64; 7] = [577.326, 90.129, 463.919, 5.137, 1.474, 5.297, 2.4232];

/// The projection, with what does not change from place to place worked
/// out once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AustriaLambert {
    /// Bessel's eccentricity.
    e: f64,
    /// The cone's constant.
    n: f64,
    /// The radius of a parallel on the map is `scale * t(latitude)^n`.
    scale: f64,
    /// The radius of the origin's parallel.
    origin_radius: f64,
}

/// Snyder's `t` for a latitude in radians.
fn t(e: f64, lat: f64) -> f64 {
    let s = e * lat.sin();
    (FRAC_PI_4 - lat / 2.0).tan() / ((1.0 - s) / (1.0 + s)).powf(e / 2.0)
}

/// Snyder's `m` for a latitude in radians.
fn m(e: f64, lat: f64) -> f64 {
    lat.cos() / (1.0 - (e * lat.sin()).powi(2)).sqrt()
}

impl AustriaLambert {
    pub(crate) fn new() -> Self {
        let e = (2.0 * BESSEL_F - BESSEL_F * BESSEL_F).sqrt();
        let (first, second) = (
            STANDARD_PARALLELS.0.to_radians(),
            STANDARD_PARALLELS.1.to_radians(),
        );
        let n = (m(e, first).ln() - m(e, second).ln()) / (t(e, first).ln() - t(e, second).ln());
        let scale = BESSEL_A * m(e, first) / (n * t(e, first).powf(n));
        Self {
            e,
            n,
            scale,
            origin_radius: scale * t(e, ORIGIN.1.to_radians()).powf(n),
        }
    }

    /// Where a place of the map lies on the grid's plane: easting and
    /// northing in metres. The place is given in degrees on WGS84, as a
    /// web map has them.
    pub(crate) fn forward(&self, lon_deg: f64, lat_deg: f64) -> (f64, f64) {
        let (lon, lat) = wgs84_to_mgi(lon_deg.to_radians(), lat_deg.to_radians());
        self.project(lon, lat)
    }

    /// The same for a place given in degrees on MGI itself: the
    /// projection without the datum's shift. For the test that measures
    /// what the shift is worth.
    #[cfg(test)]
    pub(crate) fn forward_unshifted(&self, lon_deg: f64, lat_deg: f64) -> (f64, f64) {
        self.project(lon_deg.to_radians(), lat_deg.to_radians())
    }

    fn project(&self, lon: f64, lat: f64) -> (f64, f64) {
        let radius = self.scale * t(self.e, lat).powf(self.n);
        let turned = self.n * (lon - ORIGIN.0.to_radians());
        (
            FALSE_EASTING + radius * turned.sin(),
            FALSE_NORTHING + self.origin_radius - radius * turned.cos(),
        )
    }
}

/// A place on WGS84's ellipsoid, at its surface, as the same place on
/// Bessel's ellipsoid in the datum MGI: longitude and latitude in radians.
fn wgs84_to_mgi(lon: f64, lat: f64) -> (f64, f64) {
    // To a point in space, from the earth's centre as WGS84 has it.
    let e2 = 2.0 * WGS84_F - WGS84_F * WGS84_F;
    let normal = WGS84_A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    let from = [
        normal * lat.cos() * lon.cos(),
        normal * lat.cos() * lon.sin(),
        normal * (1.0 - e2) * lat.sin(),
    ];

    // The seven numbers lead from MGI to WGS84; this is the way back. The
    // turns are a few seconds of arc, for which turning back is turning by
    // the same angles the other way, to within millimetres.
    let [dx, dy, dz, rx, ry, rz, ppm] = TO_WGS84;
    let seconds = std::f64::consts::PI / 180.0 / 3600.0;
    let (rx, ry, rz) = (rx * seconds, ry * seconds, rz * seconds);
    let k = 1.0 + ppm * 1e-6;
    let (x, y, z) = ((from[0] - dx) / k, (from[1] - dy) / k, (from[2] - dz) / k);
    let (x, y, z) = (
        x + rz * y - ry * z,
        y - rz * x + rx * z,
        z + ry * x - rx * y,
    );

    // Back to degrees, on Bessel's ellipsoid. The latitude is found by
    // trying: each round is good for some six more digits.
    let e2 = 2.0 * BESSEL_F - BESSEL_F * BESSEL_F;
    let from_axis = x.hypot(y);
    let mut lat = z.atan2(from_axis * (1.0 - e2));
    for _ in 0..4 {
        let normal = BESSEL_A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
        lat = (z + e2 * normal * lat.sin()).atan2(from_axis);
    }
    (y.atan2(x), lat)
}

#[cfg(test)]
mod tests {
    use venti_hdf5::File;

    use super::*;
    use crate::fixtures;

    #[test]
    fn the_origin_lies_at_the_false_easting_and_northing() {
        let projection = AustriaLambert::new();
        let (x, y) = projection.forward_unshifted(ORIGIN.0, ORIGIN.1);
        assert!((x - 400_000.0).abs() < 1e-6 && (y - 400_000.0).abs() < 1e-6);
        // A degree of latitude north of it is 111.2 km, and a little more
        // on the map north of the standard parallels' middle.
        let (x, y) = projection.forward_unshifted(ORIGIN.0, ORIGIN.1 + 1.0);
        assert!((x - 400_000.0).abs() < 1e-6);
        assert!((111_100.0..111_300.0).contains(&(y - 400_000.0)), "{y}");
    }

    #[test]
    fn every_point_of_the_recorded_frames_lies_where_its_own_degrees_put_it() {
        let projection = AustriaLambert::new();
        for (_, body) in fixtures::frames() {
            let file = File::open(body).unwrap();
            let x = file.numbers(&file.object(&["x"]).unwrap(), 600).unwrap();
            let y = file.numbers(&file.object(&["y"]).unwrap(), 252).unwrap();
            let lat = file
                .shuffled_f32(&file.object(&["lat"]).unwrap(), &[252, 600])
                .unwrap();
            let lon = file
                .shuffled_f32(&file.object(&["lon"]).unwrap(), &[252, 600])
                .unwrap();

            let (mut worst, mut worst_unshifted, mut least_unshifted) = (0.0f64, 0.0f64, f64::MAX);
            for (at, (lat, lon)) in lat.iter().zip(&lon).enumerate() {
                let (east, north) = (x[at % 600], y[at / 600]);
                let (lon, lat) = (f64::from(*lon), f64::from(*lat));
                let (px, py) = projection.forward(lon, lat);
                worst = worst.max((px - east).hypot(py - north));
                let (px, py) = projection.forward_unshifted(lon, lat);
                let off = (px - east).hypot(py - north);
                worst_unshifted = worst_unshifted.max(off);
                least_unshifted = least_unshifted.min(off);
            }
            // The file's degrees are floats of four bytes: a step of them
            // is 0.4 m of latitude and 0.6 m of longitude there.
            assert!(worst < 0.95, "{worst} m");
            // Without the datum's shift every point is off, by up to a
            // ninth of a cell.
            assert!(
                least_unshifted > 54.0 && worst_unshifted > 114.0 && worst_unshifted < 115.0,
                "{least_unshifted} to {worst_unshifted} m"
            );
        }
    }
}
