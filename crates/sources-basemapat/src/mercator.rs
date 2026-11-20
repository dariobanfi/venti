//! The Web Mercator scheme of map tiles (EPSG:3857, `google3857` in the
//! service's own words).
//!
//! Positions are worked with as fractions of the world: `u` runs from 0 at
//! 180° W to 1 at 180° E, `v` from 0 at the northern edge of the map (about
//! 85.05° N) to 1 at the southern. A tile at zoom `z` is a square of side
//! `2^-z` in these units, and its pixels are a grid inside it. The
//! projection keeps angles, so near any one place a circle on the ground is
//! a circle here too, of a radius that grows with the latitude
//! ([`units_per_metre`]).

use std::f64::consts::PI;

use venti_sources::TileAddress;

/// The length of the equator on the sphere Web Mercator is drawn on, in
/// metres: `2π · 6378137`.
pub const EQUATOR_M: f64 = 40_075_016.685_578_49;

/// A point in world fractions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// From west to east, 0 to 1.
    pub u: f64,
    /// From north to south, 0 to 1.
    pub v: f64,
}

/// The point of a longitude and latitude in degrees.
pub fn project(lon: f64, lat: f64) -> Point {
    let u = (lon + 180.0) / 360.0;
    let v = (1.0 - lat.to_radians().tan().asinh() / PI) / 2.0;
    Point { u, v }
}

/// The latitude in degrees of a `v`.
pub fn latitude(v: f64) -> f64 {
    (PI * (1.0 - 2.0 * v)).sinh().atan().to_degrees()
}

/// The longitude in degrees of a `u`.
pub fn longitude(u: f64) -> f64 {
    u * 360.0 - 180.0
}

/// How many world fractions one metre on the ground is, at a latitude.
pub fn units_per_metre(lat: f64) -> f64 {
    1.0 / (EQUATOR_M * lat.to_radians().cos())
}

/// A box in world fractions: `u0 < u1`, `v0 < v1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Western edge.
    pub u0: f64,
    /// Northern edge.
    pub v0: f64,
    /// Eastern edge.
    pub u1: f64,
    /// Southern edge.
    pub v1: f64,
}

impl Rect {
    /// The square a tile covers.
    pub fn of_tile(tile: TileAddress) -> Self {
        let side = 1.0 / f64::from(1u32 << tile.z());
        let u0 = f64::from(tile.x()) * side;
        let v0 = f64::from(tile.y()) * side;
        Self {
            u0,
            v0,
            u1: u0 + side,
            v1: v0 + side,
        }
    }

    /// The box of two corners given in degrees, west-south and east-north.
    pub fn of_degrees(west: f64, south: f64, east: f64, north: f64) -> Self {
        let a = project(west, north);
        let b = project(east, south);
        Self {
            u0: a.u,
            v0: a.v,
            u1: b.u,
            v1: b.v,
        }
    }

    /// The same box grown by `by` on every side.
    pub fn grown(self, by: f64) -> Self {
        Self {
            u0: self.u0 - by,
            v0: self.v0 - by,
            u1: self.u1 + by,
            v1: self.v1 + by,
        }
    }

    /// Whether the two boxes share more than an edge.
    pub fn overlaps(self, other: Self) -> bool {
        self.u0 < other.u1 && other.u0 < self.u1 && self.v0 < other.v1 && other.v0 < self.v1
    }

    /// The middle.
    pub fn centre(self) -> Point {
        Point {
            u: (self.u0 + self.u1) / 2.0,
            v: (self.v0 + self.v1) / 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, within: f64) {
        assert!((a - b).abs() <= within, "{a} is not {b}");
    }

    #[test]
    fn a_position_lands_in_the_tile_every_web_map_puts_it_in() {
        // Innsbruck at zoom 15, as the service and OpenStreetMap number it.
        let point = project(11.39, 47.27);
        let tile = |z: u8| {
            let n = f64::from(1u32 << z);
            ((point.u * n).floor(), (point.v * n).floor())
        };
        assert_eq!(tile(15), (17420.0, 11489.0));
        assert_eq!(tile(0), (0.0, 0.0));
        // The corners of the map.
        close(project(-180.0, 0.0).u, 0.0, 1e-15);
        close(project(0.0, 0.0).v, 0.5, 1e-15);
        close(project(0.0, 85.051_128_779_806_59).v, 0.0, 1e-12);
    }

    #[test]
    fn degrees_come_back_as_they_went() {
        for (lon, lat) in [(11.39, 47.27), (-70.0, -33.4), (16.37, 48.21)] {
            let point = project(lon, lat);
            close(longitude(point.u), lon, 1e-12);
            close(latitude(point.v), lat, 1e-12);
        }
    }

    #[test]
    fn a_tile_is_a_square_of_its_zoom() {
        let tile = TileAddress::new(1, 1, 0).unwrap();
        let rect = Rect::of_tile(tile);
        assert_eq!((rect.u0, rect.v0, rect.u1, rect.v1), (0.5, 0.0, 1.0, 0.5));
        assert!(rect.overlaps(rect.grown(-0.1)));
        let neighbour = Rect::of_tile(TileAddress::new(1, 0, 0).unwrap());
        // Sharing an edge is not overlapping.
        assert!(!rect.overlaps(neighbour));
    }

    #[test]
    fn a_metre_is_longer_in_world_fractions_further_north() {
        // At the equator the world is 40,075 km round.
        close(units_per_metre(0.0) * EQUATOR_M, 1.0, 1e-12);
        // At 47° a metre is about 1.47 times as much of the map.
        close(units_per_metre(47.0) / units_per_metre(0.0), 1.466, 1e-3);
    }
}
