//! Tests of the tokens a pilot gives to another program, through the real
//! router: making, listing and withdrawing one, what each scope opens and
//! what no token may do, and the name a write through a token leaves.

use axum::Router;
use axum::http::{Method, StatusCode};
use chrono::Duration;
use venti_api_types::v1::{
    ACCESS_TOKEN_PREFIX, ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNT_TOKENS_PATH, AccessToken,
    AccessTokensResponse, AccountResponse, AddSiteRequest, CreateAccessTokenRequest,
    CreateAccessTokenResponse, ErrorCode, ErrorResponse, FLIGHT_CHANGES_PATH, FLIGHTS_PATH,
    FlightsResponse, GROUP_JOIN_PATH, GROUPS_PATH, KEPT_SITE_CHANGES_PATH, KEPT_SITES_PATH,
    LIVE_POSITIONS_PATH, PLAN_BOARD_PATH, Password, REPORTS_PATH, SESSIONS_PATH, SITES_PATH, Scope,
    SiteDraft, SiteHistoryResponse, SiteResponse, SiteVia, UploadResponse, account_token_path,
    flight_file_path, flight_path, site_history_path,
};
use venti_domain::{GeoPoint, SiteUse};
use venti_server::clock::Clock;
use venti_server::dev::{
    DEMO_ACCESS_TOKEN, DEMO_ACCESS_TOKEN_ID, DEMO_ACCESS_TOKEN_NAME, DEMO_PASSWORD, DEMO_TOKEN,
};
use venti_sources::testing::assert_golden;
use venti_store::Store;

mod common;

use common::{
    Answer, PASSWORD, TestClock, ask, golden, recorded, recorded_with_its_store, sample_file,
    server, settings, sign_up, sign_up_private, upload,
};

// --- Helpers ----------------------------------------------------------------

fn wanted(name: &str, scopes: &[Scope], password: &str) -> CreateAccessTokenRequest {
    CreateAccessTokenRequest {
        name: name.to_owned(),
        scopes: scopes
            .iter()
            .map(|scope| scope.as_str().to_owned())
            .collect(),
        expires_at: None,
        password: Password::new(password),
        google_id_token: None,
    }
}

async fn try_make(router: &Router, session: &str, request: &CreateAccessTokenRequest) -> Answer {
    ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(session)
        .json(request)
        .send(router)
        .await
}

/// Makes a token with these scopes and returns it with its secret.
async fn make(router: &Router, session: &str, scopes: &[Scope]) -> CreateAccessTokenResponse {
    let answer = try_make(router, session, &wanted("Logger", scopes, PASSWORD)).await;
    answer.is_private();
    answer.ok(StatusCode::CREATED)
}

async fn listed(router: &Router, session: &str) -> Vec<AccessToken> {
    let answer = ask(Method::GET, ACCOUNT_TOKENS_PATH)
        .token(session)
        .send(router)
        .await;
    answer.is_private();
    answer.ok::<AccessTokensResponse>(StatusCode::OK).tokens
}

/// Checks that an answer refuses a token for the scope it lacks, and
/// names that scope.
fn lacks(answer: &Answer, scope: Scope) {
    answer.error(StatusCode::FORBIDDEN, ErrorCode::InsufficientScope);
    let error: ErrorResponse = answer.json();
    assert_eq!(error.error.scope, Some(scope));
    assert!(error.error.message.contains(scope.as_str()));
}

const ID: &str = "0123456789abcdefghjkmnpqrs";

/// Every request that acts for an account within a scope: the method, the
/// path, and the scope it needs. `docs/api.md` has the same table.
fn scoped() -> Vec<(Method, String, Scope)> {
    let g = format!("{GROUPS_PATH}/{ID}");
    // A site of the crowdsourced layer, which any pilot writes to: a
    // request for a partner's site is refused for who asks, whatever the
    // token.
    let site = format!("{SITES_PATH}/venti-pilots:{ID}");
    let flight = format!("{FLIGHTS_PATH}/{ID}");
    let kept = format!("{KEPT_SITES_PATH}/venti:{ID}");
    use Scope::{
        AccountRead, AlertsRead, AlertsWrite, FlightsRead, FlightsWrite, GroupsRead, GroupsWrite,
        KeptSitesRead, KeptSitesWrite, LiveWrite, ReportsWrite, SitesWrite, SocialRead,
        SocialWrite,
    };
    let alert = venti_api_types::v1::alert_path(ID);
    let follow = venti_api_types::v1::pilot_follow_path("nobody");
    let cheer = format!("{flight}/cheer");
    let comments = format!("{flight}/comments");
    let comment = format!("{flight}/comments/{ID}");
    vec![
        (Method::GET, ACCOUNT_PATH.to_owned(), AccountRead),
        (Method::PATCH, ACCOUNT_PATH.to_owned(), FlightsWrite),
        (Method::GET, FLIGHTS_PATH.to_owned(), FlightsRead),
        (Method::GET, FLIGHT_CHANGES_PATH.to_owned(), FlightsRead),
        (Method::POST, FLIGHTS_PATH.to_owned(), FlightsWrite),
        (Method::PATCH, FLIGHTS_PATH.to_owned(), FlightsWrite),
        (Method::PATCH, flight.clone(), FlightsWrite),
        (Method::DELETE, flight, FlightsWrite),
        (Method::POST, LIVE_POSITIONS_PATH.to_owned(), LiveWrite),
        (Method::DELETE, format!("/v1/live/{ID}"), LiveWrite),
        (Method::POST, SITES_PATH.to_owned(), SitesWrite),
        (Method::PUT, site.clone(), SitesWrite),
        (Method::DELETE, site.clone(), SitesWrite),
        (Method::POST, format!("{site}/revisions"), SitesWrite),
        (Method::PUT, format!("{site}/confirmation"), SitesWrite),
        (Method::DELETE, format!("{site}/confirmation"), SitesWrite),
        (Method::GET, GROUPS_PATH.to_owned(), GroupsRead),
        (Method::GET, g.clone(), GroupsRead),
        (Method::GET, format!("{g}/plans"), GroupsRead),
        (Method::GET, PLAN_BOARD_PATH.to_owned(), GroupsRead),
        (Method::POST, GROUPS_PATH.to_owned(), GroupsWrite),
        (Method::POST, GROUP_JOIN_PATH.to_owned(), GroupsWrite),
        (Method::PATCH, g.clone(), GroupsWrite),
        (Method::DELETE, g.clone(), GroupsWrite),
        (Method::POST, format!("{g}/invite"), GroupsWrite),
        (Method::PUT, format!("{g}/keeper"), GroupsWrite),
        (Method::DELETE, format!("{g}/members/demo"), GroupsWrite),
        (Method::POST, format!("{g}/plans"), GroupsWrite),
        (Method::PUT, format!("{g}/plans/{ID}"), GroupsWrite),
        (Method::DELETE, format!("{g}/plans/{ID}"), GroupsWrite),
        (Method::PUT, format!("{g}/plans/{ID}/in"), GroupsWrite),
        (Method::DELETE, format!("{g}/plans/{ID}/in"), GroupsWrite),
        (Method::POST, REPORTS_PATH.to_owned(), ReportsWrite),
        (
            Method::GET,
            KEPT_SITE_CHANGES_PATH.to_owned(),
            KeptSitesRead,
        ),
        (Method::PUT, kept.clone(), KeptSitesWrite),
        (Method::DELETE, kept, KeptSitesWrite),
        (
            Method::GET,
            venti_api_types::v1::ALERTS_PATH.to_owned(),
            AlertsRead,
        ),
        (
            Method::GET,
            venti_api_types::v1::ALERTS_FIRED_PATH.to_owned(),
            AlertsRead,
        ),
        (
            Method::POST,
            venti_api_types::v1::ALERTS_PATH.to_owned(),
            AlertsWrite,
        ),
        (Method::PUT, alert.clone(), AlertsWrite),
        (Method::DELETE, alert, AlertsWrite),
        (Method::PUT, follow.clone(), SocialWrite),
        (Method::DELETE, follow, SocialWrite),
        (
            Method::GET,
            venti_api_types::v1::ACCOUNT_FOLLOWING_PATH.to_owned(),
            SocialRead,
        ),
        (
            Method::GET,
            venti_api_types::v1::ACCOUNT_FEED_PATH.to_owned(),
            SocialRead,
        ),
        (
            Method::GET,
            venti_api_types::v1::ACCOUNT_NOTIFICATIONS_PATH.to_owned(),
            SocialRead,
        ),
        (Method::PUT, cheer.clone(), SocialWrite),
        (Method::DELETE, cheer, SocialWrite),
        (Method::POST, comments.clone(), SocialWrite),
        (Method::PATCH, comments, FlightsWrite),
        (Method::DELETE, comment, SocialWrite),
    ]
}

/// What only the pilot's own sign-in may do.
fn for_the_session_only() -> Vec<(Method, String)> {
    vec![
        (Method::GET, SESSIONS_PATH.to_owned()),
        (Method::DELETE, SESSIONS_PATH.to_owned()),
        (Method::DELETE, format!("{SESSIONS_PATH}/{ID}")),
        (Method::PUT, ACCOUNT_PASSWORD_PATH.to_owned()),
        (Method::DELETE, ACCOUNT_PATH.to_owned()),
        (Method::POST, ACCOUNT_TOKENS_PATH.to_owned()),
        (Method::GET, ACCOUNT_TOKENS_PATH.to_owned()),
        (Method::DELETE, account_token_path(ID)),
    ]
}

// --- Behaviours -------------------------------------------------------------

#[tokio::test]
async fn a_pilot_makes_a_token_sees_it_listed_and_withdraws_it() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    assert!(listed(&router, &dario.token).await.is_empty());

    let made = make(
        &router,
        &dario.token,
        &[Scope::FlightsWrite, Scope::FlightsRead],
    )
    .await;
    // The secret says what it is, and is no session's token.
    assert!(made.secret.starts_with(ACCESS_TOKEN_PREFIX));
    assert_eq!(made.secret.len(), 62);
    assert_eq!(made.token.name, "Logger");
    // The scopes come back in the documents' order, whatever order they
    // were asked in.
    assert_eq!(made.token.scopes, [Scope::FlightsRead, Scope::FlightsWrite]);
    assert_eq!(made.token.last_used_on, None);
    assert_eq!(made.token.expires_at, None);
    // Listed for its pilot and for nobody else, without the secret.
    assert_eq!(
        listed(&router, &dario.token).await,
        std::slice::from_ref(&made.token)
    );
    assert!(listed(&router, &anna.token).await.is_empty());
    let list = ask(Method::GET, ACCOUNT_TOKENS_PATH)
        .token(&dario.token)
        .send(&router)
        .await;
    assert!(!list.text().contains(&made.secret));
    assert!(!list.text().contains("secret"));

    // It acts for the account within its scopes, and the day of its use
    // is kept.
    let flights = ask(Method::GET, FLIGHTS_PATH)
        .token(&made.secret)
        .send(&router)
        .await;
    flights.is_private();
    flights.ok::<FlightsResponse>(StatusCode::OK);
    let today = clock.now().date_naive();
    assert_eq!(
        listed(&router, &dario.token).await[0].last_used_on,
        Some(today)
    );

    // Another account cannot withdraw it, and text that is no identifier
    // names none.
    for (session, id) in [
        (&anna.token, made.token.id.as_str()),
        (&dario.token, ID),
        (&dario.token, "nothing"),
        (&dario.token, "a%00b"),
    ] {
        ask(Method::DELETE, &account_token_path(id))
            .token(session)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::TokenNotFound);
    }
    // Withdrawn: gone from the list, and its very next request is
    // answered as one with a token nobody was given.
    let withdrawn = ask(Method::DELETE, &account_token_path(&made.token.id))
        .token(&dario.token)
        .send(&router)
        .await;
    assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
    withdrawn.is_private();
    assert!(listed(&router, &dario.token).await.is_empty());
    let after = ask(Method::GET, FLIGHTS_PATH)
        .token(&made.secret)
        .send(&router)
        .await;
    after.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    ask(Method::DELETE, &account_token_path(&made.token.id))
        .token(&dario.token)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::TokenNotFound);
}

#[tokio::test]
async fn a_token_opens_what_its_scopes_say_and_nothing_else() {
    let (router, _) = server(settings(), &TestClock::new());
    let dario = sign_up(&router, "dario").await;

    for scope in Scope::ALL {
        let only_made = make(&router, &dario.token, &[scope]).await;
        let only = only_made.secret.clone();
        let others: Vec<Scope> = Scope::ALL.into_iter().filter(|s| *s != scope).collect();
        let all_but_made = make(&router, &dario.token, &others).await;
        let all_but = all_but_made.secret.clone();
        let mut asked = 0;
        for (method, path, needs) in scoped() {
            let with = ask(method.clone(), &path).token(&only).send(&router).await;
            let without = ask(method.clone(), &path)
                .token(&all_but)
                .send(&router)
                .await;
            if needs == scope {
                asked += 1;
                // With the scope the request gets as far as a session's:
                // whatever it is told, it is not told about scopes.
                let as_session = ask(method.clone(), &path)
                    .token(&dario.token)
                    .send(&router)
                    .await;
                assert_eq!(with.status, as_session.status, "{method} {path}");
                assert_ne!(with.status, StatusCode::FORBIDDEN, "{method} {path}");
                assert_ne!(with.status, StatusCode::UNAUTHORIZED, "{method} {path}");
                lacks(&without, scope);
            } else {
                lacks(&with, needs);
            }
        }
        assert!(asked > 0, "no request needs {scope}");
        // Withdrawn again: an account holds only so many tokens.
        for made in [only_made, all_but_made] {
            let withdrawn = ask(Method::DELETE, &account_token_path(&made.token.id))
                .token(&dario.token)
                .send(&router)
                .await;
            assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
        }
    }
}

#[tokio::test]
async fn no_token_does_what_is_for_the_pilots_own_sign_in() {
    let (router, _) = server(settings(), &TestClock::new());
    let dario = sign_up(&router, "dario").await;
    let every = make(&router, &dario.token, &Scope::ALL).await;

    for (method, path) in for_the_session_only() {
        let answer = ask(method.clone(), &path)
            .token(&every.secret)
            .send(&router)
            .await;
        let message = answer.error(StatusCode::FORBIDDEN, ErrorCode::SessionRequired);
        assert!(
            message.contains("own sign-in"),
            "{method} {path}: {message}"
        );
        // And nobody without any token gets further than "sign in".
        ask(method, &path)
            .send(&router)
            .await
            .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    }
    // Nothing happened: the session, the token and the account are there.
    let still = listed(&router, &dario.token).await;
    assert_eq!(still.len(), 1);
    assert_eq!(still[0].id, every.token.id);
    // Not with the password in the body either: a token cannot make a
    // token, and so cannot widen itself.
    let more = try_make(
        &router,
        &every.secret,
        &wanted("More", &Scope::ALL, PASSWORD),
    )
    .await;
    more.error(StatusCode::FORBIDDEN, ErrorCode::SessionRequired);

    // With every scope it reads the account.
    let me: AccountResponse = ask(Method::GET, ACCOUNT_PATH)
        .token(&every.secret)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(me.account, dario.account);
}

#[tokio::test]
async fn making_a_token_takes_the_password_a_name_scopes_and_an_end_to_come() {
    let clock = TestClock::new();
    let mut limited = settings();
    limited.limits.tokens_per_account = 2;
    let (router, _) = server(limited, &clock);
    let dario = sign_up(&router, "dario").await;
    let session = dario.token.as_str();
    let scopes = [Scope::FlightsRead];

    // A session alone is not enough.
    try_make(
        &router,
        session,
        &wanted("Logger", &scopes, "not the password!"),
    )
    .await
    .error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);

    for name in ["", "   ", &"x".repeat(61), "two\nlines"] {
        try_make(&router, session, &wanted(name, &scopes, PASSWORD))
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidTokenName);
    }
    let named = |names: &[&str]| CreateAccessTokenRequest {
        scopes: names.iter().map(|&name| name.to_owned()).collect(),
        ..wanted("Logger", &[], PASSWORD)
    };
    for names in [
        &[][..],
        &["flights"],
        &["flights:read", "flights:read"],
        &["flights:read", "everything"],
        &["other"],
        &["Flights:Read"],
        &["a"; 9],
    ] {
        let message = try_make(&router, session, &named(names))
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidScopes);
        // A name that is no scope's is not repeated: it is a stranger's text.
        assert!(!message.contains("everything"), "{message}");
    }
    for end in [clock.now(), clock.now() - Duration::days(1)] {
        let request = CreateAccessTokenRequest {
            expires_at: Some(end),
            ..wanted("Logger", &scopes, PASSWORD)
        };
        try_make(&router, session, &request)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidExpiry);
    }
    assert!(listed(&router, session).await.is_empty());

    // A token with an end is good until then, and answered as an unknown
    // one from then on. It stays in the list until it is withdrawn.
    let end = clock.now() + Duration::days(30);
    let request = CreateAccessTokenRequest {
        expires_at: Some(end),
        ..wanted("  Sommer-Logger ", &scopes, PASSWORD)
    };
    let ending: CreateAccessTokenResponse = try_make(&router, session, &request)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(ending.token.name, "Sommer-Logger");
    assert_eq!(ending.token.expires_at, Some(end));
    let flights = || ask(Method::GET, FLIGHTS_PATH).token(&ending.secret);
    clock.advance(Duration::days(30) - Duration::seconds(1));
    assert_eq!(flights().send(&router).await.status, StatusCode::OK);
    clock.advance(Duration::seconds(1));
    flights()
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    assert_eq!(listed(&router, session).await.len(), 1);

    // An account holds only so many, the ended one among them, and no
    // older one makes room.
    make(&router, session, &scopes).await;
    let full = try_make(&router, session, &wanted("Third", &scopes, PASSWORD)).await;
    let message = full.error(StatusCode::CONFLICT, ErrorCode::TooManyTokens);
    assert!(message.contains("at most 2"), "{message}");
    assert_eq!(listed(&router, session).await.len(), 2);
    let withdrawn = ask(Method::DELETE, &account_token_path(&ending.token.id))
        .token(session)
        .send(&router)
        .await;
    assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
    make(&router, session, &scopes).await;
}

#[tokio::test]
async fn a_private_flight_is_read_through_a_token_only_with_the_scope() {
    let (router, _) = server(settings(), &TestClock::new());
    let dario = sign_up_private(&router, "dario").await;
    let anna = sign_up_private(&router, "anna").await;
    let flight = upload(&router, &dario.token, &sample_file()).await.flight;
    let reads = make(&router, &dario.token, &[Scope::FlightsRead])
        .await
        .secret;
    let writes = make(&router, &dario.token, &[Scope::FlightsWrite])
        .await
        .secret;
    let annas = make(&router, &anna.token, &Scope::ALL).await.secret;

    for path in [flight_path(&flight.id), flight_file_path(&flight.id)] {
        let read = ask(Method::GET, &path).token(&reads).send(&router).await;
        assert_eq!(read.status, StatusCode::OK, "{path}");
        read.is_private();
        // The pilot's own token without the scope is told which it lacks.
        let refused = ask(Method::GET, &path).token(&writes).send(&router).await;
        lacks(&refused, Scope::FlightsRead);
        // Another pilot's token, with every scope, sees what a stranger
        // sees: nothing.
        ask(Method::GET, &path)
            .token(&annas)
            .send(&router)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    }

    // Once the flight is shared, any token reads it as anybody does.
    common::set_visibility(
        &router,
        &dario.token,
        &flight.id,
        venti_api_types::v1::Visibility::Unlisted,
    )
    .await;
    for token in [&writes, &annas] {
        let read = ask(Method::GET, &flight_path(&flight.id))
            .token(token)
            .send(&router)
            .await;
        assert_eq!(read.status, StatusCode::OK);
    }
}

#[tokio::test]
async fn a_site_written_through_a_token_keeps_the_tokens_name() {
    let (router, store) = recorded_with_its_store().await;
    let request = wanted("Gipfelbuch", &[Scope::SitesWrite], DEMO_PASSWORD);
    let made: CreateAccessTokenResponse = try_make(&router, DEMO_TOKEN, &request)
        .await
        .ok(StatusCode::CREATED);
    let add = AddSiteRequest {
        site: SiteDraft {
            name: "Beispielkopf Nord".to_owned(),
            position: GeoPoint::new(47.41, 11.21).unwrap(),
            elevation_m: Some(1500.0),
            uses: vec![SiteUse::Takeoff],
            wind_directions: Vec::new(),
            paragliding: Some(true),
            hang_gliding: None,
            website: None,
            description: None,
            fee: None,
            guest_guidelines: None,
            no_flight_time: None,
            outline: None,
        },
        comment: None,
    };
    let added: SiteResponse = ask(Method::POST, SITES_PATH)
        .token(&made.secret)
        .json(&add)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let id = added.site.id.clone();

    // Everyone reads that it came through a token; which, is the
    // pilot's own matter and stays on the server.
    let history: SiteHistoryResponse = common::get(&router, &site_history_path(&id))
        .await
        .ok(StatusCode::OK);
    assert_eq!(history.revisions[0].via, SiteVia::Token);
    let text = common::get(&router, &site_history_path(&id)).await;
    assert!(!text.text().contains("Gipfelbuch"));
    let (_, kept) = store
        .pilot_site_revisions(&id, 0, 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        kept[0].via,
        venti_store::SiteVia::Token("Gipfelbuch".to_owned())
    );

    // The name stays with the revision when the token is withdrawn.
    ask(Method::DELETE, &account_token_path(&made.token.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let (_, kept) = store
        .pilot_site_revisions(&id, 0, 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        kept[0].via,
        venti_store::SiteVia::Token("Gipfelbuch".to_owned())
    );
}

#[tokio::test]
async fn the_development_mode_has_a_token_for_a_program() {
    let router = recorded().await;

    // The list, as the documents show it.
    let tokens = AccessTokensResponse {
        tokens: listed(&router, DEMO_TOKEN).await,
    };
    assert_eq!(tokens.tokens.len(), 1);
    assert_eq!(tokens.tokens[0].id, DEMO_ACCESS_TOKEN_ID);
    assert_eq!(tokens.tokens[0].name, DEMO_ACCESS_TOKEN_NAME);
    assert_golden(&golden("tokens.json"), &tokens);

    // What piece 3 of docs/data-platform.md is done by: the token for
    // "write flights" uploads a flight and is refused the groups.
    let uploaded = ask(Method::POST, FLIGHTS_PATH)
        .token(DEMO_ACCESS_TOKEN)
        .bytes(common::sample_file_on("011126"))
        .send(&router)
        .await;
    uploaded.ok::<UploadResponse>(StatusCode::CREATED);
    let refused = ask(Method::GET, GROUPS_PATH)
        .token(DEMO_ACCESS_TOKEN)
        .send(&router)
        .await;
    lacks(&refused, Scope::GroupsRead);
    assert_golden(
        &golden("error_insufficient_scope.json"),
        &refused.json::<ErrorResponse>(),
    );

    // Making one, with what differs from start to start replaced.
    let request = wanted(
        "My logger",
        &[Scope::FlightsWrite, Scope::LiveWrite],
        DEMO_PASSWORD,
    );
    assert_golden(&golden("token_create_request.json"), &request);
    let made: CreateAccessTokenResponse = try_make(&router, DEMO_TOKEN, &request)
        .await
        .ok(StatusCode::CREATED);
    let example = CreateAccessTokenResponse {
        secret: "venti_pat_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01".to_owned(),
        token: AccessToken {
            id: "3kq9x2m7r4t8v1w6y0a5b3c2d4".to_owned(),
            ..made.token.clone()
        },
    };
    assert_golden(&golden("token_created.json"), &example);

    // The pilot withdraws the demo token, and its next request is `401`.
    let withdrawn = ask(Method::DELETE, &account_token_path(DEMO_ACCESS_TOKEN_ID))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
    ask(Method::GET, FLIGHTS_PATH)
        .token(DEMO_ACCESS_TOKEN)
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
}

/// `docs/api.md` has the table of which request needs which scope. This
/// holds it to the table the tests walk.
#[test]
fn the_documentation_lists_every_request_with_its_scope() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api.md");
    let docs = std::fs::read_to_string(path).unwrap();
    for (method, path, scope) in scoped() {
        let path = path
            .replace(&format!("plans/{ID}"), "plans/{plan}")
            .replace("members/demo", "members/{handle}")
            .replace("pilots/nobody", "pilots/{handle}")
            .replace(&format!("comments/{ID}"), "comments/{comment}")
            .replace(&format!("venti-pilots:{ID}"), "{id}")
            .replace(&format!("venti:{ID}"), "{id}")
            .replace(ID, "{id}");
        let row = format!("| `{method} {path}` | `{scope}` |");
        assert!(docs.contains(&row), "docs/api.md lacks the row: {row}");
    }
    for (method, path) in for_the_session_only() {
        let path = path.replace(ID, "{id}");
        let row = format!("| `{method} {path}` | the session only |");
        assert!(docs.contains(&row), "docs/api.md lacks the row: {row}");
    }
    for written_down in [
        DEMO_ACCESS_TOKEN,
        DEMO_ACCESS_TOKEN_NAME,
        ACCESS_TOKEN_PREFIX,
    ] {
        assert!(docs.contains(written_down), "{written_down}");
    }
}
