//! What the server's development mode and the API's tests feed the live
//! service with, without connecting anywhere. **Made up, not network
//! data**: lines and a device database written by hand in the network's
//! form, because the network forbids passing on its data older than 24
//! hours. `tests/fixtures/NOTICE.txt` lists what each holds.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};

/// The moment the made-up lines are for: shortly after the newest.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn feed_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 19, 9, 10)
        .single()
        .expect("a valid date")
}

/// The first region's box, which the lines lie in.
///
/// # Panics
///
/// Never: the bounds are constants.
pub fn area() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box")
}

/// The made-up lines of the feed, over the Tegernsee and Munich.
pub fn feed() -> &'static str {
    include_str!("../tests/fixtures/feed_made_up.txt")
}

/// The made-up device database, in the network's form.
pub fn directory() -> &'static [u8] {
    include_bytes!("../tests/fixtures/ddb_made_up.json")
}
