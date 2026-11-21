//! The behaviour every [`Store`] must have.
//!
//! `tests/memory.rs` and `tests/postgres.rs` both run every function here,
//! each against its own implementation. A new behaviour is added once, as a
//! function and a name in [`store_suite`], and both stores must pass it.
//! Stations and observations are below; airspace, flying sites, the sites
//! that pilots make, forecasts, accounts, flights, live tracks, groups with
//! their plans, and what whoever runs the server does about what is shared
//! have files of their own.

mod accounts;
mod airspace;
mod alerts;
mod flights;
mod forecasts;
mod groups;
mod kept_sites;
mod live;
mod moderation;
mod pilot_sites;
mod push;
mod sites;
mod takeoffs;

pub(crate) use accounts::*;
pub(crate) use airspace::*;
pub(crate) use alerts::*;
pub(crate) use flights::*;
pub(crate) use forecasts::*;
pub(crate) use groups::*;
pub(crate) use kept_sites::*;
pub(crate) use live::*;
pub(crate) use moderation::*;
pub(crate) use pilot_sites::*;
pub(crate) use push::*;
pub(crate) use sites::*;
pub(crate) use takeoffs::*;

use chrono::{Duration, TimeZone, Utc};
use venti_domain::{BoundingBox, GeoPoint, Observation, SourceId, Station, StationId, Timestamp};
use venti_store::{Month, RunRecord, SourceStatus, Store, StoreError};

/// Declares one test per behaviour. `$make` is an async function that takes
/// the test's name and returns a fresh, empty store, or `None` to skip.
macro_rules! store_suite {
    ($make:path) => {
        $crate::suite::store_suite!(@tests $make;
            a_saved_run_can_be_read_back,
            values_come_back_exactly_as_stored,
            stations_are_found_by_box_edges_included,
            stations_are_sorted_by_source_then_identifier,
            the_limit_keeps_the_first_stations,
            latest_is_the_newest_observation,
            saving_the_same_run_twice_changes_nothing,
            a_later_run_updates_without_erasing,
            a_station_is_updated_by_a_later_run,
            observations_come_oldest_first_within_the_window,
            observations_span_a_month_boundary,
            a_bad_batch_is_refused_and_nothing_is_stored,
            source_status_follows_the_run_history,
            pruning_removes_whole_months_and_what_belongs_to_them,
            an_airspace_set_is_stored_and_read_back_exactly,
            airspace_is_found_by_its_box_edges_included,
            a_healthy_run_replaces_the_whole_set,
            an_unhealthy_run_leaves_the_set_alone,
            an_older_run_does_not_replace_a_newer_set,
            saving_the_same_airspace_run_twice_changes_nothing,
            a_bad_airspace_set_is_refused_and_nothing_is_stored,
            airspace_layers_are_kept_apart_and_sorted,
            a_set_says_where_it_is_whole_as_its_run_said,
            pruning_leaves_airspace_alone,
            a_site_set_is_stored_and_read_back_exactly,
            sites_are_found_by_box_edges_included,
            a_healthy_site_run_replaces_the_whole_set,
            an_unhealthy_site_run_leaves_the_set_alone,
            an_older_site_run_does_not_replace_a_newer_set,
            a_bad_site_set_is_refused_and_nothing_is_stored,
            site_layers_are_kept_apart_and_sorted,
            pruning_leaves_sites_alone,
            a_site_a_pilot_made_is_read_back_with_its_author_and_times,
            a_change_to_a_site_is_a_new_revision_and_the_old_ones_stay,
            a_removed_site_keeps_its_address_and_can_be_restored,
            a_site_taken_down_comes_back_only_by_the_operator,
            standing_sites_keep_their_distance_from_each_other,
            adds_racing_for_one_place_put_one_site_there,
            the_history_of_a_site_is_read_page_by_page,
            the_revisions_of_several_sites_are_read_at_once,
            what_changed_among_the_sites_is_listed_once_per_site,
            the_identifier_of_a_site_is_never_used_again,
            an_account_is_trusted_with_the_sites_until_it_is_not,
            the_two_layers_of_sites_are_kept_and_read_apart,
            a_site_that_stays_where_it_is_takes_no_place,
            a_pilot_confirms_a_site_once_and_takes_it_back,
            what_an_account_gave_is_listed_for_it_and_for_nobody_else,
            a_revision_through_a_token_keeps_the_tokens_name,
            a_deleted_account_leaves_its_sites_without_an_author,
            sites_of_pilots_are_found_by_box_apart_from_a_sources,
            what_a_look_at_the_forecast_models_found_is_read_back,
            a_healthy_look_replaces_what_is_known_about_the_models,
            a_failed_or_older_look_changes_nothing_about_the_models,
            a_bad_look_at_the_forecast_models_is_refused,
            pruning_leaves_the_forecast_models_alone,
            a_part_of_the_thermal_map_is_read_back_by_source_and_model,
            a_later_part_of_the_thermal_map_replaces_an_earlier_one,
            an_account_is_created_and_found_by_its_handle,
            a_handle_belongs_to_one_account,
            an_invite_creates_exactly_one_account,
            requests_racing_for_one_invite_create_one_account,
            a_session_is_found_by_its_token_until_it_expires,
            using_a_session_is_recorded_every_few_minutes,
            sessions_are_listed_and_removed_per_account,
            no_session_has_an_identifier_that_cannot_be_one,
            a_token_for_a_program_is_good_until_its_end,
            a_kept_site_is_read_back_and_the_newer_note_wins,
            a_forgotten_site_is_told_to_the_other_devices,
            an_account_keeps_so_many_sites_and_no_more,
            kept_sites_are_an_accounts_own_and_go_with_it,
            an_alarm_is_added_changed_and_removed,
            alarms_are_found_by_station_and_by_kind_when_on,
            what_fired_is_numbered_kept_and_pruned,
            a_browser_is_subscribed_bounded_and_removed,
            a_browser_is_of_the_account_that_subscribed_it_last,
            the_first_push_key_kept_is_the_one,
            tokens_for_programs_are_withdrawn_one_by_one,
            an_account_holds_only_so_many_tokens_for_programs,
            an_account_keeps_only_its_most_recent_sessions,
            sessions_unused_for_too_long_are_pruned,
            a_new_password_signs_out_the_other_devices,
            a_google_account_is_found_by_its_subject,
            a_google_account_is_linked_to_one_account_at_a_time,
            a_deleted_account_leaves_nothing_behind,
            a_flight_is_stored_private_and_read_back_exactly,
            storing_the_same_flight_again_changes_nothing,
            two_accounts_may_hold_the_same_flight,
            uploads_racing_for_one_flight_create_it_once,
            an_account_keeps_no_more_than_it_has_room_for,
            uploads_racing_for_the_last_place_fill_it_once,
            a_flight_for_an_unknown_account_is_refused,
            flights_are_listed_newest_first_page_by_page,
            visibility_changes_for_the_owner_only,
            only_the_owner_deletes_a_flight,
            changes_are_reported_in_order_with_deletions,
            change_numbers_follow_the_clock_and_never_go_back,
            a_deleted_flight_can_be_uploaded_again,
            derived_values_can_be_worked_out_again,
            public_flights_are_listed_newest_first_whoever_flew_them,
            what_is_not_public_is_in_no_list_of_public_flights,
            public_flights_are_found_by_where_they_took_off,
            public_flights_are_found_by_when_they_started,
            public_flights_are_listed_best_first_by_their_points,
            pilots_are_ranked_by_the_sum_of_their_best_public_flights,
            a_flight_has_the_moment_it_became_public,
            public_flights_are_found_by_their_pilot,
            what_changed_for_the_public_is_told_with_withdrawals,
            the_public_changes_are_told_page_by_page_and_of_a_pilot,
            suspending_and_deleting_an_account_withdraws_flights,
            the_public_flights_that_began_at_a_place_are_counted,
            sites_near_take_offs_and_flights_near_a_site,
            a_place_several_pilots_took_off_at_is_a_candidate,
            candidates_come_by_pilots_and_a_cell_counts_alone,
            a_live_track_begins_with_its_first_fixes,
            the_same_live_fixes_again_change_nothing,
            live_fixes_that_arrive_late_reach_those_who_wait,
            the_newest_fix_says_who_sees_a_live_track,
            a_live_track_takes_no_more_fixes_than_it_has_room_for,
            only_recent_public_live_tracks_in_the_box_are_listed,
            a_live_track_belongs_to_the_account_that_began_it,
            a_deleted_live_track_takes_nothing_more,
            an_identifier_can_be_retired_before_anything_arrived,
            a_live_track_is_begun_only_where_that_is_allowed,
            live_tracks_that_ended_long_ago_are_removed,
            requests_racing_for_one_live_track_begin_it_once,
            a_deletion_and_a_first_batch_never_both_win,
            a_bad_live_batch_is_refused_and_nothing_is_stored,
            no_live_track_has_an_identifier_that_cannot_be_one,
            a_flight_taken_down_is_private_until_it_is_released,
            a_flight_taken_down_stays_so_when_uploaded_again,
            an_account_whose_publishing_is_suspended_publishes_nothing,
            a_suspension_and_a_publication_never_both_win,
            a_live_track_is_removed_whoever_it_belongs_to,
            a_report_is_kept_once_for_whoever_runs_the_server,
            reports_racing_for_one_subject_are_kept_once,
            a_closed_report_is_kept_for_a_while_and_then_removed,
            a_report_goes_with_either_of_its_accounts,
            a_group_is_made_with_its_maker_as_keeper_and_only_member,
            a_pilot_comes_into_a_group_with_its_invite_code,
            a_group_is_seen_by_its_members_and_by_nobody_else,
            groups_and_accounts_take_no_more_than_they_have_room_for,
            joins_racing_for_the_last_place_fill_it_once,
            a_new_invite_code_ends_the_one_before,
            a_join_and_a_new_code_never_both_win,
            only_the_keeper_renames_hands_over_and_deletes_a_group,
            the_keeper_removes_a_member_and_the_code_ends_with_that,
            a_member_who_goes_takes_what_they_put_into_the_group,
            when_the_keeper_goes_the_longest_standing_member_keeps_it,
            a_group_has_a_keeper_who_is_a_member_whoever_goes_at_once,
            no_group_or_plan_has_an_identifier_that_cannot_be_one,
            a_plan_is_stored_with_its_author_in_and_read_back_exactly,
            plans_are_listed_by_day_and_time_for_the_days_asked_for,
            members_say_that_they_are_in_and_take_it_back,
            only_the_author_changes_a_plan_and_the_keeper_deletes_any,
            a_group_holds_no_more_plans_than_it_has_room_for,
            plans_of_days_gone_by_are_gone_and_then_removed,
            whoever_runs_the_server_reads_any_group_and_its_plans,
            whoever_runs_the_server_renames_a_group_and_removes_a_plan,
            whoever_runs_the_server_removes_a_group_with_its_plans,
            a_report_about_a_group_or_a_plan_keeps_its_words,
            one_open_report_about_a_group_whoever_keeps_it,
            a_report_about_a_site_names_no_second_account,
        );
    };
    (@tests $make:path; $($name:ident),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() {
                let Some(store) = $make(stringify!($name)).await else {
                    return;
                };
                $crate::suite::$name(&store).await;
            }
        )*
    };
}
pub(crate) use store_suite;

// --- Helpers -------------------------------------------------------------

const DWD: &str = "dwd-10min";
const TAWES: &str = "geosphere-tawes";

fn source(name: &str) -> SourceId {
    SourceId::new(name).unwrap()
}

fn id(text: &str) -> StationId {
    text.parse().unwrap()
}

/// A time on 18 November 2026.
fn at(hour: u32, minute: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 18, hour, minute, 0).unwrap()
}

fn station(text: &str, lat: f64, lon: f64) -> Station {
    Station {
        id: id(text),
        name: format!("Station {text}"),
        position: GeoPoint::new(lat, lon).unwrap(),
        elevation_m: Some(700.0),
    }
}

fn wind(station: &str, time: Timestamp, speed: f64) -> Observation {
    Observation {
        wind_speed_ms: Some(speed),
        ..Observation::empty(id(station), time)
    }
}

/// A healthy run of `source` that started at `started_at` and took a minute.
fn run(source_name: &str, started_at: Timestamp) -> RunRecord {
    RunRecord {
        source: source(source_name),
        started_at,
        finished_at: started_at + Duration::minutes(1),
        healthy: true,
        requests: 12,
        failed_requests: 0,
        skipped: 0,
        rejected: 0,
        stations: 0,
        observations: 0,
        newest_observation: None,
        airspaces: 0,
    }
}

fn alps() -> BoundingBox {
    BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
}

fn everything() -> BoundingBox {
    BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap()
}

async fn ids_in(store: &impl Store, area: &BoundingBox) -> Vec<String> {
    store
        .stations_in(area, 1000)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.station.id.to_string())
        .collect()
}

async fn all_day(store: &impl Store, station: &str) -> Vec<Observation> {
    store
        .observations(&id(station), at(0, 0), at(23, 59))
        .await
        .unwrap()
}

// --- Behaviours ----------------------------------------------------------

pub(crate) async fn a_saved_run_can_be_read_back(store: &impl Store) {
    let zugspitze = station("dwd-10min:05792", 47.421, 10.9848);
    let reading = wind("dwd-10min:05792", at(18, 20), 14.9);

    store
        .save_run(
            &run(DWD, at(18, 30)),
            std::slice::from_ref(&zugspitze),
            std::slice::from_ref(&reading),
        )
        .await
        .unwrap();

    assert_eq!(
        store.station(&zugspitze.id).await.unwrap(),
        Some(zugspitze.clone())
    );
    assert_eq!(store.station(&id("dwd-10min:00000")).await.unwrap(), None);
    assert_eq!(
        all_day(store, "dwd-10min:05792").await,
        std::slice::from_ref(&reading)
    );
    let listed = store.stations_in(&alps(), 10).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].station, zugspitze);
    assert_eq!(listed[0].latest, Some(reading));
}

pub(crate) async fn values_come_back_exactly_as_stored(store: &impl Store) {
    // Awkward decimals, a name outside ASCII, a station without a height and
    // an observation with every measurement.
    let hut = Station {
        id: id("geosphere-tawes:11126"),
        name: "Patscherkofel – Gipfelhütte".to_owned(),
        position: GeoPoint::new(47.208_611_111_1, 11.462_222_222_3).unwrap(),
        elevation_m: None,
    };
    let full = Observation {
        station: hut.id.clone(),
        time: Utc.with_ymd_and_hms(2026, 11, 18, 18, 50, 7).unwrap(),
        wind_speed_ms: Some(0.1 + 0.2),
        wind_gust_ms: Some(31.400_000_000_000_002),
        wind_direction_deg: Some(360.0),
        gust_direction_deg: Some(0.0),
        temperature_c: Some(-7.1),
        humidity_pct: Some(100.0),
        pressure_hpa: Some(778.3),
    };

    store
        .save_run(
            &run(TAWES, at(19, 0)),
            std::slice::from_ref(&hut),
            std::slice::from_ref(&full),
        )
        .await
        .unwrap();

    assert_eq!(store.station(&hut.id).await.unwrap(), Some(hut));
    assert_eq!(all_day(store, "geosphere-tawes:11126").await, [full]);
}

pub(crate) async fn stations_are_found_by_box_edges_included(store: &impl Store) {
    let stations = [
        station("dwd-10min:inside", 47.0, 11.0),
        station("dwd-10min:north-edge", 48.5, 11.0),
        station("dwd-10min:south-west-corner", 46.3, 9.4),
        station("dwd-10min:just-north", 48.500_001, 11.0),
        station("dwd-10min:just-west", 47.0, 9.399_999),
        station("dwd-10min:far-away", 52.9, 12.8),
    ];
    store
        .save_run(&run(DWD, at(18, 30)), &stations, &[])
        .await
        .unwrap();

    assert_eq!(
        ids_in(store, &alps()).await,
        [
            "dwd-10min:inside",
            "dwd-10min:north-edge",
            "dwd-10min:south-west-corner"
        ]
    );
    assert_eq!(ids_in(store, &everything()).await.len(), 6);
    // A box that is a single point finds the station standing on it.
    let point = BoundingBox::new(47.0, 11.0, 47.0, 11.0).unwrap();
    assert_eq!(ids_in(store, &point).await, ["dwd-10min:inside"]);
    let empty = BoundingBox::new(10.0, 10.0, 11.0, 11.0).unwrap();
    assert!(ids_in(store, &empty).await.is_empty());
}

pub(crate) async fn stations_are_sorted_by_source_then_identifier(store: &impl Store) {
    // "dwd" sorts before "dwd-10min" as a source, although the text
    // "dwd:…" sorts after "dwd-10min:…". Upper case sorts before lower case.
    store
        .save_run(
            &run("dwd-10min", at(18, 30)),
            &[
                station("dwd-10min:b", 47.0, 11.0),
                station("dwd-10min:B", 47.0, 11.0),
                station("dwd-10min:10", 47.0, 11.0),
                station("dwd-10min:9", 47.0, 11.0),
            ],
            &[],
        )
        .await
        .unwrap();
    store
        .save_run(
            &run("dwd", at(18, 30)),
            &[station("dwd:z", 47.0, 11.0)],
            &[],
        )
        .await
        .unwrap();

    assert_eq!(
        ids_in(store, &alps()).await,
        [
            "dwd:z",
            "dwd-10min:10",
            "dwd-10min:9",
            "dwd-10min:B",
            "dwd-10min:b"
        ]
    );
}

pub(crate) async fn the_limit_keeps_the_first_stations(store: &impl Store) {
    let stations: Vec<Station> = (0..5)
        .map(|n| station(&format!("dwd-10min:{n}"), 47.0, 11.0))
        .collect();
    store
        .save_run(&run(DWD, at(18, 30)), &stations, &[])
        .await
        .unwrap();

    let first: Vec<String> = store
        .stations_in(&alps(), 3)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.station.id.to_string())
        .collect();

    assert_eq!(first, ["dwd-10min:0", "dwd-10min:1", "dwd-10min:2"]);
    assert!(store.stations_in(&alps(), 0).await.unwrap().is_empty());
}

pub(crate) async fn latest_is_the_newest_observation(store: &impl Store) {
    let stations = [
        station("dwd-10min:busy", 47.0, 11.0),
        station("dwd-10min:silent", 47.1, 11.1),
    ];
    // Delivered out of order, and over two runs.
    store
        .save_run(
            &run(DWD, at(18, 30)),
            &stations,
            &[
                wind("dwd-10min:busy", at(18, 20), 3.0),
                wind("dwd-10min:busy", at(18, 0), 1.0),
            ],
        )
        .await
        .unwrap();
    store
        .save_run(
            &run(DWD, at(18, 40)),
            &stations,
            &[wind("dwd-10min:busy", at(18, 10), 2.0)],
        )
        .await
        .unwrap();

    let listed = store.stations_in(&alps(), 10).await.unwrap();

    assert_eq!(listed[0].station.id, id("dwd-10min:busy"));
    assert_eq!(
        listed[0].latest,
        Some(wind("dwd-10min:busy", at(18, 20), 3.0))
    );
    assert_eq!(listed[1].station.id, id("dwd-10min:silent"));
    assert_eq!(listed[1].latest, None);
}

pub(crate) async fn saving_the_same_run_twice_changes_nothing(store: &impl Store) {
    let stations = [station("dwd-10min:05792", 47.421, 10.9848)];
    let observations = [
        wind("dwd-10min:05792", at(18, 10), 12.0),
        wind("dwd-10min:05792", at(18, 20), 14.9),
    ];
    let record = RunRecord {
        stations: 1,
        observations: 2,
        newest_observation: Some(at(18, 20)),
        ..run(DWD, at(18, 30))
    };

    store
        .save_run(&record, &stations, &observations)
        .await
        .unwrap();
    let once = (
        store.stations_in(&alps(), 10).await.unwrap(),
        all_day(store, "dwd-10min:05792").await,
        store.source_status(&[source(DWD)]).await.unwrap(),
    );
    store
        .save_run(&record, &stations, &observations)
        .await
        .unwrap();
    let twice = (
        store.stations_in(&alps(), 10).await.unwrap(),
        all_day(store, "dwd-10min:05792").await,
        store.source_status(&[source(DWD)]).await.unwrap(),
    );

    assert_eq!(once, twice);
    assert_eq!(twice.1.len(), 2);
}

pub(crate) async fn a_later_run_updates_without_erasing(store: &impl Store) {
    let stations = [station("dwd-10min:05792", 47.421, 10.9848)];
    let key = "dwd-10min:05792";

    // The wind file arrives first.
    let first = Observation {
        wind_speed_ms: Some(14.9),
        wind_direction_deg: Some(270.0),
        ..Observation::empty(id(key), at(18, 20))
    };
    store
        .save_run(&run(DWD, at(18, 30)), &stations, &[first])
        .await
        .unwrap();

    // The next run brings the temperature, a corrected wind speed, and no
    // direction at all because that request failed.
    let second = Observation {
        wind_speed_ms: Some(15.2),
        temperature_c: Some(-7.1),
        ..Observation::empty(id(key), at(18, 20))
    };
    store
        .save_run(&run(DWD, at(18, 40)), &stations, &[second])
        .await
        .unwrap();

    let expected = Observation {
        wind_speed_ms: Some(15.2),
        wind_direction_deg: Some(270.0),
        temperature_c: Some(-7.1),
        ..Observation::empty(id(key), at(18, 20))
    };
    assert_eq!(all_day(store, key).await, [expected]);
}

pub(crate) async fn a_station_is_updated_by_a_later_run(store: &impl Store) {
    let before = station("dwd-10min:01550", 47.483, 11.0621);
    store
        .save_run(
            &run(DWD, at(18, 30)),
            std::slice::from_ref(&before),
            &[wind("dwd-10min:01550", at(18, 20), 2.2)],
        )
        .await
        .unwrap();

    // The provider renames the station, moves it and stops giving a height.
    let after = Station {
        name: "Garmisch-Partenkirchen".to_owned(),
        position: GeoPoint::new(47.4831, 11.0622).unwrap(),
        elevation_m: None,
        ..before.clone()
    };
    store
        .save_run(&run(DWD, at(18, 40)), std::slice::from_ref(&after), &[])
        .await
        .unwrap();

    assert_eq!(store.station(&after.id).await.unwrap(), Some(after));
    // Its observations stay.
    assert_eq!(all_day(store, "dwd-10min:01550").await.len(), 1);
}

pub(crate) async fn observations_come_oldest_first_within_the_window(store: &impl Store) {
    let key = "dwd-10min:05792";
    let stations = [
        station(key, 47.421, 10.9848),
        station("dwd-10min:02290", 47.8009, 11.0108),
    ];
    let observations = [
        wind(key, at(18, 30), 4.0),
        wind(key, at(18, 0), 1.0),
        wind(key, at(18, 20), 3.0),
        wind(key, at(18, 10), 2.0),
        wind("dwd-10min:02290", at(18, 10), 9.0),
    ];
    store
        .save_run(&run(DWD, at(18, 40)), &stations, &observations)
        .await
        .unwrap();

    let speeds = |observations: Vec<Observation>| -> Vec<f64> {
        observations
            .iter()
            .map(|o| o.wind_speed_ms.unwrap())
            .collect()
    };
    let window = store
        .observations(&id(key), at(18, 10), at(18, 20))
        .await
        .unwrap();
    assert_eq!(speeds(window), [2.0, 3.0], "both ends are included");
    assert_eq!(speeds(all_day(store, key).await), [1.0, 2.0, 3.0, 4.0]);

    let none = store
        .observations(&id(key), at(19, 0), at(20, 0))
        .await
        .unwrap();
    assert!(none.is_empty());
    let backwards = store
        .observations(&id(key), at(18, 20), at(18, 10))
        .await
        .unwrap();
    assert!(backwards.is_empty());
    assert!(all_day(store, "dwd-10min:unknown").await.is_empty());
}

pub(crate) async fn observations_span_a_month_boundary(store: &impl Store) {
    let key = "dwd-10min:05792";
    let before_midnight = Utc.with_ymd_and_hms(2026, 11, 30, 23, 50, 0).unwrap();
    let midnight = Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap();
    let new_year = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
    let observations = [
        wind(key, before_midnight, 1.0),
        wind(key, midnight, 2.0),
        wind(key, new_year, 3.0),
    ];

    store
        .save_run(
            &run(DWD, new_year),
            &[station(key, 47.421, 10.9848)],
            &observations,
        )
        .await
        .unwrap();

    let stored = store
        .observations(&id(key), before_midnight, new_year)
        .await
        .unwrap();
    assert_eq!(stored, observations);
    let latest = &store.stations_in(&alps(), 1).await.unwrap()[0].latest;
    assert_eq!(latest.as_ref(), Some(&observations[2]));
}

pub(crate) async fn a_bad_batch_is_refused_and_nothing_is_stored(store: &impl Store) {
    let good = station("dwd-10min:05792", 47.421, 10.9848);
    let refused = |result: Result<(), StoreError>| {
        assert!(
            matches!(result, Err(StoreError::Invalid(_))),
            "expected a refusal, got {result:?}"
        );
    };

    // An observation for a station the run did not deliver.
    refused(
        store
            .save_run(
                &run(DWD, at(18, 30)),
                std::slice::from_ref(&good),
                &[wind("dwd-10min:00000", at(18, 20), 1.0)],
            )
            .await,
    );
    // Two observations for one station and time.
    refused(
        store
            .save_run(
                &run(DWD, at(18, 30)),
                std::slice::from_ref(&good),
                &[
                    wind("dwd-10min:05792", at(18, 20), 1.0),
                    wind("dwd-10min:05792", at(18, 20), 2.0),
                ],
            )
            .await,
    );
    // A station listed twice.
    refused(
        store
            .save_run(&run(DWD, at(18, 30)), &[good.clone(), good.clone()], &[])
            .await,
    );
    // A station of another source.
    refused(
        store
            .save_run(&run(TAWES, at(18, 30)), std::slice::from_ref(&good), &[])
            .await,
    );
    // Observations spread over years.
    let scattered: Vec<Observation> = (0..30)
        .map(|n| {
            let time = Utc.with_ymd_and_hms(2020 + n / 12, 1 + (n % 12) as u32, 1, 0, 0, 0);
            wind("dwd-10min:05792", time.unwrap(), 1.0)
        })
        .collect();
    refused(
        store
            .save_run(
                &run(DWD, at(18, 30)),
                std::slice::from_ref(&good),
                &scattered,
            )
            .await,
    );

    assert!(ids_in(store, &everything()).await.is_empty());
    assert_eq!(
        store
            .source_status(&[source(DWD), source(TAWES)])
            .await
            .unwrap(),
        [
            SourceStatus::never_ran(source(DWD)),
            SourceStatus::never_ran(source(TAWES))
        ]
    );
}

pub(crate) async fn source_status_follows_the_run_history(store: &impl Store) {
    let stations = [station("dwd-10min:05792", 47.421, 10.9848)];
    let healthy = |started_at: Timestamp, newest: Timestamp| RunRecord {
        stations: 50,
        observations: 900,
        newest_observation: Some(newest),
        ..run(DWD, started_at)
    };
    let failed = |started_at: Timestamp| RunRecord {
        healthy: false,
        failed_requests: 12,
        ..run(DWD, started_at)
    };

    // 18:00 fine, 18:10 fine, 18:20 failed, 18:30 failed.
    for record in [
        healthy(at(18, 0), at(17, 40)),
        healthy(at(18, 10), at(17, 50)),
        failed(at(18, 20)),
        failed(at(18, 30)),
    ] {
        store.save_run(&record, &stations, &[]).await.unwrap();
    }

    let status = store
        .source_status(&[source(TAWES), source(DWD)])
        .await
        .unwrap();

    // In the order asked for, and a source without runs is not an error.
    assert_eq!(status[0], SourceStatus::never_ran(source(TAWES)));
    assert_eq!(
        status[1],
        SourceStatus {
            source: source(DWD),
            last_started: Some(at(18, 30)),
            last_success: Some(at(18, 11)),
            last_failure: Some(at(18, 31)),
            stations: 50,
            newest_observation: Some(at(17, 50)),
        }
    );
    assert!(store.source_status(&[]).await.unwrap().is_empty());
}

pub(crate) async fn pruning_removes_whole_months_and_what_belongs_to_them(store: &impl Store) {
    let day = |month: u32| Utc.with_ymd_and_hms(2026, month, 15, 12, 0, 0).unwrap();
    let kept = "dwd-10min:kept";
    let gone = "dwd-10min:gone";

    // September: both stations report. October and November: only one does.
    store
        .save_run(
            &run(DWD, day(9)),
            &[station(kept, 47.0, 11.0), station(gone, 47.1, 11.1)],
            &[wind(kept, day(9), 9.0), wind(gone, day(9), 9.0)],
        )
        .await
        .unwrap();
    let last_of_october = Utc.with_ymd_and_hms(2026, 10, 31, 23, 50, 0).unwrap();
    store
        .save_run(
            &run(DWD, day(10)),
            &[station(kept, 47.0, 11.0)],
            &[wind(kept, day(10), 10.0), wind(kept, last_of_october, 10.5)],
        )
        .await
        .unwrap();
    let first_of_november = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    store
        .save_run(
            &run(DWD, day(11)),
            &[station(kept, 47.0, 11.0)],
            &[
                wind(kept, first_of_november, 11.0),
                wind(kept, day(11), 11.5),
            ],
        )
        .await
        .unwrap();

    store
        .prune_before(Month::new(2026, 11).unwrap())
        .await
        .unwrap();

    // The station last delivered in September is gone, the other stays.
    assert_eq!(ids_in(store, &everything()).await, [kept]);
    // Only November's observations are left, from its first instant on.
    let left = store
        .observations(&id(kept), day(1), day(12))
        .await
        .unwrap();
    assert_eq!(
        left,
        [
            wind(kept, first_of_november, 11.0),
            wind(kept, day(11), 11.5)
        ]
    );
    assert!(
        store
            .observations(&id(gone), day(1), day(12))
            .await
            .unwrap()
            .is_empty()
    );
    // So is the run history: only November's run is left.
    let status = &store.source_status(&[source(DWD)]).await.unwrap()[0];
    assert_eq!(status.last_started, Some(day(11)));

    // Pruning again, or pruning nothing, is harmless; new data still goes in.
    store
        .prune_before(Month::new(2026, 11).unwrap())
        .await
        .unwrap();
    store
        .prune_before(Month::new(2020, 1).unwrap())
        .await
        .unwrap();
    store
        .save_run(
            &run(DWD, day(12)),
            &[station(kept, 47.0, 11.0)],
            &[wind(kept, day(10), 10.0), wind(kept, day(12), 12.0)],
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .observations(&id(kept), day(1), day(12))
            .await
            .unwrap()
            .len(),
        4,
        "an old month can be written again after it was pruned"
    );
}
