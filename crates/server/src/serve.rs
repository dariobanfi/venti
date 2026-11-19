//! The `serve` subcommand: the server as it runs in production.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use chrono::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use venti_store::PgStore;

use crate::accounts::password::Cost;
use crate::accounts::{Limits, Registration};
use crate::api::{Api, Cors, Settings};
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::flights::analyser::Analyser;
use crate::flights::refresh;
use crate::flights::share::PublicUrl;
use crate::forecast::{self, Asking};
use crate::ingest::{Collector, DirArchive, HttpFetch, scheduler};
use crate::{adapters, live, maintenance, plan};

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
    // The fetcher asks HTTPS addresses, and without encryption only those
    // the configuration names with http://, such as a forecast service on
    // the server's own network.
    let fetcher =
        || HttpFetch::new(&config.user_agent)?.plain_http_to(config.providers.addresses());
    let fetch = fetcher()?;

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
        live: live::Limits::keeping(Duration::hours(i64::from(config.retention.live_hours))),
    };
    // Forecasts are fetched when they are asked for, by a service of their
    // own with a fetcher like the collector's. It exists only if the server
    // was told where its Open-Meteo instance is.
    let forecasts = forecast::service_for(
        &jobs,
        adapters::forecast_sources(&config.providers)?,
        fetcher()?,
        archive.clone(),
        clock.clone(),
        Asking::Live,
    );
    if forecasts.is_none() {
        tracing::info!(
            "no forecasts: the server was not told where its forecast service is (open_meteo_url)"
        );
    }

    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let mut api = Api::new(store.clone(), clock.clone(), manifests, settings);
    if let Some(forecasts) = &forecasts {
        api = api.with_forecasts(forecasts.clone());
    }
    let router = api.router(&Cors::Origins(config.cors_origins.clone()));
    tracing::info!("accounts: registration is {:?}", config.registration);
    if let Some(warning) = plain_http_warning(config.public_url.as_ref(), config.registration) {
        tracing::warn!("{warning}");
    }
    tracing::info!(
        "live tracks: kept for {} hours after their newest position",
        config.retention.live_hours
    );
    tracing::info!("client addresses: {}", config.trusted_proxies.in_words());

    let shutdown = CancellationToken::new();
    tokio::spawn(cancel_on_signal(shutdown.clone()));

    let sources: Vec<&str> = jobs.iter().map(|job| job.source.as_str()).collect();
    tracing::info!("collecting {}", sources.join(", "));
    let mut collector = Collector::new(
        registry,
        store.clone(),
        fetch,
        archive.clone(),
        clock.clone(),
        Some(LOOKBACK),
    );
    if let Some(forecasts) = forecasts {
        collector = collector.with_forecasts(forecasts);
    }
    let collector = Arc::new(collector);
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

/// What the log says at start about a server that takes passwords without
/// encryption, if this is one: its public address is plain HTTP and not
/// this machine's own, and it lets people sign in.
///
/// A server always lets people sign in. So the one way to have nothing to
/// say is HTTPS, or an address only this machine reaches. With
/// registration closed there are no accounts to protect unless some exist
/// already, which this does not know: it says so in fewer words.
pub(crate) fn plain_http_warning(
    public_url: Option<&PublicUrl>,
    registration: Registration,
) -> Option<String> {
    let url = public_url.filter(|url| url.is_plain_http_to_elsewhere())?;
    Some(match registration {
        Registration::Closed => format!(
            "{} is plain HTTP: if this server has accounts, their passwords and session \
             tokens travel unencrypted. Give it a domain name and HTTPS \
             (VENTI_SITE_ADDRESS, VENTI_PUBLIC_URL)",
            url.as_str()
        ),
        Registration::Open | Registration::Invite => format!(
            "ACCOUNTS OVER PLAIN HTTP: {} is not HTTPS, and this server takes accounts. \
             Every password and session token travels unencrypted, readable to anyone \
             between a pilot and this machine. Do not hand out invite codes or let anyone \
             sign in before the site has a domain name and HTTPS (VENTI_SITE_ADDRESS, \
             VENTI_PUBLIC_URL); until then set VENTI_REGISTRATION=closed",
            url.as_str()
        ),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> PublicUrl {
        PublicUrl::new(text).unwrap()
    }

    #[test]
    fn a_server_that_takes_accounts_over_plain_http_says_so_loudly_at_start() {
        let warning =
            |address: &str, registration| plain_http_warning(Some(&url(address)), registration);

        // The first try without a domain, as env.example starts out.
        for registration in [Registration::Invite, Registration::Open] {
            let said = warning("http://203.0.113.9", registration).unwrap();
            assert!(said.starts_with("ACCOUNTS OVER PLAIN HTTP"), "{said}");
            assert!(said.contains("VENTI_REGISTRATION=closed"), "{said}");
            assert!(said.contains("http://203.0.113.9"), "{said}");
        }
        // With registration closed there may still be accounts.
        let said = warning("http://venti.example:8080", Registration::Closed).unwrap();
        assert!(said.contains("if this server has accounts"), "{said}");

        // Nothing to say with HTTPS, on this machine's own address, or
        // where no public address was given: that has a warning of its own.
        for address in [
            "https://venti.example",
            "http://127.0.0.1:8080",
            "http://localhost:8080",
            "http://[::1]:8080",
        ] {
            assert_eq!(warning(address, Registration::Open), None, "{address}");
        }
        assert_eq!(plain_http_warning(None, Registration::Invite), None);
    }
}
