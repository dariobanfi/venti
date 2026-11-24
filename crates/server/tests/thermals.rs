//! Tests of the thermal map's endpoint through the real router.
//!
//! The development mode makes its map from answers recorded from an
//! Open-Meteo instance for four places in one request, by the code a real
//! server runs; its answer is kept as a golden file, which is also the
//! example in `docs/api.md`. The recorded days are four of late November:
//! weak thermals where there are any, and that is what the map says.

use std::sync::Arc;

use axum::http::{StatusCode, header};
use venti_api_types::v1::{
    ErrorCode, THERMAL_HOURS_PATH, THERMALS_PATH, ThermalHoursRequest, ThermalHoursResponse,
    ThermalKind, ThermalLayer, ThermalMapResponse, ThermalReason, ThermalVerdict,
};
use venti_domain::Timestamp;
use venti_regions::RegionPack;
use venti_server::adapters::{self, Providers};
use venti_server::api::{Api, Cors};
use venti_server::clock::{Clock, FixedClock};
use venti_server::forecast::{self, Asking};
use venti_server::ingest::{Collector, DirArchive};
use venti_server::plan;
use venti_server::thermals::ThermalMap;
use venti_sources::testing::assert_golden;
use venti_store::MemoryStore;

mod common;

use common::{TestClock, get, get_with, golden, recorded, server, settings};

const FIRST_REGION: &str = include_str!("../../../regions/alps-de-at/region.toml");

#[tokio::test]
async fn the_development_mode_serves_the_map_of_the_recorded_places() {
    let router = recorded().await;

    let answer = get(&router, THERMALS_PATH).await;
    let map: ThermalMapResponse = answer.ok(StatusCode::OK);

    // Cacheable like every answer that is the same for everyone.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    let etag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, THERMALS_PATH, &[(header::IF_NONE_MATCH, &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    // Compressed for a client that takes it, as every answer of the API:
    // the map of the first region is some 126 kB, and 10 kB packed.
    let packed = get_with(&router, THERMALS_PATH, &[(header::ACCEPT_ENCODING, "gzip")]).await;
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");

    assert_eq!(map.source.as_str(), "open-meteo");
    assert_eq!(map.model.as_str(), "icon-d2");
    assert_eq!(map.run, adapters::thermal_map_recorded_runs()[0].1);
    assert_eq!(map.computed_at, adapters::thermal_map_recorded_at());
    assert_eq!(map.grid, adapters::recorded_thermal_grid());
    assert_eq!(
        map.ground_m,
        [Some(1118), Some(1046), Some(1069), Some(947)]
    );
    assert_eq!(map.attributions.len(), 1);
    assert!(map.attributions[0].text.contains("Open-Meteo.com"));
    // The heights the hours of its days have the wind at.
    assert_eq!(map.heights_m, [1000, 1500, 2000, 2500, 3000, 4000]);

    // The runs of 06 UTC of both models hold four days between them.
    // ICON-D2's reaches 48 hours, to 06 UTC of the 23rd: today and
    // tomorrow, each with all of its daylight, and nothing of the 23rd by
    // daylight. ICON-EU's is a main run and reaches 78 hours, to noon UTC
    // of the 24th: it has the first two days whole as well, and they stay
    // the finer model's; the 23rd, whole; and of the 24th the hours until
    // noon, which is a day that is not whole.
    let runs = adapters::thermal_map_recorded_runs();
    let (icon_d2, icon_eu) = (runs[0].1, runs[1].1);
    let days: Vec<(String, &str, Option<Timestamp>, bool)> = map
        .days
        .iter()
        .map(|day| {
            let model = day.model.as_ref().map_or("", |model| model.as_str());
            (day.date.to_string(), model, day.run, day.whole)
        })
        .collect();
    assert_eq!(
        days,
        [
            ("2026-11-21".to_owned(), "icon-d2", Some(icon_d2), true),
            ("2026-11-22".to_owned(), "icon-d2", Some(icon_d2), true),
            ("2026-11-23".to_owned(), "icon-eu", Some(icon_eu), true),
            ("2026-11-24".to_owned(), "icon-eu", Some(icon_eu), false),
        ]
    );
    for day in &map.days {
        assert_eq!(day.verdict.len(), 4);
        assert_eq!(day.why.len(), 4);
        assert_eq!(day.thermals.len(), 4);
        assert_eq!(day.climb_ms.len(), 4);
        assert_eq!(day.top_m.len(), 4);
        assert_eq!(day.cloud.len(), 4);
        for cell in 0..4 {
            // Every cell has a verdict on every day. A day that is not
            // flyable says nothing of thermals, and a November day has no
            // thermals, or weak ones at the most.
            match day.verdict[cell] {
                Some(ThermalVerdict::NotFlyable) => assert_eq!(day.thermals[cell], None),
                Some(_) => assert!(matches!(
                    day.thermals[cell],
                    Some(ThermalKind::NoThermals | ThermalKind::Weak) | None
                )),
                None => panic!("no verdict for cell {cell} on {}", day.date),
            }
            if let Some(climb) = day.climb_ms[cell] {
                assert_eq!(climb, (climb * 10.0).round() / 10.0);
            }
            if let Some(top) = day.top_m[cell] {
                assert_eq!(top % 50, 0);
            }
        }
    }

    assert_golden(&golden("thermals.json"), &map);
}

#[tokio::test]
async fn the_development_mode_serves_the_hours_of_each_recorded_day_layer_by_layer() {
    let router = recorded().await;
    let map: ThermalMapResponse = get(&router, THERMALS_PATH).await.ok(StatusCode::OK);
    let path = |date, layer, height_m| {
        ThermalHoursRequest {
            date,
            layer,
            height_m,
            above_ground_m: None,
        }
        .path_and_query()
    };

    // Tomorrow, the 22nd, from ICON-D2, and of it the wind at 1000 m
    // above the sea: the answer shown in the documentation.
    let tomorrow = map.days[1].date;
    let asked = path(tomorrow, ThermalLayer::Wind, Some(1000));
    assert_eq!(
        asked,
        "/v1/thermals/hours?date=2026-11-22&layer=wind&height=1000"
    );
    let answer = get(&router, &asked).await;
    let wind: ThermalHoursResponse = answer.ok(StatusCode::OK);

    // Cacheable, validated and compressed as the map of the days is.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    let etag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, &asked, &[(header::IF_NONE_MATCH, &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    let packed = get_with(&router, &asked, &[(header::ACCEPT_ENCODING, "gzip")]).await;
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");

    assert_eq!(wind.source.as_str(), "open-meteo");
    assert_eq!(wind.attributions.len(), 1);
    assert!(wind.attributions[0].text.contains("Open-Meteo.com"));
    assert_eq!(
        (wind.layer, wind.height_m),
        (ThermalLayer::Wind, Some(1000))
    );
    assert_eq!(wind.heights_m, [1000, 1500, 2000, 2500, 3000, 4000]);
    assert_eq!(wind.grid, map.grid);
    assert_eq!(wind.computed_at, map.computed_at);
    // The ground of the day's model, which here is the map's finest. Of
    // the four cells one lies under 1000 m: it alone has a wind there.
    assert_eq!(wind.ground_m, map.ground_m);
    assert_eq!(
        wind.ground_m,
        [Some(1118), Some(1046), Some(1069), Some(947)]
    );
    for hour in &wind.hours {
        assert_eq!(hour.wind_kmh[..3], [None; 3]);
        assert_eq!(hour.wind_from_deg[..3], [None; 3]);
        assert!(hour.wind_kmh[3].is_some() && hour.wind_from_deg[3].is_some());
        // No gusts at a height, and no list of another layer.
        assert!(hour.gust_kmh.is_empty() && hour.verdict.is_empty());
    }

    // Late November at 47.6° N: the sun rises about 06:25 UTC and sets
    // about 15:30, so the hours from an hour after sunrise to an hour
    // before sunset are those of 08 to 14 UTC.
    let clock: Vec<String> = wind
        .hours
        .iter()
        .map(|hour| hour.time.format("%d %H:%M").to_string())
        .collect();
    assert_eq!(
        clock,
        [
            "22 08:00", "22 09:00", "22 10:00", "22 11:00", "22 12:00", "22 13:00", "22 14:00"
        ]
    );
    assert_golden(&golden("thermal_hours_wind.json"), &wind);

    // Without a layer the answer is the hours' verdicts and thermals,
    // which make the day of the map: kept as a golden file too.
    let judged: ThermalHoursResponse = get(&router, "/v1/thermals/hours?date=2026-11-22")
        .await
        .ok(StatusCode::OK);
    assert_eq!(
        (judged.layer, judged.height_m),
        (ThermalLayer::Thermals, None)
    );
    assert_golden(&golden("thermal_hours.json"), &judged);

    // Every day of the map has its hours, of the day's model and run, in
    // every layer, and every list of every hour is as long as the grid.
    let heights = wind.heights_m.clone();
    for day in &map.days {
        let layer = |layer: ThermalLayer, height_m: Option<i32>| {
            let router = &router;
            let asked = path(day.date, layer, height_m);
            async move {
                let hours: ThermalHoursResponse = get(router, &asked).await.ok(StatusCode::OK);
                assert_eq!((hours.layer, hours.height_m), (layer, height_m));
                assert_eq!(hours.date, day.date);
                assert_eq!(Some(&hours.model), day.model.as_ref());
                assert_eq!(Some(hours.run), day.run);
                assert_eq!(hours.whole, day.whole);
                assert!(!hours.hours.is_empty(), "{}", day.date);
                hours
            }
        };
        let judged = layer(ThermalLayer::Thermals, None).await;
        let ground = layer(ThermalLayer::Wind, None).await;
        let cloud = layer(ThermalLayer::Cloud, None).await;
        let rain = layer(ThermalLayer::Rain, None).await;
        for (at, hour) in judged.hours.iter().enumerate() {
            let lengths = [
                hour.verdict.len(),
                hour.why.len(),
                hour.thermals.len(),
                hour.climb_ms.len(),
                hour.top_m.len(),
                hour.cloud.len(),
                ground.hours[at].wind_kmh.len(),
                ground.hours[at].wind_from_deg.len(),
                ground.hours[at].gust_kmh.len(),
                cloud.hours[at].cloud_cover_pct.len(),
                rain.hours[at].precipitation_mm.len(),
            ];
            assert_eq!(lengths, [4; 11]);
            // The layers of a day have the same hours.
            let time = hour.time;
            assert_eq!(
                [
                    ground.hours[at].time,
                    cloud.hours[at].time,
                    rain.hours[at].time
                ],
                [time; 3]
            );
            for cell in 0..4 {
                // Every cell has every hour here, with the model's wind,
                // gusts, cloud and rain; rain rules an hour out, and the
                // hour says so.
                assert!(hour.verdict[cell].is_some());
                let wind = &ground.hours[at];
                assert!(wind.wind_kmh[cell].is_some() && wind.gust_kmh[cell].is_some());
                assert!(wind.wind_from_deg[cell].is_some_and(|from| from % 10 == 0 && from < 360));
                assert!(cloud.hours[at].cloud_cover_pct[cell].is_some_and(|cloud| cloud <= 100));
                let fell = rain.hours[at].precipitation_mm[cell].expect("rain");
                assert_eq!(fell, (fell * 10.0).round() / 10.0);
                if fell >= 0.2 {
                    assert_eq!(hour.verdict[cell], Some(ThermalVerdict::NotFlyable));
                    assert_eq!(hour.why[cell], Some(ThermalReason::Rain));
                }
                // An hour that is not flyable says nothing of thermals.
                if hour.verdict[cell] == Some(ThermalVerdict::NotFlyable) {
                    assert_eq!(hour.thermals[cell], None);
                    assert_eq!(hour.climb_ms[cell], None);
                }
            }
        }
        for height in &heights {
            let wind = layer(ThermalLayer::Wind, Some(*height)).await;
            assert_eq!(wind.hours.len(), judged.hours.len());
            for hour in &wind.hours {
                assert_eq!((hour.wind_kmh.len(), hour.wind_from_deg.len()), (4, 4));
                assert!(hour.gust_kmh.is_empty());
                for cell in 0..4 {
                    // No wind at a height under the model's ground, and
                    // one at every height above it: the models' levels
                    // reach above 4000 m.
                    let under = wind.ground_m[cell].is_some_and(|ground| ground > *height);
                    assert_eq!(
                        hour.wind_kmh[cell].is_none(),
                        under,
                        "{height} m, cell {cell}"
                    );
                    assert_eq!(hour.wind_from_deg[cell].is_none(), under);
                }
            }
        }
    }
    // The 24th is ICON-EU's and ends with its run, at noon UTC.
    let last: ThermalHoursResponse =
        get(&router, &path(map.days[3].date, ThermalLayer::Rain, None))
            .await
            .ok(StatusCode::OK);
    assert_eq!(last.model.as_str(), "icon-eu");
    assert!(!last.whole);
    assert_eq!(last.hours.len(), 5);
}

#[tokio::test]
async fn the_hours_are_asked_for_by_a_day_the_map_has_and_a_layer_it_keeps() {
    let router = recorded().await;

    // Without a day, and with something that is none.
    let answer = get(&router, THERMAL_HOURS_PATH).await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidDay);
    assert!(message.contains("date=YYYY-MM-DD"), "{message}");
    let answer = get(&router, "/v1/thermals/hours?date=tomorrow").await;
    let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidDay);
    assert!(message.contains("\"tomorrow\""), "{message}");

    // A day the map does not have: the answer says which it has.
    let answer = get(&router, "/v1/thermals/hours?date=2026-12-24&layer=cloud").await;
    let message = answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    assert!(
        message.contains("2026-11-21, 2026-11-22, 2026-11-23, 2026-11-24"),
        "{message}"
    );

    // A layer there is none of, a height the map does not keep, and a
    // height asked of a layer that has none.
    let refused = |path: &'static str, says: &'static str| {
        let router = &router;
        async move {
            let answer = get(router, path).await;
            let message = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
            assert!(message.contains(says), "{message}");
        }
    };
    refused(
        "/v1/thermals/hours?date=2026-11-22&layer=snow",
        "thermals, wind, cloud, rain",
    )
    .await;
    refused(
        "/v1/thermals/hours?date=2026-11-22&layer=wind&height=1800",
        "1000, 1500, 2000, 2500, 3000, 4000",
    )
    .await;
    refused(
        "/v1/thermals/hours?date=2026-11-22&layer=wind&height=high",
        "metres above mean sea level",
    )
    .await;
    refused(
        "/v1/thermals/hours?date=2026-11-22&layer=rain&height=2000",
        "height goes with layer=wind",
    )
    .await;
}

#[tokio::test]
async fn a_server_without_forecasts_has_no_thermal_map() {
    let (router, _) = server(settings(), &TestClock::new());

    let answer = get(&router, THERMALS_PATH).await;
    let message = answer.error(StatusCode::NOT_IMPLEMENTED, ErrorCode::NoForecasts);
    assert!(message.contains("no thermal map"), "{message}");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");

    // Nor the hours of any day of it.
    let answer = get(&router, "/v1/thermals/hours?date=2026-11-21").await;
    answer.error(StatusCode::NOT_IMPLEMENTED, ErrorCode::NoForecasts);
}

#[tokio::test]
async fn a_server_with_forecasts_and_no_map_yet_says_when_to_ask_again() {
    let providers = Providers::recorded();
    let registry = adapters::registry_for(&providers).unwrap();
    let pack = RegionPack::from_toml(FIRST_REGION).unwrap();
    let jobs = plan::plan(&[pack], &registry).unwrap();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::forecasts_recorded_at()));
    let forecasts = forecast::service_for(
        &jobs,
        adapters::forecast_sources(&providers).unwrap(),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock.clone(),
        Asking::Live,
    )
    .unwrap();
    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let router = Api::new(Arc::new(MemoryStore::new()), clock, manifests, settings())
        .with_forecasts(forecasts)
        .router(&Cors::Origins(Vec::new()));

    for path in [THERMALS_PATH, "/v1/thermals/hours?date=2026-11-21"] {
        let answer = get(&router, path).await;
        let message = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert!(message.contains("not made yet"), "{message}");
        assert_eq!(answer.header(header::RETRY_AFTER), "60");
        assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    }
}

#[tokio::test]
async fn a_look_that_sees_a_run_has_the_map_made() {
    let providers = Providers::recorded();
    let registry = adapters::registry_for(&providers).unwrap();
    let pack = RegionPack::from_toml(FIRST_REGION).unwrap();
    let jobs = plan::plan(&[pack], &registry).unwrap();
    let job = jobs
        .iter()
        .find(|job| job.kind == venti_sources::Kind::Forecasts)
        .unwrap()
        .clone();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::thermal_map_recorded_at()));
    let service = |fetch| {
        forecast::service_for(
            &jobs,
            adapters::forecast_sources(&providers).unwrap(),
            fetch,
            None::<DirArchive>,
            clock.clone(),
            Asking::Recorded(adapters::recorded_forecast_places()),
        )
        .unwrap()
    };
    // The look asks the answers for single places, the map those for
    // four: two sets, each with its own answers about the runs, recorded
    // together and of the same runs.
    let looked_at = service(adapters::recorded_payloads());
    let map = ThermalMap::on_grid(
        service(adapters::recorded_thermal_payloads()),
        job.source.clone(),
        adapters::recorded_thermal_grid(),
        clock.clone(),
    )
    .map(Arc::new)
    .unwrap();
    let collector = Collector::new(
        registry,
        Arc::new(MemoryStore::new()),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock,
        None,
    )
    .with_forecasts(looked_at)
    .with_thermal_map(map.clone());

    assert!(map.current().is_none());
    assert!(collector.collect(&job).await);
    // The map is made in a task of its own.
    for _ in 0..1000 {
        if map.current().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let made = map.current().expect("the map is made after the look");
    assert_eq!(made.run, adapters::thermal_map_recorded_runs()[0].1);
}

#[tokio::test]
async fn the_development_mode_serves_the_wind_250_m_above_the_ground() {
    let router = recorded().await;
    let map: ThermalMapResponse = get(&router, THERMALS_PATH).await.ok(StatusCode::OK);
    assert_eq!(map.heights_above_ground_m, [250]);
    let tomorrow = map.days[1].date;
    let asked = ThermalHoursRequest {
        date: tomorrow,
        layer: ThermalLayer::Wind,
        height_m: None,
        above_ground_m: Some(250),
    }
    .path_and_query();
    assert_eq!(
        asked,
        "/v1/thermals/hours?date=2026-11-22&layer=wind&above_ground=250"
    );
    let low: ThermalHoursResponse = get(&router, &asked).await.ok(StatusCode::OK);
    assert_eq!((low.height_m, low.above_ground_m), (None, Some(250)));
    assert_eq!(low.heights_above_ground_m, [250]);
    let ground: ThermalHoursResponse =
        get(&router, "/v1/thermals/hours?date=2026-11-22&layer=wind")
            .await
            .ok(StatusCode::OK);
    for (hour, at_ground) in low.hours.iter().zip(&ground.hours) {
        assert!(hour.gust_kmh.is_empty());
        // Every cell has a ground, so every cell has a wind 250 m above it.
        for cell in 0..4 {
            assert!(hour.wind_kmh[cell].is_some(), "{} cell {cell}", hour.time);
            assert!(hour.wind_from_deg[cell].is_some_and(|from| from % 10 == 0 && from < 360));
        }
        assert_eq!(hour.time, at_ground.time);
    }
    assert_golden(&golden("thermal_hours_wind_above_ground.json"), &low);

    // A height above the ground the map does not keep, one with a height
    // above the sea, and one of a layer that has none.
    for (path, says) in [
        (
            "/v1/thermals/hours?date=2026-11-22&layer=wind&above_ground=100",
            "above_ground is one of 250",
        ),
        (
            "/v1/thermals/hours?date=2026-11-22&layer=wind&height=1000&above_ground=250",
            "without height",
        ),
        (
            "/v1/thermals/hours?date=2026-11-22&layer=cloud&above_ground=250",
            "goes with layer=wind",
        ),
    ] {
        let message = get(&router, path)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
        assert!(message.contains(says), "{message}");
    }
}

#[tokio::test]
async fn the_regions_are_served_once_as_geojson_with_their_credit() {
    use venti_api_types::v1::{REGIONS_PATH, RegionsResponse};

    let router = recorded().await;
    let answer = get(&router, REGIONS_PATH).await;
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=3600");
    let etag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, REGIONS_PATH, &[(header::IF_NONE_MATCH, &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    let size = answer.body.len();
    let packed = get_with(&router, REGIONS_PATH, &[(header::ACCEPT_ENCODING, "gzip")]).await;
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");
    println!(
        "GET /v1/regions: {size} bytes, {} packed",
        packed.body.len()
    );
    let mut regions: RegionsResponse = answer.ok(StatusCode::OK);
    assert_eq!(regions.kind, "FeatureCollection");
    assert!(regions.version.starts_with("eaws-regions "));
    assert_eq!(regions.features.len(), 459);
    assert_eq!(regions.attributions.len(), 1);
    assert_eq!(regions.attributions[0].text, "Regions: EAWS (CC0)");
    assert_eq!(regions.attributions[0].licence, "CC0 1.0");
    assert!(size < 1_000_000, "{size}");
    // The golden file and the documentation's example: the first two.
    regions.features.truncate(2);
    assert_golden(&golden("regions.json"), &regions);
}

#[tokio::test]
async fn the_development_mode_serves_the_forecast_by_region() {
    use venti_api_types::v1::{
        REGION_FORECAST_PATH, RegionForecastRequest, RegionForecastResponse,
    };

    let router = recorded().await;
    let map: ThermalMapResponse = get(&router, THERMALS_PATH).await.ok(StatusCode::OK);
    let tomorrow = map.days[1].date;
    let asked = RegionForecastRequest {
        date: Some(tomorrow),
    }
    .path_and_query();
    assert_eq!(asked, "/v1/regions/forecast?date=2026-11-22");
    let forecast: RegionForecastResponse = get(&router, &asked).await.ok(StatusCode::OK);
    assert_eq!(forecast.date, tomorrow);
    let dates: Vec<_> = map.days.iter().map(|day| day.date).collect();
    assert_eq!(forecast.dates, dates);
    assert_eq!(Some(&forecast.model), map.days[1].model.as_ref());
    assert!(forecast.regions_version.starts_with("eaws-regions "));
    let credits: Vec<&str> = forecast
        .attributions
        .iter()
        .map(|credit| credit.text.as_str())
        .collect();
    assert!(credits[0].contains("Open-Meteo.com"), "{credits:?}");
    assert_eq!(credits[1], "Regions: EAWS (CC0)");
    // The four recorded cells lie in the Prealps either side of the
    // border at the Achensee: their regions have a level for the day and
    // for each hour that the map has.
    assert!(!forecast.day.is_empty());
    assert!(
        forecast
            .day
            .iter()
            .all(|reading| reading.id.starts_with("DE-BY-") || reading.id.starts_with("AT-07-"))
    );
    let hours: ThermalHoursResponse = get(&router, "/v1/thermals/hours?date=2026-11-22")
        .await
        .ok(StatusCode::OK);
    let times: Vec<_> = forecast.hours.iter().map(|hour| hour.time).collect();
    let map_times: Vec<_> = hours.hours.iter().map(|hour| hour.time).collect();
    assert_eq!(times, map_times);
    assert_golden(&golden("region_forecast.json"), &forecast);

    // Without a day: today by the map's clock, the 21st.
    let first: RegionForecastResponse = get(&router, REGION_FORECAST_PATH).await.ok(StatusCode::OK);
    assert_eq!(first.date, map.days[0].date);
    // A day the map does not have.
    let answer = get(&router, "/v1/regions/forecast?date=2026-12-24").await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    let answer = get(&router, "/v1/regions/forecast?date=tomorrow").await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidDay);
}
