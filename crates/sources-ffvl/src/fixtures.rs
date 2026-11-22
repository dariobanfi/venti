//! Answers recorded from data.gouv.fr on 2026-11-22 at 22:51 UTC.
//!
//! The dataset's description as it came, and the FFVL's file cut down to
//! the places around Annecy and the Mont Blanc (45.75° to 46.10° N, 6.05°
//! to 6.95° E) and a few more that show a reading of their own, each with
//! only the fields the adapter reads. `tests/fixtures/NOTICE.txt` says
//! what was cut and how to record them again.
//!
//! The data is the FFVL's, under the Licence Ouverte 2.0: see the
//! manifest.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Timestamp};

use crate::DATASET;

/// The address of the file that the recorded description names.
pub const FILE: &str = "https://static.data.gouv.fr/resources/terrains-de-pratique-de-la-ffvl-federation-francaise-de-vol-libre/20261122-071002/terrains.json";

/// Shortly after the answers were recorded. A run over them uses this as
/// its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 22, 22, 52, 0)
        .single()
        .expect("a valid date")
}

/// The area of a run over the recording: the box of the western Alps'
/// region pack (`regions/alps-west`), the one pack whose box takes in
/// France.
///
/// # Panics
///
/// Never: the edges are constants that form a box.
pub fn area() -> BoundingBox {
    BoundingBox::new(43.5, 5.0, 48.5, 9.4).expect("a valid box")
}

/// The recorded answers with the addresses they answer.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    vec![
        (
            DATASET.to_owned(),
            include_bytes!("../tests/fixtures/dataset.json").as_slice(),
        ),
        (
            FILE.to_owned(),
            include_bytes!("../tests/fixtures/terrains.json").as_slice(),
        ),
    ]
}
