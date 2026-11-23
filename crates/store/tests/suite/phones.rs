//! What every store must do with the phones registered for push, the
//! settings for what pilots nearby do, their positions, and requests for
//! help after landing.

use chrono::Duration;
use venti_domain::GeoPoint;
use venti_store::{
    AddingDevice, Circle, HelpAnswering, HelpCreation, HelpReach, NearbySettings, NewHelpRequest,
    PhoneDevice, Store,
};

use super::accounts::{create, id, sign_in};
use super::at;

fn phone(account_id: &str, session: &str, n: u32, minute: u32) -> PhoneDevice {
    PhoneDevice {
        account_id: account_id.to_owned(),
        session_id: id(session),
        platform: if n.is_multiple_of(2) { "fcm" } else { "apns" }.to_owned(),
        token: format!("token{n}"),
        sandbox: n == 1,
        created_at: at(10, minute),
    }
}

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

/// Innsbruck, and places some kilometres away.
fn innsbruck() -> GeoPoint {
    point(47.27, 11.39)
}

fn asking(store_ids: (&str, &str)) -> NewHelpRequest {
    NewHelpRequest {
        id: id(store_ids.0),
        account_id: store_ids.1.to_owned(),
        position: innsbruck(),
        note: Some("By the river, anyone going to Innsbruck?".to_owned()),
        created_at: at(12, 0),
        ends_at: at(16, 0),
    }
}

fn reach() -> HelpReach {
    HelpReach {
        radius_km: 30,
        positions_since: at(11, 0),
        most: 2,
    }
}

fn helping() -> NearbySettings {
    NearbySettings {
        open_plans_near: None,
        may_be_asked_for_help: true,
    }
}

pub(crate) async fn a_phone_is_registered_bounded_and_goes_with_its_session(store: &impl Store) {
    let dario = create(store, "dario").await;
    sign_in(store, "s1", &dario, at(9, 0), 1).await;
    sign_in(store, "s2", &dario, at(9, 0), 2).await;
    for (n, session) in [(1, "s1"), (2, "s2")] {
        assert_eq!(
            store
                .add_device(&phone(&dario.id, session, n, n), 2)
                .await
                .unwrap(),
            AddingDevice::Added
        );
    }
    let ids = [dario.id.clone()];
    assert_eq!(
        store.devices_of(&ids).await.unwrap(),
        [phone(&dario.id, "s1", 1, 1), phone(&dario.id, "s2", 2, 2)]
    );
    assert_eq!(
        store
            .add_device(&phone(&dario.id, "s1", 3, 3), 2)
            .await
            .unwrap(),
        AddingDevice::Full
    );
    // A session that is not there registers nothing.
    assert_eq!(
        store
            .add_device(&phone(&dario.id, "gone", 4, 4), 2)
            .await
            .unwrap(),
        AddingDevice::NoSession
    );

    // A token another account registers is that account's from then on.
    let anna = create(store, "anna").await;
    sign_in(store, "a1", &anna, at(9, 0), 3).await;
    assert_eq!(
        store
            .add_device(&phone(&anna.id, "a1", 2, 5), 2)
            .await
            .unwrap(),
        AddingDevice::Added
    );
    assert_eq!(
        store.devices_of(&ids).await.unwrap(),
        [phone(&dario.id, "s1", 1, 1)]
    );
    assert!(
        !store
            .remove_device(&anna.id, "apns", "token1")
            .await
            .unwrap()
    );

    // Signing out of a session takes its phone along.
    assert!(store.delete_session(&dario.id, &id("s1")).await.unwrap());
    assert!(store.devices_of(&ids).await.unwrap().is_empty());

    // A push service that says a token is no more removes it.
    assert!(store.drop_device("fcm", "token2").await.unwrap());
    assert!(!store.drop_device("fcm", "token2").await.unwrap());
    assert!(
        store
            .devices_of(std::slice::from_ref(&anna.id))
            .await
            .unwrap()
            .is_empty()
    );
}

pub(crate) async fn nearby_settings_start_off_and_hold_a_position(store: &impl Store) {
    let dario = create(store, "dario").await;
    assert_eq!(
        store.nearby_settings(&dario.id).await.unwrap(),
        NearbySettings::default()
    );
    assert!(
        !store
            .keep_position(&dario.id, innsbruck(), at(11, 0))
            .await
            .unwrap()
    );
    assert_eq!(store.position_of(&dario.id, at(10, 0)).await.unwrap(), None);

    let settings = NearbySettings {
        open_plans_near: Some(Circle {
            position: point(47.6, 11.7),
            radius_km: 30,
        }),
        may_be_asked_for_help: true,
    };
    assert!(
        store
            .set_nearby_settings(&dario.id, &settings, at(10, 0))
            .await
            .unwrap()
    );
    assert!(
        !store
            .set_nearby_settings(&id("nobody"), &settings, at(10, 0))
            .await
            .unwrap()
    );
    assert_eq!(store.nearby_settings(&dario.id).await.unwrap(), settings);
    assert!(
        store
            .keep_position(&dario.id, innsbruck(), at(11, 0))
            .await
            .unwrap()
    );
    assert_eq!(
        store.position_of(&dario.id, at(11, 0)).await.unwrap(),
        Some(innsbruck())
    );
    assert_eq!(store.position_of(&dario.id, at(11, 1)).await.unwrap(), None);

    // The circle holds a plan at the Wallberg, not one at Innsbruck.
    let anna = create(store, "anna").await;
    let wallberg = point(47.65, 11.79);
    assert_eq!(
        store
            .open_plan_watchers(wallberg, &anna.id, 10)
            .await
            .unwrap(),
        std::slice::from_ref(&dario.id)
    );
    assert!(
        store
            .open_plan_watchers(wallberg, &dario.id, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .open_plan_watchers(innsbruck(), &anna.id, 10)
            .await
            .unwrap()
            .is_empty()
    );

    // Turning help off forgets the position.
    let off = NearbySettings {
        may_be_asked_for_help: false,
        ..settings
    };
    store
        .set_nearby_settings(&dario.id, &off, at(11, 5))
        .await
        .unwrap();
    assert_eq!(store.position_of(&dario.id, at(10, 0)).await.unwrap(), None);
}

pub(crate) async fn a_request_for_help_goes_to_the_nearest_who_may_be_asked(store: &impl Store) {
    let asker = create(store, "asker").await;
    // Near, nearer, too far, too old, and one who may not be asked.
    let places = [
        ("near", point(47.35, 11.39), 11, true),
        ("nearer", point(47.28, 11.40), 11, true),
        ("nearest", point(47.27, 11.39), 11, true),
        ("far", point(48.2, 11.39), 11, true),
        ("old", point(47.27, 11.39), 10, true),
        ("quiet", point(47.27, 11.39), 11, false),
    ];
    let mut ids = Vec::new();
    for (name, position, hour, may) in places {
        let pilot = create(store, name).await;
        store
            .set_nearby_settings(&pilot.id, &helping(), at(9, 0))
            .await
            .unwrap();
        store
            .keep_position(&pilot.id, position, at(hour, 30))
            .await
            .unwrap();
        if !may {
            store
                .set_nearby_settings(&pilot.id, &NearbySettings::default(), at(9, 0))
                .await
                .unwrap();
        }
        ids.push(pilot.id);
    }
    // The asker's own position does not make them asked.
    store
        .set_nearby_settings(&asker.id, &helping(), at(9, 0))
        .await
        .unwrap();
    store
        .keep_position(&asker.id, innsbruck(), at(11, 30))
        .await
        .unwrap();

    let HelpCreation::Created(request) = store
        .create_help_request(&asking(("h1", &asker.id)), &reach())
        .await
        .unwrap()
    else {
        panic!("not created");
    };
    assert_eq!(request.asked, [ids[2].clone(), ids[1].clone()]);
    assert_eq!(request.pilot.handle, "asker");
    assert!(request.stands(at(12, 0)));
    assert!(request.helpers.is_empty());
    assert_eq!(
        store.help_request(&id("h1")).await.unwrap(),
        Some(*request.clone())
    );

    // One request stands at a time.
    assert_eq!(
        store
            .create_help_request(&asking(("h2", &asker.id)), &reach())
            .await
            .unwrap(),
        HelpCreation::StillOpen
    );
    assert_eq!(
        store
            .create_help_request(&asking(("h3", &id("nobody"))), &reach())
            .await
            .unwrap(),
        HelpCreation::NoAccount
    );
    assert_eq!(
        store.help_requests_of(&asker.id, at(12, 5)).await.unwrap(),
        [*request.clone()]
    );
    assert_eq!(
        store.help_requests_asked(&ids[2], at(12, 5)).await.unwrap(),
        [*request.clone()]
    );
    assert!(
        store
            .help_requests_asked(&ids[0], at(12, 5))
            .await
            .unwrap()
            .is_empty()
    );
}

pub(crate) async fn help_is_answered_closed_and_pruned(store: &impl Store) {
    let asker = create(store, "asker").await;
    let helper = create(store, "helper").await;
    let other = create(store, "other").await;
    for pilot in [&helper, &other] {
        store
            .set_nearby_settings(&pilot.id, &helping(), at(9, 0))
            .await
            .unwrap();
        store
            .keep_position(&pilot.id, innsbruck(), at(11, 30))
            .await
            .unwrap();
    }
    let HelpCreation::Created(_) = store
        .create_help_request(&asking(("h1", &asker.id)), &reach())
        .await
        .unwrap()
    else {
        panic!("not created");
    };
    let h1 = id("h1");
    // Only those asked answer.
    assert_eq!(
        store
            .answer_help_request(&h1, &asker.id, true, at(12, 1))
            .await
            .unwrap(),
        HelpAnswering::NotAsked
    );
    let HelpAnswering::Answered(answered, true) = store
        .answer_help_request(&h1, &helper.id, true, at(12, 1))
        .await
        .unwrap()
    else {
        panic!("not answered");
    };
    assert!(answered.can_help(&helper.id));
    assert!(!answered.can_help(&other.id));
    // The same answer again changes nothing; taking it back does.
    assert!(matches!(
        store
            .answer_help_request(&h1, &helper.id, true, at(12, 2))
            .await
            .unwrap(),
        HelpAnswering::Answered(_, false)
    ));
    let HelpAnswering::Answered(back, true) = store
        .answer_help_request(&h1, &other.id, true, at(12, 3))
        .await
        .unwrap()
    else {
        panic!("not answered");
    };
    assert_eq!(back.helpers.len(), 2);
    assert_eq!(back.helpers[0].handle, "helper");

    // Only its pilot closes it; then nobody answers.
    assert_eq!(
        store
            .close_help_request(&h1, &helper.id, at(12, 10))
            .await
            .unwrap(),
        None
    );
    let closed = store
        .close_help_request(&h1, &asker.id, at(12, 10))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.closed_at, Some(at(12, 10)));
    assert!(!closed.stands(at(12, 11)));
    assert_eq!(
        store
            .answer_help_request(&h1, &helper.id, false, at(12, 11))
            .await
            .unwrap(),
        HelpAnswering::NotAsked
    );
    assert!(
        store
            .help_requests_of(&asker.id, at(12, 11))
            .await
            .unwrap()
            .is_empty()
    );

    // A new one may be asked once the last is closed; it ends by itself.
    let mut next = asking(("h2", &asker.id));
    next.created_at = at(12, 20);
    assert!(matches!(
        store.create_help_request(&next, &reach()).await.unwrap(),
        HelpCreation::Created(_)
    ));
    assert!(
        store
            .help_requests_asked(&helper.id, at(16, 0))
            .await
            .unwrap()
            .is_empty()
    );

    // Pruned: positions told before, requests ended before.
    let pruned = store.prune_nearby(at(11, 31), at(13, 0)).await.unwrap();
    assert_eq!(pruned, 3);
    assert_eq!(store.help_request(&h1).await.unwrap(), None);
    assert!(store.help_request(&id("h2")).await.unwrap().is_some());
    assert_eq!(store.position_of(&helper.id, at(0, 0)).await.unwrap(), None);

    // An account that goes takes its requests and answers along.
    store.delete_account(&asker.id).await.unwrap();
    assert_eq!(store.help_request(&id("h2")).await.unwrap(), None);
    store
        .prune_nearby(at(0, 0), at(0, 0) - Duration::days(1))
        .await
        .unwrap();
}
