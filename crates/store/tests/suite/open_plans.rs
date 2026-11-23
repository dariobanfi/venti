//! Open plans: posted for everyone flying from an area, found by place
//! and day, with who is in, bounded, reportable, and gone with their
//! author and with their day.

use chrono::{NaiveDate, NaiveTime};
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{
    NewOpenPlan, NewReport, OpenPlan, OpenPlanArea, OpenPlanChange, OpenPlanDetails,
    OpenPlanRemoval, OpenPlanRoom, OpenPlanSaving, ReportListing, ReportSaved, ReportSubject,
    Store, StoreError,
};

use super::accounts::{create, id};
use super::at;
use super::groups::day;

/// The Wallberg's take-off, and a place 40 km west of it.
fn wallberg() -> GeoPoint {
    GeoPoint::new(47.6642, 11.7967).unwrap()
}

fn farther_west() -> GeoPoint {
    GeoPoint::new(47.6642, 11.2650).unwrap()
}

fn room() -> OpenPlanRoom {
    OpenPlanRoom {
        plans_per_account: 3,
        pilots_per_plan: 3,
        gone_before: day(17),
    }
}

fn says(on: NaiveDate, at_place: GeoPoint, seats: Option<u8>) -> OpenPlanDetails {
    OpenPlanDetails {
        day: on,
        place: "Wallberg, Talstation".to_owned(),
        position: at_place,
        meet_at: NaiveTime::from_hms_opt(9, 30, 0),
        seats,
        note: Some("Two seats from Munich.\nBack by six.".to_owned()),
    }
}

fn new_plan(name: &str, author: &str, details: OpenPlanDetails, posted: Timestamp) -> NewOpenPlan {
    NewOpenPlan {
        id: id(name),
        author_id: id(author),
        created_at: posted,
        details,
    }
}

async fn post(store: &impl Store, new: &NewOpenPlan) -> OpenPlan {
    match store.save_open_plan(new, &room()).await.unwrap() {
        OpenPlanSaving::Stored(plan) => *plan,
        other => panic!("not stored: {other:?}"),
    }
}

fn who_is_in(plan: &OpenPlan) -> Vec<&str> {
    plan.pilots_in
        .iter()
        .map(|pilot| pilot.handle.as_str())
        .collect()
}

fn ids(plans: &[OpenPlan]) -> Vec<&str> {
    plans
        .iter()
        .map(|plan| plan.id.trim_start_matches('0'))
        .collect()
}

fn area(around: GeoPoint, radius_km: f64, from: NaiveDate, to: NaiveDate) -> OpenPlanArea {
    OpenPlanArea {
        around,
        radius_m: radius_km * 1000.0,
        from,
        to,
    }
}

pub(crate) async fn an_open_plan_is_stored_and_found_near_its_place(store: &impl Store) {
    let anna = create(store, "anna").await;
    create(store, "bernd").await;
    let new = new_plan("p1", "anna", says(day(20), wallberg(), Some(2)), at(10, 0));

    let plan = post(store, &new).await;

    assert_eq!(plan.id, new.id);
    assert_eq!(plan.author.handle, "anna");
    assert_eq!(plan.author.display_name, anna.display_name);
    assert_eq!(plan.details, new.details);
    assert_eq!((plan.created_at, plan.changed_at), (at(10, 0), at(10, 0)));
    assert_eq!(who_is_in(&plan), ["anna"]);
    assert_eq!(
        store.open_plan(&new.id, day(17)).await.unwrap(),
        Some(plan.clone())
    );

    // Found within the radius and on the days asked for, and nowhere else.
    let near = |around, km, from, to| {
        let area = area(around, km, from, to);
        async move { store.open_plans_near(&area, 10).await.unwrap() }
    };
    assert_eq!(
        near(wallberg(), 5.0, day(18), day(24)).await,
        std::slice::from_ref(&plan)
    );
    assert_eq!(
        near(farther_west(), 45.0, day(20), day(20)).await,
        std::slice::from_ref(&plan)
    );
    assert!(
        near(farther_west(), 35.0, day(18), day(24))
            .await
            .is_empty()
    );
    assert!(near(wallberg(), 5.0, day(21), day(24)).await.is_empty());
    assert!(near(wallberg(), 5.0, day(18), day(19)).await.is_empty());
    // A pilot's own list holds what they posted.
    assert_eq!(
        store.open_plans_of(&id("anna"), day(17), 10).await.unwrap(),
        [plan]
    );
    assert!(
        store
            .open_plans_of(&id("bernd"), day(17), 10)
            .await
            .unwrap()
            .is_empty()
    );
}

pub(crate) async fn open_plans_are_listed_by_day_and_time(store: &impl Store) {
    create(store, "anna").await;
    create(store, "bernd").await;
    let late = OpenPlanDetails {
        meet_at: NaiveTime::from_hms_opt(14, 0, 0),
        ..says(day(20), wallberg(), None)
    };
    let untimed = OpenPlanDetails {
        meet_at: None,
        ..says(day(20), wallberg(), None)
    };
    post(store, &new_plan("c", "anna", untimed, at(9, 0))).await;
    post(store, &new_plan("b", "bernd", late, at(9, 5))).await;
    post(
        store,
        &new_plan("a", "anna", says(day(20), wallberg(), None), at(9, 10)),
    )
    .await;
    post(
        store,
        &new_plan("d", "bernd", says(day(19), wallberg(), None), at(9, 15)),
    )
    .await;

    let area = area(wallberg(), 10.0, day(18), day(24));
    let all = store.open_plans_near(&area, 10).await.unwrap();
    assert_eq!(ids(&all), ["d", "a", "b", "c"]);
    let first = store.open_plans_near(&area, 2).await.unwrap();
    assert_eq!(ids(&first), ["d", "a"]);
}

pub(crate) async fn pilots_say_they_are_in_on_an_open_plan(store: &impl Store) {
    for handle in ["anna", "bernd", "carla", "dario"] {
        create(store, handle).await;
    }
    let plan = post(
        store,
        &new_plan("p1", "anna", says(day(20), wallberg(), Some(3)), at(10, 0)),
    )
    .await;
    let set = |handle: &'static str, is_in: bool, when: Timestamp| {
        let plan = plan.id.clone();
        async move {
            store
                .set_open_plan_in(&plan, &id(handle), is_in, when, &room())
                .await
                .unwrap()
        }
    };

    let OpenPlanChange::Now(now) = set("bernd", true, at(11, 0)).await else {
        panic!("bernd is not in");
    };
    assert_eq!(who_is_in(&now), ["anna", "bernd"]);
    // Again: nothing changes, and the first moment stays.
    set("bernd", true, at(12, 0)).await;
    let OpenPlanChange::Now(now) = set("carla", true, at(11, 30)).await else {
        panic!("carla is not in");
    };
    assert_eq!(who_is_in(&now), ["anna", "bernd", "carla"]);
    // Three is as many as the room lets in.
    assert_eq!(set("dario", true, at(12, 0)).await, OpenPlanChange::Full);
    // The author can take it back like anyone else; the plan stays.
    let OpenPlanChange::Now(now) = set("anna", false, at(12, 10)).await else {
        panic!("anna is still in");
    };
    assert_eq!(who_is_in(&now), ["bernd", "carla"]);
    let OpenPlanChange::Now(now) = set("dario", true, at(12, 20)).await else {
        panic!("dario is not in");
    };
    assert_eq!(who_is_in(&now), ["bernd", "carla", "dario"]);
    // What a pilot is in on is on their own list.
    assert_eq!(
        ids(&store
            .open_plans_of(&id("dario"), day(17), 10)
            .await
            .unwrap()),
        ["p1"]
    );
    // No plan, or one that is no identifier, says so.
    assert_eq!(
        store
            .set_open_plan_in(&id("none"), &id("dario"), true, at(13, 0), &room())
            .await
            .unwrap(),
        OpenPlanChange::NoSuchPlan
    );
    assert_eq!(
        store
            .set_open_plan_in("x\0", &id("dario"), true, at(13, 0), &room())
            .await
            .unwrap(),
        OpenPlanChange::NoSuchPlan
    );
}

pub(crate) async fn only_its_author_changes_an_open_plan(store: &impl Store) {
    create(store, "anna").await;
    create(store, "bernd").await;
    let plan = post(
        store,
        &new_plan("p1", "anna", says(day(20), wallberg(), Some(2)), at(10, 0)),
    )
    .await;
    store
        .set_open_plan_in(&plan.id, &id("bernd"), true, at(10, 30), &room())
        .await
        .unwrap();
    let changed = OpenPlanDetails {
        seats: None,
        note: None,
        meet_at: None,
        ..says(day(21), farther_west(), None)
    };

    assert_eq!(
        store
            .change_open_plan(&plan.id, &id("bernd"), &changed, at(11, 0), day(17))
            .await
            .unwrap(),
        OpenPlanChange::NotAuthor
    );
    let OpenPlanChange::Now(now) = store
        .change_open_plan(&plan.id, &id("anna"), &changed, at(11, 0), day(17))
        .await
        .unwrap()
    else {
        panic!("not changed");
    };
    assert_eq!(now.details, changed);
    assert_eq!((now.created_at, now.changed_at), (at(10, 0), at(11, 0)));
    assert_eq!(who_is_in(&now), ["anna", "bernd"]);

    assert_eq!(
        store
            .delete_open_plan(&plan.id, &id("bernd"), day(17))
            .await
            .unwrap(),
        OpenPlanRemoval::NotAuthor
    );
    assert_eq!(
        store
            .delete_open_plan(&plan.id, &id("anna"), day(17))
            .await
            .unwrap(),
        OpenPlanRemoval::Removed
    );
    assert_eq!(store.open_plan(&plan.id, day(17)).await.unwrap(), None);
    assert_eq!(
        store
            .delete_open_plan(&plan.id, &id("anna"), day(17))
            .await
            .unwrap(),
        OpenPlanRemoval::NoSuchPlan
    );
    assert_eq!(
        store
            .change_open_plan(&plan.id, &id("anna"), &changed, at(12, 0), day(17))
            .await
            .unwrap(),
        OpenPlanChange::NoSuchPlan
    );
}

pub(crate) async fn an_account_holds_few_open_plans(store: &impl Store) {
    create(store, "anna").await;
    for (number, on) in [(1, 17), (2, 18), (3, 19)] {
        post(
            store,
            &new_plan(
                &format!("p{number}"),
                "anna",
                says(day(on), wallberg(), None),
                at(10, number),
            ),
        )
        .await;
    }
    let fourth = new_plan("p4", "anna", says(day(20), wallberg(), None), at(10, 4));
    assert_eq!(
        store.save_open_plan(&fourth, &room()).await.unwrap(),
        OpenPlanSaving::TooMany
    );
    // A plan whose day is gone takes no place.
    let later = OpenPlanRoom {
        gone_before: day(18),
        ..room()
    };
    assert!(matches!(
        store.save_open_plan(&fourth, &later).await.unwrap(),
        OpenPlanSaving::Stored(_)
    ));
    // An identifier in use, and an author who is nobody, are refused.
    let again = new_plan("p4", "anna", says(day(21), wallberg(), None), at(10, 5));
    assert!(matches!(
        store
            .save_open_plan(
                &again,
                &OpenPlanRoom {
                    plans_per_account: 10,
                    ..later
                }
            )
            .await,
        Err(StoreError::Invalid(_))
    ));
    let nobody = new_plan("p5", "nobody", says(day(21), wallberg(), None), at(10, 6));
    assert!(matches!(
        store.save_open_plan(&nobody, &later).await,
        Err(StoreError::Invalid(_))
    ));
}

pub(crate) async fn open_plans_go_with_their_day_and_author(store: &impl Store) {
    create(store, "anna").await;
    create(store, "bernd").await;
    let old = post(
        store,
        &new_plan("old", "anna", says(day(17), wallberg(), None), at(9, 0)),
    )
    .await;
    let kept = post(
        store,
        &new_plan("kept", "anna", says(day(19), wallberg(), None), at(9, 0)),
    )
    .await;
    let bernds = post(
        store,
        &new_plan("b", "bernd", says(day(19), wallberg(), None), at(9, 0)),
    )
    .await;
    store
        .set_open_plan_in(&bernds.id, &id("anna"), true, at(9, 30), &room())
        .await
        .unwrap();

    // Gone by its day before it is removed, and then removed.
    assert_eq!(store.open_plan(&old.id, day(18)).await.unwrap(), None);
    assert_eq!(store.prune_plans(day(18)).await.unwrap(), 1);
    assert_eq!(store.open_plan(&old.id, day(17)).await.unwrap(), None);
    assert!(store.open_plan(&kept.id, day(17)).await.unwrap().is_some());

    // An account that goes takes its plans along, and its word on others.
    store.delete_account(&id("anna")).await.unwrap();
    assert_eq!(store.open_plan(&kept.id, day(17)).await.unwrap(), None);
    let left = store.open_plan(&bernds.id, day(17)).await.unwrap().unwrap();
    assert_eq!(who_is_in(&left), ["bernd"]);
}

pub(crate) async fn the_operator_removes_a_reported_open_plan(store: &impl Store) {
    create(store, "anna").await;
    create(store, "bernd").await;
    let plan = post(
        store,
        &new_plan("p1", "anna", says(day(20), wallberg(), None), at(9, 0)),
    )
    .await;
    let subject = ReportSubject::OpenPlan {
        id: plan.id.clone(),
        place: plan.details.place.clone(),
        note: plan.details.note.clone(),
    };
    let report = NewReport {
        id: id("r1"),
        created_at: at(9, 30),
        reporter_id: id("bernd"),
        about_account_id: Some(id("anna")),
        subject: subject.clone(),
        reason: "spam".to_owned(),
        text: None,
    };
    assert_eq!(
        store.save_report(&report).await.unwrap(),
        ReportSaved::Stored
    );

    let removed = store.remove_open_plan(&plan.id, day(17)).await.unwrap();
    assert_eq!(removed, Some(plan.clone()));
    assert_eq!(
        store.remove_open_plan(&plan.id, day(17)).await.unwrap(),
        None
    );
    assert_eq!(store.open_plan(&plan.id, day(17)).await.unwrap(), None);

    // The report still says what the plan said.
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].subject, subject);
}
