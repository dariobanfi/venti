//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three stations, with what they measured at 18:50 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18, where the
//! first two sources were recorded. This source was added three days later,
//! and its service answers with the newest values only. So the sample is not
//! an answer of the service as it gave one. It is put together from answers
//! that are: the list of stations, cut down to three, and for their values
//! the rows of the answer of 2026-11-21 with the date and the value that the
//! service's time series gives for that evening. `tests/fixtures/NOTICE.txt`
//! says how, and the crate's tests hold the sample against the recorded
//! time series.
//!
//! The whole answers of 2026-11-21 are in `tests/fixtures` as well. They are
//! what the crate's tests read the adapter against, and they are not
//! compiled in.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::{sensors_url, stations_url};

/// The moment the sample is for: fifteen minutes after its values. When the
/// service was asked three times on 2026-11-21, its newest values were 12,
/// 17 and 21 minutes old.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The sample, with the addresses the adapter asks: Sterzing in the valley
/// south of the Brenner, the Jaufenkamm on the ridge above it, and Bozen.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![
        (
            stations_url(),
            include_bytes!("../tests/fixtures/stations_evening.json").as_slice(),
        ),
        (
            sensors_url(),
            include_bytes!("../tests/fixtures/sensors_evening.json").as_slice(),
        ),
    ]
}
