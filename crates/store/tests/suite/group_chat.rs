//! Groups as communities with a chat: public groups found by place and
//! name, private groups seen by their members and those invited, owners
//! and admins, the messages of a group with their pages, versions,
//! removals and read marks, and a group's plans as posts.

use chrono::{NaiveDate, NaiveTime};
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{
    Account, GroupAct, GroupJoining, GroupPicture, GroupProfile, GroupRole, GroupRoom, GroupSearch,
    HomeArea, InviteRoom, LinkKind, MemberRemoval, Message, MessageKind, MessageLink, MessagePage,
    MessagePosting, MessageRemoval, NewGroup, NewMessage, NewPlan, PlanChange, PlanDetails,
    PlanRoom, PlanSaving, ProfileChange, RemovedBy, Store,
};

use super::accounts::{create, id, secret};
use super::at;

const ROOM: GroupRoom = GroupRoom {
    groups_per_account: 10,
    members_per_group: 10,
};

const INVITES: InviteRoom = InviteRoom {
    invites_per_group: 2,
};

fn home(lat: f64, lon: f64, place: &str) -> Option<HomeArea> {
    Some(HomeArea {
        center: GeoPoint::new(lat, lon).unwrap(),
        radius_km: 30.0,
        place: Some(place.to_owned()),
    })
}

async fn group(
    store: &impl Store,
    name: &str,
    owner: &Account,
    public: bool,
    home: Option<HomeArea>,
) -> String {
    let new = NewGroup {
        id: id(name),
        name: name.to_owned(),
        created_at: at(9, 0),
        keeper_id: owner.id.clone(),
        // Every group a code of its own.
        invite: secret(
            name.bytes()
                .fold(7_u8, |sum, byte| sum.wrapping_mul(31).wrapping_add(byte)),
        ),
        profile: GroupProfile {
            public,
            description: Some("Made up.".to_owned()),
            picture: None,
            home,
        },
    };
    store.create_group(&new, &ROOM).await.unwrap();
    new.id
}

async fn enter(store: &impl Store, group: &str, account: &Account) -> GroupJoining {
    store
        .enter_group(group, &account.id, &ROOM, at(9, 30))
        .await
        .unwrap()
}

fn new_message(name: &str, group: &str, author: &Account, text: &str, at: Timestamp) -> NewMessage {
    NewMessage {
        id: id(name),
        group_id: group.to_owned(),
        author_id: author.id.clone(),
        text: text.to_owned(),
        pictures: Vec::new(),
        links: Vec::new(),
        created_at: at,
    }
}

async fn post(store: &impl Store, new: &NewMessage) -> Message {
    match store.post_message(new).await.unwrap() {
        MessagePosting::Posted(message) => *message,
        other => panic!("not posted: {other:?}"),
    }
}

async fn page(store: &impl Store, group: &str, who: &Account, page: MessagePage) -> Vec<String> {
    store
        .messages(group, &who.id, page, 10)
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|message| message.text)
        .collect()
}

pub(crate) async fn public_groups_are_found_near_a_place_and_by_name(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let munich = group(
        store,
        "Munich Fliers",
        &dario,
        true,
        home(48.137, 11.575, "Munich"),
    )
    .await;
    let rosenheim = group(
        store,
        "Rosenheim Hills",
        &anna,
        true,
        home(47.856, 12.128, "Chiemgau"),
    )
    .await;
    group(
        store,
        "Garmisch",
        &anna,
        true,
        home(47.49, 11.1, "Werdenfels"),
    )
    .await;
    // Private, at Munich too: in no list.
    group(
        store,
        "Munich Secret",
        &dario,
        false,
        home(48.14, 11.58, "Munich"),
    )
    .await;
    // Anna is in the Munich group too: it has more members than the
    // others.
    assert!(matches!(
        enter(store, &munich, &anna).await,
        GroupJoining::Joined(_)
    ));

    let near = |near: Option<GeoPoint>, text: Option<&str>| GroupSearch {
        near,
        within_km: 100.0,
        text: text.map(str::to_owned),
        limit: 10,
    };
    let at_munich = GeoPoint::new(48.1, 11.6).unwrap();
    let found = store
        .find_groups(&dario.id, &near(Some(at_munich), None))
        .await
        .unwrap();
    let names: Vec<&str> = found
        .iter()
        .map(|found| found.group.name.as_str())
        .collect();
    assert_eq!(names, ["Munich Fliers", "Rosenheim Hills", "Garmisch"]);
    assert!(found[0].member);
    assert!(found[0].distance_km.unwrap() < 5.0);
    assert!(!found[1].member);
    assert_eq!(found[0].group.members, 2);

    // Within a smaller circle, the far ones are left out.
    let closer = GroupSearch {
        within_km: 60.0,
        ..near(Some(at_munich), None)
    };
    let found = store.find_groups(&anna.id, &closer).await.unwrap();
    assert_eq!(found.len(), 2, "{found:?}");

    // By name or place, in any case; without a place, the larger first.
    let by_place = store
        .find_groups(&dario.id, &near(None, Some("CHIEMGAU")))
        .await
        .unwrap();
    assert_eq!(by_place.len(), 1);
    assert_eq!(by_place[0].group.id, rosenheim);
    assert_eq!(by_place[0].distance_km, None);
    let by_name = store
        .find_groups(&dario.id, &near(None, Some("munich")))
        .await
        .unwrap();
    let names: Vec<&str> = by_name
        .iter()
        .map(|found| found.group.name.as_str())
        .collect();
    assert_eq!(names, ["Munich Fliers"]);
    assert_eq!(
        by_name[0]
            .group
            .profile
            .home
            .as_ref()
            .unwrap()
            .place
            .as_deref(),
        Some("Munich")
    );
}

pub(crate) async fn a_private_group_is_seen_by_its_members_and_those_invited(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let jonas = create(store, "jonas").await;
    let mara = create(store, "mara").await;
    let private = group(store, "Quiet", &dario, false, None).await;
    post(
        store,
        &new_message("m1", &private, &dario, "Hello", at(10, 0)),
    )
    .await;

    // Nobody else sees it, joins it or reads its messages.
    assert_eq!(
        store.group_seen(&private, &anna.id, 10).await.unwrap(),
        None
    );
    assert_eq!(
        enter(store, &private, &anna).await,
        GroupJoining::NoSuchInvite
    );
    let none = store
        .messages(&private, &anna.id, MessagePage::Before(None), 10)
        .await
        .unwrap();
    assert_eq!(none, None);

    // A member may not invite; the owner may.
    store
        .invite_to_group(&private, &dario.id, "jonas", at(10, 1), &INVITES)
        .await
        .unwrap();
    assert!(matches!(
        enter(store, &private, &jonas).await,
        GroupJoining::Joined(_)
    ));
    let refused = store
        .invite_to_group(&private, &jonas.id, "anna", at(10, 5), &INVITES)
        .await
        .unwrap();
    assert_eq!(refused, GroupAct::NotAllowed);
    let invited = store
        .invite_to_group(&private, &dario.id, "anna", at(10, 5), &INVITES)
        .await
        .unwrap();
    let GroupAct::Done(invite) = invited else {
        panic!("not invited: {invited:?}");
    };
    assert_eq!(invite.invited_by.handle, "dario");
    assert_eq!(
        store.invites_of(&anna.id).await.unwrap(),
        std::slice::from_ref(&invite)
    );
    for nobody in ["nobody", ""] {
        let act = store
            .invite_to_group(&private, &dario.id, nobody, at(10, 5), &INVITES)
            .await
            .unwrap();
        assert_eq!(act, GroupAct::NoSuchPilot);
    }

    // Invited, anna sees the group and not its messages.
    let seen = store
        .group_seen(&private, &anna.id, 10)
        .await
        .unwrap()
        .unwrap();
    assert!(seen.standing.invited);
    assert_eq!(seen.standing.role, None);
    assert!(
        store
            .messages(&private, &anna.id, MessagePage::Before(None), 10)
            .await
            .unwrap()
            .is_none()
    );

    // She comes in with it, and it is used up; she starts with nothing
    // unread.
    assert!(matches!(
        enter(store, &private, &anna).await,
        GroupJoining::Joined(_)
    ));
    assert!(store.invites_of(&anna.id).await.unwrap().is_empty());
    let seen = store
        .group_seen(&private, &anna.id, 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.standing.role, Some(GroupRole::Member));
    assert_eq!(seen.standing.unread, 0);
    assert_eq!(
        page(store, &private, &anna, MessagePage::Before(None)).await,
        ["Hello"]
    );
    let again = store
        .invite_to_group(&private, &dario.id, "anna", at(10, 6), &INVITES)
        .await
        .unwrap();
    assert_eq!(again, GroupAct::AlreadyIn);

    // An invite declined is gone; one taken back too.
    store
        .invite_to_group(&private, &dario.id, "mara", at(10, 7), &INVITES)
        .await
        .unwrap();
    assert_eq!(
        store
            .drop_group_invite(&private, &mara.id, "mara")
            .await
            .unwrap(),
        GroupAct::Done(())
    );
    assert_eq!(
        store.group_seen(&private, &mara.id, 10).await.unwrap(),
        None
    );
    assert_eq!(
        store
            .drop_group_invite(&private, &dario.id, "mara")
            .await
            .unwrap(),
        GroupAct::NoSuchPilot
    );
}

pub(crate) async fn the_owner_makes_admins_who_moderate(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let jonas = create(store, "jonas").await;
    let mara = create(store, "mara").await;
    let public = group(store, "Open", &dario, true, home(47.7, 11.8, "Tegernsee")).await;
    for pilot in [&anna, &jonas, &mara] {
        enter(store, &public, pilot).await;
    }
    // Only the owner makes admins.
    assert_eq!(
        store
            .set_group_admin(&public, &anna.id, "jonas", true)
            .await
            .unwrap(),
        GroupAct::NotAllowed
    );
    assert_eq!(
        store
            .set_group_admin(&public, &dario.id, "anna", true)
            .await
            .unwrap(),
        GroupAct::Done(())
    );
    let seen = store
        .group_seen(&public, &anna.id, 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.admins, std::slice::from_ref(&anna.id));
    assert_eq!(seen.standing.role, Some(GroupRole::Admin));

    // An admin changes the profile, and may not make the group private.
    let renamed = ProfileChange {
        name: Some("Open Air".to_owned()),
        description: Some(None),
        ..ProfileChange::default()
    };
    let GroupAct::Done(changed) = store
        .change_group(&public, &anna.id, &renamed)
        .await
        .unwrap()
    else {
        panic!("not changed");
    };
    assert_eq!(changed.group.name, "Open Air");
    assert_eq!(changed.group.profile.description, None);
    assert!(changed.group.profile.public);
    let private = ProfileChange {
        public: Some(false),
        ..ProfileChange::default()
    };
    assert_eq!(
        store
            .change_group(&public, &anna.id, &private)
            .await
            .unwrap(),
        GroupAct::NotAllowed
    );
    assert_eq!(
        store
            .change_group(&public, &jonas.id, &renamed)
            .await
            .unwrap(),
        GroupAct::NotAllowed
    );

    // An admin removes a member and a member's message, not the owner.
    let spam = post(store, &new_message("m1", &public, &mara, "Buy!", at(10, 0))).await;
    let removed = store
        .remove_message(&public, &spam.id, &anna.id, at(10, 1))
        .await
        .unwrap();
    let MessageRemoval::Removed(removed) = removed else {
        panic!("not removed: {removed:?}");
    };
    assert_eq!(removed.text, "");
    assert_eq!(removed.removed, Some((at(10, 1), RemovedBy::Admin)));
    assert!(removed.version > spam.version);
    assert_eq!(
        store
            .remove_group_member(&public, &anna.id, "mara")
            .await
            .unwrap(),
        MemberRemoval::Removed
    );
    assert_eq!(
        store
            .remove_group_member(&public, &anna.id, "dario")
            .await
            .unwrap(),
        MemberRemoval::NotKeeper
    );
    // A member removes only their own messages.
    let own = post(
        store,
        &new_message("m2", &public, &dario, "Mine", at(10, 2)),
    )
    .await;
    assert_eq!(
        store
            .remove_message(&public, &own.id, &jonas.id, at(10, 3))
            .await
            .unwrap(),
        MessageRemoval::NotAllowed
    );
    // The removed message stays in its place; mara's message stays with
    // her gone from the group.
    assert_eq!(
        page(store, &public, &jonas, MessagePage::Before(None)).await,
        ["Mine", ""]
    );

    // Handing the group on to an admin: the owner before is a member
    // like any other, and the new owner more than an admin.
    store
        .hand_over_group(&public, &dario.id, "anna")
        .await
        .unwrap();
    let seen = store
        .group_seen(&public, &dario.id, 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.standing.role, Some(GroupRole::Member));
    assert_eq!(seen.group.group.keeper.handle, "anna");
    assert!(seen.admins.is_empty());
}

pub(crate) async fn messages_come_in_pages_and_since_a_version(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let outsider = create(store, "jonas").await;
    let public = group(store, "Open", &dario, true, None).await;
    enter(store, &public, &anna).await;
    let mut posted = Vec::new();
    for n in 0..5 {
        let author = if n % 2 == 0 { &dario } else { &anna };
        let new = new_message(
            &format!("m{n}"),
            &public,
            author,
            &format!("text {n}"),
            at(10, n),
        );
        posted.push(post(store, &new).await);
    }
    // A link goes along as it was sent.
    let mut linked = new_message("m5", &public, &dario, "See this", at(10, 6));
    linked.links = vec![MessageLink {
        kind: LinkKind::Flight,
        id: "abcd".to_owned(),
    }];
    let linked = post(store, &linked).await;
    assert_eq!(linked.links[0].kind, LinkKind::Flight);
    assert_eq!(linked.kind, MessageKind::Text);

    let first = store
        .messages(&public, &outsider.id, MessagePage::Before(None), 3)
        .await
        .unwrap()
        .unwrap();
    let texts: Vec<&str> = first.iter().map(|message| message.text.as_str()).collect();
    assert_eq!(
        texts,
        ["See this", "text 4", "text 3"],
        "anyone reads a public group"
    );
    let older = store
        .messages(
            &public,
            &outsider.id,
            MessagePage::Before(Some(first[2].number)),
            3,
        )
        .await
        .unwrap()
        .unwrap();
    let texts: Vec<&str> = older.iter().map(|message| message.text.as_str()).collect();
    assert_eq!(texts, ["text 2", "text 1", "text 0"]);

    // Since the newest version: nothing; then a new message and a removal,
    // in the order they happened.
    let since = first.iter().map(|message| message.version).max().unwrap();
    assert!(
        page(store, &public, &anna, MessagePage::After(since))
            .await
            .is_empty()
    );
    post(store, &new_message("m6", &public, &anna, "new", at(11, 0))).await;
    store
        .remove_message(&public, &posted[0].id, &dario.id, at(11, 1))
        .await
        .unwrap();
    assert_eq!(
        page(store, &public, &anna, MessagePage::After(since)).await,
        ["new", ""]
    );

    // Unread: messages of others after the mark; one's own is read.
    let seen = store
        .group_seen(&public, &dario.id, 100)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.standing.unread, 1, "anna's new one");
    assert_eq!(store.group_unread(&dario.id, 100).await.unwrap(), 1);
    let read = store
        .set_group_standing(&public, &dario.id, Some(true), Some(i64::MAX), 100)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.unread, 0);
    assert!(read.muted);
    // The mark does not move back, nor past the newest message.
    let back = store
        .set_group_standing(&public, &dario.id, None, Some(0), 100)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(back.read_up_to, read.read_up_to);
    assert!(back.read_up_to < i64::MAX);
    assert_eq!(
        store
            .set_group_standing(&public, &outsider.id, Some(true), None, 100)
            .await
            .unwrap(),
        None
    );
    let listeners = store.listeners(&public).await.unwrap();
    let muted: Vec<(&str, bool)> = listeners
        .iter()
        .map(|listener| (listener.handle.as_str(), listener.muted))
        .collect();
    assert_eq!(muted, [("dario", true), ("anna", false)]);

    // The list of the account's groups has its newest message.
    let listed = store.group_list(&anna.id, 100).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].last.as_ref().unwrap().text, "new");

    // Not a member: no post.
    let refused = store
        .post_message(&new_message("m7", &public, &outsider, "hi", at(12, 0)))
        .await
        .unwrap();
    assert_eq!(refused, MessagePosting::NoSuchGroup);
}

fn plan_details(on: NaiveDate) -> PlanDetails {
    PlanDetails {
        day: on,
        place: "Musterberg".to_owned(),
        position: None,
        meet_at: NaiveTime::from_hms_opt(9, 0, 0),
        note: None,
    }
}

pub(crate) async fn a_plan_is_a_post_of_its_group(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let private = group(store, "Quiet", &dario, false, None).await;
    store
        .invite_to_group(&private, &dario.id, "anna", at(9, 0), &INVITES)
        .await
        .unwrap();
    enter(store, &private, &anna).await;
    let day = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
    let room = PlanRoom {
        plans_per_group: 10,
        gone_before: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
    };
    let new = NewPlan {
        id: id("p1"),
        group_id: private.clone(),
        author_id: dario.id.clone(),
        created_at: at(10, 0),
        details: plan_details(day),
    };
    assert!(matches!(
        store.save_plan(&new, &room).await.unwrap(),
        PlanSaving::Stored(_)
    ));
    let messages = store
        .messages(&private, &anna.id, MessagePage::Before(None), 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].kind, MessageKind::Plan);
    assert_eq!(messages[0].plan_id.as_deref(), Some(new.id.as_str()));
    assert_eq!(messages[0].author.handle, "dario");
    let version = messages[0].version;

    // "I'm in" changes the card: the post is new since then.
    let said = store
        .set_plan_in(
            &private,
            &new.id,
            &anna.id,
            true,
            at(10, 5),
            room.gone_before,
        )
        .await
        .unwrap();
    assert!(matches!(said, PlanChange::Now(_)));
    let changed = store
        .messages(&private, &anna.id, MessagePage::After(version), 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].id, messages[0].id);

    // A plan that is gone leaves its post without a plan.
    store
        .delete_plan(&private, &new.id, &dario.id, room.gone_before)
        .await
        .unwrap();
    let gone = store
        .group_message(&private, &messages[0].id, &anna.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(gone.plan_id, None);
}

pub(crate) async fn a_picture_goes_with_its_message(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let public = group(store, "Open", &dario, true, None).await;
    let picture = |name: &str, by: &Account| GroupPicture {
        id: id(name),
        group_id: public.clone(),
        account_id: by.id.clone(),
        bytes: vec![1, 2, 3],
        width: 3,
        height: 1,
        created_at: at(10, 0),
    };
    // Only a member sends one.
    assert!(
        !store
            .save_group_picture(&picture("x1", &anna))
            .await
            .unwrap()
    );
    assert!(
        store
            .save_group_picture(&picture("x2", &dario))
            .await
            .unwrap()
    );
    assert_eq!(
        store.group_picture(&id("x2")).await.unwrap(),
        Some(picture("x2", &dario))
    );
    let mut with = new_message("m1", &public, &dario, "", at(10, 1));
    with.pictures = vec![id("x2")];
    let message = post(store, &with).await;
    assert_eq!(message.pictures, [id("x2")]);
    // Once used, it is no other message's.
    let mut again = new_message("m2", &public, &dario, "", at(10, 2));
    again.pictures = vec![id("x2")];
    assert_eq!(
        store.post_message(&again).await.unwrap(),
        MessagePosting::NoSuchPicture
    );
    // Removed with its message.
    store
        .remove_message(&public, &message.id, &dario.id, at(10, 3))
        .await
        .unwrap();
    assert_eq!(store.group_picture(&id("x2")).await.unwrap(), None);
    // The group's own picture is one sent to it.
    assert!(
        store
            .save_group_picture(&picture("x3", &dario))
            .await
            .unwrap()
    );
    let own = ProfileChange {
        picture: Some(Some(id("x3"))),
        ..ProfileChange::default()
    };
    let GroupAct::Done(changed) = store.change_group(&public, &dario.id, &own).await.unwrap()
    else {
        panic!("no picture");
    };
    assert_eq!(changed.group.profile.picture, Some(id("x3")));
}
