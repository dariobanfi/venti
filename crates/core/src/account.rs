//! The pilot's account on a Venti server, and keeping the logbook in step
//! with it.
//!
//! Without an account everything stays on the device. With one, the
//! pilot's flight files are copied to the server, so that they appear on
//! the pilot's other devices and can be shared. A flight is public on the
//! server from its upload unless the pilot keeps new flights private
//! ([`Core::new_flights_public_request`]); all of them are made public or
//! private at once with [`Core::all_flights_request`].
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
    self as api, AccountResponse, CreateAccountRequest, DeleteAccountRequest, FlightChangesRequest,
    FlightChangesResponse, FlightId, FlightResponse, Handle, Password, SignInRequest,
    SignInResponse, UpdateFlightRequest, UploadResponse, Visibility,
};

use crate::live::one_line;
use crate::logbook::LogbookError;
use crate::server::{answer, code, refusal};
use crate::{Core, CoreError};

/// The name the account is stored under.
const FILE: &str = "account.json";

/// How many changes are asked for at a time: the most the server gives.
const CHANGES_AT_A_TIME: usize = 100;

/// A request for the shell to send to the Venti server.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ServerRequest {
    /// The HTTP method: `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.
    pub method: String,
    /// The path and query on the server.
    pub path: String,
    /// The body, if the request has one. It may hold a password: do not
    /// log it.
    #[cfg_attr(feature = "wasm", serde(serialize_with = "as_bytes"))]
    #[cfg_attr(feature = "wasm", tsify(type = "Uint8Array | undefined"))]
    pub body: Option<Vec<u8>>,
    /// The body's content type, if it has a body.
    pub content_type: Option<String>,
    /// Whether the session token goes with it, as
    /// `Authorization: Bearer <token>`.
    pub needs_token: bool,
}

/// Who may see a flight on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
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
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
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
    /// Whether the server says this account is one of Venti's partners
    /// for flying sites: it adds sites to the partners' layer and
    /// corrects sites of both layers. As the server said it last: at the
    /// sign-in, or at the newest read of the account
    /// ([`Core::account_request`]). `false` for every other account, and
    /// for an account that an earlier build stored, until it is read
    /// again. What a shell offers for a site is not worked out from this:
    /// ask [`Core::site_offers`] and [`Core::site_adding`].
    #[uniffi(default = false)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub may_edit_partner_sites: bool,
    /// The email of the Google account the account signs in with, if it
    /// is linked to one, as the server said it last. A shell offers "Link
    /// Google" where it is `None` and the server offers Google.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub google_email: Option<String>,
    /// Whether a flight sent to the server is public from the start: the
    /// pilot's own setting, as the server said it last. On for every
    /// account until the pilot switches it off
    /// ([`Core::new_flights_public_request`]); read as on for an account
    /// an earlier build stored, as the server has it.
    #[uniffi(default = true)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub new_flights_public: bool,
    /// The pilot's country, an ISO 3166-1 alpha-2 code, as the server said
    /// it last: shown with their name on their public flights and page
    /// once set ([`Core::country_request`]). A shell draws its flag.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub country: Option<String>,
    /// The class of the glider the pilot flies now, as the server said it
    /// last; a flight sent to the server takes it
    /// ([`Core::glider_class_request`]).
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub glider_class: Option<crate::GliderClassKind>,
}

/// The words of who sees a pilot's flights, the same on every platform:
/// the setting for new flights and the change of all of them at once.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightVisibilityWords {
    /// The switch's label.
    pub new_flights_public: String,
    /// The one line under it.
    pub new_flights_public_line: String,
    /// The button that makes every flight public; ask
    /// [`all_flights_question`] before sending.
    pub make_all_public: String,
    /// The button that makes every flight private.
    pub make_all_private: String,
}

/// The words of who sees a pilot's flights.
#[uniffi::export]
pub fn flight_visibility_words() -> FlightVisibilityWords {
    FlightVisibilityWords {
        new_flights_public: "New flights are public".to_owned(),
        new_flights_public_line: "Switch off to keep new flights to yourself.".to_owned(),
        make_all_public: "Make all public".to_owned(),
        make_all_private: "Make all private".to_owned(),
    }
}

/// The question asked before every flight is made public or private:
/// "Make all 101 flights public?". `flights` is how many the account has
/// on the server, as the shell knows it ([`AccountView::flights_on_server`]
/// on a phone).
#[uniffi::export]
pub fn all_flights_question(flights: u32, visibility: FlightVisibility) -> String {
    let to = visibility_word(visibility);
    match flights {
        0 => format!("Make all flights {to}?"),
        1 => format!("Make your flight {to}?"),
        n => format!("Make all {n} flights {to}?"),
    }
}

/// The word for who may see a flight, as a line says it.
fn visibility_word(visibility: FlightVisibility) -> &'static str {
    match visibility {
        FlightVisibility::Private => "private",
        FlightVisibility::Unlisted => "shared by link",
        FlightVisibility::Public => "public",
    }
}

/// How the account's flights stand after they were all made public or
/// private ([`Core::all_flights_request`]).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AllFlightsSet {
    /// What was asked for.
    pub visibility: FlightVisibility,
    /// How many of the account's flights are so now, those that were so
    /// before included.
    pub flights: u32,
    /// How many the request changed.
    pub changed: u32,
    /// How many stay private because whoever runs the server took them
    /// down.
    pub taken_down: u32,
    /// The answer in a line for the pilot: "101 flights are public".
    pub line: String,
}

/// The line that says how the account's flights stand.
fn all_flights_line(visibility: FlightVisibility, flights: u32, taken_down: u32) -> String {
    let to = visibility_word(visibility);
    let mut line = match flights {
        0 => format!("No flights are {to}"),
        1 => format!("1 flight is {to}"),
        n => format!("{n} flights are {to}"),
    };
    if taken_down > 0 {
        line.push_str(&format!(". {taken_down} taken down stays private"));
    }
    line
}

/// What a sign-in with Google came to: signed in, or a first sign-in
/// that goes on to choosing a handle. Exactly one of the two is there.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GoogleSignIn {
    /// Signed in, as with the password: the account is stored as
    /// [`Core::signed_in`] stores it.
    pub signed_in: Option<SignedIn>,
    /// A Google account no Venti account is linked to yet: show the
    /// handle step, then send [`Core::google_account_request`].
    pub needs_handle: Option<GoogleSignUp>,
}

/// What the handle step after a first sign-in with Google needs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GoogleSignUp {
    /// What carries the Google account on: pass it to
    /// [`Core::google_account_request`]. Good for a quarter of an hour.
    pub signup: String,
    /// A free handle to put in the field, if the server could make one.
    pub suggested_handle: Option<String>,
    /// The line under the field that says where the suggestion is from,
    /// when there is a suggestion.
    pub suggestion_line: Option<String>,
    /// The name the Google account gives, to put in the display name's
    /// field.
    pub name: Option<String>,
}

/// The words of signing in with Google, the same on every platform.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GoogleWords {
    /// The account screen's first button.
    pub continue_with_google: String,
    /// What opens the handle and password form behind it.
    pub with_password: String,
    /// The title of the handle step after a first sign-in with Google.
    pub choose_handle: String,
    /// The line under the title of that step.
    pub choose_handle_line: String,
    /// The button that creates the account.
    pub create_account: String,
    /// The button of a signed-in password account that links Google.
    pub link_google: String,
    /// The button that unlinks it.
    pub unlink_google: String,
}

/// The words of signing in with Google.
#[uniffi::export]
pub fn google_words() -> GoogleWords {
    GoogleWords {
        continue_with_google: "Continue with Google".to_owned(),
        with_password: "Sign in with a handle and password".to_owned(),
        choose_handle: "Choose your handle".to_owned(),
        choose_handle_line:
            "Your handle is how other pilots find you and stands on every flight you share. Your Google email stays private."
                .to_owned(),
        create_account: "Create account".to_owned(),
        link_google: "Link Google".to_owned(),
        unlink_google: "Unlink Google".to_owned(),
    }
}

/// The line under the handle's field for a suggestion the server made.
fn suggestion_line(handle: &str) -> String {
    format!("@{handle} is free. It was made from your Google account; change it if you like.")
}

/// The result of signing in or creating an account.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SignedIn {
    /// The account.
    pub account: AccountView,
    /// The session token. The shell keeps it where the device keeps
    /// secrets and sends it with requests that need it. It is shown once.
    pub token: String,
}

/// How a flight is shared.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlightSharing {
    /// Who may see it.
    pub visibility: FlightVisibility,
    /// The link to its page on the server. Useful to others only when the
    /// flight is not private.
    pub link: String,
    /// The flight's id on the server, the end of `link`: for the calls
    /// that name a flight of the server, such as
    /// [`crate::comments_open_request`], so that no shell reads a link.
    #[uniffi(default = "")]
    pub flight_id: String,
}

/// What a sync did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
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

/// What a sync did, in a line for the pilot: "3 sent, 1 fetched". The same
/// words on every platform.
#[uniffi::export]
pub fn sync_summary_in_words(summary: SyncSummary) -> String {
    let parts: Vec<String> = [
        (summary.sent, "sent"),
        (summary.fetched, "fetched"),
        (summary.removed_here, "removed here"),
        (summary.removed_there, "removed on the server"),
        (summary.refused, "not taken by the server"),
    ]
    .into_iter()
    .filter(|(flights, _)| *flights > 0)
    .map(|(flights, what)| format!("{flights} {what}"))
    .collect();
    if parts.is_empty() {
        "Nothing to send or fetch".to_owned()
    } else {
        parts.join(", ")
    }
}

/// What the shell does next in a sync.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(
    feature = "wasm",
    serde(
        tag = "kind",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
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
    /// When the server last said the account is full. For an hour after,
    /// nothing is sent: every try costs a whole file. Forgotten as soon as
    /// a flight is deleted there or one is taken.
    #[serde(default)]
    full_since_unix_s: Option<i64>,
    #[serde(default)]
    last_sync_unix_s: Option<i64>,
    /// Whether the server says the account is a partner for flying
    /// sites, as it said last. Missing from a record an earlier build
    /// stored, which reads as no.
    #[serde(default)]
    edits_sites: bool,
    /// Whether the server says the account's publishing is suspended, as
    /// it said last.
    #[serde(default)]
    publishing_suspended: bool,
    /// The email of the Google account the account is linked to, as the
    /// server said it last.
    #[serde(default)]
    google_email: Option<String>,
    /// Whether a flight sent to the server is public from the start, as
    /// the server said it last. Missing from a record an earlier build
    /// stored, which reads as on: so it is for every account on the
    /// server until its pilot switches it off.
    #[serde(default = "on")]
    new_flights_public: bool,
    /// The pilot's country, as the server said it last.
    #[serde(default)]
    country: Option<String>,
    /// The class of the glider the pilot flies now, as the server said
    /// it last.
    #[serde(default, with = "crate::own_flights::stored_class")]
    glider_class: Option<crate::GliderClassKind>,
}

/// What a setting that is on unless it was switched off reads as.
fn on() -> bool {
    true
}

/// A flight on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Remote {
    /// Its address there.
    id: String,
    visibility: Visibility,
    /// The name of the site it took off from, as the server said last.
    /// Missing from a record an earlier build stored, and for a flight the
    /// server names no site for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    takeoff_name: Option<String>,
}

/// The name of the site a flight of the server took off from, on one line.
fn takeoff_name(flight: &api::Flight) -> Option<String> {
    flight
        .takeoff_site
        .as_ref()
        .map(|site| one_line(&site.name, 60))
        .filter(|name| !name.is_empty())
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
        stored.edits_sites = response.account.edits_sites;
        stored.publishing_suspended = response.account.publishing_suspended_at.is_some();
        stored.google_email = response.account.google_email;
        stored.new_flights_public = response.account.new_flights_public;
        stored.country.clone_from(&response.account.country);
        stored.glider_class = response
            .account
            .glider_class
            .and_then(crate::own_flights::glider_class_kind);
        stored.session_id = response.session.id;
        self.store_account(&stored)?;
        Ok(SignedIn {
            account: self.account_view(&stored),
            token: response.token,
        })
    }

    /// The request that signs this device in with a Google ID token, which
    /// the shell got from Google for one of the server's client IDs. Its
    /// answer goes to [`Core::google_sign_in`].
    pub fn google_sign_in_request(
        &self,
        id_token: String,
        device: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        if id_token.trim().is_empty() {
            return Err(invalid("Google gave no sign-in. Try again."));
        }
        json_request(
            "POST",
            api::AUTH_GOOGLE_PATH,
            &api::GoogleSignInRequest {
                id_token: id_token.trim().to_owned(),
                device,
            },
            false,
        )
    }

    /// Reads the server's answer to [`Core::google_sign_in_request`]. A
    /// sign-in is stored as [`Core::signed_in`] stores one; a first
    /// sign-in with a Google account goes on to the handle step.
    pub fn google_sign_in(
        &self,
        server: String,
        status: u16,
        body: Vec<u8>,
    ) -> Result<GoogleSignIn, CoreError> {
        if status == 200 {
            let response: api::GoogleSignUpResponse = answer(status, &body)?;
            if response.needs_handle {
                return Ok(GoogleSignIn {
                    signed_in: None,
                    needs_handle: Some(GoogleSignUp {
                        suggestion_line: response.suggested_handle.as_deref().map(suggestion_line),
                        signup: response.signup,
                        suggested_handle: response.suggested_handle,
                        name: response.name,
                    }),
                });
            }
        }
        Ok(GoogleSignIn {
            signed_in: Some(self.signed_in(server, status, body)?),
            needs_handle: None,
        })
    }

    /// The request that creates an account after a first sign-in with
    /// Google, with the handle the pilot chose. Its answer goes to
    /// [`Core::signed_in`], as that of creating an account with a
    /// password. What the server would refuse anyway is refused here, in
    /// words.
    pub fn google_account_request(
        &self,
        signup: String,
        handle: String,
        display_name: String,
        invite: Option<String>,
        device: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        let handle =
            Handle::new(&handle).map_err(|error| invalid(&sentence(&error.to_string())))?;
        let display_name = api::parse_display_name(&display_name)
            .map_err(|error| invalid(&sentence(&error.to_string())))?;
        json_request(
            "POST",
            api::AUTH_GOOGLE_ACCOUNT_PATH,
            &api::CreateGoogleAccountRequest {
                signup,
                handle: handle.as_str().to_owned(),
                display_name,
                invite: invite
                    .map(|invite| invite.trim().to_owned())
                    .filter(|invite| !invite.is_empty()),
                device,
            },
            false,
        )
    }

    /// The request that links the account this device is signed in to
    /// to the Google account of an ID token. Its answer goes to
    /// [`Core::google_linked`].
    pub fn link_google_request(&self, id_token: String) -> Result<ServerRequest, CoreError> {
        if id_token.trim().is_empty() {
            return Err(invalid("Google gave no sign-in. Try again."));
        }
        json_request(
            "POST",
            api::ACCOUNT_GOOGLE_PATH,
            &api::LinkGoogleRequest {
                id_token: id_token.trim().to_owned(),
            },
            true,
        )
    }

    /// Reads the server's answer to [`Core::link_google_request`]: the
    /// account as it is known now, with the Google account's email.
    pub fn google_linked(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Option<AccountView>, CoreError> {
        if !(200..300).contains(&status) {
            return Err(refusal(status, &body));
        }
        self.account_read(status, body)
    }

    /// The request that unlinks the account from its Google account. Its
    /// answer goes to [`Core::google_unlinked`].
    pub fn unlink_google_request(&self) -> ServerRequest {
        ServerRequest {
            method: "DELETE".to_owned(),
            path: api::ACCOUNT_GOOGLE_PATH.to_owned(),
            body: None,
            content_type: None,
            needs_token: true,
        }
    }

    /// Reads the server's answer to [`Core::unlink_google_request`]: the
    /// account as it is known now, without the Google account.
    pub fn google_unlinked(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Option<AccountView>, CoreError> {
        if !(200..300).contains(&status) {
            return Err(refusal(status, &body));
        }
        let Some(mut stored) = self.stored_account()? else {
            return Ok(None);
        };
        stored.google_email = None;
        self.store_account(&stored)?;
        Ok(Some(self.account_view(&stored)))
    }

    /// The request that reads the account from the server, if this
    /// device is signed in: `GET /v1/account`. What the server says of
    /// the account can change while a device stays signed in: its name,
    /// and whether it is a partner of Venti for flying sites. A shell
    /// sends it when it starts and when the account's page opens, and
    /// hands the answer to [`Core::account_read`].
    pub fn account_request(&self) -> Result<Option<ServerRequest>, CoreError> {
        Ok(self
            .stored_account()?
            .filter(|stored| !stored.session_id.is_empty())
            .map(|_| ServerRequest {
                method: "GET".to_owned(),
                path: api::ACCOUNT_PATH.to_owned(),
                body: None,
                content_type: None,
                needs_token: true,
            }))
    }

    /// Reads the server's answer to [`Core::account_request`] and
    /// remembers what it says of the account. The account as it is known
    /// now; `None` when the device was signed out meanwhile.
    ///
    /// An answer that the session is no longer known (`401`) is no
    /// error: the account then says that the pilot has to sign in again.
    /// An answer about another account than the one this device is
    /// signed in to changes nothing.
    ///
    /// # Errors
    ///
    /// Any other refusal of the server, in words; what is stored stays.
    pub fn account_read(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Option<AccountView>, CoreError> {
        let Some(mut stored) = self.stored_account()? else {
            return Ok(None);
        };
        if status == 401 {
            stored.session_id.clear();
            self.store_account(&stored)?;
            return Ok(Some(self.account_view(&stored)));
        }
        let response: AccountResponse = answer(status, &body)?;
        if response.account.handle.as_str() == stored.handle {
            stored.display_name = response.account.display_name;
            stored.edits_sites = response.account.edits_sites;
            stored.publishing_suspended = response.account.publishing_suspended_at.is_some();
            stored.google_email = response.account.google_email;
            stored.new_flights_public = response.account.new_flights_public;
            stored.country.clone_from(&response.account.country);
            stored.glider_class = response
                .account
                .glider_class
                .and_then(crate::own_flights::glider_class_kind);
            self.store_account(&stored)?;
        }
        Ok(Some(self.account_view(&stored)))
    }

    /// The request that sets whether a flight sent to the server from now
    /// on is public from the start: `PATCH /v1/account`. Its answer is the
    /// account, which goes to [`Core::account_read`]; the switch shows
    /// [`AccountView::new_flights_public`] from it. It changes no flight
    /// the server has: [`Core::all_flights_request`] does.
    pub fn new_flights_public_request(&self, public: bool) -> Result<ServerRequest, CoreError> {
        self.stored_account()?.ok_or_else(not_signed_in)?;
        json_request(
            "PATCH",
            api::ACCOUNT_PATH,
            &api::UpdateAccountRequest {
                new_flights_public: Some(public),
                ..api::UpdateAccountRequest::default()
            },
            true,
        )
    }

    /// The request that sets the pilot's country, or clears it with
    /// `None`: two letters, an ISO 3166-1 alpha-2 code such as `DE`, in
    /// either case. Its answer is the account, which goes to
    /// [`Core::account_read`]. Text that is no such code is refused here,
    /// in words.
    pub fn country_request(&self, country: Option<String>) -> Result<ServerRequest, CoreError> {
        self.stored_account()?.ok_or_else(not_signed_in)?;
        let country = country
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                api::country_code(&text)
                    .ok_or_else(|| invalid("A country is two letters, such as DE or AT."))
            })
            .transpose()?;
        json_request(
            "PATCH",
            api::ACCOUNT_PATH,
            &api::UpdateAccountRequest {
                country: Some(country),
                ..api::UpdateAccountRequest::default()
            },
            true,
        )
    }

    /// The request that sets the class of the glider the pilot flies now,
    /// or clears it with `None`. A flight the server takes from then on is
    /// marked with it; flights it has already keep theirs
    /// ([`crate::flight_glider_class_request`] changes one). Its answer goes to
    /// [`Core::account_read`].
    pub fn glider_class_request(
        &self,
        class: Option<crate::GliderClassKind>,
    ) -> Result<ServerRequest, CoreError> {
        self.stored_account()?.ok_or_else(not_signed_in)?;
        json_request(
            "PATCH",
            api::ACCOUNT_PATH,
            &api::UpdateAccountRequest {
                glider_class: Some(class.map(crate::own_flights::wire_class)),
                ..api::UpdateAccountRequest::default()
            },
            true,
        )
    }

    /// The request that makes every flight of the account public, or every
    /// one private: `PATCH /v1/flights`. Ask [`all_flights_question`]
    /// first; the answer goes to [`Core::all_flights_set`]. Shared by link
    /// is not something all flights are made at once.
    pub fn all_flights_request(
        &self,
        visibility: FlightVisibility,
    ) -> Result<ServerRequest, CoreError> {
        self.stored_account()?.ok_or_else(not_signed_in)?;
        let visibility = match visibility {
            FlightVisibility::Public => Visibility::Public,
            FlightVisibility::Private => Visibility::Private,
            FlightVisibility::Unlisted => {
                return Err(invalid("All flights can be made public or private."));
            }
        };
        json_request(
            "PATCH",
            api::FLIGHTS_PATH,
            &api::UpdateFlightsRequest { visibility },
            true,
        )
    }

    /// Reads the server's answer to [`Core::all_flights_request`]: how the
    /// account's flights stand, with the line to show. What this device
    /// knows of who sees each flight follows where the answer leaves no
    /// doubt; the next sync brings each changed flight as it is.
    ///
    /// # Errors
    ///
    /// A refusal of the server, in words: an account whose publishing is
    /// suspended makes no flight public, and nothing changed.
    pub fn all_flights_set(&self, status: u16, body: Vec<u8>) -> Result<AllFlightsSet, CoreError> {
        let response: api::UpdateFlightsResponse = answer(status, &body)?;
        let visibility = match response.visibility {
            Visibility::Public => FlightVisibility::Public,
            Visibility::Private => FlightVisibility::Private,
            Visibility::Unlisted | Visibility::Unknown => {
                return Err(CoreError::Server {
                    message:
                        "The server's answer could not be read. The app may be older than the server."
                            .to_owned(),
                });
            }
        };
        // Every flight is so now, unless some were taken down: which those
        // are, the next sync says.
        if response.taken_down == 0
            && let Some(mut stored) = self.stored_account()?
        {
            for remote in stored.flights.values_mut() {
                remote.visibility = response.visibility;
            }
            self.store_account(&stored)?;
        }
        Ok(AllFlightsSet {
            visibility,
            flights: response.flights,
            changed: response.changed,
            taken_down: response.taken_down,
            line: all_flights_line(visibility, response.flights, response.taken_down),
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
    ///
    /// The pilot's kept sites stay on the device too. What the device
    /// noted of keeping them in step with the account is forgotten, so
    /// that the next sign-in offers the account what the device then
    /// holds ([`crate::KeptSitesSync`]).
    pub fn signed_out(&self) -> Result<(), CoreError> {
        self.forget_kept_sync()?;
        self.forget_alerts()?;
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
                google_id_token: None,
            },
            true,
        )
    }

    /// The request that deletes the account on the server for an account
    /// that signs in with Google: a fresh ID token of its Google account
    /// in place of the password. Its answer goes to
    /// [`Core::account_deleted`].
    pub fn delete_account_with_google_request(
        &self,
        id_token: String,
    ) -> Result<ServerRequest, CoreError> {
        json_request(
            "DELETE",
            api::ACCOUNT_PATH,
            &DeleteAccountRequest {
                password: Password::default(),
                google_id_token: Some(id_token.trim().to_owned()),
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
                visibility: Some(wire_visibility(visibility)),
                ..UpdateFlightRequest::default()
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
            takeoff_name: takeoff_name(&response.flight),
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
    /// The names of the sites the server says the logbook's flights took
    /// off from, by the flight's identity. Empty when signed out.
    pub(crate) fn server_takeoff_names(&self) -> BTreeMap<String, String> {
        self.stored_account()
            .ok()
            .flatten()
            .map(|stored| {
                stored
                    .flights
                    .into_iter()
                    .filter_map(|(identity, remote)| Some((identity, remote.takeoff_name?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn stored_account(&self) -> Result<Option<Stored>, CoreError> {
        self.read_account().map_err(storage_error)
    }

    /// The handle of the pilot this device is signed in as, if it is.
    pub(crate) fn my_handle(&self) -> Result<Option<String>, CoreError> {
        Ok(self.stored_account()?.map(|stored| stored.handle))
    }

    /// The server and the handle of the account this device is signed in
    /// to, and whether its session still stands. `None` when signed out.
    pub(crate) fn signed_in_to(&self) -> Result<Option<(String, String, bool)>, CoreError> {
        Ok(self
            .stored_account()?
            .map(|stored| (stored.server, stored.handle, !stored.session_id.is_empty())))
    }

    /// Notes that the server no longer knows this device's session: the
    /// pilot has to sign in again.
    pub(crate) fn session_ended(&self) {
        if let Ok(Some(mut stored)) = self.stored_account() {
            stored.session_id.clear();
            let _ = self.store_account(&stored);
        }
    }

    /// The stored account, with the storage's own error: a caller that
    /// must leave everything as it was when the file has yet to be fetched
    /// ([`io::ErrorKind::WouldBlock`]) can tell.
    fn read_account(&self) -> io::Result<Option<Stored>> {
        let Some(bytes) = self.storage.read(FILE)? else {
            return Ok(None);
        };
        // A file that cannot be read is as good as no account: the pilot
        // signs in again and nothing but the sync's memory is lost.
        Ok(serde_json::from_slice(&bytes).ok())
    }

    /// Fails if the stored account has yet to be fetched by the storage,
    /// and only then. Called before a flight is deleted: afterwards the
    /// deletion could no longer be noted for the server, and the flight
    /// would come back with the next sync.
    pub(crate) fn account_at_hand(&self) -> Result<(), CoreError> {
        match self.read_account() {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(storage_error(error)),
            _ => Ok(()),
        }
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
            may_edit_partner_sites: stored.edits_sites,
            google_email: stored.google_email.clone(),
            new_flights_public: stored.new_flights_public,
            country: stored.country.clone(),
            glider_class: stored.glider_class,
        }
    }

    /// Who would write a site from this device: for what a shell may
    /// offer ([`crate::Core::site_offers`]).
    pub(crate) fn site_actor(&self) -> Result<crate::site_offers::Actor, CoreError> {
        use crate::site_offers::Actor;
        Ok(match self.stored_account()? {
            None => Actor::SignedOut,
            Some(stored) if stored.session_id.is_empty() => Actor::SessionEnded,
            Some(stored) => Actor::Pilot {
                handle: stored.handle,
                partner: stored.edits_sites,
                suspended: stored.publishing_suspended,
            },
        })
    }

    /// The server's address of a flight of the logbook, if the server
    /// has the flight.
    pub(crate) fn flight_address(&self, id: &str) -> Result<Option<String>, CoreError> {
        Ok(self
            .stored_account()?
            .and_then(|stored| stored.flights.get(id).map(|remote| remote.id.clone())))
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

/// How long nothing is sent to an account that said it is full, seconds.
const FULL_HOLD_S: i64 = 3600;

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

/// What the sync waits for.
#[derive(Debug)]
enum Waiting {
    Deletion {
        identity: String,
    },
    Changes,
    File {
        identity: String,
    },
    Upload {
        identity: String,
    },
    /// Not an answer: the last one was taken in, and the step after it
    /// needs a stored file that the storage has yet to fetch.
    Storage,
}

/// What to say when the server answers that it is busy or that too much
/// was asked of it, with its `Retry-After` if it sent one.
pub(crate) fn asked_to_wait(retry_after_s: Option<u32>) -> String {
    match retry_after_s {
        Some(seconds) if seconds > 90 => format!(
            "The server asks to wait {} minutes. What was done so far is kept.",
            seconds.div_ceil(60)
        ),
        Some(seconds) => {
            format!("The server asks to wait {seconds} seconds. What was done so far is kept.")
        }
        None => "The server is busy. Try again in a moment.".to_owned(),
    }
}

/// Why a step did not come about.
pub(crate) enum Halt {
    /// The storage has yet to fetch a file ([`io::ErrorKind::WouldBlock`]).
    /// The same call goes through once it has.
    Fetch(CoreError),
    /// The sync is over, as the step says.
    Over(SyncStep),
}

impl From<SyncStep> for Halt {
    fn from(step: SyncStep) -> Self {
        Self::Over(step)
    }
}

#[uniffi::export]
impl FlightSync {
    /// Begins. `now_unix_s` is the current time, kept as the time of the
    /// sync.
    pub fn start(&self, now_unix_s: i64) -> SyncStep {
        settled(self.try_start(now_unix_s))
    }

    /// Takes the server's answer to the last request. `retry_after_s` is
    /// the answer's `Retry-After` header, if it had one.
    pub fn receive(&self, status: u16, body: Vec<u8>, retry_after_s: Option<u32>) -> SyncStep {
        settled(self.try_receive(status, &body, retry_after_s))
    }
}

impl FlightSync {
    /// [`start`](Self::start) for a host whose storage hands a file over
    /// only after it was asked for it ([`crate::StagedStorage`]). An error
    /// says that a stored file has to be fetched first: the host fetches
    /// what the storage wants and makes the same call again.
    pub fn start_when_fetched(&self, now_unix_s: i64) -> Result<SyncStep, CoreError> {
        fetched(self.try_start(now_unix_s))
    }

    /// [`receive`](Self::receive) for such a host. After an error the host
    /// fetches what the storage wants and makes the same call again, with
    /// the same answer; the answer is taken in once however often that is.
    pub fn receive_when_fetched(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<SyncStep, CoreError> {
        fetched(self.try_receive(status, body, retry_after_s))
    }

    fn try_start(&self, now_unix_s: i64) -> Result<SyncStep, Halt> {
        let mut progress = self.lock();
        *progress = Progress {
            now_unix_s,
            ..Progress::default()
        };
        self.next(&mut progress)
    }

    fn try_receive(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<SyncStep, Halt> {
        let mut progress = self.lock();
        if progress.waiting_for.is_none() {
            return Ok(failed("Nothing was asked of the server.", false));
        }
        // An answer is taken in once. If the call that did so then had to
        // wait for the storage, this one only goes on from there.
        if !matches!(progress.waiting_for, Some(Waiting::Storage)) {
            self.take_in(&mut progress, status, body, retry_after_s)?;
        }
        progress.waiting_for = None;
        let step = self.next(&mut progress);
        if matches!(step, Err(Halt::Fetch(_))) {
            progress.waiting_for = Some(Waiting::Storage);
        }
        step
    }

    /// Applies the server's answer to the request that is waiting for one,
    /// and saves what was learned.
    fn take_in(
        &self,
        progress: &mut Progress,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<(), Halt> {
        // Read before anything is changed: if the storage has yet to fetch
        // the account, the request goes on waiting for this same answer.
        let Some(mut stored) = self.account()? else {
            return Err(failed("This device is not signed in.", true).into());
        };
        let Some(waiting) = progress.waiting_for.take() else {
            return Ok(());
        };

        // What holds for every request.
        if status == 401 {
            stored.session_id.clear();
            let _ = self.core.store_account(&stored);
            return Err(failed("This device is no longer signed in. Sign in again.", true).into());
        }
        if status == 429 || status == 503 {
            return Err(failed(&asked_to_wait(retry_after_s), false).into());
        }

        match waiting {
            Waiting::Deletion { identity } => {
                // Gone already is as good as deleted.
                if !(200..300).contains(&status) && status != 404 {
                    return Err(failed(&refusal(status, body).to_string(), false).into());
                }
                stored.deleted_here.remove(&identity);
                progress.summary.removed_there += 1;
                // Room was made.
                stored.full_since_unix_s = None;
            }
            Waiting::Changes => {
                let changes: FlightChangesResponse = match answer(status, body) {
                    Ok(changes) => changes,
                    Err(error) => return Err(failed(&error.to_string(), false).into()),
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
                    let takeoff_name = takeoff_name(&flight);
                    stored.flights.insert(
                        flight.identity,
                        Remote {
                            id: flight.id.to_string(),
                            visibility: flight.visibility,
                            takeoff_name,
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
                    let text = String::from_utf8_lossy(body);
                    if self.core.logbook.import(&text).is_ok() {
                        progress.summary.fetched += 1;
                    }
                }
                // Deleted or made private in the meantime: the next sync's
                // changes say so.
                404 => {
                    stored.flights.remove(&identity);
                }
                _ => return Err(failed(&refusal(status, body).to_string(), false).into()),
            },
            Waiting::Upload { identity } => match status {
                200..=299 => {
                    let response: UploadResponse = match answer(status, body) {
                        Ok(response) => response,
                        Err(error) => return Err(failed(&error.to_string(), false).into()),
                    };
                    stored.flights.insert(
                        identity,
                        Remote {
                            id: response.flight.id.to_string(),
                            visibility: response.flight.visibility,
                            takeoff_name: takeoff_name(&response.flight),
                        },
                    );
                    progress.summary.sent += 1;
                    stored.full_since_unix_s = None;
                }
                // No room for it. It stays on this device and is sent once
                // there is room; what was done so far is kept.
                403 if code(body).as_deref() == Some("account_full") => {
                    stored.full_since_unix_s = Some(progress.now_unix_s);
                    if let Err(error) = self.core.store_account(&stored) {
                        return Err(failed(&error.to_string(), false).into());
                    }
                    return Err(failed(&refusal(status, body).to_string(), false).into());
                }
                // Not a flight the server can take, or too large: it stays
                // on this device and is not sent again.
                413 | 422 => {
                    stored.refused.insert(identity);
                    progress.summary.refused += 1;
                }
                _ => {
                    return Err(failed(
                        &match code(body) {
                            Some(_) => refusal(status, body).to_string(),
                            None => format!("The server did not take a flight (status {status})."),
                        },
                        false,
                    )
                    .into());
                }
            },
            // Not a request: `try_receive` never comes here with it.
            Waiting::Storage => {}
        }

        self.core
            .store_account(&stored)
            .map_err(|error| failed(&error.to_string(), false).into())
    }

    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The stored account, or why the sync cannot go on.
    fn account(&self) -> Result<Option<Stored>, Halt> {
        self.core.read_account().map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                Halt::Fetch(storage_error(error))
            } else {
                Halt::Over(failed(&storage_error(error).to_string(), false))
            }
        })
    }

    /// The next request, or the end. Nothing the server said is waited
    /// for when this is called, and nothing is lost if it has to be called
    /// again because the storage had a file to fetch.
    fn next(&self, progress: &mut Progress) -> Result<SyncStep, Halt> {
        let mut stored = match self.account()? {
            Some(stored) if !stored.session_id.is_empty() => stored,
            _ => return Ok(failed("This device is not signed in.", true)),
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
            return Ok(SyncStep::Send { request });
        }

        // 2. What changed there is learned.
        if !progress.pulled {
            progress.waiting_for = Some(Waiting::Changes);
            return Ok(SyncStep::Send {
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
            });
        }

        // 3. What this device lacks is fetched.
        if let Some((identity, address)) = progress.to_fetch.pop_front() {
            progress.waiting_for = Some(Waiting::File { identity });
            return Ok(SyncStep::Send {
                request: ServerRequest {
                    method: "GET".to_owned(),
                    path: format!("{}/{address}/igc", api::FLIGHTS_PATH),
                    body: None,
                    content_type: None,
                    needs_token: true,
                },
            });
        }

        // 4. What the server lacks is sent, one flight after another. Not
        // to an account that said within the hour that it is full: the
        // answer would be the same, after a whole file was sent for it.
        let full = stored
            .full_since_unix_s
            .is_some_and(|since| (0..FULL_HOLD_S).contains(&(progress.now_unix_s - since)));
        let waiting = || {
            self.core
                .flight_ids()
                .into_iter()
                .filter(|id| !stored.flights.contains_key(id) && !stored.refused.contains(id))
        };
        let held_back = full && waiting().next().is_some();
        let to_send = progress.to_send.get_or_insert_with(|| {
            if full {
                VecDeque::new()
            } else {
                waiting().collect()
            }
        });
        while let Some(identity) = to_send.front().cloned() {
            let text = match self.core.logbook.flight_text(&identity) {
                Ok(text) => text,
                // The storage has yet to fetch the file. The flight keeps
                // its place in the queue until it has.
                Err(LogbookError::Storage(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Err(Halt::Fetch(CoreError::from(LogbookError::Storage(error))));
                }
                // A flight whose file cannot be read stays where it is.
                Err(_) => {
                    to_send.pop_front();
                    continue;
                }
            };
            to_send.pop_front();
            progress.waiting_for = Some(Waiting::Upload { identity });
            return Ok(SyncStep::Send {
                request: ServerRequest {
                    method: "POST".to_owned(),
                    path: api::FLIGHTS_PATH.to_owned(),
                    body: Some(text.into_bytes()),
                    content_type: Some("application/octet-stream".to_owned()),
                    needs_token: true,
                },
            });
        }

        stored.last_sync_unix_s = Some(progress.now_unix_s);
        if let Err(error) = self.core.store_account(&stored) {
            return Ok(failed(&error.to_string(), false));
        }
        if held_back {
            // Everything else is done; the pilot is told why flights wait.
            let full = br#"{"error":{"code":"account_full"}}"#;
            return Ok(failed(&refusal(403, full).to_string(), false));
        }
        Ok(SyncStep::Finished {
            summary: progress.summary,
        })
    }
}

/// A step for a shell whose storage never makes it wait: the phones'.
pub(crate) fn settled(step: Result<SyncStep, Halt>) -> SyncStep {
    match step {
        Ok(step) | Err(Halt::Over(step)) => step,
        Err(Halt::Fetch(error)) => failed(&error.to_string(), false),
    }
}

/// A step for a host that fetches what the storage wants and calls again.
pub(crate) fn fetched(step: Result<SyncStep, Halt>) -> Result<SyncStep, CoreError> {
    match step {
        Ok(step) | Err(Halt::Over(step)) => Ok(step),
        Err(Halt::Fetch(error)) => Err(error),
    }
}

pub(crate) fn failed(message: &str, needs_sign_in: bool) -> SyncStep {
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

pub(crate) fn not_signed_in() -> CoreError {
    CoreError::Server {
        message: "This device is not signed in.".to_owned(),
    }
}

fn storage_error(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The account could not be read or written: {error}."),
    }
}

/// A request's body for JavaScript: bytes as a `Uint8Array`, not as a list
/// of numbers.
#[cfg(feature = "wasm")]
fn as_bytes<S: serde::Serializer>(
    bytes: &Option<Vec<u8>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match bytes {
        Some(bytes) => serializer.serialize_bytes(bytes),
        None => serializer.serialize_none(),
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

pub(crate) fn json_request<T: Serialize>(
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
    sharing_of(server, &remote.id, remote.visibility)
}

/// How the flight at `id` on `server` is shared. `None` for a setting
/// this version does not know.
pub(crate) fn sharing_of(server: &str, id: &str, visibility: Visibility) -> Option<FlightSharing> {
    let visibility = match visibility {
        Visibility::Private => FlightVisibility::Private,
        Visibility::Unlisted => FlightVisibility::Unlisted,
        Visibility::Public => FlightVisibility::Public,
        Visibility::Unknown => return None,
    };
    Some(FlightSharing {
        visibility,
        link: format!("{}/f/{id}", server.trim().trim_end_matches('/')),
        flight_id: id.to_owned(),
    })
}

/// Who may see a flight, as the API names it.
pub(crate) fn wire_visibility(visibility: FlightVisibility) -> Visibility {
    match visibility {
        FlightVisibility::Private => Visibility::Private,
        FlightVisibility::Unlisted => Visibility::Unlisted,
        FlightVisibility::Public => Visibility::Public,
    }
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
    fn a_google_sign_in_signs_in_or_goes_on_to_the_handle() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let asked = core
            .google_sign_in_request(" eyJ.a.b ".to_owned(), Some("Pixel".to_owned()))
            .unwrap();
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("POST", "/v1/auth/google")
        );
        assert!(!asked.needs_token);
        let body: serde_json::Value = serde_json::from_slice(&asked.body.unwrap()).unwrap();
        assert_eq!(body, json!({"id_token": "eyJ.a.b", "device": "Pixel"}));
        assert!(core.google_sign_in_request(" ".to_owned(), None).is_err());

        // A first sign-in: the handle step, nothing stored.
        let first = core
            .google_sign_in(SERVER.to_owned(), 200, golden("google_sign_up.json"))
            .unwrap();
        let step = first.needs_handle.unwrap();
        assert!(first.signed_in.is_none());
        assert_eq!(step.suggested_handle.as_deref(), Some("ui_test"));
        assert_eq!(
            step.suggestion_line.as_deref(),
            Some("@ui_test is free. It was made from your Google account; change it if you like.")
        );
        assert_eq!(core.account().unwrap(), None);
        let create = core
            .google_account_request(
                step.signup.clone(),
                "UI_Test".to_owned(),
                "UI Test".to_owned(),
                Some(" ".to_owned()),
                None,
            )
            .unwrap();
        assert_eq!(create.path, "/v1/auth/google/account");
        let body: serde_json::Value = serde_json::from_slice(&create.body.unwrap()).unwrap();
        assert_eq!(body["handle"], "ui_test");
        assert!(body.get("invite").is_none());
        assert!(
            core.google_account_request(step.signup, "x".to_owned(), "X".to_owned(), None, None)
                .is_err()
        );

        // A sign-in: stored as with the password.
        let signed = core
            .google_sign_in(SERVER.to_owned(), 201, golden("sign_in.json"))
            .unwrap();
        assert!(signed.needs_handle.is_none());
        assert_eq!(signed.signed_in.unwrap().account.handle, "demo");
        assert!(core.account().unwrap().is_some());
        // A refusal in words.
        let refused = core
            .google_sign_in(
                SERVER.to_owned(),
                501,
                br#"{"error":{"code":"google_not_configured","message":"no"}}"#.to_vec(),
            )
            .unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("does not offer signing in with Google"),
            "{refused}"
        );
        assert_eq!(google_words().continue_with_google, "Continue with Google");
        let unlink = core.unlink_google_request();
        assert_eq!(
            (unlink.method.as_str(), unlink.needs_token),
            ("DELETE", true)
        );
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
    fn whether_the_account_is_a_partner_is_kept_and_follows_the_server() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert_eq!(core.account_request().unwrap(), None);
        assert_eq!(
            core.account_read(200, golden("account.json")).unwrap(),
            None
        );

        // The development server's demo account is a partner, and its
        // sign-in says so.
        assert!(signed_in(&core).account.may_edit_partner_sites);
        let asked = core.account_request().unwrap().unwrap();
        assert_eq!(
            (
                asked.method.as_str(),
                asked.path.as_str(),
                asked.needs_token
            ),
            ("GET", "/v1/account", true)
        );
        assert_eq!(asked.body, None);

        // The owner takes the trust back: the next read of the account
        // says so, with whatever else changed, and it is kept.
        let plain = json!({"account": {"handle": "demo", "display_name": "Demo P.",
            "created_at": "2026-11-18T19:05:00Z"}})
        .to_string()
        .into_bytes();
        let read = core.account_read(200, plain).unwrap().unwrap();
        assert!(!read.may_edit_partner_sites);
        assert_eq!(read.display_name, "Demo P.");
        drop(core);
        let core = self::core(&folder);
        assert!(!core.account().unwrap().unwrap().may_edit_partner_sites);
        // And gives it again.
        let read = core.account_read(200, golden("account.json")).unwrap();
        assert!(read.unwrap().may_edit_partner_sites);
        assert!(matches!(
            core.site_actor().unwrap(),
            crate::site_offers::Actor::Pilot {
                partner: true,
                suspended: false,
                ..
            }
        ));

        // A record that an earlier build stored has neither fact: it
        // reads, as an account that is no partner, with all it knew.
        let path = folder.path().join("account.json");
        let mut stored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let fields = stored.as_object_mut().unwrap();
        assert_eq!(fields.remove("edits_sites"), Some(json!(true)));
        assert_eq!(fields.remove("publishing_suspended"), Some(json!(false)));
        std::fs::write(&path, stored.to_string()).unwrap();
        drop(core);
        let core = self::core(&folder);
        let earlier = core.account().unwrap().unwrap();
        assert_eq!(earlier.handle, "demo");
        assert!(!earlier.needs_sign_in && !earlier.may_edit_partner_sites);

        // Another account's answer changes nothing; a refusal neither.
        let other = json!({"account": {"handle": "mara", "display_name": "Mara",
            "created_at": "2026-11-18T19:05:00Z", "edits_sites": true}})
        .to_string()
        .into_bytes();
        assert!(
            !core
                .account_read(200, other)
                .unwrap()
                .unwrap()
                .may_edit_partner_sites
        );
        assert!(core.account_read(503, Vec::new()).is_err());
        assert!(!core.account().unwrap().unwrap().needs_sign_in);
        // A suspended account is known as one.
        let suspended = core
            .account_read(200, golden("account_suspended.json"))
            .unwrap();
        assert!(suspended.is_some());
        assert!(matches!(
            core.site_actor().unwrap(),
            crate::site_offers::Actor::Pilot {
                suspended: true,
                ..
            }
        ));
        // The session is gone on the server: sign in again, and no request.
        let ended = core.account_read(401, Vec::new()).unwrap().unwrap();
        assert!(ended.needs_sign_in);
        assert_eq!(core.account_request().unwrap(), None);
        assert_eq!(
            core.site_actor().unwrap(),
            crate::site_offers::Actor::SessionEnded
        );
    }

    /// A body of a request as JSON, to compare with a golden file.
    fn body_of(request: &ServerRequest) -> Value {
        serde_json::from_slice(request.body.as_deref().unwrap()).unwrap()
    }

    #[test]
    fn whether_new_flights_are_public_is_the_pilots_and_follows_the_server() {
        let folder = TempFolder::new();
        let core = core(&folder);
        // Signed out, there is no account to set it for.
        assert!(core.new_flights_public_request(false).is_err());

        // On for the demo account, as for every account.
        assert!(signed_in(&core).account.new_flights_public);
        let asked = core.new_flights_public_request(false).unwrap();
        assert_eq!(
            (
                asked.method.as_str(),
                asked.path.as_str(),
                asked.needs_token
            ),
            ("PATCH", "/v1/account", true)
        );
        let sent: Value = serde_json::from_slice(&golden("account_settings_request.json")).unwrap();
        assert_eq!(body_of(&asked), sent);
        // The answer is the account, read as any read of it.
        let read = core
            .account_read(200, golden("account_settings.json"))
            .unwrap()
            .unwrap();
        assert!(!read.new_flights_public);
        drop(core);
        let core = self::core(&folder);
        assert!(!core.account().unwrap().unwrap().new_flights_public);

        // A record that an earlier build stored reads as on, as the
        // server has it for every account until its pilot says otherwise.
        let path = folder.path().join("account.json");
        let mut stored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let fields = stored.as_object_mut().unwrap();
        assert_eq!(fields.remove("new_flights_public"), Some(json!(false)));
        std::fs::write(&path, stored.to_string()).unwrap();
        drop(core);
        let core = self::core(&folder);
        assert!(core.account().unwrap().unwrap().new_flights_public);
    }

    #[test]
    fn the_pilots_country_and_glider_class_are_set_and_remembered() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert!(core.country_request(Some("DE".to_owned())).is_err());
        signed_in(&core);

        // The documentation's example, sent as one request each.
        let sent: Value = serde_json::from_slice(&golden("account_details_request.json")).unwrap();
        let country = core.country_request(Some("de".to_owned())).unwrap();
        assert_eq!(country.path, "/v1/account");
        assert_eq!(body_of(&country), json!({"country": "DE"}));
        assert_eq!(sent["country"], "de");
        let class = core
            .glider_class_request(Some(crate::GliderClassKind::B))
            .unwrap();
        assert_eq!(
            body_of(&class),
            json!({"glider_class": sent["glider_class"]})
        );
        // None clears either; what is no code is refused in words.
        assert_eq!(
            body_of(&core.country_request(None).unwrap()),
            json!({"country": null})
        );
        assert_eq!(
            body_of(&core.glider_class_request(None).unwrap()),
            json!({"glider_class": null})
        );
        assert!(core.country_request(Some("Germany".to_owned())).is_err());

        // The answer is the account, remembered across a restart.
        let read = core
            .account_read(200, golden("account_details.json"))
            .unwrap()
            .unwrap();
        assert_eq!(read.country.as_deref(), Some("DE"));
        assert_eq!(read.glider_class, Some(crate::GliderClassKind::B));
        drop(core);
        let core = self::core(&folder);
        let again = core.account().unwrap().unwrap();
        assert_eq!(
            (again.country.as_deref(), again.glider_class),
            (Some("DE"), Some(crate::GliderClassKind::B))
        );
    }

    #[test]
    fn every_flight_is_made_public_or_private_at_once() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert!(core.all_flights_request(FlightVisibility::Public).is_err());
        let id = core.import_flight(sample()).unwrap().id;
        signed_in(&core);
        let sync = core.clone().sync();
        sync.start(NOW);
        sync.receive(200, no_more_changes(), None);
        sync.receive(201, golden("upload.json"), None);
        // Public from its upload, as the account's setting says.
        let shared = core.flight_sharing(id.clone()).unwrap().unwrap();
        assert_eq!(shared.visibility, FlightVisibility::Public);

        let asked = core.all_flights_request(FlightVisibility::Private).unwrap();
        assert_eq!(
            (
                asked.method.as_str(),
                asked.path.as_str(),
                asked.needs_token
            ),
            ("PATCH", "/v1/flights", true)
        );
        assert_eq!(body_of(&asked), json!({"visibility": "private"}));
        let asked = core.all_flights_request(FlightVisibility::Public).unwrap();
        let sent: Value =
            serde_json::from_slice(&golden("flights_visibility_request.json")).unwrap();
        assert_eq!(body_of(&asked), sent);
        // Shared by link is for one flight at a time.
        assert!(
            core.all_flights_request(FlightVisibility::Unlisted)
                .is_err()
        );

        // All private: every flight this device knows of is so now.
        let answer = json!({"visibility": "private", "flights": 3, "changed": 3, "taken_down": 0});
        let set = core
            .all_flights_set(200, answer.to_string().into_bytes())
            .unwrap();
        assert_eq!(set.visibility, FlightVisibility::Private);
        assert_eq!((set.flights, set.changed, set.taken_down), (3, 3, 0));
        assert_eq!(set.line, "3 flights are private");
        let shared = core.flight_sharing(id.clone()).unwrap().unwrap();
        assert_eq!(shared.visibility, FlightVisibility::Private);

        // All public, as the development server answers.
        let set = core
            .all_flights_set(200, golden("flights_visibility.json"))
            .unwrap();
        assert_eq!(set.line, "3 flights are public");
        let shared = core.flight_sharing(id.clone()).unwrap().unwrap();
        assert_eq!(shared.visibility, FlightVisibility::Public);

        // With a flight taken down nobody here knows which: the next sync
        // says, and nothing is guessed meanwhile.
        core.all_flights_set(200, answer.to_string().into_bytes())
            .unwrap();
        let some = json!({"visibility": "public", "flights": 1, "changed": 1, "taken_down": 1});
        let set = core
            .all_flights_set(200, some.to_string().into_bytes())
            .unwrap();
        assert_eq!(set.line, "1 flight is public. 1 taken down stays private");
        let shared = core.flight_sharing(id).unwrap().unwrap();
        assert_eq!(shared.visibility, FlightVisibility::Private);

        // A suspended account is refused in words, and nothing changed.
        let refused = json!({"error": {"code": "publishing_suspended", "message": "x"}});
        let error = core
            .all_flights_set(403, refused.to_string().into_bytes())
            .unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn the_words_of_who_sees_the_flights_are_short() {
        let words = flight_visibility_words();
        assert_eq!(words.new_flights_public, "New flights are public");
        assert_eq!(words.make_all_public, "Make all public");
        assert_eq!(words.make_all_private, "Make all private");
        assert_eq!(
            all_flights_question(101, FlightVisibility::Public),
            "Make all 101 flights public?"
        );
        assert_eq!(
            all_flights_question(1, FlightVisibility::Private),
            "Make your flight private?"
        );
        assert_eq!(
            all_flights_question(0, FlightVisibility::Public),
            "Make all flights public?"
        );
        assert_eq!(
            all_flights_line(FlightVisibility::Public, 101, 0),
            "101 flights are public"
        );
        assert_eq!(
            all_flights_line(FlightVisibility::Private, 0, 0),
            "No flights are private"
        );
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
        assert_eq!(sample_shared.flight_id, "7cf5ytrw45b0aeb5tcr8wet2km");
        // Public from its upload, as the account's setting says.
        let sent = core.flight_sharing(only_here).unwrap().unwrap();
        assert_eq!(sent.visibility, FlightVisibility::Public);

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

        // A full account says so in words, and for the next hour the
        // waiting flight is not sent again: the sync does the rest and
        // ends with the same words, having asked for the changes only.
        let folder_full = TempFolder::new();
        let full_core = super::tests::core(&folder_full);
        full_core.import_flight(sample()).unwrap();
        signed_in(&full_core);
        let words = "Your account on Venti is full. Delete flights you no longer need to make room; what was not sent stays on this device.";
        let no_room = json!({"error": {"code": "account_full", "message": "x"}});
        let sync = full_core.clone().sync();
        sync.start(NOW);
        let step = sync.receive(200, no_more_changes(), None);
        assert_eq!(request(&step).method, "POST");
        assert_eq!(
            sync.receive(403, no_room.to_string().into_bytes(), None),
            SyncStep::Failed {
                message: words.to_owned(),
                needs_sign_in: false
            }
        );
        assert_eq!(full_core.account().unwrap().unwrap().flights_to_send, 1);
        let sync = full_core.clone().sync();
        sync.start(NOW + 600);
        assert_eq!(
            sync.receive(200, no_more_changes(), None),
            SyncStep::Failed {
                message: words.to_owned(),
                needs_sign_in: false
            }
        );
        // An hour on it is tried again, and taken once there is room.
        let sync = full_core.clone().sync();
        sync.start(NOW + 3700);
        let step = sync.receive(200, no_more_changes(), None);
        assert_eq!(request(&step).method, "POST");
        assert!(matches!(
            sync.receive(201, golden("upload.json"), None),
            SyncStep::Finished { .. }
        ));
        assert_eq!(full_core.account().unwrap().unwrap().flights_to_send, 0);

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
        assert_eq!(now.flight_id, "5t1eshypdtvrq7rxw39a18vsrg");
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

    #[test]
    fn what_a_sync_did_is_said_in_a_line() {
        let said = |summary: SyncSummary| sync_summary_in_words(summary);
        assert_eq!(said(SyncSummary::default()), "Nothing to send or fetch");
        assert_eq!(
            said(SyncSummary {
                sent: 3,
                fetched: 1,
                ..SyncSummary::default()
            }),
            "3 sent, 1 fetched"
        );
        assert_eq!(
            said(SyncSummary {
                sent: 0,
                fetched: 0,
                removed_here: 2,
                removed_there: 1,
                refused: 1,
            }),
            "2 removed here, 1 removed on the server, 1 not taken by the server"
        );
    }

    /// A browser: a store that can only be read and written with a wait,
    /// the storage in front of it, and the page's part done by hand.
    struct Browser {
        store: BTreeMap<String, Vec<u8>>,
        staged: Arc<crate::StagedStorage>,
        core: Arc<Core>,
    }

    impl Browser {
        fn new() -> Self {
            let staged = Arc::new(crate::StagedStorage::new([]));
            let core = Core::with_storage(Arc::clone(&staged) as Arc<dyn crate::Storage>).unwrap();
            let mut browser = Self {
                store: BTreeMap::new(),
                staged,
                core,
            };
            browser.commit();
            browser
        }

        /// Writes the core's changes to the store. Their contents are out
        /// of memory afterwards, as are the files fetched before.
        fn commit(&mut self) {
            for file in self.staged.take_staged() {
                match file.contents {
                    Some(contents) => self.store.insert(file.name, contents),
                    None => self.store.remove(&file.name),
                };
            }
            self.staged.release();
        }

        /// Fetches the files the core asked for. Returns their names.
        fn fetch(&self) -> Vec<String> {
            let wanted = self.staged.take_wanted();
            for name in &wanted {
                self.staged.provide(name, self.store.get(name).cloned());
            }
            wanted
        }

        /// Makes a call the way a page does: again and again, fetching
        /// what it wanted, until it no longer fails for want of a file.
        /// Returns the result and every file that had to be fetched.
        fn call<T>(&self, call: impl Fn() -> Result<T, CoreError>) -> (T, Vec<String>) {
            let mut fetched = Vec::new();
            loop {
                match call() {
                    Ok(result) => return (result, fetched),
                    Err(error) => {
                        let wanted = self.fetch();
                        assert!(!wanted.is_empty(), "{error}");
                        fetched.extend(wanted);
                        assert!(fetched.len() < 20, "the core keeps asking: {fetched:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_sync_waits_for_storage_that_fetches_and_takes_each_answer_in_once() {
        let mut browser = Browser::new();
        let core = Arc::clone(&browser.core);
        let here = core.import_flight(on_another_day("030725")).unwrap().id;
        signed_in(&core);
        browser.commit();
        assert!(!browser.store["account.json"].is_empty());

        let sync = Arc::clone(&core).sync();
        // The account is in the store, not in memory: the first step asks
        // for it, changes nothing, and goes through once it is there.
        let error = sync.start_when_fetched(NOW).unwrap_err();
        assert!(matches!(error, CoreError::Storage { .. }), "{error:?}");
        let (step, fetched) = browser.call(|| sync.start_when_fetched(NOW));
        assert_eq!(fetched, ["account.json"]);
        assert!(request(&step).path.starts_with("/v1/flights/changes"));

        // The page stores what each step changed, and nothing stays in
        // memory. The next answer therefore needs the account again, and
        // the step after it the file of the flight to send.
        browser.commit();
        let changes = golden("flight_changes.json");
        let (step, fetched) = browser.call(|| sync.receive_when_fetched(200, &changes, None));
        assert_eq!(fetched, ["account.json"]);
        assert!(
            request(&step)
                .path
                .starts_with("/v1/flights/changes?since=")
        );

        browser.commit();
        let done = no_more_changes();
        let (step, fetched) = browser.call(|| sync.receive_when_fetched(200, &done, None));
        assert_eq!(fetched, ["account.json"]);
        // Two flights the server has are not here: the first is fetched.
        assert!(request(&step).path.ends_with("/igc"), "{step:?}");

        browser.commit();
        let file = on_another_day("150826").into_bytes();
        let (step, _) = browser.call(|| sync.receive_when_fetched(200, &file, None));
        assert!(request(&step).path.ends_with("/igc"), "{step:?}");
        browser.commit();
        let file = sample().into_bytes();
        // This answer is given three times over: once for the account, once
        // for the file of the flight to send, and then it goes through.
        let (step, fetched) = browser.call(|| sync.receive_when_fetched(200, &file, None));
        assert_eq!(fetched.len(), 2, "{fetched:?}");
        assert_eq!(fetched[0], "account.json");
        assert!(fetched[1].starts_with("flights/"), "{fetched:?}");
        // What the server lacks is sent: the flight from 2025 and, in this
        // test, the stand-in for the flight the server described.
        let uploaded = golden("upload.json");
        let mut sent = Vec::new();
        let mut step = step;
        while let SyncStep::Send { request: asked } = &step {
            assert_eq!(asked.method, "POST");
            sent.push(asked.body.clone().unwrap());
            browser.commit();
            step = browser
                .call(|| sync.receive_when_fetched(201, &uploaded, None))
                .0;
        }
        assert_eq!(sent.len(), 2);
        assert!(sent.contains(&on_another_day("030725").into_bytes()));
        // Each answer was taken in once, however often it was handed over.
        assert_eq!(
            step,
            SyncStep::Finished {
                summary: SyncSummary {
                    sent: 2,
                    fetched: 2,
                    ..SyncSummary::default()
                }
            }
        );
        browser.commit();
        assert_eq!(core.logbook().totals.flights, 3);
        assert_eq!(
            browser
                .store
                .keys()
                .filter(|name| name.starts_with("flights/"))
                .count(),
            3
        );
        let (account, _) = browser.call(|| core.account());
        let account = account.unwrap();
        assert_eq!(account.flights_to_send, 0);
        assert_eq!(account.last_sync_unix_s, Some(NOW));
        let (sharing, _) = browser.call(|| core.flight_sharing(here.clone()));
        assert_eq!(sharing.unwrap().visibility, FlightVisibility::Public);

        // With nothing more to ask, a further answer is refused in words.
        assert_eq!(
            sync.receive_when_fetched(200, &done, None).unwrap(),
            failed("Nothing was asked of the server.", false)
        );
    }

    #[test]
    fn the_phones_sync_is_told_in_words_when_storage_would_make_it_wait() {
        // The phones' calls cannot ask to be made again. Their storage
        // never makes them wait; if it did, the sync would stop and say so.
        let mut browser = Browser::new();
        let core = Arc::clone(&browser.core);
        signed_in(&core);
        browser.commit();

        let step = Arc::clone(&core).sync().start(NOW);

        assert!(
            matches!(&step, SyncStep::Failed { needs_sign_in: false, message } if message.contains("account")),
            "{step:?}"
        );
    }

    #[test]
    fn a_flight_is_not_deleted_before_storage_has_fetched_the_account() {
        let mut browser = Browser::new();
        let core = Arc::clone(&browser.core);
        let id = core.import_flight(sample()).unwrap().id;
        signed_in(&core);
        browser.commit();
        // A first sync, so that the server is known to have the flight.
        let sync = Arc::clone(&core).sync();
        browser.call(|| sync.start_when_fetched(NOW));
        browser.commit();
        let changes = golden("flight_changes.json");
        browser.call(|| sync.receive_when_fetched(200, &changes, None));
        browser.commit();

        // The account is not in memory. Deleting says so before anything
        // is gone: afterwards the server could no longer be told.
        let error = core.delete_flight(id.clone()).unwrap_err();
        assert!(matches!(error, CoreError::Storage { .. }), "{error:?}");
        assert_eq!(core.logbook().totals.flights, 1);
        assert!(browser.staged.take_staged().is_empty());

        let ((), fetched) = browser.call(|| core.delete_flight(id.clone()));
        assert_eq!(fetched, ["account.json"]);
        browser.commit();
        assert_eq!(core.logbook().totals.flights, 0);

        // The next sync deletes it on the server first.
        let sync = Arc::clone(&core).sync();
        let (step, _) = browser.call(|| sync.start_when_fetched(NOW));
        let asked = request(&step);
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            ("DELETE", "/v1/flights/7cf5ytrw45b0aeb5tcr8wet2km")
        );
    }

    #[test]
    fn without_an_account_a_flight_is_deleted_without_asking_for_one() {
        let mut browser = Browser::new();
        let core = Arc::clone(&browser.core);
        let id = core.import_flight(sample()).unwrap().id;
        browser.commit();

        core.delete_flight(id).unwrap();

        assert!(browser.staged.take_wanted().is_empty());
        assert_eq!(core.logbook().totals.flights, 0);
    }
}
