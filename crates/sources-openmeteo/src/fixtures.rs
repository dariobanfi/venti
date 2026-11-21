//! Answers recorded from an Open-Meteo instance on 2026-11-21, shortly
//! after midnight UTC.
//!
//! The instance was one run for the purpose, from Open-Meteo's own
//! container image, reading Open-Meteo's open data on AWS: the arrangement
//! a server's operator runs. It held the runs of 21 UTC of the evening
//! before, of ICON-D2 and of ICON-EU. (A first set, of 2026-11-19, was
//! replaced when the adapter began to ask for the heat the ground gives to
//! the air.) Each file is the answer to one of the adapter's own requests,
//! unchanged: which run of a model the instance holds (two files), and what
//! each model forecasts for five places. `tests/fixtures/NOTICE.txt` says
//! how they were recorded and how to record them again.
//!
//! The data is Deutscher Wetterdienst's, processed by Open-Meteo: see the
//! manifest for the credit.
//!
//! The crate's own tests run the pipeline over the files. A server's
//! development mode answers forecasts from them, for these five places and
//! no others, so client work needs neither the network nor an instance.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, GeoPoint, ModelId, Timestamp};
use venti_sources::{ForecastSource, PlanCtx};

use crate::OpenMeteo;

/// The address the recorded answers stand for. An adapter made for this
/// address asks for the URLs that [`recorded`] answers. It is the name the
/// instance has beside a server in the deployment, and nothing on the
/// internet.
pub const BASE: &str = "http://open-meteo:8080";

/// Shortly after the answers were recorded. A forecast run over them uses
/// this as its time.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 0, 30, 0)
        .single()
        .expect("a valid date")
}

/// A place with what the two models answered for it.
type Recorded = (f64, f64, &'static [u8], &'static [u8]);

/// The recorded places: latitude, longitude, the answer of ICON-D2 and the
/// answer of ICON-EU.
///
/// The first four lie around Tegernsee: the east shore of the lake, the
/// take-off on the Wallberg, and the take-off and the landing below the
/// Jägerkamp near Spitzingsee, each rounded to a fiftieth of a degree. The
/// fifth is the middle of the first region, which a server asks for when it
/// looks at its instance.
const PLACES: [Recorded; 5] = [
    (
        47.70,
        11.80,
        include_bytes!("../tests/fixtures/icon_d2_47.70_11.80.json"),
        include_bytes!("../tests/fixtures/icon_eu_47.70_11.80.json"),
    ),
    (
        47.66,
        11.80,
        include_bytes!("../tests/fixtures/icon_d2_47.66_11.80.json"),
        include_bytes!("../tests/fixtures/icon_eu_47.66_11.80.json"),
    ),
    (
        47.70,
        11.88,
        include_bytes!("../tests/fixtures/icon_d2_47.70_11.88.json"),
        include_bytes!("../tests/fixtures/icon_eu_47.70_11.88.json"),
    ),
    (
        47.68,
        11.90,
        include_bytes!("../tests/fixtures/icon_d2_47.68_11.90.json"),
        include_bytes!("../tests/fixtures/icon_eu_47.68_11.90.json"),
    ),
    (
        47.40,
        13.30,
        include_bytes!("../tests/fixtures/icon_d2_47.40_13.30.json"),
        include_bytes!("../tests/fixtures/icon_eu_47.40_13.30.json"),
    ),
];

const RUN_ICON_D2: &[u8] = include_bytes!("../tests/fixtures/meta_icon_d2.json");
const RUN_ICON_EU: &[u8] = include_bytes!("../tests/fixtures/meta_icon_eu.json");

/// The places the answers were recorded for.
///
/// # Panics
///
/// Never: the coordinates are constants on the globe.
pub fn places() -> Vec<GeoPoint> {
    PLACES
        .iter()
        .map(|(lat, lon, _, _)| GeoPoint::new(*lat, *lon).expect("a place on the globe"))
        .collect()
}

/// The recorded responses with the URLs they answer: the requests of an
/// adapter for [`BASE`] that is asked for each of [`places`].
///
/// The addresses are the adapter's own: each is taken from the request the
/// adapter makes, so the two cannot drift apart.
///
/// # Panics
///
/// Panics if the adapter no longer reads the recorded answers about the
/// runs, which the crate's tests rule out.
pub fn recorded() -> Vec<(String, &'static [u8])> {
    let source = OpenMeteo::new(BASE).expect("the address of the recording is an address");
    let ctx = PlanCtx {
        now: recorded_at(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box"),
    };
    let mut responses = Vec::new();
    for (place, (_, _, icon_d2, icon_eu)) in places().into_iter().zip(PLACES) {
        let runs = source.plan(place, &ctx);
        for (run, (held, series)) in runs
            .iter()
            .zip([(RUN_ICON_D2, icon_d2), (RUN_ICON_EU, icon_eu)])
        {
            let asked = source
                .parse(run, held, place, &ctx)
                .expect("the recorded answer about the run is read");
            responses.push((run.url.clone(), held));
            responses.push((asked.follow_up[0].url.clone(), series));
        }
    }
    responses
}

/// Answers to the adapter's requests for several places at once, recorded
/// from the same kind of instance on 2026-11-21 at 03:20 UTC: which run of
/// each model it held, and what each model forecasts for the four places
/// of [`grid_places`] in one request. It then held the run of ICON-D2 of
/// 2026-11-21 00:00 UTC, newer than the one of the answers for single
/// places, and the run of ICON-EU of 2026-11-20 21:00 UTC, the same.
///
/// The address of the answers about the runs is the same as for single
/// places, and the answers differ: the two sets are for two fetchers, and
/// are never put into one.
const PLACES_RUN_ICON_D2: &[u8] = include_bytes!("../tests/fixtures/places/meta_icon_d2.json");
const PLACES_RUN_ICON_EU: &[u8] = include_bytes!("../tests/fixtures/places/meta_icon_eu.json");
const PLACES_ICON_D2: &[u8] = include_bytes!("../tests/fixtures/places/icon_d2.json");
const PLACES_ICON_EU: &[u8] = include_bytes!("../tests/fixtures/places/icon_eu.json");

/// Shortly after the answers for several places were recorded.
///
/// # Panics
///
/// Never: the date is a constant.
pub fn places_recorded_at() -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 21, 3, 25, 0)
        .single()
        .expect("a valid date")
}

/// The four places of the answers for several places, in the order they
/// were asked for: the points of a grid of a tenth of a degree around the
/// Tegernsee and the Spitzingsee, south-west first, west to east, rows
/// south to north.
///
/// # Panics
///
/// Never: the coordinates are constants on the globe.
pub fn grid_places() -> Vec<GeoPoint> {
    [(47.6, 11.8), (47.6, 11.9), (47.7, 11.8), (47.7, 11.9)]
        .iter()
        .map(|(lat, lon)| GeoPoint::new(*lat, *lon).expect("a place on the globe"))
        .collect()
}

/// The recorded responses for several places with the URLs they answer:
/// the requests of an adapter for [`BASE`] that is asked for the forecast
/// of each model for all of [`grid_places`] at once.
///
/// # Panics
///
/// Panics if the adapter no longer reads the recorded answers about the
/// runs, which the crate's tests rule out.
pub fn recorded_places() -> Vec<(String, &'static [u8])> {
    let source = OpenMeteo::new(BASE).expect("the address of the recording is an address");
    let ctx = PlanCtx {
        now: places_recorded_at(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).expect("a valid box"),
    };
    let points = grid_places();
    let mut responses = Vec::new();
    for (model, held, series) in [
        ("icon-d2", PLACES_RUN_ICON_D2, PLACES_ICON_D2),
        ("icon-eu", PLACES_RUN_ICON_EU, PLACES_ICON_EU),
    ] {
        let model = ModelId::new(model).expect("a model's name");
        for run in source.plan_places(&model, &points, &ctx) {
            let asked = source
                .parse_places(&run, held, &points, &ctx)
                .expect("the recorded answer about the run is read");
            responses.push((run.url.clone(), held));
            responses.push((asked.follow_up[0].url.clone(), series));
        }
    }
    responses
}
