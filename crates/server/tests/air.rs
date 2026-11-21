//! Tests of what many gliders say together, through the real router: the
//! climbs of the last minutes, the pilots in the air at flying sites, the
//! alarm about them, and where the public flights' thermals have been.
//!
//! The development mode's gliders and flights are made up; its answers
//! for the Tegernsee are kept as golden files, which are also the
//! examples in `docs/api.md`.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ALERTS_FIRED_PATH, ALERTS_PATH, AlertRequest, AlertRule, ErrorCode, FLOWN_MIN_PILOTS,
    FiredAlertsResponse, FiredBecause, FlownThermalsResponse, LIVE_POSITIONS_PATH,
    LiveClimbsResponse, LiveFix, LivePositionsRequest, LiveSitesResponse, LiveTrackId,
    LiveVisibility, PilotsAlert, PushMessage, VENTI_LIVE_SOURCE, live_track_path,
};
use venti_domain::{GeoPoint, Timestamp};
use venti_server::adapters;
use venti_server::alerts::Checks;
use venti_server::clock::Clock;
use venti_server::live::air::made_up::{circling, feed, gliding};
use venti_server::live::air::{Sample, Trails};
use venti_sources::testing::assert_golden;

mod common;

use common::{TestClock, ask, get, golden, recorded, server, settings, sign_up};

const TEGERNSEE: &str = "bbox=11.6,47.55,12.0,47.85";

#[tokio::test]
async fn the_development_mode_has_climbs_over_the_tegernsee() {
    let router = recorded().await;
    let answer = get(&router, &format!("/v1/live/climbs?{TEGERNSEE}")).await;
    let climbs: LiveClimbsResponse = answer.ok(StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=5");
    assert_eq!(climbs.time, adapters::live_feed_at());
    assert_eq!((climbs.listed_minutes, climbs.cell_deg), (10, 0.005));

    // The network's layer under its credit, then Venti's own.
    let sources: Vec<&str> = climbs.layers.iter().map(|l| l.source.as_str()).collect();
    assert_eq!(sources, ["ogn", VENTI_LIVE_SOURCE]);
    assert_eq!(climbs.attributions.len(), 1);
    assert_eq!(climbs.attributions[0].licence, "ODbL 1.0");

    let network = &climbs.layers[0];
    // South of the lake two circle now; east of the Wallberg three did
    // until four minutes ago.
    assert_eq!(network.climbs.len(), 2);
    let (now, before) = (&network.climbs[0], &network.climbs[1]);
    assert_eq!((now.pilots, before.pilots), (2, 3));
    assert!(now.age_s < 10 && (235..=245).contains(&before.age_s));
    assert_eq!(
        (before.climb_ms, before.from_m, before.to_m),
        (2.4, 1600, 2100)
    );
    // A climb is a place in the air and numbers: no address, no name.
    let json = serde_json::to_string(&climbs).unwrap();
    assert!(!json.contains("E000") && !json.contains("\"id\""), "{json}");

    assert_golden(&golden("live_climbs.json"), &climbs);

    // A box without them, a box too large, no box.
    let empty: LiveClimbsResponse = get(&router, "/v1/live/climbs?bbox=9.5,46.4,10.0,46.8")
        .await
        .ok(StatusCode::OK);
    assert!(empty.layers.iter().all(|layer| layer.climbs.is_empty()));
    for path in [
        "/v1/live/climbs?bbox=0,40,20,50",
        "/v1/live/climbs",
        "/v1/live/sites",
    ] {
        get(&router, path)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
    }
}

#[tokio::test]
async fn the_development_mode_counts_the_pilots_in_the_air_at_its_sites() {
    let router = recorded().await;
    let answer = get(&router, &format!("/v1/live/sites?{TEGERNSEE}")).await;
    let sites: LiveSitesResponse = answer.ok(StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=5");
    assert_eq!((sites.within_m, sites.took_off_minutes), (5000.0, 60));
    let network = &sites.layers[0];
    assert_eq!(network.source, "ogn");
    assert!(!network.sites.is_empty());
    // The most pilots first; numbers and the site's identifier only.
    assert!(
        network
            .sites
            .windows(2)
            .all(|pair| pair[0].airborne >= pair[1].airborne)
    );
    assert!(network.sites[0].airborne >= 3, "{:?}", network.sites[0]);
    assert!(
        network.sites.iter().any(|site| site.took_off >= 1),
        "{network:?}"
    );
    assert_golden(&golden("live_sites.json"), &sites);
}

#[tokio::test]
async fn the_development_mode_has_cells_where_three_pilots_climbed() {
    let router = recorded().await;
    let answer = get(&router, &format!("/v1/thermals/flown?{TEGERNSEE}")).await;
    let flown: FlownThermalsResponse = answer.ok(StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert_eq!(
        (flown.min_pilots, flown.cell_deg),
        (FLOWN_MIN_PILOTS, 0.005)
    );
    assert!(!flown.cells.is_empty() && !flown.more);
    assert!(flown.flights >= 3);
    assert!(
        flown
            .cells
            .iter()
            .all(|cell| cell.pilots >= FLOWN_MIN_PILOTS)
    );
    assert!(flown.cells.iter().all(|cell| cell.climbs >= cell.pilots));
    assert!(!flown.terms.decided);
    // No pilot and no flight.
    let json = serde_json::to_string(&flown).unwrap();
    assert!(
        !json.contains("handle") && !json.contains("\"id\""),
        "{json}"
    );
    assert_golden(&golden("thermals_flown.json"), &flown);

    // The month and the hours narrow it, and the guard is applied after
    // them: a month in which fewer than three pilots climbed has no cell.
    let one: FlownThermalsResponse = get(
        &router,
        &format!("/v1/thermals/flown?{TEGERNSEE}&month=1&hours=11-15"),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!((one.month, one.hours), (Some(1), Some([11, 15])));
    assert!(one.cells.is_empty());

    for bad in ["month=13", "month=july", "hours=15-11", "hours=noon"] {
        get(&router, &format!("/v1/thermals/flown?{TEGERNSEE}&{bad}"))
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidTime);
    }
    get(&router, "/v1/thermals/flown?bbox=10,46,12,48")
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidBbox);
}

fn fixes(samples: &[Sample]) -> Vec<LiveFix> {
    samples
        .iter()
        .map(|sample| LiveFix {
            time: sample.time,
            position: sample.position,
            altitude_m: sample.altitude_m.round() as i32,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        })
        .collect()
}

async fn send(
    router: &Router,
    token: &str,
    track: &str,
    visibility: LiveVisibility,
    samples: &[Sample],
) {
    let body = LivePositionsRequest {
        id: track.to_owned(),
        visibility,
        landed: false,
        fixes: fixes(samples),
    };
    let answer = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(token)
        .json(&body)
        .send(router)
        .await;
    assert!(answer.status.is_success(), "{}", answer.text());
}

fn track(n: u8) -> String {
    LiveTrackId::from_random(&[n; 26]).as_str().to_owned()
}

#[tokio::test]
async fn ventis_own_climbs_come_from_public_live_tracks_only() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let now = clock.now();
    let centre = GeoPoint::new(47.7012, 11.7989).unwrap();
    let thermal = |began: i64, from_m: f64| {
        circling(
            centre,
            now - Duration::seconds(began),
            160,
            4,
            20.0,
            from_m,
            2.0,
        )
    };
    let anna = sign_up(&router, "anna").await.token;
    let ben = sign_up(&router, "ben").await.token;
    let cara = sign_up(&router, "cara").await.token;
    send(
        &router,
        &anna,
        &track(1),
        LiveVisibility::Public,
        &thermal(170, 1500.0),
    )
    .await;
    send(
        &router,
        &ben,
        &track(2),
        LiveVisibility::Public,
        &thermal(200, 1600.0),
    )
    .await;
    // Cara shows her flight to those with the link: it marks nothing.
    send(
        &router,
        &cara,
        &track(3),
        LiveVisibility::Link,
        &thermal(180, 1700.0),
    )
    .await;

    let path = "/v1/live/climbs?bbox=11.6,47.55,12.0,47.85";
    let climbs: LiveClimbsResponse = get(&router, path).await.ok(StatusCode::OK);
    let [venti] = &climbs.layers[..] else {
        panic!("a server without a network has Venti's layer only");
    };
    assert_eq!(venti.source, VENTI_LIVE_SOURCE);
    assert!(climbs.attributions.is_empty());
    assert_eq!(venti.climbs.len(), 1);
    assert_eq!(venti.climbs[0].pilots, 2);
    assert_eq!((venti.climbs[0].from_m, venti.climbs[0].to_m), (1500, 1900));
    assert_eq!(venti.climbs[0].climb_ms, 2.0);
    // The middle of the cell: no track's place.
    let place = venti.climbs[0].position;
    assert_eq!((place.lat(), place.lon()), (47.7025, 11.7975));

    // A pilot who stops showing the flight to everyone is out of it with
    // the next fixes, and one who deletes the track at once.
    let more = [Sample {
        time: now,
        position: centre,
        altitude_m: 1900.0,
    }];
    send(&router, &ben, &track(2), LiveVisibility::Link, &more).await;
    let climbs: LiveClimbsResponse = get(&router, path).await.ok(StatusCode::OK);
    assert_eq!(climbs.layers[0].climbs[0].pilots, 1);
    let deleted = ask(Method::DELETE, &live_track_path(&track(1).parse().unwrap()))
        .token(&anna)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let climbs: LiveClimbsResponse = get(&router, path).await.ok(StatusCode::OK);
    assert!(climbs.layers[0].climbs.is_empty());
}

#[tokio::test]
async fn a_pilots_alarm_fires_when_enough_fly_at_the_place_and_then_rests() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;
    let place = GeoPoint::new(47.6640, 11.7480).unwrap();
    let body = AlertRequest {
        name: "Wallberg is on".to_owned(),
        enabled: true,
        rule: AlertRule::Pilots(PilotsAlert {
            place,
            site: None,
            place_name: "Wallberg".to_owned(),
            min_pilots: 3,
            rest_hours: 6,
        }),
    };
    let added = ask(Method::POST, ALERTS_PATH)
        .token(&pilot)
        .json(&body)
        .send(&router)
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    assert_golden(&golden("alert_pilots_request.json"), &body);

    let checks = Checks::new(store.clone(), Arc::new(clock.clone()));
    let network = Trails::default();
    let venti = Trails::default();
    let flying = |trails: &Trails, key: &str, at: Timestamp, north: f64| {
        let from = GeoPoint::new(47.6640 + north, 11.7480).unwrap();
        feed(
            trails,
            key,
            &gliding(from, at - Duration::seconds(120), 120, 4, 9.0, 1700.0),
        );
    };
    let sources = |at: Timestamp| {
        vec![
            (at, network.flyers(at, |_| true)),
            (at, venti.flyers(at, |_| true)),
        ]
    };
    let now = clock.now();
    // Nobody; then two by the network and two by Venti: no source has
    // three, and the two are not added up.
    assert_eq!(checks.after_air(&sources(now)).await, 0);
    flying(&network, "a", now, 0.0);
    flying(&network, "b", now, 0.01);
    flying(&venti, "v1", now, 0.0);
    flying(&venti, "v2", now, 0.005);
    assert_eq!(checks.after_air(&sources(now)).await, 0);
    // A third the network hears, one of the three having taken off
    // there; one far away changes nothing.
    flying(&network, "c", now, 0.02);
    flying(&network, "far", now, 0.2);
    assert_eq!(checks.after_air(&sources(now)).await, 1);
    let mut fired: FiredAlertsResponse = ask(Method::GET, ALERTS_FIRED_PATH)
        .token(&pilot)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(fired.fired.len(), 1);
    // The alarm's identifier is drawn at random: a fixed one for the file.
    fired.fired[0].alert_id = "0000000000000000000000a003".to_owned();
    assert_eq!(
        fired.fired[0].because,
        FiredBecause::Pilots {
            time: now,
            pilots: 3,
            took_off: 1,
        }
    );
    assert_golden(&golden("alerts_fired_pilots.json"), &fired);
    // The words a browser is told.
    let message = PushMessage::of(&fired.fired[0]);
    assert_eq!(message.title, "Wallberg is on");
    assert!(
        message
            .line
            .starts_with("3 pilots are in the air at Wallberg at "),
        "{}",
        message.line
    );
    assert!(message.line.ends_with(", 1 took off in the last hour"));

    // It rests six hours, however many fly.
    clock.advance(Duration::hours(5));
    let later = clock.now();
    for key in ["a", "b", "c"] {
        flying(&network, key, later, 0.01);
    }
    assert_eq!(checks.after_air(&sources(later)).await, 0);
    clock.advance(Duration::hours(1));
    let again = clock.now();
    for key in ["a", "b", "c"] {
        flying(&network, key, again, 0.01);
    }
    assert_eq!(checks.after_air(&sources(again)).await, 1);
}
