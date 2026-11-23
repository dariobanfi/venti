//! The `dev` subcommand: the same API over recorded data and a demo account.
//!
//! No network and no database. The real adapters run once over the payloads
//! recorded in their crates (wind stations, a sample of the first region's
//! airspace, and the flying sites OpenStreetMap has for the first region),
//! the result goes into an in-memory store, and the real router serves it.
//! The thermal map is made from answers recorded for four places around
//! the Tegernsee, a grid of two by two cells, by the code a real server
//! runs. It has four days: two of ICON-D2, and two of ICON-EU, of which
//! the last is not whole.
//!
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
//! group that the demo account is not in. Three more made-up pilots have
//! flights from a made-up take-off and from a place where no site stands,
//! so that what public flights say about a site and the places no site
//! names have something to show: eleven public flights of six pilots in
//! all. Client work and UI tests run against this.
//!
//! The rain radar has both of the first region's sources, each with the
//! two frames recorded on 2026-11-21 within two minutes of each other:
//! DWD's of 13:10 and 13:15 UTC, GeoSphere Austria's analysis of 12:45 and
//! 13:00. They are collected by the service a real server has, whose
//! clock stands at 13:19 UTC of that day: the newest radar frame is four
//! minutes old and the newest analysis nineteen
//! ([`adapters::radar_recorded_at`]).
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
use venti_api_types::v1::{Password, Scope};
use venti_domain::Timestamp;
use venti_flight::igc::{self, IgcFile};
use venti_flight::testing::synthetic_flight;
use venti_regions::RegionPack;
use venti_store::{
    AccessToken, Account, AccountCreation, GroupCreation, GroupJoining, Listing, LiveBatch,
    LiveFix, LiveRoom, LiveSaved, LiveVisibility, MemoryStore, NewAccount, NewGroup, NewPlan,
    PlanChange, PlanDetails, PlanRoom, PlanSaving, Room, SecretHash, Session, Store, Visibility,
};

use crate::accounts::client::TrustedProxies;
use crate::accounts::password::{Cost, Hasher};
use crate::accounts::secret::{AccessSecret, InviteCode, Token};
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
use crate::thermals::ThermalMap;
use crate::{live, plan, radar, serve};

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

/// The secret of a token the account starts with for another program,
/// "Demo logger", which may read and write the account's flights and
/// nothing else: for trying out what a token opens and what it does not.
/// Not a secret, as the password is none.
pub const DEMO_ACCESS_TOKEN: &str =
    "venti_pat_0000000000000000000000000000000000000000000000000000";

/// What the pilot called that token.
pub const DEMO_ACCESS_TOKEN_NAME: &str = "Demo logger";

/// The token's identifier.
pub const DEMO_ACCESS_TOKEN_ID: &str = "00000000000000000000000003";

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
        // The development mode takes sites and confirmations from
        // pilots, in the first region: its data is made up, and an app is
        // built against it.
        sites: crate::pilot_sites::Settings {
            accept: true,
            regions: RegionPack::from_toml(FIRST_REGION)
                .map(|pack| vec![pack.bounds])
                .unwrap_or_default(),
            limits: crate::pilot_sites::Limits::none(),
        },
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

/// The made-up sites of Venti's own two layers that the development mode
/// starts with. They are fixed, so that a client or a test can ask for
/// one without adding it first.
///
/// - The first, of the partners' layer: a take-off the demo account, the
///   one partner of this mode, added twenty days ago by the mode's clock
///   and gave its wind directions two days later. `mara` and `jonas` say
///   they took off there.
/// - The second, of the crowdsourced layer: a take-off `jonas` added;
///   `mara` gave it a wrong height, and the demo account put it back to
///   the first revision. `mara` says she took off there.
/// - The third, of the partners' layer: a landing field the demo account
///   added, at its first revision.
/// - The fourth, of the crowdsourced layer: a take-off `jonas` added
///   twice by mistake and removed again. Its address answers that it is
///   gone.
///
/// They lie in the Lechtal Alps, in a box of their own:
/// [`DEMO_SITES_BBOX`].
pub const DEMO_SITES: [&str; 4] = [
    "venti:0000000000000000000000s001",
    "venti-pilots:0000000000000000000000s002",
    "venti:0000000000000000000000s003",
    "venti-pilots:0000000000000000000000s004",
];

/// A box that holds the sites of [`DEMO_SITES`] and nothing of another
/// layer in the recorded data: `west,south,east,north`.
pub const DEMO_SITES_BBOX: &str = "10.85,47.25,10.95,47.32";

/// Puts the made-up sites of the development mode into the store, with
/// their histories. `accounts` are the three pilots, by handle.
async fn add_demo_sites(
    store: &MemoryStore,
    accounts: &[Account],
    now: Timestamp,
) -> Result<(), String> {
    use venti_domain::{GeoPoint, Site, SiteUse, WindSector};
    use venti_store::{NewPilotSite, SiteAct, SiteRevising, SiteVia, SiteWrite};

    use crate::pilot_sites::APART_M;

    let account = |handle: &str| {
        accounts
            .iter()
            .find(|account| account.handle == handle)
            .map(|account| account.id.clone())
            .ok_or_else(|| format!("the development mode has no pilot {handle}"))
    };
    let made_up = "A made-up site of the development mode. No such place exists.";
    let site = |id: &str, name: &str, lat: f64, lon: f64, height: f64, used: SiteUse| {
        Ok::<Site, String>(Site {
            id: id
                .parse()
                .map_err(|_| format!("{id} is no site identifier"))?,
            name: Some(name.to_owned()),
            position: GeoPoint::new(lat, lon).map_err(|e| e.to_string())?,
            elevation_m: Some(height),
            uses: vec![used],
            wind_directions: Vec::new(),
            paragliding: Some(true),
            hang_gliding: None,
            official: None,
            discouraged: false,
            url: None,
            website: None,
            description: Some(made_up.to_owned()),
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        })
    };
    let days = |days: i64| now - chrono::Duration::days(days);
    let must = |written: Result<SiteWrite, venti_store::StoreError>, what: &str| match written {
        Ok(SiteWrite::Written(_)) => Ok(()),
        other => Err(format!("a made-up site could not be {what}: {other:?}")),
    };
    let add = |site: Site, by: String, at: Timestamp, why: &str| NewPilotSite {
        site,
        author_id: by,
        at,
        via: SiteVia::Session,
        comment: Some(why.to_owned()),
    };
    let revise = |id: &str, by: String, at: Timestamp, act: SiteAct, why: &str| {
        Ok::<SiteRevising, String>(SiteRevising {
            id: id
                .parse()
                .map_err(|_| format!("{id} is no site identifier"))?,
            author_id: Some(by),
            at,
            via: SiteVia::Session,
            act,
            comment: Some(why.to_owned()),
            expects: None,
        })
    };
    let [first, second, third, fourth] = DEMO_SITES;

    // The first: added, then given its wind directions.
    let kogel = site(
        first,
        "Musterkogel West",
        47.3,
        10.9,
        1862.0,
        SiteUse::Takeoff,
    )?;
    let new = add(
        kogel.clone(),
        account(DEMO_HANDLE)?,
        days(20),
        "Made up, for the development mode.",
    );
    must(store.add_pilot_site(&new, APART_M).await, "added")?;
    let with_wind = Site {
        wind_directions: vec![WindSector {
            from_deg: 225.0,
            to_deg: 315.0,
        }],
        ..kogel
    };
    let change = revise(
        first,
        account(DEMO_HANDLE)?,
        days(18),
        SiteAct::Change(Box::new(with_wind)),
        "Wind directions added.",
    )?;
    must(store.revise_pilot_site(&change, APART_M).await, "changed")?;

    // The second: added, changed for the worse, and put back.
    let joch = site(
        second,
        "Beispieljoch",
        47.29,
        10.91,
        2104.0,
        SiteUse::Takeoff,
    )?;
    let new = add(
        joch.clone(),
        account("jonas")?,
        days(12),
        "Made up, for the development mode.",
    );
    must(store.add_pilot_site(&new, APART_M).await, "added")?;
    let wrong = Site {
        elevation_m: Some(1204.0),
        ..joch
    };
    let change = revise(
        second,
        account("mara")?,
        days(6),
        SiteAct::Change(Box::new(wrong)),
        "Height corrected.",
    )?;
    must(store.revise_pilot_site(&change, APART_M).await, "changed")?;
    let undo = revise(
        second,
        account(DEMO_HANDLE)?,
        days(5),
        SiteAct::Restore(1),
        "The height was right before.",
    )?;
    must(store.revise_pilot_site(&undo, APART_M).await, "restored")?;

    // The third: a landing field at its first revision.
    let wiese = site(third, "Musterwiese", 47.28, 10.89, 1010.0, SiteUse::Landing)?;
    let new = add(
        wiese,
        account(DEMO_HANDLE)?,
        days(4),
        "Made up, for the development mode.",
    );
    must(store.add_pilot_site(&new, APART_M).await, "added")?;

    // The fourth: added by mistake and removed again.
    let twice = site(
        fourth,
        "Musterkogel",
        47.305,
        10.905,
        1870.0,
        SiteUse::Takeoff,
    )?;
    let new = add(
        twice,
        account("jonas")?,
        days(3),
        "Made up, for the development mode.",
    );
    must(store.add_pilot_site(&new, APART_M).await, "added")?;
    let removal = revise(
        fourth,
        account("jonas")?,
        days(3) + chrono::Duration::minutes(10),
        SiteAct::Remove { take_down: false },
        "Added twice: the same take-off as Musterkogel West.",
    )?;
    must(store.revise_pilot_site(&removal, APART_M).await, "removed")?;

    // What pilots say of them: two of the partner's take-off, one of the
    // crowdsourced one.
    for (id, handle, ago, note) in [
        (first, "mara", 2, Some("North-west at noon, smooth.")),
        (first, "jonas", 9, None),
        (second, "mara", 6, None),
    ] {
        let confirmation = venti_store::NewSiteConfirmation {
            site: id
                .parse()
                .map_err(|_| format!("{id} is no site identifier"))?,
            account_id: account(handle)?,
            day: days(ago).date_naive(),
            note: note.map(str::to_owned),
            with_flight: false,
            at: days(ago),
        };
        match store.confirm_pilot_site(&confirmation).await {
            Ok(venti_store::SiteConfirming::Confirmed(_)) => {}
            other => return Err(format!("a made-up site could not be confirmed: {other:?}")),
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
/// three pilots from this take-off: two on 15 August 2026, two on the 16th
/// and one on 6 September. The mode's other six public flights are those
/// of [`add_demo_takeoffs`], flown elsewhere. One flight of each further pilot is not public, and must
/// show in no list. The first one's was public and is shared by link now
/// ([`withdraw_a_flight`]).
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

/// The sample flight as [`sample`] makes it, flown from another place: every
/// fix moved by `north` degrees of latitude and `east` of longitude.
/// Nothing but the sample flight is shipped as a flight, and this is still
/// that flight.
fn sample_from(
    date: NaiveDate,
    seconds: Option<usize>,
    (north, east): (f64, f64),
) -> Result<Vec<u8>, String> {
    let text = String::from_utf8(sample(date, Duration::zero(), seconds, 90))
        .map_err(|e| e.to_string())?;
    let mut file = igc::parse(&text).map_err(|e| format!("the sample flight is unusable: {e}"))?;
    for fix in &mut file.fixes {
        fix.position =
            venti_domain::GeoPoint::new(fix.position.lat() + north, fix.position.lon() + east)
                .map_err(|e| e.to_string())?;
    }
    Ok(igc::write(&file).into_bytes())
}

/// Three more made-up pilots, for [`add_demo_takeoffs`]. Each signs in
/// with [`DEMO_PASSWORD`].
pub const DEMO_TAKEOFF_PILOTS: [DemoPilot; 3] = [
    DemoPilot {
        id: "00000000000000000000000005",
        handle: "lena",
        name: "Lena Probe",
    },
    DemoPilot {
        id: "00000000000000000000000006",
        handle: "timo",
        name: "Timo Entwurf",
    },
    DemoPilot {
        id: "00000000000000000000000007",
        handle: "ines",
        name: "Ines Vorlage",
    },
];

/// A box that holds the candidate of [`add_demo_takeoffs`] and nothing
/// else that is one: `west,south,east,north`. It lies inside
/// [`DEMO_SITES_BBOX`], 4 km from the nearest made-up site.
pub const DEMO_CANDIDATE_BBOX: &str = "10.92,47.25,10.95,47.27";

/// Made-up public flights that say something about where pilots take off
/// (`GET /v1/sites/{id}/flights`, `GET /v1/sites/candidates`): the sample
/// flight, flown by [`DEMO_TAKEOFF_PILOTS`] from two made-up places in
/// the box of the made-up sites.
///
/// - From the first of [`DEMO_SITES`], the take-off "Musterkogel West":
///   three public flights of two pilots, in May and in July, so that the
///   site has flights, months and best flights.
/// - From a place 4 km south-east of it where no site stands: one public
///   flight each of three pilots, in April and June, so that the place is
///   a candidate; and a fourth flight that is private and counts for
///   nothing.
///
/// The development mode starts with these since 2026-11-21 (they are
/// added by [`add_demo_account`]): eleven public flights of six pilots in
/// all, which the apps' tests count.
pub async fn add_demo_takeoffs(store: &MemoryStore, password_cost: Cost) -> Result<(), String> {
    let now = adapters::recorded_at();
    // From the sample flight's own take-off (47.6642 N, 11.7967 E) to the
    // made-up take-off at 47.3 N, 10.9 E, and to the place without a site
    // at 47.2612 N, 10.9312 E.
    let kogel = |north: f64, east: f64| (47.3 - 47.6642 + north, 10.9 - 11.7967 + east);
    let nowhere = |north: f64, east: f64| (47.2612 - 47.6642 + north, 10.9312 - 11.7967 + east);
    let public = Visibility::Public;
    let [lena, timo, ines] = [
        vec![
            (
                sample_from(day(2026, 5, 9), None, kogel(0.0004, 0.0))?,
                public,
            ),
            (
                sample_from(day(2026, 7, 11), Some(1500), kogel(0.0, 0.0003))?,
                public,
            ),
            (
                sample_from(day(2026, 6, 6), Some(600), nowhere(-0.0001, -0.0001))?,
                public,
            ),
        ],
        vec![
            (
                sample_from(day(2025, 7, 19), None, kogel(-0.0003, 0.0006))?,
                public,
            ),
            (
                sample_from(day(2026, 6, 7), Some(700), nowhere(0.0, 0.0001))?,
                public,
            ),
        ],
        vec![
            (
                sample_from(day(2026, 4, 25), Some(800), nowhere(0.0001, 0.0))?,
                public,
            ),
            (
                sample_from(day(2026, 6, 13), Some(900), nowhere(0.0, 0.0))?,
                Visibility::Private,
            ),
        ],
    ];
    for (pilot, flights) in DEMO_TAKEOFF_PILOTS.iter().zip([lena, timo, ines]) {
        let account = Account {
            id: pilot.id.to_owned(),
            handle: pilot.handle.to_owned(),
            display_name: pilot.name.to_owned(),
            created_at: now,
            publishing_suspended_at: None,
            edits_sites_since: None,
            new_flights_public: true,
            country: None,
            glider_class: None,
        };
        add_pilot(store, &account, flights, password_cost).await?;
    }
    Ok(())
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
    // Each demo flight is uploaded private and then given the visibility
    // it is meant to have, so that none of them is withdrawn on the way;
    // the account's own setting is put in after them.
    let new = NewAccount {
        account: Account {
            new_flights_public: false,
            country: None,
            glider_class: None,
            ..account.clone()
        },
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
        let new = new_flight(&account.id, &file, derivation, None, now);
        store
            .save_flight(&new, &Room::UNLIMITED)
            .await
            .map_err(|e| e.to_string())?;
        store
            .set_visibility(&account.id, &new.id, visibility, now)
            .await
            .map_err(|e| e.to_string())?;
    }
    store
        .update_account_settings(
            &account.id,
            &venti_store::AccountSettings {
                new_flights_public: Some(account.new_flights_public),
                ..venti_store::AccountSettings::default()
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Makes the flight a pilot shares by link one that was public before: so
/// that what changed among the public flights
/// (`GET /v1/flights/public/changes`) holds a flight that was withdrawn,
/// for a client that keeps a copy to try removing one.
async fn withdraw_a_flight(
    store: &MemoryStore,
    pilot: &Account,
    now: Timestamp,
) -> Result<(), String> {
    let flights = store
        .flights_of(&pilot.id, Listing::All, None, 10)
        .await
        .map_err(|e| e.to_string())?;
    let by_link = flights
        .iter()
        .find(|flight| flight.visibility == Visibility::Unlisted)
        .ok_or("the development mode has no flight shared by link to withdraw")?;
    for visibility in [Visibility::Public, Visibility::Unlisted] {
        store
            .set_visibility(&pilot.id, &by_link.id, visibility, now)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Puts the demo account, its session, its flights and its live tracks
/// into the store, the two further pilots with their flights, the groups
/// the three are in, their made-up sites, and the three pilots of
/// [`add_demo_takeoffs`] with theirs.
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
        edits_sites_since: None,
        new_flights_public: true,
        country: None,
        glider_class: None,
    };
    add_pilot(store, &account, demo_flights(), password_cost).await?;
    // The demo account is this mode's one partner for flying sites: it
    // writes to the partners' layer, and the other two pilots do not, so
    // that both sides can be seen and tested.
    store
        .let_edit_sites(&account.id, now)
        .await
        .map_err(|error| format!("the demo account could not be trusted with sites: {error}"))?;
    let mut pilots = vec![account.clone()];
    for (pilot, flights) in DEMO_PILOTS.iter().zip(further_flights()) {
        let further = Account {
            id: pilot.id.to_owned(),
            handle: pilot.handle.to_owned(),
            display_name: pilot.name.to_owned(),
            created_at: now,
            publishing_suspended_at: None,
            edits_sites_since: None,
            new_flights_public: true,
            country: None,
            glider_class: None,
        };
        add_pilot(store, &further, flights, password_cost).await?;
        pilots.push(further);
    }
    withdraw_a_flight(store, &pilots[1], now).await?;
    add_demo_groups(store, &pilots, now).await?;
    add_demo_sites(store, &pilots, now).await?;
    add_demo_takeoffs(store, password_cost).await?;
    add_demo_alerts(store, &account.id, now).await?;
    add_demo_social(store, now).await?;

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

    let logger = AccessToken {
        id: DEMO_ACCESS_TOKEN_ID.to_owned(),
        account_id: account.id.clone(),
        name: DEMO_ACCESS_TOKEN_NAME.to_owned(),
        scopes: [Scope::FlightsRead, Scope::FlightsWrite]
            .map(|scope| scope.as_str().to_owned())
            .to_vec(),
        created_at: now,
        last_used_on: None,
        expires_at: None,
        client: None,
    };
    let secret = AccessSecret::parse(DEMO_ACCESS_TOKEN)
        .ok_or("the demo token for a program does not have the form of one")?;
    let stored = store
        .create_access_token(&logger, &secret.hash(), usize::MAX)
        .await
        .map_err(|e| e.to_string())?;
    if !stored {
        return Err("the demo token for a program could not be stored".to_owned());
    }

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

/// The comments the development mode starts with, by these identifiers:
/// two under the demo account's public flight and one under a flight of
/// the first further pilot.
pub const DEMO_COMMENTS: [&str; 3] = [
    "0000000000000000000000c001",
    "0000000000000000000000c002",
    "0000000000000000000000c003",
];

/// Makes the made-up pilots follow each other, cheer a few of each
/// other's public flights and write three comments, and tells the demo
/// account of what concerned it. Adds no flight and changes none.
async fn add_demo_social(store: &MemoryStore, now: Timestamp) -> Result<(), String> {
    use venti_api_types::v1::{MAX_FOLLOWS, Notification, NotificationKind, NotifiedFlight, Pilot};
    use venti_store::{Cheering, Commenting, Following, NewComment};

    let failed = |error: venti_store::StoreError| error.to_string();
    let ago = |minutes: i64| now - Duration::minutes(minutes);
    let (demo, mara, jonas, lena) = (
        DEMO_ACCOUNT_ID,
        DEMO_PILOTS[0].id,
        DEMO_PILOTS[1].id,
        DEMO_TAKEOFF_PILOTS[0].id,
    );
    let follows = [
        (demo, mara, 600),
        (demo, jonas, 590),
        (demo, lena, 580),
        (mara, demo, 570),
        (mara, jonas, 560),
        (jonas, demo, 550),
        (lena, demo, 90),
    ];
    for (follower, followed, minutes) in follows {
        let made = store
            .follow(follower, followed, ago(minutes), MAX_FOLLOWS)
            .await
            .map_err(failed)?;
        if made != (Following::Follows { new: true }) {
            return Err(format!(
                "a made-up pilot could not follow another: {made:?}"
            ));
        }
    }

    // The newest public flight of each of the three.
    let mut newest = Vec::new();
    for pilot in [demo, mara, jonas] {
        let flight = store
            .flights_of(pilot, Listing::Public, None, 1)
            .await
            .map_err(failed)?
            .pop()
            .ok_or("a made-up pilot has no public flight to cheer")?;
        newest.push(flight);
    }
    let (of_demo, of_mara, of_jonas) = (&newest[0], &newest[1], &newest[2]);
    let cheers = [
        (mara, of_demo, 60),
        (lena, of_demo, 45),
        (demo, of_mara, 500),
        (jonas, of_mara, 480),
        (demo, of_jonas, 470),
    ];
    for (pilot, flight, minutes) in cheers {
        let made = store
            .cheer(pilot, &flight.id, ago(minutes))
            .await
            .map_err(failed)?;
        if made != (Cheering::Cheers { new: true }) {
            return Err(format!(
                "a made-up pilot could not cheer a flight: {made:?}"
            ));
        }
    }
    let comments = [
        (
            DEMO_COMMENTS[0],
            of_demo,
            mara,
            "Well flown. How was the air above the ridge?",
            55,
        ),
        (
            DEMO_COMMENTS[1],
            of_demo,
            demo,
            "Rough at first, calm above 2000 m.\nA made-up answer of the development mode.",
            50,
        ),
        (
            DEMO_COMMENTS[2],
            of_mara,
            demo,
            "A made-up comment of the development mode.",
            490,
        ),
    ];
    for (id, flight, author, text, minutes) in comments {
        let new = NewComment {
            id: id.to_owned(),
            flight_id: flight.id.clone(),
            author_id: author.to_owned(),
            text: text.to_owned(),
            created_at: ago(minutes),
        };
        let made = store.add_comment(&new).await.map_err(failed)?;
        if !matches!(made, Commenting::Added(_)) {
            return Err(format!("a made-up comment could not be written: {made:?}"));
        }
    }

    // What the demo account was told of it, as the API would have noted
    // it. No site is named: these flights were not looked up.
    let pilot = |pilot: &DemoPilot| -> Result<Pilot, String> {
        Ok(Pilot {
            handle: pilot
                .handle
                .parse()
                .map_err(|_| "a made-up handle is none")?,
            display_name: pilot.name.to_owned(),
            country: None,
        })
    };
    let flight = NotifiedFlight {
        id: of_demo
            .id
            .parse()
            .map_err(|_| "a demo flight has no address")?,
        start: of_demo.derived.start,
        takeoff_site: None,
    };
    let told = [
        (
            NotificationKind::Followed,
            &DEMO_TAKEOFF_PILOTS[0],
            90,
            None,
            None,
        ),
        (
            NotificationKind::Cheered,
            &DEMO_PILOTS[0],
            60,
            Some(flight.clone()),
            None,
        ),
        (
            NotificationKind::Commented,
            &DEMO_PILOTS[0],
            55,
            Some(flight.clone()),
            Some(DEMO_COMMENTS[0].to_owned()),
        ),
        (
            NotificationKind::Cheered,
            &DEMO_TAKEOFF_PILOTS[0],
            45,
            Some(flight),
            None,
        ),
    ];
    for (kind, by, minutes, flight, comment) in told {
        let notification = Notification {
            kind,
            at: ago(minutes),
            pilot: pilot(by)?,
            flight,
            comment,
        };
        let record = serde_json::to_string(&notification).map_err(|e| e.to_string())?;
        let noted = store
            .add_notice(demo, by.id, ago(minutes), &record)
            .await
            .map_err(failed)?;
        if !noted {
            return Err("the demo account could not be told of a made-up cheer".to_owned());
        }
    }
    Ok(())
}

/// The demo account's alarms: a wind alarm on the Patscherkofel station
/// and a day alarm near Innsbruck, by these identifiers.
pub const DEMO_ALERTS: [&str; 2] = ["0000000000000000000000a001", "0000000000000000000000a002"];

/// Gives the demo account one alarm of each kind, and makes the wind
/// alarm have fired once, on the day of `now`, for a made-up reading.
async fn add_demo_alerts(
    store: &MemoryStore,
    account_id: &str,
    now: Timestamp,
) -> Result<(), String> {
    use venti_api_types::v1::{
        AlertHours, AlertRule, AlertSector, DayAlert, DayAtLeast, FiredAlert, FiredBecause,
        WindAlert,
    };
    use venti_store::{AddingAlert, AlertCheck, NewAlert};

    let wind = WindAlert {
        station: "geosphere-tawes:11126"
            .parse()
            .map_err(|_| "the demo station's identifier is not one")?,
        station_name: "Patscherkofel".to_owned(),
        directions: Some(AlertSector {
            from_deg: 349,
            to_deg: 56,
        }),
        min_kmh: Some(10),
        max_kmh: Some(20),
        gusts_max_kmh: Some(30),
        hours: Some(AlertHours {
            from_hour: 10,
            to_hour: 16,
        }),
    };
    let day = DayAlert {
        place: venti_domain::GeoPoint::new(47.3, 11.4).map_err(|e| e.to_string())?,
        site: None,
        place_name: "Nordkette".to_owned(),
        days_ahead: vec![0, 1, 2],
        at_least: DayAtLeast::CrossCountry,
    };
    let alarms = [
        (
            DEMO_ALERTS[0],
            "Patscherkofel north",
            AlertRule::Wind(wind.clone()),
            Some("geosphere-tawes:11126"),
            venti_store::WIND_ALERT,
        ),
        (
            DEMO_ALERTS[1],
            "Nordkette cross-country",
            AlertRule::Day(day),
            None,
            venti_store::DAY_ALERT,
        ),
    ];
    let mut wind_alarm = None;
    for (id, name, rule, station, kind) in alarms {
        let new = NewAlert {
            id: id.to_owned(),
            name: name.to_owned(),
            enabled: true,
            kind: kind.to_owned(),
            station_id: station.map(str::to_owned),
            rule: serde_json::to_string(&rule).map_err(|e| e.to_string())?,
        };
        match store
            .add_alert(account_id, &new, now - Duration::days(2), usize::MAX)
            .await
            .map_err(|e| e.to_string())?
        {
            AddingAlert::Added(alert) if wind_alarm.is_none() => wind_alarm = Some(alert),
            AddingAlert::Added(_) => {}
            _ => return Err("a demo alarm could not be added".to_owned()),
        }
    }
    let alarm = wind_alarm.ok_or("the demo wind alarm is missing")?;
    // 11:20 by the alarms' clock on the day of `now`, within its hours.
    let read_at = now
        .date_naive()
        .and_hms_opt(10, 20, 0)
        .map(|time| time.and_utc())
        .filter(|read_at| *read_at <= now)
        .unwrap_or(now - Duration::hours(1));
    let fired = FiredAlert {
        alert_id: alarm.id.clone(),
        alert_name: alarm.name.clone(),
        fired_at: read_at + Duration::minutes(3),
        rule: AlertRule::Wind(wind),
        because: FiredBecause::Wind {
            time: read_at,
            speed_kmh: 15,
            gust_kmh: Some(22),
            direction_deg: Some(10),
        },
    };
    let check = AlertCheck {
        account_id: account_id.to_owned(),
        alert_id: alarm.id,
        changed_at: alarm.changed_at,
        state: format!(
            r#"{{"last":"{}"}}"#,
            read_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        ),
        fired: Some((
            fired.fired_at,
            serde_json::to_string(&fired).map_err(|e| e.to_string())?,
        )),
    };
    if !store
        .note_alert_check(&check)
        .await
        .map_err(|e| e.to_string())?
    {
        return Err("the demo alarm's firing could not be written".to_owned());
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
    let packs = [pack];
    let mut jobs: Vec<plan::Job> = plan::plan(&packs, &registry).map_err(|e| e.to_string())?;
    // The cables were recorded for a small box near the Tegernsee, not for
    // the whole region: the job asks for that box, and that box is where
    // the development mode's set of cables is whole.
    for job in &mut jobs {
        if job.kind == venti_sources::Kind::Cables {
            job.area = venti_sources_osm::fixtures::cables_area();
        }
    }

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

    // The thermal map is made by the code a real server runs, from answers
    // recorded for four places in one request, by a forecast service of
    // its own over them, with a clock of its own. The two sets were
    // recorded together and are of the same runs, so the two clocks stand
    // at the same moment; they are kept as two because each set has its
    // own answers about the runs, under the same addresses.
    let map_clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::thermal_map_recorded_at()));
    let map_forecasts = forecast::service_for(
        &jobs,
        adapters::forecast_sources(&Providers::recorded())?,
        adapters::recorded_thermal_payloads(),
        None::<DirArchive>,
        map_clock.clone(),
        Asking::Recorded(Vec::new()),
    )
    .ok_or("the first region lists no forecast source")?;
    let source = jobs
        .iter()
        .find(|job| job.kind == venti_sources::Kind::Forecasts)
        .map(|job| job.source.clone())
        .ok_or("the first region lists no forecast source")?;
    let map = ThermalMap::on_grid(
        map_forecasts,
        source,
        adapters::recorded_thermal_grid(),
        map_clock,
    )
    .ok_or("the forecast source cannot be asked for several places")?;
    map.update(&adapters::thermal_map_recorded_runs()).await;
    if map.current().is_none() {
        return Err("the recorded answers for several places no longer make a thermal map".into());
    }

    // The rain radar is collected by the service a real server has, from
    // the two frames of each source recorded on 2026-11-21, with a clock
    // of its own that stands shortly after them. It keeps the frames the recording holds
    // and asks for no others.
    let radar_clock: Arc<dyn Clock> = Arc::new(FixedClock(adapters::radar_recorded_at()));
    let radar = radar::service_for(
        &jobs,
        adapters::radar_sources(),
        adapters::recorded_radar_payloads(),
        radar_clock,
        |source| adapters::radar_recorded_at() - adapters::radar_recorded_from(source),
    )
    .ok_or("the first region lists no radar source")?;

    // The live network is fed lines in the Open Glider Network's form,
    // made up over the Tegernsee (not network data: the network forbids
    // passing on its data older than 24 hours), by the code a real server
    // runs; its clock stands just after the newest. It connects nowhere.
    let at = adapters::live_feed_at();
    let live_clock: Arc<dyn Clock> = Arc::new(FixedClock(at));
    let mut networks = Vec::new();
    for (id, _) in plan::live(&packs) {
        let source = adapters::live_sources()
            .into_iter()
            .find(|source| source.manifest().id == id)
            .ok_or_else(|| format!("no adapter of the live source {id}"))?;
        let network =
            live::network::Network::new(source.manifest(), live_clock.clone()).with_salt(0);
        live::network::feed(
            &network,
            source.as_ref(),
            adapters::made_up_live_lines(),
            at,
        );
        let directory = source
            .parse_directory(adapters::made_up_live_directory())
            .map_err(|error| format!("the made-up directory of devices: {error}"))?;
        network.take_directory(directory, at);
        for (device, way) in made_up_climbers(at)? {
            network.take_trail(device, &way);
        }
        networks.push(Arc::new(network));
    }

    // Where thermals have been, read from the made-up public flights by
    // the code a real server runs, once.
    let flown = Arc::new(crate::flights::flown::FlownThermals::default());
    flown
        .update_all(store.as_ref())
        .await
        .map_err(|error| format!("the made-up public flights' thermals: {error}"))?;

    let manifests = jobs.iter().map(|job| job.manifest(&registry));
    let api = Api::new(store.clone(), clock.clone(), manifests, settings)
        .with_station_priority(plan::station_priority(&packs))
        .with_site_priority(plan::site_priority(&packs))
        .with_forecasts(forecasts.clone())
        .with_thermal_map(Arc::new(map))
        .with_radar(radar.clone())
        .with_networks(networks)
        .with_flown_thermals(flown)
        // Its own made-up "Google": the apps' UI tests sign in with it.
        .with_google(Arc::new(
            crate::accounts::google::GoogleSignIn::development()?,
        ));

    // Everything in the payloads is taken: a day of history per station.
    let collector = Collector::new(
        registry,
        store.clone(),
        adapters::recorded_payloads(),
        None::<DirArchive>,
        clock,
        None,
    )
    .with_forecasts(forecasts)
    .with_radar(radar);
    for job in &jobs {
        if !collector.collect(job).await {
            return Err(format!(
                "the recorded payloads of {} no longer give a healthy run",
                job.source
            ));
        }
    }
    add_made_up_pressures(&store, now).await?;
    Ok((api, store))
}

/// Made-up pressures for the foehn reading's two pairs, so that this mode
/// shows one pair at "likely" and one at "none".
///
/// Bolzano's pressure at sea level stands 6.2 hPa above Innsbruck's, which
/// is the recorded one as GeoSphere reduced it, after rising against it
/// all day: south foehn likely. Klagenfurt and Salzburg, two stations the
/// recordings do not have and that get a light made-up wind with it, stand
/// 0.8 hPa apart: no sign. Every one of the four is reduced to sea level
/// by its provider, as on the owner's server: the province sends Bolzano's
/// so, and GeoSphere the three others' beside the pressure at the station.
/// Each station gets a value every
/// two hours of the last day, back from its newest recorded reading and
/// never after it, so that no station's newest reading becomes one without
/// a wind. The runs are dated a day and an hour back, behind the recorded
/// ones, which stay each source's newest.
async fn add_made_up_pressures(store: &MemoryStore, now: Timestamp) -> Result<(), String> {
    use std::collections::BTreeMap;

    use venti_domain::{GeoPoint, Observation, SourceId, Station, StationId};
    use venti_flight::foehn::{PAIRS, reduce_to_sea_level};
    use venti_store::Store as _;

    const STEPS: i32 = 12;
    // Stations send their pressure to a tenth of a hectopascal.
    let tenth = |hpa: f64| (hpa * 10.0).round() / 10.0;
    // Innsbruck's recorded reading as GeoSphere reduced it to sea level:
    // 930.9 hPa at the station and 7.8 °C.
    let innsbruck_hpa = 1001.7;
    // Each station: its name and height where the recordings do not have
    // it, the pressure at sea level now, and how much lower it was a step
    // earlier.
    let made_up = [
        (PAIRS[0].south, None, innsbruck_hpa + 6.2, 0.4),
        (PAIRS[0].north, None, innsbruck_hpa, 0.0),
        (
            PAIRS[1].south,
            Some(("Klagenfurt-Flughafen", 450.0)),
            1001.0,
            0.05,
        ),
        (
            PAIRS[1].north,
            Some(("Salzburg-Flughafen", 430.0)),
            1000.2,
            0.05,
        ),
    ];
    let mut runs: BTreeMap<SourceId, (Vec<Station>, Vec<Observation>)> = BTreeMap::new();
    for (place, unknown, hpa_now, fall) in made_up {
        let id: StationId = place.id.parse().map_err(|e| format!("{}: {e}", place.id))?;
        let station = match store.station(&id).await.map_err(|e| e.to_string())? {
            Some(station) => station,
            None => {
                let (name, elevation_m) =
                    unknown.ok_or_else(|| format!("the recordings have no station {id}"))?;
                Station {
                    id: id.clone(),
                    name: name.to_owned(),
                    position: GeoPoint::new(place.lat, place.lon).map_err(|e| e.to_string())?,
                    elevation_m: Some(elevation_m),
                }
            }
        };
        let newest = store
            .observations(&id, now - Duration::hours(24), now)
            .await
            .map_err(|e| e.to_string())?
            .last()
            .map_or(now - Duration::minutes(15), |observation| observation.time);
        let (stations, observations) = runs.entry(id.source().clone()).or_default();
        for step in 0..STEPS {
            let mut observation =
                Observation::empty(id.clone(), newest - Duration::hours(2) * step);
            let hpa = hpa_now - fall * f64::from(step);
            // Both providers send the pressure reduced to sea level.
            observation.pressure_sea_level_hpa = Some(tenth(hpa));
            if id.source().as_str() != "suedtirol-meteo" {
                // GeoSphere sends the pressure at the station with it:
                // Innsbruck's as far under the reduced one as in its
                // recorded reading, which stays as it was recorded, and a
                // made-up station's by Venti's formula at 7.8 °C and its
                // height.
                let at_station = if unknown.is_none() {
                    930.9 / innsbruck_hpa
                } else {
                    let elevation_m = station.elevation_m.unwrap_or_default();
                    1.0 / reduce_to_sea_level(1.0, elevation_m, 7.8)
                };
                observation.temperature_c = Some(7.8);
                observation.pressure_hpa = Some(tenth(hpa * at_station));
            }
            // A station this mode makes up has a wind in its newest
            // reading, as every recorded one has: light, from the south.
            if unknown.is_some() && step == 0 {
                observation.wind_speed_ms = Some(1.2);
                observation.wind_gust_ms = Some(2.1);
                observation.wind_direction_deg = Some(190.0);
            }
            observations.push(observation);
        }
        stations.push(station);
    }
    let started_at = now - Duration::hours(25);
    for (source, (stations, observations)) in runs {
        let run = venti_store::RunRecord {
            source,
            started_at,
            finished_at: started_at,
            healthy: true,
            requests: 0,
            failed_requests: 0,
            skipped: 0,
            rejected: 0,
            stations: u32::try_from(stations.len()).unwrap_or(u32::MAX),
            observations: u32::try_from(observations.len()).unwrap_or(u32::MAX),
            newest_observation: observations.iter().map(|o| o.time).max(),
            airspaces: 0,
        };
        store
            .save_run(&run, &stations, &observations)
            .await
            .map_err(|e| format!("the made-up pressures: {e}"))?;
    }
    Ok(())
}

/// Made-up paragliders with a way behind them, under made-up addresses
/// the made-up directory does not know, for the climbs of the last
/// minutes and the pilots in the air above the Tegernsee: three that
/// circled in one thermal east of the Wallberg until four minutes before
/// `at`, one of them first heard at the take-off; two that circle south
/// of the lake now; one that glides north. They are in no list of
/// aircraft: `GET /v1/live/others` is as it was.
fn made_up_climbers(at: Timestamp) -> Result<Vec<(&'static str, Vec<live::air::Sample>)>, String> {
    use live::air::made_up::{circling, gliding};
    let place = |lat: f64, lon: f64| {
        venti_domain::GeoPoint::new(lat, lon).map_err(|error| error.to_string())
    };
    let ago = |seconds: i64| at - Duration::seconds(seconds);
    let wallberg = place(47.6690, 11.8010)?;
    let south = place(47.6260, 11.7430)?;
    // Twelve minutes ago at the sample flight's take-off, then on its
    // way to the thermal.
    let mut first = gliding(place(47.6642, 11.7967)?, ago(720), 60, 4, 9.0, 1620.0);
    first.extend(circling(wallberg, ago(448), 208, 4, 20.0, 1600.0, 2.4));
    Ok(vec![
        ("E000C1", first),
        (
            "E000C2",
            circling(
                place(47.6693, 11.8013)?,
                ago(408),
                168,
                4,
                22.0,
                1650.0,
                2.4,
            ),
        ),
        (
            "E000C3",
            circling(
                place(47.6688, 11.8006)?,
                ago(368),
                128,
                4,
                18.0,
                1700.0,
                2.4,
            ),
        ),
        (
            "E000C4",
            circling(south, ago(150), 148, 4, 20.0, 1350.0, 1.3),
        ),
        (
            "E000C5",
            circling(place(47.6263, 11.7433)?, ago(100), 96, 4, 24.0, 1400.0, 1.3),
        ),
        (
            "E000C6",
            gliding(place(47.6800, 11.7800)?, ago(200), 196, 4, 10.0, 1900.0),
        ),
    ])
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
    tracing::info!(
        "its token for another program, which may read and write flights only: \
         'Authorization: Bearer {DEMO_ACCESS_TOKEN}'"
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
        assert!(AccessSecret::parse(DEMO_ACCESS_TOKEN).is_some());
        assert!(crate::accounts::secret::is_id(DEMO_ACCESS_TOKEN_ID));
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
