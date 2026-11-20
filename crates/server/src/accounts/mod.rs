//! Accounts: who may create one, how a password and a session token are
//! handled, and how attempts are kept slow.
//!
//! - [`password`] hashes and checks passwords.
//! - [`secret`] makes identifiers, session tokens and invite codes.
//! - [`throttle`] counts attempts; [`client`] finds the address to count
//!   them under.
//! - [`invite`] is the `invite` subcommand.
//!
//! What is not here: any way back into an account whose password is
//! forgotten. That needs email or a second credential, and neither exists
//! yet.

pub mod client;
pub mod invite;
pub mod password;
pub mod secret;
pub mod throttle;

use std::str::FromStr;

use chrono::Duration;
use venti_api_types::v1::Handle;

/// Who may create an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// Anyone.
    Open,
    /// Whoever has an invite code. Codes are made with `venti-server
    /// invite`.
    Invite,
    /// Nobody. Existing accounts keep working.
    Closed,
}

impl FromStr for Registration {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "open" => Ok(Self::Open),
            "invite" => Ok(Self::Invite),
            "closed" => Ok(Self::Closed),
            other => Err(format!("{other:?} is not one of open, invite, closed")),
        }
    }
}

/// The numbers that keep abuse cheap to bear. They are set here and not in
/// the configuration: none is something an operator tunes from day to day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wrong passwords accepted for one handle within
    /// [`sign_in_window`](Self::sign_in_window), from any address. After
    /// that nobody signs in to it until the window closes, not even with
    /// the right password: an attacker must not be able to tell a right
    /// guess from a wrong one. Devices that are signed in are not affected.
    pub wrong_passwords_per_handle: u32,
    /// Wrong passwords accepted from one address within the window,
    /// whatever handles they were for.
    pub wrong_passwords_per_address: u32,
    /// Wrong passwords accepted from one IPv6 /48 network within the
    /// window. An IPv6 client is counted under its /64 as "one address"
    /// and under its /48 as well; see
    /// [`ClientKeys`](client::ClientKeys). Four times what one address
    /// may, here and for the two limits like it below: enough for the
    /// several real people one /48 can hold, and little enough that the
    /// 65,536 networks of one /48 are of no use to whoever has them all.
    pub wrong_passwords_per_network: u32,
    /// Wrong passwords accepted from one signed-in device within the
    /// window, where a request that carries a session token has to name
    /// the password as well: changing it, deleting the account. They are
    /// counted for the session and not for the handle. Whoever holds a
    /// token they should not have gets this many guesses and no more; and
    /// what strangers do to a handle from outside, where they can use up
    /// its wrong passwords whenever they like, does not keep a signed-in
    /// pilot from their own account.
    pub wrong_passwords_per_session: u32,
    /// The window for all of these.
    pub sign_in_window: Duration,
    /// Attempts to create an account from one address within
    /// [`registration_window`](Self::registration_window), successful or
    /// not.
    pub registrations_per_address: u32,
    /// The same, from one IPv6 /48 network.
    pub registrations_per_network: u32,
    /// The window for both.
    pub registration_window: Duration,
    /// Flight files one account may upload within a day. A file the
    /// account already has counts too: the server has to read it to find
    /// that out.
    pub uploads_per_day: u32,
    /// What a file that could not be analysed in time counts as. Such a
    /// file costs the full time allowed, so few are tolerated.
    pub slow_upload_weight: u32,
    /// Reports one account may send within a day, whatever becomes of
    /// them. Each one that is stored is something a person has to read:
    /// twenty is more than anyone comes across in a day who reports in
    /// good faith (an estimate), and few enough that one account cannot
    /// bury whoever runs the server. It also bounds how fast an account
    /// can try addresses through this endpoint.
    pub reports_per_day: u32,
    /// Requests to the connector from one address within
    /// [`connector_window`](Self::connector_window). The connector is open
    /// to the internet without sign-in. An assistant's provider sends the
    /// questions of many people from few addresses, so this is far more
    /// than one person asks, and far less than would be a burden: every
    /// request is a read of the server's own database.
    pub connector_requests_per_address: u32,
    /// The same, from one IPv6 /48 network.
    pub connector_requests_per_network: u32,
    /// The window for both.
    pub connector_window: Duration,
    /// Forecasts one address may ask for within
    /// [`forecast_window`](Self::forecast_window) that make the server ask
    /// its forecast service: places nobody asked for in the last half
    /// hour. A forecast that is at hand costs nothing and is not counted.
    ///
    /// What the service may be asked in an hour is one budget for
    /// everyone (400 requests, some ninety places), and it was the
    /// only bound: one client asking for place after place used it up for
    /// all, for up to an hour. Twelve places an hour is more than a pilot
    /// looks up before a flight, since neighbouring take-offs share a
    /// place of two kilometres, and an eighth of the budget.
    pub forecast_fetches_per_address: u32,
    /// The same, from one IPv6 /48 network.
    pub forecast_fetches_per_network: u32,
    /// The window for both.
    pub forecast_window: Duration,
    /// How many keys (addresses, handles, accounts) each of the tables
    /// that count attempts watches at most. A key costs about 110 bytes
    /// (measured 2026-11-19), so each full table is 11 MB, and the server
    /// has thirteen: ten until 2026-11-20, and three since for what an
    /// account does with groups, which count by account and so cannot be
    /// filled by strangers. The size is not what the server expects to need, which is
    /// some hundreds: it is what someone who wants to fill a table has to
    /// bring. With IPv6 counted at two levels that is 100,000 IPv4
    /// addresses within a window, or some hundreds to thousands of IPv6
    /// /48 networks. What happens when a table is full all the same is
    /// said at [`Throttle`](throttle::Throttle).
    pub watched_keys: usize,
    /// Devices that can be signed in to one account. The one used longest
    /// ago makes room for a new one.
    pub sessions_per_account: usize,
    /// A session that has not been used for this long stops working.
    pub session_idle: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wrong_passwords_per_handle: 10,
            wrong_passwords_per_address: 30,
            wrong_passwords_per_network: 120,
            wrong_passwords_per_session: 10,
            sign_in_window: Duration::hours(1),
            registrations_per_address: 10,
            registrations_per_network: 40,
            registration_window: Duration::hours(1),
            uploads_per_day: 1000,
            slow_upload_weight: 50,
            reports_per_day: 20,
            connector_requests_per_address: 120,
            connector_requests_per_network: 480,
            connector_window: Duration::minutes(1),
            forecast_fetches_per_address: 12,
            forecast_fetches_per_network: 48,
            forecast_window: Duration::hours(1),
            watched_keys: 100_000,
            sessions_per_account: 50,
            // More than a year: a pilot who does not fly all winter must
            // still be signed in when spring comes, because a forgotten
            // password cannot be recovered yet.
            session_idle: Duration::days(400),
        }
    }
}

impl Limits {
    /// Limits that never bite, for the development mode: its clock stands
    /// still, so a window that opened would never close, and client tests
    /// sign in and upload as often as they like.
    pub fn none() -> Self {
        Self {
            wrong_passwords_per_handle: u32::MAX,
            wrong_passwords_per_address: u32::MAX,
            wrong_passwords_per_network: u32::MAX,
            wrong_passwords_per_session: u32::MAX,
            registrations_per_address: u32::MAX,
            registrations_per_network: u32::MAX,
            uploads_per_day: u32::MAX,
            reports_per_day: u32::MAX,
            connector_requests_per_address: u32::MAX,
            connector_requests_per_network: u32::MAX,
            forecast_fetches_per_address: u32::MAX,
            forecast_fetches_per_network: u32::MAX,
            ..Self::default()
        }
    }
}

/// Handles nobody may register: names that would pass for the project or
/// its staff, and words that mean something in a path.
const RESERVED_HANDLES: &[&str] = &[
    "abuse",
    "account",
    "accounts",
    "admin",
    "administrator",
    "anonymous",
    "api",
    "app",
    "assets",
    "contact",
    "deleted",
    "everyone",
    "flight",
    "flights",
    "help",
    "hostmaster",
    "info",
    "legal",
    "login",
    "logout",
    "mod",
    "moderator",
    "nobody",
    "null",
    "official",
    "owner",
    "pilot",
    "pilots",
    "postmaster",
    "privacy",
    "register",
    "root",
    "security",
    "session",
    "sessions",
    "settings",
    "signin",
    "signup",
    "staff",
    "static",
    "support",
    "system",
    "team",
    "terms",
    "tiles",
    "undefined",
    "unknown",
    "venti",
    "venti_app",
    "venti_team",
    "webmaster",
    "www",
];

/// Whether a handle is kept back from registration.
pub fn is_reserved(handle: &Handle) -> bool {
    RESERVED_HANDLES.binary_search(&handle.as_str()).is_ok()
}

/// Makes text a client sent to label a device fit to store and show: no
/// control characters, at most sixty characters, and `None` if nothing is
/// left.
pub fn device_label(text: Option<&str>) -> Option<String> {
    let label: String = text?
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(60)
        .collect();
    (!label.is_empty()).then_some(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_modes_are_read_from_their_names() {
        assert_eq!("open".parse(), Ok(Registration::Open));
        assert_eq!("invite".parse(), Ok(Registration::Invite));
        assert_eq!("closed".parse(), Ok(Registration::Closed));
        assert!("Open".parse::<Registration>().is_err());
        assert!("".parse::<Registration>().is_err());
    }

    #[test]
    fn reserved_handles_are_found_and_every_one_could_be_a_handle() {
        // The search needs the list sorted.
        assert!(RESERVED_HANDLES.is_sorted(), "keep the list sorted");
        for reserved in RESERVED_HANDLES {
            let handle = Handle::new(reserved).unwrap_or_else(|e| panic!("{reserved}: {e}"));
            assert!(is_reserved(&handle));
        }
        assert!(is_reserved(&Handle::new("Admin").unwrap()));
        assert!(!is_reserved(&Handle::new("dario").unwrap()));
        assert!(!is_reserved(&Handle::new("demo").unwrap()));
    }

    #[test]
    fn a_device_label_is_cleaned_before_it_is_kept() {
        assert_eq!(device_label(None), None);
        assert_eq!(device_label(Some("   ")), None);
        assert_eq!(
            device_label(Some("  iPhone\u{0}\n 17 ")),
            Some("iPhone 17".to_owned())
        );
        let long = "x".repeat(200);
        assert_eq!(device_label(Some(&long)).unwrap().len(), 60);
    }
}
