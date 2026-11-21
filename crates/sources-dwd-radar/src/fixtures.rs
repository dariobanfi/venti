//! Answers recorded from DWD's open-data server on 2026-11-21: the list of
//! the frames of RY and the two frames it names last, each whole and
//! unchanged. `tests/fixtures/NOTICE.txt` says when and how.
//!
//! The crate's own tests run the pipeline over them. The server's
//! development mode does the same and serves the result, so client work
//! needs neither the network nor a database.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::{frame_url, list_url};

macro_rules! payload {
    ($name:literal) => {
        include_bytes!(concat!("../tests/fixtures/", $name)).as_slice()
    };
}

/// The list of frames, as DWD's server gave it at 13:18:20 UTC.
pub const LIST: &[u8] = payload!("list.html");

/// What a server answers that has no such file: not DWD's data, written
/// by hand for the tests.
pub const NOT_A_FRAME: &[u8] = payload!("damaged/not_a_frame.html");

/// The first 4096 bytes of the frame of 13:10: not a frame, cut for the
/// tests.
pub const CUT_SHORT: &[u8] = payload!("damaged/cut_after_4096_bytes.hdf5");

fn at(hour: u32, minute: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, hour, minute, 0)
        .single()
        .expect("a valid date")
}

/// Shortly after the list was recorded. A run over the recorded answers
/// uses this as its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    at(13, 19)
}

/// The moment up to which a run over the recorded answers takes the
/// frames as held: 13:05 UTC. The list names two days of frames and the
/// recording holds the two after this moment; a run that wanted more
/// would ask for frames nobody recorded.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn held_until() -> Timestamp {
    at(13, 5)
}

/// The recorded frames, the older first, each with the end of the five
/// minutes it is of: 13:10 and 13:15 UTC.
pub fn frames() -> [(Timestamp, &'static [u8]); 2] {
    [
        (
            at(13, 10),
            payload!("raa01-ry_10000-2611211310-dwd---bin.hdf5"),
        ),
        (
            at(13, 15),
            payload!("raa01-ry_10000-2611211315-dwd---bin.hdf5"),
        ),
    ]
}

/// Every recorded response with the address it answers.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    std::iter::once((list_url(), LIST))
        .chain(
            frames()
                .into_iter()
                .map(|(time, body)| (frame_url(time), body)),
        )
        .collect()
}
