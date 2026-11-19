//! The pilot's account on a Venti server, and keeping the logbook in step
//! with it.
//!
//! Without an account everything stays on the device. With one, the
//! pilot's flight files are copied to the server, so that they appear on
//! the pilot's other devices and can be shared with a link. A flight is
//! private on the server until the pilot says otherwise.
//!
//! The core does no I/O here either. For every conversation with the
//! server it hands the shell a [`ServerRequest`] and reads the answer; the
//! shell sends the request to the server the pilot signed in to, with the
//! session token where the request says it needs one. The token is the
//! shell's to keep, where the device keeps secrets; the core never stores
//! it. What the core does store, in `account.json`, is which account this
//! is and what it knows of the server's side of the logbook.
//!
//! Sync ([`FlightSync`]) is a loop of such steps: deletions made here are
//! sent, the server's changes are fetched and applied, flights this device
//! lacks are downloaded, and flights the server lacks are uploaded. Every
//! finished step is saved, so a sync that is cut off loses nothing and the
//! next one goes on from there.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use venti_api_types::v1::{
    self as api, CreateAccountRequest, DeleteAccountRequest, FlightChangesRequest,
    FlightChangesResponse, FlightId, FlightResponse, Handle, Password, SignInRequest,
    SignInResponse, UpdateFlightRequest, UploadResponse, Visibility,
};

use crate::server::{answer, code, refusal};
use crate::{Core, CoreError};

/// The name the account is stored under.
const FILE: &str = "account.json";

/// How many changes are asked for at a time: the most the server gives.
const CHANGES_AT_A_TIME: usize = 100;

/// A request for the shell to send to the Venti server.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServerRequest {
    /// The HTTP method: `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.
    pub method: String,
    /// The path and query on the server.
    pub path: String,
    /// The body, if the request has one. It may hold a password: do not
    /// log it.
    pub body: Option<Vec<u8>>,
    /// The body's content type, if it has a body.
    pub content_type: Option<String>,
    /// Whether the session token goes with it, as
    /// `Authorization: Bearer <token>`.
    pub needs_token: bool,
}

/// Who may see a flight on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FlightVisibility {
    /// Only its pilot.
    Private,
    /// Anyone who has its link. It is listed nowhere.
    Unlisted,
    /// Anyone. It is listed on its pilot's page.
    Public,
}

/// The account this device is signed in to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccountView {
    /// The server, as an address without a slash at the end.
    pub server: String,
    /// The pilot's handle.
    pub handle: String,
    /// The name the pilot is shown by.
    pub display_name: String,
    /// Whether the session has ended and the pilot has to sign in again.
    /// The account's data on this device is kept meanwhile.
    pub needs_sign_in: bool,
    /// How many of the logbook's flights the server has.
    pub flights_on_server: u32,
    /// How many of the logbook's flights still wait to be sent.
    pub flights_to_send: u32,
    /// When the logbook was last brought in step, seconds since
    /// 1970-01-01 UTC.
    pub last_sync_unix_s: Option<i64>,
}

/// The result of signing in or creating an account.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SignedIn {
    /// The account.
    pub account: AccountView,
    /// The session token. The shell keeps it where the device keeps
    /// secrets and sends it with requests that need it. It is shown once.
    pub token: String,
}

/// How a flight is shared.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FlightSharing {
    /// Who may see it.
    pub visibility: FlightVisibility,
    /// The link to its page on the server. Useful to others only when the
    /// flight is not private.
    pub link: String,
}

/// What a sync did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
pub struct SyncSummary {
    /// Flights sent to the server.
    pub sent: u32,
    /// Flights fetched from the server.
    pub fetched: u32,
    /// Flights removed here because they were deleted on another device.
    pub removed_here: u32,
    /// Flights removed on the server because they were deleted here.
    pub removed_there: u32,
    /// Flights the server would not take.
    pub refused: u32,
}

/// What the shell does next in a sync.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SyncStep {
    /// Send this request and pass the answer to `receive`.
    Send {
        /// The request.
        request: ServerRequest,
    },
    /// The logbook and the server are in step.
    Finished {
        /// What was done.
        summary: SyncSummary,
    },
    /// It stopped. What was done so far is kept.
    Failed {
        /// Why, in words for the pilot.
        message: String,
        /// Whether the session has ended: the pilot has to sign in again.
        needs_sign_in: bool,
    },
}

/// What the device knows of the account, as stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Stored {
    server: String,
    handle: String,
    display_name: String,
    /// The session's identifier, for signing it out. Empty once the
    /// session has ended.
    #[serde(default)]
    session_id: String,
    /// Where the server's list of changes was read up to.
    #[serde(default)]
    cursor: Option<String>,
    /// The flights the server has, by identity.
    #[serde(default)]
    flights: BTreeMap<String, Remote>,
    /// Flights deleted on this device that the server still has: identity
    /// and address.
    #[serde(default)]
    deleted_here: BTreeMap<String, String>,
    /// Flights the server would not take. They are not sent again.
    #[serde(default)]
    refused: BTreeSet<String>,
    #[serde(default)]
    last_sync_unix_s: Option<i64>,
}

/// A flight on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Remote {
    /// Its address there.
    id: String,
    visibility: Visibility,
}

#[uniffi::export]
impl Core {
    /// The account this device is signed in to, if any.
    pub fn account(&self) -> Result<Option<AccountView>, CoreError> {
        Ok(self
            .stored_account()?
            .map(|stored| self.account_view(&stored)))
    }

    /// The request that signs this device in. `device` is what to call it
    /// in the account's list of devices.
    pub fn sign_in_request(
        &self,
        handle: String,
        password: String,
        device: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        if handle.trim().is_empty() || password.is_empty() {
            return Err(invalid("Enter your handle and your password."));
        }
        json_request(
            "POST",
            api::SESSIONS_PATH,
            &SignInRequest {
                handle: handle.trim().to_owned(),
                password: Password::new(password),
                device,
            },
            false,
        )
    }

    /// The request that creates an account and signs this device in. What
    /// the server would refuse anyway is refused here, in words.
    pub fn create_account_request(
        &self,
        handle: String,
        display_name: String,
        password: String,
        invite: Option<String>,
        device: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        let handle =
            Handle::new(&handle).map_err(|error| invalid(&sentence(&error.to_string())))?;
        let display_name = api::parse_display_name(&display_name)
            .map_err(|error| invalid(&sentence(&error.to_string())))?;
        let password = Password::new(password);
        password
            .check()
            .map_err(|error| invalid(&sentence(&error.to_string())))?;
        json_request(
            "POST",
            api::ACCOUNTS_PATH,
            &CreateAccountRequest {
                handle: handle.as_str().to_owned(),
                display_name,
                password,
                invite: invite
                    .map(|invite| invite.trim().to_owned())
                    .filter(|invite| !invite.is_empty()),
                device,
            },
            false,
        )
    }

    /// Reads the server's answer to signing in or creating an account,
    /// and remembers the account. `server` is the address the request was
    /// sent to.
    pub fn signed_in(
        &self,
        server: String,
        status: u16,
        body: Vec<u8>,
    ) -> Result<SignedIn, CoreError> {
        let response: SignInResponse = answer(status, &body)?;
        let server = server.trim().trim_end_matches('/').to_owned();
        let handle = response.account.handle.as_str().to_owned();
        // Signing in again to the same account keeps what is known of it;
        // another account, or another server, starts afresh.
        let mut stored = self
            .stored_account()?
            .filter(|known| known.server == server && known.handle == handle)
            .unwrap_or_default();
        stored.server = server;
        stored.handle = handle;
        stored.display_name = response.account.display_name;
        stored.session_id = response.session.id;
        self.store_account(&stored)?;
        Ok(SignedIn {
            account: self.account_view(&stored),
            token: response.token,
        })
    }

    /// The request that signs this device out on the server, if it is
    /// signed in. Whatever the server answers, call [`Core::signed_out`].
    pub fn sign_out_request(&self) -> Result<Option<ServerRequest>, CoreError> {
        Ok(self
            .stored_account()?
            .filter(|stored| !stored.session_id.is_empty())
            .map(|stored| ServerRequest {
                method: "DELETE".to_owned(),
                path: api::session_path(&stored.session_id),
                body: None,
                content_type: None,
                needs_token: true,
            }))
    }

    /// Forgets the account on this device. The flights in the logbook
    /// stay, and so do their copies on the server.
    pub fn signed_out(&self) -> Result<(), CoreError> {
        self.storage.remove(FILE).map_err(storage_error)
    }

    /// The request that deletes the account on the server, with all its
    /// flights there. It needs the password, not only the session.
    pub fn delete_account_request(&self, password: String) -> Result<ServerRequest, CoreError> {
        json_request(
            "DELETE",
            api::ACCOUNT_PATH,
            &DeleteAccountRequest {
                password: Password::new(password),
            },
            true,
        )
    }

    /// Reads the server's answer to deleting the account. On success the
    /// account is forgotten here too; the logbook on this device stays.
    pub fn account_deleted(&self, status: u16, body: Vec<u8>) -> Result<(), CoreError> {
        if !(200..300).contains(&status) {
            return Err(refusal(status, &body));
        }
        self.signed_out()
    }

    /// How a flight of the logbook is shared, if the server has it.
    pub fn flight_sharing(&self, id: String) -> Result<Option<FlightSharing>, CoreError> {
        Ok(self.stored_account()?.and_then(|stored| {
            let remote = stored.flights.get(&id)?;
            sharing(&stored.server, remote)
        }))
    }

    /// The request that changes who may see a flight.
    pub fn share_request(
        &self,
        id: String,
        visibility: FlightVisibility,
    ) -> Result<ServerRequest, CoreError> {
        let stored = self.stored_account()?.ok_or_else(not_signed_in)?;
        let remote = stored.flights.get(&id).ok_or_else(|| CoreError::Server {
            message: "This flight is not on the server yet. Sync first.".to_owned(),
        })?;
        let address: FlightId = remote.id.parse().map_err(|_| CoreError::Server {
            message: "The server's address for this flight could not be read.".to_owned(),
        })?;
        json_request(
            "PATCH",
            &api::flight_path(&address),
            &UpdateFlightRequest {
                visibility: match visibility {
                    FlightVisibility::Private => Visibility::Private,
                    FlightVisibility::Unlisted => Visibility::Unlisted,
                    FlightVisibility::Public => Visibility::Public,
                },
            },
            true,
        )
    }

    /// Reads the server's answer to [`Core::share_request`] and remembers
    /// how the flight is now shared.
    pub fn shared(
        &self,
        id: String,
        status: u16,
        body: Vec<u8>,
    ) -> Result<FlightSharing, CoreError> {
        let response: FlightResponse = answer(status, &body)?;
        let mut stored = self.stored_account()?.ok_or_else(not_signed_in)?;
        let remote = Remote {
            id: response.flight.id.to_string(),
            visibility: response.flight.visibility,
        };
        let now = sharing(&stored.server, &remote).ok_or_else(|| CoreError::Server {
            message: "The server's answer could not be read. The app may be older than the server."
                .to_owned(),
        })?;
        stored.flights.insert(id, remote);
        self.store_account(&stored)?;
        Ok(now)
    }

    /// Starts bringing the logbook in step with the server.
    pub fn sync(self: Arc<Self>) -> Arc<FlightSync> {
        Arc::new(FlightSync {
            core: self,
            progress: Mutex::new(Progress::default()),
        })
    }
}

impl Core {
    fn stored_account(&self) -> Result<Option<Stored>, CoreError> {
        let Some(bytes) = self.storage.read(FILE).map_err(storage_error)? else {
            return Ok(None);
        };
        // A file that cannot be read is as good as no account: the pilot
        // signs in again and nothing but the sync's memory is lost.
        Ok(serde_json::from_slice(&bytes).ok())
    }

    fn store_account(&self, stored: &Stored) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(stored).map_err(|error| CoreError::Storage {
            message: format!("The account could not be saved: {error}."),
        })?;
        self.storage.write(FILE, &bytes).map_err(storage_error)
    }

    fn account_view(&self, stored: &Stored) -> AccountView {
        let here = self.flight_ids();
        let on_server = here
            .iter()
            .filter(|id| stored.flights.contains_key(*id))
            .count();
        let to_send = here
            .iter()
            .filter(|id| !stored.flights.contains_key(*id) && !stored.refused.contains(*id))
            .count();
        AccountView {
            server: stored.server.clone(),
            handle: stored.handle.clone(),
            display_name: stored.display_name.clone(),
            needs_sign_in: stored.session_id.is_empty(),
            flights_on_server: u32::try_from(on_server).unwrap_or(u32::MAX),
            flights_to_send: u32::try_from(to_send).unwrap_or(u32::MAX),
            last_sync_unix_s: stored.last_sync_unix_s,
        }
    }

    /// The identifiers of the flights in the logbook.
    fn flight_ids(&self) -> Vec<String> {
        self.logbook()
            .years
            .into_iter()
            .flat_map(|year| year.flights)
            .map(|flight| flight.id)
            .collect()
    }

    /// Notes that a flight was deleted on this device, so that the next
    /// sync deletes it on the server instead of fetching it back.
    pub(crate) fn note_flight_deleted(&self, id: &str) {
        let Ok(Some(mut stored)) = self.stored_account() else {
            return;
        };
        stored.refused.remove(id);
        if let Some(remote) = stored.flights.remove(id) {
            stored.deleted_here.insert(id.to_owned(), remote.id);
        }
        // If this cannot be saved the flight comes back at the next sync,
        // which is better than failing the deletion the pilot asked for.
        let _ = self.store_account(&stored);
    }
}

/// One run of bringing the logbook in step with the server.
#[derive(Debug, uniffi::Object)]
pub struct FlightSync {
    core: Arc<Core>,
    progress: Mutex<Progress>,
}

#[derive(Debug, Default)]
struct Progress {
    now_unix_s: i64,
    waiting_for: Option<Waiting>,
    /// Whether the server's changes have been read to their end.
    pulled: bool,
    /// Flights to fetch: identity and address.
    to_fetch: VecDeque<(String, String)>,
    /// Flights to send, once the list has been made.
    to_send: Option<VecDeque<String>>,
    summary: SyncSummary,
}

/// The request whose answer is awaited.
#[derive(Debug)]
enum Waiting {
    Deletion { identity: String },
    Changes,
    File { identity: String },
    Upload { identity: String },
}

#[uniffi::export]
impl FlightSync {
    /// Begins. `now_unix_s` is the current time, kept as the time of the
    /// sync.
    pub fn start(&self, now_unix_s: i64) -> SyncStep {
        let mut progress = self.lock();
        *progress = Progress {
            now_unix_s,
            ..Progress::default()
        };
        self.next(&mut progress)
    }

    /// Takes the server's answer to the last request. `retry_after_s` is
    /// the answer's `Retry-After` header, if it had one.
    pub fn receive(&self, status: u16, body: Vec<u8>, retry_after_s: Option<u32>) -> SyncStep {
        let mut progress = self.lock();
        let Some(waiting) = progress.waiting_for.take() else {
            return failed("Nothing was asked of the server.", false);
        };
        let mut stored = match self.core.stored_account() {
            Ok(Some(stored)) => stored,
            Ok(None) => return failed("This device is not signed in.", true),
            Err(error) => return failed(&error.to_string(), false),
        };

        // What holds for every request.
        if status == 401 {
            stored.session_id.clear();
            let _ = self.core.store_account(&stored);
            return failed("This device is no longer signed in. Sign in again.", true);
        }
        if status == 429 || status == 503 {
            return failed(
                &match retry_after_s {
                    Some(seconds) if seconds > 90 => format!(
                        "The server asks to wait {} minutes. What was done so far is kept.",
                        seconds.div_ceil(60)
                    ),
                    Some(seconds) => format!(
                        "The server asks to wait {seconds} seconds. What was done so far is kept."
                    ),
                    None => "The server is busy. Try again in a moment.".to_owned(),
                },
                false,
            );
        }

        match waiting {
            Waiting::Deletion { identity } => {
                // Gone already is as good as deleted.
                if !(200..300).contains(&status) && status != 404 {
                    return failed(&refusal(status, &body).to_string(), false);
                }
                stored.deleted_here.remove(&identity);
                progress.summary.removed_there += 1;
            }
            Waiting::Changes => {
                let changes: FlightChangesResponse = match answer(status, &body) {
                    Ok(changes) => changes,
                    Err(error) => return failed(&error.to_string(), false),
                };
                if changes.reset {
                    // The server no longer knows where this device read up
                    // to: what follows is everything it has.
                    stored.flights.clear();
                }
                let here: BTreeSet<String> = self.core.flight_ids().into_iter().collect();
                for flight in changes.flights {
                    // A flight deleted here and not yet deleted there is
                    // on its way out; it is not fetched back.
                    if stored.deleted_here.contains_key(&flight.identity) {
                        continue;
                    }
                    if !here.contains(&flight.identity) {
                        progress
                            .to_fetch
                            .push_back((flight.identity.clone(), flight.id.to_string()));
                    }
                    stored.flights.insert(
                        flight.identity,
                        Remote {
                            id: flight.id.to_string(),
                            visibility: flight.visibility,
                        },
                    );
                }
                for deleted in changes.deleted {
                    stored.deleted_here.remove(&deleted.identity);
                    progress
                        .to_fetch
                        .retain(|(identity, _)| *identity != deleted.identity);
                    // Deleted on another device: gone here too. Only a
                    // flight the server is known to have had is removed,
                    // never one that was only ever on this device.
                    if stored.flights.remove(&deleted.identity).is_some()
                        && here.contains(&deleted.identity)
                        && self.core.logbook.delete(&deleted.identity).is_ok()
                    {
                        progress.summary.removed_here += 1;
                    }
                }
                stored.cursor = Some(changes.cursor);
                progress.pulled = !changes.more;
            }
            Waiting::File { identity } => match status {
                200..=299 => {
                    let text = String::from_utf8_lossy(&body);
                    if self.core.logbook.import(&text).is_ok() {
                        progress.summary.fetched += 1;
                    }
                }
                // Deleted or made private in the meantime: the next sync's
                // changes say so.
                404 => {
                    stored.flights.remove(&identity);
                }
                _ => return failed(&refusal(status, &body).to_string(), false),
            },
            Waiting::Upload { identity } => match status {
                200..=299 => {
                    let response: UploadResponse = match answer(status, &body) {
                        Ok(response) => response,
                        Err(error) => return failed(&error.to_string(), false),
                    };
                    stored.flights.insert(
                        identity,
                        Remote {
                            id: response.flight.id.to_string(),
                            visibility: response.flight.visibility,
                        },
                    );
                    progress.summary.sent += 1;
                }
                // Not a flight the server can take, or too large: it stays
                // on this device and is not sent again.
                413 | 422 => {
                    stored.refused.insert(identity);
                    progress.summary.refused += 1;
                }
                _ => {
                    return failed(
                        &match code(&body) {
                            Some(_) => refusal(status, &body).to_string(),
                            None => format!("The server did not take a flight (status {status})."),
                        },
                        false,
                    );
                }
            },
        }

        if let Err(error) = self.core.store_account(&stored) {
            return failed(&error.to_string(), false);
        }
        self.next(&mut progress)
    }
}

impl FlightSync {
    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The next request, or the end.
    fn next(&self, progress: &mut Progress) -> SyncStep {
        let mut stored = match self.core.stored_account() {
            Ok(Some(stored)) if !stored.session_id.is_empty() => stored,
            Ok(_) => return failed("This device is not signed in.", true),
            Err(error) => return failed(&error.to_string(), false),
        };

        // 1. What was deleted here is deleted there.
        if let Some((identity, address)) = stored.deleted_here.first_key_value() {
            let request = ServerRequest {
                method: "DELETE".to_owned(),
                path: format!("{}/{address}", api::FLIGHTS_PATH),
                body: None,
                content_type: None,
                needs_token: true,
            };
            progress.waiting_for = Some(Waiting::Deletion {
                identity: identity.clone(),
            });
            return SyncStep::Send { request };
        }

        // 2. What changed there is learned.
        if !progress.pulled {
            progress.waiting_for = Some(Waiting::Changes);
            return SyncStep::Send {
                request: ServerRequest {
                    method: "GET".to_owned(),
                    path: FlightChangesRequest {
                        since: stored.cursor.clone(),
                        limit: Some(CHANGES_AT_A_TIME),
                    }
                    .path_and_query(),
                    body: None,
                    content_type: None,
                    needs_token: true,
                },
            };
        }

        // 3. What this device lacks is fetched.
        if let Some((identity, address)) = progress.to_fetch.pop_front() {
            progress.waiting_for = Some(Waiting::File { identity });
            return SyncStep::Send {
                request: ServerRequest {
                    method: "GET".to_owned(),
                    path: format!("{}/{address}/igc", api::FLIGHTS_PATH),
                    body: None,
                    content_type: None,
                    needs_token: true,
                },
            };
        }

        // 4. What the server lacks is sent, one flight after another.
        let to_send = progress.to_send.get_or_insert_with(|| {
            self.core
                .flight_ids()
                .into_iter()
                .filter(|id| !stored.flights.contains_key(id) && !stored.refused.contains(id))
                .collect()
        });
        while let Some(identity) = to_send.pop_front() {
            // A flight whose file cannot be read stays where it is.
            let Ok(file) = self.core.flight_file(identity.clone()) else {
                continue;
            };
            progress.waiting_for = Some(Waiting::Upload { identity });
            return SyncStep::Send {
                request: ServerRequest {
                    method: "POST".to_owned(),
                    path: api::FLIGHTS_PATH.to_owned(),
                    body: Some(file.igc_text.into_bytes()),
                    content_type: Some("application/octet-stream".to_owned()),
                    needs_token: true,
                },
            };
        }

        stored.last_sync_unix_s = Some(progress.now_unix_s);
        if let Err(error) = self.core.store_account(&stored) {
            return failed(&error.to_string(), false);
        }
        SyncStep::Finished {
            summary: progress.summary,
        }
    }
}

fn failed(message: &str, needs_sign_in: bool) -> SyncStep {
    SyncStep::Failed {
        message: message.to_owned(),
        needs_sign_in,
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::Server {
        message: message.to_owned(),
    }
}

fn not_signed_in() -> CoreError {
    CoreError::Server {
        message: "This device is not signed in.".to_owned(),
    }
}

fn storage_error(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The account could not be read or written: {error}."),
    }
}

/// A rule of the API as a sentence: a capital at the start, a full stop
/// at the end.
fn sentence(rule: &str) -> String {
    let mut letters = rule.chars();
    match letters.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), letters.as_str()),
        None => String::new(),
    }
}

fn json_request<T: Serialize>(
    method: &str,
    path: &str,
    body: &T,
    needs_token: bool,
) -> Result<ServerRequest, CoreError> {
    let body = serde_json::to_vec(body).map_err(|_| CoreError::Server {
        message: "The request could not be put together.".to_owned(),
    })?;
    Ok(ServerRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        body: Some(body),
        content_type: Some("application/json".to_owned()),
        needs_token,
    })
}

/// How a flight on the server is shared, with the link to its page.
/// `None` for a setting this version does not know.
fn sharing(server: &str, remote: &Remote) -> Option<FlightSharing> {
    let visibility = match remote.visibility {
        Visibility::Private => FlightVisibility::Private,
        Visibility::Unlisted => FlightVisibility::Unlisted,
        Visibility::Public => FlightVisibility::Public,
        Visibility::Unknown => return None,
    };
    Some(FlightSharing {
        visibility,
        link: format!("{server}/f/{}", remote.id),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    const SERVER: &str = "http://127.0.0.1:8080";
    const NOW: i64 = 1_795_028_700;

    /// The server's own recorded answers, which its tests keep current.
    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    /// The sample flight as the apps bundle it: the development server's
    /// demo account has it too.
    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    fn on_another_day(day: &str) -> String {
        sample().replace("040726", day)
    }

    fn signed_in(core: &Arc<Core>) -> SignedIn {
        core.signed_in(format!("{SERVER}/"), 201, golden("sign_in.json"))
            .unwrap()
    }

    fn request(step: &SyncStep) -> &ServerRequest {
        match step {
            SyncStep::Send { request } => request,
            other => panic!("expected a request, got {other:?}"),
        }
    }

    /// An answer that says nothing more changed.
    fn no_more_changes() -> Vec<u8> {
        json!({"flights": [], "deleted": [], "cursor": "the-end", "more": false, "reset": false})
            .to_string()
            .into_bytes()
    }

    #[test]
    fn signing_in_remembers_the_account_and_hands_the_token_to_the_shell() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert_eq!(core.account().unwrap(), None);

        let asked = core
            .sign_in_request(
                " demo ".to_owned(),
                "demo-pilot-password".to_owned(),
                Some("iPhone".to_owned()),
            )
            .unwrap();
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("POST", "/v1/sessions")
        );
        assert!(!asked.needs_token);
        let body: Value = serde_json::from_slice(asked.body.as_ref().unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"handle": "demo", "password": "demo-pilot-password", "device": "iPhone"})
        );
        assert!(
            core.sign_in_request(String::new(), "x".to_owned(), None)
                .is_err()
        );

        let signed = signed_in(&core);
        assert!(signed.token.starts_with("venti_"));
        assert_eq!(signed.account.handle, "demo");
        assert_eq!(signed.account.display_name, "Demo Pilot");
        assert_eq!(signed.account.server, SERVER);
        assert!(!signed.account.needs_sign_in);

        // The account is remembered across a restart; the token is not in it.
        let stored = std::fs::read_to_string(folder.path().join("account.json")).unwrap();
        assert!(!stored.contains("venti_"), "{stored}");
        drop(core);
        let core = self::core(&folder);
        assert_eq!(core.account().unwrap().unwrap().handle, "demo");

        // Signing out asks the server to end this session, then forgets.
        let out = core.sign_out_request().unwrap().unwrap();
        assert_eq!(out.method, "DELETE");
        assert_eq!(out.path, "/v1/sessions/3kq9x2m7r4t8v1w6y0a5b3c2d4");
        assert!(out.needs_token);
        core.signed_out().unwrap();
        assert_eq!(core.account().unwrap(), None);
        assert_eq!(core.sign_out_request().unwrap(), None);
    }

    #[test]
    fn a_wrong_password_and_other_refusals_are_said_in_words() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let refuse = |status: u16, code: &str| {
            core.signed_in(
                SERVER.to_owned(),
                status,
                json!({"error": {"code": code, "message": "for developers"}})
                    .to_string()
                    .into_bytes(),
            )
            .unwrap_err()
            .to_string()
        };
        assert_eq!(
            refuse(401, "invalid_credentials"),
            "The handle or the password is wrong."
        );
        assert!(refuse(429, "rate_limited").contains("Too many attempts"));
        assert!(refuse(403, "invalid_invite").contains("invite code"));
        assert!(refuse(409, "handle_taken").contains("taken"));
        assert!(refuse(403, "registration_closed").contains("no new accounts"));
        assert_eq!(core.account().unwrap(), None);
    }

    #[test]
    fn an_account_the_server_would_refuse_is_refused_before_it_is_asked() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let create = |handle: &str, name: &str, password: &str| {
            core.create_account_request(
                handle.to_owned(),
                name.to_owned(),
                password.to_owned(),
                Some(" 7k3m-9q2x ".to_owned()),
                None,
            )
        };
        let said = |result: Result<ServerRequest, CoreError>| result.unwrap_err().to_string();
        assert_eq!(
            said(create("ab", "A", "correct horse battery staple")),
            "A handle has at least 3 characters."
        );
        assert!(
            said(create("9lives", "A", "correct horse battery staple"))
                .contains("starts with a letter")
        );
        assert!(
            said(create("dario", "  ", "correct horse battery staple")).contains("display name")
        );
        assert!(said(create("dario", "Dario", "too short")).contains("at least 15 characters"));

        let asked = create("@Dario_B", " Dario B. ", "correct horse battery staple").unwrap();
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("POST", "/v1/accounts")
        );
        let body: Value = serde_json::from_slice(asked.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["handle"], "dario_b");
        assert_eq!(body["display_name"], "Dario B.");
        assert_eq!(body["invite"], "7k3m-9q2x");
    }

    #[test]
    fn a_sync_fetches_what_is_missing_here_and_sends_what_is_missing_there() {
        let folder = TempFolder::new();
        let core = core(&folder);
        // On this device: the sample flight, which the server has too, and
        // one from 2025, which it lacks.
        let shared = core.import_flight(sample()).unwrap().id;
        let only_here = core.import_flight(on_another_day("030725")).unwrap().id;
        signed_in(&core);
        let before = core.account().unwrap().unwrap();
        assert_eq!((before.flights_on_server, before.flights_to_send), (0, 2));

        let sync = core.clone().sync();
        // First the server's changes, from the beginning.
        let step = sync.start(NOW);
        let asked = request(&step);
        assert_eq!(asked.method, "GET");
        assert_eq!(asked.path, "/v1/flights/changes?limit=100");
        assert!(asked.needs_token);

        // The server's recorded answer: two flights, and more to come.
        let step = sync.receive(200, golden("flight_changes.json"), None);
        assert!(
            request(&step)
                .path
                .starts_with("/v1/flights/changes?since="),
            "{step:?}"
        );
        let step = sync.receive(200, no_more_changes(), None);

        // One of the two is not on this device: its file is fetched.
        let asked = request(&step);
        assert_eq!(asked.method, "GET");
        assert_eq!(asked.path, "/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/igc");
        let step = sync.receive(200, on_another_day("150826").into_bytes(), None);

        // What the server lacks is sent, each as the file it is: the
        // flight from 2025, and in this test also the file just fetched,
        // which is a stand-in and not the flight the server described.
        let mut sent = Vec::new();
        let mut step = step;
        while let SyncStep::Send { request: asked } = &step {
            assert_eq!(
                (asked.method.as_str(), asked.path.as_str()),
                ("POST", "/v1/flights")
            );
            assert_eq!(
                asked.content_type.as_deref(),
                Some("application/octet-stream")
            );
            assert!(asked.needs_token);
            sent.push(asked.body.clone().unwrap());
            step = sync.receive(201, golden("upload.json"), None);
        }
        assert_eq!(sent.len(), 2);
        assert!(sent.contains(&on_another_day("030725").into_bytes()));
        assert_eq!(
            step,
            SyncStep::Finished {
                summary: SyncSummary {
                    sent: 2,
                    fetched: 1,
                    ..SyncSummary::default()
                }
            }
        );
        assert_eq!(core.logbook().totals.flights, 3);
        let after = core.account().unwrap().unwrap();
        assert_eq!(after.flights_to_send, 0);
        assert_eq!(after.last_sync_unix_s, Some(NOW));

        // How each is shared is known: the sample by its link, the one
        // just sent as private.
        let sample_shared = core.flight_sharing(shared).unwrap().unwrap();
        assert_eq!(sample_shared.visibility, FlightVisibility::Unlisted);
        assert_eq!(
            sample_shared.link,
            "http://127.0.0.1:8080/f/7cf5ytrw45b0aeb5tcr8wet2km"
        );
        let sent = core.flight_sharing(only_here).unwrap().unwrap();
        assert_eq!(sent.visibility, FlightVisibility::Private);

        // The next sync asks only for what changed since, and has nothing to do.
        let sync = core.clone().sync();
        let step = sync.start(NOW + 60);
        assert_eq!(
            request(&step).path,
            "/v1/flights/changes?since=the-end&limit=100"
        );
        assert_eq!(
            sync.receive(200, no_more_changes(), None),
            SyncStep::Finished {
                summary: SyncSummary::default()
            }
        );
    }

    #[test]
    fn deletions_travel_both_ways() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let sample_id = core.import_flight(sample()).unwrap().id;
        let never_sent = core.import_flight(on_another_day("030725")).unwrap().id;
        signed_in(&core);
        // After a first sync the server is known to have the sample.
        let sync = core.clone().sync();
        sync.start(NOW);
        sync.receive(200, golden("flight_changes.json"), None);
        let step = sync.receive(200, no_more_changes(), None);
        assert!(request(&step).path.ends_with("/igc"));
        // Cut off here: what was learned so far is kept.
        drop(sync);

        // Deleted on this device: the next sync deletes it there first,
        // and does not fetch it back.
        core.delete_flight(sample_id.clone()).unwrap();
        let sync = core.clone().sync();
        let step = sync.start(NOW);
        let asked = request(&step);
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("DELETE", "/v1/flights/7cf5ytrw45b0aeb5tcr8wet2km")
        );
        let step = sync.receive(204, Vec::new(), None);
        assert!(request(&step).path.starts_with("/v1/flights/changes"));

        // Meanwhile another device deleted the public flight, and the
        // server reports the sample's deletion back.
        let changes = json!({
            "flights": [],
            "deleted": [
                {"id": "7cf5ytrw45b0aeb5tcr8wet2km", "identity": sample_id},
                {"id": "5t1eshypdtvrq7rxw39a18vsrg", "identity": "0f0d88442e01f52f291d25b2660b8bed"},
                // A flight that was only ever on this device is not touched
                // by a deletion the server reports for its identity.
                {"id": "0000000000000000000000000a", "identity": never_sent}
            ],
            "cursor": "later", "more": false, "reset": false
        });
        let step = sync.receive(200, changes.to_string().into_bytes(), None);
        // The flight that was never sent is still here, and is sent now.
        assert_eq!(request(&step).method, "POST");
        let step = sync.receive(201, golden("upload.json"), None);
        assert_eq!(
            step,
            SyncStep::Finished {
                summary: SyncSummary {
                    sent: 1,
                    removed_there: 1,
                    ..SyncSummary::default()
                }
            }
        );
        assert_eq!(core.logbook().totals.flights, 1);
    }

    #[test]
    fn a_flight_deleted_on_another_device_goes_here_too() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let sample_id = core.import_flight(sample()).unwrap().id;
        signed_in(&core);
        let sync = core.clone().sync();
        sync.start(NOW);
        let changes = json!({
            "flights": [serde_json::from_slice::<Value>(&golden("flight_changes.json")).unwrap()["flights"][1]],
            "deleted": [], "cursor": "one", "more": false, "reset": false
        });
        assert!(matches!(
            sync.receive(200, changes.to_string().into_bytes(), None),
            SyncStep::Finished { .. }
        ));
        assert_eq!(core.logbook().totals.flights, 1);

        let sync = core.clone().sync();
        sync.start(NOW);
        let changes = json!({
            "flights": [],
            "deleted": [{"id": "7cf5ytrw45b0aeb5tcr8wet2km", "identity": sample_id}],
            "cursor": "two", "more": false, "reset": false
        });
        assert_eq!(
            sync.receive(200, changes.to_string().into_bytes(), None),
            SyncStep::Finished {
                summary: SyncSummary {
                    removed_here: 1,
                    ..SyncSummary::default()
                }
            }
        );
        assert_eq!(core.logbook().totals.flights, 0);
    }

    #[test]
    fn a_sync_stops_politely_and_keeps_what_it_did() {
        let folder = TempFolder::new();
        let core = core(&folder);
        core.import_flight(sample()).unwrap();
        core.import_flight(on_another_day("030725")).unwrap();
        signed_in(&core);

        // The server asks to wait in the middle of sending.
        let sync = core.clone().sync();
        sync.start(NOW);
        let step = sync.receive(200, no_more_changes(), None);
        assert_eq!(request(&step).method, "POST");
        let step = sync.receive(201, golden("upload.json"), None);
        assert_eq!(request(&step).method, "POST");
        let step = sync.receive(429, Vec::new(), Some(30));
        assert_eq!(
            step,
            SyncStep::Failed {
                message: "The server asks to wait 30 seconds. What was done so far is kept."
                    .to_owned(),
                needs_sign_in: false
            }
        );
        assert_eq!(core.account().unwrap().unwrap().flights_to_send, 1);

        // A file the server will not take is not sent again and again.
        let sync = core.clone().sync();
        sync.start(NOW);
        let step = sync.receive(200, no_more_changes(), None);
        assert_eq!(request(&step).method, "POST");
        let refused = json!({"error": {"code": "invalid_flight", "message": "x"}});
        assert_eq!(
            sync.receive(422, refused.to_string().into_bytes(), None),
            SyncStep::Finished {
                summary: SyncSummary {
                    refused: 1,
                    ..SyncSummary::default()
                }
            }
        );
        assert_eq!(core.account().unwrap().unwrap().flights_to_send, 0);

        // A session that has ended is said, and remembered, and nothing
        // more is asked until the pilot has signed in again.
        let sync = core.clone().sync();
        sync.start(NOW);
        let step = sync.receive(401, Vec::new(), None);
        assert!(
            matches!(
                step,
                SyncStep::Failed {
                    needs_sign_in: true,
                    ..
                }
            ),
            "{step:?}"
        );
        assert!(core.account().unwrap().unwrap().needs_sign_in);
        assert!(matches!(
            core.clone().sync().start(NOW),
            SyncStep::Failed {
                needs_sign_in: true,
                ..
            }
        ));
        // Signing in again keeps what was known of the account.
        signed_in(&core);
        let again = core.account().unwrap().unwrap();
        assert!(!again.needs_sign_in);
        assert_eq!(again.flights_on_server, 1);
    }

    #[test]
    fn a_flight_is_shared_by_changing_who_may_see_it() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let id = core.import_flight(sample()).unwrap().id;
        signed_in(&core);
        // Not on the server yet: nothing to share.
        assert_eq!(core.flight_sharing(id.clone()).unwrap(), None);
        assert!(
            core.share_request(id.clone(), FlightVisibility::Public)
                .is_err()
        );

        let sync = core.clone().sync();
        sync.start(NOW);
        let step = sync.receive(200, no_more_changes(), None);
        assert_eq!(request(&step).method, "POST");
        sync.receive(201, golden("upload.json"), None);

        let asked = core
            .share_request(id.clone(), FlightVisibility::Public)
            .unwrap();
        assert_eq!(asked.method, "PATCH");
        assert_eq!(asked.path, "/v1/flights/wkvydb8am8wmdcxzds9vkrw5kr");
        assert_eq!(
            asked.body.as_deref(),
            Some(br#"{"visibility":"public"}"#.as_slice())
        );
        assert!(asked.needs_token);

        let now = core.shared(id.clone(), 200, golden("flight.json")).unwrap();
        assert_eq!(now.visibility, FlightVisibility::Public);
        assert_eq!(
            now.link,
            "http://127.0.0.1:8080/f/5t1eshypdtvrq7rxw39a18vsrg"
        );
        assert_eq!(core.flight_sharing(id).unwrap(), Some(now));
    }

    #[test]
    fn deleting_the_account_needs_the_password_and_forgets_it_here() {
        let folder = TempFolder::new();
        let core = core(&folder);
        core.import_flight(sample()).unwrap();
        signed_in(&core);

        let asked = core
            .delete_account_request("demo-pilot-password".to_owned())
            .unwrap();
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("DELETE", "/v1/account")
        );
        assert!(asked.needs_token);

        let wrong = json!({"error": {"code": "wrong_password", "message": "x"}});
        let error = core
            .account_deleted(403, wrong.to_string().into_bytes())
            .unwrap_err();
        assert_eq!(error.to_string(), "The password is wrong.");
        assert!(core.account().unwrap().is_some());

        core.account_deleted(204, Vec::new()).unwrap();
        assert_eq!(core.account().unwrap(), None);
        // The logbook on this device is the pilot's and stays.
        assert_eq!(core.logbook().totals.flights, 1);
    }
}
