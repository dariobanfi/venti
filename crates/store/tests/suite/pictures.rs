//! What every store must do with pilots' pictures and the pilots near a
//! place.

use venti_domain::GeoPoint;
use venti_store::{AccountSettings, NewPicture, PictureOrigin, Store, Visibility};

use super::accounts::{create, id};
use super::at;
use super::flights::{new_flight, save};

fn picture(account_id: &str, n: u8, origin: PictureOrigin) -> NewPicture {
    NewPicture {
        id: id(&format!("picture{n}")),
        account_id: account_id.to_owned(),
        bytes: vec![n; 3],
        origin,
        created_at: at(12, n.into()),
    }
}

pub(crate) async fn a_picture_is_kept_replaced_and_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    assert_eq!(dario.picture, None);
    assert!(
        store
            .set_picture(&picture(&dario.id, 1, PictureOrigin::Upload))
            .await
            .unwrap()
    );
    let account = store.account("dario").await.unwrap().unwrap();
    assert_eq!(account.picture.as_deref(), Some(id("picture1").as_str()));
    let kept = store.picture(&id("picture1")).await.unwrap().unwrap();
    assert_eq!(
        (kept.bytes, kept.origin),
        (vec![1; 3], PictureOrigin::Upload)
    );

    // A flight names its pilot's picture as it is now.
    let flight = save(store, &new_flight(&dario, 1, at(10, 0))).await;
    assert_eq!(
        flight.pilot_picture.as_deref(),
        Some(id("picture1").as_str())
    );

    // One from Google takes no place a picture holds, and claiming it is
    // refused while there is one.
    store
        .set_picture(&picture(&dario.id, 2, PictureOrigin::Google))
        .await
        .unwrap();
    assert_eq!(store.picture(&id("picture2")).await.unwrap(), None);
    assert!(!store.claim_google_picture(&dario.id).await.unwrap());

    // Another of the pilot's own replaces it, and the old address is gone.
    store
        .set_picture(&picture(&dario.id, 3, PictureOrigin::Upload))
        .await
        .unwrap();
    assert_eq!(store.picture(&id("picture1")).await.unwrap(), None);
    let account = store.account("dario").await.unwrap().unwrap();
    assert_eq!(account.picture.as_deref(), Some(id("picture3").as_str()));

    // Removed, it is gone; the second removal finds nothing.
    let removed = store.remove_picture(&dario.id).await.unwrap().unwrap();
    assert_eq!(removed.id, id("picture3"));
    assert_eq!(store.remove_picture(&dario.id).await.unwrap(), None);
    assert_eq!(store.account("dario").await.unwrap().unwrap().picture, None);

    // Google's picture is claimed once, and then never again, picture or
    // not.
    let anna = create(store, "anna").await;
    assert!(store.claim_google_picture(&anna.id).await.unwrap());
    assert!(!store.claim_google_picture(&anna.id).await.unwrap());
    assert!(
        store
            .set_picture(&picture(&anna.id, 4, PictureOrigin::Google))
            .await
            .unwrap()
    );
    assert_eq!(
        store
            .picture(&id("picture4"))
            .await
            .unwrap()
            .unwrap()
            .origin,
        PictureOrigin::Google
    );
    // Nobody's account has no picture and claims nothing.
    assert!(
        !store
            .set_picture(&picture(&id("nobody"), 5, PictureOrigin::Upload))
            .await
            .unwrap()
    );
    assert!(!store.claim_google_picture(&id("nobody")).await.unwrap());

    // An account that goes takes its picture along.
    store.delete_account(&anna.id).await.unwrap();
    assert_eq!(store.picture(&id("picture4")).await.unwrap(), None);
}

pub(crate) async fn pilots_near_a_place_are_counted_from_public_flights(store: &impl Store) {
    // The test flights take off at 47.6642, 11.7967.
    let near = GeoPoint::new(47.66, 11.8).unwrap();
    let far = GeoPoint::new(46.5, 11.8).unwrap();
    let publish = async |account: &venti_store::Account, n: u8, start| {
        let flight = save(store, &new_flight(account, n, start)).await;
        store
            .set_visibility(&account.id, &flight.id, Visibility::Public, at(21, 0))
            .await
            .unwrap();
    };
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let mara = create(store, "mara").await;
    publish(&dario, 1, at(9, 0)).await;
    publish(&anna, 1, at(9, 0)).await;
    publish(&anna, 2, at(10, 0)).await;
    // Mara's flight stays private: it counts for nobody.
    save(store, &new_flight(&mara, 1, at(9, 0))).await;

    let found = store
        .pilots_near(near, 5_000.0, at(0, 0), 10)
        .await
        .unwrap();
    let names: Vec<(&str, u32)> = found
        .iter()
        .map(|pilot| (pilot.handle.as_str(), pilot.flights))
        .collect();
    assert_eq!(names, [("anna", 2), ("dario", 1)]);
    assert_eq!(found[0].account_id, anna.id);
    assert!(
        store
            .pilots_near(far, 5_000.0, at(0, 0), 10)
            .await
            .unwrap()
            .is_empty()
    );
    // Flights older than `since` are not counted; the limit keeps the first.
    let later = store
        .pilots_near(near, 5_000.0, at(9, 30), 10)
        .await
        .unwrap();
    assert_eq!(later.len(), 1);
    assert_eq!((later[0].handle.as_str(), later[0].flights), ("anna", 1));
    assert_eq!(
        store.pilots_near(near, 5_000.0, at(0, 0), 1).await.unwrap()[0].handle,
        "anna"
    );

    // A pilot who leaves the list is in it no more.
    let off = AccountSettings {
        shown_near_places: Some(false),
        ..AccountSettings::default()
    };
    let anna_off = store
        .update_account_settings(&anna.id, &off)
        .await
        .unwrap()
        .unwrap();
    assert!(!anna_off.shown_near_places);
    let found = store
        .pilots_near(near, 5_000.0, at(0, 0), 10)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].handle, "dario");
    // Nor is one whose publishing is suspended.
    store
        .suspend_publishing(&dario.id, at(22, 0))
        .await
        .unwrap();
    assert!(
        store
            .pilots_near(near, 5_000.0, at(0, 0), 10)
            .await
            .unwrap()
            .is_empty()
    );
}
