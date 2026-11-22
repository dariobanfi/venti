//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three stations of the first region's box, with what they measured over
//! the ten minutes to 18:50 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18. This
//! source was added four days later, and its file holds the newest values
//! only; ARSO's two-day history of a station did not reach that evening
//! either. So the sample is not an answer of ARSO as it gave one. It is the
//! recorded file of 2026-11-22 cut down to three stations, with each one's
//! times and values replaced by what ARSO's archive of measurements gives
//! for that station and those ten minutes. `tests/fixtures/NOTICE.txt` says
//! how, and the crate's tests hold the sample against the recorded archive.
//!
//! The whole recorded file is in `tests/fixtures` as well, cut down to
//! fifteen stations. It is what the crate's tests read the adapter against,
//! and it is not compiled in.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::latest_url;

/// The moment the sample is for: fifteen minutes after its values.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The sample, with the address the adapter asks: Kredarica below the
/// Triglav, Rogla on the Pohorje and Uršlja gora above Carinthia's valleys.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![(
        latest_url(),
        include_bytes!("../tests/fixtures/observationAms_evening.xml").as_slice(),
    )]
}
