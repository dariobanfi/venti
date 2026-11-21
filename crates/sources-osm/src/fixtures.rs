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

use crate::{DEFAULT_INTERPRETER, OsmFreeFlying};

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
