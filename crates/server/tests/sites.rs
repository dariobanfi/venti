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
        website: None,
        description: None,
        fee: None,
        guest_guidelines: None,
        no_flight_time: None,
        outline: None,
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
    // An answer that may hold a layer of Venti's own, which changes
    // whenever a partner or a pilot writes, is reused for a minute.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    // The sources' sites alone change once a day at most: an hour. And an
    // answer that was asked for the crowdsourced layer: a minute.
    let router = recorded().await;
    for (layers, kept) in [
        (OSM, "public, max-age=3600"),
        ("default", "public, max-age=60"),
        ("venti-pilots", "public, max-age=60"),
        ("osm-free-flying,venti", "public, max-age=60"),
    ] {
        let only = get(
            &router,
            &format!("{}&layers={layers}", path(first_region())),
        )
        .await;
        assert_eq!(only.status, StatusCode::OK, "{layers}");
        assert_eq!(only.header(header::CACHE_CONTROL), kept, "{layers}");
        assert!(only.header(header::ETAG).starts_with("W/\""), "{layers}");
    }
    let sites: SitesResponse = answer.json();

    // OpenStreetMap's layer, and beside it the layer of the sites that
    // pilots made, which the tests further down are about.
    assert_eq!(sites.layers.len(), 2);
    assert_eq!(sites.layers[1].source.as_str(), "venti");
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
    assert_eq!(sites.attributions.len(), 2);
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

// --- The sites that pilots make -------------------------------------------------

mod of_pilots {
    use axum::http::Method;
    use venti_api_types::v1::ErrorResponse;
    use venti_api_types::v1::{
        Account as OwnAccount, AddSiteRequest, ChangeSiteRequest, ConfirmSiteRequest,
        RemoveSiteRequest, RestoreSiteRequest, SiteChangesResponse, SiteConfirmationsResponse,
        SiteDraft, SiteHistoryResponse, SiteResponse, SiteRevisionKind, SiteVia, SitesFromPilots,
    };
    use venti_domain::{SiteId, WindSector};
    use venti_server::{dev, pilot_sites};
    use venti_store::{SiteAct, SiteRevising, SiteWrite};

    use super::common::{ask, recorded_with_its_store, server, sign_up};
    use super::*;

    fn box_of_the_made_up_sites() -> String {
        format!("/v1/sites?bbox={}", dev::DEMO_SITES_BBOX)
    }

    fn site_path(id: &str) -> String {
        format!("/v1/sites/{id}")
    }

    /// A take-off a pilot might add, at a place of the first region where
    /// nothing stands.
    fn draft(name: &str, lat: f64, lon: f64) -> SiteDraft {
        SiteDraft {
            name: name.to_owned(),
            position: GeoPoint::new(lat, lon).unwrap(),
            elevation_m: Some(1540.0),
            uses: vec![SiteUse::Takeoff],
            wind_directions: vec![WindSector {
                from_deg: 135.0,
                to_deg: 225.0,
            }],
            paragliding: Some(true),
            hang_gliding: None,
            website: None,
            description: Some("Made up, for the documentation. No such place exists.".to_owned()),
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        }
    }

    #[tokio::test]
    async fn the_sites_pilots_made_are_a_layer_of_their_own_under_their_own_licence() {
        let router = recorded().await;
        let sites: SitesResponse = get(&router, &box_of_the_made_up_sites()).await.json();
        assert_golden(&golden("sites_venti.json"), &sites);

        // In this box the recorded data of OpenStreetMap has nothing: its
        // layer is there without sites, and the partners' beside it. The
        // crowdsourced layer is not in an answer that did not ask for it.
        assert_eq!(sites.layers.len(), 2);
        assert!(sites.layers[0].sites.is_empty());
        let layer = &sites.layers[1];
        assert_eq!(layer.source.as_str(), pilot_sites::SOURCE);
        // The two of the partners' layer, by identifier. Each record is
        // of this layer and no other.
        let ids: Vec<String> = layer.sites.iter().map(|site| site.id.to_string()).collect();
        assert_eq!(ids, [dev::DEMO_SITES[0], dev::DEMO_SITES[2]]);
        // Who made each and where it stands, in the same order.
        let standing: Vec<(String, u32)> = layer
            .standing
            .iter()
            .map(|standing| (standing.id.to_string(), standing.revision))
            .collect();
        assert_eq!(
            standing,
            [
                (dev::DEMO_SITES[0].to_owned(), 2),
                (dev::DEMO_SITES[2].to_owned(), 1),
            ]
        );
        // How many pilots say they took off there, and when last; left
        // out where nobody has.
        assert_eq!(layer.standing[0].confirmed.pilots, 2);
        assert!(layer.standing[0].confirmed.latest_day.is_some());
        assert!(layer.standing[1].confirmed.is_nobody());
        assert_eq!(
            layer.standing[0]
                .created_by
                .as_ref()
                .unwrap()
                .handle
                .as_str(),
            dev::DEMO_HANDLE
        );
        assert_eq!(
            layer.standing[0]
                .changed_by
                .as_ref()
                .unwrap()
                .handle
                .as_str(),
            dev::DEMO_HANDLE
        );
        // The layer says nowhere that it is whole.
        assert!(layer.covers.is_empty());

        // The licence is the one the server names in one place.
        let credit = &sites.attributions[1];
        assert_eq!(credit.source.as_str(), pilot_sites::SOURCE);
        let licence = pilot_sites::Layer::Partners.licence();
        assert_eq!(credit.licence, licence.name);
        assert_eq!(credit.licence, "CC0 1.0");
        assert_eq!(credit.url, licence.url);
        assert!(
            credit
                .notice
                .as_deref()
                .unwrap()
                .contains("a partner of Venti")
        );

        // An older client reads the answer as before: a layer is a source
        // with sites, and what it does not know it skips.
        let plain: serde_json::Value = get(&router, &box_of_the_made_up_sites()).await.json();
        assert!(plain["layers"][0].get("standing").is_none());
        assert!(plain["layers"][1]["sites"][0]["id"].is_string());

        // A box where no partner added a site has no such layer.
        let tegernsee: SitesResponse = get(&router, TEGERNSEE).await.json();
        assert_eq!(tegernsee.layers.len(), 1);

        // The crowdsourced layer, for whoever asks for it: the one site
        // of it that stands, under the same licence and with a notice of
        // its own.
        let asked = format!("{}&layers=venti-pilots", box_of_the_made_up_sites());
        let crowd: SitesResponse = get(&router, &asked).await.json();
        assert_golden(&golden("sites_venti_pilots.json"), &crowd);
        assert_eq!(crowd.layers.len(), 1);
        assert_eq!(crowd.layers[0].source.as_str(), pilot_sites::PILOTS_SOURCE);
        let ids: Vec<String> = crowd.layers[0]
            .sites
            .iter()
            .map(|site| site.id.to_string())
            .collect();
        assert_eq!(ids, [dev::DEMO_SITES[1]]);
        assert_eq!(crowd.layers[0].standing[0].confirmed.pilots, 1);
        let credit = &crowd.attributions[0];
        assert_eq!(credit.licence, "CC0 1.0");
        assert!(credit.notice.as_deref().unwrap().contains("not official"));
        // Every layer by name; the order of the names does not matter,
        // and one named twice is there once.
        let all = format!(
            "{}&layers=venti-pilots,venti,osm-free-flying,venti",
            box_of_the_made_up_sites()
        );
        let all: SitesResponse = get(&router, &all).await.json();
        let names: Vec<&str> = all.layers.iter().map(|l| l.source.as_str()).collect();
        assert_eq!(names, ["osm-free-flying", "venti", "venti-pilots"]);
        // `default` stands for what an answer holds without the list, for
        // an app that wants the crowdsourced layer as well.
        let with = format!("{}&layers=default,venti-pilots", box_of_the_made_up_sites());
        let with: SitesResponse = get(&router, &with).await.json();
        let names: Vec<&str> = with.layers.iter().map(|l| l.source.as_str()).collect();
        assert_eq!(names, ["osm-free-flying", "venti", "venti-pilots"]);
        // A layer the server does not have is refused, with those it has.
        for wrong in ["nothing", "", "venti,,venti-pilots"] {
            let asked = format!("{}&layers={wrong}", box_of_the_made_up_sites());
            let said = get(&router, &asked)
                .await
                .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidLayers);
            assert!(
                said.contains("osm-free-flying, venti, venti-pilots"),
                "{said}"
            );
        }
    }

    #[tokio::test]
    async fn a_site_has_an_address_and_says_what_the_address_promises() {
        let router = recorded().await;

        // A site that pilots made: the address is for good.
        let answer = get(&router, &site_path(dev::DEMO_SITES[0])).await;
        assert_eq!(answer.status, StatusCode::OK);
        let site: SiteResponse = answer.json();
        assert_golden(&golden("site.json"), &site);
        assert!(site.lasting);
        assert_eq!(site.site.name.as_deref(), Some("Musterkogel West"));
        assert_eq!(site.standing.as_ref().unwrap().revision, 2);
        assert_eq!(site.attributions[0].licence, "CC0 1.0");

        // A source's site: the record of the set the server holds now.
        let osm = "osm-free-flying:way/1080986173";
        let site: SiteResponse = get(&router, &site_path(osm)).await.json();
        assert_golden(&golden("site_of_a_source.json"), &site);
        assert!(!site.lasting);
        assert!(site.standing.is_none());
        assert_eq!(site.attributions[0].licence, "ODbL 1.0");

        // A removed site answers that it is gone; no site, that there is
        // none; and so does what is no identifier at all.
        get(&router, &site_path(dev::DEMO_SITES[3]))
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
        for missing in [
            "venti:0000000000000000000000s009",
            // A site is found under its own layer's name only.
            "venti:0000000000000000000000s002",
            "venti-pilots:0000000000000000000000s001",
            "venti:short",
            "osm-free-flying:node/1",
            "club:1",
            "nothing",
        ] {
            get(&router, &site_path(missing))
                .await
                .error(StatusCode::NOT_FOUND, ErrorCode::SiteNotFound);
        }
    }

    #[tokio::test]
    async fn a_site_keeps_every_revision_and_the_layer_says_what_changed_since() {
        let router = recorded().await;

        // The second site: added, changed for the worse, put back.
        let path = format!("{}/history", site_path(dev::DEMO_SITES[1]));
        let history: SiteHistoryResponse = get(&router, &path).await.json();
        assert_golden(&golden("site_history.json"), &history);
        assert_eq!(
            (history.revision, history.removed, history.more),
            (3, false, false)
        );
        let kinds: Vec<(u32, SiteRevisionKind, Option<u32>)> = history
            .revisions
            .iter()
            .map(|revision| (revision.number, revision.kind, revision.restores))
            .collect();
        assert_eq!(
            kinds,
            [
                (1, SiteRevisionKind::Created, None),
                (2, SiteRevisionKind::Changed, None),
                (3, SiteRevisionKind::Restored, Some(1)),
            ]
        );
        assert!(history.revisions.iter().all(|r| r.via == SiteVia::Session));
        assert_eq!(history.revisions[1].site.elevation_m, Some(1204.0));
        assert_eq!(history.revisions[2].site, history.revisions[0].site);
        // Page by page.
        let page: SiteHistoryResponse = get(&router, &format!("{path}?limit=2")).await.json();
        assert_eq!((page.revisions.len(), page.more), (2, true));
        let rest: SiteHistoryResponse = get(&router, &format!("{path}?after=2")).await.json();
        assert_eq!((rest.revisions[0].number, rest.more), (3, false));
        // A removed site keeps its history.
        let removed = format!("{}/history", site_path(dev::DEMO_SITES[3]));
        let history: SiteHistoryResponse = get(&router, &removed).await.json();
        assert!(history.removed);
        assert_eq!(history.revisions[1].kind, SiteRevisionKind::Removed);

        // What changed since the beginning, a layer at a time: every
        // site once, as it is now, and the removed one as removed. The
        // partners' layer is the one a request that names none gets.
        let changes: SiteChangesResponse = get(&router, "/v1/sites/changes").await.json();
        assert_golden(&golden("site_changes.json"), &changes);
        let named: SiteChangesResponse = get(&router, "/v1/sites/changes?layer=venti").await.json();
        assert_eq!(named, changes);
        assert_eq!((changes.sites.len(), changes.removed.len()), (2, 0));
        // The two layers share one count of changes: eight revisions and
        // three confirmations, of which the partners' latest is the
        // tenth.
        assert_eq!(
            (changes.cursor.as_str(), changes.more, changes.reset),
            ("10", false, false)
        );
        let pilots = "/v1/sites/changes?layer=venti-pilots";
        let crowd: SiteChangesResponse = get(&router, pilots).await.json();
        assert_eq!(crowd.sites.len(), 1);
        assert_eq!(
            crowd.removed,
            [dev::DEMO_SITES[3].parse::<SiteId>().unwrap()]
        );
        assert_eq!(crowd.cursor, "11");
        assert_eq!(crowd.attributions[0].source.as_str(), "venti-pilots");
        // And named, for a list of what was removed lately: the name it
        // last had, who removed it and when.
        assert_golden(&golden("site_changes_pilots.json"), &crowd);
        let [removal] = crowd.removals.as_slice() else {
            panic!("one site of the layer is removed: {:?}", crowd.removals);
        };
        assert_eq!(removal.id, crowd.removed[0]);
        assert_eq!(removal.name.as_deref(), Some("Musterkogel"));
        assert_eq!(
            removal.by.as_ref().map(|by| by.handle.as_str()),
            Some("jonas")
        );
        // Since then: nothing, and the cursor stays.
        let since: SiteChangesResponse = get(&router, &format!("{pilots}&since=11")).await.json();
        assert!(since.sites.is_empty() && since.removed.is_empty());
        assert_eq!(since.cursor, "11");
        // In pages.
        let page: SiteChangesResponse = get(&router, &format!("{pilots}&limit=1")).await.json();
        assert_eq!(
            (page.sites.len() + page.removed.len(), page.more),
            (1, true)
        );
        let next = format!("{pilots}&since={}&limit=1", page.cursor);
        let page: SiteChangesResponse = get(&router, &next).await.json();
        assert_eq!(
            (page.sites.len() + page.removed.len(), page.more),
            (1, false)
        );
        // A cursor this server never gave out starts again, and says so.
        let reset: SiteChangesResponse = get(&router, "/v1/sites/changes?since=99").await.json();
        assert!(reset.reset);
        assert_eq!(reset.sites.len(), 2);
        get(&router, "/v1/sites/changes?since=x")
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
        get(&router, "/v1/sites/changes?layer=osm-free-flying")
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidLayers);
    }

    #[tokio::test]
    async fn a_partner_adds_corrects_undoes_and_removes_a_site_and_reads_each_step_back() {
        let (router, store) = recorded_with_its_store().await;
        // The demo account is this mode's partner; `petra` is any pilot.
        let token = dev::DEMO_TOKEN;
        let other = sign_up(&router, "petra").await.token;

        // Nobody adds a site without an account.
        let request = AddSiteRequest {
            site: draft("Beispielkopf Süd", 47.4, 11.2),
            comment: Some("From where I stood on 14 November.".to_owned()),
        };
        assert_golden(&golden("site_add_request.json"), &request);
        ask(Method::POST, "/v1/sites")
            .json(&request)
            .send(&router)
            .await
            .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

        // Added: the first revision, by the pilot, under a new identifier.
        let added: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(token)
            .json(&request)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        let id = added.site.id.clone();
        assert_eq!(id.source().as_str(), "venti");
        assert_eq!(id.local().len(), 26);
        let standing = added.standing.as_ref().unwrap();
        assert_eq!(standing.revision, 1);
        assert_eq!(
            standing.created_by.as_ref().unwrap().handle.as_str(),
            "demo"
        );
        assert!(added.lasting);
        // What a pilot does not say is not in the record.
        assert_eq!((added.site.official, added.site.discouraged), (None, false));
        assert_eq!(added.site.url, None);
        let read: SiteResponse = get(&router, &site_path(&id.to_string())).await.json();
        assert_eq!(read, added);

        // The same place again, by anyone: refused, naming the site that
        // stands there.
        let again = AddSiteRequest {
            site: draft("Beispielkopf", 47.40005, 11.2),
            comment: None,
        };
        let refused = ask(Method::POST, "/v1/sites")
            .token(&other)
            .json(&again)
            .send(&router)
            .await;
        let message = refused.error(StatusCode::CONFLICT, ErrorCode::SiteTooClose);
        assert!(message.contains(&id.to_string()), "{message}");
        // The error names it for a program too: its identifier and the
        // name it has now.
        let error: ErrorResponse = refused.json();
        let named = error.error.site.expect("the other site");
        assert_eq!(named.id, id);
        assert_eq!(named.name.as_deref(), Some("Beispielkopf Süd"));
        // The example of the documentation: a few metres from the first
        // of the made-up sites.
        let beside = AddSiteRequest {
            site: draft("Musterkogel", 47.30005, 10.9),
            comment: None,
        };
        let refused = ask(Method::POST, "/v1/sites")
            .token(&other)
            .json(&beside)
            .send(&router)
            .await;
        refused.error(StatusCode::CONFLICT, ErrorCode::SiteTooClose);
        let error: ErrorResponse = refused.json();
        assert_golden(&golden("site_too_close.json"), &error);
        assert_eq!(
            error.error.site.map(|site| site.id.to_string()).as_deref(),
            Some(dev::DEMO_SITES[0])
        );
        // No other error names a site.
        let other_error: serde_json::Value = get(&router, &site_path("venti:x")).await.json();
        assert!(other_error["error"].get("site").is_none(), "{other_error}");

        // What cannot be kept is refused, with the field's name.
        let mut nameless = draft(" \u{200b} ", 47.5, 11.3);
        for (what, code) in [
            (nameless.clone(), ErrorCode::InvalidSite),
            (
                SiteDraft {
                    uses: Vec::new(),
                    ..draft("A", 47.5, 11.3)
                },
                ErrorCode::InvalidSite,
            ),
            (
                SiteDraft {
                    website: Some("ftp://example.com".to_owned()),
                    ..draft("A", 47.5, 11.3)
                },
                ErrorCode::InvalidSite,
            ),
            (
                SiteDraft {
                    elevation_m: Some(12000.0),
                    ..draft("A", 47.5, 11.3)
                },
                ErrorCode::InvalidSite,
            ),
            (
                SiteDraft {
                    description: Some("x".repeat(1001)),
                    ..draft("A", 47.5, 11.3)
                },
                ErrorCode::InvalidSite,
            ),
            // On Earth, and in no region this server serves.
            (draft("A", 0.5, 0.5), ErrorCode::SiteOutOfArea),
        ] {
            let request = AddSiteRequest {
                site: what,
                comment: None,
            };
            ask(Method::POST, "/v1/sites")
                .token(token)
                .json(&request)
                .send(&router)
                .await
                .error(StatusCode::BAD_REQUEST, code);
        }
        nameless.name = "A".to_owned();
        let long = AddSiteRequest {
            site: nameless,
            comment: Some("x".repeat(201)),
        };
        ask(Method::POST, "/v1/sites")
            .token(token)
            .json(&long)
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidSite);

        // Corrected: a new revision, made from the one that was read. A
        // partner's site is corrected by a partner and by nobody else.
        let first = dev::DEMO_SITES[0];
        let before: SiteResponse = get(&router, &site_path(first)).await.json();
        let change = ChangeSiteRequest {
            site: SiteDraft {
                elevation_m: Some(1871.0),
                ..SiteDraft::of(&before.site)
            },
            revision: 2,
            comment: Some("Height from my own GPS.".to_owned()),
        };
        assert_golden(&golden("site_change_request.json"), &change);
        let refused = ask(Method::PUT, &site_path(first))
            .token(&other)
            .json(&change)
            .send(&router)
            .await;
        let said = refused.error(StatusCode::FORBIDDEN, ErrorCode::SitesForPartners);
        assert!(said.contains("reports it"), "{said}");
        let error: ErrorResponse = refused.json();
        assert_golden(&golden("error_sites_for_partners.json"), &error);
        let changed: SiteResponse = ask(Method::PUT, &site_path(first))
            .token(token)
            .json(&change)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert_golden(&golden("site_written.json"), &changed);
        let standing = changed.standing.as_ref().unwrap();
        assert_eq!(standing.revision, 3);
        assert_eq!(
            standing.created_by.as_ref().unwrap().handle.as_str(),
            "demo"
        );
        assert_eq!(
            standing.changed_by.as_ref().unwrap().handle.as_str(),
            "demo"
        );
        assert_eq!(changed.site.elevation_m, Some(1871.0));
        // Made from a revision the site no longer stands at: refused.
        let stale = ChangeSiteRequest {
            site: SiteDraft {
                elevation_m: Some(1.0),
                ..SiteDraft::of(&before.site)
            },
            ..change.clone()
        };
        ask(Method::PUT, &site_path(first))
            .token(token)
            .json(&stale)
            .send(&router)
            .await
            .error(StatusCode::CONFLICT, ErrorCode::SiteChanged);
        // A source's record is corrected at its source.
        ask(Method::PUT, &site_path("osm-free-flying:way/1080986173"))
            .token(token)
            .json(&change)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesClosed);

        // Undone: a revision that restores the one before, and names it.
        let undo = RestoreSiteRequest {
            restore: 2,
            comment: Some("The height was right before.".to_owned()),
        };
        assert_golden(&golden("site_restore_request.json"), &undo);
        let revisions = format!("{}/revisions", site_path(first));
        ask(Method::POST, &revisions)
            .token(&other)
            .json(&undo)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesForPartners);
        let restored: SiteResponse = ask(Method::POST, &revisions)
            .token(token)
            .json(&undo)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert_eq!(restored.standing.as_ref().unwrap().revision, 4);
        assert_eq!(restored.site, before.site);
        let history: SiteHistoryResponse = get(&router, &format!("{}/history", site_path(first)))
            .await
            .json();
        assert_eq!(history.revisions.len(), 4);
        assert_eq!(history.revisions[3].restores, Some(2));
        assert_eq!(history.revisions[2].site.elevation_m, Some(1871.0));
        let none = RestoreSiteRequest {
            restore: 9,
            comment: None,
        };
        ask(Method::POST, &revisions)
            .token(token)
            .json(&none)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::SiteRevisionNotFound);

        // Removed: gone at its address and from the map, its history
        // kept, and listed as removed for whoever follows the changes.
        let third = dev::DEMO_SITES[2];
        let removal = RemoveSiteRequest {
            comment: Some("The field is built on.".to_owned()),
        };
        ask(Method::DELETE, &site_path(third))
            .token(&other)
            .json(&removal)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesForPartners);
        let answer = ask(Method::DELETE, &site_path(third))
            .token(token)
            .json(&removal)
            .send(&router)
            .await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
        get(&router, &site_path(third))
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
        let sites: SitesResponse = get(&router, &box_of_the_made_up_sites()).await.json();
        assert_eq!(sites.layers[1].sites.len(), 1);
        let changes: SiteChangesResponse = get(&router, "/v1/sites/changes?since=11").await.json();
        assert!(changes.removed.contains(&third.parse().unwrap()));
        assert!(changes.sites.iter().any(|record| record.site.id == id));
        // Removing it again, and changing it, find nothing to change.
        ask(Method::DELETE, &site_path(third))
            .token(token)
            .send(&router)
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
        // A partner brings it back.
        let back = RestoreSiteRequest {
            restore: 1,
            comment: None,
        };
        let third_revisions = format!("{}/revisions", site_path(third));
        let restored: SiteResponse = ask(Method::POST, &third_revisions)
            .token(token)
            .json(&back)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert_eq!(restored.standing.as_ref().unwrap().revision, 3);
        get(&router, &site_path(third))
            .await
            .ok::<SiteResponse>(StatusCode::OK);

        // Taken down by whoever runs the server: gone with its history,
        // and no pilot brings it back.
        let take_down = SiteRevising {
            id: third.parse().unwrap(),
            author_id: None,
            at: adapters::recorded_at(),
            via: venti_store::SiteVia::Operator,
            act: SiteAct::Remove { take_down: true },
            comment: None,
            expects: None,
        };
        let taken = store
            .revise_pilot_site(&take_down, pilot_sites::APART_M)
            .await
            .unwrap();
        assert!(matches!(taken, SiteWrite::Written(_)));
        get(&router, &format!("{}/history", site_path(third)))
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
        // Among the removed it is named by its identifier and the moment
        // alone: not by what it was called, and by nobody.
        let changes: SiteChangesResponse = get(&router, "/v1/sites/changes").await.json();
        let removal = changes
            .removals
            .iter()
            .find(|removal| removal.id.to_string() == third)
            .unwrap();
        assert_eq!((removal.name.as_deref(), removal.by.as_ref()), (None, None));
        assert_eq!(removal.at, adapters::recorded_at());
        ask(Method::POST, &third_revisions)
            .token(token)
            .json(&back)
            .send(&router)
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
    }

    #[tokio::test]
    async fn a_pilot_writes_to_the_crowdsourced_layer_and_a_partner_to_both() {
        let (router, _) = recorded_with_its_store().await;
        let partner = dev::DEMO_TOKEN;
        let petra = sign_up(&router, "petra").await.token;
        let quirin = sign_up(&router, "quirin").await.token;

        // The account says which it is, and only when it is a partner.
        let own: serde_json::Value = ask(Method::GET, "/v1/account")
            .token(partner)
            .send(&router)
            .await
            .json();
        assert_eq!(own["account"]["edits_sites"], true);
        let own: serde_json::Value = ask(Method::GET, "/v1/account")
            .token(&petra)
            .send(&router)
            .await
            .json();
        assert!(own["account"].get("edits_sites").is_none(), "{own}");
        let read: OwnAccount = serde_json::from_value(own["account"].clone()).unwrap();
        assert!(!read.edits_sites);

        // A pilot's site goes to the crowdsourced layer, whatever else
        // the request says: who adds it decides.
        let request = AddSiteRequest {
            site: draft("Beispielkopf Süd", 47.4, 11.2),
            comment: None,
        };
        let added: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(&petra)
            .json(&request)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        let id = added.site.id.clone();
        assert_eq!(id.source().as_str(), "venti-pilots");
        assert_eq!(id.local().len(), 26);
        assert!(added.lasting);
        assert_eq!(added.attributions[0].source.as_str(), "venti-pilots");
        // It is at its address, in its layer for whoever asks for that,
        // and not in an answer that did not.
        let read: SiteResponse = get(&router, &site_path(&id.to_string())).await.json();
        assert_eq!(read, added);
        let around = "/v1/sites?bbox=11.1,47.35,11.3,47.45";
        let default: SitesResponse = get(&router, around).await.json();
        assert!(default.layers.iter().all(|l| l.sites.is_empty()));
        let asked: SitesResponse = get(&router, &format!("{around}&layers=venti-pilots"))
            .await
            .json();
        assert_eq!(asked.layers[0].sites[0].id, id);

        // Any other pilot corrects it, and so does a partner: it stays a
        // site of the crowdsourced layer.
        let mut revision = 1;
        for (token, handle, height) in [
            (&quirin, "quirin", 1550.0),
            (&partner.to_owned(), "demo", 1560.0),
        ] {
            let change = ChangeSiteRequest {
                site: SiteDraft {
                    elevation_m: Some(height),
                    ..SiteDraft::of(&added.site)
                },
                revision,
                comment: None,
            };
            let changed: SiteResponse = ask(Method::PUT, &site_path(&id.to_string()))
                .token(token)
                .json(&change)
                .send(&router)
                .await
                .ok(StatusCode::OK);
            revision += 1;
            let standing = changed.standing.unwrap();
            assert_eq!(standing.revision, revision);
            assert_eq!(standing.changed_by.unwrap().handle.as_str(), handle);
            assert_eq!(changed.site.id, id);
        }

        // A partner's site stands in a pilot's way, and a pilot's does
        // not stand in a partner's: the official one is put there, and
        // both stand.
        let beside = AddSiteRequest {
            site: draft("Beispielkopf", 47.40005, 11.2),
            comment: None,
        };
        ask(Method::POST, "/v1/sites")
            .token(&quirin)
            .json(&beside)
            .send(&router)
            .await
            .error(StatusCode::CONFLICT, ErrorCode::SiteTooClose);
        let official: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(partner)
            .json(&beside)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        assert_eq!(official.site.id.source().as_str(), "venti");
        let both: SitesResponse = get(&router, &format!("{around}&layers=venti,venti-pilots"))
            .await
            .json();
        assert_eq!(both.layers.len(), 2);
        // No pilot changes the partner's one.
        ask(Method::DELETE, &site_path(&official.site.id.to_string()))
            .token(&petra)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesForPartners);
    }

    #[tokio::test]
    async fn a_place_from_pilots_is_refused_where_a_partners_site_stands() {
        let (router, _) = recorded_with_its_store().await;
        let partner = dev::DEMO_TOKEN;
        let quirin = sign_up(&router, "quirin").await.token;

        // A partner's site, with nothing of either layer near it.
        let official: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(partner)
            .json(&AddSiteRequest {
                site: draft("Beispielgrat", 47.42, 11.2),
                comment: None,
            })
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        let official_id = official.site.id.clone();
        assert_eq!(official_id.source().as_str(), "venti");
        let refused_by_the_official = |refused: &ErrorResponse| {
            let named = refused.error.site.clone().expect("the site in the way");
            assert_eq!(named.id, official_id);
            assert_eq!(named.name.as_deref(), Some("Beispielgrat"));
        };

        // A new place from pilots a few metres from it: refused across
        // the layers, with the partner's site named to be opened.
        let refused = ask(Method::POST, "/v1/sites")
            .token(&quirin)
            .json(&AddSiteRequest {
                site: draft("Beispielgrat, by a pilot", 47.42003, 11.2),
                comment: None,
            })
            .send(&router)
            .await;
        refused.error(StatusCode::CONFLICT, ErrorCode::SiteTooClose);
        refused_by_the_official(&refused.json());

        // A place from pilots that stands elsewhere, moved there by a
        // correction: refused the same, and it stays where it stood.
        let elsewhere: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(&quirin)
            .json(&AddSiteRequest {
                site: draft("Beispielscharte", 47.43, 11.2),
                comment: None,
            })
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        let place = elsewhere.site.id.clone();
        assert_eq!(place.source().as_str(), "venti-pilots");
        let moved = ChangeSiteRequest {
            site: SiteDraft {
                position: GeoPoint::new(47.42003, 11.2).unwrap(),
                ..SiteDraft::of(&elsewhere.site)
            },
            revision: 1,
            comment: None,
        };
        let refused = ask(Method::PUT, &site_path(&place.to_string()))
            .token(&quirin)
            .json(&moved)
            .send(&router)
            .await;
        refused.error(StatusCode::CONFLICT, ErrorCode::SiteTooClose);
        refused_by_the_official(&refused.json());
        let stands: SiteResponse = get(&router, &site_path(&place.to_string())).await.json();
        assert_eq!(stands.standing.unwrap().revision, 1);
        assert_eq!(stands.site.position, elsewhere.site.position);

        // The other way round nothing is refused: a partner's site is
        // put where a place from pilots stands, and both stand.
        let over: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(partner)
            .json(&AddSiteRequest {
                site: draft("Beispielscharte, official", 47.43002, 11.2),
                comment: None,
            })
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        assert_eq!(over.site.id.source().as_str(), "venti");
        let both: SitesResponse = get(
            &router,
            "/v1/sites?bbox=11.19,47.425,11.21,47.435&layers=venti,venti-pilots",
        )
        .await
        .json();
        let standing: usize = both.layers.iter().map(|layer| layer.sites.len()).sum();
        assert_eq!(standing, 2);
    }

    #[tokio::test]
    async fn a_pilot_says_they_took_off_at_a_site_and_takes_it_back() {
        let (router, store) = recorded_with_its_store().await;
        let first = dev::DEMO_SITES[0];
        let confirmation = format!("{}/confirmation", site_path(first));
        let confirmations = format!("{}/confirmations", site_path(first));
        let today = adapters::recorded_at().date_naive();

        // Who says so of the first made-up site: read by anyone.
        let said: SiteConfirmationsResponse = get(&router, &confirmations).await.json();
        assert_golden(&golden("site_confirmations.json"), &said);
        assert_eq!(said.confirmed.pilots, 2);
        let who: Vec<&str> = said
            .confirmations
            .iter()
            .map(|c| c.by.handle.as_str())
            .collect();
        assert_eq!(who, ["mara", "jonas"]);
        assert_eq!(said.confirmed.latest_day, Some(said.confirmations[0].day));
        assert!(said.confirmations[0].note.is_some());

        // Nobody confirms without an account.
        let request = ConfirmSiteRequest {
            day: today,
            note: Some("  South-west,\n easy launch. ".to_owned()),
            flight: None,
        };
        assert_golden(&golden("site_confirm_request.json"), &request);
        ask(Method::PUT, &confirmation)
            .json(&request)
            .send(&router)
            .await
            .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

        // Any pilot confirms a partner's site: the site says one more,
        // at the revision it stood at.
        let petra = sign_up(&router, "petra").await.token;
        let confirmed: SiteResponse = ask(Method::PUT, &confirmation)
            .token(&petra)
            .json(&request)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        let standing = confirmed.standing.unwrap();
        assert_eq!((standing.confirmed.pilots, standing.revision), (3, 2));
        assert_eq!(standing.confirmed.latest_day, Some(today));
        let said: SiteConfirmationsResponse = get(&router, &confirmations).await.json();
        assert_eq!(said.confirmations[0].by.handle.as_str(), "petra");
        // The note is cleaned to one line, as a revision's comment is.
        assert_eq!(
            said.confirmations[0].note.as_deref(),
            Some("South-west, easy launch.")
        );
        assert!(!said.confirmations[0].with_flight);
        // Whoever follows the changes reads the site again.
        let changes: SiteChangesResponse = get(&router, "/v1/sites/changes?since=11").await.json();
        assert_eq!(changes.sites.len(), 1);
        assert_eq!(changes.sites[0].standing.confirmed.pilots, 3);
        assert_eq!(changes.cursor, "12");

        // Saying it again moves the day and is still one.
        let earlier = ConfirmSiteRequest {
            day: today - chrono::Duration::days(3),
            note: None,
            flight: None,
        };
        let again: SiteResponse = ask(Method::PUT, &confirmation)
            .token(&petra)
            .json(&earlier)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert_eq!(again.standing.unwrap().confirmed.pilots, 3);

        // What cannot be kept: a day to come or long gone, a note that
        // is too long, a flight that is not the pilot's or did not take
        // off there.
        let flights: serde_json::Value = ask(Method::GET, "/v1/flights")
            .token(dev::DEMO_TOKEN)
            .send(&router)
            .await
            .json();
        let demos_flight = flights["flights"][0]["id"].as_str().unwrap().to_owned();
        for wrong in [
            ConfirmSiteRequest {
                day: today + chrono::Duration::days(2),
                ..earlier.clone()
            },
            ConfirmSiteRequest {
                day: today - chrono::Duration::days(91),
                ..earlier.clone()
            },
            ConfirmSiteRequest {
                note: Some("x".repeat(201)),
                ..earlier.clone()
            },
            ConfirmSiteRequest {
                flight: Some(demos_flight.clone()),
                ..earlier.clone()
            },
            ConfirmSiteRequest {
                flight: Some("nothing".to_owned()),
                ..earlier.clone()
            },
        ] {
            ask(Method::PUT, &confirmation)
                .token(&petra)
                .json(&wrong)
                .send(&router)
                .await
                .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidConfirmation);
        }
        // The pilot's own flight, which took off somewhere else.
        let far = ConfirmSiteRequest {
            flight: Some(demos_flight.clone()),
            ..earlier.clone()
        };
        let said = ask(Method::PUT, &confirmation)
            .token(dev::DEMO_TOKEN)
            .json(&far)
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidConfirmation);
        assert!(said.contains("500 m"), "{said}");
        // And one that took off there: a site where that flight began,
        // which the partner adds, and confirms with the flight.
        let takeoff = &flights["flights"][0]["takeoff"]["position"];
        let (lat, lon) = (
            takeoff["lat"].as_f64().unwrap(),
            takeoff["lon"].as_f64().unwrap(),
        );
        let there = AddSiteRequest {
            site: draft("Beispielstart", lat + 0.001, lon),
            comment: None,
        };
        let added: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(dev::DEMO_TOKEN)
            .json(&there)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        let its = format!("{}/confirmation", site_path(&added.site.id.to_string()));
        let with: SiteResponse = ask(Method::PUT, &its)
            .token(dev::DEMO_TOKEN)
            .json(&far)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert_eq!(with.standing.unwrap().confirmed.pilots, 1);
        let said: SiteConfirmationsResponse = get(&router, &format!("{its}s")).await.json();
        assert!(said.confirmations[0].with_flight);
        // Which flight is not said to anyone.
        assert!(
            !get(&router, &format!("{its}s"))
                .await
                .text()
                .contains(&demos_flight)
        );

        // Taken back, by its pilot, and again without complaint.
        for _ in 0..2 {
            let answer = ask(Method::DELETE, &confirmation)
                .token(&petra)
                .send(&router)
                .await;
            assert_eq!(answer.status, StatusCode::NO_CONTENT);
        }
        let said: SiteConfirmationsResponse = get(&router, &confirmations).await.json();
        assert_eq!(said.confirmed.pilots, 2);

        // A site of a source, a site that is not there and one that is
        // gone take none; reading the caller's own is no request.
        let gone = format!("{}/confirmation", site_path(dev::DEMO_SITES[3]));
        ask(Method::PUT, &gone)
            .token(&petra)
            .json(&request)
            .send(&router)
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
        get(&router, &format!("{gone}s"))
            .await
            .error(StatusCode::GONE, ErrorCode::SiteGone);
        let nowhere = "/v1/sites/venti:0000000000000000000000s009/confirmation";
        ask(Method::PUT, nowhere)
            .token(&petra)
            .json(&request)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::SiteNotFound);
        get(&router, &confirmation)
            .await
            .error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);

        // A confirmation goes with its account; the count follows.
        let mara = store.account("mara").await.unwrap().unwrap();
        store.delete_account(&mara.id).await.unwrap();
        let said: SiteConfirmationsResponse = get(&router, &confirmations).await.json();
        assert_eq!(said.confirmed.pilots, 1);
        assert_eq!(said.confirmations[0].by.handle.as_str(), "jonas");
    }

    #[tokio::test]
    async fn a_server_takes_sites_from_pilots_only_when_it_is_told_to() {
        // A server as it is configured unless told otherwise: it serves
        // the first region and takes no site.
        let clock = TestClock::new();
        let mut closed = settings();
        closed.sites.accept = false;
        let (router, store) = server(closed, &clock);
        let token = sign_up(&router, "petra").await.token;
        let request = AddSiteRequest {
            site: draft("Beispielkopf Süd", 47.4, 11.2),
            comment: None,
        };
        ask(Method::POST, "/v1/sites")
            .token(&token)
            .json(&request)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesClosed);
        // The health answer says so before anybody is refused, and says
        // the opposite on a server that takes sites.
        let health: HealthResponse = get(&router, "/v1/health").await.json();
        assert_eq!(health.sites_from_pilots, Some(SitesFromPilots::Closed));
        let (open, _) = recorded_with_its_store().await;
        let health: HealthResponse = get(&open, "/v1/health").await.json();
        assert_eq!(health.sites_from_pilots, Some(SitesFromPilots::Open));
        // A server from before this said nothing, which reads as nothing.
        let old: HealthResponse = serde_json::from_str(
            r#"{"status":"ok","version":"0","time":"2026-11-21T10:00:00Z","sources":[]}"#,
        )
        .unwrap();
        assert_eq!(old.sites_from_pilots, None);
        // Reading is never off.
        let changes: SiteChangesResponse = get(&router, "/v1/sites/changes").await.json();
        assert_eq!((changes.sites.len(), changes.cursor.as_str()), (0, "0"));
        // The partners' layer needs no setting: the trust is its gate.
        // Whoever runs the server names petra, and her next request adds
        // a site to it.
        let petra = store.account("petra").await.unwrap().unwrap();
        store.let_edit_sites(&petra.id, clock.now()).await.unwrap();
        let added: SiteResponse = ask(Method::POST, "/v1/sites")
            .token(&token)
            .json(&request)
            .send(&router)
            .await
            .ok(StatusCode::CREATED);
        assert_eq!(added.site.id.source().as_str(), "venti");
        // Confirmations are pilots' words and are closed with the
        // setting, a partner's too.
        let confirmation = format!("/v1/sites/{}/confirmation", added.site.id);
        let today = ConfirmSiteRequest {
            day: clock.now().date_naive(),
            note: None,
            flight: None,
        };
        ask(Method::PUT, &confirmation)
            .token(&token)
            .json(&today)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesClosed);
        // Withdrawn, the account is a pilot as any other: on this server
        // it writes nothing, and what it added stays.
        store.stop_editing_sites(&petra.id).await.unwrap();
        ask(Method::DELETE, &format!("/v1/sites/{}", added.site.id))
            .token(&token)
            .send(&router)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::SitesForPartners);
        get(&router, &format!("/v1/sites/{}", added.site.id))
            .await
            .ok::<SiteResponse>(StatusCode::OK);

        // A real server bounds what one account writes in a day; the
        // numbers are the documented ones.
        let limits = pilot_sites::Limits::default();
        assert_eq!((limits.added_per_day, limits.revisions_per_day), (20, 100));
        let mut open = settings();
        open.sites.limits = pilot_sites::Limits {
            added_per_day: 1,
            revisions_per_day: 1,
            confirmations_per_day: 1,
        };
        let (router, _) = server(open, &clock);
        let token = sign_up(&router, "petra").await.token;
        ask(Method::POST, "/v1/sites")
            .token(&token)
            .json(&request)
            .send(&router)
            .await
            .ok::<SiteResponse>(StatusCode::CREATED);
        let second = AddSiteRequest {
            site: draft("Beispielkopf Nord", 47.41, 11.2),
            comment: None,
        };
        ask(Method::POST, "/v1/sites")
            .token(&token)
            .json(&second)
            .send(&router)
            .await
            .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    }
}
