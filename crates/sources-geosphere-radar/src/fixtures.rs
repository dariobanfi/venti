//! Answers recorded from GeoSphere Austria's dataset service on
//! 2026-11-21: the list of the runs it held and the first step of the two
//! newest, each whole and unchanged. `tests/fixtures/NOTICE.txt` says when
//! and how.
//!
//! The two frames were asked for as the adapter asks: with the first
//! region's box widened by a twentieth of a degree ([`crate::frame_url`]).
//! The answers hold a rectangle of INCA's grid of 608 by 263 points, and
//! every cell of the region's map lies within it.
//!
//! They were recorded in the same two minutes as the frames of
//! `venti-sources-dwd-radar`, so that one clock is honest for both: the
//! server's development mode shows the two as layers of one moment.
//!
//! The crate's own tests run the pipeline over them. The server's
//! development mode does the same and serves the result, so client work
//! needs neither the network nor a database.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};

use crate::{frame_url, list_url};

macro_rules! payload {
    ($name:literal) => {
        include_bytes!(concat!("../tests/fixtures/", $name)).as_slice()
    };
}

/// The list of runs, as the service gave it at 13:17:11 UTC.
pub const LIST: &[u8] = payload!("metadata.json");

fn at(hour: u32, minute: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, hour, minute, 0)
        .single()
        .expect("a valid date")
}

/// The area the recording is of: the first region's box.
///
/// # Panics
///
/// Never: the box is a constant.
pub fn area() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box")
}

/// Shortly after the answers were recorded. A run over them uses this as
/// its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    at(13, 19)
}

/// The moment up to which a run over the recorded answers takes the
/// frames as held: 12:30 UTC. The list names six runs and the recording
/// holds the two after this moment; a run that wanted more would ask for
/// frames nobody recorded.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn held_until() -> Timestamp {
    at(12, 30)
}

/// The recorded frames, the older first, each with the end of the quarter
/// of an hour it is of: 12:45 and 13:00 UTC.
pub fn frames() -> [(Timestamp, &'static [u8]); 2] {
    [
        (at(12, 45), payload!("rr-202611211245.nc")),
        (at(13, 0), payload!("rr-202611211300.nc")),
    ]
}

/// Every recorded response with the address the adapter asks it at.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    let [(older, first), (newer, second)] = frames();
    vec![
        (list_url(), LIST),
        (frame_url(older, 1, &area()), first),
        (frame_url(newer, 0, &area()), second),
    ]
}
