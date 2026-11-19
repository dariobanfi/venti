//! Payloads recorded from the GeoSphere Austria Data Hub on 2026-11-18.
//!
//! The crate's own tests run the pipeline over them. The server's
//! development mode does the same and serves the result, so client work
//! needs neither the network nor a database.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::DATASET;

/// Shortly after the payloads were recorded. A run over them uses this as
/// its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0)
        .single()
        .expect("a valid date")
}

/// Every recorded response with the URL it answers.
///
/// The metadata is cut down to four stations, one of them inactive. The
/// second URL is the request the adapter builds from that metadata for a run
/// over the first region: the newest values of the three active stations.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    vec![
        (
            format!("{DATASET}/metadata"),
            include_bytes!("../tests/fixtures/metadata.json").as_slice(),
        ),
        (
            format!(
                "{DATASET}?parameters=FF,FFX,DD,DDX,TL,RF,P\
                 &station_ids=11126,11320,11343&output_format=geojson"
            ),
            include_bytes!("../tests/fixtures/current.json").as_slice(),
        ),
    ]
}
