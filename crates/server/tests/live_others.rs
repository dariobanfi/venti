//! Tests of `GET /v1/live/others` through the real router.
//!
//! The development mode feeds the live network with lines in the Open
//! Glider Network's form, made up over the Tegernsee (not network data),
//! by the code a real server runs. Its answer for the Tegernsee is kept as a golden file, which is
//! also the example in `docs/api.md`.

use axum::http::{StatusCode, header};
use venti_api_types::v1::{AircraftKind, ErrorCode, LIVE_OTHERS_PATH, LiveOthersResponse};
use venti_server::adapters;
use venti_sources::testing::assert_golden;

mod common;

use common::{get, golden, recorded};

const TEGERNSEE: &str = "/v1/live/others?bbox=11.6,47.55,12.0,47.85";

#[tokio::test]
async fn the_development_mode_lists_the_aircraft_over_the_tegernsee() {
    let router = recorded().await;
    let answer = get(&router, TEGERNSEE).await;
    let others: LiveOthersResponse = answer.ok(StatusCode::OK);

    // Anyone may keep it for five seconds.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=5");
    assert_eq!(others.time, adapters::live_feed_at());
    assert_eq!(others.listed_minutes, 10);

    let [layer] = &others.layers[..] else {
        panic!("one network: {:?}", others.layers.len());
    };
    assert_eq!(layer.source.as_str(), "ogn");
    assert!(!layer.truncated);
    // Paragliders and hang gliders first.
    let kinds: Vec<AircraftKind> = layer.aircraft.iter().map(|a| a.aircraft).collect();
    let free = kinds
        .iter()
        .take_while(|kind| kind.is_free_flight())
        .count();
    assert!(free >= 3, "{kinds:?}");
    assert!(kinds[free..].iter().all(|kind| !kind.is_free_flight()));
    // Every one within the box and the ten minutes.
    for aircraft in &layer.aircraft {
        assert!(aircraft.age_s <= 600, "{}", aircraft.age_s);
        assert_eq!(aircraft.id.len(), 16);
    }
    // The made-up devices that opted out, or whose line says so, or that
    // were heard too long ago, are not there; no address is.
    let json = serde_json::to_string(&others).unwrap();
    assert!(!json.contains("E000"), "{json}");
    // The identified glider has its registration; the anonymous
    // paraglider has none.
    assert!(
        layer
            .aircraft
            .iter()
            .any(|a| a.registration.as_deref() == Some("D-0000"))
    );

    assert_eq!(others.attributions.len(), 1);
    assert_eq!(others.attributions[0].licence, "ODbL 1.0");
    assert!(others.attributions[0].notice.is_some());

    assert_golden(&golden("live_others.json"), &others);
}

#[tokio::test]
async fn the_whole_region_has_the_airliner_after_the_gliders() {
    let router = recorded().await;
    let path = format!("{LIVE_OTHERS_PATH}?bbox=9.4,46.3,17.2,48.5");
    let others: LiveOthersResponse = get(&router, &path).await.ok(StatusCode::OK);
    let layer = &others.layers[0];
    // The Tegernsee's seven and the airliner over Munich.
    assert_eq!(layer.aircraft.len(), 8);
    assert!(layer.aircraft[0].aircraft.is_free_flight());
    assert!(
        layer
            .aircraft
            .iter()
            .any(|a| a.aircraft == AircraftKind::JetAircraft)
    );
}

#[tokio::test]
async fn a_box_too_large_or_missing_is_refused() {
    let router = recorded().await;
    for path in [
        LIVE_OTHERS_PATH.to_owned(),
        format!("{LIVE_OTHERS_PATH}?bbox=0,40,20,50"),
        format!("{LIVE_OTHERS_PATH}?bbox=11,47"),
    ] {
        let answer = get(&router, &path).await;
        answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    }
}
