//! A sample of the evening of 2026-11-18 for the server's development mode:
//! the reports of three airfields.
//!
//! The development mode's clock stands at 19:05 UTC on 2026-11-18, where the
//! first two sources were recorded. This source was added three days later.
//! Its service can be asked for an earlier moment (`date`), which the
//! adapter never does, and was asked for that one on 2026-11-21. The sample
//! is three reports of that answer, each as the service wrote it; it is
//! served as the answer to the adapter's own request for the first region.
//! `tests/fixtures/NOTICE.txt` says more, and the crate's tests hold the
//! sample against the answer it was cut from.
//!
//! The whole answer to the adapter's own request, recorded on 2026-11-21,
//! is in `tests/fixtures` as well. It is what the crate's tests read the
//! adapter against, and it is not compiled in.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};

use crate::reports_url;

/// The moment the sample is for: fifteen minutes after its reports.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn evening_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// The area the sample answers for: the first region's.
///
/// # Panics
///
/// Never: the bounds are constants.
pub fn area() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box")
}

/// The sample, with the address the adapter asks for [`area`]: Memmingen
/// on the plain before the Alps, with a gust; Innsbruck in the Inn valley;
/// and Samedan in the Engadin at 1708 m, with a wind of no direction.
pub fn evening() -> Vec<(String, &'static [u8])> {
    vec![(
        reports_url(&area()),
        include_bytes!("../tests/fixtures/metar_evening.json").as_slice(),
    )]
}
