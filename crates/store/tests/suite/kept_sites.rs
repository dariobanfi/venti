//! What every store must do with the sites a pilot keeps as their own.

use chrono::Duration;
use venti_domain::Timestamp;
use venti_store::{
    Account, ForgottenSite, KeepingSite, KeptRoom, KeptSite, KeptSiteChanges, NewKeptSite, Store,
};

use super::accounts::{create, id};
use super::at;

const ROOM: KeptRoom = KeptRoom {
    sites: 3,
    forgotten: 2,
};

fn site(name: &str, kept_at: Option<Timestamp>) -> NewKeptSite {
    NewKeptSite {
        site_id: format!("osm-free-flying:node/{name}"),
        source: "osm-free-flying".to_owned(),
        kept_at,
        record: format!(r#"{{"site":{{"id":"osm-free-flying:node/{name}"}}}}"#),
    }
}

async fn keep(
    store: &impl Store,
    account: &Account,
    new: &NewKeptSite,
    now: Timestamp,
) -> KeptSite {
    match store.keep_site(&account.id, new, now, ROOM).await.unwrap() {
        KeepingSite::Kept(kept) => kept,
        other => panic!("expected the site to be kept, got {other:?}"),
    }
}

async fn changes(store: &impl Store, account: &Account, since: i64) -> KeptSiteChanges {
    store
        .kept_site_changes(&account.id, since, 100)
        .await
        .unwrap()
}

/// The last part of a site's identifier.
fn last(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

fn names(changes: &KeptSiteChanges) -> (Vec<&str>, Vec<&str>) {
    (
        changes
            .kept
            .iter()
            .map(|kept| last(&kept.site_id))
            .collect(),
        changes
            .forgotten
            .iter()
            .map(|forgotten| last(&forgotten.site_id))
            .collect(),
    )
}

pub(crate) async fn a_kept_site_is_read_back_and_the_newer_note_wins(store: &impl Store) {
    let dario = create(store, "dario").await;
    assert_eq!(changes(store, &dario, 0).await, KeptSiteChanges::default());

    // Kept with the moment the device noted; without one, with the
    // moment it arrived.
    let noted = keep(store, &dario, &site("1", Some(at(8, 0))), at(10, 0)).await;
    assert_eq!(noted.kept_at, at(8, 0));
    assert_eq!(noted.record, site("1", None).record);
    assert_eq!(noted.source, "osm-free-flying");
    let unnoted = keep(store, &dario, &site("2", None), at(10, 1)).await;
    assert_eq!(unnoted.kept_at, at(10, 1));
    assert!(unnoted.change > noted.change);

    // An older note, the same note and no note leave what the account
    // has, and take no change number.
    let older = NewKeptSite {
        record: "older".to_owned(),
        ..site("1", Some(at(7, 0)))
    };
    assert_eq!(keep(store, &dario, &older, at(10, 2)).await, noted);
    let same = NewKeptSite {
        record: "same".to_owned(),
        ..site("1", Some(at(8, 0)))
    };
    assert_eq!(keep(store, &dario, &same, at(10, 2)).await, noted);
    let without = NewKeptSite {
        record: "without".to_owned(),
        ..site("1", None)
    };
    assert_eq!(keep(store, &dario, &without, at(10, 2)).await, noted);
    let all = changes(store, &dario, 0).await;
    assert_eq!(all.kept, [noted.clone(), unnoted.clone()]);
    assert_eq!(all.latest, unnoted.change);
    assert_eq!((all.more, all.floor), (false, 0));

    // A newer note replaces it. A note from a clock that runs ahead
    // counts as now.
    let newer = NewKeptSite {
        record: "newer".to_owned(),
        ..site("1", Some(at(9, 0)))
    };
    let replaced = keep(store, &dario, &newer, at(10, 3)).await;
    assert_eq!(
        (replaced.kept_at, replaced.record.as_str()),
        (at(9, 0), "newer")
    );
    let ahead = NewKeptSite {
        record: "ahead".to_owned(),
        ..site("1", Some(at(23, 0)))
    };
    let clamped = keep(store, &dario, &ahead, at(10, 4)).await;
    assert_eq!(
        (clamped.kept_at, clamped.record.as_str()),
        (at(10, 4), "ahead")
    );

    // What changed since is each site once, as it is now.
    let since = changes(store, &dario, unnoted.change).await;
    assert_eq!(since.kept, std::slice::from_ref(&clamped));
    assert_eq!(names(&changes(store, &dario, 0).await).0, ["2", "1"]);
    assert_eq!(changes(store, &dario, clamped.change).await.kept, []);
}

pub(crate) async fn a_forgotten_site_is_told_to_the_other_devices(store: &impl Store) {
    let dario = create(store, "dario").await;
    let one = keep(store, &dario, &site("1", None), at(10, 0)).await;
    let two = keep(store, &dario, &site("2", None), at(10, 1)).await;

    // Forgetting what is not kept changes nothing.
    let unknown = "osm-free-flying:node/9";
    assert!(
        !store
            .forget_kept_site(&dario.id, unknown, at(10, 2), ROOM)
            .await
            .unwrap()
    );
    assert!(
        !store
            .forget_kept_site(&id("nobody"), &one.site_id, at(10, 2), ROOM)
            .await
            .unwrap()
    );
    assert_eq!(changes(store, &dario, 0).await.latest, two.change);

    assert!(
        store
            .forget_kept_site(&dario.id, &one.site_id, at(10, 3), ROOM)
            .await
            .unwrap()
    );
    assert!(
        !store
            .forget_kept_site(&dario.id, &one.site_id, at(10, 4), ROOM)
            .await
            .unwrap()
    );
    let since = changes(store, &dario, two.change).await;
    assert_eq!(since.kept, []);
    assert_eq!(since.forgotten.len(), 1);
    assert_eq!(since.forgotten[0].site_id, one.site_id);
    assert_eq!(since.latest, since.forgotten[0].change);
    assert_eq!(
        names(&changes(store, &dario, 0).await),
        (vec!["2"], vec!["1"])
    );

    // Kept again, it is kept, whatever its note says: keeping wins over
    // forgetting.
    let again = keep(store, &dario, &site("1", Some(at(7, 0))), at(10, 5)).await;
    assert_eq!(again.kept_at, at(7, 0));
    assert_eq!(
        names(&changes(store, &dario, 0).await),
        (vec!["2", "1"], vec![])
    );

    // Changes come page by page, in the order they were made.
    let first = store.kept_site_changes(&dario.id, 0, 1).await.unwrap();
    assert_eq!((names(&first).0, first.more), (vec!["2"], true));
    let rest = store
        .kept_site_changes(&dario.id, first.last().unwrap(), 1)
        .await
        .unwrap();
    assert_eq!((names(&rest).0, rest.more), (vec!["1"], false));
}

pub(crate) async fn an_account_keeps_so_many_sites_and_no_more(store: &impl Store) {
    let dario = create(store, "dario").await;
    for (minute, name) in ["1", "2", "3"].into_iter().enumerate() {
        let now = at(10, 0) + Duration::minutes(i64::try_from(minute).unwrap());
        keep(store, &dario, &site(name, None), now).await;
    }
    let full = store
        .keep_site(&dario.id, &site("4", None), at(11, 0), ROOM)
        .await;
    assert_eq!(full.unwrap(), KeepingSite::Full);
    // One that is kept already is kept anew, also by a full account.
    let anew = keep(store, &dario, &site("2", Some(at(11, 1))), at(11, 2)).await;
    assert_eq!(anew.kept_at, at(11, 1));
    // An account that is not there keeps nothing.
    let nobody = store
        .keep_site(&id("nobody"), &site("1", None), at(11, 0), ROOM)
        .await;
    assert_eq!(nobody.unwrap(), KeepingSite::NoAccount);

    // Forgetting makes room. Of the forgotten the two newest are
    // remembered, and the floor says up to where the rest was dropped.
    let mut dropped = 0;
    for (minute, name) in ["1", "2", "3"].into_iter().enumerate() {
        let now = at(12, 0) + Duration::minutes(i64::try_from(minute).unwrap());
        let site_id = site(name, None).site_id;
        if name == "1" {
            let before = changes(store, &dario, 0).await.latest;
            assert!(
                store
                    .forget_kept_site(&dario.id, &site_id, now, ROOM)
                    .await
                    .unwrap()
            );
            dropped = changes(store, &dario, before).await.forgotten[0].change;
        } else {
            assert!(
                store
                    .forget_kept_site(&dario.id, &site_id, now, ROOM)
                    .await
                    .unwrap()
            );
        }
    }
    let all = changes(store, &dario, 0).await;
    assert_eq!(names(&all), (vec![], vec!["2", "3"]));
    assert_eq!(all.floor, dropped);
    keep(store, &dario, &site("4", None), at(13, 0)).await;
    assert_eq!(
        names(&changes(store, &dario, 0).await),
        (vec!["4"], vec!["2", "3"])
    );
}

pub(crate) async fn kept_sites_are_an_accounts_own_and_go_with_it(store: &impl Store) {
    let dario = create(store, "dario").await;
    let anna = create(store, "anna").await;
    let his = keep(store, &dario, &site("1", None), at(10, 0)).await;
    let hers = keep(store, &anna, &site("1", Some(at(9, 0))), at(10, 1)).await;
    keep(store, &anna, &site("2", None), at(10, 2)).await;

    // The same site, kept by two, is two rows that know nothing of each
    // other.
    assert_eq!(
        changes(store, &dario, 0).await.kept,
        std::slice::from_ref(&his)
    );
    assert_eq!(names(&changes(store, &anna, 0).await).0, ["1", "2"]);
    assert!(
        store
            .forget_kept_site(&dario.id, &his.site_id, at(10, 3), ROOM)
            .await
            .unwrap()
    );
    assert_eq!(changes(store, &anna, 0).await.kept[0], hers);

    // The list goes with its account, and with no other.
    store.delete_account(&anna.id).await.unwrap();
    assert_eq!(changes(store, &anna, 0).await, KeptSiteChanges::default());
    let left = changes(store, &dario, 0).await;
    assert_eq!(
        left.forgotten,
        [ForgottenSite {
            site_id: his.site_id.clone(),
            change: left.latest,
        }]
    );
}
