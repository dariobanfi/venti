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
//!   its plans and `remove-group` the group with everything in it;
//! - `site` shows one site of Venti's own two layers with its whole
//!   history, `revert-site` puts it back to what an earlier revision
//!   said, and `remove-site` takes it down;
//! - `grant-sites` names an account as one of Venti's partners for flying
//!   sites, which lets it add to the partners' layer and correct sites of
//!   both layers; `withdraw-sites` takes that back, and `site-partners`
//!   lists who holds it.
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
//! What is done about a site that pilots made is a revision like any
//! other, kept in the site's history and shown there as the operator's:
//! unlike a group, a site is everyone's to read, and so is what became of
//! it. A site that is taken down is gone to everyone, its identifier is
//! never used again, and only `revert-site` brings it back.
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
    FlightId, GroupId, GroupTextError, Handle, LiveTrackId, PlanId, VENTI_PILOTS_SITES_SOURCE,
    VENTI_SITES_LICENCE, VENTI_SITES_SOURCE, format_day, live_page_path, parse_group_name,
    parse_site_comment, share_path,
};
use venti_domain::{Site as FlyingSite, SiteId, SiteUse, Timestamp};
use venti_store::{
    Flight, GroupMembers, GroupPilot, LiveVisibility, PgStore, PilotSite, Plan, Report,
    ReportClosing, ReportListing, ReportSubject, RevisionKind, SiteAct, SitePilot, SiteRevising,
    SiteRevision, SiteVia, SiteWrite, Store, Visibility,
};

use crate::accounts::secret;
use crate::clock::{Clock, SystemClock};
use crate::config::Config;
use crate::groups;

/// The most characters a closing note may have. It is the operator's own
/// text; the bound is what keeps a slip of the hand out of the table.
pub const MAX_NOTE_LENGTH: usize = 1000;

/// How many reports, or revisions of a site, are read from the store at a
/// time.
const PAGE: usize = 100;

/// How far apart, in metres, two sites that pilots made stand at least.
/// A site that `revert-site` brings back is held to it as a new one is.
use crate::pilot_sites::{APART_M, Layer};

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
    /// Show one site that pilots made, with its whole history.
    Site {
        /// The site's identifier, or a link that ends in it.
        site: String,
    },
    /// Put a site that pilots made back to what an earlier revision said,
    /// as a revision of the operator's. Brings back a site that was
    /// removed or taken down.
    RevertSite {
        /// The site's identifier, or a link that ends in it.
        site: String,
        /// The number of the revision to restore.
        revision: u32,
        /// Why, in one line. Kept with the revision, where everyone who
        /// reads the site's history reads it.
        comment: Option<String>,
    },
    /// Take a site that pilots made down.
    RemoveSite {
        /// The site's identifier, or a link that ends in it.
        site: String,
        /// Why, in one line. Kept with the revision; nobody but the
        /// operator reads the history of a site that is taken down.
        comment: Option<String>,
    },
    /// Name an account as trusted to add and correct flying sites: one of
    /// Venti's partners.
    GrantSites {
        /// The account's handle.
        handle: String,
    },
    /// Take that back. What the account wrote stays.
    WithdrawSites {
        /// The account's handle.
        handle: String,
    },
    /// List the accounts that are trusted with the sites.
    SitePartners,
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
        Action::GrantSites { handle } => {
            let Some(account) = account(store, handle).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_account(handle)));
            };
            let granted = store.let_edit_sites(&account.id, clock.now()).await;
            let Some(since) = granted.map_err(failed)? else {
                return Ok(Outcome::nothing(no_account(handle)));
            };
            let pilot = who(&account.handle, &account.display_name);
            let what = if account.edits_sites_since.is_some() {
                format!(
                    "{pilot} is a partner for flying sites since {} already. Nothing changed.",
                    day_and_time(since)
                )
            } else {
                format!("{pilot} is a partner for flying sites from now on.")
            };
            let suspended = if account.publishing_suspended_at.is_some() {
                format!(
                    "\nThe account's publishing is suspended, and while it is the account \
                     writes no site: `venti-server resume-publishing {}`.",
                    account.handle
                )
            } else {
                String::new()
            };
            Ok(Outcome::found(format!(
                "{what}\n\
                 The account adds sites to the partners' layer (`{VENTI_SITES_SOURCE}`), \
                 which every app shows beside the sources' sites, and corrects, restores \
                 and removes sites of that layer and of the crowdsourced one \
                 (`{VENTI_PILOTS_SITES_SOURCE}`), through its own sign-in and through a \
                 token of its own with the scope sites:write. What it adds is given under \
                 {VENTI_SITES_LICENCE}, with its handle in the site's history. The server \
                 tells the account's apps that it is a partner; it tells nobody else.\n\
                 Nothing else about the account changed. `venti-server withdraw-sites {}` \
                 undoes this.{suspended}",
                account.handle,
            )))
        }
        Action::WithdrawSites { handle } => {
            let Some(account) = account(store, handle).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_account(handle)));
            };
            let pilot = who(&account.handle, &account.display_name);
            let withdrawn = store.stop_editing_sites(&account.id).await;
            Ok(match withdrawn.map_err(failed)? {
                None => Outcome::nothing(no_account(handle)),
                Some(false) => Outcome::found(format!(
                    "{pilot} was not a partner for flying sites. Nothing changed."
                )),
                Some(true) => Outcome::found(format!(
                    "{pilot} is no partner for flying sites any more: the account adds no \
                     site to the partners' layer and corrects none there. Where this server \
                     takes sites from pilots it adds to the crowdsourced layer, as any \
                     pilot.\n\
                     Every site the account added and every revision it wrote stays as it \
                     is, with the account in its history. To undo one of them, \
                     `venti-server site`, then revert-site or remove-site."
                )),
            })
        }
        Action::SitePartners => {
            let partners = store.site_editors().await.map_err(failed)?;
            if partners.is_empty() {
                return Ok(Outcome::found(
                    "No account is a partner for flying sites: nobody adds to the partners' \
                     layer. `venti-server grant-sites <HANDLE>` names one.",
                ));
            }
            let mut said = format!(
                "Partners for flying sites, {}, by handle:",
                counted(partners.len(), "account", "accounts")
            );
            for partner in &partners {
                let since = partner
                    .edits_sites_since
                    .map_or_else(String::new, day_and_time);
                let suspended = if partner.publishing_suspended_at.is_some() {
                    "; publishing suspended, so it writes no site now"
                } else {
                    ""
                };
                let _ = write!(
                    said,
                    "\n  {}  since {since}{suspended}",
                    who(&partner.handle, &partner.display_name)
                );
            }
            said.push_str(
                "\nChanges nothing. `venti-server withdraw-sites <HANDLE>` takes one off.",
            );
            Ok(Outcome::found(said))
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
        Action::Site { site } => {
            let Some(id) = pilot_site_id(site) else {
                return Ok(Outcome::nothing(no_site_identifier(site)));
            };
            // The history is read page by page, and the site with each
            // page: the last page's is the one that is shown, so that no
            // revision is listed beyond the one the site is said to stand
            // at.
            let mut revisions: Vec<SiteRevision> = Vec::new();
            let seen = loop {
                let after = revisions.last().map_or(0, |revision| revision.number);
                let page = store.pilot_site_revisions(&id, after, PAGE).await;
                let Some((seen, page)) = page.map_err(failed)? else {
                    return Ok(Outcome::nothing(no_site(&id)));
                };
                let last = page.len() < PAGE;
                revisions.extend(page);
                if last {
                    break seen;
                }
            };
            Ok(Outcome::found(shown_site(&seen, &revisions)))
        }
        Action::RevertSite {
            site,
            revision,
            comment,
        } => {
            let comment = site_comment(comment.as_deref())?;
            let Some(id) = pilot_site_id(site) else {
                return Ok(Outcome::nothing(no_site_identifier(site)));
            };
            let Some(before) = store.pilot_site(&id).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_site(&id)));
            };
            let revising = SiteRevising {
                id: id.clone(),
                author_id: None,
                at: clock.now(),
                via: SiteVia::Operator,
                act: SiteAct::Restore(*revision),
                comment,
                expects: None,
            };
            let written = store.revise_pilot_site(&revising, APART_M).await;
            Ok(match written.map_err(failed)? {
                SiteWrite::Written(now) => {
                    let back = match (before.taken_down, before.stands()) {
                        (true, _) => {
                            " It was taken down and stands again: it is in every \
                                      answer about its place, and its history can be read."
                        }
                        (false, false) => " It was removed and stands again.",
                        (false, true) => "",
                    };
                    Outcome::found(format!(
                        "Site {id} {} says again what its revision {revision} said, as its \
                         revision {}.{back}\n\
                         The revision is in the site's history as the operator's, with its \
                         comment, for everyone to read. Nobody is told, and nothing stops a \
                         pilot from changing the site again.\n\
                         Nothing else changed: no earlier revision was removed, no account was \
                         touched, and no report was closed.",
                        named(&now.site),
                        now.revision,
                    ))
                }
                SiteWrite::Same(now) => Outcome::found(format!(
                    "Site {id} {} says already what its revision {revision} said, and \
                     stands. Nothing changed.",
                    named(&now.site),
                )),
                SiteWrite::NoSuchRevision => Outcome::nothing(format!(
                    "Site {id} has no revision {revision}: it stands at revision {}, and \
                     `venti-server site {id}` lists them. Nothing changed.",
                    before.revision,
                )),
                SiteWrite::TooClose(other) => Outcome::nothing(format!(
                    "Nothing changed: as its revision {revision} said, site {id} would stand \
                     less than {APART_M} m from site {other}, and two sites that pilots made \
                     are never that near to each other. Look at that one with \
                     `venti-server site {other}`; restore another revision, or take one of \
                     the two down.",
                )),
                SiteWrite::NoSuchSite => Outcome::nothing(no_site(&id)),
                // Not answers to a restore by the operator that names no
                // revision to start from.
                SiteWrite::Stale { .. }
                | SiteWrite::Removed
                | SiteWrite::TakenDown
                | SiteWrite::NoSuchAccount => {
                    return Err(format!(
                        "the store refused to restore revision {revision} of site {id} in a \
                         way it does not refuse the operator. Nothing changed."
                    ));
                }
            })
        }
        Action::RemoveSite { site, comment } => {
            let comment = site_comment(comment.as_deref())?;
            let Some(id) = pilot_site_id(site) else {
                return Ok(Outcome::nothing(no_site_identifier(site)));
            };
            let Some(before) = store.pilot_site(&id).await.map_err(failed)? else {
                return Ok(Outcome::nothing(no_site(&id)));
            };
            let revising = SiteRevising {
                id: id.clone(),
                author_id: None,
                at: clock.now(),
                via: SiteVia::Operator,
                act: SiteAct::Remove { take_down: true },
                comment,
                expects: None,
            };
            let written = store.revise_pilot_site(&revising, APART_M).await;
            Ok(match written.map_err(failed)? {
                SiteWrite::Written(now) => {
                    let was = if before.stands() {
                        "It stood until now"
                    } else {
                        "A pilot had removed it, and could have brought it back until now"
                    };
                    Outcome::found(format!(
                        "Site {id} {} is taken down, as its revision {}. {was}.\n\
                         It is in no answer about its place, its address and its history \
                         answer that it is gone, and no pilot can bring it back. Whoever \
                         follows what changed among the sites learns that it is gone, and \
                         nobody is told by whom or why. Its identifier is never used for \
                         another site.\n\
                         Nothing was deleted: the site and every revision are kept, and \
                         `venti-server site {id}` still shows them. \
                         `venti-server revert-site {id} --revision {}` brings it back as it \
                         last stood. It does not stop anyone from adding a site at the place \
                         again. No account was touched, and no report was closed.",
                        named(&now.site),
                        now.revision,
                        before.revision,
                    ))
                }
                // Removed once more only to take it down: so it is down.
                SiteWrite::Removed => Outcome::found(format!(
                    "Site {id} {} is taken down already. Nothing changed.",
                    named(&before.site),
                )),
                SiteWrite::NoSuchSite => Outcome::nothing(no_site(&id)),
                SiteWrite::Same(_)
                | SiteWrite::TooClose(_)
                | SiteWrite::Stale { .. }
                | SiteWrite::TakenDown
                | SiteWrite::NoSuchRevision
                | SiteWrite::NoSuchAccount => {
                    return Err(format!(
                        "the store refused to take site {id} down in a way it does not \
                         refuse the operator. Nothing changed."
                    ));
                }
            })
        }
    }
}

// --- Sites that pilots made -------------------------------------------------

/// The identifier of a site that pilots made, from what an operator
/// pastes: the identifier, or a link that ends in it. `None` for anything
/// else, an identifier of another source's site included: those are the
/// source's, and nothing here acts on them.
fn pilot_site_id(text: &str) -> Option<SiteId> {
    let id: SiteId = last_part(text).parse().ok()?;
    (Layer::of(&id).is_some() && secret::is_id(id.local())).then_some(id)
}

fn no_site_identifier(text: &str) -> String {
    format!(
        "{:?} is not the identifier of a site of Venti's own: such an identifier is \
         `{VENTI_SITES_SOURCE}:` (a partner's) or `{VENTI_PILOTS_SITES_SOURCE}:` (a \
         pilot's) and 26 characters. A site of another source is that source's to correct, \
         and goes or changes with its next run.",
        on_one_line(text)
    )
}

fn no_site(id: &SiteId) -> String {
    format!("There is no site with the identifier {id}, and there never was one.")
}

/// The comment of a revision of the operator's, cleaned as a pilot's is.
fn site_comment(comment: Option<&str>) -> Result<Option<String>, String> {
    parse_site_comment(comment).map_err(|most| {
        format!(
            "a comment is one line of at most {most} characters: it is kept with the \
             revision, in the site's history"
        )
    })
}

/// A site by its name between quotation marks, or by saying that it has
/// none.
fn named(site: &FlyingSite) -> String {
    site.name
        .as_deref()
        .map_or_else(|| "without a name".to_owned(), quoted)
}

fn site_pilot(pilot: Option<&SitePilot>) -> String {
    pilot.map_or_else(
        || "an account that is gone".to_owned(),
        |pilot| who(&pilot.handle, &pilot.display_name),
    )
}

fn use_in_words(used_for: SiteUse) -> &'static str {
    match used_for {
        SiteUse::Takeoff => "take-off",
        SiteUse::Landing => "landing",
        SiteUse::Towing => "towing",
        SiteUse::Training => "training",
        SiteUse::Other => "another use",
    }
}

fn yes_or_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// What a site's record says, as the lines of a block: every field that
/// says something, and of an outline how many points it has.
fn record_lines(block: &mut String, site: &FlyingSite) {
    labelled(block, "Name:", site.name.as_deref().unwrap_or("none"));
    let place = format!("{:.5}, {:.5}", site.position.lat(), site.position.lon());
    labelled(block, "Place:", &place);
    if let Some(height) = site.elevation_m {
        labelled(block, "Height:", &format!("{height:.0} m"));
    }
    let uses: Vec<&str> = site.uses.iter().copied().map(use_in_words).collect();
    labelled(block, "Uses:", &uses.join(", "));
    if !site.wind_directions.is_empty() {
        let sectors: Vec<String> = site
            .wind_directions
            .iter()
            .map(|sector| {
                if sector.from_deg == sector.to_deg {
                    format!("{:.0}°", sector.from_deg)
                } else {
                    format!("{:.0}° to {:.0}°", sector.from_deg, sector.to_deg)
                }
            })
            .collect();
        labelled(block, "Wind from:", &sectors.join(", "));
    }
    for (label, value) in [
        ("Paragliders:", site.paragliding),
        ("Hang gliders:", site.hang_gliding),
        ("Guest rules:", site.guest_guidelines),
    ] {
        if let Some(value) = value {
            labelled(block, label, yes_or_no(value));
        }
    }
    for (label, text) in [
        ("Web page:", &site.website),
        ("Fee:", &site.fee),
        ("No flying:", &site.no_flight_time),
        ("Description:", &site.description),
    ] {
        if let Some(text) = text {
            labelled(block, label, text);
        }
    }
    if let Some(outline) = &site.outline {
        let points = format!("{} points", outline.points().len());
        labelled(block, "Outline:", &points);
    }
}

/// The fields in which two records of a site differ, by the names the
/// API gives them.
fn differing(before: &FlyingSite, after: &FlyingSite) -> Vec<&'static str> {
    [
        ("name", before.name != after.name),
        ("position", before.position != after.position),
        ("elevation_m", before.elevation_m != after.elevation_m),
        ("uses", before.uses != after.uses),
        (
            "wind_directions",
            before.wind_directions != after.wind_directions,
        ),
        ("paragliding", before.paragliding != after.paragliding),
        ("hang_gliding", before.hang_gliding != after.hang_gliding),
        ("website", before.website != after.website),
        ("description", before.description != after.description),
        ("fee", before.fee != after.fee),
        (
            "guest_guidelines",
            before.guest_guidelines != after.guest_guidelines,
        ),
        (
            "no_flight_time",
            before.no_flight_time != after.no_flight_time,
        ),
        ("outline", before.outline != after.outline),
    ]
    .into_iter()
    .filter_map(|(field, differs)| differs.then_some(field))
    .collect()
}

/// One revision of a site as a line: its number, when, by whom and
/// through what, what it did, in which fields the record then differed
/// from the one before, and its comment.
fn revision_line(revision: &SiteRevision, before: Option<&FlyingSite>) -> String {
    let by = match &revision.via {
        SiteVia::Operator => "the operator".to_owned(),
        SiteVia::Session => format!(
            "{}, through a session",
            site_pilot(revision.author.as_ref())
        ),
        SiteVia::Token(name) => format!(
            "{}, through the token {}",
            site_pilot(revision.author.as_ref()),
            quoted(name)
        ),
    };
    let did = match revision.kind {
        RevisionKind::Created => "made the site".to_owned(),
        RevisionKind::Changed => "changed it".to_owned(),
        RevisionKind::Restored(number) => format!("restored revision {number}"),
        RevisionKind::Removed => "removed it".to_owned(),
    };
    let fields = before
        .map(|before| differing(before, &revision.record))
        .filter(|fields| !fields.is_empty())
        .map(|fields| format!(" ({})", fields.join(", ")))
        .unwrap_or_default();
    let comment = revision
        .comment
        .as_deref()
        .map(|comment| format!(": {}", quoted(comment)))
        .unwrap_or_default();
    format!(
        "{:>4}  {}  {by} {did}{fields}{comment}",
        revision.number,
        day_and_time(revision.at),
    )
}

/// One site that pilots made, as text: where it stands, what its record
/// says, and every revision in a line.
fn shown_site(seen: &PilotSite, revisions: &[SiteRevision]) -> String {
    let mut said = format!("Site {}", seen.site.id);
    let layer = match Layer::of(&seen.site.id) {
        Some(Layer::Partners) => "the partners' (added by an account trusted with sites)",
        Some(Layer::Pilots) => "the crowdsourced one (added by a pilot, asked for to be shown)",
        None => "none of Venti's own",
    };
    labelled(&mut said, "Layer:", layer);
    record_lines(&mut said, &seen.site);
    let made = format!(
        "{} by {}",
        day_and_time(seen.created_at),
        site_pilot(seen.created_by.as_ref())
    );
    labelled(&mut said, "Made:", &made);
    let stands = match (seen.removed_at, seen.taken_down) {
        (None, _) => format!("at revision {}", seen.revision),
        (Some(since), false) => format!(
            "removed since {}, at revision {}; a pilot can bring it back",
            day_and_time(since),
            seen.revision
        ),
        (Some(since), true) => format!(
            "taken down since {}, at revision {}; gone to everyone, and only revert-site \
             brings it back",
            day_and_time(since),
            seen.revision
        ),
    };
    labelled(&mut said, "Stands:", &stands);
    if let Some(day) = seen.confirmed.latest_day {
        let confirmed = format!(
            "by {}, last of {}",
            counted(seen.confirmed.pilots as usize, "pilot", "pilots"),
            format_day(day)
        );
        labelled(&mut said, "Confirmed:", &confirmed);
    }
    let _ = write!(
        said,
        "\n\nHistory, {}, the first one first:",
        counted(revisions.len(), "revision", "revisions")
    );
    let mut before: Option<&FlyingSite> = None;
    for revision in revisions {
        let _ = write!(said, "\n{}", revision_line(revision, before));
        before = Some(&revision.record);
    }
    said.push_str(
        "\n\nThe record is as it stands, or as it last stood. Nothing was changed: see \
         `venti-server --help` for revert-site and remove-site.",
    );
    said
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
    // Every report but one about a site names an account.
    let about = report.about.as_ref().map_or_else(
        || "nobody".to_owned(),
        |about| who(&about.handle, &about.name),
    );
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
        ReportSubject::Site { id, revision } => {
            // Nothing of the site is kept with the report: its revisions
            // are, and `site` shows them.
            let now = match id.parse::<SiteId>() {
                Ok(site_id) => match store.pilot_site(&site_id).await? {
                    Some(seen) if seen.taken_down => "taken down".to_owned(),
                    Some(seen) if !seen.stands() => {
                        format!("removed, at revision {}", seen.revision)
                    }
                    Some(seen) => format!(
                        "at revision {}, called {}",
                        seen.revision,
                        seen.site
                            .name
                            .as_deref()
                            .map_or_else(|| "nothing".to_owned(), quoted)
                    ),
                    None => "gone".to_owned(),
                },
                Err(_) => "gone".to_owned(),
            };
            labelled(&mut kept, "Now:", &now);
            labelled(
                &mut kept,
                "Read it:",
                &format!("venti-server site {}", on_one_line(id)),
            );
            (
                format!(
                    "site {}, revision {revision}; made by pilots and nobody's",
                    on_one_line(id)
                ),
                None,
            )
        }
        ReportSubject::Pilot => {
            let handle = report.about.as_ref().map_or("", |about| &about.handle);
            let suspended = store
                .account(handle)
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
