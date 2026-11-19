//! What every store must do with live tracks.

use chrono::Duration;
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_store::{
    Account, LiveBatch, LiveFix, LiveFixAt, LiveKept, LiveRemoval, LiveRoom, LiveSaved, LiveTrack,
    LiveVisibility, Store, StoreError,
};

use super::accounts::{create, id};
use super::at;

// --- Helpers -------------------------------------------------------------

/// The moment `second` seconds after 18:00.
fn sec(second: i64) -> Timestamp {
    at(18, 0) + Duration::seconds(second)
}

/// A fix at `second`, a little further north with every second.
fn fix(second: i64) -> LiveFix {
    LiveFix {
        time: sec(second),
        position: GeoPoint::new(47.66 + second as f64 * 1e-5, 11.80).unwrap(),
        altitude_m: 1600 + i32::try_from(second).unwrap(),
        ground_speed_ms: None,
        climb_ms: None,
        course_deg: None,
    }
}

fn batch(
    account: &Account,
    track: &str,
    seconds: &[i64],
    visibility: LiveVisibility,
    landed: bool,
) -> LiveBatch {
    LiveBatch {
        id: id(track),
        account_id: account.id.clone(),
        visibility,
        landed,
        fixes: seconds.iter().map(|second| fix(*second)).collect(),
    }
}

/// Fixes for a track everyone may see, of a glider in the air.
fn public(account: &Account, track: &str, seconds: &[i64]) -> LiveBatch {
    batch(account, track, seconds, LiveVisibility::Public, false)
}

/// Room for a thousand fixes, in a call made at 19:00 that may begin a
/// track and finds nothing too old.
fn room() -> LiveRoom {
    LiveRoom {
        fixes_per_track: 1000,
        may_create: true,
        expired_before: at(0, 0),
        now: at(19, 0),
    }
}

async fn save(store: &impl Store, batch: &LiveBatch) -> LiveSaved {
    store.save_live_fixes(batch, &room()).await.unwrap()
}

/// Hands a batch in and expects the track to take it.
async fn send(store: &impl Store, batch: &LiveBatch) -> LiveKept {
    match save(store, batch).await {
        LiveSaved::Kept(kept) => *kept,
        other => panic!("expected the batch to be kept, got {other:?}"),
    }
}

async fn all_fixes(store: &impl Store, track: &str) -> Vec<LiveFixAt> {
    store.live_fixes(&id(track), 0, None, 10_000).await.unwrap()
}

/// The seconds after 18:00 of some fixes.
fn seconds(fixes: &[LiveFixAt]) -> Vec<i64> {
    fixes
        .iter()
        .map(|at| (at.fix.time - sec(0)).num_seconds())
        .collect()
}

fn names(tracks: &[LiveTrack]) -> Vec<String> {
    tracks
        .iter()
        .map(|track| track.id.trim_start_matches('0').to_owned())
        .collect()
}

/// The first region, and then some.
fn alps() -> BoundingBox {
    BoundingBox::new(46.0, 9.0, 49.0, 18.0).unwrap()
}

async fn listed(store: &impl Store, area: &BoundingBox, newest_since: Timestamp) -> Vec<String> {
    names(&store.live_tracks_in(area, newest_since, 100).await.unwrap())
}

/// Whether nothing is stored under an identifier that could be read.
async fn nothing_under(store: &impl Store, track: &str) -> bool {
    store.live_track(&id(track)).await.unwrap().is_none()
        && all_fixes(store, track).await.is_empty()
        && store.live_tail(&id(track), 10).await.unwrap().is_empty()
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn a_live_track_begins_with_its_first_fixes(store: &impl Store) {
    let dario = create(store, "dario").await;
    // Values that do not survive rounding, and values that are missing.
    let exact = LiveFix {
        time: sec(1),
        position: GeoPoint::new(47.6642 + 4e-15, 0.1 + 0.2).unwrap(),
        altitude_m: -12,
        ground_speed_ms: Some(10.499_999_999_999_998),
        climb_ms: Some(-1.2),
        course_deg: Some(359.999_999_999_999_94),
    };
    let first = LiveBatch {
        fixes: vec![fix(0), exact, fix(2)],
        ..batch(&dario, "live1", &[], LiveVisibility::Link, false)
    };

    let kept = send(store, &first).await;

    let expected = LiveTrack {
        id: id("live1"),
        account_id: dario.id.clone(),
        pilot_handle: "dario".to_owned(),
        pilot_name: "Pilot dario".to_owned(),
        visibility: LiveVisibility::Link,
        landed: false,
        started_at: sec(0),
        latest: fix(2),
        fixes: 3,
        cursor: 3,
    };
    assert!(kept.created);
    assert_eq!((kept.stored, kept.known, kept.full), (3, 0, 0));
    assert_eq!(kept.track, expected);
    assert_eq!(
        store.live_track(&id("live1")).await.unwrap(),
        Some(expected)
    );
    let fixes = all_fixes(store, "live1").await;
    let numbered: Vec<i64> = fixes.iter().map(|at| at.seq).collect();
    assert_eq!(numbered, [1, 2, 3]);
    let fixes: Vec<LiveFix> = fixes.into_iter().map(|at| at.fix).collect();
    assert_eq!(fixes, first.fixes);
    assert_eq!(
        store.live_tail(&id("live1"), 10).await.unwrap(),
        first.fixes
    );

    assert!(nothing_under(store, "unknown").await);
}

pub(crate) async fn the_same_live_fixes_again_change_nothing(store: &impl Store) {
    let dario = create(store, "dario").await;
    let first = public(&dario, "live1", &[0, 1, 2]);
    let begun = send(store, &first).await;

    // The answer was lost and the app sends the batch again; once more
    // with other values for the same seconds.
    let again = send(store, &first).await;
    let mut other = first.clone();
    for fix in &mut other.fixes {
        fix.altitude_m += 500;
    }
    let other = send(store, &other).await;

    for kept in [&again, &other] {
        assert!(!kept.created);
        assert_eq!((kept.stored, kept.known, kept.full), (0, 3, 0));
        assert_eq!(kept.track, begun.track);
    }
    let fixes: Vec<LiveFix> = all_fixes(store, "live1")
        .await
        .into_iter()
        .map(|at| at.fix)
        .collect();
    assert_eq!(
        fixes, first.fixes,
        "the first fix received for a second stays"
    );

    // A batch that overlaps adds what is new.
    let more = send(store, &public(&dario, "live1", &[1, 2, 3, 4])).await;
    assert_eq!((more.stored, more.known), (2, 2));
    assert_eq!(more.track.fixes, 5);
    assert_eq!(more.track.cursor, 5);
    assert_eq!(more.track.latest, fix(4));
}

pub(crate) async fn live_fixes_that_arrive_late_reach_those_who_wait(store: &impl Store) {
    let dario = create(store, "dario").await;
    send(store, &public(&dario, "live1", &[0, 1, 2])).await;
    // The network was gone from second 3 to second 49. The app sends where
    // the glider is now first.
    let now = send(store, &public(&dario, "live1", &[50, 51])).await;
    let seen = now.track.cursor;
    assert_eq!(seen, 5);

    // Then it fills the gap, and a moment later sends on.
    let gap = send(store, &public(&dario, "live1", &[10, 20, 30])).await;
    send(store, &public(&dario, "live1", &[52])).await;

    assert_eq!(gap.track.latest, fix(51), "the glider is where it was");
    assert_eq!(gap.track.started_at, sec(0));
    // Whoever had seen everything up to the gap's arrival gets the gap and
    // what came after, in the order of arrival.
    let news = store
        .live_fixes(&id("live1"), seen, None, 100)
        .await
        .unwrap();
    assert_eq!(seconds(&news), [10, 20, 30, 52]);
    let numbered: Vec<i64> = news.iter().map(|at| at.seq).collect();
    assert_eq!(numbered, [6, 7, 8, 9]);
    // Nothing is waiting beyond the last number.
    let nothing = store.live_fixes(&id("live1"), 9, None, 100).await.unwrap();
    assert!(nothing.is_empty());

    // A page at a time, from wherever the page before ended.
    let page = store.live_fixes(&id("live1"), 0, None, 4).await.unwrap();
    assert_eq!(seconds(&page), [0, 1, 2, 50]);
    let next = store
        .live_fixes(&id("live1"), page[3].seq, None, 4)
        .await
        .unwrap();
    assert_eq!(seconds(&next), [51, 10, 20, 30]);

    // Only what lies after a moment, still by arrival.
    let late = store
        .live_fixes(&id("live1"), 0, Some(sec(20)), 100)
        .await
        .unwrap();
    assert_eq!(seconds(&late), [50, 51, 30, 52]);
    let late = store
        .live_fixes(&id("live1"), seen, Some(sec(20)), 100)
        .await
        .unwrap();
    assert_eq!(seconds(&late), [30, 52]);

    // The end of the track is its newest fixes by time, oldest first.
    let tail = store.live_tail(&id("live1"), 4).await.unwrap();
    let tail: Vec<Timestamp> = tail.iter().map(|fix| fix.time).collect();
    assert_eq!(tail, [sec(30), sec(50), sec(51), sec(52)]);
    assert_eq!(store.live_tail(&id("live1"), 100).await.unwrap().len(), 9);
    assert!(store.live_tail(&id("live1"), 0).await.unwrap().is_empty());
}

pub(crate) async fn the_newest_fix_says_who_sees_a_live_track(store: &impl Store) {
    use LiveVisibility::{Link, Public};
    let dario = create(store, "dario").await;
    let state = |kept: &LiveKept| (kept.track.visibility, kept.track.landed);

    let begun = send(store, &batch(&dario, "live1", &[0, 1], Public, false)).await;
    assert_eq!(state(&begun), (Public, false));
    // A newer fix carries the pilot's new wish.
    let narrowed = send(store, &batch(&dario, "live1", &[2], Link, false)).await;
    assert_eq!(state(&narrowed), (Link, false));
    // A batch that was on its way since before does not undo it, though
    // its fixes are taken.
    let late = send(store, &batch(&dario, "live1", &[-5, -4], Public, true)).await;
    assert_eq!(state(&late), (Link, false));
    assert_eq!(late.stored, 2);
    assert_eq!(late.track.started_at, sec(-5));

    // With no newer fix to send, the newest one again narrows who sees
    // the track and says that it landed, and cannot do the opposite.
    let widened = send(store, &batch(&dario, "live1", &[2], Public, false)).await;
    assert_eq!(state(&widened), (Link, false));
    let landed = send(store, &batch(&dario, "live1", &[2], Public, true)).await;
    assert_eq!(state(&landed), (Link, true));
    let still = send(store, &batch(&dario, "live1", &[1, 2], Public, false)).await;
    assert_eq!(state(&still), (Link, true));
    assert_eq!(still.stored, 0);

    // A glider that flies on is flying, and is seen as its pilot now says.
    let on = send(store, &batch(&dario, "live1", &[3], Public, false)).await;
    assert_eq!(state(&on), (Public, false));
    let stored = store.live_track(&id("live1")).await.unwrap().unwrap();
    assert_eq!((stored.visibility, stored.landed), (Public, false));
}

pub(crate) async fn a_live_track_takes_no_more_fixes_than_it_has_room_for(store: &impl Store) {
    let dario = create(store, "dario").await;
    let five = LiveRoom {
        fixes_per_track: 5,
        ..room()
    };
    let save = |batch: LiveBatch| async move {
        match store.save_live_fixes(&batch, &five).await.unwrap() {
            LiveSaved::Kept(kept) => *kept,
            other => panic!("expected the batch to be kept, got {other:?}"),
        }
    };

    let begun = save(public(&dario, "live1", &[0, 1, 2])).await;
    assert_eq!((begun.stored, begun.full), (3, 0));
    // One it has and four it has not: the two oldest new ones fit.
    let filled = save(public(&dario, "live1", &[2, 3, 4, 5, 6])).await;
    assert_eq!((filled.stored, filled.known, filled.full), (2, 1, 2));
    assert_eq!(filled.track.fixes, 5);
    assert_eq!(filled.track.latest, fix(4));
    // Nothing more fits, and nothing changes.
    let refused = save(batch(&dario, "live1", &[7, 8], LiveVisibility::Link, true)).await;
    assert_eq!((refused.stored, refused.known, refused.full), (0, 0, 2));
    assert_eq!(refused.track, filled.track);
    assert_eq!(seconds(&all_fixes(store, "live1").await), [0, 1, 2, 3, 4]);
    // What it holds can still be said again, and other tracks have room.
    let again = save(public(&dario, "live1", &[3, 4])).await;
    assert_eq!((again.stored, again.known, again.full), (0, 2, 0));
    let other = save(public(&dario, "live2", &[0, 1, 2, 3, 4, 5, 6])).await;
    assert_eq!((other.stored, other.full), (5, 2));

    // A track too large to begin with begins with what fits.
    assert!(other.created);
    assert_eq!(other.track.latest, fix(4));
}

pub(crate) async fn only_recent_public_live_tracks_in_the_box_are_listed(store: &impl Store) {
    use LiveVisibility::{Link, Public};
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    // A fix at a place, at `second`.
    let at_place = |second: i64, lat: f64, lon: f64| LiveFix {
        position: GeoPoint::new(lat, lon).unwrap(),
        ..fix(second)
    };
    let track = |account: &Account, name: &str, visibility, fixes: Vec<LiveFix>| LiveBatch {
        fixes,
        ..batch(account, name, &[], visibility, false)
    };
    let tracks = [
        // In the box, public, heard of at second 100.
        track(
            &dario,
            "wallberg",
            Public,
            vec![at_place(100, 47.66, 11.80)],
        ),
        // The same for another pilot, exactly on the box's corner.
        track(&anna, "corner", Public, vec![at_place(100, 47.0, 11.0)]),
        // Only for those with the link.
        track(&dario, "secret", Link, vec![at_place(100, 47.66, 11.81)]),
        // Not heard of since second 10.
        track(&anna, "silent", Public, vec![at_place(10, 47.66, 11.82)]),
        // Began in the box and has flown out of it.
        track(
            &anna,
            "away",
            Public,
            vec![at_place(90, 47.66, 11.83), at_place(100, 48.5, 11.83)],
        ),
        // Began outside and has flown in.
        track(
            &dario,
            "arrived",
            Public,
            vec![at_place(90, 48.5, 11.84), at_place(100, 47.66, 11.84)],
        ),
    ];
    for track in &tracks {
        send(store, track).await;
    }
    let valley = BoundingBox::new(47.0, 11.0, 48.0, 12.0).unwrap();

    // Sorted by identifier, with the pilot of each.
    let found = store.live_tracks_in(&valley, sec(50), 100).await.unwrap();
    assert_eq!(names(&found), ["corner", "arrived", "wallberg"]);
    let pilots: Vec<&str> = found.iter().map(|t| t.pilot_handle.as_str()).collect();
    assert_eq!(pilots, ["anna", "dario", "dario"]);
    assert_eq!(
        found[2],
        store.live_track(&id("wallberg")).await.unwrap().unwrap()
    );
    assert!(found.iter().all(|track| track.visibility == Public));

    // The moment asked for is included; a longer memory finds the silent
    // one too, a shorter one nobody.
    assert_eq!(listed(store, &valley, sec(100)).await.len(), 3);
    assert_eq!(
        listed(store, &valley, sec(0)).await,
        ["corner", "silent", "arrived", "wallberg"]
    );
    assert!(listed(store, &valley, sec(101)).await.is_empty());
    // Elsewhere.
    let north = BoundingBox::new(48.2, 11.0, 49.0, 12.0).unwrap();
    assert_eq!(listed(store, &north, sec(50)).await, ["away"]);
    // The limit keeps the first.
    let first = store.live_tracks_in(&valley, sec(50), 2).await.unwrap();
    assert_eq!(names(&first), ["corner", "arrived"]);

    // A track its pilot no longer shows to everyone leaves the list, and
    // one that is opened joins it.
    let narrowed = track(&dario, "wallberg", Link, vec![at_place(101, 47.66, 11.80)]);
    send(store, &narrowed).await;
    let opened = track(&dario, "secret", Public, vec![at_place(101, 47.66, 11.81)]);
    send(store, &opened).await;
    assert_eq!(
        listed(store, &valley, sec(50)).await,
        ["corner", "secret", "arrived"]
    );
}

pub(crate) async fn a_live_track_belongs_to_the_account_that_began_it(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let begun = send(store, &public(&dario, "live1", &[0, 1])).await;

    // Anna has the link, and tries to send under it and to delete it.
    let taken = save(store, &public(&anna, "live1", &[2, 3])).await;
    assert_eq!(taken, LiveSaved::Taken);
    for may_retire in [false, true] {
        let removal = store
            .delete_live_track(&anna.id, &id("live1"), at(19, 0), may_retire)
            .await
            .unwrap();
        assert_eq!(removal, LiveRemoval::NotYours);
    }

    assert_eq!(
        store.live_track(&id("live1")).await.unwrap(),
        Some(begun.track)
    );
    assert_eq!(seconds(&all_fixes(store, "live1").await), [0, 1]);
}

pub(crate) async fn a_deleted_live_track_takes_nothing_more(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    send(store, &public(&dario, "live1", &[0, 1, 2])).await;
    send(store, &public(&dario, "live2", &[0, 1, 2])).await;
    let delete = |account: &Account| {
        let account_id = account.id.clone();
        async move {
            store
                .delete_live_track(&account_id, &id("live1"), at(19, 0), true)
                .await
                .unwrap()
        }
    };

    assert_eq!(delete(&dario).await, LiveRemoval::Removed);

    assert!(nothing_under(store, "live1").await);
    assert_eq!(listed(store, &alps(), sec(0)).await, ["live2"]);
    // Fixes that were on their way begin nothing, whoever sends them.
    assert_eq!(
        save(store, &public(&dario, "live1", &[3])).await,
        LiveSaved::Gone
    );
    assert_eq!(
        save(store, &public(&anna, "live1", &[3])).await,
        LiveSaved::Gone
    );
    assert!(nothing_under(store, "live1").await);
    // Deleting again is harmless, and still only the pilot's to do.
    assert_eq!(delete(&dario).await, LiveRemoval::AlreadyRemoved);
    assert_eq!(delete(&anna).await, LiveRemoval::NotYours);
    // The other track is untouched.
    assert_eq!(seconds(&all_fixes(store, "live2").await), [0, 1, 2]);
}

pub(crate) async fn an_identifier_can_be_retired_before_anything_arrived(store: &impl Store) {
    let dario = create(store, "dario").await;
    let delete = |track: &'static str, may_retire: bool| {
        let account_id = dario.id.clone();
        async move {
            store
                .delete_live_track(&account_id, &id(track), at(19, 0), may_retire)
                .await
        }
    };

    // The app switched live tracking off before its first batch arrived.
    assert_eq!(delete("early", true).await, Ok(LiveRemoval::Retired));
    assert_eq!(
        save(store, &public(&dario, "early", &[0])).await,
        LiveSaved::Gone
    );
    assert!(nothing_under(store, "early").await);
    assert_eq!(delete("early", true).await, Ok(LiveRemoval::AlreadyRemoved));
    assert_eq!(
        delete("early", false).await,
        Ok(LiveRemoval::AlreadyRemoved)
    );

    // Where that is not allowed, nothing is remembered.
    assert_eq!(delete("other", false).await, Ok(LiveRemoval::NotRetired));
    let begun = send(store, &public(&dario, "other", &[0])).await;
    assert!(begun.created);

    // Nobody takes an identifier out of use for an account that is not.
    let nobody = super::accounts::account("nobody");
    let result = store
        .delete_live_track(&nobody.id, &id("third"), at(19, 0), true)
        .await;
    assert!(
        matches!(result, Err(StoreError::Invalid(_))),
        "expected a refusal, got {result:?}"
    );
    assert!(send(store, &public(&dario, "third", &[0])).await.created);
}

pub(crate) async fn a_live_track_is_begun_only_where_that_is_allowed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let no_more = LiveRoom {
        may_create: false,
        ..room()
    };

    let refused = store
        .save_live_fixes(&public(&dario, "live1", &[0, 1]), &no_more)
        .await
        .unwrap();
    assert_eq!(refused, LiveSaved::NotCreated);
    assert!(nothing_under(store, "live1").await);

    // A track that exists takes fixes all the same.
    send(store, &public(&dario, "live1", &[0, 1])).await;
    let more = store
        .save_live_fixes(&public(&dario, "live1", &[2]), &no_more)
        .await
        .unwrap();
    assert!(matches!(more, LiveSaved::Kept(kept) if kept.stored == 1 && !kept.created));
}

pub(crate) async fn live_tracks_that_ended_long_ago_are_removed(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    // Three tracks, last heard of at seconds 10, 20 and 30.
    send(store, &public(&dario, "old", &[0, 10])).await;
    send(store, &public(&anna, "older", &[5, 20])).await;
    send(store, &public(&dario, "new", &[0, 30])).await;

    // A track that ended before the moment given is gone; one that ended
    // at that moment stays.
    assert_eq!(store.prune_live_tracks(sec(20), at(19, 0)).await, Ok(1));
    assert!(nothing_under(store, "old").await);
    assert_eq!(listed(store, &alps(), sec(0)).await, ["new", "older"]);
    assert_eq!(store.prune_live_tracks(sec(20), at(19, 0)).await, Ok(0));
    // Its identifier is used up, for its pilot and for everyone.
    assert_eq!(
        save(store, &public(&dario, "old", &[40])).await,
        LiveSaved::Gone
    );
    assert_eq!(
        save(store, &public(&anna, "old", &[40])).await,
        LiveSaved::Gone
    );
    let again = store
        .delete_live_track(&dario.id, &id("old"), at(19, 0), true)
        .await;
    assert_eq!(again, Ok(LiveRemoval::AlreadyRemoved));

    // A track that ended too long ago takes nothing more also before the
    // cleanup has come by, and is removed on the spot.
    let late = LiveRoom {
        expired_before: sec(25),
        ..room()
    };
    let saved = store
        .save_live_fixes(&public(&anna, "older", &[40]), &late)
        .await
        .unwrap();
    assert_eq!(saved, LiveSaved::Gone);
    assert!(nothing_under(store, "older").await);
    assert_eq!(
        save(store, &public(&anna, "older", &[41])).await,
        LiveSaved::Gone
    );
    // One that is young enough carries on.
    let saved = store
        .save_live_fixes(&public(&dario, "new", &[40]), &late)
        .await
        .unwrap();
    assert!(matches!(saved, LiveSaved::Kept(kept) if kept.track.fixes == 3));
    assert_eq!(store.prune_live_tracks(sec(41), at(19, 0)).await, Ok(1));
    assert!(listed(store, &alps(), sec(0)).await.is_empty());
}

pub(crate) async fn requests_racing_for_one_live_track_begin_it_once(store: &impl Store) {
    let dario = create(store, "dario").await;
    let same = public(&dario, "live1", &[0, 1, 2]);

    let (a, b, c, d) = tokio::join!(
        send(store, &same),
        send(store, &same),
        send(store, &same),
        send(store, &same),
    );

    let results = [a, b, c, d];
    assert_eq!(results.iter().filter(|kept| kept.created).count(), 1);
    assert_eq!(results.iter().map(|kept| kept.stored).sum::<u32>(), 3);
    assert_eq!(seconds(&all_fixes(store, "live1").await), [0, 1, 2]);

    // Different batches of one track, all at once: every fix is taken
    // once and gets a number of its own.
    let batches = [
        public(&dario, "live2", &[0, 1]),
        public(&dario, "live2", &[2, 3]),
        public(&dario, "live2", &[4, 5]),
        public(&dario, "live2", &[1, 2, 6]),
    ];
    let (a, b, c, d) = tokio::join!(
        send(store, &batches[0]),
        send(store, &batches[1]),
        send(store, &batches[2]),
        send(store, &batches[3]),
    );
    let results = [a, b, c, d];
    assert_eq!(results.iter().filter(|kept| kept.created).count(), 1);
    assert_eq!(results.iter().map(|kept| kept.stored).sum::<u32>(), 7);
    let fixes = all_fixes(store, "live2").await;
    let numbered: Vec<i64> = fixes.iter().map(|at| at.seq).collect();
    assert_eq!(numbered, [1, 2, 3, 4, 5, 6, 7]);
    let mut times = seconds(&fixes);
    times.sort_unstable();
    assert_eq!(times, [0, 1, 2, 3, 4, 5, 6]);
    let track = store.live_track(&id("live2")).await.unwrap().unwrap();
    assert_eq!((track.fixes, track.cursor, track.latest), (7, 7, fix(6)));
}

pub(crate) async fn a_deletion_and_a_first_batch_never_both_win(store: &impl Store) {
    let dario = create(store, "dario").await;
    for round in 0..8 {
        let track = format!("race{round}");
        let first = public(&dario, &track, &[0, 1]);
        let delete = store.delete_live_track(&dario.id, &first.id, at(19, 0), true);

        let (saved, removal) = tokio::join!(save(store, &first), delete);

        // Either the batch came first and its track was deleted, or the
        // deletion came first and the batch began nothing.
        match (&saved, removal.unwrap()) {
            (LiveSaved::Kept(kept), LiveRemoval::Removed) => assert!(kept.created),
            (LiveSaved::Gone, LiveRemoval::Retired) => {}
            other => panic!("round {round}: {other:?}"),
        }
        assert!(nothing_under(store, &track).await, "round {round}");
        assert_eq!(save(store, &first).await, LiveSaved::Gone, "round {round}");
    }
}

pub(crate) async fn a_bad_live_batch_is_refused_and_nothing_is_stored(store: &impl Store) {
    let dario = create(store, "dario").await;
    let nobody = super::accounts::account("nobody");
    let mut fraction = public(&dario, "live1", &[0, 1]);
    fraction.fixes[1].time += Duration::milliseconds(250);
    let bad = [
        ("no fix", public(&dario, "live1", &[])),
        ("newest first", public(&dario, "live1", &[2, 1])),
        ("a second twice", public(&dario, "live1", &[1, 1])),
        ("a fraction of a second", fraction),
        ("no such account", public(&nobody, "live1", &[0, 1])),
    ];
    for (what, batch) in &bad {
        let result = store.save_live_fixes(batch, &room()).await;
        assert!(
            matches!(result, Err(StoreError::Invalid(_))),
            "{what}: expected a refusal, got {result:?}"
        );
    }
    let no_room = LiveRoom {
        fixes_per_track: 0,
        ..room()
    };
    let result = store
        .save_live_fixes(&public(&dario, "live1", &[0]), &no_room)
        .await;
    assert!(matches!(result, Err(StoreError::Invalid(_))), "{result:?}");
    assert!(nothing_under(store, "live1").await);

    // The same for a track that exists: it stays as it was.
    let begun = send(store, &public(&dario, "live1", &[0, 1])).await;
    for (_, batch) in &bad[..4] {
        assert!(store.save_live_fixes(batch, &room()).await.is_err());
    }
    assert_eq!(
        store.live_track(&id("live1")).await.unwrap(),
        Some(begun.track)
    );
}

pub(crate) async fn no_live_track_has_an_identifier_that_cannot_be_one(store: &impl Store) {
    let dario = create(store, "dario").await;
    send(store, &public(&dario, "live1", &[0, 1])).await;

    // PostgreSQL takes no zero byte in text; no identifier holds one.
    for bad in ["\0", "live\0one", "", "'; DROP TABLE live_tracks;--"] {
        assert_eq!(store.live_track(bad).await, Ok(None), "{bad:?}");
        assert_eq!(store.live_fixes(bad, 0, None, 10).await, Ok(Vec::new()));
        assert_eq!(store.live_tail(bad, 10).await, Ok(Vec::new()));
    }
    let mut batch = public(&dario, "live2", &[0]);
    batch.id = "live\0two".to_owned();
    let saved = store.save_live_fixes(&batch, &room()).await;
    assert!(matches!(saved, Err(StoreError::Invalid(_))), "{saved:?}");
    let removed = store
        .delete_live_track(&dario.id, "live\0one", at(19, 0), true)
        .await;
    assert!(
        matches!(removed, Err(StoreError::Invalid(_))),
        "{removed:?}"
    );

    assert_eq!(seconds(&all_fixes(store, "live1").await), [0, 1]);
}

/// The names of the two live tracks [`begin_live_tracks_for`] makes for an
/// account: one that stays and one that is deleted.
fn tracks_of(account: &Account) -> [String; 2] {
    [
        format!("{}stays", account.handle),
        format!("{}goes", account.handle),
    ]
}

/// Gives an account a live track and an identifier that takes nothing
/// more, for the behaviour that deletes an account.
pub(crate) async fn begin_live_tracks_for(store: &impl Store, account: &Account) {
    let [stays, goes] = tracks_of(account);
    send(store, &public(account, &stays, &[0, 1, 2])).await;
    send(store, &public(account, &goes, &[0, 1, 2])).await;
    let removal = store
        .delete_live_track(&account.id, &id(&goes), at(19, 0), true)
        .await
        .unwrap();
    assert_eq!(removal, LiveRemoval::Removed);
}

/// Whether an account still has what [`begin_live_tracks_for`] gave it.
pub(crate) async fn has_its_live_tracks(store: &impl Store, account: &Account) -> bool {
    let [stays, goes] = tracks_of(account);
    seconds(&all_fixes(store, &stays).await) == [0, 1, 2]
        && save(store, &public(account, &goes, &[3])).await == LiveSaved::Gone
}

/// Whether nothing is left of what [`begin_live_tracks_for`] gave an
/// account that has been deleted since: no track, and no memory of the
/// identifiers, under which `other` can begin tracks of its own.
pub(crate) async fn nothing_is_left_of_the_live_tracks_of(
    store: &impl Store,
    deleted: &Account,
    other: &Account,
) -> bool {
    let tracks = tracks_of(deleted);
    for track in &tracks {
        if !nothing_under(store, track).await {
            return false;
        }
    }
    for track in &tracks {
        let saved = save(store, &public(other, track, &[0])).await;
        if !matches!(saved, LiveSaved::Kept(kept) if kept.created) {
            return false;
        }
    }
    true
}
