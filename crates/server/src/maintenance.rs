//! Housekeeping: removes what is older than the configured retention.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use venti_domain::Timestamp;
use venti_store::{Month, Store};

use crate::clock::Clock;
use crate::config::Retention;
use crate::ingest::DirArchive;

/// How often old data is looked for. Observations leave by the month and
/// payloads by the day, so once a day is as often as anything can change.
const INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// Cleans up when the server starts and once a day after that, until
/// `shutdown` is cancelled. A session that was not used for `session_idle`
/// has stopped working already; this removes it.
pub async fn run<S: Store>(
    store: Arc<S>,
    archive: Option<DirArchive>,
    retention: Retention,
    session_idle: chrono::Duration,
    clock: Arc<dyn Clock>,
    shutdown: CancellationToken,
) {
    loop {
        let now = clock.now();
        clean(&*store, archive.as_ref(), retention, now).await;
        match store.prune_sessions(now - session_idle).await {
            Ok(0) => {}
            Ok(sessions) => tracing::info!("removed {sessions} sessions that had expired"),
            Err(error) => tracing::warn!("expired sessions could not be removed: {error}"),
        }
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// Removes observations older than the retention's months and archived
/// payloads older than its days, counted back from `now`.
///
/// A failure is logged and tried again the next day: nothing breaks when old
/// data stays a little longer.
async fn clean<S: Store>(
    store: &S,
    archive: Option<&DirArchive>,
    retention: Retention,
    now: Timestamp,
) {
    let first_kept = Month::of(now).back(retention.observation_months);
    match store.prune_before(first_kept).await {
        Ok(()) => tracing::debug!("observations from before {first_kept} are gone"),
        Err(error) => tracing::warn!("old observations could not be removed: {error}"),
    }

    let Some(archive) = archive.cloned() else {
        return;
    };
    let first_kept = (now - chrono::Duration::days(i64::from(retention.archive_days))).date_naive();
    // Deleting folders blocks; keep it off the threads that serve requests.
    let pruned = tokio::task::spawn_blocking(move || archive.prune_before(first_kept)).await;
    match pruned {
        Ok(Ok(0)) => {}
        Ok(Ok(days)) => tracing::info!("removed {days} day folders of archived payloads"),
        Ok(Err(error)) => tracing::warn!("old payloads could not be removed: {error}"),
        Err(error) => tracing::warn!("removing old payloads failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::{GeoPoint, Observation, SourceId, Station};
    use venti_sources::{Archive, ArchiveEntry, Request};
    use venti_store::{MemoryStore, RunRecord};

    use super::*;

    fn noon(year: i32, month: u32, day: u32) -> Timestamp {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn old_observations_and_old_payloads_are_removed() {
        let source = SourceId::new("dwd-10min").unwrap();
        let station = Station {
            id: "dwd-10min:05792".parse().unwrap(),
            name: "Zugspitze".to_owned(),
            position: GeoPoint::new(47.421, 10.9848).unwrap(),
            elevation_m: Some(2956.0),
        };
        let now = noon(2026, 11, 18);

        // One observation in each of the last four months.
        let store = MemoryStore::new();
        let observations: Vec<Observation> = (8..=11)
            .map(|month| Observation {
                wind_speed_ms: Some(3.0),
                ..Observation::empty(station.id.clone(), noon(2026, month, 1))
            })
            .collect();
        let run = RunRecord {
            source: source.clone(),
            started_at: now,
            finished_at: now,
            healthy: true,
            requests: 1,
            failed_requests: 0,
            skipped: 0,
            rejected: 0,
            stations: 1,
            observations: 4,
            newest_observation: None,
        };
        store
            .save_run(&run, std::slice::from_ref(&station), &observations)
            .await
            .unwrap();

        // One payload on each of the last four days.
        let root = std::env::temp_dir().join(format!("venti-maintenance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let archive = DirArchive::new(root.clone());
        let request = Request::new("values", "https://example.org/values");
        for day in 15..=18 {
            let entry = ArchiveEntry {
                source: &source,
                request: &request,
                fetched_at: noon(2026, 11, day),
                body: b"payload",
            };
            archive.store(entry).unwrap();
        }

        // Keep two whole months before this one, and two days of payloads.
        let retention = Retention {
            observation_months: 2,
            archive_days: 2,
        };
        clean(&store, Some(&archive), retention, now).await;

        let left = store
            .observations(&station.id, noon(2026, 1, 1), now)
            .await
            .unwrap();
        let months: Vec<u32> = left
            .iter()
            .map(|o| chrono::Datelike::month(&o.time))
            .collect();
        assert_eq!(months, [9, 10, 11]);

        let mut days: Vec<String> = std::fs::read_dir(root.join("dwd-10min"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        days.sort();
        assert_eq!(days, ["2026-11-16", "2026-11-17", "2026-11-18"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
