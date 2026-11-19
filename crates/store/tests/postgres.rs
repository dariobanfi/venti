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
