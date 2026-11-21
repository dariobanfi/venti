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
//! flights holds the flights of several. The three pilots are in a group
//! that the demo account keeps, with plans for the day this mode's clock
//! shows and the two days after; the two further pilots have a second
//! group that the demo account is not in. Client work and UI tests run
//! against this.
//!
//! Map imagery is answered from the tiles recorded from its providers, cut
//! as a real server cuts them, and every other tile is "nothing here". With
//! `--online-imagery` this one route asks the providers over the network
//! and keeps their tiles in a folder under the system's temporary folder;
//! nothing else of the mode does.
//!
//! Map regions are the one thing that is not compiled in: the mode hands
//! out those of a folder it is given (`--maps`), by the code a real server
//! runs, and none without. The region the apps bundle is such a folder.
//!
//! The clock stands still at the moment of the recording, so the data never
//! ages: `/v1/health` reports fresh sources, and "the last 24 hours" is
//! always the recorded day. Every start serves exactly the same answers.
//! What a client changes (an account it creates, a flight it uploads) lasts
//! until the program stops.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Duration, NaiveDate, NaiveTime};
use tokio_util::sync::CancellationToken;
use venti_api_types::v1::Password;
use venti_domain::Timestamp;
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::synthetic_flight;
use venti_regions::RegionPack;
use venti_store::{
    Account, AccountCreation, GroupCreation, GroupJoining, LiveBatch, LiveFix, LiveRoom, LiveSaved,
    LiveVisibility, MemoryStore, NewAccount, NewGroup, NewPlan, PlanChange, PlanDetails, PlanRoom,
    PlanSaving, Room, SecretHash, Session, Store, Visibility,
};

use crate::accounts::client::TrustedProxies;
use crate::accounts::password::{Cost, Hasher};
use crate::accounts::secret::{InviteCode, Token};
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
use crate::imagery::{self, Imagery, TileCache};
use crate::ingest::{Collector, DirArchive, HttpFetch};
use crate::maps::Maps;
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
        groups: crate::groups::Limits::none(),
    }
}

/// The identifier of the group the demo account keeps, with the two
/// further pilots in it. Like everything below it is fixed, so that a
/// client or a test can ask for it without making it first, and no
/// secret: it exists only in this mode.
pub const DEMO_GROUP: &str = "44444444444444444444444444";

/// The name of that group: "Mustergruppe", a made-up group as "Max
/// Mustermann" is a made-up man.
pub const DEMO_GROUP_NAME: &str = "Mustergruppe";

/// The invite code of that group. An account that a test creates comes
/// into the group with it.
pub const DEMO_GROUP_CODE: &str = "4444-4444-4444-4444";

/// The identifier of a group the demo account is not in: `mara` keeps it
/// and `jonas` is in it. To the demo account it is no group, until the
/// account comes into it with [`DEMO_OTHER_GROUP_CODE`].
pub const DEMO_OTHER_GROUP: &str = "55555555555555555555555555";

/// The name of that group.
pub const DEMO_OTHER_GROUP_NAME: &str = "Beispielgruppe";

/// The invite code of that group.
pub const DEMO_OTHER_GROUP_CODE: &str = "5555-5555-5555-5555";

/// The identifiers of the plans of the demo account's group: for the day
/// this mode's clock shows, 18 November 2026, and for the two days after.
pub const DEMO_PLANS: [&str; 3] = [
    "66666666666666666666666666",
    "77777777777777777777777777",
    "88888888888888888888888888",
];

/// The identifier of the one plan of the other group, for 19 November.
pub const DEMO_OTHER_PLAN: &str = "99999999999999999999999999";

/// One plan of the development mode, as [`add_demo_groups`] stores it.
struct DemoPlan {
    id: &'static str,
    group: &'static str,
    /// The handle of the pilot who posted it.
    author: &'static str,
    /// Hours before this mode's clock at which it was posted.
    posted_hours_ago: i64,
    /// The day of November 2026 it is for.
    day: u32,
    place: &'static str,
    /// Where the place is: the sample flight's take-off or nowhere. These
    /// are the project's own coordinates; no name and no position here is
    /// taken from the flying sites the server collects.
    at_the_takeoff: bool,
    /// When to meet: hour and minute.
    meet_at: Option<(u32, u32)>,
    note: Option<&'static str>,
    /// The handles of the others who are in, each with how many minutes
    /// before this mode's clock they said so.
    also_in: &'static [(&'static str, i64)],
}

/// The plans of the two groups. The places are made up, like the groups:
/// a "Musterberg" and a "Beispielwiese" are a sample mountain and an
/// example meadow.
///
/// - Today, the 18th: posted by the demo account, with a position, a time
///   and a note; all three are in.
/// - The 19th: posted by `mara`, with a time and a note and no position;
///   `jonas` is in, the demo account is not.
/// - The 20th: posted by `jonas`, a place and nothing else; the demo
///   account is in.
/// - In the other group, the 19th: posted by `jonas`, with `mara` in.
const DEMO_GROUP_PLANS: [DemoPlan; 4] = [
    DemoPlan {
        id: DEMO_PLANS[0],
        group: DEMO_GROUP,
        author: DEMO_HANDLE,
        posted_hours_ago: 26,
        day: 18,
        place: "Musterberg, Startplatz Ost",
        at_the_takeoff: true,
        meet_at: Some((10, 30)),
        note: Some(
            "A made-up plan of the development mode.\nAt the lift at half past ten, in the air by noon.",
        ),
        also_in: &[("mara", 25 * 60), ("jonas", 20 * 60)],
    },
    DemoPlan {
        id: DEMO_PLANS[1],
        group: DEMO_GROUP,
        author: "mara",
        posted_hours_ago: 3,
        day: 19,
        place: "Beispielwiese",
        at_the_takeoff: false,
        meet_at: Some((9, 0)),
        note: Some("A made-up plan. Ground handling if the wind allows, coffee if not."),
        also_in: &[("jonas", 120)],
    },
    DemoPlan {
        id: DEMO_PLANS[2],
        group: DEMO_GROUP,
        author: "jonas",
        posted_hours_ago: 1,
        day: 20,
        place: "Musterberg",
        at_the_takeoff: false,
        meet_at: None,
        note: None,
        also_in: &[(DEMO_HANDLE, 30)],
    },
    DemoPlan {
        id: DEMO_OTHER_PLAN,
        group: DEMO_OTHER_GROUP,
        author: "jonas",
        posted_hours_ago: 4,
        day: 19,
        place: "Beispielhang, Nordseite",
        at_the_takeoff: false,
        meet_at: Some((13, 0)),
        note: None,
        also_in: &[("mara", 210)],
    },
];

/// Puts the two groups of the development mode and their plans into the
/// store. `accounts` are the three pilots, by handle.
///
/// - [`DEMO_GROUP`]: made thirty days before `now` by the demo account;
///   `mara` came in a day later and `jonas` ten days after that.
/// - [`DEMO_OTHER_GROUP`]: made ten days before `now` by `mara`; `jonas`
///   came in a day later.
async fn add_demo_groups(
    store: &MemoryStore,
    accounts: &[Account],
    now: Timestamp,
) -> Result<(), String> {
    let account = |handle: &str| {
        accounts
            .iter()
            .find(|account| account.handle == handle)
            .ok_or_else(|| format!("the development mode has no pilot {handle}"))
    };
    let code = |text: &str| {
        InviteCode::parse(text)
            .map(|code| code.group_hash())
            .ok_or_else(|| format!("{text} does not have the form of an invite code"))
    };
    let room = crate::groups::Limits::none().room();
    let groups = [
        (
            (DEMO_GROUP, DEMO_GROUP_NAME, DEMO_GROUP_CODE),
            [(DEMO_HANDLE, 30), ("mara", 29), ("jonas", 20)].as_slice(),
        ),
        (
            (
                DEMO_OTHER_GROUP,
                DEMO_OTHER_GROUP_NAME,
                DEMO_OTHER_GROUP_CODE,
            ),
            [("mara", 10), ("jonas", 9)].as_slice(),
        ),
    ];
    for ((id, name, invite), members) in groups {
        let invite = code(invite)?;
        // The first of them makes the group; the others come into it.
        for (index, (handle, days_ago)) in members.iter().enumerate() {
            let (member, at) = (account(handle)?, now - Duration::days(*days_ago));
            let there = if index == 0 {
                let new = NewGroup {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    created_at: at,
                    keeper_id: member.id.clone(),
                    invite,
                };
                let made = store.create_group(&new, &room).await;
                matches!(made, Ok(GroupCreation::Created(_)))
            } else {
                let joined = store.join_group(&member.id, &invite, &room, at).await;
                matches!(joined, Ok(GroupJoining::Joined(_)))
            };
            if !there {
                return Err(format!("{handle} could not be put into the group {name}"));
            }
        }
    }

    let plan_room = PlanRoom {
        plans_per_group: u32::MAX,
        gone_before: NaiveDate::MIN,
    };
    for plan in &DEMO_GROUP_PLANS {
        let new = NewPlan {
            id: plan.id.to_owned(),
            group_id: plan.group.to_owned(),
            author_id: account(plan.author)?.id.clone(),
            created_at: now - Duration::hours(plan.posted_hours_ago),
            details: PlanDetails {
                day: day(2026, 11, plan.day),
                place: plan.place.to_owned(),
                position: plan
                    .at_the_takeoff
                    .then(|| synthetic_flight().fixes.first().map(|fix| fix.position))
                    .flatten(),
                meet_at: plan
                    .meet_at
                    .and_then(|(hour, minute)| NaiveTime::from_hms_opt(hour, minute, 0)),
                note: plan.note.map(str::to_owned),
            },
        };
        let stored = store.save_plan(&new, &plan_room).await;
        if !matches!(stored, Ok(PlanSaving::Stored(_))) {
            return Err(format!("the plan for {} could not be stored", plan.place));
        }
        for (handle, minutes_ago) in plan.also_in {
            let said = store
                .set_plan_in(
                    plan.group,
                    plan.id,
                    &account(handle)?.id,
                    true,
                    now - Duration::minutes(*minutes_ago),
                    NaiveDate::MIN,
                )
                .await;
            if !matches!(said, Ok(PlanChange::Now(_))) {
                return Err(format!("{handle} could not be put down for {}", plan.place));
            }
        }
    }
    Ok(())
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
/// into the store, the two further pilots with their flights, and the
/// groups the three are in.
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
    let mut pilots = vec![account.clone()];
    for (pilot, flights) in DEMO_PILOTS.iter().zip(further_flights()) {
        let further = Account {
            id: pilot.id.to_owned(),
            handle: pilot.handle.to_owned(),
            display_name: pilot.name.to_owned(),
            created_at: now,
            publishing_suspended_at: None,
        };
        add_pilot(store, &further, flights, password_cost).await?;
        pilots.push(further);
    }
    add_demo_groups(store, &pilots, now).await?;

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
    // recorded, some days after the observations: a forecast cannot have
    // been fetched before its model ran.
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

/// The development mode's map imagery, and a line for its log that says
/// where it comes from.
///
/// Without `online`, the tiles recorded from the providers and nothing
/// else: no network, and nothing kept. With it, the providers over the
/// network, as on a real server, with the tiles kept in
/// `venti-dev-imagery` under the system's temporary folder.
pub fn imagery(online: bool) -> Result<(Arc<Imagery>, String), String> {
    let sources = adapters::imagery_sources();
    if !online {
        let imagery = Imagery::new(
            sources,
            Arc::new(adapters::recorded_imagery()),
            None,
            imagery::Limits::default(),
        );
        return Ok((
            Arc::new(imagery),
            "imagery: the recorded tiles only (--online-imagery asks the providers)".to_owned(),
        ));
    }
    let folder = std::env::temp_dir().join("venti-dev-imagery");
    let cache = Arc::new(TileCache::new(
        folder,
        imagery::DEFAULT_CACHE_MB * 1024 * 1024,
    )?);
    let said = format!(
        "imagery: ONLINE, asked of the providers over the network and kept in {}",
        cache.root().display()
    );
    let fetch = HttpFetch::new(&crate::config::default_user_agent())?;
    let imagery = Imagery::new(
        sources,
        Arc::new(fetch),
        Some(cache),
        imagery::Limits::default(),
    );
    Ok((Arc::new(imagery), said))
}

/// Serves the recorded data on `address` until the process is told to stop,
/// and with it the map regions in the folder `maps`, if one is given. With
/// `online_imagery` the route of map imagery asks the providers; see
/// [`imagery`].
///
/// Any web origin may call it: it holds nothing but test data, and a web
/// app under development runs on whatever port its tooling picks.
pub async fn dev(
    address: SocketAddr,
    maps: Option<PathBuf>,
    online_imagery: bool,
) -> Result<(), String> {
    // Uploads are analysed as on a real server, in a child of this program.
    let settings = settings(
        serve::local_url(address)?,
        Analyser::child_of_this_program()?,
        Cost::OWASP,
    );
    let shutdown = CancellationToken::new();
    tokio::spawn(serve::cancel_on_signal(shutdown.clone()));
    let mut api = recorded_api(settings).await?;
    if let Some(folder) = maps {
        // Read by the code a real server runs, hashes and all, but before
        // the first request is answered and not beside it: a test that
        // asks for the regions as soon as the mode answers finds them.
        // The region the apps bundle is a few megabytes.
        let regions = Maps::new(shutdown.clone());
        regions
            .read_in_background(folder)
            .await
            .map_err(|error| format!("the map regions could not be read: {error}"))?;
        api = api.with_maps(regions);
    }
    let (imagery, imagery_said) = imagery(online_imagery)?;
    api = api.with_imagery(imagery);
    let router = api.router(&Cors::Any);
    let network = if online_imagery {
        "no network but for map imagery"
    } else {
        "no network"
    };
    tracing::info!(
        "development mode: recorded data of {}, {network}, no database",
        adapters::recorded_at().format("%Y-%m-%d %H:%M UTC")
    );
    tracing::info!("{imagery_said}");
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
