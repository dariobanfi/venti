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
    assert_eq!(versions, [1, 2, 3, 4]);
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
