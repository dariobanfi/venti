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
async fn a_tile_that_fails_leaves_the_stored_cables_as_they_were() {
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
        (set.fetched_at, set.cables)
    };

    // Both tiles answer: their seventeen cables are the stored set.
    let both = FixtureFetch::new().with_all(fixtures::cables_tiles());
    assert!(collect(both, first_run).collect(&job).await);
    assert_eq!(held().await, (first_run, 17));

    // A week later the northern tile is refused three times. The southern
    // one's answer is not half a set to store: the week before's stays.
    let north = fixtures::cables_tiles()[1].0.clone();
    let busy = FixtureFetch::new()
        .with_all(fixtures::cables_tiles())
        .refusing(&north, 502, 3);
    let a_week_later = first_run + chrono::Duration::days(7);
    assert!(!collect(busy, a_week_later).collect(&job).await);
    assert_eq!(held().await, (first_run, 17));
}
