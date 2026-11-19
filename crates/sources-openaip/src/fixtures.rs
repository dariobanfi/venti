//! Payloads recorded from openAIP's export store on 2026-11-19.
//!
//! The two files are the airspace exports for Germany and Austria as
//! published that day, cut down to the airspace a pilot from Munich meets
//! in the northern Alps: around Munich, Innsbruck and Salzburg. Each
//! feature that was kept is as openAIP published it, except that the user
//! identifiers of its contributors are removed. Left out are the features
//! outside that area and those with more than a couple of hundred corners,
//! which are the flight information regions and the largest nature
//! reserves.
//!
//! The data is openAIP's, licensed under CC BY-NC 4.0: see the manifest.
//!
//! The crate's own tests run the pipeline over these files. A server's
//! development mode can do the same and serve the result, so client work
//! needs neither the network nor a database.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::export_url;

/// Shortly after the payloads were recorded. A run over them uses this as
/// its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 19, 13, 35, 0)
        .single()
        .expect("a valid date")
}

/// Every recorded response with the URL it answers: 41 airspaces of
/// Germany and 47 of Austria.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    vec![
        (
            export_url("DE"),
            include_bytes!("../tests/fixtures/de_asp.geojson").as_slice(),
        ),
        (
            export_url("AT"),
            include_bytes!("../tests/fixtures/at_asp.geojson").as_slice(),
        ),
    ]
}
