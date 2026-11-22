//! Tests of agreeing where the pilot is signed in already, through the
//! real router: a program's request passes the authorization endpoint's
//! checks and is kept, the server's page links into the web app, and the
//! web app reads the request and answers it with the pilot's session,
//! however the pilot signed in.
//!
//! As `oauth.rs`, most of this file plays whoever should not get through:
//! a token for a program that answers, a second answer, a request that
//! has ended, a request read after it was answered, an address to go back
//! to that is not the registered one, an answer that names another
//! account, an account whose publishing is suspended. What reaches the
//! log is tested in `oauth_log.rs`.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use serde_json::{Value, json};
use venti_api_types::v1::{
    ACCOUNT_TOKENS_PATH, AUTH_GOOGLE_ACCOUNT_PATH, AUTH_GOOGLE_PATH, AccessTokensResponse,
    AnswerAuthorizationRequest, AuthorizationAnswerResponse, AuthorizationRequestResponse,
    CreateAccessTokenRequest, CreateAccessTokenResponse, CreateGoogleAccountRequest, ErrorCode,
    GoogleSignInRequest, GoogleSignUpResponse, Password, Scope, SignInResponse, account_token_path,
    oauth_request_path,
};
use venti_server::accounts::Limits;
use venti_server::accounts::google::GoogleSignIn;
use venti_server::accounts::google::dev::DevIssuer;
use venti_server::adapters::recorded_at;
use venti_server::api::{Api, Cors, Settings};
use venti_server::flights::share::PublicUrl;
use venti_sources::testing::assert_golden;
use venti_store::{MemoryStore, Store};

mod common;

use common::oauth::{
    CONNECTOR, REDIRECT, STATE, agree_in_the_web_app, answer_request, call, carried, connect,
    exchange_of, page, read_request, redirected, registered, registered_at, request, token_request,
    tokens_of, tool_json, waiting, way_on, with,
};
use common::{
    Answer, PASSWORD, PUBLIC_URL, TestClock, WEB_APP, ask, golden, sample_file, sample_file_on,
    server, settings, sign_up, upload,
};

/// A server that takes anyone, with the limits a real one has.
fn real(clock: &TestClock) -> (Router, Arc<MemoryStore>) {
    let settings = Settings {
        limits: Limits::default(),
        ..settings()
    };
    server(settings, clock)
}

/// The same, which also signs in with the development mode's "Google".
fn with_google(clock: &TestClock) -> (Router, Arc<MemoryStore>) {
    let settings = Settings {
        limits: Limits::default(),
        ..settings()
    };
    let store = Arc::new(MemoryStore::new());
    let google = GoogleSignIn::development().unwrap();
    let api = Api::new(store.clone(), Arc::new(clock.clone()), [], settings)
        .with_google(Arc::new(google));
    (api.router(&Cors::Origins(vec![WEB_APP.to_owned()])), store)
}

/// An account made by a first sign-in with Google: it has no password.
async fn sign_up_with_google(router: &Router, handle: &str) -> SignInResponse {
    let id_token = DevIssuer::new().id_token(&format!("{handle}@example.com"), None, recorded_at());
    let first: GoogleSignUpResponse = ask(Method::POST, AUTH_GOOGLE_PATH)
        .json(&GoogleSignInRequest {
            id_token,
            device: Some("Browser".to_owned()),
        })
        .send(router)
        .await
        .ok(StatusCode::OK);
    ask(Method::POST, AUTH_GOOGLE_ACCOUNT_PATH)
        .json(&CreateGoogleAccountRequest {
            signup: first.signup,
            handle: handle.to_owned(),
            display_name: "Anna Alpin".to_owned(),
            invite: None,
            device: Some("Browser".to_owned()),
        })
        .send(router)
        .await
        .ok(StatusCode::CREATED)
}

/// The tokens an account lists, as its own session sees them.
async fn listed(router: &Router, session: &str) -> Vec<venti_api_types::v1::AccessToken> {
    let answer = ask(Method::GET, ACCOUNT_TOKENS_PATH)
        .token(session)
        .send(router)
        .await;
    answer.ok::<AccessTokensResponse>(StatusCode::OK).tokens
}

/// A token the pilot makes by hand, with these scopes.
async fn by_hand(router: &Router, session: &str, scopes: &[Scope]) -> CreateAccessTokenResponse {
    ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(session)
        .json(&CreateAccessTokenRequest {
            name: "Logger".to_owned(),
            scopes: scopes
                .iter()
                .map(|scope| scope.as_str().to_owned())
                .collect(),
            expires_at: None,
            password: Password::new(PASSWORD),
            google_id_token: None,
        })
        .send(router)
        .await
        .ok(StatusCode::CREATED)
}

/// Checks that an answer is a refusal that says nothing of the request:
/// not its identifier, not the program, not where it would go back to.
fn says_nothing(answer: &Answer, id: &str) {
    assert!(!answer.headers.contains_key(header::LOCATION));
    assert!(!answer.headers.contains_key(header::SET_COOKIE));
    let text = answer.text();
    for never in [
        id,
        "venti_oar_",
        "venti_oac_",
        "Assistant",
        "assistant.example",
        STATE,
        "redirect_to",
    ] {
        assert!(!text.contains(never), "{never}: {text}");
    }
}

/// Checks that a request still waits for its answer.
async fn still_waits(router: &Router, id: &str) {
    let read = read_request(router, id).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
}

// --- The whole way ---------------------------------------------------------------

/// An account that signs in with Google has no password and cannot use the
/// form on the server's page. It agrees in the web app, with the session
/// it has, and the assistant reads its logbook.
#[tokio::test]
async fn a_pilot_without_a_password_agrees_where_they_are_signed_in() {
    let clock = TestClock::new();
    let (router, store) = with_google(&clock);
    let anna = sign_up_with_google(&router, "anna").await;
    assert!(
        store.password_hash("anna").await.unwrap().is_none(),
        "the account has no password"
    );
    let flight = upload(&router, &anna.token, &sample_file()).await.flight;
    let client_id = registered(&router, "Assistant").await;

    // The authorization endpoint keeps its checks, shows its page, and
    // redirects nobody: the way on is a link the pilot presses.
    let shown = page(&router, &request(&client_id)).await;
    assert_eq!(shown.status, StatusCode::OK);
    assert!(!shown.headers.contains_key(header::LOCATION));
    let html = shown.text();
    let id = way_on(html).expect("the page links into the web app");
    assert!(id.starts_with("venti_oar_") && id.len() == 62, "{id}");
    assert!(html.contains("Continue in Venti"), "{html}");
    // The link carries the identifier and nothing the program sent.
    let link = format!("href=\"{PUBLIC_URL}/connect/?request={id}\" rel=\"noreferrer\"");
    assert!(html.contains(&link), "{html}");
    // The form for whoever has a password is still there.
    assert!(html.contains("name=\"password\""), "{html}");
    assert!(!html.contains("cannot agree"), "{html}");

    // The web app reads what the pilot must see. Nobody is asked who
    // they are for that.
    let read = read_request(&router, &id).await;
    read.is_private();
    let asked: AuthorizationRequestResponse = read.ok(StatusCode::OK);
    assert_eq!(asked.program, "Assistant");
    assert_eq!(asked.returns_to, "assistant.example");
    assert!(!asked.on_this_device);
    assert_eq!(asked.scopes, [Scope::FlightsRead]);
    assert_eq!(asked.may_read.len(), 1);
    assert!(asked.may_read[0].starts_with("Your flights, the private ones"));
    assert!(asked.may_not.starts_with("Change, delete or upload"));
    assert!(asked.lasts.contains("180 days"), "{}", asked.lasts);
    assert_eq!(asked.expires_at, recorded_at() + Duration::minutes(15));
    // Neither the program's own value nor its challenge is handed out.
    assert!(!read.text().contains("state"), "{}", read.text());
    assert!(!read.text().contains("challenge"), "{}", read.text());
    assert_golden(&golden("oauth_request.json"), &asked);

    // The pilot agrees with the session, and the answer is the address
    // the web app sends the browser to.
    let request_body = AnswerAuthorizationRequest { agree: true };
    assert_golden(&golden("oauth_request_answer.json"), &request_body);
    let answered = answer_request(&router, &id, Some(&anna.token), true).await;
    let (address, back) = redirected(&answered);
    assert_eq!(address, REDIRECT);
    assert_eq!(carried(&back, "state"), Some(STATE));
    assert_eq!(carried(&back, "iss"), Some(PUBLIC_URL));
    let names: Vec<&str> = back.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["code", "state", "iss"]);
    let code = carried(&back, "code").unwrap().to_owned();
    assert!(code.starts_with("venti_oac_"));
    let example = AuthorizationAnswerResponse {
        redirect_to: answered
            .json::<AuthorizationAnswerResponse>()
            .redirect_to
            .replace(
                &code,
                "venti_oac_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01",
            ),
    };
    assert_golden(&golden("oauth_request_answered.json"), &example);

    // The program exchanges the code and reads the pilot's logbook.
    let tokens = tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);
    let flights = tool_json(&call(&router, Some(&tokens.access), "list_flights", json!({})).await);
    assert!(
        flights.to_string().contains(flight.id.as_str()),
        "{flights}"
    );

    // The agreement is in the pilot's list, to be withdrawn there.
    let held = listed(&router, &anna.token).await;
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].name, "Assistant");
    assert_eq!(held[0].scopes, [Scope::FlightsRead]);
    assert!(held[0].client.is_some());
    let withdrawn = ask(Method::DELETE, &account_token_path(&held[0].id))
        .token(&anna.token)
        .send(&router)
        .await;
    assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
    let after = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

/// The form on the server's page still works for whoever has a password,
/// beside the way on, and one who refuses in the web app is sent back as
/// one who refuses on the page.
#[tokio::test]
async fn the_page_keeps_its_form_and_a_refusal_goes_back_as_one() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let dario = sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;

    let tokens = connect(&router, &client_id, "dario").await;
    let listed_by_form = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    assert_eq!(listed_by_form.status, StatusCode::OK);

    let id = waiting(&router, &request(&client_id)).await;
    let refused = answer_request(&router, &id, Some(&dario.token), false).await;
    let (address, back) = redirected(&refused);
    assert_eq!(address, REDIRECT);
    assert_eq!(carried(&back, "error"), Some("access_denied"));
    assert_eq!(carried(&back, "state"), Some(STATE));
    assert_eq!(carried(&back, "iss"), Some(PUBLIC_URL));
    assert_eq!(carried(&back, "code"), None);
    assert!(!refused.text().contains("venti_oac_"));
    // Refusing withdrew nothing and gave nothing.
    assert_eq!(listed(&router, &dario.token).await.len(), 1);
}

// --- Who may answer --------------------------------------------------------------

/// A token a program holds can never agree, for itself or for another
/// program: not one the pilot made by hand with every scope, not one the
/// authorization server gave out, and nobody without a token.
#[tokio::test]
async fn only_the_pilots_own_session_answers() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let dario = sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;
    let other_program = registered(&router, "Another assistant").await;

    let every_scope = by_hand(&router, &dario.token, &Scope::ALL).await;
    let of_an_agreement = connect(&router, &client_id, "dario").await;
    let before = listed(&router, &dario.token).await.len();

    for target in [&client_id, &other_program] {
        let id = waiting(&router, &request(target)).await;
        for agree in [true, false] {
            // Made by hand, with every scope there is.
            let answer = answer_request(&router, &id, Some(&every_scope.secret), agree).await;
            answer.error(StatusCode::FORBIDDEN, ErrorCode::SessionRequired);
            says_nothing(&answer, &id);
            // What the authorization server gave out is no token at the
            // API at all, the access token as little as the refresh token.
            for token in [&of_an_agreement.access, &of_an_agreement.refresh] {
                let answer = answer_request(&router, &id, Some(token), agree).await;
                answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
                says_nothing(&answer, &id);
            }
            // Nobody, and a token nobody was given.
            let answer = answer_request(&router, &id, None, agree).await;
            answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
            says_nothing(&answer, &id);
            let made_up = format!("venti_{}", "0".repeat(52));
            let answer = answer_request(&router, &id, Some(&made_up), agree).await;
            answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
            // The identifier in the place of a token opens nothing.
            let answer = answer_request(&router, &id, Some(&id), agree).await;
            answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
        }
        // None of it used the request up or gave anything out.
        still_waits(&router, &id).await;
        assert_eq!(listed(&router, &dario.token).await.len(), before);
        // The session answers.
        agree_in_the_web_app(&router, &id, &dario.token).await;
    }

    // A session that was signed out answers no more.
    let id = waiting(&router, &request(&client_id)).await;
    let signed_out = ask(Method::DELETE, "/v1/sessions")
        .token(&dario.token)
        .send(&router)
        .await;
    assert_eq!(signed_out.status, StatusCode::NO_CONTENT);
    let answer = answer_request(&router, &id, Some(&dario.token), true).await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    still_waits(&router, &id).await;
}

/// The agreement is the account's whose session answers. A request names
/// no account, and an answer that tries to name one is not read.
#[tokio::test]
async fn the_agreement_is_of_the_account_whose_session_answers() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let dario = sign_up(&router, "dario").await;
    let bernd = sign_up(&router, "bernd").await;
    let darios = upload(&router, &dario.token, &sample_file()).await.flight;
    let bernds = upload(&router, &bernd.token, &sample_file_on("011126"))
        .await
        .flight;
    let client_id = registered(&router, "Assistant").await;

    // Dario's browser was sent to the page; Bernd holds the identifier.
    let id = waiting(&router, &request(&client_id)).await;
    // An answer that names another account is not an answer.
    for body in [
        json!({"agree": true, "account": "dario"}),
        json!({"agree": true, "handle": "dario", "account_id": dario.account.handle.as_str()}),
        json!({"agree": "dario"}),
        json!({}),
    ] {
        let answer = ask(Method::POST, &oauth_request_path(&id))
            .token(&bernd.token)
            .json(&body)
            .send(&router)
            .await;
        answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
        says_nothing(&answer, &id);
    }
    still_waits(&router, &id).await;

    let code = agree_in_the_web_app(&router, &id, &bernd.token).await;
    let tokens = tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);
    let flights = tool_json(&call(&router, Some(&tokens.access), "list_flights", json!({})).await)
        .to_string();
    assert!(flights.contains(bernds.id.as_str()), "{flights}");
    assert!(!flights.contains(darios.id.as_str()), "{flights}");
    let other = call(
        &router,
        Some(&tokens.access),
        "get_flight",
        json!({"flight_id": darios.id.as_str()}),
    )
    .await;
    assert!(
        !other.text().contains("\"isError\":false"),
        "{}",
        other.text()
    );
    // It is in Bernd's list and not in Dario's.
    assert_eq!(listed(&router, &bernd.token).await.len(), 1);
    assert!(listed(&router, &dario.token).await.is_empty());
}

/// An account whose publishing whoever runs the server suspended gives no
/// program access. It may refuse.
#[tokio::test]
async fn a_suspended_account_does_not_agree() {
    let clock = TestClock::new();
    let (router, store) = real(&clock);
    let dario = sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;
    let account = store.account("dario").await.unwrap().unwrap();
    store
        .suspend_publishing(&account.id, recorded_at())
        .await
        .unwrap()
        .unwrap();

    let id = waiting(&router, &request(&client_id)).await;
    let answer = answer_request(&router, &id, Some(&dario.token), true).await;
    answer.error(StatusCode::FORBIDDEN, ErrorCode::PublishingSuspended);
    says_nothing(&answer, &id);
    assert!(listed(&router, &dario.token).await.is_empty());
    // The request was not used up by that, and the pilot refuses it.
    still_waits(&router, &id).await;
    let (_, back) = redirected(&answer_request(&router, &id, Some(&dario.token), false).await);
    assert_eq!(carried(&back, "error"), Some("access_denied"));

    // Once the suspension is lifted the account agrees.
    store.resume_publishing(&account.id).await.unwrap();
    let id = waiting(&router, &request(&client_id)).await;
    agree_in_the_web_app(&router, &id, &dario.token).await;
}

/// An account that holds as many tokens as it may is told so, as on the
/// server's page, and answers the same request once it has made room.
#[tokio::test]
async fn a_full_account_makes_room_and_answers_the_same_request() {
    let clock = TestClock::new();
    let limits = Limits {
        tokens_per_account: 1,
        ..Limits::default()
    };
    let (router, _) = server(
        Settings {
            limits,
            ..settings()
        },
        &clock,
    );
    let dario = sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;
    let logger = by_hand(&router, &dario.token, &[Scope::FlightsWrite]).await;

    let id = waiting(&router, &request(&client_id)).await;
    let answer = answer_request(&router, &id, Some(&dario.token), true).await;
    answer.error(StatusCode::CONFLICT, ErrorCode::TooManyTokens);
    says_nothing(&answer, &id);
    let withdrawn = ask(Method::DELETE, &account_token_path(&logger.token.id))
        .token(&dario.token)
        .send(&router)
        .await;
    assert_eq!(withdrawn.status, StatusCode::NO_CONTENT);
    agree_in_the_web_app(&router, &id, &dario.token).await;
}

// --- Once, and within its time ---------------------------------------------------

#[tokio::test]
async fn a_request_is_answered_once_and_read_no_more() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let dario = sign_up(&router, "dario").await;
    let bernd = sign_up(&router, "bernd").await;
    let client_id = registered(&router, "Assistant").await;

    for first in [true, false] {
        let id = waiting(&router, &request(&client_id)).await;
        let (_, back) = redirected(&answer_request(&router, &id, Some(&dario.token), first).await);
        assert_eq!(carried(&back, "code").is_some(), first);

        // A second answer, the same or the other, by the same pilot or
        // by another, gets nothing: no second code, no refusal that
        // would follow the agreement.
        for (session, agree) in [
            (&dario.token, true),
            (&dario.token, false),
            (&bernd.token, true),
        ] {
            let again = answer_request(&router, &id, Some(session), agree).await;
            again.error(StatusCode::GONE, ErrorCode::AuthorizationRequestAnswered);
            says_nothing(&again, &id);
        }
        // And it is read no more: nothing of it is handed out again.
        let read = read_request(&router, &id).await;
        read.error(StatusCode::GONE, ErrorCode::AuthorizationRequestAnswered);
        says_nothing(&read, &id);

        if first {
            // The one code is good, once, as a code from the page is.
            let code = carried(&back, "code").unwrap();
            tokens_of(&token_request(&router, &exchange_of(&client_id, code)).await);
            assert_eq!(listed(&router, &dario.token).await.len(), 1);
            assert!(listed(&router, &bernd.token).await.is_empty());
        }
    }

    // Every showing of the page is a request of its own.
    let one = waiting(&router, &request(&client_id)).await;
    let other = waiting(&router, &request(&client_id)).await;
    assert_ne!(one, other);
    agree_in_the_web_app(&router, &one, &dario.token).await;
    still_waits(&router, &other).await;
}

#[tokio::test]
async fn an_ended_or_unknown_request_is_neither_read_nor_answered() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let dario = sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;

    let gone = |answer: Answer, id: &str| {
        answer.error(
            StatusCode::NOT_FOUND,
            ErrorCode::AuthorizationRequestNotFound,
        );
        says_nothing(&answer, id);
    };

    // A quarter of an hour, to the second.
    let id = waiting(&router, &request(&client_id)).await;
    clock.advance(Duration::minutes(15) - Duration::seconds(1));
    still_waits(&router, &id).await;
    clock.advance(Duration::seconds(1));
    gone(read_request(&router, &id).await, &id);
    for agree in [true, false] {
        gone(
            answer_request(&router, &id, Some(&dario.token), agree).await,
            &id,
        );
    }
    assert!(listed(&router, &dario.token).await.is_empty());

    // An identifier nobody was given, text that is none, and the other
    // secrets of the authorization server in its place.
    let code = {
        let id = waiting(&router, &request(&client_id)).await;
        agree_in_the_web_app(&router, &id, &dario.token).await
    };
    let never_given = format!("venti_oar_{}", "0".repeat(52));
    for unknown in [
        never_given.as_str(),
        "venti_oar_",
        "x",
        "0123456789abcdefghjkmnpqrs",
        client_id.as_str(),
        code.as_str(),
        &code.replace("venti_oac_", "venti_oar_"),
        &never_given.to_uppercase(),
    ] {
        let read = read_request(&router, unknown).await;
        read.error(
            StatusCode::NOT_FOUND,
            ErrorCode::AuthorizationRequestNotFound,
        );
        let answer = answer_request(&router, unknown, Some(&dario.token), true).await;
        answer.error(
            StatusCode::NOT_FOUND,
            ErrorCode::AuthorizationRequestNotFound,
        );
    }

    // A request that did not pass the endpoint's checks is not kept:
    // its page has no way on.
    for bad in [
        with(request(&client_id), "resource", "https://other.example/mcp"),
        with(request(&client_id), "scope", "flights:write"),
        with(request(&client_id), "code_challenge_method", "plain"),
        with(
            request(&client_id),
            "redirect_uri",
            "https://evil.example/cb",
        ),
        with(
            request(&client_id),
            "client_id",
            "0123456789abcdefghjkmnpqrs",
        ),
    ] {
        let page = page(&router, &bad).await;
        assert_eq!(page.status, StatusCode::BAD_REQUEST);
        assert_eq!(way_on(page.text()), None);
        assert!(!page.text().contains("/connect/"), "{}", page.text());
    }

    // A server that gives out no tokens has no requests.
    let plain = Settings {
        public_url: PublicUrl::new("http://venti.example").unwrap(),
        ..settings()
    };
    let (plain, _) = server(plain, &clock);
    let pilot = sign_up(&plain, "dario").await;
    let read = read_request(&plain, &never_given).await;
    read.error(
        StatusCode::NOT_FOUND,
        ErrorCode::AuthorizationRequestNotFound,
    );
    let answer = answer_request(&plain, &never_given, Some(&pilot.token), true).await;
    answer.error(
        StatusCode::NOT_FOUND,
        ErrorCode::AuthorizationRequestNotFound,
    );
}

// --- Where the browser is sent ----------------------------------------------------

/// The address in the answer is the one the program registered, to the
/// character, with the answer after it; it comes from what the server
/// kept and from nothing the web app sends.
#[tokio::test]
async fn the_browser_is_sent_to_the_registered_address_and_no_other() {
    let clock = TestClock::new();
    let (router, store) = real(&clock);
    let dario = sign_up(&router, "dario").await;

    // An address with a query of its own: the answer goes after it.
    let with_query = "https://assistant.example/oauth/callback?tenant=7";
    let client_id = registered_at(&router, "Assistant", with_query).await;
    let asked = with(request(&client_id), "redirect_uri", with_query);
    let id = waiting(&router, &asked).await;
    let answered = answer_request(&router, &id, Some(&dario.token), true).await;
    let to = answered.json::<AuthorizationAnswerResponse>().redirect_to;
    let rest = to
        .strip_prefix(with_query)
        .expect("the registered address, to the character");
    assert!(rest.starts_with("&code=venti_oac_"), "{rest}");
    assert!(
        rest.ends_with("&state=state-7f3a%2F%2B%3D%26%3F&iss=https%3A%2F%2Fventi.example"),
        "{rest}"
    );

    // What the web app sends beside the answer changes nothing: there is
    // no field for an address, and one that is sent is refused.
    let id = waiting(&router, &asked).await;
    let answer = ask(Method::POST, &oauth_request_path(&id))
        .token(&dario.token)
        .json(&json!({"agree": true, "redirect_uri": "https://evil.example/cb"}))
        .send(&router)
        .await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    let with_a_query = format!(
        "{}?redirect_uri=https%3A%2F%2Fevil.example%2Fcb&state=other",
        oauth_request_path(&id)
    );
    let answer = ask(Method::POST, &with_a_query)
        .token(&dario.token)
        .json(&AnswerAuthorizationRequest { agree: true })
        .send(&router)
        .await;
    let to = answer
        .ok::<AuthorizationAnswerResponse>(StatusCode::OK)
        .redirect_to;
    assert!(to.starts_with(&format!("{with_query}&code=")), "{to}");
    assert!(
        !to.contains("evil.example") && !to.contains("other"),
        "{to}"
    );

    // A program on the pilot's own device: the port it listens on this
    // time, as OAuth 2.1 allows, and the page says what it is.
    let local = registered_at(&router, "Local tool", "http://127.0.0.1:3000/callback").await;
    let asked = with(
        request(&local),
        "redirect_uri",
        "http://127.0.0.1:51234/callback",
    );
    let id = waiting(&router, &asked).await;
    let read: AuthorizationRequestResponse = read_request(&router, &id).await.ok(StatusCode::OK);
    assert_eq!(read.returns_to, "127.0.0.1");
    assert!(read.on_this_device);
    let (address, _) = redirected(&answer_request(&router, &id, Some(&dario.token), true).await);
    assert_eq!(address, "http://127.0.0.1:51234/callback");

    // A program whose registration is gone takes its requests along:
    // nobody is sent to an address no program stands behind.
    let unused = registered_at(&router, "Unused", "https://unused.example/cb").await;
    let asked = with(
        request(&unused),
        "redirect_uri",
        "https://unused.example/cb",
    );
    let id = waiting(&router, &asked).await;
    still_waits(&router, &id).await;
    let now = recorded_at();
    store
        .prune_oauth(now, now + Duration::seconds(1))
        .await
        .unwrap();
    assert!(store.oauth_client(&unused).await.unwrap().is_none());
    let read = read_request(&router, &id).await;
    read.error(
        StatusCode::NOT_FOUND,
        ErrorCode::AuthorizationRequestNotFound,
    );
    let answer = answer_request(&router, &id, Some(&dario.token), true).await;
    answer.error(
        StatusCode::NOT_FOUND,
        ErrorCode::AuthorizationRequestNotFound,
    );
    let _: Value = json!(CONNECTOR);
}

/// Reading and answering are counted for the address they come from, with
/// the page they follow.
#[tokio::test]
async fn reading_is_counted_for_its_address() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let never_given = format!("venti_oar_{}", "0".repeat(52));
    let mut refused = None;
    for _ in 0..70 {
        let read = read_request(&router, &never_given).await;
        if read.status == StatusCode::TOO_MANY_REQUESTS {
            refused = Some(read);
            break;
        }
        assert_eq!(read.status, StatusCode::NOT_FOUND);
    }
    let refused = refused.expect("60 a minute");
    refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(refused.headers.contains_key(header::RETRY_AFTER));
}
