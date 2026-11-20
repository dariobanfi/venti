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
    assert_eq!(versions, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
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
                  score_closed, score_distance_m, score_points, outline, profile)
             SELECT lpad(n::text, 26, '0'), '000000000000000000000pilot', lpad(n::text, 32, '0'),
                    CASE WHEN n % 4 = 0 THEN 'private' ELSE 'public' END,
                    '2026-11-18 19:05+00', n, ''::bytea, 60000, 1,
                    timestamptz '2025-01-01 09:00+00' + n * interval '53 minutes', 3600,
                    timestamptz '2025-01-01 09:00+00' + n * interval '53 minutes',
                    46.5 + (n % 200) * 0.01, 9.5 + (n % 173) * 0.04, 1500,
                    20000, 2400, 2.5, 'free_flight', false,
                    (n % 997) * 100.0, (n % 997) / 10.0, '{}', '{}'
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
