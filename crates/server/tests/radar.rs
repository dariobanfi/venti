//! Tests of the rain radar's endpoints through the real router.
//!
//! The development mode collects the frames recorded on 2026-11-21 from
//! the first region's two radar sources, DWD's open-data server and
//! GeoSphere Austria's dataset service, two of each and all within two
//! minutes, by the code a real server runs; its list of them is kept as a
//! golden file, which is also the example in `docs/api.md`. A frame's picture is held against what the adapter made
//! of the recorded file, pixel for pixel.

use axum::http::{StatusCode, header};
use venti_api_types::v1::{ErrorCode, HEALTH_PATH, HealthResponse, RADAR_PATH, RadarResponse};
use venti_domain::{BoundingBox, RADAR_UNSEEN, RadarGrid};
use venti_server::adapters;
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden};
use venti_sources::{PlanCtx, RadarWanted, RunOptions, run_radar};
use venti_sources_dwd_radar::{DwdRadarRy, fixtures};

mod common;

use common::{TestClock, get, get_with, golden, recorded, server, settings};

#[tokio::test]
async fn the_development_mode_lists_the_recorded_frames() {
    let router = recorded().await;

    let answer = get(&router, RADAR_PATH).await;
    let radar: RadarResponse = answer.ok(StatusCode::OK);

    // A list may be kept for a minute, like every answer that is the same
    // for everyone, and is not sent again to a client that has it.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    let etag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, RADAR_PATH, &[(header::IF_NONE_MATCH, &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);

    // The radar's clock stands shortly after the recording, three days
    // after the clock of the observations.
    assert_eq!(radar.time, adapters::radar_recorded_at());
    assert_eq!(radar.steps_mm_h, [0.1, 0.2, 1.0, 4.0, 10.0, 30.0]);
    assert_eq!(radar.unseen, 7);

    assert_eq!(radar.layers.len(), 2);
    let layer = &radar.layers[0];
    assert_eq!(layer.source.as_str(), "dwd-radar-ry");
    assert_eq!(layer.cadence_seconds, 300);
    // The first region's box, on squares of the map an eightieth of a
    // degree wide; the last row reaches a little beyond its south.
    assert_eq!((layer.west, layer.north, layer.east), (9.4, 48.5, 17.2));
    assert!(layer.south < 46.3 && layer.south > 46.29, "{}", layer.south);
    assert_eq!((layer.columns, layer.rows), (624, 261));

    // Two frames, the older first, each with where its picture is.
    let [(older, _), (newer, _)] = fixtures::frames();
    let times: Vec<_> = layer.frames.iter().map(|frame| frame.time).collect();
    assert_eq!(times, [older, newer]);
    assert_eq!(
        layer.frames[1].path,
        "/v1/radar/dwd-radar-ry/20261121T1315Z.png"
    );
    assert!(layer.frames.iter().all(|frame| frame.interval_s == 300));
    // A picture of the first region on a day of light rain is some five
    // kilobytes.
    for frame in &layer.frames {
        assert!((3_000..7_000).contains(&frame.bytes), "{}", frame.bytes);
    }

    // The credit says that the data was changed, in DWD's own words for it.
    assert_eq!(radar.attributions.len(), 2);
    assert_eq!(radar.attributions[0].licence, "CC BY 4.0");
    assert_eq!(
        radar.attributions[0].text,
        "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben"
    );

    assert_golden(&golden("radar.json"), &radar);
}

#[tokio::test]
async fn a_frames_picture_holds_the_steps_the_adapter_made() {
    let router = recorded().await;
    let radar: RadarResponse = get(&router, RADAR_PATH).await.ok(StatusCode::OK);
    let listed = &radar.layers[0].frames[1];

    let answer = get(&router, &listed.path).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "image/png");
    // A frame of a moment does not change.
    assert_eq!(
        answer.header(header::CACHE_CONTROL),
        "public, max-age=86400, immutable"
    );
    assert_eq!(answer.body.len(), listed.bytes as usize);

    // The picture, read as any app reads a PNG: a byte a pixel.
    let decoder = png::Decoder::new(std::io::Cursor::new(&answer.body));
    let mut reader = decoder.read_info().unwrap();
    let info = reader.info().clone();
    assert_eq!((info.width, info.height), (624, 261));
    assert_eq!(info.color_type, png::ColorType::Indexed);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let mut pixels = vec![0u8; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();

    // What the adapter makes of the recorded file, by the pipeline.
    let area = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
    let ctx = PlanCtx {
        now: fixtures::recorded_at(),
        area,
    };
    let wanted = RadarWanted {
        after: fixtures::held_until(),
        grid: RadarGrid::over(&area).unwrap(),
    };
    let output = run_radar(
        &DwdRadarRy::new(),
        &FixtureFetch::new().with_all(fixtures::recorded()),
        &MemoryArchive::default(),
        &ctx,
        &RunOptions::default(),
        &wanted,
    )
    .await;
    assert_eq!(pixels, output.frames[1].cells);
    // 8,030 cells of rain, and more than half of the box not seen.
    let count = |value: u8| pixels.iter().filter(|pixel| **pixel == value).count();
    assert_eq!(count(1) + count(2) + count(3) + count(4), 8_030);
    assert_eq!(count(RADAR_UNSEEN), 92_979);
}

#[tokio::test]
async fn what_is_not_a_frame_the_server_holds_is_not_found() {
    let router = recorded().await;

    for path in [
        // A frame the recording does not hold, and one of another day.
        "/v1/radar/dwd-radar-ry/20261121T1305Z.png",
        "/v1/radar/dwd-radar-ry/20251121T1315Z.png",
        // Not the name of a frame.
        "/v1/radar/dwd-radar-ry/20261121T1315Z",
        "/v1/radar/dwd-radar-ry/20261121T1315Z.jpg",
        "/v1/radar/dwd-radar-ry/latest.png",
        "/v1/radar/dwd-radar-ry/2026-11-21T13:15:00Z.png",
        "/v1/radar/dwd-radar-ry/..%2F..%2Fhealth.png",
        // Not a radar source, and not a source at all.
        "/v1/radar/dwd-10min/20261121T1315Z.png",
        "/v1/radar/nobody/20261121T1315Z.png",
        "/v1/radar/Not%20A%20Source/20261121T1315Z.png",
    ] {
        let answer = get(&router, path).await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    }
}

#[tokio::test]
async fn the_health_report_has_both_sources_each_with_its_newest_frames_age() {
    let router = recorded().await;

    let health: HealthResponse = get(&router, HEALTH_PATH).await.ok(StatusCode::OK);

    let radar: Vec<(&str, u32, Option<i64>)> = health
        .radar
        .iter()
        .map(|radar| {
            (
                radar.source.as_str(),
                radar.frames,
                radar.newest_frame_age_s,
            )
        })
        .collect();
    // The frame of 13:15 and the run of 13:00, by the radar's clock at
    // 13:19.
    assert_eq!(
        radar,
        [
            ("dwd-radar-ry", 2, Some(240)),
            ("geosphere-inca-rr", 2, Some(1140))
        ]
    );
    assert_eq!(health.radar[0].newest_frame, Some(fixtures::frames()[1].0));
    for radar in &health.radar {
        assert!((6_000..14_000).contains(&radar.bytes), "{}", radar.bytes);
    }
    // Each source is among the sources like every other, with its runs,
    // and neither is behind: each is judged by its own cadence.
    for (name, cadence) in [("dwd-radar-ry", 300), ("geosphere-inca-rr", 900)] {
        let source = health
            .sources
            .iter()
            .find(|source| source.source.as_str() == name)
            .expect("the radar source has an entry");
        assert_eq!(source.cadence_seconds, cadence);
        assert!(!source.stale, "{name}");
        assert!(source.last_success.is_some(), "{name}");
    }
}

#[tokio::test]
async fn a_server_without_a_radar_source_says_so() {
    let (router, _) = server(settings(), &TestClock::new());

    let answer = get(&router, RADAR_PATH).await;
    let message = answer.error(StatusCode::NOT_IMPLEMENTED, ErrorCode::NoRadar);
    assert!(message.contains("collects no rain radar"), "{message}");

    let frame = get(&router, "/v1/radar/dwd-radar-ry/20261121T1315Z.png").await;
    frame.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);

    let health: HealthResponse = get(&router, HEALTH_PATH).await.ok(StatusCode::OK);
    assert!(health.radar.is_empty());
}

#[tokio::test]
async fn two_sources_are_two_layers_each_with_its_frames_and_its_credit() {
    let router = recorded().await;

    let radar: RadarResponse = get(&router, RADAR_PATH).await.ok(StatusCode::OK);

    // Sorted by source, on the same grid, each with its own cadence.
    let layers: Vec<(&str, u32, u32, u32)> = radar
        .layers
        .iter()
        .map(|layer| {
            (
                layer.source.as_str(),
                layer.cadence_seconds,
                layer.columns,
                layer.rows,
            )
        })
        .collect();
    assert_eq!(
        layers,
        [
            ("dwd-radar-ry", 300, 624, 261),
            ("geosphere-inca-rr", 900, 624, 261)
        ]
    );
    let analysis = &radar.layers[1];
    let paths: Vec<&str> = analysis.frames.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/v1/radar/geosphere-inca-rr/20261121T1245Z.png",
            "/v1/radar/geosphere-inca-rr/20261121T1300Z.png"
        ]
    );
    assert!(analysis.frames.iter().all(|frame| frame.interval_s == 900));
    assert_eq!(radar.layers[0].frames.len(), 2);

    // Each source's own credit, in its own words.
    let credits: Vec<(&str, &str)> = radar
        .attributions
        .iter()
        .map(|credit| (credit.source.as_str(), credit.text.as_str()))
        .collect();
    assert_eq!(
        credits,
        [
            (
                "dwd-radar-ry",
                "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben"
            ),
            (
                "geosphere-inca-rr",
                "Data source: GeoSphere Austria - https://data.hub.geosphere.at"
            )
        ]
    );
}

#[tokio::test]
async fn the_two_pictures_are_never_one_and_together_see_all_of_the_region() {
    let router = recorded().await;
    let radar: RadarResponse = get(&router, RADAR_PATH).await.ok(StatusCode::OK);

    // The newest picture of each layer, a byte a pixel.
    let mut pictures = Vec::new();
    for layer in &radar.layers {
        let answer = get(&router, &layer.frames[1].path).await;
        assert_eq!(answer.status, StatusCode::OK);
        let decoder = png::Decoder::new(std::io::Cursor::new(&answer.body));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0u8; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        pictures.push(pixels);
    }
    let (radars, analysis) = (&pictures[0], &pictures[1]);
    let cells = 624 * 261;
    assert_eq!((radars.len(), analysis.len()), (cells, cells));

    // How much of the first region's box each source sees, and both: the
    // German radars 42.9 %, the analysis all of it, asked for as the
    // adapter asks, with the box widened.
    let seen = |pixels: &[u8]| pixels.iter().filter(|p| **p != RADAR_UNSEEN).count();
    let both = (0..cells)
        .filter(|at| radars[*at] != RADAR_UNSEEN && analysis[*at] != RADAR_UNSEEN)
        .count();
    let either = (0..cells)
        .filter(|at| radars[*at] != RADAR_UNSEEN || analysis[*at] != RADAR_UNSEEN)
        .count();
    assert_eq!(
        (cells, seen(radars), seen(analysis), both, either),
        (162_864, 69_885, 162_864, 69_885, 162_864)
    );
    // They are two pictures of two moments by two methods: where both
    // see, they differ at some thousands of cells, and neither is made of
    // the other.
    let differ = (0..cells)
        .filter(|at| radars[*at] != RADAR_UNSEEN && radars[*at] != analysis[*at])
        .count();
    assert!(differ > 1000, "{differ}");
}
