use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use chrono::NaiveDate;
use serde::Deserialize;
use venti_api_types::v1::{
    ErrorCode, ThermalHoursResponse, ThermalLayer, ThermalMapResponse, format_day, parse_day,
};
use venti_sources::Kind;
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};
use crate::thermals::{MAP_HEIGHTS_M, ThermalMap};

const DATE_HELP: &str = "give the day as date=YYYY-MM-DD, one of the days /v1/thermals lists";

#[derive(Debug, Deserialize)]
pub(crate) struct HoursParams {
    date: Option<String>,
    layer: Option<String>,
    height: Option<String>,
}

/// After how many seconds a client that was told the map is not there yet
/// should ask again. Making the map takes a minute or two after a start.
const COME_BACK_S: u64 = 60;

/// `GET /v1/thermals`: the server's thermal map, the kind of day cell by
/// cell over the area it has forecasts for.
pub(crate) async fn thermals<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let answer = api.thermal_map()?;
    Ok(cache::json(&headers, &answer))
}

/// `GET /v1/thermals/hours?date=&layer=&height=`: one layer of one day of
/// the thermal map hour by hour: the hours' verdicts and thermals, or the
/// model's wind, cloud or rain in each.
pub(crate) async fn thermal_hours<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<HoursParams>,
) -> Result<Response, ApiError> {
    let date = params
        .date
        .as_deref()
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidDay,
                format!("date is missing; {DATE_HELP}"),
            )
        })
        .and_then(|text| {
            parse_day(text).map_err(|error| {
                ApiError::bad_request(ErrorCode::InvalidDay, format!("date: {error}; {DATE_HELP}"))
            })
        })?;
    // Without a layer, the hours' verdicts: what the map of the days is
    // made of.
    let layer = match params.layer.as_deref() {
        None => ThermalLayer::Thermals,
        Some(text) => ThermalLayer::parse(text).ok_or_else(|| {
            let layers: Vec<&str> = ThermalLayer::ALL
                .iter()
                .map(|layer| layer.as_str())
                .collect();
            ApiError::bad_request(
                ErrorCode::InvalidRequest,
                format!("layer is one of {}", layers.join(", ")),
            )
        })?,
    };
    let height_m = match params.height.as_deref() {
        None => None,
        Some(_) if layer != ThermalLayer::Wind => {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidRequest,
                "height goes with layer=wind: the other layers have no height",
            ));
        }
        Some(text) => Some(
            text.parse::<i32>()
                .ok()
                .filter(|asked| MAP_HEIGHTS_M.iter().any(|kept| i32::from(*kept) == *asked))
                .ok_or_else(|| {
                    let heights: Vec<String> =
                        MAP_HEIGHTS_M.iter().map(ToString::to_string).collect();
                    ApiError::bad_request(
                        ErrorCode::InvalidRequest,
                        format!(
                            "height is one of {} (metres above mean sea level), or left out \
                             for the wind 10 m above the model's ground",
                            heights.join(", ")
                        ),
                    )
                })?,
        ),
    };
    let answer = api.thermal_hours(date, layer, height_m)?;
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The thermal map, as the API answers it, with its source's credit.
    ///
    /// A server without a forecast service has no map and says so as for a
    /// forecast. One that has a service and no map yet, after a start or
    /// while its source does not answer, says when to ask again.
    pub(crate) fn thermal_map(&self) -> Result<ThermalMapResponse, ApiError> {
        let (_, made) = self.made_thermal_map()?;
        let mut answer = ThermalMapResponse::clone(&made);
        answer.attributions = self.attributions([&answer.source]);
        Ok(answer)
    }

    /// One layer of the hours of one day of the thermal map, as the API
    /// answers it, with the source's credit. Refused as the map itself is
    /// while there is none; a day the map does not have is not found.
    /// `height_m` is one of the heights the map keeps, or none.
    pub(crate) fn thermal_hours(
        &self,
        date: NaiveDate,
        layer: ThermalLayer,
        height_m: Option<i32>,
    ) -> Result<ThermalHoursResponse, ApiError> {
        let (map, made) = self.made_thermal_map()?;
        let mut answer = map
            .hours_of(date)
            .and_then(|day| day.layer(layer, height_m))
            .ok_or_else(|| {
                let has: Vec<String> = made.days.iter().map(|day| format_day(day.date)).collect();
                ApiError::new(
                    StatusCode::NOT_FOUND,
                    ErrorCode::NotFound,
                    format!(
                        "the thermal map has no hours for {}; it has {}",
                        format_day(date),
                        has.join(", ")
                    ),
                )
            })?;
        answer.attributions = self.attributions([&answer.source]);
        Ok(answer)
    }

    /// The server's thermal map and the days it serves, or why there is
    /// none.
    fn made_thermal_map(&self) -> Result<(&Arc<ThermalMap>, Arc<ThermalMapResponse>), ApiError> {
        let serves_forecasts = self
            .sources
            .values()
            .any(|served| served.kinds.contains(&Kind::Forecasts));
        if self.forecasts.is_none() || !serves_forecasts {
            return Err(ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                ErrorCode::NoForecasts,
                "this server has no forecast service, so it has no thermal map; \
                 /v1/health says the same under \"forecasts\"",
            ));
        }
        self.thermal_map
            .as_ref()
            .filter(|map| self.serves(map.source()))
            .and_then(|map| Some((map, map.current()?)))
            .ok_or_else(|| {
                ApiError::busy(
                    "the thermal map is not made yet: the server makes it from each run of \
                     the model, and after a start that takes a few minutes",
                )
                .retry_after(COME_BACK_S)
            })
    }
}
