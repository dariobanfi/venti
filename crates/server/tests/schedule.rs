//! `docs/schedule.md`: everything the server does by itself, on a clock or
//! after something else it does, with how often and how soon again after a
//! failure. Written from the manifests and from the constants of the loops,
//! so that a changed cadence or interval fails this test until the page is
//! written again with VENTI_UPDATE_GOLDEN=1.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use venti_server::accounts::Limits;
use venti_server::ingest::schedule::{JITTER_DIVISOR, Pace};
use venti_server::push::PushSettings;
use venti_server::push::phones::PhoneSettings;
use venti_server::{alerts, api, flights, live, maintenance};
use venti_sources::{Kind, Manifest};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every manifest of the repository, with where it is, sorted by id.
fn manifests() -> Vec<(Manifest, String)> {
    let mut found = Vec::new();
    for crate_dir in std::fs::read_dir(repository().join("crates")).unwrap() {
        let crate_dir = crate_dir.unwrap().path();
        let crate_name = crate_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if !crate_name.starts_with("sources-") {
            continue;
        }
        for file in std::fs::read_dir(&crate_dir).unwrap() {
            let file = file.unwrap().path();
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            if !(name.starts_with("manifest") && name.ends_with(".toml")) {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap();
            let manifest = Manifest::from_toml(&text)
                .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
            found.push((manifest, format!("crates/{crate_name}/{name}")));
        }
    }
    found.sort_by(|a, b| a.0.id.as_str().cmp(b.0.id.as_str()));
    assert!(!found.is_empty());
    found
}

/// A length of time as the page says it: "30 s", "10 min", "6 h", "2 days".
fn said(time: Duration) -> String {
    let seconds = time.as_secs();
    match seconds {
        0 => "at once".to_owned(),
        s if s % 86_400 == 0 && s >= 2 * 86_400 => format!("{} days", s / 86_400),
        s if s % 3600 == 0 => format!("{} h", s / 3600),
        s if s % 60 == 0 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

fn said_chrono(time: chrono::Duration) -> String {
    said(time.to_std().unwrap())
}

/// The waits after one failed run, two, three, ... up to where they stop
/// growing: "6 h, 12 h, 24 h, 2 days, 4 days, then 7 days".
fn waits_after_failures(pace: &Pace) -> String {
    let mut waits: Vec<Duration> = Vec::new();
    for failures in 1..=24 {
        let wait = pace.next_delay(failures, 0.0);
        if waits.last() != Some(&wait) {
            waits.push(wait);
        }
    }
    let mut words: Vec<String> = waits.iter().map(|wait| said(*wait)).collect();
    if words.len() > 1 {
        let last = words.pop().unwrap();
        format!("{}, then {last}", words.join(", "))
    } else {
        words.remove(0)
    }
}

/// What a manifest asks of the provider's limits, in a few words.
fn limits(manifest: &Manifest) -> String {
    let limit = &manifest.rate_limit;
    let mut said_of = Vec::new();
    if let Some(per_second) = limit.per_second {
        said_of.push(format!("{per_second} a second"));
    }
    if let Some(per_hour) = limit.per_hour {
        said_of.push(format!("{per_hour} an hour"));
    }
    // The forecasts' hour is shared by the look, the thermal map and what
    // pilots ask for (`crates/server/src/forecast.rs`): no budget of a run.
    let shared = manifest.kinds.contains(&Kind::Forecasts);
    if let Some(budget) = manifest.requests_per_run().filter(|_| !shared) {
        said_of.push(format!("{budget} a run at most"));
    }
    if let Some(pause) = manifest.pause_between() {
        said_of.push(format!("{} between two", said(pause)));
    }
    if let Some(again) = manifest.retry_after() {
        said_of.push(format!(
            "a refused request asked again after {}",
            said(again)
        ));
    }
    if said_of.is_empty() {
        "none stated".to_owned()
    } else {
        said_of.join(", ")
    }
}

/// Whether the scheduler runs sources of this kind.
fn on_the_schedule(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Observations
            | Kind::Airspace
            | Kind::Sites
            | Kind::Cables
            | Kind::Forecasts
            | Kind::Radar
    )
}

fn kind_words(kind: Kind) -> &'static str {
    match kind {
        Kind::Observations => "station readings",
        Kind::Airspace => "airspace",
        Kind::Forecasts => "a look at the forecasts",
        Kind::Sites => "flying sites",
        Kind::Cables => "cables",
        Kind::Radar => "rain radar",
        Kind::Imagery => "map imagery",
        Kind::Live => "live positions",
        Kind::Boundaries => "boundaries",
    }
}

fn row(page: &mut String, cells: &[&str]) {
    writeln!(page, "| {} |", cells.join(" | ")).unwrap();
}

fn page() -> String {
    let all = manifests();
    let forecasts = all
        .iter()
        .find(|(manifest, _)| manifest.id.as_str() == "open-meteo")
        .map(|(manifest, _)| manifest)
        .expect("the thermal map's source has a manifest");
    let look = said(Pace::of(forecasts).next_delay(0, 0.0));
    let push = PushSettings::new("venti", None).retries;
    let phones = PhoneSettings::new("venti").retries;
    assert_eq!(push, phones, "the page says both with one line");
    let push_retries: Vec<String> = push.iter().map(|wait| said(*wait)).collect();
    let session_idle = said_chrono(Limits::default().session_idle);
    let daily = said(maintenance::INTERVAL);
    let hourly = said(maintenance::LIVE_INTERVAL);

    let mut page = String::new();
    page.push_str(
        "# What runs by itself\n\n\
         Written by `crates/server/tests/schedule.rs` from the manifests and the server's \
         constants; do not edit. After a change: \
         `VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server --test schedule`.\n\n\
         There is no cron: one server process does all of this, each job in a loop of its own. \
         A wait is from the start of one run to the start of the next. \
         [running-a-server.md](running-a-server.md#what-runs-by-itself) says how to look at it.\n\n",
    );

    page.push_str("## Data sources\n\n");
    writeln!(
        page,
        "A source runs when a region pack names it (`regions/*/region.toml`), first when the \
         server starts unless its latest run was more recent than its wait, then at its \
         cadence. After a failed run it waits the time below, doubling with each further \
         failure; a restart keeps to it. Every wait has up to a {} of it added at random. The \
         cadence and the limits are set in the source's manifest, the wait after a failure by \
         `rate_limit.retry_failed_run_after_hours` there or else by \
         `crates/server/src/ingest/schedule.rs`.\n",
        match JITTER_DIVISOR {
            10 => "tenth".to_owned(),
            n => format!("1/{n}"),
        }
    )
    .unwrap();
    row(
        &mut page,
        &[
            "Source",
            "What",
            "Every",
            "After a failed run",
            "Provider's limits",
            "Manifest",
        ],
    );
    row(&mut page, &["---"; 6]);
    let mut elsewhere = Vec::new();
    for (manifest, path) in &all {
        let kinds: Vec<&str> = manifest
            .kinds
            .iter()
            .map(|kind| kind_words(*kind))
            .collect();
        if !manifest.kinds.iter().any(|kind| on_the_schedule(*kind)) {
            elsewhere.push((manifest, path, kinds.join(", ")));
            continue;
        }
        let pace = Pace::of(manifest);
        row(
            &mut page,
            &[
                &format!("`{}`", manifest.id),
                &kinds.join(", "),
                &said(pace.next_delay(0, 0.0)),
                &waits_after_failures(&pace),
                &limits(manifest),
                &format!("`{path}`"),
            ],
        );
    }
    page.push_str("\nNot on the schedule: ");
    let words: Vec<String> = elsewhere
        .iter()
        .map(|(manifest, _, kinds)| {
            let how = if manifest.kinds.contains(&Kind::Imagery) {
                "fetched when an app asks for a tile"
            } else if manifest.kinds.contains(&Kind::Live) {
                "a connection held open, below"
            } else {
                "built into the server, never fetched"
            };
            format!("`{}` ({kinds}: {how})", manifest.id)
        })
        .collect();
    page.push_str(&words.join("; "));
    page.push_str(".\n\n");

    page.push_str("## The server's own jobs\n\n");
    row(
        &mut page,
        &["Job", "When", "After a failure", "Where it is set"],
    );
    row(&mut page, &["---"; 4]);
    let jobs: Vec<[String; 4]> = vec![
        [
            "Makes the thermal map".to_owned(),
            format!(
                "with each new run of its models, which the look at `open-meteo` notices every {look}"
            ),
            format!("goes on at the next look, {look} later"),
            "`crates/server/src/thermals.rs`".to_owned(),
        ],
        [
            "Checks the wind alarms".to_owned(),
            "after each run of a source of station readings".to_owned(),
            "the next run".to_owned(),
            "`crates/server/src/alerts.rs`".to_owned(),
        ],
        [
            "Checks the day alarms".to_owned(),
            "after each thermal map".to_owned(),
            "the next map".to_owned(),
            "`crates/server/src/alerts.rs`".to_owned(),
        ],
        [
            "Checks the alarms about pilots in the air".to_owned(),
            format!("every {}", said(alerts::AIR_CHECK_EVERY)),
            "the next check".to_owned(),
            "`AIR_CHECK_EVERY`, `crates/server/src/alerts.rs`".to_owned(),
        ],
        [
            "Sends a fired alarm to browsers and phones".to_owned(),
            "when an alarm fires".to_owned(),
            format!(
                "tried again after {}, then given up",
                push_retries.join(" and after ")
            ),
            "`PushSettings::new`, `crates/server/src/push/mod.rs`; `PhoneSettings::new`, \
             `crates/server/src/push/phones.rs`"
                .to_owned(),
        ],
        [
            "Reads the public flights for where thermals were".to_owned(),
            format!(
                "every {}, at once again while new flights wait",
                said(flights::flown::EVERY)
            ),
            format!("{} later", said(flights::flown::EVERY)),
            "`EVERY`, `crates/server/src/flights/flown.rs`".to_owned(),
        ],
        [
            "Listens to the live network (`ogn`)".to_owned(),
            format!(
                "a connection held open, a keepalive every {}, dropped after {} of silence",
                said(live::network::KEEPALIVE),
                said(live::network::SILENCE)
            ),
            format!(
                "connects again after {}, doubling up to {}",
                said(live::network::FIRST_BACKOFF),
                said(live::network::LONGEST_BACKOFF)
            ),
            "`crates/server/src/live/network.rs`".to_owned(),
        ],
        [
            "Fetches the live network's directory of devices".to_owned(),
            format!(
                "at the start, then every {}",
                said(live::network::DIRECTORY_EVERY)
            ),
            format!(
                "{} later; after {} without one nothing is listed",
                said(live::network::DIRECTORY_RETRY),
                said_chrono(live::network::DIRECTORY_STALE)
            ),
            "`crates/server/src/live/network.rs`".to_owned(),
        ],
        [
            "Removes old observations and archived payloads".to_owned(),
            format!("at the start, then every {daily}"),
            format!("the next time, {daily} later"),
            "`[retention]` `observation_months`, `archive_days`; `crates/server/src/maintenance.rs`"
                .to_owned(),
        ],
        [
            "Removes sessions that were not used".to_owned(),
            format!("at the start, then every {daily}"),
            format!("the next time, {daily} later"),
            format!(
                "unused for {session_idle}: `Limits::session_idle`, `crates/server/src/accounts/mod.rs`"
            ),
        ],
        [
            "Removes closed reports, and logs how many are open".to_owned(),
            format!("at the start, then every {daily}"),
            format!("the next time, {daily} later"),
            "`[retention]` `report_days`".to_owned(),
        ],
        [
            "Removes what the authorization server no longer needs".to_owned(),
            format!("at the start, then every {daily}"),
            format!("the next time, {daily} later"),
            format!(
                "programs unused for {}: `CLIENT_IDLE_LIFE`, `crates/server/src/accounts/oauth.rs`",
                said_chrono(venti_server::accounts::oauth::CLIENT_IDLE_LIFE)
            ),
        ],
        [
            "Removes fired alarms and notices".to_owned(),
            format!("at the start, then every {daily}"),
            format!("the next time, {daily} later"),
            format!(
                "alarms fired {} days ago, notices of {} days ago: `FIRED_KEPT_DAYS`, \
                 `NOTIFICATIONS_KEPT_DAYS`, `crates/api-types`",
                venti_api_types::v1::FIRED_KEPT_DAYS,
                venti_api_types::v1::NOTIFICATIONS_KEPT_DAYS
            ),
        ],
        [
            "Removes ended live tracks".to_owned(),
            format!("at the start, then every {hourly}"),
            format!("the next time, {hourly} later"),
            "`[retention]` `live_hours`".to_owned(),
        ],
        [
            "Removes the plans of days gone by".to_owned(),
            format!("at the start, then every {hourly}"),
            format!("the next time, {hourly} later"),
            "`[retention]` `plan_days`".to_owned(),
        ],
        [
            "Removes pilots' positions and ended requests for help".to_owned(),
            format!("at the start, then every {hourly}"),
            format!("the next time, {hourly} later"),
            format!(
                "positions older than {}, requests ended {} ago: `POSITION_KEPT_MINUTES`, \
                 `HELP_KEPT_HOURS`",
                said(Duration::from_secs(
                    60 * venti_api_types::v1::POSITION_KEPT_MINUTES.unsigned_abs()
                )),
                said(Duration::from_secs(
                    3600 * api::HELP_KEPT_HOURS.unsigned_abs()
                ))
            ),
        ],
        [
            "Works stored flights out again after the analysis changed".to_owned(),
            "once, at the start".to_owned(),
            "the next start".to_owned(),
            "`crates/server/src/flights/refresh.rs`".to_owned(),
        ],
        [
            "Reads and checks the map regions".to_owned(),
            "once, at the start".to_owned(),
            "the next start".to_owned(),
            "`maps_dir`; `crates/server/src/maps.rs`".to_owned(),
        ],
        [
            "Trims the imagery cache".to_owned(),
            "at the start, and when it holds more than its size".to_owned(),
            "the next tile kept".to_owned(),
            "`imagery_cache_mb`; `crates/server/src/imagery.rs`".to_owned(),
        ],
    ];
    for job in &jobs {
        let cells: Vec<&str> = job.iter().map(String::as_str).collect();
        row(&mut page, &cells);
    }
    page
}

#[test]
fn the_page_of_what_runs_by_itself_is_up_to_date() {
    let path = repository().join("docs/schedule.md");
    let page = page();
    if std::env::var_os("VENTI_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &page).unwrap();
        return;
    }
    let written = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        written == page,
        "docs/schedule.md is not what the manifests and the server's constants say; \
         write it again with VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server --test schedule \
         and look at the diff"
    );
}
