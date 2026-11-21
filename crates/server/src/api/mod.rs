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
//! flight that does not exist get the same answer. Only a flight its pilot
//! made public is in a list that others read: the pilot's page and the list
//! of everyone's public flights ([`feed`]). What may keep an answer is said
//! in [`cache`]. A pilot who is signed in can report what others share
//! ([`reports`]); what whoever runs the server does about it is done from
//! the command line and shows here in two places: a flight that was taken
//! down stays private, and an account whose publishing is suspended makes
//! nothing public.
//!
//! **What pilots share with a group.** A group and the plans its members
//! make for a flying day are shown to the group's members and to nobody
//! else: to any other account a group answers exactly as one that does not
//! exist ([`groups`]). Nothing of it is in any answer that others read.
//!
//! **The map regions the server hands out** ([`maps`]) are neither: they
//! are files that whoever runs the server put in a folder, open data with
//! the credits their descriptions name. Anyone may list and fetch them.
//!
//! **Map imagery** ([`imagery`]) is a provider's photographs from above,
//! passed on tile by tile and cut where they may not be shown: the one
//! other way a request can make the server ask a provider, for a tile at
//! an address the adapter makes from three checked numbers.
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
mod feed;
mod flights;
mod forecast;
mod groups;
mod health;
mod imagery;
mod live;
mod log;
mod maps;
mod mcp;
mod observations;
mod pilots;
mod ranking;
mod reports;
mod shape;
mod share;
mod sites;
mod stations;
mod thermals;
mod wire;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{any, delete, get, post, put};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use venti_api_types::v1::{
    ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNTS_PATH, AIRSPACE_PATH, Attribution,
    FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FORECAST_PATH, GROUP_JOIN_PATH, GROUPS_PATH, HEALTH_PATH,
    LIVE_PATH, LIVE_POSITIONS_PATH, MAPS_PATH, MAX_LIVE_BODY_BYTES, PLAN_BOARD_PATH,
    PUBLIC_FLIGHTS_PATH, RANKING_PATH, REPORTS_PATH, SESSIONS_PATH, SITES_PATH, STATIONS_PATH,
    THERMAL_HOURS_PATH, THERMALS_PATH,
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
use crate::imagery::Imagery;
use crate::maps::Maps;
use crate::thermals::ThermalMap;

pub use mcp::MCP_PATH;

/// The largest request body accepted where a request carries JSON or
/// nothing: far more than a sign-in needs, and too little to be a burden.
const MAX_BODY_BYTES: usize = 16 * 1024;

/// How long a request may take before the server gives up on it.
const DEADLINE: Duration = Duration::from_secs(10);

/// How long an upload may take, from its first byte to the answer. The
/// handler has bounds of its own for every part (the file's arrival, the
/// wait for an analysis, the analysis); this is behind them, for whatever
/// they miss.
const UPLOAD_DEADLINE: Duration = Duration::from_secs(150);

/// How long a browser may reuse the answer to a CORS preflight request.
const PREFLIGHT_MAX_AGE: Duration = Duration::from_secs(3600);

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
    /// What bounds live tracking, and for how long a live track is kept.
    pub live: crate::live::Limits,
    /// What bounds groups and their plans, and for how long a plan is
    /// kept after its day.
    pub groups: crate::groups::Limits,
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
    /// Reports sent, by account.
    reports: Throttle,
    /// Uploads under way.
    uploading: flights::Uploading,
    /// The preview pictures of shared flights: the turns at drawing one,
    /// and the ones drawn last.
    pictures: share::Pictures,
    /// The connector's side of the Model Context Protocol.
    mcp: venti_mcp::Server,
    /// Requests to the connector, by client address.
    connector_requests: Throttle,
    /// Forecasts that had to be fetched, by client address.
    forecast_fetches: Throttle,
    /// Where requests come from, to notice a setup that hides it.
    clients: clients::ClientWatch,
    /// Where forecasts come from, on a server that was told where its
    /// forecast service is.
    forecasts: Option<Arc<dyn Forecasts>>,
    /// The thermal map, on a server that makes one.
    thermal_map: Option<Arc<ThermalMap>>,
    /// Live tracking: its limits, and what each account has used of them.
    live: live::Live,
    /// Groups and plans: their limits, and what each account has used of
    /// them.
    groups: groups::Groups,
    /// The map regions the server hands out. None, unless it was given
    /// some.
    maps: Maps,
    /// The map imagery the server passes on, if it passes any on.
    imagery: Option<Arc<Imagery>>,
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
            reports: Throttle::new(
                "accounts that sent reports",
                chrono::Duration::days(1),
                limits.watched_keys,
            ),
            uploading: flights::Uploading::new(),
            pictures: share::Pictures::default(),
            mcp,
            connector_requests: Throttle::new(
                "addresses that asked the connector",
                limits.connector_window,
                limits.watched_keys,
            ),
            forecast_fetches: Throttle::new(
                "addresses that asked for forecasts of new places",
                limits.forecast_window,
                limits.watched_keys,
            ),
            clients: clients::ClientWatch::default(),
            limits,
            forecasts: None,
            thermal_map: None,
            live: live::Live::new(settings.live, limits.watched_keys),
            groups: groups::Groups::new(settings.groups, limits.watched_keys),
            maps: Maps::default(),
            imagery: None,
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

    /// The same API, answering `/v1/thermals` and `/v1/thermals/hours`
    /// from `map`.
    ///
    /// Without this, a server with forecasts says that its thermal map is
    /// not there yet, and one without says that it has no forecasts.
    #[must_use]
    pub fn with_thermal_map(mut self, map: Arc<ThermalMap>) -> Self {
        self.thermal_map = Some(map);
        self
    }

    /// The same API, handing out the map regions in `maps`.
    ///
    /// Without this `/v1/maps` lists no region. The list follows `maps`:
    /// a region that is read after this call is listed from then on.
    #[must_use]
    pub fn with_maps(mut self, maps: Maps) -> Self {
        self.maps = maps;
        self
    }

    /// The same API, passing on the map imagery of `imagery`.
    ///
    /// Without this every address under `/v1/imagery` is "not found".
    #[must_use]
    pub fn with_imagery(mut self, imagery: Arc<Imagery>) -> Self {
        self.imagery = Some(imagery);
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
    /// and then one of five groups of routes. The files of a map region
    /// are a sixth group, which response compression is not around.
    ///
    /// - The API's ordinary routes: the guarantee that every error is a
    ///   JSON error body, the deadline, a small limit on the body, the
    ///   handler. A tile of map imagery is one of them: compression leaves
    ///   images alone.
    /// - The fixes of a live track: the same, with a body limit that fits
    ///   a batch of them.
    /// - The upload of a flight file: the same, with a deadline and a body
    ///   limit that fit a file.
    /// - The share pages, of a flight and of a live track: they answer
    ///   people, in HTML, errors included.
    /// - The connector: it answers assistants, in the protocol's own shape,
    ///   errors included, with a deadline and a body limit of its own.
    /// - The files of a map region ([`maps`]): the request log, CORS, the
    ///   note of the address, the guarantee about errors, the handler. A
    ///   file is sent as it is on the disk, never compressed: a compressed
    ///   answer has no length and cannot be taken up again where it broke
    ///   off. And the group has no deadline and no limit of any kind: a
    ///   download of some gigabytes takes as long as the client's network
    ///   needs.
    ///
    /// The request log names the method and the path and never a header,
    /// so no token reaches it; a password travels in a body, which is not
    /// logged either. Of a path that names a live track it leaves the
    /// track's identifier out, and of a path under a group everything
    /// that names the group, a plan or a member. How much is written
    /// about requests that fail is said in [`log`].
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
            .route(THERMALS_PATH, get(thermals::thermals::<S>))
            .route(THERMAL_HOURS_PATH, get(thermals::thermal_hours::<S>))
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
            .route(PUBLIC_FLIGHTS_PATH, get(feed::list::<S>))
            .route(RANKING_PATH, get(ranking::list::<S>))
            .route(
                "/v1/flights/{id}",
                get(flights::show::<S>)
                    .patch(flights::update::<S>)
                    .delete(flights::delete::<S>),
            )
            .route("/v1/flights/{id}/igc", get(flights::file::<S>))
            .route("/v1/pilots/{handle}", get(pilots::show::<S>))
            .route(LIVE_PATH, get(live::list::<S>))
            .route(REPORTS_PATH, post(reports::create::<S>))
            .route(
                "/v1/live/{id}",
                get(live::show::<S>).delete(live::delete::<S>),
            )
            .route(
                GROUPS_PATH,
                post(groups::create::<S>).get(groups::list::<S>),
            )
            .route(GROUP_JOIN_PATH, post(groups::join::<S>))
            .route(PLAN_BOARD_PATH, get(groups::board::<S>))
            .route(MAPS_PATH, get(maps::list::<S>))
            .route(imagery::ROUTE, get(imagery::tile::<S>))
            .route(
                "/v1/groups/{id}",
                get(groups::show::<S>)
                    .patch(groups::rename::<S>)
                    .delete(groups::delete::<S>),
            )
            .route("/v1/groups/{id}/invite", post(groups::renew_invite::<S>))
            .route("/v1/groups/{id}/keeper", put(groups::hand_over::<S>))
            .route(
                "/v1/groups/{id}/members/{handle}",
                delete(groups::remove_member::<S>),
            )
            .route(
                "/v1/groups/{id}/plans",
                get(groups::plans::<S>).post(groups::post_plan::<S>),
            )
            .route(
                "/v1/groups/{id}/plans/{plan}",
                put(groups::change_plan::<S>).delete(groups::delete_plan::<S>),
            )
            .route(
                "/v1/groups/{id}/plans/{plan}/in",
                put(groups::say_in::<S>).delete(groups::take_back::<S>),
            )
            .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
            .layer(deadline(DEADLINE));

        // The fixes of a live track: a batch is larger than the other JSON
        // bodies may be, and far smaller than a file.
        let positions = Router::new()
            .route(LIVE_POSITIONS_PATH, post(live::send::<S>))
            .layer(RequestBodyLimitLayer::new(MAX_LIVE_BODY_BYTES))
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
            .route("/live/{id}", get(live::page::<S>))
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
        // The files of a map region. The group is merged in below, after
        // the compression is laid around the others, and it has no
        // deadline: see above. `get` answers `HEAD` as well.
        let map_files = Router::new()
            .route(maps::FILE_ROUTE, get(maps::file::<S>))
            .layer(axum::middleware::from_fn(shape::uniform_errors))
            .layer(axum::middleware::from_fn_with_state(
                api.clone(),
                clients::note::<S>,
            ))
            .with_state(api.clone())
            .layer(cors_layer(cors));

        Router::new()
            .merge(ordinary)
            .merge(positions)
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
            .merge(map_files)
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
                // For a part of a map file.
                header::RANGE,
                header::IF_RANGE,
            ]
            .into_iter()
            .chain(mcp::REQUEST_HEADERS),
        ))
        .expose_headers([
            header::ETAG,
            header::RETRY_AFTER,
            header::ACCEPT_RANGES,
            header::CONTENT_RANGE,
        ])
        .max_age(PREFLIGHT_MAX_AGE)
}
