//! What every store must say that public flights say about where pilots
//! take off: the flights that began at a place, and the places where
//! several pilots took off and no site stands.

use chrono::{NaiveDate, TimeZone, Utc};
use venti_domain::{BoundingBox, GeoPoint, Site, SiteId, SiteUse, SourceId, Timestamp};
use venti_store::{
    Account, CandidateRule, NewPilotSite, Place, Score, ScoreKind, SiteAct, SiteDelivery,
    SiteRevising, SiteVia, SiteWrite, Store, TakeoffCandidate, Visibility,
};

use super::accounts::{create, id};
use super::flights::{new_flight, put};
use super::{alps, at, run, source};

// --- Helpers -------------------------------------------------------------

fn moment(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Timestamp {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
}

fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

/// Stores flight `n` of an account, which took off at a place and a
/// height and scores `points`, and sets who may see it.
async fn flown(
    store: &impl Store,
    account: &Account,
    n: u8,
    start: Timestamp,
    (lat, lon, altitude_m): (f64, f64, i32),
    points: Option<f64>,
    visibility: Visibility,
) {
    let mut new = new_flight(account, n, start);
    new.derived.takeoff = Some(Place {
        time: start,
        position: GeoPoint::new(lat, lon).unwrap(),
        altitude_m,
    });
    new.derived.score = points.map(|points| Score {
        kind: ScoreKind::FreeFlight,
        closed: false,
        distance_m: points * 1000.0,
        points,
    });
    put(store, &new).await;
    see(store, account, n, visibility).await;
}

/// Sets who may see flight `n` of an account.
async fn see(store: &impl Store, account: &Account, n: u8, visibility: Visibility) {
    store
        .set_visibility(
            &account.id,
            &id(&format!("{}{n}", account.handle)),
            visibility,
            at(21, 0),
        )
        .await
        .unwrap()
        .flight()
        .expect("the flight is there");
}

/// A take-off under an identifier, at a place.
fn site(id: &str, lat: f64, lon: f64) -> Site {
    Site {
        id: id.parse().unwrap(),
        name: Some("Musterberg".to_owned()),
        position: GeoPoint::new(lat, lon).unwrap(),
        elevation_m: Some(1400.0),
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

const OSM: &str = "osm-free-flying";

/// Makes `sites` the whole set of OpenStreetMap's, by a run that started
/// at `started_at`.
async fn deliver(store: &impl Store, started_at: Timestamp, sites: &[Site]) {
    let delivery = SiteDelivery {
        as_of: started_at,
        covers: vec![alps()],
    };
    store
        .save_site_run(&run(OSM, started_at), &delivery, sites)
        .await
        .unwrap();
}

const RULE: CandidateRule = CandidateRule {
    min_pilots: 3,
    clear_m: 500.0,
};

async fn offered(
    store: &impl Store,
    sources: &[SourceId],
    area: &BoundingBox,
) -> Vec<TakeoffCandidate> {
    store
        .takeoff_candidates(sources, area, &RULE, 100)
        .await
        .unwrap()
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn the_public_flights_that_began_at_a_place_are_counted(store: &impl Store) {
    let here = GeoPoint::new(47.6642, 11.7967).unwrap();
    let nothing = store.flights_from(here, 500.0, 5).await.unwrap();
    assert_eq!((nothing.flights, nothing.pilots), (0, 0));
    assert_eq!(nothing.latest_day, None);
    assert_eq!(nothing.by_month, [0; 12]);
    assert!(nothing.best.is_empty());

    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    let dora = create(store, "dora").await;
    let public = Visibility::Public;
    // At the place, 200 m north of it and 400 m east of it.
    let at_it = (47.6642, 11.7967, 1620);
    flown(
        store,
        &anna,
        1,
        moment(2025, 7, 4, 10, 0),
        at_it,
        Some(30.0),
        public,
    )
    .await;
    let north = (47.6660, 11.7967, 1615);
    flown(
        store,
        &anna,
        2,
        moment(2026, 7, 19, 11, 0),
        north,
        Some(12.5),
        public,
    )
    .await;
    let east = (47.6642, 11.8020, 1600);
    flown(
        store,
        &bernd,
        1,
        moment(2026, 3, 8, 12, 0),
        east,
        None,
        public,
    )
    .await;
    // Half an hour before midnight in UTC on the last day of a year is
    // the first of January where the glider took off: 47 minutes later
    // by the sun.
    flown(
        store,
        &carla,
        1,
        moment(2024, 12, 31, 23, 30),
        at_it,
        Some(45.0),
        public,
    )
    .await;
    // What is not public is in no count, however good it is.
    let private = Visibility::Private;
    flown(
        store,
        &dora,
        1,
        moment(2026, 8, 1, 10, 0),
        at_it,
        Some(99.0),
        private,
    )
    .await;
    let unlisted = Visibility::Unlisted;
    flown(
        store,
        &dora,
        2,
        moment(2026, 8, 2, 10, 0),
        at_it,
        Some(98.0),
        unlisted,
    )
    .await;
    // A flight that began 600 m away began somewhere else.
    let away = (47.6696, 11.7967, 1500);
    flown(
        store,
        &bernd,
        2,
        moment(2026, 9, 1, 10, 0),
        away,
        Some(80.0),
        public,
    )
    .await;
    // And one whose recording never shows the glider flying began nowhere.
    let mut grounded = new_flight(&carla, 2, moment(2026, 9, 2, 10, 0));
    grounded.derived.takeoff = None;
    put(store, &grounded).await;
    see(store, &carla, 2, public).await;

    let said = store.flights_from(here, 500.0, 5).await.unwrap();
    assert_eq!((said.flights, said.pilots), (4, 3));
    assert_eq!(said.latest_day, Some(day(2026, 7, 19)));
    assert_eq!(said.by_month, [1, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0]);
    // The best by points; a flight too short to score is not among them.
    let best: Vec<(&str, &str)> = said
        .best
        .iter()
        .map(|flight| {
            (
                flight.pilot_handle.as_str(),
                flight.id.trim_start_matches('0'),
            )
        })
        .collect();
    assert_eq!(
        best,
        [("carla", "carla1"), ("anna", "anna1"), ("anna", "anna2")]
    );
    assert!(said.best.iter().all(|f| f.visibility == Visibility::Public));
    let two = store.flights_from(here, 500.0, 2).await.unwrap();
    assert_eq!(two.best.len(), 2);
    assert_eq!((two.flights, two.by_month), (said.flights, said.by_month));

    // A smaller distance holds fewer, and none holds none.
    let close = store.flights_from(here, 100.0, 5).await.unwrap();
    assert_eq!((close.flights, close.pilots), (2, 2));
    assert_eq!(close.by_month, [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
    let elsewhere = GeoPoint::new(47.6696, 11.7967).unwrap();
    let there = store.flights_from(elsewhere, 100.0, 5).await.unwrap();
    assert_eq!((there.flights, there.pilots), (1, 1));
    assert_eq!(there.latest_day, Some(day(2026, 9, 1)));

    // A flight counts from the moment it is public and no longer.
    see(store, &dora, 1, public).await;
    see(store, &anna, 1, private).await;
    let said = store.flights_from(here, 500.0, 1).await.unwrap();
    assert_eq!((said.flights, said.pilots), (4, 4));
    assert_eq!(said.latest_day, Some(day(2026, 8, 1)));
    assert_eq!(said.by_month, [1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0]);
    assert_eq!(said.best[0].pilot_handle, "dora");
    // And goes with its account.
    store.delete_account(&carla.id).await.unwrap();
    let said = store.flights_from(here, 500.0, 5).await.unwrap();
    assert_eq!((said.flights, said.pilots), (3, 3));
    assert_eq!(said.by_month, [0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0]);
}

pub(crate) async fn a_place_several_pilots_took_off_at_is_a_candidate(store: &impl Store) {
    let anna = create(store, "anna").await;
    let bernd = create(store, "bernd").await;
    let carla = create(store, "carla").await;
    let public = Visibility::Public;
    let around = BoundingBox::new(47.70, 12.02, 47.72, 12.04).unwrap();
    let osm = [source(OSM)];

    // Two pilots, one of them twice: not enough to say that pilots take
    // off here.
    flown(
        store,
        &anna,
        1,
        moment(2026, 5, 2, 10, 0),
        (47.7111, 12.0311, 1402),
        None,
        public,
    )
    .await;
    flown(
        store,
        &anna,
        2,
        moment(2026, 6, 6, 10, 0),
        (47.7112, 12.0309, 1405),
        None,
        public,
    )
    .await;
    flown(
        store,
        &bernd,
        1,
        moment(2026, 6, 7, 11, 0),
        (47.7113, 12.0313, 1398),
        None,
        public,
    )
    .await;
    assert!(offered(store, &osm, &around).await.is_empty());
    // A third pilot whose flight is not public changes nothing.
    let private = Visibility::Private;
    flown(
        store,
        &carla,
        1,
        moment(2025, 6, 8, 9, 0),
        (47.7112, 12.0315, 1411),
        None,
        private,
    )
    .await;
    assert!(offered(store, &osm, &around).await.is_empty());

    // With the third pilot's flight public, the place is offered: the
    // middle of the take-offs, rounded, and what the flights say. It
    // names no pilot and no flight.
    see(store, &carla, 1, public).await;
    let candidate = TakeoffCandidate {
        position: GeoPoint::new(47.7112, 12.0312).unwrap(),
        altitude_m: 1400,
        flights: 4,
        pilots: 3,
        latest_day: day(2026, 6, 7),
        by_month: [0, 0, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0],
    };
    assert_eq!(
        offered(store, &osm, &around).await,
        std::slice::from_ref(&candidate)
    );
    // A box asks for every cell it touches, whole: one that holds none of
    // the take-offs and a corner of their cell is told the same.
    let corner = BoundingBox::new(47.70, 12.02, 47.7101, 12.0301).unwrap();
    assert_eq!(
        offered(store, &osm, &corner).await,
        std::slice::from_ref(&candidate)
    );
    let elsewhere = BoundingBox::new(47.60, 11.70, 47.70, 11.90).unwrap();
    assert!(offered(store, &osm, &elsewhere).await.is_empty());
    // A rule that asks for more pilots is not met.
    let four = CandidateRule {
        min_pilots: 4,
        ..RULE
    };
    let asked = store.takeoff_candidates(&osm, &around, &four, 100).await;
    assert!(asked.unwrap().is_empty());

    // One of the three makes their flight private, or shares it by link
    // only: at the next question the place is no candidate.
    for withdrawn in [Visibility::Private, Visibility::Unlisted] {
        see(store, &carla, 1, withdrawn).await;
        assert!(offered(store, &osm, &around).await.is_empty());
        see(store, &carla, 1, public).await;
        assert_eq!(
            offered(store, &osm, &around).await,
            std::slice::from_ref(&candidate)
        );
    }

    // A site of a source 300 m to the north names the place. A set that
    // is not asked for is in the way of nothing.
    let far = site("osm-free-flying:node/2", 47.60, 11.80);
    let near = site("osm-free-flying:node/1", 47.7139, 12.0312);
    deliver(store, at(8, 0), &[near, far.clone()]).await;
    assert!(offered(store, &osm, &around).await.is_empty());
    assert_eq!(
        offered(store, &[], &around).await,
        std::slice::from_ref(&candidate)
    );
    // The source's next set has no site there, and one 700 m away.
    let beyond = site("osm-free-flying:node/3", 47.7175, 12.0312);
    deliver(store, at(9, 0), &[beyond, far]).await;
    assert_eq!(
        offered(store, &osm, &around).await,
        std::slice::from_ref(&candidate)
    );

    // A pilot adds the site: the candidate is gone. The site is removed
    // again: the place is offered again.
    let made: SiteId = format!("venti-pilots:{}", id("here")).parse().unwrap();
    let new = NewPilotSite {
        site: site(&made.to_string(), 47.7110, 12.0314),
        author_id: anna.id.clone(),
        at: at(10, 0),
        via: SiteVia::Session,
        comment: None,
    };
    let written = store.add_pilot_site(&new, 20.0).await.unwrap();
    assert!(matches!(written, SiteWrite::Written(_)), "{written:?}");
    assert!(offered(store, &osm, &around).await.is_empty());
    assert!(offered(store, &[], &around).await.is_empty());
    let removing = SiteRevising {
        id: made,
        author_id: Some(bernd.id.clone()),
        at: at(11, 0),
        via: SiteVia::Session,
        act: SiteAct::Remove { take_down: false },
        comment: None,
        expects: None,
    };
    let written = store.revise_pilot_site(&removing, 20.0).await.unwrap();
    assert!(matches!(written, SiteWrite::Written(_)), "{written:?}");
    assert_eq!(
        offered(store, &osm, &around).await,
        std::slice::from_ref(&candidate)
    );

    // An account that goes takes its flights with it, and the place has
    // two pilots.
    store.delete_account(&carla.id).await.unwrap();
    assert!(offered(store, &osm, &around).await.is_empty());
}

pub(crate) async fn candidates_come_by_pilots_and_a_cell_counts_alone(store: &impl Store) {
    let mut pilots = Vec::new();
    for handle in ["anna", "bernd", "carla", "dora", "emil"] {
        pilots.push(create(store, handle).await);
    }
    let public = Visibility::Public;
    let when = moment(2026, 4, 11, 10, 0);
    // Three pilots at one place, five at another, each once.
    for pilot in &pilots[..3] {
        flown(
            store,
            pilot,
            1,
            when,
            (47.7112, 12.0312, 1400),
            None,
            public,
        )
        .await;
    }
    for pilot in &pilots {
        flown(store, pilot, 2, when, (47.8012, 12.1012, 900), None, public).await;
    }
    // And three pilots at a place that an edge of the grid runs through,
    // 47.75 north: two took off south of it and one north of it, 20 m
    // apart. Neither cell has three pilots, so the place is not offered.
    flown(
        store,
        &pilots[0],
        3,
        when,
        (47.7499, 12.2012, 1100),
        None,
        public,
    )
    .await;
    flown(
        store,
        &pilots[1],
        3,
        when,
        (47.7499, 12.2013, 1100),
        None,
        public,
    )
    .await;
    flown(
        store,
        &pilots[2],
        3,
        when,
        (47.7501, 12.2012, 1100),
        None,
        public,
    )
    .await;

    let region = BoundingBox::new(47.6, 11.9, 47.9, 12.3).unwrap();
    let all = offered(store, &[], &region).await;
    let places: Vec<(f64, f64, u32)> = all
        .iter()
        .map(|c| (c.position.lat(), c.position.lon(), c.pilots))
        .collect();
    assert_eq!(places, [(47.8012, 12.1012, 5), (47.7112, 12.0312, 3)]);
    assert_eq!(all[0].by_month, [0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(all[0].altitude_m, 900);

    // The first of them where only one is asked for, and none where none.
    let first = store.takeoff_candidates(&[], &region, &RULE, 1).await;
    assert_eq!(first.unwrap(), all[..1]);
    let none = store.takeoff_candidates(&[], &region, &RULE, 0).await;
    assert!(none.unwrap().is_empty());
}
