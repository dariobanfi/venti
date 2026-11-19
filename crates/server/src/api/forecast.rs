use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{ErrorCode, ForecastLayer, ForecastResponse, parse_position};
use venti_domain::GeoPoint;
use venti_sources::Kind;
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};
use crate::forecast::Refusal;

const POSITION_HELP: &str =
    "give the place as lat and lon in degrees, for example lat=47.7&lon=11.8";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    lat: Option<String>,
    lon: Option<String>,
}

/// `GET /v1/forecast?lat=&lon=`: what the weather models say about a
/// place, one layer per source and one series per model.
pub(crate) async fn forecast<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Result<Response, ApiError> {
    let point = parse_position(params.lat.as_deref(), params.lon.as_deref()).map_err(|error| {
        ApiError::bad_request(
            ErrorCode::InvalidPosition,
            format!("{error}; {POSITION_HELP}"),
        )
    })?;
    let answer = api.forecast_for(point).await?;
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The forecast for a place, as the API answers it.
    ///
    /// A server that was not told where its forecast service is has no
    /// forecasts and says so; it asks nobody else. Only the layers of
    /// sources this server is configured to serve go out.
    pub(crate) async fn forecast_for(&self, point: GeoPoint) -> Result<ForecastResponse, ApiError> {
        let serves_forecasts = self
            .sources
            .values()
            .any(|served| served.kinds.contains(&Kind::Forecasts));
        let Some(forecasts) = self.forecasts.as_ref().filter(|_| serves_forecasts) else {
            return Err(ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                ErrorCode::NoForecasts,
                "this server has no forecast service, so it has no forecasts; \
                 /v1/health says the same under \"forecasts\"",
            ));
        };

        let forecast = forecasts.forecast(point).await.map_err(refused)?;
        let layers: Vec<ForecastLayer> = forecast
            .layers
            .iter()
            .filter(|(source, _)| self.serves(source))
            .map(|(source, series)| ForecastLayer {
                source: source.clone(),
                series: series.clone(),
            })
            .collect();
        let attributions = self.attributions(layers.iter().map(|layer| &layer.source));
        Ok(ForecastResponse {
            point: forecast.point,
            fetched_at: forecast.fetched_at,
            layers,
            attributions,
        })
    }
}

/// Why there is no forecast, as the API says it.
fn refused(refusal: Refusal) -> ApiError {
    match refusal {
        Refusal::OutsideRegion => ApiError::missing(
            ErrorCode::OutsideRegion,
            "the place lies outside the area this server has forecasts for",
        ),
        Refusal::NotYet { retry_after_s } => ApiError::busy(
            "the forecast for this place is being fetched and is not there yet; \
             ask again in a few seconds",
        )
        .retry_after(retry_after_s),
        Refusal::AskedEnough { retry_after_s } => ApiError::busy(
            "the server has asked its forecast service as often as it may in an hour; \
             places that were asked for in the last half hour are still answered",
        )
        .retry_after(retry_after_s),
        Refusal::SourceFailed => ApiError::busy(
            "the forecast service did not answer, or had no forecast for this place; \
             try again shortly",
        ),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::header;
    use axum::response::IntoResponse;

    use super::*;

    #[test]
    fn a_refusal_says_whether_and_when_to_ask_again() {
        let outside = refused(Refusal::OutsideRegion);
        assert_eq!(outside.status(), StatusCode::NOT_FOUND);
        assert_eq!(outside.code(), ErrorCode::OutsideRegion);

        let not_yet = refused(Refusal::NotYet { retry_after_s: 5 }).into_response();
        assert_eq!(not_yet.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(not_yet.headers()[header::RETRY_AFTER], "5");
        assert_eq!(not_yet.headers()[header::CACHE_CONTROL], "no-store");

        let full = refused(Refusal::AskedEnough {
            retry_after_s: 2700,
        });
        assert_eq!(full.code(), ErrorCode::Unavailable);
        assert_eq!(full.into_response().headers()[header::RETRY_AFTER], "2700");

        let failed = refused(Refusal::SourceFailed).into_response();
        assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failed.headers()[header::RETRY_AFTER], "30");
    }
}
