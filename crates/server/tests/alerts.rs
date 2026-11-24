//! Tests of the alarms a pilot sets on the account, through the real
//! router, and of the checks that fire them where the data arrives.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::{Duration, NaiveDate};
use venti_api_types::v1::{
    ALERTS_PATH, AlertHours, AlertRequest, AlertResponse, AlertRule, AlertSector, AlertsResponse,
    DayAlert, DayAtLeast, ErrorCode, FiredAlertsRequest, FiredAlertsResponse, FiredBecause,
    MAX_ALERTS, ThermalGrid, ThermalKind, ThermalMapDay, ThermalMapResponse, ThermalVerdict,
    WindAlert, alert_path,
};
use venti_domain::{GeoPoint, Observation, Timestamp};
use venti_server::alerts::Checks;
use venti_server::clock::Clock;
use venti_server::dev::{DEMO_ALERTS, DEMO_TOKEN};
use venti_sources::testing::assert_golden;

mod common;

use common::{Answer, TestClock, ask, golden, recorded, server, settings, sign_up};

const STATION: &str = "dwd-10min:05792";

fn wind_rule() -> WindAlert {
    WindAlert {
        station: STATION.parse().unwrap(),
        station_name: "Zugspitze".to_owned(),
        directions: Some(AlertSector {
            from_deg: 349,
            to_deg: 56,
        }),
        min_kmh: Some(10),
        max_kmh: Some(20),
        gusts_max_kmh: Some(30),
        hours: None,
    }
}

fn day_rule() -> DayAlert {
    DayAlert {
        place: GeoPoint::new(47.6, 11.5).unwrap(),
        site: None,
        place_name: "Brauneck".to_owned(),
        days_ahead: vec![0, 1, 2, 3],
        at_least: DayAtLeast::CrossCountry,
    }
}

fn request(name: &str, rule: AlertRule) -> AlertRequest {
    AlertRequest {
        name: name.to_owned(),
        enabled: true,
        rule,
    }
}

async fn try_add(router: &Router, token: &str, body: &AlertRequest) -> Answer {
    ask(Method::POST, ALERTS_PATH)
        .token(token)
        .json(body)
        .send(router)
        .await
}

async fn add(router: &Router, token: &str, body: &AlertRequest) -> AlertResponse {
    let answer = try_add(router, token, body).await;
    answer.is_private();
    answer.ok(StatusCode::CREATED)
}

async fn list(router: &Router, token: &str) -> AlertsResponse {
    let answer = ask(Method::GET, ALERTS_PATH)
        .token(token)
        .send(router)
        .await;
    answer.is_private();
    answer.ok(StatusCode::OK)
}

async fn fired(router: &Router, token: &str, since: Option<&str>) -> FiredAlertsResponse {
    let request = FiredAlertsRequest {
        since: since.map(str::to_owned),
        limit: Some(100),
    };
    let answer = ask(Method::GET, &request.path_and_query())
        .token(token)
        .send(router)
        .await;
    answer.is_private();
    answer.ok(StatusCode::OK)
}

#[tokio::test]
async fn a_pilot_adds_changes_and_removes_alarms() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;

    assert_eq!(list(&router, &pilot).await.alerts, []);
    let wind = add(
        &router,
        &pilot,
        &request("Zugspitze north", AlertRule::Wind(wind_rule())),
    )
    .await
    .alert;
    assert_eq!(
        (wind.name.as_str(), wind.enabled, wind.created_at),
        ("Zugspitze north", true, clock.now())
    );
    clock.advance(Duration::minutes(1));
    let day = add(
        &router,
        &pilot,
        &request("Brauneck XC", AlertRule::Day(day_rule())),
    )
    .await
    .alert;
    assert_eq!(
        list(&router, &pilot).await.alerts,
        [wind.clone(), day.clone()]
    );

    // Changed: off, and another name.
    clock.advance(Duration::minutes(1));
    let mut change = request("Zugspitze, off", AlertRule::Wind(wind_rule()));
    change.enabled = false;
    let changed: AlertResponse = ask(Method::PUT, &alert_path(&wind.id))
        .token(&pilot)
        .json(&change)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(
        (
            changed.alert.name.as_str(),
            changed.alert.enabled,
            changed.alert.created_at,
            changed.alert.changed_at
        ),
        ("Zugspitze, off", false, wind.created_at, clock.now())
    );

    // Another pilot's alarm is none of this one's.
    let other = sign_up(&router, "anna").await.token;
    assert_eq!(list(&router, &other).await.alerts, []);
    ask(Method::PUT, &alert_path(&wind.id))
        .token(&other)
        .json(&change)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::AlertNotFound);

    let removed = ask(Method::DELETE, &alert_path(&wind.id))
        .token(&pilot)
        .send(&router)
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(list(&router, &pilot).await.alerts, [day]);
}

#[tokio::test]
async fn an_alarm_that_cannot_be_checked_is_refused_and_the_list_is_bounded() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;

    let mut slow = wind_rule();
    slow.min_kmh = Some(30);
    let message = try_add(&router, &pilot, &request("Odd", AlertRule::Wind(slow)))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidAlert);
    assert!(message.starts_with("rule.max_kmh"), "{message}");
    let mut late = day_rule();
    late.days_ahead = vec![5];
    try_add(&router, &pilot, &request("Odd", AlertRule::Day(late)))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidAlert);

    for n in 0..MAX_ALERTS {
        add(
            &router,
            &pilot,
            &request(&format!("Alarm {n}"), AlertRule::Day(day_rule())),
        )
        .await;
    }
    try_add(
        &router,
        &pilot,
        &request("One more", AlertRule::Day(day_rule())),
    )
    .await
    .error(StatusCode::CONFLICT, ErrorCode::TooManyAlerts);
}

fn reading(at: Timestamp, speed_ms: f64, gust_ms: f64, direction: f64) -> Observation {
    Observation {
        wind_speed_ms: Some(speed_ms),
        wind_gust_ms: Some(gust_ms),
        wind_direction_deg: Some(direction),
        ..Observation::empty(STATION.parse().unwrap(), at)
    }
}

#[tokio::test]
async fn a_wind_alarm_fires_for_a_fresh_reading_that_fits_and_then_rests() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;
    add(
        &router,
        &pilot,
        &request("Zugspitze north", AlertRule::Wind(wind_rule())),
    )
    .await;
    let checks = Checks::new(store.clone(), Arc::new(clock.clone()));
    let now = clock.now();

    // Too strong, from the east, and too old: nothing.
    assert_eq!(
        checks.after_readings(&[reading(now, 7.0, 7.5, 10.0)]).await,
        0
    );
    assert_eq!(
        checks.after_readings(&[reading(now, 4.0, 6.0, 90.0)]).await,
        0
    );
    assert_eq!(
        checks
            .after_readings(&[reading(now - Duration::hours(2), 4.0, 6.0, 10.0)])
            .await,
        0
    );
    // Of a run's readings the newest counts: 15 km/h from 10°, gusts 22.
    let older = reading(now - Duration::minutes(10), 7.0, 8.0, 10.0);
    let newest = reading(now, 4.2, 6.1, 10.0);
    assert_eq!(checks.after_readings(&[newest.clone(), older]).await, 1);
    let first = fired(&router, &pilot, None).await;
    assert_eq!(first.fired.len(), 1);
    assert_eq!(
        first.fired[0].because,
        FiredBecause::Wind {
            time: now,
            speed_kmh: 15,
            gust_kmh: Some(22),
            direction_deg: Some(10),
        }
    );
    assert_eq!(first.fired[0].alert_name, "Zugspitze north");

    // It rests three hours from the reading that fired it.
    clock.advance(Duration::hours(2));
    let later = reading(clock.now(), 4.2, 6.1, 10.0);
    assert_eq!(checks.after_readings(&[later]).await, 0);
    clock.advance(Duration::hours(1));
    let again = reading(clock.now(), 4.2, 6.1, 10.0);
    assert_eq!(checks.after_readings(&[again]).await, 1);

    // From the cursor, only the new one.
    let next = fired(&router, &pilot, Some(&first.cursor)).await;
    assert_eq!((next.fired.len(), next.reset), (1, false));
    let beyond = fired(&router, &pilot, Some("99999999999999999")).await;
    assert_eq!((beyond.fired.len(), beyond.reset), (2, true));
}

fn map_of(
    days: &[(NaiveDate, ThermalVerdict, Option<ThermalKind>)],
    run: Timestamp,
) -> ThermalMapResponse {
    ThermalMapResponse {
        source: venti_domain::SourceId::new("open-meteo").unwrap(),
        model: venti_domain::ModelId::new("icon-d2").unwrap(),
        run,
        computed_at: run,
        grid: ThermalGrid {
            south: 47.5,
            west: 11.4,
            step_deg: 0.1,
            rows: 2,
            columns: 2,
        },
        ground_m: vec![Some(1000); 4],
        heights_m: Vec::new(),
        heights_above_ground_m: Vec::new(),
        days: days
            .iter()
            .map(|(date, verdict, kind)| ThermalMapDay {
                date: *date,
                model: None,
                run: None,
                whole: true,
                // The place lies in cell 3, the north-east.
                verdict: vec![None, None, None, Some(*verdict)],
                why: Vec::new(),
                thermals: vec![None, None, None, *kind],
                climb_ms: vec![None; 4],
                top_m: vec![None; 4],
                cloud: vec![false; 4],
            })
            .collect(),
        attributions: Vec::new(),
    }
}

#[tokio::test]
async fn a_day_alarm_fires_when_a_run_first_says_so_and_again_after_it_stopped() {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    let pilot = sign_up(&router, "dario").await.token;
    let mut rule = day_rule();
    rule.days_ahead = vec![1];
    add(
        &router,
        &pilot,
        &request("Brauneck XC", AlertRule::Day(rule)),
    )
    .await;
    let checks = Checks::new(store.clone(), Arc::new(clock.clone()));
    let today = venti_api_types::v1::alert_day(clock.now());
    let tomorrow = today + Duration::days(1);
    let after = today + Duration::days(2);
    let run = clock.now();
    use ThermalKind::{CrossCountry, Local};
    use ThermalVerdict::Flyable;

    // Tomorrow cross-country; the day after too, which it does not look at.
    let good = map_of(
        &[
            (tomorrow, Flyable, Some(CrossCountry)),
            (after, Flyable, Some(CrossCountry)),
        ],
        run,
    );
    assert_eq!(checks.after_map(&good).await, 1);
    // The next run says the same: nothing new.
    assert_eq!(checks.after_map(&good).await, 0);
    // One says less, and a later one says so again.
    let weaker = map_of(&[(tomorrow, Flyable, Some(Local))], run);
    assert_eq!(checks.after_map(&weaker).await, 0);
    assert_eq!(checks.after_map(&good).await, 1);

    let all = fired(&router, &pilot, None).await;
    assert_eq!(all.fired.len(), 2);
    match &all.fired[0].because {
        FiredBecause::Day {
            date,
            verdict,
            thermals,
            ..
        } => assert_eq!(
            (*date, *verdict, *thermals),
            (tomorrow, Flyable, Some(CrossCountry))
        ),
        other => panic!("expected a day, got {other:?}"),
    }
    // A map whose grid does not hold the place fires nothing.
    let mut elsewhere = good.clone();
    elsewhere.grid.south = 40.0;
    assert_eq!(checks.after_map(&elsewhere).await, 0);
}

#[tokio::test]
async fn the_development_mode_has_an_alarm_of_each_kind_and_one_fired() {
    let router = recorded().await;
    let alerts = list(&router, DEMO_TOKEN).await;
    let ids: Vec<&str> = alerts
        .alerts
        .iter()
        .map(|alert| alert.id.as_str())
        .collect();
    assert_eq!(ids, DEMO_ALERTS);
    assert_golden(&golden("alerts.json"), &alerts);

    let what = fired(&router, DEMO_TOKEN, None).await;
    assert_eq!(what.fired.len(), 1);
    assert_golden(&golden("alerts_fired.json"), &what);

    let AlertRule::Wind(mut rule) = alerts.alerts[0].rule.clone() else {
        panic!("the first demo alarm is a wind alarm");
    };
    rule.hours = Some(AlertHours {
        from_hour: 9,
        to_hour: 17,
    });
    let change = request("Patscherkofel north", AlertRule::Wind(rule));
    assert_golden(&golden("alert_request.json"), &change);
    let changed: AlertResponse = ask(Method::PUT, &alert_path(DEMO_ALERTS[0]))
        .token(DEMO_TOKEN)
        .json(&change)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_golden(&golden("alert.json"), &changed);
}
