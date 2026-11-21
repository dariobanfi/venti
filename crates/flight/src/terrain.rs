//! The height of the ground, from elevation tiles.
//!
//! Elevation tiles are the map's own: square images cut on the web map's
//! grid, in which each pixel's colour is a height. The same tiles shade
//! the map's relief. Here they answer how high the ground is under a
//! place, which the cockpit needs for the height above ground, for
//! airspace whose limits are given above ground, and for seeing whether a
//! glide clears a ridge.
//!
//! The encoding is the one called Terrarium: a pixel's height in metres is
//! red × 256 + green + blue / 256 − 32768.
//!
//! Nothing here reads a file or decodes an image: whoever has a tile
//! hands in its pixels.

use std::collections::BTreeMap;

use venti_domain::GeoPoint;

/// The web map's grid ends at this latitude, north and south.
const GRID_EDGE_DEG: f64 = 85.051_128_78;

/// One tile of the web map's grid: zoom, column from the west, row from
/// the north.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileAddress {
    /// The zoom level: the world is 2^z tiles wide.
    pub z: u8,
    /// The column, counted from 180° west.
    pub x: u32,
    /// The row, counted from the north.
    pub y: u32,
}

impl TileAddress {
    /// The tile of a zoom level that holds a place. `None` beyond the
    /// grid's edge near the poles, and for a zoom the grid does not have.
    pub fn holding(position: GeoPoint, z: u8) -> Option<Self> {
        let (x, y) = grid_position(position, z)?;
        Some(Self {
            z,
            x: x.floor() as u32,
            y: y.floor() as u32,
        })
    }

    /// The tiles of a zoom level that cover a box of latitudes and
    /// longitudes, row by row from the north-west. A box that reaches
    /// beyond the grid's edge near a pole is cut off there. Empty for a
    /// zoom the grid does not have, for numbers that are none, and for a
    /// box whose west lies east of its east: one across the date line is
    /// not put together here.
    pub fn covering(south: f64, west: f64, north: f64, east: f64, z: u8) -> Vec<Self> {
        if !(south <= north && west <= east) || z > 22 {
            return Vec::new();
        }
        let corner = |lat: f64, lon: f64| {
            GeoPoint::new(lat.clamp(-GRID_EDGE_DEG, GRID_EDGE_DEG), lon)
                .ok()
                .and_then(|place| Self::holding(place, z))
        };
        let (Some(first), Some(last)) = (corner(north, west), corner(south, east)) else {
            return Vec::new();
        };
        // The east edge of the grid belongs to its first column: a box
        // that ends there ends in the last.
        let last_x = if last.x < first.x {
            (1_u32 << z) - 1
        } else {
            last.x
        };
        (first.y..=last.y)
            .flat_map(|y| (first.x..=last_x).map(move |x| Self { z, x, y }))
            .collect()
    }

    /// The tiles of this tile's zoom level around it, itself included: a
    /// square of `2 × rings + 1` tiles a side, the nearest first. Tiles
    /// beyond the grid's north or south edge are left out; east and west
    /// the grid goes around.
    pub fn with_neighbours(self, rings: u32) -> Vec<Self> {
        let width = 1_i64 << self.z;
        let rings = i64::from(rings);
        let mut tiles = Vec::new();
        for down in -rings..=rings {
            for right in -rings..=rings {
                let y = i64::from(self.y) + down;
                if !(0..width).contains(&y) {
                    continue;
                }
                let x = (i64::from(self.x) + right).rem_euclid(width);
                tiles.push((
                    down.abs().max(right.abs()),
                    down.abs() + right.abs(),
                    Self {
                        z: self.z,
                        x: x as u32,
                        y: y as u32,
                    },
                ));
            }
        }
        tiles.sort();
        // On a grid narrower than the square, going around east and west
        // comes to the same tile twice: the nearer of the two stands.
        let mut seen = std::collections::BTreeSet::new();
        tiles
            .into_iter()
            .map(|(_, _, tile)| tile)
            .filter(|tile| seen.insert(*tile))
            .collect()
    }
}

/// Where a place lies on the grid of a zoom level, in tiles from the
/// grid's north-west corner, with fractions.
fn grid_position(position: GeoPoint, z: u8) -> Option<(f64, f64)> {
    if z > 22 || position.lat().abs() > GRID_EDGE_DEG {
        return None;
    }
    let tiles = f64::from(1_u32 << z);
    let x = (position.lon() + 180.0) / 360.0 * tiles;
    let latitude = position.lat().to_radians();
    let y =
        (1.0 - (latitude.tan() + 1.0 / latitude.cos()).ln() / std::f64::consts::PI) / 2.0 * tiles;
    // The east edge belongs to the first column again.
    Some((
        x.rem_euclid(tiles),
        y.clamp(0.0, tiles - f64::EPSILON * tiles),
    ))
}

/// Why pixels were not taken as an elevation tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TileError {
    /// The image is not square, is empty, or is larger than a tile is.
    #[error("an elevation tile is a square image of at most 1024 pixels a side")]
    Shape,
    /// The pixels given are not the image's width × height × 4 bytes.
    #[error("the pixels do not fill the image")]
    Length,
    /// The tile's address is not on the grid.
    #[error("there is no such tile on the map's grid")]
    Address,
}

/// One tile's heights, a metre apart at the finest: a row after the
/// other from the north-west corner.
#[derive(Debug, Clone)]
struct Tile {
    side: usize,
    heights: Vec<i16>,
}

impl Tile {
    fn at(&self, column: usize, row: usize) -> f64 {
        f64::from(self.heights[row.min(self.side - 1) * self.side + column.min(self.side - 1)])
    }
}

/// The ground under a region: elevation tiles, and the height they give
/// for a place.
#[derive(Debug, Clone, Default)]
pub struct Terrain {
    tiles: BTreeMap<TileAddress, Tile>,
    /// The zoom levels of the tiles held, the finest first: a height is
    /// asked for thousands of times a second, and this is not put
    /// together anew each time.
    zooms: Vec<u8>,
}

impl Terrain {
    /// A terrain that knows no ground yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes one tile's pixels: red, green, blue and opacity for each, a
    /// row after the other from the north-west corner, in the Terrarium
    /// encoding. A tile held already is replaced.
    ///
    /// # Errors
    ///
    /// When the image is not a square of the size given, or the address
    /// is not on the grid.
    pub fn add_pixels(
        &mut self,
        address: TileAddress,
        side: u32,
        rgba: &[u8],
    ) -> Result<(), TileError> {
        if address.z > 22 || u64::from(address.x.max(address.y)) >= (1_u64 << address.z) {
            return Err(TileError::Address);
        }
        if side == 0 || side > 1024 {
            return Err(TileError::Shape);
        }
        let side = side as usize;
        if rgba.len() != side * side * 4 {
            return Err(TileError::Length);
        }
        let heights = rgba
            .chunks_exact(4)
            .map(|pixel| {
                let metres =
                    f64::from(pixel[0]) * 256.0 + f64::from(pixel[1]) + f64::from(pixel[2]) / 256.0
                        - 32_768.0;
                // Nothing on Earth is outside what sixteen bits hold.
                metres
                    .round()
                    .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
            })
            .collect();
        self.tiles.insert(address, Tile { side, heights });
        self.note_zooms();
        Ok(())
    }

    fn note_zooms(&mut self) {
        let mut zooms: Vec<u8> = self.tiles.keys().map(|address| address.z).collect();
        zooms.sort_unstable();
        zooms.dedup();
        zooms.reverse();
        self.zooms = zooms;
    }

    /// Whether a tile is held.
    pub fn has(&self, address: TileAddress) -> bool {
        self.tiles.contains_key(&address)
    }

    /// How many tiles are held.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether no tile is held.
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Forgets the tiles that are further than `rings` tiles from the
    /// one that holds a place, at their own zoom level. For keeping what
    /// is held to what a flight can still need.
    pub fn keep_around(&mut self, position: GeoPoint, rings: u32) {
        self.tiles.retain(|address, _| {
            let Some(here) = TileAddress::holding(position, address.z) else {
                return false;
            };
            let width = 1_i64 << address.z;
            let across = (i64::from(address.x) - i64::from(here.x)).rem_euclid(width);
            let across = across.min(width - across);
            let down = (i64::from(address.y) - i64::from(here.y)).abs();
            across.max(down) <= i64::from(rings)
        });
        self.note_zooms();
    }

    /// The height of the ground under a place, metres above mean sea
    /// level, from the finest tile held that covers it. `None` where no
    /// tile is held.
    ///
    /// Between the pixels the height is the mean of the four around,
    /// weighed by how near each is. A pixel's height stands for its
    /// middle. Beyond the middles of a tile's outermost pixels the
    /// outermost pixel's height is taken: at the seam between two tiles
    /// the height may step by what the ground changes over half a pixel.
    pub fn elevation_m(&self, position: GeoPoint) -> Option<f64> {
        // The finest zoom first.
        self.zooms.iter().copied().find_map(|z| {
            let (x, y) = grid_position(position, z)?;
            let address = TileAddress {
                z,
                x: x.floor() as u32,
                y: y.floor() as u32,
            };
            let tile = self.tiles.get(&address)?;
            let side = tile.side as f64;
            // In pixels, measured from the middle of the first pixel.
            let column = ((x - x.floor()) * side - 0.5).clamp(0.0, side - 1.0);
            let row = ((y - y.floor()) * side - 0.5).clamp(0.0, side - 1.0);
            let (left, top) = (column.floor() as usize, row.floor() as usize);
            let (right_share, down_share) = (column - column.floor(), row - row.floor());
            let north =
                tile.at(left, top) * (1.0 - right_share) + tile.at(left + 1, top) * right_share;
            let south = tile.at(left, top + 1) * (1.0 - right_share)
                + tile.at(left + 1, top + 1) * right_share;
            Some(north * (1.0 - down_share) + south * down_share)
        })
    }

    /// The ground along a straight line, at `count` places a `step_m`
    /// apart, the first of them at its start. `None` at each place no tile
    /// covers. `bearing_deg` is the direction at the start, clockwise from
    /// north; over the tens of kilometres a glide is long, a line of one
    /// bearing and a straight one are the same.
    pub fn profile(
        &self,
        from: GeoPoint,
        bearing_deg: f64,
        step_m: f64,
        count: usize,
    ) -> Vec<Option<f64>> {
        self.along(from, bearing_deg, step_m).take(count).collect()
    }

    /// The ground along a straight line, a `step_m` apart from its start
    /// on, read only as far as it is asked for: [`Terrain::profile`]
    /// without an end. For a line whose end is found on the way.
    pub fn along(
        &self,
        from: GeoPoint,
        bearing_deg: f64,
        step_m: f64,
    ) -> impl Iterator<Item = Option<f64>> + '_ {
        // Metres to degrees at this latitude: fine for a line this short.
        const METRES_PER_DEGREE: f64 = 111_194.9;
        let (east, north) = bearing_deg.to_radians().sin_cos();
        let across = from.lat().to_radians().cos().max(1e-6);
        (0_u32..).map(move |index| {
            let along = step_m * f64::from(index);
            GeoPoint::new(
                from.lat() + along * north / METRES_PER_DEGREE,
                from.lon() + along * east / (METRES_PER_DEGREE * across),
            )
            .ok()
            .and_then(|place| self.elevation_m(place))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    /// A tile's pixels for heights given by column and row.
    fn pixels(side: u32, height: impl Fn(u32, u32) -> f64) -> Vec<u8> {
        let mut rgba = Vec::new();
        for row in 0..side {
            for column in 0..side {
                let coded = height(column, row) + 32_768.0;
                let whole = coded.floor();
                rgba.extend([
                    (whole / 256.0).floor() as u8,
                    (whole % 256.0) as u8,
                    ((coded - whole) * 256.0) as u8,
                    255,
                ]);
            }
        }
        rgba
    }

    #[test]
    fn a_place_is_found_on_the_grid_as_the_web_map_cuts_it() {
        // The Wallberg, at zoom 12: column 2182, row 1429, as any map
        // library computes it.
        let wallberg = place(47.6642, 11.7967);
        assert_eq!(
            TileAddress::holding(wallberg, 12),
            Some(TileAddress {
                z: 12,
                x: 2182,
                y: 1429
            })
        );
        // The whole world in one tile, and its four quarters.
        assert_eq!(
            TileAddress::holding(wallberg, 0),
            Some(TileAddress { z: 0, x: 0, y: 0 })
        );
        assert_eq!(
            TileAddress::holding(place(-33.9, 151.2), 1),
            Some(TileAddress { z: 1, x: 1, y: 1 })
        );
        assert_eq!(
            TileAddress::holding(place(0.0, 180.0), 3),
            Some(TileAddress { z: 3, x: 0, y: 4 })
        );
        // Beyond the grid's edge there is no tile.
        assert_eq!(TileAddress::holding(place(89.0, 0.0), 5), None);
        assert_eq!(TileAddress::holding(wallberg, 30), None);
    }

    #[test]
    fn the_tiles_that_cover_a_box_are_listed_row_by_row() {
        // The sample flight's valley: two tiles wide and two high at
        // zoom 12, one tile at zoom 10.
        let fine = TileAddress::covering(47.66, 11.74, 47.70, 11.80, 12);
        assert_eq!(
            fine.iter().map(|tile| (tile.x, tile.y)).collect::<Vec<_>>(),
            [(2181, 1428), (2182, 1428), (2181, 1429), (2182, 1429)]
        );
        assert_eq!(
            TileAddress::covering(47.66, 11.74, 47.70, 11.80, 10),
            [TileAddress {
                z: 10,
                x: 545,
                y: 357
            }]
        );
        // A place is covered by its own tile.
        assert_eq!(
            TileAddress::covering(47.6642, 11.7967, 47.6642, 11.7967, 12).len(),
            1
        );
        // A box up to the pole is cut off at the grid's edge, and one
        // that ends at the date line ends in the last column.
        assert_eq!(TileAddress::covering(80.0, 170.0, 90.0, 180.0, 3).len(), 1);
        // Nothing for a box that is none.
        assert!(TileAddress::covering(48.0, 11.0, 47.0, 12.0, 12).is_empty());
        assert!(TileAddress::covering(47.0, 12.0, 48.0, 11.0, 12).is_empty());
        assert!(TileAddress::covering(f64::NAN, 11.0, 48.0, 12.0, 12).is_empty());
    }

    #[test]
    fn the_tiles_around_one_are_the_nearest_first() {
        let here = TileAddress {
            z: 12,
            x: 2182,
            y: 1429,
        };
        let around = here.with_neighbours(2);
        assert_eq!(around.len(), 25);
        assert_eq!(around[0], here);
        // The four that share a side come before the four at the corners.
        assert!(
            around[1..5]
                .iter()
                .all(|tile| tile.x.abs_diff(here.x) + tile.y.abs_diff(here.y) == 1)
        );
        assert!(
            around[5..9]
                .iter()
                .all(|tile| tile.x.abs_diff(here.x) == 1 && tile.y.abs_diff(here.y) == 1)
        );

        // At the top of the grid there is nothing further north, and east
        // of the last column comes the first.
        let corner = TileAddress { z: 2, x: 3, y: 0 };
        let around = corner.with_neighbours(1);
        assert_eq!(around.len(), 6);
        assert!(around.contains(&TileAddress { z: 2, x: 0, y: 1 }));
        // A grid smaller than the square asked for is given once.
        assert_eq!(TileAddress { z: 1, x: 0, y: 0 }.with_neighbours(3).len(), 4);
    }

    #[test]
    fn heights_are_read_from_the_colours_and_mixed_between_pixels() {
        let wallberg = place(47.6642, 11.7967);
        let address = TileAddress::holding(wallberg, 12).unwrap();
        let mut terrain = Terrain::new();
        assert_eq!(terrain.elevation_m(wallberg), None);

        // A slope that rises ten metres a pixel to the east and falls two
        // a pixel to the south, from 800 m in the north-west corner.
        terrain
            .add_pixels(
                address,
                256,
                &pixels(256, |column, row| {
                    800.0 + 10.0 * f64::from(column) - 2.0 * f64::from(row)
                }),
            )
            .unwrap();
        assert!(terrain.has(address));
        assert_eq!(terrain.len(), 1);

        // Where the place lies in its tile, in pixels from the middle of
        // the first.
        let (x, y) = grid_position(wallberg, 12).unwrap();
        let (column, row) = ((x - x.floor()) * 256.0 - 0.5, (y - y.floor()) * 256.0 - 0.5);
        let expected = 800.0 + 10.0 * column - 2.0 * row;
        let found = terrain.elevation_m(wallberg).unwrap();
        assert!((found - expected).abs() < 0.01, "{found} {expected}");

        // Next door nothing is known.
        assert_eq!(terrain.elevation_m(place(47.6642, 11.90)), None);

        // Below sea level, and a height with a fraction: rounded to the
        // metre.
        terrain
            .add_pixels(address, 2, &pixels(2, |_, _| -12.4))
            .unwrap();
        assert_eq!(terrain.elevation_m(wallberg), Some(-12.0));
    }

    #[test]
    fn the_finest_tile_that_covers_a_place_answers() {
        let wallberg = place(47.6642, 11.7967);
        let mut terrain = Terrain::new();
        terrain
            .add_pixels(
                TileAddress::holding(wallberg, 9).unwrap(),
                4,
                &pixels(4, |_, _| 900.0),
            )
            .unwrap();
        assert_eq!(terrain.elevation_m(wallberg), Some(900.0));
        terrain
            .add_pixels(
                TileAddress::holding(wallberg, 12).unwrap(),
                4,
                &pixels(4, |_, _| 1620.0),
            )
            .unwrap();
        assert_eq!(terrain.elevation_m(wallberg), Some(1620.0));
        // Ten kilometres east the fine tile has ended and the coarse one
        // still answers.
        assert_eq!(terrain.elevation_m(place(47.6642, 11.93)), Some(900.0));

        // Tiles away from where the glider is are forgotten, each at its
        // own zoom level: the coarse one holds both places.
        terrain.keep_around(place(47.6642, 11.93), 0);
        assert_eq!(terrain.len(), 1);
        assert_eq!(terrain.elevation_m(wallberg), Some(900.0));
    }

    #[test]
    fn pixels_that_are_no_tile_are_refused() {
        let mut terrain = Terrain::new();
        let address = TileAddress { z: 3, x: 1, y: 2 };
        assert_eq!(
            terrain.add_pixels(address, 2, &[0; 15]),
            Err(TileError::Length)
        );
        assert_eq!(terrain.add_pixels(address, 0, &[]), Err(TileError::Shape));
        assert_eq!(
            terrain.add_pixels(address, 2048, &[]),
            Err(TileError::Shape)
        );
        assert_eq!(
            terrain.add_pixels(TileAddress { z: 3, x: 8, y: 0 }, 1, &[0; 4]),
            Err(TileError::Address)
        );
        assert!(terrain.is_empty());
    }

    #[test]
    fn the_ground_along_a_line_is_read_place_by_place() {
        let wallberg = place(47.6642, 11.7967);
        let address = TileAddress::holding(wallberg, 12).unwrap();
        let mut terrain = Terrain::new();
        // Rising a metre a pixel to the east: at this latitude a pixel of
        // a 256-pixel tile at zoom 12 is 25.7 m wide.
        terrain
            .add_pixels(address, 256, &pixels(256, |column, _| f64::from(column)))
            .unwrap();
        let eastwards = terrain.profile(wallberg, 90.0, 257.4, 4);
        let heights: Vec<f64> = eastwards.iter().map(|height| height.unwrap()).collect();
        for pair in heights.windows(2) {
            assert!((pair[1] - pair[0] - 10.0).abs() < 0.2, "{heights:?}");
        }
        // Northwards the height stays, until the tile ends.
        let northwards = terrain.profile(wallberg, 0.0, 1000.0, 12);
        assert!((northwards[1].unwrap() - heights[0]).abs() < 0.01);
        assert_eq!(northwards[11], None);
    }
}
