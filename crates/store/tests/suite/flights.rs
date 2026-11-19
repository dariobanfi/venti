//! What every store must do with pilots' flights.

use chrono::Duration;
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{
    Account, Derived, Flight, Kept, Listing, NewFlight, Place, Room, Saved, Score, ScoreKind,
    Store, StoreError, Visibility,
};

use super::accounts::{create, id};
use super::at;

// --- Helpers -------------------------------------------------------------

/// What an analysis of a short flight from `start` gives.
fn derived(start: Timestamp) -> Derived {
    Derived {
        version: 1,
        start,
        airtime_s: 1744,
        takeoff: Some(Place {
            time: start,
            position: GeoPoint::new(47.6642, 11.7967).unwrap(),
            altitude_m: 1620,
        }),
        landing: Some(Place {
            time: start + Duration::seconds(1744),
            position: GeoPoint::new(47.683_333_333_3, 11.76).unwrap(),
            altitude_m: 786,
        }),
        track_length_m: 16_432.1,
        max_altitude_m: 2117,
        max_climb_ms: 2.41,
        score: Some(Score {
            kind: ScoreKind::FreeTriangle,
            closed: false,
            distance_m: 9876.5,
            points: 11.85,
        }),
        glider: Some("Sample Wing".to_owned()),
        outline: vec![[47.6642, 11.7967], [47.7, 11.75], [47.683_33, 11.76]],
        profile: vec![[0, 1620], [342, 2117], [1744, 786]],
    }
}

/// Flight number `n` of an account, starting at `start`. The identity
/// depends on `n` alone, the address on the account and `n`.
pub(crate) fn new_flight(account: &Account, n: u8, start: Timestamp) -> NewFlight {
    NewFlight {
        id: id(&format!("{}{n}", account.handle)),
        account_id: account.id.clone(),
        identity: format!("{n:032x}"),
        uploaded_at: at(20, 0),
        file: format!("gzip of flight {n}").into_bytes(),
        file_bytes: 64_000 + u32::from(n),
        derived: derived(start),
    }
}

/// What a flight reads back as right after it was stored.
fn stored(new: &NewFlight, account: &Account, change: i64) -> Flight {
    Flight {
        id: new.id.clone(),
        account_id: new.account_id.clone(),
        pilot_handle: account.handle.clone(),
        pilot_name: account.display_name.clone(),
        identity: new.identity.clone(),
        visibility: Visibility::Private,
        uploaded_at: new.uploaded_at,
        file_bytes: new.file_bytes,
        derived: new.derived.clone(),
        change,
    }
}

/// Stores a flight for an account with room for anything, and says
/// whether that created it.
pub(crate) async fn put(store: &impl Store, new: &NewFlight) -> (Flight, bool) {
    let saved = store.save_flight(new, &Room::UNLIMITED).await.unwrap();
    saved.stored().expect("there is room for anything")
}

async fn save(store: &impl Store, new: &NewFlight) -> Flight {
    let (flight, created) = put(store, new).await;
    assert!(created, "{} was already there", new.id);
    flight
}

/// The change number a change made at `time` gets when nothing came later.
fn micros(time: Timestamp) -> i64 {
    time.timestamp_micros()
}

fn short(flights: &[Flight]) -> Vec<String> {
    flights
        .iter()
        .map(|flight| flight.id.trim_start_matches('0').to_owned())
        .collect()
}

async fn list(store: &impl Store, account: &Account, listing: Listing) -> Vec<String> {
    let flights = store
        .flights_of(&account.id, listing, None, 100)
        .await
        .unwrap();
    short(&flights)
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn a_flight_is_stored_private_and_read_back_exactly(store: &impl Store) {
    let dario = create(store, "dario").await;
    // Bytes that are not text, and values that do not survive rounding.
    let new = NewFlight {
        file: vec![0x1f, 0x8b, 0x00, 0xff, 0x00, 0x27, 0x5c, 0x80],
        derived: Derived {
            track_length_m: 0.1 + 0.2,
            max_climb_ms: 2.400_000_000_000_000_4,
            glider: Some("Wing “Ω” <i>'; DROP TABLE flights;--".to_owned()),
            ..derived(at(10, 30))
        },
        ..new_flight(&dario, 1, at(10, 30))
    };

    let (flight, created) = put(store, &new).await;

    assert!(created);
    let expected = stored(&new, &dario, micros(new.uploaded_at));
    assert_eq!(flight, expected);
    assert_eq!(store.flight(&new.id).await.unwrap(), Some(expected));
    assert_eq!(store.flight_file(&new.id).await.unwrap(), Some(new.file));
    assert_eq!(store.flight(&id("unknown")).await.unwrap(), None);
    assert_eq!(store.flight_file(&id("unknown")).await.unwrap(), None);

    // A recording that never shows the glider flying has less to say.
    let ground = NewFlight {
        derived: Derived {
            takeoff: None,
            landing: None,
            score: None,
            glider: None,
            outline: Vec::new(),
            profile: Vec::new(),
            ..derived(at(12, 0))
        },
        ..new_flight(&dario, 2, at(12, 0))
    };
    let flight = save(store, &ground).await;
    assert_eq!(flight.derived, ground.derived);
    assert_eq!(
        store.flight(&ground.id).await.unwrap().unwrap().derived,
        ground.derived
    );
}

pub(crate) async fn storing_the_same_flight_again_changes_nothing(store: &impl Store) {
    let dario = create(store, "dario").await;
    let first = new_flight(&dario, 1, at(10, 30));
    let flight = save(store, &first).await;
    store
        .set_visibility(&dario.id, &first.id, Visibility::Public, at(21, 0))
        .await
        .unwrap();
    let before = store.flight(&first.id).await.unwrap().unwrap();
    assert_ne!(before, flight);

    // The same fixes arrive again, in a file with other headers.
    let again = NewFlight {
        uploaded_at: at(22, 0),
        file: b"another file with the same fixes".to_vec(),
        file_bytes: 1,
        derived: Derived {
            glider: Some("Another Wing".to_owned()),
            ..derived(at(10, 30))
        },
        ..first.clone()
    };
    let (returned, created) = put(store, &again).await;

    assert!(!created);
    assert_eq!(returned, before);
    assert_eq!(store.flight(&first.id).await.unwrap(), Some(before));
    assert_eq!(
        store.flight_file(&first.id).await.unwrap(),
        Some(first.file)
    );
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(changes.flights.len(), 1);
    assert_eq!(
        changes.latest,
        micros(at(21, 0)),
        "nothing changed at 22:00"
    );
}

pub(crate) async fn two_accounts_may_hold_the_same_flight(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let darios = new_flight(&dario, 1, at(10, 30));
    let annas = new_flight(&anna, 1, at(10, 30));
    assert_eq!(darios.identity, annas.identity);

    let first = save(store, &darios).await;
    let second = save(store, &annas).await;

    assert_eq!(first.pilot_handle, "dario");
    assert_eq!(second.pilot_handle, "anna");
    assert_eq!(list(store, &dario, Listing::All).await, ["dario1"]);
    assert_eq!(list(store, &anna, Listing::All).await, ["anna1"]);
}

pub(crate) async fn uploads_racing_for_one_flight_create_it_once(store: &impl Store) {
    let dario = create(store, "dario").await;
    let new = new_flight(&dario, 1, at(10, 30));

    let (a, b, c, d) = tokio::join!(
        put(store, &new),
        put(store, &new),
        put(store, &new),
        put(store, &new),
    );

    let results = [a, b, c, d];
    let created = results.iter().filter(|(_, created)| *created).count();
    assert_eq!(created, 1);
    for (flight, _) in &results {
        assert_eq!(flight, &results[0].0);
    }
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(changes.flights.len(), 1);
    assert_eq!(changes.latest, changes.flights[0].change);
}

pub(crate) async fn a_flight_for_an_unknown_account_is_refused(store: &impl Store) {
    let nobody = super::accounts::account("nobody");
    let flight = new_flight(&nobody, 1, at(10, 30));
    let result = store.save_flight(&flight, &Room::UNLIMITED).await;
    assert!(
        matches!(result, Err(StoreError::Invalid(_))),
        "expected a refusal, got {result:?}"
    );
    assert_eq!(store.flight(&id("nobody1")).await.unwrap(), None);
}

pub(crate) async fn an_account_keeps_no_more_than_it_has_room_for(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    // `new_flight` gives flight n a file of 64,000 + n bytes.
    let flight = |account: &Account, n: u8| new_flight(account, n, at(10, n.into()));
    let three_flights = Room {
        flights: 3,
        file_bytes: u64::MAX,
    };
    let try_save =
        |new: NewFlight, room: Room| async move { store.save_flight(&new, &room).await.unwrap() };

    // Three flights fit; the fourth does not, and nothing of it is kept.
    for n in 1..=3 {
        let saved = try_save(flight(&dario, n), three_flights).await;
        assert!(matches!(saved, Saved::Created(_)), "{saved:?}");
    }
    let kept = Kept {
        flights: 3,
        file_bytes: 3 * 64_000 + 1 + 2 + 3,
    };
    let fourth = flight(&dario, 4);
    assert_eq!(
        try_save(fourth.clone(), three_flights).await,
        Saved::NoRoom(kept)
    );
    assert_eq!(store.flight(&fourth.id).await.unwrap(), None);
    assert_eq!(store.flight_file(&fourth.id).await.unwrap(), None);
    assert_eq!(list(store, &dario, Listing::All).await.len(), 3);
    // Being refused is no change to the account's flights.
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(changes.flights.len(), 3);
    assert!(changes.deleted.is_empty());

    // A flight the account already has is handed back, full or not.
    let again = try_save(flight(&dario, 2), three_flights).await;
    assert!(matches!(again, Saved::AlreadyThere(_)), "{again:?}");
    // Another account has room of its own.
    let annas = try_save(flight(&anna, 4), three_flights).await;
    assert!(matches!(annas, Saved::Created(_)), "{annas:?}");

    // Room is counted in bytes too, to the byte: the files that are there
    // and the one that asks.
    let exactly = Room {
        flights: u64::MAX,
        file_bytes: kept.file_bytes + 64_004,
    };
    let one_short = Room {
        file_bytes: exactly.file_bytes - 1,
        ..exactly
    };
    assert_eq!(
        try_save(fourth.clone(), one_short).await,
        Saved::NoRoom(kept)
    );
    let saved = try_save(fourth.clone(), exactly).await;
    assert!(matches!(saved, Saved::Created(_)), "{saved:?}");

    // Deleting a flight makes room.
    let fifth = flight(&dario, 5);
    let four_flights = Room {
        flights: 4,
        file_bytes: u64::MAX,
    };
    assert!(matches!(
        try_save(fifth.clone(), four_flights).await,
        Saved::NoRoom(_)
    ));
    assert!(
        store
            .delete_flight(&dario.id, &fourth.id, at(12, 0))
            .await
            .unwrap()
    );
    let saved = try_save(fifth, four_flights).await;
    assert!(matches!(saved, Saved::Created(_)), "{saved:?}");
}

pub(crate) async fn uploads_racing_for_the_last_place_fill_it_once(store: &impl Store) {
    let dario = create(store, "dario").await;
    let one_flight = Room {
        flights: 1,
        file_bytes: u64::MAX,
    };
    let flights: Vec<NewFlight> = (1..=4)
        .map(|n| new_flight(&dario, n, at(10, n.into())))
        .collect();

    let (a, b, c, d) = tokio::join!(
        store.save_flight(&flights[0], &one_flight),
        store.save_flight(&flights[1], &one_flight),
        store.save_flight(&flights[2], &one_flight),
        store.save_flight(&flights[3], &one_flight),
    );

    let results = [a.unwrap(), b.unwrap(), c.unwrap(), d.unwrap()];
    let created = results
        .iter()
        .filter(|saved| matches!(saved, Saved::Created(_)))
        .count();
    let refused = results
        .iter()
        .filter(|saved| matches!(saved, Saved::NoRoom(_)))
        .count();
    assert_eq!((created, refused), (1, 3), "{results:?}");
    assert_eq!(list(store, &dario, Listing::All).await.len(), 1);
}

pub(crate) async fn flights_are_listed_newest_first_page_by_page(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    // Flights 2 and 3 start in the same second; flight 5 is the oldest
    // although it is stored last.
    let starts = [(1, 9), (2, 11), (3, 11), (4, 13), (5, 7)];
    for (n, hour) in starts {
        save(store, &new_flight(&dario, n, at(hour, 0))).await;
    }
    save(store, &new_flight(&anna, 1, at(12, 0))).await;

    let order = ["dario4", "dario3", "dario2", "dario1", "dario5"];
    assert_eq!(list(store, &dario, Listing::All).await, order);
    assert_eq!(list(store, &anna, Listing::All).await, ["anna1"]);

    // Two at a time, each page continuing after the last flight of the
    // page before.
    let mut seen = Vec::new();
    let mut after = None;
    for expected in [2, 2, 1, 0] {
        let page = store
            .flights_of(&dario.id, Listing::All, after.as_ref(), 2)
            .await
            .unwrap();
        assert_eq!(page.len(), expected);
        seen.extend(short(&page));
        if let Some(last) = page.last() {
            after = Some(last.key());
        }
    }
    assert_eq!(seen, order);
    let none = store
        .flights_of(&dario.id, Listing::All, None, 0)
        .await
        .unwrap();
    assert!(none.is_empty());

    // The public list holds the public flights and nothing else.
    assert!(list(store, &dario, Listing::Public).await.is_empty());
    for (n, visibility) in [
        (1, Visibility::Public),
        (2, Visibility::Unlisted),
        (4, Visibility::Public),
    ] {
        store
            .set_visibility(&dario.id, &id(&format!("dario{n}")), visibility, at(21, 0))
            .await
            .unwrap();
    }
    assert_eq!(
        list(store, &dario, Listing::Public).await,
        ["dario4", "dario1"]
    );
    let first = store
        .flights_of(&dario.id, Listing::Public, None, 1)
        .await
        .unwrap();
    let second = store
        .flights_of(&dario.id, Listing::Public, Some(&first[0].key()), 1)
        .await
        .unwrap();
    assert_eq!(short(&second), ["dario1"]);
}

pub(crate) async fn visibility_changes_for_the_owner_only(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let new = new_flight(&dario, 1, at(10, 30));
    let private = save(store, &new).await;

    // Another account cannot change it, and learns nothing.
    let foreign = store
        .set_visibility(&anna.id, &new.id, Visibility::Public, at(21, 0))
        .await
        .unwrap();
    assert_eq!(foreign, None);
    assert_eq!(store.flight(&new.id).await.unwrap(), Some(private.clone()));
    let unknown = store
        .set_visibility(&dario.id, &id("unknown"), Visibility::Public, at(21, 0))
        .await
        .unwrap();
    assert_eq!(unknown, None);

    let unlisted = store
        .set_visibility(&dario.id, &new.id, Visibility::Unlisted, at(21, 0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        unlisted,
        Flight {
            visibility: Visibility::Unlisted,
            change: micros(at(21, 0)),
            ..private
        }
    );
    assert_eq!(store.flight(&new.id).await.unwrap(), Some(unlisted.clone()));

    // Setting what is set is not a change.
    let same = store
        .set_visibility(&dario.id, &new.id, Visibility::Unlisted, at(22, 0))
        .await
        .unwrap();
    assert_eq!(same, Some(unlisted));
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(changes.latest, micros(at(21, 0)));
}

pub(crate) async fn only_the_owner_deletes_a_flight(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let new = new_flight(&dario, 1, at(10, 30));
    let other = new_flight(&dario, 2, at(11, 30));
    save(store, &new).await;
    save(store, &other).await;

    assert!(
        !store
            .delete_flight(&anna.id, &new.id, at(21, 0))
            .await
            .unwrap()
    );
    assert!(
        !store
            .delete_flight(&dario.id, &id("unknown"), at(21, 0))
            .await
            .unwrap()
    );
    assert!(store.flight(&new.id).await.unwrap().is_some());
    let untouched = store.flight_changes(&anna.id, 0, 10).await.unwrap();
    assert!(untouched.deleted.is_empty());

    assert!(
        store
            .delete_flight(&dario.id, &new.id, at(21, 0))
            .await
            .unwrap()
    );

    assert_eq!(store.flight(&new.id).await.unwrap(), None);
    assert_eq!(store.flight_file(&new.id).await.unwrap(), None);
    assert_eq!(list(store, &dario, Listing::All).await, ["dario2"]);
    // There is nothing left to delete.
    assert!(
        !store
            .delete_flight(&dario.id, &new.id, at(22, 0))
            .await
            .unwrap()
    );
}

pub(crate) async fn changes_are_reported_in_order_with_deletions(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let flight = |n: u8, uploaded: Timestamp| NewFlight {
        uploaded_at: uploaded,
        ..new_flight(&dario, n, at(10, 0))
    };
    // 12:00 flight 1 arrives, 12:10 flight 2, 12:20 flight 3; 12:30 flight 1
    // becomes public; 12:40 flight 2 is deleted.
    for (n, minute) in [(1, 0), (2, 10), (3, 20)] {
        save(store, &flight(n, at(12, minute))).await;
    }
    store
        .set_visibility(&dario.id, &id("dario1"), Visibility::Public, at(12, 30))
        .await
        .unwrap();
    store
        .delete_flight(&dario.id, &id("dario2"), at(12, 40))
        .await
        .unwrap();
    save(store, &new_flight(&anna, 1, at(10, 0))).await;

    // Everything: each flight once, as it is now, oldest change first.
    let all = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(short(&all.flights), ["dario3", "dario1"]);
    assert_eq!(all.flights[1].visibility, Visibility::Public);
    assert_eq!(all.deleted.len(), 1);
    assert_eq!(all.deleted[0].id, id("dario2"));
    assert_eq!(all.deleted[0].identity, flight(2, at(12, 10)).identity);
    assert!(!all.more);
    assert_eq!(all.latest, micros(at(12, 40)));
    assert_eq!(all.last(), Some(all.latest));

    // A device that last asked at 12:25 hears what happened since.
    let since = store
        .flight_changes(&dario.id, micros(at(12, 25)), 10)
        .await
        .unwrap();
    assert_eq!(short(&since.flights), ["dario1"]);
    assert_eq!(since.deleted.len(), 1);
    // Nothing happened after the last change.
    let nothing = store
        .flight_changes(&dario.id, all.latest, 10)
        .await
        .unwrap();
    assert!(nothing.flights.is_empty() && nothing.deleted.is_empty());
    assert!(!nothing.more);
    assert_eq!(nothing.last(), None);
    assert_eq!(nothing.latest, all.latest);

    // One change at a time: flight 3, flight 1, the deletion.
    let mut since = 0;
    let mut steps = Vec::new();
    loop {
        let page = store.flight_changes(&dario.id, since, 1).await.unwrap();
        let Some(last) = page.last() else {
            assert!(!page.more);
            break;
        };
        assert_eq!(page.flights.len() + page.deleted.len(), 1);
        steps.extend(short(&page.flights));
        steps.extend(page.deleted.iter().map(|_| "deleted".to_owned()));
        since = last;
        assert_eq!(page.more, since != all.latest);
    }
    assert_eq!(steps, ["dario3", "dario1", "deleted"]);
    let none = store.flight_changes(&dario.id, 0, 0).await.unwrap();
    assert!(none.flights.is_empty() && none.deleted.is_empty() && none.more);

    // An account that never had a flight has no changes.
    let empty = store.flight_changes(&id("nobody"), 0, 10).await.unwrap();
    assert!(empty.flights.is_empty() && empty.deleted.is_empty());
    assert_eq!(empty.latest, 0);
}

pub(crate) async fn change_numbers_follow_the_clock_and_never_go_back(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let flight = |account: &Account, n: u8, uploaded: Timestamp| NewFlight {
        uploaded_at: uploaded,
        ..new_flight(account, n, at(10, 0))
    };

    let first = save(store, &flight(&dario, 1, at(12, 0))).await;
    assert_eq!(first.change, micros(at(12, 0)));
    // Two more changes in the same instant, and one by a clock that went
    // back: each still gets a larger number.
    let second = save(store, &flight(&dario, 2, at(12, 0))).await;
    let third = save(store, &flight(&dario, 3, at(12, 0))).await;
    let fourth = save(store, &flight(&dario, 4, at(11, 0))).await;
    assert_eq!(second.change, first.change + 1);
    assert_eq!(third.change, first.change + 2);
    assert_eq!(fourth.change, first.change + 3);
    // When the clock has moved on, the number is the time again.
    let fifth = save(store, &flight(&dario, 5, at(13, 0))).await;
    assert_eq!(fifth.change, micros(at(13, 0)));

    // Each account counts for itself.
    let annas = save(store, &flight(&anna, 1, at(9, 0))).await;
    assert_eq!(annas.change, micros(at(9, 0)));
}

pub(crate) async fn a_deleted_flight_can_be_uploaded_again(store: &impl Store) {
    let dario = create(store, "dario").await;
    let new = NewFlight {
        uploaded_at: at(12, 0),
        ..new_flight(&dario, 1, at(10, 0))
    };
    save(store, &new).await;
    store
        .set_visibility(&dario.id, &new.id, Visibility::Public, at(12, 10))
        .await
        .unwrap();
    store
        .delete_flight(&dario.id, &new.id, at(12, 20))
        .await
        .unwrap();

    let again = NewFlight {
        uploaded_at: at(12, 30),
        file: b"the second upload".to_vec(),
        ..new.clone()
    };
    let flight = save(store, &again).await;

    // It is a new flight: private again, with the file of this upload.
    assert_eq!(flight, stored(&again, &dario, micros(at(12, 30))));
    assert_eq!(store.flight_file(&new.id).await.unwrap(), Some(again.file));
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(short(&changes.flights), ["dario1"]);
    assert!(changes.deleted.is_empty(), "{:?}", changes.deleted);

    // Deleted a second time, it is listed as deleted once.
    store
        .delete_flight(&dario.id, &new.id, at(12, 40))
        .await
        .unwrap();
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert!(changes.flights.is_empty());
    assert_eq!(changes.deleted.len(), 1);
    assert_eq!(changes.deleted[0].change, micros(at(12, 40)));
}

pub(crate) async fn derived_values_can_be_worked_out_again(store: &impl Store) {
    let dario = create(store, "dario").await;
    let old = NewFlight {
        uploaded_at: at(12, 0),
        ..new_flight(&dario, 1, at(10, 0))
    };
    let current = NewFlight {
        uploaded_at: at(12, 10),
        derived: Derived {
            version: 2,
            ..derived(at(11, 0))
        },
        ..new_flight(&dario, 2, at(11, 0))
    };
    save(store, &old).await;
    save(store, &current).await;

    // Only the flight analysed by version 1 is behind version 2.
    let behind = |version: u32| async move {
        let outdated = store.outdated_flights(version, None, 10).await.unwrap();
        outdated
            .into_iter()
            .map(|flight| (flight.id, flight.version, flight.file))
            .collect::<Vec<_>>()
    };
    assert_eq!(behind(2).await, [(old.id.clone(), 1, old.file.clone())]);
    assert!(behind(1).await.is_empty());
    // Behind version 3 are both: the oldest analysis first.
    assert_eq!(
        behind(3).await,
        [
            (old.id.clone(), 1, old.file.clone()),
            (current.id.clone(), 2, current.file.clone())
        ]
    );
    // Page by page, each continuing after the last flight of the one
    // before, whether or not that flight was brought up to date.
    let first = store.outdated_flights(3, None, 1).await.unwrap();
    assert_eq!(first[0].id, old.id);
    let second = store
        .outdated_flights(3, Some(&first[0].key()), 1)
        .await
        .unwrap();
    assert_eq!(second[0].id, current.id);
    let third = store
        .outdated_flights(3, Some(&second[0].key()), 1)
        .await
        .unwrap();
    assert!(third.is_empty());
    // Among flights of one version, by address.
    let third_flight = NewFlight {
        uploaded_at: at(12, 20),
        ..new_flight(&dario, 0, at(9, 0))
    };
    save(store, &third_flight).await;
    let ids: Vec<String> = store
        .outdated_flights(2, None, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|flight| flight.id)
        .collect();
    assert_eq!(ids, [third_flight.id.clone(), old.id.clone()]);
    store
        .delete_flight(&dario.id, &third_flight.id, at(12, 30))
        .await
        .unwrap();

    // The new analysis finds no landing and scores the flight differently.
    let better = Derived {
        version: 2,
        landing: None,
        score: Some(Score {
            kind: ScoreKind::FaiTriangle,
            closed: true,
            distance_m: 10_001.0,
            points: 16.0,
        }),
        glider: None,
        outline: vec![[47.0, 11.0], [47.1, 11.1]],
        profile: vec![[0, 1000]],
        ..derived(at(11, 30))
    };
    assert!(
        store
            .set_derived(&old.id, &better, at(13, 0))
            .await
            .unwrap()
    );

    let flight = store.flight(&old.id).await.unwrap().unwrap();
    assert_eq!(flight.derived, better);
    assert_eq!(flight.change, micros(at(13, 0)), "it counts as changed");
    assert_eq!(flight.visibility, Visibility::Private);
    assert_eq!(store.flight_file(&old.id).await.unwrap(), Some(old.file));
    assert!(behind(2).await.is_empty());
    // The list follows the new start.
    assert_eq!(
        list(store, &dario, Listing::All).await,
        ["dario1", "dario2"]
    );

    assert!(
        !store
            .set_derived(&id("unknown"), &better, at(13, 0))
            .await
            .unwrap()
    );
}
