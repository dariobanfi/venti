//! A pilot's picture and the pilots near a place, through the real router:
//! what a picture becomes, where it shows, Google's taken once, a report
//! about one, and who is among the pilots near a place and who is not.

use std::io::Cursor;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use venti_api_types::v1::{
    ACCOUNT_PATH, ACCOUNT_PICTURE_PATH, AUTH_GOOGLE_ACCOUNT_PATH, AUTH_GOOGLE_PATH,
    AccountResponse, CreateGoogleAccountRequest, ErrorCode, GoogleSignInRequest,
    GoogleSignUpResponse, PICTURE_SIDE, PilotResponse, PilotsNearResponse, REPORTS_PATH,
    ReportReason, ReportRequest, ReportSubject, SignInResponse, UpdateAccountRequest, Visibility,
    pilot_path,
};
use venti_server::accounts::google::GoogleSignIn;
use venti_server::accounts::google::dev::DevIssuer;
use venti_server::accounts::picture::made_up_png;
use venti_server::adapters::recorded_at;
use venti_server::api::{Api, Cors};
use venti_server::dev::{DEMO_ACCESS_TOKEN, DEMO_PICTURE_ID, DEMO_TOKEN};
use venti_sources::testing::assert_golden;
use venti_store::MemoryStore;

mod common;

use common::{
    TestClock, WEB_APP, ask, get, golden, recorded, sample_file, server, set_visibility, settings,
    sign_up, upload,
};

async fn send_picture(router: &Router, token: &str, bytes: Vec<u8>, media: &str) -> common::Answer {
    ask(Method::PUT, ACCOUNT_PICTURE_PATH)
        .token(token)
        .header(header::CONTENT_TYPE, media)
        .bytes(bytes)
        .send(router)
        .await
}

fn decoded_side(webp: &[u8]) -> (u32, u32) {
    image_webp::WebPDecoder::new(Cursor::new(webp))
        .unwrap()
        .dimensions()
}

#[tokio::test]
async fn a_picture_sent_becomes_the_servers_own_square_and_shows_where_the_pilot_does() {
    let (router, _) = server(settings(), &TestClock::new());
    let anna = sign_up(&router, "anna").await;
    assert_eq!(anna.account.picture, None);
    assert!(anna.account.shown_near_places);

    let photo = made_up_png(800, 600, 7);
    let account: AccountResponse = send_picture(&router, &anna.token, photo, "image/png")
        .await
        .ok(StatusCode::OK);
    let path = account.account.picture.clone().expect("a picture now");
    assert!(path.starts_with("/v1/pictures/"));

    let picture = get(&router, &path).await;
    assert_eq!(picture.status, StatusCode::OK);
    assert_eq!(picture.header(header::CONTENT_TYPE), "image/webp");
    assert_eq!(
        picture.header(header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(decoded_side(&picture.body), (PICTURE_SIDE, PICTURE_SIDE));
    // Asked again with its tag, nothing is sent.
    let tag = picture.header(header::ETAG).to_owned();
    let again = ask(Method::GET, &path)
        .header(header::IF_NONE_MATCH, &tag)
        .send(&router)
        .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);

    // On the pilot's page and their flights, once they have one.
    let flight = upload(&router, &anna.token, &sample_file()).await.flight;
    assert_eq!(flight.pilot.picture.as_deref(), Some(path.as_str()));
    let page: PilotResponse = get(&router, &pilot_path(&"anna".parse().unwrap()))
        .await
        .ok(StatusCode::OK);
    assert_eq!(page.pilot.picture.as_deref(), Some(path.as_str()));
    assert_eq!(
        page.flights[0].pilot.picture.as_deref(),
        Some(path.as_str())
    );

    // Another takes its place, and the old address is gone.
    let account: AccountResponse =
        send_picture(&router, &anna.token, made_up_png(64, 64, 1), "image/png")
            .await
            .ok(StatusCode::OK);
    let newer = account.account.picture.unwrap();
    assert_ne!(newer, path);
    get(&router, &path)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PictureNotFound);

    // Removed, the account and the page show none.
    let account: AccountResponse = ask(Method::DELETE, ACCOUNT_PICTURE_PATH)
        .token(&anna.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(account.account.picture, None);
    get(&router, &newer)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PictureNotFound);
    let page: PilotResponse = get(&router, &pilot_path(&"anna".parse().unwrap()))
        .await
        .ok(StatusCode::OK);
    assert_eq!(page.pilot.picture, None);
}

#[tokio::test]
async fn what_is_no_picture_the_server_takes_is_refused() {
    let (router, _) = server(settings(), &TestClock::new());
    let anna = sign_up(&router, "anna").await;
    let png = made_up_png(64, 64, 1);
    send_picture(&router, &anna.token, png.clone(), "image/gif")
        .await
        .error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ErrorCode::UnsupportedMediaType,
        );
    send_picture(
        &router,
        &anna.token,
        b"GIF89a, not a picture".to_vec(),
        "image/jpeg",
    )
    .await
    .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPicture);
    send_picture(&router, &anna.token, made_up_png(20, 20, 1), "image/png")
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidPicture);
    let too_large = vec![0_u8; venti_api_types::v1::MAX_PICTURE_BYTES + 1];
    send_picture(&router, &anna.token, too_large, "image/png")
        .await
        .error(StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge);
    // Nobody signed in sends none, and nor does a program.
    ask(Method::PUT, ACCOUNT_PICTURE_PATH)
        .header(header::CONTENT_TYPE, "image/png")
        .bytes(png.clone())
        .send(&router)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    let recorded = recorded().await;
    send_picture(&recorded, DEMO_ACCESS_TOKEN, png, "image/png")
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::SessionRequired);
    // A picture address that is no identifier is no picture.
    get(&router, "/v1/pictures/nothing")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PictureNotFound);
}

fn google_server(clock: &TestClock) -> Router {
    let store = std::sync::Arc::new(MemoryStore::new());
    let google = GoogleSignIn::development().unwrap();
    Api::new(store, std::sync::Arc::new(clock.clone()), [], settings())
        .with_google(std::sync::Arc::new(google))
        .router(&Cors::Origins(vec![WEB_APP.to_owned()]))
}

async fn google_sign_in(router: &Router) -> common::Answer {
    let id_token = DevIssuer::new().id_token_with_picture(
        "anna@example.com",
        Some("Anna Alpin"),
        recorded_at(),
    );
    ask(Method::POST, AUTH_GOOGLE_PATH)
        .json(&GoogleSignInRequest {
            id_token,
            device: None,
        })
        .send(router)
        .await
}

#[tokio::test]
async fn googles_picture_is_taken_once_at_the_first_sign_in() {
    let clock = TestClock::new();
    let router = google_server(&clock);
    let signup: GoogleSignUpResponse = google_sign_in(&router).await.ok(StatusCode::OK);
    let made: SignInResponse = ask(Method::POST, AUTH_GOOGLE_ACCOUNT_PATH)
        .json(&CreateGoogleAccountRequest {
            signup: signup.signup,
            handle: "anna".to_owned(),
            display_name: "Anna Alpin".to_owned(),
            invite: None,
            device: None,
        })
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    let path = made
        .account
        .picture
        .expect("Google's picture, as the server's own");
    let picture = get(&router, &path).await;
    assert_eq!(decoded_side(&picture.body), (PICTURE_SIDE, PICTURE_SIDE));

    // The pilot removes it; the next sign-in does not bring it back.
    let removed: AccountResponse = ask(Method::DELETE, ACCOUNT_PICTURE_PATH)
        .token(&made.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(removed.account.picture, None);
    let again: SignInResponse = google_sign_in(&router).await.ok(StatusCode::CREATED);
    assert_eq!(again.account.picture, None);
}

#[test]
fn only_googles_own_picture_addresses_are_fetched() {
    use venti_server::accounts::google::picture_address;
    assert_eq!(
        picture_address("https://lh3.googleusercontent.com/a/ACg8ocK=s96-c").as_deref(),
        Some("https://lh3.googleusercontent.com/a/ACg8ocK=s512-c")
    );
    assert_eq!(
        picture_address("https://lh3.googleusercontent.com/a/plain").as_deref(),
        Some("https://lh3.googleusercontent.com/a/plain")
    );
    for refused in [
        "http://lh3.googleusercontent.com/a/x",
        "https://example.com/a.png",
        "https://googleusercontent.com.example.com/a",
        "https://user@lh3.googleusercontent.com/a",
        "https://lh3.googleusercontent.com:8443/a",
        "file:///etc/passwd",
    ] {
        assert_eq!(picture_address(refused), None, "{refused}");
    }
}

#[tokio::test]
async fn a_picture_is_reported_like_a_pilots_page() {
    let router = recorded().await;
    let report = ReportRequest {
        subject: ReportSubject::Picture("mara".to_owned()),
        reason: ReportReason::Offensive,
        text: Some("Not a picture to show.".to_owned()),
    };
    assert_golden(&golden("report_picture_request.json"), &report);
    let answer: venti_api_types::v1::ReportResponse = ask(Method::POST, REPORTS_PATH)
        .token(DEMO_TOKEN)
        .json(&report)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    assert!(answer.created);
    // A pilot without a picture has none to report.
    let nothing = ReportRequest {
        subject: ReportSubject::Picture("jonas".to_owned()),
        ..report.clone()
    };
    ask(Method::POST, REPORTS_PATH)
        .token(DEMO_TOKEN)
        .json(&nothing)
        .send(&router)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PictureNotFound);
    // Not a flight: the reason about a flight is not one for a picture.
    let wrong = ReportRequest {
        reason: ReportReason::NotAFlight,
        ..report
    };
    ask(Method::POST, REPORTS_PATH)
        .token(DEMO_TOKEN)
        .json(&wrong)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
}

#[tokio::test]
async fn the_pilots_near_a_place_are_those_of_its_public_flights() {
    let router = recorded().await;
    let answer = get(&router, "/v1/pilots/near?lat=47.3&lon=10.9&limit=3").await;
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    let near: PilotsNearResponse = answer.ok(StatusCode::OK);
    assert_golden(&golden("pilots_near.json"), &near);
    assert_eq!((near.radius_km, near.months), (25, 12));
    assert!(near.pilots.iter().any(|near| !near.takeoffs.is_empty()));
    // Where Mara flies, her picture is named.
    let brauneck: PilotsNearResponse = get(&router, "/v1/pilots/near?lat=47.66&lon=11.8")
        .await
        .ok(StatusCode::OK);
    let mara = brauneck
        .pilots
        .iter()
        .find(|near| near.pilot.handle.as_str() == "mara")
        .expect("Mara flies there");
    assert_eq!(
        mara.pilot.picture.as_deref(),
        Some(format!("/v1/pictures/{DEMO_PICTURE_ID}").as_str())
    );
    // Far away, nobody.
    let far: PilotsNearResponse = get(&router, "/v1/pilots/near?lat=45.0&lon=6.0")
        .await
        .ok(StatusCode::OK);
    assert!(far.pilots.is_empty());
    for bad in [
        "/v1/pilots/near?lat=47.66",
        "/v1/pilots/near?lat=147&lon=11.8",
        "/v1/pilots/near?lat=47.66&lon=11.8&radius_km=0",
        "/v1/pilots/near?lat=47.66&lon=11.8&radius_km=101",
        "/v1/pilots/near?lat=47.66&lon=11.8&limit=51",
    ] {
        assert_eq!(
            get(&router, bad).await.status,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn a_pilot_who_switches_it_off_is_not_among_the_pilots_near_a_place() {
    let (router, _) = server(settings(), &TestClock::new());
    let anna = sign_up(&router, "anna").await;
    let flight = upload(&router, &anna.token, &sample_file()).await.flight;
    set_visibility(&router, &anna.token, &flight.id, Visibility::Public).await;
    let off: AccountResponse = ask(Method::PATCH, ACCOUNT_PATH)
        .token(&anna.token)
        .json(&UpdateAccountRequest {
            shown_near_places: Some(false),
            ..UpdateAccountRequest::default()
        })
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(!off.account.shown_near_places);
    // The setting is the account's alone: whether new flights are public
    // stays as it was.
    assert!(off.account.new_flights_public);
    let near: PilotsNearResponse = get(&router, "/v1/pilots/near?lat=47.66&lon=11.8")
        .await
        .ok(StatusCode::OK);
    assert!(near.pilots.is_empty());

    // Switched on again, an answer not made before shows her (an answer is
    // kept for ten minutes).
    let _: AccountResponse = ask(Method::PATCH, ACCOUNT_PATH)
        .token(&anna.token)
        .json(&UpdateAccountRequest {
            shown_near_places: Some(true),
            ..UpdateAccountRequest::default()
        })
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let near: PilotsNearResponse = get(&router, "/v1/pilots/near?lat=47.66&lon=11.8&radius_km=30")
        .await
        .ok(StatusCode::OK);
    let handles: Vec<&str> = near
        .pilots
        .iter()
        .map(|near| near.pilot.handle.as_str())
        .collect();
    assert_eq!(handles, ["anna"]);
    assert_eq!(near.pilots[0].flights, 1);
    // The flight is of 2026, the year of the test's clock.
    assert_eq!(near.pilots[0].season.as_ref().unwrap().totals.flights, 1);
}
