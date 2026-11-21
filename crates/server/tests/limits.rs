//! Bounds per token and per address, through the real router: a token for
//! a program spends a budget of its own and every answer it gets says what
//! is left; a read of public flights without a token spends the
//! address's, said the same way.

mod common;

use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ACCOUNT_PATH, ACCOUNT_TOKENS_PATH, CreateAccessTokenRequest, CreateAccessTokenResponse,
    ErrorCode, HEALTH_PATH, PUBLIC_FLIGHTS_PATH, Password, Scope,
};

use common::{Answer, PASSWORD, TestClock, WEB_APP, ask, server, settings, sign_up};

const POLICY: &str = "ratelimit-policy";
const LEFT: &str = "ratelimit";

/// A token for a program with `scopes`, made with the session `session`;
/// its secret.
async fn token(router: &axum::Router, session: &str, scopes: &[Scope]) -> String {
    let request = CreateAccessTokenRequest {
        name: "Logger".to_owned(),
        scopes: scopes
            .iter()
            .map(|scope| scope.as_str().to_owned())
            .collect(),
        expires_at: None,
        password: Password::new(PASSWORD),
        google_id_token: None,
    };
    ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(session)
        .json(&request)
        .send(router)
        .await
        .ok::<CreateAccessTokenResponse>(StatusCode::CREATED)
        .secret
}

fn said(answer: &Answer) -> (String, String) {
    (
        answer
            .header(header::HeaderName::from_static(POLICY))
            .to_owned(),
        answer
            .header(header::HeaderName::from_static(LEFT))
            .to_owned(),
    )
}

#[tokio::test]
async fn a_token_spends_a_budget_of_its_own_and_every_answer_says_what_is_left() {
    let clock = TestClock::new();
    let mut settings = settings();
    settings.limits.token_requests_per_window = 3;
    let (router, _) = server(settings, &clock);
    let dario = sign_up(&router, "dario").await;
    let logger = token(&router, &dario.token, &[Scope::AccountRead]).await;
    let script = token(&router, &dario.token, &[Scope::AccountRead]).await;
    let read = |secret: String| {
        let router = router.clone();
        async move {
            ask(Method::GET, ACCOUNT_PATH)
                .token(&secret)
                .send(&router)
                .await
        }
    };

    // Three requests in the hour, each answer saying what is left.
    for left in [2, 1, 0] {
        let answer = read(logger.clone()).await;
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(
            said(&answer),
            (
                "\"token\";q=3;w=3600".to_owned(),
                format!("\"token\";a={left};w=3600")
            )
        );
    }
    // The fourth is refused, and says when to ask again: the same moment
    // in both fields.
    let refused = read(logger.clone()).await;
    let message = refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("this token"), "{message}");
    assert_eq!(refused.header(header::RETRY_AFTER), "3600");
    assert_eq!(said(&refused).1, "\"token\";a=0;w=3600");
    // Whatever it asks for: a read open to anyone is a request of the
    // token's as well.
    let open = ask(Method::GET, HEALTH_PATH)
        .token(&logger)
        .send(&router)
        .await;
    assert_eq!(open.status, StatusCode::TOO_MANY_REQUESTS);

    // Another token of the same account has its own budget, and the
    // pilot's session spends none: its answers say nothing of one.
    let other = read(script.clone()).await;
    assert_eq!(other.status, StatusCode::OK);
    assert_eq!(said(&other).1, "\"token\";a=2;w=3600");
    let session = read(dario.token.clone()).await;
    assert_eq!(session.status, StatusCode::OK);
    assert!(!session.headers.contains_key(LEFT));
    assert!(!session.headers.contains_key(POLICY));

    // A page may read both fields.
    let from_a_page = ask(Method::GET, ACCOUNT_PATH)
        .token(&script)
        .header(header::ORIGIN, WEB_APP)
        .send(&router)
        .await;
    let exposed = from_a_page.header(header::ACCESS_CONTROL_EXPOSE_HEADERS);
    let names: Vec<&str> = exposed.split(',').map(str::trim).collect();
    assert!(
        names.contains(&POLICY) && names.contains(&LEFT),
        "{exposed}"
    );

    // An hour after the first request the window has closed.
    clock.advance(Duration::minutes(59));
    assert_eq!(
        read(logger.clone()).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    clock.advance(Duration::minutes(1));
    let again = read(logger.clone()).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(said(&again).1, "\"token\";a=2;w=3600");
}

#[tokio::test]
async fn a_token_that_is_not_good_is_refused_whatever_it_asks_for() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    // A token for a program in its form that this server never gave out
    // (the development mode's): a read that is open to anyone is refused
    // with it, so that no one escapes the bound of an address by sending
    // made-up tokens.
    let made_up = venti_server::dev::DEMO_ACCESS_TOKEN;
    for path in [HEALTH_PATH, PUBLIC_FLIGHTS_PATH] {
        let answer = ask(Method::GET, path).token(made_up).send(&router).await;
        answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
        assert!(!answer.headers.contains_key(LEFT));
    }
    // Without a token the same reads are answered.
    let answer = ask(Method::GET, HEALTH_PATH).send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn a_read_without_a_token_spends_the_addresss_budget_and_says_so() {
    let clock = TestClock::new();
    let mut settings = settings();
    settings.limits.public_flight_reads_per_address = 2;
    settings.limits.public_flight_reads_per_network = 8;
    settings.limits.token_requests_per_window = 50;
    let (router, _) = server(settings, &clock);
    let dario = sign_up(&router, "dario").await;
    let club_app = token(&router, &dario.token, &[Scope::FlightsRead]).await;
    let from_the_club = |secret: Option<String>| {
        let router = router.clone();
        async move {
            let asked = ask(Method::GET, PUBLIC_FLIGHTS_PATH).from("203.0.113.7");
            match secret {
                Some(secret) => asked.token(&secret),
                None => asked,
            }
            .send(&router)
            .await
        }
    };

    for left in [1, 0] {
        let answer = from_the_club(None).await;
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(
            said(&answer),
            (
                "\"address\";q=2;w=3600".to_owned(),
                format!("\"address\";a={left};w=3600")
            )
        );
    }
    let refused = from_the_club(None).await;
    refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert_eq!(refused.header(header::RETRY_AFTER), "3600");
    assert_eq!(said(&refused).1, "\"address\";a=0;w=3600");

    // A program with a token behind the same address reads on, from its
    // own budget, and is told about that one.
    let with_a_token = from_the_club(Some(club_app)).await;
    assert_eq!(with_a_token.status, StatusCode::OK);
    let (policy, left) = said(&with_a_token);
    assert_eq!(policy, "\"token\";q=50;w=3600");
    // Making the token was a request of the session's, not the token's.
    assert_eq!(left, "\"token\";a=49;w=3600");
}
