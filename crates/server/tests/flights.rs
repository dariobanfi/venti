//! Tests of a pilot's flights through the real router: uploading, listing,
//! what changed, who may see what, the file itself, and what a hostile
//! client can and cannot do.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use futures::StreamExt;
use venti_api_types::v1::{
    ErrorCode, FLIGHT_CHANGES_PATH, FLIGHTS_PATH, Flight, FlightChangesRequest,
    FlightChangesResponse, FlightId, FlightResponse, FlightsRequest, FlightsResponse, Handle,
    MAX_FLIGHT_FILE_BYTES, PilotRequest, PilotResponse, RouteKind, UpdateFlightRequest,
    UploadResponse, Visibility, flight_file_path, flight_path, pilot_path, share_image_path,
    share_path,
};
use venti_server::accounts::Limits;
use venti_server::api::Settings;
use venti_server::dev::DEMO_TOKEN;
use venti_server::flights::analyser::{Analyser, Where};
use venti_sources::testing::assert_golden;
use venti_store::Room;
use venti_store::testing::TroubledStore;

mod common;

#[cfg(unix)]
use common::stalling_analyser;
use common::{
    Answer, TestClock, ask, get, get_with, golden, recorded, sample_file, sample_file_on, server,
    set_visibility, settings, sign_up, upload,
};

// --- Helpers ----------------------------------------------------------------

fn open_server() -> Router {
    server(settings(), &TestClock::new()).0
}

async fn flight_as(router: &Router, token: &str, id: &FlightId) -> Answer {
    ask(Method::GET, &flight_path(id))
        .token(token)
        .send(router)
        .await
}

async fn my_flights(router: &Router, token: &str, request: &FlightsRequest) -> FlightsResponse {
    ask(Method::GET, &request.path_and_query())
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK)
}

async fn changes(router: &Router, token: &str, since: Option<&str>) -> FlightChangesResponse {
    let request = FlightChangesRequest {
        since: since.map(str::to_owned),
        limit: None,
    };
    ask(Method::GET, &request.path_and_query())
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK)
}

fn ids(flights: &[Flight]) -> Vec<&FlightId> {
    flights.iter().map(|flight| &flight.id).collect()
}

/// A flight file with a mark the stalling analyser knows: a few thousand
/// fixes that jump from one side of the globe to the other.
fn hostile_file(fixes: usize) -> Vec<u8> {
    let mut text = String::from("AXVE000\r\nHFDTEDATE:040726,01\r\n");
    for n in 0..fixes {
        let fade = 80.0 * (1.0 - n as f64 / fixes as f64);
        let (degrees, minutes) = (fade.trunc(), fade.fract() * 60_000.0);
        let hemisphere = if n % 2 == 0 { 'N' } else { 'S' };
        let east = 170.0 * n as f64 / fixes as f64;
        text.push_str(&format!(
            "B{:02}{:02}{:02}{:02.0}{:05.0}{hemisphere}{:03.0}{:05.0}EA0100001000\r\n",
            10 + n / 3600,
            n / 60 % 60,
            n % 60,
            degrees,
            minutes.min(59_999.0),
            east.trunc(),
            (east.fract() * 60_000.0).min(59_999.0),
        ));
    }
    text.into_bytes()
}

// --- Uploading ----------------------------------------------------------------

#[tokio::test]
async fn an_uploaded_flight_is_kept_private_with_what_the_phone_shows_for_it() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;

    let answer = ask(Method::POST, FLIGHTS_PATH)
        .token(&dario.token)
        .bytes(sample_file())
        .send(&router)
        .await;

    let uploaded: UploadResponse = answer.ok(StatusCode::CREATED);
    answer.is_private();
    assert!(uploaded.created);
    let flight = uploaded.flight;
    assert_eq!(flight.visibility, Visibility::Private);
    assert_eq!(flight.pilot.handle.as_str(), "dario");
    assert_eq!(flight.pilot.display_name, "Pilot dario");
    // The identity the apps give the bundled sample flight, and the
    // numbers the core's own tests expect of it.
    assert_eq!(flight.identity, "6c55247b80a34b03fa0bb3bd38173c59");
    assert_eq!(flight.airtime_s, 1744);
    assert_eq!(flight.max_altitude_m, 2117);
    assert_eq!(flight.glider.as_deref(), Some("Sample Wing"));
    assert_eq!(flight.file_bytes as usize, sample_file().len());
    assert_eq!(flight.score.unwrap().kind, RouteKind::FreeFlight);
    assert!(flight.takeoff.is_some());
    assert!(flight.outline.len() > 3);

    // It can be read back, by its pilot.
    let read: FlightResponse = flight_as(&router, &dario.token, &flight.id)
        .await
        .ok(StatusCode::OK);
    assert_eq!(read.flight, flight);
}

#[tokio::test]
async fn uploading_a_flight_again_is_a_success_that_says_so() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    let first = upload(&router, &dario.token, &sample_file()).await;
    set_visibility(
        &router,
        &dario.token,
        &first.flight.id,
        Visibility::Unlisted,
    )
    .await;

    // The same file, and the same recording with other headers and line
    // endings, as another app would export it.
    let reworded = String::from_utf8(sample_file())
        .unwrap()
        .replace("Sample Pilot", "S. Pilot")
        .replace("\r\n", "\n");
    for file in [sample_file(), reworded.into_bytes()] {
        let answer = ask(Method::POST, FLIGHTS_PATH)
            .token(&dario.token)
            .bytes(file)
            .send(&router)
            .await;
        let again: UploadResponse = answer.ok(StatusCode::OK);
        assert!(!again.created);
        assert_eq!(again.flight.id, first.flight.id);
        // What the pilot set stays set, and the first file stays the file.
        assert_eq!(again.flight.visibility, Visibility::Unlisted);
        assert_eq!(again.flight.file_bytes, first.flight.file_bytes);
    }
    let listed = my_flights(&router, &dario.token, &FlightsRequest::default()).await;
    assert_eq!(listed.flights.len(), 1);

    // Another pilot's upload of the same recording is another flight, with
    // an address of its own.
    let annas = upload(&router, &anna.token, &sample_file()).await;
    assert!(annas.created);
    assert_eq!(annas.flight.identity, first.flight.identity);
    assert_ne!(annas.flight.id, first.flight.id);
    assert_eq!(annas.flight.visibility, Visibility::Private);
}

#[tokio::test]
async fn the_file_comes_back_byte_for_byte() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    // A file as untidy as recorders write them: a name in Latin-1, lines
    // the server does not read, a signature, mixed line endings, no line
    // ending at the end.
    let mut file = sample_file();
    let extra = b"LXVEcomment \xe4\xf6\xfc\nGA1B2C3D4E5F6\r\nGSECONDLINE";
    file.extend_from_slice(extra);
    assert!(std::str::from_utf8(&file).is_err());

    let flight = upload(&router, &dario.token, &file).await.flight;
    let answer = ask(Method::GET, &flight_file_path(&flight.id))
        .token(&dario.token)
        .send(&router)
        .await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body, file);
    answer.is_private();
    assert_eq!(
        answer.header(header::CONTENT_TYPE),
        "application/octet-stream"
    );
    assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    // The name of the download is made of the date and the address.
    assert_eq!(
        answer.header(header::CONTENT_DISPOSITION),
        format!(
            "attachment; filename=\"venti-2026-07-04-{}.igc\"",
            &flight.id.as_str()[..8]
        )
    );
    assert_eq!(flight.file_bytes as usize, file.len());
    // The same flight as the tidy file: the fixes are the same.
    assert_eq!(flight.identity, "6c55247b80a34b03fa0bb3bd38173c59");

    // Compressed on the way when the client can take that; still the same
    // bytes once unpacked.
    let packed = ask(Method::GET, &flight_file_path(&flight.id))
        .token(&dario.token)
        .header(header::ACCEPT_ENCODING, "gzip")
        .send(&router)
        .await;
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");
    assert!(packed.body.len() * 3 < file.len());
    assert_eq!(
        venti_server::flights::unpack(&packed.body).as_deref(),
        Some(&file[..])
    );
}

#[tokio::test]
async fn what_is_not_a_usable_flight_file_is_refused_and_nothing_is_kept() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let refuse = |file: Vec<u8>| {
        let (router, token) = (router.clone(), dario.token.clone());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .bytes(file)
                .send(&router)
                .await
                .error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::InvalidFlight)
        }
    };

    assert!(refuse(Vec::new()).await.contains("no body"));
    assert!(refuse(b"hello".to_vec()).await.contains("no date header"));
    assert!(
        refuse(b"\x89PNG\r\n\x1a\n\0\0\0\0".to_vec())
            .await
            .contains("no date")
    );
    assert!(refuse(vec![0xff; 100_000]).await.contains("no date"));
    assert!(
        refuse(b"AXVE000\r\nHFDTEDATE:040726,01\r\n".to_vec())
            .await
            .contains("no readable position")
    );
    // JSON is not a flight file either.
    refuse(br#"{"visibility":"public"}"#.to_vec()).await;
    // Nor is anything at all with a date and one position in front of it:
    // the server is no place to keep files and hand them out.
    let mut smuggled = b"HFDTEDATE:040726\r\nB1030004739852N01147802EA0162001620\r\n".to_vec();
    smuggled.extend((0..3_000_000).map(|n| (n * 131 % 256) as u8));
    assert!(
        refuse(smuggled)
            .await
            .contains("most of it is not positions")
    );

    let listed = my_flights(&router, &dario.token, &FlightsRequest::default()).await;
    assert!(listed.flights.is_empty());

    // Without an account there is nowhere to keep a flight.
    let answer = ask(Method::POST, FLIGHTS_PATH)
        .bytes(sample_file())
        .send(&router)
        .await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
}

#[tokio::test]
async fn a_file_larger_than_any_flight_is_not_read() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    // Up to the limit a body is taken, and judged by what it holds: six
    // hours of positions with everything a recorder may add to each, and
    // empty lines to the last byte.
    let mut large = String::from("AXVE000\r\nHFDTEDATE:040726,01\r\n");
    for n in 0..22_000 {
        let (hour, minute, second) = (n / 3600, n / 60 % 60, n % 60);
        let more = "0".repeat(65);
        large.push_str(&format!(
            "B{hour:02}{minute:02}{second:02}4739852N01147802EA0162001620{more}\r\n"
        ));
    }
    let mut large = large.into_bytes();
    assert!(large.len() > MAX_FLIGHT_FILE_BYTES / 2 && large.len() < MAX_FLIGHT_FILE_BYTES);
    large.resize(MAX_FLIGHT_FILE_BYTES, b'\n');
    let at_limit = upload(&router, &dario.token, &large).await;
    assert!(at_limit.created);
    assert_eq!(at_limit.flight.file_bytes as usize, MAX_FLIGHT_FILE_BYTES);

    large.push(b'\n');
    let answer = ask(Method::POST, FLIGHTS_PATH)
        .token(&dario.token)
        .bytes(large)
        .send(&router)
        .await;
    answer.error(StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge);

    // Everywhere else even a small file is too much to send.
    let answer = ask(Method::PATCH, &flight_path(&at_limit.flight.id))
        .token(&dario.token)
        .bytes(vec![b' '; 64 * 1024])
        .send(&router)
        .await;
    answer.error(StatusCode::PAYLOAD_TOO_LARGE, ErrorCode::PayloadTooLarge);
}

#[cfg(unix)]
#[tokio::test]
async fn a_file_that_keeps_the_analysis_busy_is_given_up_on() {
    // As on a real server, each file is analysed by a program of its own,
    // which is killed when it takes too long: two seconds here, ten there.
    let clock = TestClock::new();
    let patient = StdDuration::from_secs(2);
    let folder = std::env::temp_dir().join(format!("venti-stalling-{}", std::process::id()));
    let program = stalling_analyser(&folder);
    let settings = Settings {
        analyser: Analyser::with_time_allowed(Where::Child(program), patient),
        limits: Limits {
            uploads_per_day: 100,
            slow_upload_weight: 50,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, _) = server(settings, &clock);
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    let send = |token: &str, file: Vec<u8>| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .bytes(file)
                .send(&router)
                .await
        }
    };

    // A file over which the analysis never finishes.
    let began = Instant::now();
    let answer = send(&dario.token, hostile_file(6000)).await;
    let took = began.elapsed();

    let message = answer.error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::InvalidFlight);
    assert!(message.contains("could not be analysed"), "{message}");
    assert!(took >= patient, "{took:?}");
    assert!(took < patient + StdDuration::from_secs(3), "{took:?}");
    // The file is gone from the server's hands: others are served.
    assert!(upload(&router, &anna.token, &sample_file()).await.created);

    // Such a file counts for fifty of the hundred this account may upload
    // today: after the second, the day is used up, also for honest files.
    let answer = send(&dario.token, hostile_file(6001)).await;
    answer.error(StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::InvalidFlight);
    let answer = send(&dario.token, sample_file()).await;
    let message = answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("too many files today"), "{message}");
    assert!(answer.header(header::RETRY_AFTER).parse::<u64>().unwrap() > 86_000);
    // Nobody else's are, and nothing is left running that would keep
    // their files waiting.
    let began = Instant::now();
    assert!(!upload(&router, &anna.token, &sample_file()).await.created);
    assert!(began.elapsed() < patient, "{:?}", began.elapsed());

    clock.advance(Duration::days(1));
    assert!(upload(&router, &dario.token, &sample_file()).await.created);
    let _ = std::fs::remove_dir_all(&folder);
}

/// The start of a flight file, and then a connection on which nothing
/// more arrives.
fn nine_bytes_and_then_nothing() -> Body {
    let first = futures::stream::once(async {
        Ok::<_, std::io::Error>(Bytes::from_static(b"AXVE000\r\n"))
    });
    Body::from_stream(first.chain(futures::stream::pending()))
}

/// A flight file that arrives a byte every five seconds and never ends.
fn a_byte_now_and_then() -> Body {
    Body::from_stream(futures::stream::unfold((), |()| async {
        tokio::time::sleep(StdDuration::from_secs(5)).await;
        Some((Ok::<_, std::io::Error>(Bytes::from_static(b"A")), ()))
    }))
}

// The clock of the runtime is the test's: ten seconds of waiting take none
// of the test's time.
#[tokio::test(start_paused = true)]
async fn uploads_that_stand_still_give_their_places_back() {
    let clock = TestClock::new();
    let settings = Settings {
        limits: Limits {
            uploads_per_day: 60,
            slow_upload_weight: 50,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, _) = server(settings, &clock);
    let attempt = |token: &str, body: Body| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .body(body)
                .send(&router)
                .await
        }
    };

    // Four accounts, two uploads each, which is every place the server
    // has: nine bytes, and then nothing.
    let mut tokens = Vec::new();
    let mut standing_still = Vec::new();
    for handle in ["anna", "bernd", "carla", "dora"] {
        let token = sign_up(&router, handle).await.token;
        for _ in 0..2 {
            let upload = attempt(&token, nine_bytes_and_then_nothing());
            standing_still.push(tokio::spawn(upload));
        }
        tokens.push(token);
    }
    tokio::time::sleep(StdDuration::from_secs(1)).await;

    // A fifth pilot finds no place at that moment...
    let dario = sign_up(&router, "dario").await;
    let answer = attempt(&dario.token, sample_file().into()).await;
    answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
    // ...and ten seconds later finds one, where it used to be two minutes.
    tokio::time::sleep(StdDuration::from_secs(10)).await;
    assert!(upload(&router, &dario.token, &sample_file()).await.created);

    // The eight were told what happened,
    for upload in standing_still {
        let answer = upload.await.unwrap();
        let message = answer.error(StatusCode::REQUEST_TIMEOUT, ErrorCode::Timeout);
        assert!(message.contains("did not arrive"), "{message}");
    }
    // and holding a place for nothing counts like a file that keeps the
    // analysis busy: after two, the account's day is used up.
    for token in &tokens {
        let answer = attempt(token, sample_file().into()).await;
        answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    }

    // A file that keeps arriving, a byte at a time, is not standing still.
    // It gets the time a file gets and no more, and counts the same.
    let emil = sign_up(&router, "emil").await;
    let began = tokio::time::Instant::now();
    let answer = attempt(&emil.token, a_byte_now_and_then()).await;
    answer.error(StatusCode::REQUEST_TIMEOUT, ErrorCode::Timeout);
    let took = began.elapsed();
    assert!(
        took >= StdDuration::from_secs(100) && took < StdDuration::from_secs(101),
        "{took:?}"
    );
    let answer = attempt(&emil.token, a_byte_now_and_then()).await;
    answer.error(StatusCode::REQUEST_TIMEOUT, ErrorCode::Timeout);
    let answer = attempt(&emil.token, sample_file().into()).await;
    answer.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
}

/// A stand-in for the analysis that takes two seconds over a file with the
/// word SLOWLY in it, and then analyses it like any other.
#[cfg(unix)]
fn dawdling_analyser(folder: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let real = env!("CARGO_BIN_EXE_venti-server");
    let script = folder.join("analyser.sh");
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfile=\"$0.$$\"\ncat > \"$file\"\n\
             if grep -q SLOWLY \"$file\"; then sleep 2; fi\n\
             \"{real}\" \"$@\" < \"$file\"\nstatus=$?\nrm -f \"$file\"\nexit $status\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(unix)]
#[tokio::test]
async fn the_time_an_analysis_takes_counts_against_the_account_also_when_it_ends_well() {
    let clock = TestClock::new();
    let folder = std::env::temp_dir().join(format!("venti-dawdling-{}", std::process::id()));
    let program = dawdling_analyser(&folder);
    // Ten seconds are allowed. The first is free, and the nine after it
    // would count for ninety uploads: two seconds count for ten, beside
    // the one every upload counts for. An account has eleven.
    let settings = Settings {
        analyser: Analyser::with_time_allowed(Where::Child(program), StdDuration::from_secs(10)),
        limits: Limits {
            uploads_per_day: 11,
            slow_upload_weight: 90,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, _) = server(settings, &clock);
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    let attempt = |token: &str, file: Vec<u8>| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .bytes(file)
                .send(&router)
                .await
                .status
        }
    };
    let mut slow = sample_file();
    slow.extend_from_slice(b"LXVESLOWLY\r\n");

    // The file is a flight and is kept. What it cost is remembered: the
    // account's day is used up by one upload and its two seconds.
    assert_eq!(attempt(&dario.token, slow).await, StatusCode::CREATED);
    assert_eq!(
        attempt(&dario.token, sample_file_on("050726")).await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // A file that is analysed in the fraction of a second a flight takes
    // counts for itself and nothing more.
    assert_eq!(
        attempt(&anna.token, sample_file()).await,
        StatusCode::CREATED
    );
    assert_eq!(
        attempt(&anna.token, sample_file_on("050726")).await,
        StatusCode::CREATED
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn an_account_sends_two_files_at_a_time_and_no_more() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    let send = |token: &str, file: Vec<u8>| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .bytes(file)
                .send(&router)
                .await
        }
    };

    // Four uploads at the same moment, three of them from one account.
    let (first, second, third, annas) = tokio::join!(
        send(&dario.token, sample_file_on("010726")),
        send(&dario.token, sample_file_on("020726")),
        send(&dario.token, sample_file_on("030726")),
        send(&anna.token, sample_file_on("010726")),
    );

    assert_eq!(first.status, StatusCode::CREATED);
    assert_eq!(second.status, StatusCode::CREATED);
    let message = third.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(message.contains("one after another"), "{message}");
    assert_eq!(third.header(header::RETRY_AFTER), "5");
    assert_eq!(annas.status, StatusCode::CREATED);
    // The third was turned away before it was read: once the others are
    // done it goes through, and it did not count against the day.
    let again = send(&dario.token, sample_file_on("030726")).await;
    assert_eq!(again.status, StatusCode::CREATED);
    let listed = my_flights(&router, &dario.token, &FlightsRequest::default()).await;
    assert_eq!(listed.flights.len(), 3);
}

#[tokio::test]
async fn an_account_uploads_a_limited_number_of_files_a_day() {
    let clock = TestClock::new();
    let settings = Settings {
        limits: Limits {
            uploads_per_day: 3,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, _) = server(settings, &clock);
    let dario = sign_up(&router, "dario").await;
    let attempt = |file: Vec<u8>| {
        let (router, token) = (router.clone(), dario.token.clone());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .bytes(file)
                .send(&router)
                .await
                .status
        }
    };

    // A new flight, the same one again, and something that is no flight:
    // each had to be read, so each counts.
    assert_eq!(attempt(sample_file()).await, StatusCode::CREATED);
    assert_eq!(attempt(sample_file()).await, StatusCode::OK);
    assert_eq!(
        attempt(b"hello".to_vec()).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        attempt(sample_file_on("050726")).await,
        StatusCode::TOO_MANY_REQUESTS
    );

    clock.advance(Duration::hours(24));
    assert_eq!(attempt(sample_file_on("050726")).await, StatusCode::CREATED);
}

#[tokio::test]
async fn an_account_keeps_no_more_than_the_server_gives_it_room_for() {
    let file_bytes = sample_file().len() as u64;
    let attempt = |router: &Router, token: &str, file: Vec<u8>| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, FLIGHTS_PATH)
                .token(&token)
                .bytes(file)
                .send(&router)
                .await
        }
    };

    // Room for two flights.
    let two_flights = Settings {
        room: Room {
            flights: 2,
            file_bytes: u64::MAX,
        },
        ..settings()
    };
    let (router, _) = server(two_flights, &TestClock::new());
    let dario = sign_up(&router, "dario").await;
    let first = upload(&router, &dario.token, &sample_file()).await;
    upload(&router, &dario.token, &sample_file_on("050726")).await;

    let answer = attempt(&router, &dario.token, sample_file_on("060726")).await;
    let message = answer.error(StatusCode::FORBIDDEN, ErrorCode::AccountFull);
    assert!(message.contains("keeps 2 flights"), "{message}");
    assert!(message.contains("room for 2 flights"), "{message}");
    let listed = my_flights(&router, &dario.token, &FlightsRequest::default()).await;
    assert_eq!(listed.flights.len(), 2);
    // A flight the account has is still answered as such: a device that
    // sends its logbook again learns what is there.
    let answer = attempt(&router, &dario.token, sample_file()).await;
    assert_eq!(answer.status, StatusCode::OK);
    // Another account has room of its own.
    let anna = sign_up(&router, "anna").await;
    upload(&router, &anna.token, &sample_file_on("060726")).await;
    // Deleting a flight makes room.
    let deleted = ask(Method::DELETE, &flight_path(&first.flight.id))
        .token(&dario.token)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let answer = attempt(&router, &dario.token, sample_file_on("060726")).await;
    assert_eq!(answer.status, StatusCode::CREATED);

    // Room for the bytes of two files and one byte less than a third.
    let nearly_three_files = Settings {
        room: Room {
            flights: u64::MAX,
            file_bytes: 3 * file_bytes - 1,
        },
        ..settings()
    };
    let (router, _) = server(nearly_three_files, &TestClock::new());
    let dario = sign_up(&router, "dario").await;
    upload(&router, &dario.token, &sample_file()).await;
    upload(&router, &dario.token, &sample_file_on("050726")).await;
    let answer = attempt(&router, &dario.token, sample_file_on("060726")).await;
    let message = answer.error(StatusCode::FORBIDDEN, ErrorCode::AccountFull);
    assert!(message.contains("delete flights to make room"), "{message}");
}

// --- Lists ------------------------------------------------------------------

#[tokio::test]
async fn flights_are_listed_newest_first_page_by_page() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    // Uploaded in no particular order.
    let mut by_day = Vec::new();
    for day in ["030726", "010726", "050726", "020726", "040726"] {
        let flight = upload(&router, &dario.token, &sample_file_on(day))
            .await
            .flight;
        by_day.push((day, flight.id));
    }
    upload(&router, &anna.token, &sample_file()).await;
    by_day.sort_by(|a, b| b.0.cmp(a.0));
    let newest_first: Vec<&FlightId> = by_day.iter().map(|(_, id)| id).collect();

    let answer = ask(Method::GET, FLIGHTS_PATH)
        .token(&dario.token)
        .send(&router)
        .await;
    let all: FlightsResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(ids(&all.flights), newest_first);
    assert_eq!(all.next, None);

    // Two at a time, each page asked for with the `next` of the one before.
    let mut seen = Vec::new();
    let mut request = FlightsRequest {
        after: None,
        limit: Some(2),
    };
    for expected in [2, 2, 1] {
        let page = my_flights(&router, &dario.token, &request).await;
        assert_eq!(page.flights.len(), expected);
        seen.extend(page.flights.into_iter().map(|flight| flight.id));
        request.after = page.next;
    }
    assert_eq!(request.after, None, "the last page names no next one");
    assert_eq!(seen.iter().collect::<Vec<_>>(), newest_first);
    // A page that ends exactly at the last flight names no next one either.
    let exact = FlightsRequest {
        after: None,
        limit: Some(5),
    };
    assert_eq!(my_flights(&router, &dario.token, &exact).await.next, None);

    // What is not a limit or not a `next` is refused.
    for (query, code) in [
        ("limit=0", ErrorCode::InvalidLimit),
        ("limit=101", ErrorCode::InvalidLimit),
        ("limit=ten", ErrorCode::InvalidLimit),
        ("after=yesterday", ErrorCode::InvalidCursor),
        ("after=", ErrorCode::InvalidCursor),
        (
            "after=1783161000000000_x'%20OR%20'1'='1",
            ErrorCode::InvalidCursor,
        ),
    ] {
        let answer = ask(Method::GET, &format!("{FLIGHTS_PATH}?{query}"))
            .token(&dario.token)
            .send(&router)
            .await;
        answer.error(StatusCode::BAD_REQUEST, code);
    }
    let answer = get(&router, FLIGHTS_PATH).await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
}

#[tokio::test]
async fn a_device_asks_what_changed_since_it_last_asked() {
    let clock = TestClock::new();
    let (router, _) = server(settings(), &clock);
    let dario = sign_up(&router, "dario").await;
    let token = dario.token.as_str();

    // A new account has no changes, and a cursor all the same.
    let nothing = changes(&router, token, None).await;
    assert!(nothing.flights.is_empty() && nothing.deleted.is_empty());
    assert!(!nothing.more && !nothing.reset);

    // The phone uploads two flights; the tablet asks for the first time.
    let first = upload(&router, token, &sample_file_on("010726"))
        .await
        .flight;
    clock.advance(Duration::minutes(1));
    let second = upload(&router, token, &sample_file_on("020726"))
        .await
        .flight;
    let answer = ask(Method::GET, FLIGHT_CHANGES_PATH)
        .token(token)
        .send(&router)
        .await;
    let seen: FlightChangesResponse = answer.ok(StatusCode::OK);
    answer.is_private();
    assert_eq!(ids(&seen.flights), [&first.id, &second.id]);
    assert!(seen.deleted.is_empty());
    assert!(!seen.more && !seen.reset);

    // Nothing happened since.
    let quiet = changes(&router, token, Some(&seen.cursor)).await;
    assert!(quiet.flights.is_empty() && quiet.deleted.is_empty());
    assert_eq!(quiet.cursor, seen.cursor);

    // On the phone the pilot shares the first flight, deletes the second
    // and uploads a third.
    clock.advance(Duration::minutes(1));
    set_visibility(&router, token, &first.id, Visibility::Public).await;
    clock.advance(Duration::minutes(1));
    let answer = ask(Method::DELETE, &flight_path(&second.id))
        .token(token)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    clock.advance(Duration::minutes(1));
    let third = upload(&router, token, &sample_file_on("030726"))
        .await
        .flight;

    // The tablet hears of all three, each as it is now.
    let news = changes(&router, token, Some(&seen.cursor)).await;
    assert_eq!(ids(&news.flights), [&first.id, &third.id]);
    assert_eq!(news.flights[0].visibility, Visibility::Public);
    assert_eq!(news.deleted.len(), 1);
    assert_eq!(news.deleted[0].id, second.id);
    assert_eq!(news.deleted[0].identity, second.identity);
    assert_ne!(news.cursor, seen.cursor);
    // A device that asks for the first time now gets the same picture.
    let fresh = changes(&router, token, None).await;
    assert_eq!(ids(&fresh.flights), [&first.id, &third.id]);
    assert_eq!(fresh.cursor, news.cursor);

    // One change at a time.
    let mut cursor = seen.cursor.clone();
    let mut steps = 0;
    loop {
        let request = FlightChangesRequest {
            since: Some(cursor.clone()),
            limit: Some(1),
        };
        let page: FlightChangesResponse = ask(Method::GET, &request.path_and_query())
            .token(token)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert!(page.flights.len() + page.deleted.len() <= 1);
        cursor = page.cursor;
        steps += page.flights.len() + page.deleted.len();
        if !page.more {
            break;
        }
    }
    assert_eq!(steps, 3);
    assert_eq!(cursor, news.cursor);

    // Another account's cursor, or one from before a restore of the
    // database, lies beyond this account's last change: the server starts
    // from the beginning and says so.
    clock.advance(Duration::hours(1));
    let anna = sign_up(&router, "anna").await;
    upload(&router, &anna.token, &sample_file()).await;
    let annas = changes(&router, &anna.token, None).await;
    let reset = changes(&router, token, Some(&annas.cursor)).await;
    assert!(reset.reset);
    assert_eq!(ids(&reset.flights), [&first.id, &third.id]);
    assert_eq!(reset.deleted.len(), 1);
    assert_eq!(reset.cursor, news.cursor);

    for bad in ["soon", "-1", "", "1.5", "99999999999999999999999"] {
        let request = FlightChangesRequest {
            since: Some(bad.to_owned()),
            limit: None,
        };
        let answer = ask(Method::GET, &request.path_and_query())
            .token(token)
            .send(&router)
            .await;
        answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidCursor);
    }
}

// --- Who may see what ---------------------------------------------------------

/// Everything about an answer that a client can observe.
fn observable(answer: &Answer) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
    let mut headers: Vec<(String, String)> = answer
        .headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        .collect();
    headers.sort();
    (answer.status, headers, answer.body.clone())
}

#[tokio::test]
async fn a_private_flight_looks_like_no_flight_to_everyone_but_its_pilot() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    let private = upload(&router, &dario.token, &sample_file()).await.flight;
    // An address of the right form that belongs to no flight.
    let nothing: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
    assert_ne!(private.id, nothing);

    // Its pilot sees it.
    assert_eq!(
        flight_as(&router, &dario.token, &private.id).await.status,
        StatusCode::OK
    );

    // For each way of asking, a stranger gets for the private flight
    // exactly what they get for the address of no flight: the same status,
    // the same headers, the same body.
    type Path = fn(&FlightId) -> String;
    let paths: [Path; 4] = [flight_path, flight_file_path, share_path, share_image_path];
    for path in paths {
        for token in [None, Some(anna.token.as_str())] {
            let ask_for = |id: &FlightId| {
                let mut request = ask(Method::GET, &path(id));
                if let Some(token) = token {
                    request = request.token(token);
                }
                request.send(&router)
            };
            let (hidden, missing) = (ask_for(&private.id).await, ask_for(&nothing).await);
            assert_eq!(
                hidden.status,
                StatusCode::NOT_FOUND,
                "{}",
                path(&private.id)
            );
            let (hidden, missing) = (observable(&hidden), observable(&missing));
            // The one thing that may differ is the address the client sent.
            let same = |text: &[u8]| {
                String::from_utf8_lossy(text).replace(private.id.as_str(), nothing.as_str())
            };
            assert_eq!(hidden.0, missing.0);
            assert_eq!(hidden.1, missing.1, "{}", path(&private.id));
            assert_eq!(same(&hidden.2), same(&missing.2));
            assert!(!same(&hidden.2).contains("private"));
        }
    }
    // A check with a validator does not tell them apart either.
    let conditional = get_with(
        &router,
        &flight_path(&private.id),
        &[(header::IF_NONE_MATCH, "*")],
    )
    .await;
    assert_eq!(conditional.status, StatusCode::NOT_FOUND);

    // Nor do attempts to change or delete it.
    for id in [&private.id, &nothing] {
        let patch = ask(Method::PATCH, &flight_path(id))
            .token(&anna.token)
            .json(&UpdateFlightRequest {
                visibility: Visibility::Public,
            })
            .send(&router)
            .await;
        patch.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
        let delete = ask(Method::DELETE, &flight_path(id))
            .token(&anna.token)
            .send(&router)
            .await;
        delete.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    }
    // The flight is as it was, and on nobody's page.
    let read: FlightResponse = flight_as(&router, &dario.token, &private.id)
        .await
        .ok(StatusCode::OK);
    assert_eq!(read.flight, private);
    get(&router, "/v1/pilots/dario")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);

    // Addresses of another form are the addresses of no flight.
    for odd in [
        "x",
        "0123456789ABCDEFGHJKMNPQRS",
        "..%2F..%2Fetc%2Fpasswd",
        "0123456789abcdefghjkmnpqr'",
        &"0".repeat(4000),
    ] {
        let answer = ask(Method::DELETE, &format!("{FLIGHTS_PATH}/{odd}"))
            .token(&dario.token)
            .send(&router)
            .await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
        let answer = get(&router, &format!("{FLIGHTS_PATH}/{odd}/igc")).await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    }
}

#[tokio::test]
async fn a_shared_flight_is_seen_by_others_and_changed_by_its_pilot_only() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let anna = sign_up(&router, "anna").await;
    let public = upload(&router, &dario.token, &sample_file_on("010726"))
        .await
        .flight;
    let unlisted = upload(&router, &dario.token, &sample_file_on("020726"))
        .await
        .flight;
    upload(&router, &dario.token, &sample_file_on("030726")).await;
    set_visibility(&router, &dario.token, &public.id, Visibility::Public).await;
    set_visibility(&router, &dario.token, &unlisted.id, Visibility::Unlisted).await;

    // Anyone reads both, signed in or not, and gets the file.
    for id in [&public.id, &unlisted.id] {
        let anonymous: FlightResponse = get(&router, &flight_path(id)).await.ok(StatusCode::OK);
        let signed_in: FlightResponse =
            flight_as(&router, &anna.token, id).await.ok(StatusCode::OK);
        assert_eq!(anonymous.flight, signed_in.flight);
        assert_eq!(anonymous.flight.pilot.handle.as_str(), "dario");
        let file = get(&router, &flight_file_path(id)).await;
        assert_eq!(file.status, StatusCode::OK);
        assert!(file.body.starts_with(b"AXVE000"));
    }

    // A token that is not good is told so even here, where no token is
    // needed: the device that sent it believes to be signed in.
    let never_issued = format!("venti_{}", "1".repeat(52));
    flight_as(&router, &never_issued, &public.id)
        .await
        .error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

    // Only the public one is listed on the pilot's page.
    let page: PilotResponse = get(&router, "/v1/pilots/dario").await.ok(StatusCode::OK);
    assert_eq!(ids(&page.flights), [&public.id]);

    // Seeing is not changing: to another pilot these are no flights of
    // theirs, and the answer is the one for no flight.
    for id in [&public.id, &unlisted.id] {
        let patch = ask(Method::PATCH, &flight_path(id))
            .token(&anna.token)
            .json(&UpdateFlightRequest {
                visibility: Visibility::Private,
            })
            .send(&router)
            .await;
        patch.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
        let delete = ask(Method::DELETE, &flight_path(id))
            .token(&anna.token)
            .send(&router)
            .await;
        delete.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
        // Nor without an account.
        let delete = ask(Method::DELETE, &flight_path(id)).send(&router).await;
        delete.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
        assert_eq!(get(&router, &flight_path(id)).await.status, StatusCode::OK);
    }

    // A setting the server does not know is refused, not guessed at.
    for body in [
        r#"{"visibility":"followers"}"#,
        r#"{"visibility":"PUBLIC"}"#,
    ] {
        let answer = ask(Method::PATCH, &flight_path(&public.id))
            .token(&dario.token)
            .header(header::CONTENT_TYPE, "application/json")
            .bytes(body)
            .send(&router)
            .await;
        answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidVisibility);
    }
    let answer = ask(Method::PATCH, &flight_path(&public.id))
        .token(&dario.token)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(r#"{"visible":true}"#)
        .send(&router)
        .await;
    answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);

    // Made private again, it is gone for everyone else at once.
    set_visibility(&router, &dario.token, &public.id, Visibility::Private).await;
    get(&router, &flight_path(&public.id))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    get(&router, "/v1/pilots/dario")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);

    // Deleted, it is gone for its pilot too.
    let answer = ask(Method::DELETE, &flight_path(&unlisted.id))
        .token(&dario.token)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    for token in [&dario.token, &anna.token] {
        flight_as(&router, token, &unlisted.id)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    }
    let again = ask(Method::DELETE, &flight_path(&unlisted.id))
        .token(&dario.token)
        .send(&router)
        .await;
    again.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
}

#[tokio::test]
async fn what_may_keep_an_answer_depends_on_who_it_was_for() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let public = upload(&router, &dario.token, &sample_file_on("010726"))
        .await
        .flight;
    let unlisted = upload(&router, &dario.token, &sample_file_on("020726"))
        .await
        .flight;
    set_visibility(&router, &dario.token, &public.id, Visibility::Public).await;
    set_visibility(&router, &dario.token, &unlisted.id, Visibility::Unlisted).await;

    for (id, keeper) in [
        (&public.id, "public, no-cache"),
        (&unlisted.id, "private, no-cache"),
    ] {
        for path in [flight_path(id), flight_file_path(id)] {
            // Asked without a token: may be kept, must be checked before
            // every reuse, and differs by who asks.
            let anonymous = get(&router, &path).await;
            assert_eq!(anonymous.status, StatusCode::OK);
            assert_eq!(anonymous.header(header::CACHE_CONTROL), keeper, "{path}");
            assert!(
                anonymous
                    .header_values(header::VARY)
                    .contains("authorization")
            );
            let tag = anonymous.header(header::ETAG).to_owned();
            let again = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
            assert_eq!(again.status, StatusCode::NOT_MODIFIED);
            assert!(again.body.is_empty());
            assert_eq!(again.header(header::CACHE_CONTROL), keeper);

            // Asked with a token: made for that account, kept by nobody,
            // and never answered with "you have it already".
            let signed = ask(Method::GET, &path)
                .token(&dario.token)
                .header(header::IF_NONE_MATCH, &tag)
                .send(&router)
                .await;
            assert_eq!(signed.status, StatusCode::OK);
            signed.is_private();
        }
    }

    // The check before reuse is what takes a flight back: once private,
    // a copy that someone kept is not confirmed any more.
    let path = flight_path(&public.id);
    let tag = get(&router, &path).await.header(header::ETAG).to_owned();
    set_visibility(&router, &dario.token, &public.id, Visibility::Private).await;
    let check = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
    check.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
}

#[tokio::test]
async fn a_pilots_page_lists_their_public_flights() {
    let router = open_server();
    let dario = sign_up(&router, "dario").await;
    let mut public = Vec::new();
    for day in ["010726", "020726", "030726"] {
        let flight = upload(&router, &dario.token, &sample_file_on(day))
            .await
            .flight;
        set_visibility(&router, &dario.token, &flight.id, Visibility::Public).await;
        public.push(flight.id);
    }
    upload(&router, &dario.token, &sample_file_on("040726")).await;
    public.reverse();
    let handle = Handle::new("dario").unwrap();

    let answer = get(&router, &pilot_path(&handle)).await;
    let page: PilotResponse = answer.ok(StatusCode::OK);
    assert_eq!(page.pilot.handle, handle);
    assert_eq!(page.pilot.display_name, "Pilot dario");
    assert_eq!(ids(&page.flights), public.iter().collect::<Vec<_>>());
    assert!(
        page.flights
            .iter()
            .all(|f| f.visibility == Visibility::Public)
    );
    // The same for everyone; checked before every reuse.
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    let tag = answer.header(header::ETAG).to_owned();
    let again = get_with(
        &router,
        "/v1/pilots/dario",
        &[(header::IF_NONE_MATCH, &tag)],
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    // The pilot sees their own page as everyone does: no private flight.
    let own: PilotResponse = ask(Method::GET, "/v1/pilots/dario")
        .token(&dario.token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(own, page);
    // However the handle is written.
    let shouted: PilotResponse = get(&router, "/v1/pilots/DARIO").await.ok(StatusCode::OK);
    assert_eq!(shouted, page);

    // Page by page.
    let mut request = PilotRequest {
        handle,
        after: None,
        limit: Some(2),
    };
    let first: PilotResponse = get(&router, &request.path_and_query())
        .await
        .ok(StatusCode::OK);
    assert_eq!(ids(&first.flights), [&public[0], &public[1]]);
    request.after = first.next;
    assert!(request.after.is_some());
    let second: PilotResponse = get(&router, &request.path_and_query())
        .await
        .ok(StatusCode::OK);
    assert_eq!(ids(&second.flights), [&public[2]]);
    assert_eq!(second.next, None);

    // Past the end of the list there is nothing, which is not the same
    // as no pilot.
    let past = "/v1/pilots/dario?after=0_0123456789abcdefghjkmnpqrs";
    let beyond: PilotResponse = get(&router, past).await.ok(StatusCode::OK);
    assert!(beyond.flights.is_empty());

    // A pilot nobody knows, and text that is no handle.
    for unknown in ["nobody", "x", "dario%20b", "..", "dario'--"] {
        let answer = get(&router, &format!("/v1/pilots/{unknown}")).await;
        let message = answer.error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
        assert_eq!(message, "there is no pilot with this handle");
    }
    // A pilot who shares nothing publicly has no page either, and looks
    // exactly like a handle nobody has: with flights that are private or
    // unlisted, and whatever is asked for.
    let anna = sign_up(&router, "anna").await;
    upload(&router, &anna.token, &sample_file_on("010726")).await;
    let unlisted = upload(&router, &anna.token, &sample_file_on("020726"))
        .await
        .flight;
    set_visibility(&router, &anna.token, &unlisted.id, Visibility::Unlisted).await;
    for query in ["", "?limit=1", "?after=0_0123456789abcdefghjkmnpqrs"] {
        let quiet = get(&router, &format!("/v1/pilots/anna{query}")).await;
        let nobody = get(&router, &format!("/v1/pilots/nobody{query}")).await;
        assert_eq!(observable(&quiet), observable(&nobody), "{query}");
        assert_eq!(quiet.status, StatusCode::NOT_FOUND);
    }
    // Also to Anna herself: her own page is the page everyone sees.
    let own = ask(Method::GET, "/v1/pilots/anna")
        .token(&anna.token)
        .send(&router)
        .await;
    own.error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    get(&router, "/v1/pilots/dario?limit=0")
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidLimit);
}

// --- The development mode -----------------------------------------------------

#[tokio::test]
async fn the_demo_account_has_three_flights() {
    let router = recorded().await;

    let answer = ask(Method::GET, FLIGHTS_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let mine: FlightsResponse = answer.ok(StatusCode::OK);
    let visibility: Vec<Visibility> = mine.flights.iter().map(|f| f.visibility).collect();
    assert_eq!(
        visibility,
        [
            Visibility::Public,
            Visibility::Unlisted,
            Visibility::Private
        ]
    );
    // The one in the middle is the sample file the apps bundle.
    assert_eq!(mine.flights[1].identity, "6c55247b80a34b03fa0bb3bd38173c59");
    assert!(mine.flights[0].landing.is_some());
    assert!(mine.flights[1].landing.is_none());
    assert_golden(&golden("flights.json"), &mine);

    let one: FlightResponse = get(&router, &flight_path(&mine.flights[0].id))
        .await
        .ok(StatusCode::OK);
    assert_eq!(one.flight, mine.flights[0]);
    assert_golden(&golden("flight.json"), &one);

    let page: PilotResponse = get(&router, "/v1/pilots/demo").await.ok(StatusCode::OK);
    assert_eq!(ids(&page.flights), [&mine.flights[0].id]);
    assert_golden(&golden("pilot.json"), &page);

    // What a device that asks for the first time gets, two changes a page.
    let request = FlightChangesRequest {
        since: None,
        limit: Some(2),
    };
    let first: FlightChangesResponse = ask(Method::GET, &request.path_and_query())
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(first.more);
    assert_eq!(first.flights.len(), 2);
    // A shorter example for the documentation: the outlines left out.
    let mut example = first.clone();
    for flight in &mut example.flights {
        flight.outline.truncate(2);
    }
    assert_golden(&golden("flight_changes.json"), &example);

    // Uploading the bundled sample again says that it is there already.
    let again = upload(&router, DEMO_TOKEN, &sample_file()).await;
    assert!(!again.created);
    assert_eq!(again.flight, mine.flights[1]);
    // A new flight for the demo account gets the same address on every
    // start, because the account is the same on every start.
    let new = upload(&router, DEMO_TOKEN, &sample_file_on("010926")).await;
    assert!(new.created);
    let mut example = new.clone();
    example.flight.outline.truncate(2);
    assert_golden(&golden("upload.json"), &example);
}

// --- A store in trouble ---------------------------------------------------------

#[tokio::test]
async fn a_database_that_is_down_is_said_to_be_down() {
    let api = venti_server::api::Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(TestClock::new()),
        std::iter::empty(),
        settings(),
    );
    let router = api.router(&venti_server::api::Cors::Any);
    let token = format!("venti_{}", "7".repeat(52));
    let id: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();

    // Not "you are not signed in", and not "no such flight": try again.
    for request in [
        ask(Method::GET, FLIGHTS_PATH).token(&token),
        ask(Method::POST, FLIGHTS_PATH)
            .token(&token)
            .bytes(sample_file()),
        ask(Method::GET, &flight_path(&id)),
        ask(Method::GET, &flight_file_path(&id)),
        ask(Method::GET, "/v1/pilots/dario"),
    ] {
        let answer = request.send(&router).await;
        let message = answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);
        assert_eq!(answer.header(header::RETRY_AFTER), "30");
        assert!(!message.contains("connection refused"), "{message}");
    }
    let page = get(&router, &share_path(&id)).await;
    assert_eq!(page.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(page.text().contains("Try again in a moment"));
    assert_eq!(page.header(header::RETRY_AFTER), "30");
}
