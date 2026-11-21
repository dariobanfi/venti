//! The behaviour every [`Store`] must have for the flying sites that
//! pilots make: a record with an identifier that is used once, every
//! change a revision that is kept, a removal that is a revision too, a
//! distance that standing sites keep from each other, and a count of
//! changes that a mirror can follow.

use chrono::{TimeZone, Utc};
use venti_domain::{
    BoundingBox, GeoPoint, Site, SiteId, SiteOutline, SiteUse, Timestamp, WindSector,
};
use venti_store::{
    Account, NewPilotSite, PilotSite, PilotSiteChanges, RevisionKind, SiteAct, SiteDelivery,
    SitePilot, SiteRevising, SiteRevision, SiteVia, SiteWrite, Store, StoreError,
};

use super::accounts::{create, id};
use super::{alps, at, everything, run};

// --- Helpers -------------------------------------------------------------

/// How near standing sites may not be to each other in these tests, in
/// metres. A ten-thousandth of a degree of latitude is about 11 m.
const APART_M: f64 = 20.0;

fn site_id(name: &str) -> SiteId {
    format!("venti:{}", id(name)).parse().unwrap()
}

/// A take-off that a pilot made, under an identifier that ends in `name`.
fn record(name: &str, lat: f64, lon: f64) -> Site {
    Site {
        id: site_id(name),
        name: Some(format!("Musterberg {name}")),
        position: GeoPoint::new(lat, lon).unwrap(),
        elevation_m: Some(1500.0),
        uses: vec![SiteUse::Takeoff],
        wind_directions: Vec::new(),
        paragliding: None,
        hang_gliding: None,
        official: None,
        discouraged: false,
        url: None,
        website: None,
        description: None,
        fee: None,
        guest_guidelines: None,
        no_flight_time: None,
        outline: None,
    }
}

fn new_site(site: Site, author: &Account, at: Timestamp) -> NewPilotSite {
    NewPilotSite {
        site,
        author_id: author.id.clone(),
        at,
        via: SiteVia::Session,
        comment: None,
    }
}

/// Adds a site, which nothing stands in the way of.
async fn add(store: &impl Store, site: Site, author: &Account, at: Timestamp) -> PilotSite {
    let new = new_site(site, author, at);
    match store.add_pilot_site(&new, APART_M).await.unwrap() {
        SiteWrite::Written(site) => *site,
        other => panic!("the site was not added: {other:?}"),
    }
}

/// A revision by a pilot, through a session.
fn by(author: &Account, name: &str, at: Timestamp, act: SiteAct) -> SiteRevising {
    SiteRevising {
        id: site_id(name),
        author_id: Some(author.id.clone()),
        at,
        via: SiteVia::Session,
        act,
        comment: None,
        expects: None,
    }
}

/// A revision by whoever runs the server.
fn by_operator(name: &str, at: Timestamp, act: SiteAct) -> SiteRevising {
    SiteRevising {
        id: site_id(name),
        author_id: None,
        at,
        via: SiteVia::Operator,
        act,
        comment: None,
        expects: None,
    }
}

async fn revise(store: &impl Store, revising: &SiteRevising) -> SiteWrite {
    store.revise_pilot_site(revising, APART_M).await.unwrap()
}

/// Writes a revision, which nothing stands in the way of.
async fn written(store: &impl Store, revising: &SiteRevising) -> PilotSite {
    match revise(store, revising).await {
        SiteWrite::Written(site) => *site,
        other => panic!("the revision was not written: {other:?}"),
    }
}

fn change(site: Site) -> SiteAct {
    SiteAct::Change(Box::new(site))
}

const REMOVE: SiteAct = SiteAct::Remove { take_down: false };
const TAKE_DOWN: SiteAct = SiteAct::Remove { take_down: true };

fn pilot(account: &Account) -> Option<SitePilot> {
    Some(SitePilot {
        account_id: account.id.clone(),
        handle: account.handle.clone(),
        display_name: account.display_name.clone(),
    })
}

async fn read(store: &impl Store, name: &str) -> PilotSite {
    store
        .pilot_site(&site_id(name))
        .await
        .unwrap()
        .expect("the site")
}

/// The whole history of a site.
async fn history(store: &impl Store, name: &str) -> Vec<SiteRevision> {
    let (_, revisions) = store
        .pilot_site_revisions(&site_id(name), 0, 1000)
        .await
        .unwrap()
        .expect("the site");
    revisions
}

fn short(id: &SiteId) -> String {
    id.local().trim_start_matches('0').to_owned()
}

/// The standing sites in a box, by the ends of their identifiers.
async fn standing_in(store: &impl Store, area: &BoundingBox) -> Vec<String> {
    let sites = store.pilot_sites_in(area, 1000).await.unwrap();
    sites.iter().map(|site| short(&site.site.id)).collect()
}

/// What changed after a change number: the ends of the identifiers with
/// whether each site stands, whether more is waiting, and the latest
/// number.
async fn changes(store: &impl Store, since: i64, limit: usize) -> (Vec<(String, bool)>, bool, i64) {
    let page = store.pilot_site_changes(since, limit).await.unwrap();
    let sites = page
        .sites
        .iter()
        .map(|site| (short(&site.site.id), site.stands()))
        .collect();
    (sites, page.more, page.latest)
}

fn names(sites: &[(&str, bool)]) -> Vec<(String, bool)> {
    sites
        .iter()
        .map(|(name, stands)| ((*name).to_owned(), *stands))
        .collect()
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn a_site_a_pilot_made_is_read_back_with_its_author_and_times(store: &impl Store) {
    let anna = create(store, "anna").await;
    // Everything a pilot can say, with awkward decimals and words outside
    // ASCII.
    let full = Site {
        name: Some("Musterberg \"Ost\" / Gipfelwiese".to_owned()),
        position: GeoPoint::new(47.679_100_000_000_004, 11.905_1 + 0.1 + 0.2).unwrap(),
        elevation_m: Some(1560.5),
        uses: vec![SiteUse::Takeoff, SiteUse::Landing],
        wind_directions: vec![
            WindSector::single(292.5),
            WindSector {
                from_deg: 315.0,
                to_deg: 45.0,
            },
        ],
        paragliding: Some(true),
        hang_gliding: Some(false),
        website: Some("https://example.org/fluggebiet?platz=ost".to_owned()),
        description: Some("Ausgedacht.\nWindsack \"Ost\" für Gäste.".to_owned()),
        fee: Some("no".to_owned()),
        guest_guidelines: Some(true),
        no_flight_time: Some("01.04-30.09".to_owned()),
        outline: Some(
            SiteOutline::new(
                [
                    (47.678_844_5, 11.904_653_3),
                    (47.679_438_2, 11.904_9 + 0.1 + 0.2 - 0.3),
                    (47.679_1, 11.905_553_2),
                    (47.678_844_5, 11.904_653_3),
                ]
                .into_iter()
                .map(|(lat, lon)| GeoPoint::new(lat, lon).unwrap())
                .collect(),
            )
            .unwrap(),
        ),
        ..record("a", 0.0, 0.0)
    };
    let new = NewPilotSite {
        comment: Some("Vom eigenen Flug".to_owned()),
        ..new_site(full.clone(), &anna, at(10, 0))
    };
    let added = store.add_pilot_site(&new, APART_M).await.unwrap();

    let expected = PilotSite {
        site: full.clone(),
        revision: 1,
        created_at: at(10, 0),
        created_by: pilot(&anna),
        changed_at: at(10, 0),
        changed_by: pilot(&anna),
        removed_at: None,
        taken_down: false,
        change: 1,
    };
    assert_eq!(added, SiteWrite::Written(Box::new(expected.clone())));
    assert_eq!(read(store, "a").await, expected);
    assert!(expected.stands());
    assert_eq!(
        store.pilot_sites_in(&alps(), 10).await.unwrap(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(
        history(store, "a").await,
        [SiteRevision {
            number: 1,
            at: at(10, 0),
            author: pilot(&anna),
            via: SiteVia::Session,
            kind: RevisionKind::Created,
            comment: Some("Vom eigenen Flug".to_owned()),
            record: full,
        }]
    );

    // A site nobody made is none, and so is its history.
    assert_eq!(store.pilot_site(&site_id("b")).await.unwrap(), None);
    assert_eq!(
        store
            .pilot_site_revisions(&site_id("b"), 0, 10)
            .await
            .unwrap(),
        None
    );
    // A site of an account that does not exist is not added.
    let ghost = Account {
        id: id("ghost"),
        ..anna.clone()
    };
    let nobody = new_site(record("b", 47.1, 11.1), &ghost, at(10, 5));
    assert_eq!(
        store.add_pilot_site(&nobody, APART_M).await.unwrap(),
        SiteWrite::NoSuchAccount
    );
    assert_eq!(store.pilot_site(&site_id("b")).await.unwrap(), None);
    assert_eq!(
        changes(store, 0, 10).await,
        (names(&[("a", true)]), false, 1)
    );
}

pub(crate) async fn a_change_to_a_site_is_a_new_revision_and_the_old_ones_stay(store: &impl Store) {
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let first = record("a", 47.0, 11.0);
    add(store, first.clone(), &anna, at(10, 0)).await;

    // Another pilot corrects it, from the revision they read.
    let second = Site {
        name: Some("Musterberg Nord".to_owned()),
        elevation_m: Some(1512.0),
        ..first.clone()
    };
    let correcting = SiteRevising {
        comment: Some("Höhe vom Vario".to_owned()),
        expects: Some(1),
        ..by(&bernd, "a", at(11, 0), change(second.clone()))
    };
    let changed = written(store, &correcting).await;
    let expected = PilotSite {
        site: second.clone(),
        revision: 2,
        created_at: at(10, 0),
        created_by: pilot(&anna),
        changed_at: at(11, 0),
        changed_by: pilot(&bernd),
        removed_at: None,
        taken_down: false,
        change: 2,
    };
    assert_eq!(changed, expected);
    assert_eq!(read(store, "a").await, expected);

    // A change made from the revision before is not written.
    let third = Site {
        name: Some("Musterberg Süd".to_owned()),
        ..first.clone()
    };
    let stale = SiteRevising {
        expects: Some(1),
        ..by(&anna, "a", at(11, 30), change(third.clone()))
    };
    assert_eq!(revise(store, &stale).await, SiteWrite::Stale { current: 2 });
    // Nor is one that changes nothing; it answers the site as it is.
    let same = by(&anna, "a", at(11, 40), change(second.clone()));
    assert_eq!(
        revise(store, &same).await,
        SiteWrite::Same(Box::new(expected.clone()))
    );
    assert_eq!(read(store, "a").await, expected);
    assert_eq!(
        changes(store, 0, 10).await,
        (names(&[("a", true)]), false, 2)
    );

    // Whoever runs the server changes a site as nobody.
    let fixed = written(store, &by_operator("a", at(12, 0), change(third.clone()))).await;
    assert_eq!(
        (fixed.revision, &fixed.changed_by, fixed.changed_at),
        (3, &None, at(12, 0))
    );
    assert_eq!(fixed.created_by, pilot(&anna));

    // An undo is a revision that restores an earlier one and names it.
    let undone = written(store, &by(&anna, "a", at(12, 30), SiteAct::Restore(1))).await;
    assert_eq!((undone.revision, &undone.site), (4, &first));
    // Restoring what the site says already writes nothing.
    assert_eq!(
        revise(store, &by(&bernd, "a", at(12, 40), SiteAct::Restore(1))).await,
        SiteWrite::Same(Box::new(undone.clone()))
    );
    for number in [0, 5, u32::MAX] {
        assert_eq!(
            revise(store, &by(&anna, "a", at(12, 50), SiteAct::Restore(number))).await,
            SiteWrite::NoSuchRevision,
            "revision {number}"
        );
    }

    // Every state is still there, each with who made it.
    let kept: Vec<_> = history(store, "a")
        .await
        .into_iter()
        .map(|revision| {
            let author = revision.author.map(|pilot| pilot.handle);
            (
                revision.number,
                revision.kind,
                author,
                revision.via,
                revision.record,
            )
        })
        .collect();
    let (session, operator) = (SiteVia::Session, SiteVia::Operator);
    let name = |handle: &str| Some(handle.to_owned());
    assert_eq!(
        kept,
        [
            (
                1,
                RevisionKind::Created,
                name("anna"),
                session,
                first.clone()
            ),
            (2, RevisionKind::Changed, name("bernd"), session, second),
            (3, RevisionKind::Changed, None, operator, third),
            (4, RevisionKind::Restored(1), name("anna"), session, first),
        ]
    );
    assert_eq!(
        history(store, "a").await[1].comment.as_deref(),
        Some("Höhe vom Vario")
    );

    // A site that is not there, and a pilot who is not, change nothing.
    let nowhere = by(&anna, "x", at(13, 0), REMOVE);
    assert_eq!(revise(store, &nowhere).await, SiteWrite::NoSuchSite);
    let ghost = Account {
        id: id("ghost"),
        ..anna
    };
    assert_eq!(
        revise(store, &by(&ghost, "a", at(13, 0), REMOVE)).await,
        SiteWrite::NoSuchAccount
    );
    assert_eq!(read(store, "a").await, undone);
}

pub(crate) async fn a_removed_site_keeps_its_address_and_can_be_restored(store: &impl Store) {
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let first = record("a", 47.0, 11.0);
    add(store, first.clone(), &anna, at(10, 0)).await;
    add(store, record("b", 47.1, 11.1), &anna, at(10, 1)).await;

    let removed = written(store, &by(&bernd, "a", at(11, 0), REMOVE)).await;
    assert_eq!(
        removed,
        PilotSite {
            site: first.clone(),
            revision: 2,
            created_at: at(10, 0),
            created_by: pilot(&anna),
            changed_at: at(11, 0),
            changed_by: pilot(&bernd),
            removed_at: Some(at(11, 0)),
            taken_down: false,
            change: 3,
        }
    );
    assert!(!removed.stands());
    // It is in no box, and still answers under its identifier, with its
    // history.
    assert_eq!(standing_in(store, &everything()).await, ["b"]);
    assert_eq!(read(store, "a").await, removed);
    let kinds: Vec<_> = history(store, "a")
        .await
        .into_iter()
        .map(|revision| (revision.kind, revision.record))
        .collect();
    assert_eq!(
        kinds,
        [
            (RevisionKind::Created, first.clone()),
            (RevisionKind::Removed, first.clone()),
        ]
    );

    // There is nothing to change or to remove.
    let moved = record("a", 47.2, 11.2);
    assert_eq!(
        revise(store, &by(&anna, "a", at(11, 10), change(moved))).await,
        SiteWrite::Removed
    );
    assert_eq!(
        revise(store, &by(&anna, "a", at(11, 20), REMOVE)).await,
        SiteWrite::Removed
    );
    assert_eq!(read(store, "a").await, removed);

    // A restore brings it back, as a revision of its own.
    let back = written(store, &by(&anna, "a", at(12, 0), SiteAct::Restore(1))).await;
    assert_eq!(
        (back.revision, back.removed_at, back.taken_down, back.change),
        (3, None, false, 4)
    );
    assert_eq!((&back.site, &back.changed_by), (&first, &pilot(&anna)));
    assert_eq!(standing_in(store, &everything()).await, ["a", "b"]);
    assert_eq!(history(store, "a").await[2].kind, RevisionKind::Restored(1));
}

pub(crate) async fn a_site_taken_down_comes_back_only_by_the_operator(store: &impl Store) {
    let anna = create(store, "anna").await;
    let first = record("a", 47.0, 11.0);
    add(store, first.clone(), &anna, at(10, 0)).await;

    let down = written(store, &by_operator("a", at(11, 0), TAKE_DOWN)).await;
    assert_eq!(
        (
            down.revision,
            down.removed_at,
            down.taken_down,
            &down.changed_by
        ),
        (2, Some(at(11, 0)), true, &None)
    );
    assert_eq!(standing_in(store, &everything()).await, [""; 0]);
    // No pilot brings it back, changes it or removes it.
    for act in [SiteAct::Restore(1), change(record("a", 47.2, 11.2)), REMOVE] {
        assert_eq!(
            revise(store, &by(&anna, "a", at(11, 30), act)).await,
            SiteWrite::TakenDown
        );
    }
    // Taking it down again is nothing to do.
    assert_eq!(
        revise(store, &by_operator("a", at(11, 40), TAKE_DOWN)).await,
        SiteWrite::Removed
    );
    assert_eq!(read(store, "a").await, down);

    // Whoever runs the server restores a revision, and the site stands.
    let back = written(store, &by_operator("a", at(12, 0), SiteAct::Restore(1))).await;
    assert_eq!(
        (back.revision, back.removed_at, back.taken_down),
        (3, None, false)
    );
    assert_eq!(standing_in(store, &everything()).await, ["a"]);

    // A site a pilot removed is taken down once more, so that it stays
    // away.
    written(store, &by(&anna, "a", at(13, 0), REMOVE)).await;
    let down = written(store, &by_operator("a", at(13, 30), TAKE_DOWN)).await;
    assert_eq!(
        (down.revision, down.removed_at, down.taken_down),
        (5, Some(at(13, 30)), true)
    );
    assert_eq!(
        revise(store, &by(&anna, "a", at(14, 0), SiteAct::Restore(1))).await,
        SiteWrite::TakenDown
    );
    let via: Vec<_> = history(store, "a")
        .await
        .into_iter()
        .map(|revision| (revision.kind, revision.via))
        .collect();
    let (session, operator) = (SiteVia::Session, SiteVia::Operator);
    assert_eq!(
        via,
        [
            (RevisionKind::Created, session),
            (RevisionKind::Removed, operator),
            (RevisionKind::Restored(1), operator),
            (RevisionKind::Removed, session),
            (RevisionKind::Removed, operator),
        ]
    );
}

pub(crate) async fn standing_sites_keep_their_distance_from_each_other(store: &impl Store) {
    let anna = create(store, "anna").await;
    add(store, record("a", 47.0, 11.0), &anna, at(10, 0)).await;

    // About 11 m to the north: too near. About 33 m: far enough.
    let near = new_site(record("b", 47.0001, 11.0), &anna, at(10, 5));
    assert_eq!(
        store.add_pilot_site(&near, APART_M).await.unwrap(),
        SiteWrite::TooClose(site_id("a"))
    );
    assert_eq!(store.pilot_site(&site_id("b")).await.unwrap(), None);
    // The distance is the caller's: with a shorter one the place is free.
    let nearer = new_site(record("n", 47.0001, 11.0), &anna, at(10, 6));
    assert!(matches!(
        store.add_pilot_site(&nearer, 5.0).await.unwrap(),
        SiteWrite::Written(_)
    ));
    written(store, &by(&anna, "n", at(10, 7), REMOVE)).await;
    add(store, record("c", 47.0003, 11.0), &anna, at(10, 10)).await;
    // Of two that are too near, the nearer one is named: about 7 m east
    // of a, and 34 m from c.
    let between = new_site(record("d", 47.0, 11.0001), &anna, at(10, 15));
    assert_eq!(
        store.add_pilot_site(&between, 50.0).await.unwrap(),
        SiteWrite::TooClose(site_id("a"))
    );

    // A site that moves is held to the same distance, and may stay where
    // it is: it is not too near to itself.
    let onto_a = by(&anna, "c", at(11, 0), change(record("c", 47.000_05, 11.0)));
    assert_eq!(
        revise(store, &onto_a).await,
        SiteWrite::TooClose(site_id("a"))
    );
    let renamed = Site {
        name: Some("Musterberg Gipfel".to_owned()),
        ..record("c", 47.0003, 11.0)
    };
    written(store, &by(&anna, "c", at(11, 5), change(renamed))).await;
    assert_eq!(read(store, "c").await.revision, 2);

    // A removed site holds no place.
    written(store, &by(&anna, "a", at(12, 0), REMOVE)).await;
    add(store, record("e", 47.0, 11.0), &anna, at(12, 5)).await;
    // And does not come back onto one that is taken meanwhile.
    assert_eq!(
        revise(store, &by(&anna, "a", at(12, 10), SiteAct::Restore(1))).await,
        SiteWrite::TooClose(site_id("e"))
    );
    assert!(!read(store, "a").await.stands());
    assert_eq!(standing_in(store, &everything()).await, ["c", "e"]);
    // What was refused took no change number: n and its removal, c and
    // its change, a's removal, e.
    assert_eq!(store.pilot_site_changes(0, 100).await.unwrap().latest, 7);
}

pub(crate) async fn adds_racing_for_one_place_put_one_site_there(store: &impl Store) {
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    for round in 0..8_u32 {
        let lat = 47.0 + f64::from(round) * 0.01;
        let (a, b) = (format!("a{round}"), format!("b{round}"));
        let first = new_site(record(&a, lat, 11.0), &anna, at(10, round));
        let second = new_site(record(&b, lat + 0.000_05, 11.0), &bernd, at(10, round));
        let (one, other) = tokio::join!(
            store.add_pilot_site(&first, APART_M),
            store.add_pilot_site(&second, APART_M)
        );
        let (one, other) = (one.unwrap(), other.unwrap());
        match (&one, &other) {
            (SiteWrite::Written(_), SiteWrite::TooClose(near)) => assert_eq!(near, &site_id(&a)),
            (SiteWrite::TooClose(near), SiteWrite::Written(_)) => assert_eq!(near, &site_id(&b)),
            _ => panic!("round {round}: {one:?} and {other:?}"),
        }
    }
    // Eight sites, and eight change numbers without a gap.
    let (sites, more, latest) = changes(store, 0, 100).await;
    assert_eq!((sites.len(), more, latest), (8, false, 8));
    assert_eq!(
        store
            .pilot_sites_in(&everything(), 100)
            .await
            .unwrap()
            .len(),
        8
    );
}

pub(crate) async fn the_history_of_a_site_is_read_page_by_page(store: &impl Store) {
    let anna = create(store, "anna").await;
    let first = record("a", 47.0, 11.0);
    add(store, first.clone(), &anna, at(10, 0)).await;
    for step in 1..=4_u32 {
        let next = Site {
            elevation_m: Some(1500.0 + f64::from(step)),
            ..first.clone()
        };
        written(store, &by(&anna, "a", at(10, step), change(next))).await;
    }

    let page = |after: u32, limit: usize| async move {
        let (site, revisions) = store
            .pilot_site_revisions(&site_id("a"), after, limit)
            .await
            .unwrap()
            .expect("the site");
        // Every page says where the site stands.
        assert_eq!(site.revision, 5);
        revisions
            .into_iter()
            .map(|revision| (revision.number, revision.record.elevation_m))
            .collect::<Vec<_>>()
    };
    assert_eq!(page(0, 2).await, [(1, Some(1500.0)), (2, Some(1501.0))]);
    assert_eq!(page(2, 2).await, [(3, Some(1502.0)), (4, Some(1503.0))]);
    assert_eq!(page(4, 2).await, [(5, Some(1504.0))]);
    assert_eq!(page(5, 2).await, []);
    assert_eq!(page(u32::MAX, 2).await, []);
    assert_eq!(page(0, 100).await.len(), 5);
    assert_eq!(page(0, 0).await, []);
}

pub(crate) async fn what_changed_among_the_sites_is_listed_once_per_site(store: &impl Store) {
    // Before anything was written there is nothing, from any number.
    assert_eq!(changes(store, 0, 10).await, (Vec::new(), false, 0));
    assert_eq!(changes(store, 7, 10).await, (Vec::new(), false, 0));

    let anna = create(store, "anna").await;
    add(store, record("a", 47.0, 11.0), &anna, at(10, 0)).await; // 1
    add(store, record("b", 47.1, 11.0), &anna, at(10, 1)).await; // 2
    add(store, record("c", 47.2, 11.0), &anna, at(10, 2)).await; // 3
    let all = [("a", true), ("b", true), ("c", true)];
    assert_eq!(changes(store, 0, 10).await, (names(&all), false, 3));

    // A site that changed again is listed once, at its latest change, and
    // a removed one is listed as removed.
    let renamed = Site {
        name: Some("Musterberg West".to_owned()),
        ..record("a", 47.0, 11.0)
    };
    written(store, &by(&anna, "a", at(11, 0), change(renamed))).await; // 4
    written(store, &by(&anna, "b", at(11, 1), REMOVE)).await; // 5
    let now = [("c", true), ("a", true), ("b", false)];
    assert_eq!(changes(store, 0, 10).await, (names(&now), false, 5));
    let numbers: Vec<i64> = store
        .pilot_site_changes(0, 10)
        .await
        .unwrap()
        .sites
        .iter()
        .map(|site| site.change)
        .collect();
    assert_eq!(numbers, [3, 4, 5]);

    // Since a number: only what took a later one.
    assert_eq!(changes(store, 3, 10).await, (names(&now[1..]), false, 5));
    assert_eq!(changes(store, 4, 10).await, (names(&now[2..]), false, 5));
    assert_eq!(changes(store, 5, 10).await, (Vec::new(), false, 5));
    // A number from beyond the latest finds nothing; the latest says so.
    assert_eq!(changes(store, 99, 10).await, (Vec::new(), false, 5));

    // Page by page: more is waiting until the last page.
    assert_eq!(changes(store, 0, 2).await, (names(&now[..2]), true, 5));
    assert_eq!(changes(store, 4, 1).await, (names(&now[2..]), false, 5));
    assert_eq!(changes(store, 0, 3).await, (names(&now), false, 5));

    // A site that comes back is a change like any other.
    written(store, &by(&anna, "b", at(12, 0), SiteAct::Restore(1))).await; // 6
    assert_eq!(
        changes(store, 5, 10).await,
        (names(&[("b", true)]), false, 6)
    );
}

pub(crate) async fn the_identifier_of_a_site_is_never_used_again(store: &impl Store) {
    let anna = create(store, "anna").await;
    add(store, record("a", 47.0, 11.0), &anna, at(10, 0)).await;

    // Not while the site stands, wherever the other one would be.
    let again = new_site(record("a", 48.0, 12.0), &anna, at(10, 5));
    assert!(matches!(
        store.add_pilot_site(&again, APART_M).await,
        Err(StoreError::Invalid(_))
    ));
    // Not after it was removed, and not after it was taken down.
    let removed = written(store, &by(&anna, "a", at(11, 0), REMOVE)).await;
    assert!(matches!(
        store.add_pilot_site(&again, APART_M).await,
        Err(StoreError::Invalid(_))
    ));
    let down = written(store, &by_operator("a", at(11, 30), TAKE_DOWN)).await;
    assert!(matches!(
        store.add_pilot_site(&again, APART_M).await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(removed.site, down.site);
    assert_eq!(read(store, "a").await, down);
    assert_eq!(history(store, "a").await.len(), 3);
    assert_eq!(
        changes(store, 0, 10).await,
        (names(&[("a", false)]), false, 3)
    );

    // What no store keeps is refused before anything is asked: a record
    // under another site's identifier, and a zero byte in a text.
    let other = by(&anna, "a", at(12, 0), change(record("b", 47.0, 11.0)));
    assert!(matches!(
        store.revise_pilot_site(&other, APART_M).await,
        Err(StoreError::Invalid(_))
    ));
    let zero = NewPilotSite {
        comment: Some("a\0b".to_owned()),
        ..new_site(record("z", 47.5, 11.5), &anna, at(12, 0))
    };
    assert!(matches!(
        store.add_pilot_site(&zero, APART_M).await,
        Err(StoreError::Invalid(_))
    ));
    // An identifier with a zero byte in it is the identifier of no site.
    let odd: SiteId = "venti:a\0".parse().unwrap();
    assert_eq!(store.pilot_site(&odd).await.unwrap(), None);
    assert_eq!(store.pilot_site_revisions(&odd, 0, 10).await.unwrap(), None);
    assert_eq!(store.set_site(&odd).await.unwrap(), None);
    assert_eq!(store.pilot_site_changes(0, 10).await.unwrap().latest, 3);
}

pub(crate) async fn a_deleted_account_leaves_its_sites_without_an_author(store: &impl Store) {
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let first = record("a", 47.0, 11.0);
    add(store, first.clone(), &anna, at(10, 0)).await;
    let renamed = Site {
        name: Some("Musterberg West".to_owned()),
        ..first.clone()
    };
    written(store, &by(&bernd, "a", at(11, 0), change(renamed.clone()))).await;
    written(store, &by(&anna, "a", at(12, 0), SiteAct::Restore(1))).await;
    // A site only anna wrote, and one she removed.
    add(store, record("b", 47.1, 11.0), &anna, at(12, 10)).await;
    add(store, record("c", 47.2, 11.0), &bernd, at(12, 20)).await;
    written(store, &by(&anna, "c", at(12, 30), REMOVE)).await;
    let before = store.pilot_site_changes(0, 10).await.unwrap();

    store.delete_account(&anna.id).await.unwrap();

    // Every site and every revision is still there, as it was; what anna
    // wrote has no author.
    let a = read(store, "a").await;
    assert_eq!(
        (&a.site, a.revision, &a.created_by, &a.changed_by),
        (&first, 3, &None, &None)
    );
    assert_eq!((a.created_at, a.changed_at), (at(10, 0), at(12, 0)));
    let authors: Vec<_> = history(store, "a")
        .await
        .into_iter()
        .map(|revision| (revision.number, revision.author, revision.record))
        .collect();
    assert_eq!(
        authors,
        [
            (1, None, first.clone()),
            (2, pilot(&bernd), renamed),
            (3, None, first),
        ]
    );
    let b = read(store, "b").await;
    assert_eq!(
        (&b.created_by, &b.changed_by, b.stands()),
        (&None, &None, true)
    );
    let c = read(store, "c").await;
    assert_eq!(
        (&c.created_by, &c.changed_by, c.removed_at),
        (&pilot(&bernd), &None, Some(at(12, 30)))
    );
    assert_eq!(standing_in(store, &everything()).await, ["a", "b"]);
    // Nothing about the sites changed: no change number was taken.
    let after = store.pilot_site_changes(0, 10).await.unwrap();
    assert_eq!(after.latest, before.latest);
    let numbers = |page: &PilotSiteChanges| -> Vec<(SiteId, i64)> {
        page.sites
            .iter()
            .map(|site| (site.site.id.clone(), site.change))
            .collect()
    };
    assert_eq!(numbers(&after), numbers(&before));

    // A pilot who is still there goes on correcting what the other left.
    let again = written(store, &by(&bernd, "a", at(13, 0), SiteAct::Restore(2))).await;
    assert_eq!((again.revision, &again.changed_by), (4, &pilot(&bernd)));
    assert_eq!(again.created_by, None);
}

pub(crate) async fn sites_of_pilots_are_found_by_box_apart_from_a_sources(store: &impl Store) {
    let anna = create(store, "anna").await;
    // On the edges of the box and just outside it, stored out of order.
    add(store, record("c", 48.5, 17.2), &anna, at(10, 0)).await;
    add(store, record("a", 46.3, 9.4), &anna, at(10, 1)).await;
    add(store, record("b", 47.0, 11.0), &anna, at(10, 2)).await;
    add(store, record("out", 48.500_001, 11.0), &anna, at(10, 3)).await;
    assert_eq!(standing_in(store, &alps()).await, ["a", "b", "c"]);
    assert_eq!(
        standing_in(store, &everything()).await,
        ["a", "b", "c", "out"]
    );
    let tegernsee = BoundingBox::new(46.9, 10.9, 47.1, 11.1).unwrap();
    assert_eq!(standing_in(store, &tegernsee).await, ["b"]);
    // The limit keeps the first by identifier.
    let first: Vec<_> = store.pilot_sites_in(&alps(), 2).await.unwrap();
    assert_eq!(
        first
            .iter()
            .map(|site| short(&site.site.id))
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(store.pilot_sites_in(&alps(), 0).await.unwrap(), []);

    // A source's set is another matter: its sites are in no answer about
    // the pilots', and the pilots' are in none about a source's.
    let osm = Site {
        id: "osm-free-flying:node/1".parse().unwrap(),
        ..record("b", 47.0, 11.0)
    };
    let delivery = SiteDelivery {
        as_of: Utc.with_ymd_and_hms(2026, 11, 17, 21, 21, 2).unwrap(),
        covers: vec![alps()],
    };
    store
        .save_site_run(
            &run("osm-free-flying", at(6, 0)),
            &delivery,
            std::slice::from_ref(&osm),
        )
        .await
        .unwrap();
    assert_eq!(standing_in(store, &tegernsee).await, ["b"]);
    let sources = [osm.id.source().clone(), site_id("b").source().clone()];
    let layers = store.sites_in(&sources, &tegernsee, 100).await.unwrap();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].sites, std::slice::from_ref(&osm));

    // A source's site by its identifier, as the set has it now.
    assert_eq!(store.set_site(&osm.id).await.unwrap(), Some(osm.clone()));
    let unknown: SiteId = "osm-free-flying:node/2".parse().unwrap();
    assert_eq!(store.set_site(&unknown).await.unwrap(), None);
    assert_eq!(store.set_site(&site_id("b")).await.unwrap(), None);
    assert_eq!(store.pilot_site(&osm.id).await.unwrap(), None);
    // It goes when a run no longer delivers it.
    let other = Site {
        id: unknown.clone(),
        ..osm.clone()
    };
    store
        .save_site_run(
            &run("osm-free-flying", at(7, 0)),
            &delivery,
            std::slice::from_ref(&other),
        )
        .await
        .unwrap();
    assert_eq!(store.set_site(&osm.id).await.unwrap(), None);
    assert_eq!(store.set_site(&unknown).await.unwrap(), Some(other));
    // The pilots' sites stay through every run.
    assert_eq!(standing_in(store, &alps()).await, ["a", "b", "c"]);
}
