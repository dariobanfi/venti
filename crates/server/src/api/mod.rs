//! The HTTP API, version 1.
//!
//! The wire format lives in `venti-api-types`, shared with the clients;
//! `docs/api.md` describes the endpoints for people. This module turns
//! requests into store queries and store results into those types.
//!
//! Every response that carries data names the sources it came from and
//! carries their credit lines. A station whose source this server is not
//! configured to serve is never returned, even when the store still holds
//! it: without a manifest there is no licence to serve it under.

mod cache;
mod error;
mod health;
mod observations;
mod shape;
mod stations;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use venti_api_types::v1::{Attribution, HEALTH_PATH, STATIONS_PATH};
use venti_domain::SourceId;
use venti_sources::Manifest;
use venti_store::Store;

use crate::clock::Clock;

/// The largest request body accepted. The API only reads, so any body at
/// all is unexpected; this keeps a careless or hostile client cheap.
const MAX_BODY_BYTES: usize = 16 * 1024;

/// How long a request may take before the server gives up on it.
const DEADLINE: Duration = Duration::from_secs(10);

/// How long a browser may reuse the answer to a CORS preflight request.
const PREFLIGHT_MAX_AGE: Duration = Duration::from_secs(3600);

/// A source whose data the server serves, with what the API says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedSource {
    /// Who publishes the data.
    pub provider: String,
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
            cadence_seconds: manifest.cadence_seconds,
            attribution: Attribution {
                source: manifest.id.clone(),
                provider: manifest.provider.clone(),
                licence: manifest.licence.name.clone(),
                text: manifest.licence.attribution.clone(),
                url: manifest.licence.link.clone(),
            },
        }
    }
}

/// Which web origins may call the API from a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cors {
    /// Only these origins, for example `https://venti.example`.
    Origins(Vec<String>),
    /// Any origin. The development mode uses this.
    Any,
}

/// What the request handlers work with.
pub struct Api<S> {
    store: Arc<S>,
    clock: Arc<dyn Clock>,
    sources: BTreeMap<SourceId, ServedSource>,
}

impl<S: Store> Api<S> {
    /// An API over `store` that serves the sources described by `manifests`.
    pub fn new<'a>(
        store: Arc<S>,
        clock: Arc<dyn Clock>,
        manifests: impl IntoIterator<Item = &'a Manifest>,
    ) -> Self {
        let sources = manifests
            .into_iter()
            .map(|manifest| (manifest.id.clone(), ServedSource::from_manifest(manifest)))
            .collect();
        Self {
            store,
            clock,
            sources,
        }
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
    /// compression, CORS, the guarantee that every error is a JSON error
    /// body, the deadline, the body-size limit, and then its handler.
    pub fn router(self, cors: &Cors) -> Router {
        Router::new()
            .route(HEALTH_PATH, get(health::health::<S>))
            .route(STATIONS_PATH, get(stations::stations::<S>))
            .route(
                "/v1/stations/{id}/observations",
                get(observations::observations::<S>),
            )
            .fallback(error::not_found)
            .with_state(Arc::new(self))
            .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
            .layer(axum::middleware::from_fn(shape::deadline))
            .layer(axum::middleware::from_fn(shape::uniform_errors))
            .layer(cors_layer(cors))
            .layer(CompressionLayer::new())
            .layer(TraceLayer::new_for_http())
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
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::HEAD])
        .allow_headers([header::IF_NONE_MATCH])
        .expose_headers([header::ETAG])
        .max_age(PREFLIGHT_MAX_AGE)
}
