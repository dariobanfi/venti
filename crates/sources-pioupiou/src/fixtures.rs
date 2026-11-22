//! A sample of the evening of 2026-11-18 for the server's development mode:
//! three sensors of the first region, with what they measured shortly
//! after 19:00 UTC.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18. This
//! source was added four days later, and its live answer holds the newest
//! reading only. So the sample is not an answer of the network as it gave
//! one. It is the live answer of 2026-11-22 cut down to three sensors, with
//! each one's position and reading replaced by the last row before 19:05
//! UTC of the network's own archive for that sensor. `tests/fixtures/NOTICE.txt`
//! says how, and the crate's tests hold the sample against the recorded
//! archive.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::live_url;

/// The moment the sample is for.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The sample, with the address the adapter asks: the Kampenwand above the
/// Chiemsee, the Osterfelder take-off above Garmisch, and the
/// Coburgerhütte on the Mieminger Kette.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![(
        live_url(),
        include_bytes!("../tests/fixtures/live_evening.json").as_slice(),
    )]
}
