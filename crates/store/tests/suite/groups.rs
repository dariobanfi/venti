//! What every store must do with groups of pilots and with the plans their
//! members make for a flying day.
//!
//! Much of it is what must not happen: an account that is not in a group
//! learning anything about it, a group with more members or plans than it
//! has room for, a pilot coming in under a code that was replaced, a group
//! without a keeper, and anything a pilot put into a group outlasting
//! their place in it.

use chrono::{NaiveDate, NaiveTime};
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{
    Account, Group, GroupCreation, GroupJoining, GroupMembers, GroupPilot, GroupRoom, KeeperAct,
    MemberRemoval, NewGroup, NewPlan, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom,
    PlanSaving, SecretHash, Store, StoreError,
};

use super::accounts::{create, id, secret};
use super::at;

// --- Helpers -------------------------------------------------------------

/// Room for more than these tests make, where a test is not about room.
const ROOM: GroupRoom = GroupRoom {
    groups_per_account: 20,
    members_per_group: 50,
};

fn pilot(account: &Account) -> GroupPilot {
    GroupPilot {
        account_id: account.id.clone(),
        handle: account.handle.clone(),
        display_name: account.display_name.clone(),
    }
}

/// The hash of the invite code of the group these tests call `number`.
pub(crate) fn code(number: u8) -> SecretHash {
    secret(number)
}

fn new_group(name: &str, keeper: &Account, invite: u8, made_at: Timestamp) -> NewGroup {
    NewGroup {
        id: id(name),
        name: format!("Group {name}"),
        created_at: made_at,
        keeper_id: keeper.id.clone(),
        invite: code(invite),
    }
}

/// Makes group `name`, kept by `keeper`, whose invite code is `invite`.
pub(crate) async fn make(
    store: &impl Store,
    name: &str,
    keeper: &Account,
    invite: u8,
    made_at: Timestamp,
) -> Group {
    let new = new_group(name, keeper, invite, made_at);
    match store.create_group(&new, &ROOM).await.unwrap() {
        GroupCreation::Created(group) => group,
        other => panic!("expected the group to be made, got {other:?}"),
    }
}

/// Brings an account into the group whose invite code is `invite`.
pub(crate) async fn come_in(
    store: &impl Store,
    account: &Account,
    invite: u8,
    now: Timestamp,
) -> Group {
    let joining = store
        .join_group(&account.id, &code(invite), &ROOM, now)
        .await
        .unwrap();
    match joining {
        GroupJoining::Joined(group) => group,
        other => panic!("expected {} to come in, got {other:?}", account.handle),
    }
}

async fn read(store: &impl Store, group: &str, account: &Account) -> Option<GroupMembers> {
    store.group(&id(group), &account.id).await.unwrap()
}

/// The handles of a group's members in the order they are listed in, as
/// one of them sees them, with the keeper's handle first of all.
async fn keeper_and_members(
    store: &impl Store,
    group: &str,
    account: &Account,
) -> (String, Vec<String>) {
    let seen = read(store, group, account)
        .await
        .unwrap_or_else(|| panic!("{} is not in group {group}", account.handle));
    assert_eq!(seen.group.members as usize, seen.members.len());
    assert!(seen.members.contains(&seen.group.keeper));
    let members = seen.members.into_iter().map(|member| member.handle);
    (seen.group.keeper.handle, members.collect())
}

/// The short names of the groups an account is in, in their order.
async fn groups_of(store: &impl Store, account: &Account) -> Vec<String> {
    let groups = store.groups_of(&account.id).await.unwrap();
    groups.iter().map(|group| short(&group.id)).collect()
}

fn short(id: &str) -> String {
    id.trim_start_matches('0').to_owned()
}

/// A day of November 2026.
pub(crate) fn day(of_month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, of_month).unwrap()
}

fn clock(hour: u32, minute: u32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour, minute, 0)
}

/// Room for more plans than these tests make, none of them gone.
const PLAN_ROOM: PlanRoom = PlanRoom {
    plans_per_group: 100,
    gone_before: NaiveDate::MIN,
};

fn says(on: NaiveDate, meet_at: Option<NaiveTime>) -> PlanDetails {
    PlanDetails {
        day: on,
        place: "Musterberg".to_owned(),
        position: None,
        meet_at,
        note: None,
    }
}

/// Plan `name` of a group, posted by `author` at `posted_at`.
pub(crate) fn new_plan(
    name: &str,
    group: &str,
    author: &Account,
    details: PlanDetails,
    posted_at: Timestamp,
) -> NewPlan {
    NewPlan {
        id: id(name),
        group_id: id(group),
        author_id: author.id.clone(),
        created_at: posted_at,
        details,
    }
}

pub(crate) async fn post(store: &impl Store, new: &NewPlan) -> Plan {
    match store.save_plan(new, &PLAN_ROOM).await.unwrap() {
        PlanSaving::Stored(plan) => *plan,
        other => panic!("expected the plan to be stored, got {other:?}"),
    }
}

/// Posts plan `name` for a day of November at a time of day, at 12:00 plus
/// `minute` minutes.
pub(crate) async fn plan_on(
    store: &impl Store,
    name: &str,
    group: &str,
    author: &Account,
    (on, meet_at): (u32, Option<NaiveTime>),
    minute: u32,
) -> Plan {
    let new = new_plan(name, group, author, says(day(on), meet_at), at(12, minute));
    post(store, &new).await
}

/// The short names of a group's plans from a day on, as a member sees
/// them. `None` for an account that is not in the group.
async fn plans(
    store: &impl Store,
    group: &str,
    account: &Account,
    from: NaiveDate,
) -> Option<Vec<String>> {
    let plans = store
        .plans_of_group(&id(group), &account.id, from, None)
        .await
        .unwrap()?;
    Some(plans.iter().map(|plan| short(&plan.id)).collect())
}

/// The short names of the plans of every group of an account.
async fn board(store: &impl Store, account: &Account) -> Vec<String> {
    let plans = store
        .plans_for(&account.id, NaiveDate::MIN, None, 1000)
        .await
        .unwrap();
    plans.iter().map(|plan| short(&plan.id)).collect()
}

/// The handles of those who are in on a plan.
pub(crate) fn who_is_in(plan: &Plan) -> Vec<&str> {
    plan.pilots_in
        .iter()
        .map(|pilot| pilot.handle.as_str())
        .collect()
}

/// Says that an account is in on a plan, or takes that back, and returns
/// the handles of those who then are.
pub(crate) async fn set_in(
    store: &impl Store,
    (group, plan): (&str, &str),
    account: &Account,
    is_in: bool,
    now: Timestamp,
) -> Vec<String> {
    let change = store
        .set_plan_in(
            &id(group),
            &id(plan),
            &account.id,
            is_in,
            now,
            NaiveDate::MIN,
        )
        .await
        .unwrap();
    match change {
        PlanChange::Now(plan) => who_is_in(&plan).into_iter().map(str::to_owned).collect(),
        other => panic!("expected the plan, got {other:?}"),
    }
}

// --- Groups ----------------------------------------------------------------

pub(crate) async fn a_group_is_made_with_its_maker_as_keeper_and_only_member(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let new = NewGroup {
        name: "Mustergruppe «Süd» <b>".to_owned(),
        ..new_group("g1", &dario, 1, at(10, 0))
    };

    let made = store.create_group(&new, &ROOM).await.unwrap();

    let expected = Group {
        id: id("g1"),
        name: "Mustergruppe «Süd» <b>".to_owned(),
        created_at: at(10, 0),
        keeper: pilot(&dario),
        members: 1,
    };
    assert_eq!(made, GroupCreation::Created(expected.clone()));
    let seen = read(store, "g1", &dario).await.unwrap();
    assert_eq!(seen.group, expected);
    assert_eq!(seen.members, [pilot(&dario)]);
    assert_eq!(store.groups_of(&dario.id).await.unwrap(), [expected]);
    // Nobody else is in it or sees it.
    assert_eq!(read(store, "g1", &anna).await, None);
    assert!(store.groups_of(&anna.id).await.unwrap().is_empty());

    // An identifier and an invite belong to one group, and a group to an
    // account that exists. None of these is stored.
    let same_id = new_group("g1", &anna, 2, at(10, 5));
    let same_invite = new_group("g2", &anna, 1, at(10, 5));
    let nobodys = NewGroup {
        keeper_id: id("nobody"),
        ..new_group("g3", &anna, 3, at(10, 5))
    };
    let zero = NewGroup {
        name: "a\0b".to_owned(),
        ..new_group("g4", &anna, 4, at(10, 5))
    };
    for refused in [&same_id, &same_invite, &nobodys, &zero] {
        let outcome = store.create_group(refused, &ROOM).await;
        assert!(
            matches!(outcome, Err(StoreError::Invalid(_))),
            "{outcome:?}"
        );
    }
    assert!(store.groups_of(&anna.id).await.unwrap().is_empty());
    assert_eq!(read(store, "g1", &dario).await, Some(seen));
}

pub(crate) async fn a_pilot_comes_into_a_group_with_its_invite_code(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &anna, 2, at(10, 30)).await;

    // A code nobody made lets nobody in, and says nothing.
    let wrong = store
        .join_group(&anna.id, &code(9), &ROOM, at(11, 0))
        .await
        .unwrap();
    assert_eq!(wrong, GroupJoining::NoSuchInvite);

    let joined = come_in(store, &anna, 1, at(11, 0)).await;
    assert_eq!(joined.id, id("g1"));
    assert_eq!(joined.members, 2);
    assert_eq!(joined.keeper, pilot(&dario));
    come_in(store, &bernd, 1, at(11, 30)).await;

    // Everyone in the group sees the same: the members in the order in
    // which they came in.
    for member in [&dario, &anna, &bernd] {
        let seen = read(store, "g1", member).await.unwrap();
        assert_eq!(seen.group.members, 3);
        assert_eq!(seen.members, [pilot(&dario), pilot(&anna), pilot(&bernd)]);
    }
    // A pilot's groups are in the order in which they came into them.
    assert_eq!(groups_of(store, &anna).await, ["g2", "g1"]);
    assert_eq!(groups_of(store, &dario).await, ["g1"]);

    // Coming in again changes nothing, and says so.
    let again = store
        .join_group(&anna.id, &code(1), &ROOM, at(12, 0))
        .await
        .unwrap();
    assert_eq!(again, GroupJoining::AlreadyIn(joined_as_it_is(3)));
    let keeper = store
        .join_group(&dario.id, &code(1), &ROOM, at(12, 0))
        .await
        .unwrap();
    assert_eq!(keeper, GroupJoining::AlreadyIn(joined_as_it_is(3)));
    let (_, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!(members, ["dario", "anna", "bernd"]);

    // Members who came in at the same moment are listed by their accounts'
    // identifiers, and so are the groups a pilot came into at one moment.
    let carla = create(store, "carla").await;
    let alois = create(store, "alois").await;
    come_in(store, &carla, 1, at(13, 0)).await;
    come_in(store, &alois, 1, at(13, 0)).await;
    let (_, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!(members, ["dario", "anna", "bernd", "alois", "carla"]);
    come_in(store, &carla, 2, at(13, 0)).await;
    assert_eq!(groups_of(store, &carla).await, ["g1", "g2"]);

    // An account that does not exist comes into nothing.
    let nobody = store
        .join_group(&id("nobody"), &code(1), &ROOM, at(14, 0))
        .await;
    assert!(matches!(nobody, Err(StoreError::Invalid(_))), "{nobody:?}");

    fn joined_as_it_is(members: u32) -> Group {
        Group {
            id: id("g1"),
            name: "Group g1".to_owned(),
            created_at: at(10, 0),
            keeper: GroupPilot {
                account_id: id("dario"),
                handle: "dario".to_owned(),
                display_name: "Pilot dario".to_owned(),
            },
            members,
        }
    }
}

pub(crate) async fn a_group_is_seen_by_its_members_and_by_nobody_else(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let eve = create(store, "eve").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    plan_on(store, "p1", "g1", &anna, (19, clock(9, 30)), 0).await;
    let before = read(store, "g1", &dario).await.unwrap();
    let plans_before = store
        .plans_of_group(&id("g1"), &dario.id, NaiveDate::MIN, None)
        .await
        .unwrap()
        .unwrap();
    let details = says(day(20), None);

    // Whatever an account that is not in the group asks of it, the answer
    // is the one for a group that does not exist; `nowhere` is one.
    for group in ["g1", "nowhere"] {
        let group = id(group);
        assert_eq!(store.group(&group, &eve.id).await.unwrap(), None);
        assert_eq!(
            store.rename_group(&group, &eve.id, "Mine").await.unwrap(),
            KeeperAct::NoSuchGroup
        );
        assert_eq!(
            store
                .renew_group_invite(&group, &eve.id, &code(7))
                .await
                .unwrap(),
            KeeperAct::NoSuchGroup
        );
        for handle in ["eve", "anna", "dario", "nobody"] {
            assert_eq!(
                store
                    .hand_over_group(&group, &eve.id, handle)
                    .await
                    .unwrap(),
                KeeperAct::NoSuchGroup
            );
            assert_eq!(
                store
                    .remove_group_member(&group, &eve.id, handle)
                    .await
                    .unwrap(),
                MemberRemoval::NoSuchGroup
            );
        }
        assert_eq!(
            store.delete_group(&group, &eve.id).await.unwrap(),
            KeeperAct::NoSuchGroup
        );
        let new = NewPlan {
            group_id: group.clone(),
            ..new_plan("p9", "g1", &eve, details.clone(), at(12, 0))
        };
        assert_eq!(
            store.save_plan(&new, &PLAN_ROOM).await.unwrap(),
            PlanSaving::NoSuchGroup
        );
        // Also for a plan that is there, named by its identifier.
        for plan in ["p1", "p9"] {
            let plan = id(plan);
            let changed = store
                .change_plan(&group, &plan, &eve.id, &details, at(12, 0), NaiveDate::MIN)
                .await
                .unwrap();
            assert_eq!(changed, PlanChange::NoSuchGroup);
            let removed = store
                .delete_plan(&group, &plan, &eve.id, NaiveDate::MIN)
                .await
                .unwrap();
            assert_eq!(removed, PlanRemoval::NoSuchGroup);
            for is_in in [true, false] {
                let said = store
                    .set_plan_in(&group, &plan, &eve.id, is_in, at(12, 0), NaiveDate::MIN)
                    .await
                    .unwrap();
                assert_eq!(said, PlanChange::NoSuchGroup);
            }
        }
        let listed = store
            .plans_of_group(&group, &eve.id, NaiveDate::MIN, None)
            .await
            .unwrap();
        assert_eq!(listed, None);
    }
    assert!(store.groups_of(&eve.id).await.unwrap().is_empty());
    assert!(board(store, &eve).await.is_empty());

    // And none of it changed anything.
    assert_eq!(read(store, "g1", &dario).await, Some(before));
    let plans_after = store
        .plans_of_group(&id("g1"), &dario.id, NaiveDate::MIN, None)
        .await
        .unwrap();
    assert_eq!(plans_after, Some(plans_before));
    // The code still lets a pilot in.
    come_in(store, &eve, 1, at(13, 0)).await;
    assert_eq!(plans(store, "g1", &eve, day(1)).await.unwrap(), ["p1"]);
}

pub(crate) async fn groups_and_accounts_take_no_more_than_they_have_room_for(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    let small = GroupRoom {
        groups_per_account: 2,
        members_per_group: 3,
    };
    let join = |account: &Account, invite: u8| {
        let account_id = account.id.clone();
        async move {
            store
                .join_group(&account_id, &code(invite), &small, at(11, 0))
                .await
                .unwrap()
        }
    };
    make(store, "g1", &dario, 1, at(10, 0)).await;
    assert!(matches!(join(&anna, 1).await, GroupJoining::Joined(_)));
    assert!(matches!(join(&bernd, 1).await, GroupJoining::Joined(_)));

    // Three members are as many as this group may have.
    assert_eq!(join(&carla, 1).await, GroupJoining::GroupFull);
    assert_eq!(read(store, "g1", &carla).await, None);
    // Who is in it already is told so, full or not.
    assert!(matches!(join(&anna, 1).await, GroupJoining::AlreadyIn(_)));
    // A place that comes free is taken.
    let left = store
        .remove_group_member(&id("g1"), &bernd.id, "bernd")
        .await
        .unwrap();
    assert_eq!(left, MemberRemoval::Removed);
    assert!(matches!(join(&carla, 1).await, GroupJoining::Joined(_)));

    // Two groups are as many as an account may be in here, whether it
    // keeps them or came into them.
    make(store, "g2", &anna, 2, at(10, 0)).await;
    make(store, "g3", &bernd, 3, at(10, 0)).await;
    assert_eq!(join(&anna, 3).await, GroupJoining::TooManyGroups);
    let third = new_group("g4", &anna, 4, at(12, 0));
    assert_eq!(
        store.create_group(&third, &small).await.unwrap(),
        GroupCreation::TooManyGroups
    );
    assert_eq!(groups_of(store, &anna).await, ["g2", "g1"]);
    // A wrong code is a wrong code also for a pilot with no room.
    assert_eq!(join(&anna, 9).await, GroupJoining::NoSuchInvite);
    // Leaving one makes room for another.
    store
        .remove_group_member(&id("g1"), &anna.id, "anna")
        .await
        .unwrap();
    assert!(matches!(join(&anna, 3).await, GroupJoining::Joined(_)));
    assert_eq!(groups_of(store, &anna).await, ["g2", "g3"]);
}

pub(crate) async fn joins_racing_for_the_last_place_fill_it_once(store: &impl Store) {
    let dario = create(store, "dario").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    let handles = ["anna", "bernd", "carla", "dora"];
    let mut racers = Vec::new();
    for handle in handles {
        racers.push(create(store, handle).await);
    }
    // Room for the keeper and one more.
    let room = GroupRoom {
        groups_per_account: 20,
        members_per_group: 2,
    };
    let join = |account: &Account| {
        let (account_id, room) = (account.id.clone(), &room);
        async move {
            store
                .join_group(&account_id, &code(1), room, at(11, 0))
                .await
        }
    };

    let (a, b, c, d) = tokio::join!(
        join(&racers[0]),
        join(&racers[1]),
        join(&racers[2]),
        join(&racers[3]),
    );

    let outcomes = [a.unwrap(), b.unwrap(), c.unwrap(), d.unwrap()];
    let joined = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, GroupJoining::Joined(_)))
        .count();
    let full = outcomes
        .iter()
        .filter(|outcome| **outcome == GroupJoining::GroupFull)
        .count();
    assert_eq!((joined, full), (1, 3), "{outcomes:?}");
    let seen = read(store, "g1", &dario).await.unwrap();
    assert_eq!(seen.group.members, 2);
    assert_eq!(seen.members.len(), 2);

    // The same for an account's last place: one account that races into
    // three groups at once, with room for two, is in two.
    let erik = create(store, "erik").await;
    for (name, invite) in [("h1", 11), ("h2", 12), ("h3", 13)] {
        make(store, name, &dario, invite, at(10, 0)).await;
    }
    let room = GroupRoom {
        groups_per_account: 2,
        members_per_group: 50,
    };
    let into = |invite: u8| {
        let room = &room;
        let erik = &erik;
        async move {
            store
                .join_group(&erik.id, &code(invite), room, at(11, 0))
                .await
        }
    };
    let (a, b, c) = tokio::join!(into(11), into(12), into(13));
    let outcomes = [a.unwrap(), b.unwrap(), c.unwrap()];
    let joined = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, GroupJoining::Joined(_)))
        .count();
    assert_eq!(joined, 2, "{outcomes:?}");
    assert!(outcomes.contains(&GroupJoining::TooManyGroups));
    assert_eq!(store.groups_of(&erik.id).await.unwrap().len(), 2);
}

pub(crate) async fn a_new_invite_code_ends_the_one_before(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &dario, 2, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;

    // Only the keeper makes a new one.
    let by_member = store
        .renew_group_invite(&id("g1"), &anna.id, &code(5))
        .await
        .unwrap();
    assert_eq!(by_member, KeeperAct::NotKeeper);
    let by_keeper = store
        .renew_group_invite(&id("g1"), &dario.id, &code(5))
        .await
        .unwrap();
    assert_eq!(by_keeper, KeeperAct::Done(()));

    // The old code lets nobody in from then on, and the new one does.
    let old = store
        .join_group(&bernd.id, &code(1), &ROOM, at(11, 0))
        .await
        .unwrap();
    assert_eq!(old, GroupJoining::NoSuchInvite);
    assert_eq!(come_in(store, &bernd, 5, at(11, 0)).await.id, id("g1"));
    // Who came in under the old one stays in.
    let (_, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!(members, ["dario", "anna", "bernd"]);
    // The other group's code is as it was, and no group gets a code
    // another one has.
    let taken = store
        .renew_group_invite(&id("g1"), &dario.id, &code(2))
        .await;
    assert!(matches!(taken, Err(StoreError::Invalid(_))), "{taken:?}");
    assert_eq!(come_in(store, &bernd, 2, at(11, 5)).await.id, id("g2"));
    // A member who names the new code is told that they are in.
    let member = store
        .join_group(&anna.id, &code(5), &ROOM, at(11, 5))
        .await
        .unwrap();
    assert!(matches!(member, GroupJoining::AlreadyIn(group) if group.id == id("g1")));
    // Making the code a group has again is no change.
    let same = store
        .renew_group_invite(&id("g1"), &dario.id, &code(5))
        .await
        .unwrap();
    assert_eq!(same, KeeperAct::Done(()));
}

pub(crate) async fn a_join_and_a_new_code_never_both_win(store: &impl Store) {
    let dario = create(store, "dario").await;
    // Several rounds, each with a group and two pilots of its own: the
    // two calls meet in whatever order they come.
    for round in 0..6_u8 {
        let group = format!("g{round}");
        let (old, new) = (10 + round, 100 + round);
        make(store, &group, &dario, old, at(10, 0)).await;
        let early = create(store, &format!("early{round}")).await;
        let late = create(store, &format!("late{round}")).await;

        let (old_code, new_code, group_id) = (code(old), code(new), id(&group));

        let (joining, renewing) = tokio::join!(
            store.join_group(&early.id, &old_code, &ROOM, at(11, 0)),
            store.renew_group_invite(&group_id, &dario.id, &new_code),
        );

        assert_eq!(renewing.unwrap(), KeeperAct::Done(()));
        let (_, members) = keeper_and_members(store, &group, &dario).await;
        // The pilot came in under the old code before it ended, or did not
        // come in at all: nothing in between.
        match joining.unwrap() {
            GroupJoining::Joined(joined) => {
                assert_eq!(joined.id, id(&group));
                assert_eq!(members, ["dario", early.handle.as_str()]);
            }
            GroupJoining::NoSuchInvite => assert_eq!(members, ["dario"]),
            other => panic!("{other:?}"),
        }
        // Either way the old code is over.
        let after = store
            .join_group(&late.id, &code(old), &ROOM, at(11, 5))
            .await
            .unwrap();
        assert_eq!(after, GroupJoining::NoSuchInvite);
        assert_eq!(come_in(store, &late, new, at(11, 5)).await.id, id(&group));
    }
}

pub(crate) async fn only_the_keeper_renames_hands_over_and_deletes_a_group(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    come_in(store, &bernd, 1, at(10, 10)).await;
    let g1 = id("g1");

    // A member who does not keep the group changes nothing about it.
    assert_eq!(
        store.rename_group(&g1, &anna.id, "Annas").await.unwrap(),
        KeeperAct::NotKeeper
    );
    for handle in ["anna", "bernd", "nobody"] {
        assert_eq!(
            store.hand_over_group(&g1, &anna.id, handle).await.unwrap(),
            KeeperAct::NotKeeper
        );
    }
    assert_eq!(
        store.delete_group(&g1, &anna.id).await.unwrap(),
        KeeperAct::NotKeeper
    );
    let seen = read(store, "g1", &bernd).await.unwrap();
    assert_eq!(seen.group.name, "Group g1");
    assert_eq!(seen.group.keeper, pilot(&dario));

    // The keeper renames it, for everyone in it.
    let renamed = store
        .rename_group(&g1, &dario.id, "Thermik & Kuchen")
        .await
        .unwrap();
    let KeeperAct::Done(renamed) = renamed else {
        panic!("{renamed:?}");
    };
    assert_eq!(renamed.group.name, "Thermik & Kuchen");
    assert_eq!(renamed.members.len(), 3);
    assert_eq!(read(store, "g1", &anna).await, Some(renamed));
    let zero = store.rename_group(&g1, &dario.id, "a\0").await;
    assert!(matches!(zero, Err(StoreError::Invalid(_))), "{zero:?}");

    // The keeper hands the keeping to a member, and to nobody else.
    for handle in ["nobody", "", "Bernd", "x' OR '1'='1"] {
        assert_eq!(
            store.hand_over_group(&g1, &dario.id, handle).await.unwrap(),
            KeeperAct::NoSuchMember,
            "{handle:?}"
        );
    }
    let stranger = create(store, "stranger").await;
    assert_eq!(
        store
            .hand_over_group(&g1, &dario.id, &stranger.handle)
            .await
            .unwrap(),
        KeeperAct::NoSuchMember
    );
    // To the keeper is no change.
    let same = store
        .hand_over_group(&g1, &dario.id, "dario")
        .await
        .unwrap();
    assert!(matches!(same, KeeperAct::Done(seen) if seen.group.keeper == pilot(&dario)));
    let handed = store
        .hand_over_group(&g1, &dario.id, "bernd")
        .await
        .unwrap();
    let KeeperAct::Done(handed) = handed else {
        panic!("{handed:?}");
    };
    assert_eq!(handed.group.keeper, pilot(&bernd));
    // Everyone stays where they were in the list: the keeping is no place
    // in it.
    assert_eq!(handed.members, [pilot(&dario), pilot(&anna), pilot(&bernd)]);
    // Who kept it is a member like any other from then on.
    assert_eq!(
        store.rename_group(&g1, &dario.id, "Again").await.unwrap(),
        KeeperAct::NotKeeper
    );
    assert_eq!(
        store.delete_group(&g1, &dario.id).await.unwrap(),
        KeeperAct::NotKeeper
    );

    // The keeper deletes it, and it is gone for everyone, with its code.
    plan_on(store, "p1", "g1", &anna, (19, None), 0).await;
    assert_eq!(
        store.delete_group(&g1, &bernd.id).await.unwrap(),
        KeeperAct::Done(())
    );
    for member in [&dario, &anna, &bernd] {
        assert_eq!(read(store, "g1", member).await, None);
        assert!(store.groups_of(&member.id).await.unwrap().is_empty());
        assert_eq!(plans(store, "g1", member, day(1)).await, None);
        assert!(board(store, member).await.is_empty());
    }
    let gone = store
        .join_group(&stranger.id, &code(1), &ROOM, at(12, 0))
        .await
        .unwrap();
    assert_eq!(gone, GroupJoining::NoSuchInvite);
    assert_eq!(
        store.delete_group(&g1, &bernd.id).await.unwrap(),
        KeeperAct::NoSuchGroup
    );
    // Its identifier, its code and its plans' identifiers are free again.
    make(store, "g1", &anna, 1, at(13, 0)).await;
    plan_on(store, "p1", "g1", &anna, (19, None), 0).await;
}

pub(crate) async fn the_keeper_removes_a_member_and_the_code_ends_with_that(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    for (member, minute) in [(&anna, 5), (&bernd, 10)] {
        come_in(store, member, 1, at(10, minute)).await;
    }
    let g1 = id("g1");
    let remove = |by: &Account, handle: &'static str| {
        let (g1, by) = (g1.clone(), by.id.clone());
        async move { store.remove_group_member(&g1, &by, handle).await.unwrap() }
    };

    // A member removes nobody but themselves, whoever they name.
    for handle in ["bernd", "dario", "nobody", ""] {
        assert_eq!(remove(&anna, handle).await, MemberRemoval::NotKeeper);
    }
    // The keeper removes a member, and nobody who is none.
    for handle in ["carla", "nobody", "", "Anna"] {
        assert_eq!(remove(&dario, handle).await, MemberRemoval::NoSuchMember);
    }
    let (_, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!(members, ["dario", "anna", "bernd"]);
    // Until here the code lets a pilot in.
    come_in(store, &carla, 1, at(10, 15)).await;

    assert_eq!(remove(&dario, "bernd").await, MemberRemoval::Removed);

    let (keeper, members) = keeper_and_members(store, "g1", &anna).await;
    assert_eq!(keeper, "dario");
    assert_eq!(members, ["dario", "anna", "carla"]);
    // To whoever was removed, the group is no group.
    assert_eq!(read(store, "g1", &bernd).await, None);
    assert!(store.groups_of(&bernd.id).await.unwrap().is_empty());
    // And the code they knew is over: for them, and for anyone else.
    let back = store
        .join_group(&bernd.id, &code(1), &ROOM, at(11, 0))
        .await
        .unwrap();
    assert_eq!(back, GroupJoining::NoSuchInvite);
    let erik = create(store, "erik").await;
    let other = store
        .join_group(&erik.id, &code(1), &ROOM, at(11, 0))
        .await
        .unwrap();
    assert_eq!(other, GroupJoining::NoSuchInvite);
    // A member who tries it finds it gone as well: whether a code is good
    // is looked at before anything else.
    let member = store
        .join_group(&anna.id, &code(1), &ROOM, at(11, 0))
        .await
        .unwrap();
    assert_eq!(member, GroupJoining::NoSuchInvite);
    // The keeper makes a new code, and with it the group takes pilots in
    // again, the one who was removed too if he is given it.
    store
        .renew_group_invite(&g1, &dario.id, &code(6))
        .await
        .unwrap();
    come_in(store, &erik, 6, at(11, 5)).await;
    come_in(store, &bernd, 6, at(11, 10)).await;
    let (_, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!(members, ["dario", "anna", "carla", "erik", "bernd"]);

    // A member who leaves by themselves ends no code.
    assert_eq!(remove(&anna, "anna").await, MemberRemoval::Removed);
    assert_eq!(read(store, "g1", &anna).await, None);
    assert_eq!(remove(&anna, "anna").await, MemberRemoval::NoSuchGroup);
    come_in(store, &anna, 6, at(11, 15)).await;
    let (_, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!(members, ["dario", "carla", "erik", "bernd", "anna"]);
}

pub(crate) async fn a_member_who_goes_takes_what_they_put_into_the_group(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &dario, 2, at(10, 0)).await;
    for (member, minute) in [(&anna, 5), (&bernd, 10)] {
        come_in(store, member, 1, at(10, minute)).await;
        come_in(store, member, 2, at(10, minute)).await;
    }
    // In each group Anna and Bernd post a plan, and each is in on the
    // other's.
    for (group, annas, bernds) in [("g1", "a1", "b1"), ("g2", "a2", "b2")] {
        plan_on(store, annas, group, &anna, (19, None), 0).await;
        plan_on(store, bernds, group, &bernd, (20, None), 1).await;
        set_in(store, (group, annas), &bernd, true, at(12, 5)).await;
        set_in(store, (group, annas), &dario, true, at(12, 6)).await;
        set_in(store, (group, bernds), &anna, true, at(12, 7)).await;
    }

    // Anna leaves the first group; the keeper removes Bernd from the
    // second.
    store
        .remove_group_member(&id("g1"), &anna.id, "anna")
        .await
        .unwrap();
    store
        .remove_group_member(&id("g2"), &dario.id, "bernd")
        .await
        .unwrap();

    // In the first group Anna's plan is gone with who was in, and she is
    // no longer in on Bernd's. In the second it is the other way round.
    let read_plans = |group: &'static str| {
        let asking = dario.id.clone();
        async move {
            store
                .plans_of_group(&id(group), &asking, NaiveDate::MIN, None)
                .await
                .unwrap()
                .unwrap()
        }
    };
    let first = read_plans("g1").await;
    assert_eq!(first.len(), 1);
    assert_eq!(short(&first[0].id), "b1");
    assert_eq!(who_is_in(&first[0]), ["bernd"]);
    let second = read_plans("g2").await;
    assert_eq!(second.len(), 1);
    assert_eq!(short(&second[0].id), "a2");
    assert_eq!(who_is_in(&second[0]), ["anna", "dario"]);
    // What they put into the group they are still in is untouched.
    assert_eq!(board(store, &anna).await, ["a2"]);
    assert_eq!(board(store, &bernd).await, ["b1"]);
    assert_eq!(board(store, &dario).await, ["a2", "b1"]);

    // Coming in again brings nothing back.
    come_in(store, &anna, 1, at(13, 0)).await;
    assert_eq!(plans(store, "g1", &anna, day(1)).await.unwrap(), ["b1"]);
    assert_eq!(who_is_in(&read_plans("g1").await[0]), ["bernd"]);
    // The identifier of a plan that went is free again.
    plan_on(store, "a1", "g1", &anna, (19, None), 30).await;
}

pub(crate) async fn when_the_keeper_goes_the_longest_standing_member_keeps_it(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    // Bernd came in before Anna; Carla at the same moment as Anna.
    come_in(store, &bernd, 1, at(10, 5)).await;
    come_in(store, &carla, 1, at(10, 10)).await;
    come_in(store, &anna, 1, at(10, 10)).await;
    plan_on(store, "p1", "g1", &dario, (19, None), 0).await;
    plan_on(store, "p2", "g1", &carla, (19, None), 1).await;
    let leave = |account: &Account| {
        let (leaving, handle) = (account.id.clone(), account.handle.clone());
        async move {
            store
                .remove_group_member(&id("g1"), &leaving, &handle)
                .await
                .unwrap()
        }
    };

    // The keeper leaves: the group stays, without the keeper's plan, and
    // is kept by the member who has been in it longest.
    assert_eq!(leave(&dario).await, MemberRemoval::Removed);
    assert_eq!(read(store, "g1", &dario).await, None);
    let (keeper, members) = keeper_and_members(store, "g1", &anna).await;
    assert_eq!(keeper, "bernd");
    assert_eq!(members, ["bernd", "anna", "carla"]);
    assert_eq!(plans(store, "g1", &anna, day(1)).await.unwrap(), ["p2"]);
    // The new keeper is the keeper in every respect.
    let renamed = store
        .rename_group(&id("g1"), &bernd.id, "Bernds")
        .await
        .unwrap();
    assert!(matches!(renamed, KeeperAct::Done(_)));
    // The code is as it was: nobody was removed.
    come_in(store, &dario, 1, at(11, 0)).await;

    // Among those who came in at the same moment, the account's identifier
    // decides: Anna's sorts before Carla's.
    assert_eq!(leave(&bernd).await, MemberRemoval::Removed);
    let (keeper, members) = keeper_and_members(store, "g1", &carla).await;
    assert_eq!(keeper, "anna");
    assert_eq!(members, ["anna", "carla", "dario"]);

    // A member who is not the keeper leaves, and the keeper stays.
    assert_eq!(leave(&carla).await, MemberRemoval::Removed);
    assert_eq!(leave(&anna).await, MemberRemoval::Removed);
    let (keeper, members) = keeper_and_members(store, "g1", &dario).await;
    assert_eq!((keeper.as_str(), members.len()), ("dario", 1));

    // The last member leaves: the group goes, with its code. Nobody can
    // come into a group that nobody is in.
    plan_on(store, "p3", "g1", &dario, (19, None), 2).await;
    assert_eq!(leave(&dario).await, MemberRemoval::GroupRemoved);
    assert_eq!(read(store, "g1", &dario).await, None);
    assert!(store.groups_of(&dario.id).await.unwrap().is_empty());
    assert!(board(store, &dario).await.is_empty());
    let gone = store
        .join_group(&anna.id, &code(1), &ROOM, at(12, 0))
        .await
        .unwrap();
    assert_eq!(gone, GroupJoining::NoSuchInvite);
    // The identifier is free again, and the plans went with the group.
    make(store, "g1", &anna, 1, at(13, 0)).await;
    assert!(plans(store, "g1", &anna, day(1)).await.unwrap().is_empty());
}

pub(crate) async fn a_group_has_a_keeper_who_is_a_member_whoever_goes_at_once(store: &impl Store) {
    // The keeper's account is deleted, the next in line leaves and the one
    // after is deleted too, all at the same moment and several times over.
    // Whatever order they come in, the last member keeps the group.
    for round in 0..5 {
        let group = format!("g{round}");
        let invite = 10 + round;
        let named = |name: &str| format!("{name}{round}");
        let keeper = create(store, &named("keeper")).await;
        let second = create(store, &named("second")).await;
        let third = create(store, &named("third")).await;
        let last = create(store, &named("last")).await;
        make(store, &group, &keeper, invite, at(10, 0)).await;
        for (member, minute) in [(&second, 1), (&third, 2), (&last, 3)] {
            come_in(store, member, invite, at(10, minute)).await;
        }
        plan_on(store, &named("p"), &group, &last, (19, None), 0).await;
        set_in(store, (&group, &named("p")), &third, true, at(12, 1)).await;

        let group_id = id(&group);

        let (a, b, c) = tokio::join!(
            store.delete_account(&keeper.id),
            store.remove_group_member(&group_id, &second.id, &second.handle),
            store.delete_account(&third.id),
        );

        a.unwrap();
        assert_eq!(b.unwrap(), MemberRemoval::Removed);
        c.unwrap();
        let (keeps, members) = keeper_and_members(store, &group, &last).await;
        assert_eq!(keeps, last.handle);
        assert_eq!(members, std::slice::from_ref(&last.handle));
        let plans = store
            .plans_of_group(&id(&group), &last.id, NaiveDate::MIN, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(who_is_in(&plans[0]), [last.handle.as_str()]);
    }

    // Two pilots who each keep a group in which the other is next in line
    // delete their accounts at the same moment. Both go, and both groups.
    let x = create(store, "xaver").await;
    let y = create(store, "yvonne").await;
    make(store, "x1", &x, 101, at(10, 0)).await;
    make(store, "y1", &y, 102, at(10, 0)).await;
    come_in(store, &y, 101, at(10, 5)).await;
    come_in(store, &x, 102, at(10, 5)).await;

    let (a, b) = tokio::join!(store.delete_account(&x.id), store.delete_account(&y.id));

    a.unwrap();
    b.unwrap();
    assert_eq!(store.account("xaver").await.unwrap(), None);
    assert_eq!(store.account("yvonne").await.unwrap(), None);
    let outsider = create(store, "outsider").await;
    for invite in [101, 102] {
        let gone = store
            .join_group(&outsider.id, &code(invite), &ROOM, at(12, 0))
            .await
            .unwrap();
        assert_eq!(gone, GroupJoining::NoSuchInvite);
    }

    // A keeper hands the group to a member whose account is deleted at
    // that moment: the group ends up kept by someone who is in it.
    for round in 0..5 {
        let group = format!("h{round}");
        let invite = 50 + round;
        let keeper = create(store, &format!("holder{round}")).await;
        let heir = create(store, &format!("heir{round}")).await;
        make(store, &group, &keeper, invite, at(10, 0)).await;
        come_in(store, &heir, invite, at(10, 1)).await;

        let group_id = id(&group);

        let (handed, deleted) = tokio::join!(
            store.hand_over_group(&group_id, &keeper.id, &heir.handle),
            store.delete_account(&heir.id),
        );

        deleted.unwrap();
        // The keeping passed and came back, or never passed.
        let handed = handed.unwrap();
        assert!(
            matches!(handed, KeeperAct::Done(_) | KeeperAct::NoSuchMember),
            "{handed:?}"
        );
        let (keeps, members) = keeper_and_members(store, &group, &keeper).await;
        assert_eq!(keeps, keeper.handle);
        assert_eq!(members, std::slice::from_ref(&keeper.handle));
    }
}

pub(crate) async fn no_group_or_plan_has_an_identifier_that_cannot_be_one(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    plan_on(store, "p1", "g1", &dario, (19, None), 0).await;
    let g1 = id("g1");
    let details = says(day(19), None);
    let long = "x".repeat(100_000);

    // What a path or a body can carry and a database may refuse to look
    // at: a zero byte above all. None of it is an error, none of it names
    // anything, and none of it changes anything.
    for text in [
        "\0",
        "0000000000000000000000000\0",
        "",
        " ",
        "g1' OR '1'='1",
        "\u{feff}",
        long.as_str(),
    ] {
        let shown = &text[..text.len().min(40)];
        // In place of a group.
        assert_eq!(
            store.group(text, &dario.id).await.unwrap(),
            None,
            "{shown:?}"
        );
        assert_eq!(
            store.rename_group(text, &dario.id, "x").await.unwrap(),
            KeeperAct::NoSuchGroup
        );
        assert_eq!(
            store
                .renew_group_invite(text, &dario.id, &code(8))
                .await
                .unwrap(),
            KeeperAct::NoSuchGroup
        );
        assert_eq!(
            store
                .hand_over_group(text, &dario.id, "anna")
                .await
                .unwrap(),
            KeeperAct::NoSuchGroup
        );
        assert_eq!(
            store
                .remove_group_member(text, &dario.id, "anna")
                .await
                .unwrap(),
            MemberRemoval::NoSuchGroup
        );
        assert_eq!(
            store.delete_group(text, &dario.id).await.unwrap(),
            KeeperAct::NoSuchGroup
        );
        assert_eq!(
            store
                .plans_of_group(text, &dario.id, NaiveDate::MIN, None)
                .await
                .unwrap(),
            None
        );
        // In place of an account.
        assert_eq!(store.group(&g1, text).await.unwrap(), None, "{shown:?}");
        assert!(store.groups_of(text).await.unwrap().is_empty());
        assert!(
            store
                .plans_for(text, NaiveDate::MIN, None, 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store.delete_group(&g1, text).await.unwrap(),
            KeeperAct::NoSuchGroup
        );
        // In place of a member's handle.
        assert_eq!(
            store.hand_over_group(&g1, &dario.id, text).await.unwrap(),
            KeeperAct::NoSuchMember
        );
        assert_eq!(
            store
                .remove_group_member(&g1, &dario.id, text)
                .await
                .unwrap(),
            MemberRemoval::NoSuchMember
        );
        assert_eq!(
            store
                .remove_group_member(&g1, &anna.id, text)
                .await
                .unwrap(),
            MemberRemoval::NotKeeper
        );
        // In place of a plan.
        let changed = store
            .change_plan(&g1, text, &dario.id, &details, at(12, 0), NaiveDate::MIN)
            .await
            .unwrap();
        assert_eq!(changed, PlanChange::NoSuchPlan, "{shown:?}");
        let removed = store
            .delete_plan(&g1, text, &dario.id, NaiveDate::MIN)
            .await
            .unwrap();
        assert_eq!(removed, PlanRemoval::NoSuchPlan);
        let said = store
            .set_plan_in(&g1, text, &anna.id, true, at(12, 0), NaiveDate::MIN)
            .await
            .unwrap();
        assert_eq!(said, PlanChange::NoSuchPlan);
    }
    let (keeper, members) = keeper_and_members(store, "g1", &anna).await;
    assert_eq!((keeper.as_str(), members.len()), ("dario", 2));
    assert_eq!(plans(store, "g1", &anna, day(1)).await.unwrap(), ["p1"]);
}

// --- Plans -----------------------------------------------------------------

pub(crate) async fn a_plan_is_stored_with_its_author_in_and_read_back_exactly(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    // Awkward decimals, text outside ASCII and several lines.
    let details = PlanDetails {
        day: day(19),
        place: "Musterberg – «Startplatz Ost» <b>".to_owned(),
        position: Some(GeoPoint::new(47.664_211_111_1, 11.796_722_222_3).unwrap()),
        meet_at: clock(9, 30),
        note: Some("Bring a radio.\nGrüße, 山 🪂".to_owned()),
    };
    let new = new_plan("p1", "g1", &anna, details.clone(), at(12, 0));

    let stored = store.save_plan(&new, &PLAN_ROOM).await.unwrap();

    let expected = Plan {
        id: id("p1"),
        group_id: id("g1"),
        author: pilot(&anna),
        details,
        created_at: at(12, 0),
        changed_at: at(12, 0),
        pilots_in: vec![pilot(&anna)],
    };
    assert_eq!(stored, PlanSaving::Stored(Box::new(expected.clone())));
    for member in [&dario, &anna] {
        let listed = store
            .plans_of_group(&id("g1"), &member.id, day(19), Some(day(19)))
            .await
            .unwrap();
        assert_eq!(listed, Some(vec![expected.clone()]));
        let all = store.plans_for(&member.id, day(1), None, 10).await.unwrap();
        assert_eq!(all, std::slice::from_ref(&expected));
    }

    // A plan that says as little as a plan may.
    let bare = new_plan("p2", "g1", &dario, says(day(20), None), at(12, 5));
    let stored = post(store, &bare).await;
    assert_eq!(stored.details.position, None);
    assert_eq!(stored.details.meet_at, None);
    assert_eq!(stored.details.note, None);
    assert_eq!(stored.pilots_in, [pilot(&dario)]);

    // An identifier names one plan, text has no zero byte, and a time to
    // meet is a whole minute. None of these is stored.
    let same_id = new_plan("p1", "g1", &dario, says(day(21), None), at(12, 10));
    let zero = PlanDetails {
        note: Some("a\0".to_owned()),
        ..says(day(21), None)
    };
    let seconds = says(day(21), NaiveTime::from_hms_opt(9, 30, 30));
    for refused in [
        same_id,
        new_plan("p3", "g1", &dario, zero, at(12, 10)),
        new_plan("p4", "g1", &dario, seconds, at(12, 10)),
    ] {
        let outcome = store.save_plan(&refused, &PLAN_ROOM).await;
        assert!(
            matches!(outcome, Err(StoreError::Invalid(_))),
            "{outcome:?}"
        );
    }
    assert_eq!(
        plans(store, "g1", &dario, day(1)).await.unwrap(),
        ["p1", "p2"]
    );
    let unchanged = store
        .plans_of_group(&id("g1"), &dario.id, day(19), Some(day(19)))
        .await
        .unwrap();
    assert_eq!(unchanged, Some(vec![expected]));
}

pub(crate) async fn plans_are_listed_by_day_and_time_for_the_days_asked_for(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &anna, 2, at(10, 0)).await;
    make(store, "g3", &bernd, 3, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    come_in(store, &dario, 2, at(10, 5)).await;
    // Posted out of order. Within a day: by the time to meet, then those
    // without one, then in the order of posting, then by identifier.
    plan_on(store, "f", "g1", &dario, (20, None), 0).await;
    plan_on(store, "e", "g1", &anna, (19, None), 9).await;
    plan_on(store, "d", "g1", &dario, (19, None), 7).await;
    plan_on(store, "c", "g1", &anna, (19, None), 7).await;
    plan_on(store, "b", "g1", &dario, (19, clock(14, 0)), 8).await;
    plan_on(store, "a", "g1", &anna, (19, clock(9, 30)), 9).await;
    plan_on(store, "z", "g1", &dario, (17, clock(8, 0)), 3).await;
    // Other groups: one that both are in, one that neither is in.
    plan_on(store, "m", "g2", &anna, (19, clock(11, 0)), 4).await;
    plan_on(store, "n", "g2", &dario, (21, None), 5).await;
    plan_on(store, "s", "g3", &bernd, (19, clock(7, 0)), 6).await;

    let of_group = |from: u32, to: Option<u32>| {
        let asking = anna.id.clone();
        async move {
            let plans = store
                .plans_of_group(&id("g1"), &asking, day(from), to.map(day))
                .await
                .unwrap()
                .unwrap();
            let names: Vec<String> = plans.iter().map(|plan| short(&plan.id)).collect();
            names.join("")
        }
    };
    assert_eq!(of_group(1, None).await, "zabcdef");
    // From a day on, and up to a day, both included.
    assert_eq!(of_group(19, None).await, "abcdef");
    assert_eq!(of_group(20, None).await, "f");
    assert_eq!(of_group(21, None).await, "");
    assert_eq!(of_group(19, Some(19)).await, "abcde");
    assert_eq!(of_group(17, Some(18)).await, "z");
    assert_eq!(of_group(18, Some(18)).await, "");
    // A span that ends before it begins holds nothing.
    assert_eq!(of_group(20, Some(19)).await, "");

    // All of a pilot's groups in one list, in the same order, and only
    // their groups: the third group's plan is in nobody else's list.
    let all = |account: &Account, from: u32, to: Option<u32>, limit: usize| {
        let account_id = account.id.clone();
        async move {
            let plans = store
                .plans_for(&account_id, day(from), to.map(day), limit)
                .await
                .unwrap();
            let names: Vec<String> = plans.iter().map(|plan| short(&plan.id)).collect();
            names.join("")
        }
    };
    assert_eq!(all(&anna, 1, None, 100).await, "zambcdefn");
    assert_eq!(all(&dario, 1, None, 100).await, "zambcdefn");
    assert_eq!(all(&bernd, 1, None, 100).await, "s");
    assert_eq!(all(&anna, 19, Some(19), 100).await, "ambcde");
    assert_eq!(all(&anna, 20, Some(21), 100).await, "fn");
    // The limit keeps the first.
    assert_eq!(all(&anna, 1, None, 4).await, "zamb");
    assert_eq!(all(&anna, 1, None, 0).await, "");
    // Each plan says which group it is of.
    let listed = store
        .plans_for(&anna.id, day(19), Some(day(19)), 2)
        .await
        .unwrap();
    assert_eq!(short(&listed[0].group_id), "g1");
    assert_eq!(short(&listed[1].group_id), "g2");

    // Who leaves a group no longer has its plans in their list.
    store
        .remove_group_member(&id("g2"), &dario.id, "dario")
        .await
        .unwrap();
    assert_eq!(all(&dario, 1, None, 100).await, "zabcdef");
    assert_eq!(all(&anna, 1, None, 100).await, "zambcdef");
}

pub(crate) async fn members_say_that_they_are_in_and_take_it_back(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &dario, 2, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    come_in(store, &bernd, 1, at(10, 5)).await;
    plan_on(store, "p1", "g1", &anna, (19, None), 0).await;
    plan_on(store, "q1", "g2", &dario, (19, None), 0).await;
    let p1 = ("g1", "p1");

    // The author is in from the start. The others follow in the order in
    // which they say so.
    assert_eq!(
        set_in(store, p1, &dario, true, at(12, 10)).await,
        ["anna", "dario"]
    );
    assert_eq!(
        set_in(store, p1, &bernd, true, at(12, 5)).await,
        ["anna", "bernd", "dario"]
    );
    // Saying it again changes nothing, and keeps the place in the list.
    assert_eq!(
        set_in(store, p1, &bernd, true, at(12, 30)).await,
        ["anna", "bernd", "dario"]
    );
    // Taking it back, once or twice.
    assert_eq!(
        set_in(store, p1, &bernd, false, at(12, 35)).await,
        ["anna", "dario"]
    );
    assert_eq!(
        set_in(store, p1, &bernd, false, at(12, 36)).await,
        ["anna", "dario"]
    );
    // In again is in again from that moment: at the end of the list.
    assert_eq!(
        set_in(store, p1, &bernd, true, at(12, 40)).await,
        ["anna", "dario", "bernd"]
    );
    // The author takes it back like anyone, and the plan stays.
    assert_eq!(
        set_in(store, p1, &anna, false, at(12, 45)).await,
        ["dario", "bernd"]
    );
    assert_eq!(plans(store, "g1", &anna, day(1)).await.unwrap(), ["p1"]);
    // Those who said so at the same moment are listed by their accounts.
    assert_eq!(
        set_in(store, p1, &anna, true, at(12, 40)).await,
        ["dario", "anna", "bernd"]
    );
    // A change to who is in is no change to the plan.
    let listed = store
        .plans_of_group(&id("g1"), &dario.id, day(1), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(listed[0].changed_at, at(12, 0));

    // A plan is a plan of its own group only. Anna and Bernd are not in
    // the second group, and the first group has no plan `q1`.
    for (group, plan, expected) in [
        ("g2", "q1", PlanChange::NoSuchGroup),
        ("g1", "q1", PlanChange::NoSuchPlan),
        ("g1", "nothing", PlanChange::NoSuchPlan),
    ] {
        let said = store
            .set_plan_in(
                &id(group),
                &id(plan),
                &anna.id,
                true,
                at(13, 0),
                NaiveDate::MIN,
            )
            .await
            .unwrap();
        assert_eq!(said, expected, "{group} {plan}");
    }
    // The keeper of both names a plan under the wrong group: no such plan.
    let crossed = store
        .set_plan_in(
            &id("g2"),
            &id("p1"),
            &dario.id,
            true,
            at(13, 0),
            NaiveDate::MIN,
        )
        .await
        .unwrap();
    assert_eq!(crossed, PlanChange::NoSuchPlan);
    assert_eq!(
        set_in(store, ("g2", "q1"), &dario, true, at(13, 0)).await,
        ["dario"]
    );
}

pub(crate) async fn only_the_author_changes_a_plan_and_the_keeper_deletes_any(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &dario, 2, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    come_in(store, &bernd, 1, at(10, 5)).await;
    let posted = plan_on(store, "p1", "g1", &anna, (19, clock(9, 30)), 0).await;
    set_in(store, ("g1", "p1"), &bernd, true, at(12, 5)).await;
    plan_on(store, "q1", "g2", &dario, (19, None), 0).await;
    let g1 = id("g1");
    let other = PlanDetails {
        day: day(21),
        place: "Beispielwiese".to_owned(),
        position: Some(GeoPoint::new(47.69, 11.77).unwrap()),
        meet_at: None,
        note: Some("Later, and elsewhere.".to_owned()),
    };
    let change = |by: &Account, plan: &str, details: &PlanDetails| {
        let (g1, by, plan, details) = (g1.clone(), by.id.clone(), id(plan), details.clone());
        async move {
            store
                .change_plan(&g1, &plan, &by, &details, at(13, 0), NaiveDate::MIN)
                .await
        }
    };

    // Nobody but its author changes a plan, the keeper neither.
    for member in [&dario, &bernd] {
        assert_eq!(
            change(member, "p1", &other).await.unwrap(),
            PlanChange::NotAuthor
        );
    }
    assert_eq!(
        change(&anna, "nothing", &other).await.unwrap(),
        PlanChange::NoSuchPlan
    );
    // A plan of another group is no plan of this one.
    assert_eq!(
        change(&dario, "q1", &other).await.unwrap(),
        PlanChange::NoSuchPlan
    );
    let zero = PlanDetails {
        place: "a\0".to_owned(),
        ..other.clone()
    };
    assert!(matches!(
        change(&anna, "p1", &zero).await,
        Err(StoreError::Invalid(_))
    ));
    let listed = store
        .plans_of_group(&g1, &dario.id, day(1), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(listed[0].details, posted.details);

    // The author changes everything it says: what the change leaves out,
    // the plan no longer has. Who is in stays.
    let changed = change(&anna, "p1", &other).await.unwrap();
    let PlanChange::Now(changed) = changed else {
        panic!("{changed:?}");
    };
    assert_eq!(changed.details, other);
    assert_eq!(changed.details.meet_at, None);
    assert_eq!(changed.created_at, at(12, 0));
    assert_eq!(changed.changed_at, at(13, 0));
    assert_eq!(who_is_in(&changed), ["anna", "bernd"]);
    assert_eq!(changed.author, pilot(&anna));
    let listed = store
        .plans_of_group(&g1, &bernd.id, day(21), None)
        .await
        .unwrap();
    assert_eq!(listed, Some(vec![*changed]));

    // Deleting: the author may, the keeper may, and no other member.
    let delete = |by: &Account, plan: &str| {
        let (g1, by, plan) = (g1.clone(), by.id.clone(), id(plan));
        async move {
            store
                .delete_plan(&g1, &plan, &by, NaiveDate::MIN)
                .await
                .unwrap()
        }
    };
    assert_eq!(delete(&bernd, "p1").await, PlanRemoval::NotAllowed);
    assert_eq!(delete(&anna, "nothing").await, PlanRemoval::NoSuchPlan);
    assert_eq!(delete(&dario, "q1").await, PlanRemoval::NoSuchPlan);
    assert_eq!(plans(store, "g1", &anna, day(1)).await.unwrap(), ["p1"]);
    plan_on(store, "p2", "g1", &bernd, (20, None), 5).await;
    assert_eq!(delete(&anna, "p1").await, PlanRemoval::Removed);
    assert_eq!(delete(&anna, "p1").await, PlanRemoval::NoSuchPlan);
    assert_eq!(delete(&dario, "p2").await, PlanRemoval::Removed);
    assert!(plans(store, "g1", &anna, day(1)).await.unwrap().is_empty());
    // With a plan goes who was in: a new plan under its identifier starts
    // with its author alone.
    let again = plan_on(store, "p1", "g1", &dario, (19, None), 30).await;
    assert_eq!(who_is_in(&again), ["dario"]);
}

pub(crate) async fn a_group_holds_no_more_plans_than_it_has_room_for(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    make(store, "g2", &dario, 2, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    // Room for three plans from the 18th on.
    let room = PlanRoom {
        plans_per_group: 3,
        gone_before: day(18),
    };
    let save = |name: &'static str, group: &'static str, author: &Account, on: u32| {
        let new = new_plan(name, group, author, says(day(on), None), at(12, 0));
        async move { store.save_plan(&new, &room).await.unwrap() }
    };
    // A plan of a day that is gone takes no place.
    assert!(matches!(
        save("old", "g1", &dario, 17).await,
        PlanSaving::Stored(_)
    ));
    for (name, on) in [("p1", 18), ("p2", 19), ("p3", 19)] {
        assert!(matches!(
            save(name, "g1", &anna, on).await,
            PlanSaving::Stored(_)
        ));
    }

    // The fourth finds the group full, whoever posts it and for whichever
    // day.
    assert_eq!(save("p4", "g1", &dario, 25).await, PlanSaving::GroupFull);
    assert_eq!(save("p4", "g1", &anna, 18).await, PlanSaving::GroupFull);
    assert_eq!(
        plans(store, "g1", &dario, day(18)).await.unwrap(),
        ["p1", "p2", "p3"]
    );
    // Another group has its own room.
    assert!(matches!(
        save("q1", "g2", &dario, 19).await,
        PlanSaving::Stored(_)
    ));
    // A plan that is deleted makes room.
    store
        .delete_plan(&id("g1"), &id("p2"), &anna.id, day(18))
        .await
        .unwrap();
    assert!(matches!(
        save("p4", "g1", &dario, 25).await,
        PlanSaving::Stored(_)
    ));

    // Requests that race for the last place fill it once.
    store
        .delete_plan(&id("g1"), &id("p4"), &dario.id, day(18))
        .await
        .unwrap();
    let (a, b, c, d) = tokio::join!(
        save("r1", "g1", &dario, 20),
        save("r2", "g1", &anna, 20),
        save("r3", "g1", &dario, 21),
        save("r4", "g1", &anna, 22),
    );
    let outcomes = [a, b, c, d];
    let stored = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, PlanSaving::Stored(_)))
        .count();
    assert_eq!(stored, 1, "{outcomes:?}");
    assert_eq!(plans(store, "g1", &dario, day(18)).await.unwrap().len(), 3);
}

pub(crate) async fn plans_of_days_gone_by_are_gone_and_then_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    for (name, on) in [("p16", 16), ("p17", 17), ("p18", 18), ("p19", 19)] {
        plan_on(store, name, "g1", &anna, (on, None), on).await;
        set_in(store, ("g1", name), &dario, true, at(12, 30)).await;
    }
    let g1 = id("g1");
    // To a caller for whom plans before the 18th are gone, they are no
    // plans: not to change, not to delete, not to be in on.
    let gone_before = day(18);
    for name in ["p16", "p17"] {
        let plan = id(name);
        let changed = store
            .change_plan(
                &g1,
                &plan,
                &anna.id,
                &says(day(25), None),
                at(13, 0),
                gone_before,
            )
            .await
            .unwrap();
        assert_eq!(changed, PlanChange::NoSuchPlan, "{name}");
        let removed = store
            .delete_plan(&g1, &plan, &anna.id, gone_before)
            .await
            .unwrap();
        assert_eq!(removed, PlanRemoval::NoSuchPlan);
        let said = store
            .set_plan_in(&g1, &plan, &dario.id, false, at(13, 0), gone_before)
            .await
            .unwrap();
        assert_eq!(said, PlanChange::NoSuchPlan);
    }
    // The day itself is kept in full.
    let said = store
        .set_plan_in(&g1, &id("p18"), &dario.id, false, at(13, 0), gone_before)
        .await
        .unwrap();
    assert!(matches!(said, PlanChange::Now(plan) if who_is_in(&plan) == ["anna"]));
    // Nothing was done to those that are gone: they are still stored, for
    // whoever asks from an earlier day on.
    assert_eq!(
        plans(store, "g1", &dario, day(1)).await.unwrap(),
        ["p16", "p17", "p18", "p19"]
    );

    // Removing them takes those before the day named, with who was in,
    // and says how many.
    assert_eq!(store.prune_plans(day(18)).await.unwrap(), 2);
    assert_eq!(
        plans(store, "g1", &dario, day(1)).await.unwrap(),
        ["p18", "p19"]
    );
    assert_eq!(board(store, &anna).await, ["p18", "p19"]);
    assert_eq!(store.prune_plans(day(18)).await.unwrap(), 0);
    // Their identifiers are free again, with nobody in.
    let again = plan_on(store, "p16", "g1", &dario, (20, None), 40).await;
    assert_eq!(who_is_in(&again), ["dario"]);
    // The group and its members are as they were.
    let (keeper, members) = keeper_and_members(store, "g1", &anna).await;
    assert_eq!((keeper.as_str(), members.len()), ("dario", 2));
    assert_eq!(store.prune_plans(day(30)).await.unwrap(), 3);
    assert!(plans(store, "g1", &dario, day(1)).await.unwrap().is_empty());
}
