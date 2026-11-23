//! Accounts: who may create one, how a password and a session token are
//! handled, and how attempts are kept slow.
//!
//! - [`password`] hashes and checks passwords.
//! - [`secret`] makes identifiers, session tokens and invite codes.
//! - [`throttle`] counts attempts; [`client`] finds the address to count
//!   them under.
//! - [`invite`] is the `invite` subcommand.
//! - [`google`] checks Google's ID tokens, for signing in with Google.
//!
//! What is not here: any way back into an account whose password is
//! forgotten. That needs email or a second credential, and neither exists
//! yet.

pub mod client;
pub mod google;
pub mod invite;
pub mod oauth;
pub mod password;
pub mod picture;
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
    /// Comments one account may write on flights within a day. A hundred
    /// is more than a pilot writes who reads what they answer (an
    /// estimate), and few enough that one account cannot fill the pages
    /// of others.
    pub comments_per_day: u32,
    /// Times one account may follow a pilot or cheer a flight within a
    /// day, each of which may tell another pilot something. Taking one
    /// back is not counted.
    pub follows_and_cheers_per_day: u32,
    /// Archives one account may ask for within a day
    /// (`GET /v1/account/archive`), whatever becomes of the request. An
    /// archive reads every flight file the account has and packs them
    /// anew: a pilot wants one now and then, and a program that asked in a
    /// loop would keep the server at nothing else. Four lets a pilot ask
    /// again after a download that broke off (an estimate).
    pub archives_per_day: u32,
    /// Times one account may make all its flights public or private at
    /// once within a day (`PATCH /v1/flights`), whatever becomes of the
    /// request. Each time can change every flight the account has, and
    /// each change is told to the pilot's devices and to every copy of
    /// the public flights: a pilot does it now and then, and a program
    /// that did it in a loop would fill both. Twenty is more than a pilot
    /// who changes their mind needs (an estimate).
    pub all_flights_changes_per_day: u32,
    /// Pictures one account may send within a day
    /// (`PUT /v1/account/picture`), whatever becomes of them. Each is
    /// read and written anew by the server, which is the expensive part
    /// ([`venti_api_types::v1::PICTURES_PER_DAY`]).
    pub pictures_per_day: u32,
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
    /// Reads of public flights as data that one address may make within
    /// [`public_flight_window`](Self::public_flight_window): a page of
    /// everyone's public flights, a page of what changed among them, and
    /// a flight's track. They are open to anyone without a token, and a
    /// track is worked out from its file for every answer that is not
    /// kept.
    ///
    /// Six hundred an hour is one every six seconds: a program that
    /// follows the changes once a minute uses a tenth of it, a copy of
    /// 60,000 flights' records is read within an hour at a hundred a
    /// page, and a pilot who browses never comes near. An estimate, and
    /// the owner's to set.
    pub public_flight_reads_per_address: u32,
    /// The same, from one IPv6 /48 network.
    pub public_flight_reads_per_network: u32,
    /// The window for both.
    pub public_flight_window: Duration,
    /// Requests one token for a program may make within
    /// [`token_window`](Self::token_window), of any kind, whatever their
    /// answers: its own budget, which no other token of the account and
    /// no session shares (`docs/api.md`, "Limits"). Every answer to a
    /// request with such a token says what is left of it.
    ///
    /// Three thousand six hundred an hour is one a second, all hour long:
    /// a logger that sends its position every five seconds uses a fifth
    /// of it, and a program that keeps a copy of a pilot's logbook in
    /// step once a minute a sixtieth. It is of the order of what Venti's
    /// own apps ask with a session in an hour of use, and six times what
    /// an address without a token may read of public flights. An
    /// estimate, and the owner's to set.
    pub token_requests_per_window: u32,
    /// The window for it.
    pub token_window: Duration,
    /// Programs that one address may register with the authorization
    /// server in a registration window, and a whole IPv6 /48. A program
    /// registers once and is then used by every pilot who agrees to it, so
    /// an honest address needs one or two. Estimates, and the owner's to
    /// set.
    pub client_registrations_per_address: u32,
    /// The same for a whole IPv6 /48.
    pub client_registrations_per_network: u32,
    /// How many registered programs the server keeps. Each is a name and
    /// a few addresses, a kilobyte or two at the most. When the table is
    /// full, the program that registered longest ago among those no pilot
    /// ever agreed to makes room.
    pub oauth_clients: usize,
    /// Requests one address may make to the authorization server's other
    /// endpoints (the page a pilot agrees on, the exchange of codes and
    /// refresh tokens, giving a token up) in a connector window, and a
    /// whole IPv6 /48. None of them needs an account to ask, and each
    /// costs a look into the database. A program refreshes once an hour.
    pub oauth_requests_per_address: u32,
    /// The same for a whole IPv6 /48.
    pub oauth_requests_per_network: u32,
    /// How many keys (addresses, handles, accounts) each of the tables
    /// that count attempts watches at most. A key costs about 110 bytes
    /// (measured 2026-11-19), so each full table is 11 MB, and the server
    /// has nineteen (2026-11-21), most of which count by account or by a
    /// token that was checked and so cannot be filled by strangers. The size is not what the server expects to need, which is
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
    /// Tokens for other programs that one account can hold. None makes
    /// room for a new one: the pilot withdraws one first.
    pub tokens_per_account: usize,
    /// How long the dump of a layer of Venti's own sites is handed out
    /// before the server looks whether the layer has changed, and makes
    /// the dump anew if it has (`GET /v1/dumps`). Making one reads the
    /// whole layer with its history: this is what bounds how often that
    /// happens, whoever asks and however often. It is also the longest a
    /// site that whoever runs the server took down stays in a file that
    /// is handed out. Five minutes is an estimate, and the owner's to
    /// set.
    pub dump_remake_after: Duration,
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
            comments_per_day: venti_api_types::v1::COMMENTS_PER_DAY,
            follows_and_cheers_per_day: venti_api_types::v1::FOLLOWS_AND_CHEERS_PER_DAY,
            archives_per_day: 4,
            all_flights_changes_per_day: 20,
            pictures_per_day: venti_api_types::v1::PICTURES_PER_DAY,
            connector_requests_per_address: 120,
            connector_requests_per_network: 480,
            connector_window: Duration::minutes(1),
            forecast_fetches_per_address: 12,
            forecast_fetches_per_network: 48,
            forecast_window: Duration::hours(1),
            public_flight_reads_per_address: 600,
            public_flight_reads_per_network: 2400,
            public_flight_window: Duration::hours(1),
            token_requests_per_window: 3600,
            token_window: Duration::hours(1),
            client_registrations_per_address: 10,
            client_registrations_per_network: 40,
            oauth_clients: 50_000,
            oauth_requests_per_address: 60,
            oauth_requests_per_network: 240,
            watched_keys: 100_000,
            sessions_per_account: 50,
            // More than a year: a pilot who does not fly all winter must
            // still be signed in when spring comes, because a forgotten
            // password cannot be recovered yet.
            session_idle: Duration::days(400),
            tokens_per_account: 20,
            dump_remake_after: Duration::minutes(5),
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
            comments_per_day: u32::MAX,
            follows_and_cheers_per_day: u32::MAX,
            archives_per_day: u32::MAX,
            all_flights_changes_per_day: u32::MAX,
            pictures_per_day: u32::MAX,
            connector_requests_per_address: u32::MAX,
            connector_requests_per_network: u32::MAX,
            forecast_fetches_per_address: u32::MAX,
            forecast_fetches_per_network: u32::MAX,
            public_flight_reads_per_address: u32::MAX,
            public_flight_reads_per_network: u32::MAX,
            token_requests_per_window: u32::MAX,
            client_registrations_per_address: u32::MAX,
            client_registrations_per_network: u32::MAX,
            oauth_requests_per_address: u32::MAX,
            oauth_requests_per_network: u32::MAX,
            // A dump is as the layer is at every request: the clock
            // never gets as far as a remake would be due.
            dump_remake_after: Duration::zero(),
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
    "near",
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
