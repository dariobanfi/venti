//! What whoever runs the server does about what is shared on it, from the
//! command line.
//!
//! Pilots report flights, pilots' pages, live tracks, groups and the plans
//! of groups through the API ([`crate::api`]). A report is stored and
//! nothing else happens. These subcommands are how the operator reads the
//! reports and acts:
//!
//! - `reports` lists them, `close-report` closes one with a note;
//! - `take-down` makes a flight private and keeps it so, `release` lets
//!   it go;
//! - `remove-live-track` removes a live track and retires its identifier;
//! - `suspend-publishing` takes from an account the ability to make
//!   anything public, `resume-publishing` gives it back;
//! - `group` shows one group with its members and its plans,
//!   `rename-group` gives it another name, `remove-plan` removes one of
//!   its plans and `remove-group` the group with everything in it.
//!
//! There is no web page for any of this, and the API offers none of it:
//! whoever can run this program on the server's machine, with the server's
//! database, is the operator. Each command does one thing and says in
//! words what it did and what it did not. None deletes a flight or an
//! account, and none tells a pilot anything: what a pilot sees is in the
//! API, as a flight that says since when it is taken down and an account
//! that says since when it may not publish.
//!
//! What is done about a group is the keeper's own doing, done by someone
//! else, as removing a live track is its pilot's own deletion: the name is
//! another, the plan or the group is gone, and nothing in the API says by
//! whom. A group that is gone leaves nothing behind that could say so, and
//! the server keeps no list of who was in one.
//!
//! What a command prints is for the operator's eyes. It names reporters,
//! which the API never does, the identifiers of live tracks, which are the
//! keys to them, and who is in a group and where its members mean to be,
//! which is nobody's business but theirs: none of it goes to the server's
//! log.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;
use venti_api_types::v1::{
    FlightId, GroupId, GroupTextError, Handle, LiveTrackId, PlanId, format_day, live_page_path,
    parse_group_name, share_path,
};
use venti_domain::Timestamp;
use venti_store::{
    Flight, GroupMembers, GroupPilot, LiveVisibility, PgStore, Plan, Report, ReportClosing,
    ReportListing, ReportSubject, Store, Visibility,
};

use crate::accounts::secret;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::groups;

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
    /// Show one group, with its members and its plans.
    Group {
        /// The group's identifier.
        group: String,
    },
    /// Give a group another name.
    RenameGroup {
        /// The group's identifier.
        group: String,
        /// The name it is to have.
        name: String,
    },
    /// Remove one plan of a group, with who was in on it.
    RemovePlan {
        /// The plan's identifier.
        plan: String,
    },
    /// Remove a group, with its members' places in it and all its plans.
    RemoveGroup {
        /// The group's identifier.
        group: String,
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
    /// For how many days a plan is kept after its day. A plan that the
    /// API no longer shows is no plan to these commands either.
    pub plan_days: u32,
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

fn member(pilot: &GroupPilot) -> String {
    who(&pilot.handle, &pilot.display_name)
}

/// A name, a place or another line a pilot typed, between quotation marks
/// for a sentence.
fn quoted(text: &str) -> String {
    format!("\"{}\"", on_one_line(text))
}

/// How wide the names in front of a report's and a group's lines are.
const LABEL_WIDTH: usize = 14;

/// Adds text a pilot typed to a block under a label, line by line, and
/// nothing in it that a terminal acts on.
fn labelled(block: &mut String, label: &str, text: &str) {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let _ = write!(block, "\n  {label:<LABEL_WIDTH$}{}", on_one_line(first));
    for line in lines {
        let _ = write!(block, "\n  {:<LABEL_WIDTH$}{}", "", on_one_line(line));
    }
}

fn counted(count: usize, one: &str, several: &str) -> String {
    match count {
        1 => format!("1 {one}"),
        count => format!("{count} {several}"),
    }
}

/// How many pilots are in on a plan, or were: the number and not who.
fn in_on_it(plan: &Plan, still: bool) -> String {
    match (plan.pilots_in.len(), still) {
        (0, true) => "nobody is in".to_owned(),
        (0, false) => "nobody was in on it".to_owned(),
        (1, true) => "1 pilot is in".to_owned(),
        (1, false) => "1 pilot was in on it".to_owned(),
        (count, true) => format!("{count} pilots are in"),
        (count, false) => format!("{count} pilots were in on it"),
    }
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
    // The first day whose plans are still there, as the API reckons it.
    let gone_before = groups::gone_before(clock.now(), site.plan_days);
    match action {
        Action::Reports { all } => list(store, site, gone_before, *all).await.map_err(failed),
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
        Action::Group { group } => {
            let Ok(id) = group.trim().parse::<GroupId>() else {
                return Ok(Outcome::nothing(no_group_identifier(group)));
            };
            let Some(seen) = store.any_group(id.as_str()).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_group(&id)));
            };
            let plans = store.plans_of_any_group(id.as_str(), gone_before).await;
            // A group that went between the two questions has no plans.
            let plans = plans.map_err(failed)?.unwrap_or_default();
            Ok(Outcome::found(shown_group(&seen, &plans)))
        }
        Action::RenameGroup { group, name } => {
            let name = parse_group_name(name).map_err(|error| match error {
                GroupTextError::Empty => "a group's name must not be empty".to_owned(),
                GroupTextError::TooLong(most) => {
                    format!("a group's name is one line of at most {most} characters")
                }
            })?;
            let Ok(id) = group.trim().parse::<GroupId>() else {
                return Ok(Outcome::nothing(no_group_identifier(group)));
            };
            let Some(before) = store.any_group(id.as_str()).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_group(&id)));
            };
            if before.group.name == name {
                return Ok(Outcome::found(format!(
                    "Group {id} is called {} already. Nothing changed.",
                    quoted(&name)
                )));
            }
            let renamed = store.rename_any_group(id.as_str(), &name).await;
            let Some(now) = renamed.map_err(failed)? else {
                return Ok(Outcome::nothing(format!(
                    "Group {id} was deleted a moment ago. Nothing was renamed."
                )));
            };
            Ok(Outcome::found(format!(
                "Group {id}, kept by {}, was called {} and is called {} now, for its {} and \
                 for nobody else.\n\
                 Nobody is told, and nothing says who renamed it: the members' apps show the \
                 new name when they next ask. Its keeper can rename it again; against a \
                 keeper who names it back, `venti-server remove-group {id}` is what there \
                 is.\n\
                 Nothing else changed: its members, its plans and its invite code are as \
                 they were, and no report was closed.",
                member(&now.group.keeper),
                quoted(&before.group.name),
                quoted(&now.group.name),
                counted(now.members.len(), "member", "members"),
            )))
        }
        Action::RemovePlan { plan } => {
            let Ok(id) = plan.trim().parse::<PlanId>() else {
                return Ok(Outcome::nothing(format!(
                    "{:?} is not the identifier of a plan: an identifier is 26 characters, as \
                     `venti-server reports` and `venti-server group` print it",
                    on_one_line(plan)
                )));
            };
            let removed = store.remove_plan(id.as_str(), gone_before).await;
            let Some(plan) = removed.map_err(failed)? else {
                return Ok(Outcome::nothing(format!(
                    "There is no plan with the identifier {id}: none ever was, it was \
                     deleted or removed, it went with its author or its group, or its day is \
                     more than {} days gone by.",
                    site.plan_days
                )));
            };
            Ok(Outcome::found(format!(
                "The plan {id} of {} in group {} is removed: for {}, at {}; {}.\n\
                 To the members of the group it is as if its author had deleted it: it is in \
                 no list, and nobody is told that it was removed or by whom.\n\
                 This cannot be undone. It does not stop its author from posting it again; \
                 the group's keeper can remove a member from the group. Nothing else of the \
                 group changed, and no report was closed.",
                member(&plan.author),
                on_one_line(&plan.group_id),
                format_day(plan.details.day),
                quoted(&plan.details.place),
                in_on_it(&plan, false),
            )))
        }
        Action::RemoveGroup { group } => {
            let Ok(id) = group.trim().parse::<GroupId>() else {
                return Ok(Outcome::nothing(no_group_identifier(group)));
            };
            let removed = store.remove_group(id.as_str(), gone_before).await;
            let Some(removed) = removed.map_err(failed)? else {
                return Ok(Outcome::nothing(no_group(&id)));
            };
            let was = &removed.group;
            Ok(Outcome::found(format!(
                "Group {id} {}, kept by {}, is removed, with the places of its {} in it and \
                 its {}. Its invite code lets nobody in.\n\
                 To those who were in it, it is as if its keeper had deleted it: it is in \
                 none of their lists, and nobody is told that it was removed or by whom.\n\
                 This cannot be undone. It does not stop anyone from making a group again. \
                 No account was touched, and no report was closed.",
                quoted(&was.group.name),
                member(&was.group.keeper),
                counted(was.members.len(), "member", "members"),
                counted(removed.plans as usize, "plan", "plans"),
            )))
        }
    }
}

fn no_group_identifier(text: &str) -> String {
    format!(
        "{:?} is not the identifier of a group: an identifier is 26 characters, as \
         `venti-server reports` prints it",
        on_one_line(text)
    )
}

fn no_group(id: &GroupId) -> String {
    format!(
        "There is no group with the identifier {id}: none ever was, its keeper deleted it, \
         its last member left, or it was removed."
    )
}

/// When a plan is for, and when its pilots meet if it says.
fn day_of(plan: &Plan) -> String {
    let day = format_day(plan.details.day);
    match plan.details.meet_at {
        Some(time) => format!("{day} at {}", time.format("%H:%M")),
        None => day,
    }
}

/// One group as its members see it, as text: who is in it, and its plans
/// with what they say. Of who is in on a plan it says how many.
fn shown_group(seen: &GroupMembers, plans: &[Plan]) -> String {
    let group = &seen.group;
    let mut said = format!(
        "Group {}, made {}",
        on_one_line(&group.id),
        day_and_time(group.created_at)
    );
    labelled(&mut said, "Name:", &group.name);
    labelled(&mut said, "Kept by:", &member(&group.keeper));
    let members: Vec<String> = seen.members.iter().map(member).collect();
    labelled(
        &mut said,
        "Members:",
        &format!("{}, in the order they came in", members.len()),
    );
    labelled(&mut said, "", &members.join("\n"));
    let how_many = match plans.len() {
        0 => "none".to_owned(),
        count => count.to_string(),
    };
    labelled(&mut said, "Plans:", &how_many);
    for plan in plans {
        let _ = write!(
            said,
            "\n\nPlan {}, for {}, posted by {}; {}",
            on_one_line(&plan.id),
            day_of(plan),
            member(&plan.author),
            in_on_it(plan, true),
        );
        labelled(&mut said, "Place:", &plan.details.place);
        if let Some(position) = plan.details.position {
            let degrees = format!("{:.5}, {:.5}", position.lat(), position.lon());
            labelled(&mut said, "Position:", &degrees);
        }
        if let Some(note) = &plan.details.note {
            labelled(&mut said, "Note:", note);
        }
    }
    said.push_str(
        "\n\nThis is what the members of the group see, and nobody else does. Nothing was \
         changed: see `venti-server --help` for rename-group, remove-plan and remove-group.",
    );
    said
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
    gone_before: NaiveDate,
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
        said.push_str(&described(store, site, gone_before, report).await?);
    }
    if !reports.is_empty() {
        said.push_str(
            "\n\nA report is one pilot's word about another. Nothing was done about any of \
             them: see `venti-server --help` for take-down, remove-live-track, \
             suspend-publishing, group, rename-group, remove-plan, remove-group and \
             close-report.",
        );
    }
    Ok(Outcome::found(said))
}

/// What a group that was reported is now, in a line.
async fn group_now(
    store: &impl Store,
    id: &str,
    name_then: &str,
) -> Result<String, venti_store::StoreError> {
    let Some(seen) = store.any_group(id).await? else {
        return Ok("gone".to_owned());
    };
    let name = if seen.group.name == name_then {
        "the same name".to_owned()
    } else {
        format!("called {}", quoted(&seen.group.name))
    };
    Ok(format!(
        "{name}; kept by {}, {}",
        member(&seen.group.keeper),
        counted(seen.members.len(), "member", "members"),
    ))
}

/// What a plan that was reported is now, in a line; and its words as they
/// are, if they are no longer what was reported.
async fn plan_now(
    store: &impl Store,
    gone_before: NaiveDate,
    (id, group_id): (&str, &str),
    (place_then, note_then): (&str, Option<&str>),
) -> Result<(String, Option<Plan>), venti_store::StoreError> {
    let group = match store.any_group(group_id).await? {
        Some(seen) => format!("its group is called {}", quoted(&seen.group.name)),
        None => "its group is gone".to_owned(),
    };
    let Some(plan) = store.any_plan(id, gone_before).await? else {
        return Ok((format!("gone; {group}"), None));
    };
    let same = plan.details.place == place_then && plan.details.note.as_deref() == note_then;
    let (words, changed) = if same {
        ("the same words", None)
    } else {
        ("other words", Some(plan.clone()))
    };
    Ok((format!("{words}, for {}; {group}", day_of(&plan)), changed))
}

/// One report as a block of lines, with what its subject is now.
async fn described(
    store: &impl Store,
    site: Site<'_>,
    gone_before: NaiveDate,
    report: &Report,
) -> Result<String, venti_store::StoreError> {
    let about = who(&report.about_handle, &report.about_name);
    let link = |path: String| site.public_url.map(|url| format!("{url}{path}"));
    // What a report about a group or a plan kept of it, and what it is
    // now: lines of their own under the first.
    let mut kept = String::new();
    let (subject, address) = match &report.subject {
        ReportSubject::Group { id, name } => {
            labelled(&mut kept, "Name then:", name);
            labelled(&mut kept, "Now:", &group_now(store, id, name).await?);
            (
                format!(
                    "group {}, which {about} kept when it was reported",
                    on_one_line(id)
                ),
                None,
            )
        }
        ReportSubject::Plan {
            id,
            group_id,
            place,
            note,
        } => {
            labelled(&mut kept, "Place then:", place);
            if let Some(note) = note {
                labelled(&mut kept, "Note then:", note);
            }
            let words = (place.as_str(), note.as_deref());
            let (now, changed) = plan_now(store, gone_before, (id, group_id), words).await?;
            labelled(&mut kept, "Now:", &now);
            if let Some(plan) = changed {
                labelled(&mut kept, "Place now:", &plan.details.place);
                if let Some(note) = &plan.details.note {
                    labelled(&mut kept, "Note now:", note);
                }
            }
            (
                format!(
                    "plan {} of {about}, in group {}",
                    on_one_line(id),
                    on_one_line(group_id)
                ),
                None,
            )
        }
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
    block.push_str(&kept);
    labelled(&mut block, "Reason:", &report.reason);
    if let Some(text) = &report.text {
        // The reporter's own words.
        labelled(&mut block, "Text:", text);
    }
    labelled(&mut block, "Reported by:", &report.reporter_handle);
    if let Some(closed) = &report.closed {
        labelled(&mut block, "Note:", &closed.note);
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
        plan_days: config.retention.plan_days,
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
