//! An answer recorded from an Overpass instance on 2026-11-21.
//!
//! It is the answer to the adapter's own query for the first region
//! (46.3 to 48.5° N, 9.4 to 17.2° E): every element of OpenStreetMap there
//! that carries the free-flying tags, 517 of them, from the instance's
//! copy of OpenStreetMap of 2026-11-20 21:21 UTC, each with its geometry.
//! Each element is as OpenStreetMap had it, with one exception: of its
//! tags, only those the adapter reads are kept. The others are removed,
//! among them the names, phone numbers and e-mail addresses of operators,
//! because this repository holds no personal data. `tests/fixtures/NOTICE.txt` says
//! which instance answered and how to record it again.
//!
//! The data is OpenStreetMap's, available under the ODbL: see the
//! manifest.
//!
//! The crate's own tests run the pipeline over the file. A server's
//! development mode does the same and serves the result, so client work
//! needs neither the network nor a database.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};

use crate::{DEFAULT_INTERPRETER, OsmCables, OsmFreeFlying};

/// The interpreter the recorded answer stands for. An adapter made for
/// this address asks for the URL that [`recorded`] answers.
pub const INTERPRETER: &str = DEFAULT_INTERPRETER;

/// Shortly after the answer was recorded. A run over it uses this as its
/// time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 11, 46, 0)
        .single()
        .expect("a valid date")
}

/// The area the answer was recorded for: the box of the first region.
///
/// # Panics
///
/// Never: the edges are constants that form a box.
pub fn area() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box")
}

/// The recorded response with the URL it answers: the request of an
/// adapter for [`INTERPRETER`] whose run covers [`area`].
///
/// # Panics
///
/// Never: the address is a constant the adapter accepts.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    let source = OsmFreeFlying::new(INTERPRETER).expect("the default interpreter is an address");
    vec![(
        source.url(&area()),
        include_bytes!("../tests/fixtures/overpass_alps.json").as_slice(),
    )]
}

/// When the cables near the Tegernsee were recorded: shortly after the
/// answer came. A run over them uses this as its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn cables_recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 19, 51, 0)
        .single()
        .expect("a valid date")
}

/// The small box the cables were recorded for: the Tegernsee's south
/// shore, Rottach-Egern and the Wallberg.
///
/// # Panics
///
/// Never: the edges are constants that form a box.
pub fn cables_area() -> BoundingBox {
    BoundingBox::new(47.66, 11.70, 47.74, 11.82).expect("a valid box")
}

/// The recorded answer of the cables near the Tegernsee with the URL it
/// answers: the request of a cable adapter for [`INTERPRETER`] whose run
/// covers [`cables_area`]. `tests/fixtures/NOTICE.txt` says how it was
/// recorded.
///
/// # Panics
///
/// Never: the address is a constant the adapter accepts.
pub fn cables_recorded() -> Vec<(String, &'static [u8])> {
    let source = OsmCables::new(INTERPRETER).expect("the default interpreter is an address");
    vec![(
        source.url(&cables_area()),
        include_bytes!("../tests/fixtures/overpass_cables_tegernsee.json").as_slice(),
    )]
}

/// When the tile south of Munich was recorded: shortly after the answer
/// came. A run over the two tiles uses this as its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn cables_tiles_recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 23, 16, 24, 0)
        .single()
        .expect("a valid date")
}

/// A small box south of Munich across the grid line at 48° N, which the
/// cable adapter asks for in two tiles: 47.97 to 48° N and 48 to 48.03° N,
/// both from 11.42 to 11.6° E. Three lines cross from one into the other.
///
/// # Panics
///
/// Never: the edges are constants that form a box.
pub fn cables_tiles_area() -> BoundingBox {
    BoundingBox::new(47.97, 11.42, 48.03, 11.6).expect("a valid box")
}

/// The answers for the two tiles of [`cables_tiles_area`], each with the
/// URL it answers, the southern tile first. The southern one was recorded;
/// the northern one was not, and is made by hand from it: the three ways
/// of the southern answer with points in the northern tile, as the
/// instance sends a way, whole. `tests/fixtures/NOTICE.txt` says more.
///
/// # Panics
///
/// Never: the address is a constant the adapter accepts.
pub fn cables_tiles() -> Vec<(String, &'static [u8])> {
    let source = OsmCables::new(INTERPRETER).expect("the default interpreter is an address");
    let south = BoundingBox::new(47.97, 11.42, 48.0, 11.6).expect("a valid box");
    let north = BoundingBox::new(48.0, 11.42, 48.03, 11.6).expect("a valid box");
    vec![
        (
            source.url(&south),
            include_bytes!("../tests/fixtures/overpass_cables_tile_south.json").as_slice(),
        ),
        (
            source.url(&north),
            include_bytes!("../tests/fixtures/overpass_cables_tile_north.json").as_slice(),
        ),
    ]
}
