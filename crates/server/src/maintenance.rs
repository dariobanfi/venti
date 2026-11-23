//! Housekeeping: removes what is older than the configured retention.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use venti_domain::Timestamp;
use venti_store::{Month, ReportListing, Store};

use crate::clock::Clock;
use crate::config::Retention;
use crate::groups;
use crate::ingest::DirArchive;

/// How often old data is looked for. Observations leave by the month and
/// payloads by the day, so once a day is as often as anything can change.
const INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// How often live tracks that ended long enough ago are looked for. A live
/// track is where a person was. The API stops showing one the moment its
/// time is up; this is how soon after that it is gone from the database
/// too, and a day would be half as long again as a track is kept. The
/// plans of groups are looked at as often, for the same reason: a plan is
/// where people meant to be.
const LIVE_INTERVAL: Duration = Duration::from_secs(3600);

/// Cleans up when the server starts and once a day after that, and removes
/// ended live tracks and the plans of days gone by once an hour, until
/// `shutdown` is cancelled. A session that was not used for `session_idle`
/// has stopped working already; this removes it. Once a day it also removes
/// the reports that were closed long enough ago, and says how many are
/// open.
pub async fn run<S: Store>(
    store: Arc<S>,
    archive: Option<DirArchive>,
    retention: Retention,
    session_idle: chrono::Duration,
    clock: Arc<dyn Clock>,
    shutdown: CancellationToken,
) {
    let rounds_a_day = INTERVAL.as_secs() / LIVE_INTERVAL.as_secs();
    let mut round = 0_u64;
    loop {
        let now = clock.now();
        if round.is_multiple_of(rounds_a_day) {
            clean(&*store, archive.as_ref(), retention, now).await;
            match store.prune_sessions(now - session_idle).await {
                Ok(0) => {}
                Ok(sessions) => tracing::info!("removed {sessions} sessions that had expired"),
                Err(error) => tracing::warn!("expired sessions could not be removed: {error}"),
            }
            look_at_reports(&*store, retention, now).await;
            clean_up_after_the_authorization_server(&*store, now).await;
            remove_old_fired_alerts(&*store, now).await;
            remove_old_notices(&*store, now).await;
        }
        remove_live_tracks(&*store, retention, now).await;
        remove_old_plans(&*store, retention, now).await;
        remove_old_positions_and_calls_for_help(&*store, now).await;
        round += 1;
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(LIVE_INTERVAL) => {}
        }
    }
}

/// Removes what the authorization server no longer needs: codes and
/// refresh tokens whose end has come, agreements that ended because their
/// program stopped using them, and registered programs that hold no
/// agreement and showed no sign of life for
/// [`CLIENT_IDLE_LIFE`](crate::accounts::oauth::CLIENT_IDLE_LIFE).
async fn clean_up_after_the_authorization_server<S: Store>(store: &S, now: Timestamp) {
    let unused_since = now - crate::accounts::oauth::CLIENT_IDLE_LIFE;
    match store.prune_oauth(now, unused_since).await {
        Ok(0) => {}
        Ok(removed) => tracing::info!(
            "removed {removed} ended agreements with programs and programs that registered \
             and were not used"
        ),
        Err(error) => tracing::warn!(
            "what the authorization server no longer needs could not be removed: {error}"
        ),
    }
}

/// Removes the positions pilots' phones told longer ago than
/// [`POSITION_KEPT_MINUTES`](venti_api_types::v1::POSITION_KEPT_MINUTES),
/// which no request for help reads any more, and the requests for help
/// that ended longer ago than
/// [`HELP_KEPT_HOURS`](crate::api::HELP_KEPT_HOURS). Once an hour, as
/// live tracks: a position is where a person was.
///
/// The log line says how many, and nothing about whose or where.
async fn remove_old_positions_and_calls_for_help<S: Store>(store: &S, now: Timestamp) {
    let positions_before =
        now - chrono::Duration::minutes(venti_api_types::v1::POSITION_KEPT_MINUTES);
    let requests_before = now - chrono::Duration::hours(crate::api::HELP_KEPT_HOURS);
    match store.prune_nearby(positions_before, requests_before).await {
        Ok(0) => {}
        Ok(removed) => tracing::info!(
            "removed {removed} positions of pilots who may be asked for help and ended              requests for help"
        ),
        Err(error) => {
            tracing::warn!("old positions and requests for help could not be removed: {error}")
        }
    }
}

/// Removes the live tracks whose newest fix is older than the retention's
/// hours, counted back from `now`, with their fixes.
///
/// The log line says how many, and nothing about which or whose.
async fn remove_live_tracks<S: Store>(store: &S, retention: Retention, now: Timestamp) {
    let ended_before = now - chrono::Duration::hours(i64::from(retention.live_hours));
    match store.prune_live_tracks(ended_before, now).await {
        Ok(0) => {}
        Ok(tracks) => tracing::info!(
            "removed {tracks} live tracks that had ended more than {} hours ago",
            retention.live_hours
        ),
        Err(error) => tracing::warn!("ended live tracks could not be removed: {error}"),
    }
}

/// Removes what the pilots' alarms fired longer ago than
/// [`FIRED_KEPT_DAYS`](venti_api_types::v1::FIRED_KEPT_DAYS) days.
///
/// The log line says how many, and nothing about whose.
async fn remove_old_fired_alerts<S: Store>(store: &S, now: Timestamp) {
    let days = venti_api_types::v1::FIRED_KEPT_DAYS;
    match store
        .prune_fired_alerts(now - chrono::Duration::days(days))
        .await
    {
        Ok(0) => {}
        Ok(fired) => {
            tracing::info!("removed {fired} fired alarms older than {days} days");
        }
        Err(error) => tracing::warn!("old fired alarms could not be removed: {error}"),
    }
}

/// Removes what pilots were told of follows, cheers and comments longer
/// ago than
/// [`NOTIFICATIONS_KEPT_DAYS`](venti_api_types::v1::NOTIFICATIONS_KEPT_DAYS)
/// days.
///
/// The log line says how many, and nothing about whose.
async fn remove_old_notices<S: Store>(store: &S, now: Timestamp) {
    let days = venti_api_types::v1::NOTIFICATIONS_KEPT_DAYS;
    match store
        .prune_notices(now - chrono::Duration::days(days))
        .await
    {
        Ok(0) => {}
        Ok(told) => {
            tracing::info!("removed {told} notices to pilots older than {days} days");
        }
        Err(error) => tracing::warn!("old notices to pilots could not be removed: {error}"),
    }
}

/// Removes the plans that the members of groups made for days that lie
/// further back than the retention's days, by the date `now` has in UTC,
/// with who was in on them. The API has stopped showing them already.
///
/// The log line says how many, and nothing about which, whose or where.
async fn remove_old_plans<S: Store>(store: &S, retention: Retention, now: Timestamp) {
    let gone_before = groups::gone_before(now, retention.plan_days);
    match store.prune_plans(gone_before).await {
        Ok(0) => {}
        Ok(plans) => tracing::info!(
            "removed {plans} plans of groups for days more than {} days gone by",
            retention.plan_days
        ),
        Err(error) => tracing::warn!("the plans of days gone by could not be removed: {error}"),
    }
}

/// The most open reports the daily line counts. Beyond it the line says
/// "more than".
const REPORTS_COUNTED: usize = 100;

/// What the log says once a day while reports wait, so that whoever runs
/// the server hears of them without looking: how many, and nothing about
/// which, whose or why. `None` while none waits.
fn waiting_reports(open: usize) -> Option<String> {
    let how_many = match open {
        0 => return None,
        1 => "1 report is".to_owned(),
        open if open > REPORTS_COUNTED => format!("more than {REPORTS_COUNTED} reports are"),
        open => format!("{open} reports are"),
    };
    Some(format!(
        "{how_many} open: `venti-server reports` lists them"
    ))
}

/// Removes the reports that were closed longer ago than the retention's
/// days, counted back from `now`, and says in the log how many reports are
/// open. A report that is open is never removed.
async fn look_at_reports<S: Store>(store: &S, retention: Retention, now: Timestamp) {
    let closed_before = now - chrono::Duration::days(i64::from(retention.report_days));
    match store.prune_reports(closed_before).await {
        Ok(0) => {}
        Ok(reports) => tracing::info!(
            "removed {reports} reports that were closed more than {} days ago",
            retention.report_days
        ),
        Err(error) => tracing::warn!("old reports could not be removed: {error}"),
    }
    // One more than is counted, to know whether there are more.
    let open = store
        .reports(ReportListing::Open, None, REPORTS_COUNTED + 1)
        .await;
    match open {
        Ok(open) => {
            if let Some(line) = waiting_reports(open.len()) {
                tracing::info!("{line}");
            }
        }
        Err(error) => tracing::warn!("the open reports could not be counted: {error}"),
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
            airspaces: 0,
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
            live_hours: 48,
            report_days: 90,
            plan_days: 2,
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

    #[tokio::test]
    async fn live_tracks_are_removed_when_their_hours_are_up() {
        use venti_store::{
            Account, LiveBatch, LiveFix, LiveRoom, LiveSaved, LiveVisibility, NewAccount,
        };

        let now = noon(2026, 11, 18);
        let store = MemoryStore::new();
        let account = Account {
            id: "00000000000000000000000001".to_owned(),
            handle: "dario".to_owned(),
            display_name: "Dario B.".to_owned(),
            created_at: now,
            publishing_suspended_at: None,
            edits_sites_since: None,
            new_flights_public: false,
            country: None,
            glider_class: None,
            picture: None,
            shown_near_places: true,
        };
        let new = NewAccount {
            account: account.clone(),
            password_hash: "$argon2id$hash".to_owned(),
            invite: None,
        };
        store.create_account(&new).await.unwrap();
        // Tracks whose newest fix is 47 hours, exactly 48 hours and 49
        // hours old.
        let room = LiveRoom {
            fixes_per_track: 10,
            may_create: true,
            expired_before: noon(2026, 1, 1),
            now,
        };
        let track = |hours: i64| LiveBatch {
            id: format!("{hours:0>26}"),
            account_id: account.id.clone(),
            visibility: LiveVisibility::Public,
            landed: true,
            fixes: vec![LiveFix {
                time: now - chrono::Duration::hours(hours),
                position: GeoPoint::new(47.66, 11.8).unwrap(),
                altitude_m: 800,
                ground_speed_ms: None,
                climb_ms: None,
                course_deg: None,
            }],
        };
        for hours in [47, 48, 49] {
            let saved = store.save_live_fixes(&track(hours), &room).await.unwrap();
            assert!(matches!(saved, LiveSaved::Kept(_)));
        }
        let retention = Retention {
            observation_months: 13,
            archive_days: 14,
            live_hours: 48,
            report_days: 90,
            plan_days: 2,
        };

        remove_live_tracks(&store, retention, now).await;

        let left = |hours: i64| {
            let store = &store;
            async move {
                let id = format!("{hours:0>26}");
                store.live_track(&id).await.unwrap().is_some()
            }
        };
        assert!(left(47).await);
        assert!(left(48).await, "48 hours are kept in full");
        assert!(!left(49).await);
        // An hour later the next one has had its time.
        remove_live_tracks(&store, retention, now + chrono::Duration::hours(1)).await;
        assert!(left(47).await);
        assert!(!left(48).await);
        // What was removed takes nothing more.
        let saved = store.save_live_fixes(&track(49), &room).await.unwrap();
        assert_eq!(saved, LiveSaved::Gone);
    }

    #[tokio::test]
    async fn closed_reports_are_removed_when_their_days_are_up_and_open_ones_never() {
        use venti_store::{
            Account, NewAccount, NewReport, ReportListing, ReportSaved, ReportSubject,
        };

        let now = noon(2026, 11, 18);
        let store = MemoryStore::new();
        let account = |handle: &str| Account {
            id: format!("{handle:0>26}"),
            handle: handle.to_owned(),
            display_name: handle.to_owned(),
            created_at: noon(2026, 1, 1),
            publishing_suspended_at: None,
            edits_sites_since: None,
            new_flights_public: false,
            country: None,
            glider_class: None,
            picture: None,
            shown_near_places: true,
        };
        for handle in ["dario", "anna"] {
            let new = NewAccount {
                account: account(handle),
                password_hash: "$argon2id$hash".to_owned(),
                invite: None,
            };
            store.create_account(&new).await.unwrap();
        }
        // Three reports made half a year ago. One was closed 91 days ago,
        // one exactly 90 days ago, and one is still open.
        let days = chrono::Duration::days;
        for (name, closed_days_ago) in [("r1", Some(91)), ("r2", Some(90)), ("r3", None)] {
            let new = NewReport {
                id: format!("{name:0>26}"),
                created_at: now - days(180),
                reporter_id: account("dario").id,
                about_account_id: Some(account("anna").id),
                subject: ReportSubject::Flight(format!("{name:0>26}")),
                reason: "spam".to_owned(),
                text: None,
            };
            assert_eq!(store.save_report(&new).await.unwrap(), ReportSaved::Stored);
            if let Some(ago) = closed_days_ago {
                let closed_at = now - days(ago);
                store
                    .close_report(&new.id, "done", closed_at)
                    .await
                    .unwrap();
            }
        }
        let retention = Retention {
            observation_months: 13,
            archive_days: 14,
            live_hours: 48,
            report_days: 90,
            plan_days: 2,
        };

        look_at_reports(&store, retention, now).await;

        async fn left(store: &MemoryStore) -> Vec<String> {
            let reports = store.reports(ReportListing::All, None, 10).await.unwrap();
            reports
                .iter()
                .map(|report| report.id.trim_start_matches('0').to_owned())
                .collect()
        }
        assert_eq!(left(&store).await, ["r2", "r3"], "90 days are kept in full");
        // A day later the next one has had its time; the open one stays,
        // however old it is.
        look_at_reports(&store, retention, now + days(1)).await;
        assert_eq!(left(&store).await, ["r3"]);
        look_at_reports(&store, retention, now + days(3650)).await;
        assert_eq!(left(&store).await, ["r3"]);
    }

    #[tokio::test]
    async fn the_plans_of_days_gone_by_are_removed_when_their_days_are_up() {
        use chrono::NaiveDate;
        use venti_store::{
            Account, GroupCreation, GroupRoom, NewAccount, NewGroup, NewPlan, PlanDetails,
            PlanRoom, PlanSaving, SecretHash,
        };

        // The evening of the 18th, as the development mode's clock shows.
        let now = Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0).unwrap();
        let store = MemoryStore::new();
        let account = Account {
            id: "00000000000000000000000001".to_owned(),
            handle: "dario".to_owned(),
            display_name: "Dario B.".to_owned(),
            created_at: now,
            publishing_suspended_at: None,
            edits_sites_since: None,
            new_flights_public: false,
            country: None,
            glider_class: None,
            picture: None,
            shown_near_places: true,
        };
        let new = NewAccount {
            account: account.clone(),
            password_hash: "$argon2id$hash".to_owned(),
            invite: None,
        };
        store.create_account(&new).await.unwrap();
        let group = NewGroup {
            id: "0000000000000000000000000g".to_owned(),
            name: "Mustergruppe".to_owned(),
            created_at: now,
            keeper_id: account.id.clone(),
            invite: SecretHash([1; 32]),
        };
        let room = GroupRoom {
            groups_per_account: 20,
            members_per_group: 50,
        };
        let made = store.create_group(&group, &room).await.unwrap();
        assert!(matches!(made, GroupCreation::Created(_)));
        // Plans for the 15th to the 19th.
        let day = |of_month: u32| NaiveDate::from_ymd_opt(2026, 11, of_month).unwrap();
        let room = PlanRoom {
            plans_per_group: 100,
            gone_before: NaiveDate::MIN,
        };
        for of_month in 15..=19 {
            let plan = NewPlan {
                id: format!("{of_month:0>26}"),
                group_id: group.id.clone(),
                author_id: account.id.clone(),
                created_at: now,
                details: PlanDetails {
                    day: day(of_month),
                    place: "Musterberg".to_owned(),
                    position: None,
                    meet_at: None,
                    note: None,
                },
            };
            let stored = store.save_plan(&plan, &room).await.unwrap();
            assert!(matches!(stored, PlanSaving::Stored(_)));
        }
        let retention = Retention {
            observation_months: 13,
            archive_days: 14,
            live_hours: 48,
            report_days: 90,
            plan_days: 2,
        };
        async fn left(store: &MemoryStore) -> Vec<u32> {
            let plans = store
                .plans_for("00000000000000000000000001", NaiveDate::MIN, None, 100)
                .await
                .unwrap();
            plans
                .iter()
                .map(|plan| chrono::Datelike::day(&plan.details.day))
                .collect()
        }

        remove_old_plans(&store, retention, now).await;

        // Two days after its day a plan is kept in full.
        assert_eq!(left(&store).await, [16, 17, 18, 19]);
        // Nothing changes until the server's date does.
        let before_midnight = Utc.with_ymd_and_hms(2026, 11, 18, 23, 59, 59).unwrap();
        remove_old_plans(&store, retention, before_midnight).await;
        assert_eq!(left(&store).await, [16, 17, 18, 19]);
        remove_old_plans(
            &store,
            retention,
            before_midnight + chrono::Duration::seconds(1),
        )
        .await;
        assert_eq!(left(&store).await, [17, 18, 19]);
        // A server that keeps them for a day removes more.
        let shorter = Retention {
            plan_days: 1,
            ..retention
        };
        remove_old_plans(&store, shorter, now).await;
        assert_eq!(left(&store).await, [17, 18, 19]);
        remove_old_plans(&store, shorter, now + chrono::Duration::days(1)).await;
        assert_eq!(left(&store).await, [18, 19]);
    }

    #[test]
    fn the_daily_line_says_how_many_reports_wait_and_nothing_else() {
        assert_eq!(waiting_reports(0), None);
        assert_eq!(
            waiting_reports(1).unwrap(),
            "1 report is open: `venti-server reports` lists them"
        );
        assert_eq!(
            waiting_reports(17).unwrap(),
            "17 reports are open: `venti-server reports` lists them"
        );
        assert_eq!(
            waiting_reports(REPORTS_COUNTED).unwrap(),
            "100 reports are open: `venti-server reports` lists them"
        );
        assert_eq!(
            waiting_reports(REPORTS_COUNTED + 1).unwrap(),
            "more than 100 reports are open: `venti-server reports` lists them"
        );
    }
}
