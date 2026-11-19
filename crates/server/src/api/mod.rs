//! The HTTP API, version 1, with the share pages and the connector beside
//! it.
//!
//! The wire format lives in `venti-api-types`, shared with the clients;
//! `docs/api.md` describes the endpoints for people. This module turns
//! requests into store queries and store results into those types.
//!
//! Two kinds of data go out, and they are kept apart.
//!
//! **What the data sources deliver.** Every response that carries it names
//! the sources it came from and carries their credit lines. A station or an
//! airspace whose source this server is not configured to serve is never
//! returned, even when the store still holds it: without a manifest there
//! is no licence to serve it under. Airspace, flying sites and forecasts go
//! out in layers, one per source, and are never merged across sources.
//!
//! Everything but a forecast is read from the store. A forecast is asked of
//! the forecast service ([`crate::forecast`]), which asks the server's own
//! Open-Meteo instance when it has no answer at hand: the one way a request
//! can make the server ask a provider, at an address that comes from the
//! server's configuration and never from the request.
//!
//! **What pilots keep here.** An account is acted for with a session token
//! in the `Authorization` header ([`auth`]). A flight is shown to its pilot
//! and, unless it is private, to everyone else; a private flight and a
//! flight that does not exist get the same answer. What may keep an answer
//! is said in [`cache`].
//!
//! The connector ([`mcp`]) offers what anyone may read to a pilot's own
//! assistant, over the Model Context Protocol. It is not part of `/v1`.

mod account;
mod airspace;
mod auth;
mod body;
mod cache;
mod clients;
mod error;
mod flights;
mod forecast;
mod health;
mod log;
mod mcp;
mod observations;
mod pilots;
mod shape;
mod share;
mod sites;
mod stations;
mod wire;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{any, delete, get, post, put};
use tokio::sync::Semaphore;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use venti_api_types::v1::{
    ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNTS_PATH, AIRSPACE_PATH, Attribution,
    FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FORECAST_PATH, HEALTH_PATH, SESSIONS_PATH, SITES_PATH,
    STATIONS_PATH,
};
use venti_domain::SourceId;
use venti_sources::{Kind, Manifest};
use venti_store::{Room, Store};

use crate::accounts::client::TrustedProxies;
use crate::accounts::password::{Cost, Hasher};
use crate::accounts::throttle::Throttle;
use crate::accounts::{Limits, Registration};
use crate::clock::Clock;
use crate::flights::analyser::Analyser;
use crate::flights::share::PublicUrl;
use crate::forecast::Forecasts;

pub use mcp::MCP_PATH;

/// The largest request body accepted where a request carries JSON or
/// nothing: far more than a sign-in needs, and too little to be a burden.
const MAX_BODY_BYTES: usize = 16 * 1024;

/// How long a request may take before the server gives up on it.
const DEADLINE: Duration = Duration::from_secs(10);

/// How long an upload may take, from its first byte to the answer. A flight
/// file sent over a weak mobile connection needs time; analysing it has a
/// limit of its own.
const UPLOAD_DEADLINE: Duration = Duration::from_secs(120);

/// How long a browser may reuse the answer to a CORS preflight request.
const PREFLIGHT_MAX_AGE: Duration = Duration::from_secs(3600);

/// How many preview pictures are drawn at the same time.
const PICTURES_AT_ONCE: usize = 2;

/// A source whose data the server serves, with what the API says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedSource {
    /// Who publishes the data.
    pub provider: String,
    /// The kinds of data the source offers.
    pub kinds: Vec<Kind>,
    /// How often the source is collected, in seconds.
    pub cadence_seconds: u32,
    /// The credit that goes out with its data.
    pub attribution: Attribution,
}

impl ServedSource {
    /// What the API says about the source a manifest describes.
    pub fn from_manifest(manifest: &Manifest) -> Self {
        Self {
            provider: manifest.provider.clone(),
            kinds: manifest.kinds.clone(),
            cadence_seconds: manifest.cadence_seconds,
            attribution: Attribution {
                source: manifest.id.clone(),
                provider: manifest.provider.clone(),
                licence: manifest.licence.name.clone(),
                text: manifest.licence.attribution.clone(),
                url: manifest.licence.link.clone(),
                notice: manifest.licence.notice.clone(),
            },
        }
    }
}

/// Which web origins may call the API from a browser.
///
/// Either way the answer never tells a browser to send credentials along:
/// the API knows no cookies, and a session token gets into a request only
/// when the page's own code puts it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cors {
    /// Only these origins, for example `https://venti.example`.
    Origins(Vec<String>),
    /// Any origin. The development mode uses this.
    Any,
}

/// How the API treats accounts and flights.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Who may create an account.
    pub registration: Registration,
    /// The address the server is reached at from outside. Links in share
    /// pages start with it.
    pub public_url: PublicUrl,
    /// The proxies whose word on a client's address is believed.
    pub trusted_proxies: TrustedProxies,
    /// The limits on attempts.
    pub limits: Limits,
    /// The most one account may keep.
    pub room: Room,
    /// What hashing a password costs.
    pub password_cost: Cost,
    /// Where uploaded flight files are analysed.
    pub analyser: Analyser,
}

/// What the request handlers work with.
pub struct Api<S> {
    store: Arc<S>,
    clock: Arc<dyn Clock>,
    sources: BTreeMap<SourceId, ServedSource>,
    registration: Registration,
    public_url: PublicUrl,
    proxies: TrustedProxies,
    limits: Limits,
    room: Room,
    hasher: Hasher,
    analyser: Analyser,
    /// Wrong passwords by handle and by client address.
    wrong_by_handle: Throttle,
    wrong_by_address: Throttle,
    /// Wrong passwords on requests that carry a session token, by session.
    wrong_by_session: Throttle,
    /// Attempts to create an account, by client address.
    registrations: Throttle,
    /// Uploaded files, by account.
    uploads: Throttle,
    /// Uploads under way.
    uploading: flights::Uploading,
    pictures: Semaphore,
    /// The connector's side of the Model Context Protocol.
    mcp: venti_mcp::Server,
    /// Requests to the connector, by client address.
    connector_requests: Throttle,
    /// Where requests come from, to notice a setup that hides it.
    clients: clients::ClientWatch,
    /// Where forecasts come from, on a server that was told where its
    /// forecast service is.
    forecasts: Option<Arc<dyn Forecasts>>,
}

impl<S: Store> Api<S> {
    /// An API over `store` that serves the sources described by `manifests`.
    pub fn new<'a>(
        store: Arc<S>,
        clock: Arc<dyn Clock>,
        manifests: impl IntoIterator<Item = &'a Manifest>,
        settings: Settings,
    ) -> Self {
        let sources: BTreeMap<SourceId, ServedSource> = manifests
            .into_iter()
            .map(|manifest| (manifest.id.clone(), ServedSource::from_manifest(manifest)))
            .collect();
        let limits = settings.limits;
        // No page in a browser may call the connector until the router is
        // told which may call the API.
        let nobody = Cors::Origins(Vec::new());
        let mcp = mcp::protocol(sources.values(), &nobody, &settings.public_url);
        Self {
            store,
            clock,
            sources,
            registration: settings.registration,
            public_url: settings.public_url,
            proxies: settings.trusted_proxies,
            room: settings.room,
            hasher: Hasher::new(settings.password_cost),
            analyser: settings.analyser,
            wrong_by_handle: Throttle::new(
                "handles with wrong passwords",
                limits.sign_in_window,
                limits.watched_keys,
            ),
            wrong_by_address: Throttle::new(
                "addresses that sent wrong passwords",
                limits.sign_in_window,
                limits.watched_keys,
            ),
            wrong_by_session: Throttle::new(
                "signed-in devices that sent wrong passwords",
                limits.sign_in_window,
                limits.watched_keys,
            ),
            registrations: Throttle::new(
                "addresses that asked for accounts",
                limits.registration_window,
                limits.watched_keys,
            ),
            uploads: Throttle::new(
                "accounts that uploaded flights",
                chrono::Duration::days(1),
                limits.watched_keys,
            ),
            uploading: flights::Uploading::new(),
            pictures: Semaphore::new(PICTURES_AT_ONCE),
            mcp,
            connector_requests: Throttle::new(
                "addresses that asked the connector",
                limits.connector_window,
                limits.watched_keys,
            ),
            clients: clients::ClientWatch::default(),
            limits,
            forecasts: None,
        }
    }

    /// The same API, answering forecasts from `forecasts`.
    ///
    /// Without this, or when none of the served sources is a source of
    /// forecasts, `/v1/forecast` says that the server has no forecasts.
    #[must_use]
    pub fn with_forecasts(mut self, forecasts: Arc<dyn Forecasts>) -> Self {
        self.forecasts = Some(forecasts);
        self
    }

    /// Whether data of this source may go out.
    fn serves(&self, source: &SourceId) -> bool {
        self.sources.contains_key(source)
    }

    /// The credits for a set of sources, sorted by source.
    fn attributions<'a>(
        &self,
        sources: impl IntoIterator<Item = &'a SourceId>,
    ) -> Vec<Attribution> {
        let sources: BTreeSet<&SourceId> = sources.into_iter().collect();
        sources
            .into_iter()
            .filter_map(|source| self.sources.get(source))
            .map(|served| served.attribution.clone())
            .collect()
    }

    /// The router that answers every request.
    ///
    /// From the outside in, a request passes: the request log, response
    /// compression, CORS, a note of the address it came from ([`clients`]),
    /// and then one of four groups of routes.
    ///
    /// - The API's ordinary routes: the guarantee that every error is a
    ///   JSON error body, the deadline, a small limit on the body, the
    ///   handler.
    /// - The upload of a flight file: the same, with a deadline and a body
    ///   limit that fit a file.
    /// - The share pages: they answer people, in HTML, errors included.
    /// - The connector: it answers assistants, in the protocol's own shape,
    ///   errors included, with a deadline and a body limit of its own.
    ///
    /// The request log names the method and the path and never a header,
    /// so no token reaches it; a password travels in a body, which is not
    /// logged either. How much is written about requests that fail is said
    /// in [`log`].
    pub fn router(mut self, cors: &Cors) -> Router {
        self.mcp = mcp::protocol(self.sources.values(), cors, &self.public_url);
        let deadline = |limit: Duration| {
            axum::middleware::from_fn(move |request, next| shape::deadline(limit, request, next))
        };

        let ordinary = Router::new()
            .route(HEALTH_PATH, get(health::health::<S>))
            .route(STATIONS_PATH, get(stations::stations::<S>))
            .route(
                "/v1/stations/{id}/observations",
                get(observations::observations::<S>),
            )
            .route(AIRSPACE_PATH, get(airspace::airspace::<S>))
            .route(SITES_PATH, get(sites::sites::<S>))
            .route(FORECAST_PATH, get(forecast::forecast::<S>))
            .route(ACCOUNTS_PATH, post(account::create::<S>))
            .route(
                ACCOUNT_PATH,
                get(account::show).delete(account::delete::<S>),
            )
            .route(ACCOUNT_PASSWORD_PATH, put(account::change_password::<S>))
            .route(
                SESSIONS_PATH,
                post(account::sign_in::<S>)
                    .get(account::sessions::<S>)
                    .delete(account::sign_out_everywhere::<S>),
            )
            .route("/v1/sessions/{id}", delete(account::sign_out::<S>))
            .route(FLIGHTS_PATH, get(flights::list::<S>))
            .route(FLIGHT_CHANGES_PATH, get(flights::changes::<S>))
            .route(
                "/v1/flights/{id}",
                get(flights::show::<S>)
                    .patch(flights::update::<S>)
                    .delete(flights::delete::<S>),
            )
            .route("/v1/flights/{id}/igc", get(flights::file::<S>))
            .route("/v1/pilots/{handle}", get(pilots::show::<S>))
            .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
            .layer(deadline(DEADLINE));

        let upload = Router::new()
            .route(FLIGHTS_PATH, post(flights::upload::<S>))
            // The framework's own limit is for forms; the layer below is
            // the one that counts here.
            .layer(DefaultBodyLimit::disable())
            .layer(RequestBodyLimitLayer::new(flights::MAX_UPLOAD_BYTES))
            .layer(deadline(UPLOAD_DEADLINE));

        let share = Router::new()
            .route("/f/{id}", get(share::page::<S>))
            .route("/f/{id}/preview.png", get(share::preview::<S>))
            .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
            .layer(deadline(DEADLINE));

        let connector = Router::new()
            .route(MCP_PATH, any(mcp::endpoint::<S>))
            // The framework's own limit is for forms; the layer below is
            // the one that counts here.
            .layer(DefaultBodyLimit::disable())
            .layer(RequestBodyLimitLayer::new(mcp::MAX_BODY_BYTES))
            .layer(axum::middleware::from_fn(|request, next| {
                mcp::deadline(DEADLINE, request, next)
            }))
            .layer(axum::middleware::from_fn(mcp::rpc_errors));

        let api = Arc::new(self);
        Router::new()
            .merge(ordinary)
            .merge(upload)
            .fallback(error::not_found)
            .layer(axum::middleware::from_fn(shape::uniform_errors))
            .merge(share)
            .merge(connector)
            .layer(axum::middleware::from_fn_with_state(
                api.clone(),
                clients::note::<S>,
            ))
            .with_state(api)
            .layer(cors_layer(cors))
            .layer(CompressionLayer::new())
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(log::request_span)
                    .on_failure(log::failure),
            )
    }
}

fn cors_layer(cors: &Cors) -> CorsLayer {
    let origins = match cors {
        Cors::Any => AllowOrigin::any(),
        Cors::Origins(origins) => AllowOrigin::list(
            origins
                .iter()
                .filter_map(|origin| HeaderValue::from_str(origin).ok()),
        ),
    };
    // No `allow_credentials`: see `Cors`.
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(AllowHeaders::list(
            [
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::IF_NONE_MATCH,
            ]
            .into_iter()
            .chain(mcp::REQUEST_HEADERS),
        ))
        .expose_headers([header::ETAG, header::RETRY_AFTER])
        .max_age(PREFLIGHT_MAX_AGE)
}
