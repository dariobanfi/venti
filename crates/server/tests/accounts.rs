//! Tests of accounts and sessions through the real router: creating an
//! account, signing in and out, changing the password, deleting the
//! account, and what an attacker gets to see along the way.

use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNTS_PATH, AccountResponse, ChangePasswordRequest,
    ClientAddressesHealth, CreateAccountRequest, DeleteAccountRequest, ErrorCode, FLIGHTS_PATH,
    HEALTH_PATH, HealthResponse, Password, SESSIONS_PATH, SessionsResponse, SignInResponse,
    Visibility, flight_path, session_path, share_path,
};
use venti_server::accounts::client::TrustedProxies;
use venti_server::accounts::invite;
use venti_server::accounts::password::Cost;
use venti_server::accounts::{Limits, Registration};
use venti_server::api::{Api, Cors, MCP_PATH, Settings};
use venti_server::clock::Clock;
use venti_server::dev::{DEMO_HANDLE, DEMO_PASSWORD, DEMO_TOKEN};
use venti_sources::testing::assert_golden;
use venti_store::MemoryStore;

mod common;

use common::{
    Answer, PASSWORD, TestClock, WEB_APP, ask, get, golden, recorded, sample_file, send, server,
    set_visibility, settings, sign_up, try_sign_in, upload,
};

// --- Helpers ----------------------------------------------------------------

/// A server that takes anyone, with an empty store.
fn open_server() -> Router {
    server(settings(), &TestClock::new()).0
}

fn create(handle: &str, display_name: &str, password: &str) -> CreateAccountRequest {
    CreateAccountRequest {
        handle: handle.to_owned(),
        display_name: display_name.to_owned(),
        password: Password::new(password),
        invite: None,
        device: None,
    }
}

async fn register(router: &Router, request: &CreateAccountRequest) -> Answer {
    ask(Method::POST, ACCOUNTS_PATH)
        .json(request)
        .send(router)
        .await
}

async fn account(router: &Router, token: &str) -> Answer {
    ask(Method::GET, ACCOUNT_PATH)
        .token(token)
        .send(router)
        .await
}

/// Whether a token still acts for an account.
async fn works(router: &Router, token: &str) -> bool {
    let answer = account(router, token).await;
    match answer.status {
        StatusCode::OK => true,
        StatusCode::UNAUTHORIZED => false,
        other => panic!("unexpected {other}"),
    }
}

async fn sessions(router: &Router, token: &str) -> SessionsResponse {
    ask(Method::GET, SESSIONS_PATH)
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK)
}

// --- Creating an account and signing in -------------------------------------

#[tokio::test]
async fn a_new_account_is_signed_in_at_once() {
    let router = open_server();
    let request = CreateAccountRequest {
        device: Some("iPhone 17".to_owned()),
        ..create("  @Dario_B ", "  Dario B. ", PASSWORD)
    };

    let answer = register(&router, &request).await;

    let signed_in: SignInResponse = answer.ok(StatusCode::CREATED);
    answer.is_private();
    // The handle is kept in lower case, the name without the spaces around it.
    assert_eq!(signed_in.account.handle.as_str(), "dario_b");
    assert_eq!(signed_in.account.display_name, "Dario B.");
    assert_eq!(signed_in.session.device.as_deref(), Some("iPhone 17"));
    assert!(signed_in.session.current);
    assert!(signed_in.token.starts_with("venti_"));

    let me: AccountResponse = account(&router, &signed_in.token).await.ok(StatusCode::OK);
    assert_eq!(me.account, signed_in.account);
    // The password signs in again, however the handle is written.
    for handle in ["dario_b", "DARIO_B", " @Dario_b "] {
        let again: SignInResponse = try_sign_in(&router, handle, PASSWORD)
            .await
            .ok(StatusCode::CREATED);
        assert_eq!(again.account, signed_in.account);
        assert_ne!(
            again.token, signed_in.token,
            "every sign-in has its own token"
        );
    }
}

#[tokio::test]
async fn what_is_asked_of_a_handle_a_name_and_a_password() {
    let router = open_server();
    let refused = |request: CreateAccountRequest, code: ErrorCode| {
        let router = router.clone();
        async move {
            register(&router, &request)
                .await
                .error(StatusCode::BAD_REQUEST, code)
        }
    };

    for handle in [
        "ab", "2fast", "dario b", "dario-b", "a__b", "jürgen", "../etc", "",
    ] {
        refused(create(handle, "Dario", PASSWORD), ErrorCode::InvalidHandle).await;
    }
    let message = refused(
        create("a".repeat(21).as_str(), "D", PASSWORD),
        ErrorCode::InvalidHandle,
    )
    .await;
    assert_eq!(message, "a handle has at most 20 characters");

    for name in [
        "",
        "   ",
        "two\nlines",
        "right\u{202E}to left",
        &"x".repeat(41),
    ] {
        refused(
            create("dario", name, PASSWORD),
            ErrorCode::InvalidDisplayName,
        )
        .await;
    }

    let message = refused(
        create("dario", "Dario", "too short"),
        ErrorCode::InvalidPassword,
    )
    .await;
    assert!(message.contains("at least 15 characters"), "{message}");
    refused(
        create("dario", "Dario", &"a".repeat(129)),
        ErrorCode::InvalidPassword,
    )
    .await;
    refused(
        create("dario_the_pilot", "Dario", "Dario_The_Pilot"),
        ErrorCode::InvalidPassword,
    )
    .await;

    // Nothing was created by any of that.
    let answer = try_sign_in(&router, "dario", PASSWORD).await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    // Any characters are a password if there are enough of them.
    for (handle, password) in [
        ("anna", "               "),
        ("bernd", "gl\u{00FC}cklicher flug \u{1F60A}"),
        ("carla", "aaaaaaaaaaaaaaa"),
    ] {
        let answer = register(&router, &create(handle, "Pilot", password)).await;
        assert_eq!(answer.status, StatusCode::CREATED, "{handle}");
        let answer = try_sign_in(&router, handle, password).await;
        assert_eq!(answer.status, StatusCode::CREATED, "{handle}");
    }
}

#[tokio::test]
async fn a_handle_belongs_to_one_account_whatever_its_case() {
    let router = open_server();
    sign_up(&router, "dario").await;

    for handle in ["dario", "Dario", "DARIO"] {
        let answer = register(&router, &create(handle, "Another", "another password")).await;
        let message = answer.error(StatusCode::CONFLICT, ErrorCode::HandleTaken);
        assert_eq!(message, "this handle is not available");
    }
    // Names that would pass for the project are not for the taking, and
    // are refused with the same words.
    for handle in ["admin", "Venti", "support"] {
        let answer = register(&router, &create(handle, "Another", "another password")).await;
        let message = answer.error(StatusCode::CONFLICT, ErrorCode::HandleTaken);
        assert_eq!(message, "this handle is not available");
    }
    // The first account's password is still its password.
    let answer = try_sign_in(&router, "dario", "another password").await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
}

// --- Who may create an account ----------------------------------------------

fn server_with(registration: Registration) -> (Router, Arc<MemoryStore>) {
    let settings = Settings {
        registration,
        ..settings()
    };
    server(settings, &TestClock::new())
}

#[tokio::test]
async fn by_invite_an_account_needs_a_code_that_was_not_used() {
    let clock = TestClock::new();
    let settings = Settings {
        registration: Registration::Invite,
        ..settings()
    };
    let (router, store) = server(settings, &clock);
    let codes = invite::make(&*store, &clock, 2, Some(7)).await.unwrap();
    let with = |handle: &str, code: Option<String>| CreateAccountRequest {
        invite: code,
        ..create(handle, "Pilot", PASSWORD)
    };
    let invalid = |answer: Answer| {
        let message = answer.error(StatusCode::FORBIDDEN, ErrorCode::InvalidInvite);
        assert!(message.contains("by invite"), "{message}");
    };

    // No code, a mistyped one, one that was never made.
    invalid(register(&router, &with("anna", None)).await);
    invalid(register(&router, &with("anna", Some("not a code".to_owned()))).await);
    invalid(
        register(
            &router,
            &with("anna", Some("0000-0000-0000-0000".to_owned())),
        )
        .await,
    );

    // A good code, typed the way people type.
    let typed = codes[0].to_string().to_uppercase().replace('-', " ");
    let answer = register(&router, &with("anna", Some(typed.clone()))).await;
    assert_eq!(answer.status, StatusCode::CREATED);
    // It is used up.
    invalid(register(&router, &with("bernd", Some(typed))).await);

    // A handle that is taken does not cost the invite...
    let second = codes[1].to_string();
    let answer = register(&router, &with("anna", Some(second.clone()))).await;
    answer.error(StatusCode::CONFLICT, ErrorCode::HandleTaken);
    // ...and whether a handle is taken is said only to someone with a good
    // invite: without one, a taken handle and a free one look the same.
    let taken = register(
        &router,
        &with("anna", Some("0000-0000-0000-0000".to_owned())),
    )
    .await;
    let free = register(
        &router,
        &with("zoe", Some("0000-0000-0000-0000".to_owned())),
    )
    .await;
    assert_eq!(taken.status, free.status);
    assert_eq!(taken.body, free.body);
    invalid(taken);

    // The second code expires after seven days.
    clock.advance(Duration::days(7));
    invalid(register(&router, &with("bernd", Some(second))).await);
    assert_eq!(
        try_sign_in(&router, "bernd", PASSWORD).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_closed_server_takes_no_accounts_and_keeps_the_ones_it_has() {
    let (router, _) = server_with(Registration::Closed);
    let answer = register(&router, &create("dario", "Dario", PASSWORD)).await;
    answer.error(StatusCode::FORBIDDEN, ErrorCode::RegistrationClosed);
    // Also with what would be refused for other reasons: nothing is looked at.
    let answer = register(&router, &create("x", "", "short")).await;
    answer.error(StatusCode::FORBIDDEN, ErrorCode::RegistrationClosed);
}

#[tokio::test]
async fn the_development_mode_has_a_demo_account() {
    let router = recorded().await;

    let signed_in: SignInResponse = try_sign_in(&router, DEMO_HANDLE, DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(signed_in.account.display_name, "Demo Pilot");

    // The token that is written down works without signing in.
    let answer = account(&router, DEMO_TOKEN).await;
    let me: AccountResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(me.account, signed_in.account);
    assert_golden(&golden("account.json"), &me);

    // Its sessions: the one that was just made, and the written-down one.
    let listed = sessions(&router, DEMO_TOKEN).await;
    assert_eq!(listed.sessions.len(), 2);
    let development = listed
        .sessions
        .iter()
        .find(|session| session.current)
        .unwrap();
    assert_eq!(development.device.as_deref(), Some("Development token"));
    assert_golden(
        &golden("sessions.json"),
        &SessionsResponse {
            sessions: vec![development.clone()],
        },
    );

    // The answer to signing in, with what differs from start to start
    // replaced.
    let example = SignInResponse {
        token: "venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01".to_owned(),
        session: venti_api_types::v1::Session {
            id: "3kq9x2m7r4t8v1w6y0a5b3c2d4".to_owned(),
            ..signed_in.session.clone()
        },
        ..signed_in.clone()
    };
    assert_golden(&golden("sign_in.json"), &example);
}

// --- What an attacker learns from signing in --------------------------------

#[tokio::test]
async fn a_wrong_password_and_an_unknown_handle_get_the_same_answer() {
    let router = open_server();
    sign_up(&router, "dario").await;

    let wrong_password = try_sign_in(&router, "dario", "not the password!").await;
    let unknown_handle = try_sign_in(&router, "nobody", "not the password!").await;
    let no_handle_at_all = try_sign_in(&router, "not a handle!", "not the password!").await;
    let reserved = try_sign_in(&router, "admin", "not the password!").await;

    let message = wrong_password.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    assert_eq!(message, "the handle or the password is not correct");
    for other in [&unknown_handle, &no_handle_at_all, &reserved] {
        assert_eq!(other.status, wrong_password.status);
        assert_eq!(other.body, wrong_password.body);
        assert_eq!(other.headers, wrong_password.headers);
    }
    assert_eq!(wrong_password.header(header::WWW_AUTHENTICATE), "Bearer");
}

/// The shortest of a few sign-in attempts, which is the one the machine's
/// other work disturbed least.
async fn fastest_sign_in(router: &Router, handle: &str) -> std::time::Duration {
    let mut fastest = std::time::Duration::MAX;
    for _ in 0..4 {
        let began = Instant::now();
        let answer = try_sign_in(router, handle, "not the password!").await;
        fastest = fastest.min(began.elapsed());
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    }
    fastest
}

#[tokio::test]
async fn signing_in_takes_as_long_for_a_handle_nobody_has() {
    // With the real cost of a password hash, so that the hash is what the
    // time is spent on.
    let settings = Settings {
        password_cost: Cost::OWASP,
        ..settings()
    };
    let (router, _) = server(settings, &TestClock::new());
    sign_up(&router, "dario").await;

    let known = fastest_sign_in(&router, "dario").await;
    let unknown = fastest_sign_in(&router, "nobody").await;
    let impossible = fastest_sign_in(&router, "not a handle!").await;

    // A server that answered "no such account" without hashing would be
    // done in microseconds. The two must be the same work; a factor of two
    // either way is what a busy test machine can do to equal work.
    for other in [unknown, impossible] {
        assert!(
            other * 2 > known && known * 2 > other,
            "known {known:?}, other {other:?}"
        );
    }
    assert!(known > std::time::Duration::from_millis(5), "{known:?}");
}

/// A server with limits small enough to reach in a test.
fn limited_server(clock: &TestClock, proxies: &[&str]) -> Router {
    let settings = Settings {
        limits: Limits {
            wrong_passwords_per_handle: 3,
            wrong_passwords_per_address: 5,
            registrations_per_address: 4,
            ..Limits::default()
        },
        trusted_proxies: TrustedProxies::parse(proxies).unwrap(),
        ..settings()
    };
    server(settings, clock).0
}

async fn sign_in_from(router: &Router, peer: &str, handle: &str, password: &str) -> Answer {
    let request = venti_api_types::v1::SignInRequest {
        handle: handle.to_owned(),
        password: Password::new(password),
        device: None,
    };
    ask(Method::POST, SESSIONS_PATH)
        .from(peer)
        .json(&request)
        .send(router)
        .await
}

#[tokio::test]
async fn wrong_passwords_for_one_handle_are_counted_wherever_they_come_from() {
    let clock = TestClock::new();
    let router = limited_server(&clock, &[]);
    sign_up(&router, "dario").await;
    sign_up(&router, "anna").await;

    // Three wrong guesses, each from another address.
    for n in 1..=3 {
        let answer = sign_in_from(
            &router,
            &format!("198.51.100.{n}"),
            "dario",
            "a wrong guess!!",
        )
        .await;
        answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    }

    // Now nobody signs in to that account, not even with the password and
    // from an address that never guessed: whoever guesses must not be able
    // to tell a hit from a miss.
    let answer = sign_in_from(&router, "203.0.113.50", "dario", PASSWORD).await;
    let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("try again in 60 minutes"), "{message}");
    assert_eq!(answer.header(header::RETRY_AFTER), "3600");
    // Other accounts are not affected.
    let answer = sign_in_from(&router, "203.0.113.50", "anna", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);

    // A handle nobody has is counted exactly like one that exists, so the
    // limit gives nothing away either.
    for n in 1..=3 {
        let answer = sign_in_from(
            &router,
            &format!("198.51.100.{n}"),
            "nobody",
            "a wrong guess!!",
        )
        .await;
        answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    }
    let answer = sign_in_from(&router, "203.0.113.50", "nobody", "a wrong guess!!").await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);

    // The window closes an hour after the first wrong guess.
    clock.advance(Duration::minutes(59));
    let answer = sign_in_from(&router, "203.0.113.50", "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(answer.header(header::RETRY_AFTER), "60");
    clock.advance(Duration::minutes(1));
    let answer = sign_in_from(&router, "203.0.113.50", "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);
}

#[tokio::test]
async fn wrong_passwords_from_one_address_are_counted_whatever_handle_they_are_for() {
    let clock = TestClock::new();
    let router = limited_server(&clock, &[]);
    sign_up(&router, "dario").await;

    // One password tried on five handles: the limit per handle is never
    // reached, the limit per address is.
    for handle in ["anna", "bernd", "carla", "dora", "emil"] {
        let answer = sign_in_from(&router, "198.51.100.7", handle, "Sommer2026Sommer").await;
        answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    }
    let answer = sign_in_from(&router, "198.51.100.7", "dario", PASSWORD).await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);

    // From anywhere else the account is reachable.
    let answer = sign_in_from(&router, "198.51.100.8", "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);
    // Signing in correctly is never counted.
    for _ in 0..10 {
        let answer = sign_in_from(&router, "198.51.100.8", "dario", PASSWORD).await;
        assert_eq!(answer.status, StatusCode::CREATED);
    }
    // An IPv6 client is counted with its whole network, which it can pick
    // addresses from at will.
    for (n, handle) in ["frieda", "gustav", "hanna", "ida", "jakob"]
        .into_iter()
        .enumerate()
    {
        let peer = format!("2001:db8:1:2::{n}");
        let answer = sign_in_from(&router, &peer, handle, "Sommer2026Sommer").await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    }
    let answer = sign_in_from(&router, "2001:db8:1:2:ffff::1", "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    let answer = sign_in_from(&router, "2001:db8:1:3::1", "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);
}

/// Sends sign-in attempts at the same moment, each a task of its own, and
/// returns the statuses of the answers.
async fn sign_in_all_at_once(
    router: &Router,
    attempts: impl IntoIterator<Item = (String, String, &'static str)>,
) -> Vec<StatusCode> {
    let sent: Vec<_> = attempts
        .into_iter()
        .map(|(peer, handle, password)| {
            let router = router.clone();
            tokio::spawn(
                async move { sign_in_from(&router, &peer, &handle, password).await.status },
            )
        })
        .collect();
    let mut statuses = Vec::with_capacity(sent.len());
    for answer in sent {
        statuses.push(answer.await.unwrap());
    }
    statuses
}

fn count(statuses: &[StatusCode], status: StatusCode) -> usize {
    statuses.iter().filter(|s| **s == status).count()
}

// Several threads, as a server has: the guesses really do arrive together.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn guesses_that_arrive_together_get_no_more_checks_than_one_after_another() {
    let clock = TestClock::new();
    let settings = Settings {
        limits: Limits::default(),
        ..settings()
    };
    let router = server(settings, &clock).0;
    sign_up(&router, "dario").await;

    // A thousand guesses for one handle at the same moment, each from an
    // address of its own. Ten are looked at, which is the limit; the rest
    // are refused before any password is hashed for them.
    let guesses = (0..1000).map(|n| {
        let peer = format!("10.{}.{}.7", n / 250, n % 250);
        (peer, "dario".to_owned(), "a wrong guess!!")
    });
    let answers = sign_in_all_at_once(&router, guesses).await;
    assert_eq!(count(&answers, StatusCode::UNAUTHORIZED), 10);
    assert_eq!(count(&answers, StatusCode::TOO_MANY_REQUESTS), 990);

    // A thousand from one address, each for another handle: thirty.
    let guesses = (0..1000).map(|n| {
        let handle = format!("pilot{n}");
        ("198.51.100.7".to_owned(), handle, "Sommer2026Sommer")
    });
    let answers = sign_in_all_at_once(&router, guesses).await;
    assert_eq!(count(&answers, StatusCode::UNAUTHORIZED), 30);
    assert_eq!(count(&answers, StatusCode::TOO_MANY_REQUESTS), 970);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_right_password_is_not_counted_however_many_arrive_together() {
    let clock = TestClock::new();
    let settings = Settings {
        limits: Limits::default(),
        ..settings()
    };
    let router = server(settings, &clock).0;
    sign_up(&router, "dario").await;

    // A pilot's devices all sign in at once, several times over. Each
    // attempt holds a place while its password is checked and gives it
    // back; none is left counted.
    for _ in 0..20 {
        let devices = (0..8).map(|_| ("203.0.113.50".to_owned(), "dario".to_owned(), PASSWORD));
        let answers = sign_in_all_at_once(&router, devices).await;
        assert_eq!(count(&answers, StatusCode::CREATED), 8, "{answers:?}");
    }
    // All ten wrong passwords of the handle are still to be had.
    for n in 0..10 {
        let peer = format!("198.51.100.{n}");
        let answer = sign_in_from(&router, &peer, "dario", "a wrong guess!!").await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    }
    let answer = sign_in_from(&router, "203.0.113.50", "dario", PASSWORD).await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
}

#[tokio::test]
async fn a_forwarded_address_is_believed_from_a_named_proxy_only() {
    let clock = TestClock::new();
    let behind_proxy = limited_server(&clock, &["172.18.0.2"]);
    sign_up(&behind_proxy, "dario").await;
    let guess = |router: Router, peer: &'static str, forwarded: String, handle: &'static str| async move {
        let request = venti_api_types::v1::SignInRequest {
            handle: handle.to_owned(),
            password: Password::new("Sommer2026Sommer"),
            device: None,
        };
        ask(Method::POST, SESSIONS_PATH)
            .from(peer)
            .header(
                header::HeaderName::from_static("x-forwarded-for"),
                &forwarded,
            )
            .json(&request)
            .send(&router)
            .await
            .status
    };

    // Through the proxy, the client is who the proxy says. Five guesses by
    // one client use up that client's attempts...
    for handle in ["anna", "bernd", "carla", "dora", "emil"] {
        let status = guess(
            behind_proxy.clone(),
            "172.18.0.2",
            "198.51.100.7".to_owned(),
            handle,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let status = guess(
        behind_proxy.clone(),
        "172.18.0.2",
        "198.51.100.7".to_owned(),
        "frieda",
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    // ...and nobody else's, although every connection comes from the proxy.
    let status = guess(
        behind_proxy.clone(),
        "172.18.0.2",
        "198.51.100.8".to_owned(),
        "frieda",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // What the client itself wrote in front of the proxy's entry changes
    // nothing.
    let status = guess(
        behind_proxy.clone(),
        "172.18.0.2",
        "203.0.113.99, 198.51.100.7".to_owned(),
        "gustav",
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);

    // Whoever connects directly cannot talk its way out of the count with
    // a header of its own.
    let direct = limited_server(&clock, &["172.18.0.2"]);
    for (n, handle) in ["anna", "bernd", "carla", "dora", "emil"]
        .into_iter()
        .enumerate()
    {
        let status = guess(
            direct.clone(),
            "198.51.100.7",
            format!("10.0.0.{n}"),
            handle,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let status = guess(
        direct.clone(),
        "198.51.100.7",
        "10.0.0.99".to_owned(),
        "frieda",
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_setup_that_hides_the_visitors_addresses_shows_in_the_health_answer() {
    let clock = TestClock::new();
    let forwarded_for = header::HeaderName::from_static("x-forwarded-for");
    // Twenty-five requests that reach the server from `peer`, which says
    // each came from the address `reported` gives; then what the health
    // answer says about the addresses the server sees.
    let after_requests = |router: Router, peer: &'static str, reported: fn(usize) -> String| {
        let forwarded_for = forwarded_for.clone();
        async move {
            for n in 0..25 {
                let answer = ask(Method::GET, HEALTH_PATH)
                    .from(peer)
                    .header(forwarded_for.clone(), &reported(n))
                    .send(&router)
                    .await;
                assert_eq!(answer.status, StatusCode::OK);
            }
            let health: HealthResponse = get(&router, HEALTH_PATH).await.json();
            health.client_addresses
        }
    };
    const CADDY: &str = "172.30.250.2";

    // Before anyone has asked, there is nothing to say.
    let router = limited_server(&clock, &[CADDY]);
    let health: HealthResponse = get(&router, HEALTH_PATH).await.json();
    assert_eq!(health.client_addresses, None);

    // Docker hands the proxy the gateway's address for every visitor, and
    // the proxy passes it on in good faith.
    let hidden = after_requests(router.clone(), CADDY, |_| "172.30.250.1".to_owned()).await;
    assert_eq!(hidden, Some(ClientAddressesHealth::Hidden));
    // The next hour, with the setup repaired: every visitor has an address.
    clock.advance(Duration::hours(1));
    let seen = after_requests(router, CADDY, |n| format!("198.51.100.{n}")).await;
    assert_eq!(seen, Some(ClientAddressesHealth::Seen));

    // The proxy is not at the address the configuration names: what it
    // reports is not believed, and every request is the proxy's.
    let router = limited_server(&clock, &[CADDY]);
    let hidden = after_requests(router, "172.30.250.9", |n| format!("198.51.100.{n}")).await;
    assert_eq!(hidden, Some(ClientAddressesHealth::Hidden));

    // No proxy at all, and visitors that connect themselves.
    let router = limited_server(&clock, &[]);
    for n in 0..25 {
        let peer = format!("203.0.113.{n}");
        ask(Method::GET, HEALTH_PATH)
            .from(&peer)
            .send(&router)
            .await;
    }
    let health: HealthResponse = get(&router, HEALTH_PATH).await.json();
    assert_eq!(health.client_addresses, Some(ClientAddressesHealth::Seen));
}

#[tokio::test]
async fn attempts_to_create_accounts_are_counted_per_address() {
    let clock = TestClock::new();
    let router = limited_server(&clock, &[]);
    let attempt = |peer: &'static str, handle: &'static str| {
        let router = router.clone();
        async move {
            ask(Method::POST, ACCOUNTS_PATH)
                .from(peer)
                .json(&create(handle, "Pilot", PASSWORD))
                .send(&router)
                .await
        }
    };

    // Four attempts, of which two fail: all four count.
    assert_eq!(
        attempt("198.51.100.7", "anna").await.status,
        StatusCode::CREATED
    );
    assert_eq!(
        attempt("198.51.100.7", "anna").await.status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        attempt("198.51.100.7", "x").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        attempt("198.51.100.7", "bernd").await.status,
        StatusCode::CREATED
    );
    let answer = attempt("198.51.100.7", "carla").await;
    let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("from this address"), "{message}");
    assert_eq!(answer.header(header::RETRY_AFTER), "3600");

    assert_eq!(
        attempt("198.51.100.8", "carla").await.status,
        StatusCode::CREATED
    );
    clock.advance(Duration::hours(1));
    assert_eq!(
        attempt("198.51.100.7", "dora").await.status,
        StatusCode::CREATED
    );
}

/// One request to the connector from `peer`, and the status of the answer.
async fn ask_connector(router: &Router, peer: &str) -> StatusCode {
    ask(Method::POST, MCP_PATH)
        .from(peer)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .send(router)
        .await
        .status
}

async fn register_from(router: &Router, peer: &str, handle: &str) -> StatusCode {
    ask(Method::POST, ACCOUNTS_PATH)
        .from(peer)
        .json(&create(handle, "Pilot", PASSWORD))
        .send(router)
        .await
        .status
}

#[tokio::test]
async fn whoever_holds_many_ipv6_networks_is_still_one_client() {
    let clock = TestClock::new();
    let settings = Settings {
        limits: Limits {
            wrong_passwords_per_address: 5,
            wrong_passwords_per_network: 8,
            registrations_per_address: 4,
            registrations_per_network: 6,
            connector_requests_per_address: 5,
            connector_requests_per_network: 8,
            ..Limits::default()
        },
        ..settings()
    };
    let router = server(settings, &clock).0;
    assert_eq!(
        register_from(&router, "198.51.100.1", "dario").await,
        StatusCode::CREATED
    );
    // A rented server has a /48: 65,536 networks of the size a client is
    // counted by. Each request below comes from another of them.
    let from = |n: usize| format!("2001:db8:1:{n:x}::1");
    let elsewhere = "2001:db8:2:1::1";

    // Wrong passwords: eight from the /48 and no more, wherever in it they
    // come from, and whatever the password.
    for n in 0..8 {
        let (peer, handle) = (from(n), format!("pilot{n}"));
        let answer = sign_in_from(&router, &peer, &handle, "Sommer2026Sommer").await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    }
    let answer = sign_in_from(&router, &from(100), "dario", PASSWORD).await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    let answer = sign_in_from(&router, elsewhere, "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);

    // Asking for accounts: six.
    for n in 0..6 {
        let status = register_from(&router, &from(n), &format!("pilot{n}")).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let status = register_from(&router, &from(100), "late").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        register_from(&router, elsewhere, "late").await,
        StatusCode::CREATED
    );

    // The connector: eight.
    for n in 0..8 {
        assert_eq!(ask_connector(&router, &from(n)).await, StatusCode::OK);
    }
    assert_eq!(
        ask_connector(&router, &from(100)).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(ask_connector(&router, elsewhere).await, StatusCode::OK);
}

#[tokio::test]
async fn a_full_table_does_not_refuse_whoever_the_server_has_not_seen() {
    let clock = TestClock::new();
    // The limits of a real server, and tables small enough to fill here.
    let settings = Settings {
        limits: Limits {
            watched_keys: 40,
            ..Limits::default()
        },
        ..settings()
    };
    let router = server(settings, &clock).0;
    assert_eq!(
        register_from(&router, "198.51.100.1", "dario").await,
        StatusCode::CREATED
    );
    // Many addresses, each seen once: that is all it takes to fill a table.
    let stranger = |n: usize| format!("10.0.{}.{}", n / 250, n % 250 + 1);

    for n in 0..60 {
        assert_eq!(ask_connector(&router, &stranger(n)).await, StatusCode::OK);
    }
    // Someone new asks the connector for the first time.
    assert_eq!(ask_connector(&router, "203.0.113.50").await, StatusCode::OK);

    for n in 0..60 {
        // Not a handle: refused, and counted all the same.
        let status = register_from(&router, &stranger(n), "x").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    // Someone new asks for an account.
    assert_eq!(
        register_from(&router, "203.0.113.50", "anna").await,
        StatusCode::CREATED
    );

    for n in 0..60 {
        let (peer, handle) = (stranger(n), format!("pilot{n}"));
        let answer = sign_in_from(&router, &peer, &handle, "Sommer2026Sommer").await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    }
    // A pilot signs in from an address the server has not seen, to an
    // account nobody has guessed at.
    let answer = sign_in_from(&router, "203.0.113.51", "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);
    // What the server does know is still held to its own limit: the first
    // address has asked for one account and may ask for nine more.
    for n in 0..9 {
        let status = register_from(&router, "198.51.100.1", &format!("more{n}")).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    assert_eq!(
        register_from(&router, "198.51.100.1", "toomany").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

// --- Sessions ---------------------------------------------------------------

#[tokio::test]
async fn a_request_for_an_account_needs_a_good_token_in_the_header() {
    let router = open_server();
    let signed_in = sign_up(&router, "dario").await;
    let token = signed_in.token.as_str();
    let refused = |answer: Answer| {
        let message = answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
        assert_eq!(answer.header(header::WWW_AUTHENTICATE), "Bearer");
        message
    };
    let with = |value: String| {
        let router = router.clone();
        async move {
            ask(Method::GET, ACCOUNT_PATH)
                .header(header::AUTHORIZATION, &value)
                .send(&router)
                .await
        }
    };

    assert!(refused(get(&router, ACCOUNT_PATH).await).contains("needs an account"));
    assert_eq!(with(format!("Bearer {token}")).await.status, StatusCode::OK);
    assert_eq!(with(format!("bearer {token}")).await.status, StatusCode::OK);

    // A token of the right form that was never given out, and things that
    // are no token at all.
    let never_issued = format!("venti_{}", "7".repeat(52));
    refused(with(format!("Bearer {never_issued}")).await);
    for value in [
        token.to_owned(),
        format!("Basic {token}"),
        format!("Bearer {}", &token[..token.len() - 1]),
        format!("Bearer {}", token.to_uppercase()),
        "Bearer".to_owned(),
        "Bearer ".to_owned(),
        String::new(),
    ] {
        refused(with(value).await);
    }
    // Two tokens in one request are not a request the server guesses about.
    let twice = ask(Method::GET, ACCOUNT_PATH)
        .token(token)
        .token(token)
        .send(&router)
        .await;
    refused(twice);

    // A token is not taken from anywhere but the header: not from the
    // address, where it would end up in logs, and not from a cookie, which
    // a browser would send along on its own.
    refused(get(&router, &format!("{ACCOUNT_PATH}?token={token}")).await);
    refused(get(&router, &format!("{ACCOUNT_PATH}?access_token={token}")).await);
    let cookie = ask(Method::GET, ACCOUNT_PATH)
        .header(header::COOKIE, &format!("token={token}; session={token}"))
        .send(&router)
        .await;
    refused(cookie);
}

#[tokio::test]
async fn devices_are_listed_and_signed_out_one_by_one_or_all_at_once() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let phone = sign_up(&router, "dario").await;
    clock.advance(Duration::hours(1));
    let laptop: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let stranger = sign_up(&router, "anna").await;

    // The most recently used device first; the asking one is marked. The
    // phone signed in an hour before the laptop.
    let answer = ask(Method::GET, SESSIONS_PATH)
        .token(&laptop.token)
        .send(&router)
        .await;
    let listed: SessionsResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    let ids: Vec<&str> = listed.sessions.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, [laptop.session.id.as_str(), phone.session.id.as_str()]);
    let current: Vec<bool> = listed.sessions.iter().map(|s| s.current).collect();
    assert_eq!(current, [true, false]);
    assert_eq!(listed.sessions[1].device.as_deref(), Some("Test device"));
    assert_eq!(listed.sessions[0].device, None);
    // Asking is using: an hour later the phone asks and is first.
    clock.advance(Duration::hours(1));
    let listed = sessions(&router, &phone.token).await;
    assert_eq!(listed.sessions[0].id, phone.session.id);
    assert!(listed.sessions[0].current);
    assert!(listed.sessions[0].last_used_at > listed.sessions[1].last_used_at);
    // A token is shown once, when it is made, and never again.
    assert!(!answer.text().contains("venti_"));

    // Nobody signs out a device of another account.
    let foreign = ask(Method::DELETE, &session_path(&phone.session.id))
        .token(&stranger.token)
        .send(&router)
        .await;
    foreign.error(StatusCode::NOT_FOUND, ErrorCode::SessionNotFound);
    assert!(works(&router, &phone.token).await);

    // The phone signs the laptop out.
    let answer = ask(Method::DELETE, &session_path(&laptop.session.id))
        .token(&phone.token)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert!(answer.body.is_empty());
    assert!(!works(&router, &laptop.token).await);
    assert!(works(&router, &phone.token).await);
    assert_eq!(sessions(&router, &phone.token).await.sessions.len(), 1);
    let again = ask(Method::DELETE, &session_path(&laptop.session.id))
        .token(&phone.token)
        .send(&router)
        .await;
    again.error(StatusCode::NOT_FOUND, ErrorCode::SessionNotFound);

    // Everything at once, the asking device included.
    let tablet: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let answer = ask(Method::DELETE, SESSIONS_PATH)
        .token(&phone.token)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert!(!works(&router, &phone.token).await);
    assert!(!works(&router, &tablet.token).await);
    assert!(works(&router, &stranger.token).await);
}

#[tokio::test]
async fn a_session_that_is_not_used_expires_and_one_that_is_used_does_not() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let used = sign_up(&router, "dario").await;
    let unused: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok(StatusCode::CREATED);

    // A winter without flying: one device is opened now and then.
    for _ in 0..3 {
        clock.advance(Duration::days(150));
        assert!(works(&router, &used.token).await);
    }
    // 450 days after it signed in, the other has not been used for more
    // than the 400 days a session lasts.
    assert!(!works(&router, &unused.token).await);
    assert!(works(&router, &used.token).await);
    let listed = sessions(&router, &used.token).await;
    assert_eq!(listed.sessions.len(), 1);
    assert_eq!(listed.sessions[0].last_used_at, clock.now());
}

#[tokio::test]
async fn an_account_keeps_a_limited_number_of_devices() {
    let clock = TestClock::new();
    let settings = Settings {
        limits: Limits {
            sessions_per_account: 3,
            ..Limits::none()
        },
        ..settings()
    };
    let (router, _) = server(settings, &clock);
    let first = sign_up(&router, "dario").await;
    let mut tokens = vec![first.token];
    for _ in 0..3 {
        clock.advance(Duration::minutes(10));
        let next: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
            .await
            .ok(StatusCode::CREATED);
        tokens.push(next.token);
    }

    // The device used longest ago made room for the fourth.
    assert!(!works(&router, &tokens[0]).await);
    for token in &tokens[1..] {
        assert!(works(&router, token).await);
    }
    assert_eq!(sessions(&router, &tokens[3]).await.sessions.len(), 3);
}

// --- Changing the password, deleting the account ------------------------------

#[tokio::test]
async fn a_new_password_needs_the_old_one_and_signs_other_devices_out() {
    let router = open_server();
    let phone = sign_up(&router, "dario").await;
    let laptop: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let change = |token: &str, current: &str, new: &str| {
        let request = ChangePasswordRequest {
            current_password: Password::new(current),
            new_password: Password::new(new),
        };
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::PUT, ACCOUNT_PASSWORD_PATH)
                .token(&token)
                .json(&request)
                .send(&router)
                .await
        }
    };
    let new = "a better password!";

    // A token alone does not change the password.
    let answer = change(&phone.token, "not the password!", new).await;
    let message = answer.error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);
    assert_eq!(message, "the password is not correct");
    // The new password has to be one the server would take.
    let answer = change(&phone.token, PASSWORD, "short").await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPassword);
    let answer = ask(Method::PUT, ACCOUNT_PASSWORD_PATH)
        .json(&ChangePasswordRequest {
            current_password: Password::new(PASSWORD),
            new_password: Password::new(new),
        })
        .send(&router)
        .await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    assert!(works(&router, &laptop.token).await);

    let answer = change(&phone.token, PASSWORD, new).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    answer.is_private();

    // The device that changed it stays signed in; every other one is out.
    assert!(works(&router, &phone.token).await);
    assert!(!works(&router, &laptop.token).await);
    let old = try_sign_in(&router, "dario", PASSWORD).await;
    old.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    let answer = try_sign_in(&router, "dario", new).await;
    assert_eq!(answer.status, StatusCode::CREATED);
}

/// A server whose limits on wrong passwords are small enough to reach:
/// three for a handle, three for a signed-in device.
fn server_with_three_guesses(clock: &TestClock) -> Router {
    let settings = Settings {
        limits: Limits {
            wrong_passwords_per_handle: 3,
            wrong_passwords_per_session: 3,
            ..Limits::default()
        },
        ..settings()
    };
    server(settings, clock).0
}

async fn delete_account(router: &Router, token: &str, password: &str) -> Answer {
    ask(Method::DELETE, ACCOUNT_PATH)
        .token(token)
        .json(&DeleteAccountRequest {
            password: Password::new(password),
        })
        .send(router)
        .await
}

async fn change_password(router: &Router, token: &str, current: &str, new: &str) -> Answer {
    ask(Method::PUT, ACCOUNT_PASSWORD_PATH)
        .token(token)
        .json(&ChangePasswordRequest {
            current_password: Password::new(current),
            new_password: Password::new(new),
        })
        .send(router)
        .await
}

#[tokio::test]
async fn wrong_passwords_while_signed_in_are_counted_for_the_device() {
    let clock = TestClock::new();
    let router = server_with_three_guesses(&clock);
    let phone = sign_up(&router, "dario").await;
    let laptop: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok(StatusCode::CREATED);

    // A stolen token is no way to guess the password without limit,
    // whichever of the two requests is used for it.
    let answer = delete_account(&router, &phone.token, "a wrong guess!!").await;
    answer.error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);
    for _ in 0..2 {
        let answer =
            change_password(&router, &phone.token, "a wrong guess!!", "a new password!!").await;
        answer.error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);
    }
    let answer = delete_account(&router, &phone.token, PASSWORD).await;
    let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("from this device"), "{message}");
    let answer = change_password(&router, &phone.token, PASSWORD, "a new password!!").await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    // The account is still there, and so is the session.
    assert!(works(&router, &phone.token).await);

    // It is that device that guessed. The pilot's other device is not
    // held to account for it, and nobody is kept from signing in.
    let answer = try_sign_in(&router, "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::CREATED);
    let answer = change_password(&router, &laptop.token, PASSWORD, "a new password!!").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);

    // An hour later the first device may try again; by now its session
    // has gone with the old password.
    clock.advance(Duration::hours(1));
    let answer = delete_account(&router, &phone.token, "a new password!!").await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
}

#[tokio::test]
async fn a_stranger_cannot_keep_a_pilot_from_changing_or_deleting_their_account() {
    let clock = TestClock::new();
    let router = server_with_three_guesses(&clock);
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;

    // Strangers use up the wrong passwords of both handles, from wherever
    // they like. Signing in to them is closed for the hour, as decided.
    for handle in ["dario", "anna"] {
        for n in 0..3 {
            let peer = format!("198.51.100.{n}");
            let answer = sign_in_from(&router, &peer, handle, "a wrong guess!!").await;
            answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
        }
        let answer = try_sign_in(&router, handle, PASSWORD).await;
        answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    }

    // The pilots are signed in, and their accounts are theirs: one changes
    // the password, the other deletes the account.
    let answer = change_password(&router, &dario.token, PASSWORD, "a new password!!").await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    let answer = delete_account(&router, &anna.token, PASSWORD).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert!(works(&router, &dario.token).await);
    assert!(!works(&router, &anna.token).await);

    // And a pilot who mistypes while signed in closes nothing for anyone
    // who signs in.
    clock.advance(Duration::hours(1));
    for _ in 0..3 {
        let answer = delete_account(&router, &dario.token, "a wrong guess!!").await;
        answer.error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);
    }
    let answer = try_sign_in(&router, "dario", "a new password!!").await;
    assert_eq!(answer.status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_deleted_account_is_gone_with_everything_it_had() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let laptop: SignInResponse = try_sign_in(&router, "dario", PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    let anna = sign_up(&router, "anna").await;
    let flight = upload(&router, &dario.token, &sample_file()).await.flight;
    set_visibility(&router, &dario.token, &flight.id, Visibility::Public).await;
    let annas = upload(&router, &anna.token, &sample_file()).await.flight;
    assert_eq!(
        get(&router, &share_path(&flight.id)).await.status,
        StatusCode::OK
    );

    let delete = |token: &str, password: &str| {
        let request = DeleteAccountRequest {
            password: Password::new(password),
        };
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::DELETE, ACCOUNT_PATH)
                .token(&token)
                .json(&request)
                .send(&router)
                .await
        }
    };
    // Whoever holds an unlocked phone cannot do this without the password.
    let answer = delete(&dario.token, "not the password!").await;
    answer.error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);
    let answer = ask(Method::DELETE, ACCOUNT_PATH)
        .token(&dario.token)
        .send(&router)
        .await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    assert!(works(&router, &dario.token).await);

    let answer = delete(&dario.token, PASSWORD).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);

    // Every device is out, the password opens nothing, the flight and its
    // page are gone, and the pilot's page too.
    assert!(!works(&router, &dario.token).await);
    assert!(!works(&router, &laptop.token).await);
    let answer = try_sign_in(&router, "dario", PASSWORD).await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidCredentials);
    for path in [
        flight_path(&flight.id),
        format!("{}/igc", flight_path(&flight.id)),
        share_path(&flight.id),
        format!("{}/preview.png", share_path(&flight.id)),
        "/v1/pilots/dario".to_owned(),
    ] {
        assert_eq!(
            get(&router, &path).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    // Nobody else lost anything.
    assert!(works(&router, &anna.token).await);
    let answer = ask(Method::GET, &flight_path(&annas.id))
        .token(&anna.token)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);

    // The handle is free again, and whoever takes it starts with nothing.
    let newcomer = sign_up(&router, "dario").await;
    let answer = ask(Method::GET, FLIGHTS_PATH)
        .token(&newcomer.token)
        .send(&router)
        .await;
    let flights: venti_api_types::v1::FlightsResponse = answer.ok(StatusCode::OK);
    assert!(flights.flights.is_empty());
}

// --- Browsers ---------------------------------------------------------------

#[tokio::test]
async fn a_browser_is_never_told_to_send_credentials_along() {
    let preflight = |router: Router, origin: &'static str| async move {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri(SESSIONS_PATH)
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "authorization, content-type",
            )
            .body(Body::empty())
            .unwrap();
        send(&router, request).await
    };

    // The configured web app may sign in and send a token it holds.
    let router = open_server();
    let ours = preflight(router.clone(), WEB_APP).await;
    assert_eq!(ours.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
    let allowed = ours.header(header::ACCESS_CONTROL_ALLOW_HEADERS);
    assert!(allowed.contains("authorization") && allowed.contains("content-type"));
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        assert!(
            ours.header(header::ACCESS_CONTROL_ALLOW_METHODS)
                .contains(method)
        );
    }
    // Another site is told nothing.
    let theirs = preflight(router.clone(), "https://elsewhere.example").await;
    assert!(
        !theirs
            .headers
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );

    // The development mode lets any origin call it.
    let api = Api::new(
        Arc::new(MemoryStore::new()),
        Arc::new(TestClock::new()),
        std::iter::empty(),
        settings(),
    );
    let anyone = api.router(&Cors::Any);
    let open = preflight(anyone.clone(), "https://elsewhere.example").await;
    assert_eq!(open.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");

    // Neither ever says that cookies or other credentials of the browser
    // may come along, and the server sets no cookie.
    let signed_in = sign_up(&anyone, "dario").await;
    let cross_site = ask(Method::GET, ACCOUNT_PATH)
        .token(&signed_in.token)
        .header(header::ORIGIN, "https://elsewhere.example")
        .send(&anyone)
        .await;
    for answer in [&ours, &theirs, &open, &cross_site] {
        assert!(
            !answer
                .headers
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        );
        assert!(!answer.headers.contains_key(header::SET_COOKIE));
    }
}
