//! What every store must do with accounts, invites and sessions.

use chrono::Duration;
use venti_domain::Timestamp;
use venti_store::{
    AccessToken, Account, AccountCreation, GoogleLink, GoogleLinking, GroupJoining, GroupRoom,
    Listing, NewAccount, NewGoogleAccount, NewInvite, ReportListing, SESSION_TOUCH_MINUTES,
    SecretHash, Session, Store,
};

use super::at;
use super::flights::new_flight;
use super::groups::{code, come_in, day, make, plan_on, set_in, who_is_in};
use super::live::{
    begin_live_tracks_for, has_its_live_tracks, nothing_is_left_of_the_live_tracks_of,
};
use super::moderation::{about_flight, names, stored};

// --- Helpers -------------------------------------------------------------

/// An identifier of the server's form: 26 characters that end in `name`.
pub(crate) fn id(name: &str) -> String {
    format!("{name:0>26}")
}

/// An account whose new flights start private: the tests here set who may
/// see each flight themselves. The server makes every account with the
/// setting on; the tests of the setting (`new_flights_are_public_by_their_
/// pilots_setting` and those after it) make theirs so.
pub(crate) fn account(handle: &str) -> Account {
    Account {
        id: id(handle),
        handle: handle.to_owned(),
        display_name: format!("Pilot {handle}"),
        created_at: at(9, 0),
        publishing_suspended_at: None,
        edits_sites_since: None,
        new_flights_public: false,
        country: None,
        glider_class: None,
        picture: None,
        shown_near_places: true,
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
pub(crate) async fn sign_in(
    store: &impl Store,
    name: &str,
    account: &Account,
    used: Timestamp,
    token: u8,
) {
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

pub(crate) async fn no_session_has_an_identifier_that_cannot_be_one(store: &impl Store) {
    let dario = create(store, "dario").await;
    sign_in(store, "phone", &dario, at(10, 0), 1).await;

    // What a path can carry and a database may refuse to look at: a zero
    // byte above all, which PostgreSQL takes in no text at all. None of it
    // is an error, and none of it removes anything.
    let long = "x".repeat(100_000);
    for text in [
        "\0",
        "0000000000000000000000000\0",
        "",
        " ",
        "phone' OR '1'='1",
        "\u{feff}",
        "\u{1F6E9}",
        long.as_str(),
    ] {
        let removed = store.delete_session(&dario.id, text).await;
        assert_eq!(
            removed.ok(),
            Some(false),
            "{:?}",
            &text[..text.len().min(40)]
        );
        let removed = store.delete_session(text, &id("phone")).await;
        assert_eq!(
            removed.ok(),
            Some(false),
            "{:?}",
            &text[..text.len().min(40)]
        );
    }
    assert_eq!(session_names(store, &dario).await, ["phone"]);
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

fn google(subject: &str, email: &str) -> GoogleLink {
    GoogleLink {
        subject: subject.to_owned(),
        email: email.to_owned(),
        linked_at: at(9, 30),
    }
}

pub(crate) async fn a_google_account_is_found_by_its_subject(store: &impl Store) {
    let new = NewGoogleAccount {
        account: account("anna"),
        google: google("1001", "anna@example.com"),
        invite: None,
    };
    assert_eq!(
        store.create_google_account(&new).await.unwrap(),
        AccountCreation::Created
    );
    assert_eq!(
        store.google_account("1001").await.unwrap(),
        Some(new.account.clone())
    );
    assert_eq!(store.google_account("1002").await.unwrap(), None);
    assert_eq!(store.google_account("10\u{0}01").await.unwrap(), None);
    assert_eq!(
        store.google_link(&new.account.id).await.unwrap(),
        Some(new.google.clone())
    );
    assert!(store.account("anna").await.unwrap().is_some());
    // No password: signing in with one finds nothing to check.
    assert!(!store.has_password(&new.account.id).await.unwrap());
    assert_eq!(store.password_hash("anna").await.unwrap(), None);

    // The same Google account cannot make a second one, and nothing of
    // the second is stored.
    let again = NewGoogleAccount {
        account: account("bernd"),
        google: google("1001", "anna@example.com"),
        invite: None,
    };
    assert_eq!(
        store.create_google_account(&again).await.unwrap(),
        AccountCreation::GoogleTaken
    );
    assert_eq!(store.account("bernd").await.unwrap(), None);
    // A handle that is taken is said before that.
    let taken = NewGoogleAccount {
        account: Account {
            id: id("other"),
            ..account("anna")
        },
        google: google("1003", "x@example.com"),
        invite: None,
    };
    assert_eq!(
        store.create_google_account(&taken).await.unwrap(),
        AccountCreation::HandleTaken
    );
    // An invite is used up as by a password account.
    store
        .create_invite(&NewInvite {
            code: secret(40),
            created_at: at(8, 0),
            expires_at: None,
        })
        .await
        .unwrap();
    let invited = NewGoogleAccount {
        account: account("carla"),
        google: google("1004", "carla@example.com"),
        invite: Some(secret(40)),
    };
    assert_eq!(
        store.create_google_account(&invited).await.unwrap(),
        AccountCreation::Created
    );
    let twice = NewGoogleAccount {
        account: account("dora"),
        google: google("1005", "dora@example.com"),
        invite: Some(secret(40)),
    };
    assert_eq!(
        store.create_google_account(&twice).await.unwrap(),
        AccountCreation::InviteRefused
    );

    // Deleting the account frees the Google account.
    store.delete_account(&new.account.id).await.unwrap();
    assert_eq!(store.google_account("1001").await.unwrap(), None);
    assert_eq!(
        store.create_google_account(&again).await.unwrap(),
        AccountCreation::Created
    );
}

pub(crate) async fn a_google_account_is_linked_to_one_account_at_a_time(store: &impl Store) {
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    assert!(store.has_password(&anna.id).await.unwrap());
    assert_eq!(store.google_link(&anna.id).await.unwrap(), None);

    let first = google("2001", "anna@example.com");
    assert_eq!(
        store.link_google(&anna.id, &first).await.unwrap(),
        GoogleLinking::Linked
    );
    assert_eq!(
        store.google_account("2001").await.unwrap(),
        Some(anna.clone())
    );
    // The password stays.
    assert!(store.password_hash("anna").await.unwrap().is_some());
    // Another account cannot have it.
    assert_eq!(
        store.link_google(&bernd.id, &first).await.unwrap(),
        GoogleLinking::TakenByAnother
    );
    assert_eq!(store.google_link(&bernd.id).await.unwrap(), None);
    assert_eq!(
        store
            .link_google(&id("nobody"), &google("2009", "x@example.com"))
            .await
            .unwrap(),
        GoogleLinking::NoSuchAccount
    );
    // Linked again, it keeps its moment and takes the new email.
    let renamed = GoogleLink {
        email: "anna@new.example".to_owned(),
        linked_at: at(11, 0),
        ..first.clone()
    };
    assert_eq!(
        store.link_google(&anna.id, &renamed).await.unwrap(),
        GoogleLinking::Linked
    );
    assert_eq!(
        store.google_link(&anna.id).await.unwrap(),
        Some(GoogleLink {
            email: "anna@new.example".to_owned(),
            ..first.clone()
        })
    );
    // Another Google account takes the place of the first.
    let second = google("2002", "anna@other.example");
    assert_eq!(
        store.link_google(&anna.id, &second).await.unwrap(),
        GoogleLinking::Linked
    );
    assert_eq!(store.google_account("2001").await.unwrap(), None);
    assert_eq!(store.google_link(&anna.id).await.unwrap(), Some(second));
    // The first is free for another account now.
    assert_eq!(
        store.link_google(&bernd.id, &first).await.unwrap(),
        GoogleLinking::Linked
    );

    assert!(store.unlink_google(&anna.id).await.unwrap());
    assert!(!store.unlink_google(&anna.id).await.unwrap());
    assert_eq!(store.google_account("2002").await.unwrap(), None);
    assert_eq!(store.google_link(&anna.id).await.unwrap(), None);
    assert_eq!(store.google_account("2001").await.unwrap(), Some(bernd));
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
        super::flights::put(store, flight).await;
    }
    store
        .delete_flight(&dario.id, &deleted.id, at(13, 0))
        .await
        .unwrap();
    // Each has a live track, and an identifier that takes nothing more.
    begin_live_tracks_for(store, &dario).await;
    begin_live_tracks_for(store, &anna).await;
    // Each reported the other, one report is closed, and whoever runs the
    // server took a flight of the account down and suspended its
    // publishing.
    let bernd = create(store, "bernd").await;
    for new in [
        about_flight("r1", &dario, &anna, 1, at(13, 10)),
        about_flight("r2", &anna, &dario, 1, at(13, 20)),
        about_flight("r3", &anna, &dario, 2, at(13, 30)),
        about_flight("r4", &bernd, &anna, 1, at(13, 40)),
    ] {
        stored(store, &new).await;
    }
    store
        .close_report(&id("r2"), "taken down", at(13, 50))
        .await
        .unwrap();
    store
        .take_down_flight(&kept.id, at(13, 50))
        .await
        .unwrap()
        .unwrap();
    store
        .suspend_publishing(&dario.id, at(13, 55))
        .await
        .unwrap()
        .unwrap();
    // The account keeps a group that the other is in and a group it is
    // alone in, and is in a group the other keeps. In the two that both
    // are in, each posted a plan and is in on the other's.
    make(store, "kept", &dario, 21, at(13, 0)).await;
    make(store, "alone", &dario, 22, at(13, 0)).await;
    make(store, "hers", &anna, 23, at(13, 0)).await;
    come_in(store, &anna, 21, at(13, 5)).await;
    come_in(store, &dario, 23, at(13, 5)).await;
    plan_on(store, "lone", "alone", &dario, (19, None), 0).await;
    for (group, his, hers) in [("kept", "d1", "a1"), ("hers", "d2", "a2")] {
        plan_on(store, his, group, &dario, (19, None), 0).await;
        plan_on(store, hers, group, &anna, (20, None), 1).await;
        set_in(store, (group, his), &anna, true, at(13, 30)).await;
        set_in(store, (group, hers), &dario, true, at(13, 30)).await;
    }

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
    assert!(has_its_live_tracks(store, &anna).await);
    // Of the deleted account's live tracks nothing is left, not even that
    // their identifiers were used.
    assert!(nothing_is_left_of_the_live_tracks_of(store, &dario, &anna).await);
    // Nor of what it reported, or of what was reported about it. A report
    // between two others is still there.
    assert_eq!(names(store, ReportListing::All).await, ["r4"]);
    // It is in no group, and nothing it put into one is left: not its
    // plans, and not its word on the other's plans. The group it kept is
    // the other member's to keep now; the group it was alone in is gone,
    // with its code.
    assert!(store.groups_of(&dario.id).await.unwrap().is_empty());
    for (group, hers) in [("kept", "a1"), ("hers", "a2")] {
        let seen = store.group(&id(group), &anna.id).await.unwrap().unwrap();
        assert_eq!(seen.group.keeper.handle, "anna", "{group}");
        assert_eq!(seen.members.len(), 1);
        let plans = store
            .plans_of_group(&id(group), &anna.id, day(1), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(plans.len(), 1, "{group}");
        assert_eq!(plans[0].id, id(hers));
        assert_eq!(who_is_in(&plans[0]), ["anna"]);
    }
    let room = GroupRoom {
        groups_per_account: 20,
        members_per_group: 50,
    };
    let into_the_lone_one = store
        .join_group(&anna.id, &code(22), &room, at(14, 0))
        .await
        .unwrap();
    assert_eq!(into_the_lone_one, GroupJoining::NoSuchInvite);
    // The code of the group that stays is as it was.
    come_in(store, &bernd, 21, at(14, 0)).await;

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

// --- Tokens a pilot gives to another program -----------------------------

fn access_token(name: &str, account: &Account, made: Timestamp) -> AccessToken {
    AccessToken {
        id: id(name),
        account_id: account.id.clone(),
        name: format!("Program {name}"),
        scopes: vec!["flights:read".to_owned(), "flights:write".to_owned()],
        created_at: made,
        last_used_on: None,
        expires_at: None,
        client: None,
    }
}

async fn token_names(store: &impl Store, account: &Account) -> Vec<String> {
    let tokens = store.access_tokens(&account.id).await.unwrap();
    tokens
        .iter()
        .map(|token| token.id.trim_start_matches('0').to_owned())
        .collect()
}

pub(crate) async fn a_token_for_a_program_is_good_until_its_end(store: &impl Store) {
    let dario = create(store, "dario").await;
    let lasting = access_token("logger", &dario, at(10, 0));
    let ending = AccessToken {
        expires_at: Some(at(12, 0)),
        ..access_token("vario", &dario, at(10, 1))
    };
    assert!(
        store
            .create_access_token(&lasting, &secret(1), 10)
            .await
            .unwrap()
    );
    assert!(
        store
            .create_access_token(&ending, &secret(2), 10)
            .await
            .unwrap()
    );
    // Listed as made, the newest first, and not yet used.
    assert_eq!(
        store.access_tokens(&dario.id).await.unwrap(),
        [ending.clone(), lasting.clone()]
    );

    // A use is found with the account, and the day of it is kept.
    let used = store.use_access_token(&secret(1), at(11, 0)).await.unwrap();
    let today = at(11, 0).date_naive();
    let lasting_used = AccessToken {
        last_used_on: Some(today),
        ..lasting.clone()
    };
    assert_eq!(used, Some((lasting_used.clone(), dario.clone())));
    // A later use on the same day changes nothing; one on a later day
    // moves the day on, and an earlier clock does not move it back.
    let again = store.use_access_token(&secret(1), at(11, 30)).await;
    assert_eq!(again.unwrap().unwrap().0.last_used_on, Some(today));
    let next_day = at(11, 0) + Duration::days(1);
    let later = store.use_access_token(&secret(1), next_day).await;
    assert_eq!(
        later.unwrap().unwrap().0.last_used_on,
        Some(next_day.date_naive())
    );
    let earlier = store.use_access_token(&secret(1), at(11, 0)).await;
    assert_eq!(
        earlier.unwrap().unwrap().0.last_used_on,
        Some(next_day.date_naive())
    );

    // A token is good until its end and not at it; a use that is refused
    // is not written down.
    let before = store
        .use_access_token(&secret(2), at(12, 0) - Duration::seconds(1))
        .await;
    assert!(before.unwrap().is_some());
    let at_the_end = store.use_access_token(&secret(2), at(12, 0)).await;
    assert_eq!(at_the_end.unwrap(), None);
    let after = store.use_access_token(&secret(2), next_day).await;
    assert_eq!(after.unwrap(), None);
    let listed = store.access_tokens(&dario.id).await.unwrap();
    assert_eq!(listed[0].last_used_on, Some(today), "{listed:?}");

    // A secret nobody was given, and a session's token, open nothing.
    let unknown = store.use_access_token(&secret(3), at(11, 0)).await;
    assert_eq!(unknown.unwrap(), None);
    sign_in(store, "phone", &dario, at(10, 0), 7).await;
    let a_session = store.use_access_token(&secret(7), at(11, 0)).await;
    assert_eq!(a_session.unwrap(), None);
    let as_session = store
        .use_session(&secret(1), at(11, 0), long_ago())
        .await
        .unwrap();
    assert_eq!(as_session, None, "a token for a program is no session");
}

pub(crate) async fn tokens_for_programs_are_withdrawn_one_by_one(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let made = |name, account, minute, byte| {
        let token = access_token(name, account, at(10, minute));
        async move {
            assert!(
                store
                    .create_access_token(&token, &secret(byte), 10)
                    .await
                    .unwrap()
            );
        }
    };
    made("logger", &dario, 0, 1).await;
    made("vario", &dario, 1, 2).await;
    made("annas", &anna, 2, 3).await;
    assert_eq!(token_names(store, &dario).await, ["vario", "logger"]);
    assert!(store.access_tokens(&id("nobody")).await.unwrap().is_empty());

    // Another account's token is not there to withdraw, nor is text that
    // cannot be an identifier.
    assert!(
        !store
            .delete_access_token(&dario.id, &id("annas"))
            .await
            .unwrap()
    );
    for text in ["", "a\0b", "' OR 1=1 --"] {
        assert!(!store.delete_access_token(&dario.id, text).await.unwrap());
        assert!(
            !store
                .delete_access_token(text, &id("logger"))
                .await
                .unwrap()
        );
    }
    assert!(
        store
            .delete_access_token(&dario.id, &id("logger"))
            .await
            .unwrap()
    );
    assert!(
        !store
            .delete_access_token(&dario.id, &id("logger"))
            .await
            .unwrap()
    );
    assert_eq!(token_names(store, &dario).await, ["vario"]);
    let gone = store.use_access_token(&secret(1), at(11, 0)).await;
    assert_eq!(gone.unwrap(), None, "a withdrawn token is worth nothing");
    let kept = store.use_access_token(&secret(2), at(11, 0)).await;
    assert!(kept.unwrap().is_some());

    // Signing out everywhere and a new password are about sessions: the
    // tokens stay.
    sign_in(store, "phone", &dario, at(10, 0), 8).await;
    store.delete_sessions(&dario.id).await.unwrap();
    store
        .set_password(&dario.id, "$argon2id$another", &id("phone"))
        .await
        .unwrap();
    assert_eq!(token_names(store, &dario).await, ["vario"]);

    // The account takes its tokens with it.
    store.delete_account(&dario.id).await.unwrap();
    assert!(token_names(store, &dario).await.is_empty());
    let orphan = store.use_access_token(&secret(2), at(11, 0)).await;
    assert_eq!(orphan.unwrap(), None);
    assert_eq!(token_names(store, &anna).await, ["annas"]);
}

pub(crate) async fn an_account_holds_only_so_many_tokens_for_programs(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    for (index, name) in ["a", "b", "c"].iter().enumerate() {
        let token = access_token(name, &dario, at(10, index as u32));
        let stored = store
            .create_access_token(&token, &secret(index as u8), 2)
            .await
            .unwrap();
        // The third is refused, and no older one makes room for it.
        assert_eq!(stored, index < 2, "{name}");
    }
    assert_eq!(token_names(store, &dario).await, ["b", "a"]);
    // The bound is per account, and withdrawing one makes room.
    let annas = access_token("annas", &anna, at(10, 5));
    assert!(
        store
            .create_access_token(&annas, &secret(9), 2)
            .await
            .unwrap()
    );
    assert!(
        store
            .delete_access_token(&dario.id, &id("a"))
            .await
            .unwrap()
    );
    let again = access_token("c", &dario, at(10, 6));
    assert!(
        store
            .create_access_token(&again, &secret(2), 2)
            .await
            .unwrap()
    );
    assert_eq!(token_names(store, &dario).await, ["c", "b"]);

    // An account that is not there holds none.
    let nobody = account("nobody");
    let stray = access_token("stray", &nobody, at(10, 7));
    assert!(
        !store
            .create_access_token(&stray, &secret(5), 2)
            .await
            .unwrap()
    );
}
