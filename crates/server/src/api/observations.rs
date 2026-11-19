use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::Duration;
use serde::Deserialize;
use venti_api_types::v1::{
    DEFAULT_WINDOW_HOURS, ErrorCode, MAX_WINDOW_DAYS, ObservationsResponse, Reading, Station,
    format_time, parse_time,
};
use venti_domain::{StationId, Timestamp};
use venti_store::Store;

use super::error::ApiError;
use super::{Api, cache};

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    from: Option<String>,
    to: Option<String>,
}

/// `GET /v1/stations/{id}/observations?from=&to=`: what one station measured
/// in a window of time, oldest first.
pub(crate) async fn observations<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Result<Response, ApiError> {
    let id: StationId = id.parse().map_err(|error| {
        ApiError::bad_request(
            ErrorCode::InvalidStationId,
            format!("{error}; a station identifier looks like dwd-10min:05792"),
        )
    })?;
    let answer = api
        .observations_of(&id, params.from.as_deref(), params.to.as_deref())
        .await?;
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// What one station measured in a window of time, as the API answers
    /// it. `from` and `to` are the request's parameters as text; see
    /// [`window`]. The connector's history tool is answered from this too.
    pub(crate) async fn observations_of(
        &self,
        id: &StationId,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<ObservationsResponse, ApiError> {
        let (from, to) = window(from, to, self.clock.now())?;

        let unknown = || {
            ApiError::missing(
                ErrorCode::StationNotFound,
                format!("there is no station {id}"),
            )
        };
        if !self.serves(id.source()) {
            return Err(unknown());
        }
        let station = self.store.station(id).await?.ok_or_else(unknown)?;
        let observations = self.store.observations(id, from, to).await?;

        Ok(ObservationsResponse {
            station: Station::from(&station),
            observations: observations.iter().map(Reading::from).collect(),
            attributions: self.attributions([id.source()]),
        })
    }
}

/// Works out the window a request asks for.
///
/// `to` defaults to now and `from` to a day before `to`. Both ends are
/// included. The window is bounded so that no request can ask for a
/// station's whole history at once.
fn window(
    from: Option<&str>,
    to: Option<&str>,
    now: Timestamp,
) -> Result<(Timestamp, Timestamp), ApiError> {
    let time = |name: &str, text: Option<&str>| {
        text.map(parse_time).transpose().map_err(|error| {
            ApiError::bad_request(ErrorCode::InvalidTime, format!("{name}: {error}"))
        })
    };
    let to = time("to", to)?.unwrap_or(now);
    let from = time("from", from)?.unwrap_or(to - Duration::hours(DEFAULT_WINDOW_HOURS));

    if from > to {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRange,
            "from lies after to; when to is left out it is the current time",
        ));
    }
    if to - from > Duration::days(MAX_WINDOW_DAYS) {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRange,
            format!(
                "the window from {} to {} is longer than {MAX_WINDOW_DAYS} days; \
                 ask for it in several requests",
                format_time(from),
                format_time(to)
            ),
        ));
    }
    Ok((from, to))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn now() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0).unwrap()
    }

    fn day(day: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, day, 0, 0, 0).unwrap()
    }

    #[test]
    fn the_default_window_is_the_last_day() {
        let (from, to) = window(None, None, now()).unwrap();
        assert_eq!(to, now());
        assert_eq!(from, now() - Duration::hours(24));
    }

    #[test]
    fn a_missing_end_is_filled_in_from_the_other() {
        // Only `to`: the day before it.
        let (from, to) = window(None, Some("2026-11-10T00:00:00Z"), now()).unwrap();
        assert_eq!((from, to), (day(9), day(10)));
        // Only `from`: up to now.
        let (from, to) = window(Some("2026-11-17T00:00:00Z"), None, now()).unwrap();
        assert_eq!((from, to), (day(17), now()));
    }

    #[test]
    fn a_window_of_exactly_the_limit_is_allowed_and_a_longer_one_is_not() {
        let week = window(
            Some("2026-11-03T00:00:00Z"),
            Some("2026-11-10T00:00:00Z"),
            now(),
        );
        assert_eq!(week, Ok((day(3), day(10))));

        let longer = window(
            Some("2026-11-02T23:59:59Z"),
            Some("2026-11-10T00:00:00Z"),
            now(),
        );
        let error = longer.unwrap_err();
        assert_eq!(error, {
            let message = "the window from 2026-11-02T23:59:59Z to 2026-11-10T00:00:00Z \
                           is longer than 7 days; ask for it in several requests";
            ApiError::bad_request(ErrorCode::InvalidRange, message)
        });
        // An old `from` alone reaches to now, which is too far.
        assert!(window(Some("2026-10-01T00:00:00Z"), None, now()).is_err());
    }

    #[test]
    fn a_backwards_window_and_unreadable_times_are_refused() {
        let backwards = window(
            Some("2026-11-10T00:00:00Z"),
            Some("2026-11-09T00:00:00Z"),
            now(),
        );
        assert!(backwards.is_err());
        // A start in the future, with the end left at now.
        assert!(window(Some("2026-11-19T00:00:00Z"), None, now()).is_err());
        assert!(window(Some("yesterday"), None, now()).is_err());
        assert!(window(None, Some("2026-11-18"), now()).is_err());
        // A point in time is a window.
        let point = "2026-11-18T18:00:00Z";
        assert!(window(Some(point), Some(point), now()).is_ok());
    }
}
