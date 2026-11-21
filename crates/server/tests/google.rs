//! Signing in with Google through the real router: a first sign-in that
//! goes on to a handle, the sign-ins after it, linking and unlinking, what
//! the registration setting does, and what a server without Google says.
//!
//! The tokens are signed by the development mode's own "Google", whose key
//! is a test-only key ([`dev::DevIssuer`]); no request leaves the test.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use serde_json::json;
use venti_api_types::v1::{
    ACCOUNT_GOOGLE_PATH, ACCOUNT_PATH, ACCOUNT_TOKENS_PATH, AUTH_GOOGLE_ACCOUNT_PATH,
    AUTH_GOOGLE_PATH, AccountResponse, CreateAccessTokenRequest, CreateGoogleAccountRequest,
    DEV_GOOGLE_ID_TOKEN_PATH, DeleteAccountRequest, DevGoogleIdTokenResponse, ErrorCode,
    GoogleSignInRequest, GoogleSignUpResponse, HEALTH_PATH, HealthResponse, LinkGoogleRequest,
    Password, SignInResponse, pilot_path,
};
use venti_server::accounts::google::GoogleSignIn;
use venti_server::accounts::google::dev::{CLIENT_ID, DevIssuer};
use venti_server::accounts::{Registration, invite};
use venti_server::adapters::recorded_at;
use venti_server::api::{Api, Cors, Settings};
use venti_store::MemoryStore;

mod common;

use common::{Answer, PASSWORD, TestClock, WEB_APP, ask, golden, recorded, settings, sign_up};
use venti_sources::testing::assert_golden;

/// A server with an empty store that signs in with the development mode's
/// "Google".
fn google_server(settings: Settings, clock: &TestClock) -> (Router, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let google = GoogleSignIn::development().unwrap();
    let api = Api::new(store.clone(), Arc::new(clock.clone()), [], settings)
        .with_google(Arc::new(google));
    (api.router(&Cors::Origins(vec![WEB_APP.to_owned()])), store)
}

fn id_token(email: &str, name: Option<&str>) -> String {
    DevIssuer::new().id_token(email, name, recorded_at())
}

async fn google_sign_in(router: &Router, id_token: &str) -> Answer {
    ask(Method::POST, AUTH_GOOGLE_PATH)
        .json(&GoogleSignInRequest {
            id_token: id_token.to_owned(),
            device: Some("Pixel".to_owned()),
        })
        .send(router)
        .await
}

fn choose(signup: &str, handle: &str) -> CreateGoogleAccountRequest {
    CreateGoogleAccountRequest {
        signup: signup.to_owned(),
        handle: handle.to_owned(),
        display_name: "Anna Alpin".to_owned(),
        invite: None,
        device: Some("Pixel".to_owned()),
    }
}

async fn create(router: &Router, request: &CreateGoogleAccountRequest) -> Answer {
    ask(Method::POST, AUTH_GOOGLE_ACCOUNT_PATH)
        .json(request)
        .send(router)
        .await
}

#[tokio::test]
async fn a_first_google_sign_in_chooses_a_handle_and_the_next_signs_in() {
    let clock = TestClock::new();
    let (router, _) = google_server(settings(), &clock);
    let health: HealthResponse = ask(Method::GET, HEALTH_PATH)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(health.google_sign_in);

    let token = id_token("anna.alpin@example.com", Some("Anna Alpin"));
    let first: GoogleSignUpResponse = google_sign_in(&router, &token).await.ok(StatusCode::OK);
    assert!(first.needs_handle);
    assert_eq!(first.suggested_handle.as_deref(), Some("anna_alpin"));
    assert_eq!(first.name.as_deref(), Some("Anna Alpin"));
    assert!(
        !first.signup.contains("anna"),
        "the email is not in the clear"
    );

    let signed: SignInResponse = create(&router, &choose(&first.signup, "anna"))
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(signed.account.handle.as_str(), "anna");
    assert_eq!(
        signed.account.google_email.as_deref(),
        Some("anna.alpin@example.com")
    );
    assert_eq!(signed.session.device.as_deref(), Some("Pixel"));

    // The account is the pilot's: the email in their own answer, and in
    // no answer that others read.
    let own: AccountResponse = ask(Method::GET, ACCOUNT_PATH)
        .token(&signed.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(
        own.account.google_email.as_deref(),
        Some("anna.alpin@example.com")
    );
    let page = ask(Method::GET, &pilot_path(&signed.account.handle))
        .send(&router)
        .await;
    assert!(!String::from_utf8_lossy(&page.body).contains("example.com"));

    // The next sign-in with the same Google account signs in.
    let again: SignInResponse = google_sign_in(&router, &token)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(again.account.handle.as_str(), "anna");
    assert_ne!(again.token, signed.token);

    // The same sign-up cannot make a second account, and the suggestion
    // for another who would be "anna" is free.
    create(&router, &choose(&first.signup, "anna_two"))
        .await
        .error(StatusCode::CONFLICT, ErrorCode::GoogleTaken);
    let other: GoogleSignUpResponse = google_sign_in(&router, &id_token("anna@example.org", None))
        .await
        .ok(StatusCode::OK);
    assert_eq!(other.suggested_handle.as_deref(), Some("anna2"));
    assert_eq!(other.name, None);

    // A handle is checked as for a password account.
    create(&router, &choose(&other.signup, "an"))
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidHandle);
    create(&router, &choose(&other.signup, "admin"))
        .await
        .error(StatusCode::CONFLICT, ErrorCode::HandleTaken);
    create(&router, &choose(&other.signup, "anna"))
        .await
        .error(StatusCode::CONFLICT, ErrorCode::HandleTaken);

    // A sign-up holds for a quarter of an hour.
    clock.advance(chrono::Duration::minutes(16));
    create(&router, &choose(&other.signup, "anna_b"))
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidSignup);
    create(&router, &choose("venti_signup_nonsense", "anna_b"))
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidSignup);
}

#[tokio::test]
async fn a_token_that_is_not_good_signs_nobody_in() {
    let (router, _) = google_server(settings(), &TestClock::new());
    let issuer = DevIssuer::new();
    let claims = |aud: &str, verified: bool| {
        json!({
            "iss": "https://accounts.google.com",
            "aud": aud,
            "sub": "1234",
            "email": "x@example.com",
            "email_verified": verified,
            "exp": recorded_at().timestamp() + 600,
        })
    };
    for token in [
        issuer.sign(&claims("another-app.apps.googleusercontent.com", true)),
        issuer.sign(&claims(CLIENT_ID, false)),
        "not a token".to_owned(),
    ] {
        google_sign_in(&router, &token)
            .await
            .error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidGoogleToken);
    }
    // Expired a minute and more ago.
    let mut old = claims(CLIENT_ID, true);
    old["exp"] = json!(recorded_at().timestamp() - 61);
    google_sign_in(&router, &issuer.sign(&old))
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::InvalidGoogleToken);
}

#[tokio::test]
async fn the_registration_setting_applies_as_to_a_password_account() {
    let clock = TestClock::new();
    let closed = Settings {
        registration: Registration::Closed,
        ..settings()
    };
    let (router, _) = google_server(closed, &clock);
    let first: GoogleSignUpResponse = google_sign_in(&router, &id_token("c@example.com", None))
        .await
        .ok(StatusCode::OK);
    create(&router, &choose(&first.signup, "carla"))
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::RegistrationClosed);

    let by_invite = Settings {
        registration: Registration::Invite,
        ..settings()
    };
    let (router, store) = google_server(by_invite, &clock);
    let codes = invite::make(&*store, &clock, 1, None).await.unwrap();
    let first: GoogleSignUpResponse = google_sign_in(&router, &id_token("c@example.com", None))
        .await
        .ok(StatusCode::OK);
    create(&router, &choose(&first.signup, "carla"))
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidInvite);
    let with_code = CreateGoogleAccountRequest {
        invite: Some(codes[0].to_string()),
        ..choose(&first.signup, "carla")
    };
    let _: SignInResponse = create(&router, &with_code).await.ok(StatusCode::CREATED);
    // Used up.
    let second: GoogleSignUpResponse = google_sign_in(&router, &id_token("d@example.com", None))
        .await
        .ok(StatusCode::OK);
    let reused = CreateGoogleAccountRequest {
        invite: Some(codes[0].to_string()),
        ..choose(&second.signup, "dora")
    };
    create(&router, &reused)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidInvite);
}

#[tokio::test]
async fn a_password_account_links_and_unlinks_google() {
    let (router, _) = google_server(settings(), &TestClock::new());
    let dario = sign_up(&router, "dario").await;
    let token = id_token("dario@example.com", Some("Dario"));

    let linked: AccountResponse = ask(Method::POST, ACCOUNT_GOOGLE_PATH)
        .token(&dario.token)
        .json(&LinkGoogleRequest {
            id_token: token.clone(),
        })
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(
        linked.account.google_email.as_deref(),
        Some("dario@example.com")
    );
    // Google signs in to it now.
    let signed: SignInResponse = google_sign_in(&router, &token)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(signed.account.handle.as_str(), "dario");

    // Another account cannot have the same Google account.
    let other = sign_up(&router, "bernd").await;
    ask(Method::POST, ACCOUNT_GOOGLE_PATH)
        .token(&other.token)
        .json(&LinkGoogleRequest {
            id_token: token.clone(),
        })
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::GoogleTaken);

    // Unlinked, Google signs in to it no more; the password still does.
    let unlinked = ask(Method::DELETE, ACCOUNT_GOOGLE_PATH)
        .token(&dario.token)
        .send(&router)
        .await;
    assert_eq!(unlinked.status, StatusCode::NO_CONTENT);
    ask(Method::DELETE, ACCOUNT_GOOGLE_PATH)
        .token(&dario.token)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GoogleNotLinked);
    let first: GoogleSignUpResponse = google_sign_in(&router, &token).await.ok(StatusCode::OK);
    assert!(first.needs_handle);
    let own: AccountResponse = ask(Method::GET, ACCOUNT_PATH)
        .token(&dario.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(own.account.google_email, None);
}

#[tokio::test]
async fn an_account_made_with_google_confirms_with_google_and_keeps_it() {
    let (router, _) = google_server(settings(), &TestClock::new());
    let token = id_token("erik@example.com", None);
    let first: GoogleSignUpResponse = google_sign_in(&router, &token).await.ok(StatusCode::OK);
    let erik: SignInResponse = create(&router, &choose(&first.signup, "erik"))
        .await
        .ok(StatusCode::CREATED);

    // Google is its only way in: it cannot be unlinked.
    ask(Method::DELETE, ACCOUNT_GOOGLE_PATH)
        .token(&erik.token)
        .send(&router)
        .await
        .error(StatusCode::CONFLICT, ErrorCode::GoogleOnlySignIn);

    // A token for a program is made with a fresh ID token in place of the
    // password; another Google account's is refused.
    let request = |google: &str| CreateAccessTokenRequest {
        name: "Logger".to_owned(),
        scopes: vec!["flights:read".to_owned()],
        expires_at: None,
        password: Password::default(),
        google_id_token: Some(google.to_owned()),
    };
    ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(&erik.token)
        .json(&request(&id_token("someone@example.com", None)))
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGoogleToken);
    let made = ask(Method::POST, ACCOUNT_TOKENS_PATH)
        .token(&erik.token)
        .json(&request(&token))
        .send(&router)
        .await;
    assert_eq!(made.status, StatusCode::CREATED);
    // Without either, nothing: an account made with Google has no
    // password to be right.
    ask(Method::DELETE, ACCOUNT_PATH)
        .token(&erik.token)
        .json(&DeleteAccountRequest {
            password: Password::new(PASSWORD),
            google_id_token: None,
        })
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::WrongPassword);
    let deleted = ask(Method::DELETE, ACCOUNT_PATH)
        .token(&erik.token)
        .json(&DeleteAccountRequest {
            password: Password::default(),
            google_id_token: Some(token.clone()),
        })
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    // The Google account is free again.
    let after: GoogleSignUpResponse = google_sign_in(&router, &token).await.ok(StatusCode::OK);
    assert!(after.needs_handle);
}

#[tokio::test]
async fn a_server_without_google_says_so() {
    let (router, _) = common::server(settings(), &TestClock::new());
    let health: HealthResponse = ask(Method::GET, HEALTH_PATH)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(!health.google_sign_in);
    let raw = ask(Method::GET, HEALTH_PATH).send(&router).await;
    assert!(String::from_utf8_lossy(&raw.body).contains(r#""google_sign_in":false"#));
    google_sign_in(&router, &id_token("x@example.com", None))
        .await
        .error(StatusCode::NOT_IMPLEMENTED, ErrorCode::GoogleNotConfigured);
    create(&router, &choose("venti_signup_x", "xaver"))
        .await
        .error(StatusCode::NOT_IMPLEMENTED, ErrorCode::GoogleNotConfigured);
    // And it hands out no made-up tokens.
    ask(
        Method::GET,
        &format!("{DEV_GOOGLE_ID_TOKEN_PATH}?email=x@example.com"),
    )
    .send(&router)
    .await
    .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
}

#[tokio::test]
async fn the_development_mode_hands_out_tokens_of_its_own_google() {
    let router = recorded().await;
    let health: HealthResponse = ask(Method::GET, HEALTH_PATH)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(health.google_sign_in);
    let made: DevGoogleIdTokenResponse = ask(
        Method::GET,
        &format!("{DEV_GOOGLE_ID_TOKEN_PATH}?email=ui.test@example.com&name=UI%20Test"),
    )
    .send(&router)
    .await
    .ok(StatusCode::OK);
    let first: GoogleSignUpResponse = google_sign_in(&router, &made.id_token)
        .await
        .ok(StatusCode::OK);
    assert_eq!(first.suggested_handle.as_deref(), Some("ui_test"));
    assert_eq!(first.name.as_deref(), Some("UI Test"));
    // The answers, with what differs from start to start replaced. The
    // token is the same on every start: the development mode's clock
    // stands still, and its key is fixed.
    assert_golden(&golden("dev_google_id_token.json"), &made);
    let example = GoogleSignUpResponse {
        signup: "venti_signup_kZ3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0".to_owned(),
        ..first.clone()
    };
    assert_golden(&golden("google_sign_up.json"), &example);
    let _: SignInResponse = create(&router, &choose(&first.signup, "ui_test"))
        .await
        .ok(StatusCode::CREATED);
    // The same email is the same made-up Google account.
    let again: DevGoogleIdTokenResponse = ask(
        Method::GET,
        &format!("{DEV_GOOGLE_ID_TOKEN_PATH}?email=ui.test@example.com"),
    )
    .send(&router)
    .await
    .ok(StatusCode::OK);
    let signed: SignInResponse = google_sign_in(&router, &again.id_token)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(signed.account.handle.as_str(), "ui_test");
    ask(Method::GET, DEV_GOOGLE_ID_TOKEN_PATH)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidGoogleToken);
}
