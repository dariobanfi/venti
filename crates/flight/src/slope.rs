//! How steep the ground is, from the elevation tiles, and the classes of
//! steepness a map of slopes is drawn in.
//!
//! A walker to a take-off and a pilot choosing a place to land both ask
//! how steep a slope is. The steepness at a place is read from the ground
//! [`SLOPE_BASE_M`] apart across it, east to west and north to south: the
//! rise of the plane through those four places. It is a mean over that
//! stretch; a step or a gully narrower than it is not in it, nor anything
//! finer than the tiles' own pixels.
//!
//! The classes are those avalanche maps of slopes draw from 30° on, with
//! two below: under 25°, 25° to 30°, 30° to 35°, 35° to 40°, and 40° and
//! more. swisstopo's map of slopes over 30° splits the steepest class
//! again at 45° (*to verify* on its own page); a walker's question is
//! answered by 40°.

use venti_domain::GeoPoint;

use crate::geodesy::Earth;
use crate::terrain::Terrain;

/// The ground is read this far apart across a place for its steepness,
/// metres. The tiles give heights in whole metres: over this stretch a
/// metre's rounding moves a slope of 35° by under one degree. Two pixels
/// of a tile of zoom 12 and 256 pixels, four of one of 512.
pub const SLOPE_BASE_M: f64 = 50.0;

/// A class of steepness, as maps of slopes draw them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlopeClass {
    /// Under 25°.
    Under25,
    /// From 25° to under 30°.
    From25,
    /// From 30° to under 35°: where avalanche maps begin.
    From30,
    /// From 35° to under 40°.
    From35,
    /// 40° and steeper.
    From40,
}

impl SlopeClass {
    /// The class of a steepness in degrees. A steepness that is no number
    /// is the gentlest class.
    #[must_use]
    pub fn of(slope_deg: f64) -> Self {
        match slope_deg {
            s if s >= 40.0 => Self::From40,
            s if s >= 35.0 => Self::From35,
            s if s >= 30.0 => Self::From30,
            s if s >= 25.0 => Self::From25,
            _ => Self::Under25,
        }
    }

    /// The steepness the class begins at, degrees.
    #[must_use]
    pub fn from_deg(self) -> f64 {
        match self {
            Self::Under25 => 0.0,
            Self::From25 => 25.0,
            Self::From30 => 30.0,
            Self::From35 => 35.0,
            Self::From40 => 40.0,
        }
    }
}

/// How steep the ground is at a place, degrees from level. `None` where
/// the ground across it is not all held.
#[must_use]
pub fn slope_deg(terrain: &Terrain, at: GeoPoint) -> Option<f64> {
    let half = SLOPE_BASE_M / 2.0;
    let height =
        |bearing: f64| terrain.elevation_m(Earth::FaiSphere.destination(at, bearing, half));
    let (north, east, south, west) = (height(0.0)?, height(90.0)?, height(180.0)?, height(270.0)?);
    let rise = ((east - west) / SLOPE_BASE_M).hypot((north - south) / SLOPE_BASE_M);
    Some(rise.atan().to_degrees())
}

/// The steepness over a box, for a picture of it: `columns` × `rows`
/// cells, row after row from the north-west, each read at its middle,
/// evenly in latitude and longitude between the two corners. `None` for a
/// cell whose ground is not held. Empty for no cells.
#[must_use]
pub fn slope_grid(
    terrain: &Terrain,
    north_west: GeoPoint,
    south_east: GeoPoint,
    columns: usize,
    rows: usize,
) -> Vec<Option<f64>> {
    let (height, width) = (
        north_west.lat() - south_east.lat(),
        south_east.lon() - north_west.lon(),
    );
    (0..rows)
        .flat_map(|row| (0..columns).map(move |column| (row, column)))
        .map(|(row, column)| {
            let lat = north_west.lat() - height * (row as f64 + 0.5) / rows as f64;
            let lon = north_west.lon() + width * (column as f64 + 0.5) / columns as f64;
            GeoPoint::new(lat, lon)
                .ok()
                .and_then(|place| slope_deg(terrain, place))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TileAddress;

    /// One tile of zoom 12 and 256 pixels around the Wallberg, rising to
    /// the east by `east` and to the south by `south` metres a metre.
    fn plane(east: f64, south: f64) -> (Terrain, GeoPoint) {
        let wallberg = GeoPoint::new(47.6642, 11.7967).unwrap();
        let address = TileAddress::holding(wallberg, 12).unwrap();
        // A pixel's width over the ground at this latitude on the sphere
        // the slope is measured on; the web map's pixels are square.
        let pixel_m = 2.0 * std::f64::consts::PI * 6_371_000.0 * wallberg.lat().to_radians().cos()
            / (4096.0 * 256.0);
        let mut rgba = Vec::new();
        for row in 0..256 {
            for column in 0..256 {
                let height = 1000.0 + pixel_m * (east * f64::from(column) + south * f64::from(row));
                let coded = height + 32_768.0;
                let whole = coded.floor();
                rgba.extend([
                    (whole / 256.0).floor() as u8,
                    (whole % 256.0) as u8,
                    ((coded - whole) * 256.0) as u8,
                    255,
                ]);
            }
        }
        let mut terrain = Terrain::new();
        terrain.add_pixels(address, 256, &rgba).unwrap();
        (terrain, wallberg)
    }

    #[test]
    fn the_steepness_of_a_plane_is_read_whichever_way_it_faces() {
        let (level, at) = plane(0.0, 0.0);
        assert!(slope_deg(&level, at).unwrap() < 0.01);

        for (east, south) in [(35f64.to_radians().tan(), 0.0), (0.0, -0.7), (0.5, 0.5)] {
            let (terrain, at) = plane(east, south);
            let expected = east.hypot(south).atan().to_degrees();
            let found = slope_deg(&terrain, at).unwrap();
            assert!((found - expected).abs() < 1.0, "{found} {expected}");
        }
        // Where the ground is not held there is no steepness.
        assert_eq!(slope_deg(&level, GeoPoint::new(46.0, 8.0).unwrap()), None);
    }

    #[test]
    fn steepness_falls_into_the_classes_of_avalanche_maps() {
        assert_eq!(SlopeClass::of(0.0), SlopeClass::Under25);
        assert_eq!(SlopeClass::of(24.9), SlopeClass::Under25);
        assert_eq!(SlopeClass::of(25.0), SlopeClass::From25);
        assert_eq!(SlopeClass::of(32.0), SlopeClass::From30);
        assert_eq!(SlopeClass::of(35.0), SlopeClass::From35);
        assert_eq!(SlopeClass::of(55.0), SlopeClass::From40);
        assert_eq!(SlopeClass::of(f64::NAN), SlopeClass::Under25);
        assert_eq!(SlopeClass::From30.from_deg(), 30.0);
    }

    #[test]
    fn a_box_is_read_cell_by_cell_from_the_north_west() {
        let (terrain, at) = plane(0.6, 0.0);
        let north_west = GeoPoint::new(at.lat() + 0.002, at.lon() - 0.003).unwrap();
        let south_east = GeoPoint::new(at.lat() - 0.002, at.lon() + 0.003).unwrap();
        let grid = slope_grid(&terrain, north_west, south_east, 4, 3);
        assert_eq!(grid.len(), 12);
        let expected = 0.6f64.atan().to_degrees();
        assert!(
            grid.iter()
                .all(|slope| (slope.unwrap() - expected).abs() < 1.0),
            "{grid:?}"
        );
        // A box reaching off the tile has cells without ground.
        let far = GeoPoint::new(at.lat() - 0.1, at.lon() + 0.1).unwrap();
        let grid = slope_grid(&terrain, north_west, far, 5, 5);
        assert!(grid.first().unwrap().is_some());
        assert!(grid.last().unwrap().is_none());
        assert!(slope_grid(&terrain, north_west, south_east, 0, 3).is_empty());
    }
}
