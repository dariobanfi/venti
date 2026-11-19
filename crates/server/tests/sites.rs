//! Tests of flying sites: the endpoint through the real router, and the
//! path from a provider's answer into the store.
//!
//! The endpoint is tested against what the development mode serves: the
//! sites OpenStreetMap has for the first region, recorded from an Overpass
//! instance and run through the real adapter. One answer is kept as a
//! golden file, which is also the example in `docs/api.md`.

use std::sync::Arc;

use axum::Router;
use axum::http::{StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ErrorCode, HealthResponse, HealthStatus, MAX_SITES, SitesRequest, SitesResponse,
};
use venti_domain::{BoundingBox, GeoPoint, Site, SiteUse, SourceId};
use venti_server::adapters::{self, Providers};
use venti_server::api::{Api, Cors};
use venti_server::clock::Clock;
use venti_server::ingest::{Collector, DirArchive};
use venti_server::plan::Job;
use venti_sources::testing::{FixtureFetch, assert_golden, block_on};
use venti_sources::{Fetch, Kind, Manifest, PlanCtx};
use venti_store::{MemoryStore, RunRecord, SiteDelivery, Store};

mod common;

use common::{TestClock, get, get_with, golden, recorded, settings};

// --- Helpers ----------------------------------------------------------------

const OSM: &str = "osm-free-flying";

fn source(name: &str) -> SourceId {
    SourceId::new(name).unwrap()
}

fn first_region() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn path(bbox: BoundingBox) -> String {
    SitesRequest { bbox }.path_and_query()
}

/// The valley of the apps' sample flight: the Tegernsee with the Wallberg,
/// and the Spitzingsee east of it.
const TEGERNSEE: &str = "/v1/sites?bbox=11.7,47.6,11.95,47.75";

/// A take-off at the given position.
fn takeoff(id: &str, lat: f64, lon: f64) -> Site {
    Site {
        id: id.parse().unwrap(),
        name: Some(id.to_uppercase()),
        position: GeoPoint::new(lat, lon).unwrap(),
        elevation_m: None,
        uses: vec![SiteUse::Takeoff],
        wind_directions: Vec::new(),
        paragliding: None,
        hang_gliding: None,
        official: None,
        discouraged: false,
        url: None,
    }
}

/// A healthy run of a source of sites at the time of the recording.
fn run(source_name: &str) -> RunRecord {
    RunRecord {
        source: source(source_name),
        started_at: adapters::recorded_at(),
        finished_at: adapters::recorded_at(),
        healthy: true,
        requests: 1,
        failed_requests: 0,
        skipped: 0,
        rejected: 0,
        stations: 0,
        observations: 0,
        newest_observation: None,
        airspaces: 0,
    }
}

fn whole_region() -> SiteDelivery {
    SiteDelivery {
        as_of: adapters::recorded_at() - Duration::hours(3),
        covers: vec![first_region()],
    }
}

/// The manifest of a second source of sites, under another licence.
fn club() -> Manifest {
    let text = r#"
        id = "club"
        provider = "Example Flying Club"
        kinds = ["sites"]
        countries = ["DE"]
        cadence_seconds = 86400
        auth = "none"

        [licence]
        name = "CC BY 4.0"
        commercial = true
        redistribution = true
        share_alike = false
        attribution = "Example Flying Club"
        link = "https://club.example"
        verified_on = "2026-11-19"
        verified_from = ["https://club.example/licence"]
    "#;
    Manifest::from_toml(text).unwrap()
}

fn osm_manifest() -> Manifest {
    let registry = adapters::registry();
    registry
        .manifest(&source(OSM), Kind::Sites)
        .unwrap()
        .clone()
}

/// An API over `store` that serves the sources `manifests` describe.
fn serving<'a>(
    store: Arc<MemoryStore>,
    clock: &TestClock,
    manifests: impl IntoIterator<Item = &'a Manifest>,
) -> Router {
    Api::new(store, Arc::new(clock.clone()), manifests, settings()).router(&Cors::Any)
}

// --- The endpoint over the recorded data ------------------------------------

#[tokio::test]
async fn sites_come_in_a_layer_with_their_source_their_dates_and_their_credit() {
    let answer = get(&recorded().await, &path(first_region())).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    // Sites change once a day at most.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=3600");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    let sites: SitesResponse = answer.json();

    assert_eq!(sites.layers.len(), 1);
    let layer = &sites.layers[0];
    assert_eq!(layer.source.as_str(), OSM);
    // Everything OpenStreetMap has for the first region.
    assert_eq!(layer.sites.len(), 502);
    assert!(layer.sites.windows(2).all(|pair| pair[0].id < pair[1].id));
    // Every record belongs to the layer's source, and to no other.
    assert!(layer.sites.iter().all(|s| s.id.source() == &layer.source));

    // How old the data is: when the server fetched it, and the date
    // OpenStreetMap's copy is of. The development mode's clock stands
    // before the moment the answer was recorded, and data is never newer
    // than its fetch.
    assert_eq!(layer.fetched_at, adapters::recorded_at());
    assert_eq!(layer.as_of, adapters::recorded_at());
    // Where the server's set is whole: the region it collects for.
    assert_eq!(layer.covers, [first_region()]);

    // What a client has to show: the credit, the licence, and that anyone
    // can correct the data.
    assert_eq!(sites.attributions.len(), 1);
    let credit = &sites.attributions[0];
    assert_eq!(credit.source, layer.source);
    assert_eq!(credit.licence, "ODbL 1.0");
    assert_eq!(credit.text, "© OpenStreetMap contributors");
    assert_eq!(credit.url, "https://www.openstreetmap.org/copyright");
    let notice = credit.notice.as_deref().unwrap();
    assert!(
        notice.contains("Open Database License") && notice.contains("fixthemap"),
        "{notice}"
    );
}

#[tokio::test]
async fn a_small_box_gets_what_lies_in_it() {
    let router = recorded().await;

    let answer = get(&router, TEGERNSEE).await;
    let sites: SitesResponse = answer.json();
    let found: Vec<(&str, Option<&str>)> = sites.layers[0]
        .sites
        .iter()
        .map(|site| (site.id.local(), site.name.as_deref()))
        .collect();
    assert_eq!(
        found,
        [
            // The two ramps on the Wallberg, which have no name.
            ("node/5964676196", None),
            ("node/5964676197", None),
            ("node/9908030941", None),
            ("way/1080986173", Some("Fluggelände Jägerkamp/Nagelspitz")),
            ("way/1080988126", Some("Landeplatz Jägerkamp/Nagelspitz")),
        ]
    );
    assert_golden(&golden("sites.json"), &sites);

    // A box without a site gets the layer all the same, without sites: it
    // still says how old the set is and where it is whole.
    let quiet = "/v1/sites?bbox=14.90,48.20,14.91,48.21";
    let nothing: SitesResponse = get(&router, quiet).await.json();
    assert_eq!(nothing.layers.len(), 1);
    assert!(nothing.layers[0].sites.is_empty());
    assert_eq!(nothing.layers[0].covers, [first_region()]);
    assert_eq!(nothing.attributions.len(), 1);

    // Outside the area the answer looks the same and says nothing about
    // the box: the client compares the box it asked for with `covers`.
    let nowhere: SitesResponse = get(&router, "/v1/sites?bbox=0,0,1,1").await.json();
    assert!(nowhere.layers[0].sites.is_empty());
    let asked = GeoPoint::new(0.5, 0.5).unwrap();
    assert!(
        !nowhere.layers[0]
            .covers
            .iter()
            .any(|area| area.contains(asked))
    );
}

#[tokio::test]
async fn an_unchanged_answer_is_not_sent_again_and_health_counts_the_sites() {
    let router = recorded().await;

    let first = get(&router, TEGERNSEE).await;
    let tag = first.header(header::ETAG).to_owned();
    let again = get_with(&router, TEGERNSEE, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.body.is_empty());

    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert_eq!(health.status, HealthStatus::Ok);
    let osm = health
        .sources
        .iter()
        .find(|entry| entry.source.as_str() == OSM)
        .unwrap();
    assert_eq!(osm.provider, "OpenStreetMap contributors");
    assert_eq!(osm.cadence_seconds, 86_400);
    assert_eq!(osm.sites, Some(502));
    assert_eq!(osm.sites_as_of, Some(adapters::recorded_at()));
    assert_eq!(osm.last_success, Some(adapters::recorded_at()));
    assert!(!osm.stale);
    assert_eq!(osm.airspaces, None);
    // The sources of everything else say nothing about sites.
    assert!(
        health
            .sources
            .iter()
            .filter(|entry| entry.source.as_str() != OSM)
            .all(|entry| entry.sites.is_none())
    );
}

#[tokio::test]
async fn a_bad_box_is_explained() {
    let router = recorded().await;

    let missing = get(&router, "/v1/sites").await;
    let message = missing.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    assert!(message.contains("bbox=11.6,47.55,12.0,47.85"), "{message}");

    let short = get(&router, "/v1/sites?bbox=11.6,47.55,12.0").await;
    short.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
}

// --- Stores other than the recorded one -------------------------------------

#[tokio::test]
async fn two_sources_are_two_layers_and_a_source_that_is_not_served_stays_inside() {
    let store = Arc::new(MemoryStore::new());
    let ours = [takeoff("osm-free-flying:node/1", 47.66, 11.80)];
    let theirs = [
        takeoff("club:wallberg", 47.66, 11.80),
        takeoff("club:brauneck", 47.66, 11.52),
    ];
    store
        .save_site_run(&run(OSM), &whole_region(), &ours)
        .await
        .unwrap();
    store
        .save_site_run(&run("club"), &whole_region(), &theirs)
        .await
        .unwrap();

    // The same take-off from two sources is two records in two layers:
    // nothing is merged, and each layer has its own credit.
    let both = serving(store.clone(), &TestClock::new(), [&osm_manifest(), &club()]);
    let sites: SitesResponse = get(&both, &path(first_region())).await.json();
    let layers: Vec<(&str, usize)> = sites
        .layers
        .iter()
        .map(|layer| (layer.source.as_str(), layer.sites.len()))
        .collect();
    assert_eq!(layers, [("club", 2), (OSM, 1)]);
    let credits: Vec<(&str, &str)> = sites
        .attributions
        .iter()
        .map(|a| (a.source.as_str(), a.licence.as_str()))
        .collect();
    assert_eq!(credits, [("club", "CC BY 4.0"), (OSM, "ODbL 1.0")]);

    // A server that no longer serves the club's sites does not hand them
    // out, though the store still holds them.
    let one = serving(store, &TestClock::new(), [&osm_manifest()]);
    let sites: SitesResponse = get(&one, &path(first_region())).await.json();
    assert_eq!(sites.layers.len(), 1);
    assert_eq!(sites.layers[0].source.as_str(), OSM);
    assert_eq!(sites.attributions.len(), 1);
}

#[tokio::test]
async fn a_box_with_too_many_sites_is_refused_and_not_cut() {
    let store = Arc::new(MemoryStore::new());
    // One more than an answer carries, in a row from west to east.
    let many: Vec<Site> = (0..=MAX_SITES)
        .map(|n| {
            takeoff(
                &format!("osm-free-flying:node/{n:05}"),
                47.0,
                n as f64 * 0.001,
            )
        })
        .collect();
    let everywhere = SiteDelivery {
        as_of: adapters::recorded_at(),
        covers: vec![BoundingBox::new(40.0, -1.0, 50.0, 20.0).unwrap()],
    };
    store
        .save_site_run(&run(OSM), &everywhere, &many)
        .await
        .unwrap();
    let router = serving(store, &TestClock::new(), [&osm_manifest()]);

    let all = get(&router, "/v1/sites?bbox=-1,46,6,48").await;
    let message = all.error(StatusCode::BAD_REQUEST, ErrorCode::BoxTooLarge);
    assert!(message.contains("smaller boxes"), "{message}");

    // Exactly as many as an answer carries is an answer, and a whole one.
    let most = get(&router, "/v1/sites?bbox=0,46,4.9995,48").await;
    let sites: SitesResponse = most.ok(StatusCode::OK);
    assert_eq!(sites.layers[0].sites.len(), MAX_SITES);
}

// --- From the provider's answer into the store ------------------------------

fn sites_job() -> Job {
    Job {
        source: source(OSM),
        kind: Kind::Sites,
        area: first_region(),
    }
}

fn collector(
    store: Arc<MemoryStore>,
    fetch: FixtureFetch,
    clock: &TestClock,
) -> Collector<MemoryStore, FixtureFetch, Option<DirArchive>> {
    Collector::new(
        adapters::registry_for(&Providers::recorded()).unwrap(),
        store,
        fetch,
        None,
        Arc::new(clock.clone()),
        None,
    )
}

/// The one address a run of the adapter asks, and the recorded answer to
/// it. The provider's crate is not named here: the server's registry is the
/// one place that knows it.
fn recorded_answer() -> (String, Vec<u8>) {
    let registry = adapters::registry_for(&Providers::recorded()).unwrap();
    let adapter = registry.sites(&source(OSM)).unwrap();
    let ctx = PlanCtx {
        now: adapters::recorded_at(),
        area: first_region(),
    };
    let request = adapter.plan(&ctx).remove(0);
    let body = block_on(adapters::recorded_payloads().get(&request))
        .expect("the answer is recorded")
        .body;
    (request.url, body)
}

#[tokio::test]
async fn a_run_that_fails_keeps_the_last_set_and_health_says_so() {
    let clock = TestClock::new();
    let store = Arc::new(MemoryStore::new());
    let (url, body) = recorded_answer();
    let router = serving(store.clone(), &clock, [&osm_manifest()]);

    // Before the first run the server holds no sites and says so: there is
    // no layer, and the source is stale.
    let sites: SitesResponse = get(&router, TEGERNSEE).await.json();
    assert!(sites.layers.is_empty());
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert_eq!(health.sources[0].sites, Some(0));
    assert!(health.sources[0].stale);

    let whole = FixtureFetch::new().with(url.clone(), body.clone());
    assert!(
        collector(store.clone(), whole, &clock)
            .collect(&sites_job())
            .await
    );
    let fetched_at = clock.now();
    let sites: SitesResponse = get(&router, &path(first_region())).await.json();
    assert_eq!(sites.layers[0].sites.len(), 502);

    // A day later the instance runs out of time and sends half an answer
    // with a remark; the day after it does not answer at all.
    clock.advance(Duration::days(1));
    let half = String::from_utf8(body).unwrap().replacen(
        "\"elements\": [",
        "\"remark\": \"runtime error: Query timed out\",\n  \"elements\": [",
        1,
    );
    let cut_short = FixtureFetch::new().with(url, half.into_bytes());
    assert!(
        !collector(store.clone(), cut_short, &clock)
            .collect(&sites_job())
            .await
    );
    clock.advance(Duration::days(1));
    assert!(
        !collector(store.clone(), FixtureFetch::new(), &clock)
            .collect(&sites_job())
            .await
    );

    // The set of the last whole run is still served, with that run's date.
    let sites: SitesResponse = get(&router, &path(first_region())).await.json();
    assert_eq!(sites.layers[0].sites.len(), 502);
    assert_eq!(sites.layers[0].fetched_at, fetched_at);
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    let osm = &health.sources[0];
    assert_eq!(osm.sites, Some(502));
    assert_eq!(osm.last_success, Some(fetched_at));
    assert_eq!(osm.last_failure, Some(clock.now()));
    // Two days without a whole run: not stale yet, by the rule for a
    // source that is asked once a day.
    assert!(!osm.stale);
    clock.advance(Duration::hours(7));
    let health: HealthResponse = get(&router, "/v1/health").await.json();
    assert!(health.sources[0].stale);
    assert_eq!(health.status, HealthStatus::Degraded);
    // The store kept what it had through all of it.
    let held = store.site_sets(&[source(OSM)]).await.unwrap();
    assert_eq!(held[0].sites, 502);
}
