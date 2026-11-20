//! What whoever runs the server does about what is shared on it, from the
//! command line.
//!
//! Pilots report flights, pilots' pages and live tracks through the API
//! ([`crate::api`]). A report is stored and nothing else happens. These
//! subcommands are how the operator reads the reports and acts:
//!
//! - `reports` lists them, `close-report` closes one with a note;
//! - `take-down` makes a flight private and keeps it so, `release` lets
//!   it go;
//! - `remove-live-track` removes a live track and retires its identifier;
//! - `suspend-publishing` takes from an account the ability to make
//!   anything public, `resume-publishing` gives it back.
//!
//! There is no web page for any of this, and the API offers none of it:
//! whoever can run this program on the server's machine, with the server's
//! database, is the operator. Each command does one thing and says in
//! words what it did and what it did not. None deletes a flight or an
//! account, and none tells a pilot anything: what a pilot sees is in the
//! API, as a flight that says since when it is taken down and an account
//! that says since when it may not publish.
//!
//! What a command prints is for the operator's eyes. It names reporters,
//! which the API never does, and the identifiers of live tracks, which are
//! the keys to them: none of it goes to the server's log.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use venti_api_types::v1::{FlightId, Handle, LiveTrackId, live_page_path, share_path};
use venti_domain::Timestamp;
use venti_store::{
    Flight, LiveVisibility, PgStore, Report, ReportClosing, ReportListing, ReportSubject, Store,
    Visibility,
};

use crate::accounts::secret;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;

/// The most characters a closing note may have. It is the operator's own
/// text; the bound is what keeps a slip of the hand out of the table.
pub const MAX_NOTE_LENGTH: usize = 1000;

/// How many reports are read from the store at a time.
const PAGE: usize = 100;

/// One thing the operator asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// List the reports: the open ones, or with `all` the closed ones too.
    Reports {
        /// Whether closed reports are listed as well.
        all: bool,
    },
    /// Close a report with a note.
    CloseReport {
        /// The report's identifier, as `reports` prints it.
        id: String,
        /// What was done, or why nothing was.
        note: String,
    },
    /// Make a flight private and keep it so.
    TakeDown {
        /// The flight's address, or a link to its page.
        flight: String,
    },
    /// Let a flight that was taken down go.
    Release {
        /// The flight's address, or a link to its page.
        flight: String,
    },
    /// Remove a live track and retire its identifier.
    RemoveLiveTrack {
        /// The track's identifier, or a link to its page.
        track: String,
    },
    /// Take from an account the ability to publish.
    SuspendPublishing {
        /// The account's handle.
        handle: String,
    },
    /// Give it back.
    ResumePublishing {
        /// The account's handle.
        handle: String,
    },
}

/// What `venti-server` was asked to do as the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The server's configuration file, if one was named. The database is
    /// taken from it or from the environment, as for `serve`.
    pub config: Option<PathBuf>,
    /// What to do.
    pub action: Action,
}

/// What became of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// What to tell the operator.
    pub said: String,
    /// Whether the command found what it was pointed at. A command that
    /// did not ends with a failure status, so that a script notices.
    pub found: bool,
}

impl Outcome {
    fn found(said: impl Into<String>) -> Self {
        Self {
            said: said.into(),
            found: true,
        }
    }

    fn nothing(said: impl Into<String>) -> Self {
        Self {
            said: said.into(),
            found: false,
        }
    }
}

/// What the commands need to know of the server beside its store.
#[derive(Debug, Clone, Copy)]
pub struct Site<'a> {
    /// The address the server is reached at, if it was configured: links
    /// are printed with it.
    pub public_url: Option<&'a str>,
    /// For how many hours a live track is kept after its newest position.
    pub live_hours: u32,
}

/// Text from a pilot or an account as a terminal may show it on one line:
/// whatever does not print, or would move the cursor, becomes a question
/// mark.
fn on_one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

fn day_and_time(time: Timestamp) -> String {
    time.format("%Y-%m-%d %H:%M UTC").to_string()
}

/// The last part of a link, or the text itself if it is no link: what an
/// operator pastes is a flight's address or the link someone reported.
fn last_part(text: &str) -> &str {
    let text = text.trim();
    let text = text.split(['?', '#']).next().unwrap_or(text);
    let text = text.trim_end_matches('/');
    text.rsplit('/').next().unwrap_or(text)
}

fn who(handle: &str, name: &str) -> String {
    format!("{} ({})", on_one_line(handle), on_one_line(name))
}

fn pilot_of(flight: &Flight) -> String {
    who(&flight.pilot_handle, &flight.pilot_name)
}

fn visibility_in_words(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Unlisted => "shared by link",
        Visibility::Public => "public",
    }
}

/// Does what the operator asked for and says what became of it.
pub async fn perform(
    store: &impl Store,
    clock: &dyn Clock,
    site: Site<'_>,
    action: &Action,
) -> Result<Outcome, String> {
    let failed = |error: venti_store::StoreError| format!("the database failed: {error}");
    match action {
        Action::Reports { all } => list(store, site, *all).await.map_err(failed),
        Action::CloseReport { id, note } => {
            let note = note.trim();
            if note.is_empty() || note.chars().count() > MAX_NOTE_LENGTH {
                return Err(format!(
                    "a note has from 1 to {MAX_NOTE_LENGTH} characters: say what was done, or \
                     why nothing was"
                ));
            }
            if !secret::is_id(id) {
                return Ok(Outcome::nothing(format!(
                    "{:?} is not the identifier of a report; `venti-server reports` prints them",
                    on_one_line(id)
                )));
            }
            // Kept as one line without anything a terminal acts on: it is
            // printed again with every list of all reports.
            let note = on_one_line(note);
            let closing = store.close_report(id, &note, clock.now()).await;
            Ok(match closing.map_err(failed)? {
                ReportClosing::Closed => Outcome::found(format!(
                    "Report {id} is closed. Nothing else was done: closing a report changes \
                     nothing about what was reported."
                )),
                ReportClosing::AlreadyClosed(closed) => Outcome::found(format!(
                    "Report {id} was closed on {} already, with the note: {}\nNothing changed.",
                    day_and_time(closed.at),
                    on_one_line(&closed.note)
                )),
                ReportClosing::NoSuchReport => {
                    Outcome::nothing(format!("There is no report with the identifier {id}."))
                }
            })
        }
        Action::TakeDown { flight } => {
            let Ok(id) = last_part(flight).parse::<FlightId>() else {
                return Ok(Outcome::nothing(no_flight_address(flight)));
            };
            let Some(before) = store.flight(id.as_str()).await.map_err(failed)? else {
                return Ok(Outcome::nothing(format!(
                    "There is no flight with the address {id}."
                )));
            };
            if let Some(since) = before.taken_down_at {
                return Ok(Outcome::found(format!(
                    "Flight {id} of {} was taken down on {} already. Nothing changed.",
                    pilot_of(&before),
                    day_and_time(since)
                )));
            }
            let taken = store.take_down_flight(id.as_str(), clock.now()).await;
            let Some(flight) = taken.map_err(failed)? else {
                return Ok(Outcome::nothing(format!(
                    "Flight {id} was deleted a moment ago. Nothing was taken down."
                )));
            };
            Ok(Outcome::found(format!(
                "Flight {id} of {} is taken down. It was {} and is private now: nobody but \
                 its pilot sees it, its link shows the page for no flight, and it is in no \
                 list.\n\
                 Its pilot cannot share it again, also not by deleting it and uploading the \
                 same file. The server tells the pilot's apps that the flight was taken down \
                 and since when, and not why: it does not know why.\n\
                 Nothing was deleted, and no report was closed. `venti-server release {id}` \
                 undoes this.",
                pilot_of(&flight),
                visibility_in_words(before.visibility),
            )))
        }
        Action::Release { flight } => {
            let Ok(id) = last_part(flight).parse::<FlightId>() else {
                return Ok(Outcome::nothing(no_flight_address(flight)));
            };
            let Some(before) = store.flight(id.as_str()).await.map_err(failed)? else {
                return Ok(Outcome::nothing(format!(
                    "There is no flight with the address {id}. A flight that was deleted \
                     while it was taken down comes back taken down when its file is uploaded \
                     again, and can be released then."
                )));
            };
            if before.taken_down_at.is_none() {
                return Ok(Outcome::found(format!(
                    "Flight {id} of {} is not taken down. Nothing changed.",
                    pilot_of(&before)
                )));
            }
            let released = store.release_flight(id.as_str(), clock.now()).await;
            let Some(flight) = released.map_err(failed)? else {
                return Ok(Outcome::nothing(format!(
                    "Flight {id} was deleted a moment ago. Nothing was released."
                )));
            };
            Ok(Outcome::found(format!(
                "Flight {id} of {} is released: its pilot may share it again. It is still \
                 private, and nobody but its pilot makes it anything else.",
                pilot_of(&flight)
            )))
        }
        Action::RemoveLiveTrack { track } => {
            let Ok(id) = last_part(track).parse::<LiveTrackId>() else {
                return Ok(Outcome::nothing(format!(
                    "{:?} is not the identifier of a live track or a link to one: an \
                     identifier is 26 characters, the end of the link /live/…",
                    on_one_line(track)
                )));
            };
            let gone = format!(
                "There is no live track with this identifier: none ever was, it was removed \
                 or deleted, or it ended more than {} hours ago and is gone.",
                site.live_hours
            );
            let Some(before) = store.live_track(id.as_str()).await.map_err(failed)? else {
                return Ok(Outcome::nothing(gone));
            };
            let removed = store.remove_live_track(id.as_str(), clock.now()).await;
            if !removed.map_err(failed)? {
                return Ok(Outcome::nothing(gone));
            }
            let was = match before.visibility {
                LiveVisibility::Public => "on everyone's map",
                LiveVisibility::Link => "for those with its link",
            };
            Ok(Outcome::found(format!(
                "The live track of {} is removed with its {} positions. It was {was}. Its \
                 identifier takes nothing more, so its link shows no track from now on.\n\
                 This does not stop the pilot's app: it is told that the identifier is used \
                 up and goes on under a new one. To keep the account's gliders off \
                 everyone's map, `venti-server suspend-publishing {}`.",
                who(&before.pilot_handle, &before.pilot_name),
                before.fixes,
                on_one_line(&before.pilot_handle),
            )))
        }
        Action::SuspendPublishing { handle } => {
            let Some(account) = account(store, handle).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_account(handle)));
            };
            let suspended = store.suspend_publishing(&account.id, clock.now()).await;
            let Some(suspension) = suspended.map_err(failed)? else {
                return Ok(Outcome::nothing(no_account(handle)));
            };
            let pilot = who(&account.handle, &account.display_name);
            let what = if account.publishing_suspended_at.is_some() {
                format!(
                    "{pilot} may not publish since {} already.",
                    day_and_time(suspension.since)
                )
            } else {
                format!("{pilot} may not publish from now on.")
            };
            Ok(Outcome::found(format!(
                "{what}\n  \
                 Public flights that are for those with their link now: {}\n  \
                 Live tracks taken off everyone's map, for those with their link now: {}\n\
                 None of it is in a list any more. Whoever has a link still sees what the \
                 link shows.\n\
                 The account can make no flight public, and its live tracks are shared by \
                 link whatever its app asks for. Everything else works: signing in, the \
                 logbook, sharing by link. The server tells the pilot's apps that publishing \
                 is suspended and since when, and not why.\n\
                 Nothing was deleted. To take one flight away from those who have its link \
                 as well, `venti-server take-down`. `venti-server resume-publishing {}` \
                 undoes this; it makes nothing public again.",
                suspension.flights, suspension.live_tracks, account.handle,
            )))
        }
        Action::ResumePublishing { handle } => {
            let Some(account) = account(store, handle).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_account(handle)));
            };
            let pilot = who(&account.handle, &account.display_name);
            let resumed = store.resume_publishing(&account.id).await;
            Ok(match resumed.map_err(failed)? {
                None => Outcome::nothing(no_account(handle)),
                Some(false) => Outcome::found(format!(
                    "{pilot} was not suspended from publishing. Nothing changed."
                )),
                Some(true) => Outcome::found(format!(
                    "{pilot} may publish again. Nothing was made public: what the \
                     suspension made unlisted stays so until its pilot publishes it."
                )),
            })
        }
    }
}

fn no_flight_address(text: &str) -> String {
    format!(
        "{:?} is not the address of a flight or a link to one: an address is 26 characters, \
         the end of the link /f/…",
        on_one_line(text)
    )
}

fn no_account(handle: &str) -> String {
    format!(
        "There is no account with the handle {:?}.",
        on_one_line(handle)
    )
}

/// The account behind a handle as the operator typed it.
async fn account(
    store: &impl Store,
    handle: &str,
) -> Result<Option<venti_store::Account>, venti_store::StoreError> {
    let Ok(handle) = Handle::new(handle.trim().trim_start_matches('@')) else {
        return Ok(None);
    };
    store.account(handle.as_str()).await
}

/// The reports, oldest first, as text.
async fn list(
    store: &impl Store,
    site: Site<'_>,
    all: bool,
) -> Result<Outcome, venti_store::StoreError> {
    let listing = if all {
        ReportListing::All
    } else {
        ReportListing::Open
    };
    let mut reports: Vec<Report> = Vec::new();
    loop {
        let after = reports.last().map(Report::key);
        let page = store.reports(listing, after.as_ref(), PAGE).await?;
        let last_page = page.len() < PAGE;
        reports.extend(page);
        if last_page {
            break;
        }
    }

    let open = reports.iter().filter(|r| r.closed.is_none()).count();
    let mut said = match (all, reports.len(), open) {
        (false, 0, _) => "No report is open.".to_owned(),
        (false, 1, _) => "1 report is open.".to_owned(),
        (false, n, _) => format!("{n} reports are open, oldest first."),
        (true, 0, _) => "There are no reports.".to_owned(),
        (true, n, open) => format!("{n} reports, {open} of them open, oldest first."),
    };
    for report in &reports {
        said.push_str("\n\n");
        said.push_str(&described(store, site, report).await?);
    }
    if !reports.is_empty() {
        said.push_str(
            "\n\nA report is one pilot's word about another. Nothing was done about any of \
             them: see `venti-server --help` for take-down, remove-live-track, \
             suspend-publishing and close-report.",
        );
    }
    Ok(Outcome::found(said))
}

/// One report as a block of lines, with what its subject is now.
async fn described(
    store: &impl Store,
    site: Site<'_>,
    report: &Report,
) -> Result<String, venti_store::StoreError> {
    let about = who(&report.about_handle, &report.about_name);
    let link = |path: String| site.public_url.map(|url| format!("{url}{path}"));
    let (subject, address) = match &report.subject {
        ReportSubject::Flight(id) => {
            let now = match store.flight(id).await? {
                Some(flight) if flight.taken_down_at.is_some() => "taken down",
                Some(flight) => visibility_in_words(flight.visibility),
                None => "deleted",
            };
            let address = id.parse::<FlightId>().ok().map(|id| share_path(&id));
            (
                format!("flight {} of {about}; now {now}", on_one_line(id)),
                address.and_then(link),
            )
        }
        ReportSubject::Pilot => {
            let suspended = store
                .account(&report.about_handle)
                .await?
                .is_some_and(|account| account.publishing_suspended_at.is_some());
            let now = if suspended {
                "; publishing is suspended"
            } else {
                ""
            };
            (format!("the page of {about}{now}"), None)
        }
        ReportSubject::LiveTrack(id) => {
            let now = match store.live_track(id).await? {
                Some(track) if track.visibility == LiveVisibility::Public => {
                    "now on everyone's map"
                }
                Some(_) => "now for those with its link",
                None => "gone",
            };
            let address = id.parse::<LiveTrackId>().ok().map(|id| live_page_path(&id));
            (
                format!("live track {} of {about}; {now}", on_one_line(id)),
                address.and_then(link),
            )
        }
    };

    let state = match &report.closed {
        None => "open".to_owned(),
        Some(closed) => format!("closed {}", day_and_time(closed.at)),
    };
    let mut block = format!(
        "Report {}, {state}, made {}\n  About:        {subject}",
        report.id,
        day_and_time(report.created_at)
    );
    if let Some(address) = address {
        let _ = write!(block, "\n                {address}");
    }
    let _ = write!(block, "\n  Reason:       {}", on_one_line(&report.reason));
    if let Some(text) = &report.text {
        // The reporter's own words, line by line, and nothing in them
        // that a terminal acts on.
        let mut lines = text.lines();
        if let Some(first) = lines.next() {
            let _ = write!(block, "\n  Text:         {}", on_one_line(first));
        }
        for line in lines {
            let _ = write!(block, "\n                {}", on_one_line(line));
        }
    }
    let _ = write!(
        block,
        "\n  Reported by:  {}",
        on_one_line(&report.reporter_handle)
    );
    if let Some(closed) = &report.closed {
        let _ = write!(block, "\n  Note:         {}", on_one_line(&closed.note));
    }
    Ok(block)
}

/// Runs one of the operator's subcommands against the server's database:
/// prints what became of it on standard output.
pub async fn run(args: Args) -> ExitCode {
    let config = match Config::load(args.config.as_deref()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let store = match PgStore::connect(config.database_url.expose()).await {
        Ok(store) => store,
        Err(error) => {
            eprintln!("cannot open the database: {error}");
            return ExitCode::FAILURE;
        }
    };
    let site = Site {
        public_url: config.public_url.as_ref().map(|url| url.as_str()),
        live_hours: config.retention.live_hours,
    };
    match perform(&store, &SystemClock, site, &args.action).await {
        Ok(outcome) => {
            println!("{}", outcome.said);
            if outcome.found {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
