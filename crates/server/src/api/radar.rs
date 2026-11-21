//! `GET /v1/radar` and `GET /v1/radar/{source}/{time}.png`: the rain radar
//! frames the server holds ([`crate::radar`]), as a list and as pictures.
//!
//! | Answer | When | May be kept |
//! |---|---|---|
//! | `200`, the list | always, on a server that collects radar; a source it holds no frame of is a layer without frames | a minute |
//! | `501` | the server collects no radar | |
//! | `200`, `image/png` | a frame the server holds | a day: a frame of a moment does not change |
//! | `404` | no such source, not the name of a frame, or a frame the server does not hold (any longer) | |

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::{
    ErrorCode, RadarFrameRef, RadarHealth, RadarLayer, RadarResponse, parse_radar_time,
    radar_frame_path,
};
use venti_domain::{RADAR_UNSEEN, RAIN_STEPS_MM_H, SourceId};
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};
use crate::radar::Radar;

/// The route of a frame's picture, as the router names it.
pub(crate) const FRAME_ROUTE: &str = "/v1/radar/{source}/{frame}";

/// How long a frame's picture may be kept: a day. The server itself drops
/// it after two hours; a picture of a moment stays the picture of that
/// moment.
const KEPT_A_DAY: &str = "public, max-age=86400, immutable";

/// `GET /v1/radar`: the frames the server holds, in layers by source.
pub(crate) async fn radar<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let answer = api.radar_frames()?;
    Ok(cache::json(&headers, &answer))
}

/// `GET /v1/radar/{source}/{time}.png`: one frame's picture.
pub(crate) async fn frame<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path((source, frame)): Path<(String, String)>,
) -> Response {
    let not_found = || {
        ApiError::missing(
            ErrorCode::NotFound,
            "there is no such frame: /v1/radar lists the frames this server holds, each with \
             its path; a frame older than two hours is dropped",
        )
        .into_response()
    };
    let Some(radar) = api.radar_of_served() else {
        return not_found();
    };
    let Some(time) = frame.strip_suffix(".png").and_then(parse_radar_time) else {
        return not_found();
    };
    let Some(picture) = SourceId::new(source)
        .ok()
        .filter(|source| api.serves(source))
        .and_then(|source| radar.picture(&source, time))
    else {
        return not_found();
    };
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (header::CACHE_CONTROL, HeaderValue::from_static(KEPT_A_DAY)),
        ],
        picture.png.clone(),
    )
        .into_response()
}

/// A degree to six decimals, a tenth of a metre: the edges a grid works
/// out go on the wire without the last digits of the arithmetic.
fn millionths(degrees: f64) -> f64 {
    (degrees * 1e6).round() / 1e6
}

impl<S: Store> Api<S> {
    /// The radar, if the server collects any.
    fn radar_of_served(&self) -> Option<&Arc<dyn Radar>> {
        self.radar.as_ref()
    }

    /// The list of radar frames, as the API answers it, with each
    /// source's credit. A server that collects no radar says so.
    pub(crate) fn radar_frames(&self) -> Result<RadarResponse, ApiError> {
        let radar = self.radar_of_served().ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                ErrorCode::NoRadar,
                "this server collects no rain radar: none of its regions names a radar source",
            )
        })?;
        let layers: Vec<RadarLayer> = radar
            .layers()
            .into_iter()
            .filter(|layer| self.serves(&layer.source))
            .map(|layer| RadarLayer {
                west: layer.grid.west,
                south: millionths(layer.grid.south()),
                east: millionths(layer.grid.east()),
                north: layer.grid.north,
                columns: layer.grid.columns,
                rows: layer.grid.rows,
                cadence_seconds: self
                    .sources
                    .get(&layer.source)
                    .map_or(0, |served| served.cadence_seconds),
                frames: layer
                    .frames
                    .iter()
                    .map(|picture| RadarFrameRef {
                        time: picture.time,
                        interval_s: picture.interval_s,
                        path: radar_frame_path(&layer.source, picture.time),
                        bytes: u32::try_from(picture.png.len()).unwrap_or(u32::MAX),
                    })
                    .collect(),
                source: layer.source,
            })
            .collect();
        Ok(RadarResponse {
            time: radar.now(),
            steps_mm_h: RAIN_STEPS_MM_H.to_vec(),
            unseen: RADAR_UNSEEN,
            attributions: self.attributions(layers.iter().map(|layer| &layer.source)),
            layers,
        })
    }

    /// Where the radar stands, for the health report: per source the
    /// newest frame held, its age by the radar's clock, and what the
    /// pictures weigh.
    pub(crate) fn radar_health(&self) -> Vec<RadarHealth> {
        let Some(radar) = self.radar_of_served() else {
            return Vec::new();
        };
        let now = radar.now();
        radar
            .layers()
            .into_iter()
            .filter(|layer| self.serves(&layer.source))
            .map(|layer| {
                let newest = layer.frames.last().map(|picture| picture.time);
                RadarHealth {
                    source: layer.source,
                    frames: u32::try_from(layer.frames.len()).unwrap_or(u32::MAX),
                    newest_frame: newest,
                    newest_frame_age_s: newest.map(|newest| (now - newest).num_seconds()),
                    bytes: layer
                        .frames
                        .iter()
                        .map(|picture| picture.png.len() as u64)
                        .sum(),
                }
            })
            .collect()
    }
}
