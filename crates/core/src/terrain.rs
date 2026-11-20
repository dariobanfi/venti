//! The ground under the glider, from the map's own elevation tiles.
//!
//! The shell fetches tiles, or has them in an archive that came with the
//! app or was downloaded for flying without a network, and hands them
//! over as they are. The core reads the pixels and answers how high the
//! ground is. See [`venti_flight::terrain`] for how a height is read from
//! a tile.

use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, MutexGuard};

use venti_domain::GeoPoint;
use venti_flight::terrain::{self as model, TileAddress};

use crate::{Bounds, CoreError, TrackPoint};

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

/// For the ground under an area, such as a whole flight, no more tiles
/// than this are asked for: the zoom level is made coarser until they are
/// enough. Thirty-six tiles of zoom 12 cover some 40 km by 40 km, and are
/// about 5 MB to fetch.
const AREA_TILES_AT_MOST: usize = 36;

/// And no coarser than this: at zoom 8 a pixel is 200 m wide, which still
/// shows the valleys and ridges of a flight of several hundred
/// kilometres, and nothing finer.
const AREA_COARSEST_ZOOM: u8 = 8;

/// An image larger than this is no elevation tile.
const IMAGE_AT_MOST_BYTES: usize = 4 * 1024 * 1024;

/// An archive handed over as bytes is read whole from memory, so only a
/// small one is taken that way: the region that comes with the app. A
/// downloaded region's archive is opened from its file and read piece by
/// piece, see [`Terrain::open_archive`].
const ARCHIVE_AT_MOST_BYTES: usize = 64 * 1024 * 1024;

/// A directory of an archive larger than this is none this app reads. The
/// format keeps directories to a few kilobytes each.
const DIRECTORY_AT_MOST_BYTES: u64 = 16 * 1024 * 1024;

/// How many of an opened archive's further directories are kept in
/// memory. Each lists the tiles of a patch of the map, so a flight needs
/// one or two at a time.
const LEAVES_KEPT: usize = 16;

/// How many directories lead to a tile at the most. The format allows a
/// root and three levels below it.
const DIRECTORY_LEVELS: usize = 4;

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

/// The ground under one moment of a flight.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct GroundUnderFlight {
    /// Seconds since the first point of the track.
    pub seconds: u32,
    /// The height of the ground under the glider then, metres above mean
    /// sea level.
    pub elevation_m: f64,
    /// How high over it the glider was, metres.
    pub height_above_m: f64,
}

/// The height of the ground, as far as tiles for it were handed over.
#[derive(Debug, Default, uniffi::Object)]
pub struct Terrain {
    model: Mutex<model::Terrain>,
    /// The archives opened from their files: downloaded regions.
    archives: Mutex<Vec<Arc<ArchiveFile>>>,
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

    /// The tiles to have for the ground under an area, such as the
    /// bounds of a flight, without those that are held already: at the
    /// finest zoom level at which the area needs no more than a few dozen.
    /// Empty when all are there, and for an area that is none.
    pub fn tiles_covering(&self, area: Bounds) -> Vec<TerrainTile> {
        let model = self.lock();
        area_tiles(area)
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

    /// Opens an archive of elevation tiles in the PMTiles format, version
    /// 3, from its file: a region downloaded for flying without a network.
    /// Nothing but its directory is read now; [`Terrain::read_around`] and
    /// [`Terrain::read_covering`] take tiles from it as they are needed.
    /// Opening a file that is open already opens it afresh.
    ///
    /// # Errors
    ///
    /// When the file cannot be read, is no such archive, or its tiles are
    /// not WebP images.
    pub fn open_archive(&self, path: String) -> Result<(), CoreError> {
        let archive = Arc::new(ArchiveFile::open(&path)?);
        let mut archives = self.archives();
        archives.retain(|open| open.path != path);
        archives.push(archive);
        Ok(())
    }

    /// Lets every opened archive go, before its file is deleted or
    /// replaced. The tiles already taken from it stay.
    pub fn close_archives(&self) {
        self.archives().clear();
    }

    /// Takes from the opened archives the tiles for flying around a place
    /// that are not held yet, and returns how many it took. What they do
    /// not have is still in [`Terrain::tiles_wanted`] afterwards.
    ///
    /// This reads files and decodes images: a few hundredths of a second
    /// for each tile. Not for the thread that draws.
    pub fn read_around(&self, lat: f64, lon: f64) -> u32 {
        let Some(here) = GeoPoint::new(lat, lon)
            .ok()
            .and_then(|position| TileAddress::holding(position, ZOOM))
        else {
            return 0;
        };
        self.read(here.with_neighbours(RINGS_WANTED))
    }

    /// Takes from the opened archives the tiles for the ground under an
    /// area that are not held yet, and returns how many it took: the
    /// tiles [`Terrain::tiles_covering`] names. Not for the thread that
    /// draws.
    pub fn read_covering(&self, area: Bounds) -> u32 {
        self.read(area_tiles(area))
    }

    /// The ground under a flight's track, at up to `places` moments
    /// spread evenly over it, in order. A moment whose place no tile
    /// covers is left out.
    pub fn ground_under(&self, track: Vec<TrackPoint>, places: u32) -> Vec<GroundUnderFlight> {
        let model = self.lock();
        let wanted = (places as usize).clamp(2, track.len().max(2));
        let last = track.len().saturating_sub(1);
        let mut ground = Vec::with_capacity(wanted);
        let mut before = None;
        for place in 0..wanted {
            let index = place * last / (wanted - 1);
            if before == Some(index) {
                continue;
            }
            before = Some(index);
            let Some(point) = track.get(index) else {
                break;
            };
            let elevation = GeoPoint::new(point.lat, point.lon)
                .ok()
                .and_then(|position| model.elevation_m(position));
            if let Some(elevation_m) = elevation {
                ground.push(GroundUnderFlight {
                    seconds: point.seconds,
                    elevation_m,
                    height_above_m: point.altitude_m - elevation_m,
                });
            }
        }
        ground
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

    /// Lets the flight engine work on the heights.
    pub(crate) fn with<R>(&self, work: impl FnOnce(&model::Terrain) -> R) -> R {
        work(&self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, model::Terrain> {
        self.model
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn archives(&self) -> MutexGuard<'_, Vec<Arc<ArchiveFile>>> {
        self.archives
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes the tiles named from the opened archives, where one has
    /// them, and says how many. A tile that an archive cannot give, or
    /// that is no image, is left: the shell may still fetch it.
    fn read(&self, wanted: Vec<TileAddress>) -> u32 {
        let archives = self.archives().clone();
        if archives.is_empty() {
            return 0;
        }
        let mut taken = 0;
        for address in wanted {
            if self.lock().has(address) {
                continue;
            }
            for archive in &archives {
                let Ok(Some(image)) = archive.tile(address) else {
                    continue;
                };
                let Ok((side, rgba)) = pixels(&image) else {
                    continue;
                };
                if self.lock().add_pixels(address, side, &rgba).is_ok() {
                    taken += 1;
                    break;
                }
            }
        }
        taken
    }
}

/// The tiles for the ground under an area: at the finest zoom level at
/// which the area needs no more than a few dozen.
fn area_tiles(area: Bounds) -> Vec<TileAddress> {
    for z in (AREA_COARSEST_ZOOM..=ZOOM).rev() {
        let tiles = TileAddress::covering(area.south, area.west, area.north, area.east, z);
        if tiles.len() > AREA_TILES_AT_MOST && z > AREA_COARSEST_ZOOM {
            continue;
        }
        // An area larger than a country gets its middle and no more;
        // nobody flies it in a day.
        return tiles.into_iter().take(4 * AREA_TILES_AT_MOST).collect();
    }
    Vec::new()
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
    let header = header(archive)?;
    let size = |number: u64| usize::try_from(number).unwrap_or(usize::MAX);
    let part = |at: u64, length: u64| {
        size(at)
            .checked_add(size(length))
            .and_then(|end| archive.get(size(at)..end))
            .ok_or_else(|| not_an_archive("it is cut short"))
    };

    let mut tiles = Vec::new();
    // The root directory, and the directories it points to. A directory
    // that points back would go round for ever: no more are followed than
    // an archive of this size can hold.
    let mut waiting = vec![part(header.root_at, header.root_length)?];
    let mut followed = 0;
    while let Some(packed) = waiting.pop() {
        followed += 1;
        if followed > 4096 {
            return Err(not_an_archive("its directories do not end"));
        }
        for entry in directory(packed, header.is_gzip)? {
            if entry.run == 0 {
                let leaves = part(header.leaves_at, header.leaves_length)?;
                waiting.push(within(leaves, &entry)?);
                continue;
            }
            let data = part(header.tiles_at, header.tiles_length)?;
            let image = within(data, &entry)?;
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

/// Where the parts of a PMTiles archive are: what its first bytes say.
#[derive(Debug, Clone, Copy)]
struct Header {
    root_at: u64,
    root_length: u64,
    leaves_at: u64,
    leaves_length: u64,
    tiles_at: u64,
    tiles_length: u64,
    /// Whether the directories are packed with gzip.
    is_gzip: bool,
}

/// The length of an archive's header.
const HEADER_BYTES: usize = 127;

/// Reads the header from the first bytes of an archive of elevation
/// tiles, and refuses what this app does not read.
fn header(start: &[u8]) -> Result<Header, CoreError> {
    if start.len() < HEADER_BYTES || &start[..7] != b"PMTiles" {
        return Err(not_an_archive(
            "it does not begin as a PMTiles archive does",
        ));
    }
    if start[7] != 3 {
        return Err(not_an_archive("it is not of version 3 of the format"));
    }
    let number = |at: usize| {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&start[at..at + 8]);
        u64::from_le_bytes(bytes)
    };
    let (directories_packed, tiles_packed, tile_kind) = (start[97], start[98], start[99]);
    // 1 is "not compressed", 2 is gzip; a tile kind of 4 is WebP.
    if !(1..=2).contains(&directories_packed) {
        return Err(not_an_archive(
            "its directory is packed in a way this app does not read",
        ));
    }
    if tiles_packed != 1 || tile_kind != 4 {
        return Err(not_an_archive("its tiles are not plain WebP images"));
    }
    Ok(Header {
        root_at: number(8),
        root_length: number(16),
        leaves_at: number(40),
        leaves_length: number(48),
        tiles_at: number(56),
        tiles_length: number(64),
        is_gzip: directories_packed == 2,
    })
}

/// An archive opened from its file. Its first directory is held; further
/// directories and tiles are read from the file when a tile is asked for.
#[derive(Debug)]
struct ArchiveFile {
    path: String,
    file: Mutex<File>,
    header: Header,
    root: Arc<Vec<Entry>>,
    /// The further directories read last, by where they are in the file,
    /// the latest at the end.
    leaves: Mutex<Vec<(u64, Arc<Vec<Entry>>)>>,
}

impl ArchiveFile {
    fn open(path: &str) -> Result<Self, CoreError> {
        let mut file = File::open(path).map_err(|_| not_an_archive("its file cannot be opened"))?;
        let mut start = [0; HEADER_BYTES];
        file.read_exact(&mut start)
            .map_err(|_| not_an_archive("it is cut short"))?;
        let header = header(&start)?;
        let packed = read_part(
            &mut file,
            header.root_at,
            header.root_length,
            DIRECTORY_AT_MOST_BYTES,
        )?;
        let root = Arc::new(directory(&packed, header.is_gzip)?);
        Ok(Self {
            path: path.to_owned(),
            file: Mutex::new(file),
            header,
            root,
            leaves: Mutex::new(Vec::new()),
        })
    }

    /// The image of one tile, as the archive holds it. `None` for a tile
    /// the archive does not have.
    fn tile(&self, address: TileAddress) -> Result<Option<Vec<u8>>, CoreError> {
        let Some(number) = number_of(address) else {
            return Ok(None);
        };
        let mut entries = Arc::clone(&self.root);
        for _ in 0..DIRECTORY_LEVELS {
            // The entry that begins at or before the tile: its run holds
            // the tile, or it points to the directory that knows.
            let before = entries.partition_point(|entry| entry.first <= number);
            let Some(entry) = before
                .checked_sub(1)
                .and_then(|index| entries.get(index))
                .copied()
            else {
                return Ok(None);
            };
            if entry.run == 0 {
                entries = self.leaf(entry)?;
                continue;
            }
            if number - entry.first >= entry.run {
                return Ok(None);
            }
            let at = self
                .header
                .tiles_at
                .checked_add(entry.offset)
                .filter(|_| {
                    entry
                        .offset
                        .checked_add(entry.length)
                        .is_some_and(|end| end <= self.header.tiles_length)
                })
                .ok_or_else(|| not_an_archive("it is cut short"))?;
            return self
                .read(at, entry.length, IMAGE_AT_MOST_BYTES as u64)
                .map(Some);
        }
        Err(not_an_archive("its directories do not end"))
    }

    /// A further directory, from memory if it was read lately.
    fn leaf(&self, entry: Entry) -> Result<Arc<Vec<Entry>>, CoreError> {
        let lock = || {
            self.leaves
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        };
        {
            let mut leaves = lock();
            if let Some(index) = leaves.iter().position(|(at, _)| *at == entry.offset) {
                let found = leaves.remove(index);
                let entries = Arc::clone(&found.1);
                leaves.push(found);
                return Ok(entries);
            }
        }
        let at = self
            .header
            .leaves_at
            .checked_add(entry.offset)
            .filter(|_| {
                entry
                    .offset
                    .checked_add(entry.length)
                    .is_some_and(|end| end <= self.header.leaves_length)
            })
            .ok_or_else(|| not_an_archive("it is cut short"))?;
        let packed = self.read(at, entry.length, DIRECTORY_AT_MOST_BYTES)?;
        let entries = Arc::new(directory(&packed, self.header.is_gzip)?);
        let mut leaves = lock();
        if leaves.len() >= LEAVES_KEPT {
            leaves.remove(0);
        }
        leaves.push((entry.offset, Arc::clone(&entries)));
        Ok(entries)
    }

    fn read(&self, at: u64, length: u64, at_most: u64) -> Result<Vec<u8>, CoreError> {
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        read_part(&mut file, at, length, at_most)
    }
}

/// Reads a part of a file: `length` bytes from `at` on.
fn read_part(file: &mut File, at: u64, length: u64, at_most: u64) -> Result<Vec<u8>, CoreError> {
    if length > at_most {
        return Err(not_an_archive(
            "a part of it is larger than any this app reads",
        ));
    }
    let cut_short = || not_an_archive("it is cut short");
    let mut bytes = vec![0; usize::try_from(length).map_err(|_| cut_short())?];
    file.seek(SeekFrom::Start(at)).map_err(|_| cut_short())?;
    file.read_exact(&mut bytes).map_err(|_| cut_short())?;
    Ok(bytes)
}

/// The part of `whole` that an entry names.
fn within<'a>(whole: &'a [u8], entry: &Entry) -> Result<&'a [u8], CoreError> {
    let size = |number: u64| usize::try_from(number).unwrap_or(usize::MAX);
    size(entry.offset)
        .checked_add(size(entry.length))
        .and_then(|end| whole.get(size(entry.offset)..end))
        .ok_or_else(|| not_an_archive("it is cut short"))
}

/// One line of a directory: a run of tiles that share one image, or with
/// a run of zero a further directory.
#[derive(Debug, Clone, Copy)]
struct Entry {
    first: u64,
    run: u64,
    length: u64,
    offset: u64,
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
        entry.length = next()?;
    }
    let mut before: Option<(u64, u64)> = None;
    for entry in &mut entries {
        let written = next()?;
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

/// The number a tile has in an archive: the reverse of [`address_of`].
/// `None` for an address the map does not have.
fn number_of(address: TileAddress) -> Option<u64> {
    if address.z > 22 {
        return None;
    }
    let side = 1_u64 << address.z;
    let (mut x, mut y) = (u64::from(address.x), u64::from(address.y));
    if x >= side || y >= side {
        return None;
    }
    // Every coarser level comes first.
    let first: u64 = (0..u32::from(address.z)).map(|z| 1_u64 << (2 * z)).sum();
    let mut along = 0_u64;
    let mut half = side / 2;
    while half > 0 {
        let right = u64::from(x & half > 0);
        let down = u64::from(y & half > 0);
        along += half * half * ((3 * right) ^ down);
        // What is left to place lies within this quarter.
        x &= half - 1;
        y &= half - 1;
        if down == 0 {
            if right == 1 {
                x = half - 1 - x;
                y = half - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        half /= 2;
    }
    Some(first + along)
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
    fn the_ground_under_a_flight_is_read_along_its_track() {
        let flight = crate::flight_from_file(venti_flight::igc::write(
            &venti_flight::testing::synthetic_flight(),
        ))
        .unwrap();
        let terrain = Terrain::new();

        // The tiles under the flight: a handful at zoom 12, the flight
        // being some six kilometres across. A country's worth of area is
        // asked for at a coarser zoom, and never by the hundred.
        let wanted = terrain.tiles_covering(flight.bounds);
        assert!((2..=12).contains(&wanted.len()), "{}", wanted.len());
        assert!(wanted.iter().all(|tile| tile.z == 12));
        let alps = Bounds {
            south: 45.5,
            west: 6.0,
            north: 48.0,
            east: 14.0,
        };
        let wanted = terrain.tiles_covering(alps);
        assert!(wanted.iter().all(|tile| tile.z == 8));
        assert!(!wanted.is_empty() && wanted.len() <= 36, "{}", wanted.len());
        assert!(terrain.ground_under(flight.track.clone(), 240).is_empty());

        // With the region that comes with the app, all of them are there.
        terrain.add_archive(bundled()).unwrap();
        assert!(terrain.tiles_covering(flight.bounds).is_empty());

        // The flight takes off from the Wallberg at 1,620 m and ends over
        // the valley floor: the ground under it falls from the one to the
        // other.
        let ground = terrain.ground_under(flight.track.clone(), 240);
        assert_eq!(ground.len(), 240);
        assert_eq!(ground[0].seconds, 0);
        assert_eq!(ground[239].seconds, flight.track.last().unwrap().seconds);
        assert!(
            (ground[0].elevation_m - 1620.0).abs() < 10.0,
            "{:?}",
            ground[0]
        );
        assert!(ground[0].height_above_m.abs() < 10.0);
        let last = ground[239];
        assert!((700.0..800.0).contains(&last.elevation_m), "{last:?}");
        // The sample flight is made up, and in its first half minute it
        // is drawn through the mountain's summit, north of its take-off:
        // there it is under the ground. Nowhere else.
        assert!(
            ground
                .iter()
                .all(|moment| moment.height_above_m > -10.0 || moment.seconds < 60)
        );
        let highest_over = ground
            .iter()
            .map(|moment| moment.height_above_m)
            .fold(0.0, f64::max);
        assert!((1000.0..1500.0).contains(&highest_over), "{highest_over}");

        // A track of three points asked for at ten places gives three.
        let short: Vec<TrackPoint> = flight.track.iter().take(3).copied().collect();
        assert_eq!(terrain.ground_under(short, 10).len(), 3);
        assert!(terrain.ground_under(Vec::new(), 10).is_empty());
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

    /// A file for one test, removed when the test is over.
    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn holding(name: &str, bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "venti-terrain-{}-{name}.pmtiles",
                std::process::id()
            ));
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }

        fn path(&self) -> String {
            self.0.to_str().unwrap().to_owned()
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// Packs tiles into an archive as a large one is laid out: its first
    /// directory lists no tiles but further directories, of `per_leaf`
    /// tiles each.
    fn packed_with_leaves(tiles: &[(TileAddress, &[u8])], per_leaf: usize) -> Vec<u8> {
        fn varint(out: &mut Vec<u8>, mut value: u64) {
            loop {
                let low = u8::try_from(value & 0x7F).unwrap();
                value >>= 7;
                if value == 0 {
                    out.push(low);
                    return;
                }
                out.push(low | 0x80);
            }
        }
        // Entries as (first tile, run, length, offset).
        fn directory_of(entries: &[(u64, u64, u64, u64)]) -> Vec<u8> {
            let mut out = Vec::new();
            varint(&mut out, entries.len() as u64);
            let mut before = 0;
            for entry in entries {
                varint(&mut out, entry.0 - before);
                before = entry.0;
            }
            for entry in entries {
                varint(&mut out, entry.1);
            }
            for entry in entries {
                varint(&mut out, entry.2);
            }
            for entry in entries {
                varint(&mut out, entry.3 + 1);
            }
            out
        }

        let mut numbered: Vec<(u64, &[u8])> = tiles
            .iter()
            .map(|(address, image)| (number_of(*address).unwrap(), *image))
            .collect();
        numbered.sort_by_key(|(number, _)| *number);
        numbered.dedup_by_key(|(number, _)| *number);

        let (mut data, mut leaves, mut root) = (Vec::new(), Vec::new(), Vec::new());
        for chunk in numbered.chunks(per_leaf) {
            let mut entries = Vec::new();
            for (number, image) in chunk {
                entries.push((*number, 1, image.len() as u64, data.len() as u64));
                data.extend_from_slice(image);
            }
            let leaf = directory_of(&entries);
            root.push((chunk[0].0, 0, leaf.len() as u64, leaves.len() as u64));
            leaves.extend(leaf);
        }
        let root = directory_of(&root);

        let mut header = vec![0_u8; HEADER_BYTES];
        header[..7].copy_from_slice(b"PMTiles");
        header[7] = 3;
        let mut put = |at: usize, value: usize| {
            header[at..at + 8].copy_from_slice(&(value as u64).to_le_bytes());
        };
        let leaves_at = HEADER_BYTES + root.len();
        put(8, HEADER_BYTES);
        put(16, root.len());
        put(24, leaves_at);
        put(40, leaves_at);
        put(48, leaves.len());
        put(56, leaves_at + leaves.len());
        put(64, data.len());
        // Tiles in order, directories and tiles not packed, WebP.
        header[96..100].copy_from_slice(&[1, 1, 1, 4]);
        [header, root, leaves, data].concat()
    }

    #[test]
    fn a_downloaded_region_is_read_from_its_file_as_tiles_are_needed() {
        let whole = Terrain::new();
        whole.add_archive(bundled()).unwrap();

        // The region as it comes with the app, opened from its file.
        let file = TempFile::holding("as-it-is", &bundled());
        let terrain = Terrain::new();
        terrain.open_archive(file.path()).unwrap();
        // Opening reads the directory and no tile.
        assert_eq!(terrain.tile_count(), 0);
        assert_eq!(terrain.elevation(47.6662, 11.7982), None);
        assert_eq!(terrain.tiles_wanted(47.6662, 11.7982).len(), 25);

        // Around the Wallberg the region has twelve of the twenty-five
        // tiles of zoom 12; the others are still wanted, from a server.
        assert_eq!(terrain.read_around(47.6662, 11.7982), 12);
        assert_eq!(terrain.tile_count(), 12);
        assert_eq!(terrain.tiles_wanted(47.6662, 11.7982).len(), 13);
        // Nothing is read twice.
        assert_eq!(terrain.read_around(47.6662, 11.7982), 0);
        for (lat, lon) in [(47.6662, 11.7982), (47.7130, 11.7400), (47.6642, 11.7967)] {
            assert_eq!(terrain.elevation(lat, lon), whole.elevation(lat, lon));
            assert!(terrain.elevation(lat, lon).is_some());
        }

        // Opened again, the file is open once, and closing lets it go:
        // what was taken stays, and nothing more is read.
        terrain.open_archive(file.path()).unwrap();
        assert_eq!(terrain.archives().len(), 1);
        terrain.close_archives();
        assert_eq!(terrain.tile_count(), 12);
        let fresh = Terrain::new();
        assert_eq!(fresh.read_around(47.6662, 11.7982), 0);
    }

    #[test]
    fn a_large_archive_with_directories_of_directories_is_read_the_same() {
        let whole = Terrain::new();
        whole.add_archive(bundled()).unwrap();
        let bundled = bundled();
        let tiles = archive_tiles(&bundled).unwrap();

        // One tile to a directory makes thirty directories, more than are
        // kept in memory; four to a directory makes eight.
        for per_leaf in [1, 4] {
            let packed = packed_with_leaves(&tiles, per_leaf);
            // The reader of whole archives follows the directories too.
            assert_eq!(archive_tiles(&packed).unwrap().len(), 30);

            let file = TempFile::holding(&format!("leaves-{per_leaf}"), &packed);
            let terrain = Terrain::new();
            terrain.open_archive(file.path()).unwrap();
            assert_eq!(terrain.read_around(47.6662, 11.7982), 12);
            for (lat, lon) in [(47.6662, 11.7982), (47.7130, 11.7400), (47.6642, 11.7967)] {
                assert_eq!(terrain.elevation(lat, lon), whole.elevation(lat, lon));
            }

            // The ground under a flight is read at the zoom level its
            // area asks for: here the coarser tiles the archive has.
            let under = Terrain::new();
            under.open_archive(file.path()).unwrap();
            let area = Bounds {
                south: 47.60,
                west: 11.50,
                north: 47.90,
                east: 12.10,
            };
            let wanted = under.tiles_covering(area);
            assert!(wanted.iter().all(|tile| tile.z < ZOOM), "{wanted:?}");
            let taken = under.read_covering(area);
            assert!(taken > 0, "{taken}");
            assert_eq!(
                under.tiles_covering(area).len(),
                wanted.len() - taken as usize
            );
            let lake = under.elevation(47.7130, 11.7400).unwrap();
            assert!((lake - 725.0).abs() < 3.0, "{lake}");
        }
    }

    #[test]
    fn a_file_that_is_no_archive_of_elevation_is_not_opened() {
        let terrain = Terrain::new();
        let nowhere = std::env::temp_dir().join("venti-terrain-no-such-file.pmtiles");
        assert!(
            words(terrain.open_archive(nowhere.to_str().unwrap().to_owned()))
                .contains("cannot be opened")
        );
        let short = TempFile::holding("short", &bundled()[..100]);
        assert!(words(terrain.open_archive(short.path())).contains("cut short"));
        let basemap = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/ios/Venti/Resources/MapDemo/basemap.pmtiles"
        ))
        .unwrap();
        let other = TempFile::holding("basemap", &basemap);
        assert!(words(terrain.open_archive(other.path())).contains("not plain WebP images"));
        assert!(terrain.archives().is_empty());

        // A file whose end is missing opens, since its directory is
        // there; the tiles that are missing are left, and the others read.
        let bundled = bundled();
        let cut = TempFile::holding("cut", &bundled[..bundled.len() / 2]);
        terrain.open_archive(cut.path()).unwrap();
        assert!(terrain.read_around(47.6662, 11.7982) < 12);
    }

    #[test]
    fn a_tile_has_the_number_its_address_is_read_from() {
        for z in 0..=6_u8 {
            let side = 1_u32 << z;
            for x in 0..side {
                for y in 0..side {
                    let address = TileAddress { z, x, y };
                    let number = number_of(address).unwrap();
                    assert_eq!(address_of(number), Some(address), "{address:?}");
                }
            }
        }
        for address in [
            TileAddress {
                z: 12,
                x: 2181,
                y: 1429,
            },
            TileAddress {
                z: 12,
                x: 4095,
                y: 0,
            },
            TileAddress {
                z: 14,
                x: 8725,
                y: 5717,
            },
            TileAddress {
                z: 22,
                x: 4_194_303,
                y: 4_194_303,
            },
        ] {
            assert_eq!(address_of(number_of(address).unwrap()), Some(address));
        }
        assert_eq!(number_of(TileAddress { z: 1, x: 1, y: 0 }), Some(4));
        // Beyond the edge of a level, and beyond the levels there are.
        assert_eq!(number_of(TileAddress { z: 1, x: 2, y: 0 }), None);
        assert_eq!(number_of(TileAddress { z: 23, x: 0, y: 0 }), None);
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
