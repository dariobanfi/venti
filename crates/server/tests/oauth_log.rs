//! What the server writes to its log while a program is given access to a
//! pilot's logbook, and what it must never write there: a code, a token,
//! a verifier, a password, or anything else a request held.
//!
//! Like `log.rs` and `live_log.rs`, this test has a file, and so a
//! process, to itself: it collects the log of its own thread, and the
//! logging library decides once per log statement whether anyone listens.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode, header};
use serde_json::json;
use venti_server::accounts::Limits;
use venti_server::api::{AUTHORIZE_PATH, Settings};

mod common;

use common::oauth::{
    CHALLENGE, REDIRECT, STATE, VERIFIER, call, carried, decide, exchange_of, form, page,
    refresh_of, register, request, revoke, sent_back, shown, token_request, tokens_of, with,
};
use common::{PASSWORD, TestClock, ask, sample_file, server, settings, sign_up, upload};

/// Collects what the server logs while a test holds it.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A name no other text of these tests holds.
const NAME: &str = "Zephyrine Assistant 7Q";
const WRONG_PASSWORD: &str = "quite the wrong password";

#[tokio::test]
async fn no_code_token_verifier_or_password_reaches_the_log() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // This test runs on one thread, so everything the router does while
    // the guard lives is logged here, at the most talkative level.
    let _guard = tracing::subscriber::set_default(subscriber);

    let clock = TestClock::new();
    let real = Settings {
        limits: Limits::default(),
        ..settings()
    };
    let (router, _) = server(real, &clock);
    let session = sign_up(&router, "dario").await.token;
    let flight = upload(&router, &session, &sample_file()).await.flight;

    // A program registers, and one that is refused.
    let answer = register(
        &router,
        &json!({"client_name": NAME, "redirect_uris": [REDIRECT]}),
    )
    .await;
    let registered: serde_json::Value = answer.ok(StatusCode::CREATED);
    let client_id = registered["client_id"].as_str().unwrap().to_owned();
    let refused_name = format!("{NAME} refused");
    register(
        &router,
        &json!({"client_name": refused_name, "redirect_uris": ["http://evil.example/zephyr"]}),
    )
    .await;

    // The page, as it is looked at, refused by the server, answered with
    // a wrong password, answered by a page elsewhere, and agreed to.
    let asked = request(&client_id);
    let looked = page(&router, &asked).await;
    let first_csrf = shown(&looked).csrf;
    page(
        &router,
        &with(asked.clone(), "resource", "https://other.example/mcp"),
    )
    .await;
    page(
        &router,
        &with(asked.clone(), "redirect_uri", "https://evil.example/zephyr"),
    )
    .await;
    let wrong = decide(&router, &asked, "dario", WRONG_PASSWORD, "agree").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    let mut forged = asked.clone();
    forged.push(("csrf", first_csrf.clone()));
    forged.push(("handle", "dario".to_owned()));
    forged.push(("password", PASSWORD.to_owned()));
    forged.push(("decision", "agree".to_owned()));
    let answer = ask(Method::POST, AUTHORIZE_PATH)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, "https://evil.example")
        .bytes(form(&forged))
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    // A form that is cut off in the middle of the password.
    let cut = format!("{}&password=cut%2", form(&asked));
    ask(Method::POST, AUTHORIZE_PATH)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .bytes(cut)
        .send(&router)
        .await;
    let agreed = decide(&router, &asked, "dario", PASSWORD, "agree").await;
    let (_, back) = sent_back(&agreed);
    let code = carried(&back, "code").unwrap().to_owned();

    // The exchange: with a wrong verifier, rightly, and a second time.
    let wrong_verifier = "w".repeat(43);
    let attempt = with(
        exchange_of(&client_id, &code),
        "code_verifier",
        &wrong_verifier,
    );
    token_request(&router, &attempt).await;
    let first = tokens_of(&token_request(&router, &exchange_of(&client_id, &code)).await);
    let answer = call(&router, Some(&first.access), "list_flights", json!({})).await;
    assert!(answer.text().contains(flight.id.as_str()));
    token_request(&router, &exchange_of(&client_id, &code)).await;

    // Again, then a refresh, the old refresh token a second time, a
    // token that is given up, and tokens that are not good.
    let agreed = decide(&router, &asked, "dario", PASSWORD, "agree").await;
    let (_, back) = sent_back(&agreed);
    let second_code = carried(&back, "code").unwrap().to_owned();
    let second = tokens_of(&token_request(&router, &exchange_of(&client_id, &second_code)).await);
    let third = tokens_of(&token_request(&router, &refresh_of(&client_id, &second.refresh)).await);
    call(
        &router,
        Some(&third.access),
        "get_flight",
        json!({"flight_id": flight.id.as_str()}),
    )
    .await;
    token_request(&router, &refresh_of(&client_id, &second.refresh)).await;
    revoke(&router, &client_id, &third.refresh).await;
    let never_good = "venti_oat_0000000000000000000000000000000000000000000000000000";
    call(&router, Some(never_good), "list_flights", json!({})).await;
    call(&router, Some(&session), "list_flights", json!({})).await;
    // A token where it does not belong: in the address.
    ask(Method::POST, &format!("/mcp?access_token={}", third.access))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .send(&router)
        .await;
    ask(
        Method::GET,
        &format!("/oauth/token?refresh_token={}", third.refresh),
    )
    .send(&router)
    .await;

    drop(_guard);
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    // The log was listened to: it has what it should have.
    assert!(
        log.contains("a pilot agreed that a program may read their flights"),
        "{log}"
    );
    assert!(log.contains("exchanged a second time"), "{log}");
    assert!(
        log.contains("a refresh token was used a second time"),
        "{log}"
    );
    assert!(log.contains(&client_id), "{log}");

    let never = [
        ("a code", code.as_str()),
        ("a code", second_code.as_str()),
        ("an access token", first.access.as_str()),
        ("an access token", second.access.as_str()),
        ("an access token", third.access.as_str()),
        ("an access token that is none", never_good),
        ("a refresh token", first.refresh.as_str()),
        ("a refresh token", second.refresh.as_str()),
        ("a refresh token", third.refresh.as_str()),
        ("a session's token", session.as_str()),
        ("the verifier", VERIFIER),
        ("a wrong verifier", wrong_verifier.as_str()),
        ("the challenge", CHALLENGE),
        ("the program's state", STATE),
        ("the program's state", "state-7f3a"),
        ("the value against forged answers", first_csrf.as_str()),
        ("the password", PASSWORD),
        ("a wrong password", WRONG_PASSWORD),
        ("the name a program gave itself", NAME),
        ("the name a program gave itself", "Zephyrine"),
        ("an address a program named", "assistant.example"),
        ("an address a program named", "evil.example"),
        ("an address a program named", "zephyr"),
        ("a query", "code_challenge"),
        ("a query", "access_token="),
        ("a query", "refresh_token="),
        // The prefixes of the three kinds of secret, whatever follows.
        ("a code", "venti_oac_"),
        ("an access token", "venti_oat_"),
        ("a refresh token", "venti_ort_"),
        ("a flight's address", flight.id.as_str()),
    ];
    for (what, text) in never {
        assert!(!log.contains(text), "the log holds {what}:\n{log}");
    }
}
