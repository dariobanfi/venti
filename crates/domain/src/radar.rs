//! Rain as a weather radar saw it: one picture of an area at one moment.
//!
//! A radar product comes on its provider's own grid, in its own projection
//! and its own units. The canonical frame is none of those. It is a grid in
//! the projection the apps' maps are drawn in (Web Mercator), over the area
//! a server collects for, and each cell holds one of a small, fixed set of
//! steps of rain rate, or the word that the radar did not see the place.
//! The steps are Venti's and the same for every radar source, so two
//! sources' layers read alike on a map; the values behind them stay with
//! the provider.

use serde::{Deserialize, Serialize};

use crate::{BoundingBox, SourceId, Timestamp};

/// From how many millimetres an hour a cell is in each step of rain, the
/// first step first. A rate below the first is no rain.
///
/// The second, third and fourth bounds are the ones the forecast map calls
/// light rain, rain and heavy rain by the hour; the first is the least a
/// radar product reports (DWD's smallest value is 0.12 mm/h), and the last
/// two are there for the cores of showers and thunderstorms, which a
/// forecast by the hour does not have. The set is ours.
pub const RAIN_STEPS_MM_H: [f64; 6] = [0.1, 0.2, 1.0, 4.0, 10.0, 30.0];

/// The value of a cell the radar saw and found dry.
pub const RADAR_DRY: u8 = 0;

/// The value of a cell the radar did not see: beyond its range, behind a
/// mountain, outside the product's grid, or marked by the provider as
/// without data. It says nothing about rain there.
pub const RADAR_UNSEEN: u8 = 7;

/// The step of rain a rate belongs to: [`RADAR_DRY`] below the first
/// bound of [`RAIN_STEPS_MM_H`], else 1 to 6. A rate that is not a number
/// is no rain seen: [`RADAR_UNSEEN`].
pub fn rain_step(mm_per_hour: f64) -> u8 {
    if mm_per_hour.is_nan() {
        return RADAR_UNSEEN;
    }
    let mut step = RADAR_DRY;
    for bound in RAIN_STEPS_MM_H {
        if mm_per_hour >= bound {
            step += 1;
        }
    }
    step
}

/// The finest step of a radar grid, in degrees of longitude: an eightieth
/// of a degree, which is 0.94 km at 47.5° N. The radar products of the
/// first region are of one kilometre; a finer grid would hold no more.
pub const RADAR_STEP_DEG: f64 = 1.0 / 80.0;

/// The most columns or rows a radar grid has. A frame is handed to a map
/// as one picture, and a picture wider than this is more than a phone's
/// map draws as one texture. A larger area gets a coarser grid.
pub const RADAR_MAX_SIDE: u32 = 2048;

/// The grid of a radar frame: cells that are square in Web Mercator, the
/// projection the apps' maps are drawn in, so that a frame lies on a map
/// as a picture held by its four corners.
///
/// Columns run from the west, rows from the north. Every column is
/// `step_deg` of longitude wide; every row is as high as a column is wide
/// on the map, which is fewer degrees of latitude the further north it
/// lies.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RadarGrid {
    /// The western edge, degrees of longitude.
    pub west: f64,
    /// The northern edge, degrees of latitude.
    pub north: f64,
    /// The width of a column, degrees of longitude.
    pub step_deg: f64,
    /// How many columns.
    pub columns: u32,
    /// How many rows.
    pub rows: u32,
}

/// A latitude on the Web Mercator map: the distance from the equator in
/// radians of longitude.
fn mercator_y(lat_deg: f64) -> f64 {
    (std::f64::consts::FRAC_PI_4 + lat_deg.to_radians() / 2.0)
        .tan()
        .ln()
}

/// The latitude of a place on the Web Mercator map.
fn latitude(mercator_y: f64) -> f64 {
    (2.0 * mercator_y.exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees()
}

impl RadarGrid {
    /// The grid over an area: from its north-west corner, as many columns
    /// and rows of [`RADAR_STEP_DEG`] as cover it, the last ones reaching a
    /// little beyond. An area that would need more than
    /// [`RADAR_MAX_SIDE`] of either gets a step twice as coarse, and so
    /// on. `None` for an area that reaches beyond 85° of latitude, where
    /// the projection has no map.
    pub fn over(area: &BoundingBox) -> Option<Self> {
        if area.north() > 85.0 || area.south() < -85.0 {
            return None;
        }
        let width = area.east() - area.west();
        let height = mercator_y(area.north()) - mercator_y(area.south());
        let mut step_deg = RADAR_STEP_DEG;
        loop {
            // A hair less than the quotient, so that an area that is a
            // whole number of steps wide gets that number.
            let columns = (width / step_deg - 1e-9).ceil().max(1.0);
            let rows = (height / step_deg.to_radians() - 1e-9).ceil().max(1.0);
            if columns <= f64::from(RADAR_MAX_SIDE) && rows <= f64::from(RADAR_MAX_SIDE) {
                // Both are whole numbers of at most 2048.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                return Some(Self {
                    west: area.west(),
                    north: area.north(),
                    step_deg,
                    columns: columns as u32,
                    rows: rows as u32,
                });
            }
            step_deg *= 2.0;
        }
    }

    /// How many cells the grid has.
    pub fn cells(&self) -> usize {
        self.columns as usize * self.rows as usize
    }

    /// The eastern edge, degrees of longitude.
    pub fn east(&self) -> f64 {
        self.west + f64::from(self.columns) * self.step_deg
    }

    /// The southern edge, degrees of latitude.
    pub fn south(&self) -> f64 {
        latitude(mercator_y(self.north) - f64::from(self.rows) * self.step_deg.to_radians())
    }

    /// The middle of a cell, as longitude and latitude in degrees.
    pub fn centre(&self, column: u32, row: u32) -> (f64, f64) {
        let lon = self.west + (f64::from(column) + 0.5) * self.step_deg;
        let y = mercator_y(self.north) - (f64::from(row) + 0.5) * self.step_deg.to_radians();
        (lon, latitude(y))
    }

    /// Whether the grid is one a frame can have: a step, a size within
    /// the bounds, and edges on the map.
    pub fn is_sound(&self) -> bool {
        self.step_deg.is_finite()
            && self.step_deg > 0.0
            && (1..=RADAR_MAX_SIDE).contains(&self.columns)
            && (1..=RADAR_MAX_SIDE).contains(&self.rows)
            && self.west.is_finite()
            && (-180.0..=180.0).contains(&self.west)
            && self.east() <= 180.0 + 1e-9
            && self.north.is_finite()
            && (-85.0..=85.0).contains(&self.north)
            && self.south() >= -85.1
    }
}

/// One picture of a rain radar: what one source saw over a grid in one
/// interval of time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarFrame {
    /// The source that delivered the frame.
    pub source: SourceId,
    /// The end of the interval the frame is of. A frame of 14:35 that
    /// covers five minutes is the rain between 14:30 and 14:35.
    pub time: Timestamp,
    /// How long the interval is, in seconds.
    pub interval_s: u32,
    /// The grid the cells lie on.
    pub grid: RadarGrid,
    /// One value for each cell, row by row from the north, each row from
    /// the west: [`RADAR_DRY`], a step of rain from 1 to 6 by
    /// [`RAIN_STEPS_MM_H`], or [`RADAR_UNSEEN`].
    pub cells: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_falls_into_the_step_whose_bound_it_reaches() {
        let steps: Vec<u8> = [
            0.0, 0.09, 0.1, 0.12, 0.2, 0.99, 1.0, 4.0, 9.9, 10.0, 30.0, 250.0,
        ]
        .into_iter()
        .map(rain_step)
        .collect();
        assert_eq!(steps, [0, 0, 1, 1, 2, 2, 3, 4, 4, 5, 6, 6]);
        assert_eq!(rain_step(f64::NAN), RADAR_UNSEEN);
        // The value for "not seen" is none of the steps.
        assert!(usize::from(RADAR_UNSEEN) > RAIN_STEPS_MM_H.len());
    }

    #[test]
    fn the_first_regions_grid_is_624_by_261_squares_of_the_map() {
        let area = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        let grid = RadarGrid::over(&area).unwrap();

        assert_eq!((grid.columns, grid.rows), (624, 261));
        assert_eq!(grid.cells(), 162_864);
        assert!((grid.east() - 17.2).abs() < 1e-9);
        // The last row reaches a little beyond the area's south.
        assert!(
            grid.south() <= 46.3 && grid.south() > 46.29,
            "{}",
            grid.south()
        );
        assert!(grid.is_sound());

        // A cell is square on the map: its height in degrees of latitude
        // is its width times the cosine of its latitude.
        let (lon, lat) = grid.centre(0, 0);
        let (_, below) = grid.centre(0, 1);
        assert!((lon - (9.4 + 0.00625)).abs() < 1e-9);
        let height = lat - below;
        assert!((height - grid.step_deg * lat.to_radians().cos()).abs() < 1e-5);
        // Rows count from the north.
        assert!(lat < 48.5 && lat > 48.49);
    }

    #[test]
    fn a_larger_area_gets_a_coarser_grid_and_the_poles_none() {
        // All of Europe: 70 degrees wide is 5,600 columns of the finest
        // step, and three doublings bring it under the bound.
        let europe = BoundingBox::new(35.0, -25.0, 72.0, 45.0).unwrap();
        let grid = RadarGrid::over(&europe).unwrap();
        assert!((grid.step_deg - 0.05).abs() < 1e-12);
        assert_eq!(grid.columns, 1400);
        assert!(grid.rows <= RADAR_MAX_SIDE);
        assert!(grid.is_sound());

        let polar = BoundingBox::new(80.0, 0.0, 89.0, 10.0).unwrap();
        assert_eq!(RadarGrid::over(&polar), None);
    }
}
