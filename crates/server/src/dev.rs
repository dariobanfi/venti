//! The `dev` subcommand: the same API over recorded data and a demo account.
//!
//! No network and no database. The real adapters run once over the payloads
//! recorded in their crates (wind stations, a sample of the first region's
//! airspace, and the flying sites OpenStreetMap has for the first region),
//! the result goes into an in-memory store, and the real router serves it.
//! Forecasts are answered from recorded answers as well: there are five
//! places with one, and a place anywhere in the region gets the forecast of
//! the nearest of them, which the answer names. One account is already there, with three
//! flights made from the sample flight; its handle, password and a session
//! token are fixed and written down in `docs/api.md`. It also has three
//! live tracks, made from the same flight and dated by this mode's clock:
//! two that everyone may see, of a glider in the air and of one that has
//! landed, and one for those with its link. Two further pilots with made-up
//! names have three flights each, so that the list of everyone's public
//! flights holds the flights of several. Client work and UI tests run
//! against this.
//!
//! The clock stands still at the moment of the recording, so the data never
//! ages: `/v1/health` reports fresh sources, and "the last 24 hours" is
//! always the recorded day. Every start serves exactly the same answers.
//! What a client changes (an account it creates, a flight it uploads) lasts
//! until the program stops.

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{Duration, NaiveDate};
use tokio_util::sync::CancellationToken;
use venti_api_types::v1::Password;
use venti_domain::Timestamp;
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::synthetic_flight;
use venti_regions::RegionPack;
use venti_store::{
    Account, AccountCreation, LiveBatch, LiveFix, LiveRoom, LiveSaved, LiveVisibility, MemoryStore,
    NewAccount, Room, SecretHash, Session, Store, Visibility,
};

use crate::accounts::client::TrustedProxies;
use crate::accounts::password::{Cost, Hasher};
use crate::accounts::secret::Token;
use crate::accounts::{Limits, Registration};
use crate::adapters::{self, Providers};
use crate::api::{Api, Cors, Settings};
use crate::clock::{Clock, FixedClock};
use crate::config::{DEFAULT_ACCOUNT_FLIGHTS, DEFAULT_ACCOUNT_STORAGE_MB};
use crate::flights::analyser::Analyser;
use crate::flights::derive::derive;
use crate::flights::new_flight;
use crate::flights::share::PublicUrl;
use crate::forecast::{self, Asking};
use crate::ingest::{Collector, DirArchive};
use crate::{plan, serve};

/// The region the payloads were recorded for, compiled in so that the
/// development mode needs no files.
const FIRST_REGION: &str = include_str!("../../../regions/alps-de-at/region.toml");

/// The handle of the account the development mode starts with.
pub const DEMO_HANDLE: &str = "demo";

/// Its display name.
pub const DEMO_NAME: &str = "Demo Pilot";

/// Its password. Not a secret: the account exists only in this mode.
pub const DEMO_PASSWORD: &str = "demo-pilot-password";

/// The token of a session the account starts with, so that a client or a
/// test can ask for the account's data without signing in first.
pub const DEMO_TOKEN: &str = "venti_0000000000000000000000000000000000000000000000000000";

/// The account's identifier, fixed so that the addresses of its flights
/// are the same on every start.
const DEMO_ACCOUNT_ID: &str = "00000000000000000000000001";

const DEMO_SESSION_ID: &str = "00000000000000000000000002";

/// The settings of the development mode.
///
/// Anyone may create an account, and no limit on attempts ever bites: the
/// clock stands still, so a window that opened would never close.
pub fn settings(public_url: PublicUrl, analyser: Analyser, password_cost: Cost) -> Settings {
    Settings {
        registration: Registration::Open,
        public_url,
        trusted_proxies: TrustedProxies::default(),
        limits: Limits::none(),
        // As much as a real server gives: an app's handling of a full
        // account is tested against the server's own tests, not here.
        room: Room {
            flights: DEFAULT_ACCOUNT_FLIGHTS.into(),
            file_bytes: u64::from(DEFAULT_ACCOUNT_STORAGE_MB) * 1024 * 1024,
        },
        password_cost,
        analyser,
        live: crate::live::Limits::none(),
    }
}

/// The identifiers of the live tracks the demo account starts with. They
/// are fixed, so that a client or a test can ask for a track without
/// sending one first. Not secrets: the tracks exist only in this mode.
///
/// This one everyone may see, and its glider is in the air: the newest
/// position is four seconds old by this mode's clock.
pub const DEMO_LIVE_FLYING: &str = "11111111111111111111111111";

/// A track everyone may see, of a glider that landed nine minutes ago.
pub const DEMO_LIVE_LANDED: &str = "22222222222222222222222222";

/// A track for those with its link only, of a glider in the air. It is in
/// no list.
pub const DEMO_LIVE_LINK: &str = "33333333333333333333333333";

/// A part of the sample flight as the fixes of a live track: the fixes
/// numbered `from` to `to` and then `standing_s` seconds without moving,
/// moved in time so that the last of them is `age_s` seconds old at `now`.
/// Each fix but the first says how the glider moved since the one before.
fn sample_fixes(
    (from, to): (usize, usize),
    standing_s: i64,
    age_s: i64,
    now: Timestamp,
) -> Vec<LiveFix> {
    let flight = synthetic_flight().fixes;
    let mut part: Vec<igc::Fix> = flight[from..=to.min(flight.len() - 1)].to_vec();
    if let Some(last) = part.last().copied() {
        for second in 1..=standing_s {
            let mut standing = last;
            standing.time += Duration::seconds(second);
            part.push(standing);
        }
    }
    let Some(last) = part.last() else {
        return Vec::new();
    };
    let moved_by = now - Duration::seconds(age_s) - last.time;
    let tenth = |value: f64| (value * 10.0).round() / 10.0;

    let mut fixes = Vec::with_capacity(part.len());
    for (index, fix) in part.iter().enumerate() {
        let mut live = LiveFix {
            time: fix.time + moved_by,
            position: fix.position,
            altitude_m: fix.gps_altitude_m,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        };
        if let Some(before) = index.checked_sub(1).map(|index| &part[index]) {
            let seconds = (fix.time - before.time).num_seconds().max(1) as f64;
            let metres = before.position.distance_m(fix.position);
            let speed = tenth(metres / seconds);
            live.ground_speed_ms = Some(speed);
            live.climb_ms = Some(tenth(
                f64::from(fix.gps_altitude_m - before.gps_altitude_m) / seconds,
            ));
            // A glider that stands still moves in no direction.
            if speed > 0.0 {
                let north = fix.position.lat() - before.position.lat();
                let east = (fix.position.lon() - before.position.lon())
                    * fix.position.lat().to_radians().cos();
                live.course_deg = Some(east.atan2(north).to_degrees().rem_euclid(360.0).round());
            }
        }
        fixes.push(live);
    }
    fixes
}

/// The demo account's live tracks, as they stand at `now`:
///
/// - the first twenty minutes of the sample flight, public, the glider in
///   the air above the Tegernsee valley and heard of four seconds ago;
/// - the last ten minutes of the sample flight and a minute on the ground
///   after them, public, landed nine minutes ago;
/// - the first seven minutes of the sample flight, for those with the link
///   only, heard of twenty seconds ago.
fn demo_live_tracks(account_id: &str, now: Timestamp) -> Vec<LiveBatch> {
    let track = |id: &str, visibility, landed, fixes| LiveBatch {
        id: id.to_owned(),
        account_id: account_id.to_owned(),
        visibility,
        landed,
        fixes,
    };
    vec![
        track(
            DEMO_LIVE_FLYING,
            LiveVisibility::Public,
            false,
            sample_fixes((0, 1200), 0, 4, now),
        ),
        track(
            DEMO_LIVE_LANDED,
            LiveVisibility::Public,
            true,
            sample_fixes((1145, 1744), 60, 9 * 60, now),
        ),
        track(
            DEMO_LIVE_LINK,
            LiveVisibility::Link,
            false,
            sample_fixes((0, 420), 0, 20, now),
        ),
    ]
}

/// The sample flight moved to another day and `later` in that day than it
/// starts by itself, which is at 10:30 UTC; cut to its first `seconds` if
/// given, and with the glider standing still for `landed_s` seconds at the
/// end.
fn sample(date: NaiveDate, later: Duration, seconds: Option<usize>, landed_s: i64) -> Vec<u8> {
    let original = synthetic_flight();
    let moved_by = date - original.date + later;
    let mut fixes = original.fixes;
    if let Some(seconds) = seconds {
        fixes.truncate(seconds + 1);
    }
    for fix in &mut fixes {
        fix.time += moved_by;
    }
    if let Some(last) = fixes.last().copied() {
        for second in 1..=landed_s {
            let mut standing = last;
            standing.time += Duration::seconds(second);
            fixes.push(standing);
        }
    }
    let file = IgcFile {
        date,
        fixes,
        ..original
    };
    igc::write(&file).into_bytes()
}

fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

/// The demo account's flight files with who may see each, newest first:
///
/// - a flight that lands, public: it is on the pilot's page and has a
///   share page;
/// - the sample flight exactly as the apps bundle it, unlisted: its share
///   page works and it is listed nowhere. Its recording ends in the air;
/// - a short flight of the year before, private.
fn demo_flights() -> Vec<(Vec<u8>, Visibility)> {
    let at_its_time = Duration::zero();
    vec![
        (
            sample(day(2026, 8, 15), at_its_time, None, 90),
            Visibility::Public,
        ),
        (
            igc::write(&synthetic_flight()).into_bytes(),
            Visibility::Unlisted,
        ),
        (
            sample(day(2025, 6, 21), at_its_time, Some(700), 90),
            Visibility::Private,
        ),
    ]
}

/// A pilot the development mode starts with beside the demo account, so
/// that the list of everyone's public flights holds the flights of
/// several. The names are made up, as "Max Mustermann" is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoPilot {
    /// The account's identifier, fixed so that the addresses of its
    /// flights are the same on every start.
    id: &'static str,
    /// Its handle.
    pub handle: &'static str,
    /// Its display name.
    pub name: &'static str,
}

/// The two further pilots. Each signs in with [`DEMO_PASSWORD`]; neither
/// has a token that always works, and neither has a live track.
pub const DEMO_PILOTS: [DemoPilot; 2] = [
    DemoPilot {
        id: "00000000000000000000000003",
        handle: "mara",
        name: "Mara Muster",
    },
    DemoPilot {
        id: "00000000000000000000000004",
        handle: "jonas",
        name: "Jonas Beispiel",
    },
];

/// The flight files of the two further pilots, in the order of
/// [`DEMO_PILOTS`], with who may see each. Every one is the sample flight:
/// on another day, at another time of day, some of them cut short. A flight
/// that is cut short ends in the air, as a recording does whose recorder
/// was switched off early. Nothing else may be shipped as a flight, so all
/// of them take off from the same place.
///
/// With the demo account's public flight that makes five public flights of
/// three pilots: two on 15 August 2026, two on the 16th and one on 6
/// September. One flight of each further pilot is not public, and must
/// show in no list.
fn further_flights() -> [Vec<(Vec<u8>, Visibility)>; 2] {
    let minutes = Duration::minutes;
    [
        vec![
            (
                sample(day(2026, 8, 15), minutes(40), Some(1500), 0),
                Visibility::Public,
            ),
            (
                sample(day(2026, 8, 16), minutes(95), None, 90),
                Visibility::Public,
            ),
            (
                sample(day(2026, 8, 17), minutes(20), Some(900), 0),
                Visibility::Unlisted,
            ),
        ],
        vec![
            (
                sample(day(2026, 8, 16), minutes(-30), Some(1200), 0),
                Visibility::Public,
            ),
            (
                sample(day(2026, 9, 6), minutes(150), None, 90),
                Visibility::Public,
            ),
            (
                sample(day(2026, 9, 7), minutes(0), Some(600), 0),
                Visibility::Private,
            ),
        ],
    ]
}

/// Puts an account with the demo password and its flights into the store.
async fn add_pilot(
    store: &MemoryStore,
    account: &Account,
    flights: Vec<(Vec<u8>, Visibility)>,
    password_cost: Cost,
) -> Result<(), String> {
    let now = account.created_at;
    let password_hash = Hasher::new(password_cost)
        .hash(&Password::new(DEMO_PASSWORD))
        .await
        .map_err(|e| e.to_string())?;
    let new = NewAccount {
        account: account.clone(),
        password_hash,
        invite: None,
    };
    let created = store
        .create_account(&new)
        .await
        .map_err(|e| e.to_string())?;
    if created != AccountCreation::Created {
        return Err(format!(
            "the account {} of the development mode could not be created",
            account.handle
        ));
    }
    for (file, visibility) in flights {
        // These files are the server's own; they are analysed right here.
        let derivation = derive(&file).map_err(|e| format!("a demo flight is unusable: {e}"))?;
        let new = new_flight(&account.id, &file, derivation, now);
        store
            .save_flight(&new, &Room::UNLIMITED)
            .await
            .map_err(|e| e.to_string())?;
        store
            .set_visibility(&account.id, &new.id, visibility, now)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Puts the demo account, its session, its flights and its live tracks
/// into the store, and the two further pilots with their flights.
async fn add_demo_account(
    store: &MemoryStore,
    now: Timestamp,
    password_cost: Cost,
) -> Result<(), String> {
    let account = Account {
        id: DEMO_ACCOUNT_ID.to_owned(),
        handle: DEMO_HANDLE.to_owned(),
        display_name: DEMO_NAME.to_owned(),
        created_at: now,
        publishing_suspended_at: None,
    };
    add_pilot(store, &account, demo_flights(), password_cost).await?;
    for (pilot, flights) in DEMO_PILOTS.iter().zip(further_flights()) {
        let further = Account {
            id: pilot.id.to_owned(),
            handle: pilot.handle.to_owned(),
            display_name: pilot.name.to_owned(),
            created_at: now,
            publishing_suspended_at: None,
        };
        add_pilot(store, &further, flights, password_cost).await?;
    }

    let session = Session {
        id: DEMO_SESSION_ID.to_owned(),
        account_id: account.id.clone(),
        device: Some("Development token".to_owned()),
        created_at: now,
        last_used_at: now,
    };
    store
        .create_session(&session, &demo_token_hash()?, usize::MAX)
        .await
        .map_err(|e| e.to_string())?;

    // The live tracks are the server's own too: every fix is taken,
    // whatever its time.
    let room = LiveRoom {
        fixes_per_track: u32::MAX,
        may_create: true,
        expired_before: Timestamp::MIN_UTC,
        now,
    };
    for track in demo_live_tracks(&account.id, now) {
        let saved = store
            .save_live_fixes(&track, &room)
            .await
            .map_err(|e| e.to_string())?;
        if !matches!(saved, LiveSaved::Kept(kept) if kept.created && kept.full == 0) {
            return Err("a demo live track could not be begun".to_owned());
        }
    }
    Ok(())
}

fn demo_token_hash() -> Result<SecretHash, String> {
    Token::parse(DEMO_TOKEN)
        .map(|token| token.hash())
        .ok_or_else(|| "the demo token does not have the form of a token".to_owned())
}

/// An API over the recorded data and the demo account.
///
/// The API tests use this too, so what they check is what the development
/// mode serves.
pub async fn recorded_api(settings: Settings) -> Result<Api<MemoryStore>, String> {
    Ok(recorded_with_its_store(settings).await?.0)
}

/// The same API, and the store behind it: for tests that also do what only
/// whoever runs a server can do, which the API offers nobody.
pub async fn recorded_with_its_store(
    settings: Settings,
) -> Result<(Api<MemoryStore>, Arc<MemoryStore>), String> {
    let registry = adapters::registry_for(&Providers::recorded())?;
    let pack = RegionPack::from_toml(FIRST_REGION).map_err(|e| e.to_string())?;
    pack.check_against(&registry).map_err(|e| e.to_string())?;
    let jobs = plan::plan(&[pack], &registry).map_err(|e| e.to_string())?;

    let store = Arc::new(MemoryStore::new());
    let now = adapters::recorded_at();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(now));
    add_demo_account(&store, now, settings.password_cost).await?;

    // Forecasts are answered from the recorded answers too, by the service
    // a real server has. Its clock stands at the moment those were
    // recorded, a day after the observations: a forecast cannot have been
    // fetched before its model ran.
    let forecast_clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::forecasts_recorded_at()));
    let forecasts = forecast::service_for(
        &jobs,
        adapters::forecast_sources(&Providers::recorded())?,
        adapters::recorded_payloads(),
        None::<DirArchive>,
        forecast_clock,
        Asking::Recorded(adapters::recorded_forecast_places()),
    )
    .ok_or("the first region lists no forecast source")?;

    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let api = Api::new(store.clone(), clock.clone(), manifests, settings)
        .with_forecasts(forecasts.clone());

    // Everything in the payloads is taken: a day of history per station.
    let collector = Collector::new(
        registry,
        store.clone(),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock,
        None,
    )
    .with_forecasts(forecasts);
    for job in &jobs {
        if !collector.collect(job).await {
            return Err(format!(
                "the recorded payloads of {} no longer give a healthy run",
                job.source
            ));
        }
    }
    Ok((api, store))
}

/// Serves the recorded data on `address` until the process is told to stop.
///
/// Any web origin may call it: it holds nothing but test data, and a web
/// app under development runs on whatever port its tooling picks.
pub async fn dev(address: SocketAddr) -> Result<(), String> {
    // Uploads are analysed as on a real server, in a child of this program.
    let settings = settings(
        serve::local_url(address)?,
        Analyser::child_of_this_program()?,
        Cost::OWASP,
    );
    let router = recorded_api(settings).await?.router(&Cors::Any);
    let shutdown = CancellationToken::new();
    tokio::spawn(serve::cancel_on_signal(shutdown.clone()));
    tracing::info!(
        "development mode: recorded data of {}, no network, no database",
        adapters::recorded_at().format("%Y-%m-%d %H:%M UTC")
    );
    tracing::info!(
        "demo account: handle {DEMO_HANDLE:?}, password {DEMO_PASSWORD:?}; \
         or send 'Authorization: Bearer {DEMO_TOKEN}'"
    );
    serve::listen(address, router, shutdown).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_flights_are_three_different_flights() {
        let flights: Vec<_> = demo_flights()
            .iter()
            .map(|(file, _)| derive(file).unwrap())
            .collect();

        let mut identities: Vec<&str> = flights.iter().map(|f| f.identity.as_str()).collect();
        identities.dedup();
        assert_eq!(identities.len(), 3);
        // The second is the file the apps bundle.
        assert_eq!(flights[1].identity, "6c55247b80a34b03fa0bb3bd38173c59");
        // Newest first, and the two that were given a landing have one.
        assert!(flights[0].derived.start > flights[1].derived.start);
        assert!(flights[1].derived.start > flights[2].derived.start);
        assert!(flights[0].derived.landing.is_some());
        assert!(flights[1].derived.landing.is_none());
        assert!(flights[2].derived.landing.is_some());
        assert!(flights.iter().all(|f| f.derived.takeoff.is_some()));
    }

    #[test]
    fn the_demo_token_and_password_have_the_form_the_server_asks_for() {
        assert!(Token::parse(DEMO_TOKEN).is_some());
        assert!(Password::new(DEMO_PASSWORD).check().is_ok());
        assert!(venti_api_types::v1::Handle::new(DEMO_HANDLE).is_ok());
    }

    #[test]
    fn the_further_pilots_fly_the_sample_flight_on_days_of_their_own() {
        use crate::accounts::{is_reserved, secret};
        use venti_api_types::v1::{Handle, parse_display_name};

        let mut identities = Vec::new();
        let mut starts = Vec::new();
        let mut public = 0;
        for (pilot, flights) in DEMO_PILOTS.iter().zip(further_flights()) {
            // An account anyone could have registered.
            let handle = Handle::new(pilot.handle).unwrap();
            assert_eq!(handle.as_str(), pilot.handle);
            assert!(!is_reserved(&handle));
            assert_eq!(parse_display_name(pilot.name).unwrap(), pilot.name);
            assert!(secret::is_id(pilot.id));
            assert_ne!(pilot.id, DEMO_ACCOUNT_ID);
            assert_ne!(pilot.id, DEMO_SESSION_ID);

            for (file, visibility) in &flights {
                let flight = derive(file).unwrap();
                // Every one took off where the sample flight takes off.
                let takeoff = flight.derived.takeoff.unwrap().position;
                assert!((takeoff.lat() - 47.6642).abs() < 1e-4, "{takeoff:?}");
                assert!((takeoff.lon() - 11.7967).abs() < 1e-4, "{takeoff:?}");
                assert!(flight.derived.score.is_some());
                public += usize::from(*visibility == Visibility::Public);
                identities.push(flight.identity);
                starts.push(flight.derived.start);
            }
            // Each pilot has a flight that is in no list.
            assert!(flights.iter().any(|(_, v)| *v != Visibility::Public));
        }
        assert_eq!(public, 4);
        // No two are the same flight, none is one of the demo account's,
        // and no two start in the same second.
        for (file, _) in demo_flights() {
            let flight = derive(&file).unwrap();
            identities.push(flight.identity);
            starts.push(flight.derived.start);
        }
        let all = identities.len();
        identities.sort();
        identities.dedup();
        assert_eq!(identities.len(), all);
        starts.sort();
        starts.dedup();
        assert_eq!(starts.len(), all);
    }
}
