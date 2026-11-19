use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::Duration;
use venti_api_types::v1::{HealthResponse, HealthStatus, SourceHealth};
use venti_domain::{SourceId, Timestamp};
use venti_store::{SourceStatus, Store};

use super::error::ApiError;
use super::{Api, ServedSource};

/// A source is stale when no run has succeeded within this many of its
/// cadence intervals. One missed run is normal; three in a row are not.
const STALE_AFTER_INTERVALS: i32 = 3;

/// `GET /v1/health`: how the server and each of its sources are doing.
///
/// Answers `200` whenever the server can read its database, also when
/// sources are stale: the body says so. Monitoring that only looks at the
/// status code learns whether the server works; the body says whether its
/// data is fresh.
pub(crate) async fn health<S: Store>(State(api): State<Arc<Api<S>>>) -> Result<Response, ApiError> {
    let ids: Vec<SourceId> = api.sources.keys().cloned().collect();
    let statuses = api.store.source_status(&ids).await?;
    let now = api.clock.now();

    let sources: Vec<SourceHealth> = api
        .sources
        .values()
        .zip(&statuses)
        .map(|(served, status)| source_health(served, status, now))
        .collect();
    let status = if sources.iter().any(|source| source.stale) {
        HealthStatus::Degraded
    } else {
        HealthStatus::Ok
    };
    let body = HealthResponse {
        status,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        time: now,
        sources,
    };

    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn source_health(served: &ServedSource, status: &SourceStatus, now: Timestamp) -> SourceHealth {
    let patience = Duration::seconds(i64::from(served.cadence_seconds)) * STALE_AFTER_INTERVALS;
    SourceHealth {
        source: status.source.clone(),
        provider: served.provider.clone(),
        cadence_seconds: served.cadence_seconds,
        last_success: status.last_success,
        last_failure: status.last_failure,
        newest_observation: status.newest_observation,
        stale: status
            .last_success
            .is_none_or(|success| now - success > patience),
        stations: status.stations,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_api_types::v1::Attribution;

    use super::*;

    fn at(minute: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 19, minute, 0).unwrap()
    }

    fn stale(last_success: Option<Timestamp>, now: Timestamp) -> bool {
        let source = SourceId::new("dwd-10min").unwrap();
        let served = ServedSource {
            provider: "Deutscher Wetterdienst".to_owned(),
            cadence_seconds: 600,
            attribution: Attribution {
                source: source.clone(),
                provider: String::new(),
                licence: String::new(),
                text: String::new(),
                url: String::new(),
            },
        };
        let status = SourceStatus {
            last_success,
            ..SourceStatus::never_ran(source)
        };
        source_health(&served, &status, now).stale
    }

    #[test]
    fn a_source_is_stale_after_three_missed_intervals() {
        assert!(!stale(Some(at(0)), at(5)));
        assert!(!stale(Some(at(0)), at(30)));
        assert!(stale(Some(at(0)), at(31)));
        assert!(stale(None, at(0)), "a source that never ran has no data");
    }
}
