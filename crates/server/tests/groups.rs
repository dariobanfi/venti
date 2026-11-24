//! Groups of pilots and the plans their members make for a flying day,
//! through the real router.
//!
//! What must not happen is tested here too: a group answering anyone who
//! is not in it differently from a group that does not exist, a pilot
//! coming in without the code or under one that was replaced, a group or
//! an account with more than it has room for, a member doing what only
//! the keeper may, and a plan of a day long gone still being there.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::{Duration, NaiveDate};
use serde_json::{Value, json};
use venti_api_types::v1::{
    ACCOUNT_PATH, CreateGroupRequest, CreateGroupResponse, DeleteAccountRequest, ErrorCode,
    GROUP_JOIN_PATH, GROUPS_PATH, Group, GroupId, GroupInviteCode, GroupInviteResponse,
    GroupResponse, GroupsResponse, HandOverGroupRequest, Handle, JoinGroupRequest,
    JoinGroupResponse, MAX_GROUP_NAME_LENGTH, MAX_PLAN_NOTE_LENGTH, MAX_PLAN_PLACE_LENGTH,
    PLAN_BOARD_PATH, Password, Pilot, Plan, PlanBoardRequest, PlanBoardResponse, PlanDays, PlanId,
    PlanRequest, PlanResponse, PlansRequest, PlansResponse, SignInResponse, TimeOfDay,
    UpdateGroupRequest, group_invite_path, group_keeper_path, group_member_path, group_path,
    group_plans_path, plan_in_path, plan_path,
};
use venti_domain::GeoPoint;
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::FixedClock;
use venti_server::dev::{
    DEMO_GROUP, DEMO_GROUP_CODE, DEMO_GROUP_NAME, DEMO_HANDLE, DEMO_OTHER_GROUP,
    DEMO_OTHER_GROUP_CODE, DEMO_OTHER_GROUP_NAME, DEMO_OTHER_PLAN, DEMO_PASSWORD, DEMO_PLANS,
    DEMO_TOKEN,
};
use venti_server::groups::Limits;
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;
use venti_store::{MemoryStore, Store};

mod common;

use common::{
    Answer, PASSWORD, TestClock, ask, golden, recorded, server, settings, sign_up, try_sign_in,
};

// --- Helpers ----------------------------------------------------------------

/// An identifier of the right form that nothing has.
const NOBODYS: &str = "0123456789abcdefghjkmnpqrs";

/// A server that takes anyone, with the limits a real server has on groups
/// and plans, its store, and its clock, which stands at 19:05 UTC on 18
/// November 2026.
fn open_server() -> (Router, Arc<MemoryStore>, TestClock) {
    server_with(Limits::default())
}

fn server_with(groups: Limits) -> (Router, Arc<MemoryStore>, TestClock) {
    let clock = TestClock::new();
    let (router, store) = server(
        Settings {
            groups,
            ..settings()
        },
        &clock,
    );
    (router, store, clock)
}

/// A day of November 2026.
fn day(of_month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, of_month).unwrap()
}

fn id_of(text: &str) -> GroupId {
    text.parse().unwrap()
}

/// A plan that says as little as a plan may.
fn a_plan(on: NaiveDate) -> PlanRequest {
    PlanRequest {
        day: on,
        place: "Musterberg".to_owned(),
        position: None,
        meet_at: None,
        note: None,
    }
}

async fn try_make(router: &Router, token: &str, name: &str) -> Answer {
    let request = CreateGroupRequest::named(name.to_owned());
    ask(Method::POST, GROUPS_PATH)
        .token(token)
        .json(&request)
        .send(router)
        .await
}

/// Makes a group and returns it with its invite code.
async fn make(router: &Router, token: &str, name: &str) -> CreateGroupResponse {
    try_make(router, token, name).await.ok(StatusCode::CREATED)
}

async fn try_join(router: &Router, token: &str, code: &str) -> Answer {
    let request = JoinGroupRequest {
        code: GroupInviteCode::new(code),
    };
    ask(Method::POST, GROUP_JOIN_PATH)
        .token(token)
        .json(&request)
        .send(router)
        .await
}

/// Comes into the group a code belongs to.
async fn come_in(router: &Router, token: &str, code: &GroupInviteCode) -> Group {
    let joined: JoinGroupResponse = try_join(router, token, code.expose())
        .await
        .ok(StatusCode::CREATED);
    assert!(joined.joined);
    joined.group
}

async fn new_code(router: &Router, token: &str, group: &GroupId) -> GroupInviteCode {
    let renewed: GroupInviteResponse = ask(Method::POST, &group_invite_path(group))
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK);
    renewed.invite_code
}

async fn look(router: &Router, token: &str, group: &GroupId) -> Answer {
    ask(Method::GET, &group_path(group))
        .token(token)
        .send(router)
        .await
}

/// A group as one of its members sees it.
async fn seen(router: &Router, token: &str, group: &GroupId) -> GroupResponse {
    look(router, token, group).await.ok(StatusCode::OK)
}

fn handles(pilots: &[Pilot]) -> Vec<&str> {
    pilots.iter().map(|pilot| pilot.handle.as_str()).collect()
}

/// The handles of a group's keeper and of its members in their order.
async fn keeper_and_members(
    router: &Router,
    token: &str,
    group: &GroupId,
) -> (String, Vec<String>) {
    let seen = seen(router, token, group).await;
    assert_eq!(seen.group.member_count as usize, seen.members.len());
    let members = handles(&seen.members).into_iter().map(str::to_owned);
    (seen.group.keeper.handle.to_string(), members.collect())
}

async fn my_groups(router: &Router, token: &str) -> Vec<Group> {
    let listed: GroupsResponse = ask(Method::GET, GROUPS_PATH)
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK);
    listed.groups
}

async fn remove(router: &Router, token: &str, group: &GroupId, handle: &str) -> Answer {
    let handle = Handle::new(handle).unwrap();
    ask(Method::DELETE, &group_member_path(group, &handle))
        .token(token)
        .send(router)
        .await
}

async fn try_post(router: &Router, token: &str, group: &GroupId, plan: &PlanRequest) -> Answer {
    ask(Method::POST, &group_plans_path(group))
        .token(token)
        .json(plan)
        .send(router)
        .await
}

async fn post(router: &Router, token: &str, group: &GroupId, plan: &PlanRequest) -> Plan {
    let posted: PlanResponse = try_post(router, token, group, plan)
        .await
        .ok(StatusCode::CREATED);
    posted.plan
}

/// A group's plans from a day on, as a member is given them.
async fn plans_from(router: &Router, token: &str, group: &GroupId, from: NaiveDate) -> Vec<Plan> {
    let request = PlansRequest {
        group: group.clone(),
        days: PlanDays {
            from: Some(from),
            to: None,
        },
    };
    let listed: PlansResponse = ask(Method::GET, &request.path_and_query())
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK);
    listed.plans
}

async fn board(router: &Router, token: &str, days: PlanDays) -> PlanBoardResponse {
    ask(Method::GET, &PlanBoardRequest { days }.path_and_query())
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK)
}

async fn set_in(router: &Router, token: &str, plan: &Plan, is_in: bool) -> Answer {
    let method = if is_in { Method::PUT } else { Method::DELETE };
    ask(method, &plan_in_path(&plan.group, &plan.id))
        .token(token)
        .send(router)
        .await
}

/// The handles of those who are in on a plan after a pilot said that they
/// are, or took it back.
async fn in_after(router: &Router, token: &str, plan: &Plan, is_in: bool) -> Vec<String> {
    let changed: PlanResponse = set_in(router, token, plan, is_in).await.ok(StatusCode::OK);
    handles(&changed.plan.pilots_in)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Everything about an answer that a client can observe.
fn observable(answer: &Answer) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
    let mut headers: Vec<(String, String)> = answer
        .headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        .collect();
    headers.sort();
    (answer.status, headers, answer.body.clone())
}

// --- What the development mode starts with -----------------------------------------

#[tokio::test]
async fn the_demo_pilot_reads_their_group_its_members_and_its_plans() {
    let router = recorded().await;
    let group = id_of(DEMO_GROUP);

    // The groups the demo account is in: the one it keeps.
    let answer = ask(Method::GET, GROUPS_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let listed: GroupsResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(listed.groups.len(), 1);
    assert_eq!(listed.groups[0].id, group);
    assert_eq!(listed.groups[0].name, DEMO_GROUP_NAME);
    assert_eq!(listed.groups[0].keeper.handle.as_str(), DEMO_HANDLE);
    assert_eq!(listed.groups[0].member_count, 3);
    assert_golden(&golden("groups.json"), &listed);

    // The group with its members, in the order in which they came in.
    let answer = look(&router, DEMO_TOKEN, &group).await;
    let shown: GroupResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(shown.group, listed.groups[0]);
    assert_eq!(handles(&shown.members), [DEMO_HANDLE, "mara", "jonas"]);
    assert_golden(&golden("group.json"), &shown);

    // Its plans for today and the days ahead. This mode's clock stands on
    // the evening of 18 November 2026.
    let answer = ask(Method::GET, &group_plans_path(&group))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let plans: PlansResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    let ids: Vec<&str> = plans.plans.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(ids, DEMO_PLANS);
    let days: Vec<NaiveDate> = plans.plans.iter().map(|plan| plan.day).collect();
    assert_eq!(days, [day(18), day(19), day(20)]);
    let who: Vec<Vec<&str>> = plans
        .plans
        .iter()
        .map(|plan| handles(&plan.pilots_in))
        .collect();
    assert_eq!(
        who,
        [
            vec![DEMO_HANDLE, "mara", "jonas"],
            vec!["mara", "jonas"],
            vec!["jonas", DEMO_HANDLE]
        ]
    );
    // The first says everything a plan can say, the last as little as one
    // may.
    let today = &plans.plans[0];
    assert_eq!(today.author.handle.as_str(), DEMO_HANDLE);
    assert_eq!(
        today.position,
        Some(GeoPoint::new(47.6642, 11.7967).unwrap())
    );
    assert_eq!(today.meet_at, TimeOfDay::new(10, 30));
    assert!(today.note.as_deref().unwrap().contains("made-up"));
    assert!(today.note.as_deref().unwrap().contains('\n'));
    let last = &plans.plans[2];
    assert_eq!(
        (last.position, last.meet_at, &last.note),
        (None, None, &None)
    );
    assert_eq!(last.created_at, last.changed_at);
    assert_golden(&golden("plans.json"), &plans);

    // The board of one day: every group of the pilot, and the plans for
    // that day.
    let today_only = PlanDays {
        from: Some(day(18)),
        to: Some(day(18)),
    };
    let answer = ask(
        Method::GET,
        &PlanBoardRequest { days: today_only }.path_and_query(),
    )
    .token(DEMO_TOKEN)
    .send(&router)
    .await;
    let today: PlanBoardResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(today.groups, listed.groups);
    assert_eq!(today.plans, plans.plans[..1]);
    assert!(!today.truncated);
    assert_golden(&golden("plan_board.json"), &today);
    // Without days it is today and everything ahead, as for one group.
    let ahead = board(&router, DEMO_TOKEN, PlanDays::default()).await;
    assert_eq!(ahead.plans, plans.plans);
    let later = PlanDays {
        from: Some(day(19)),
        to: None,
    };
    assert_eq!(
        board(&router, DEMO_TOKEN, later).await.plans,
        plans.plans[1..]
    );

    // The other group is no group to the demo account, and its plan is in
    // no answer the account gets.
    let other = id_of(DEMO_OTHER_GROUP);
    look(&router, DEMO_TOKEN, &other)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    assert!(
        ahead
            .plans
            .iter()
            .all(|plan| plan.id.as_str() != DEMO_OTHER_PLAN)
    );
    // Mara, who keeps it, has both groups and all four plans on her board.
    let mara: SignInResponse = try_sign_in(&router, "mara", DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let hers = board(&router, &mara.token, PlanDays::default()).await;
    let names: Vec<&str> = hers
        .groups
        .iter()
        .map(|group| group.name.as_str())
        .collect();
    // Beside the development mode's public groups she is in.
    let private: Vec<&str> = hers
        .groups
        .iter()
        .filter(|group| !group.public)
        .map(|group| group.name.as_str())
        .collect();
    assert_eq!(private, [DEMO_GROUP_NAME, DEMO_OTHER_GROUP_NAME]);
    assert_eq!(names.len(), 5, "{names:?}");
    let ids: Vec<&str> = hers.plans.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(
        ids,
        [DEMO_PLANS[0], DEMO_PLANS[1], DEMO_OTHER_PLAN, DEMO_PLANS[2]]
    );
    assert_eq!(hers.plans[2].group, other);
    let theirs = seen(&router, &mara.token, &other).await;
    assert_eq!(theirs.group.keeper.handle.as_str(), "mara");
    assert_eq!(handles(&theirs.members), ["mara", "jonas"]);
}

#[tokio::test]
async fn a_pilot_makes_a_group_and_others_come_in_with_its_code() {
    let router = recorded().await;

    // The demo account makes a second group. This is the example of the
    // documentation, with what differs from start to start replaced.
    let request = CreateGroupRequest::named("Zweite Mustergruppe".to_owned());
    assert_golden(&golden("group_create_request.json"), &request);
    let answer = ask(Method::POST, GROUPS_PATH)
        .token(DEMO_TOKEN)
        .json(&request)
        .send(&router)
        .await;
    let made: CreateGroupResponse = answer.ok(StatusCode::CREATED);
    answer.is_private();
    assert_eq!(made.group.name, "Zweite Mustergruppe");
    assert_eq!(made.group.keeper.handle.as_str(), DEMO_HANDLE);
    assert_eq!(made.group.member_count, 1);
    assert_eq!(made.group.created_at, adapters::recorded_at());
    // The code has the form of a code, and each group its own.
    let code = made.invite_code.expose();
    assert_eq!(code.len(), 19);
    assert_eq!(code.matches('-').count(), 3);
    assert_ne!(code, DEMO_GROUP_CODE);
    let example = CreateGroupResponse {
        group: Group {
            id: id_of(NOBODYS),
            ..made.group.clone()
        },
        invite_code: GroupInviteCode::new("7k3m-9q2x-4r8t-1v6w"),
    };
    assert_golden(&golden("group_created.json"), &example);
    let mine = my_groups(&router, DEMO_TOKEN).await;
    assert_eq!(mine.len(), 2);
    assert_eq!(
        mine[1], made.group,
        "the one the pilot came into last is last"
    );

    // The demo account comes into the group it is not in, with that
    // group's code as it is written down.
    let request = JoinGroupRequest {
        code: GroupInviteCode::new(DEMO_OTHER_GROUP_CODE),
    };
    assert_golden(&golden("group_join_request.json"), &request);
    let answer = ask(Method::POST, GROUP_JOIN_PATH)
        .token(DEMO_TOKEN)
        .json(&request)
        .send(&router)
        .await;
    let joined: JoinGroupResponse = answer.ok(StatusCode::CREATED);
    answer.is_private();
    assert!(joined.joined);
    assert_eq!(joined.group.id.as_str(), DEMO_OTHER_GROUP);
    assert_eq!(joined.group.member_count, 3);
    assert_golden(&golden("group_joined.json"), &joined);
    let theirs = seen(&router, DEMO_TOKEN, &joined.group.id).await;
    assert_eq!(handles(&theirs.members), ["mara", "jonas", DEMO_HANDLE]);
    // Its plan is on the demo account's board from then on.
    let ahead = board(&router, DEMO_TOKEN, PlanDays::default()).await;
    assert!(
        ahead
            .plans
            .iter()
            .any(|plan| plan.id.as_str() == DEMO_OTHER_PLAN)
    );

    // Again is no change, and says so. A code is read as a person types
    // it.
    for typed in [
        DEMO_OTHER_GROUP_CODE.to_owned(),
        DEMO_OTHER_GROUP_CODE.replace('-', " "),
        DEMO_OTHER_GROUP_CODE.replace('-', ""),
    ] {
        let answer = try_join(&router, DEMO_TOKEN, &typed).await;
        let again: JoinGroupResponse = answer.ok(StatusCode::OK);
        assert!(!again.joined);
        assert_eq!(again.group, joined.group);
    }

    // A new account comes into the demo account's first group with the
    // code that is written down, and into the new one with its own.
    let anna = sign_up(&router, "anna").await.token;
    let first = try_join(&router, &anna, DEMO_GROUP_CODE).await;
    let first: JoinGroupResponse = first.ok(StatusCode::CREATED);
    assert_eq!(first.group.id.as_str(), DEMO_GROUP);
    assert_eq!(first.group.member_count, 4);
    come_in(&router, &anna, &made.invite_code).await;
    let (keeper, members) = keeper_and_members(&router, &anna, &made.group.id).await;
    assert_eq!(keeper, DEMO_HANDLE);
    assert_eq!(members, [DEMO_HANDLE, "anna"]);

    // The keeper makes a new code: the old one lets nobody in, the new one
    // does, and who is in stays in.
    let answer = ask(Method::POST, &group_invite_path(&made.group.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let renewed: GroupInviteResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_ne!(renewed.invite_code, made.invite_code);
    let example = GroupInviteResponse {
        invite_code: GroupInviteCode::new("2w8x-5c9p-3h7k-6n4d"),
    };
    assert_golden(&golden("group_invite.json"), &example);
    let bernd = sign_up(&router, "bernd").await.token;
    try_join(&router, &bernd, made.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    come_in(&router, &bernd, &renewed.invite_code).await;
    // This mode's clock stands still, so the two came in at one moment:
    // their order among themselves is their accounts' and means nothing.
    let (_, mut members) = keeper_and_members(&router, &anna, &made.group.id).await;
    assert_eq!(members.remove(0), DEMO_HANDLE);
    members.sort();
    assert_eq!(members, ["anna", "bernd"]);
}

#[tokio::test]
async fn a_code_that_is_none_lets_nobody_in_and_says_nothing() {
    let (router, _store, _clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let made = make(&router, &keeper, "Mustergruppe").await;
    let code = made.invite_code.expose();

    // A code nobody made, an account's invite code in a group code's
    // place, text that is no code, and the group's identifier: all get one
    // answer, in every header and byte.
    let mut wrong = code.to_owned();
    let last = if wrong.ends_with('0') { "1" } else { "0" };
    wrong.replace_range(wrong.len() - 1.., last);
    let mut answers = Vec::new();
    for text in [
        wrong.as_str(),
        "0000-0000-0000-0000",
        "",
        "let me in",
        &code[..18],
        made.group.id.as_str(),
        "uuuu-uuuu-uuuu-uuuu",
    ] {
        let answer = try_join(&router, &anna, text).await;
        let said = answer.error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
        assert!(!said.contains(text) || text.is_empty(), "{said}");
        answers.push(observable(&answer));
    }
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(my_groups(&router, &anna).await.is_empty());
    assert_eq!(
        seen(&router, &keeper, &made.group.id).await.members.len(),
        1
    );

    // A body that is no request to come in is refused without repeating
    // what stands in it.
    for body in [
        format!(r#"{{"code":"{code}""#),
        format!(r#"{{"invite":"{code}"}}"#),
        r#"{"code":7}"#.to_owned(),
    ] {
        let answer = ask(Method::POST, GROUP_JOIN_PATH)
            .token(&anna)
            .header(header::CONTENT_TYPE, "application/json")
            .bytes(body)
            .send(&router)
            .await;
        let said = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
        assert!(!said.contains(code) && !said.contains(&code[..4]), "{said}");
    }
    // A code in a path is a path the API does not have, and the answer
    // does not repeat it.
    let in_the_path = ask(Method::POST, &format!("{GROUP_JOIN_PATH}/{code}"))
        .token(&anna)
        .send(&router)
        .await;
    let said = in_the_path.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    assert!(!said.contains(code), "{said}");
    // What a browser can be made to send without asking is not read.
    let plain = ask(Method::POST, GROUP_JOIN_PATH)
        .token(&anna)
        .header(header::CONTENT_TYPE, "text/plain")
        .bytes(format!(r#"{{"code":"{code}"}}"#))
        .send(&router)
        .await;
    plain.error(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedMediaType,
    );
    // Nobody comes in without an account.
    let nobody = ask(Method::POST, GROUP_JOIN_PATH)
        .json(&JoinGroupRequest {
            code: made.invite_code.clone(),
        })
        .send(&router)
        .await;
    nobody.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    assert!(my_groups(&router, &anna).await.is_empty());
    // The code itself was good all along.
    come_in(&router, &anna, &made.invite_code).await;
}

// --- Nobody but its members ---------------------------------------------------------

/// Every request there is about one group, and one of its plans: the
/// method, the path, a body if it has one, and whether an account that is
/// not in the group is told "no such group" (and not that its request
/// cannot be read).
fn every_request_about(group: &str, plan: &str) -> Vec<(Method, String, Option<Value>, bool)> {
    let a_plan = json!({ "day": "2026-11-19", "place": "Musterberg", "meet_at": "09:30" });
    let mut requests = vec![
        (Method::GET, format!("/v1/groups/{group}"), None, true),
        (
            Method::PATCH,
            format!("/v1/groups/{group}"),
            Some(json!({ "name": "Taken over" })),
            true,
        ),
        (Method::DELETE, format!("/v1/groups/{group}"), None, true),
        (
            Method::POST,
            format!("/v1/groups/{group}/invite"),
            None,
            true,
        ),
        (Method::GET, format!("/v1/groups/{group}/plans"), None, true),
        (
            Method::GET,
            format!("/v1/groups/{group}/plans?from=2026-11-01"),
            None,
            true,
        ),
        (
            Method::GET,
            format!("/v1/groups/{group}/plans?from=2026-11-19&to=2026-11-19"),
            None,
            true,
        ),
        (
            Method::POST,
            format!("/v1/groups/{group}/plans"),
            Some(a_plan.clone()),
            true,
        ),
        // Requests that cannot be read are told so, group or no group.
        (
            Method::PATCH,
            format!("/v1/groups/{group}"),
            Some(json!({ "name": "" })),
            false,
        ),
        (
            Method::GET,
            format!("/v1/groups/{group}/plans?from=tomorrow"),
            None,
            false,
        ),
        (
            Method::POST,
            format!("/v1/groups/{group}/plans"),
            Some(json!({ "day": "2026-11-19", "place": "" })),
            false,
        ),
        (
            Method::PUT,
            format!("/v1/groups/{group}/keeper"),
            Some(json!({ "name": "member" })),
            false,
        ),
    ];
    // What names a member, with the asker's own handle, a member's, the
    // keeper's, and a handle nobody has.
    for handle in [
        "outsider", "removed", "leaver", "member", "keeper", "nobody",
    ] {
        requests.push((
            Method::PUT,
            format!("/v1/groups/{group}/keeper"),
            Some(json!({ "handle": handle })),
            true,
        ));
        requests.push((
            Method::DELETE,
            format!("/v1/groups/{group}/members/{handle}"),
            None,
            true,
        ));
    }
    // What names a plan: one that is there, one that is not, and text that
    // is no identifier.
    for plan in [plan, NOBODYS, "x"] {
        let path = format!("/v1/groups/{group}/plans/{plan}");
        requests.push((Method::PUT, path.clone(), Some(a_plan.clone()), true));
        requests.push((Method::DELETE, path.clone(), None, true));
        requests.push((Method::PUT, format!("{path}/in"), None, true));
        requests.push((Method::DELETE, format!("{path}/in"), None, true));
    }
    requests
}

async fn send_as(
    router: &Router,
    token: Option<&str>,
    request: &(Method, String, Option<Value>, bool),
) -> Answer {
    let (method, path, body, _) = request;
    let mut asking = ask(method.clone(), path);
    if let Some(token) = token {
        asking = asking.token(token);
    }
    if let Some(body) = body {
        asking = asking.json(body);
    }
    asking.send(router).await
}

#[tokio::test]
async fn to_anyone_else_a_group_answers_as_one_that_does_not_exist() {
    let (router, _store, clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    let member = sign_up(&router, "member").await.token;
    let outsider = sign_up(&router, "outsider").await.token;
    let removed = sign_up(&router, "removed").await.token;
    let leaver = sign_up(&router, "leaver").await.token;
    let made = make(&router, &keeper, "Mustergruppe").await;
    for token in [&member, &removed, &leaver] {
        clock.advance(Duration::minutes(1));
        come_in(&router, token, &made.invite_code).await;
    }
    let group = made.group.id.clone();
    let plan = post(&router, &member, &group, &a_plan(day(19))).await;
    // One was in the group and was removed, one was in it and left. To
    // both it is no group any more.
    let answer = remove(&router, &keeper, &group, "removed").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    let answer = remove(&router, &leaver, &group, "leaver").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    let code = new_code(&router, &keeper, &group).await;
    let before = (
        seen(&router, &keeper, &group).await,
        plans_from(&router, &keeper, &group, day(1)).await,
    );

    let there = every_request_about(group.as_str(), plan.id.as_str());
    let nowhere = every_request_about(NOBODYS, plan.id.as_str());
    assert!(there.len() > 30);
    // The comparison means something: a member gets other answers for the
    // group than for none. (Only reading is tried, to change nothing.)
    assert_eq!(look(&router, &member, &group).await.status, StatusCode::OK);
    look(&router, &member, &id_of(NOBODYS))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);

    for (who, token) in [
        ("an outsider", &outsider),
        ("a removed member", &removed),
        ("a member who left", &leaver),
    ] {
        for (real, none) in there.iter().zip(&nowhere) {
            let (method, path, _, about_the_group) = real;
            let for_the_group = send_as(&router, Some(token), real).await;
            let for_no_group = send_as(&router, Some(token), none).await;
            // The same in status, in every header and in every byte.
            assert_eq!(
                observable(&for_the_group),
                observable(&for_no_group),
                "{who}: {method} {path}"
            );
            if *about_the_group {
                let said = for_the_group.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
                assert!(
                    !said.contains(group.as_str()) && !said.contains("Mustergruppe"),
                    "{said}"
                );
            } else {
                assert_eq!(
                    for_the_group.status,
                    StatusCode::BAD_REQUEST,
                    "{method} {path}"
                );
            }
        }
        // Nothing lists it for them either.
        assert!(my_groups(&router, token).await.is_empty(), "{who}");
        let mine = board(&router, token, PlanDays::default()).await;
        assert!(mine.groups.is_empty() && mine.plans.is_empty(), "{who}");
    }
    // Without an account nothing about a group is said at all.
    for (real, none) in there.iter().zip(&nowhere) {
        let for_the_group = send_as(&router, None, real).await;
        let for_no_group = send_as(&router, None, none).await;
        for_the_group.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
        assert_eq!(observable(&for_the_group), observable(&for_no_group));
    }
    // Identifiers of another form are the identifiers of no group.
    for odd in [
        "x",
        "join2",
        "DEMO",
        &NOBODYS.to_uppercase(),
        "%00",
        "..%2Faccount",
    ] {
        let answer = ask(Method::GET, &format!("/v1/groups/{odd}"))
            .token(&outsider)
            .send(&router)
            .await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    }

    // None of it changed anything: the group, its members, its plans and
    // its code are as they were.
    let after = (
        seen(&router, &keeper, &group).await,
        plans_from(&router, &keeper, &group, day(1)).await,
    );
    assert_eq!(after, before);
    assert_eq!(handles(&after.0.members), ["keeper", "member"]);
    // With the code the outsider is in, and then the group answers.
    come_in(&router, &outsider, &code).await;
    assert_eq!(
        look(&router, &outsider, &group).await.status,
        StatusCode::OK
    );
    let listed = plans_from(&router, &outsider, &group, day(1)).await;
    assert_eq!(listed, before.1);
}

// --- Members and the keeper ------------------------------------------------------------

#[tokio::test]
async fn only_the_keeper_renames_removes_hands_over_and_deletes() {
    let (router, _store, clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let bernd = sign_up(&router, "bernd").await.token;
    let carla = sign_up(&router, "carla").await.token;
    let made = make(&router, &keeper, "Mustergruppe").await;
    let group = made.group.id.clone();
    // They come in a minute apart: the order of a group's members is the
    // order in which they came in.
    let a_minute_later = |token: &str| {
        clock.advance(Duration::minutes(1));
        let (router, token, code) = (&router, token.to_owned(), &made.invite_code);
        async move { come_in(router, &token, code).await }
    };
    a_minute_later(&anna).await;
    a_minute_later(&bernd).await;
    let rename = |token: &str, name: &str| {
        let request = UpdateGroupRequest::rename(name.to_owned());
        ask(Method::PATCH, &group_path(&group))
            .token(token)
            .json(&request)
            .send(&router)
    };
    let hand_over = |token: &str, handle: &str| {
        let request = HandOverGroupRequest {
            handle: handle.to_owned(),
        };
        ask(Method::PUT, &group_keeper_path(&group))
            .token(token)
            .json(&request)
            .send(&router)
    };

    // A member is told that this is the keeper's to do, and nothing
    // changes.
    rename(&anna, "Annas Gruppe")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    hand_over(&anna, "anna")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    remove(&router, &anna, &group, "bernd")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    ask(Method::POST, &group_invite_path(&group))
        .token(&anna)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    ask(Method::DELETE, &group_path(&group))
        .token(&anna)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    let as_it_was = seen(&router, &bernd, &group).await;
    assert_eq!(as_it_was.group, made.group_with(3));
    a_minute_later(&carla).await;

    // The keeper renames the group. The name is cleaned as it is kept.
    let answer = rename(&keeper, "  Thermik\t&  Kuchen\u{202e} <b>\n").await;
    let renamed: GroupResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(renamed.group.name, "Thermik & Kuchen <b>");
    // As every member reads it; how each stands to it is their own.
    let as_anna = seen(&router, &anna, &group).await;
    assert_eq!(
        (&as_anna.group, &as_anna.members),
        (&renamed.group, &renamed.members)
    );

    // The keeper removes a member, who is then out, and no other.
    remove(&router, &keeper, &group, "nobody")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupMemberNotFound);
    let answer = remove(&router, &keeper, &group, "Bernd").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    answer.is_private();
    assert!(answer.body.is_empty());
    look(&router, &bernd, &group)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    remove(&router, &keeper, &group, "bernd")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupMemberNotFound);
    // The code the removed pilot knew is over, for everyone.
    try_join(&router, &bernd, made.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    let dora = sign_up(&router, "dora").await.token;
    try_join(&router, &dora, made.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    let code = new_code(&router, &keeper, &group).await;
    clock.advance(Duration::minutes(1));
    come_in(&router, &dora, &code).await;

    // The keeper hands the keeping to a member, and to nobody else.
    for handle in ["nobody", "bernd", "", "x' OR '1'='1"] {
        hand_over(&keeper, handle)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::GroupMemberNotFound);
    }
    let handed: GroupResponse = hand_over(&keeper, "@Anna").await.ok(StatusCode::OK);
    assert_eq!(handed.group.keeper.handle.as_str(), "anna");
    assert_eq!(
        handles(&handed.members),
        ["keeper", "anna", "carla", "dora"]
    );
    // Who kept it is a member like any other from then on.
    rename(&keeper, "Again")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    let same: GroupResponse = hand_over(&anna, "anna").await.ok(StatusCode::OK);
    assert_eq!(same, handed);

    // The keeper deletes the group. It is gone for everyone, with its
    // plans and its code.
    let plan = post(&router, &keeper, &group, &a_plan(day(19))).await;
    let answer = ask(Method::DELETE, &group_path(&group))
        .token(&anna)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    answer.is_private();
    for token in [&keeper, &anna, &dora] {
        look(&router, token, &group)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
        assert!(my_groups(&router, token).await.is_empty());
        set_in(&router, token, &plan, true)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    }
    try_join(&router, &bernd, code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
}

/// What a group that was just made looks like with `members` members.
trait AsMade {
    fn group_with(&self, members: u32) -> Group;
}

impl AsMade for CreateGroupResponse {
    fn group_with(&self, members: u32) -> Group {
        Group {
            member_count: members,
            ..self.group.clone()
        }
    }
}

#[tokio::test]
async fn a_member_leaves_and_when_the_keeper_goes_the_group_is_kept_by_the_next() {
    let (router, _store, clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    let made = make(&router, &keeper, "Mustergruppe").await;
    let group = made.group.id.clone();
    // Three come in, a minute apart.
    let mut tokens = Vec::new();
    for handle in ["anna", "bernd", "carla"] {
        clock.advance(Duration::minutes(1));
        let token = sign_up(&router, handle).await.token;
        come_in(&router, &token, &made.invite_code).await;
        tokens.push(token);
    }
    let [anna, bernd, carla] = [&tokens[0], &tokens[1], &tokens[2]];
    let annas = post(&router, anna, &group, &a_plan(day(19))).await;
    let carlas = post(&router, carla, &group, &a_plan(day(20))).await;
    clock.advance(Duration::minutes(1));
    assert_eq!(
        in_after(&router, anna, &carlas, true).await,
        ["carla", "anna"]
    );

    // A member leaves, by their own handle: their plan goes with them, and
    // their word on the plans of others.
    let answer = remove(&router, anna, &group, "anna").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    look(&router, anna, &group)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    let left = plans_from(&router, &keeper, &group, day(1)).await;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, carlas.id);
    assert_eq!(handles(&left[0].pilots_in), ["carla"]);
    set_in(&router, &keeper, &annas, true)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    // Leaving ends no code: the pilot can come back, as the last in the
    // list, and their plan does not come back with them.
    clock.advance(Duration::minutes(1));
    come_in(&router, anna, &made.invite_code).await;
    let (_, members) = keeper_and_members(&router, anna, &group).await;
    assert_eq!(members, ["keeper", "bernd", "carla", "anna"]);
    assert_eq!(plans_from(&router, anna, &group, day(1)).await.len(), 1);

    // The keeper leaves: the member who has been in the group longest
    // keeps it from then on.
    let answer = remove(&router, &keeper, &group, "keeper").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    look(&router, &keeper, &group)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    let (keeps, members) = keeper_and_members(&router, carla, &group).await;
    assert_eq!(keeps, "bernd");
    assert_eq!(members, ["bernd", "carla", "anna"]);
    // The new keeper may what a keeper may.
    let answer = remove(&router, bernd, &group, "anna").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);

    // The last members leave one after the other; with the last of them
    // the group goes, and its code with it.
    for (token, handle) in [(bernd, "bernd"), (carla, "carla")] {
        let answer = remove(&router, token, &group, handle).await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
    }
    let code = made.invite_code.expose();
    for token in [&keeper, anna, bernd, carla] {
        assert!(my_groups(&router, token).await.is_empty());
        try_join(&router, token, code)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    }
}

#[tokio::test]
async fn a_deleted_account_takes_what_it_put_into_its_groups() {
    let (router, _store, clock) = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    // Dario keeps one group that Anna is in and one he is alone in, and is
    // in a group that Anna keeps.
    let kept = make(&router, &dario, "Mustergruppe").await;
    let alone = make(&router, &dario, "Beispielgruppe").await;
    let hers = make(&router, &anna, "Annas Mustergruppe").await;
    clock.advance(Duration::minutes(1));
    come_in(&router, &anna, &kept.invite_code).await;
    come_in(&router, &dario, &hers.invite_code).await;
    let his = post(&router, &dario, &hers.group.id, &a_plan(day(19))).await;
    let her_plan = post(&router, &anna, &hers.group.id, &a_plan(day(20))).await;
    in_after(&router, &dario, &her_plan, true).await;
    in_after(&router, &anna, &his, true).await;
    post(&router, &dario, &kept.group.id, &a_plan(day(19))).await;

    let answer = ask(Method::DELETE, ACCOUNT_PATH)
        .token(&dario)
        .json(&DeleteAccountRequest {
            password: Password::new(PASSWORD),
            google_id_token: None,
        })
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);

    // The group he kept is Anna's to keep now, without his plan.
    let (keeps, members) = keeper_and_members(&router, &anna, &kept.group.id).await;
    assert_eq!(keeps, "anna");
    assert_eq!(members, ["anna"]);
    assert!(
        plans_from(&router, &anna, &kept.group.id, day(1))
            .await
            .is_empty()
    );
    // In her group his plan is gone, and he is no longer in on hers.
    let (_, members) = keeper_and_members(&router, &anna, &hers.group.id).await;
    assert_eq!(members, ["anna"]);
    let left = plans_from(&router, &anna, &hers.group.id, day(1)).await;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, her_plan.id);
    assert_eq!(handles(&left[0].pilots_in), ["anna"]);
    // The group he was alone in is gone, with its code. The other's code
    // is as it was.
    let bernd = sign_up(&router, "bernd").await.token;
    try_join(&router, &bernd, alone.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    come_in(&router, &bernd, &kept.invite_code).await;
    // A new account with his handle is in nothing.
    let again = sign_up(&router, "dario").await.token;
    assert!(my_groups(&router, &again).await.is_empty());
    look(&router, &again, &kept.group.id)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
}

// --- Plans ------------------------------------------------------------------------------

#[tokio::test]
async fn members_post_plans_change_them_and_say_that_they_are_in() {
    let router = recorded().await;
    let group = id_of(DEMO_GROUP);
    let mara: SignInResponse = try_sign_in(&router, "mara", DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let jonas: SignInResponse = try_sign_in(&router, "jonas", DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let (mara, jonas) = (mara.token, jonas.token);

    // The demo account posts a plan for Saturday. This is the example of
    // the documentation, with the plan's identifier replaced.
    let request = PlanRequest {
        day: day(21),
        place: "Beispielwiese, Parkplatz".to_owned(),
        position: Some(GeoPoint::new(47.6922, 11.7675).unwrap()),
        meet_at: TimeOfDay::new(9, 30),
        note: Some("A made-up plan. Whoever has room in the car says so here.".to_owned()),
    };
    assert_golden(&golden("plan_request.json"), &request);
    let answer = try_post(&router, DEMO_TOKEN, &group, &request).await;
    let posted: PlanResponse = answer.ok(StatusCode::CREATED);
    answer.is_private();
    let plan = posted.plan;
    assert_eq!(plan.group, group);
    assert_eq!((plan.day, &plan.place), (request.day, &request.place));
    assert_eq!(
        (plan.position, plan.meet_at),
        (request.position, request.meet_at)
    );
    assert_eq!(plan.note, request.note);
    assert_eq!(plan.author.handle.as_str(), DEMO_HANDLE);
    assert_eq!(plan.created_at, adapters::recorded_at());
    assert_eq!(plan.changed_at, plan.created_at);
    // Whoever posts a plan is in.
    assert_eq!(handles(&plan.pilots_in), [DEMO_HANDLE]);
    let example = PlanResponse {
        plan: Plan {
            id: NOBODYS.parse().unwrap(),
            ..plan.clone()
        },
    };
    assert_golden(&golden("plan.json"), &example);
    // It is the last of the group's plans, and every member sees it.
    for token in [DEMO_TOKEN, &mara, &jonas] {
        let listed = plans_from(&router, token, &group, day(18)).await;
        assert_eq!(listed.len(), 4);
        assert_eq!(listed[3], plan);
    }

    // The others say that they are in, and take it back.
    let answer = set_in(&router, &mara, &plan, true).await;
    let with_mara: PlanResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(handles(&with_mara.plan.pilots_in), [DEMO_HANDLE, "mara"]);
    // Who is in is no change to the plan.
    assert_eq!(with_mara.plan.changed_at, plan.changed_at);
    assert_eq!(
        in_after(&router, &mara, &plan, true).await,
        [DEMO_HANDLE, "mara"]
    );
    assert_eq!(in_after(&router, &mara, &plan, false).await, [DEMO_HANDLE]);
    assert_eq!(in_after(&router, &mara, &plan, false).await, [DEMO_HANDLE]);
    assert_eq!(
        in_after(&router, &jonas, &plan, true).await,
        [DEMO_HANDLE, "jonas"]
    );
    // The author takes their word back like anyone, and the plan stays.
    assert_eq!(in_after(&router, DEMO_TOKEN, &plan, false).await, ["jonas"]);
    assert_eq!(plans_from(&router, &mara, &group, day(21)).await.len(), 1);

    // Only its author changes what a plan says: the keeper's own plan here,
    // so that being the keeper is not what decides.
    let maras: PlanId = DEMO_PLANS[1].parse().unwrap();
    let changed = PlanRequest {
        place: "Beispielwiese, anderer Parkplatz".to_owned(),
        ..a_plan(day(22))
    };
    let change = |token: &str, plan: &PlanId, request: &PlanRequest| {
        ask(Method::PUT, &plan_path(&group, plan))
            .token(token)
            .json(request)
            .send(&router)
    };
    for token in [DEMO_TOKEN, &jonas] {
        change(token, &maras, &changed)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::NotPlanAuthor);
    }
    change(&mara, &plan.id, &changed)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotPlanAuthor);
    let answer = change(DEMO_TOKEN, &plan.id, &changed).await;
    let now: PlanResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    // A plan is sent whole: what the change leaves out, the plan no
    // longer has. Who is in stays.
    assert_eq!((now.plan.day, &now.plan.place), (day(22), &changed.place));
    assert_eq!(
        (now.plan.position, now.plan.meet_at, &now.plan.note),
        (None, None, &None)
    );
    assert_eq!(handles(&now.plan.pilots_in), ["jonas"]);
    assert_eq!(now.plan.created_at, plan.created_at);
    let listed = plans_from(&router, &jonas, &group, day(22)).await;
    assert_eq!(listed, std::slice::from_ref(&now.plan));

    // Deleting: its author may, the keeper may, and no other member.
    let delete = |token: &str, plan: &PlanId| {
        ask(Method::DELETE, &plan_path(&group, plan))
            .token(token)
            .send(&router)
    };
    delete(&jonas, &maras)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotPlanAuthor);
    delete(&mara, &plan.id)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotPlanAuthor);
    let answer = delete(&mara, &maras).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    answer.is_private();
    delete(&mara, &maras)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    let jonass: PlanId = DEMO_PLANS[2].parse().unwrap();
    let answer = delete(DEMO_TOKEN, &jonass).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    let left = plans_from(&router, &mara, &group, day(1)).await;
    let ids: Vec<&str> = left.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(ids, [DEMO_PLANS[0], plan.id.as_str()]);

    // A plan is a plan of its own group: the other group's plan is none
    // under this group, also for those who are in both.
    let others: PlanId = DEMO_OTHER_PLAN.parse().unwrap();
    for token in [&mara, &jonas] {
        delete(token, &others)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
        ask(Method::PUT, &plan_in_path(&group, &others))
            .token(token)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    }
    change(&jonas, &others, &changed)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    for odd in ["x", "in", "..%2F.."] {
        ask(Method::DELETE, &format!("/v1/groups/{group}/plans/{odd}"))
            .token(&mara)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    }
}

#[tokio::test]
async fn what_is_no_name_and_no_plan_is_refused_and_nothing_is_kept() {
    let (router, _store, _clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;

    // A name is one line of 1 to 60 characters, counted after cleaning.
    let long = "ä".repeat(MAX_GROUP_NAME_LENGTH);
    for bad in ["", "   ", "\u{200b}\n", &format!("{long}x")] {
        let said = try_make(&router, &keeper, bad)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidGroupName);
        assert!(said.contains("name of a group"), "{said}");
        assert!(!said.contains('ä'), "{said}");
    }
    assert!(my_groups(&router, &keeper).await.is_empty());
    let made = make(&router, &keeper, &format!("  {long}\u{7} ")).await;
    assert_eq!(made.group.name, long);
    let group = made.group.id.clone();
    for bad in ["", &format!("{long}x")] {
        ask(Method::PATCH, &group_path(&group))
            .token(&keeper)
            .json(&UpdateGroupRequest::rename(bad.to_owned()))
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidGroupName);
    }
    assert_eq!(seen(&router, &keeper, &group).await.group.name, long);

    // A plan is for a day from the server's yesterday to a year ahead. The
    // clock shows 18 November 2026.
    let refused = |plan: PlanRequest| {
        let (router, keeper, group) = (&router, &keeper, &group);
        async move {
            try_post(router, keeper, group, &plan)
                .await
                .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan)
        }
    };
    let said = refused(a_plan(day(16))).await;
    assert!(
        said.contains("2026-11-17") && said.contains("2027-11-18"),
        "{said}"
    );
    assert!(said.contains("2026-11-18 in UTC"), "{said}");
    let year_ahead = NaiveDate::from_ymd_opt(2027, 11, 18).unwrap();
    refused(a_plan(year_ahead + Duration::days(1))).await;
    refused(a_plan(NaiveDate::from_ymd_opt(9999, 12, 31).unwrap())).await;
    refused(a_plan(NaiveDate::from_ymd_opt(1, 1, 1).unwrap())).await;
    // Its place is one line of 1 to 80 characters, its note at most 500.
    for place in ["", " \n ", &"x".repeat(MAX_PLAN_PLACE_LENGTH + 1)] {
        let said = refused(PlanRequest {
            place: place.to_owned(),
            ..a_plan(day(19))
        })
        .await;
        assert!(said.contains("place of a plan"), "{said}");
    }
    let said = refused(PlanRequest {
        note: Some("Zanzibar ".repeat(60)),
        ..a_plan(day(19))
    })
    .await;
    assert!(said.contains("note of a plan has at most 500"), "{said}");
    assert!(!said.contains("Zanzibar"), "{said}");
    assert!(
        plans_from(&router, &keeper, &group, day(1))
            .await
            .is_empty()
    );

    // What is taken: the first and the last day, and text at its longest,
    // cleaned.
    let yesterday = post(&router, &keeper, &group, &a_plan(day(17))).await;
    post(&router, &keeper, &group, &a_plan(year_ahead)).await;
    let full = PlanRequest {
        place: format!("  Muster\tberg\u{202e},\n {} ", "x".repeat(60)),
        note: Some(format!("{}\r\nline two\u{1b}[2J \n", "ä".repeat(480))),
        ..a_plan(day(19))
    };
    let kept = post(&router, &keeper, &group, &full).await;
    assert_eq!(kept.place, format!("Muster berg, {}", "x".repeat(60)));
    assert_eq!(
        kept.note.as_deref().unwrap(),
        format!("{}\nline two[2J", "ä".repeat(480))
    );
    assert!(kept.note.as_deref().unwrap().chars().count() <= MAX_PLAN_NOTE_LENGTH);
    // A note of nothing but white space is no note.
    let blank = PlanRequest {
        note: Some(" \n\t".to_owned()),
        ..a_plan(day(19))
    };
    assert_eq!(post(&router, &keeper, &group, &blank).await.note, None);
    // A change is held to the same.
    let change = |plan: PlanRequest| {
        ask(Method::PUT, &plan_path(&group, &yesterday.id))
            .token(&keeper)
            .json(&plan)
            .send(&router)
    };
    change(a_plan(day(16)))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan);
    change(PlanRequest {
        place: String::new(),
        ..a_plan(day(19))
    })
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan);

    // A body that is no plan: a day that is none, a time that is none, a
    // place on no globe.
    for body in [
        json!({ "place": "Musterberg" }),
        json!({ "day": "tomorrow", "place": "Musterberg" }),
        json!({ "day": "2026-11-31", "place": "Musterberg" }),
        json!({ "day": "2026-11-19", "place": "Musterberg", "meet_at": "9 o'clock" }),
        json!({ "day": "2026-11-19", "place": "Musterberg", "meet_at": "24:00" }),
        json!({ "day": "2026-11-19", "place": "Musterberg", "position": { "lat": 147.6, "lon": 11.8 } }),
        json!({ "day": "2026-11-19", "place": 7 }),
    ] {
        ask(Method::POST, &group_plans_path(&group))
            .token(&keeper)
            .json(&body)
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    }
    // A list asked for with days that are none.
    for (query, code) in [
        ("from=tomorrow", ErrorCode::InvalidDay),
        ("to=2026-11-31", ErrorCode::InvalidDay),
        ("from=2026-11-18T19:05:00Z", ErrorCode::InvalidDay),
        ("from=2026-1-5", ErrorCode::InvalidDay),
        ("from=2026-11-20&to=2026-11-19", ErrorCode::InvalidRange),
    ] {
        for path in [group_plans_path(&group), PLAN_BOARD_PATH.to_owned()] {
            ask(Method::GET, &format!("{path}?{query}"))
                .token(&keeper)
                .send(&router)
                .await
                .error(StatusCode::BAD_REQUEST, code);
        }
    }
    assert_eq!(plans_from(&router, &keeper, &group, day(1)).await.len(), 4);
}

#[tokio::test]
async fn plans_of_days_gone_by_are_gone_from_every_answer() {
    let (router, store, clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    let made = make(&router, &keeper, "Mustergruppe").await;
    let group = made.group.id.clone();
    // The evening of the 18th. Plans for yesterday, today and tomorrow.
    let mut plans = Vec::new();
    for on in [17, 18, 19] {
        plans.push(post(&router, &keeper, &group, &a_plan(day(on))).await);
    }
    let days_listed = |from: Option<u32>| {
        let (router, keeper, group) = (&router, &keeper, &group);
        async move {
            let request = PlansRequest {
                group: group.clone(),
                days: PlanDays {
                    from: from.map(day),
                    to: None,
                },
            };
            let listed: PlansResponse = ask(Method::GET, &request.path_and_query())
                .token(keeper)
                .send(router)
                .await
                .ok(StatusCode::OK);
            let ours: Vec<NaiveDate> = listed.plans.iter().map(|plan| plan.day).collect();
            let on_the_board = board(
                router,
                keeper,
                PlanDays {
                    from: from.map(day),
                    to: None,
                },
            )
            .await;
            let theirs: Vec<NaiveDate> = on_the_board.plans.iter().map(|plan| plan.day).collect();
            assert_eq!(ours, theirs, "the board and the group's list agree");
            ours
        }
    };

    // Without a first day the list begins at the server's date; a plan of
    // yesterday is there for whoever asks for it.
    assert_eq!(days_listed(None).await, [day(18), day(19)]);
    assert_eq!(days_listed(Some(17)).await, [day(17), day(18), day(19)]);
    assert_eq!(days_listed(Some(1)).await, [day(17), day(18), day(19)]);

    // Two days on: the 20th. The plan for the 17th is three days old and
    // gone from every answer, whatever day is asked from; the plan for the
    // 18th is two days old and still there.
    clock.advance(Duration::days(2));
    assert_eq!(days_listed(None).await, Vec::<NaiveDate>::new());
    assert_eq!(days_listed(Some(1)).await, [day(18), day(19)]);
    // It is no plan to change, to delete or to be in on. The store has
    // not removed it yet: this is the API's doing.
    let stored = store
        .plans_for(&store_id(&store, "keeper").await, NaiveDate::MIN, None, 10)
        .await
        .unwrap();
    assert_eq!(stored.len(), 3);
    let gone = &plans[0];
    ask(Method::PUT, &plan_path(&group, &gone.id))
        .token(&keeper)
        .json(&a_plan(day(21)))
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    ask(Method::DELETE, &plan_path(&group, &gone.id))
        .token(&keeper)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    for is_in in [true, false] {
        set_in(&router, &keeper, gone, is_in)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    }
    // The one that is still there takes all of that.
    assert_eq!(
        in_after(&router, &keeper, &plans[1], false).await,
        Vec::<String>::new()
    );
    // A plan can no longer be made for the 18th, which is the day before
    // yesterday now, and can for the 19th.
    try_post(&router, &keeper, &group, &a_plan(day(18)))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan);
    post(&router, &keeper, &group, &a_plan(day(19))).await;

    // A day later the next one is gone.
    clock.advance(Duration::days(1));
    assert_eq!(days_listed(Some(1)).await, [day(19), day(19)]);
}

/// The identifier the store knows an account by.
async fn store_id(store: &MemoryStore, handle: &str) -> String {
    store.account(handle).await.unwrap().unwrap().id
}

// --- Bounds ---------------------------------------------------------------------------

#[tokio::test]
async fn groups_and_plans_are_bounded() {
    let small = Limits {
        groups_per_account: 2,
        members_per_group: 2,
        plans_per_group: 3,
        ..Limits::default()
    };
    let (router, _store, clock) = server_with(small);
    let keeper = sign_up(&router, "keeper").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let bernd = sign_up(&router, "bernd").await.token;

    // A group of two takes no third member.
    let first = make(&router, &keeper, "Mustergruppe eins").await;
    come_in(&router, &anna, &first.invite_code).await;
    let said = try_join(&router, &bernd, first.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::GroupFull);
    assert!(said.contains("2 members"), "{said}");
    assert!(my_groups(&router, &bernd).await.is_empty());
    // Who is in it is told so, full or not.
    let again: JoinGroupResponse = try_join(&router, &anna, first.invite_code.expose())
        .await
        .ok(StatusCode::OK);
    assert!(!again.joined);

    // An account in two groups makes no third and comes into no third.
    let second = make(&router, &keeper, "Mustergruppe zwei").await;
    let said = try_make(&router, &keeper, "Mustergruppe drei")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::TooManyGroups);
    assert!(said.contains("2 groups"), "{said}");
    let annas = make(&router, &anna, "Annas Mustergruppe").await;
    try_join(&router, &anna, second.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::TooManyGroups);
    try_join(&router, &keeper, annas.invite_code.expose())
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::TooManyGroups);
    // Leaving one makes room.
    let answer = remove(&router, &anna, &first.group.id, "anna").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    come_in(&router, &anna, &second.invite_code).await;

    // Two requests at the same moment for the last place of a group: one
    // gets it.
    let carla = sign_up(&router, "carla").await.token;
    let code = first.invite_code.expose();
    let (a, b) = tokio::join!(
        try_join(&router, &bernd, code),
        try_join(&router, &carla, code),
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::FORBIDDEN]);
    assert_eq!(
        seen(&router, &keeper, &first.group.id).await.members.len(),
        2
    );

    // A group of three plans takes no fourth, until one is deleted. Plans
    // that are gone take no place.
    let group = &second.group.id;
    let old = post(&router, &keeper, group, &a_plan(day(17))).await;
    for on in [18, 19] {
        post(&router, &anna, group, &a_plan(day(on))).await;
    }
    let said = try_post(&router, &keeper, group, &a_plan(day(20)))
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::GroupPlansFull);
    assert!(said.contains("3 plans"), "{said}");
    assert_eq!(old.day, day(17));
    // Two days on, the plan for the 17th is gone: there is room for one
    // more, and for no second.
    clock.advance(Duration::days(2));
    post(&router, &keeper, group, &a_plan(day(21))).await;
    try_post(&router, &keeper, group, &a_plan(day(22)))
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::GroupPlansFull);
}

#[tokio::test]
async fn what_an_account_does_in_a_day_is_counted() {
    let counted = Limits {
        groups_made_per_day: 3,
        joins_per_day: 4,
        plans_posted_per_day: 5,
        ..Limits::default()
    };
    let (router, _store, clock) = server_with(counted);
    let keeper = sign_up(&router, "keeper").await.token;
    let anna = sign_up(&router, "anna").await.token;

    // Groups made, whatever becomes of them: one that is made, one whose
    // name is none, one that is deleted again.
    let made = make(&router, &keeper, "Mustergruppe").await;
    try_make(&router, &keeper, "")
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidGroupName);
    let second = make(&router, &keeper, "Zweite Mustergruppe").await;
    let answer = try_make(&router, &keeper, "Dritte Mustergruppe").await;
    let said = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(said.contains("as many groups today"), "{said}");
    let wait: u64 = answer.header(header::RETRY_AFTER).parse().unwrap();
    assert!((86_000..=86_400).contains(&wait), "{wait}");
    // Another account is not affected, and what a browser was made to
    // send is not counted.
    for _ in 0..5 {
        ask(Method::POST, GROUPS_PATH)
            .token(&anna)
            .header(header::CONTENT_TYPE, "text/plain")
            .bytes(r#"{"name":"x"}"#)
            .send(&router)
            .await
            .error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                ErrorCode::UnsupportedMediaType,
            );
    }
    make(&router, &anna, "Annas Mustergruppe").await;

    // Codes tried, right or wrong.
    for _ in 0..3 {
        try_join(&router, &anna, "0000-0000-0000-0000")
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    }
    come_in(&router, &anna, &made.invite_code).await;
    // The fifth is refused, also with a code that is right.
    let answer = try_join(&router, &anna, second.invite_code.expose()).await;
    let said = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(said.contains("invite codes today"), "{said}");
    assert!(!said.contains(second.invite_code.expose()), "{said}");
    assert_eq!(my_groups(&router, &anna).await.len(), 2);

    // Plans posted or changed.
    let group = &made.group.id;
    let plan = post(&router, &anna, group, &a_plan(day(19))).await;
    for _ in 0..2 {
        ask(Method::PUT, &plan_path(group, &plan.id))
            .token(&anna)
            .json(&a_plan(day(20)))
            .send(&router)
            .await
            .ok::<PlanResponse>(StatusCode::OK);
    }
    try_post(
        &router,
        &anna,
        group,
        &PlanRequest {
            place: String::new(),
            ..a_plan(day(19))
        },
    )
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPlan);
    post(&router, &anna, group, &a_plan(day(21))).await;
    let answer = try_post(&router, &anna, group, &a_plan(day(22))).await;
    let said = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(said.contains("plans today"), "{said}");
    ask(Method::PUT, &plan_path(group, &plan.id))
        .token(&anna)
        .json(&a_plan(day(20)))
        .send(&router)
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    // Saying "I am in" is not counted, nor is reading or deleting.
    for _ in 0..10 {
        in_after(&router, &anna, &plan, false).await;
        in_after(&router, &anna, &plan, true).await;
    }
    assert_eq!(plans_from(&router, &anna, group, day(1)).await.len(), 2);

    // A day later each count begins again.
    clock.advance(Duration::days(1) + Duration::seconds(1));
    make(&router, &keeper, "Dritte Mustergruppe").await;
    come_in(&router, &anna, &second.invite_code).await;
    post(&router, &anna, group, &a_plan(day(22))).await;
}

#[tokio::test]
async fn a_join_while_the_code_is_renewed_comes_in_under_the_old_code_or_not_at_all() {
    let (router, _store, _clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    for round in 0..4 {
        let made = make(&router, &keeper, &format!("Mustergruppe {round}")).await;
        let group = made.group.id.clone();
        let early = sign_up(&router, &format!("early{round}")).await.token;
        let late = sign_up(&router, &format!("late{round}")).await.token;
        let old = made.invite_code.expose();

        let (joining, renewing) = tokio::join!(
            try_join(&router, &early, old),
            ask(Method::POST, &group_invite_path(&group))
                .token(&keeper)
                .send(&router),
        );

        let renewed: GroupInviteResponse = renewing.ok(StatusCode::OK);
        let members = seen(&router, &keeper, &group).await.members.len();
        match joining.status {
            StatusCode::CREATED => assert_eq!(members, 2),
            StatusCode::FORBIDDEN => {
                joining.error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
                assert_eq!(members, 1);
            }
            other => panic!("{other}"),
        }
        // Either way the old code is over and the new one works.
        try_join(&router, &late, old)
            .await
            .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
        come_in(&router, &late, &renewed.invite_code).await;
    }
}

// --- A store in trouble ----------------------------------------------------------------

#[tokio::test]
async fn a_database_that_is_down_is_said_to_be_down() {
    let api = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(FixedClock(adapters::recorded_at())),
        [],
        settings(),
    );
    let router = api.router(&Cors::Any);
    // The token cannot be looked up, so every request ends the same way,
    // and none says anything of what it carried.
    let token = format!("venti_{}", "7".repeat(52));
    let group = id_of(DEMO_GROUP);
    for answer in [
        try_make(&router, &token, "Mustergruppe").await,
        try_join(&router, &token, DEMO_GROUP_CODE).await,
        look(&router, &token, &group).await,
        try_post(&router, &token, &group, &a_plan(day(19))).await,
    ] {
        let said = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert!(
            !said.contains(DEMO_GROUP) && !said.contains("4444"),
            "{said}"
        );
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
    }
}
