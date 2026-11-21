//! The behaviour every [`Store`] must have for what it keeps about
//! forecasts: the run each model of a source was at, as the last healthy
//! look found it.

use chrono::{Duration, TimeZone, Utc};
use venti_domain::{ModelId, SourceId, Timestamp};
use venti_store::{ForecastModel, Month, RunRecord, Store, StoreError, ThermalPart};

use super::{at, run, source};

const OPEN_METEO: &str = "open-meteo";
const OTHER: &str = "other-weather";

// --- Helpers -------------------------------------------------------------

/// A model whose newest run began `hours_ago` before the look at
/// `checked_at` and was ready an hour and a half later.
fn model(source_name: &str, name: &str, checked_at: Timestamp, hours_ago: i64) -> ForecastModel {
    let run = checked_at - Duration::hours(hours_ago);
    ForecastModel {
        source: source(source_name),
        model: ModelId::new(name).unwrap(),
        run,
        available_at: Some(run + Duration::minutes(84)),
        checked_at,
    }
}

async fn models_of(store: &impl Store, sources: &[&str]) -> Vec<ForecastModel> {
    let sources: Vec<SourceId> = sources.iter().map(|name| source(name)).collect();
    store.forecast_models(&sources).await.unwrap()
}

async fn save(
    store: &impl Store,
    source_name: &str,
    checked_at: Timestamp,
    models: &[ForecastModel],
) {
    store
        .save_forecast_check(&run(source_name, checked_at), models)
        .await
        .unwrap();
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn what_a_look_at_the_forecast_models_found_is_read_back(store: &impl Store) {
    let unsaid = ForecastModel {
        available_at: None,
        ..model(OPEN_METEO, "icon-eu", at(18, 0), 6)
    };
    // Handed over in any order, read back by model.
    let found = [unsaid.clone(), model(OPEN_METEO, "icon-d2", at(18, 0), 3)];

    save(store, OPEN_METEO, at(18, 0), &found).await;

    let stored = models_of(store, &[OPEN_METEO]).await;
    assert_eq!(stored, [found[1].clone(), unsaid]);
    assert_eq!(stored[0].run, at(15, 0));
    assert_eq!(stored[0].available_at, Some(at(16, 24)));
    // The look is in the history like any run.
    let status = &store.source_status(&[source(OPEN_METEO)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(18, 0)));
    assert_eq!(status.last_success, Some(at(18, 1)));
    // A source nobody has looked at has no models.
    assert!(models_of(store, &[OTHER]).await.is_empty());
    assert!(models_of(store, &[]).await.is_empty());
}

pub(crate) async fn a_healthy_look_replaces_what_is_known_about_the_models(store: &impl Store) {
    save(
        store,
        OPEN_METEO,
        at(18, 0),
        &[
            model(OPEN_METEO, "icon-d2", at(18, 0), 3),
            model(OPEN_METEO, "icon-eu", at(18, 0), 6),
        ],
    )
    .await;
    save(
        store,
        OTHER,
        at(18, 0),
        &[model(OTHER, "icon-d2", at(18, 0), 9)],
    )
    .await;

    // Ten minutes later the fine model has a new run, and the service no
    // longer has the coarse one.
    let newer = [model(OPEN_METEO, "icon-d2", at(18, 10), 0)];
    save(store, OPEN_METEO, at(18, 10), &newer).await;

    assert_eq!(models_of(store, &[OPEN_METEO]).await, newer);
    // The other source's models are its own.
    let both = models_of(store, &[OPEN_METEO, OTHER]).await;
    assert_eq!(both.len(), 2);
    assert_eq!(both[1].source, source(OTHER));
    assert_eq!(both[1].checked_at, at(18, 0));
}

pub(crate) async fn a_failed_or_older_look_changes_nothing_about_the_models(store: &impl Store) {
    let failed = |started_at: Timestamp| RunRecord {
        healthy: false,
        failed_requests: 1,
        ..run(OPEN_METEO, started_at)
    };

    // A source whose first look fails has no models.
    store
        .save_forecast_check(&failed(at(17, 50)), &[])
        .await
        .unwrap();
    assert!(models_of(store, &[OPEN_METEO]).await.is_empty());

    let known = [
        model(OPEN_METEO, "icon-d2", at(18, 0), 3),
        model(OPEN_METEO, "icon-eu", at(18, 0), 6),
    ];
    save(store, OPEN_METEO, at(18, 0), &known).await;

    // The service does not answer: what was known stays known, with the
    // time it was found out.
    store
        .save_forecast_check(&failed(at(18, 10)), &[])
        .await
        .unwrap();
    // Half an answer from a failed look is not looked at.
    store
        .save_forecast_check(
            &failed(at(18, 20)),
            &[model(OPEN_METEO, "icon-eu", at(18, 20), 0)],
        )
        .await
        .unwrap();
    assert_eq!(models_of(store, &[OPEN_METEO]).await, known);
    let status = &store.source_status(&[source(OPEN_METEO)]).await.unwrap()[0];
    assert_eq!(status.last_success, Some(at(18, 1)));
    assert_eq!(status.last_failure, Some(at(18, 21)));

    // A look that started earlier and is stored late replaces nothing.
    save(
        store,
        OPEN_METEO,
        at(17, 0),
        &[model(OPEN_METEO, "icon-d2", at(17, 0), 5)],
    )
    .await;
    assert_eq!(models_of(store, &[OPEN_METEO]).await, known);
    // And saving the same look again changes nothing.
    save(store, OPEN_METEO, at(18, 0), &known).await;
    assert_eq!(models_of(store, &[OPEN_METEO]).await, known);
}

pub(crate) async fn a_bad_look_at_the_forecast_models_is_refused(store: &impl Store) {
    let known = [model(OPEN_METEO, "icon-d2", at(18, 0), 3)];
    save(store, OPEN_METEO, at(18, 0), &known).await;
    let refused = |result: Result<(), StoreError>| match result {
        Err(StoreError::Invalid(problem)) => problem,
        other => panic!("expected a refusal, got {other:?}"),
    };
    let next = run(OPEN_METEO, at(18, 10));

    // A healthy look that found no model would erase what is known.
    let problem = refused(store.save_forecast_check(&next, &[]).await);
    assert!(problem.contains("no model"), "{problem}");

    // A model of another source.
    let stranger = [model(OTHER, "icon-d2", at(18, 10), 0)];
    let problem = refused(store.save_forecast_check(&next, &stranger).await);
    assert!(problem.contains("other-weather"), "{problem}");

    // A model looked at at another time than the run's.
    let stale = [model(OPEN_METEO, "icon-d2", at(18, 0), 0)];
    let problem = refused(store.save_forecast_check(&next, &stale).await);
    assert!(problem.contains("looked at"), "{problem}");

    // The same model twice.
    let twice = [
        model(OPEN_METEO, "icon-d2", at(18, 10), 0),
        model(OPEN_METEO, "icon-d2", at(18, 10), 3),
    ];
    let problem = refused(store.save_forecast_check(&next, &twice).await);
    assert!(problem.contains("twice"), "{problem}");

    // Nothing of the four attempts was stored, not even the run.
    assert_eq!(models_of(store, &[OPEN_METEO]).await, known);
    let status = &store.source_status(&[source(OPEN_METEO)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(at(18, 0)));
}

pub(crate) async fn pruning_leaves_the_forecast_models_alone(store: &impl Store) {
    let known = [model(OPEN_METEO, "icon-d2", at(18, 0), 3)];
    save(store, OPEN_METEO, at(18, 0), &known).await;

    // Everything before 2027 goes: the look of November 2026 with it.
    let new_year = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
    store.prune_before(Month::of(new_year)).await.unwrap();

    let status = &store.source_status(&[source(OPEN_METEO)]).await.unwrap()[0];
    assert_eq!(status.last_started, None);
    // What was found stays, with its date, until a newer look replaces it.
    assert_eq!(models_of(store, &[OPEN_METEO]).await, known);
}

fn part(source_name: &str, name: &str, written_at: Timestamp, cells: &[u8]) -> ThermalPart {
    ThermalPart {
        source: source(source_name),
        model: ModelId::new(name).unwrap(),
        made_from: written_at - Duration::minutes(70),
        written_at,
        cells: cells.to_vec(),
    }
}

pub(crate) async fn a_part_of_the_thermal_map_is_read_back_by_source_and_model(store: &impl Store) {
    let icon_eu = part(OPEN_METEO, "icon-eu", at(18, 0), &[0x1f, 0x8b, 0, 255]);
    let icon_d2 = part(OPEN_METEO, "icon-d2", at(18, 10), &[1, 2, 3]);
    let elsewhere = part(OTHER, "icon-d2", at(18, 20), &[9]);
    for one in [&icon_eu, &icon_d2, &elsewhere] {
        store.save_thermal_part(one).await.unwrap();
    }

    // By model, the bytes as they were handed over.
    let stored = store.thermal_parts(&source(OPEN_METEO)).await.unwrap();
    assert_eq!(stored, [icon_d2, icon_eu]);
    let stored = store.thermal_parts(&source(OTHER)).await.unwrap();
    assert_eq!(stored, [elsewhere]);
    assert!(
        store
            .thermal_parts(&source("nobody"))
            .await
            .unwrap()
            .is_empty()
    );
}

pub(crate) async fn a_later_part_of_the_thermal_map_replaces_an_earlier_one(store: &impl Store) {
    let first = part(OPEN_METEO, "icon-d2", at(18, 0), &[1]);
    let later = part(OPEN_METEO, "icon-d2", at(18, 3), &[2, 2]);
    store.save_thermal_part(&first).await.unwrap();
    store.save_thermal_part(&later).await.unwrap();
    assert_eq!(
        store.thermal_parts(&source(OPEN_METEO)).await.unwrap(),
        std::slice::from_ref(&later)
    );

    // One written before what is stored changes nothing.
    store.save_thermal_part(&first).await.unwrap();
    assert_eq!(
        store.thermal_parts(&source(OPEN_METEO)).await.unwrap(),
        [later]
    );
}
