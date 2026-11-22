//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three stations of Graubünden, with what they measured at 18:50 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18, where the
//! first two sources were recorded. This source was added four days later,
//! and its file of current values holds the newest values only. So the
//! sample is not an answer of MeteoSwiss as it gave one. It is put together
//! from answers that are: the station list, cut down to three stations, and
//! for their values a row each in the form of the current file, with what
//! each station's own file of ten-minute values gives for that evening.
//! `tests/fixtures/NOTICE.txt` says how, and the crate's tests hold the
//! sample against those files.
//!
//! The whole answers of 2026-11-22 are in `tests/fixtures` as well. They are
//! what the crate's tests read the adapter against, and they are not
//! compiled in.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::{stations_url, values_url};

/// The moment the sample is for: fifteen minutes after its values. When
/// the file was asked on 2026-11-22 at 08:09 UTC, its values were of 07:50.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The sample, with the addresses the adapter asks: Davos, the
/// Weissfluhjoch on the ridge above it, and Samedan in the Engadine.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![
        (
            stations_url(),
            include_bytes!("../tests/fixtures/stations_evening.csv").as_slice(),
        ),
        (
            values_url(),
            include_bytes!("../tests/fixtures/VQHA80_evening.csv").as_slice(),
        ),
    ]
}
