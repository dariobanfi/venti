//! What every store must do with pilots' flights.

use chrono::Duration;
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_store::{
    Account, Derived, Flight, Kept, Listing, NewFlight, Place, PublicFlightChanges, PublicFlights,
    PublicOrder, Room, Saved, Score, ScoreKind, Store, StoreError, Visibility, VisibilityChange,
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
        taken_down_at: None,
        published_at: None,
        public_change: None,
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
    assert_eq!(foreign, VisibilityChange::NoSuchFlight);
    assert_eq!(store.flight(&new.id).await.unwrap(), Some(private.clone()));
    let unknown = store
        .set_visibility(&dario.id, &id("unknown"), Visibility::Public, at(21, 0))
        .await
        .unwrap();
    assert_eq!(unknown, VisibilityChange::NoSuchFlight);

    let unlisted = store
        .set_visibility(&dario.id, &new.id, Visibility::Unlisted, at(21, 0))
        .await
        .unwrap()
        .flight()
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
    assert_eq!(same, VisibilityChange::Set(Box::new(unlisted)));
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

// --- Everyone's public flights -------------------------------------------

const WALLBERG: (f64, f64) = (47.6642, 11.7967);
const BRAUNECK: (f64, f64) = (47.6639, 11.5236);
const KOESSEN: (f64, f64) = (47.6536, 12.4291);

/// A flight like [`new_flight`] that took off at a place and scores
/// `points`, or is too short to score.
fn flown(
    account: &Account,
    n: u8,
    start: Timestamp,
    (lat, lon): (f64, f64),
    points: Option<f64>,
) -> NewFlight {
    let mut new = new_flight(account, n, start);
    new.derived.takeoff = Some(Place {
        time: start,
        position: GeoPoint::new(lat, lon).unwrap(),
        altitude_m: 1620,
    });
    new.derived.score = points.map(|points| Score {
        kind: ScoreKind::FreeFlight,
        closed: false,
        distance_m: points * 1000.0,
        points,
    });
    new
}

/// Stores a flight and sets who may see it.
async fn share(store: &impl Store, new: &NewFlight, visibility: Visibility) -> Flight {
    save(store, new).await;
    store
        .set_visibility(&new.account_id, &new.id, visibility, at(21, 0))
        .await
        .unwrap()
        .flight()
        .expect("the flight was just stored")
}

async fn publish(store: &impl Store, new: &NewFlight) -> Flight {
    share(store, new, Visibility::Public).await
}

async fn public(store: &impl Store, which: &PublicFlights, order: &PublicOrder) -> Vec<String> {
    short(&store.public_flights(which, order, 100).await.unwrap())
}

fn everywhere() -> PublicFlights {
    PublicFlights::default()
}

fn area(south: f64, west: f64, north: f64, east: f64) -> PublicFlights {
    PublicFlights {
        area: Some(BoundingBox::new(south, west, north, east).unwrap()),
        ..PublicFlights::default()
    }
}

const NEWEST: PublicOrder = PublicOrder::Newest(None);
const BEST: PublicOrder = PublicOrder::Best(None);

pub(crate) async fn public_flights_are_listed_newest_first_whoever_flew_them(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    assert!(public(store, &everywhere(), &NEWEST).await.is_empty());

    // Dario's second flight and Anna's first start in the same second.
    publish(store, &new_flight(&dario, 1, at(9, 0))).await;
    publish(store, &new_flight(&dario, 2, at(11, 0))).await;
    publish(store, &new_flight(&anna, 1, at(11, 0))).await;
    publish(store, &new_flight(&bernd, 1, at(7, 0))).await;
    // What is not public: a private flight, and one for those with a link.
    save(store, &new_flight(&dario, 3, at(13, 0))).await;
    share(
        store,
        &new_flight(&anna, 2, at(12, 0)),
        Visibility::Unlisted,
    )
    .await;

    let order = ["dario2", "anna1", "dario1", "bernd1"];
    assert_eq!(public(store, &everywhere(), &NEWEST).await, order);
    // Each flight names its own pilot, as the account is now.
    let all = store
        .public_flights(&everywhere(), &NEWEST, 100)
        .await
        .unwrap();
    let pilots: Vec<&str> = all.iter().map(|f| f.pilot_handle.as_str()).collect();
    assert_eq!(pilots, ["dario", "anna", "dario", "bernd"]);
    assert_eq!(all[1].pilot_name, "Pilot anna");
    assert!(all.iter().all(|f| f.visibility == Visibility::Public));

    // Page by page, each page continuing after the last flight of the page
    // before, also between two flights of the same second.
    for size in [1, 3] {
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page = store
                .public_flights(&everywhere(), &PublicOrder::Newest(after), size)
                .await
                .unwrap();
            let Some(last) = page.last() else { break };
            assert!(page.len() <= size);
            after = Some(last.key());
            seen.extend(short(&page));
        }
        assert_eq!(seen, order, "pages of {size}");
    }
    let none = store
        .public_flights(&everywhere(), &NEWEST, 0)
        .await
        .unwrap();
    assert!(none.is_empty());

    // A flight joins the list when it is made public and leaves it when it
    // is not public any more, deleted, or its account is.
    store
        .set_visibility(&dario.id, &id("dario3"), Visibility::Public, at(21, 10))
        .await
        .unwrap();
    store
        .set_visibility(&dario.id, &id("dario2"), Visibility::Unlisted, at(21, 20))
        .await
        .unwrap();
    assert!(
        store
            .delete_flight(&dario.id, &id("dario1"), at(21, 30))
            .await
            .unwrap()
    );
    assert_eq!(
        public(store, &everywhere(), &NEWEST).await,
        ["dario3", "anna1", "bernd1"]
    );
    store.delete_account(&anna.id).await.unwrap();
    assert_eq!(
        public(store, &everywhere(), &NEWEST).await,
        ["dario3", "bernd1"]
    );
}

pub(crate) async fn what_is_not_public_is_in_no_list_of_public_flights(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    // One public flight among flights that are not, all from one take-off
    // on one day. Of the ones that are not public some are newer and score
    // more than the public one, some older and less, and one is its twin.
    let shown = publish(store, &flown(&dario, 1, at(10, 0), WALLBERG, Some(20.0))).await;
    let hidden = [
        (flown(&dario, 2, at(12, 0), WALLBERG, Some(90.0)), None),
        (
            flown(&dario, 3, at(8, 0), WALLBERG, Some(5.0)),
            Some(Visibility::Unlisted),
        ),
        (
            flown(&anna, 1, at(11, 0), WALLBERG, Some(300.0)),
            Some(Visibility::Unlisted),
        ),
        (flown(&anna, 2, at(9, 0), WALLBERG, Some(1.0)), None),
        (flown(&anna, 3, at(10, 0), WALLBERG, Some(20.0)), None),
    ];
    let mut secret = Vec::new();
    for (new, visibility) in &hidden {
        secret.push(match visibility {
            Some(visibility) => share(store, new, *visibility).await,
            None => save(store, new).await,
        });
    }
    // A flight that was public for a while, and whose place in both
    // orders someone wrote down then.
    let once = publish(store, &flown(&anna, 4, at(10, 30), WALLBERG, Some(25.0))).await;
    store
        .set_visibility(&anna.id, &once.id, Visibility::Private, at(21, 30))
        .await
        .unwrap();
    secret.push(once);

    // Every way of asking: anywhere, in a box around the take-off, on the
    // day, both; from the top of either order, and after the place of
    // every flight there is, public or not.
    let the_day = PublicFlights {
        from: Some(at(0, 0)),
        before: Some(at(0, 0) + Duration::days(1)),
        ..PublicFlights::default()
    };
    let around = area(47.6, 11.7, 47.7, 11.9);
    let which = [
        everywhere(),
        around.clone(),
        the_day.clone(),
        PublicFlights {
            area: around.area,
            ..the_day
        },
    ];
    let last_address = "z".repeat(26);
    let mut orders = vec![
        NEWEST,
        BEST,
        PublicOrder::Newest(Some((at(23, 59), last_address.clone()))),
        PublicOrder::Best(Some((1e12, last_address))),
    ];
    for flight in secret.iter().chain([&shown]) {
        orders.push(PublicOrder::Newest(Some(flight.key())));
        orders.push(PublicOrder::Best(flight.score_key()));
        // The place just above it, from where a list would go on with it.
        let (start, points) = (flight.derived.start, flight.score_key().unwrap().0);
        let above = format!("{}z", &flight.id[..25]);
        orders.push(PublicOrder::Newest(Some((start, above.clone()))));
        orders.push(PublicOrder::Best(Some((points, above))));
    }
    let mut asked = 0;
    let mut answered = 0;
    for which in &which {
        for order in &orders {
            for limit in [1, 2, 100] {
                let found = store.public_flights(which, order, limit).await.unwrap();
                assert!(
                    found.iter().all(|flight| flight.id == shown.id),
                    "{which:?} {order:?} gave {:?}",
                    short(&found)
                );
                assert!(found.len() <= 1, "{which:?} {order:?}");
                asked += 1;
                answered += found.len();
            }
        }
    }
    assert!(asked > 300, "{asked} questions were asked");
    assert!(answered > 0, "the public flight is found at all");
}

pub(crate) async fn public_flights_are_found_by_where_they_took_off(store: &impl Store) {
    let dario = create(store, "dario").await;
    publish(store, &flown(&dario, 1, at(9, 0), WALLBERG, Some(10.0))).await;
    publish(store, &flown(&dario, 2, at(10, 0), BRAUNECK, Some(30.0))).await;
    publish(store, &flown(&dario, 3, at(11, 0), KOESSEN, Some(20.0))).await;
    // A recording that never shows the glider flying took off nowhere.
    let mut ground = new_flight(&dario, 4, at(12, 0));
    ground.derived.takeoff = None;
    publish(store, &ground).await;
    // A flight from the Wallberg that is not public.
    save(store, &flown(&dario, 5, at(13, 0), WALLBERG, Some(99.0))).await;

    let whole = ["dario4", "dario3", "dario2", "dario1"];
    assert_eq!(public(store, &everywhere(), &NEWEST).await, whole);
    // The valley of the Tegernsee, and everything east of the Isar.
    let tegernsee = area(47.6, 11.7, 47.8, 11.9);
    assert_eq!(public(store, &tegernsee, &NEWEST).await, ["dario1"]);
    let east = area(47.0, 11.6, 48.0, 13.0);
    assert_eq!(public(store, &east, &NEWEST).await, ["dario3", "dario1"]);
    assert_eq!(public(store, &east, &BEST).await, ["dario3", "dario1"]);
    // The edges of a box belong to it: boxes with the Wallberg in a
    // corner, and one that is that point and nothing else.
    let (lat, lon) = WALLBERG;
    for corner in [
        area(lat, lon, 48.0, 12.0),
        area(47.0, 11.6, lat, lon),
        area(lat, lon, lat, lon),
    ] {
        assert_eq!(public(store, &corner, &NEWEST).await, ["dario1"]);
    }
    // A hair past it, the flight is outside.
    let beside = area(lat + 1e-9, lon, 48.0, 12.0);
    assert!(public(store, &beside, &NEWEST).await.is_empty());
    // A box with nothing in it, and the whole globe, which holds every
    // flight that took off somewhere.
    let nothing = area(0.0, 0.0, 1.0, 1.0);
    assert!(public(store, &nothing, &NEWEST).await.is_empty());
    let globe = area(-90.0, -180.0, 90.0, 180.0);
    assert_eq!(
        public(store, &globe, &NEWEST).await,
        ["dario3", "dario2", "dario1"]
    );

    // Page by page inside a box.
    let first = store.public_flights(&east, &NEWEST, 1).await.unwrap();
    assert_eq!(short(&first), ["dario3"]);
    let after = PublicOrder::Newest(Some(first[0].key()));
    assert_eq!(public(store, &east, &after).await, ["dario1"]);
}

pub(crate) async fn public_flights_are_found_by_when_they_started(store: &impl Store) {
    let dario = create(store, "dario").await;
    let midnight = at(0, 0);
    let day = Duration::days(1);
    let second = Duration::seconds(1);
    // The last second of the 17th, the first and the middle of the 18th,
    // and the first second of the 19th.
    for (n, start) in [
        (1, midnight - second),
        (2, midnight),
        (3, at(12, 0)),
        (4, midnight + day),
    ] {
        publish(store, &flown(&dario, n, start, WALLBERG, Some(n.into()))).await;
    }
    save(store, &flown(&dario, 5, at(13, 0), WALLBERG, Some(50.0))).await;
    let span = |from: Option<Timestamp>, before: Option<Timestamp>| PublicFlights {
        from,
        before,
        ..PublicFlights::default()
    };

    // A day begins with its first moment and ends before the next day's.
    let the_18th = span(Some(midnight), Some(midnight + day));
    assert_eq!(
        public(store, &the_18th, &NEWEST).await,
        ["dario3", "dario2"]
    );
    assert_eq!(public(store, &the_18th, &BEST).await, ["dario3", "dario2"]);
    let the_17th = span(Some(midnight - day), Some(midnight));
    assert_eq!(public(store, &the_17th, &NEWEST).await, ["dario1"]);
    // Either end alone.
    let since = span(Some(at(12, 0)), None);
    assert_eq!(public(store, &since, &NEWEST).await, ["dario4", "dario3"]);
    let until = span(None, Some(at(12, 0)));
    assert_eq!(public(store, &until, &NEWEST).await, ["dario2", "dario1"]);
    // A span without a moment in it holds nothing.
    for empty in [
        span(Some(midnight), Some(midnight)),
        span(Some(at(12, 0)), Some(midnight)),
        span(Some(midnight + day + day), None),
    ] {
        assert!(public(store, &empty, &NEWEST).await.is_empty(), "{empty:?}");
    }
    // Together with a box, and page by page.
    let here = PublicFlights {
        area: area(47.6, 11.7, 47.8, 11.9).area,
        ..the_18th.clone()
    };
    assert_eq!(public(store, &here, &NEWEST).await, ["dario3", "dario2"]);
    let first = store.public_flights(&here, &NEWEST, 1).await.unwrap();
    let after = PublicOrder::Newest(Some(first[0].key()));
    assert_eq!(public(store, &here, &after).await, ["dario2"]);
    let elsewhere = PublicFlights {
        area: area(0.0, 0.0, 1.0, 1.0).area,
        ..the_18th
    };
    assert!(public(store, &elsewhere, &NEWEST).await.is_empty());
}

pub(crate) async fn public_flights_are_listed_best_first_by_their_points(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    // Points that do not survive rounding, two flights with the same
    // points, one with none, and one too short to score.
    publish(
        store,
        &flown(&dario, 1, at(9, 0), WALLBERG, Some(0.1 + 0.2)),
    )
    .await;
    publish(store, &flown(&dario, 2, at(10, 0), KOESSEN, Some(104.37))).await;
    publish(store, &flown(&dario, 3, at(11, 0), WALLBERG, Some(51.5))).await;
    publish(store, &flown(&anna, 1, at(12, 0), WALLBERG, Some(51.5))).await;
    publish(store, &flown(&anna, 2, at(13, 0), WALLBERG, Some(0.0))).await;
    publish(store, &flown(&anna, 3, at(14, 0), WALLBERG, None)).await;
    // The best flight of all is not public.
    save(store, &flown(&anna, 4, at(15, 0), WALLBERG, Some(400.0))).await;

    // Among flights with the same points, by address, as among flights of
    // the same second.
    let order = ["dario2", "dario3", "anna1", "dario1", "anna2"];
    assert_eq!(public(store, &everywhere(), &BEST).await, order);
    // A flight too short to score is in the list of all, and not here.
    assert_eq!(public(store, &everywhere(), &NEWEST).await.len(), 6);

    for size in [1, 2] {
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page = store
                .public_flights(&everywhere(), &PublicOrder::Best(after), size)
                .await
                .unwrap();
            let Some(last) = page.last() else { break };
            after = last.score_key();
            assert!(after.is_some());
            seen.extend(short(&page));
        }
        assert_eq!(seen, order, "pages of {size}");
    }

    // The best of a part of the map, of a span of time, and of both.
    let tegernsee = area(47.6, 11.7, 47.8, 11.9);
    assert_eq!(
        public(store, &tegernsee, &BEST).await,
        ["dario3", "anna1", "dario1", "anna2"]
    );
    let morning = PublicFlights {
        from: Some(at(9, 0)),
        before: Some(at(12, 0)),
        ..PublicFlights::default()
    };
    assert_eq!(
        public(store, &morning, &BEST).await,
        ["dario2", "dario3", "dario1"]
    );
    let both = PublicFlights {
        area: tegernsee.area,
        ..morning
    };
    assert_eq!(public(store, &both, &BEST).await, ["dario3", "dario1"]);
    let first = store.public_flights(&both, &BEST, 1).await.unwrap();
    let after = PublicOrder::Best(first[0].score_key());
    assert_eq!(public(store, &both, &after).await, ["dario1"]);
}

/// A ranking in short: each pilot's handle and points, the flights that
/// count and how many scored.
async fn ranked(
    store: &impl Store,
    which: &PublicFlights,
    counted: usize,
    limit: usize,
) -> Vec<(String, f64, Vec<String>, u32)> {
    store
        .ranking(which, counted, limit)
        .await
        .unwrap()
        .into_iter()
        .map(|pilot| {
            (
                pilot.pilot_handle.clone(),
                pilot.points,
                short(&pilot.flights),
                pilot.scored,
            )
        })
        .collect()
}

fn place(
    handle: &str,
    points: f64,
    flights: &[&str],
    scored: u32,
) -> (String, f64, Vec<String>, u32) {
    (
        handle.to_owned(),
        points,
        flights.iter().map(|&flight| flight.to_owned()).collect(),
        scored,
    )
}

pub(crate) async fn pilots_are_ranked_by_the_sum_of_their_best_public_flights(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    assert!(ranked(store, &everywhere(), 3, 100).await.is_empty());

    // Dario flew four flights that score, a minute apart from 9:01.
    for (n, points) in [(1, 30.0), (2, 50.0), (3, 10.0), (4, 40.0)] {
        let start = at(9, u32::from(n));
        publish(store, &flown(&dario, n, start, WALLBERG, Some(points))).await;
    }
    // Anna flew one great flight, and two that are not public: those count
    // for nobody.
    publish(store, &flown(&anna, 1, at(10, 0), KOESSEN, Some(120.0))).await;
    save(store, &flown(&anna, 2, at(10, 30), KOESSEN, Some(500.0))).await;
    share(
        store,
        &flown(&anna, 3, at(11, 0), KOESSEN, Some(400.0)),
        Visibility::Unlisted,
    )
    .await;
    // Bernd flew two, from two mountains.
    publish(store, &flown(&bernd, 1, at(8, 0), WALLBERG, Some(90.0))).await;
    publish(store, &flown(&bernd, 2, at(8, 30), BRAUNECK, Some(60.0))).await;
    // Carla's flight is too short to score: she is not ranked.
    publish(store, &flown(&carla, 1, at(12, 0), WALLBERG, None)).await;

    // The best three count. Anna and Dario have the same points: the
    // handle decides.
    let best_three = [
        place("bernd", 150.0, &["bernd1", "bernd2"], 2),
        place("anna", 120.0, &["anna1"], 1),
        place("dario", 120.0, &["dario2", "dario4", "dario1"], 4),
    ];
    assert_eq!(ranked(store, &everywhere(), 3, 100).await, best_three);
    // Each pilot is named as the account is now.
    let all = store.ranking(&everywhere(), 3, 100).await.unwrap();
    assert_eq!(all[1].pilot_name, "Pilot anna");
    assert!(
        all.iter()
            .flat_map(|pilot| &pilot.flights)
            .all(|flight| flight.visibility == Visibility::Public)
    );
    // From the top, as many as asked for.
    assert_eq!(ranked(store, &everywhere(), 3, 2).await, best_three[..2]);
    assert!(ranked(store, &everywhere(), 3, 0).await.is_empty());
    // With only the best flight counting the order is another one; how
    // many scored stays what it is.
    assert_eq!(
        ranked(store, &everywhere(), 1, 100).await,
        [
            place("anna", 120.0, &["anna1"], 1),
            place("bernd", 90.0, &["bernd1"], 2),
            place("dario", 50.0, &["dario2"], 4),
        ]
    );
    assert!(ranked(store, &everywhere(), 0, 100).await.is_empty());

    // The pilots of a part of the map, of a span of time, and of both.
    let tegernsee = area(47.6, 11.7, 47.8, 11.9);
    assert_eq!(
        ranked(store, &tegernsee, 3, 100).await,
        [
            place("dario", 120.0, &["dario2", "dario4", "dario1"], 4),
            place("bernd", 90.0, &["bernd1"], 1),
        ]
    );
    let two_minutes = PublicFlights {
        from: Some(at(9, 2)),
        before: Some(at(9, 4)),
        ..PublicFlights::default()
    };
    assert_eq!(
        ranked(store, &two_minutes, 3, 100).await,
        [place("dario", 60.0, &["dario2", "dario3"], 2)]
    );
    let elsewhere = PublicFlights {
        area: area(0.0, 0.0, 1.0, 1.0).area,
        ..two_minutes
    };
    assert!(ranked(store, &elsewhere, 3, 100).await.is_empty());

    // A flight counts while it is public, and no longer.
    store
        .set_visibility(&dario.id, &id("dario2"), Visibility::Private, at(21, 10))
        .await
        .unwrap();
    assert_eq!(
        ranked(store, &everywhere(), 3, 100).await,
        [
            place("bernd", 150.0, &["bernd1", "bernd2"], 2),
            place("anna", 120.0, &["anna1"], 1),
            place("dario", 80.0, &["dario4", "dario1", "dario3"], 3),
        ]
    );
}

// --- Public flights as data ------------------------------------------------

/// Sets who may see a stored flight at `now`.
async fn set(
    store: &impl Store,
    new: &NewFlight,
    visibility: Visibility,
    now: Timestamp,
) -> Flight {
    store
        .set_visibility(&new.account_id, &new.id, visibility, now)
        .await
        .unwrap()
        .flight()
        .expect("the flight is stored")
}

/// What changed for the public since a number: the flights and the
/// withdrawn ones, each by its short address, in the order of the changes.
async fn told(
    store: &impl Store,
    account: Option<&Account>,
    since: i64,
) -> (Vec<String>, Vec<String>, PublicFlightChanges) {
    let changes = store
        .public_flight_changes(account.map(|account| account.id.as_str()), since, 100)
        .await
        .unwrap();
    let withdrawn = changes
        .withdrawn
        .iter()
        .map(|withdrawn| withdrawn.id.trim_start_matches('0').to_owned())
        .collect();
    (short(&changes.flights), withdrawn, changes)
}

pub(crate) async fn a_flight_has_the_moment_it_became_public(store: &impl Store) {
    let dario = create(store, "dario").await;
    let flight = new_flight(&dario, 1, at(10, 0));
    let saved = save(store, &flight).await;
    assert_eq!((saved.published_at, saved.public_change), (None, None));

    // Shared by link, it is not public and has no such moment.
    let unlisted = set(store, &flight, Visibility::Unlisted, at(20, 30)).await;
    assert_eq!(
        (unlisted.published_at, unlisted.public_change),
        (None, None)
    );

    let public = set(store, &flight, Visibility::Public, at(21, 0)).await;
    assert_eq!(public.published_at, Some(at(21, 0)));
    assert_eq!(public.public_change, Some(micros(at(21, 0))));
    assert_eq!(
        store.flight(&flight.id).await.unwrap(),
        Some(public.clone())
    );
    // Asking for what it is already changes nothing.
    assert_eq!(
        set(store, &flight, Visibility::Public, at(21, 5)).await,
        public
    );

    // Withdrawn, the moment is gone; public again, it is the second time's.
    let private = set(store, &flight, Visibility::Private, at(21, 10)).await;
    assert_eq!((private.published_at, private.public_change), (None, None));
    let again = set(store, &flight, Visibility::Public, at(21, 20)).await;
    assert_eq!(again.published_at, Some(at(21, 20)));
    assert_eq!(again.public_change, Some(micros(at(21, 20))));

    // The public count is one for all accounts and never goes back: a
    // second pilot who publishes at an earlier time by the clock still
    // gets a larger number.
    let anna = create(store, "anna").await;
    let annas = new_flight(&anna, 1, at(9, 0));
    save(store, &annas).await;
    let annas = set(store, &annas, Visibility::Public, at(21, 15)).await;
    assert_eq!(annas.public_change, Some(micros(at(21, 20)) + 1));

    // Taken down, a flight is public no more.
    let down = store
        .take_down_flight(&flight.id, at(22, 0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((down.published_at, down.public_change), (None, None));
    assert_eq!(down.visibility, Visibility::Private);
}

pub(crate) async fn public_flights_are_found_by_their_pilot(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    publish(store, &new_flight(&dario, 1, at(10, 0))).await;
    publish(store, &new_flight(&dario, 2, at(12, 0))).await;
    publish(store, &new_flight(&anna, 1, at(11, 0))).await;
    share(
        store,
        &new_flight(&anna, 2, at(13, 0)),
        Visibility::Unlisted,
    )
    .await;
    save(store, &new_flight(&dario, 3, at(14, 0))).await;

    let of = |account: &Account| PublicFlights {
        pilot: Some(account.id.clone()),
        ..PublicFlights::default()
    };
    assert_eq!(
        public(store, &everywhere(), &NEWEST).await,
        ["dario2", "anna1", "dario1"]
    );
    assert_eq!(
        public(store, &of(&dario), &NEWEST).await,
        ["dario2", "dario1"]
    );
    assert_eq!(public(store, &of(&anna), &NEWEST).await, ["anna1"]);
    // By points too, and together with a time and a place.
    assert_eq!(
        public(store, &of(&dario), &PublicOrder::Best(None))
            .await
            .len(),
        2
    );
    let late = PublicFlights {
        from: Some(at(11, 30)),
        ..of(&dario)
    };
    assert_eq!(public(store, &late, &NEWEST).await, ["dario2"]);
    let here = PublicFlights {
        area: area(47.6, 11.7, 47.7, 11.9).area,
        ..of(&anna)
    };
    assert_eq!(public(store, &here, &NEWEST).await, ["anna1"]);
    let elsewhere = PublicFlights {
        area: area(0.0, 0.0, 1.0, 1.0).area,
        ..of(&anna)
    };
    assert!(public(store, &elsewhere, &NEWEST).await.is_empty());
    // An account nobody has flies nothing.
    let nobody = PublicFlights {
        pilot: Some(id("nobody")),
        ..PublicFlights::default()
    };
    assert!(public(store, &nobody, &NEWEST).await.is_empty());
    // The ranking counts the same flights.
    let ranked = store.ranking(&of(&anna), 3, 10).await.unwrap();
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].pilot_handle, "anna");
}

pub(crate) async fn what_changed_for_the_public_is_told_with_withdrawals(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let nothing = told(store, None, 0).await;
    assert_eq!((nothing.0.len(), nothing.1.len()), (0, 0));
    assert_eq!((nothing.2.latest, nothing.2.more), (0, false));

    let d1 = new_flight(&dario, 1, at(10, 0));
    let d2 = new_flight(&dario, 2, at(11, 0));
    let d3 = new_flight(&dario, 3, at(12, 0));
    let a1 = new_flight(&anna, 1, at(10, 30));
    let a2 = new_flight(&anna, 2, at(11, 30));
    for flight in [&d1, &d2, &d3, &a1, &a2] {
        save(store, flight).await;
    }
    // Stored flights are private: nothing for the public yet.
    assert_eq!(told(store, None, 0).await.2.latest, 0);

    set(store, &d1, Visibility::Public, at(21, 0)).await;
    set(store, &a1, Visibility::Public, at(21, 1)).await;
    set(store, &d2, Visibility::Public, at(21, 2)).await;
    // Shared by link only: never public, in neither list.
    set(store, &a2, Visibility::Unlisted, at(21, 3)).await;
    let (flights, withdrawn, changes) = told(store, None, 0).await;
    assert_eq!(flights, ["dario1", "anna1", "dario2"]);
    assert!(withdrawn.is_empty());
    assert_eq!(changes.latest, micros(at(21, 2)));
    assert_eq!(changes.last(), Some(micros(at(21, 2))));
    assert!(
        changes
            .flights
            .iter()
            .all(|flight| flight.visibility == Visibility::Public)
    );
    let seen = changes.latest;
    let nothing_new = told(store, None, seen).await;
    assert!(nothing_new.0.is_empty() && nothing_new.1.is_empty());
    assert_eq!(nothing_new.2.last(), None);

    // Each way a flight stops being public withdraws it: made private,
    // made unlisted, taken down, deleted.
    set(store, &d1, Visibility::Private, at(22, 0)).await;
    set(store, &a1, Visibility::Unlisted, at(22, 1)).await;
    set(store, &d3, Visibility::Public, at(22, 2)).await;
    store.take_down_flight(&d2.id, at(22, 3)).await.unwrap();
    assert!(
        store
            .delete_flight(&dario.id, &d3.id, at(22, 4))
            .await
            .unwrap()
    );
    // What was never public is withdrawn from nothing.
    set(store, &a2, Visibility::Private, at(22, 5)).await;
    assert!(
        store
            .delete_flight(&anna.id, &a2.id, at(22, 6))
            .await
            .unwrap()
    );

    let (flights, withdrawn, changes) = told(store, None, seen).await;
    assert!(flights.is_empty(), "{flights:?}");
    assert_eq!(withdrawn, ["dario1", "anna1", "dario2", "dario3"]);
    let times: Vec<_> = changes.withdrawn.iter().map(|w| w.withdrawn_at).collect();
    assert_eq!(
        times,
        [at(22, 0), at(22, 1), at(22, 3), at(22, 4)].map(Some)
    );
    assert_eq!(changes.latest, micros(at(22, 4)));
    // From the beginning, a flight is in one list and never in both, and
    // the lists hold what a copy needs to end up with nothing.
    let (flights, withdrawn, _) = told(store, None, 0).await;
    assert!(flights.is_empty(), "{flights:?}");
    assert_eq!(withdrawn, ["dario1", "anna1", "dario2", "dario3"]);
    assert!(public(store, &everywhere(), &NEWEST).await.is_empty());

    // Public again, a flight is a flight again and withdrawn no more; the
    // one that was deleted and uploaded again stays withdrawn while it is
    // private.
    let seen = changes.latest;
    set(store, &a1, Visibility::Public, at(23, 0)).await;
    save(store, &d3).await;
    let (flights, withdrawn, _) = told(store, None, seen).await;
    assert_eq!(flights, ["anna1"]);
    assert!(withdrawn.is_empty());
    let (flights, withdrawn, _) = told(store, None, 0).await;
    assert_eq!(flights, ["anna1"]);
    assert_eq!(withdrawn, ["dario1", "dario2", "dario3"]);

    // Worked out again, a public flight has changed for the public; a
    // private one has not.
    let seen = micros(at(23, 0));
    assert!(
        store
            .set_derived(&a1.id, &derived(at(10, 31)), at(23, 10))
            .await
            .unwrap()
    );
    assert!(
        store
            .set_derived(&d1.id, &derived(at(10, 1)), at(23, 11))
            .await
            .unwrap()
    );
    let (flights, withdrawn, changes) = told(store, None, seen).await;
    assert_eq!(flights, ["anna1"]);
    assert!(withdrawn.is_empty());
    assert_eq!(changes.flights[0].derived.start, at(10, 31));
    assert_eq!(changes.flights[0].public_change, Some(micros(at(23, 10))));
    // It is public since the same moment as before.
    assert_eq!(changes.flights[0].published_at, Some(at(23, 0)));
    assert_eq!(changes.latest, micros(at(23, 10)));
}

pub(crate) async fn the_public_changes_are_told_page_by_page_and_of_a_pilot(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let d1 = new_flight(&dario, 1, at(10, 0));
    let d2 = new_flight(&dario, 2, at(11, 0));
    let a1 = new_flight(&anna, 1, at(10, 30));
    let a2 = new_flight(&anna, 2, at(11, 30));
    for flight in [&d1, &d2, &a1, &a2] {
        save(store, flight).await;
    }
    set(store, &d1, Visibility::Public, at(21, 0)).await;
    set(store, &a1, Visibility::Public, at(21, 1)).await;
    set(store, &d2, Visibility::Public, at(21, 2)).await;
    set(store, &a2, Visibility::Public, at(21, 3)).await;
    set(store, &d1, Visibility::Private, at(21, 4)).await;
    set(store, &a1, Visibility::Unlisted, at(21, 5)).await;

    // The changes in order: d2, a2 published, d1 and a1 withdrawn.
    let mut since = 0;
    let mut pages = Vec::new();
    loop {
        let page = store.public_flight_changes(None, since, 1).await.unwrap();
        let one: Vec<String> = short(&page.flights)
            .into_iter()
            .chain(
                page.withdrawn
                    .iter()
                    .map(|w| format!("-{}", w.id.trim_start_matches('0'))),
            )
            .collect();
        assert_eq!(one.len(), 1, "{one:?}");
        assert_eq!(page.latest, micros(at(21, 5)));
        since = page.last().unwrap();
        pages.extend(one);
        if !page.more {
            break;
        }
    }
    assert_eq!(pages, ["dario2", "anna2", "-dario1", "-anna1"]);
    let page = store.public_flight_changes(None, 0, 3).await.unwrap();
    assert_eq!(short(&page.flights), ["dario2", "anna2"]);
    assert_eq!(page.withdrawn.len(), 1);
    assert!(page.more);
    let none = store.public_flight_changes(None, 0, 0).await.unwrap();
    assert!(none.flights.is_empty() && none.withdrawn.is_empty() && none.more);

    // Of one pilot: that pilot's flights and withdrawals, by the same
    // numbers.
    let (flights, withdrawn, changes) = told(store, Some(&dario), 0).await;
    assert_eq!(
        (flights, withdrawn),
        (vec!["dario2".to_owned()], vec!["dario1".to_owned()])
    );
    assert_eq!(changes.latest, micros(at(21, 5)));
    let (flights, withdrawn, _) = told(store, Some(&anna), micros(at(21, 3))).await;
    assert!(flights.is_empty());
    assert_eq!(withdrawn, ["anna1"]);
    let nobody = store
        .public_flight_changes(Some(&id("nobody")), 0, 100)
        .await
        .unwrap();
    assert!(nobody.flights.is_empty() && nobody.withdrawn.is_empty());
}

pub(crate) async fn suspending_and_deleting_an_account_withdraws_flights(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let d1 = new_flight(&dario, 1, at(10, 0));
    let d2 = new_flight(&dario, 2, at(11, 0));
    let d3 = new_flight(&dario, 3, at(12, 0));
    let a1 = new_flight(&anna, 1, at(10, 30));
    let a2 = new_flight(&anna, 2, at(11, 30));
    let a3 = new_flight(&anna, 3, at(12, 30));
    for flight in [&d1, &d2, &d3, &a1, &a2, &a3] {
        save(store, flight).await;
    }
    set(store, &d2, Visibility::Public, at(21, 0)).await;
    set(store, &d1, Visibility::Public, at(21, 1)).await;
    set(store, &a1, Visibility::Public, at(21, 2)).await;
    set(store, &a2, Visibility::Public, at(21, 3)).await;
    set(store, &a3, Visibility::Public, at(21, 4)).await;
    set(store, &a3, Visibility::Private, at(21, 5)).await;
    let seen = micros(at(21, 5));

    // Suspended, an account's public flights become unlisted: each is
    // withdrawn, in the order of the addresses.
    store
        .suspend_publishing(&dario.id, at(22, 0))
        .await
        .unwrap()
        .unwrap();
    let (flights, withdrawn, changes) = told(store, None, seen).await;
    assert!(flights.is_empty());
    assert_eq!(withdrawn, ["dario1", "dario2"]);
    assert_eq!(changes.withdrawn[0].change, micros(at(22, 0)));
    assert_eq!(changes.withdrawn[1].change, micros(at(22, 0)) + 1);
    assert_eq!(changes.withdrawn[0].withdrawn_at, Some(at(22, 0)));
    let unlisted = store.flight(&d1.id).await.unwrap().unwrap();
    assert_eq!(unlisted.visibility, Visibility::Unlisted);
    assert_eq!(
        (unlisted.published_at, unlisted.public_change),
        (None, None)
    );
    let seen = changes.latest;
    assert_eq!(seen, micros(at(22, 0)) + 1);

    // Deleted, an account takes its flights with it: the public ones are
    // withdrawn, and nothing says when or whose they were.
    store.delete_account(&anna.id).await.unwrap();
    let (flights, withdrawn, changes) = told(store, None, seen).await;
    assert!(flights.is_empty());
    assert_eq!(withdrawn, ["anna1", "anna2"]);
    assert_eq!(changes.withdrawn[0].change, seen + 1);
    assert_eq!(changes.withdrawn[1].change, seen + 2);
    assert_eq!(changes.withdrawn[0].withdrawn_at, None);
    assert_eq!(changes.latest, seen + 2);
    // Asked for by the account that is gone, nothing is listed: not what
    // went with it, and not what it withdrew before.
    let (flights, withdrawn, _) = told(store, Some(&anna), 0).await;
    assert!(flights.is_empty() && withdrawn.is_empty());
    // Asked for of everyone, all of it is there, the earlier withdrawal
    // too.
    let (_, withdrawn, _) = told(store, None, 0).await;
    assert_eq!(withdrawn, ["anna3", "dario1", "dario2", "anna1", "anna2"]);
    // The count goes on from there.
    store.resume_publishing(&dario.id).await.unwrap();
    let back = set(store, &d1, Visibility::Public, at(22, 30)).await;
    assert_eq!(back.public_change, Some(micros(at(22, 30))));
}
