//! The behaviour every [`Store`] must have for airspace: one whole set per
//! source, replaced by a healthy run and by nothing else.

use chrono::{NaiveTime, TimeZone, Utc, Weekday};
use venti_domain::{
    Activity, Airspace, AirspaceClass, AirspaceType, BoundingBox, Circle, DayTime, Frequency,
    GeoPoint, HeightUnit, OperatingHours, Polygon, SourceId, Timestamp, Validity, VerticalLimit,
    VerticalReference,
};
use venti_store::{
    AirspaceLayer, AirspaceSet, Coverage, Month, RunRecord, SourceStatus, Store, StoreError,
};

use super::{alps, at, everything, run, source};

const OPENAIP: &str = "openaip";
const CHARTS: &str = "charts";

// --- Helpers -------------------------------------------------------------

fn point(lat: f64, lon: f64) -> GeoPoint {
    GeoPoint::new(lat, lon).unwrap()
}

/// A square airspace whose south-west corner is at the given position.
fn square(id: &str, south: f64, west: f64, size: f64) -> Airspace {
    let (north, east) = (south + size, west + size);
    let ring = vec![
        point(south, west),
        point(south, east),
        point(north, east),
        point(north, west),
        point(south, west),
    ];
    Airspace {
        id: id.parse().unwrap(),
        name: id.to_uppercase(),
        class: AirspaceClass::D,
        airspace_type: AirspaceType::Ctr,
        country: Some("AT".to_owned()),
        lower: VerticalLimit::ground(),
        upper: VerticalLimit::new(VerticalReference::MeanSeaLevel, 7000.0, HeightUnit::Feet)
            .unwrap(),
        lower_at_least: None,
        upper_at_most: None,
        geometry: Polygon::new(ring, Vec::new()).unwrap(),
        activity: Activity::default(),
        frequencies: Vec::new(),
        remarks: None,
        validity: Validity::default(),
    }
}

/// A healthy run of an airspace source that delivered `airspaces`.
fn delivered(source_name: &str, started_at: Timestamp, airspaces: &[Airspace]) -> RunRecord {
    RunRecord {
        requests: 2,
        airspaces: u32::try_from(airspaces.len()).unwrap(),
        ..run(source_name, started_at)
    }
}

/// Where the runs of these tests collect: the first region, for Germany
/// and Austria.
fn region() -> Coverage {
    Coverage {
        boxes: vec![alps()],
        countries: vec!["DE".to_owned(), "AT".to_owned()],
    }
}

async fn save(store: &impl Store, source_name: &str, started_at: Timestamp, set: &[Airspace]) {
    store
        .save_airspace_run(&delivered(source_name, started_at, set), &region(), set)
        .await
        .unwrap();
}

async fn layers_in(store: &impl Store, sources: &[&str], area: &BoundingBox) -> Vec<AirspaceLayer> {
    let sources: Vec<SourceId> = sources.iter().map(|name| source(name)).collect();
    store.airspaces_in(&sources, area, 1000).await.unwrap()
}

/// Where the stored set of openAIP says it is whole.
async fn coverage_of(store: &impl Store) -> Coverage {
    let sets = store.airspace_sets(&[source(OPENAIP)]).await.unwrap();
    sets[0].coverage.clone()
}

/// What each layer of an answer holds: its source and the identifiers of
/// its airspaces.
fn listed(layers: &[AirspaceLayer]) -> Vec<(String, Vec<String>)> {
    layers
        .iter()
        .map(|layer| {
            let ids = layer.airspaces.iter().map(|a| a.id.to_string()).collect();
            (layer.set.source.to_string(), ids)
        })
        .collect()
}

fn ids(of: &[&str]) -> Vec<String> {
    of.iter().map(|&id| id.to_owned()).collect()
}

/// The identifiers of what one source has in a box.
async fn ids_in(store: &impl Store, source_name: &str, area: &BoundingBox) -> Vec<String> {
    layers_in(store, &[source_name], area)
        .await
        .into_iter()
        .flat_map(|layer| layer.airspaces)
        .map(|airspace| airspace.id.to_string())
        .collect()
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn an_airspace_set_is_stored_and_read_back_exactly(store: &impl Store) {
    // Everything a record can carry, with awkward decimals and a name
    // outside ASCII.
    let hole = vec![
        point(47.24, 11.31),
        point(47.24, 11.33),
        point(47.26, 11.33),
        point(47.24, 11.31),
    ];
    let full = Airspace {
        name: "Innsbruck – Kühtai \"TRA\"".to_owned(),
        class: AirspaceClass::Unclassified,
        airspace_type: AirspaceType::Other("openAIP type 37".to_owned()),
        lower: VerticalLimit::new(VerticalReference::AboveGround, 1000.0, HeightUnit::Feet)
            .unwrap(),
        upper: VerticalLimit::flight_level(125).unwrap(),
        lower_at_least: Some(
            VerticalLimit::new(VerticalReference::MeanSeaLevel, 1500.5, HeightUnit::Metres)
                .unwrap(),
        ),
        upper_at_most: Some(VerticalLimit::unlimited()),
        geometry: Polygon::new(
            vec![
                point(47.208_611_111_1, 11.262_222_222_3),
                point(47.2, 11.4 + 0.1 + 0.2),
                point(47.300_000_000_000_004, 11.7),
                point(47.31, 11.25),
                point(47.208_611_111_1, 11.262_222_222_3),
            ],
            vec![hole],
        )
        .unwrap()
        .with_circle(Circle {
            centre: point(47.26, 11.35),
            radius_m: 9_260.000_000_000_002,
        })
        .unwrap(),
        activity: Activity {
            by_notam: true,
            on_request: false,
            on_demand: true,
            special_agreement: false,
            hours: vec![OperatingHours {
                day: Weekday::Sat,
                start: DayTime::Clock(NaiveTime::from_hms_opt(8, 30, 0).unwrap()),
                end: DayTime::Sunset,
                by_notam: false,
                except_public_holidays: true,
                remarks: Some("Nur im Sommer".to_owned()),
            }],
            hours_remarks: Some("See AIP".to_owned()),
            from: Some(at(6, 0)),
            until: None,
        },
        frequencies: vec![Frequency {
            mhz: 120.105,
            name: Some("INNSBRUCK TURM".to_owned()),
            primary: true,
        }],
        remarks: Some("Line one.\nLine two.".to_owned()),
        validity: Validity {
            airac: Some("2612".to_owned()),
            file_date: Some(Utc.with_ymd_and_hms(2026, 11, 18, 1, 57, 33).unwrap()),
            updated: Some(Utc.with_ymd_and_hms(2026, 11, 11, 22, 35, 41).unwrap()),
        },
        ..square("openaip:677d59be13e31af480aca6d9", 47.2, 11.2, 0.1)
    };
    let plain = Airspace {
        country: None,
        validity: Validity {
            airac: None,
            file_date: Some(Utc.with_ymd_and_hms(2026, 11, 18, 1, 12, 30).unwrap()),
            updated: Some(Utc.with_ymd_and_hms(2026, 11, 18, 10, 29, 12).unwrap()),
        },
        ..square("openaip:plain", 47.7, 12.9, 0.2)
    };
    let set = [full.clone(), plain.clone()];

    save(store, OPENAIP, at(18, 30), &set).await;

    let layers = layers_in(store, &[OPENAIP], &alps()).await;
    assert_eq!(
        layers,
        [AirspaceLayer {
            set: AirspaceSet {
                source: source(OPENAIP),
                fetched_at: at(18, 30),
                airspaces: 2,
                // The oldest file and the newest change of the two.
                file_date: plain.validity.file_date,
                newest_change: plain.validity.updated,
                coverage: region(),
            },
            airspaces: vec![full, plain],
        }]
    );
    assert_eq!(
        store.airspace_sets(&[source(OPENAIP)]).await.unwrap(),
        [layers[0].set.clone()]
    );
    // The run is in the history like any other.
    let status = &store.source_status(&[source(OPENAIP)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(18, 30)));
    assert_eq!(status.last_success, Some(at(18, 31)));
}

pub(crate) async fn airspace_is_found_by_its_box_edges_included(store: &impl Store) {
    // The query box is the first region: 46.3..48.5 N, 9.4..17.2 E.
    let set = [
        square("openaip:inside", 47.0, 11.0, 0.5),
        // Reaches in from the north: only its southern part is in the box.
        square("openaip:straddles", 48.4, 11.0, 0.5),
        // Its southern edge lies on the box's northern edge.
        square("openaip:touches-north", 48.5, 11.0, 0.5),
        // Its north-east corner is the box's south-west corner.
        square("openaip:touches-corner", 45.8, 8.9, 0.5),
        // A hair beyond the northern and the western edge.
        square("openaip:just-north", 48.500_001, 11.0, 0.5),
        square("openaip:just-west", 47.0, 8.899_999, 0.5),
        square("openaip:far-away", 53.4, 9.8, 0.5),
        // So large that the whole box lies inside it.
        square("openaip:around", 40.0, 5.0, 15.0),
    ];
    save(store, OPENAIP, at(18, 30), &set).await;

    assert_eq!(
        ids_in(store, OPENAIP, &alps()).await,
        [
            "openaip:around",
            "openaip:inside",
            "openaip:straddles",
            "openaip:touches-corner",
            "openaip:touches-north",
        ]
    );
    assert_eq!(ids_in(store, OPENAIP, &everything()).await.len(), 8);
    // A box that is a single point finds what lies over it, on an edge too.
    let over = BoundingBox::new(47.25, 11.25, 47.25, 11.25).unwrap();
    assert_eq!(
        ids_in(store, OPENAIP, &over).await,
        ["openaip:around", "openaip:inside"]
    );
    let on_edge = BoundingBox::new(47.5, 11.2, 47.5, 11.2).unwrap();
    assert_eq!(
        ids_in(store, OPENAIP, &on_edge).await,
        ["openaip:around", "openaip:inside"]
    );
    // Where the set has nothing, the answer is the set without airspaces:
    // it still says how old it is and where it is whole.
    let nowhere = BoundingBox::new(10.0, 10.0, 11.0, 11.0).unwrap();
    let empty = layers_in(store, &[OPENAIP], &nowhere).await;
    assert_eq!(empty.len(), 1);
    assert!(empty[0].airspaces.is_empty());
    assert_eq!(empty[0].set.airspaces, 8);
    assert_eq!(empty[0].set.fetched_at, at(18, 30));
    assert_eq!(empty[0].set.coverage, region());
}

pub(crate) async fn a_healthy_run_replaces_the_whole_set(store: &impl Store) {
    let innsbruck = square("openaip:innsbruck", 47.2, 11.3, 0.1);
    let salzburg = square("openaip:salzburg", 47.7, 12.9, 0.1);
    let other = square("charts:innsbruck", 47.2, 11.3, 0.1);
    save(store, OPENAIP, at(3, 0), &[innsbruck.clone(), salzburg]).await;
    save(store, CHARTS, at(3, 0), std::slice::from_ref(&other)).await;

    // The next day's file: Salzburg is gone, Munich is new, Innsbruck grew.
    let grown = Airspace {
        upper: VerticalLimit::flight_level(195).unwrap(),
        ..innsbruck
    };
    let muenchen = square("openaip:muenchen", 48.3, 11.7, 0.1);
    save(store, OPENAIP, at(4, 0), &[grown.clone(), muenchen.clone()]).await;

    let layers = layers_in(store, &[OPENAIP], &everything()).await;
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].airspaces, [grown, muenchen]);
    assert_eq!(layers[0].set.fetched_at, at(4, 0));
    assert_eq!(layers[0].set.airspaces, 2);
    assert_eq!(layers[0].set.coverage, region());
    // The other source's set is its own.
    let charts = layers_in(store, &[CHARTS], &everything()).await;
    assert_eq!(charts[0].airspaces, [other]);
    assert_eq!(charts[0].set.fetched_at, at(3, 0));
}

pub(crate) async fn an_unhealthy_run_leaves_the_set_alone(store: &impl Store) {
    let failed = |started_at: Timestamp| RunRecord {
        healthy: false,
        failed_requests: 1,
        airspaces: 1,
        ..run(OPENAIP, started_at)
    };
    let half = [square("openaip:innsbruck", 47.2, 11.3, 0.1)];

    // A source whose first run fails has no airspace at all.
    store
        .save_airspace_run(&failed(at(2, 0)), &region(), &half)
        .await
        .unwrap();
    assert!(layers_in(store, &[OPENAIP], &everything()).await.is_empty());
    assert!(
        store
            .airspace_sets(&[source(OPENAIP)])
            .await
            .unwrap()
            .is_empty()
    );

    let whole = [
        square("openaip:innsbruck", 47.2, 11.3, 0.1),
        square("openaip:muenchen", 48.3, 11.7, 0.1),
    ];
    save(store, OPENAIP, at(3, 0), &whole).await;

    // Germany's file could not be fetched: the run delivers Austria only,
    // and yesterday's whole set stays. So does what it says of itself: the
    // failed run would have claimed the whole world.
    let world = Coverage {
        boxes: vec![everything()],
        countries: vec!["AT".to_owned()],
    };
    store
        .save_airspace_run(&failed(at(4, 0)), &world, &half)
        .await
        .unwrap();
    // Unhealthy with nothing delivered is no different.
    store
        .save_airspace_run(&failed(at(5, 0)), &world, &[])
        .await
        .unwrap();

    let layers = layers_in(store, &[OPENAIP], &everything()).await;
    assert_eq!(layers[0].airspaces, whole);
    assert_eq!(layers[0].set.fetched_at, at(3, 0));
    assert_eq!(layers[0].set.coverage, region());
    // The history knows about the failures all the same.
    assert_eq!(
        store.source_status(&[source(OPENAIP)]).await.unwrap(),
        [SourceStatus {
            source: source(OPENAIP),
            last_started: Some(at(5, 0)),
            last_success: Some(at(3, 1)),
            last_failure: Some(at(5, 1)),
            stations: 0,
            newest_observation: None,
        }]
    );
}

pub(crate) async fn an_older_run_does_not_replace_a_newer_set(store: &impl Store) {
    let newer = [square("openaip:newer", 47.2, 11.3, 0.1)];
    let older = [square("openaip:older", 47.2, 11.3, 0.1)];
    save(store, OPENAIP, at(4, 0), &newer).await;

    // A run that started earlier and is stored late. It collected for
    // another area, which must not become the newer set's.
    let elsewhere = Coverage {
        boxes: vec![everything()],
        countries: vec!["FR".to_owned()],
    };
    store
        .save_airspace_run(&delivered(OPENAIP, at(3, 0), &older), &elsewhere, &older)
        .await
        .unwrap();

    assert_eq!(
        ids_in(store, OPENAIP, &everything()).await,
        ["openaip:newer"]
    );
    let set = &store.airspace_sets(&[source(OPENAIP)]).await.unwrap()[0];
    assert_eq!(set.fetched_at, at(4, 0));
    assert_eq!(set.coverage, region());
    // Both runs are in the history.
    let status = &store.source_status(&[source(OPENAIP)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(4, 0)));
}

pub(crate) async fn saving_the_same_airspace_run_twice_changes_nothing(store: &impl Store) {
    let set = [
        square("openaip:innsbruck", 47.2, 11.3, 0.1),
        square("openaip:muenchen", 48.3, 11.7, 0.1),
    ];
    save(store, OPENAIP, at(3, 0), &set).await;
    let once = (
        layers_in(store, &[OPENAIP], &everything()).await,
        store.source_status(&[source(OPENAIP)]).await.unwrap(),
    );

    save(store, OPENAIP, at(3, 0), &set).await;
    let twice = (
        layers_in(store, &[OPENAIP], &everything()).await,
        store.source_status(&[source(OPENAIP)]).await.unwrap(),
    );

    assert_eq!(once, twice);
    assert_eq!(twice.0[0].airspaces.len(), 2);
}

pub(crate) async fn a_bad_airspace_set_is_refused_and_nothing_is_stored(store: &impl Store) {
    let good = square("openaip:innsbruck", 47.2, 11.3, 0.1);
    save(store, OPENAIP, at(3, 0), std::slice::from_ref(&good)).await;
    let refused = |result: Result<(), StoreError>| {
        assert!(
            matches!(result, Err(StoreError::Invalid(_))),
            "expected a refusal, got {result:?}"
        );
    };

    // An airspace of another source.
    let stranger = [good.clone(), square("charts:muenchen", 48.3, 11.7, 0.1)];
    refused(
        store
            .save_airspace_run(
                &delivered(OPENAIP, at(4, 0), &stranger),
                &region(),
                &stranger,
            )
            .await,
    );
    // An airspace listed twice.
    let twice = [good.clone(), good.clone()];
    refused(
        store
            .save_airspace_run(&delivered(OPENAIP, at(4, 0), &twice), &region(), &twice)
            .await,
    );
    // A run that calls itself healthy and delivers nothing would empty the
    // source.
    refused(
        store
            .save_airspace_run(&delivered(OPENAIP, at(4, 0), &[]), &region(), &[])
            .await,
    );

    // The set of the morning is untouched, and none of the refused runs is
    // in the history.
    let layers = layers_in(store, &[OPENAIP], &everything()).await;
    assert_eq!(layers[0].airspaces, [good]);
    let status = &store.source_status(&[source(OPENAIP)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(3, 0)));
}

pub(crate) async fn airspace_layers_are_kept_apart_and_sorted(store: &impl Store) {
    // "charts" sorts before "openaip"; within a source upper case sorts
    // before lower case and "10" before "9".
    save(
        store,
        OPENAIP,
        at(3, 0),
        &[
            square("openaip:b", 47.0, 11.0, 0.1),
            square("openaip:B", 47.0, 11.0, 0.1),
            square("openaip:9", 47.0, 11.0, 0.1),
            square("openaip:10", 47.0, 11.0, 0.1),
        ],
    )
    .await;
    save(
        store,
        CHARTS,
        at(4, 0),
        &[
            square("charts:z", 47.0, 11.0, 0.1),
            square("charts:far", 53.4, 9.8, 0.1),
        ],
    )
    .await;

    let both = layers_in(store, &[OPENAIP, CHARTS], &alps()).await;
    assert_eq!(
        listed(&both),
        [
            (CHARTS.to_owned(), ids(&["charts:z"])),
            (
                OPENAIP.to_owned(),
                ids(&["openaip:10", "openaip:9", "openaip:B", "openaip:b"])
            ),
        ]
    );
    // A layer describes its whole set, also the part outside the box.
    assert_eq!(both[0].set.airspaces, 2);
    assert_eq!(both[0].set.fetched_at, at(4, 0));
    assert_eq!(both[1].set.fetched_at, at(3, 0));

    // Only the sources asked for, and a source nobody stored is not an error.
    let one = layers_in(store, &["dwd-10min", OPENAIP], &alps()).await;
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].set.source, source(OPENAIP));
    assert!(layers_in(store, &[], &alps()).await.is_empty());
    let sets = store
        .airspace_sets(&[source(OPENAIP), source("dwd-10min"), source(CHARTS)])
        .await
        .unwrap();
    let named: Vec<&str> = sets.iter().map(|set| set.source.as_str()).collect();
    assert_eq!(named, [CHARTS, OPENAIP]);

    // A source with nothing in the box is a layer all the same, in its
    // place in the order.
    let hamburg = BoundingBox::new(53.0, 9.0, 54.0, 10.0).unwrap();
    assert_eq!(
        listed(&layers_in(store, &[OPENAIP, CHARTS], &hamburg).await),
        [
            (CHARTS.to_owned(), ids(&["charts:far"])),
            (OPENAIP.to_owned(), ids(&[])),
        ]
    );

    // The limit counts airspaces across the layers and keeps the first. A
    // layer without airspaces does not count, and one with nothing left in
    // it is still listed.
    let sources = [source(OPENAIP), source(CHARTS)];
    let first = store.airspaces_in(&sources, &alps(), 3).await.unwrap();
    assert_eq!(
        listed(&first),
        [
            (CHARTS.to_owned(), ids(&["charts:z"])),
            (OPENAIP.to_owned(), ids(&["openaip:10", "openaip:9"])),
        ]
    );
    let one = store.airspaces_in(&sources, &alps(), 1).await.unwrap();
    assert_eq!(
        listed(&one),
        [
            (CHARTS.to_owned(), ids(&["charts:z"])),
            (OPENAIP.to_owned(), ids(&[])),
        ]
    );
    let none = store.airspaces_in(&sources, &alps(), 0).await.unwrap();
    assert_eq!(
        listed(&none),
        [
            (CHARTS.to_owned(), ids(&[])),
            (OPENAIP.to_owned(), ids(&[]))
        ]
    );
    // An empty layer before a full one takes nothing from the limit.
    let south = BoundingBox::new(46.9, 10.9, 47.2, 11.2).unwrap();
    let far = [square("charts:far", 53.4, 9.8, 0.1)];
    save(store, CHARTS, at(5, 0), &far).await;
    let after_empty = store.airspaces_in(&sources, &south, 4).await.unwrap();
    assert_eq!(
        listed(&after_empty),
        [
            (CHARTS.to_owned(), ids(&[])),
            (
                OPENAIP.to_owned(),
                ids(&["openaip:10", "openaip:9", "openaip:B", "openaip:b"])
            ),
        ]
    );
}

pub(crate) async fn a_set_says_where_it_is_whole_as_its_run_said(store: &impl Store) {
    let set = [square("openaip:innsbruck", 47.2, 11.3, 0.1)];

    // Several boxes with awkward decimals, in the order given, and
    // countries in the order given: they are not sorted or merged.
    let two = Coverage {
        boxes: vec![
            BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
            BoundingBox::new(42.300_000_000_000_004, -1.9, 43.1 + 0.2, 3.3).unwrap(),
        ],
        countries: vec!["FR".to_owned(), "ES".to_owned(), "AD".to_owned()],
    };
    store
        .save_airspace_run(&delivered(OPENAIP, at(3, 0), &set), &two, &set)
        .await
        .unwrap();
    assert_eq!(coverage_of(store).await, two);
    let layers = layers_in(store, &[OPENAIP], &everything()).await;
    assert_eq!(layers[0].set.coverage, two);

    // The next run collects for less: its set says less.
    save(store, OPENAIP, at(4, 0), &set).await;
    assert_eq!(coverage_of(store).await, region());

    // A run that does not say where its set is whole stores a set that
    // says nothing, which is not "everywhere".
    store
        .save_airspace_run(
            &delivered(OPENAIP, at(5, 0), &set),
            &Coverage::default(),
            &set,
        )
        .await
        .unwrap();
    assert_eq!(coverage_of(store).await, Coverage::default());
}

pub(crate) async fn pruning_leaves_airspace_alone(store: &impl Store) {
    let september = Utc.with_ymd_and_hms(2026, 9, 15, 3, 0, 0).unwrap();
    let set = [square("openaip:innsbruck", 47.2, 11.3, 0.1)];
    save(store, OPENAIP, september, &set).await;

    store
        .prune_before(Month::new(2026, 11).unwrap())
        .await
        .unwrap();

    // The record of the run is gone with its month. The airspace is not:
    // an old set is what there is until a new one arrives.
    let status = &store.source_status(&[source(OPENAIP)]).await.unwrap()[0];
    assert_eq!(status.last_started, None);
    let layers = layers_in(store, &[OPENAIP], &everything()).await;
    assert_eq!(layers[0].airspaces, set);
    assert_eq!(layers[0].set.fetched_at, september);
}
