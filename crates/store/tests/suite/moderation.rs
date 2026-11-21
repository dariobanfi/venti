//! What every store must do for whoever runs the server: keep the reports
//! pilots send, with the words of a group or a plan that was reported,
//! take a flight down and keep it down, remove a live track, and suspend
//! an account's publishing. What it does about a group and a plan is with
//! the groups (`groups.rs`).

use chrono::{Duration, NaiveDate, NaiveTime};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_store::{
    Account, Flight, KeeperAct, Listing, LiveBatch, LiveFix, LiveKept, LiveRoom, LiveSaved,
    LiveVisibility, NewFlight, NewReport, PlanChange, PlanDetails, PlanRemoval, PublicFlights,
    PublicOrder, Report, ReportAbout, ReportClosing, ReportClosure, ReportListing, ReportSaved,
    ReportSubject, Room, Saved, Store, StoreError, Suspension, Visibility, VisibilityChange,
};

use super::accounts::{create, id};
use super::at;
use super::flights::{new_flight, put};
use super::groups::{come_in, day, make, new_plan, plan_on, post};

// --- Helpers -------------------------------------------------------------

fn micros(time: Timestamp) -> i64 {
    time.timestamp_micros()
}

/// Stores flight `n` of an account, starting at 10:00 plus `n` minutes,
/// uploaded at 12:00, and sets who may see it at 12:30.
async fn flight(store: &impl Store, account: &Account, n: u8, visibility: Visibility) -> Flight {
    let new = NewFlight {
        uploaded_at: at(12, 0),
        ..new_flight(account, n, at(10, n.into()))
    };
    let (stored, created) = put(store, &new).await;
    assert!(created);
    if visibility == Visibility::Private {
        return stored;
    }
    set(store, &stored, visibility, at(12, 30))
        .await
        .flight()
        .expect("nothing stands in the way")
}

/// Asks, as the flight's pilot, for the flight to be seen by `visibility`.
async fn set(
    store: &impl Store,
    flight: &Flight,
    visibility: Visibility,
    now: Timestamp,
) -> VisibilityChange {
    store
        .set_visibility(&flight.account_id, &flight.id, visibility, now)
        .await
        .unwrap()
}

async fn read(store: &impl Store, flight: &Flight) -> Flight {
    store.flight(&flight.id).await.unwrap().expect("the flight")
}

fn short(id: &str) -> String {
    id.trim_start_matches('0').to_owned()
}

/// Everyone's public flights, by their short names.
async fn public_flights(store: &impl Store) -> Vec<String> {
    let flights = store
        .public_flights(&PublicFlights::default(), &PublicOrder::Newest(None), 100)
        .await
        .unwrap();
    flights.iter().map(|flight| short(&flight.id)).collect()
}

/// An account's own page, by the short names of its flights.
async fn page_of(store: &impl Store, account: &Account) -> Vec<String> {
    let flights = store
        .flights_of(&account.id, Listing::Public, None, 100)
        .await
        .unwrap();
    flights.iter().map(|flight| short(&flight.id)).collect()
}

fn live_fix(second: i64) -> LiveFix {
    LiveFix {
        time: at(18, 0) + Duration::seconds(second),
        position: GeoPoint::new(47.66, 11.80).unwrap(),
        altitude_m: 1600,
        ground_speed_ms: None,
        climb_ms: None,
        course_deg: None,
    }
}

/// Hands in fixes for a live track of an account, as a call made at 19:00
/// that may begin a track.
async fn live(
    store: &impl Store,
    account: &Account,
    track: &str,
    seconds: &[i64],
    visibility: LiveVisibility,
) -> LiveSaved {
    let batch = LiveBatch {
        id: id(track),
        account_id: account.id.clone(),
        visibility,
        landed: false,
        fixes: seconds.iter().map(|second| live_fix(*second)).collect(),
    };
    let room = LiveRoom {
        fixes_per_track: 1000,
        may_create: true,
        expired_before: at(0, 0),
        now: at(19, 0),
    };
    store.save_live_fixes(&batch, &room).await.unwrap()
}

async fn kept(
    store: &impl Store,
    account: &Account,
    track: &str,
    seconds: &[i64],
    visibility: LiveVisibility,
) -> LiveKept {
    match live(store, account, track, seconds, visibility).await {
        LiveSaved::Kept(kept) => *kept,
        other => panic!("expected the batch to be kept, got {other:?}"),
    }
}

/// The public live tracks anywhere, by their short names.
async fn public_tracks(store: &impl Store) -> Vec<String> {
    let globe = BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap();
    let tracks = store.live_tracks_in(&globe, at(0, 0), 100).await.unwrap();
    tracks.iter().map(|track| short(&track.id)).collect()
}

async fn visibility_of(store: &impl Store, track: &str) -> LiveVisibility {
    let track = store.live_track(&id(track)).await.unwrap();
    track.expect("the track").visibility
}

/// Report `name`, made at `made_at` by one account about something of
/// another's.
pub(crate) fn report(
    name: &str,
    reporter: &Account,
    about: &Account,
    subject: ReportSubject,
    made_at: Timestamp,
) -> NewReport {
    NewReport {
        id: id(name),
        created_at: made_at,
        reporter_id: reporter.id.clone(),
        about_account_id: Some(about.id.clone()),
        subject,
        reason: "offensive".to_owned(),
        text: Some(format!("What {} says about it", reporter.handle)),
    }
}

/// A report about flight `n` of an account.
pub(crate) fn about_flight(
    name: &str,
    reporter: &Account,
    about: &Account,
    n: u8,
    made_at: Timestamp,
) -> NewReport {
    let flight = id(&format!("{}{n}", about.handle));
    report(
        name,
        reporter,
        about,
        ReportSubject::Flight(flight),
        made_at,
    )
}

pub(crate) async fn stored(store: &impl Store, new: &NewReport) {
    assert_eq!(store.save_report(new).await.unwrap(), ReportSaved::Stored);
}

/// The reports of a listing, by their short names, oldest first.
pub(crate) async fn names(store: &impl Store, listing: ReportListing) -> Vec<String> {
    let reports = store.reports(listing, None, 100).await.unwrap();
    reports.iter().map(|report| short(&report.id)).collect()
}

// --- Taking a flight down ----------------------------------------------------

pub(crate) async fn a_flight_taken_down_is_private_until_it_is_released(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let public = flight(store, &dario, 1, Visibility::Public).await;
    let unlisted = flight(store, &dario, 2, Visibility::Unlisted).await;
    let annas = flight(store, &anna, 1, Visibility::Public).await;
    assert_eq!(public.taken_down_at, None);
    assert_eq!(public_flights(store).await, ["dario1", "anna1"]);

    // Taken down at 14:00, whatever its pilot had set.
    let down = store.take_down_flight(&public.id, at(14, 0)).await.unwrap();
    let expected = Flight {
        visibility: Visibility::Private,
        taken_down_at: Some(at(14, 0)),
        change: micros(at(14, 0)),
        ..public.clone()
    };
    assert_eq!(down, Some(expected.clone()));
    assert_eq!(read(store, &public).await, expected);
    // It is in no list, and its pilot's devices hear of it.
    assert_eq!(public_flights(store).await, ["anna1"]);
    assert!(page_of(store, &dario).await.is_empty());
    let changes = store
        .flight_changes(&dario.id, micros(at(13, 0)), 10)
        .await
        .unwrap();
    assert_eq!(changes.flights, std::slice::from_ref(&expected));

    // Its pilot cannot share it again, in either way, and nothing about
    // the flight changes for trying.
    for wanted in [Visibility::Public, Visibility::Unlisted] {
        let refused = set(store, &public, wanted, at(15, 0)).await;
        assert_eq!(refused, VisibilityChange::TakenDown, "{wanted:?}");
        assert_eq!(refused.flight(), None);
    }
    assert_eq!(read(store, &public).await, expected);
    // Asking for what it is, private, is fine and changes nothing.
    let same = set(store, &public, Visibility::Private, at(15, 0)).await;
    assert_eq!(same, VisibilityChange::Set(Box::new(expected.clone())));
    // Nor can another account do anything about it.
    let foreign = store
        .set_visibility(&anna.id, &public.id, Visibility::Public, at(15, 0))
        .await
        .unwrap();
    assert_eq!(foreign, VisibilityChange::NoSuchFlight);

    // Taking it down again keeps the moment of the first time.
    let again = store.take_down_flight(&public.id, at(16, 0)).await.unwrap();
    assert_eq!(again, Some(expected.clone()));
    // Working its values out again leaves it taken down.
    let derived = public.derived.clone();
    assert!(
        store
            .set_derived(&public.id, &derived, at(16, 30))
            .await
            .unwrap()
    );
    let reworked = read(store, &public).await;
    assert_eq!(reworked.taken_down_at, Some(at(14, 0)));
    assert_eq!(reworked.visibility, Visibility::Private);

    // A flight that is shared by link or not at all can be taken down too,
    // and the other flights are as they were.
    let down = store
        .take_down_flight(&unlisted.id, at(17, 0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(down.visibility, Visibility::Private);
    assert_eq!(down.taken_down_at, Some(at(17, 0)));
    assert_eq!(read(store, &annas).await, annas);
    assert_eq!(public_flights(store).await, ["anna1"]);

    // Released at 18:00: it stays private, and its pilot may share it.
    let released = store.release_flight(&public.id, at(18, 0)).await.unwrap();
    let expected = Flight {
        taken_down_at: None,
        change: micros(at(18, 0)),
        ..reworked
    };
    assert_eq!(released, Some(expected.clone()));
    assert_eq!(public_flights(store).await, ["anna1"]);
    let shared = set(store, &public, Visibility::Public, at(19, 0)).await;
    assert_eq!(
        shared.flight().map(|flight| flight.visibility),
        Some(Visibility::Public)
    );
    assert_eq!(public_flights(store).await, ["dario1", "anna1"]);
    // Releasing a flight that is not taken down changes nothing.
    let before = read(store, &annas).await;
    let nothing = store.release_flight(&annas.id, at(20, 0)).await.unwrap();
    assert_eq!(nothing, Some(before));

    // An address that belongs to no flight, and text that is no address.
    for unknown in [id("unknown"), "x' OR '1'='1".to_owned(), "a\0b".to_owned()] {
        let down = store.take_down_flight(&unknown, at(20, 0)).await.unwrap();
        assert_eq!(down, None, "{unknown:?}");
        let released = store.release_flight(&unknown, at(20, 0)).await.unwrap();
        assert_eq!(released, None, "{unknown:?}");
    }
}

pub(crate) async fn a_flight_taken_down_stays_so_when_uploaded_again(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let new = NewFlight {
        uploaded_at: at(12, 0),
        ..new_flight(&dario, 1, at(10, 0))
    };
    put(store, &new).await;
    store
        .take_down_flight(&new.id, at(14, 0))
        .await
        .unwrap()
        .unwrap();

    // Its pilot deletes it and uploads the same recording again.
    assert!(
        store
            .delete_flight(&dario.id, &new.id, at(15, 0))
            .await
            .unwrap()
    );
    let again = NewFlight {
        uploaded_at: at(16, 0),
        ..new.clone()
    };
    let (back, created) = put(store, &again).await;
    assert!(created);
    // It is back as it was taken down, since the moment it was.
    assert_eq!(back.visibility, Visibility::Private);
    assert_eq!(back.taken_down_at, Some(at(14, 0)));
    assert_eq!(read(store, &back).await, back);
    let refused = set(store, &back, Visibility::Public, at(17, 0)).await;
    assert_eq!(refused, VisibilityChange::TakenDown);
    assert!(public_flights(store).await.is_empty());

    // The same recording in another account is another flight.
    let annas = NewFlight {
        uploaded_at: at(16, 0),
        ..new_flight(&anna, 1, at(10, 0))
    };
    assert_eq!(annas.identity, new.identity);
    let (annas, _) = put(store, &annas).await;
    assert_eq!(annas.taken_down_at, None);
    let shared = set(store, &annas, Visibility::Public, at(17, 0)).await;
    assert!(shared.flight().is_some());

    // Once it is released, deleting and uploading gives an ordinary
    // flight.
    store
        .release_flight(&back.id, at(18, 0))
        .await
        .unwrap()
        .unwrap();
    assert!(
        store
            .delete_flight(&dario.id, &new.id, at(19, 0))
            .await
            .unwrap()
    );
    let third = NewFlight {
        uploaded_at: at(20, 0),
        ..new.clone()
    };
    let saved = store.save_flight(&third, &Room::UNLIMITED).await.unwrap();
    let Saved::Created(fresh) = saved else {
        panic!("expected the flight to be created, got {saved:?}");
    };
    assert_eq!(fresh.taken_down_at, None);
    let shared = set(store, &fresh, Visibility::Public, at(21, 0)).await;
    assert!(shared.flight().is_some());
}

// --- Suspending an account's publishing ----------------------------------------

pub(crate) async fn an_account_whose_publishing_is_suspended_publishes_nothing(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    assert_eq!(dario.publishing_suspended_at, None);
    // Dario: two public flights, one by link, one private. Anna: one public.
    let third = flight(store, &dario, 3, Visibility::Public).await;
    let first = flight(store, &dario, 1, Visibility::Public).await;
    let by_link = flight(store, &dario, 2, Visibility::Unlisted).await;
    let private = flight(store, &dario, 4, Visibility::Private).await;
    let annas = flight(store, &anna, 1, Visibility::Public).await;
    // Each has a public live track; Dario has one by link as well.
    let public = LiveVisibility::Public;
    kept(store, &dario, "dlive", &[0, 1], public).await;
    kept(store, &dario, "dlink", &[0, 1], LiveVisibility::Link).await;
    kept(store, &anna, "alive", &[0, 1], public).await;
    assert_eq!(public_flights(store).await, ["dario3", "dario1", "anna1"]);
    assert_eq!(public_tracks(store).await, ["alive", "dlive"]);

    let suspended = store.suspend_publishing(&dario.id, at(14, 0)).await;
    assert_eq!(
        suspended.unwrap(),
        Some(Suspension {
            since: at(14, 0),
            flights: 2,
            live_tracks: 1,
        })
    );

    // The account says so wherever it is read, and is otherwise as it was.
    let now_suspended = Account {
        publishing_suspended_at: Some(at(14, 0)),
        ..dario.clone()
    };
    assert_eq!(
        store.account("dario").await.unwrap(),
        Some(now_suspended.clone())
    );
    let (with_hash, _) = store.password_hash("dario").await.unwrap().unwrap();
    assert_eq!(with_hash, now_suspended);
    assert_eq!(store.account("anna").await.unwrap(), Some(anna.clone()));

    // What was public is for those with its link now, each flight as a
    // change of its own in the order of their addresses; the rest is
    // untouched.
    let first_now = Flight {
        visibility: Visibility::Unlisted,
        change: micros(at(14, 0)),
        ..first.clone()
    };
    let third_now = Flight {
        visibility: Visibility::Unlisted,
        change: micros(at(14, 0)) + 1,
        ..third.clone()
    };
    assert_eq!(read(store, &first).await, first_now);
    assert_eq!(read(store, &third).await, third_now);
    assert_eq!(read(store, &by_link).await, by_link);
    assert_eq!(read(store, &private).await, private);
    assert_eq!(read(store, &annas).await, annas);
    let changes = store
        .flight_changes(&dario.id, micros(at(13, 0)), 10)
        .await
        .unwrap();
    assert_eq!(changes.flights, [first_now.clone(), third_now]);
    assert_eq!(changes.latest, micros(at(14, 0)) + 1);
    // Nothing of the account is in a list that others read.
    assert_eq!(public_flights(store).await, ["anna1"]);
    assert!(page_of(store, &dario).await.is_empty());
    assert_eq!(page_of(store, &anna).await, ["anna1"]);
    assert_eq!(public_tracks(store).await, ["alive"]);
    assert_eq!(visibility_of(store, "dlive").await, LiveVisibility::Link);

    // It can make nothing public. It can still share by link and take
    // back, and upload.
    for own in [&first, &by_link, &private] {
        let refused = set(store, own, Visibility::Public, at(15, 0)).await;
        assert_eq!(refused, VisibilityChange::PublishingSuspended);
    }
    assert_eq!(read(store, &first).await, first_now);
    let shared = set(store, &private, Visibility::Unlisted, at(15, 0)).await;
    assert!(shared.flight().is_some());
    let hidden = set(store, &first, Visibility::Private, at(15, 10)).await;
    assert!(hidden.flight().is_some());
    let new = NewFlight {
        uploaded_at: at(15, 20),
        ..new_flight(&dario, 5, at(11, 0))
    };
    assert!(put(store, &new).await.1);
    assert_eq!(public_flights(store).await, ["anna1"]);

    // Its live tracks are for those with the link, whatever its batches
    // say: one it has, with a newer fix, and one it begins. No fix is lost.
    let more = kept(store, &dario, "dlive", &[2, 3], public).await;
    assert_eq!(more.track.visibility, LiveVisibility::Link);
    assert_eq!((more.stored, more.track.fixes), (2, 4));
    let begun = kept(store, &dario, "dnew", &[5, 6], public).await;
    assert!(begun.created);
    assert_eq!(begun.track.visibility, LiveVisibility::Link);
    assert_eq!(visibility_of(store, "dnew").await, LiveVisibility::Link);
    assert_eq!(public_tracks(store).await, ["alive"]);
    // Anna's batches say what her tracks are.
    let annas_more = kept(store, &anna, "alive", &[2], public).await;
    assert_eq!(annas_more.track.visibility, LiveVisibility::Public);

    // Suspending again keeps the first moment and finds nothing to do.
    let again = store.suspend_publishing(&dario.id, at(16, 0)).await;
    assert_eq!(
        again.unwrap(),
        Some(Suspension {
            since: at(14, 0),
            flights: 0,
            live_tracks: 0,
        })
    );
    let nobody = store.suspend_publishing(&id("nobody"), at(16, 0)).await;
    assert_eq!(nobody.unwrap(), None);

    // Resumed: the account may publish again, and nothing is published
    // for it.
    assert_eq!(
        store.resume_publishing(&dario.id).await.unwrap(),
        Some(true)
    );
    assert_eq!(store.account("dario").await.unwrap(), Some(dario.clone()));
    assert_eq!(public_flights(store).await, ["anna1"]);
    assert_eq!(public_tracks(store).await, ["alive"]);
    let shared = set(store, &third, Visibility::Public, at(17, 0)).await;
    assert!(shared.flight().is_some());
    assert_eq!(public_flights(store).await, ["dario3", "anna1"]);
    let shown = kept(store, &dario, "dlive", &[9], public).await;
    assert_eq!(shown.track.visibility, LiveVisibility::Public);
    assert_eq!(public_tracks(store).await, ["alive", "dlive"]);
    // Resuming what is not suspended, and nobody.
    assert_eq!(
        store.resume_publishing(&dario.id).await.unwrap(),
        Some(false)
    );
    assert_eq!(
        store.resume_publishing(&anna.id).await.unwrap(),
        Some(false)
    );
    assert_eq!(store.resume_publishing(&id("nobody")).await.unwrap(), None);
}

pub(crate) async fn a_suspension_and_a_publication_never_both_win(store: &impl Store) {
    // A suspension races with the account making flights public and
    // beginning public live tracks. Whatever the order, when all of it is
    // done nothing of the account is public.
    for round in 0..4_u8 {
        let handle = format!("pilot{round}");
        let pilot = create(store, &handle).await;
        let mut flights = Vec::new();
        for n in 1..=4 {
            flights.push(flight(store, &pilot, n, Visibility::Unlisted).await);
        }
        let track = |n: usize| format!("{handle}t{n}");
        kept(store, &pilot, &track(0), &[0], LiveVisibility::Public).await;

        let publish = |n: usize| set(store, &flights[n], Visibility::Public, at(14, 0));
        let fly = |n: usize, second: i64| {
            let (pilot, track) = (&pilot, track(n));
            async move { live(store, pilot, &track, &[second], LiveVisibility::Public).await }
        };
        let (a, b, suspended, c, d, e, f, g) = tokio::join!(
            publish(0),
            publish(1),
            store.suspend_publishing(&pilot.id, at(14, 0)),
            publish(2),
            publish(3),
            fly(0, 5),
            fly(1, 5),
            fly(2, 5),
        );
        assert!(suspended.unwrap().is_some());
        for change in [a, b, c, d] {
            assert!(
                matches!(
                    change,
                    VisibilityChange::Set(_) | VisibilityChange::PublishingSuspended
                ),
                "{change:?}"
            );
        }
        for saved in [e, f, g] {
            assert!(matches!(saved, LiveSaved::Kept(_)), "{saved:?}");
        }

        assert!(page_of(store, &pilot).await.is_empty(), "round {round}");
        assert!(public_flights(store).await.is_empty(), "round {round}");
        assert!(public_tracks(store).await.is_empty(), "round {round}");
        // No change number was given out twice.
        let changes = store.flight_changes(&pilot.id, 0, 100).await.unwrap();
        let mut numbers: Vec<i64> = changes.flights.iter().map(|f| f.change).collect();
        numbers.dedup();
        assert_eq!(numbers.len(), 4);
        assert_eq!(changes.latest, *numbers.last().unwrap());
    }
}

// --- Removing a live track -------------------------------------------------------

pub(crate) async fn a_live_track_is_removed_whoever_it_belongs_to(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    kept(store, &dario, "dlive", &[0, 1, 2], LiveVisibility::Public).await;
    kept(store, &dario, "dlink", &[0, 1], LiveVisibility::Link).await;
    kept(store, &anna, "alive", &[0], LiveVisibility::Public).await;

    assert!(
        store
            .remove_live_track(&id("dlive"), at(19, 0))
            .await
            .unwrap()
    );

    // It is gone with its fixes, and its identifier takes nothing more,
    // from its pilot or from anyone.
    assert_eq!(store.live_track(&id("dlive")).await.unwrap(), None);
    let fixes = store.live_fixes(&id("dlive"), 0, None, 100).await.unwrap();
    assert!(fixes.is_empty());
    assert_eq!(public_tracks(store).await, ["alive"]);
    let more = live(store, &dario, "dlive", &[3], LiveVisibility::Public).await;
    assert_eq!(more, LiveSaved::Gone);
    let stranger = live(store, &anna, "dlive", &[3], LiveVisibility::Public).await;
    assert_eq!(stranger, LiveSaved::Gone);
    // A track shared by link is removed the same way; the others stay.
    assert!(
        store
            .remove_live_track(&id("dlink"), at(19, 0))
            .await
            .unwrap()
    );
    assert_eq!(store.live_track(&id("dlink")).await.unwrap(), None);
    assert!(store.live_track(&id("alive")).await.unwrap().is_some());

    // There is nothing to remove twice, and an identifier nothing is known
    // under is left alone: a track can still begin under it.
    for nothing in [id("dlive"), id("never"), "a\0b".to_owned()] {
        let removed = store.remove_live_track(&nothing, at(19, 0)).await.unwrap();
        assert!(!removed, "{nothing:?}");
    }
    let begun = kept(store, &dario, "never", &[0], LiveVisibility::Link).await;
    assert!(begun.created);
}

// --- Reports ----------------------------------------------------------------------

pub(crate) async fn a_report_is_kept_once_for_whoever_runs_the_server(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    assert!(names(store, ReportListing::Open).await.is_empty());

    // Dario reports a flight of Anna's, in words that are no query and no
    // markup.
    let first = NewReport {
        reason: "not_a_flight".to_owned(),
        text: Some("“Nicht geflogen” <b>'; DROP TABLE reports;--\nzweite Zeile".to_owned()),
        ..about_flight("r1", &dario, &anna, 1, at(12, 0))
    };
    stored(store, &first).await;
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(
        reports,
        [Report {
            id: first.id.clone(),
            created_at: at(12, 0),
            reporter_handle: "dario".to_owned(),
            about: Some(ReportAbout {
                account_id: anna.id.clone(),
                handle: "anna".to_owned(),
                name: "Pilot anna".to_owned(),
            }),
            subject: ReportSubject::Flight(id("anna1")),
            reason: "not_a_flight".to_owned(),
            text: first.text.clone(),
            closed: None,
        }]
    );

    // Reporting the same thing again adds nothing, whatever else the
    // second report says; nor does it replace the first.
    let again = NewReport {
        reason: "spam".to_owned(),
        text: None,
        ..about_flight("r1again", &dario, &anna, 1, at(12, 5))
    };
    assert_eq!(
        store.save_report(&again).await.unwrap(),
        ReportSaved::AlreadyOpen
    );
    let unchanged = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(unchanged, reports);

    // Someone else's report about the same flight is another report, and
    // so is Dario's about another flight, about Anna's page and about a
    // live track of hers, with and without words.
    stored(store, &about_flight("r2", &bernd, &anna, 1, at(12, 10))).await;
    stored(store, &about_flight("r3", &dario, &anna, 2, at(12, 20))).await;
    let page = report("r4", &dario, &anna, ReportSubject::Pilot, at(12, 30));
    stored(store, &page).await;
    let track = ReportSubject::LiveTrack(id("alive"));
    let wordless = NewReport {
        text: None,
        ..report("r5", &dario, &anna, track.clone(), at(12, 30))
    };
    stored(store, &wordless).await;
    // A flight and a live track with the same letters are two things.
    let same_letters = ReportSubject::Flight(id("alive"));
    stored(
        store,
        &report("r6", &dario, &anna, same_letters, at(12, 40)),
    )
    .await;

    // Oldest first; among reports of the same moment by identifier.
    let order = ["r1", "r2", "r3", "r4", "r5", "r6"];
    assert_eq!(names(store, ReportListing::Open).await, order);
    assert_eq!(names(store, ReportListing::All).await, order);
    let all = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(all[3].subject, ReportSubject::Pilot);
    assert_eq!(all[4].subject, track);
    assert_eq!(all[4].text, None);
    assert_eq!(all[1].reporter_handle, "bernd");
    // Page by page.
    for size in [1, 4] {
        let mut seen = Vec::new();
        let mut after = None;
        loop {
            let page = store
                .reports(ReportListing::Open, after.as_ref(), size)
                .await
                .unwrap();
            let Some(last) = page.last() else { break };
            assert!(page.len() <= size);
            after = Some(last.key());
            seen.extend(page.iter().map(|report| short(&report.id)));
        }
        assert_eq!(seen, order, "pages of {size}");
    }
    let none = store.reports(ReportListing::Open, None, 0).await.unwrap();
    assert!(none.is_empty());

    // A report that names an account nobody has is not stored, and one
    // with a zero byte in it is refused.
    let nobody = super::accounts::account("nobody");
    for strange in [
        about_flight("r7", &nobody, &anna, 1, at(13, 0)),
        about_flight("r8", &dario, &nobody, 1, at(13, 0)),
    ] {
        assert_eq!(
            store.save_report(&strange).await.unwrap(),
            ReportSaved::NoSuchAccount
        );
    }
    let zero = NewReport {
        text: Some("a\0b".to_owned()),
        ..about_flight("r9", &dario, &anna, 9, at(13, 0))
    };
    let refused = store.save_report(&zero).await;
    assert!(
        matches!(refused, Err(StoreError::Invalid(_))),
        "{refused:?}"
    );
    assert_eq!(names(store, ReportListing::All).await, order);
}

pub(crate) async fn reports_racing_for_one_subject_are_kept_once(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let attempt = |name: &'static str| {
        let new = about_flight(name, &dario, &anna, 1, at(12, 0));
        async move { store.save_report(&new).await.unwrap() }
    };

    let (a, b, c, d) = tokio::join!(attempt("ra"), attempt("rb"), attempt("rc"), attempt("rd"));

    let results = [a, b, c, d];
    let stored = results
        .iter()
        .filter(|saved| **saved == ReportSaved::Stored)
        .count();
    let already = results
        .iter()
        .filter(|saved| **saved == ReportSaved::AlreadyOpen)
        .count();
    assert_eq!((stored, already), (1, 3), "{results:?}");
    assert_eq!(names(store, ReportListing::All).await.len(), 1);
}

pub(crate) async fn a_closed_report_is_kept_for_a_while_and_then_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    for (name, n, minute) in [("r1", 1, 0), ("r2", 2, 10), ("r3", 3, 20)] {
        stored(store, &about_flight(name, &dario, &anna, n, at(12, minute))).await;
    }

    // Closed at 14:00 with a note.
    let note = "Taken down. “Thanks” <i>'; --";
    let closing = store.close_report(&id("r2"), note, at(14, 0)).await;
    assert_eq!(closing.unwrap(), ReportClosing::Closed);
    assert_eq!(names(store, ReportListing::Open).await, ["r1", "r3"]);
    assert_eq!(names(store, ReportListing::All).await, ["r1", "r2", "r3"]);
    let closure = ReportClosure {
        at: at(14, 0),
        note: note.to_owned(),
    };
    let all = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(all[1].closed, Some(closure.clone()));
    assert_eq!(all[0].closed, None);
    // Closing again says how it was closed and changes nothing.
    let again = store.close_report(&id("r2"), "another", at(15, 0)).await;
    assert_eq!(again.unwrap(), ReportClosing::AlreadyClosed(closure));
    for unknown in [id("r9"), "x' OR '1'='1".to_owned(), "a\0b".to_owned()] {
        let nothing = store.close_report(&unknown, "note", at(15, 0)).await;
        assert_eq!(nothing.unwrap(), ReportClosing::NoSuchReport, "{unknown:?}");
    }
    let zero = store.close_report(&id("r1"), "a\0b", at(15, 0)).await;
    assert!(matches!(zero, Err(StoreError::Invalid(_))), "{zero:?}");

    // What was closed can be reported anew: that is a new report.
    stored(store, &about_flight("r4", &dario, &anna, 2, at(15, 30))).await;
    assert_eq!(names(store, ReportListing::Open).await, ["r1", "r3", "r4"]);

    // Closed reports are removed once they were closed long enough ago;
    // open ones never, however old.
    store
        .close_report(&id("r3"), "nothing to do", at(16, 0))
        .await
        .unwrap();
    assert_eq!(store.prune_reports(at(14, 0)).await.unwrap(), 0);
    assert_eq!(
        store
            .prune_reports(at(14, 0) + Duration::seconds(1))
            .await
            .unwrap(),
        1
    );
    assert_eq!(names(store, ReportListing::All).await, ["r1", "r3", "r4"]);
    let far_ahead = at(0, 0) + Duration::days(3650);
    assert_eq!(store.prune_reports(far_ahead).await.unwrap(), 1);
    assert_eq!(names(store, ReportListing::All).await, ["r1", "r4"]);
    assert_eq!(store.prune_reports(far_ahead).await.unwrap(), 0);
}

pub(crate) async fn a_report_goes_with_either_of_its_accounts(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    stored(store, &about_flight("r1", &dario, &anna, 1, at(12, 0))).await;
    stored(store, &about_flight("r2", &anna, &dario, 1, at(12, 10))).await;
    stored(store, &about_flight("r3", &bernd, &anna, 1, at(12, 20))).await;
    stored(store, &about_flight("r4", &anna, &bernd, 1, at(12, 30))).await;
    stored(store, &about_flight("r5", &dario, &dario, 1, at(12, 40))).await;
    store
        .close_report(&id("r2"), "closed", at(13, 0))
        .await
        .unwrap();

    // Dario's account goes: what he reported and what was reported about
    // him go with it, open or closed.
    store.delete_account(&dario.id).await.unwrap();
    assert_eq!(names(store, ReportListing::All).await, ["r3", "r4"]);
    // A report names the accounts as they are now.
    let reports = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(reports[0].reporter_handle, "bernd");
    assert_eq!(reports[0].about.as_ref().unwrap().handle, "anna");

    store.delete_account(&anna.id).await.unwrap();
    assert!(names(store, ReportListing::All).await.is_empty());
}

// --- Reports about a group and about a plan ---------------------------------------

/// A group as a report names it, with the name it had when it was
/// reported.
fn group_named(group: &str, name: &str) -> ReportSubject {
    ReportSubject::Group {
        id: id(group),
        name: name.to_owned(),
    }
}

/// A plan of group `g1` as a report names it, with the words it had when
/// it was reported.
fn plan_saying(plan: &str, place: &str, note: Option<&str>) -> ReportSubject {
    ReportSubject::Plan {
        id: id(plan),
        group_id: id("g1"),
        place: place.to_owned(),
        note: note.map(str::to_owned),
    }
}

pub(crate) async fn a_report_about_a_group_or_a_plan_keeps_its_words(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    come_in(store, &bernd, 1, at(10, 10)).await;
    // A plan with everything a plan can say, in words that are no query
    // and no markup, and one that says as little as a plan may.
    let details = PlanDetails {
        day: day(19),
        place: "Musterberg – «Ost» <b>".to_owned(),
        position: Some(GeoPoint::new(47.66, 11.80).unwrap()),
        meet_at: NaiveTime::from_hms_opt(9, 30, 0),
        note: Some("Erste Zeile\n'; DROP TABLE reports;-- 山 🪂".to_owned()),
    };
    post(
        store,
        &new_plan("p1", "g1", &anna, details.clone(), at(12, 0)),
    )
    .await;
    plan_on(store, "p2", "g1", &anna, (20, None), 5).await;
    let (g1, p1) = (id("g1"), id("p1"));

    // Bernd and Anna report the group, which Dario keeps, by the name it
    // has; Dario reports two plans of Anna's, by what they say.
    let group = group_named("g1", "Group g1");
    let plan = plan_saying("p1", &details.place, details.note.as_deref());
    let noteless = plan_saying("p2", "Musterberg", None);
    stored(
        store,
        &report("r1", &bernd, &dario, group.clone(), at(13, 0)),
    )
    .await;
    stored(
        store,
        &report("r2", &dario, &anna, plan.clone(), at(13, 10)),
    )
    .await;
    let wordless = NewReport {
        text: None,
        ..report("r3", &dario, &anna, noteless.clone(), at(13, 20))
    };
    stored(store, &wordless).await;
    stored(
        store,
        &report("r4", &anna, &dario, group.clone(), at(13, 30)),
    )
    .await;

    // Each is read back with those words, the account it is about and
    // nothing else of the group or the plan: no day, no time, no position.
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 4);
    assert_eq!(
        reports[0],
        Report {
            id: id("r1"),
            created_at: at(13, 0),
            reporter_handle: "bernd".to_owned(),
            about: Some(ReportAbout {
                account_id: dario.id.clone(),
                handle: "dario".to_owned(),
                name: "Pilot dario".to_owned(),
            }),
            subject: group.clone(),
            reason: "offensive".to_owned(),
            text: Some("What bernd says about it".to_owned()),
            closed: None,
        }
    );
    assert_eq!(reports[1].subject, plan);
    assert_eq!(reports[1].about.as_ref().unwrap().handle, "anna");
    assert_eq!(reports[1].reporter_handle, "dario");
    assert_eq!(reports[2].subject, noteless);
    assert_eq!(reports[2].text, None);
    assert_eq!(reports[3].subject, group);

    // What was reported is changed: the keeper gives the group another
    // name, and the author makes the plan say something else.
    let renamed = store.rename_group(&g1, &dario.id, "Harmlos").await.unwrap();
    assert!(matches!(renamed, KeeperAct::Done(_)), "{renamed:?}");
    let harmless = PlanDetails {
        place: "Anderswo".to_owned(),
        note: None,
        ..details.clone()
    };
    let changed = store
        .change_plan(&g1, &p1, &anna.id, &harmless, at(14, 0), NaiveDate::MIN)
        .await
        .unwrap();
    assert!(matches!(changed, PlanChange::Now(_)), "{changed:?}");
    let kept = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(kept, reports, "a report says what was reported");
    // Then it is gone: the plan deleted by its author, the group with the
    // other plan removed. The reports are as they were.
    let deleted = store
        .delete_plan(&g1, &p1, &anna.id, NaiveDate::MIN)
        .await
        .unwrap();
    assert_eq!(deleted, PlanRemoval::Removed);
    assert!(
        store
            .remove_group(&g1, NaiveDate::MIN)
            .await
            .unwrap()
            .is_some()
    );
    let kept = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(kept, reports);
    // A report about what is gone is closed and removed like any other.
    let closing = store.close_report(&id("r3"), "Gone.", at(15, 0)).await;
    assert_eq!(closing.unwrap(), ReportClosing::Closed);
    assert_eq!(names(store, ReportListing::Open).await, ["r1", "r2", "r4"]);
    assert_eq!(store.prune_reports(at(16, 0)).await.unwrap(), 1);

    // Words with a zero byte in them are refused, wherever they stand.
    let zero = "a\0b";
    for (name, subject) in [
        ("z1", group_named(zero, "Group")),
        ("z2", group_named("g2", zero)),
        ("z3", plan_saying(zero, "Musterberg", None)),
        ("z4", plan_saying("p3", zero, None)),
        ("z5", plan_saying("p3", "Musterberg", Some(zero))),
        (
            "z6",
            ReportSubject::Plan {
                id: id("p3"),
                group_id: zero.to_owned(),
                place: "Musterberg".to_owned(),
                note: None,
            },
        ),
    ] {
        let refused = store
            .save_report(&report(name, &bernd, &dario, subject, at(16, 0)))
            .await;
        assert!(
            matches!(refused, Err(StoreError::Invalid(_))),
            "{name}: {refused:?}"
        );
    }
    assert_eq!(names(store, ReportListing::All).await, ["r1", "r2", "r4"]);

    // A report goes with either of its accounts, and the words it kept
    // with it: with the plan's author, with whoever reported, and with the
    // account that kept the group when it was reported.
    store.delete_account(&anna.id).await.unwrap();
    assert_eq!(names(store, ReportListing::All).await, ["r1"]);
    store.delete_account(&dario.id).await.unwrap();
    assert!(names(store, ReportListing::All).await.is_empty());
}

pub(crate) async fn one_open_report_about_a_group_whoever_keeps_it(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    make(store, "g1", &dario, 1, at(10, 0)).await;
    come_in(store, &anna, 1, at(10, 5)).await;
    come_in(store, &bernd, 1, at(10, 10)).await;
    plan_on(store, "p1", "g1", &anna, (19, None), 0).await;
    let g1 = id("g1");
    let already_open = |new: NewReport| async move {
        assert_eq!(
            store.save_report(&new).await.unwrap(),
            ReportSaved::AlreadyOpen,
            "{}",
            short(&new.id)
        );
    };

    // Bernd reports the group while Dario keeps it.
    let first = report(
        "r1",
        &bernd,
        &dario,
        group_named("g1", "Group g1"),
        at(13, 0),
    );
    stored(store, &first).await;
    // Reported again it is the same report, whatever the group is called
    // by then.
    let renamed = store.rename_group(&g1, &dario.id, "Other").await.unwrap();
    assert!(matches!(renamed, KeeperAct::Done(_)), "{renamed:?}");
    let other_name = group_named("g1", "Other");
    already_open(report("r1b", &bernd, &dario, other_name.clone(), at(13, 5))).await;
    // And whoever keeps it by then: the keeping passes to Anna, and the
    // group is still the same thing to report.
    let handed = store.hand_over_group(&g1, &dario.id, "anna").await.unwrap();
    assert!(matches!(handed, KeeperAct::Done(_)), "{handed:?}");
    already_open(report("r1c", &bernd, &anna, other_name.clone(), at(13, 10))).await;
    let reports = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].subject, group_named("g1", "Group g1"));
    assert_eq!(reports[0].about.as_ref().unwrap().handle, "dario");

    // Someone else's report about the group is another report, and so is
    // the keeper's own. A plan and a flight with the same letters are
    // other things.
    stored(
        store,
        &report("r2", &dario, &anna, other_name.clone(), at(13, 20)),
    )
    .await;
    stored(
        store,
        &report("r3", &anna, &anna, other_name.clone(), at(13, 30)),
    )
    .await;
    let plan_of_the_same_letters = plan_saying("g1", "Musterberg", None);
    stored(
        store,
        &report("r4", &bernd, &anna, plan_of_the_same_letters, at(13, 40)),
    )
    .await;
    let flight_of_the_same_letters = ReportSubject::Flight(g1.clone());
    stored(
        store,
        &report("r5", &bernd, &anna, flight_of_the_same_letters, at(13, 50)),
    )
    .await;
    // Another group is another thing.
    stored(
        store,
        &report("r6", &bernd, &anna, group_named("g2", "Other"), at(14, 0)),
    )
    .await;

    // A plan is one thing as well, whatever it says by the time it is
    // reported again.
    stored(
        store,
        &report(
            "r7",
            &bernd,
            &anna,
            plan_saying("p1", "Musterberg", None),
            at(14, 10),
        ),
    )
    .await;
    already_open(report(
        "r7b",
        &bernd,
        &anna,
        plan_saying("p1", "Anderswo", Some("And a note.")),
        at(14, 15),
    ))
    .await;

    // Once the first report is closed, Bernd can report the group anew,
    // and that report is about the group as it is then.
    store
        .close_report(&id("r1"), "Renamed.", at(15, 0))
        .await
        .unwrap();
    stored(
        store,
        &report("r8", &bernd, &anna, other_name.clone(), at(15, 10)),
    )
    .await;
    assert_eq!(
        names(store, ReportListing::Open).await,
        ["r2", "r3", "r4", "r5", "r6", "r7", "r8"]
    );

    // Reports that race for one group are kept once, also when they do not
    // agree on who keeps it: the keeping can change hands at that moment.
    make(store, "g3", &dario, 3, at(16, 0)).await;
    let attempt = |name: &'static str, keeper: &Account| {
        let new = report(
            name,
            &bernd,
            keeper,
            group_named("g3", "Group g3"),
            at(16, 5),
        );
        async move { store.save_report(&new).await.unwrap() }
    };
    let (a, b, c, d) = tokio::join!(
        attempt("ra", &dario),
        attempt("rb", &anna),
        attempt("rc", &dario),
        attempt("rd", &anna)
    );
    let results = [a, b, c, d];
    let stored = results
        .iter()
        .filter(|saved| **saved == ReportSaved::Stored)
        .count();
    let already = results
        .iter()
        .filter(|saved| **saved == ReportSaved::AlreadyOpen)
        .count();
    assert_eq!((stored, already), (1, 3), "{results:?}");
    assert_eq!(names(store, ReportListing::Open).await.len(), 8);
}

// --- Reports about a site that pilots made -----------------------------------

pub(crate) async fn a_report_about_a_site_names_no_second_account(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    // A site is named by its identifier, which refers to nothing here:
    // the report is kept whatever becomes of the site.
    let site = |name: &str, revision: u32| ReportSubject::Site {
        id: format!("venti:{}", id(name)),
        revision,
    };
    let about_site =
        |name: &str, reporter: &Account, subject: ReportSubject, minute: u32| NewReport {
            about_account_id: None,
            ..report(name, reporter, reporter, subject, at(13, minute))
        };

    stored(store, &about_site("r1", &dario, site("s1", 1), 0)).await;
    stored(store, &about_site("r2", &anna, site("s1", 2), 10)).await;
    stored(store, &about_site("r3", &dario, site("s2", 1), 20)).await;
    // One open report per reporter and site, whichever revision each is
    // about.
    let again = about_site("r4", &dario, site("s1", 3), 30);
    assert_eq!(
        store.save_report(&again).await.unwrap(),
        ReportSaved::AlreadyOpen
    );

    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 3);
    assert_eq!(
        reports[0],
        Report {
            id: id("r1"),
            created_at: at(13, 0),
            reporter_handle: "dario".to_owned(),
            about: None,
            subject: site("s1", 1),
            reason: "offensive".to_owned(),
            text: Some("What dario says about it".to_owned()),
            closed: None,
        }
    );
    assert_eq!(reports[1].subject, site("s1", 2));
    assert_eq!(reports[1].reporter_handle, "anna");
    assert_eq!(reports[2].subject, site("s2", 1));

    // A site is nobody's, and nothing else is: neither store keeps a
    // report that says otherwise.
    let somebodys = NewReport {
        about_account_id: Some(anna.id.clone()),
        ..about_site("r5", &bernd, site("s1", 1), 40)
    };
    let nobodys = NewReport {
        about_account_id: None,
        ..about_flight("r6", &bernd, &anna, 1, at(13, 50))
    };
    let unnumbered = about_site("r7", &bernd, site("s1", 0), 55);
    for wrong in [somebodys, nobodys, unnumbered] {
        let refused = store.save_report(&wrong).await;
        assert!(
            matches!(refused, Err(StoreError::Invalid(_))),
            "{refused:?}"
        );
    }
    // A reporter who is not there reports nothing.
    let mut ghost = about_site("r8", &bernd, site("s1", 1), 56);
    ghost.reporter_id = id("ghost");
    assert_eq!(
        store.save_report(&ghost).await.unwrap(),
        ReportSaved::NoSuchAccount
    );

    // Once the report is closed its reporter may report the site again.
    let closing = store.close_report(&id("r1"), "corrected", at(14, 0)).await;
    assert_eq!(closing.unwrap(), ReportClosing::Closed);
    stored(store, &again).await;
    assert_eq!(names(store, ReportListing::Open).await, ["r2", "r3", "r4"]);
    assert_eq!(
        names(store, ReportListing::All).await,
        ["r1", "r2", "r3", "r4"]
    );

    // An account that goes takes the reports it made, as always. No
    // other account's going touches a report about a site: whoever added
    // the site is not named by it.
    store.delete_account(&bernd.id).await.unwrap();
    assert_eq!(names(store, ReportListing::Open).await, ["r2", "r3", "r4"]);
    store.delete_account(&anna.id).await.unwrap();
    assert_eq!(names(store, ReportListing::Open).await, ["r3", "r4"]);
}
