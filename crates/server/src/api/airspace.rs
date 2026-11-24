use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{AirspaceLayer, AirspaceResponse, ErrorCode, MAX_AIRSPACES};
use venti_domain::{BoundingBox, SourceId};
use venti_sources::Kind;
use venti_store::Store;

use super::error::{ApiError, bbox_asked};
use super::{Api, cache};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=10.5,47.2,11.9,47.8";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
}

/// `GET /v1/airspace?bbox=west,south,east,north`: the airspace that touches
/// a box, one layer per source the server holds airspace of.
pub(crate) async fn airspace<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Result<Response, ApiError> {
    let area = bbox_asked(params.bbox.as_deref(), BBOX_HELP)?;

    let answer = api.airspace_in(&area).await?;
    Ok(cache::slow_json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The airspace that touches a box, as the API answers it.
    ///
    /// Every source the server holds a set of is a layer, also when
    /// nothing of it touches the box: the layer says where its set is
    /// whole, and a client can tell "no airspace here" from "not held"
    /// by that and by nothing else. Such a layer is credited like any
    /// other, because "nothing here" is a statement from the source's
    /// data and carries its warning.
    ///
    /// Only sources this server is configured to serve airspace of are
    /// asked for, so what the store still holds of a source that was taken
    /// out of the configuration stays inside. A box that holds more than
    /// one answer carries is refused whole: see
    /// [`MAX_AIRSPACES`].
    pub(crate) async fn airspace_in(
        &self,
        area: &BoundingBox,
    ) -> Result<AirspaceResponse, ApiError> {
        let sources: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|(_, served)| served.kinds.contains(&Kind::Airspace))
            .map(|(source, _)| source.clone())
            .collect();
        // One more than an answer carries, to know whether the box holds more.
        let found = self
            .store
            .airspaces_in(&sources, area, MAX_AIRSPACES + 1)
            .await?;
        let count: usize = found.iter().map(|layer| layer.airspaces.len()).sum();
        if count > MAX_AIRSPACES {
            return Err(ApiError::bad_request(
                ErrorCode::BoxTooLarge,
                format!(
                    "the box holds more than {MAX_AIRSPACES} airspaces, which is the most one \
                     answer carries; ask for it in several smaller boxes"
                ),
            ));
        }

        let layers: Vec<AirspaceLayer> = found
            .into_iter()
            .map(|layer| AirspaceLayer {
                source: layer.set.source,
                fetched_at: layer.set.fetched_at,
                file_date: layer.set.file_date,
                newest_change: layer.set.newest_change,
                covers: layer.set.coverage.boxes,
                countries: layer.set.coverage.countries,
                airspaces: layer.airspaces,
            })
            .collect();
        let attributions = self.attributions(layers.iter().map(|layer| &layer.source));
        Ok(AirspaceResponse {
            layers,
            attributions,
        })
    }
}
