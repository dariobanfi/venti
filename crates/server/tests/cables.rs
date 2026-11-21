//! Tests of the cables: the endpoint through the real router, over what
//! the development mode serves (the cables recorded from OpenStreetMap
//! near the Tegernsee, run through the real adapter). One answer is kept
//! as a golden file, which is also the example in `docs/api.md`.

use axum::http::{StatusCode, header};
use venti_api_types::v1::{CablesRequest, CablesResponse, ErrorCode};
use venti_domain::CableKind;
use venti_sources::testing::assert_golden;

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
