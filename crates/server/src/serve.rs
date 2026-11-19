//! The `serve` subcommand: the server as it runs in production.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use chrono::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use venti_store::PgStore;

use crate::api::{Api, Cors};
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::ingest::{Collector, DirArchive, HttpFetch, scheduler};
use crate::{adapters, maintenance, plan};

/// How far back from the time of a run observations are taken. See
/// [`Collector::new`].
const LOOKBACK: Duration = Duration::hours(3);

/// Runs the server until it is told to stop: collects every configured
/// source at its cadence, cleans up old data, and answers the API.
pub async fn serve(config: Config) -> Result<(), String> {
    let registry = adapters::registry();
    let jobs = plan::load(&config.regions, &registry).map_err(|e| e.to_string())?;
    let store = PgStore::connect(config.database_url.expose())
        .await
        .map_err(|e| format!("cannot open the database: {e}"))?;
    let store = Arc::new(store);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let archive = config.archive_dir.clone().map(DirArchive::new);
    let fetch = HttpFetch::new(&config.user_agent)?;

    let manifests = jobs
        .iter()
        .filter_map(|job| registry.observations(&job.source))
        .map(|adapter| adapter.manifest());
    let api = Api::new(store.clone(), clock.clone(), manifests);
    let router = api.router(&Cors::Origins(config.cors_origins.clone()));

    let shutdown = CancellationToken::new();
    tokio::spawn(cancel_on_signal(shutdown.clone()));

    let sources: Vec<&str> = jobs.iter().map(|job| job.source.as_str()).collect();
    tracing::info!("collecting {}", sources.join(", "));
    let collector = Arc::new(Collector::new(
        registry,
        store.clone(),
        fetch,
        archive.clone(),
        clock.clone(),
        Some(LOOKBACK),
    ));
    let schedule = tokio::spawn(scheduler::run(
        collector,
        jobs,
        fastrand::f64,
        shutdown.clone(),
    ));
    let housekeeping = tokio::spawn(maintenance::run(
        store,
        archive,
        config.retention,
        clock,
        shutdown.clone(),
    ));

    let served = listen(config.listen, router, shutdown.clone()).await;

    // Whatever ended the HTTP server ends the background work too.
    shutdown.cancel();
    let _ = schedule.await;
    let _ = housekeeping.await;
    served
}

/// Answers HTTP requests on `address` until `shutdown` is cancelled, then
/// lets requests in flight finish.
pub(crate) async fn listen(
    address: SocketAddr,
    router: Router,
    shutdown: CancellationToken,
) -> Result<(), String> {
    let listener = TcpListener::bind(address)
        .await
        .map_err(|e| format!("cannot listen on {address}: {e}"))?;
    let address = listener.local_addr().unwrap_or(address);
    tracing::info!("listening on http://{address}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .map_err(|e| format!("the HTTP server failed: {e}"))
}

/// Cancels `shutdown` when the process is asked to stop: Ctrl-C in a
/// terminal, or the termination signal a container runtime sends.
pub(crate) async fn cancel_on_signal(shutdown: CancellationToken) {
    let interrupt = async {
        if tokio::signal::ctrl_c().await.is_err() {
            // Without a signal handler only the other signal can stop us.
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
    tracing::info!("stopping");
    shutdown.cancel();
}
