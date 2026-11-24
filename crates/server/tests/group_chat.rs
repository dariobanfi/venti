//! Groups as communities with a chat, through the real router over the
//! development mode's data: public groups found near a place and by name
//! and joined, private groups that take only those invited, owners and
//! admins, messages in pages and since a cursor with their cards and
//! pictures, removing and reporting, muting and what is unread, and the
//! groups from before the chat with their plans as posts.

use axum::Router;
use axum::http::{Method, StatusCode, header};
use venti_api_types::v1::{
    ACCOUNT_UNREAD_PATH, Card, CreateGroupRequest, CreateGroupResponse, ErrorCode,
    GROUP_INVITES_PATH, GROUPS_PATH, GroupId, GroupInviteAnswer, GroupInvitesResponse,
    GroupMeRequest, GroupMeResponse, GroupResponse, GroupRole, GroupsFoundResponse, GroupsResponse,
    Handle, HomeArea, InviteRequest, JoinGroupResponse, Link, LinkKind, MAX_MESSAGE_LENGTH,
    MessageKind, MessageRequest, MessageResponse, MessagesResponse, REPORTS_PATH, RemovedBy,
    ReportReason, ReportRequest, ReportSubject, ReportedMessage, SentPictureResponse,
    UnreadResponse, UpdateGroupRequest, group_admin_path, group_invites_path, group_me_path,
    group_member_path, group_members_path, group_message_path, group_messages_path, group_path,
    group_send_picture_path,
};
use venti_domain::GeoPoint;
use venti_server::accounts::picture::made_up_png;
use venti_server::dev::{
    DEMO_ACCESS_TOKEN, DEMO_GROUP, DEMO_OPEN_PLANS, DEMO_PUBLIC_GROUPS, DEMO_TOKEN,
};
use venti_sources::testing::assert_golden;

mod common;

use common::{Answer, ask, golden, recorded, sign_up};

fn id_of(text: &str) -> GroupId {
    text.parse().unwrap()
}

fn handle(text: &str) -> Handle {
    Handle::new(text).unwrap()
}

/// The Rosenheimer Eagles, whose first message carries a flight.
fn eagles() -> GroupId {
    id_of(DEMO_PUBLIC_GROUPS[1])
}

async fn get(router: &Router, token: &str, path: &str) -> Answer {
    ask(Method::GET, path).token(token).send(router).await
}

async fn post_message(
    router: &Router,
    token: &str,
    group: &GroupId,
    request: &MessageRequest,
) -> Answer {
    ask(Method::POST, &group_messages_path(group))
        .token(token)
        .json(request)
        .send(router)
        .await
}

fn text(words: &str) -> MessageRequest {
    MessageRequest {
        text: words.to_owned(),
        ..MessageRequest::default()
    }
}

async fn enter(router: &Router, token: &str, group: &GroupId) -> Answer {
    ask(Method::POST, &group_members_path(group))
        .token(token)
        .send(router)
        .await
}

#[tokio::test]
async fn public_groups_near_munich_are_found_by_place_and_name_and_joined() {
    let router = recorded().await;
    let found: GroupsFoundResponse =
        get(&router, DEMO_TOKEN, "/v1/groups/find?lat=48.14&lon=11.58")
            .await
            .ok(StatusCode::OK);
    let names: Vec<&str> = found
        .groups
        .iter()
        .map(|found| found.group.name.as_str())
        .collect();
    assert_eq!(names[0], "Munich International Paragliders");
    assert_eq!(names.len(), 6, "all six are within 150 km: {names:?}");
    let distances: Vec<f64> = found
        .groups
        .iter()
        .map(|found| found.distance_km.unwrap())
        .collect();
    assert!(
        distances.windows(2).all(|pair| pair[0] <= pair[1]),
        "{distances:?}"
    );
    // Only public groups: the demo account's own private one is not
    // among them, and it is in none of these.
    assert!(
        found
            .groups
            .iter()
            .all(|found| found.group.public && !found.member)
    );
    assert_golden(&golden("groups_found.json"), &found);

    // By place and by name, in any case.
    let by_place: GroupsFoundResponse = get(&router, DEMO_TOKEN, "/v1/groups/find?q=chiemgau")
        .await
        .ok(StatusCode::OK);
    assert_eq!(by_place.groups.len(), 1);
    assert_eq!(by_place.groups[0].group.name, "Rosenheimer Eagles");
    let by_name: GroupsFoundResponse = get(&router, DEMO_TOKEN, "/v1/groups/find?q=HOCHRIES")
        .await
        .ok(StatusCode::OK);
    assert_eq!(
        by_name.groups[0].group.name,
        "Hochries Flugschule Musterwind"
    );
    let nothing: GroupsFoundResponse = get(&router, DEMO_TOKEN, "/v1/groups/find?q=Mustergruppe")
        .await
        .ok(StatusCode::OK);
    assert!(
        nothing.groups.is_empty(),
        "a private group is found by nobody"
    );
    get(&router, DEMO_TOKEN, "/v1/groups/find?lat=48.1")
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPosition);
    get(&router, DEMO_TOKEN, "/v1/groups/find?limit=51")
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidLimit);

    // Anyone signed in reads a public group, and joins it.
    let munich = id_of(DEMO_PUBLIC_GROUPS[0]);
    let shown: GroupResponse = get(&router, DEMO_TOKEN, &group_path(&munich))
        .await
        .ok(StatusCode::OK);
    let me = shown.me.clone().unwrap();
    assert_eq!(me.role, None);
    assert_eq!(shown.admins, [handle("jonas")]);
    assert_golden(&golden("group_public.json"), &shown);
    let joined: JoinGroupResponse = enter(&router, DEMO_TOKEN, &munich)
        .await
        .ok(StatusCode::CREATED);
    assert!(joined.joined);
    assert_eq!(joined.group.member_count, 4);
    let again: JoinGroupResponse = enter(&router, DEMO_TOKEN, &munich).await.ok(StatusCode::OK);
    assert!(!again.joined);
    let listed: GroupsResponse = get(&router, DEMO_TOKEN, GROUPS_PATH)
        .await
        .ok(StatusCode::OK);
    assert_eq!(listed.groups.len(), listed.chats.len());
    let chat = listed
        .chats
        .iter()
        .find(|chat| chat.group == munich)
        .unwrap();
    assert_eq!(chat.me.role, Some(GroupRole::Member));
    assert_eq!(chat.me.unread, 0, "what came before counts as read");
    assert!(chat.last_message.is_some());
}

#[tokio::test]
async fn a_private_group_takes_only_those_its_owner_and_admins_invite() {
    let router = recorded().await;
    let anna = sign_up(&router, "anna").await.token;
    let jonas = sign_up(&router, "jonasb").await.token;
    let request = CreateGroupRequest {
        home: Some(HomeArea {
            center: GeoPoint::new(47.7, 11.8).unwrap(),
            radius_km: 20.0,
            place: Some(" Tegernsee ".to_owned()),
        }),
        description: Some("Just us.".to_owned()),
        ..CreateGroupRequest::named("Quiet ones")
    };
    let made: CreateGroupResponse = ask(Method::POST, GROUPS_PATH)
        .token(DEMO_TOKEN)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let group = made.group.id.clone();
    assert!(!made.group.public);
    assert_eq!(made.group.home.unwrap().place.as_deref(), Some("Tegernsee"));

    // To anyone else it is no group.
    let path = group_path(&group);
    get(&router, &anna, &path)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    enter(&router, &anna, &group)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    get(&router, &anna, &group_messages_path(&group))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);

    // The owner invites by handle; the invited pilot sees the group.
    let invite = |token: &str, who: &str| {
        ask(Method::POST, &group_invites_path(&group))
            .token(token)
            .json(&InviteRequest {
                handle: who.to_owned(),
            })
    };
    let invited: GroupInviteAnswer = invite(DEMO_TOKEN, "@Anna")
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(invited.invite.invited_by.handle.as_str(), "demo");
    invite(DEMO_TOKEN, "nobody-here")
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupMemberNotFound);
    let invites: GroupInvitesResponse = get(&router, &anna, GROUP_INVITES_PATH)
        .await
        .ok(StatusCode::OK);
    assert_eq!(invites.invites.len(), 1);
    assert_eq!(invites.invites[0].group.id, group);
    let seen: GroupResponse = get(&router, &anna, &path).await.ok(StatusCode::OK);
    assert!(seen.me.unwrap().invited);
    get(&router, &anna, &group_messages_path(&group))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    let joined: JoinGroupResponse = enter(&router, &anna, &group).await.ok(StatusCode::CREATED);
    assert!(joined.joined);
    invite(DEMO_TOKEN, "anna")
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::AlreadyInGroup);

    // A member invites nobody; an admin does.
    invite(&anna, "jonasb")
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    let make_admin = |token: &str, who: &str| {
        ask(Method::PUT, &group_admin_path(&group, &handle(who))).token(token)
    };
    make_admin(&anna, "anna")
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    assert_eq!(
        make_admin(DEMO_TOKEN, "anna").send(&router).await.status,
        StatusCode::NO_CONTENT
    );
    invite(&anna, "jonasb")
        .send(&router)
        .await
        .ok::<GroupInviteAnswer>(StatusCode::CREATED);
    enter(&router, &jonas, &group)
        .await
        .ok::<JoinGroupResponse>(StatusCode::CREATED);
    let seen: GroupResponse = get(&router, &anna, &path).await.ok(StatusCode::OK);
    assert_eq!(seen.admins, [handle("anna")]);
    assert_eq!(seen.me.unwrap().role, Some(GroupRole::Admin));

    // An admin renames it and may not make it public; the owner may.
    let renamed: GroupResponse = ask(Method::PATCH, &path)
        .token(&anna)
        .json(&UpdateGroupRequest {
            description: Some(None),
            ..UpdateGroupRequest::rename("Quiet ones, still")
        })
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(renamed.group.name, "Quiet ones, still");
    assert_eq!(renamed.group.description, None);
    let public = UpdateGroupRequest {
        public: Some(true),
        ..UpdateGroupRequest::default()
    };
    ask(Method::PATCH, &path)
        .token(&anna)
        .json(&public)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);

    // An admin removes a member, not the owner.
    let remove = |token: &str, who: &str| {
        ask(Method::DELETE, &group_member_path(&group, &handle(who))).token(token)
    };
    remove(&anna, "demo")
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    assert_eq!(
        remove(&anna, "jonasb").send(&router).await.status,
        StatusCode::NO_CONTENT
    );
    get(&router, &jonas, &path)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    let made_public: GroupResponse = ask(Method::PATCH, &path)
        .token(DEMO_TOKEN)
        .json(&public)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(made_public.group.public);
    get(&router, &jonas, &path)
        .await
        .ok::<GroupResponse>(StatusCode::OK);
}

#[tokio::test]
async fn messages_come_newest_first_and_since_a_cursor_with_their_cards() {
    let router = recorded().await;
    // Anyone signed in reads a public group's messages; the flight is a
    // card with its pilot, take-off, distance, day and a small track.
    let path = group_messages_path(&eagles());
    let page: MessagesResponse = get(&router, DEMO_TOKEN, &path).await.ok(StatusCode::OK);
    assert_eq!(page.messages.len(), 2);
    assert_eq!(
        page.messages[1].text,
        "My flight from yesterday, the inversion broke late."
    );
    let Card::Flight(flight) = &page.messages[1].cards[0] else {
        panic!("no flight card: {:?}", page.messages[1].cards);
    };
    assert_eq!(flight.pilot.handle.as_str(), "mara");
    assert!(flight.distance_km > 0.0);
    assert!(
        (3..=48).contains(&flight.track.len()),
        "{}",
        flight.track.len()
    );
    assert_eq!(page.older, None);
    assert!(!page.more);
    assert_golden(&golden("group_messages.json"), &page);

    // A program's token reads no messages: they are conversation.
    get(&router, DEMO_ACCESS_TOKEN, &path)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::SessionRequired);
    // Nor does anyone post without being in the group.
    post_message(&router, DEMO_TOKEN, &eagles(), &text("Hi"))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    enter(&router, DEMO_TOKEN, &eagles())
        .await
        .ok::<JoinGroupResponse>(StatusCode::CREATED);

    // A message with text and links of every kind, each a card; one that
    // names nothing there is is a card that says so.
    let request = MessageRequest {
        text: "Tomorrow at the Hochries? @mara\nSee the plan.".to_owned(),
        pictures: Vec::new(),
        links: vec![
            Link {
                kind: LinkKind::OpenPlan,
                id: DEMO_OPEN_PLANS[0].to_owned(),
            },
            Link {
                kind: LinkKind::Pilot,
                id: "mara".to_owned(),
            },
            Link {
                kind: LinkKind::Site,
                id: "venti:0000000000000000000000s001".to_owned(),
            },
        ],
    };
    assert_golden(&golden("group_message_request.json"), &request);
    let posted: MessageResponse = post_message(&router, DEMO_TOKEN, &eagles(), &request)
        .await
        .ok(StatusCode::CREATED);
    let message = posted.message;
    assert_eq!(message.kind, MessageKind::Text);
    assert!(matches!(&message.cards[0], Card::OpenPlan(plan) if !plan.place.is_empty()));
    assert!(matches!(&message.cards[1], Card::Pilot { pilot } if pilot.handle.as_str() == "mara"));
    assert_eq!(message.cards.len(), 3);

    // What is new since the first page: the new one.
    let since: MessagesResponse = get(&router, DEMO_TOKEN, &format!("{path}?since={}", page.since))
        .await
        .ok(StatusCode::OK);
    assert_eq!(since.messages.len(), 1);
    assert_eq!(since.messages[0].id, message.id);
    let nothing: MessagesResponse = get(
        &router,
        DEMO_TOKEN,
        &format!("{path}?since={}", since.since),
    )
    .await
    .ok(StatusCode::OK);
    assert!(nothing.messages.is_empty());
    assert_eq!(nothing.since, since.since);

    // Pages: one at a time, older and older.
    let first: MessagesResponse = get(&router, DEMO_TOKEN, &format!("{path}?limit=1"))
        .await
        .ok(StatusCode::OK);
    assert_eq!(first.messages[0].id, message.id);
    let older = first.older.clone().unwrap();
    let second: MessagesResponse = get(
        &router,
        DEMO_TOKEN,
        &format!("{path}?limit=1&before={older}"),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!(
        second.messages[0].text,
        "Nice line! How was the wind on top?"
    );
    get(&router, DEMO_TOKEN, &format!("{path}?before=x"))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);

    // Bounds, checked before anything is kept.
    for refused in [
        text(""),
        text(&"x".repeat(MAX_MESSAGE_LENGTH + 1)),
        MessageRequest {
            pictures: vec!["0123456789abcdefghjkmnpqrs".to_owned(); 5],
            ..text("five")
        },
        MessageRequest {
            links: vec![Link {
                kind: LinkKind::Flight,
                id: "not a flight".to_owned(),
            }],
            ..text("bad")
        },
    ] {
        post_message(&router, DEMO_TOKEN, &eagles(), &refused)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidMessage);
    }

    // Removed by its author, it shows as removed, and is new since.
    let removed: MessageResponse = ask(Method::DELETE, &group_message_path(&eagles(), &message.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(removed.message.text, "");
    assert!(removed.message.cards.is_empty());
    assert_eq!(removed.message.removed.unwrap().by, RemovedBy::Author);
    let changed: MessagesResponse = get(
        &router,
        DEMO_TOKEN,
        &format!("{path}?since={}", since.since),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!(changed.messages.len(), 1);
    assert!(changed.messages[0].removed.is_some());
    // Another member's message is not the demo's to remove.
    let others = &page.messages[0].id;
    ask(Method::DELETE, &group_message_path(&eagles(), others))
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
}

#[tokio::test]
async fn pictures_are_the_servers_own_copies_and_go_with_their_message() {
    let router = recorded().await;
    enter(&router, DEMO_TOKEN, &eagles())
        .await
        .ok::<JoinGroupResponse>(StatusCode::CREATED);
    let send = |token: &str, group: &GroupId| {
        ask(Method::POST, &group_send_picture_path(group))
            .token(token)
            .header(header::CONTENT_TYPE, "image/png")
            .bytes(made_up_png(2400, 1200, 7))
    };
    let sent: SentPictureResponse = send(DEMO_TOKEN, &eagles())
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    // Scaled down to 1600 pixels on its longer side, its shape kept.
    assert_eq!((sent.picture.width, sent.picture.height), (1600, 800));
    let shown = ask(Method::GET, &sent.picture.path).send(&router).await;
    assert_eq!(shown.status, StatusCode::OK);
    assert_eq!(shown.header(header::CONTENT_TYPE), "image/webp");
    // Not a member: no picture.
    send(DEMO_TOKEN, &id_of(DEMO_PUBLIC_GROUPS[5]))
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);

    let with = MessageRequest {
        pictures: vec![sent.picture.id.clone()],
        ..text("")
    };
    let posted: MessageResponse = post_message(&router, DEMO_TOKEN, &eagles(), &with)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(posted.message.pictures[0].path, sent.picture.path);
    // Once in a message it is in no other.
    post_message(&router, DEMO_TOKEN, &eagles(), &with)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidMessage);

    // Anyone who reads the group reports it.
    let reader = sign_up(&router, "anna").await.token;
    let report = ReportRequest {
        subject: ReportSubject::GroupPicture(ReportedMessage {
            group: eagles().to_string(),
            id: sent.picture.id.clone(),
        }),
        reason: ReportReason::Offensive,
        text: None,
    };
    ask(Method::POST, REPORTS_PATH)
        .token(&reader)
        .json(&report)
        .send(&router)
        .await
        .ok::<serde_json::Value>(StatusCode::CREATED);

    // Removed with its message.
    ask(
        Method::DELETE,
        &group_message_path(&eagles(), &posted.message.id),
    )
    .token(DEMO_TOKEN)
    .send(&router)
    .await
    .ok::<MessageResponse>(StatusCode::OK);
    let gone = ask(Method::GET, &sent.picture.path).send(&router).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn each_group_counts_what_is_unread_and_a_member_mutes_it() {
    let router = recorded().await;
    let anna = sign_up(&router, "anna").await.token;
    let munich = id_of(DEMO_PUBLIC_GROUPS[0]);
    for token in [DEMO_TOKEN, anna.as_str()] {
        enter(&router, token, &munich)
            .await
            .ok::<JoinGroupResponse>(StatusCode::CREATED);
    }
    let unread = |router: &Router| {
        let router = router.clone();
        async move {
            let count: UnreadResponse = get(&router, DEMO_TOKEN, ACCOUNT_UNREAD_PATH)
                .await
                .ok(StatusCode::OK);
            count
        }
    };
    let before = unread(&router).await;
    assert_eq!(before.groups, 0);
    let posted: MessageResponse = post_message(&router, &anna, &munich, &text("Who flies?"))
        .await
        .ok(StatusCode::CREATED);
    post_message(&router, &anna, &munich, &text("Anyone?"))
        .await
        .ok::<MessageResponse>(StatusCode::CREATED);
    let after = unread(&router).await;
    assert_eq!(after.groups, 2);
    assert_eq!(
        after.total, before.total,
        "the groups are beside the notifications"
    );
    // The author has read their own.
    let hers: UnreadResponse = get(&router, &anna, ACCOUNT_UNREAD_PATH)
        .await
        .ok(StatusCode::OK);
    assert_eq!(hers.groups, 0);

    // Read up to the first, then muted.
    let me: GroupMeResponse = ask(Method::PATCH, &group_me_path(&munich))
        .token(DEMO_TOKEN)
        .json(&GroupMeRequest {
            muted: Some(true),
            read_up_to: Some(posted.message.cursor.clone()),
        })
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(me.me.unread, 1);
    assert!(me.me.muted);
    assert_eq!(me.me.read_up_to, posted.message.cursor);
    let listed: GroupsResponse = get(&router, DEMO_TOKEN, GROUPS_PATH)
        .await
        .ok(StatusCode::OK);
    let chat = listed
        .chats
        .iter()
        .find(|chat| chat.group == munich)
        .unwrap();
    assert_eq!(chat.me.unread, 1);
    assert_eq!(chat.last_message.as_ref().unwrap().text, "Anyone?");
    // A pilot who is not in it has nothing to mark.
    let outsider = sign_up(&router, "jonasb").await.token;
    ask(Method::PATCH, &group_me_path(&munich))
        .token(&outsider)
        .json(&GroupMeRequest::default())
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
}

#[tokio::test]
async fn a_message_is_reported_by_anyone_who_reads_it() {
    let router = recorded().await;
    let reader = sign_up(&router, "anna").await.token;
    let page: MessagesResponse = get(&router, &reader, &group_messages_path(&eagles()))
        .await
        .ok(StatusCode::OK);
    let report = |id: &str, group: &GroupId| ReportRequest {
        subject: ReportSubject::GroupMessage(ReportedMessage {
            group: group.to_string(),
            id: id.to_owned(),
        }),
        reason: ReportReason::Offensive,
        text: None,
    };
    let send = |request: ReportRequest| {
        let router = router.clone();
        let reader = reader.clone();
        async move {
            ask(Method::POST, REPORTS_PATH)
                .token(&reader)
                .json(&request)
                .send(&router)
                .await
        }
    };
    send(report(&page.messages[0].id, &eagles()))
        .await
        .ok::<serde_json::Value>(StatusCode::CREATED);
    // A message of a private group is no message to a pilot outside it.
    send(report(&page.messages[0].id, &id_of(DEMO_GROUP)))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
}

#[tokio::test]
async fn the_groups_from_before_the_chat_have_their_plans_as_posts() {
    let router = recorded().await;
    // The development mode's own group was made as a group of before:
    // private, with its plans, which are posts of kind plan in its chat.
    let group = id_of(DEMO_GROUP);
    let shown: GroupResponse = get(&router, DEMO_TOKEN, &group_path(&group))
        .await
        .ok(StatusCode::OK);
    assert!(!shown.group.public);
    assert_eq!(shown.me.unwrap().role, Some(GroupRole::Owner));
    let page: MessagesResponse = get(&router, DEMO_TOKEN, &group_messages_path(&group))
        .await
        .ok(StatusCode::OK);
    assert!(!page.messages.is_empty());
    assert!(
        page.messages
            .iter()
            .all(|message| message.kind == MessageKind::Plan)
    );
    let with_plan = page
        .messages
        .iter()
        .find_map(|message| message.plan.as_ref())
        .unwrap();
    assert_eq!(with_plan.group, group);
    assert_golden(&golden("group_plan_posts.json"), &page);

    // "I'm in" on its card works as before, and the post is new since.
    let path = venti_api_types::v1::plan_in_path(&group, &with_plan.id);
    ask(Method::DELETE, &path)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok::<serde_json::Value>(StatusCode::OK);
    let since: MessagesResponse = get(
        &router,
        DEMO_TOKEN,
        &format!("{}?since={}", group_messages_path(&group), page.since),
    )
    .await
    .ok(StatusCode::OK);
    assert_eq!(since.messages.len(), 1);
    assert_eq!(since.messages[0].plan.as_ref().unwrap().id, with_plan.id);
}

#[tokio::test]
async fn an_admin_is_made_and_unmade_by_the_owner_alone() {
    let router = recorded().await;
    // In the Munich group mara is the owner and jonas an admin.
    let munich = id_of(DEMO_PUBLIC_GROUPS[0]);
    enter(&router, DEMO_TOKEN, &munich)
        .await
        .ok::<JoinGroupResponse>(StatusCode::CREATED);
    let unmake =
        ask(Method::DELETE, &group_admin_path(&munich, &handle("jonas"))).token(DEMO_TOKEN);
    unmake
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::NotGroupKeeper);
    let invites: GroupInvitesResponse = get(&router, DEMO_TOKEN, GROUP_INVITES_PATH)
        .await
        .ok(StatusCode::OK);
    assert!(invites.invites.is_empty());
}
