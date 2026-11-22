//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three stations of the first region's box, with what they measured in the
//! 24 hours before 18:30 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18. This
//! source was added on 2026-11-22, and its answer for all stations holds the
//! last 24 hours only. So the sample is not an answer of the service as it
//! gave one. It is put together from answers that are: the list of stations,
//! cut down to three, and for their values the rows of each station's own
//! history of seven days that fall in the 24 hours the answer for all
//! stations would have held. `tests/fixtures/NOTICE.txt` says how, and the
//! crate's tests hold the sample against the recorded histories.
//!
//! The answers of 2026-11-22 are in `tests/fixtures` as well. They are what
//! the crate's tests read the adapter against, and they are not compiled in.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::{measurements_url, stations_url};

/// The moment the sample is for: 35 minutes after its newest values. When
/// the service was asked on 2026-11-22 at 08:09 UTC, half of the stations
/// had a value of 08:00 and half one of 07:30, so the half hour that ended
/// five minutes before is not taken to be there yet.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The sample, with the addresses the adapter asks: the Chrachenhorn on the
/// ridge south of Davos, the Weissfluhjoch above it, and the Spitz in
/// Liechtenstein.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![
        (
            stations_url(),
            include_bytes!("../tests/fixtures/stations_evening.json").as_slice(),
        ),
        (
            measurements_url(),
            include_bytes!("../tests/fixtures/measurements_evening.json").as_slice(),
        ),
    ]
}
