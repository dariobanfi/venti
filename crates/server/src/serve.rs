//! The `serve` subcommand: the server as it runs in production.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use chrono::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use venti_domain::ModelId;
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
use crate::imagery::{self, Imagery, TileCache};
use crate::ingest::{Collector, DirArchive, HttpFetch, scheduler};
use crate::maps::Maps;
use crate::thermals::{InStore, ThermalMap};
use crate::{adapters, groups, live, maintenance, pilot_sites, plan, radar};

/// How far back from the time of a run observations are taken. See
/// [`Collector::new`].
const LOOKBACK: Duration = Duration::hours(3);

/// Runs the server until it is told to stop: collects every configured
/// source at its cadence, cleans up old data, and answers the API.
pub async fn serve(config: Config) -> Result<(), String> {
    let registry = adapters::registry_for(&config.providers)?;
    let packs = plan::read(&config.regions, &registry).map_err(|e| e.to_string())?;
    let jobs = plan::plan(&packs, &registry).map_err(|e| e.to_string())?;
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
        groups: groups::Limits::keeping(config.retention.plan_days),
        sites: pilot_sites::Settings {
            accept: config.sites_from_pilots,
            regions: packs.iter().map(|pack| pack.bounds).collect(),
            limits: pilot_sites::Limits::default(),
        },
    };
    if config.sites_from_pilots {
        tracing::info!("sites: this server takes sites from pilots (sites_from_pilots)");
    }
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

    // The server's key pair for Web Push, made and kept in the database
    // when it first starts with it; and the sender, which tells pilots'
    // browsers what their alarms fired. The push services may write to
    // the address of the site about it, if it has HTTPS.
    let push_key = crate::push::key_of(store.as_ref(), clock.as_ref()).await?;
    let contact = config
        .public_url
        .as_ref()
        .map(|url| url.as_str().to_owned())
        .filter(|url| url.starts_with("https://"));
    let pusher = crate::push::Pusher::start(
        store.clone(),
        push_key.clone(),
        clock.clone(),
        crate::push::PushSettings::new(&config.user_agent, contact),
    )?;

    // The alarms pilots set, checked where the data arrives: after each
    // run of stations, and after each map made with a new run.
    let air_checks =
        Arc::new(crate::alerts::Checks::new(store.clone(), clock.clone()).with_push(pusher));
    let checks: Arc<dyn crate::alerts::Watch> = air_checks.clone();

    // The thermal map of the forecast source, made with each run of its
    // model that the collector's look sees, and kept in memory.
    let thermal_map = forecasts.as_ref().and_then(|forecasts| {
        let source = jobs
            .iter()
            .find(|job| job.kind == venti_sources::Kind::Forecasts)?
            .source
            .clone();
        let map = ThermalMap::for_source(forecasts.clone(), source, clock.clone())?
            .with_keeper(Arc::new(InStore(store.clone())))
            .with_alerts(checks.clone());
        tracing::info!(
            "thermal map: {} cells of {}, {} requests a run of each",
            map.cells(),
            map.models()
                .iter()
                .map(ModelId::as_str)
                .collect::<Vec<_>>()
                .join(" and "),
            map.requests_per_map()
        );
        Some(Arc::new(map))
    });
    // What the map judged before the server stopped, so that today keeps
    // the hours its newest runs no longer hold.
    if let Some(map) = &thermal_map {
        map.restore().await;
    }

    // The rain radar of the regions that name a radar source: its last
    // two hours of frames, kept in memory by a service of its own.
    let radar = radar::service_for(
        &jobs,
        adapters::radar_sources(),
        fetcher()?,
        clock.clone(),
        |_| radar::KEPT,
    );
    match &radar {
        Some(radar) => {
            for layer in radar.layers() {
                tracing::info!(
                    "radar: {} on {} by {} pixels, the last {} hours kept in memory",
                    layer.source,
                    layer.grid.columns,
                    layer.grid.rows,
                    radar::KEPT.num_hours()
                );
            }
        }
        None => tracing::info!("no radar: none of the regions names a radar source"),
    }

    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let mut api = Api::new(store.clone(), clock.clone(), manifests, settings)
        .with_station_priority(plan::station_priority(&packs))
        .with_push_key(&push_key);
    if let Some(radar) = &radar {
        api = api.with_radar(radar.clone());
    }
    if config.google_client_ids.is_empty() {
        tracing::info!("no sign-in with Google: no Google client ID is configured");
    } else {
        let google = crate::accounts::google::GoogleSignIn::google(
            config.google_client_ids.clone(),
            &config.user_agent,
        )?;
        tracing::info!(
            "sign-in with Google for {} client IDs",
            config.google_client_ids.len()
        );
        api = api.with_google(Arc::new(google));
    }
    if let Some(forecasts) = &forecasts {
        api = api.with_forecasts(forecasts.clone());
    }
    if let Some(map) = &thermal_map {
        api = api.with_thermal_map(map.clone());
    }
    if let Some(imagery) = imagery(&config)? {
        api = api.with_imagery(imagery);
    }
    let shutdown = CancellationToken::new();
    // The networks of live positions the regions name: a connection each,
    // held by a task of its own, and what it heard kept in memory.
    let mut listening = Vec::new();
    let mut networks = Vec::new();
    for (id, area) in plan::live(&packs) {
        let Some(source) = adapters::live_sources()
            .into_iter()
            .find(|source| source.manifest().id == id)
        else {
            return Err(format!("no adapter is registered for the live source {id}"));
        };
        let network = Arc::new(live::network::Network::new(
            source.manifest(),
            clock.clone(),
        ));
        tracing::info!(
            "live network {id}: listening for the regions' box, positions kept at most {} hours",
            source.manifest().pass_on_hours.unwrap_or(24).min(24)
        );
        listening.push(tokio::spawn(live::network::run(
            source,
            network.clone(),
            area,
            fetcher()?,
            shutdown.clone(),
        )));
        networks.push(network);
    }
    // The alarms about pilots in the air are checked once a minute
    // against what the networks hear and Venti's own live pilots send.
    let watching_the_air = tokio::spawn(crate::alerts::watch_the_air(
        air_checks,
        networks.clone(),
        api.live_air(),
        shutdown.clone(),
    ));
    api = api.with_networks(networks);
    // Where thermals have been is read from the public flights, a few
    // at a time, beside everything else.
    let flown = Arc::new(crate::flights::flown::FlownThermals::default());
    api = api.with_flown_thermals(flown.clone());
    let reading_flown = tokio::spawn(crate::flights::flown::run(
        flown,
        store.clone(),
        shutdown.clone(),
    ));
    // The map regions are listed as their files are read, which the
    // server begins further down, beside everything else. Reading them
    // and handing them out both end when the server is told to stop.
    let maps = Maps::new(shutdown.clone());
    api = api.with_maps(maps.clone());
    let router = api.router(&Cors::Origins(config.cors_origins.clone()));
    tracing::info!("accounts: registration is {:?}", config.registration);
    if let Some(warning) = plain_http_warning(config.public_url.as_ref(), config.registration) {
        tracing::warn!("{warning}");
    }
    tracing::info!(
        "live tracks: kept for {} hours after their newest position",
        config.retention.live_hours
    );
    tracing::info!(
        "plans of groups: kept for {} days after their day",
        config.retention.plan_days
    );
    tracing::info!("client addresses: {}", config.trusted_proxies.in_words());

    tokio::spawn(cancel_on_signal(shutdown.clone()));

    // Every file of every region is read once, to check its hash, before
    // the region is listed. For gigabytes that takes a while, so it does
    // not hold up the start of the API.
    let reading_maps = match config.maps_dir.clone() {
        Some(folder) => {
            tracing::info!(
                "maps: reading the regions in {}; each is handed out once its files are checked",
                folder.display()
            );
            Some(maps.read_in_background(folder))
        }
        None => {
            tracing::info!("no maps: the server was not told where its map regions are (maps_dir)");
            None
        }
    };

    let sources: Vec<&str> = jobs.iter().map(|job| job.source.as_str()).collect();
    tracing::info!("collecting {}", sources.join(", "));
    let mut collector = Collector::new(
        registry,
        store.clone(),
        fetch,
        archive.clone(),
        clock.clone(),
        Some(LOOKBACK),
    )
    .with_alerts(checks);
    if let Some(forecasts) = forecasts {
        collector = collector.with_forecasts(forecasts);
    }
    if let Some(map) = thermal_map {
        collector = collector.with_thermal_map(map);
    }
    if let Some(radar) = radar {
        collector = collector.with_radar(radar);
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
    let _ = watching_the_air.await;
    let _ = reading_flown.await;
    for network in listening {
        let _ = network.await;
    }
    if let Some(reading) = reading_maps {
        let _ = reading.await;
    }
    served
}

/// The map imagery the server passes on, if the configuration names a
/// folder to keep its tiles in; see [`crate::imagery`]. The folder is
/// swept once beside everything else, which also learns what it holds.
fn imagery(config: &Config) -> Result<Option<Arc<Imagery>>, String> {
    let Some(folder) = config.imagery_dir.clone() else {
        tracing::info!("no imagery: the server was not told where to keep its tiles (imagery_dir)");
        return Ok(None);
    };
    let cache = Arc::new(TileCache::new(
        folder,
        u64::from(config.imagery_cache_mb) * 1024 * 1024,
    )?);
    cache.sweep_in_background();
    let sources = adapters::imagery_sources();
    let imagery = Imagery::new(
        sources,
        Arc::new(HttpFetch::new(&config.user_agent)?),
        Some(cache.clone()),
        imagery::Limits::default(),
    );
    tracing::info!(
        "imagery: {} at /v1/imagery, tiles kept in {} up to {} MB",
        imagery.sources().collect::<Vec<_>>().join(", "),
        cache.root().display(),
        config.imagery_cache_mb
    );
    Ok(Some(Arc::new(imagery)))
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
