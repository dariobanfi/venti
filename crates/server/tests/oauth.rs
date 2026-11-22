//! Tests of the authorization server and of what it opens at the
//! connector, through the real router: a program registers itself, a pilot
//! signs in on the server's own page and agrees, the program exchanges the
//! code and reads that pilot's logbook over the connector, and the pilot
//! withdraws the agreement.
//!
//! Most of this file plays the attacker: a code used twice, a wrong
//! verifier, a redirect address that differs by a character, another
//! server's `resource`, a token used where it was not given out for, a
//! scope that writes, another pilot's private flight, a refresh token used
//! twice, an answer to the page that the page did not send, the page in a
//! frame, markup in a program's name. What reaches the log is tested in
//! `oauth_log.rs`.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use serde_json::{Value, json};
use venti_api_types::v1::{
    ACCOUNT_TOKENS_PATH, AccessTokensResponse, FLIGHTS_PATH, Scope, Visibility, account_token_path,
};
use venti_server::accounts::Limits;
use venti_server::accounts::secret::{OauthKind, OauthSecret};
use venti_server::api::{AUTHORIZE_PATH, Settings, TOKEN_PATH};
use venti_server::clock::Clock;
use venti_server::flights::share::PublicUrl;
use venti_store::{AuthorizationCode, MemoryStore, NewGrant, Redeemed, Store, TokenClient};

mod common;

use common::oauth::{
    CHALLENGE, CONNECTOR, REDIRECT, STATE, Tokens, agree, answer_page, asks_for_a_token, call,
    carried, connect, connector, decide, exchange_of, form, page, refresh_of, refused, register,
    registered, registered_at, request, revoke, sent_back, shown, token_request, tokens_of,
    tool_json, tool_text, with, without,
};
use common::{
    Answer, PASSWORD, PUBLIC_URL, TestClock, ask, get, sample_file, sample_file_on, server,
    set_visibility, settings, sign_up, sign_up_private, try_sign_in, upload,
};

const PROTECTED_RESOURCE: &str = "/.well-known/oauth-protected-resource";
const AUTHORIZATION_SERVER: &str = "/.well-known/oauth-authorization-server";

/// A server that takes anyone, with the limits a real one has.
fn real(clock: &TestClock) -> (Router, Arc<MemoryStore>) {
    with_limits(clock, Limits::default())
}

/// The same, for a test that asks more often than one address may: the
/// bounds per address have tests of their own.
fn patient(clock: &TestClock) -> (Router, Arc<MemoryStore>) {
    let limits = Limits {
        client_registrations_per_address: 1000,
        oauth_requests_per_address: 1000,
        ..Limits::default()
    };
    with_limits(clock, limits)
}

fn with_limits(clock: &TestClock, limits: Limits) -> (Router, Arc<MemoryStore>) {
    let settings = Settings {
        limits,
        ..settings()
    };
    server(settings, clock)
}

/// The tokens an account lists, as its own session sees them.
async fn listed(router: &Router, session: &str) -> Vec<venti_api_types::v1::AccessToken> {
    let answer = ask(Method::GET, ACCOUNT_TOKENS_PATH)
        .token(session)
        .send(router)
        .await;
    answer.ok::<AccessTokensResponse>(StatusCode::OK).tokens
}

/// Checks that the connector refuses a token as one that is not good.
fn not_good(answer: &Answer) {
    let challenge = asks_for_a_token(answer);
    assert!(challenge.contains("error=\"invalid_token\""), "{challenge}");
}

/// Checks that an answer is a page that sends nobody anywhere: it has no
/// form to agree with and no `Location`.
fn stops(answer: &Answer, status: StatusCode) {
    assert_eq!(answer.status, status, "{}", answer.text());
    assert!(!answer.headers.contains_key(header::LOCATION));
    assert!(!answer.headers.contains_key(header::SET_COOKIE));
    let html = answer.text();
    assert!(html.contains("Nothing was agreed to"), "{html}");
    assert!(!html.contains("<form"), "{html}");
    assert!(!html.contains("name=\"password\""));
    is_dressed(answer);
}

/// Checks what every page of the authorization server carries: it is
/// shown in no frame, runs no script, loads nothing from elsewhere and is
/// kept by nobody.
fn is_dressed(answer: &Answer) {
    assert_eq!(
        answer.header(header::CONTENT_TYPE),
        "text/html; charset=utf-8"
    );
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    assert_eq!(answer.header(header::X_FRAME_OPTIONS), "DENY");
    assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    let policy = answer.header(header::CONTENT_SECURITY_POLICY);
    assert!(policy.contains("default-src 'none'"), "{policy}");
    assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
    assert!(policy.contains("base-uri 'none'"), "{policy}");
    assert!(!policy.contains("unsafe"), "{policy}");
    assert!(!policy.contains("script"), "{policy}");
    let html = answer.text().to_ascii_lowercase();
    for never in [
        "<script",
        "javascript:",
        " onclick",
        " onload",
        " style=",
        "<iframe",
        "<img",
    ] {
        assert!(!html.contains(never), "{never}");
    }
}

// --- What the server says of itself ---------------------------------------------

#[tokio::test]
async fn the_server_says_what_the_connector_is_and_who_gives_out_tokens_for_it() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);

    // The connector as a resource (RFC 9728), at the root and under the
    // connector's own path, which is where a client looks first.
    for path in [
        PROTECTED_RESOURCE.to_owned(),
        format!("{PROTECTED_RESOURCE}/mcp"),
    ] {
        let answer = get(&router, &path).await;
        assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
        let resource: Value = answer.ok(StatusCode::OK);
        assert_eq!(resource["resource"], CONNECTOR);
        assert_eq!(resource["authorization_servers"], json!([PUBLIC_URL]));
        assert_eq!(resource["scopes_supported"], json!(["flights:read"]));
        assert_eq!(resource["bearer_methods_supported"], json!(["header"]));
    }

    // The authorization server (RFC 8414): its name is the address the
    // document was fetched from, every endpoint is under that address,
    // and it does what OAuth 2.1 asks and nothing older.
    let answer = get(&router, AUTHORIZATION_SERVER).await;
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    let server: Value = answer.ok(StatusCode::OK);
    assert_eq!(server["issuer"], PUBLIC_URL);
    for (endpoint, path) in [
        ("authorization_endpoint", "/oauth/authorize"),
        ("token_endpoint", "/oauth/token"),
        ("registration_endpoint", "/oauth/register"),
        ("revocation_endpoint", "/oauth/revoke"),
    ] {
        assert_eq!(server[endpoint], format!("{PUBLIC_URL}{path}"));
    }
    assert_eq!(server["response_types_supported"], json!(["code"]));
    assert_eq!(
        server["grant_types_supported"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(server["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(
        server["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
    assert_eq!(server["scopes_supported"], json!(["flights:read"]));
    assert_eq!(
        server["authorization_response_iss_parameter_supported"],
        true
    );
    // The server fetches no address a program names.
    assert_eq!(server["client_id_metadata_document_supported"], false);
    // Nothing that writes is on offer anywhere.
    for scope in Scope::ALL {
        if scope != Scope::FlightsRead {
            assert!(
                !answer.text().contains(scope.as_str()),
                "{}",
                scope.as_str()
            );
        }
    }
}

/// A server that is reached over plain HTTP from elsewhere gives out no
/// tokens: the password and every token would travel in the clear.
#[tokio::test]
async fn a_server_without_https_gives_out_no_tokens() {
    let clock = TestClock::new();
    let plain = Settings {
        public_url: PublicUrl::new("http://venti.example").unwrap(),
        ..settings()
    };
    let (router, _) = server(plain, &clock);
    for path in [
        PROTECTED_RESOURCE,
        "/.well-known/oauth-protected-resource/mcp",
        AUTHORIZATION_SERVER,
    ] {
        assert_eq!(get(&router, path).await.status, StatusCode::NOT_FOUND);
    }
    let answer = register(&router, &json!({"redirect_uris": [REDIRECT]})).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    let answer = ask(Method::GET, AUTHORIZE_PATH).send(&router).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    let answer = token_request(&router, &exchange_of("x", "y")).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);

    // The connector does not offer what nobody can get a token for.
    let list = connector(&router, None, "tools/list", json!({})).await;
    assert_eq!(list.status, StatusCode::OK);
    assert!(!list.text().contains("list_flights"), "{}", list.text());
    let answer = call(&router, None, "list_flights", json!({})).await;
    assert!(
        !answer.text().contains("flights_listed"),
        "{}",
        answer.text()
    );
}

// --- Registration ---------------------------------------------------------------

#[tokio::test]
async fn a_program_registers_itself_and_gets_no_secret() {
    let clock = TestClock::new();
    let (router, _) = patient(&clock);

    let answer = register(
        &router,
        &json!({
            "client_name": "Assistant",
            "redirect_uris": [REDIRECT, "http://127.0.0.1:33418/callback"],
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "scope": "flights:read",
            // What the server does not read, it does not keep or fetch.
            "logo_uri": "https://assistant.example/logo.png",
            "client_uri": "https://assistant.example",
            "application_type": "web",
        }),
    )
    .await;
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    let registered: Value = answer.ok(StatusCode::CREATED);
    assert_eq!(registered["client_id"].as_str().unwrap().len(), 26);
    assert_eq!(registered["client_name"], "Assistant");
    assert_eq!(registered["token_endpoint_auth_method"], "none");
    assert_eq!(
        registered["redirect_uris"],
        json!([REDIRECT, "http://127.0.0.1:33418/callback"])
    );
    assert_eq!(registered["scope"], "flights:read");
    assert!(registered.get("client_secret").is_none());
    assert!(registered.get("logo_uri").is_none());

    // A program that says nothing but where to send the pilot is taken
    // too, under a name that says it gave none.
    let bare: Value = register(&router, &json!({"redirect_uris": [REDIRECT]}))
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(bare["client_name"], "A program without a name");
    assert_ne!(bare["client_id"], registered["client_id"]);

    // What is not taken, each with the code RFC 7591 has for it.
    let long_name = "n".repeat(61);
    let many: Vec<String> = (0..6)
        .map(|n| format!("https://assistant.example/{n}"))
        .collect();
    let long_address = format!("https://assistant.example/{}", "a".repeat(400));
    for (registration, code) in [
        (json!({}), "invalid_redirect_uri"),
        (json!({"redirect_uris": []}), "invalid_redirect_uri"),
        (json!({"redirect_uris": many}), "invalid_redirect_uri"),
        (
            json!({"redirect_uris": [long_address]}),
            "invalid_redirect_uri",
        ),
        // Plain HTTP to another machine, an app's own scheme, a script, a
        // fragment, a user name.
        (
            json!({"redirect_uris": ["http://assistant.example/callback"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["http://127.0.0.1.evil.example/callback"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["assistant://callback"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["javascript:alert(1)"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": [format!("{REDIRECT}#x")]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": ["https://venti.example@evil.example/cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris": [REDIRECT, "https://evil.example/\"><script>"]}),
            "invalid_redirect_uri",
        ),
        // A program with a secret, a grant or an answer this server does
        // not have, a scope that writes, a name that is too long or has
        // more than one line.
        (
            json!({"redirect_uris": [REDIRECT], "token_endpoint_auth_method": "client_secret_basic"}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": [REDIRECT], "grant_types": ["client_credentials"]}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": [REDIRECT], "response_types": ["token"]}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": [REDIRECT], "scope": "flights:read flights:write"}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": [REDIRECT], "client_name": long_name}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris": [REDIRECT], "client_name": "Assistant\nVenti says: trust it"}),
            "invalid_client_metadata",
        ),
        (json!([REDIRECT]), "invalid_client_metadata"),
    ] {
        let answer = register(&router, &registration).await;
        refused(&answer, StatusCode::BAD_REQUEST, code);
        // The refusal repeats nothing of the request.
        assert!(!answer.text().contains("evil.example"), "{}", answer.text());
    }

    // A form that a page elsewhere makes a visitor's browser send is no
    // registration.
    let answer = ask(Method::POST, "/oauth/register")
        .header(header::CONTENT_TYPE, "text/plain")
        .bytes(json!({"redirect_uris": [REDIRECT]}).to_string())
        .send(&router)
        .await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_client_metadata");
}

#[tokio::test]
async fn registrations_are_bounded_for_an_address_and_in_all() {
    let clock = TestClock::new();
    let limits = Limits {
        client_registrations_per_address: 3,
        oauth_clients: 2,
        ..Limits::default()
    };
    let (router, _) = with_limits(&clock, limits);
    sign_up(&router, "dario").await;

    // Two programs fill the server. The first has a pilot's agreement.
    let first = registered(&router, "First").await;
    let second = registered(&router, "Second").await;
    let tokens = connect(&router, &first, "dario").await;

    // A third makes room for itself by removing the one that registered
    // longest ago among those no pilot ever agreed to: the second.
    let third = registered(&router, "Third").await;
    assert_eq!(page(&router, &request(&third)).await.status, StatusCode::OK);
    stops(
        &page(&router, &request(&second)).await,
        StatusCode::BAD_REQUEST,
    );
    assert_eq!(page(&router, &request(&first)).await.status, StatusCode::OK);
    let answer = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 0);

    // The fourth from this address within the hour is one too many.
    let answer = register(&router, &json!({"redirect_uris": [REDIRECT]})).await;
    refused(&answer, StatusCode::TOO_MANY_REQUESTS, "too_many_requests");
    assert!(answer.headers.contains_key(header::RETRY_AFTER));
    // From another address: both stored programs that could make room
    // are gone or agreed to... the third never was, so it makes room.
    let answer = ask(Method::POST, "/oauth/register")
        .from("203.0.113.9")
        .json(&json!({"redirect_uris": [REDIRECT]}))
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::CREATED);
    stops(
        &page(&router, &request(&third)).await,
        StatusCode::BAD_REQUEST,
    );
    assert_eq!(page(&router, &request(&first)).await.status, StatusCode::OK);
}

// --- The whole way --------------------------------------------------------------

/// What the brief of this piece calls done: an assistant registers, the
/// pilot agrees, the assistant reads the pilot's logbook, the private
/// flights too, goes on after a refresh, and is refused once the pilot
/// withdraws the agreement.
#[tokio::test]
async fn an_assistant_reads_a_pilots_logbook_from_their_agreement_to_its_withdrawal() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up_private(&router, "dario").await.token;
    let private = upload(&router, &session, &sample_file()).await.flight;
    let public = upload(&router, &session, &sample_file_on("050726"))
        .await
        .flight;
    set_visibility(&router, &session, &public.id, Visibility::Public).await;
    assert_eq!(private.visibility, Visibility::Private);

    // 1. The assistant asks for the logbook and is told where a token is
    //    got. A public tool answers without one.
    let answer = call(&router, None, "list_flights", json!({})).await;
    let challenge = asks_for_a_token(&answer);
    assert!(!challenge.contains("error="), "{challenge}");
    assert!(!answer.text().contains(private.id.as_str()));
    let open = call(&router, None, "public_flights", json!({"pilot": "dario"})).await;
    assert_eq!(tool_json(&open)["flights"].as_array().unwrap().len(), 1);

    // 2. It reads who gives tokens out, and that server's description.
    let resource: Value = get(&router, "/.well-known/oauth-protected-resource/mcp")
        .await
        .ok(StatusCode::OK);
    let issuer = resource["authorization_servers"][0].as_str().unwrap();
    assert_eq!(issuer, PUBLIC_URL);
    let described: Value = get(&router, AUTHORIZATION_SERVER).await.ok(StatusCode::OK);
    assert_eq!(described["issuer"], issuer);

    // 3. It registers itself.
    let client_id = registered(&router, "Assistant").await;

    // 4. It sends the pilot's browser to the page, which says who asks,
    //    where the pilot is sent back to and what may be read.
    let shown_page = page(&router, &request(&client_id)).await;
    assert_eq!(shown_page.status, StatusCode::OK);
    is_dressed(&shown_page);
    let html = shown_page.text();
    assert!(html.contains("calls itself <b>“Assistant”</b>"), "{html}");
    assert!(html.contains("Venti has not checked who"), "{html}");
    assert!(html.contains(REDIRECT), "{html}");
    assert!(
        html.contains("the private ones and those you share"),
        "{html}"
    );
    assert!(html.contains("Change, delete or upload anything"), "{html}");
    // Nothing is agreed to by looking.
    assert!(listed(&router, &session).await.is_empty());

    // 5. The pilot signs in there and agrees; the browser is sent back
    //    with a code, the program's `state` and who answered.
    let code = agree(&router, &client_id, "dario").await;
    assert!(code.starts_with("venti_oac_"));
    // A code is no token yet, and nothing is listed before the exchange.
    not_good(&call(&router, Some(&code), "list_flights", json!({})).await);
    assert!(listed(&router, &session).await.is_empty());

    // 6. The program exchanges the code.
    let tokens = tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);
    assert!(tokens.access.starts_with("venti_oat_"));
    assert!(tokens.refresh.starts_with("venti_ort_"));

    // 7. It reads the logbook: both flights, the private one too, and
    //    the answer is marked as made for one account.
    let answer = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    answer.is_private();
    let list = tool_json(&answer);
    assert_eq!(list["flights_listed"], 2);
    let ids: Vec<&str> = list["flights"]
        .as_array()
        .unwrap()
        .iter()
        .map(|flight| flight["flight_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&private.id.as_str()) && ids.contains(&public.id.as_str()));
    let answer = call(
        &router,
        Some(&tokens.access),
        "get_flight",
        json!({"flight_id": private.id.as_str()}),
    )
    .await;
    answer.is_private();
    let flight = tool_json(&answer);
    assert_eq!(flight["shared"], "private: only the pilot");
    assert!(flight["took_off_at"]["latitude"].is_number());
    let answer = call(
        &router,
        Some(&tokens.access),
        "logbook_statistics",
        json!({}),
    )
    .await;
    answer.is_private();
    assert_eq!(tool_json(&answer)["total"]["flights"], 2);
    // A public tool answers with the token as without it.
    let with_token = call(
        &router,
        Some(&tokens.access),
        "public_flights",
        json!({"pilot": "dario"}),
    )
    .await;
    assert_eq!(with_token.body, open.body);

    // 8. The pilot sees the agreement among their tokens, under the name
    //    the program gave itself and marked as one that came this way.
    let tokens_listed = listed(&router, &session).await;
    assert_eq!(tokens_listed.len(), 1);
    let agreement = &tokens_listed[0];
    assert_eq!(agreement.name, "Assistant");
    assert_eq!(agreement.scopes, [Scope::FlightsRead]);
    let bound = agreement.client.as_ref().expect("the program it is for");
    assert_eq!(bound.id, client_id);
    assert_eq!(bound.resource, CONNECTOR);
    assert_eq!(
        agreement.expires_at,
        Some(clock.now() + Duration::days(180))
    );

    // 9. An hour later the access token has ended. The program uses its
    //    refresh token and reads on with the new one.
    clock.advance(Duration::minutes(61));
    not_good(&call(&router, Some(&tokens.access), "list_flights", json!({})).await);
    let fresh = tokens_of(&token_request(&router, &refresh_of(&client_id, &tokens.refresh)).await);
    assert_ne!(fresh.access, tokens.access);
    assert_ne!(fresh.refresh, tokens.refresh);
    let answer = call(&router, Some(&fresh.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 2);
    not_good(&call(&router, Some(&tokens.access), "list_flights", json!({})).await);
    // It is still the one agreement, and it now ends later.
    let tokens_listed = listed(&router, &session).await;
    assert_eq!(tokens_listed.len(), 1);
    assert_eq!(tokens_listed[0].id, agreement.id);
    assert_eq!(
        tokens_listed[0].expires_at,
        Some(clock.now() + Duration::days(180))
    );

    // 10. The pilot withdraws it, as any other token. From then on the
    //     program is refused: at the connector, and when it asks for new
    //     tokens.
    let answer = ask(Method::DELETE, &account_token_path(&agreement.id))
        .token(&session)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert!(listed(&router, &session).await.is_empty());
    let answer = call(&router, Some(&fresh.access), "list_flights", json!({})).await;
    not_good(&answer);
    assert!(!answer.text().contains(private.id.as_str()));
    let answer = token_request(&router, &refresh_of(&client_id, &fresh.refresh)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    // The public tools stay open to it, without a token.
    let open = call(&router, None, "public_flights", json!({"pilot": "dario"})).await;
    assert_eq!(tool_json(&open)["flights"].as_array().unwrap().len(), 1);
}

// --- The code -------------------------------------------------------------------

#[tokio::test]
async fn a_code_is_good_once_for_its_program_and_its_verifier() {
    let clock = TestClock::new();
    let (router, _) = patient(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let other_program = registered(&router, "Other").await;
    let code = agree(&router, &client_id, "dario").await;
    let good = exchange_of(&client_id, &code);

    // Whoever holds the code and not what the program kept gets nothing,
    // and takes nothing from the program that does.
    let wrong_verifier = "x".repeat(43);
    for (attempt, status, error) in [
        (
            with(good.clone(), "code_verifier", &wrong_verifier),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        // The challenge itself is no verifier: there is no `plain`.
        (
            with(good.clone(), "code_verifier", CHALLENGE),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            without(good.clone(), "code_verifier"),
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        // Another registered program, and no program.
        (
            with(good.clone(), "client_id", &other_program),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            with(good.clone(), "client_id", "0000000000000000000000000a"),
            StatusCode::UNAUTHORIZED,
            "invalid_client",
        ),
        (
            without(good.clone(), "client_id"),
            StatusCode::UNAUTHORIZED,
            "invalid_client",
        ),
        // A program that sends a secret takes this for another server.
        (
            with(good.clone(), "client_secret", "secret"),
            StatusCode::UNAUTHORIZED,
            "invalid_client",
        ),
        // Another address to be sent back to than the code was sent to.
        (
            with(
                good.clone(),
                "redirect_uri",
                "https://assistant.example/oauth/callbacl",
            ),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        // Another server's resource, a near one, and none.
        (
            with(good.clone(), "resource", "https://other.example/mcp"),
            StatusCode::BAD_REQUEST,
            "invalid_target",
        ),
        (
            with(good.clone(), "resource", PUBLIC_URL),
            StatusCode::BAD_REQUEST,
            "invalid_target",
        ),
        (
            without(good.clone(), "resource"),
            StatusCode::BAD_REQUEST,
            "invalid_target",
        ),
        // Another grant, and a code that is none.
        (
            with(good.clone(), "grant_type", "password"),
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
        ),
        (
            with(good.clone(), "code", "venti_oac_nothing"),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
    ] {
        let answer = token_request(&router, &attempt).await;
        refused(&answer, status, error);
        assert!(!answer.text().contains(&code), "{}", answer.text());
    }
    // As JSON, or twice the same parameter: not a request.
    let answer = ask(Method::POST, TOKEN_PATH)
        .json(&json!({"grant_type": "authorization_code", "code": code}))
        .send(&router)
        .await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_request");
    let twice = format!("{}&code_verifier={}", form(&good), "y".repeat(43));
    let answer = ask(Method::POST, TOKEN_PATH)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .bytes(twice)
        .send(&router)
        .await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_request");
    assert!(listed(&router, &session).await.is_empty());

    // After all of that the code is still the program's, which gets its
    // tokens; the same address again, as OAuth 2.0 clients send it, is
    // taken.
    let exchange = with(good.clone(), "redirect_uri", REDIRECT);
    let tokens = tokens_of(&token_request(&router, &exchange).await);
    let answer = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 0);

    // A second exchange of the code gets nothing, and what came of the
    // first is withdrawn: somebody else saw the code.
    let answer = token_request(&router, &good).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    not_good(&call(&router, Some(&tokens.access), "list_flights", json!({})).await);
    let answer = token_request(&router, &refresh_of(&client_id, &tokens.refresh)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    assert!(listed(&router, &session).await.is_empty());
    // But not by somebody who has the code alone.
    let code = agree(&router, &client_id, "dario").await;
    let tokens = tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);
    let replay = with(
        exchange_of(&client_id, &code),
        "code_verifier",
        &wrong_verifier,
    );
    refused(
        &token_request(&router, &replay).await,
        StatusCode::BAD_REQUEST,
        "invalid_grant",
    );
    let answer = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 0);
}

#[tokio::test]
async fn a_code_lasts_two_minutes() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;

    let code = agree(&router, &client_id, "dario").await;
    clock.advance(Duration::seconds(119));
    tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);

    let code = agree(&router, &client_id, "dario").await;
    clock.advance(Duration::seconds(120));
    let answer = token_request(&router, &exchange_of(&client_id, &code)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    // The agreement of before stands: a code that ended changed nothing.
    assert_eq!(listed(&router, &session).await.len(), 1);
}

// --- What a program may ask for -------------------------------------------------

#[tokio::test]
async fn nobody_is_sent_to_an_address_the_program_did_not_register() {
    let clock = TestClock::new();
    let (router, _) = patient(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let good = request(&client_id);

    for other in [
        // One character.
        "https://assistant.example/oauth/callbacl",
        "https://assistant.example/oauth/callback/",
        "https://assistant.example/oauth/callback?next=https://evil.example",
        "https://assistant.example:443/oauth/callback",
        "https://Assistant.example/oauth/callback",
        "https://assistant.example.evil.example/oauth/callback",
        "https://assistant.example/oauth/../oauth/callback",
        "http://assistant.example/oauth/callback",
        "https://evil.example/oauth/callback",
        "javascript:alert(1)",
        "",
    ] {
        let asked = with(good.clone(), "redirect_uri", other);
        // Looking at the page: told, and sent nowhere.
        let answer = page(&router, &asked).await;
        stops(&answer, StatusCode::BAD_REQUEST);
        if !other.is_empty() {
            assert!(!answer.text().contains(other), "{other}");
        }
        assert!(!answer.text().contains("evil.example"));

        // Answering it, with the right password and a good cookie from a
        // page for the good request: the same.
        let shown = shown(&page(&router, &good).await);
        let mut fields = asked.clone();
        fields.push(("csrf", shown.csrf.clone()));
        fields.push(("handle", "dario".to_owned()));
        fields.push(("password", PASSWORD.to_owned()));
        fields.push(("decision", "agree".to_owned()));
        stops(
            &answer_page(&router, &fields, &shown.cookie).await,
            StatusCode::BAD_REQUEST,
        );
    }
    // Without the address at all, and for a program nobody registered.
    stops(
        &page(&router, &without(good.clone(), "redirect_uri")).await,
        StatusCode::BAD_REQUEST,
    );
    for unknown in [
        "0000000000000000000000000a",
        "https://assistant.example/client.json",
        "",
    ] {
        let answer = page(&router, &with(good.clone(), "client_id", unknown)).await;
        stops(&answer, StatusCode::BAD_REQUEST);
        assert!(!answer.text().contains("assistant.example"));
    }
    // A parameter twice is not a request a program makes.
    let twice = format!(
        "{AUTHORIZE_PATH}?{}&redirect_uri=https%3A%2F%2Fevil.example",
        form(&good)
    );
    stops(&get(&router, &twice).await, StatusCode::BAD_REQUEST);
    assert!(listed(&router, &session).await.is_empty());

    // A program on the pilot's own device is sent back to whatever port
    // it listens on that time, and to nothing else that differs; the
    // page says that Venti cannot tell which program it is.
    let local = registered_at(&router, "Desktop", "http://127.0.0.1:3000/callback").await;
    let other_port = with(
        request(&local),
        "redirect_uri",
        "http://127.0.0.1:51234/callback",
    );
    let answer = page(&router, &other_port).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(
        answer
            .text()
            .contains("is a program on the device this browser runs on"),
        "{}",
        answer.text()
    );
    for other in [
        "http://127.0.0.1:3000/callbacl",
        "http://localhost:3000/callback",
        "http://127.0.0.1.evil.example:3000/callback",
        "https://127.0.0.1:3000/callback",
    ] {
        let asked = with(request(&local), "redirect_uri", other);
        stops(&page(&router, &asked).await, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn what_venti_does_not_give_is_refused_before_the_pilot_is_asked() {
    let clock = TestClock::new();
    let (router, _) = patient(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let good = request(&client_id);
    let long_state = "s".repeat(513);

    let mut asked = vec![
        // The resource: another server's, this server without the
        // connector's path, a near one, none.
        (
            with(good.clone(), "resource", "https://other.example/mcp"),
            "invalid_target",
        ),
        (with(good.clone(), "resource", PUBLIC_URL), "invalid_target"),
        (
            with(good.clone(), "resource", "https://venti.example/mcp/"),
            "invalid_target",
        ),
        (
            with(
                good.clone(),
                "resource",
                "https://venti.example.evil.example/mcp",
            ),
            "invalid_target",
        ),
        (without(good.clone(), "resource"), "invalid_target"),
        // PKCE: none, the method that proves nothing, a challenge that
        // is none.
        (without(good.clone(), "code_challenge"), "invalid_request"),
        (
            without(good.clone(), "code_challenge_method"),
            "invalid_request",
        ),
        (
            with(good.clone(), "code_challenge_method", "plain"),
            "invalid_request",
        ),
        (
            with(good.clone(), "code_challenge", "short"),
            "invalid_request",
        ),
        // The answer OAuth 2.1 no longer has.
        (
            with(good.clone(), "response_type", "token"),
            "unsupported_response_type",
        ),
        (
            without(good.clone(), "response_type"),
            "unsupported_response_type",
        ),
        (with(good.clone(), "state", &long_state), "invalid_request"),
        // Scopes: what this server has not, and what the connector does
        // not offer.
        (with(good.clone(), "scope", "openid"), "invalid_scope"),
        (with(good.clone(), "scope", "account:read"), "invalid_scope"),
        (
            with(good.clone(), "scope", "flights:read groups:read"),
            "invalid_scope",
        ),
    ];
    // Every scope that writes, alone and beside the one that is given.
    for scope in Scope::ALL {
        if scope.as_str().ends_with(":write") {
            asked.push((with(good.clone(), "scope", scope.as_str()), "invalid_scope"));
            let beside = format!("flights:read {}", scope.as_str());
            asked.push((with(good.clone(), "scope", &beside), "invalid_scope"));
        }
    }
    assert!(asked.len() > 20);

    for (asked, error) in asked {
        // The pilot is told, is not asked for a password, and is sent
        // nowhere by the server: the way back is a link.
        let answer = page(&router, &asked).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{error}");
        assert!(!answer.headers.contains_key(header::LOCATION));
        assert!(!answer.headers.contains_key(header::SET_COOKIE));
        is_dressed(&answer);
        let html = answer.text();
        assert!(html.contains("Nothing was agreed to"), "{html}");
        assert!(
            !html.contains("<form") && !html.contains("password"),
            "{html}"
        );
        let link = format!("href=\"{REDIRECT}?error={error}&amp;error_description=");
        assert!(html.contains(&link), "{error}: {html}");
        assert!(html.contains("iss=https%3A%2F%2Fventi.example"), "{html}");

        // Sent as the page's answer, with everything else in order, it
        // is no agreement either.
        let shown = shown(&page(&router, &good).await);
        let mut fields = asked.clone();
        fields.push(("csrf", shown.csrf.clone()));
        fields.push(("handle", "dario".to_owned()));
        fields.push(("password", PASSWORD.to_owned()));
        fields.push(("decision", "agree".to_owned()));
        let answer = answer_page(&router, &fields, &shown.cookie).await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{error}");
        assert!(!answer.headers.contains_key(header::LOCATION));
    }
    assert!(listed(&router, &session).await.is_empty());

    // The scope left out is the one scope there is.
    let answer = page(&router, &without(good.clone(), "scope")).await;
    assert_eq!(answer.status, StatusCode::OK);
}

// --- The page -------------------------------------------------------------------

#[tokio::test]
async fn the_page_is_shown_in_no_frame_and_what_a_program_says_is_only_text() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    sign_up(&router, "dario").await;
    let markup = "<script>alert(1)</script><b title=x>\"'&";
    let client_id = registered(&router, markup).await;
    let state = "\"><script>alert(2)</script>";
    let asked = with(request(&client_id), "state", state);

    let answer = page(&router, &asked).await;
    assert_eq!(answer.status, StatusCode::OK);
    // No frame, no script, nothing loaded, nothing kept: `is_dressed`
    // also looks for markup that got through.
    is_dressed(&answer);
    let policy = answer.header(header::CONTENT_SECURITY_POLICY);
    // The one form goes to this server and, by the redirect that answers
    // it, to the origin the program registered: nowhere else.
    assert!(
        policy.contains("form-action 'self' https://assistant.example;")
            || policy.ends_with("form-action 'self' https://assistant.example")
            || policy.contains("form-action 'self' https://assistant.example "),
        "{policy}"
    );
    assert_eq!(answer.header(header::REFERRER_POLICY), "same-origin");
    assert_eq!(answer.headers.get("x-robots-tag").unwrap(), "noindex");

    let html = answer.text();
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;&lt;b title=x&gt;&quot;&#39;&amp;"),
        "{html}"
    );
    assert!(html.contains("value=\"&quot;&gt;&lt;script&gt;alert(2)&lt;/script&gt;\""));
    assert!(!html.contains("alert(1)</script>") && !html.contains("<b title"));
    // The name is marked as the program's own claim.
    assert!(
        html.contains("The name is the program's own claim"),
        "{html}"
    );

    // The cookie against forged answers names nobody, is read by no
    // script, is sent with no request from another site, and can be set
    // by this host alone.
    let cookie = answer.header(header::SET_COOKIE);
    assert!(cookie.starts_with("__Host-venti-consent="), "{cookie}");
    for attribute in [
        "Path=/",
        "Max-Age=900",
        "HttpOnly",
        "SameSite=Strict",
        "Secure",
    ] {
        assert!(cookie.contains(attribute), "{cookie}");
    }
    assert!(!cookie.contains("Domain"), "{cookie}");
    // Each showing of the page has a value of its own.
    let again = page(&router, &asked).await;
    assert_ne!(shown(&again).csrf, shown(&answer).csrf);

    // The pilot agrees: the state comes back as it was sent, and the
    // cookie is taken away again.
    let answer = decide(&router, &asked, "dario", PASSWORD, "agree").await;
    let (_, back) = sent_back(&answer);
    assert_eq!(carried(&back, "state"), Some(state));
    assert_eq!(answer.header(header::X_FRAME_OPTIONS), "DENY");
    assert!(answer.header(header::SET_COOKIE).contains("Max-Age=0"));
    // The name is among the pilot's tokens as text, like any other.
    let code = carried(&back, "code").unwrap().to_owned();
    tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);
}

#[tokio::test]
async fn an_answer_the_page_did_not_send_is_no_answer() {
    let clock = TestClock::new();
    let (router, _) = patient(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let asked = request(&client_id);
    let shown = shown(&page(&router, &asked).await);
    let fields = |csrf: &str, decision: &str| {
        let mut fields = asked.clone();
        fields.push(("csrf", csrf.to_owned()));
        fields.push(("handle", "dario".to_owned()));
        fields.push(("password", PASSWORD.to_owned()));
        fields.push(("decision", decision.to_owned()));
        fields
    };
    let good = fields(&shown.csrf, "agree");
    let post = |fields: Vec<(&'static str, String)>| {
        ask(Method::POST, AUTHORIZE_PATH)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .bytes(form(&fields))
    };

    // What a page elsewhere can make the pilot's browser send: the form
    // with the right password, but without the cookie; with a value of
    // its own making in both places it can reach; with another showing's
    // value; and from another origin, cookie and all.
    let other = "0123456789abcdefghjkmnpqrs";
    let forged = [
        post(good.clone()).header(header::ORIGIN, PUBLIC_URL),
        post(good.clone()),
        post(fields(other, "agree")).header(header::COOKIE, &shown.cookie),
        post(good.clone()).header(header::COOKIE, &format!("__Host-venti-consent={other}")),
        post(good.clone()).header(header::COOKIE, "venti-consent=".to_owned().as_str()),
        post(without(good.clone(), "csrf")).header(header::COOKIE, &shown.cookie),
        post(fields("", "agree")).header(header::COOKIE, "__Host-venti-consent="),
        post(good.clone())
            .header(header::COOKIE, &shown.cookie)
            .header(header::ORIGIN, "https://evil.example"),
        post(good.clone())
            .header(header::COOKIE, &shown.cookie)
            .header(header::ORIGIN, "null"),
        // Refusing for the pilot is not a page elsewhere's to do either.
        post(fields(&shown.csrf, "refuse")),
    ];
    for request in forged {
        stops(&request.send(&router).await, StatusCode::FORBIDDEN);
    }
    // Neither is a request to look at the page an answer to it, whatever
    // it carries.
    let get_with_answer = format!("{AUTHORIZE_PATH}?{}", form(&good));
    let answer = ask(Method::GET, &get_with_answer)
        .header(header::COOKIE, &shown.cookie)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(!answer.headers.contains_key(header::LOCATION));
    assert!(listed(&router, &session).await.is_empty());

    // An answer the page could not have sent.
    let answer = answer_page(&router, &fields(&shown.csrf, "maybe"), &shown.cookie).await;
    stops(&answer, StatusCode::BAD_REQUEST);

    // The pilot's own answer, with a wrong password: the page again, and
    // nothing agreed to. The password is not written back into the page.
    for (handle, password) in [
        ("dario", "wrong horse battery"),
        ("nobody-here", PASSWORD),
        ("not a handle!", PASSWORD),
        ("dario", ""),
    ] {
        let answer = decide(&router, &asked, handle, password, "agree").await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
        assert!(!answer.headers.contains_key(header::LOCATION));
        is_dressed(&answer);
        let html = answer.text();
        assert!(html.contains("The handle or the password is not correct"));
        assert!(!html.contains("wrong horse") && !html.contains(PASSWORD));
    }
    assert!(listed(&router, &session).await.is_empty());

    // The pilot refuses, which needs no password: the program is told.
    let answer = decide(&router, &asked, "", "", "refuse").await;
    let (address, back) = sent_back(&answer);
    assert_eq!(address, REDIRECT);
    assert_eq!(carried(&back, "error"), Some("access_denied"));
    assert_eq!(carried(&back, "state"), Some(STATE));
    assert_eq!(carried(&back, "iss"), Some(PUBLIC_URL));
    assert!(carried(&back, "code").is_none());
    assert!(listed(&router, &session).await.is_empty());
}

#[tokio::test]
async fn the_password_on_the_page_is_guessed_no_faster_than_at_the_sign_in() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let asked = request(&client_id);
    let per_handle = Limits::default().wrong_passwords_per_handle;

    // Wrong passwords at the sign-in and on the page count together.
    for attempt in 0..per_handle {
        let status = if attempt % 2 == 0 {
            try_sign_in(&router, "dario", "wrong horse battery")
                .await
                .status
        } else {
            decide(&router, &asked, "dario", "wrong horse battery", "agree")
                .await
                .status
        };
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {attempt}");
    }
    // From then on the right password is refused too, in both places,
    // and nothing is agreed to.
    let answer = decide(&router, &asked, "dario", PASSWORD, "agree").await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(!answer.headers.contains_key(header::LOCATION));
    assert!(answer.text().contains("Too many wrong passwords"));
    is_dressed(&answer);
    let answer = try_sign_in(&router, "dario", PASSWORD).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(listed(&router, &session).await.is_empty());

    // An hour later the pilot agrees.
    clock.advance(Duration::minutes(61));
    agree(&router, &client_id, "dario").await;
}

#[tokio::test]
async fn an_account_that_is_full_is_told_on_the_page() {
    let clock = TestClock::new();
    let limits = Limits {
        tokens_per_account: 1,
        ..Limits::default()
    };
    let (router, _) = with_limits(&clock, limits);
    let session = sign_up(&router, "dario").await.token;
    let first = registered(&router, "First").await;
    let second = registered(&router, "Second").await;
    connect(&router, &first, "dario").await;

    let answer = decide(&router, &request(&second), "dario", PASSWORD, "agree").await;
    assert_eq!(answer.status, StatusCode::CONFLICT);
    assert!(!answer.headers.contains_key(header::LOCATION));
    assert!(
        answer
            .text()
            .contains("as many programs with access as it may have")
    );
    // Agreeing again with the program that has the agreement replaces
    // it: there is one, and the earlier tokens are worth nothing.
    let before = listed(&router, &session).await;
    let again = connect(&router, &first, "dario").await;
    let after = listed(&router, &session).await;
    assert_eq!(after.len(), 1);
    assert_ne!(after[0].id, before[0].id);
    let answer = call(&router, Some(&again.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 0);
}

// --- Tokens ---------------------------------------------------------------------

#[tokio::test]
async fn a_refresh_token_is_good_once_and_its_second_use_ends_the_agreement() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let other_program = registered(&router, "Other").await;
    let first = connect(&router, &client_id, "dario").await;

    // What gets nothing and ends nothing: another program with this
    // refresh token, a refresh that asks for more than was agreed to or
    // for another server, an access token as a refresh token.
    for (attempt, status, error) in [
        (
            refresh_of(&other_program, &first.refresh),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            with(
                refresh_of(&client_id, &first.refresh),
                "scope",
                "flights:write",
            ),
            StatusCode::BAD_REQUEST,
            "invalid_scope",
        ),
        (
            with(
                refresh_of(&client_id, &first.refresh),
                "scope",
                "flights:read account:read",
            ),
            StatusCode::BAD_REQUEST,
            "invalid_scope",
        ),
        (
            with(
                refresh_of(&client_id, &first.refresh),
                "resource",
                "https://other.example/mcp",
            ),
            StatusCode::BAD_REQUEST,
            "invalid_target",
        ),
        (
            refresh_of(&client_id, &first.access),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            without(refresh_of(&client_id, &first.refresh), "client_id"),
            StatusCode::UNAUTHORIZED,
            "invalid_client",
        ),
    ] {
        refused(&token_request(&router, &attempt).await, status, error);
    }
    assert_eq!(listed(&router, &session).await.len(), 1);

    // The program refreshes: the old access token and the old refresh
    // token are worth nothing from that moment.
    let second = tokens_of(&token_request(&router, &refresh_of(&client_id, &first.refresh)).await);
    not_good(&call(&router, Some(&first.access), "list_flights", json!({})).await);
    let answer = call(&router, Some(&second.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 0);

    // The old refresh token turns up again: one of its two users is not
    // the program. Nobody gets tokens, and the agreement is withdrawn,
    // with the tokens the rightful refresh made.
    let answer = token_request(&router, &refresh_of(&client_id, &first.refresh)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    not_good(&call(&router, Some(&second.access), "list_flights", json!({})).await);
    let answer = token_request(&router, &refresh_of(&client_id, &second.refresh)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    assert!(listed(&router, &session).await.is_empty());
}

#[tokio::test]
async fn an_agreement_ends_when_its_program_has_not_used_it_for_half_a_year() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;
    let first = connect(&router, &client_id, "dario").await;

    clock.advance(Duration::days(179));
    let second = tokens_of(&token_request(&router, &refresh_of(&client_id, &first.refresh)).await);
    clock.advance(Duration::days(180));
    let answer = token_request(&router, &refresh_of(&client_id, &second.refresh)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
}

/// A token is good where it was given out for, and nowhere else: the
/// connector's at the API, the API's at the connector, whatever their
/// scopes, and one that was given out for another address.
#[tokio::test]
async fn a_token_is_good_only_where_it_was_given_out_for() {
    let clock = TestClock::new();
    let (router, store) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    upload(&router, &session, &sample_file()).await;
    let client_id = registered(&router, "Assistant").await;
    let tokens = connect(&router, &client_id, "dario").await;

    // The connector's token at the API: it has the scope `flights:read`,
    // and reads no flights there. Neither does its refresh token.
    for token in [&tokens.access, &tokens.refresh] {
        for path in [FLIGHTS_PATH, "/v1/account", ACCOUNT_TOKENS_PATH] {
            let answer = ask(Method::GET, path).token(token).send(&router).await;
            assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{path}");
            assert!(!answer.text().contains("dario"), "{}", answer.text());
        }
    }

    // A token the pilot made by hand for the API, with every scope
    // there is: no token at the connector, for the logbook or for
    // anything else. Neither is the app's own session.
    let made = ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(&session)
        .json(&json!({
            "name": "Logger",
            "scopes": Scope::ALL.iter().map(|scope| scope.as_str()).collect::<Vec<_>>(),
            "password": PASSWORD,
        }))
        .send(&router)
        .await;
    let made: Value = made.ok(StatusCode::CREATED);
    let by_hand = made["secret"].as_str().unwrap();
    assert_eq!(
        ask(Method::GET, FLIGHTS_PATH)
            .token(by_hand)
            .send(&router)
            .await
            .status,
        StatusCode::OK
    );
    for other in [
        by_hand,
        session.as_str(),
        tokens.refresh.as_str(),
        "nonsense",
    ] {
        for tool in [
            "list_flights",
            "get_flight",
            "logbook_statistics",
            "public_flights",
        ] {
            let answer = call(&router, Some(other), tool, json!({})).await;
            not_good(&answer);
            assert!(!answer.text().contains("flight_id"), "{}", answer.text());
        }
        not_good(&connector(&router, Some(other), "tools/list", json!({})).await);
    }
    // In another header's clothes it is no token either.
    for value in [
        format!("Basic {}", tokens.access),
        tokens.access.clone(),
        format!("Bearer {} {}", tokens.access, tokens.access),
    ] {
        let answer = ask(Method::POST, "/mcp")
            .header(header::AUTHORIZATION, &value)
            .header(header::ACCEPT, "application/json, text/event-stream")
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
            .send(&router)
            .await;
        not_good(&answer);
    }
    // And never in the address: a token there is not looked at.
    let answer = ask(
        Method::POST,
        &format!("/mcp?access_token={}", tokens.access),
    )
    .header(header::ACCEPT, "application/json, text/event-stream")
    .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "list_flights", "arguments": {}}}))
    .send(&router)
    .await;
    assert_ne!(answer.status, StatusCode::OK);

    // An agreement that names another address, as only a fault in the
    // server or another server on the same database could have made it:
    // its access token is good, and is not taken here.
    let account = store.account("dario").await.unwrap().unwrap();
    let now = clock.now();
    let elsewhere = OauthSecret::new(OauthKind::Access).unwrap();
    let code = OauthSecret::new(OauthKind::Code).unwrap();
    let other_program = registered(&router, "Other").await;
    let record = AuthorizationCode {
        client_id: other_program.clone(),
        account_id: account.id.clone(),
        redirect_uri: REDIRECT.to_owned(),
        scopes: vec!["flights:read".to_owned()],
        resource: "https://other.example/mcp".to_owned(),
        challenge: CHALLENGE.to_owned(),
        created_at: now,
        expires_at: now + Duration::minutes(2),
    };
    assert!(
        store
            .create_authorization_code(&code.hash(), &record)
            .await
            .unwrap()
    );
    let grant = NewGrant {
        token: venti_store::AccessToken {
            id: "0000000000000000000000000b".to_owned(),
            account_id: account.id.clone(),
            name: "Other".to_owned(),
            scopes: vec!["flights:read".to_owned()],
            created_at: now,
            last_used_on: None,
            expires_at: Some(now + Duration::days(180)),
            client: Some(TokenClient {
                client_id: other_program,
                resource: "https://other.example/mcp".to_owned(),
                access_until: now + Duration::hours(1),
            }),
        },
        access: elsewhere.hash(),
        refresh: OauthSecret::new(OauthKind::Refresh).unwrap().hash(),
    };
    let redeemed = store
        .redeem_authorization_code(&code.hash(), &grant, now, 20)
        .await
        .unwrap();
    assert_eq!(redeemed, Redeemed::Granted);
    for tool in ["list_flights", "public_flights"] {
        not_good(&call(&router, Some(elsewhere.expose()), tool, json!({})).await);
    }
    // The token that was given out for the connector reads on.
    let answer = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
    assert_eq!(tool_json(&answer)["flights_listed"], 1);
}

#[tokio::test]
async fn a_program_gives_its_tokens_up() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    let client_id = registered(&router, "Assistant").await;
    let other_program = registered(&router, "Other").await;

    // Another program cannot give this one's tokens up, and is answered
    // as if it had: there is nothing to learn here.
    let tokens = connect(&router, &client_id, "dario").await;
    for token in [&tokens.access, &tokens.refresh] {
        let answer = revoke(&router, &other_program, token).await;
        assert_eq!(answer.status, StatusCode::OK);
    }
    assert_eq!(
        revoke(&router, &client_id, "nonsense").await.status,
        StatusCode::OK
    );
    assert_eq!(listed(&router, &session).await.len(), 1);
    let answer = revoke(&router, "0000000000000000000000000a", &tokens.access).await;
    refused(&answer, StatusCode::UNAUTHORIZED, "invalid_client");

    // By its refresh token or by its access token: the whole agreement.
    for by_refresh in [true, false] {
        let tokens: Tokens = connect(&router, &client_id, "dario").await;
        let given_up = if by_refresh {
            &tokens.refresh
        } else {
            &tokens.access
        };
        let answer = revoke(&router, &client_id, given_up).await;
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
        not_good(&call(&router, Some(&tokens.access), "list_flights", json!({})).await);
        let answer = token_request(&router, &refresh_of(&client_id, &tokens.refresh)).await;
        refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
        assert!(listed(&router, &session).await.is_empty());
    }
}

// --- The connector --------------------------------------------------------------

/// A flight its pilot keeps private, or shares by link, is read by that
/// pilot's own assistant and by nobody else's, and is in no public tool's
/// answer, with any token or with none.
#[tokio::test]
async fn a_private_flight_is_read_by_its_pilots_assistant_and_no_other() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let dario = sign_up_private(&router, "dario").await.token;
    let anna = sign_up_private(&router, "anna").await.token;
    let private = upload(&router, &dario, &sample_file()).await.flight;
    let by_link = upload(&router, &dario, &sample_file_on("050726"))
        .await
        .flight;
    set_visibility(&router, &dario, &by_link.id, Visibility::Unlisted).await;
    let public = upload(&router, &dario, &sample_file_on("060726"))
        .await
        .flight;
    set_visibility(&router, &dario, &public.id, Visibility::Public).await;
    let annas = upload(&router, &anna, &sample_file_on("070726"))
        .await
        .flight;

    let client_id = registered(&router, "Assistant").await;
    let darios_assistant = connect(&router, &client_id, "dario").await;
    let annas_assistant = connect(&router, &client_id, "anna").await;

    // Anna's assistant reads Anna's logbook: none of Dario's flights is
    // in it, not even the public one.
    let answer = call(
        &router,
        Some(&annas_assistant.access),
        "list_flights",
        json!({}),
    )
    .await;
    let list = tool_json(&answer);
    assert_eq!(list["flights_listed"], 1);
    assert_eq!(list["flights"][0]["flight_id"], annas.id.as_str());
    let answer = call(
        &router,
        Some(&annas_assistant.access),
        "logbook_statistics",
        json!({}),
    )
    .await;
    assert_eq!(tool_json(&answer)["total"]["flights"], 1);
    for theirs in [&private, &by_link, &public] {
        let answer = call(
            &router,
            Some(&annas_assistant.access),
            "get_flight",
            json!({"flight_id": theirs.id.as_str()}),
        )
        .await;
        let (text, failed) = tool_text(&answer);
        assert!(failed, "{text}");
        assert!(
            text.contains("no flight with this id in the pilot's logbook"),
            "{text}"
        );
        assert!(
            !text.contains("latitude") && !text.contains("dario"),
            "{text}"
        );
    }
    // No argument makes a tool read another account.
    for arguments in [
        json!({"pilot": "dario"}),
        json!({"account": "dario", "handle": "dario", "account_id": "dario"}),
    ] {
        let answer = call(
            &router,
            Some(&annas_assistant.access),
            "list_flights",
            arguments,
        )
        .await;
        assert_eq!(answer.status, StatusCode::OK);
        assert!(!answer.text().contains(private.id.as_str()));
        assert!(!answer.text().contains(by_link.id.as_str()));
    }

    // The public tools: Dario's private flight and the one shared by
    // link are in none of their answers, for nobody: not without a
    // token, not for Anna's assistant, and not for Dario's own either,
    // which reads them with the logbook's tools.
    let bbox = json!({"west": -180.0, "south": -85.0, "east": 180.0, "north": 85.0});
    for token in [
        None,
        Some(&annas_assistant.access),
        Some(&darios_assistant.access),
    ] {
        let token = token.map(String::as_str);
        for hidden in [&private, &by_link] {
            let answer = call(
                &router,
                token,
                "public_flight",
                json!({"flight_id": hidden.id.as_str()}),
            )
            .await;
            let (text, failed) = tool_text(&answer);
            assert!(failed && text.contains("no public flight"), "{text}");
        }
        let mut answers = vec![
            call(&router, token, "public_flights", json!({"pilot": "dario"})).await,
            call(&router, token, "public_flights", json!({})).await,
            call(&router, token, "public_flights", json!({"bbox": bbox})).await,
            call(&router, token, "takeoff_candidates", json!({"bbox": bbox})).await,
        ];
        let shown: Value = tool_json(&answers[0]);
        assert_eq!(shown["flights"].as_array().unwrap().len(), 1);
        answers.push(
            call(
                &router,
                token,
                "public_flight",
                json!({"flight_id": public.id.as_str()}),
            )
            .await,
        );
        for answer in &answers {
            assert_eq!(answer.status, StatusCode::OK);
            let text = answer.text();
            assert!(!text.contains(private.id.as_str()), "{text}");
            assert!(!text.contains(by_link.id.as_str()), "{text}");
        }
    }

    // Dario's assistant reads all three of Dario's.
    let answer = call(
        &router,
        Some(&darios_assistant.access),
        "list_flights",
        json!({}),
    )
    .await;
    assert_eq!(tool_json(&answer)["flights_listed"], 3);
    let answer = call(
        &router,
        Some(&darios_assistant.access),
        "get_flight",
        json!({"flight_id": by_link.id.as_str()}),
    )
    .await;
    assert_eq!(
        tool_json(&answer)["shared"],
        "by link: whoever has its link"
    );
}

#[tokio::test]
async fn the_list_of_tools_is_the_same_for_everyone() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    sign_up(&router, "dario").await;
    let client_id = registered(&router, "Assistant").await;
    let tokens = connect(&router, &client_id, "dario").await;

    let open = connector(&router, None, "tools/list", json!({})).await;
    let signed = connector(&router, Some(&tokens.access), "tools/list", json!({})).await;
    assert_eq!(open.status, StatusCode::OK);
    assert_eq!(open.body, signed.body);
    // Listing asks nobody to sign in: an assistant that only wants the
    // wind is never sent to the page.
    assert!(!open.headers.contains_key(header::WWW_AUTHENTICATE));
    let listed: Value = open.json();
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    for tool in venti_tools::TOOLS {
        assert!(names.contains(&tool.name), "{}", tool.name);
    }
    // Every tool only reads.
    for tool in listed["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
    }
    // An answer to a request with a token is kept by nobody.
    signed.is_private();
}

/// A batch of the oldest revision that asks for the logbook among other
/// things is refused whole without a token: no call of it is answered.
#[tokio::test]
async fn a_batch_that_asks_for_the_logbook_needs_the_token_as_one_call_does() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    let flight = upload(&router, &session, &sample_file()).await.flight;
    let batch = json!([
        {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "public_flights", "arguments": {}}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "list_flights", "arguments": {}}},
    ]);
    let send = |token: Option<String>| {
        let mut request = ask(Method::POST, "/mcp")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .json(&batch);
        if let Some(token) = token {
            request = request.token(&token);
        }
        request.send(&router)
    };
    let answer = send(None).await;
    asks_for_a_token(&answer);
    assert!(!answer.text().contains("flights"), "{}", answer.text());

    let client_id = registered(&router, "Assistant").await;
    let tokens = connect(&router, &client_id, "dario").await;
    let answer = send(Some(tokens.access)).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    answer.is_private();
    assert!(answer.text().contains(flight.id.as_str()));
}

#[tokio::test]
async fn a_token_has_its_budget_at_the_connector() {
    let clock = TestClock::new();
    let limits = Limits {
        token_requests_per_window: 3,
        ..Limits::default()
    };
    let (router, _) = with_limits(&clock, limits);
    sign_up(&router, "dario").await;
    sign_up(&router, "anna").await;
    let client_id = registered(&router, "Assistant").await;
    let tokens = connect(&router, &client_id, "dario").await;
    let annas = connect(&router, &client_id, "anna").await;

    for _ in 0..3 {
        let answer = call(&router, Some(&tokens.access), "list_flights", json!({})).await;
        assert_eq!(answer.status, StatusCode::OK);
    }
    // The fourth request of the hour, for the logbook or for anything.
    for tool in ["list_flights", "public_flights"] {
        let answer = call(&router, Some(&tokens.access), tool, json!({})).await;
        assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
        assert!(answer.headers.contains_key(header::RETRY_AFTER));
    }
    // A refresh does not begin the count again: the budget is the
    // agreement's.
    let fresh = tokens_of(&token_request(&router, &refresh_of(&client_id, &tokens.refresh)).await);
    let answer = call(&router, Some(&fresh.access), "list_flights", json!({})).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    // Another pilot's agreement with the same program has its own, and
    // requests without a token are not counted here.
    let answer = call(&router, Some(&annas.access), "list_flights", json!({})).await;
    assert_eq!(answer.status, StatusCode::OK);
    let answer = call(&router, None, "public_flights", json!({})).await;
    assert_eq!(answer.status, StatusCode::OK);

    clock.advance(Duration::minutes(61));
    let fresh = tokens_of(&token_request(&router, &refresh_of(&client_id, &fresh.refresh)).await);
    let answer = call(&router, Some(&fresh.access), "list_flights", json!({})).await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn the_authorization_server_is_bounded_for_an_address() {
    let clock = TestClock::new();
    let limits = Limits {
        oauth_requests_per_address: 4,
        ..Limits::default()
    };
    let (router, _) = with_limits(&clock, limits);
    let client_id = registered(&router, "Assistant").await;
    let asked = request(&client_id);

    // The page, an answer to it, an exchange and giving a token up count
    // together; registering has its own count.
    assert_eq!(page(&router, &asked).await.status, StatusCode::OK);
    assert_eq!(
        decide(&router, &asked, "", "", "refuse").await.status,
        StatusCode::SEE_OTHER
    );
    let answer = token_request(&router, &exchange_of(&client_id, "venti_oac_x")).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");

    let answer = page(&router, &asked).await;
    assert_eq!(answer.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(answer.headers.contains_key(header::RETRY_AFTER));
    is_dressed(&answer);
    let answer = token_request(&router, &exchange_of(&client_id, "venti_oac_x")).await;
    refused(&answer, StatusCode::TOO_MANY_REQUESTS, "too_many_requests");
    assert!(answer.headers.contains_key(header::RETRY_AFTER));
    let answer = revoke(&router, &client_id, "x").await;
    refused(&answer, StatusCode::TOO_MANY_REQUESTS, "too_many_requests");
    // What the server says of itself is not counted: it costs nothing.
    assert_eq!(
        get(&router, AUTHORIZATION_SERVER).await.status,
        StatusCode::OK
    );

    clock.advance(Duration::minutes(2));
    assert_eq!(page(&router, &asked).await.status, StatusCode::OK);
}

/// An agreement goes with its account, as every token does.
#[tokio::test]
async fn deleting_the_account_ends_its_agreements() {
    let clock = TestClock::new();
    let (router, _) = real(&clock);
    let session = sign_up(&router, "dario").await.token;
    upload(&router, &session, &sample_file()).await;
    let client_id = registered(&router, "Assistant").await;
    let tokens = connect(&router, &client_id, "dario").await;
    let code = agree(&router, &client_id, "dario").await;

    let answer = ask(Method::DELETE, "/v1/account")
        .token(&session)
        .json(&json!({"password": PASSWORD}))
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT, "{}", answer.text());

    not_good(&call(&router, Some(&tokens.access), "list_flights", json!({})).await);
    let answer = token_request(&router, &refresh_of(&client_id, &tokens.refresh)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
    let answer = token_request(&router, &exchange_of(&client_id, &code)).await;
    refused(&answer, StatusCode::BAD_REQUEST, "invalid_grant");
}
