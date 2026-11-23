//! Open plans through the real router: posted by a pilot for everyone
//! flying from the area, found near a place by anyone, "I am in" from any
//! signed-in pilot, bounded, reported and removed like a group's plan.

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::{Duration, NaiveDate};
use venti_api_types::v1::{
    ACCOUNT_OPEN_PLANS_PATH, ErrorCode, MAX_OPEN_PLAN_NOTE_LENGTH, OPEN_PLANS_PATH, OpenPlan,
    OpenPlanRequest, OpenPlanResponse, OpenPlansRequest, OpenPlansResponse, PlanId, ReportReason,
    ReportRequest, ReportSubject, SignInResponse, TimeOfDay, open_plan_in_path, open_plan_path,
};
use venti_domain::GeoPoint;
use venti_server::adapters;
use venti_server::api::Settings;
use venti_server::dev::{DEMO_HANDLE, DEMO_OPEN_PLANS, DEMO_PASSWORD, DEMO_TOKEN};
use venti_server::groups::Limits;
use venti_sources::testing::assert_golden;
use venti_store::Store;

mod common;

use common::{
    Answer, TestClock, ask, golden, recorded, recorded_with_its_store, server, settings, sign_up,
    try_sign_in,
};

/// An identifier of the right form that nothing has.
const NOBODYS: &str = "0123456789abcdefghjkmnpqrs";

fn day(of_month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, of_month).unwrap()
}

/// The sample flight's take-off, where the development mode's open plans
/// are.
fn takeoff() -> GeoPoint {
    GeoPoint::new(47.6642, 11.7967).unwrap()
}

fn handles(plan: &OpenPlan) -> Vec<&str> {
    plan.pilots_in
        .iter()
        .map(|pilot| pilot.handle.as_str())
        .collect()
}

fn a_plan(on: NaiveDate) -> OpenPlanRequest {
    OpenPlanRequest {
        day: on,
        place: "Musterberg".to_owned(),
        position: takeoff(),
        meet_at: None,
        seats: None,
        note: None,
    }
}

async fn near(router: &Router, request: &OpenPlansRequest) -> Answer {
    ask(Method::GET, &request.path_and_query())
        .send(router)
        .await
}

async fn try_post(router: &Router, token: &str, plan: &OpenPlanRequest) -> Answer {
    ask(Method::POST, OPEN_PLANS_PATH)
        .token(token)
        .json(plan)
        .send(router)
        .await
}

async fn set_in(router: &Router, token: &str, plan: &PlanId, is_in: bool) -> Answer {
    let method = if is_in { Method::PUT } else { Method::DELETE };
    ask(method, &open_plan_in_path(plan))
        .token(token)
        .send(router)
        .await
}

#[tokio::test]
async fn anyone_reads_the_open_plans_near_a_place() {
    let router = recorded().await;

    // Without a token: the development mode's two, from its day on.
    let request = OpenPlansRequest {
        place: GeoPoint::new(47.70, 11.75).unwrap(),
        radius_km: Some(25),
        from: Some(day(18)),
        to: None,
    };
    let answer = near(&router, &request).await;
    let plans: OpenPlansResponse = answer.ok(StatusCode::OK);
    assert_golden(&golden("open_plans.json"), &plans);
    let ids: Vec<&str> = plans.plans.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(ids, DEMO_OPEN_PLANS);
    assert_eq!(handles(&plans.plans[0]), ["mara", "jonas"]);
    assert_eq!(plans.plans[0].seats, Some(2));
    assert_eq!(plans.plans[1].seats, None);
    assert!(!plans.truncated);
    // Nothing but a handle and a name of anyone on it.
    let json: serde_json::Value = answer.json();
    let author = json["plans"][0]["author"].as_object().unwrap();
    assert_eq!(author.len(), 2, "{author:?}");

    // Not from farther away, and not for other days.
    let far = OpenPlansRequest {
        place: GeoPoint::new(47.70, 11.20).unwrap(),
        ..request
    };
    let none: OpenPlansResponse = near(&router, &far).await.ok(StatusCode::OK);
    assert!(none.plans.is_empty());
    let one_day = OpenPlansRequest {
        from: Some(day(20)),
        to: Some(day(20)),
        ..request
    };
    let one: OpenPlansResponse = near(&router, &one_day).await.ok(StatusCode::OK);
    assert_eq!(one.plans.len(), 1);
    assert_eq!(one.plans[0].id.as_str(), DEMO_OPEN_PLANS[1]);

    // One plan by itself, to anyone.
    let id: PlanId = DEMO_OPEN_PLANS[0].parse().unwrap();
    let shown: OpenPlanResponse = ask(Method::GET, &open_plan_path(&id))
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(shown.plan, plans.plans[0]);
    ask(Method::GET, &format!("{OPEN_PLANS_PATH}/{NOBODYS}"))
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);

    // What is no place, no radius or no span of days is said so.
    for (query, code) in [
        ("?lat=47.7", ErrorCode::InvalidPosition),
        (
            "?lat=47.7&lon=11.8&radius_km=101",
            ErrorCode::InvalidRequest,
        ),
        (
            "?lat=47.7&lon=11.8&from=2026-11-20&to=2026-11-19",
            ErrorCode::InvalidRange,
        ),
        (
            "?lat=47.7&lon=11.8&from=2026-11-18&to=2026-12-18",
            ErrorCode::InvalidRange,
        ),
        ("?lat=47.7&lon=11.8&from=18.11.2026", ErrorCode::InvalidDay),
    ] {
        ask(Method::GET, &format!("{OPEN_PLANS_PATH}{query}"))
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, code);
    }
}

#[tokio::test]
async fn a_pilot_posts_an_open_plan_and_others_say_that_they_are_in() {
    let router = recorded().await;
    let mara: SignInResponse = try_sign_in(&router, "mara", DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);

    // The demo account offers three seats for Saturday. This is the
    // example of the documentation, with the plan's identifier replaced.
    let request = OpenPlanRequest {
        day: day(21),
        place: "Beispielwiese, Parkplatz".to_owned(),
        position: GeoPoint::new(47.6922, 11.7675).unwrap(),
        meet_at: TimeOfDay::new(8, 30),
        seats: Some(3),
        note: Some("A made-up open plan. From the station at half past eight.".to_owned()),
    };
    assert_golden(&golden("open_plan_request.json"), &request);
    let answer = try_post(&router, DEMO_TOKEN, &request).await;
    let posted: OpenPlanResponse = answer.ok(StatusCode::CREATED);
    answer.is_private();
    let plan = posted.plan;
    assert_eq!(
        (plan.day, &plan.place, plan.position),
        (day(21), &request.place, request.position)
    );
    assert_eq!((plan.meet_at, plan.seats), (request.meet_at, Some(3)));
    assert_eq!(plan.author.handle.as_str(), DEMO_HANDLE);
    assert_eq!(plan.created_at, adapters::recorded_at());
    assert_eq!(handles(&plan), [DEMO_HANDLE]);
    let example = OpenPlanResponse {
        plan: OpenPlan {
            id: NOBODYS.parse().unwrap(),
            ..plan.clone()
        },
    };
    assert_golden(&golden("open_plan.json"), &example);

    // Mara is in, and on her own list; again changes nothing.
    for _ in 0..2 {
        let with_mara: OpenPlanResponse = set_in(&router, &mara.token, &plan.id, true)
            .await
            .ok(StatusCode::OK);
        assert_eq!(handles(&with_mara.plan), [DEMO_HANDLE, "mara"]);
    }
    let hers: OpenPlansResponse = ask(Method::GET, ACCOUNT_OPEN_PLANS_PATH)
        .token(&mara.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let ids: Vec<&str> = hers.plans.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(ids, [DEMO_OPEN_PLANS[0], plan.id.as_str()]);
    let back: OpenPlanResponse = set_in(&router, &mara.token, &plan.id, false)
        .await
        .ok(StatusCode::OK);
    assert_eq!(handles(&back.plan), [DEMO_HANDLE]);

    // Only its author changes or deletes it; a change keeps who is in.
    let changed = OpenPlanRequest {
        seats: None,
        ..request.clone()
    };
    ask(Method::PUT, &open_plan_path(&plan.id))
        .token(&mara.token)
        .json(&changed)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotPlanAuthor);
    let now: OpenPlanResponse = ask(Method::PUT, &open_plan_path(&plan.id))
        .token(DEMO_TOKEN)
        .json(&changed)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(now.plan.seats, None);
    ask(Method::DELETE, &open_plan_path(&plan.id))
        .token(&mara.token)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotPlanAuthor);
    let gone = ask(Method::DELETE, &open_plan_path(&plan.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    set_in(&router, &mara.token, &plan.id, true)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);

    // Posting and saying "I am in" need a token; reading does not.
    let answer = ask(Method::POST, OPEN_PLANS_PATH)
        .json(&request)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn open_plans_are_bounded_and_checked() {
    let clock = TestClock::new();
    let (router, _store) = server(
        Settings {
            groups: Limits::default(),
            ..settings()
        },
        &clock,
    );
    let anna = sign_up(&router, "anna").await.token;
    let bernd = sign_up(&router, "bernd").await.token;

    // What a plan cannot say is refused, and repeats nothing it was sent.
    let bad = [
        OpenPlanRequest {
            place: " \n".to_owned(),
            ..a_plan(day(20))
        },
        OpenPlanRequest {
            note: Some("x".repeat(MAX_OPEN_PLAN_NOTE_LENGTH + 1)),
            ..a_plan(day(20))
        },
        OpenPlanRequest {
            seats: Some(9),
            ..a_plan(day(20))
        },
        a_plan(day(16)),
        a_plan(day(18) + Duration::days(31)),
    ];
    for request in &bad {
        let message = try_post(&router, &bernd, request)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan);
        assert!(!message.contains("xxxx"), "{message}");
    }

    // Five at a time.
    for on in 18..23 {
        try_post(&router, &anna, &a_plan(day(on)))
            .await
            .ok::<OpenPlanResponse>(StatusCode::CREATED);
    }
    // Ten posts or changes a day, whatever became of each.
    for _ in 0..5 {
        try_post(&router, &anna, &a_plan(day(24)))
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::TooManyOpenPlans);
    }
    try_post(&router, &anna, &a_plan(day(24)))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);

    // A plan whose day is two days gone is gone from every answer, and so
    // is its place: the next day another one fits.
    clock.advance(Duration::days(3));
    let request = OpenPlansRequest {
        place: takeoff(),
        radius_km: None,
        from: Some(day(18)),
        to: None,
    };
    let left: OpenPlansResponse = near(&router, &request).await.ok(StatusCode::OK);
    let days: Vec<NaiveDate> = left.plans.iter().map(|plan| plan.day).collect();
    assert_eq!(days, [day(19), day(20), day(21), day(22)]);
    try_post(&router, &anna, &a_plan(day(24)))
        .await
        .ok::<OpenPlanResponse>(StatusCode::CREATED);
}

#[tokio::test]
async fn an_open_plan_is_reported_and_whoever_runs_the_server_removes_it() {
    let (router, store) = recorded_with_its_store().await;
    let id: PlanId = DEMO_OPEN_PLANS[0].parse().unwrap();

    // Anyone signed in may report it: everyone reads it.
    let report = ReportRequest {
        subject: ReportSubject::OpenPlan(id.to_string()),
        reason: ReportReason::Spam,
        text: Some("An advertisement, not a plan.".to_owned()),
    };
    assert_golden(&golden("report_open_plan_request.json"), &report);
    let answer = ask(Method::POST, venti_api_types::v1::REPORTS_PATH)
        .token(DEMO_TOKEN)
        .json(&report)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.text());
    let nobodys = ReportRequest {
        subject: ReportSubject::OpenPlan(NOBODYS.to_owned()),
        ..report.clone()
    };
    ask(Method::POST, venti_api_types::v1::REPORTS_PATH)
        .token(DEMO_TOKEN)
        .json(&nobodys)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);

    // Removed, it is in no answer; the report keeps its words.
    let removed = store
        .remove_open_plan(id.as_str(), NaiveDate::MIN)
        .await
        .unwrap();
    assert!(removed.is_some());
    ask(Method::GET, &open_plan_path(&id))
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    let reports = store
        .reports(venti_store::ReportListing::Open, None, 10)
        .await
        .unwrap();
    assert!(reports.iter().any(|report| matches!(
        &report.subject,
        venti_store::ReportSubject::OpenPlan { place, .. } if place == "Musterberg, Talstation"
    )));
}
