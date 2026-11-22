//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three stations of the first region's box, with what they measured
//! between 18:00 and 18:30 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18. This
//! source was added four days later, and a station's answer reaches back
//! to the midnight of the day before only. So the sample is not an answer
//! of the service as it gave one. It is the station list cut down to the
//! three, and for each an answer written in the service's form with the
//! values that the service's other answer for one station, which reaches
//! back a week, gave for that evening. `tests/fixtures/NOTICE.txt` says
//! how, and the crate's tests hold the sample against those answers.
//!
//! The sample ends at 18:30 UTC, the last time for which the wind is known
//! (see `NOTICE.txt`): the development mode shows each station's newest
//! reading, and a newest one of temperature and humidity alone would put
//! three stations without a wind on its map, which is not what the network
//! looks like.
//!
//! The answers of 2026-11-22 are in `tests/fixtures` as well. They are what
//! the crate's tests read the adapter against, and they are not compiled
//! in.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::{stations_url, values_url};

/// The moment the sample is for: thirty-five minutes after its newest
/// values. On 2026-11-22 the service's newest values were 22 minutes old.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The sample, with the addresses the adapter asks: Rabbi in the valley
/// of the same name, Ciampac above Canazei, and Sass del Mul on the
/// Marmolada.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![
        (
            stations_url(),
            include_bytes!("../tests/fixtures/listaStazioni_evening.xml").as_slice(),
        ),
        (
            values_url("T0076"),
            include_bytes!("../tests/fixtures/T0076_evening.xml").as_slice(),
        ),
        (
            values_url("T0403"),
            include_bytes!("../tests/fixtures/T0403_evening.xml").as_slice(),
        ),
        (
            values_url("T0404"),
            include_bytes!("../tests/fixtures/T0404_evening.xml").as_slice(),
        ),
    ]
}
