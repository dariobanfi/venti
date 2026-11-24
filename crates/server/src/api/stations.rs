use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{MAX_STATIONS, Reading, Station, StationWithLatest, StationsResponse};
use venti_domain::{BoundingBox, SourceId};
use venti_store::Store;

use super::error::{ApiError, bbox_asked};
use super::{Api, cache, health, same_place};

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
    let area = bbox_asked(params.bbox.as_deref(), BBOX_HELP)?;

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
                station: self.station(&entry.station),
                latest: entry.latest.as_ref().map(Reading::from),
            })
            .collect();
        let attributions = self.attributions(stations.iter().map(|entry| &entry.station.source));
        let late_sources = self
            .late_among(attributions.iter().map(|credit| credit.source.clone()))
            .await?;
        Ok(StationsResponse {
            stations,
            truncated,
            attributions,
            late_sources,
        })
    }

    /// A station as the API hands it out: with the short name of its
    /// network.
    pub(crate) fn station(&self, station: &venti_domain::Station) -> Station {
        Station {
            network: self
                .sources
                .get(station.id.source())
                .and_then(|served| served.attribution.short_name.clone()),
            ..Station::from(station)
        }
    }

    /// Which of `sources` are stale, as `/v1/health` judges it. Only
    /// sources whose stations are in an answer are asked about: a late
    /// source elsewhere, or one that never delivered and so has no station
    /// anywhere, is for `/v1/health` to name.
    async fn late_among(
        &self,
        sources: impl IntoIterator<Item = SourceId>,
    ) -> Result<Vec<SourceId>, ApiError> {
        let sources: Vec<SourceId> = sources.into_iter().collect();
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        let now = self.clock.now();
        let statuses = self.store.source_status(&sources).await?;
        Ok(statuses
            .into_iter()
            .filter(|status| {
                self.sources.get(&status.source).is_some_and(|served| {
                    health::stale(served.cadence_seconds, status.last_success, now)
                })
            })
            .map(|status| status.source)
            .collect())
    }
}
