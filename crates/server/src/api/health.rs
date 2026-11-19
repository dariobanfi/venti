use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::Duration;
use venti_api_types::v1::{HealthResponse, HealthStatus, SourceHealth};
use venti_domain::{SourceId, Timestamp};
use venti_sources::Kind;
use venti_store::{AirspaceSet, SourceStatus, Store};

use super::error::ApiError;
use super::{Api, ServedSource};

/// A source is stale when no run has succeeded within this many of its
/// cadence intervals. One missed run is normal; three in a row are not.
const STALE_AFTER_INTERVALS: i32 = 3;

/// From this cadence on a source counts as slow, and is stale sooner,
/// measured in its own intervals.
const SLOW_CADENCE: Duration = Duration::hours(12);

/// How long a source may go without a healthy run before it is stale.
///
/// A source collected every few minutes: three of its intervals. A blip
/// costs it ten minutes, and three misses in a row are a problem.
///
/// A source collected once a day or less often, which is airspace: two and
/// a quarter of its intervals, 54 hours for a daily one. A failed run of
/// such a source is tried again a whole interval later, so each miss costs
/// a day. One miss is tolerated; the second is not. The quarter covers the
/// random delay the scheduler adds to every wait, a tenth of the cadence
/// each time, so that a source whose second try worked is never called
/// stale for the hours in between.
fn patience(cadence_seconds: u32) -> Duration {
    let cadence = Duration::seconds(i64::from(cadence_seconds));
    if cadence >= SLOW_CADENCE {
        cadence * 9 / 4
    } else {
        cadence * STALE_AFTER_INTERVALS
    }
}

/// `GET /v1/health`: how the server and each of its sources are doing.
///
/// Answers `200` whenever the server can read its database, also when
/// sources are stale: the body says so. Monitoring that only looks at the
/// status code learns whether the server works; the body says whether its
/// data is fresh.
pub(crate) async fn health<S: Store>(State(api): State<Arc<Api<S>>>) -> Result<Response, ApiError> {
    let ids: Vec<SourceId> = api.sources.keys().cloned().collect();
    let statuses = api.store.source_status(&ids).await?;
    let of_airspace: Vec<SourceId> = api
        .sources
        .iter()
        .filter(|(_, served)| served.kinds.contains(&Kind::Airspace))
        .map(|(source, _)| source.clone())
        .collect();
    let sets = api.store.airspace_sets(&of_airspace).await?;
    let now = api.clock.now();

    let sources: Vec<SourceHealth> = api
        .sources
        .values()
        .zip(&statuses)
        .map(|(served, status)| {
            let set = sets.iter().find(|set| set.source == status.source);
            source_health(served, status, set, now)
        })
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

fn source_health(
    served: &ServedSource,
    status: &SourceStatus,
    set: Option<&AirspaceSet>,
    now: Timestamp,
) -> SourceHealth {
    let patience = patience(served.cadence_seconds);
    let of_airspace = served.kinds.contains(&Kind::Airspace);
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
        // What the server holds, which is what it serves: the set of the
        // last healthy run, however long ago that was.
        airspaces: of_airspace.then(|| set.map_or(0, |set| set.airspaces)),
        file_date: set.and_then(|set| set.file_date),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_api_types::v1::Attribution;
    use venti_store::Coverage;

    use super::*;

    fn at(minute: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 19, minute, 0).unwrap()
    }

    fn served(source: &SourceId, kind: Kind, cadence_seconds: u32) -> ServedSource {
        ServedSource {
            provider: "Provider".to_owned(),
            kinds: vec![kind],
            cadence_seconds,
            attribution: Attribution {
                source: source.clone(),
                provider: String::new(),
                licence: String::new(),
                text: String::new(),
                url: String::new(),
                notice: None,
            },
        }
    }

    fn stale(last_success: Option<Timestamp>, now: Timestamp) -> bool {
        let source = SourceId::new("dwd-10min").unwrap();
        let status = SourceStatus {
            last_success,
            ..SourceStatus::never_ran(source.clone())
        };
        source_health(
            &served(&source, Kind::Observations, 600),
            &status,
            None,
            now,
        )
        .stale
    }

    #[test]
    fn a_source_is_stale_after_three_missed_intervals() {
        assert!(!stale(Some(at(0)), at(5)));
        assert!(!stale(Some(at(0)), at(30)));
        assert!(stale(Some(at(0)), at(31)));
        assert!(stale(None, at(0)), "a source that never ran has no data");
    }

    #[test]
    fn a_daily_source_is_stale_after_its_second_missed_run() {
        let source = SourceId::new("openaip").unwrap();
        let daily = served(&source, Kind::Airspace, 86_400);
        let success = at(0);
        let stale_after = |hours: i64| {
            let status = SourceStatus {
                last_success: Some(success),
                ..SourceStatus::never_ran(source.clone())
            };
            source_health(&daily, &status, None, success + Duration::hours(hours)).stale
        };

        // The next run is due a day later, up to 2.4 hours late.
        assert!(!stale_after(27));
        // It failed, and so the one after is due at 52.8 hours at the
        // latest. Until it has had its chance, the source is not stale.
        assert!(!stale_after(53));
        assert!(!stale_after(54));
        // That one failed too.
        assert!(stale_after(55));
        assert!(stale_after(72));

        // Twelve hours is where slow begins; below it the three intervals hold.
        assert_eq!(patience(12 * 3600), Duration::hours(27));
        assert_eq!(patience(6 * 3600), Duration::hours(18));
        assert_eq!(patience(600), Duration::minutes(30));
    }

    #[test]
    fn an_airspace_source_reports_the_set_the_server_holds() {
        let source = SourceId::new("openaip").unwrap();
        let daily = served(&source, Kind::Airspace, 86_400);
        let status = SourceStatus::never_ran(source.clone());

        // Before the first healthy run: an airspace source with nothing.
        let nothing = source_health(&daily, &status, None, at(0));
        assert_eq!(nothing.airspaces, Some(0));
        assert_eq!(nothing.file_date, None);
        assert!(nothing.stale);

        let set = AirspaceSet {
            source: source.clone(),
            fetched_at: at(0),
            airspaces: 1018,
            file_date: Some(at(0) - Duration::hours(17)),
            newest_change: None,
            coverage: Coverage::default(),
        };
        let holding = source_health(&daily, &status, Some(&set), at(5));
        assert_eq!(holding.airspaces, Some(1018));
        assert_eq!(holding.file_date, set.file_date);
        assert_eq!(holding.stations, 0);

        // A source of observations says nothing about airspace.
        let dwd = SourceId::new("dwd-10min").unwrap();
        let wind = served(&dwd, Kind::Observations, 600);
        let health = source_health(&wind, &SourceStatus::never_ran(dwd.clone()), None, at(0));
        assert_eq!(health.airspaces, None);
    }
}
