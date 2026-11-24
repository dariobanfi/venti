//! Everyone's flights at the size of a year of a busy Venti: some 3.6
//! million public flights, 10,000 a day (docs/plan.md, the owner's queue,
//! "Flight search that holds at scale").
//!
//! Runs only with `VENTI_SCALE_DATABASE_URL`, a database of its own that
//! it fills once and keeps: filling takes minutes. With it, every question
//! of `scale/queries.rs` is asked with `EXPLAIN (ANALYZE, BUFFERS)` and its
//! time printed, and the test fails if one reads the whole table of
//! flights or takes longer than `VENTI_SCALE_MAX_MS` (50 by default; the
//! few that sum many rows, `queries::SUMMED`, `VENTI_SCALE_MAX_SUM_MS`,
//! 500):
//!
//! ```text
//! docker run -d --name venti-scale -p 127.0.0.1:55434:5432 \
//!     -e POSTGRES_USER=venti -e POSTGRES_PASSWORD=venti -e POSTGRES_DB=venti \
//!     postgis/postgis:16-3.4 -c shared_buffers=1GB -c max_locks_per_transaction=256
//! VENTI_SCALE_DATABASE_URL=postgres://venti:venti@127.0.0.1:55434/venti \
//!     cargo test -p venti-store --test scale -- --nocapture
//! docker rm -f venti-scale
//! ```
//!
//! `VENTI_SCALE_FLIGHTS` sets how many public flights it makes
//! (3,600,000 by default), beside 200,000 private ones and 20,000 pilots.

#[path = "scale/queries.rs"]
mod queries;

use queries::{SUMMED, centre, execution_ms, explain, questions, reads_all_flights};
use tokio_postgres::NoTls;
use venti_store::PgStore;

const SEED: &str = include_str!("scale/seed.sql");

fn number(variable: &str, default: f64) -> f64 {
    std::env::var(variable)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(default)
}

#[tokio::test]
async fn everyone_s_flights_are_found_quickly_among_millions() {
    let Ok(url) = std::env::var("VENTI_SCALE_DATABASE_URL") else {
        eprintln!("skipped: VENTI_SCALE_DATABASE_URL is not set");
        return;
    };
    let store = PgStore::connect(&url).await.unwrap();
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(connection);
    let held: i64 = client
        .query_one("SELECT count(*) FROM flights", &[])
        .await
        .unwrap()
        .get(0);
    if held == 0 {
        let flights = number("VENTI_SCALE_FLIGHTS", 3_600_000.0) as i32;
        let started = std::time::Instant::now();
        client.batch_execute(SEED).await.unwrap();
        client
            .execute("SELECT seed_flights(20000, $1, 200000)", &[&flights])
            .await
            .unwrap();
        client
            .batch_execute("VACUUM ANALYZE flights; VACUUM ANALYZE accounts")
            .await
            .unwrap();
        eprintln!("made {flights} public flights in {:?}", started.elapsed());
    }

    let most = number("VENTI_SCALE_MAX_MS", 50.0);
    let most_summed = number("VENTI_SCALE_MAX_SUM_MS", 500.0);
    let centre = centre(&url).await;
    let mut slow = Vec::new();
    println!("| Question | ms | Whole table |");
    println!("|---|---:|---|");
    for (name, asked) in questions(centre, "seedacc00000001") {
        // Asked twice: the second answer is the one from memory, as a
        // server that has been running answers.
        let _ = explain(&store, &asked).await;
        let plan = match explain(&store, &asked).await {
            Ok(plan) => plan,
            // Given up after the read timeout of the store.
            Err(error) => {
                println!("| {name} | cancelled | |");
                slow.push(format!("{name}: {error}"));
                continue;
            }
        };
        let ms = execution_ms(&plan);
        let whole = reads_all_flights(&plan);
        println!("| {name} | {ms:.1} | {} |", if whole { "yes" } else { "" });
        if std::env::var_os("VENTI_SCALE_PLANS").is_some() {
            println!("{plan}\n");
        }
        let bound = if SUMMED.contains(&name) {
            most_summed
        } else {
            most
        };
        if whole || ms > bound {
            slow.push(format!(
                "{name}: {ms:.1} ms{}",
                if whole { ", whole table" } else { "" }
            ));
        }
    }
    assert!(slow.is_empty(), "too slow: {slow:#?}");
}
