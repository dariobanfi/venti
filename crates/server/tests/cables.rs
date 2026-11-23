//! Tests of the cables: the endpoint through the real router, over what
//! the development mode serves (the cables recorded from OpenStreetMap
//! near the Tegernsee, run through the real adapter). One answer is kept
//! as a golden file, which is also the example in `docs/api.md`.

use std::sync::Arc;

use axum::http::{StatusCode, header};
use venti_api_types::v1::{CablesRequest, CablesResponse, ErrorCode};
use venti_domain::{CableKind, SourceId};
use venti_server::adapters::{Providers, registry_for};
use venti_server::clock::FixedClock;
use venti_server::ingest::Collector;
use venti_server::plan::Job;
use venti_sources::Kind;
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden};
use venti_sources_osm::fixtures;
use venti_store::{MemoryStore, Store};

mod common;

use common::{get, golden, recorded};

#[tokio::test]
async fn a_box_gets_the_cables_that_touch_it_with_their_credit() {
    let router = recorded().await;
    // The Wallberg and Rottach-Egern: the box the cables were recorded for.
    let bbox = venti_sources_osm::fixtures::cables_area();
    let answer = get(&router, &CablesRequest { bbox }.path_and_query()).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(answer.header(header::CACHE_CONTROL).contains("public"));
    let cables: CablesResponse = answer.json();

    assert_eq!(cables.layers.len(), 1);
    let layer = &cables.layers[0];
    assert_eq!(layer.source.as_str(), "osm-cables");
    assert_eq!(layer.cables.len(), 8);
    assert_eq!(layer.covers, [venti_sources_osm::fixtures::cables_area()]);
    assert!(
        layer
            .cables
            .iter()
            .any(|cable| cable.kind == CableKind::Gondola
                && cable.name.as_deref() == Some("Wallbergbahn"))
    );
    let credit = &cables.attributions[0];
    assert!(credit.text.contains("OpenStreetMap"), "{}", credit.text);
    assert_golden(&golden("cables.json"), &cables);
}

#[tokio::test]
async fn a_box_without_cables_is_a_layer_without_cables() {
    let router = recorded().await;
    let quiet = "/v1/cables?bbox=14.90,48.20,14.91,48.21";
    let nothing: CablesResponse = get(&router, quiet).await.json();
    assert_eq!(nothing.layers.len(), 1);
    assert!(nothing.layers[0].cables.is_empty());
    assert_eq!(nothing.attributions.len(), 1);
}

#[tokio::test]
async fn a_box_is_needed() {
    let router = recorded().await;
    let answer = get(&router, "/v1/cables").await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    let answer = get(&router, "/v1/cables?bbox=11,47").await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
}

#[tokio::test]
async fn a_tile_that_fails_leaves_its_stored_cables_and_the_other_tile_replaces_its_own() {
    let source = SourceId::new("osm-cables").unwrap();
    let job = Job {
        source: source.clone(),
        kind: Kind::Cables,
        area: fixtures::cables_tiles_area(),
    };
    let store = Arc::new(MemoryStore::new());
    let first_run = fixtures::cables_tiles_recorded_at();
    let collect = |fetch: FixtureFetch, at| {
        Collector::new(
            registry_for(&Providers::recorded()).unwrap(),
            store.clone(),
            fetch,
            MemoryArchive::default(),
            Arc::new(FixedClock(at)),
            None,
        )
    };
    let held = || async {
        let sets = store.cable_sets(std::slice::from_ref(&source)).await;
        let set = sets.unwrap().remove(0);
        (set.fetched_at, set.cables, set.covers)
    };
    let tiles = fixtures::cables_tiles();
    let (south, north) = (tiles[0].0.clone(), tiles[1].0.clone());
    let area = fixtures::cables_tiles_area();

    // Both tiles answer: their seventeen cables are the stored set.
    let both = FixtureFetch::new().with_all(tiles.clone());
    assert!(collect(both, first_run).collect(&job).await);
    assert_eq!(held().await, (first_run, 17, vec![area]));

    // A week later the northern tile is refused three times. The run
    // failed, but the southern tile's answer replaces the cables inside
    // it, and those of the northern tile stay: still seventeen, here the
    // same ones, as the recording is the same.
    let busy = FixtureFetch::new()
        .with_all(tiles.clone())
        .refusing(&north, 502, 3);
    let a_week_later = first_run + chrono::Duration::days(7);
    assert!(!collect(busy, a_week_later).collect(&job).await);
    assert_eq!(held().await, (a_week_later, 17, vec![area]));

    // On an empty store, a run whose southern tile fails stores the
    // northern tile's cables, and says it covers that tile alone.
    let empty = Arc::new(MemoryStore::new());
    let half = FixtureFetch::new().with_all(tiles).refusing(&south, 502, 3);
    let collector = Collector::new(
        registry_for(&Providers::recorded()).unwrap(),
        empty.clone(),
        half,
        MemoryArchive::default(),
        Arc::new(FixedClock(first_run)),
        None,
    );
    assert!(!collector.collect(&job).await);
    let set = empty
        .cable_sets(std::slice::from_ref(&source))
        .await
        .unwrap()
        .remove(0);
    let northern_tile = venti_domain::BoundingBox::new(48.0, 11.42, 48.03, 11.6).unwrap();
    assert_eq!(set.covers, [northern_tile]);
    // The northern tile's answer holds three ways, which reach into the
    // southern tile.
    assert_eq!(set.cables, 3);
}
