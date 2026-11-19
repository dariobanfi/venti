//! Payloads recorded from DWD's open-data server on 2026-11-18.
//!
//! The crate's own tests run the pipeline over them. The server's
//! development mode does the same and serves the result, so client work
//! needs neither the network nor a database.

use chrono::{TimeZone, Utc};
use venti_domain::Timestamp;

use crate::Dataset;

macro_rules! payload {
    ($name:literal) => {
        include_bytes!(concat!("../tests/fixtures/", $name)).as_slice()
    };
}

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
/// These are the three station lists, cut down to a handful of stations, and
/// the day's files of Zugspitze, Hohenpeißenberg and Garmisch-Partenkirchen.
/// Garmisch has no temperature file, as happens when a station's file is
/// late: a run over the first region asks for it and gets a 404.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    let lists = [
        (
            Dataset::Wind,
            payload!("zehn_now_ff_Beschreibung_Stationen.txt"),
        ),
        (
            Dataset::Extremes,
            payload!("zehn_now_fx_Beschreibung_Stationen.txt"),
        ),
        (
            Dataset::Air,
            payload!("zehn_now_tu_Beschreibung_Stationen.txt"),
        ),
    ];
    let files = [
        (
            Dataset::Wind,
            "05792",
            payload!("10minutenwerte_wind_05792_now.zip"),
        ),
        (
            Dataset::Wind,
            "02290",
            payload!("10minutenwerte_wind_02290_now.zip"),
        ),
        (
            Dataset::Wind,
            "01550",
            payload!("10minutenwerte_wind_01550_now.zip"),
        ),
        (
            Dataset::Extremes,
            "05792",
            payload!("10minutenwerte_extrema_wind_05792_now.zip"),
        ),
        (
            Dataset::Extremes,
            "02290",
            payload!("10minutenwerte_extrema_wind_02290_now.zip"),
        ),
        (
            Dataset::Extremes,
            "01550",
            payload!("10minutenwerte_extrema_wind_01550_now.zip"),
        ),
        (
            Dataset::Air,
            "05792",
            payload!("10minutenwerte_TU_05792_now.zip"),
        ),
        (
            Dataset::Air,
            "02290",
            payload!("10minutenwerte_TU_02290_now.zip"),
        ),
    ];

    let lists = lists
        .into_iter()
        .map(|(dataset, body)| (dataset.station_list_request().url, body));
    let files = files
        .into_iter()
        .map(|(dataset, station, body)| (dataset.data_request(station).url, body));
    lists.chain(files).collect()
}
