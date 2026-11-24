//! Everything a pilot gave, as one ZIP file: `GET /v1/account/archive`.
//!
//! A platform that takes a pilot's data hands it back whole
//! (`docs/data-platform.md`, principle 11). The archive holds the account,
//! every flight with its file as it was uploaded, the groups and plans,
//! what the pilot gave to the sites that pilots make, the sites they keep,
//! the tokens they gave to programs, the reports they sent, and their
//! alarms with what they fired. Its JSON
//! files are the API's own records, and `index.json`
//! ([`ArchiveIndex`]) names each with the record it holds.
//!
//! **Who may ask** is the pilot's own sign-in and no token for a program:
//! no scope reaches all of an account, and a program gets its pieces
//! through the scopes it has.
//!
//! **What it costs is bounded, and none of the bounds is a setting.** The
//! archive is built in the server's memory, so only one is built at a
//! time ([`Archives`]): whoever asks meanwhile is told to try again. One
//! account asks a few times a day at most. And an archive holds at most
//! [`MAX_ARCHIVE_FILE_BYTES`] of flight files, the newest first; a flight
//! beyond that is listed without its file, and the index says so. What is
//! held at once is then the packed archive and one flight file: the files
//! are read from the store one at a time and packed as they come.
//!
//! **Never in it:** the password's hash, and the secret of a session or
//! of a token. The server has only hashes of those, and nothing here
//! reads one.

use std::collections::BTreeSet;
use std::io::{Cursor, Write};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, Timelike};
use serde::Serialize;
use tokio::sync::{Semaphore, mpsc};
use venti_api_types::v1::{
    ARCHIVE_FORMAT, ARCHIVE_INDEX_FILE, ARCHIVE_MEDIA_TYPE, AccessTokensResponse, AccountResponse,
    ArchiveAlerts, ArchiveFile, ArchiveFlight, ArchiveIndex, ArchiveLicence, ArchiveReport,
    ArchiveReports, ArchiveSiteConfirmation, ArchiveSites, FIRED_KEPT_DAYS, FlightsResponse,
    GroupsResponse, Handle, KeptSiteChangesResponse, MAX_ARCHIVE_FILE_BYTES, PlansResponse,
    ReportRequest, ReportSubject, ReportedPlan, ReportedSite, SessionsResponse,
    VENTI_SITES_LICENCE, VENTI_SITES_LICENCE_URL,
};
use venti_domain::Timestamp;
use venti_store::{Flight, Listing, Store};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::auth::OwnSession;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, alerts, cache, kept_sites, sites, wire};
use crate::accounts::throttle::{Counted, Throttle};
use crate::flights::unpack;

/// How many records one question to the store brings while an archive is
/// gathered.
const PAGE: usize = 500;

/// The most plans one archive holds. A group holds a bounded number and
/// an account is in a bounded number of groups; this is beyond both.
const MAX_PLANS: usize = 10_000;

/// How many flight files wait between the store and the packing.
const FILES_WAITING: usize = 4;

/// Who asked for an archive how often, and the one turn at making one.
pub(crate) struct Archives {
    /// Archives asked for, by account.
    asked: Throttle,
    /// One archive is built at a time: it is built in memory.
    turn: Semaphore,
}

impl Archives {
    pub(crate) fn new(watched_keys: usize) -> Self {
        Self {
            asked: Throttle::new(
                "accounts that asked for their archive",
                chrono::Duration::days(1),
                watched_keys,
            ),
            turn: Semaphore::new(1),
        }
    }
}

/// A file of the archive that is ready before the flight files are read.
struct Ready {
    listed: ArchiveFile,
    bytes: Vec<u8>,
}

fn json(value: &impl Serialize) -> Result<Vec<u8>, ApiError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| ApiError::internal())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn ready(
    path: &str,
    record: &str,
    holds: &str,
    entries: usize,
    value: &impl Serialize,
) -> Result<Ready, ApiError> {
    Ok(Ready {
        listed: ArchiveFile {
            path: path.to_owned(),
            holds: holds.to_owned(),
            record: record.to_owned(),
            entries: u32::try_from(entries).unwrap_or(u32::MAX),
            licence: None,
        },
        bytes: json(value)?,
    })
}

/// The name of a flight's file in the archive: the day and the time of
/// its take-off in UTC, and the beginning of its address. Nothing a pilot
/// or a file supplied goes into a name.
fn file_name(flight: &Flight, taken: &mut BTreeSet<String>) -> String {
    let took_off = flight
        .derived
        .takeoff
        .as_ref()
        .map_or(flight.derived.start, |takeoff| takeoff.time);
    let stamp = took_off.format("%Y-%m-%d_%H%MZ");
    let short = flight.id.get(..8).unwrap_or(&flight.id);
    let name = format!("flights/{stamp}_{short}.igc");
    if taken.insert(name.clone()) {
        return name;
    }
    // Two flights of one minute whose addresses begin alike.
    let name = format!("flights/{stamp}_{}.igc", flight.id);
    taken.insert(name.clone());
    name
}

/// A report as its sender gets it back. `None` for one whose reason or
/// subject this version of the code cannot name.
fn report(report: venti_store::Report) -> Option<ArchiveReport> {
    use venti_store::ReportSubject as Stored;
    let subject = match report.subject {
        Stored::Flight(id) => ReportSubject::Flight(id),
        Stored::Pilot => ReportSubject::Pilot(report.about?.handle),
        Stored::Picture(_) => ReportSubject::Picture(report.about?.handle),
        Stored::LiveTrack(id) => ReportSubject::LiveTrack(id),
        // The words the report kept of a group or a plan are another
        // pilot's, and are not handed out with it.
        Stored::Group { id, .. } => ReportSubject::Group(id),
        Stored::Plan { id, group_id, .. } => ReportSubject::Plan(ReportedPlan {
            group: group_id,
            id,
        }),
        Stored::OpenPlan { id, .. } => ReportSubject::OpenPlan(id),
        Stored::HelpRequest { id, .. } => ReportSubject::HelpRequest(id),
        Stored::Site { id, revision } => ReportSubject::Site(ReportedSite {
            id,
            revision: Some(revision),
        }),
        Stored::GroupMessage { id, group_id, .. } => {
            ReportSubject::GroupMessage(venti_api_types::v1::ReportedMessage {
                group: group_id,
                id,
            })
        }
        Stored::GroupPicture { id, group_id } => {
            ReportSubject::GroupPicture(venti_api_types::v1::ReportedMessage {
                group: group_id,
                id,
            })
        }
        // The words the report kept are another pilot's.
        Stored::Comment { id, flight_id, .. } => {
            ReportSubject::Comment(venti_api_types::v1::ReportedComment {
                flight: flight_id,
                id,
            })
        }
    };
    Some(ArchiveReport {
        sent_at: report.created_at,
        report: ReportRequest {
            subject,
            reason: serde_json::from_value(serde_json::Value::String(report.reason)).ok()?,
            text: report.text,
        },
    })
}

/// What the archive says of itself to a person.
fn readme(index: &ArchiveIndex) -> String {
    let mut text = format!(
        "Venti: everything the account '{}' gave\n\
         Made {} (UTC). Format {}.\n\n\
         index.json says the same as this file, for a program.\n\n\
         THE FILES\n\n",
        index.account,
        index.made_at.format("%Y-%m-%d %H:%M"),
        index.format,
    );
    for file in &index.files {
        text.push_str(&format!("{}\n    {}\n", file.path, file.holds));
        text.push_str(&format!(
            "    Entries: {}. Record: {} (docs/api.md, /v1/openapi.json).\n",
            file.entries, file.record
        ));
        if let Some(licence) = &file.licence {
            text.push_str(&format!(
                "    Licence: {} ({}).\n",
                licence.name, licence.url
            ));
        }
        text.push('\n');
    }
    let with_file = index
        .flights
        .iter()
        .filter(|flight| flight.file.is_some())
        .count();
    text.push_str(&format!(
        "flights/\n    The flight files as they were uploaded (IGC), {with_file} of {} flights,\n    \
         named by the day and time of take-off in UTC and the beginning of the\n    \
         flight's address. Any program that reads IGC reads them, and uploading\n    \
         one to a Venti account (POST /v1/flights) makes the flight there again.\n\n",
        index.flights.len(),
    ));
    if !index.complete {
        text.push_str(
            "NOT EVERY FLIGHT FILE IS HERE\n\n\
             index.json lists some flights without a file: the archive had no room\n\
             for them, or the flight was deleted while the archive was made. Each is\n\
             in flights.json, and its file is fetched by itself:\n\
             GET /v1/flights/{id}/igc.\n\n",
        );
    }
    text.push_str("WHAT IS NOT IN THE ARCHIVE\n\n");
    for line in &index.not_included {
        text.push_str(&format!("- {line}\n"));
    }
    text
}

/// What the account has on the server that an archive does not hold.
fn not_included() -> Vec<String> {
    [
        "The password and the secrets of sessions and tokens. The server keeps only a \
         fingerprint of each and cannot hand one out.",
        "Live tracks. The server keeps one for a short time after its last position and \
         then removes it; the flight reaches the logbook as a file.",
        "What other pilots put into the groups: their names as members, and their plans \
         unless this account said it is in on them.",
        "What whoever runs the server noted about a report, and reports others sent.",
        "What this account withdrew: a deleted flight, a forgotten site's record, a \
         confirmation taken back.",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// The archive's parts that are JSON, gathered from the store.
struct Gathered {
    files: Vec<Ready>,
    flights: Vec<Flight>,
}

impl<S: Store> Api<S> {
    async fn gather(&self, own: &OwnSession, now: Timestamp) -> Result<Gathered, ApiError> {
        let account_id = &own.account.id;
        let store = &self.store;
        let mut files = Vec::new();

        // The pilot's own archive: the email of the Google account is
        // theirs to have.
        let google = store.google_link(account_id).await?;
        let account = AccountResponse {
            account: wire::account(&own.account, google.as_ref())?,
        };
        files.push(ready(
            "account.json",
            "AccountResponse",
            "The account: handle, name, since when, and the email of its Google account if it has one.",
            1,
            &account,
        )?);

        let expired_before = now - self.limits.session_idle;
        let sessions: Vec<_> = store
            .sessions(account_id)
            .await?
            .iter()
            .filter(|session| session.last_used_at >= expired_before)
            .map(|session| wire::session(session, &own.session.id))
            .collect();
        files.push(ready(
            "sessions.json",
            "SessionsResponse",
            "The devices signed in to the account, by the name each gave.",
            sessions.len(),
            &SessionsResponse { sessions },
        )?);

        let tokens: Vec<_> = store
            .access_tokens(account_id)
            .await?
            .iter()
            .map(wire::access_token)
            .collect();
        files.push(ready(
            "tokens.json",
            "AccessTokensResponse",
            "The tokens given to other programs: names, scopes and dates. No secrets.",
            tokens.len(),
            &AccessTokensResponse { tokens },
        )?);

        // Every flight, newest first, page by page.
        let mut flights: Vec<Flight> = Vec::new();
        loop {
            let after = flights.last().map(Flight::key);
            let page = store
                .flights_of(account_id, Listing::All, after.as_ref(), PAGE)
                .await?;
            let last = page.len() < PAGE;
            flights.extend(page);
            if last {
                break;
            }
        }
        let listed = FlightsResponse {
            flights: wire::flights(&flights)?,
            next: None,
            terms: None,
        };
        files.push(ready(
            "flights.json",
            "FlightsResponse",
            "Every flight of the account, public or not, newest first: what the server \
             worked out of each file. The files are under flights/.",
            flights.len(),
            &listed,
        )?);

        let groups = store.groups_of(account_id).await?;
        files.push(ready(
            "groups.json",
            "GroupsResponse",
            "The groups the account is in, each with who keeps it.",
            groups.len(),
            &GroupsResponse {
                groups: wire::groups(&groups)?,
                chats: Vec::new(),
            },
        )?);

        let from = self.groups.limits.gone_before(now);
        let mut plans = store.plans_for(account_id, from, None, MAX_PLANS).await?;
        plans.retain(|plan| {
            plan.author.account_id == *account_id
                || plan
                    .pilots_in
                    .iter()
                    .any(|pilot| pilot.account_id == *account_id)
        });
        files.push(ready(
            "plans.json",
            "PlansResponse",
            "The plans the account posted to its groups, and the plans of others it said \
             it is in on. Plans for days gone by are no longer kept.",
            plans.len(),
            &PlansResponse {
                plans: wire::plans(&plans)?,
            },
        )?);

        let given = store.given_by(account_id).await?;
        let made = ArchiveSites {
            revisions: given
                .revisions
                .into_iter()
                .map(sites::revision)
                .collect::<Result<_, _>>()?,
            confirmations: given
                .confirmations
                .into_iter()
                .map(|(site, said)| ArchiveSiteConfirmation {
                    site,
                    day: said.day,
                    note: said.note,
                    with_flight: said.with_flight,
                    at: said.at,
                })
                .collect(),
        };
        let mut sites = ready(
            "sites.json",
            "ArchiveSites",
            "What the account gave to the take-offs and landings that pilots make: every \
             revision it wrote (number 1 made the site), and where it says it took off. \
             The sites were given to everyone and stay when the account goes.",
            made.revisions.len() + made.confirmations.len(),
            &made,
        )?;
        sites.listed.licence = Some(ArchiveLicence {
            name: VENTI_SITES_LICENCE.to_owned(),
            url: VENTI_SITES_LICENCE_URL.to_owned(),
        });
        files.push(sites);

        // The kept sites as the account's devices are told of them, from
        // the beginning.
        let mut kept = Vec::new();
        let mut forgotten = Vec::new();
        let mut since = 0;
        loop {
            let changes = store.kept_site_changes(account_id, since, PAGE).await?;
            since = changes.last().unwrap_or(since);
            for site in &changes.kept {
                kept.push(kept_sites::wire(site)?);
            }
            for site in &changes.forgotten {
                forgotten.push(site.site_id.parse().map_err(|_| ApiError::internal())?);
            }
            if !changes.more {
                break;
            }
        }
        let entries = kept.len();
        files.push(ready(
            "kept-sites.json",
            "KeptSiteChangesResponse",
            "The sites the account keeps as its own, each as it was when kept, and the \
             identifiers of those it forgot lately.",
            entries,
            &KeptSiteChangesResponse {
                kept,
                forgotten,
                cursor: since.to_string(),
                more: false,
                reset: false,
            },
        )?);

        let reports: Vec<ArchiveReport> = given.reports.into_iter().filter_map(report).collect();
        files.push(ready(
            "reports.json",
            "ArchiveReports",
            "The reports the account sent to whoever runs the server, as it sent them.",
            reports.len(),
            &ArchiveReports { reports },
        )?);

        // The alarms, and what they fired of the days the server keeps,
        // page by page.
        let kept_from = now - chrono::Duration::days(FIRED_KEPT_DAYS);
        let mut fired = Vec::new();
        let mut since = 0;
        loop {
            let page = store.fired_alerts(account_id, since, PAGE).await?;
            since = page.fired.last().map_or(since, |last| last.number);
            for one in page.fired.iter().filter(|one| one.fired_at >= kept_from) {
                // The text is the server's own writing.
                fired.push(serde_json::from_str(&one.record).map_err(|_| ApiError::internal())?);
            }
            if !page.more || page.fired.is_empty() {
                break;
            }
        }
        let made = ArchiveAlerts {
            alerts: store
                .alerts(account_id)
                .await?
                .iter()
                .map(alerts::wire)
                .collect::<Result<_, _>>()?,
            fired,
        };
        files.push(ready(
            "alerts.json",
            "ArchiveAlerts",
            &format!(
                "The account's alarms, and what they fired in the last {FIRED_KEPT_DAYS} days."
            ),
            made.alerts.len() + made.fired.len(),
            &made,
        )?);

        // What the account gave to other pilots: whom it follows, the
        // flights it cheered, the comments it wrote.
        let mut following = Vec::new();
        let mut after = None;
        loop {
            let page = store
                .follows(
                    account_id,
                    venti_store::FollowSide::Following,
                    false,
                    after.as_ref(),
                    PAGE,
                )
                .await?;
            after = page.last().map(venti_store::SocialPilot::key);
            let last_page = page.len() < PAGE;
            for pilot in &page {
                following.push(venti_api_types::v1::FollowedPilot {
                    pilot: wire::pictured(&pilot.handle, &pilot.name, pilot.picture.as_deref())?,
                    since: pilot.at,
                });
            }
            if last_page {
                break;
            }
        }
        let given = store.social_given(account_id).await?;
        let flight_id = |id: &str| id.parse().map_err(|_| ApiError::internal());
        let social = venti_api_types::v1::ArchiveSocial {
            following,
            cheers: given
                .cheers
                .iter()
                .map(|(flight, at)| {
                    Ok(venti_api_types::v1::ArchiveCheer {
                        flight: flight_id(flight)?,
                        at: *at,
                    })
                })
                .collect::<Result<_, ApiError>>()?,
            comments: given
                .comments
                .iter()
                .map(|comment| {
                    Ok(venti_api_types::v1::ArchiveComment {
                        id: comment.id.clone(),
                        flight: flight_id(&comment.flight_id)?,
                        text: comment.text.clone(),
                        at: comment.created_at,
                    })
                })
                .collect::<Result<_, ApiError>>()?,
        };
        files.push(ready(
            "social.json",
            "ArchiveSocial",
            "The pilots the account follows, the flights it cheered and the comments it wrote \
             under flights.",
            social.following.len() + social.cheers.len() + social.comments.len(),
            &social,
        )?);

        Ok(Gathered { files, flights })
    }
}

/// A flight file on its way into the archive: which flight, under what
/// name, and the file as the store keeps it.
struct Packed {
    flight: usize,
    name: String,
    file: Vec<u8>,
}

/// Why an archive could not be packed.
enum NotPacked {
    /// A stored flight file cannot be unpacked; the flight's address.
    Damaged(usize),
    /// The ZIP file could not be written, which in memory does not happen.
    Zip,
}

/// A moment as a ZIP file writes it, in UTC: every file of an archive
/// carries the moment the archive was made.
fn zip_time(at: Timestamp) -> zip::DateTime {
    let part = |value: u32| u8::try_from(value).unwrap_or(0);
    u16::try_from(at.year())
        .ok()
        .and_then(|year| {
            zip::DateTime::from_date_and_time(
                year,
                part(at.month()),
                part(at.day()),
                part(at.hour()),
                part(at.minute()),
                part(at.second()),
            )
            .ok()
        })
        .unwrap_or_default()
}

/// Packs the archive: the files that are ready, then each flight file as
/// it arrives, then the index and the text that say what is there.
///
/// Runs off the threads that answer requests.
fn pack(
    ready: Vec<Ready>,
    mut index: ArchiveIndex,
    mut arriving: mpsc::Receiver<Packed>,
) -> Result<Vec<u8>, NotPacked> {
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip_time(index.made_at))
        .unix_permissions(0o644);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let put = |zip: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, bytes: &[u8]| {
        zip.start_file(name, options).map_err(|_| NotPacked::Zip)?;
        zip.write_all(bytes).map_err(|_| NotPacked::Zip)
    };

    for file in ready {
        put(&mut zip, &file.listed.path, &file.bytes)?;
        index.files.push(file.listed);
    }
    while let Some(packed) = arriving.blocking_recv() {
        let file = unpack(&packed.file).ok_or(NotPacked::Damaged(packed.flight))?;
        put(&mut zip, &packed.name, &file)?;
        if let Some(listed) = index.flights.get_mut(packed.flight) {
            listed.file = Some(packed.name);
        }
    }
    index.complete = index.flights.iter().all(|flight| flight.file.is_some());

    let text = serde_json::to_vec_pretty(&index).map_err(|_| NotPacked::Zip)?;
    put(&mut zip, ARCHIVE_INDEX_FILE, &text)?;
    put(&mut zip, "README.txt", readme(&index).as_bytes())?;
    zip.finish()
        .map(Cursor::into_inner)
        .map_err(|_| NotPacked::Zip)
}

/// `GET /v1/account/archive`: everything the account gave, as a ZIP file.
pub(crate) async fn archive<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    // The turn first: whoever is told that the server is busy has not
    // used up one of the day's archives.
    let _turn = api.archives.turn.try_acquire().map_err(|_| {
        ApiError::busy("the server is making another pilot's archive; try again in a minute")
            .retry_after(60)
    })?;
    let by_day = [Counted::new(&own.account.id, api.limits.archives_per_day)];
    api.archives
        .asked
        .attempt(&by_day, now)
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "this account has asked for its archive as often today as one account may",
            )
        })?;

    let handle = Handle::new(&own.account.handle).map_err(|_| ApiError::internal())?;
    let Gathered { files, flights } = api.gather(&own, now).await?;

    // Which flights' files the archive has room for: the newest first.
    let mut taken = BTreeSet::new();
    let mut room = MAX_ARCHIVE_FILE_BYTES;
    let mut wanted = Vec::new();
    let mut listed = Vec::with_capacity(flights.len());
    for (place, flight) in flights.iter().enumerate() {
        listed.push(ArchiveFlight {
            id: flight.id.parse().map_err(|_| ApiError::internal())?,
            file: None,
        });
        let bytes = u64::from(flight.file_bytes);
        if bytes <= room {
            room -= bytes;
            wanted.push((place, flight.id.clone(), file_name(flight, &mut taken)));
        }
    }
    let index = ArchiveIndex {
        format: ARCHIVE_FORMAT,
        made_at: now,
        account: handle,
        files: Vec::new(),
        flights: listed,
        complete: false,
        not_included: not_included(),
    };

    let (sending, arriving) = mpsc::channel(FILES_WAITING);
    let packing = tokio::task::spawn_blocking(move || pack(files, index, arriving));
    for (flight, id, name) in wanted {
        // A flight that was deleted a moment ago has no file any more:
        // the index lists it without one.
        let Some(file) = api.store.flight_file(&id).await? else {
            continue;
        };
        // The packing has stopped: what it says is read below.
        if sending.send(Packed { flight, name, file }).await.is_err() {
            break;
        }
    }
    drop(sending);
    let packed = match packing.await {
        Ok(Ok(packed)) => packed,
        Ok(Err(NotPacked::Damaged(place))) => {
            static GATE: Sparse = Sparse::new();
            GATE.error(format_args!(
                "the stored file of flight {} cannot be unpacked",
                flights.get(place).map_or("?", |flight| flight.id.as_str())
            ));
            return Err(ApiError::internal());
        }
        Ok(Err(NotPacked::Zip)) | Err(_) => return Err(ApiError::internal()),
    };
    tracing::info!(
        account = %own.account.handle,
        flights = flights.len(),
        bytes = packed.len(),
        "archive made"
    );

    let mut response = cache::private((StatusCode::OK, packed).into_response());
    cache::set(&mut response, header::CONTENT_TYPE, ARCHIVE_MEDIA_TYPE);
    cache::set(&mut response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // The handle is letters, digits and hyphens: it was checked above.
    let name = format!(
        "attachment; filename=\"venti-{}-{}.zip\"",
        own.account.handle,
        now.format("%Y-%m-%d")
    );
    if let Ok(value) = HeaderValue::from_str(&name) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}
