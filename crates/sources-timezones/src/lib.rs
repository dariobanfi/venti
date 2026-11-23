//! The time zone of a place on land, for a flight's times by the clock of
//! where it was flown, offline in every app.
//!
//! Nothing is fetched: the zones are the crate's asset, `assets/zones.bin`,
//! made by `tools/build_zones.py` from timezone-boundary-builder's
//! outlines "same since 1970" (ODbL, from OpenStreetMap), with the
//! manifest's `[[asset]]` saying which release. A zone is named by its IANA
//! identifier, which the platforms' own tables turn into a clock: two
//! places whose clocks have agreed since 1970 have one name, that of the
//! larger city (Ljubljana is `Europe/Belgrade`). The tool's first comment
//! describes the file.
//!
//! The world is cut into cells of half a degree. A cell of one zone, or of
//! one zone and the sea, is that zone; only a cell that zones share keeps
//! their outlines, simplified by 0.001° (about 110 m). The sea is the
//! coast's zone up to a cell from land, and has none beyond.

use std::sync::LazyLock;

/// The asset.
const ASSET: &[u8] = include_bytes!("../assets/zones.bin");

/// What the asset's first bytes say.
const MAGIC: &[u8] = b"VTZ1";

/// Coordinates in the asset are in these parts of a degree.
const QUANTUM: f64 = 10_000.0;

/// The IANA time zone of a place, or `None` at sea away from land, in
/// Antarctica's few places without one, and for a position that is no
/// number.
pub fn zone_at(lon: f64, lat: f64) -> Option<&'static str> {
    INDEX.zone_at(lon, lat)
}

/// Every zone of the asset, by its IANA identifier, in alphabetical order.
pub fn zones() -> &'static [&'static str] {
    &INDEX.names
}

/// What the asset holds, in numbers: for a test to hold it to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The size of a cell, in ten-thousandths of a degree.
    pub cell_units: u32,
    /// The zones.
    pub zones: usize,
    /// Cells that are in no zone: the sea.
    pub cells_without_zone: usize,
    /// Cells that are in one zone whole.
    pub cells_of_one_zone: usize,
    /// Cells that zones share, which keep outlines.
    pub cells_shared: usize,
    /// Points of the outlines in the shared cells.
    pub points: usize,
    /// The asset's size in bytes.
    pub bytes: usize,
}

/// What the asset holds.
///
/// # Panics
///
/// Panics if a shared cell of the asset compiled into the crate cannot be
/// read, which the crate's tests rule out.
pub fn summary() -> Summary {
    let index = &*INDEX;
    let mut summary = Summary {
        cell_units: index.cell_units,
        zones: index.names.len(),
        cells_without_zone: 0,
        cells_of_one_zone: 0,
        cells_shared: index.shared.len(),
        points: 0,
        bytes: ASSET.len(),
    };
    for row in 0..index.rows {
        let mut start = 0;
        for &(end, code) in index.row(row) {
            let cells = (end - start) as usize;
            start = end;
            match index.cell(code) {
                Cell::None => summary.cells_without_zone += cells,
                Cell::Zone(_) => summary.cells_of_one_zone += cells,
                Cell::Shared(_) => {}
            }
        }
    }
    for &offset in &index.shared {
        summary.points += index
            .shared_points(offset)
            .expect("the asset's shared cells can be read");
    }
    summary
}

static INDEX: LazyLock<Index> =
    LazyLock::new(|| Index::read(ASSET).expect("the asset compiled into the crate can be read"));

/// The asset, read for looking up: the zones' names, each row's runs of
/// cells, and where each shared cell's outlines begin. The outlines stay
/// in the asset and are read when a place in their cell is asked.
struct Index {
    cell_units: u32,
    names: Vec<&'static str>,
    cols: u32,
    rows: u32,
    /// Each run of a row: the column after it, and its code.
    runs: Vec<(u32, u32)>,
    /// Where each row's runs begin in `runs`, and one more for the end.
    row_starts: Vec<usize>,
    /// Where each shared cell begins in the asset, in their order.
    shared: Vec<usize>,
}

/// What a cell's code means.
enum Cell {
    None,
    Zone(usize),
    Shared(usize),
}

impl Index {
    fn read(data: &'static [u8]) -> Option<Self> {
        let mut bytes = Reader { data, at: 0 };
        if bytes.take(MAGIC.len())? != MAGIC {
            return None;
        }
        let cell_units = bytes.varint()?;
        if cell_units == 0 || !(360 * QUANTUM as u32).is_multiple_of(cell_units) {
            return None;
        }
        let cols = 360 * QUANTUM as u32 / cell_units;
        let rows = 180 * QUANTUM as u32 / cell_units;
        let mut names = Vec::new();
        for _ in 0..bytes.varint()? {
            let length = bytes.varint()? as usize;
            names.push(std::str::from_utf8(bytes.take(length)?).ok()?);
        }
        let mixed = names.len() as u32 + 1;
        let mut runs = Vec::new();
        let mut row_starts = vec![0];
        let mut shared_count = 0usize;
        for _ in 0..rows {
            let mut end = 0;
            for _ in 0..bytes.varint()? {
                let length = bytes.varint()?;
                let code = bytes.varint()?;
                if code > mixed || length == 0 {
                    return None;
                }
                // A shared cell is a run of one; its code becomes its
                // number among the shared cells, after the zones.
                let code = if code == mixed {
                    shared_count += 1;
                    mixed + shared_count as u32 - 1
                } else {
                    code
                };
                end += length;
                runs.push((end, code));
            }
            if end != cols {
                return None;
            }
            row_starts.push(runs.len());
        }
        let mut shared = Vec::with_capacity(shared_count);
        for _ in 0..shared_count {
            let length = bytes.varint()? as usize;
            shared.push(bytes.at);
            bytes.take(length)?;
        }
        (bytes.at == data.len()).then_some(Self {
            cell_units,
            names,
            cols,
            rows,
            runs,
            row_starts,
            shared,
        })
    }

    fn row(&self, row: u32) -> &[(u32, u32)] {
        &self.runs[self.row_starts[row as usize]..self.row_starts[row as usize + 1]]
    }

    fn cell(&self, code: u32) -> Cell {
        let zones = self.names.len() as u32;
        match code {
            0 => Cell::None,
            code if code <= zones => Cell::Zone(code as usize - 1),
            code => Cell::Shared((code - zones - 1) as usize),
        }
    }

    fn zone_at(&self, lon: f64, lat: f64) -> Option<&'static str> {
        if !lon.is_finite() || !lat.is_finite() {
            return None;
        }
        let cell = f64::from(self.cell_units) / QUANTUM;
        let lon = (lon + 180.0).rem_euclid(360.0) - 180.0;
        let lat = lat.clamp(-90.0, 90.0);
        let col = (((lon + 180.0) / cell).floor() as u32).min(self.cols - 1);
        let row = (((lat + 90.0) / cell).floor() as u32).min(self.rows - 1);
        let runs = self.row(row);
        let (_, code) = runs[runs.partition_point(|&(end, _)| end <= col)];
        match self.cell(code) {
            Cell::None => None,
            Cell::Zone(zone) => Some(self.names[zone]),
            Cell::Shared(number) => {
                // The place in the cell's units, from its south-west corner.
                let x = (lon - (-180.0 + f64::from(col) * cell)) * QUANTUM;
                let y = (lat - (-90.0 + f64::from(row) * cell)) * QUANTUM;
                self.shared_zone(self.shared[number], x, y)
            }
        }
    }

    /// The zone of a place in a shared cell: the first whose rings a line
    /// from the place crosses an odd number of times, else the cell's own.
    fn shared_zone(&self, offset: usize, x: f64, y: f64) -> Option<&'static str> {
        let mut bytes = Reader {
            data: ASSET,
            at: offset,
        };
        let zone_of = |code: u32| match self.cell(code) {
            Cell::Zone(zone) => Some(self.names[zone]),
            _ => None,
        };
        let left = bytes.varint()?;
        for _ in 0..bytes.varint()? {
            let zone = bytes.varint()? as usize;
            let mut inside = false;
            for _ in 0..bytes.varint()? {
                let count = bytes.varint()?;
                let (mut px, mut py) = (0i64, 0i64);
                let mut first = None;
                let mut previous: Option<(f64, f64)> = None;
                for _ in 0..count {
                    px += bytes.signed()?;
                    py += bytes.signed()?;
                    let point = (px as f64, py as f64);
                    if let Some(from) = previous {
                        inside ^= crosses(from, point, x, y);
                    } else {
                        first = Some(point);
                    }
                    previous = Some(point);
                }
                if let (Some(last), Some(first)) = (previous, first) {
                    inside ^= crosses(last, first, x, y);
                }
            }
            if inside {
                return self.names.get(zone).copied();
            }
        }
        zone_of(left)
    }

    /// How many points a shared cell keeps.
    fn shared_points(&self, offset: usize) -> Option<usize> {
        let mut bytes = Reader {
            data: ASSET,
            at: offset,
        };
        let mut points = 0;
        bytes.varint()?;
        for _ in 0..bytes.varint()? {
            bytes.varint()?;
            for _ in 0..bytes.varint()? {
                let count = bytes.varint()?;
                for _ in 0..count {
                    bytes.signed()?;
                    bytes.signed()?;
                }
                points += count as usize;
            }
        }
        Some(points)
    }
}

/// Whether the edge from `a` to `b` crosses the line from the place
/// eastwards.
fn crosses(a: (f64, f64), b: (f64, f64), x: f64, y: f64) -> bool {
    (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1)
}

/// The asset's bytes, read from the front.
struct Reader {
    data: &'static [u8],
    at: usize,
}

impl Reader {
    fn take(&mut self, length: usize) -> Option<&'static [u8]> {
        let data: &'static [u8] = self.data;
        let taken = data.get(self.at..self.at.checked_add(length)?)?;
        self.at += length;
        Some(taken)
    }

    /// A LEB128 number.
    fn varint(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = *self.data.get(self.at)?;
            self.at += 1;
            value |= u32::from(byte & 0x7F).checked_shl(shift)?;
            if byte < 0x80 {
                return Some(value);
            }
        }
        None
    }

    /// A zigzagged LEB128 number.
    fn signed(&mut self) -> Option<i64> {
        let n = i64::from(self.varint()?);
        Some((n >> 1) ^ -(n & 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_and_the_asset_are_read() {
        let manifest = venti_sources::Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest is one");
        assert_eq!(manifest.id.as_str(), "timezone-boundary-builder");
        assert_eq!(manifest.assets.len(), 1);
        assert_eq!(manifest.assets[0].file, "assets/zones.bin");
        let zones = zones();
        assert!(zones.len() > 250, "{}", zones.len());
        assert!(zones.windows(2).all(|pair| pair[0] < pair[1]));
        for zone in [
            "Europe/Vienna",
            "Europe/Zurich",
            "Europe/Rome",
            "America/Los_Angeles",
        ] {
            assert!(zones.contains(&zone), "{zone}");
        }
    }

    #[test]
    fn a_take_off_is_in_its_zone() {
        // Innsbruck, the Nordkette above it, and Lienz.
        assert_eq!(zone_at(11.3933, 47.2692), Some("Europe/Vienna"));
        assert_eq!(zone_at(11.3797, 47.3075), Some("Europe/Vienna"));
        assert_eq!(zone_at(12.7697, 46.8297), Some("Europe/Vienna"));
        // Fiesch and Monte Grappa.
        assert_eq!(zone_at(8.1356, 46.4087), Some("Europe/Zurich"));
        assert_eq!(zone_at(11.7967, 45.8728), Some("Europe/Rome"));
        // Torrey Pines and Marshall Peak above San Bernardino, California.
        assert_eq!(zone_at(-117.2516, 32.8899), Some("America/Los_Angeles"));
        assert_eq!(zone_at(-117.3460, 34.2061), Some("America/Los_Angeles"));
        // Bir, India, and Valle de Bravo, Mexico.
        assert_eq!(zone_at(76.7234, 32.0522), Some("Asia/Kolkata"));
        assert_eq!(zone_at(-100.1300, 19.1950), Some("America/Mexico_City"));
    }

    #[test]
    fn both_sides_of_the_swiss_italian_border() {
        // Chiasso and Como, two kilometres apart across the border, and
        // the two halves of the Splügen pass's road, a kilometre either side.
        assert_eq!(zone_at(9.0316, 45.8326), Some("Europe/Zurich"));
        assert_eq!(zone_at(9.0852, 45.8081), Some("Europe/Rome"));
        assert_eq!(zone_at(9.3300, 46.5170), Some("Europe/Zurich"));
        assert_eq!(zone_at(9.3330, 46.4950), Some("Europe/Rome"));
        // Monte Generoso's summit, Swiss, and the slope 700 m east of it
        // into Italy; the two Ponte Tresas, Swiss and Italian, either side
        // of the river that is the border. Each by the full outlines too.
        assert_eq!(zone_at(9.0197, 45.9317), Some("Europe/Zurich"));
        assert_eq!(zone_at(9.0290, 45.9330), Some("Europe/Rome"));
        assert_eq!(zone_at(8.8640, 45.9690), Some("Europe/Zurich"));
        assert_eq!(zone_at(8.8610, 45.9640), Some("Europe/Rome"));
    }

    #[test]
    fn at_sea_there_is_no_zone() {
        assert_eq!(zone_at(-30.0, 40.0), None);
        assert_eq!(zone_at(-140.0, 0.0), None);
        assert_eq!(zone_at(f64::NAN, 47.0), None);
        assert_eq!(zone_at(11.39, f64::INFINITY), None);
    }

    #[test]
    fn the_date_line_and_the_poles_are_asked_without_harm() {
        for lon in [-180.0, 180.0, 179.9999, 540.0] {
            let _ = zone_at(lon, 0.0);
        }
        let _ = zone_at(0.0, 90.0);
        let _ = zone_at(0.0, -90.0);
        // Taveuni, Fiji, which 180° crosses, either side of it.
        assert_eq!(zone_at(179.98, -16.80), Some("Pacific/Fiji"));
        assert_eq!(zone_at(-179.98, -16.75), Some("Pacific/Fiji"));
    }
}
