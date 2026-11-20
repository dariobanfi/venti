//! What the server writes to its log while pilots make groups and plan
//! flying days in them, and what it must never write there: an invite
//! code, which lets whoever has it into a group; a group's or a plan's
//! identifier; a group's name; who is in a group; or a word of a plan,
//! which says where people mean to be.
//!
//! Like `live_log.rs` and `reports_log.rs`, this test has a file, and so a
//! process, to itself: it collects the log of its own thread, and the
//! logging library decides once per log statement whether anyone listens.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode, header};
use chrono::NaiveDate;
use venti_api_types::v1::{
    CreateGroupRequest, CreateGroupResponse, ErrorCode, GROUP_JOIN_PATH, GROUPS_PATH,
    GroupInviteCode, GroupInviteResponse, GroupResponse, HandOverGroupRequest, Handle,
    JoinGroupRequest, JoinGroupResponse, PLAN_BOARD_PATH, PlanBoardResponse, PlanRequest,
    PlanResponse, PlansResponse, UpdateGroupRequest, group_invite_path, group_keeper_path,
    group_member_path, group_path, group_plans_path, plan_in_path, plan_path,
};
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::FixedClock;
use venti_server::groups::Limits;
use venti_store::testing::TroubledStore;

mod common;

use common::{TestClock, ask, server, settings, sign_up};

/// Collects what the server logs while a test holds it.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Words that stand nowhere else in this test or in the server: the first
/// is in the group's names, the second in the plans' places, the third in
/// their notes.
const NAME: &str = "Zanzibar";
const PLACE: &str = "marmalade";
const NOTE: &str = "quotient";

#[tokio::test]
async fn nothing_of_a_group_or_a_plan_reaches_the_log() {
    let clock = TestClock::new();
    let strict = Settings {
        groups: Limits {
            joins_per_day: 6,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, _store) = server(strict, &clock);
    // Three accounts with handles that are no words of the log's own. They
    // are made before anyone listens: the log may say that an account was
    // created.
    let keeper = sign_up(&router, "xq_keeper").await.token;
    let member = sign_up(&router, "xq_member").await.token;
    let outsider = sign_up(&router, "xq_outsider").await.token;

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // This test runs on one thread, so everything the router does while
    // the guard lives is logged here, at the most talkative level.
    let listener = tracing::Dispatch::new(subscriber);
    let _listening = tracing::dispatcher::set_default(&listener);

    // A group is made, and a pilot comes into it with its code.
    let made: CreateGroupResponse = ask(Method::POST, GROUPS_PATH)
        .token(&keeper)
        .json(&CreateGroupRequest {
            name: format!("{NAME} flyers"),
        })
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let group = made.group.id.clone();
    let first_code = made.invite_code.expose().to_owned();
    let join = |token: &str, code: &str| {
        let request = JoinGroupRequest {
            code: GroupInviteCode::new(code),
        };
        ask(Method::POST, GROUP_JOIN_PATH)
            .token(token)
            .json(&request)
            .send(&router)
    };
    let joined: JoinGroupResponse = join(&member, &first_code).await.ok(StatusCode::CREATED);
    assert!(joined.joined);

    // Codes that let nobody in: a wrong one, the right one in a body that
    // is no request, in the path, and in the query string.
    let wrong = "0123-4567-89ab-cdef";
    join(&outsider, wrong)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    ask(Method::POST, GROUP_JOIN_PATH)
        .token(&outsider)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(format!(r#"{{"code":"{first_code}""#))
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    ask(Method::POST, &format!("{GROUP_JOIN_PATH}/{first_code}"))
        .token(&outsider)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    ask(
        Method::POST,
        &format!("{GROUP_JOIN_PATH}?code={first_code}"),
    )
    .token(&outsider)
    .header(header::CONTENT_TYPE, "application/json")
    .bytes("{}")
    .send(&router)
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);

    // What members and the keeper do in the group.
    ask(Method::GET, &group_path(&group))
        .token(&member)
        .send(&router)
        .await
        .ok::<GroupResponse>(StatusCode::OK);
    ask(Method::GET, GROUPS_PATH)
        .token(&member)
        .send(&router)
        .await
        .ok::<venti_api_types::v1::GroupsResponse>(StatusCode::OK);
    ask(Method::PATCH, &group_path(&group))
        .token(&keeper)
        .json(&UpdateGroupRequest {
            name: format!("{NAME} pilots"),
        })
        .send(&router)
        .await
        .ok::<GroupResponse>(StatusCode::OK);
    let renewed: GroupInviteResponse = ask(Method::POST, &group_invite_path(&group))
        .token(&keeper)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let second_code = renewed.invite_code.expose().to_owned();

    // Plans: posted, changed, listed, said yes to, taken back, deleted.
    let a_plan = |day: u32| PlanRequest {
        day: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
        place: format!("{PLACE} hill"),
        position: None,
        meet_at: None,
        note: Some(format!("Bring the {NOTE}.")),
    };
    let posted: PlanResponse = ask(Method::POST, &group_plans_path(&group))
        .token(&member)
        .json(&a_plan(19))
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let plan = posted.plan.id.clone();
    ask(Method::PUT, &plan_path(&group, &plan))
        .token(&member)
        .json(&a_plan(20))
        .send(&router)
        .await
        .ok::<PlanResponse>(StatusCode::OK);
    for method in [Method::PUT, Method::DELETE] {
        ask(method, &plan_in_path(&group, &plan))
            .token(&keeper)
            .send(&router)
            .await
            .ok::<PlanResponse>(StatusCode::OK);
    }
    let listed: PlansResponse = ask(Method::GET, &group_plans_path(&group))
        .token(&keeper)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(listed.plans.len(), 1);
    let on_the_board: PlanBoardResponse = ask(Method::GET, PLAN_BOARD_PATH)
        .token(&keeper)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(on_the_board.plans.len(), 1);
    // What is refused: a plan that is none, and everything an account that
    // is not in the group asks of it.
    ask(Method::POST, &group_plans_path(&group))
        .token(&member)
        .json(&PlanRequest {
            note: Some(NOTE.repeat(100)),
            ..a_plan(19)
        })
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan);
    ask(Method::GET, &group_path(&group))
        .token(&outsider)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    ask(Method::DELETE, &plan_path(&group, &plan))
        .token(&outsider)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    ask(Method::GET, &group_plans_path(&group))
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

    // The keeping changes hands and back, a member is removed and comes in
    // again under the new code, and leaves.
    for (token, handle) in [(&keeper, "xq_member"), (&member, "xq_keeper")] {
        ask(Method::PUT, &group_keeper_path(&group))
            .token(token)
            .json(&HandOverGroupRequest {
                handle: handle.to_owned(),
            })
            .send(&router)
            .await
            .ok::<GroupResponse>(StatusCode::OK);
    }
    let members_path = group_member_path(&group, &Handle::new("xq_member").unwrap());
    let removed = ask(Method::DELETE, &members_path)
        .token(&keeper)
        .send(&router)
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let third: GroupInviteResponse = ask(Method::POST, &group_invite_path(&group))
        .token(&keeper)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let third_code = third.invite_code.expose().to_owned();
    join(&member, &second_code)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    join(&member, &third_code)
        .await
        .ok::<JoinGroupResponse>(StatusCode::CREATED);
    let left = ask(Method::DELETE, &members_path)
        .token(&member)
        .send(&router)
        .await;
    assert_eq!(left.status, StatusCode::NO_CONTENT);

    // More codes than an account may try in a day.
    let mut refused = 0;
    for _ in 0..8 {
        let answer = join(&outsider, &third_code.replace('-', " ")).await;
        refused += usize::from(answer.status == StatusCode::TOO_MANY_REQUESTS);
    }
    assert!(refused >= 4, "{refused} were refused");

    // The group is deleted.
    let deleted = ask(Method::DELETE, &group_path(&group))
        .token(&keeper)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    // And a database that is away, which the log does hear about.
    let api = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(FixedClock(adapters::recorded_at())),
        [],
        settings(),
    );
    let troubled = api.router(&Cors::Any);
    let answer = ask(Method::POST, GROUP_JOIN_PATH)
        .token(&format!("venti_{}", "7".repeat(52)))
        .json(&JoinGroupRequest {
            code: GroupInviteCode::new(third_code.clone()),
        })
        .send(&troubled)
        .await;
    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    // The log did record all this: the requests by their paths, with what
    // names a group left out, and the trouble with the database.
    for expected in [
        "path=/v1/groups}",
        "path=/v1/groups/join}",
        "path=/v1/groups/plans}",
        "path=/v1/groups/…}",
        "the store is unavailable",
    ] {
        assert!(log.contains(expected), "the log lacks {expected:?}:\n{log}");
    }
    // It holds nothing of a group.
    let plain = |code: &str| code.replace('-', "");
    let secrets = [
        ("the group's identifier", group.to_string()),
        ("the plan's identifier", plan.to_string()),
        ("the group's first invite code", first_code.clone()),
        ("the first invite code without hyphens", plain(&first_code)),
        ("the group's second invite code", second_code.clone()),
        ("the group's third invite code", third_code.clone()),
        ("the third invite code without hyphens", plain(&third_code)),
        (
            "the third invite code as it was typed",
            third_code.replace('-', " "),
        ),
        ("a code that was tried in vain", wrong.to_owned()),
        ("a word of the group's name", NAME.to_owned()),
        ("a word of a plan's place", PLACE.to_owned()),
        ("a word of a plan's note", NOTE.to_owned()),
        ("the handle of a member", "xq_member".to_owned()),
        ("the handle of the keeper", "xq_keeper".to_owned()),
        ("the handle of who asked in vain", "xq_outsider".to_owned()),
        (
            "the name of a part of a path under a group",
            "/invite".to_owned(),
        ),
        (
            "the name of a part of a path under a group",
            "/members".to_owned(),
        ),
    ];
    for (what, secret) in &secrets {
        assert!(
            !log.contains(secret.as_str()),
            "{what} is in the log:\n{log}"
        );
    }
    // Not even the first characters of a code or an identifier.
    for (what, secret) in &secrets[..7] {
        assert!(
            !log.contains(&secret[..8]),
            "the start of {what} is in the log"
        );
    }
}
