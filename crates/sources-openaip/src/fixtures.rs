//! Payloads recorded from openAIP's export store on 2026-11-19 and
//! 2026-11-22.
//!
//! The files are the airspace exports as published on those days for the
//! ten countries the adapter fetches, cut down. Those of Germany and
//! Austria (2026-11-19) hold the airspace a pilot from Munich meets in the
//! northern Alps: around Munich, Innsbruck and Salzburg. Those of
//! Switzerland, Italy, Slovenia, Croatia, Hungary and Slovakia (2026-11-19)
//! hold the smaller airspaces that reach into the first region's box.
//! France's (2026-11-22) holds the small airspaces from Grenoble to Annecy
//! and Chamonix, and Bosnia and Herzegovina's (2026-11-22) the smaller ones
//! that reach into the box of the three Alpine regions. Each feature that was kept is as openAIP
//! published it, except that the user identifiers of its contributors are
//! removed. Left out are the features outside those areas and the ones with
//! many corners, which are the flight information regions, the large
//! control areas and the largest nature reserves.
//!
//! So the files are a sample, not the airspace of the region: nobody may
//! fly by them.
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
/// Germany, 47 of Austria, 13 of Switzerland, 46 of France, 25 of Italy,
/// 15 of Slovenia, 27 of Croatia, 19 of Bosnia and Herzegovina, 16 of
/// Hungary and 8 of Slovakia.
///
/// A run over the first region's box keeps none of France's and Bosnia's:
/// they lie outside it.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    let files: [(&str, &'static [u8]); 10] = [
        ("DE", include_bytes!("../tests/fixtures/de_asp.geojson")),
        ("AT", include_bytes!("../tests/fixtures/at_asp.geojson")),
        ("CH", include_bytes!("../tests/fixtures/ch_asp.geojson")),
        ("FR", include_bytes!("../tests/fixtures/fr_asp.geojson")),
        ("IT", include_bytes!("../tests/fixtures/it_asp.geojson")),
        ("SI", include_bytes!("../tests/fixtures/si_asp.geojson")),
        ("HR", include_bytes!("../tests/fixtures/hr_asp.geojson")),
        ("BA", include_bytes!("../tests/fixtures/ba_asp.geojson")),
        ("HU", include_bytes!("../tests/fixtures/hu_asp.geojson")),
        ("SK", include_bytes!("../tests/fixtures/sk_asp.geojson")),
    ];
    files
        .into_iter()
        .map(|(country, body)| (export_url(country), body))
        .collect()
}
