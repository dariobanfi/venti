//! What many gliders say together: the climbs of the last minutes, the
//! pilots in the air at flying sites, and where the public flights'
//! thermals have been.
//!
//! All three answers are the same whoever asks, name no pilot, no glider,
//! no track and no flight, and are read from the server's memory: the
//! rules are in [`crate::live::air`] and [`crate::flights::flown`], and
//! what may be derived from a network's positions is decided in
//! [`crate::live::network`]. A live answer may be kept for a few seconds
//! by anyone, the thermals for a minute.
//!
//! **Layers, never one record.** What comes from a network's positions
//! is a layer under the network's credit; what comes from Venti's own
//! live pilots is a layer of its own. A site in the answer about sites is
//! named by its identifier only: the site's record stays in its own
//! layer of `GET /v1/sites`, under its own licence.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{
    Attribution, DEFAULT_SITE_LAYERS, ErrorCode, FLOWN_CELL_DEG, FLOWN_MIN_PILOTS, FlightTerms,
    FlownThermalsResponse, LIVE_CLIMB_CELL_DEG, LIVE_CLIMB_MINUTES, LiveClimbsResponse,
    LiveSitesResponse, MAX_FLOWN_BOX_DEG, MAX_LIVE_OTHERS_BOX_DEG, SITE_AIR_WITHIN_M,
    SITE_TAKEOFF_MINUTES, VENTI_LIVE_SOURCE, parse_flown_hours,
};
use venti_domain::{BoundingBox, Timestamp};
use venti_store::Store;

use super::budget::Reader;
use super::error::{ApiError, bbox_asked};
use super::{Api, cache, takeoffs};
use crate::live::air::{self, Flyer, Place};
use crate::pilot_sites::Layer;

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

#[derive(Debug, Deserialize)]
pub(crate) struct BoxParams {
    bbox: Option<String>,
}

/// The box a request names, at most `widest` degrees each way.
fn boxed(bbox: Option<&str>, widest: f64) -> Result<BoundingBox, ApiError> {
    let area = bbox_asked(bbox, BBOX_HELP)?;
    if area.east() - area.west() > widest || area.north() - area.south() > widest {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidBbox,
            format!(
                "the box is larger than {widest} degrees from west to east or from south to \
                 north; ask for a smaller one"
            ),
        ));
    }
    Ok(area)
}

impl<S: Store> Api<S> {
    /// The gliders each source of live positions has heard lately, with
    /// the clock each is aged by: the networks by source, then Venti's
    /// own live pilots. Nothing of a glider that must not be shown is
    /// among them. Only those in or near `area` are read.
    fn flyers(&self, area: &BoundingBox) -> Vec<(String, Timestamp, Vec<Flyer>)> {
        let mut sources: Vec<(String, Timestamp, Vec<Flyer>)> = self
            .networks
            .iter()
            .map(|network| {
                let now = network.now();
                (
                    network.source().to_string(),
                    now,
                    network.flyers_near(now, area),
                )
            })
            .collect();
        let now = self.clock.now();
        sources.push((
            VENTI_LIVE_SOURCE.to_owned(),
            now,
            self.live_air.flyers_near(now, area, |_| true),
        ));
        sources
    }

    /// The clock a live answer says it is of: the first network's, as
    /// `GET /v1/live/others` says it, or the server's.
    fn air_time(&self) -> Timestamp {
        self.networks
            .first()
            .map_or_else(|| self.clock.now(), |network| network.now())
    }

    fn network_attributions(&self) -> Vec<Attribution> {
        self.networks
            .iter()
            .map(|network| network.attribution().clone())
            .collect()
    }

    /// The climbs of the last minutes in a box, as `GET /v1/live/climbs`
    /// answers them.
    pub(crate) fn climbs_in(&self, area: &BoundingBox) -> LiveClimbsResponse {
        LiveClimbsResponse {
            time: self.air_time(),
            listed_minutes: LIVE_CLIMB_MINUTES,
            cell_deg: LIVE_CLIMB_CELL_DEG,
            layers: self
                .flyers(area)
                .iter()
                .map(|(source, now, flyers)| air::climbs(source, flyers, area, *now))
                .collect(),
            attributions: self.network_attributions(),
        }
    }

    /// The flying sites of a box with pilots in the air, as
    /// `GET /v1/live/sites` answers them.
    pub(crate) async fn air_at_sites(
        &self,
        area: &BoundingBox,
    ) -> Result<LiveSitesResponse, ApiError> {
        // Every layer of sites this server has, the pilots' too.
        let layers = format!("{DEFAULT_SITE_LAYERS},{}", Layer::Pilots.name());
        let sites = self.sites_in(area, Some(&layers)).await?;
        let places: Vec<Place> = sites
            .layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .filter(|site| takeoffs::is_a_takeoff(site))
            .map(|site| Place {
                site: site.id.clone(),
                position: site.position,
                elevation_m: site.elevation_m,
            })
            .collect();
        Ok(LiveSitesResponse {
            time: self.air_time(),
            within_m: SITE_AIR_WITHIN_M,
            took_off_minutes: SITE_TAKEOFF_MINUTES,
            layers: self
                .flyers(area)
                .iter()
                .map(|(source, now, flyers)| air::sites(source, flyers, &places, *now))
                .collect(),
            attributions: self.network_attributions(),
        })
    }
}

/// `GET /v1/live/climbs?bbox=west,south,east,north`: where paragliders
/// and hang gliders circled and climbed in the last minutes.
pub(crate) async fn climbs<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<BoxParams>,
) -> Result<Response, ApiError> {
    let area = boxed(params.bbox.as_deref(), MAX_LIVE_OTHERS_BOX_DEG)?;
    Ok(cache::brief_json(&headers, &api.climbs_in(&area)))
}

/// `GET /v1/live/sites?bbox=west,south,east,north`: the flying sites of a
/// box with pilots in the air, and how many.
pub(crate) async fn sites<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<BoxParams>,
) -> Result<Response, ApiError> {
    let area = boxed(params.bbox.as_deref(), MAX_LIVE_OTHERS_BOX_DEG)?;
    Ok(cache::brief_json(&headers, &api.air_at_sites(&area).await?))
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlownParams {
    bbox: Option<String>,
    month: Option<String>,
    hours: Option<String>,
}

/// `GET /v1/thermals/flown?bbox=…&month=…&hours=…`: the cells of a box in
/// which the public flights of several pilots climbed.
///
/// It is made from public flights, so it counts as a read of them for the
/// address that asks, or the token ([`Api::count_public_read`]).
pub(crate) async fn flown<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    headers: HeaderMap,
    Query(params): Query<FlownParams>,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let area = boxed(params.bbox.as_deref(), MAX_FLOWN_BOX_DEG)?;
    let month = params
        .month
        .as_deref()
        .map(|text| {
            text.parse::<u8>()
                .ok()
                .filter(|month| (1..=12).contains(month))
                .ok_or_else(|| {
                    ApiError::bad_request(
                        ErrorCode::InvalidTime,
                        "month: a month of the year is a number from 1 to 12",
                    )
                })
        })
        .transpose()?;
    let hours = params
        .hours
        .as_deref()
        .map(|text| {
            parse_flown_hours(text).ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidTime,
                    "hours: from an hour of 0 to 23 to a later one, at most 24, by the sun at \
                     the place, for example hours=11-15",
                )
            })
        })
        .transpose()?;
    let (cells, more) = api.flown.cells(&area, month, hours);
    let answer = FlownThermalsResponse {
        cells,
        more,
        month,
        hours,
        cell_deg: FLOWN_CELL_DEG,
        min_pilots: FLOWN_MIN_PILOTS,
        flights: u32::try_from(api.flown.flights()).unwrap_or(u32::MAX),
        terms: FlightTerms::current(),
    };
    Ok(cache::json(&headers, &answer))
}
