//! Runs the store's behavioural suite against a real PostgreSQL with PostGIS.
//!
//! The database comes from `VENTI_TEST_DATABASE_URL`, for example
//! `postgres://venti:venti@127.0.0.1:5432/venti`. Without it the tests here
//! are skipped, so `cargo test` works on a machine with no database; CI sets
//! it, and refuses to run without it.
//!
//! Each test works in a schema of its own, named after the test and
//! recreated when the test starts, so tests run in parallel and repeated
//! runs leave nothing behind but those schemas.

mod suite;

use suite::public_setting;
use tokio_postgres::NoTls;
use venti_store::{PgStore, Store, StoreError};

const URL_VARIABLE: &str = "VENTI_TEST_DATABASE_URL";

fn database_url() -> Option<String> {
    match std::env::var(URL_VARIABLE) {
        Ok(url) if !url.is_empty() => Some(url),
        _ => {
            assert!(
                std::env::var_os("CI").is_none(),
                "{URL_VARIABLE} is not set: CI must run the PostgreSQL suite"
            );
            eprintln!("skipped: {URL_VARIABLE} is not set");
            None
        }
    }
}

/// The URL of a fresh schema for one test.
async fn fresh_schema(test: &str) -> Option<String> {
    let url = database_url()?;
    let schema = format!("test_{test}");
    assert!(
        schema.len() <= 63,
        "{schema} is longer than PostgreSQL allows for a name"
    );

    let (admin, connection) = tokio_postgres::connect(&url, NoTls)
        .await
        .unwrap_or_else(|e| panic!("cannot connect to the database in {URL_VARIABLE}: {e}"));
    tokio::spawn(connection);
    admin
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}"
        ))
        .await
        .unwrap();

    let separator = if url.contains('?') { '&' } else { '?' };
    Some(format!(
        "{url}{separator}options=-c%20search_path%3D{schema}%2Cpublic"
    ))
}

async fn fresh(test: &str) -> Option<PgStore> {
    let url = fresh_schema(test).await?;
    Some(PgStore::connect(&url).await.unwrap())
}

suite::store_suite!(fresh);

#[tokio::test]
async fn connecting_again_finds_the_schema_and_the_data() {
    let Some(url) = fresh_schema("connecting_again").await else {
        return;
    };
    let source = venti_domain::SourceId::new("dwd-10min").unwrap();

    // Several servers starting at once all try to migrate.
    let (first, second, third) = tokio::join!(
        PgStore::connect(&url),
        PgStore::connect(&url),
        PgStore::connect(&url)
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    third.unwrap();

    let started_at = chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap();
    let run = venti_store::RunRecord {
        source: source.clone(),
        started_at,
        finished_at: started_at,
        healthy: true,
        requests: 1,
        failed_requests: 0,
        skipped: 0,
        rejected: 0,
        stations: 0,
        observations: 0,
        newest_observation: None,
        airspaces: 0,
    };
    first.save_run(&run, &[], &[]).await.unwrap();

    let status = second.source_status(&[source]).await.unwrap();
    assert_eq!(status[0].last_started, Some(started_at));
}

#[tokio::test]
async fn an_unreachable_database_is_reported_as_unavailable() {
    // Nothing listens on port 1.
    let result = PgStore::connect("postgres://venti:secret@127.0.0.1:1/venti").await;
    match result {
        Err(StoreError::Unavailable(message)) => {
            assert!(!message.contains("secret"), "{message}");
        }
        other => panic!(
            "expected the store to be unavailable, got {:?}",
            other.err()
        ),
    }
}

#[tokio::test]
async fn a_malformed_url_is_refused_without_repeating_it() {
    let result = PgStore::connect("postgres://venti:secret@@@/??").await;
    match result {
        Err(StoreError::Invalid(message)) => assert!(!message.contains("secret"), "{message}"),
        other => panic!("expected a refusal, got {:?}", other.err()),
    }
}

/// A plain connection to a test's schema, for looking at the tables.
async fn look(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

#[tokio::test]
async fn a_database_from_before_accounts_is_upgraded_in_place() {
    let Some(url) = fresh_schema("upgrade_from_1").await else {
        return;
    };
    // A database as the first version of the server left it, with data.
    let old = look(&url).await;
    old.batch_execute(include_str!(
        "../migrations/0001_stations_observations_runs.sql"
    ))
    .await
    .unwrap();
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name) VALUES (1, 'first');
         INSERT INTO source_runs VALUES
             ('dwd-10min', '2026-11-18 18:00+00', '2026-11-18 18:01+00',
              true, 1, 0, 0, 0, 3, 9, NULL);",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    let source = venti_domain::SourceId::new("dwd-10min").unwrap();
    let status = store.source_status(&[source]).await.unwrap();
    assert_eq!(status[0].stations, 3, "what was there is still there");
    assert_eq!(store.account("dario").await.unwrap(), None);
    let versions = old
        .query(
            "SELECT version FROM schema_migrations ORDER BY version",
            &[],
        )
        .await
        .unwrap();
    let versions: Vec<i32> = versions.iter().map(|row| row.get(0)).collect();
    assert_eq!(
        versions,
        [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32
        ]
    );
}

#[tokio::test]
async fn accounts_from_before_the_setting_make_their_new_flights_public() {
    let Some(url) = fresh_schema("upgrade_from_26").await else {
        return;
    };
    // A database as the server left it before migration 27, with an
    // account in it: the newest schema with that migration and the ones
    // after it taken back.
    drop(PgStore::connect(&url).await.unwrap());
    let old = look(&url).await;
    old.batch_execute(
        "ALTER TABLE accounts DROP COLUMN new_flights_public;
         ALTER TABLE accounts DROP COLUMN country;
         ALTER TABLE accounts DROP COLUMN glider_class;
         ALTER TABLE flights DROP COLUMN glider_class;
         DROP TABLE pictures;
         ALTER TABLE accounts DROP COLUMN google_picture_tried;
         ALTER TABLE accounts DROP COLUMN shown_near_places;
         ALTER TABLE flights DROP COLUMN habits;
         DROP TABLE open_plan_pilots;
         DROP TABLE open_plans;
         DROP TABLE help_recipients;
         DROP TABLE help_requests;
         DROP TABLE help_positions;
         DROP TABLE nearby_settings;
         DROP TABLE phone_devices;
         DELETE FROM schema_migrations WHERE version >= 27;
         INSERT INTO accounts (id, handle, display_name, created_at)
             VALUES ('000000000000000000000dario', 'dario', 'Dario B.', '2026-11-18 09:00+00');",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    // Owner decision 21: on for every account, those from before included.
    let account = store.account("dario").await.unwrap().unwrap();
    assert!(account.new_flights_public);
    // And a pilot can switch it off.
    let off = store
        .update_account_settings(&account.id, &public_setting(false))
        .await
        .unwrap()
        .unwrap();
    assert!(!off.new_flights_public);
    assert_eq!(store.account("dario").await.unwrap(), Some(off));
}

#[tokio::test]
async fn a_database_from_before_airspace_is_upgraded_in_place() {
    let Some(url) = fresh_schema("upgrade_from_2").await else {
        return;
    };
    // A database as the server left it before it collected airspace, with
    // a run in its history.
    let old = look(&url).await;
    old.batch_execute(include_str!(
        "../migrations/0001_stations_observations_runs.sql"
    ))
    .await
    .unwrap();
    old.batch_execute(include_str!(
        "../migrations/0002_accounts_sessions_flights.sql"
    ))
    .await
    .unwrap();
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name) VALUES (1, 'first'), (2, 'second');
         INSERT INTO source_runs VALUES
             ('dwd-10min', '2026-11-18 18:00+00', '2026-11-18 18:01+00',
              true, 1, 0, 0, 0, 3, 9, NULL);",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    let dwd = venti_domain::SourceId::new("dwd-10min").unwrap();
    let status = store
        .source_status(std::slice::from_ref(&dwd))
        .await
        .unwrap();
    assert_eq!(status[0].stations, 3, "what was there is still there");
    // The old run delivered no airspace, and no source has a set yet.
    let airspaces: i32 = old
        .query_one("SELECT airspaces FROM source_runs", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(airspaces, 0);
    assert!(store.airspace_sets(&[dwd]).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_airspace_set_from_before_sets_said_where_they_are_whole_says_nothing() {
    let Some(url) = fresh_schema("upgrade_from_3").await else {
        return;
    };
    // A database as the server left it when it first collected airspace,
    // with a set in it.
    let old = look(&url).await;
    for migration in [
        include_str!("../migrations/0001_stations_observations_runs.sql"),
        include_str!("../migrations/0002_accounts_sessions_flights.sql"),
        include_str!("../migrations/0003_airspace.sql"),
    ] {
        old.batch_execute(migration).await.unwrap();
    }
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name)
             VALUES (1, 'first'), (2, 'second'), (3, 'third');
         INSERT INTO airspace_sets VALUES
             ('openaip', '2026-11-18 03:00+00', 344, '2026-11-18 01:12:30+00', NULL);",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    let openaip = venti_domain::SourceId::new("openaip").unwrap();
    let sets = store.airspace_sets(&[openaip]).await.unwrap();
    assert_eq!(sets[0].airspaces, 344, "what was there is still there");
    // Nothing is made up for it: until the next run it covers nothing.
    assert_eq!(sets[0].coverage, venti_store::Coverage::default());
}

#[tokio::test]
async fn the_box_query_for_airspace_uses_the_spatial_index() {
    let Some(url) = fresh_schema("airspace_index").await else {
        return;
    };
    PgStore::connect(&url).await.unwrap();
    let tables = look(&url).await;
    // With a handful of rows the planner would read the table from end to
    // end whatever indexes exist; forbidding that shows whether the index
    // can answer the query at all.
    tables
        .batch_execute("SET enable_seqscan = off")
        .await
        .unwrap();
    let plan = tables
        .query(
            // The store's own question, with a box written in.
            "EXPLAIN WITH found AS (
                 SELECT a.source, a.id, a.record FROM airspaces a
                 WHERE a.source = ANY(ARRAY['openaip'])
                   AND ST_MakeEnvelope(a.west, a.south, a.east, a.north, 4326)
                       && ST_MakeEnvelope(9.4, 46.3, 17.2, 48.5, 4326)
                   AND a.west <= 17.2 AND a.east >= 9.4
                   AND a.south <= 48.5 AND a.north >= 46.3
                 ORDER BY a.source, a.id
                 LIMIT 5001
             )
             SELECT f.record, s.source FROM airspace_sets s
             LEFT JOIN found f ON f.source = s.source
             WHERE s.source = ANY(ARRAY['openaip'])
             ORDER BY s.source, f.id",
            &[],
        )
        .await
        .unwrap();
    let plan: Vec<String> = plan.iter().map(|row| row.get(0)).collect();
    assert!(
        plan.iter().any(|line| line.contains("airspaces_box_idx")),
        "{plan:#?}"
    );
}

#[tokio::test]
async fn a_database_from_before_live_tracks_is_upgraded_in_place() {
    let Some(url) = fresh_schema("upgrade_from_6").await else {
        return;
    };
    // A database as the server left it before live tracks, with an account.
    let old = look(&url).await;
    for migration in [
        include_str!("../migrations/0001_stations_observations_runs.sql"),
        include_str!("../migrations/0002_accounts_sessions_flights.sql"),
        include_str!("../migrations/0003_airspace.sql"),
        include_str!("../migrations/0004_airspace_coverage.sql"),
        include_str!("../migrations/0005_sites.sql"),
        include_str!("../migrations/0006_forecast_models.sql"),
    ] {
        old.batch_execute(migration).await.unwrap();
    }
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name)
             VALUES (1, 'a'), (2, 'b'), (3, 'c'), (4, 'd'), (5, 'e'), (6, 'f');
         INSERT INTO accounts (id, handle, display_name, created_at)
             VALUES ('000000000000000000000dario', 'dario', 'Dario B.', '2026-11-18 09:00+00');",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    let dario = store.account("dario").await.unwrap().unwrap();
    assert_eq!(
        dario.display_name, "Dario B.",
        "what was there is still there"
    );
    // The account can share a live track at once.
    let time = chrono::DateTime::from_timestamp(1_795_000_000, 0).unwrap();
    let batch = venti_store::LiveBatch {
        id: "0000000000000000000000live".to_owned(),
        account_id: dario.id,
        visibility: venti_store::LiveVisibility::Public,
        landed: false,
        fixes: vec![venti_store::LiveFix {
            time,
            position: venti_domain::GeoPoint::new(47.66, 11.8).unwrap(),
            altitude_m: 1600,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        }],
    };
    let room = venti_store::LiveRoom {
        fixes_per_track: 10,
        may_create: true,
        expired_before: time,
        now: time,
    };
    let saved = store.save_live_fixes(&batch, &room).await.unwrap();
    assert!(matches!(saved, venti_store::LiveSaved::Kept(kept) if kept.created));
}

#[tokio::test]
async fn the_questions_about_live_tracks_use_their_indexes() {
    let Some(url) = fresh_schema("live_indexes").await else {
        return;
    };
    PgStore::connect(&url).await.unwrap();
    let tables = look(&url).await;
    // As for airspace: with empty tables the planner would read them from
    // end to end; forbidding that shows whether an index can answer.
    tables
        .batch_execute("SET enable_seqscan = off")
        .await
        .unwrap();
    let plan_of = |query: &'static str| {
        let tables = &tables;
        async move {
            let plan = tables.query(query, &[]).await.unwrap();
            let plan: Vec<String> = plan.iter().map(|row| row.get(0)).collect();
            plan.join("\n")
        }
    };

    // The store's own questions, with their parameters written in. The
    // public tracks that had a fix lately, which are few and are sorted
    // once found:
    let plan = plan_of(
        "EXPLAIN SELECT t.id FROM live_tracks t JOIN accounts a ON a.id = t.account_id
         WHERE t.visibility = 'public' AND t.last_at >= '2026-11-18 18:35+00'
           AND t.last_lon BETWEEN 11.0 AND 12.0 AND t.last_lat BETWEEN 47.0 AND 48.0
         ORDER BY t.id LIMIT 1001",
    )
    .await;
    assert!(plan.contains("live_tracks_public_idx"), "{plan}");
    // The tracks that ended long ago:
    let plan =
        plan_of("EXPLAIN DELETE FROM live_tracks WHERE last_at < '2026-11-16 19:05+00'").await;
    assert!(plan.contains("live_tracks_last_idx"), "{plan}");

    // A track has tens of thousands of fixes, and its questions ask for a
    // few of them in an order. With nothing to sort the planner would as
    // soon sort as read in order; forbidding that too shows whether an
    // index hands the fixes out in the order asked for.
    tables
        .batch_execute("SET enable_sort = off; SET enable_bitmapscan = off")
        .await
        .unwrap();
    // What arrived for a track after a number:
    let plan = plan_of(
        "EXPLAIN SELECT f.seq FROM live_fixes f
         WHERE f.track = (SELECT key FROM live_tracks WHERE id = '0000000000000000000000live')
           AND f.seq > 1200 AND (NULL::timestamptz IS NULL OR f.time > NULL)
         ORDER BY f.seq LIMIT 5001",
    )
    .await;
    assert!(plan.contains("live_fixes_seq_idx"), "{plan}");
    assert!(!plan.contains("Sort"), "{plan}");
    // The end of a track:
    let plan = plan_of(
        "EXPLAIN SELECT f.seq FROM live_fixes f
         WHERE f.track = (SELECT key FROM live_tracks WHERE id = '0000000000000000000000live')
         ORDER BY f.time DESC LIMIT 2000",
    )
    .await;
    assert!(plan.contains("live_fixes_pkey"), "{plan}");
    assert!(!plan.contains("Sort"), "{plan}");
}

#[tokio::test]
async fn a_database_from_before_reports_is_upgraded_in_place() {
    let Some(url) = fresh_schema("upgrade_from_7").await else {
        return;
    };
    // A database as the server left it before there was a list of public
    // flights, a report or a take-down: with an account, a public flight
    // and a flight that was deleted.
    let old = look(&url).await;
    for migration in [
        include_str!("../migrations/0001_stations_observations_runs.sql"),
        include_str!("../migrations/0002_accounts_sessions_flights.sql"),
        include_str!("../migrations/0003_airspace.sql"),
        include_str!("../migrations/0004_airspace_coverage.sql"),
        include_str!("../migrations/0005_sites.sql"),
        include_str!("../migrations/0006_forecast_models.sql"),
        include_str!("../migrations/0007_live_tracks.sql"),
    ] {
        old.batch_execute(migration).await.unwrap();
    }
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name)
             VALUES (1, 'a'), (2, 'b'), (3, 'c'), (4, 'd'), (5, 'e'), (6, 'f'), (7, 'g');
         INSERT INTO accounts (id, handle, display_name, created_at, flight_change)
             VALUES ('000000000000000000000dario', 'dario', 'Dario B.', '2026-11-18 09:00+00', 7);
         INSERT INTO flights
             (id, account_id, identity, visibility, uploaded_at, change, file, file_bytes,
              analysis_version, started_at, airtime_s, takeoff_at, takeoff_lat, takeoff_lon,
              takeoff_altitude_m, track_length_m, max_altitude_m, max_climb_ms, score_kind,
              score_closed, score_distance_m, score_points, outline, profile)
         VALUES ('0000000000000000000flight1', '000000000000000000000dario',
                 '00000000000000000000000000000001', 'public', '2026-11-18 12:00+00', 5,
                 '\\x1f8b'::bytea, 60000, 1, '2026-08-15 10:30+00', 1752,
                 '2026-08-15 10:30+00', 47.6642, 11.7967, 1620, 18402.3, 2117, 2.4,
                 'free_flight', false, 11975.7, 11.98, '{47.6642,11.7967}', '{0,1620}');
         INSERT INTO flight_deletions (account_id, flight_id, identity, change, deleted_at)
         VALUES ('000000000000000000000dario', '0000000000000000000flight2',
                 '00000000000000000000000000000002', 7, '2026-11-18 13:00+00');",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    // What was there is still there, and nothing was done to it: the
    // account may publish, and the flight is public and not taken down.
    let dario = store.account("dario").await.unwrap().unwrap();
    assert_eq!(dario.publishing_suspended_at, None);
    let flight = store
        .flight("0000000000000000000flight1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(flight.visibility, venti_store::Visibility::Public);
    assert_eq!(flight.taken_down_at, None);
    assert_eq!(flight.change, 5);
    let changes = store.flight_changes(&dario.id, 0, 10).await.unwrap();
    assert_eq!(changes.deleted.len(), 1);
    // The flight is in the list of public flights, found by where it took
    // off: the point of its take-off was filled in for it.
    let around = venti_store::PublicFlights {
        area: Some(venti_domain::BoundingBox::new(47.6, 11.7, 47.7, 11.9).unwrap()),
        ..venti_store::PublicFlights::default()
    };
    let newest = venti_store::PublicOrder::Newest(None);
    let listed = store.public_flights(&around, &newest, 10).await.unwrap();
    assert_eq!(listed, std::slice::from_ref(&flight));
    // Nothing is reported, and the new operations work on the old rows.
    let reports = store
        .reports(venti_store::ReportListing::All, None, 10)
        .await
        .unwrap();
    assert!(reports.is_empty());
    let now = chrono::DateTime::from_timestamp(1_795_100_000, 0).unwrap();
    let down = store.take_down_flight(&flight.id, now).await.unwrap();
    assert_eq!(down.unwrap().taken_down_at, Some(now));
    let listed = store.public_flights(&around, &newest, 10).await.unwrap();
    assert!(listed.is_empty());
}

/// The store's question for a page of public flights, as it writes it,
/// with the values written in: what the page is cut from, and its order.
fn public_flights_question(conditions: &str, order: &str) -> String {
    format!(
        "EXPLAIN SELECT f.id, a.handle FROM flights f JOIN accounts a ON a.id = f.account_id
         WHERE f.visibility = 'public' {conditions}
         ORDER BY {order}
         LIMIT 51"
    )
}

#[tokio::test]
async fn public_flights_from_before_get_a_moment_and_a_number() {
    let Some(url) = fresh_schema("upgrade_from_15").await else {
        return;
    };
    // A database as the server left it before a flight had a life as
    // public data: two public flights, one shared by link, one private.
    let old = look(&url).await;
    for migration in [
        include_str!("../migrations/0001_stations_observations_runs.sql"),
        include_str!("../migrations/0002_accounts_sessions_flights.sql"),
        include_str!("../migrations/0003_airspace.sql"),
        include_str!("../migrations/0004_airspace_coverage.sql"),
        include_str!("../migrations/0005_sites.sql"),
        include_str!("../migrations/0006_forecast_models.sql"),
        include_str!("../migrations/0007_live_tracks.sql"),
        include_str!("../migrations/0008_public_flights.sql"),
        include_str!("../migrations/0009_reports_and_takedowns.sql"),
        include_str!("../migrations/0010_groups_and_plans.sql"),
        include_str!("../migrations/0011_thermal_map_parts.sql"),
        include_str!("../migrations/0012_reports_about_groups.sql"),
        include_str!("../migrations/0013_pilot_sites.sql"),
        include_str!("../migrations/0014_access_tokens.sql"),
        include_str!("../migrations/0015_reports_about_sites.sql"),
    ] {
        old.batch_execute(migration).await.unwrap();
    }
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name)
             SELECT n, 'migration ' || n FROM generate_series(1, 15) AS n;
         INSERT INTO accounts (id, handle, display_name, created_at, flight_change)
             VALUES ('000000000000000000000dario', 'dario', 'Dario B.', '2026-11-18 09:00+00', 9);
         INSERT INTO flights
             (id, account_id, identity, visibility, uploaded_at, change, file, file_bytes,
              analysis_version, started_at, airtime_s, takeoff_at, takeoff_lat, takeoff_lon,
              takeoff_altitude_m, track_length_m, max_altitude_m, max_climb_ms, outline, profile)
         SELECT '0000000000000000000flight' || n, '000000000000000000000dario',
                lpad(n::text, 32, '0'),
                (ARRAY['public', 'unlisted', 'public', 'private'])[n],
                -- The third was uploaded before the first.
                timestamptz '2026-11-18 12:00+00' - n * interval '1 hour', n,
                '\\x1f8b'::bytea, 60000, 1, '2026-08-15 10:30+00', 1752,
                '2026-08-15 10:30+00', 47.6642, 11.7967, 1620, 18402.3, 2117, 2.4,
                '{47.6642,11.7967}', '{0,1620}'
         FROM generate_series(1, 4) AS n;",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    let flight = |n: u8| {
        let store = &store;
        async move {
            let id = format!("0000000000000000000flight{n}");
            store.flight(&id).await.unwrap().unwrap()
        }
    };
    // A public flight is public since its upload, the earliest it can
    // have been: the table kept nothing nearer to the moment.
    let (first, by_link, third, private) = (
        flight(1).await,
        flight(2).await,
        flight(3).await,
        flight(4).await,
    );
    assert_eq!(first.published_at, Some(first.uploaded_at));
    assert_eq!(third.published_at, Some(third.uploaded_at));
    for other in [&by_link, &private] {
        assert_eq!((other.published_at, other.public_change), (None, None));
    }
    // Their numbers are in the order of their uploads, and the count
    // stands at the larger.
    let (third_change, first_change) = (third.public_change.unwrap(), first.public_change.unwrap());
    assert_eq!(first_change, third_change + 1);
    let changes = store.public_flight_changes(None, 0, 10).await.unwrap();
    let listed: Vec<&str> = changes.flights.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(
        listed,
        ["0000000000000000000flight3", "0000000000000000000flight1"]
    );
    assert!(changes.withdrawn.is_empty());
    assert_eq!(changes.latest, first_change);

    // From here the count goes on: a withdrawal takes a larger number.
    let now = chrono::Utc::now() + chrono::Duration::minutes(1);
    store
        .set_visibility(
            "000000000000000000000dario",
            &first.id,
            venti_store::Visibility::Private,
            now,
        )
        .await
        .unwrap();
    let changes = store
        .public_flight_changes(None, first_change, 10)
        .await
        .unwrap();
    assert!(changes.flights.is_empty());
    assert_eq!(changes.withdrawn.len(), 1);
    assert_eq!(changes.withdrawn[0].id, first.id);
    assert!(changes.withdrawn[0].change > first_change);
}

#[tokio::test]
async fn the_questions_about_public_flights_use_their_indexes() {
    let Some(url) = fresh_schema("public_flight_indexes").await else {
        return;
    };
    PgStore::connect(&url).await.unwrap();
    let tables = look(&url).await;
    // Twenty thousand flights of one pilot over two years, from thousands
    // of take-offs between Lake Constance and Vienna; three in four are
    // public. Enough for the planner to choose as it would on a server,
    // which it is left to do: nothing is switched off here.
    tables
        .batch_execute(
            "INSERT INTO accounts (id, handle, display_name, created_at)
             VALUES ('000000000000000000000pilot', 'pilot', 'Pilot', '2025-01-01 00:00+00');
             INSERT INTO flights
                 (id, account_id, identity, visibility, uploaded_at, change, file, file_bytes,
                  analysis_version, started_at, airtime_s, takeoff_at, takeoff_lat, takeoff_lon,
                  takeoff_altitude_m, track_length_m, max_altitude_m, max_climb_ms, score_kind,
                  score_closed, score_distance_m, score_points, outline, profile,
                  published_at, public_change)
             SELECT lpad(n::text, 26, '0'), '000000000000000000000pilot', lpad(n::text, 32, '0'),
                    CASE WHEN n % 4 = 0 THEN 'private' ELSE 'public' END,
                    '2026-11-18 19:05+00', n, ''::bytea, 60000, 1,
                    timestamptz '2025-01-01 09:00+00' + n * interval '53 minutes', 3600,
                    timestamptz '2025-01-01 09:00+00' + n * interval '53 minutes',
                    46.5 + (n % 200) * 0.01, 9.5 + (n % 173) * 0.04, 1500,
                    20000, 2400, 2.5, 'free_flight', false,
                    (n % 997) * 100.0, (n % 997) / 10.0, '{}', '{}',
                    CASE WHEN n % 4 = 0 THEN NULL ELSE timestamptz '2026-11-18 19:05+00' END,
                    CASE WHEN n % 4 = 0 THEN NULL ELSE n END
             FROM generate_series(1, 20000) AS n;
             ANALYZE accounts;
             ANALYZE flights;",
        )
        .await
        .unwrap();
    let plan_of = |conditions: &'static str, order: &'static str| {
        let tables = &tables;
        async move {
            let question = public_flights_question(conditions, order);
            let plan = tables.query(&question, &[]).await.unwrap();
            let plan: Vec<String> = plan.iter().map(|row| row.get(0)).collect();
            plan.join("\n")
        }
    };
    let newest = "f.started_at DESC, f.id DESC";
    let best = "f.score_points DESC, f.id DESC";
    // No question reads the table from end to end, and each of these is
    // answered by walking an index in the order asked for, so that the
    // first page costs what a page holds and not what the table holds.
    let in_order = |plan: &str, index: &str| {
        assert!(plan.contains(index), "{index} is not used:\n{plan}");
        assert!(!plan.contains("Sort"), "{plan}");
        assert!(!plan.contains("Seq Scan on flights"), "{plan}");
    };

    // The newest, the page after a flight, and the flights of a day.
    in_order(&plan_of("", newest).await, "flights_public_newest_idx");
    let after = "AND (f.started_at, f.id) < ('2026-06-01 10:00+00', '00000000000000000000012345')";
    in_order(&plan_of(after, newest).await, "flights_public_newest_idx");
    let a_day =
        "AND f.started_at >= '2026-06-01 00:00+00' AND f.started_at < '2026-06-02 00:00+00'";
    in_order(&plan_of(a_day, newest).await, "flights_public_newest_idx");

    // The best, and the page after a flight.
    let scored = "AND f.score_points IS NOT NULL";
    in_order(&plan_of(scored, best).await, "flights_public_score_idx");
    let after = "AND f.score_points IS NOT NULL AND (f.score_points, f.id) < (42.5, '00000000000000000000012345')";
    in_order(&plan_of(after, best).await, "flights_public_score_idx");

    // The best of one day: the day's few flights are found by their start
    // and then sorted, which is less work than walking every flight there
    // is from the best down.
    let plan = plan_of(
        "AND f.score_points IS NOT NULL AND f.started_at >= '2026-06-01 00:00+00'
         AND f.started_at < '2026-06-02 00:00+00'",
        best,
    )
    .await;
    assert!(plan.contains("flights_public_newest_idx"), "{plan}");
    assert!(!plan.contains("Seq Scan on flights"), "{plan}");

    // A box around one take-off holds a few flights: they are found by
    // where they took off.
    let one_place = "AND f.takeoff
             && ST_MakeEnvelope(11.69, 47.59, 11.71, 47.61, 4326)
         AND f.takeoff_lon BETWEEN 11.69 AND 11.71 AND f.takeoff_lat BETWEEN 47.59 AND 47.61";
    for order in [newest, best] {
        let plan = plan_of(one_place, order).await;
        assert!(plan.contains("flights_public_takeoff_idx"), "{plan}");
        assert!(!plan.contains("Seq Scan on flights"), "{plan}");
    }
    // A box around everything holds every flight: the list is walked in
    // its order instead, as without a box.
    let everything = "AND f.takeoff
             && ST_MakeEnvelope(5.0, 44.0, 18.0, 50.0, 4326)
         AND f.takeoff_lon BETWEEN 5.0 AND 18.0 AND f.takeoff_lat BETWEEN 44.0 AND 50.0";
    in_order(
        &plan_of(everything, newest).await,
        "flights_public_newest_idx",
    );
}

#[tokio::test]
async fn deleting_an_account_leaves_no_row_that_names_it() {
    let Some(url) = fresh_schema("delete_leaves_no_row").await else {
        return;
    };
    let store = PgStore::connect(&url).await.unwrap();
    // The suite's behaviour builds an account with a password, an invite,
    // sessions, flights and a deleted flight, deletes it, and then creates
    // another account with the same handle.
    suite::a_deleted_account_leaves_nothing_behind(&store).await;

    // The suite asked through the interface. Here every table is searched
    // for the account's identifier and its flights' addresses, in any
    // column.
    let tables = look(&url).await;
    let names = tables
        .query(
            "SELECT table_name FROM information_schema.tables
             WHERE table_schema = current_schema() AND table_type = 'BASE TABLE'",
            &[],
        )
        .await
        .unwrap();
    assert!(names.len() >= 8, "the schema has {} tables", names.len());
    let account = format!("\"{:0>26}\"", "dario");
    for row in names {
        let table: &str = row.get(0);
        let mentions: i64 = tables
            .query_one(
                &format!(
                    "SELECT count(*) FROM {table} t
                     WHERE to_jsonb(t)::text LIKE '%' || $1 || '%'
                        OR to_jsonb(t)::text LIKE '%0dario1\"%'
                        OR to_jsonb(t)::text LIKE '%0dario2\"%'"
                ),
                &[&account],
            )
            .await
            .unwrap()
            .get(0);
        assert_eq!(mentions, 0, "{table} still names the deleted account");
    }
    // The invite it used is still there, used, and names nobody.
    let invite = tables
        .query_one("SELECT used_at IS NOT NULL, used_by FROM invites", &[])
        .await
        .unwrap();
    assert!(invite.get::<_, bool>(0));
    assert_eq!(invite.get::<_, Option<String>>(1), None);
}

/// Every migration before groups, in order.
const BEFORE_GROUPS: [&str; 9] = [
    include_str!("../migrations/0001_stations_observations_runs.sql"),
    include_str!("../migrations/0002_accounts_sessions_flights.sql"),
    include_str!("../migrations/0003_airspace.sql"),
    include_str!("../migrations/0004_airspace_coverage.sql"),
    include_str!("../migrations/0005_sites.sql"),
    include_str!("../migrations/0006_forecast_models.sql"),
    include_str!("../migrations/0007_live_tracks.sql"),
    include_str!("../migrations/0008_public_flights.sql"),
    include_str!("../migrations/0009_reports_and_takedowns.sql"),
];

#[tokio::test]
async fn a_database_from_before_groups_is_upgraded_in_place() {
    let Some(url) = fresh_schema("upgrade_from_9").await else {
        return;
    };
    // A database as the server left it before there were groups, with two
    // accounts.
    let old = look(&url).await;
    for migration in BEFORE_GROUPS {
        old.batch_execute(migration).await.unwrap();
    }
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name)
             SELECT n, 'migration ' || n FROM generate_series(1, 9) AS n;
         INSERT INTO accounts (id, handle, display_name, created_at) VALUES
             ('000000000000000000000dario', 'dario', 'Dario B.', '2026-11-18 09:00+00'),
             ('0000000000000000000000anna', 'anna', 'Anna A.', '2026-11-18 09:00+00');",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    let dario = store.account("dario").await.unwrap().unwrap();
    let anna = store.account("anna").await.unwrap().unwrap();
    assert_eq!(
        dario.display_name, "Dario B.",
        "what was there is still there"
    );
    // Nobody is in a group, and the accounts that were there can make one
    // and come into it at once.
    assert!(store.groups_of(&dario.id).await.unwrap().is_empty());
    let now = chrono::DateTime::from_timestamp(1_795_100_000, 0).unwrap();
    let invite = venti_store::SecretHash([7; 32]);
    let room = venti_store::GroupRoom {
        groups_per_account: 20,
        members_per_group: 50,
    };
    let new = venti_store::NewGroup {
        id: "0000000000000000000000000g".to_owned(),
        name: "Mustergruppe".to_owned(),
        created_at: now,
        keeper_id: dario.id.clone(),
        invite,
    };
    let made = store.create_group(&new, &room).await.unwrap();
    assert!(matches!(made, venti_store::GroupCreation::Created(_)));
    let joined = store
        .join_group(&anna.id, &invite, &room, now)
        .await
        .unwrap();
    assert!(
        matches!(joined, venti_store::GroupJoining::Joined(group) if group.members == 2),
        "the group takes a second member"
    );
}

#[tokio::test]
async fn a_database_from_before_groups_could_be_reported_is_upgraded_in_place() {
    let Some(url) = fresh_schema("upgrade_from_11").await else {
        return;
    };
    // A database as the server left it while a report was about a flight,
    // a page or a live track: with three accounts, an open report of each
    // kind and a closed one.
    let old = look(&url).await;
    for migration in BEFORE_GROUPS.into_iter().chain([
        include_str!("../migrations/0010_groups_and_plans.sql"),
        include_str!("../migrations/0011_thermal_map_parts.sql"),
    ]) {
        old.batch_execute(migration).await.unwrap();
    }
    old.batch_execute(
        "CREATE TABLE schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         );
         INSERT INTO schema_migrations (version, name)
             SELECT n, 'migration ' || n FROM generate_series(1, 11) AS n;
         INSERT INTO accounts (id, handle, display_name, created_at) VALUES
             ('000000000000000000000dario', 'dario', 'Dario B.', '2026-11-18 09:00+00'),
             ('0000000000000000000000anna', 'anna', 'Anna A.', '2026-11-18 09:00+00'),
             ('000000000000000000000bernd', 'bernd', 'Bernd B.', '2026-11-18 09:00+00');
         INSERT INTO reports
             (id, created_at, reporter_id, about_account_id, kind, subject_id, reason,
              reporter_text, closed_at, closing_note)
         VALUES
             ('000000000000000000000000r1', '2026-11-18 12:00+00', '000000000000000000000dario',
              '0000000000000000000000anna', 'flight', '0000000000000000000flight1',
              'not_a_flight', 'Not hers.', NULL, NULL),
             ('000000000000000000000000r2', '2026-11-18 12:10+00', '000000000000000000000dario',
              '0000000000000000000000anna', 'pilot', '', 'impersonation', NULL, NULL, NULL),
             ('000000000000000000000000r3', '2026-11-18 12:20+00', '0000000000000000000000anna',
              '000000000000000000000dario', 'live_track', '00000000000000000000track1',
              'other', 'It is night.', '2026-11-18 13:00+00', 'Looked at it.');",
    )
    .await
    .unwrap();

    let store = PgStore::connect(&url).await.unwrap();

    // The reports that were there are read as they were.
    let all = venti_store::ReportListing::All;
    let reports = store.reports(all, None, 10).await.unwrap();
    let subjects: Vec<_> = reports
        .iter()
        .map(|report| report.subject.clone())
        .collect();
    assert_eq!(
        subjects,
        [
            venti_store::ReportSubject::Flight("0000000000000000000flight1".to_owned()),
            venti_store::ReportSubject::Pilot,
            venti_store::ReportSubject::LiveTrack("00000000000000000000track1".to_owned()),
        ]
    );
    assert_eq!(reports[0].text.as_deref(), Some("Not hers."));
    assert_eq!(
        reports[2]
            .closed
            .as_ref()
            .map(|closed| closed.note.as_str()),
        Some("Looked at it.")
    );
    // The same thing reported again is still the same report.
    let dario = store.account("dario").await.unwrap().unwrap();
    let anna = store.account("anna").await.unwrap().unwrap();
    let now = chrono::DateTime::from_timestamp(1_795_100_000, 0).unwrap();
    let report = |id: &str, subject: venti_store::ReportSubject| venti_store::NewReport {
        id: id.to_owned(),
        created_at: now,
        reporter_id: dario.id.clone(),
        about_account_id: Some(anna.id.clone()),
        subject,
        reason: "spam".to_owned(),
        text: None,
    };
    let again = report(
        "000000000000000000000000r4",
        venti_store::ReportSubject::Flight("0000000000000000000flight1".to_owned()),
    );
    assert_eq!(
        store.save_report(&again).await.unwrap(),
        venti_store::ReportSaved::AlreadyOpen
    );
    // And a group and a plan can be reported, with their words.
    let group = venti_store::ReportSubject::Group {
        id: "0000000000000000000000000g".to_owned(),
        name: "Mustergruppe".to_owned(),
    };
    let plan = venti_store::ReportSubject::Plan {
        id: "0000000000000000000000000p".to_owned(),
        group_id: "0000000000000000000000000g".to_owned(),
        place: "Musterberg".to_owned(),
        note: None,
    };
    for (id, subject) in [
        ("000000000000000000000000r5", group.clone()),
        ("000000000000000000000000r6", plan.clone()),
    ] {
        assert_eq!(
            store.save_report(&report(id, subject)).await.unwrap(),
            venti_store::ReportSaved::Stored
        );
    }
    let reports = store.reports(all, None, 10).await.unwrap();
    assert_eq!(reports.len(), 5);
    assert_eq!(reports[3].subject, group);
    assert_eq!(reports[4].subject, plan);

    // The database itself holds every writer to what each kind keeps: a
    // group its name, a plan its place and its group, and nothing else
    // any words of that sort. A kind nobody knows is refused as before.
    let insert = |kind: &'static str, columns: &'static str, values: &'static str| {
        format!(
            "INSERT INTO reports
                 (id, created_at, reporter_id, about_account_id, kind, subject_id, reason{columns})
             VALUES ('00000000000000000000000bad', now(), '000000000000000000000dario',
                     '000000000000000000000bernd', '{kind}', 'x', 'spam'{values})"
        )
    };
    for (kind, columns, values) in [
        ("group", "", ""),
        ("group", ", reported_place, group_id", ", 'Musterberg', 'g'"),
        ("plan", "", ""),
        ("plan", ", reported_place", ", 'Musterberg'"),
        ("plan", ", group_id", ", 'g'"),
        ("plan", ", reported_name", ", 'Mustergruppe'"),
        ("flight", ", reported_name", ", 'Mustergruppe'"),
        ("pilot", ", reported_place, group_id", ", 'Musterberg', 'g'"),
        ("live_track", ", reported_note", ", 'A note.'"),
        (
            "group",
            ", reported_name, reported_note",
            ", 'Mustergruppe', 'A note.'",
        ),
        ("site", "", ""),
    ] {
        let refused = old.batch_execute(&insert(kind, columns, values)).await;
        let code = refused
            .expect_err(kind)
            .code()
            .map(|code| code.code().to_owned());
        assert_eq!(code.as_deref(), Some("23514"), "{kind}{columns}");
    }
    // One open report per reporter and group, whoever keeps the group,
    // also for a writer that is not the store.
    let second = insert("group", ", reported_name", ", 'Mustergruppe'")
        .replace("'x'", "'0000000000000000000000000g'");
    let refused = old.batch_execute(&second).await.expect_err("a second");
    assert_eq!(refused.code().map(|code| code.code()), Some("23505"));
    let count: i64 = old
        .query_one("SELECT count(*) FROM reports", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 5, "nothing of that was stored");
}

#[tokio::test]
async fn the_database_itself_keeps_a_group_whole() {
    let Some(url) = fresh_schema("groups_kept_whole").await else {
        return;
    };
    PgStore::connect(&url).await.unwrap();
    let tables = look(&url).await;
    // A group of two with a plan each, each in on the other's, written
    // past the store: what follows must hold for any writer.
    tables
        .batch_execute(
            "INSERT INTO accounts (id, handle, display_name, created_at) VALUES
                 ('a1', 'dario', 'Dario', '2026-11-18 09:00+00'),
                 ('a2', 'anna', 'Anna', '2026-11-18 09:00+00'),
                 ('a3', 'eve', 'Eve', '2026-11-18 09:00+00');
             BEGIN;
             INSERT INTO groups (id, name, created_at, keeper_id)
                 VALUES ('g1', 'Mustergruppe', '2026-11-18 10:00+00', 'a1');
             INSERT INTO group_members (group_id, account_id, joined_at) VALUES
                 ('g1', 'a1', '2026-11-18 10:00+00'), ('g1', 'a2', '2026-11-18 10:05+00');
             COMMIT;
             INSERT INTO group_plans (id, group_id, author_id, day, place, created_at, changed_at)
             VALUES ('p1', 'g1', 'a1', '2026-11-19', 'Musterberg', now(), now()),
                    ('p2', 'g1', 'a2', '2026-11-19', 'Musterberg', now(), now());
             INSERT INTO group_plan_pilots (plan_id, group_id, account_id, since) VALUES
                 ('p1', 'g1', 'a1', now()), ('p1', 'g1', 'a2', now()),
                 ('p2', 'g1', 'a2', now()), ('p2', 'g1', 'a1', now());",
        )
        .await
        .unwrap();
    // What a step that must fail fails with. Whatever it began is undone
    // before the next.
    let refused = |statements: &'static str| {
        let tables = &tables;
        async move {
            let outcome = tables.batch_execute(statements).await;
            tables.batch_execute("ROLLBACK").await.unwrap();
            let error = outcome.expect_err(statements);
            error.code().map(|code| code.code().to_owned())
        }
    };
    let violation = Some("23503".to_owned());

    // A group has a keeper who is one of its members. Not an account that
    // is not in it, not a member who has gone, and not from its first
    // moment on either.
    assert_eq!(
        refused("BEGIN; UPDATE groups SET keeper_id = 'a3' WHERE id = 'g1'; COMMIT").await,
        violation
    );
    assert_eq!(
        refused(
            "BEGIN; DELETE FROM group_members WHERE group_id = 'g1' AND account_id = 'a1'; COMMIT"
        )
        .await,
        violation
    );
    assert_eq!(
        refused("BEGIN; DELETE FROM accounts WHERE id = 'a1'; COMMIT").await,
        violation
    );
    assert_eq!(
        refused(
            "BEGIN; INSERT INTO groups (id, name, created_at, keeper_id)
                 VALUES ('g2', 'Nobody in it', now(), 'a1'); COMMIT"
        )
        .await,
        violation
    );
    // A plan is a member's, and so is a word on a plan.
    assert_eq!(
        refused(
            "BEGIN; INSERT INTO group_plans
                 (id, group_id, author_id, day, place, created_at, changed_at)
             VALUES ('p3', 'g1', 'a3', '2026-11-19', 'x', now(), now()); COMMIT"
        )
        .await,
        violation
    );
    assert_eq!(
        refused(
            "BEGIN; INSERT INTO group_plan_pilots (plan_id, group_id, account_id, since)
             VALUES ('p1', 'g1', 'a3', now()); COMMIT"
        )
        .await,
        violation
    );
    let counts = || {
        let tables = &tables;
        async move {
            let row = tables
                .query_one(
                    "SELECT (SELECT count(*) FROM groups), (SELECT count(*) FROM group_members),
                            (SELECT count(*) FROM group_plans),
                            (SELECT count(*) FROM group_plan_pilots)",
                    &[],
                )
                .await
                .unwrap();
            let count = |index: usize| row.get::<_, i64>(index);
            (count(0), count(1), count(2), count(3))
        }
    };
    assert_eq!(counts().await, (1, 2, 2, 4), "nothing of that was stored");

    // A member who goes takes their plan and their word on the other's
    // along, whoever deletes the row and however: here the account goes.
    tables
        .batch_execute("DELETE FROM accounts WHERE id = 'a2'")
        .await
        .unwrap();
    assert_eq!(counts().await, (1, 1, 1, 1));
    // And a group that goes takes everything along.
    tables
        .batch_execute("DELETE FROM groups WHERE id = 'g1'")
        .await
        .unwrap();
    assert_eq!(counts().await, (0, 0, 0, 0));
}

#[tokio::test]
async fn the_questions_about_groups_and_plans_use_their_indexes() {
    let Some(url) = fresh_schema("group_indexes").await else {
        return;
    };
    PgStore::connect(&url).await.unwrap();
    let tables = look(&url).await;
    // As for airspace: with empty tables the planner would read them from
    // end to end; forbidding that shows whether an index can answer.
    tables
        .batch_execute("SET enable_seqscan = off")
        .await
        .unwrap();
    let plan_of = |query: &'static str| {
        let tables = &tables;
        async move {
            let plan = tables.query(query, &[]).await.unwrap();
            let plan: Vec<String> = plan.iter().map(|row| row.get(0)).collect();
            plan.join("\n")
        }
    };

    // The store's own questions, with their parameters written in. The
    // group an invite code lets a pilot into:
    let plan =
        plan_of("EXPLAIN SELECT id FROM groups WHERE invite_hash = '\\x07'::bytea FOR UPDATE")
            .await;
    assert!(plan.contains("groups_invite_hash_key"), "{plan}");
    // The groups an account is in:
    let plan = plan_of(
        "EXPLAIN SELECT g.id FROM groups g JOIN accounts k ON k.id = g.keeper_id
         JOIN group_members m ON m.group_id = g.id
         WHERE m.account_id = '000000000000000000000dario' ORDER BY m.joined_at, g.id",
    )
    .await;
    assert!(plan.contains("group_members_account_idx"), "{plan}");
    // The groups an account keeps, for when it goes:
    let plan = plan_of(
        "EXPLAIN SELECT id FROM groups WHERE keeper_id = '000000000000000000000dario'
         ORDER BY id FOR UPDATE",
    )
    .await;
    assert!(plan.contains("groups_keeper_idx"), "{plan}");
    // A group's plans from a day on:
    let plan = plan_of(
        "EXPLAIN SELECT p.id FROM group_plans p JOIN accounts a ON a.id = p.author_id
         WHERE p.group_id = '0000000000000000000000000g' AND p.day >= '2026-11-18'
           AND (NULL::date IS NULL OR p.day <= NULL)
         ORDER BY p.day, p.meet_at NULLS LAST, p.created_at, p.id",
    )
    .await;
    assert!(plan.contains("group_plans_group_day_idx"), "{plan}");
    // Who is in on those plans:
    let plan = plan_of(
        "EXPLAIN SELECT i.plan_id FROM group_plan_pilots i JOIN accounts a ON a.id = i.account_id
         WHERE i.plan_id = ANY(ARRAY['0000000000000000000000000p'])
         ORDER BY i.plan_id, i.since, i.account_id",
    )
    .await;
    assert!(plan.contains("group_plan_pilots_pkey"), "{plan}");
    // The plans of days gone by:
    let plan = plan_of("EXPLAIN DELETE FROM group_plans WHERE day < '2026-11-16'").await;
    assert!(plan.contains("group_plans_day_idx"), "{plan}");
}
