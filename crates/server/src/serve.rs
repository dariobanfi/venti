//! The `serve` subcommand: the server as it runs in production.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use chrono::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use venti_store::PgStore;

use crate::accounts::Limits;
use crate::accounts::password::Cost;
use crate::api::{Api, Cors, Settings};
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::flights::analyser::Analyser;
use crate::flights::refresh;
use crate::flights::share::PublicUrl;
use crate::ingest::{Collector, DirArchive, HttpFetch, scheduler};
use crate::{adapters, maintenance, plan};

/// How far back from the time of a run observations are taken. See
/// [`Collector::new`].
const LOOKBACK: Duration = Duration::hours(3);

/// Runs the server until it is told to stop: collects every configured
/// source at its cadence, cleans up old data, and answers the API.
pub async fn serve(config: Config) -> Result<(), String> {
    let registry = adapters::registry_for(&config.providers)?;
    let jobs = plan::load(&config.regions, &registry).map_err(|e| e.to_string())?;
    let store = PgStore::connect(config.database_url.expose())
        .await
        .map_err(|e| format!("cannot open the database: {e}"))?;
    let store = Arc::new(store);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let archive = config.archive_dir.clone().map(DirArchive::new);
    let fetch = HttpFetch::new(&config.user_agent)?;

    let public_url = match config.public_url.clone() {
        Some(url) => url,
        None => {
            let url = local_url(config.listen)?;
            tracing::warn!(
                "public_url is not set: links in share pages will point at {}, \
                 which previews in chat apps cannot follow",
                url.as_str()
            );
            url
        }
    };
    let limits = Limits::default();
    let analyser = Analyser::child_of_this_program()?;
    let settings = Settings {
        registration: config.registration,
        public_url,
        trusted_proxies: config.trusted_proxies.clone(),
        limits,
        room: config.room,
        password_cost: Cost::OWASP,
        analyser: analyser.clone(),
    };
    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let api = Api::new(store.clone(), clock.clone(), manifests, settings);
    let router = api.router(&Cors::Origins(config.cors_origins.clone()));
    tracing::info!("accounts: registration is {:?}", config.registration);
    tracing::info!("client addresses: {}", config.trusted_proxies.in_words());

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
        store.clone(),
        archive,
        config.retention,
        limits.session_idle,
        clock.clone(),
        shutdown.clone(),
    ));
    // Flights analysed by an older version of the server are worked out
    // again, once, beside everything else.
    let refreshing = tokio::spawn(refresh::run(store, analyser, clock, shutdown.clone()));

    let served = listen(config.listen, router, shutdown.clone()).await;

    // Whatever ended the HTTP server ends the background work too.
    shutdown.cancel();
    let _ = schedule.await;
    let _ = housekeeping.await;
    let _ = refreshing.await;
    served
}

/// The address a server that listens on `address` is reached at from the
/// same machine.
pub(crate) fn local_url(address: SocketAddr) -> Result<PublicUrl, String> {
    PublicUrl::new(&format!("http://{address}"))
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
    // The address each connection comes from is handed to the handlers:
    // attempts to sign in are counted by it.
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, service)
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
