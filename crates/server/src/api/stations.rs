use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{
    ErrorCode, MAX_STATIONS, Reading, Station, StationWithLatest, StationsResponse, parse_bbox,
};
use venti_domain::BoundingBox;
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache, same_place};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=10.5,47.2,11.9,47.8";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
}

/// `GET /v1/stations?bbox=west,south,east,north`: the stations inside a box,
/// each with its newest observation.
pub(crate) async fn stations<S: Store>(
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

    let answer = api.stations_in(&area).await?;
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The stations inside a box, each with its newest observation, as the
    /// API answers them. The connector's wind tool is answered from this
    /// too.
    ///
    /// Where two sources have a station at the same place, one of the two
    /// is in the answer: see [`same_place`]. The other is still collected,
    /// and its own observations still answer by its identifier.
    pub(crate) async fn stations_in(
        &self,
        area: &BoundingBox,
    ) -> Result<StationsResponse, ApiError> {
        // The stations of the box and of a little room around it, so that a
        // station at the edge is judged against its neighbour outside. One
        // more than a response lists, to know whether there are more.
        let around = self
            .store
            .stations_in(&same_place::with_room(area), MAX_STATIONS + 1)
            .await?;
        let (mut found, truncated) = if around.len() > MAX_STATIONS {
            // More than a response lists. The box is asked for by itself
            // and cut as it always was; a station at its edge may then be
            // listed beside the one that stands for it outside.
            let mut found = self.store.stations_in(area, MAX_STATIONS + 1).await?;
            let truncated = found.len() > MAX_STATIONS;
            found.truncate(MAX_STATIONS);
            (found, truncated)
        } else {
            (around, false)
        };
        found.retain(|entry| self.serves(entry.station.id.source()));
        let left_out = same_place::left_out(&found, &self.station_priority, self.clock.now());

        let stations: Vec<StationWithLatest> = found
            .iter()
            .zip(left_out)
            .filter(|(entry, left_out)| !left_out && area.contains(entry.station.position))
            .map(|(entry, _)| StationWithLatest {
                station: Station::from(&entry.station),
                latest: entry.latest.as_ref().map(Reading::from),
            })
            .collect();
        let attributions = self.attributions(stations.iter().map(|entry| &entry.station.source));
        Ok(StationsResponse {
            stations,
            truncated,
            attributions,
        })
    }
}
