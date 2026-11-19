use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{ErrorCode, MAX_SITES, SiteLayer, SitesResponse, parse_bbox};
use venti_domain::{BoundingBox, SourceId};
use venti_sources::Kind;
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
}

/// `GET /v1/sites?bbox=west,south,east,north`: the flying sites inside a
/// box, one layer per source the server holds sites of.
pub(crate) async fn sites<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Result<Response, ApiError> {
    let bbox = params.bbox.ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidBbox,
            format!("the bbox parameter is missing; {BBOX_HELP}"),
        )
    })?;
    let area = parse_bbox(&bbox).map_err(|error| {
        ApiError::bad_request(ErrorCode::InvalidBbox, format!("{error}; {BBOX_HELP}"))
    })?;

    let answer = api.sites_in(&area).await?;
    Ok(cache::slow_json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The flying sites inside a box, as the API answers them.
    ///
    /// As for airspace, every source the server holds a set of is a layer,
    /// also when it has nothing in the box: the layer says how old its set
    /// is and where it is whole. Only sources this server is configured to
    /// serve sites of are asked for. A box that holds more than one answer
    /// carries is refused whole: see [`MAX_SITES`].
    pub(crate) async fn sites_in(&self, area: &BoundingBox) -> Result<SitesResponse, ApiError> {
        let sources: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|(_, served)| served.kinds.contains(&Kind::Sites))
            .map(|(source, _)| source.clone())
            .collect();
        // One more than an answer carries, to know whether the box holds more.
        let found = self.store.sites_in(&sources, area, MAX_SITES + 1).await?;
        let count: usize = found.iter().map(|layer| layer.sites.len()).sum();
        if count > MAX_SITES {
            return Err(ApiError::bad_request(
                ErrorCode::BoxTooLarge,
                format!(
                    "the box holds more than {MAX_SITES} sites, which is the most one answer \
                     carries; ask for it in several smaller boxes"
                ),
            ));
        }

        let layers: Vec<SiteLayer> = found
            .into_iter()
            .map(|layer| SiteLayer {
                source: layer.set.source,
                fetched_at: layer.set.fetched_at,
                as_of: layer.set.as_of,
                covers: layer.set.covers,
                sites: layer.sites,
            })
            .collect();
        let attributions = self.attributions(layers.iter().map(|layer| &layer.source));
        Ok(SitesResponse {
            layers,
            attributions,
        })
    }
}
