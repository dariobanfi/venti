use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{CableLayer, CablesResponse, ErrorCode, MAX_CABLES};
use venti_domain::{BoundingBox, SourceId};
use venti_sources::Kind;
use venti_store::Store;

use super::error::{ApiError, bbox_asked};
use super::{Api, cache};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.70,47.66,11.82,47.74";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
}

/// `GET /v1/cables?bbox=west,south,east,north`: the cables that touch a
/// box, one layer per source the server holds cables of.
pub(crate) async fn cables<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Result<Response, ApiError> {
    let area = bbox_asked(params.bbox.as_deref(), BBOX_HELP)?;
    let answer = api.cables_in(&area).await?;
    Ok(cache::slow_json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The cables that touch a box, as the API answers it: every source
    /// this server is configured to serve cables of is a layer, also when
    /// nothing of it touches the box. A box that holds more than one
    /// answer carries is refused whole: see [`MAX_CABLES`].
    pub(crate) async fn cables_in(&self, area: &BoundingBox) -> Result<CablesResponse, ApiError> {
        let sources: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|(_, served)| served.kinds.contains(&Kind::Cables))
            .map(|(source, _)| source.clone())
            .collect();
        let found = self.store.cables_in(&sources, area, MAX_CABLES + 1).await?;
        let count: usize = found.iter().map(|layer| layer.cables.len()).sum();
        if count > MAX_CABLES {
            return Err(ApiError::bad_request(
                ErrorCode::BoxTooLarge,
                format!(
                    "the box holds more than {MAX_CABLES} cables, which is the most one answer \
                     carries; ask for it in several smaller boxes"
                ),
            ));
        }
        let layers: Vec<CableLayer> = found
            .into_iter()
            .map(|layer| CableLayer {
                source: layer.set.source,
                fetched_at: layer.set.fetched_at,
                as_of: layer.set.as_of,
                covers: layer.set.covers,
                cables: layer.cables,
            })
            .collect();
        let attributions = self.attributions(layers.iter().map(|layer| &layer.source));
        Ok(CablesResponse {
            layers,
            attributions,
        })
    }
}
