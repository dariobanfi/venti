//! What every store must do with accounts, invites and sessions.

use chrono::Duration;
use venti_domain::Timestamp;
use venti_store::{
    Account, AccountCreation, Listing, NewAccount, NewInvite, SESSION_TOUCH_MINUTES, SecretHash,
    Session, Store,
};

use super::at;
use super::flights::new_flight;

// --- Helpers -------------------------------------------------------------

/// An identifier of the server's form: 26 characters that end in `name`.
pub(crate) fn id(name: &str) -> String {
    format!("{name:0>26}")
}

pub(crate) fn account(handle: &str) -> Account {
    Account {
        id: id(handle),
        handle: handle.to_owned(),
        display_name: format!("Pilot {handle}"),
        created_at: at(9, 0),
    }
}

fn new_account(handle: &str) -> NewAccount {
    NewAccount {
        account: account(handle),
        password_hash: format!("$argon2id$hash-of-{handle}"),
        invite: None,
    }
}

/// Creates an account without an invite and returns it.
pub(crate) async fn create(store: &impl Store, handle: &str) -> Account {
    let new = new_account(handle);
    assert_eq!(
        store.create_account(&new).await.unwrap(),
        AccountCreation::Created
    );
    new.account
}

/// The hash of a secret that is `byte` repeated.
pub(crate) fn secret(byte: u8) -> SecretHash {
    SecretHash([byte; 32])
}

fn session(name: &str, account: &Account, used: Timestamp) -> Session {
    Session {
        id: id(name),
        account_id: account.id.clone(),
        device: Some(format!("Device {name}")),
        created_at: used,
        last_used_at: used,
    }
}

/// Signs a device in at `used` with a token whose hash is `token`.
async fn sign_in(store: &impl Store, name: &str, account: &Account, used: Timestamp, token: u8) {
    store
        .create_session(&session(name, account, used), &secret(token), 100)
        .await
        .unwrap();
}

async fn session_names(store: &impl Store, account: &Account) -> Vec<String> {
    let sessions = store.sessions(&account.id).await.unwrap();
    sessions
        .into_iter()
        .map(|session| session.id.trim_start_matches('0').to_owned())
        .collect()
}

/// A time before which nothing in these tests happened.
fn long_ago() -> Timestamp {
    at(0, 0) - Duration::days(365)
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn an_account_is_created_and_found_by_its_handle(store: &impl Store) {
    let new = NewAccount {
        account: Account {
            display_name: "Jürgen “Jo” Ö. <b>".to_owned(),
            ..account("jo_oe")
        },
        ..new_account("jo_oe")
    };

    assert_eq!(
        store.create_account(&new).await.unwrap(),
        AccountCreation::Created
    );

    assert_eq!(
        store.account("jo_oe").await.unwrap(),
        Some(new.account.clone())
    );
    assert_eq!(
        store.password_hash("jo_oe").await.unwrap(),
        Some((new.account.clone(), "$argon2id$hash-of-jo_oe".to_owned()))
    );
    // Handles are looked up exactly; the caller writes them in lower case.
    assert_eq!(store.account("JO_OE").await.unwrap(), None);
    assert_eq!(store.account("jo").await.unwrap(), None);
    assert_eq!(store.account("").await.unwrap(), None);
    assert_eq!(store.password_hash("nobody").await.unwrap(), None);
    // A handle is text to look up, whatever it holds.
    assert_eq!(
        store.account("x' OR '1'='1").await.unwrap(),
        None,
        "a handle is a value, never part of a query"
    );
}

pub(crate) async fn a_handle_belongs_to_one_account(store: &impl Store) {
    let first = create(store, "dario").await;

    let second = NewAccount {
        account: Account {
            id: id("other"),
            ..account("dario")
        },
        password_hash: "$argon2id$another".to_owned(),
        invite: None,
    };
    assert_eq!(
        store.create_account(&second).await.unwrap(),
        AccountCreation::HandleTaken
    );

    // The first account is as it was, password included.
    assert_eq!(
        store.password_hash("dario").await.unwrap(),
        Some((first, "$argon2id$hash-of-dario".to_owned()))
    );
}

pub(crate) async fn an_invite_creates_exactly_one_account(store: &impl Store) {
    let invite = |code: u8, expires_at: Option<Timestamp>| NewInvite {
        code: secret(code),
        created_at: at(8, 0),
        expires_at,
    };
    store.create_invite(&invite(1, None)).await.unwrap();
    // Good until 10:00; accounts here are created at 09:00.
    store
        .create_invite(&invite(2, Some(at(10, 0))))
        .await
        .unwrap();
    // No longer good at 09:00 exactly.
    store
        .create_invite(&invite(3, Some(at(9, 0))))
        .await
        .unwrap();
    let with = |handle: &str, code: u8| NewAccount {
        invite: Some(secret(code)),
        ..new_account(handle)
    };
    let created = |new: NewAccount| async move { store.create_account(&new).await.unwrap() };

    assert_eq!(created(with("anna", 1)).await, AccountCreation::Created);
    // Used up.
    assert_eq!(
        created(with("bernd", 1)).await,
        AccountCreation::InviteRefused
    );
    // Never made.
    assert_eq!(
        created(with("bernd", 99)).await,
        AccountCreation::InviteRefused
    );
    // Expired.
    assert_eq!(
        created(with("bernd", 3)).await,
        AccountCreation::InviteRefused
    );
    assert_eq!(store.account("bernd").await.unwrap(), None);

    // A taken handle does not use the invite up.
    assert_eq!(created(with("anna", 2)).await, AccountCreation::HandleTaken);
    assert_eq!(created(with("bernd", 2)).await, AccountCreation::Created);

    // Without a good invite nothing is said about the handle.
    assert_eq!(
        created(with("anna", 99)).await,
        AccountCreation::InviteRefused
    );
    // Making the same invite again does not make it good again.
    store.create_invite(&invite(1, None)).await.unwrap();
    assert_eq!(
        created(with("carla", 1)).await,
        AccountCreation::InviteRefused
    );
}

pub(crate) async fn requests_racing_for_one_invite_create_one_account(store: &impl Store) {
    let invite = NewInvite {
        code: secret(7),
        created_at: at(8, 0),
        expires_at: None,
    };
    store.create_invite(&invite).await.unwrap();
    let handles = ["anna", "bernd", "carla", "dora"];
    let racers: Vec<NewAccount> = handles
        .iter()
        .map(|handle| NewAccount {
            invite: Some(secret(7)),
            ..new_account(handle)
        })
        .collect();

    let (a, b, c, d) = tokio::join!(
        store.create_account(&racers[0]),
        store.create_account(&racers[1]),
        store.create_account(&racers[2]),
        store.create_account(&racers[3]),
    );

    let outcomes = [a.unwrap(), b.unwrap(), c.unwrap(), d.unwrap()];
    let created = outcomes
        .iter()
        .filter(|outcome| **outcome == AccountCreation::Created)
        .count();
    assert_eq!(created, 1, "{outcomes:?}");
    let mut existing = 0;
    for handle in handles {
        existing += usize::from(store.account(handle).await.unwrap().is_some());
    }
    assert_eq!(existing, 1);
}

pub(crate) async fn a_session_is_found_by_its_token_until_it_expires(store: &impl Store) {
    let dario = create(store, "dario").await;
    sign_in(store, "phone", &dario, at(10, 0), 1).await;

    let found = store
        .use_session(&secret(1), at(10, 1), long_ago())
        .await
        .unwrap();
    assert_eq!(
        found,
        Some((session("phone", &dario, at(10, 0)), dario.clone()))
    );
    // Another token finds nothing.
    let other = store
        .use_session(&secret(2), at(10, 1), long_ago())
        .await
        .unwrap();
    assert_eq!(other, None);

    // Last used at 10:00. It is good while that is not before the limit.
    let at_limit = store
        .use_session(&secret(1), at(10, 2), at(10, 0))
        .await
        .unwrap();
    assert!(at_limit.is_some());
    let expired = store
        .use_session(&secret(1), at(10, 2), at(10, 0) + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(expired, None);
    // An expired session is not revived by the attempt.
    assert_eq!(
        store.sessions(&dario.id).await.unwrap()[0].last_used_at,
        at(10, 0)
    );
}

pub(crate) async fn using_a_session_is_recorded_every_few_minutes(store: &impl Store) {
    let dario = create(store, "dario").await;
    sign_in(store, "phone", &dario, at(10, 0), 1).await;
    let last_used = |now: Timestamp| async move {
        let found = store
            .use_session(&secret(1), now, long_ago())
            .await
            .unwrap();
        let returned = found.unwrap().0.last_used_at;
        let recorded = store.sessions(&id("dario")).await.unwrap()[0].last_used_at;
        assert_eq!(returned, recorded, "the answer shows what is on record");
        recorded
    };
    let touch = Duration::minutes(SESSION_TOUCH_MINUTES);

    // Soon after: nothing is written.
    assert_eq!(last_used(at(10, 1)).await, at(10, 0));
    assert_eq!(last_used(at(10, 0) + touch).await, at(10, 0));
    // Later: the use is written down.
    let later = at(10, 0) + touch + Duration::seconds(1);
    assert_eq!(last_used(later).await, later);
    assert_eq!(last_used(later + Duration::minutes(1)).await, later);
    // A clock that went back does not move the record back.
    assert_eq!(last_used(at(9, 0)).await, later);
}

pub(crate) async fn sessions_are_listed_and_removed_per_account(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    sign_in(store, "phone", &dario, at(10, 0), 1).await;
    sign_in(store, "laptop", &dario, at(12, 0), 2).await;
    sign_in(store, "tablet", &dario, at(11, 0), 3).await;
    sign_in(store, "annas", &anna, at(13, 0), 4).await;

    // Most recently used first.
    assert_eq!(
        session_names(store, &dario).await,
        ["laptop", "tablet", "phone"]
    );
    let listed = &store.sessions(&dario.id).await.unwrap()[0];
    assert_eq!(listed, &session("laptop", &dario, at(12, 0)));
    assert!(store.sessions(&id("nobody")).await.unwrap().is_empty());

    // Nobody signs out a device of another account.
    assert!(!store.delete_session(&dario.id, &id("annas")).await.unwrap());
    assert!(
        !store
            .delete_session(&dario.id, &id("unknown"))
            .await
            .unwrap()
    );
    assert_eq!(session_names(store, &anna).await, ["annas"]);

    assert!(
        store
            .delete_session(&dario.id, &id("tablet"))
            .await
            .unwrap()
    );
    assert!(
        !store
            .delete_session(&dario.id, &id("tablet"))
            .await
            .unwrap()
    );
    assert_eq!(session_names(store, &dario).await, ["laptop", "phone"]);
    let gone = store
        .use_session(&secret(3), at(13, 0), long_ago())
        .await
        .unwrap();
    assert_eq!(gone, None, "a removed session's token is worth nothing");

    store.delete_sessions(&dario.id).await.unwrap();
    assert!(session_names(store, &dario).await.is_empty());
    assert_eq!(session_names(store, &anna).await, ["annas"]);
}

pub(crate) async fn an_account_keeps_only_its_most_recent_sessions(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    sign_in(store, "annas", &anna, at(1, 0), 9).await;
    let names = ["a", "b", "c", "d", "e"];
    for (index, name) in names.iter().enumerate() {
        let hour = 10 + index as u32;
        let token = index as u8;
        store
            .create_session(&session(name, &dario, at(hour, 0)), &secret(token), 3)
            .await
            .unwrap();
    }

    assert_eq!(session_names(store, &dario).await, ["e", "d", "c"]);
    // The limit is per account.
    assert_eq!(session_names(store, &anna).await, ["annas"]);
    let oldest = store
        .use_session(&secret(0), at(20, 0), long_ago())
        .await
        .unwrap();
    assert_eq!(oldest, None);
}

pub(crate) async fn sessions_unused_for_too_long_are_pruned(store: &impl Store) {
    let dario = create(store, "dario").await;
    sign_in(store, "old", &dario, at(8, 0), 1).await;
    sign_in(store, "older", &dario, at(7, 0), 2).await;
    sign_in(store, "fresh", &dario, at(10, 0), 3).await;

    assert_eq!(store.prune_sessions(at(9, 0)).await.unwrap(), 2);

    assert_eq!(session_names(store, &dario).await, ["fresh"]);
    assert_eq!(store.prune_sessions(at(9, 0)).await.unwrap(), 0);
    // Last used exactly at the limit stays.
    assert_eq!(store.prune_sessions(at(10, 0)).await.unwrap(), 0);
}

pub(crate) async fn a_new_password_signs_out_the_other_devices(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    sign_in(store, "phone", &dario, at(10, 0), 1).await;
    sign_in(store, "laptop", &dario, at(11, 0), 2).await;
    sign_in(store, "annas", &anna, at(12, 0), 3).await;

    store
        .set_password(&dario.id, "$argon2id$new", &id("phone"))
        .await
        .unwrap();

    assert_eq!(
        store.password_hash("dario").await.unwrap().unwrap().1,
        "$argon2id$new"
    );
    assert_eq!(session_names(store, &dario).await, ["phone"]);
    // Nobody else is affected.
    assert_eq!(session_names(store, &anna).await, ["annas"]);
    assert_eq!(
        store.password_hash("anna").await.unwrap().unwrap().1,
        "$argon2id$hash-of-anna"
    );
    // An account that does not exist is not an error.
    store
        .set_password(&id("nobody"), "$argon2id$x", &id("phone"))
        .await
        .unwrap();
}

pub(crate) async fn a_deleted_account_leaves_nothing_behind(store: &impl Store) {
    let invite = NewInvite {
        code: secret(5),
        created_at: at(8, 0),
        expires_at: None,
    };
    store.create_invite(&invite).await.unwrap();
    let invited = NewAccount {
        invite: Some(secret(5)),
        ..new_account("dario")
    };
    store.create_account(&invited).await.unwrap();
    let dario = invited.account;
    let anna = create(store, "anna").await;
    sign_in(store, "phone", &dario, at(10, 0), 1).await;
    sign_in(store, "annas", &anna, at(10, 0), 2).await;
    let kept = new_flight(&dario, 1, at(11, 0));
    let deleted = new_flight(&dario, 2, at(12, 0));
    let annas = new_flight(&anna, 1, at(11, 0));
    for flight in [&kept, &deleted, &annas] {
        store.save_flight(flight).await.unwrap();
    }
    store
        .delete_flight(&dario.id, &deleted.id, at(13, 0))
        .await
        .unwrap();

    store.delete_account(&dario.id).await.unwrap();

    assert_eq!(store.account("dario").await.unwrap(), None);
    assert_eq!(store.password_hash("dario").await.unwrap(), None);
    let session = store
        .use_session(&secret(1), at(14, 0), long_ago())
        .await
        .unwrap();
    assert_eq!(session, None);
    assert!(store.sessions(&dario.id).await.unwrap().is_empty());
    assert_eq!(store.flight(&kept.id).await.unwrap(), None);
    assert_eq!(store.flight_file(&kept.id).await.unwrap(), None);
    let listed = store
        .flights_of(&dario.id, Listing::All, None, 10)
        .await
        .unwrap();
    assert!(listed.is_empty());
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert!(changes.flights.is_empty() && changes.deleted.is_empty());
    assert_eq!(changes.latest, 0);

    // The other account has everything it had.
    assert_eq!(store.account("anna").await.unwrap(), Some(anna.clone()));
    assert_eq!(session_names(store, &anna).await, ["annas"]);
    assert!(store.flight(&annas.id).await.unwrap().is_some());

    // The handle is free, the invite stays used, and deleting again is
    // harmless.
    store.delete_account(&dario.id).await.unwrap();
    let again = NewAccount {
        account: Account {
            id: id("second"),
            ..account("dario")
        },
        invite: Some(secret(5)),
        ..new_account("dario")
    };
    assert_eq!(
        store.create_account(&again).await.unwrap(),
        AccountCreation::InviteRefused
    );
    let again = NewAccount {
        invite: None,
        ..again
    };
    assert_eq!(
        store.create_account(&again).await.unwrap(),
        AccountCreation::Created
    );
    // The new account starts empty.
    let listed = store
        .flights_of(&id("second"), Listing::All, None, 10)
        .await
        .unwrap();
    assert!(listed.is_empty());
}
