//! The ground under the glider, from the map's own elevation tiles.
//!
//! The shell fetches tiles, or has them in an archive that came with the
//! app or was downloaded for flying without a network, and hands them
//! over as they are. The core reads the pixels and answers how high the
//! ground is. See [`venti_flight::terrain`] for how a height is read from
//! a tile.

use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex, MutexGuard};

use venti_domain::GeoPoint;
use venti_flight::terrain::{self as model, TileAddress};

use crate::CoreError;

/// The zoom level of the tiles the cockpit works with. A pixel of a tile
/// of 512 pixels is 13 m wide at the latitude of the Alps: fine enough to
/// say how high over the ground a glider is, and to see a ridge in the
/// way of a glide. To verify against known summits and against a pressure
/// altitude on the ground.
pub(crate) const ZOOM: u8 = 12;

/// Tiles this many steps around the one the glider is over are wanted:
/// five by five, which reaches at least 13 km from the glider every way.
/// The side view looks 10 km ahead.
const RINGS_WANTED: u32 = 2;

/// Tiles are kept until they are further than this from the glider: nine
/// by nine, about 40 MB of heights at the most.
const RINGS_KEPT: u32 = 4;

/// From an archive, tiles coarser than this are not taken: a pixel of
/// zoom 10 is 50 m wide, and what is coarser says little about the ground
/// under one glider.
const COARSEST_ZOOM: u8 = 10;

/// An image larger than this is no elevation tile.
const IMAGE_AT_MOST_BYTES: usize = 4 * 1024 * 1024;

/// An archive is read whole from memory, so only a small one is taken:
/// the region that comes with the app. Reading a country's archive piece
/// by piece from its file is not built.
const ARCHIVE_AT_MOST_BYTES: usize = 64 * 1024 * 1024;

/// One elevation tile to fetch.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TerrainTile {
    /// The zoom level.
    pub z: u8,
    /// The column, counted from 180° west.
    pub x: u32,
    /// The row, counted from the north.
    pub y: u32,
    /// Zoom, column and row as a tile server's address has them:
    /// "12/2182/1429".
    pub path: String,
}

/// The height of the ground, as far as tiles for it were handed over.
#[derive(Debug, Default, uniffi::Object)]
pub struct Terrain {
    model: Mutex<model::Terrain>,
}

#[uniffi::export]
impl Terrain {
    /// A terrain that knows no ground yet.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The tiles to have for flying around a place, the nearest first,
    /// without those that are held already. Empty when all are there, and
    /// for a place the map's grid does not reach.
    pub fn tiles_wanted(&self, lat: f64, lon: f64) -> Vec<TerrainTile> {
        let Some(here) = GeoPoint::new(lat, lon)
            .ok()
            .and_then(|position| TileAddress::holding(position, ZOOM))
        else {
            return Vec::new();
        };
        let model = self.lock();
        here.with_neighbours(RINGS_WANTED)
            .into_iter()
            .filter(|tile| !model.has(*tile))
            .map(|tile| TerrainTile {
                z: tile.z,
                x: tile.x,
                y: tile.y,
                path: format!("{}/{}/{}", tile.z, tile.x, tile.y),
            })
            .collect()
    }

    /// Takes one tile as a tile server sends it: a WebP image in the
    /// Terrarium encoding.
    ///
    /// # Errors
    ///
    /// When the bytes are no such image.
    pub fn add_tile(&self, z: u8, x: u32, y: u32, image: Vec<u8>) -> Result<(), CoreError> {
        let (side, rgba) = pixels(&image)?;
        self.lock()
            .add_pixels(TileAddress { z, x, y }, side, &rgba)
            .map_err(|error| not_a_tile(&error.to_string()))
    }

    /// Takes the tiles of an archive in the PMTiles format, version 3:
    /// the region that comes with the app. Returns how many were taken.
    /// Tiles too coarse to say anything about the ground under a glider
    /// are left.
    ///
    /// # Errors
    ///
    /// When the bytes are no such archive, or its tiles are not WebP
    /// images.
    pub fn add_archive(&self, archive: Vec<u8>) -> Result<u32, CoreError> {
        let tiles = archive_tiles(&archive)?;
        let mut taken = 0;
        for (address, image) in tiles {
            if address.z < COARSEST_ZOOM {
                continue;
            }
            let (side, rgba) = pixels(image)?;
            self.lock()
                .add_pixels(address, side, &rgba)
                .map_err(|error| not_a_tile(&error.to_string()))?;
            taken += 1;
        }
        Ok(taken)
    }

    /// The height of the ground under a place, metres above mean sea
    /// level. `None` where no tile is held.
    pub fn elevation(&self, lat: f64, lon: f64) -> Option<f64> {
        self.elevation_at(GeoPoint::new(lat, lon).ok()?)
    }

    /// How many tiles are held.
    pub fn tile_count(&self) -> u32 {
        u32::try_from(self.lock().len()).unwrap_or(u32::MAX)
    }

    /// Forgets the tiles far from a place, so that a long flight does not
    /// fill the memory with the ground it has left behind.
    pub fn keep_around(&self, lat: f64, lon: f64) {
        if let Ok(position) = GeoPoint::new(lat, lon) {
            self.lock().keep_around(position, RINGS_KEPT);
        }
    }
}

impl Terrain {
    /// The height of the ground under a place.
    pub(crate) fn elevation_at(&self, position: GeoPoint) -> Option<f64> {
        self.lock().elevation_m(position)
    }

    /// The ground along a line: see [`model::Terrain::profile`].
    pub(crate) fn profile(
        &self,
        from: GeoPoint,
        bearing_deg: f64,
        step_m: f64,
        count: usize,
    ) -> Vec<Option<f64>> {
        self.lock().profile(from, bearing_deg, step_m, count)
    }

    fn lock(&self) -> MutexGuard<'_, model::Terrain> {
        self.model
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn not_a_tile(why: &str) -> CoreError {
    CoreError::InvalidFile {
        message: format!("This is not an elevation tile: {why}."),
    }
}

fn not_an_archive(why: &str) -> CoreError {
    CoreError::InvalidFile {
        message: format!("This is not an archive of elevation tiles: {why}."),
    }
}

/// Decodes a WebP image to its side and its pixels as red, green, blue
/// and opacity.
fn pixels(image: &[u8]) -> Result<(u32, Vec<u8>), CoreError> {
    if image.len() > IMAGE_AT_MOST_BYTES {
        return Err(not_a_tile("the image is too large"));
    }
    let unreadable = |_| not_a_tile("it is no WebP image that can be read");
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(image)).map_err(unreadable)?;
    let (width, height) = decoder.dimensions();
    if width != height || width == 0 || width > 1024 {
        return Err(not_a_tile(
            "it is not a square image of at most 1024 pixels a side",
        ));
    }
    let size = decoder
        .output_buffer_size()
        .ok_or_else(|| not_a_tile("the image is too large"))?;
    let mut decoded = vec![0; size];
    decoder.read_image(&mut decoded).map_err(unreadable)?;
    let rgba = if decoder.has_alpha() {
        decoded
    } else {
        decoded
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect()
    };
    Ok((width, rgba))
}

/// The tiles of a PMTiles archive, version 3, with their addresses: each
/// as a slice of the archive.
///
/// The archive begins with 127 bytes that say where its parts are. Its
/// directory lists runs of tiles by their number along a curve that
/// visits every tile of every zoom level, and where each tile's bytes
/// are; a large archive's directory points to further directories.
fn archive_tiles(archive: &[u8]) -> Result<Vec<(TileAddress, &[u8])>, CoreError> {
    if archive.len() > ARCHIVE_AT_MOST_BYTES {
        return Err(not_an_archive(
            "it is too large to be read whole; only the region that comes with the app is read this way",
        ));
    }
    if archive.len() < 127 || &archive[..7] != b"PMTiles" {
        return Err(not_an_archive(
            "it does not begin as a PMTiles archive does",
        ));
    }
    if archive[7] != 3 {
        return Err(not_an_archive("it is not of version 3 of the format"));
    }
    let number = |at: usize| {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&archive[at..at + 8]);
        usize::try_from(u64::from_le_bytes(bytes)).unwrap_or(usize::MAX)
    };
    let (root_at, root_length) = (number(8), number(16));
    let (leaves_at, leaves_length) = (number(40), number(48));
    let (tiles_at, tiles_length) = (number(56), number(64));
    let (directories_packed, tiles_packed, tile_kind) = (archive[97], archive[98], archive[99]);
    // 1 is "not compressed", 2 is gzip; a tile kind of 4 is WebP.
    if !(1..=2).contains(&directories_packed) {
        return Err(not_an_archive(
            "its directory is packed in a way this app does not read",
        ));
    }
    if tiles_packed != 1 || tile_kind != 4 {
        return Err(not_an_archive("its tiles are not plain WebP images"));
    }
    let part = |at: usize, length: usize| {
        at.checked_add(length)
            .and_then(|end| archive.get(at..end))
            .ok_or_else(|| not_an_archive("it is cut short"))
    };

    let mut tiles = Vec::new();
    // The root directory, and the directories it points to. A directory
    // that points back would go round for ever: no more are followed than
    // an archive of this size can hold.
    let mut waiting = vec![part(root_at, root_length)?];
    let mut followed = 0;
    while let Some(packed) = waiting.pop() {
        followed += 1;
        if followed > 4096 {
            return Err(not_an_archive("its directories do not end"));
        }
        for entry in directory(packed, directories_packed == 2)? {
            if entry.run == 0 {
                let leaf = part(leaves_at, leaves_length)?;
                let slice = entry
                    .offset
                    .checked_add(entry.length)
                    .and_then(|end| leaf.get(entry.offset..end))
                    .ok_or_else(|| not_an_archive("it is cut short"))?;
                waiting.push(slice);
                continue;
            }
            let data = part(tiles_at, tiles_length)?;
            let image = entry
                .offset
                .checked_add(entry.length)
                .and_then(|end| data.get(entry.offset..end))
                .ok_or_else(|| not_an_archive("it is cut short"))?;
            for step in 0..entry.run {
                let address = address_of(entry.first + step)
                    .ok_or_else(|| not_an_archive("it names a tile the map does not have"))?;
                tiles.push((address, image));
                if tiles.len() > 100_000 {
                    return Err(not_an_archive("it holds more tiles than are read at once"));
                }
            }
        }
    }
    Ok(tiles)
}

/// One line of a directory: a run of tiles that share one image, or with
/// a run of zero a further directory.
struct Entry {
    first: u64,
    run: u64,
    length: usize,
    offset: usize,
}

/// Reads one directory: how many entries, then for all of them the tile
/// numbers (each as the step from the one before), the runs, the lengths
/// and the offsets, where a zero means "right after the entry before".
fn directory(packed: &[u8], is_gzip: bool) -> Result<Vec<Entry>, CoreError> {
    let cut_short = || not_an_archive("its directory is cut short");
    let unpacked;
    let bytes = if is_gzip {
        let mut plain = Vec::new();
        flate2::read::GzDecoder::new(packed)
            .take(ARCHIVE_AT_MOST_BYTES as u64)
            .read_to_end(&mut plain)
            .map_err(|_| not_an_archive("its directory cannot be unpacked"))?;
        unpacked = plain;
        unpacked.as_slice()
    } else {
        packed
    };

    let mut at = 0;
    let mut next = || -> Result<u64, CoreError> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = *bytes.get(at).ok_or_else(cut_short)?;
            at += 1;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(cut_short())
    };
    let count = usize::try_from(next()?).map_err(|_| cut_short())?;
    // Every entry takes four numbers of at least a byte each.
    if count > bytes.len() / 4 {
        return Err(cut_short());
    }
    let mut entries = Vec::with_capacity(count);
    let mut tile = 0_u64;
    for _ in 0..count {
        tile = tile.checked_add(next()?).ok_or_else(cut_short)?;
        entries.push(Entry {
            first: tile,
            run: 0,
            length: 0,
            offset: 0,
        });
    }
    for entry in &mut entries {
        entry.run = next()?;
    }
    for entry in &mut entries {
        entry.length = usize::try_from(next()?).map_err(|_| cut_short())?;
    }
    let mut before: Option<(usize, usize)> = None;
    for entry in &mut entries {
        let written = usize::try_from(next()?).map_err(|_| cut_short())?;
        entry.offset = match (written, before) {
            (0, Some((offset, length))) => offset.checked_add(length).ok_or_else(cut_short)?,
            (0, None) => return Err(cut_short()),
            (written, _) => written - 1,
        };
        before = Some((entry.offset, entry.length));
    }
    Ok(entries)
}

/// The tile a number stands for. The numbers count the tiles of zoom 0,
/// then of zoom 1, and so on, each level along a Hilbert curve, which
/// keeps tiles that lie together on the map together in the file.
fn address_of(number: u64) -> Option<TileAddress> {
    let (mut z, mut first) = (0_u8, 0_u64);
    loop {
        let on_level = 1_u64 << (2 * u32::from(z));
        if number < first + on_level {
            break;
        }
        first += on_level;
        z += 1;
        if z > 22 {
            return None;
        }
    }
    let (mut x, mut y) = (0_u64, 0_u64);
    let mut rest = number - first;
    let mut side = 1_u64;
    while side < (1_u64 << z) {
        let right = 1 & (rest / 2);
        let down = 1 & (rest ^ right);
        if down == 0 {
            if right == 1 {
                x = side - 1 - x;
                y = side - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += side * right;
        y += side * down;
        rest /= 4;
        side *= 2;
    }
    Some(TileAddress {
        z,
        x: u32::try_from(x).ok()?,
        y: u32::try_from(y).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The region that comes with the iPhone app: Mapterhorn's elevation
    /// around the Tegernsee, twelve tiles of zoom 12 among thirty.
    fn bundled() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/MapDemo/terrain.pmtiles"
        ))
        .unwrap()
    }

    fn words(result: Result<impl std::fmt::Debug, CoreError>) -> String {
        match result {
            Err(CoreError::InvalidFile { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_region_that_comes_with_the_app_gives_the_height_of_the_ground() {
        let terrain = Terrain::new();
        assert_eq!(terrain.elevation(47.6642, 11.7967), None);
        assert_eq!(terrain.tile_count(), 0);

        // Twelve tiles of zoom 12, six of zoom 11 and two of zoom 10; the
        // ten coarser ones are left.
        assert_eq!(terrain.add_archive(bundled()).unwrap(), 20);
        assert_eq!(terrain.tile_count(), 20);

        // The Wallberg's summit is given as 1,722 m, and the Tegernsee
        // as lying at 725 m. The lake is flat; the summit is a point in
        // pixels 13 m wide, which hold it five metres lower. The sample
        // flight's take-off below the summit is at 1,620 m.
        let summit = terrain.elevation(47.6662, 11.7982).unwrap();
        assert!((summit - 1722.0).abs() < 10.0, "{summit}");
        let lake = terrain.elevation(47.7130, 11.7400).unwrap();
        assert!((lake - 725.0).abs() < 1.0, "{lake}");
        let takeoff = terrain.elevation(47.6642, 11.7967).unwrap();
        assert!((takeoff - 1620.0).abs() < 5.0, "{takeoff}");

        // South of the region the app has no ground.
        assert_eq!(terrain.elevation(47.40, 11.79), None);
    }

    #[test]
    fn the_tiles_around_a_place_are_wanted_until_they_are_there() {
        let terrain = Terrain::new();
        let wanted = terrain.tiles_wanted(47.6642, 11.7967);
        assert_eq!(wanted.len(), 25);
        // The one the place is in comes first.
        assert_eq!(
            wanted[0],
            TerrainTile {
                z: 12,
                x: 2182,
                y: 1429,
                path: "12/2182/1429".to_owned()
            }
        );
        // The region that comes with the app has the tile itself and the
        // eleven west of it and around; none to its east.
        terrain.add_archive(bundled()).unwrap();
        let wanted = terrain.tiles_wanted(47.6642, 11.7967);
        assert!(wanted.len() < 25 && !wanted.is_empty(), "{}", wanted.len());
        assert!(
            wanted
                .iter()
                .all(|tile| !(tile.x <= 2182 && tile.x >= 2180 && (1427..=1430).contains(&tile.y)))
        );
        // Nothing is wanted where the map's grid ends.
        assert!(terrain.tiles_wanted(89.5, 0.0).is_empty());
        assert!(terrain.tiles_wanted(f64::NAN, 0.0).is_empty());

        // Far from the glider the tiles are let go, at every zoom level.
        terrain.keep_around(46.0, 8.0);
        assert_eq!(terrain.tile_count(), 0);
    }

    #[test]
    fn one_tile_is_taken_as_the_tile_server_sends_it() {
        // The first tile of zoom 12 in the archive, cut out of it.
        let archive = bundled();
        let tiles = archive_tiles(&archive).unwrap();
        assert_eq!(tiles.len(), 30);
        let (address, image) = tiles
            .iter()
            .find(|(address, _)| (address.z, address.x, address.y) == (12, 2182, 1429))
            .unwrap();
        let terrain = Terrain::new();
        terrain
            .add_tile(address.z, address.x, address.y, image.to_vec())
            .unwrap();
        assert_eq!(terrain.tile_count(), 1);
        let summit = terrain.elevation(47.6662, 11.7982).unwrap();
        assert!((summit - 1722.0).abs() < 10.0, "{summit}");

        // What is no image, or no tile of the grid, is refused in words.
        assert_eq!(
            words(terrain.add_tile(12, 0, 0, b"not an image".to_vec())),
            "This is not an elevation tile: it is no WebP image that can be read."
        );
        assert_eq!(
            words(terrain.add_tile(3, 9, 0, image.to_vec())),
            "This is not an elevation tile: there is no such tile on the map's grid."
        );
        assert_eq!(terrain.tile_count(), 1);
    }

    #[test]
    fn an_archive_that_is_none_or_is_cut_short_is_refused() {
        let terrain = Terrain::new();
        assert_eq!(
            words(terrain.add_archive(b"PK\x03\x04 a zip file".to_vec())),
            "This is not an archive of elevation tiles: it does not begin as a PMTiles archive does."
        );
        let mut archive = bundled();
        archive.truncate(archive.len() / 2);
        assert_eq!(
            words(terrain.add_archive(archive)),
            "This is not an archive of elevation tiles: it is cut short."
        );
        let mut archive = bundled();
        archive[7] = 2;
        assert!(words(terrain.add_archive(archive)).contains("version 3"));
        // The base map's archive holds vector tiles, not elevation.
        let basemap = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/MapDemo/basemap.pmtiles"
        ))
        .unwrap();
        assert!(words(terrain.add_archive(basemap)).contains("not plain WebP images"));
        assert_eq!(terrain.tile_count(), 0);
    }

    #[test]
    fn tile_numbers_follow_the_curve() {
        let zxy = |number| {
            let address = address_of(number).unwrap();
            (address.z, address.x, address.y)
        };
        assert_eq!(zxy(0), (0, 0, 0));
        assert_eq!(zxy(1), (1, 0, 0));
        assert_eq!(zxy(2), (1, 0, 1));
        assert_eq!(zxy(3), (1, 1, 1));
        assert_eq!(zxy(4), (1, 1, 0));
        assert_eq!(zxy(5), (2, 0, 0));
        // The last tile of zoom 12 and the first of zoom 13.
        let through_12: u64 = (0..=12).map(|z| 1_u64 << (2 * z)).sum();
        assert_eq!(zxy(through_12 - 1), (12, 4095, 0));
        assert_eq!(zxy(through_12), (13, 0, 0));
    }
}
