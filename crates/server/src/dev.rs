//! The `dev` subcommand: the same API over recorded data.
//!
//! No network and no database. The real adapters run once over the payloads
//! recorded in their crates, the result goes into an in-memory store, and
//! the real router serves it. Client work and UI tests run against this.
//!
//! The clock stands still at the moment of the recording, so the data never
//! ages: `/v1/health` reports fresh sources, and "the last 24 hours" is
//! always the recorded day. Every start serves exactly the same answers.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use venti_regions::RegionPack;
use venti_store::MemoryStore;

use crate::api::{Api, Cors};
use crate::clock::{Clock, FixedClock};
use crate::ingest::{Collector, DirArchive};
use crate::{adapters, plan, serve};

/// The region the payloads were recorded for, compiled in so that the
/// development mode needs no files.
const FIRST_REGION: &str = include_str!("../../../regions/alps-de-at/region.toml");

/// An API over the recorded data.
///
/// The API tests use this too, so what they check is what the development
/// mode serves.
pub async fn recorded_api() -> Result<Api<MemoryStore>, String> {
    let registry = adapters::registry();
    let pack = RegionPack::from_toml(FIRST_REGION).map_err(|e| e.to_string())?;
    pack.check_against(&registry).map_err(|e| e.to_string())?;
    let jobs = plan::plan(&[pack], &registry).map_err(|e| e.to_string())?;

    let store = Arc::new(MemoryStore::new());
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::recorded_at()));
    let manifests = jobs
        .iter()
        .filter_map(|job| registry.observations(&job.source))
        .map(|adapter| adapter.manifest());
    let api = Api::new(store.clone(), clock.clone(), manifests);

    // Everything in the payloads is taken: a day of history per station.
    let collector = Collector::new(
        registry,
        store,
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock,
        None,
    );
    for job in &jobs {
        if !collector.collect(job).await {
            return Err(format!(
                "the recorded payloads of {} no longer give a healthy run",
                job.source
            ));
        }
    }
    Ok(api)
}

/// Serves the recorded data on `address` until the process is told to stop.
///
/// Any web origin may call it: it holds nothing but public test data, and a
/// web app under development runs on whatever port its tooling picks.
pub async fn dev(address: SocketAddr) -> Result<(), String> {
    let router = recorded_api().await?.router(&Cors::Any);
    let shutdown = CancellationToken::new();
    tokio::spawn(serve::cancel_on_signal(shutdown.clone()));
    tracing::info!(
        "development mode: recorded data of {}, no network, no database",
        adapters::recorded_at().format("%Y-%m-%d %H:%M UTC")
    );
    serve::listen(address, router, shutdown).await
}
