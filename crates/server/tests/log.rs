//! What the server writes to its log while accounts are used, and what it
//! must never write there.
//!
//! This test has a file, and so a process, to itself. It collects the log
//! of its own thread, and the logging library decides once per log
//! statement whether anyone listens, on whichever thread gets there first:
//! beside other tests it would miss lines.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode, header};
use venti_api_types::v1::{
    ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNTS_PATH, ChangePasswordRequest,
    CreateAccountRequest, DeleteAccountRequest, HEALTH_PATH, Password, SESSIONS_PATH,
    SignInResponse,
};

mod common;

use common::{TestClock, ask, get, sample_file, server, settings, try_sign_in, upload};

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

#[tokio::test]
async fn no_password_and_no_token_reaches_the_log() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // This test runs on one thread, so everything the router does while
    // the guard lives is logged here, at the most talkative level.
    let _guard = tracing::subscriber::set_default(subscriber);

    let router = server(settings(), &TestClock::new()).0;
    let first = "the first password!";
    let second = "the second password";
    let guess = "hunter2-hunter2-hunter2";
    let request = CreateAccountRequest {
        handle: "dario".to_owned(),
        display_name: "Dario B.".to_owned(),
        password: Password::new(first),
        invite: None,
        device: Some("iPhone 17".to_owned()),
    };
    let signed_in: SignInResponse = ask(Method::POST, ACCOUNTS_PATH)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let token = signed_in.token.clone();

    // Wrong passwords, for an account and for none; the password typed
    // into the handle's field; a body that is not what the server expects.
    try_sign_in(&router, "dario", guess).await;
    try_sign_in(&router, "nobody", guess).await;
    try_sign_in(&router, "hunter2_hunter2", guess).await;
    try_sign_in(&router, guess, first).await;
    let malformed = ask(Method::POST, SESSIONS_PATH)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(format!(r#"{{"handle":["dario"],"password":"{guess}""#))
        .send(&router)
        .await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
    assert!(!malformed.text().contains("hunter2"));

    // Signing in, asking with the token, a bad token, changing the
    // password, an upload, deleting the account.
    let again: SignInResponse = try_sign_in(&router, "dario", first)
        .await
        .ok(StatusCode::CREATED);
    let account = |token: String| {
        let router = router.clone();
        async move {
            ask(Method::GET, ACCOUNT_PATH)
                .token(&token)
                .send(&router)
                .await
                .status
        }
    };
    assert_eq!(account(token.clone()).await, StatusCode::OK);
    let tried = format!("venti_{}", "7".repeat(52));
    assert_eq!(account(tried).await, StatusCode::UNAUTHORIZED);
    get(&router, &format!("{ACCOUNT_PATH}?token={}", "7".repeat(52))).await;
    let change = ChangePasswordRequest {
        current_password: Password::new(first),
        new_password: Password::new(second),
    };
    let changed = ask(Method::PUT, ACCOUNT_PASSWORD_PATH)
        .token(&token)
        .json(&change)
        .send(&router)
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    upload(&router, &token, &sample_file()).await;
    let deleted = ask(Method::DELETE, ACCOUNT_PATH)
        .token(&token)
        .json(&DeleteAccountRequest {
            password: Password::new(second),
            google_id_token: None,
        })
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);

    // Fifty requests that all come from one private address, as they do
    // when a proxy or Docker hides the visitors.
    for _ in 0..50 {
        let answer = ask(Method::GET, HEALTH_PATH)
            .from("172.30.250.1")
            .send(&router)
            .await;
        assert_eq!(answer.status, StatusCode::OK);
    }

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    // The server says once that it does not see who asks, and from where.
    let hidden: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("does not see its visitors' addresses"))
        .collect();
    assert_eq!(hidden.len(), 1, "{hidden:#?}");
    assert!(hidden[0].contains("172.30.250.1"), "{}", hidden[0]);
    // The log did record all this...
    for expected in [
        "account created",
        "wrong password",
        "sign-in attempt for a handle nobody has",
        "password changed",
        "account deleted",
        "/v1/sessions",
    ] {
        assert!(log.contains(expected), "the log lacks {expected:?}:\n{log}");
    }
    // ...and holds none of the secrets that went by.
    for (what, secret) in [
        ("the first password", first),
        ("the second password", second),
        ("a guessed password", guess),
        ("a password typed as the handle", "hunter2_hunter2"),
        ("a session token", &token[6..]),
        ("another session token", &again.token[6..]),
        ("a token that was tried", &"7".repeat(52)),
        ("the word Bearer", "Bearer"),
        ("a password hash", "$argon2"),
    ] {
        assert!(!log.contains(secret), "{what} is in the log:\n{log}");
    }
}
