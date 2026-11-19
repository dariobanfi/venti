//! Tests of airspace: the endpoint through the real router, and the path
//! from a provider's files into the store.
//!
//! The endpoint is tested against what the development mode serves: a
//! sample of the first region's airspace recorded from openAIP, eight
//! countries of it, run through the real adapter. Two answers are kept as
//! golden files, which are also the examples in `docs/api.md`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::http::{StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    AirspaceRequest, AirspaceResponse, ErrorCode, HealthResponse, HealthStatus, MAX_AIRSPACES,
};
use venti_domain::{
    Activity, Airspace, AirspaceClass, AirspaceType, BoundingBox, GeoPoint, Polygon, SourceId,
    Validity, VerticalLimit,
};
use venti_server::adapters;
use venti_server::api::{Api, Cors};
use venti_server::ingest::{Collector, DirArchive};
use venti_server::plan::Job;
use venti_sources::testing::{FixtureFetch, assert_golden};
use venti_sources::{Kind, Manifest};
use venti_store::{Coverage, MemoryStore, RunRecord, Store};

mod common;

use common::{TestClock, get, get_with, golden, recorded, settings};

// --- Helpers ----------------------------------------------------------------

const OPENAIP: &str = "openaip";

fn source(name: &str) -> SourceId {
    SourceId::new(name).unwrap()
}

fn first_region() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

/// The countries the first region's box takes in and openAIP has a file
/// for, in the order the adapter fetches them.
const COUNTRIES: [&str; 8] = ["DE", "AT", "CH", "IT", "SI", "HR", "HU", "SK"];

/// How many airspaces the recorded files hold in all.
const RECORDED: usize = 192;

fn path(bbox: BoundingBox) -> String {
    AirspaceRequest { bbox }.path_and_query()
}

/// The military training ground at Hochfilzen: a danger area and a
/// restricted area over the same five corners, and nothing else.
const HOCHFILZEN: &str = "/v1/airspace?bbox=12.65,47.48,12.66,47.49";

/// A square airspace whose south-west corner is at the given position.
fn square(id: &str, south: f64, west: f64, size: f64) -> Airspace {
    let point = |lat, lon| GeoPoint::new(lat, lon).unwrap();
    let (north, east) = (south + size, west + size);
    let ring = vec![
        point(south, west),
        point(south, east),
        point(north, east),
        point(north, west),
        point(south, west),
    ];
    Airspace {
        id: id.parse().unwrap(),
        name: id.to_uppercase(),
        class: AirspaceClass::D,
        airspace_type: AirspaceType::Ctr,
        country: None,
        lower: VerticalLimit::ground(),
        upper: VerticalLimit::flight_level(100).unwrap(),
        lower_at_least: None,
        upper_at_most: None,
        geometry: Polygon::new(ring, Vec::new()).unwrap(),
        activity: Activity::default(),
        frequencies: Vec::new(),
        remarks: None,
        validity: Validity::default(),
    }
}

/// A healthy run of an airspace source at the time of the recording.
fn run(source_name: &str, airspaces: usize) -> RunRecord {
    RunRecord {
        source: source(source_name),
        started_at: adapters::recorded_at(),
        finished_at: adapters::recorded_at(),
        healthy: true,
        requests: 2,
        failed_requests: 0,
        skipped: 0,
        rejected: 0,
        stations: 0,
        observations: 0,
        newest_observation: None,
        airspaces: u32::try_from(airspaces).unwrap(),
    }
}

/// Where the hand-made sets of these tests say they are whole: Austria
/// within the first region.
fn austria() -> Coverage {
    Coverage {
        boxes: vec![first_region()],
        countries: vec!["AT".to_owned()],
    }
}

/// The manifest of a second airspace source, under another licence.
fn charts() -> Manifest {
    let text = r#"
        id = "charts"
        provider = "Example Charts"
        kinds = ["airspace"]
        countries = ["AT"]
        cadence_seconds = 86400
        auth = "none"

        [licence]
        name = "CC BY 4.0"
        commercial = true
        redistribution = true
        share_alike = false
        attribution = "Example Charts"
        link = "https://charts.example"
        verified_on = "2026-11-19"
        verified_from = ["https://charts.example/licence"]
    "#;
    Manifest::from_toml(text).unwrap()
}

/// An API over `store` that serves the sources `manifests` describe.
fn serving<'a>(
    store: Arc<MemoryStore>,
    clock: &TestClock,
    manifests: impl IntoIterator<Item = &'a Manifest>,
) -> Router {
    Api::new(store, Arc::new(clock.clone()), manifests, settings()).router(&Cors::Any)
}

fn openaip_manifest() -> Manifest {
    let registry = adapters::registry();
    registry
        .manifest(&source(OPENAIP), Kind::Airspace)
        .unwrap()
        .clone()
}

// --- The endpoint over the recorded data ------------------------------------

#[tokio::test]
async fn airspace_comes_in_a_layer_with_its_source_its_dates_and_its_credit() {
    let answer = get(&recorded().await, &path(first_region())).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    // Airspace changes once a day at most.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=3600");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    let airspace: AirspaceResponse = answer.json();

    assert_eq!(airspace.layers.len(), 1);
    let layer = &airspace.layers[0];
    assert_eq!(layer.source.as_str(), OPENAIP);
    // 41 airspaces of Germany, 47 of Austria and 104 of the six other
    // countries the box takes in.
    assert_eq!(layer.airspaces.len(), RECORDED);
    for country in COUNTRIES {
        let of_country = |a: &Airspace| a.country.as_deref() == Some(country);
        assert!(layer.airspaces.iter().any(of_country), "{country}");
    }
    assert!(
        layer
            .airspaces
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    );
    // Every record belongs to the layer's source, and to no other.
    assert!(
        layer
            .airspaces
            .iter()
            .all(|a| a.id.source() == &layer.source)
    );

    // How old the data is: when the server fetched it, the date of the
    // files, and when the source last changed any of it. The recorded
    // files come without a date of their own, so they are as old as their
    // fetch.
    assert_eq!(layer.fetched_at, adapters::recorded_at());
    assert_eq!(layer.file_date, Some(adapters::recorded_at()));
    assert!(layer.newest_change.unwrap() < layer.fetched_at);
    for record in &layer.airspaces {
        assert_eq!(record.validity.file_date, layer.file_date);
    }

    // Where the server's set is whole: the region it collects for, of
    // every country the region takes in that the source has a file for.
    // The valley of the apps' sample flight is inside, and so is South
    // Tyrol; the north of Germany is not, and neither is Zurich.
    assert_eq!(layer.covers, [first_region()]);
    assert_eq!(layer.countries, COUNTRIES);
    let covered = |lat, lon| {
        let place = GeoPoint::new(lat, lon).unwrap();
        layer.covers.iter().any(|area| area.contains(place))
    };
    assert!(covered(47.7, 11.8));
    assert!(covered(46.5, 11.35));
    assert!(!covered(53.6, 9.9));
    assert!(!covered(47.4, 8.5));

    // What a client has to show: the credit, the licence and the warning.
    assert_eq!(airspace.attributions.len(), 1);
    let credit = &airspace.attributions[0];
    assert_eq!(credit.source, layer.source);
    assert_eq!(credit.licence, "CC BY-NC 4.0");
    assert!(credit.text.contains("openAIP"));
    let notice = credit.notice.as_deref().unwrap();
    assert!(
        notice.contains("not certified") && notice.contains("navigation"),
        "{notice}"
    );
}

#[tokio::test]
async fn a_small_box_gets_what_touches_it() {
    let router = recorded().await;

    let answer = get(&router, HOCHFILZEN).await;
    let airspace: AirspaceResponse = answer.json();
    let names: Vec<&str> = airspace.layers[0]
        .airspaces
        .iter()
        .map(|airspace| airspace.name.as_str())
        .collect();
    assert_eq!(names, ["LOD22 (HOCHFILZEN)", "LOR8 (HOCHFILZEN)"]);
    assert_golden(&golden("airspace.json"), &airspace);

    // A box with no airspace in it gets the layer all the same, without
    // airspaces: what it says about the box is in `covers`. Inside, the
    // source has nothing there. "Nothing here" is openAIP's data too, and
    // comes with its credit and its warning.
    let quiet = "/v1/airspace?bbox=14.90,48.20,14.91,48.21";
    let nothing: AirspaceResponse = get(&router, quiet).await.json();
    assert_eq!(nothing.layers.len(), 1);
    let layer = &nothing.layers[0];
    assert!(layer.airspaces.is_empty());
    assert_eq!(layer.covers, [first_region()]);
    assert_eq!(layer.fetched_at, adapters::recorded_at());
    assert!(nothing.attributions[0].notice.is_some());
    assert_golden(&golden("airspace_none.json"), &nothing);

    // Outside, the answer looks the same and says nothing about the box:
    // the client compares the box it asked for with `covers`.
    let nowhere: AirspaceResponse = get(&router, "/v1/airspace?bbox=0,0,1,1").await.json();
    assert_eq!(nowhere.layers.len(), 1);
    assert!(nowhere.layers[0].airspaces.is_empty());
    let asked = GeoPoint::new(0.5, 0.5).unwrap();
    assert!(
        !nowhere.layers[0]
            .covers
            .iter()
            .any(|area| area.contains(asked))
    );
}

#[tokio::test]
async fn an_unchanged_set_is_not_sent_again_and_a_whole_region_is_compressed() {
    let router = recorded().await;
    let whole = path(first_region());
    let first = get(&router, &whole).await;
    let tag = first.header(header::ETAG).to_owned();

    let again = get_with(&router, &whole, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());
    assert_eq!(again.header(header::CACHE_CONTROL), "public, max-age=3600");

    let packed = get_with(&router, &whole, &[(header::ACCEPT_ENCODING, "gzip")]).await;
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");
    assert!(packed.body.len() * 4 < first.body.len());
    assert_eq!(packed.header(header::ETAG), tag);
}

#[tokio::test]
async fn a_bad_box_is_explained_as_for_stations() {
    let router = recorded().await;

    let missing = get(&router, "/v1/airspace").await;
    let message = missing.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    assert!(message.contains("bbox=10.5,47.2,11.9,47.8"), "{message}");

    let swapped = get(&router, "/v1/airspace?bbox=10.5,47.8,11.9,47.2").await;
    let message = swapped.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    assert!(message.contains("wrong order"), "{message}");
}

// --- Stores other than the recorded one -------------------------------------

#[tokio::test]
async fn two_sources_are_two_layers_each_with_its_own_credit() {
    let store = Arc::new(MemoryStore::new());
    let ours = [square("openaip:innsbruck", 47.2, 11.3, 0.1)];
    let theirs = [
        square("charts:innsbruck", 47.2, 11.3, 0.1),
        square("charts:salzburg", 47.7, 12.9, 0.1),
    ];
    let tyrol = Coverage {
        boxes: vec![BoundingBox::new(46.8, 10.1, 47.6, 12.9).unwrap()],
        countries: vec!["AT".to_owned()],
    };
    store
        .save_airspace_run(&run(OPENAIP, 1), &tyrol, &ours)
        .await
        .unwrap();
    store
        .save_airspace_run(&run("charts", 2), &austria(), &theirs)
        .await
        .unwrap();
    let router = serving(store, &TestClock::new(), [&openaip_manifest(), &charts()]);

    let airspace: AirspaceResponse = get(&router, &path(first_region())).await.json();

    // The same control zone from two sources is two records in two
    // layers: nothing is merged.
    let layers: Vec<(&str, usize)> = airspace
        .layers
        .iter()
        .map(|layer| (layer.source.as_str(), layer.airspaces.len()))
        .collect();
    assert_eq!(layers, [("charts", 2), (OPENAIP, 1)]);
    for layer in &airspace.layers {
        assert!(
            layer
                .airspaces
                .iter()
                .all(|a| a.id.source() == &layer.source)
        );
    }
    let credits: Vec<(&str, &str, bool)> = airspace
        .attributions
        .iter()
        .map(|a| (a.source.as_str(), a.licence.as_str(), a.notice.is_some()))
        .collect();
    assert_eq!(
        credits,
        [
            ("charts", "CC BY 4.0", false),
            (OPENAIP, "CC BY-NC 4.0", true)
        ]
    );

    // Each layer says for itself where it is whole.
    assert_eq!(airspace.layers[0].covers, austria().boxes);
    assert_eq!(airspace.layers[1].covers, tyrol.boxes);

    // A box only one of them reaches into has both layers, one of them
    // without airspaces, and here outside what that one covers.
    let salzburg: AirspaceResponse = get(&router, "/v1/airspace?bbox=12.95,47.7,13.0,47.8")
        .await
        .json();
    let layers: Vec<(&str, usize)> = salzburg
        .layers
        .iter()
        .map(|layer| (layer.source.as_str(), layer.airspaces.len()))
        .collect();
    assert_eq!(layers, [("charts", 1), (OPENAIP, 0)]);
    assert_eq!(salzburg.layers[1].covers, tyrol.boxes);
    assert_eq!(salzburg.attributions.len(), 2);
}

#[tokio::test]
async fn airspace_of_a_source_the_server_does_not_serve_stays_inside() {
    // The store still holds the set of a source that was taken out of the
    // configuration, for example because its licence changed.
    let store = Arc::new(MemoryStore::new());
    let theirs = [square("charts:innsbruck", 47.2, 11.3, 0.1)];
    store
        .save_airspace_run(&run("charts", 1), &austria(), &theirs)
        .await
        .unwrap();
    let router = serving(store, &TestClock::new(), [&openaip_manifest()]);

    let airspace: AirspaceResponse = get(&router, &path(first_region())).await.json();
    assert!(airspace.layers.is_empty());
    assert!(airspace.attributions.is_empty());
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert_eq!(health.sources.len(), 1);
    assert_eq!(health.sources[0].airspaces, Some(0));
}

#[tokio::test]
async fn a_box_with_too_many_airspaces_is_refused_and_not_cut() {
    let store = Arc::new(MemoryStore::new());
    // One more than an answer carries, in a row from west to east.
    let many: Vec<Airspace> = (0..=MAX_AIRSPACES)
        .map(|n| square(&format!("openaip:{n:05}"), 47.0, n as f64 * 0.001, 0.0005))
        .collect();
    store
        .save_airspace_run(&run(OPENAIP, many.len()), &austria(), &many)
        .await
        .unwrap();
    let router = serving(store, &TestClock::new(), [&openaip_manifest()]);

    let all = get(&router, "/v1/airspace?bbox=-1,46,6,48").await;
    let message = all.error(StatusCode::BAD_REQUEST, ErrorCode::BoxTooLarge);
    assert!(message.contains("smaller boxes"), "{message}");

    // Exactly as many as an answer carries is an answer, and a whole one.
    let most = get(&router, "/v1/airspace?bbox=0,46,4.9995,48").await;
    let airspace: AirspaceResponse = most.ok(StatusCode::OK);
    assert_eq!(airspace.layers[0].airspaces.len(), MAX_AIRSPACES);
}

// --- From the provider's files into the store -------------------------------

fn airspace_job() -> Job {
    Job {
        source: source(OPENAIP),
        kind: Kind::Airspace,
        area: first_region(),
    }
}

fn collector(
    store: Arc<MemoryStore>,
    fetch: FixtureFetch,
    clock: &TestClock,
) -> Collector<MemoryStore, FixtureFetch, Option<DirArchive>> {
    Collector::new(
        adapters::registry(),
        store,
        fetch,
        None,
        Arc::new(clock.clone()),
        None,
    )
}

/// The recorded files of every country, with the date openAIP's file
/// store put on them the night they were recorded.
fn every_country(clock: &TestClock) -> (FixtureFetch, venti_domain::Timestamp) {
    use venti_server::clock::Clock;
    let written = clock.now() - Duration::hours(17);
    let fetch = recorded_files()
        .into_iter()
        .fold(FixtureFetch::new(), |fetch, (url, body)| {
            fetch.dated(url, body, written)
        });
    (fetch, written)
}

/// The recorded payloads of the airspace adapter, by the address each
/// answers. The provider's crate is not named here: the server's registry
/// is the one place that knows it.
fn recorded_files() -> Vec<(String, Vec<u8>)> {
    use venti_sources::testing::block_on;
    use venti_sources::{Fetch, PlanCtx};
    let registry = adapters::registry();
    let adapter = registry.airspace(&source(OPENAIP)).unwrap();
    let ctx = PlanCtx {
        now: adapters::recorded_at(),
        area: first_region(),
    };
    let recorded = adapters::recorded_payloads();
    adapter
        .plan(&ctx)
        .into_iter()
        .map(|request| {
            let body = block_on(recorded.get(&request)).unwrap().body;
            (request.url, body)
        })
        .collect()
}

#[tokio::test]
async fn a_run_that_misses_a_country_keeps_yesterdays_airspace() {
    let clock = TestClock::new();
    let store = Arc::new(MemoryStore::new());
    let router = serving(store.clone(), &clock, [&openaip_manifest()]);
    let (whole, written) = every_country(&clock);

    // Monday: every file arrives.
    let fetched_on_monday = adapters::recorded_at();
    assert!(
        collector(store.clone(), whole, &clock)
            .collect(&airspace_job())
            .await
    );
    let monday: AirspaceResponse = get(&router, &path(first_region())).await.json();
    assert_eq!(monday.layers[0].airspaces.len(), RECORDED);
    assert_eq!(monday.layers[0].countries, COUNTRIES);
    // The files are as old as their store says, not as their fetch.
    assert_eq!(monday.layers[0].file_date, Some(written));
    assert_eq!(monday.layers[0].fetched_at, fetched_on_monday);

    // Tuesday: Slovakia's file cannot be fetched. A set with a country
    // missing is worse than yesterday's whole one: the answer would go on
    // naming Slovakia among its countries.
    clock.advance(Duration::days(1));
    let all_but_the_last = || {
        let mut files = recorded_files();
        files.pop();
        files
            .into_iter()
            .fold(FixtureFetch::new(), |fetch, (url, body)| {
                fetch.with(url, body)
            })
    };
    let healthy = collector(store.clone(), all_but_the_last(), &clock)
        .collect(&airspace_job())
        .await;
    assert!(!healthy);

    let tuesday: AirspaceResponse = get(&router, &path(first_region())).await.json();
    assert_eq!(tuesday, monday, "the stored set is untouched");
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    let openaip = &health.sources[0];
    assert_eq!(openaip.last_success, Some(fetched_on_monday));
    assert_eq!(
        openaip.last_failure,
        Some(fetched_on_monday + Duration::days(1))
    );
    assert_eq!(openaip.airspaces, Some(192));
    assert_eq!(openaip.file_date, Some(written));
    // One missed day is not yet stale.
    assert!(!openaip.stale);
    assert_eq!(health.status, HealthStatus::Ok);

    // Wednesday fails too. A few hours on the source is stale, and the
    // server says so while it keeps serving Monday's airspace with
    // Monday's date.
    clock.advance(Duration::days(1));
    let healthy = collector(store.clone(), all_but_the_last(), &clock)
        .collect(&airspace_job())
        .await;
    assert!(!healthy);
    clock.advance(Duration::hours(7));
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert!(health.sources[0].stale);
    assert_eq!(health.status, HealthStatus::Degraded);
    let wednesday: AirspaceResponse = get(&router, &path(first_region())).await.json();
    assert_eq!(wednesday.layers[0].fetched_at, fetched_on_monday);

    // Thursday: every file arrives again, and the set is Thursday's.
    let (whole, _) = every_country(&clock);
    assert!(
        collector(store, whole, &clock)
            .collect(&airspace_job())
            .await
    );
    let thursday: AirspaceResponse = get(&router, &path(first_region())).await.json();
    assert!(thursday.layers[0].fetched_at > fetched_on_monday);
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert!(!health.sources[0].stale);
}

// --- The size of a whole region ---------------------------------------------

/// Measures the answer for the airspace of whole countries, when a folder
/// that holds openAIP's export files is named.
///
/// The recorded fixtures are a small part of each country. To measure
/// what a phone downloads for a whole region, fetch `<country>_asp.geojson`
/// for every country of the adapter's manifest into a folder and run
///
/// ```text
/// VENTI_OPENAIP_DIR=<folder> cargo test -p venti-server --test airspace whole -- --nocapture
/// ```
///
/// The numbers of 2026-11-19 are in `docs/api.md`.
#[tokio::test]
async fn whole_countries_are_measured_when_given() {
    let Some(folder) = std::env::var_os("VENTI_OPENAIP_DIR") else {
        return;
    };
    let folder = PathBuf::from(folder);
    let fetch = recorded_files()
        .into_iter()
        .fold(FixtureFetch::new(), |fetch, (url, _)| {
            let name = url.rsplit('/').next().unwrap().to_owned();
            let path = folder.join(&name);
            let body = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            fetch.with(url, body)
        });
    let clock = TestClock::new();
    let store = Arc::new(MemoryStore::new());
    let router = serving(store.clone(), &clock, [&openaip_manifest()]);
    // Every country whole, not only the first region's box.
    let everywhere = BoundingBox::new(35.0, 5.0, 56.0, 23.0).unwrap();
    let job = Job {
        area: everywhere,
        ..airspace_job()
    };
    assert!(collector(store, fetch, &clock).collect(&job).await);

    for (name, bbox) in [
        ("the eight countries whole", everywhere),
        ("the first region's box", first_region()),
    ] {
        let began = std::time::Instant::now();
        let plain = get(&router, &path(bbox)).await;
        let took = began.elapsed();
        let packed = get_with(&router, &path(bbox), &[(header::ACCEPT_ENCODING, "gzip")]).await;
        // More than one answer carries is a refusal, and worth knowing.
        assert_eq!(plain.status, StatusCode::OK, "{name}: {}", plain.text());
        let airspace: AirspaceResponse = plain.json();
        let records = &airspace.layers[0].airspaces;
        let corners: usize = records.iter().map(|a| a.geometry.exterior().len()).sum();
        println!(
            "{name}: {} airspaces, {corners} corners, {} bytes plain, {} bytes gzipped, \
             answered in {took:?} (unoptimised build)",
            records.len(),
            plain.body.len(),
            packed.body.len(),
        );
    }
}
