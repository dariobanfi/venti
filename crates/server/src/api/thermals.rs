use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use venti_api_types::v1::{ErrorCode, ThermalMapResponse};
use venti_sources::Kind;
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};

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

impl<S: Store> Api<S> {
    /// The thermal map, as the API answers it, with its source's credit.
    ///
    /// A server without a forecast service has no map and says so as for a
    /// forecast. One that has a service and no map yet, after a start or
    /// while its source does not answer, says when to ask again.
    pub(crate) fn thermal_map(&self) -> Result<ThermalMapResponse, ApiError> {
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
        let made = self
            .thermal_map
            .as_ref()
            .filter(|map| self.serves(map.source()))
            .and_then(|map| map.current())
            .ok_or_else(|| {
                ApiError::busy(
                    "the thermal map is not made yet: the server makes it from each run of \
                     the model, and after a start that takes a few minutes",
                )
                .retry_after(COME_BACK_S)
            })?;
        let mut answer = ThermalMapResponse::clone(&made);
        answer.attributions = self.attributions([&answer.source]);
        Ok(answer)
    }
}
