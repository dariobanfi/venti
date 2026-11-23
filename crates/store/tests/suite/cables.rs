//! The behaviour every [`Store`] must have for cables: one set per source,
//! replaced whole by a healthy run, and inside the boxes that answered by a
//! run that asked for its area in pieces of which some failed.

use chrono::{TimeZone, Utc};
use venti_domain::{BoundingBox, Cable, CableKind, GeoPoint, SourceId, Timestamp};
use venti_store::{CableSet, RunRecord, SiteDelivery, Store};

use super::{at, everything, run, source};

const OSM: &str = "osm-cables";

// --- Helpers -------------------------------------------------------------

fn tile(south: f64, west: f64) -> BoundingBox {
    BoundingBox::new(south, west, south + 1.0, west + 1.0).unwrap()
}

/// Two tiles side by side, west and east of 12° E.
fn west() -> BoundingBox {
    tile(47.0, 11.0)
}

fn east() -> BoundingBox {
    tile(47.0, 12.0)
}

fn both() -> BoundingBox {
    BoundingBox::new(47.0, 11.0, 48.0, 13.0).unwrap()
}

/// A power line from one point to another, named for what it is.
fn line(local: &str, from: (f64, f64), to: (f64, f64), name: &str) -> Cable {
    Cable {
        source: source(OSM),
        local: local.to_owned(),
        url: None,
        kind: CableKind::PowerLine,
        line: vec![
            GeoPoint::new(from.0, from.1).unwrap(),
            GeoPoint::new(to.0, to.1).unwrap(),
        ],
        height_m: None,
        voltage_kv: Some(110.0),
        name: Some(name.to_owned()),
        operator: None,
    }
}

/// A line inside the western tile, one inside the eastern, and one across
/// the edge between them.
fn in_the_west(name: &str) -> Cable {
    line("way/1", (47.2, 11.2), (47.4, 11.6), name)
}

fn in_the_east(name: &str) -> Cable {
    line("way/2", (47.2, 12.2), (47.4, 12.6), name)
}

fn across(name: &str) -> Cable {
    line("way/3", (47.5, 11.8), (47.6, 12.3), name)
}

fn as_of(day: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, day, 21, 0, 0).unwrap()
}

/// A run in which some pieces failed.
fn failed(started_at: Timestamp) -> RunRecord {
    RunRecord {
        healthy: false,
        failed_requests: 1,
        ..run(OSM, started_at)
    }
}

fn delivery(day: u32, covers: &[BoundingBox]) -> SiteDelivery {
    SiteDelivery {
        as_of: as_of(day),
        covers: covers.to_vec(),
    }
}

async fn set(store: &impl Store) -> Option<CableSet> {
    store.cable_sets(&[source(OSM)]).await.unwrap().pop()
}

/// What the store holds, as key and name.
async fn held(store: &impl Store) -> Vec<(String, String)> {
    let sources: Vec<SourceId> = vec![source(OSM)];
    store
        .cables_in(&sources, &everything(), 1000)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|layer| layer.cables)
        .map(|cable| (cable.local, cable.name.unwrap_or_default()))
        .collect()
}

fn named(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(local, name)| ((*local).to_owned(), (*name).to_owned()))
        .collect()
}

/// The set a healthy run stored the evening of the 16th, over both tiles.
async fn whole_week_before(store: &impl Store) {
    store
        .save_cable_run(
            &run(OSM, at(6, 0)),
            &delivery(16, &[both()]),
            &[in_the_west("old"), in_the_east("old"), across("old")],
        )
        .await
        .unwrap();
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn a_healthy_cable_run_replaces_the_whole_set(store: &impl Store) {
    whole_week_before(store).await;
    store
        .save_cable_run(
            &run(OSM, at(12, 0)),
            &delivery(17, &[both()]),
            &[in_the_east("new")],
        )
        .await
        .unwrap();
    assert_eq!(held(store).await, named(&[("way/2", "new")]));
    let set = set(store).await.unwrap();
    assert_eq!(set.cables, 1);
    assert_eq!(set.covers, [both()]);
    assert_eq!(set.as_of, as_of(17));
}

pub(crate) async fn a_tile_that_answered_replaces_the_cables_inside_it(store: &impl Store) {
    whole_week_before(store).await;
    // The western tile answered without its line, which was taken down, and
    // with the line across the edge renamed; the eastern tile failed.
    store
        .save_cable_run(
            &failed(at(12, 0)),
            &delivery(17, &[west()]),
            &[across("new")],
        )
        .await
        .unwrap();
    assert_eq!(
        held(store).await,
        named(&[("way/2", "old"), ("way/3", "new")]),
        "the eastern line stays, the line across the edge is there once"
    );
    let set = set(store).await.unwrap();
    assert_eq!(set.cables, 2);
    assert_eq!(set.fetched_at, at(12, 0));
    // Part of the set is still of the 16th.
    assert_eq!(set.as_of, as_of(16));
    assert_eq!(set.covers, [both()]);
}

pub(crate) async fn a_line_across_the_edge_of_a_failed_tile_is_kept(store: &impl Store) {
    whole_week_before(store).await;
    // The western tile answered, but the line across the edge was not in
    // its answer: the line reaches into the eastern tile, which failed, so
    // the store cannot tell it is gone and keeps it.
    store
        .save_cable_run(
            &failed(at(12, 0)),
            &delivery(17, &[west()]),
            &[in_the_west("new")],
        )
        .await
        .unwrap();
    assert_eq!(
        held(store).await,
        named(&[("way/1", "new"), ("way/2", "old"), ("way/3", "old")])
    );
}

pub(crate) async fn tiles_that_answered_build_up_a_set_from_nothing(store: &impl Store) {
    // The first run: only the western tile answered.
    store
        .save_cable_run(
            &failed(at(6, 0)),
            &delivery(16, &[west()]),
            &[in_the_west("first"), across("first")],
        )
        .await
        .unwrap();
    let first = set(store).await.unwrap();
    assert_eq!(first.cables, 2);
    assert_eq!(first.covers, [west()]);
    assert_eq!(first.as_of, as_of(16));
    // Six hours on, only the eastern one.
    store
        .save_cable_run(
            &failed(at(12, 0)),
            &delivery(17, &[east()]),
            &[in_the_east("second"), across("second")],
        )
        .await
        .unwrap();
    assert_eq!(
        held(store).await,
        named(&[("way/1", "first"), ("way/2", "second"), ("way/3", "second")])
    );
    let second = set(store).await.unwrap();
    assert_eq!(second.covers, [west(), east()]);
    assert_eq!(second.as_of, as_of(16));
    // A tile over the sea answers with no cable, and empties its box.
    store
        .save_cable_run(&failed(at(18, 0)), &delivery(18, &[west()]), &[])
        .await
        .unwrap();
    assert_eq!(
        held(store).await,
        named(&[("way/2", "second"), ("way/3", "second")])
    );
}

pub(crate) async fn a_failed_run_without_tiles_or_an_older_one_changes_nothing(store: &impl Store) {
    whole_week_before(store).await;
    // No tile answered.
    store
        .save_cable_run(&failed(at(12, 0)), &delivery(17, &[]), &[])
        .await
        .unwrap();
    // A run that started before the stored set's.
    store
        .save_cable_run(&failed(at(5, 0)), &delivery(17, &[west()]), &[])
        .await
        .unwrap();
    assert_eq!(
        held(store).await,
        named(&[("way/1", "old"), ("way/2", "old"), ("way/3", "old")])
    );
    let set = set(store).await.unwrap();
    assert_eq!(set.fetched_at, at(6, 0));
    assert_eq!(set.covers, [both()]);
    // Both runs are in the history.
    let status = store.source_status(&[source(OSM)]).await.unwrap();
    assert_eq!(
        status[0].last_failure,
        Some(at(12, 0) + chrono::Duration::minutes(1))
    );
}

pub(crate) async fn a_bad_partial_cable_run_is_refused(store: &impl Store) {
    whole_week_before(store).await;
    let twice = store
        .save_cable_run(
            &failed(at(12, 0)),
            &delivery(17, &[west()]),
            &[in_the_west("a"), in_the_west("b")],
        )
        .await;
    assert!(twice.is_err());
    assert_eq!(set(store).await.unwrap().fetched_at, at(6, 0));
}
