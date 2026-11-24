//! Tests of the forecast along a route through the real router.
//!
//! The development mode's thermal map is made of four recorded cells
//! around the Wallberg; its answer for a route through two of them and a
//! place outside the map is kept as a golden file, which is also the
//! example in `docs/api.md`.

use axum::http::{StatusCode, header};
use chrono::NaiveDate;
use venti_api_types::v1::{
    ErrorCode, ROUTE_POINTS_AT_MOST, RouteWeatherRequest, RouteWeatherResponse,
    ThermalHoursRequest, ThermalHoursResponse, ThermalLayer,
};
use venti_domain::GeoPoint;
use venti_sources::testing::assert_golden;

mod common;

use common::{get, get_with, golden, recorded};

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

#[tokio::test]
async fn a_route_has_the_hours_of_the_map_at_each_of_its_points() {
    let router = recorded().await;
    let date = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
    // The Wallberg, in the cell of 47.7° N 11.8° E; a place in the cell of
    // 47.6° N 11.9° E; and the Brauneck, west of the recorded cells.
    let asked = RouteWeatherRequest {
        points: vec![
            point(47.6642, 11.7967),
            point(47.62, 11.91),
            point(47.6603, 11.5197),
        ],
        date,
    }
    .path_and_query();
    assert_eq!(
        asked,
        "/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.62,11.91;47.6603,11.5197"
    );
    let answer = get(&router, &asked).await;
    let route: RouteWeatherResponse = answer.ok(StatusCode::OK);

    // Cacheable and validated as the map's own answers are.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    let etag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, &asked, &[(header::IF_NONE_MATCH, &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);

    assert_eq!(route.source.as_str(), "open-meteo");
    assert_eq!(route.attributions.len(), 1);
    assert_eq!(route.date, date);
    assert_eq!(route.heights_m, [1000, 1500, 2000, 2500, 3000, 4000]);
    assert_eq!(route.points.len(), 3);

    // Each point inside the map has what every layer of the hours has for
    // its cell, hour by hour.
    let layer = |layer, height_m| {
        let router = &router;
        let asked = ThermalHoursRequest {
            date,
            layer,
            height_m,
            above_ground_m: None,
        }
        .path_and_query();
        async move {
            let hours: ThermalHoursResponse = get(router, &asked).await.ok(StatusCode::OK);
            hours
        }
    };
    let judged = layer(ThermalLayer::Thermals, None).await;
    let ground = layer(ThermalLayer::Wind, None).await;
    let cloud = layer(ThermalLayer::Cloud, None).await;
    let rain = layer(ThermalLayer::Rain, None).await;
    let at_1000 = layer(ThermalLayer::Wind, Some(1000)).await;
    assert_eq!((route.model.clone(), route.run), (judged.model, judged.run));
    for (index, cell) in [(0, 2), (1, 1)] {
        let at = &route.points[index];
        assert_eq!(at.cell, judged.grid.point(cell));
        assert_eq!(at.ground_m, judged.ground_m[cell]);
        assert_eq!(at.hours.len(), judged.hours.len());
        for (hour, of_map) in at.hours.iter().zip(&judged.hours) {
            let n = judged
                .hours
                .iter()
                .position(|h| h.time == hour.time)
                .unwrap();
            assert_eq!(hour.time, of_map.time);
            assert_eq!(hour.verdict, of_map.verdict[cell]);
            assert_eq!(hour.thermals, of_map.thermals[cell]);
            assert_eq!(hour.top_m, of_map.top_m[cell]);
            assert_eq!(hour.wind_kmh, ground.hours[n].wind_kmh[cell]);
            assert_eq!(hour.gust_kmh, ground.hours[n].gust_kmh[cell]);
            assert_eq!(hour.cloud_cover_pct, cloud.hours[n].cloud_cover_pct[cell]);
            assert_eq!(hour.precipitation_mm, rain.hours[n].precipitation_mm[cell]);
            // The wind at the heights that have one: the cells' ground
            // lies above 1000 m, so the lowest is 1500 m.
            assert_eq!(at_1000.hours[n].wind_kmh[cell], None);
            let heights: Vec<i32> = hour.aloft.iter().map(|wind| wind.height_m).collect();
            assert_eq!(heights, [1500, 2000, 2500, 3000, 4000]);
        }
    }
    // A point outside the map has no hours.
    let outside = &route.points[2];
    assert_eq!((outside.cell, outside.ground_m), (None, None));
    assert!(outside.hours.is_empty());

    assert_golden(&golden("route_weather.json"), &route);
}

#[tokio::test]
async fn a_route_that_is_none_or_a_day_the_map_lacks_is_refused() {
    let router = recorded().await;
    let refused = |path: &'static str, status, code| {
        let router = &router;
        async move { get(router, path).await.error(status, code) }
    };
    let message = refused(
        "/v1/route/weather?points=47.6,11.8",
        StatusCode::BAD_REQUEST,
        ErrorCode::InvalidDay,
    )
    .await;
    assert!(message.contains("date is missing"), "{message}");
    let message = refused(
        "/v1/route/weather?date=2026-11-22",
        StatusCode::BAD_REQUEST,
        ErrorCode::InvalidPosition,
    )
    .await;
    assert!(message.contains("points is empty"), "{message}");
    let message = refused(
        "/v1/route/weather?date=2026-11-22&points=47.6,11.8;91,11.8",
        StatusCode::BAD_REQUEST,
        ErrorCode::InvalidPosition,
    )
    .await;
    assert!(message.contains("point 2"), "{message}");
    let many = vec!["47.6,11.8"; ROUTE_POINTS_AT_MOST + 1].join(";");
    let message = get(
        &router,
        &format!("/v1/route/weather?date=2026-11-22&points={many}"),
    )
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPosition);
    assert!(message.contains("at most 25"), "{message}");
    let message = refused(
        "/v1/route/weather?date=2026-12-24&points=47.6,11.8",
        StatusCode::NOT_FOUND,
        ErrorCode::NotFound,
    )
    .await;
    assert!(message.contains("it has 2026-11-21"), "{message}");
}
