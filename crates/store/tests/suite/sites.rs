//! The behaviour every [`Store`] must have for flying sites: one whole set
//! per source, replaced by a healthy run and by nothing else.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, GeoPoint, Site, SiteUse, SourceId, Timestamp, WindSector};
use venti_store::{Month, RunRecord, SiteDelivery, SiteLayer, SiteSet, Store, StoreError};

use super::{alps, at, everything, run, source};

const OSM: &str = "osm-free-flying";
const CLUB: &str = "club";

// --- Helpers -------------------------------------------------------------

/// A take-off at the given position.
fn takeoff(id: &str, lat: f64, lon: f64) -> Site {
    Site {
        id: id.parse().unwrap(),
        name: Some(format!("Startplatz {id}")),
        position: GeoPoint::new(lat, lon).unwrap(),
        elevation_m: Some(1500.0),
        uses: vec![SiteUse::Takeoff],
        wind_directions: Vec::new(),
        paragliding: None,
        hang_gliding: None,
        official: None,
        discouraged: false,
        url: None,
    }
}

/// What the runs of these tests say about their sets: OpenStreetMap as of
/// the evening before, whole inside the first region.
fn region() -> SiteDelivery {
    SiteDelivery {
        as_of: Utc.with_ymd_and_hms(2026, 11, 17, 21, 21, 2).unwrap(),
        covers: vec![alps()],
    }
}

async fn save(store: &impl Store, source_name: &str, started_at: Timestamp, set: &[Site]) {
    store
        .save_site_run(&run(source_name, started_at), &region(), set)
        .await
        .unwrap();
}

async fn layers_in(store: &impl Store, sources: &[&str], area: &BoundingBox) -> Vec<SiteLayer> {
    let sources: Vec<SourceId> = sources.iter().map(|name| source(name)).collect();
    store.sites_in(&sources, area, 1000).await.unwrap()
}

/// The identifiers of what one source has in a box.
async fn ids_in(store: &impl Store, source_name: &str, area: &BoundingBox) -> Vec<String> {
    layers_in(store, &[source_name], area)
        .await
        .into_iter()
        .flat_map(|layer| layer.sites)
        .map(|site| site.id.to_string())
        .collect()
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn a_site_set_is_stored_and_read_back_exactly(store: &impl Store) {
    // Everything a record can carry, with awkward decimals and a name
    // outside ASCII.
    let full = Site {
        name: Some("Fluggelände Jägerkamp/Nagelspitz \"Ost\"".to_owned()),
        position: GeoPoint::new(47.679_100_000_000_004, 11.905_1 + 0.1 + 0.2).unwrap(),
        elevation_m: Some(1560.5),
        uses: vec![SiteUse::Takeoff, SiteUse::Landing],
        wind_directions: vec![
            WindSector::single(292.5),
            WindSector {
                from_deg: 315.0,
                to_deg: 45.0,
            },
        ],
        paragliding: Some(true),
        hang_gliding: Some(false),
        official: Some(true),
        discouraged: true,
        url: Some("https://www.openstreetmap.org/way/1080986173".to_owned()),
        ..takeoff("osm-free-flying:way/1080986173", 47.0, 11.0)
    };
    let bare = Site {
        name: None,
        elevation_m: None,
        uses: Vec::new(),
        ..takeoff("osm-free-flying:node/5964676196", 47.6652, 11.7954)
    };
    let set = [bare.clone(), full.clone()];

    save(store, OSM, at(18, 30), &set).await;

    let layers = layers_in(store, &[OSM], &alps()).await;
    assert_eq!(
        layers,
        [SiteLayer {
            set: SiteSet {
                source: source(OSM),
                fetched_at: at(18, 30),
                sites: 2,
                as_of: region().as_of,
                covers: vec![alps()],
            },
            sites: vec![bare, full],
        }]
    );
    assert_eq!(
        store.site_sets(&[source(OSM)]).await.unwrap(),
        [layers[0].set.clone()]
    );
    // The run is in the history like any other.
    let status = &store.source_status(&[source(OSM)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(18, 30)));
    assert_eq!(status.last_success, Some(at(18, 31)));
}

pub(crate) async fn sites_are_found_by_box_edges_included(store: &impl Store) {
    // The query box is the first region: 46.3..48.5 N, 9.4..17.2 E.
    let set = [
        takeoff("osm-free-flying:inside", 47.66, 11.80),
        takeoff("osm-free-flying:on-north-edge", 48.5, 11.0),
        takeoff("osm-free-flying:on-corner", 46.3, 9.4),
        // A hair beyond the northern and the western edge.
        takeoff("osm-free-flying:just-north", 48.500_001, 11.0),
        takeoff("osm-free-flying:just-west", 47.0, 9.399_999),
        takeoff("osm-free-flying:far-away", 53.4, 9.8),
    ];
    save(store, OSM, at(18, 30), &set).await;

    assert_eq!(
        ids_in(store, OSM, &alps()).await,
        [
            "osm-free-flying:inside",
            "osm-free-flying:on-corner",
            "osm-free-flying:on-north-edge",
        ]
    );
    assert_eq!(ids_in(store, OSM, &everything()).await.len(), 6);
    // A box that is a single point finds the site that stands on it.
    let spot = BoundingBox::new(47.66, 11.80, 47.66, 11.80).unwrap();
    assert_eq!(ids_in(store, OSM, &spot).await, ["osm-free-flying:inside"]);
    // Where the set has nothing, the answer is the set without sites: it
    // still says how old it is and where it is whole.
    let nowhere = BoundingBox::new(10.0, 10.0, 11.0, 11.0).unwrap();
    let empty = layers_in(store, &[OSM], &nowhere).await;
    assert_eq!(empty.len(), 1);
    assert!(empty[0].sites.is_empty());
    assert_eq!(empty[0].set.sites, 6);
    assert_eq!(empty[0].set.fetched_at, at(18, 30));
    assert_eq!(empty[0].set.covers, [alps()]);
}

pub(crate) async fn a_healthy_site_run_replaces_the_whole_set(store: &impl Store) {
    let wallberg = takeoff("osm-free-flying:wallberg", 47.66, 11.80);
    let brauneck = takeoff("osm-free-flying:brauneck", 47.66, 11.52);
    let other = takeoff("club:wallberg", 47.66, 11.80);
    save(store, OSM, at(3, 0), &[brauneck, wallberg.clone()]).await;
    save(store, CLUB, at(3, 0), std::slice::from_ref(&other)).await;

    // The next day: the Brauneck is gone from the map, the Blomberg is new,
    // and somebody gave the Wallberg its wind directions.
    let tagged = Site {
        wind_directions: vec![WindSector::single(0.0)],
        ..wallberg
    };
    let blomberg = takeoff("osm-free-flying:blomberg", 47.73, 11.51);
    let later = SiteDelivery {
        as_of: at(1, 0),
        covers: vec![alps(), everything()],
    };
    store
        .save_site_run(
            &run(OSM, at(4, 0)),
            &later,
            &[blomberg.clone(), tagged.clone()],
        )
        .await
        .unwrap();

    let layers = layers_in(store, &[OSM], &everything()).await;
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].sites, [blomberg, tagged]);
    assert_eq!(layers[0].set.fetched_at, at(4, 0));
    assert_eq!(layers[0].set.sites, 2);
    // What the set says about itself is the new run's word too.
    assert_eq!(layers[0].set.as_of, at(1, 0));
    assert_eq!(layers[0].set.covers, [alps(), everything()]);
    // The other source's set is its own.
    let club = layers_in(store, &[CLUB], &everything()).await;
    assert_eq!(club[0].sites, [other]);
    assert_eq!(club[0].set.fetched_at, at(3, 0));
}

pub(crate) async fn an_unhealthy_site_run_leaves_the_set_alone(store: &impl Store) {
    let failed = |started_at: Timestamp| RunRecord {
        healthy: false,
        failed_requests: 1,
        ..run(OSM, started_at)
    };
    let half = [takeoff("osm-free-flying:wallberg", 47.66, 11.80)];

    // A source whose first run fails has no sites at all.
    store
        .save_site_run(&failed(at(2, 0)), &region(), &half)
        .await
        .unwrap();
    assert!(layers_in(store, &[OSM], &everything()).await.is_empty());
    assert!(store.site_sets(&[source(OSM)]).await.unwrap().is_empty());

    let whole = [
        takeoff("osm-free-flying:brauneck", 47.66, 11.52),
        takeoff("osm-free-flying:wallberg", 47.66, 11.80),
    ];
    save(store, OSM, at(3, 0), &whole).await;

    // The run after it could not finish: half a set, or none, changes nothing.
    store
        .save_site_run(&failed(at(4, 0)), &region(), &half)
        .await
        .unwrap();
    store
        .save_site_run(&failed(at(5, 0)), &region(), &[])
        .await
        .unwrap();

    let layers = layers_in(store, &[OSM], &everything()).await;
    assert_eq!(layers[0].sites, whole);
    assert_eq!(layers[0].set.fetched_at, at(3, 0));
    // The failures are in the history all the same.
    let status = &store.source_status(&[source(OSM)]).await.unwrap()[0];
    assert_eq!(status.last_success, Some(at(3, 1)));
    assert_eq!(status.last_failure, Some(at(5, 1)));
    assert_eq!(status.last_started, Some(at(5, 0)));
}

pub(crate) async fn an_older_site_run_does_not_replace_a_newer_set(store: &impl Store) {
    let newer = [takeoff("osm-free-flying:newer", 47.66, 11.80)];
    let older = [takeoff("osm-free-flying:older", 47.66, 11.52)];
    save(store, OSM, at(6, 0), &newer).await;

    // A run that started earlier finishes late.
    save(store, OSM, at(5, 0), &older).await;

    assert_eq!(
        ids_in(store, OSM, &everything()).await,
        ["osm-free-flying:newer"]
    );
    // Saving the same run again changes nothing either.
    save(store, OSM, at(6, 0), &newer).await;
    let layers = layers_in(store, &[OSM], &everything()).await;
    assert_eq!(layers[0].sites, newer);
    assert_eq!(layers[0].set.sites, 1);
}

pub(crate) async fn a_bad_site_set_is_refused_and_nothing_is_stored(store: &impl Store) {
    let good = [takeoff("osm-free-flying:wallberg", 47.66, 11.80)];
    save(store, OSM, at(3, 0), &good).await;
    let refused = |result: Result<(), StoreError>| match result {
        Err(StoreError::Invalid(problem)) => problem,
        other => panic!("expected a refusal, got {other:?}"),
    };
    let next = run(OSM, at(4, 0));

    // A healthy run that delivers nothing would empty the map.
    let problem = refused(store.save_site_run(&next, &region(), &[]).await);
    assert!(problem.contains("no site"), "{problem}");

    // A site of another source.
    let stranger = [
        takeoff("osm-free-flying:brauneck", 47.66, 11.52),
        takeoff("club:brauneck", 47.66, 11.52),
    ];
    let problem = refused(store.save_site_run(&next, &region(), &stranger).await);
    assert!(problem.contains("club:brauneck"), "{problem}");

    // The same site twice.
    let twice = [
        takeoff("osm-free-flying:brauneck", 47.66, 11.52),
        takeoff("osm-free-flying:brauneck", 47.67, 11.53),
    ];
    let problem = refused(store.save_site_run(&next, &region(), &twice).await);
    assert!(problem.contains("twice"), "{problem}");

    // Nothing of the three attempts was stored, not even the run.
    assert_eq!(
        ids_in(store, OSM, &everything()).await,
        ["osm-free-flying:wallberg"]
    );
    let status = &store.source_status(&[source(OSM)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(3, 0)));
}

pub(crate) async fn site_layers_are_kept_apart_and_sorted(store: &impl Store) {
    save(
        store,
        OSM,
        at(3, 0),
        &[
            takeoff("osm-free-flying:way/9", 47.1, 11.1),
            takeoff("osm-free-flying:node/10", 47.2, 11.2),
            takeoff("osm-free-flying:node/9", 47.3, 11.3),
        ],
    )
    .await;
    save(store, CLUB, at(3, 30), &[takeoff("club:1", 47.1, 11.1)]).await;

    // One layer per source, sorted by source; within a layer by identifier
    // as text.
    let layers = layers_in(store, &[OSM, CLUB], &everything()).await;
    let listed: Vec<(String, Vec<String>)> = layers
        .iter()
        .map(|layer| {
            let ids = layer.sites.iter().map(|s| s.id.to_string()).collect();
            (layer.set.source.to_string(), ids)
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("club".to_owned(), vec!["club:1".to_owned()]),
            (
                "osm-free-flying".to_owned(),
                vec![
                    "osm-free-flying:node/10".to_owned(),
                    "osm-free-flying:node/9".to_owned(),
                    "osm-free-flying:way/9".to_owned(),
                ]
            ),
        ]
    );

    // Only the sources asked for are answered for.
    let only = layers_in(store, &[CLUB], &everything()).await;
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].set.source, source(CLUB));
    assert!(layers_in(store, &[], &everything()).await.is_empty());
    assert!(
        layers_in(store, &["nobody"], &everything())
            .await
            .is_empty()
    );

    // The limit counts sites over all layers and keeps the first.
    let sources = [source(OSM), source(CLUB)];
    let cut = store.sites_in(&sources, &everything(), 2).await.unwrap();
    assert_eq!(cut.len(), 2);
    assert_eq!(cut[0].sites.len(), 1);
    assert_eq!(cut[1].sites.len(), 1);
    assert_eq!(cut[1].sites[0].id.to_string(), "osm-free-flying:node/10");
    // The set still says how many it holds.
    assert_eq!(cut[1].set.sites, 3);
}

pub(crate) async fn pruning_leaves_sites_alone(store: &impl Store) {
    save(
        store,
        OSM,
        at(3, 0),
        &[takeoff("osm-free-flying:wallberg", 47.66, 11.80)],
    )
    .await;

    // Everything before 2027 goes: the run of November 2026 with it.
    let new_year = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
    store.prune_before(Month::of(new_year)).await.unwrap();

    let status = &store.source_status(&[source(OSM)]).await.unwrap()[0];
    assert_eq!(status.last_started, None);
    // The sites stay until a newer set replaces them.
    assert_eq!(
        ids_in(store, OSM, &everything()).await,
        ["osm-free-flying:wallberg"]
    );
    assert_eq!(store.site_sets(&[source(OSM)]).await.unwrap().len(), 1);
}
