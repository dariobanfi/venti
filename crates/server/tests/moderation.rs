//! Reporting what others share, and what whoever runs the server does
//! about it: through the real router for what pilots do and see, and
//! through the operator's commands for the rest.
//!
//! What must not happen is tested here too: a report that tells of
//! something its reporter may not see, anything that tells a pilot that
//! they were reported, a flight that was taken down coming back, and an
//! account that may not publish getting something into a list.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ACCOUNT_PATH, AccountResponse, ErrorCode, FLIGHT_CHANGES_PATH, FLIGHTS_PATH, Flight,
    FlightChangesResponse, FlightId, FlightResponse, FlightsResponse, LIVE_POSITIONS_PATH, LiveFix,
    LivePositionsRequest, LivePositionsResponse, LiveTracksResponse, LiveVisibility,
    MAX_REPORT_TEXT_LENGTH, PUBLIC_FLIGHTS_PATH, PilotResponse, REPORTS_PATH, ReportReason,
    ReportRequest, ReportResponse, ReportSubject, SESSIONS_PATH, SignInResponse,
    UpdateFlightRequest, Visibility, flight_file_path, flight_path, share_image_path, share_path,
};
use venti_domain::GeoPoint;
use venti_server::accounts::Limits;
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::Clock;
use venti_server::dev::{
    DEMO_HANDLE, DEMO_LIVE_FLYING, DEMO_LIVE_LINK, DEMO_PASSWORD, DEMO_PILOTS, DEMO_TOKEN,
};
use venti_server::live;
use venti_server::moderation::{Action, Outcome, Site, perform};
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;
use venti_store::{MemoryStore, NewReport, ReportListing, ReportSaved, Store};

mod common;

use common::{
    Answer, PUBLIC_URL, TestClock, ask, assert_golden_text, get, golden, recorded,
    recorded_with_its_store, sample_file, sample_file_on, server, set_visibility, settings,
    sign_up, try_sign_in, upload,
};

// --- Helpers ----------------------------------------------------------------

/// A server that takes anyone, its store, and its clock.
fn open_server() -> (Router, Arc<MemoryStore>, TestClock) {
    let clock = TestClock::new();
    let (router, store) = server(settings(), &clock);
    (router, store, clock)
}

fn site() -> Site<'static> {
    Site {
        public_url: Some(PUBLIC_URL),
        live_hours: 48,
    }
}

/// Does what whoever runs the server asks for, at the clock's time.
async fn operator(store: &MemoryStore, clock: &TestClock, action: Action) -> Outcome {
    perform(store, clock, site(), &action).await.unwrap()
}

fn about_flight(id: &FlightId, reason: ReportReason, text: Option<&str>) -> ReportRequest {
    ReportRequest {
        subject: ReportSubject::Flight(id.to_string()),
        reason,
        text: text.map(str::to_owned),
    }
}

fn about(subject: ReportSubject) -> ReportRequest {
    ReportRequest {
        subject,
        reason: ReportReason::Offensive,
        text: None,
    }
}

async fn report(router: &Router, token: &str, request: &ReportRequest) -> Answer {
    ask(Method::POST, REPORTS_PATH)
        .token(token)
        .json(request)
        .send(router)
        .await
}

/// Uploads a flight and sets who may see it.
async fn fly(router: &Router, token: &str, file: &[u8], visibility: Visibility) -> Flight {
    let flight = upload(router, token, file).await.flight;
    if visibility != Visibility::Private {
        set_visibility(router, token, &flight.id, visibility).await;
    }
    flight
}

/// Asks, as the flight's pilot, for the flight to be seen by `visibility`.
async fn patch(router: &Router, token: &str, id: &FlightId, visibility: Visibility) -> Answer {
    ask(Method::PATCH, &flight_path(id))
        .token(token)
        .json(&UpdateFlightRequest { visibility })
        .send(router)
        .await
}

async fn own_flight(router: &Router, token: &str, id: &FlightId) -> Flight {
    let answer: FlightResponse = ask(Method::GET, &flight_path(id))
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK);
    answer.flight
}

/// Fixes for a live track, `seconds` before the moment of the recorded data.
fn positions(track: &str, visibility: LiveVisibility, seconds: &[i64]) -> LivePositionsRequest {
    let fixes = seconds
        .iter()
        .map(|second| LiveFix {
            time: adapters::recorded_at() - Duration::seconds(*second),
            position: GeoPoint::new(47.66, 11.8).unwrap(),
            altitude_m: 1600,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        })
        .collect();
    LivePositionsRequest {
        id: track.to_owned(),
        visibility,
        landed: false,
        fixes,
    }
}

async fn send_positions(router: &Router, token: &str, body: &LivePositionsRequest) -> Answer {
    ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(token)
        .json(body)
        .send(router)
        .await
}

async fn public_flights(router: &Router) -> Vec<FlightId> {
    let list: FlightsResponse = get(router, PUBLIC_FLIGHTS_PATH).await.ok(StatusCode::OK);
    list.flights.into_iter().map(|flight| flight.id).collect()
}

async fn gliders_on_the_map(router: &Router) -> Vec<String> {
    let list: LiveTracksResponse = get(router, "/v1/live?bbox=11,47,13,48")
        .await
        .ok(StatusCode::OK);
    list.tracks
        .into_iter()
        .map(|track| track.id.to_string())
        .collect()
}

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

// --- Reporting -----------------------------------------------------------------

#[tokio::test]
async fn a_pilot_reports_a_flight_a_page_and_a_live_track() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);
    let maras: PilotResponse = get(&router, "/v1/pilots/mara").await.ok(StatusCode::OK);
    let flight = &maras.flights[0];

    // The demo account reports a public flight of Mara's. This is the
    // example of the documentation.
    let request = about_flight(
        &flight.id,
        ReportReason::NotAFlight,
        Some("I flew there that day. This is my track, not hers."),
    );
    assert_golden(&golden("report_request.json"), &request);
    let answer = report(&router, DEMO_TOKEN, &request).await;
    let stored: ReportResponse = answer.ok(StatusCode::CREATED);
    assert!(stored.created);
    // Made for one account: nobody may keep it.
    answer.is_private();
    assert_golden(&golden("report.json"), &stored);

    // The same thing reported again is the same report, whatever it says
    // this time.
    let again = about_flight(&flight.id, ReportReason::Spam, None);
    let answer = report(&router, DEMO_TOKEN, &again).await;
    let known: ReportResponse = answer.ok(StatusCode::OK);
    assert!(!known.created);
    answer.is_private();

    // Mara's page, and a flight of Jonas's that is public. A handle is
    // taken in any case, as everywhere.
    for subject in [
        ReportSubject::Pilot("Mara".to_owned()),
        ReportSubject::Flight(first_public_flight(&router, jonas).await.to_string()),
    ] {
        let stored: ReportResponse = report(&router, DEMO_TOKEN, &about(subject))
            .await
            .ok(StatusCode::CREATED);
        assert!(stored.created);
    }
    // Mara signs in and reports the demo account's live tracks: the one on
    // everyone's map, and the one she was sent a link to.
    let signed_in: SignInResponse = try_sign_in(&router, mara, DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    for track in [DEMO_LIVE_FLYING, DEMO_LIVE_LINK] {
        let request = ReportRequest {
            subject: ReportSubject::LiveTrack(track.to_owned()),
            reason: ReportReason::Other,
            text: Some("Nobody is flying there. It is night.".to_owned()),
        };
        let stored: ReportResponse = report(&router, &signed_in.token, &request)
            .await
            .ok(StatusCode::CREATED);
        assert!(stored.created);
    }

    // Five reports wait for whoever runs the server, and nothing else
    // happened: every flight and every track is where it was.
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 5);
    // The clock of this mode stands still, so the five are of one moment
    // and in no order that means anything.
    let first = reports
        .iter()
        .find(|report| report.reason == "not_a_flight")
        .unwrap();
    assert_eq!(first.reporter_handle, DEMO_HANDLE);
    assert_eq!(first.about_handle, mara);
    assert_eq!(
        first.subject,
        venti_store::ReportSubject::Flight(flight.id.to_string())
    );
    assert_eq!(
        first.text.as_deref(),
        Some("I flew there that day. This is my track, not hers.")
    );
    let maras: Vec<_> = reports
        .iter()
        .filter(|report| report.reporter_handle == mara)
        .collect();
    assert_eq!(maras.len(), 2);
    assert!(
        maras
            .iter()
            .all(|report| report.about_handle == DEMO_HANDLE)
    );
    assert_eq!(public_flights(&router).await.len(), 5);
    assert_eq!(gliders_on_the_map(&router).await.len(), 2);
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(listed.said.starts_with("5 reports are open, oldest first."));
}

async fn first_public_flight(router: &Router, handle: &str) -> FlightId {
    let page: PilotResponse = get(router, &format!("/v1/pilots/{handle}"))
        .await
        .ok(StatusCode::OK);
    page.flights[0].id.clone()
}

#[tokio::test]
async fn what_a_reporter_may_not_see_is_not_there_to_report() {
    let clock = TestClock::new();
    let with_a_clock = Settings {
        live: live::Limits::default(),
        ..settings()
    };
    let (router, store) = server(with_a_clock, &clock);
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let private = fly(
        &router,
        &anna,
        &sample_file_on("010726"),
        Visibility::Private,
    )
    .await;
    let unlisted = fly(
        &router,
        &anna,
        &sample_file_on("020726"),
        Visibility::Unlisted,
    )
    .await;
    let track = "a".repeat(26);
    let begun = send_positions(
        &router,
        &anna,
        &positions(&track, LiveVisibility::Link, &[30]),
    )
    .await;
    assert_eq!(begun.status, StatusCode::CREATED);

    // A private flight of someone else's answers exactly as an address
    // that belongs to no flight: reporting is no way to find one.
    let no_flight: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
    let hidden = report(
        &router,
        &dario,
        &about_flight(&private.id, ReportReason::Spam, None),
    )
    .await;
    let nothing = report(
        &router,
        &dario,
        &about_flight(&no_flight, ReportReason::Spam, None),
    )
    .await;
    hidden.error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    assert_eq!(observable(&hidden), observable(&nothing));
    let reading = ask(Method::GET, &flight_path(&private.id))
        .token(&dario)
        .send(&router)
        .await;
    assert_eq!(hidden.body, reading.body, "as reading it answers");
    // Anna has no public flight, so she has no page: her handle answers as
    // a handle nobody has.
    let quiet = report(
        &router,
        &dario,
        &about(ReportSubject::Pilot("anna".to_owned())),
    )
    .await;
    let nobody = report(
        &router,
        &dario,
        &about(ReportSubject::Pilot("nobody".to_owned())),
    )
    .await;
    quiet.error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    assert_eq!(observable(&quiet), observable(&nobody));
    // Text that is no address, no handle and no identifier names nothing.
    for (subject, code) in [
        (
            ReportSubject::Flight("../account".to_owned()),
            ErrorCode::FlightNotFound,
        ),
        (
            ReportSubject::Flight(String::new()),
            ErrorCode::FlightNotFound,
        ),
        (
            ReportSubject::Pilot("x' OR '1'='1".to_owned()),
            ErrorCode::PilotNotFound,
        ),
        (
            ReportSubject::LiveTrack("positions".to_owned()),
            ErrorCode::LiveTrackNotFound,
        ),
        (
            ReportSubject::LiveTrack("c".repeat(26)),
            ErrorCode::LiveTrackNotFound,
        ),
    ] {
        let answer = report(&router, &dario, &about(subject.clone())).await;
        let said = answer.error(StatusCode::NOT_FOUND, code);
        assert!(!said.contains("account") && !said.contains("OR"), "{said}");
    }
    assert!(
        store
            .reports(ReportListing::All, None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    // What Dario was given a link to, he can report: a flight shared by
    // link, and a live track shared by link.
    let stored: ReportResponse = report(
        &router,
        &dario,
        &about_flight(&unlisted.id, ReportReason::Privacy, None),
    )
    .await
    .ok(StatusCode::CREATED);
    assert!(stored.created);
    let by_link = about(ReportSubject::LiveTrack(track.clone()));
    report(&router, &dario, &by_link)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    // Once Anna has a public flight, her page is there to report.
    set_visibility(&router, &anna, &unlisted.id, Visibility::Public).await;
    let page = about(ReportSubject::Pilot("anna".to_owned()));
    report(&router, &dario, &page)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    // A pilot may report what is their own; it is one more report to read.
    let own = about_flight(
        &private.id,
        ReportReason::Other,
        Some("Please remove this for me."),
    );
    report(&router, &anna, &own)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);

    // A live track that ended more than two days ago is gone, also for a
    // report.
    clock.advance(Duration::hours(49));
    let answer = report(&router, &anna, &about(ReportSubject::LiveTrack(track))).await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    assert_eq!(
        store
            .reports(ReportListing::All, None, 10)
            .await
            .unwrap()
            .len(),
        4
    );
}

#[tokio::test]
async fn what_is_not_a_report_is_refused_and_nothing_is_kept() {
    let (router, store, clock) = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let flight = fly(&router, &anna, &sample_file(), Visibility::Public).await;
    let good = about_flight(&flight.id, ReportReason::Spam, None);
    let body = |text: String| {
        ask(Method::POST, REPORTS_PATH)
            .token(&dario)
            .header(header::CONTENT_TYPE, "application/json")
            .bytes(text)
    };
    let id = flight.id.as_str();

    // Nobody in particular reports nothing.
    let answer = ask(Method::POST, REPORTS_PATH)
        .json(&good)
        .send(&router)
        .await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    let answer = ask(Method::POST, REPORTS_PATH)
        .token(&format!("venti_{}", "7".repeat(52)))
        .json(&good)
        .send(&router)
        .await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);
    // A body that does not say it is JSON, as a form on another site's
    // page would send it.
    let answer = ask(Method::POST, REPORTS_PATH)
        .token(&dario)
        .header(header::CONTENT_TYPE, "text/plain")
        .bytes(serde_json::to_vec(&good).unwrap())
        .send(&router)
        .await;
    answer.error(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedMediaType,
    );

    // Not the JSON a report is: no subject, two subjects, a kind of
    // subject nobody knows, no reason.
    for text in [
        String::new(),
        "report this".to_owned(),
        r#"{"reason":"spam"}"#.to_owned(),
        format!(r#"{{"subject":{{"flight":"{id}","pilot":"anna"}},"reason":"spam"}}"#),
        format!(r#"{{"subject":{{"site":"{id}"}},"reason":"spam"}}"#),
        format!(r#"{{"subject":{{"flight":"{id}"}}}}"#),
        format!(r#"{{"subject":{{"flight":["{id}"]}},"reason":"spam"}}"#),
        format!(r#"{{"subject":{{"flight":"{id}"}},"reason":"spam","text":7}}"#),
    ] {
        let answer = body(text.clone()).send(&router).await;
        let said = answer.error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
        assert!(!said.contains(id), "{said}");
    }
    // A reason the server does not know, words that are too many, and
    // "other" without a word of what.
    let unknown = format!(r#"{{"subject":{{"flight":"{id}"}},"reason":"airspace"}}"#);
    let said = body(unknown)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    assert_eq!(
        said,
        "reason must be one of offensive, impersonation, privacy, not_a_flight, spam, other"
    );
    let long = "ä".repeat(MAX_REPORT_TEXT_LENGTH + 1);
    let wordy = about_flight(&flight.id, ReportReason::Spam, Some(&long));
    let said = report(&router, &dario, &wordy)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    assert_eq!(said, "the text of a report has at most 1000 characters");
    for nothing_said in [None, Some(""), Some(" \n\t "), Some("\u{200b}\u{1b}")] {
        let other = about_flight(&flight.id, ReportReason::Other, nothing_said);
        let said = report(&router, &dario, &other)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
        assert!(said.contains("says in its text"), "{said}");
    }
    assert!(
        store
            .reports(ReportListing::All, None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    // A thousand characters are taken, and what a terminal would act on is
    // not kept.
    let full = "ä".repeat(MAX_REPORT_TEXT_LENGTH);
    let wordy = about_flight(&flight.id, ReportReason::Spam, Some(&full));
    report(&router, &dario, &wordy)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    clock.advance(Duration::seconds(1));
    let page = ReportRequest {
        subject: ReportSubject::Pilot("anna".to_owned()),
        reason: ReportReason::Offensive,
        text: Some("  The name.\r\n\u{1b}[2JSecond\u{202e} line. ".to_owned()),
    };
    report(&router, &dario, &page)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    let reports = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(reports[0].text.as_deref(), Some(full.as_str()));
    assert_eq!(
        reports[1].text.as_deref(),
        Some("The name.\n[2JSecond line.")
    );

    // Reports are sent, and nothing else is done with them here: there is
    // no reading, no list and no taking back.
    for method in [Method::GET, Method::PUT, Method::PATCH, Method::DELETE] {
        let answer = ask(method, REPORTS_PATH).token(&dario).send(&router).await;
        answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
    }
    let answer = ask(Method::GET, "/v1/reports/0123456789abcdefghjkmnpqrs")
        .token(&dario)
        .send(&router)
        .await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
}

#[tokio::test]
async fn an_account_sends_a_limited_number_of_reports_a_day() {
    let clock = TestClock::new();
    let strict = Settings {
        limits: Limits {
            reports_per_day: 3,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, store) = server(strict, &clock);
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await.token;
    let mut flights = Vec::new();
    for day in ["010726", "020726", "030726", "040726"] {
        flights.push(fly(&router, &anna, &sample_file_on(day), Visibility::Public).await);
    }
    let about = |n: usize| about_flight(&flights[n].id, ReportReason::Spam, None);

    // Three a day, whatever becomes of them: one that is stored, one that
    // was there already, and one about nothing.
    report(&router, &dario, &about(0))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    report(&router, &dario, &about(0))
        .await
        .ok::<ReportResponse>(StatusCode::OK);
    let nothing: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
    report(
        &router,
        &dario,
        &about_flight(&nothing, ReportReason::Spam, None),
    )
    .await
    .error(StatusCode::NOT_FOUND, ErrorCode::FlightNotFound);
    let refused = report(&router, &dario, &about(1)).await;
    let said = refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert!(
        said.contains("as many reports today as one account may"),
        "{said}"
    );
    assert_eq!(refused.header(header::RETRY_AFTER), "86400");
    // Asking again does not help, and does not put the day off.
    clock.advance(Duration::hours(23));
    report(&router, &dario, &about(1))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert_eq!(
        store
            .reports(ReportListing::All, None, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    // What a browser was made to send is not counted, and is refused for
    // what it is before anything else.
    let answer = ask(Method::POST, REPORTS_PATH)
        .token(&dario)
        .bytes("{}")
        .send(&router)
        .await;
    answer.error(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedMediaType,
    );
    // Another account has a day of its own.
    report(&router, &anna, &about(2))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    // The next day Dario reports again.
    clock.advance(Duration::hours(1) + Duration::seconds(1));
    report(&router, &dario, &about(1))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
}

#[tokio::test]
async fn nothing_tells_a_pilot_that_they_were_reported() {
    let (router, store, clock) = open_server();
    let dario = sign_up(&router, "dario").await.token;
    let anna = sign_up(&router, "anna").await;
    let token = &anna.token;
    let public = fly(
        &router,
        token,
        &sample_file_on("010726"),
        Visibility::Public,
    )
    .await;
    let unlisted = fly(
        &router,
        token,
        &sample_file_on("020726"),
        Visibility::Unlisted,
    )
    .await;
    let track = "a".repeat(26);
    send_positions(
        &router,
        token,
        &positions(&track, LiveVisibility::Public, &[30]),
    )
    .await;

    // Everything Anna can ask the server, with her token and without.
    let questions: Vec<(String, bool)> = [
        (ACCOUNT_PATH.to_owned(), true),
        (SESSIONS_PATH.to_owned(), true),
        (FLIGHTS_PATH.to_owned(), true),
        (FLIGHT_CHANGES_PATH.to_owned(), true),
        (flight_path(&public.id), true),
        (flight_path(&unlisted.id), true),
        (flight_file_path(&public.id), true),
        (flight_path(&public.id), false),
        (share_path(&public.id), false),
        (share_path(&unlisted.id), false),
        (share_image_path(&public.id), false),
        ("/v1/pilots/anna".to_owned(), false),
        ("/v1/pilots/anna".to_owned(), true),
        (PUBLIC_FLIGHTS_PATH.to_owned(), false),
        (format!("/v1/live/{track}"), false),
        (format!("/live/{track}"), false),
        ("/v1/live?bbox=11,47,13,48".to_owned(), false),
    ]
    .into();
    let ask_all = || async {
        let mut answers = Vec::new();
        for (path, signed) in &questions {
            let mut request = ask(Method::GET, path);
            if *signed {
                request = request.token(token);
            }
            answers.push(observable(&request.send(&router).await));
        }
        answers
    };
    let before = ask_all().await;

    // Dario reports all of it, in words that stand nowhere else.
    for subject in [
        ReportSubject::Flight(public.id.to_string()),
        ReportSubject::Flight(unlisted.id.to_string()),
        ReportSubject::Pilot("anna".to_owned()),
        ReportSubject::LiveTrack(track.clone()),
    ] {
        let request = ReportRequest {
            subject,
            reason: ReportReason::Impersonation,
            text: Some("Zanzibar marmalade".to_owned()),
        };
        report(&router, &dario, &request)
            .await
            .ok::<ReportResponse>(StatusCode::CREATED);
    }
    assert_eq!(
        store
            .reports(ReportListing::Open, None, 10)
            .await
            .unwrap()
            .len(),
        4
    );

    // Every answer is what it was, in status, headers and body: nothing
    // says that there is a report, let alone whose.
    let after = ask_all().await;
    for ((path, signed), (before, after)) in questions.iter().zip(before.iter().zip(&after)) {
        assert_eq!(before, after, "{path} (with a token: {signed})");
        let body = String::from_utf8_lossy(&after.2);
        for told in ["dario", "Zanzibar", "impersonation"] {
            assert!(!body.contains(told), "{path} says {told:?}");
        }
    }
    // Closing the reports tells nobody either.
    for listed in store.reports(ReportListing::Open, None, 10).await.unwrap() {
        let closing = Action::CloseReport {
            id: listed.id,
            note: "Looked at it. Nothing to do.".to_owned(),
        };
        assert!(operator(&store, &clock, closing).await.found);
    }
    assert_eq!(ask_all().await, before);
}

// --- What whoever runs the server reads ------------------------------------------

#[tokio::test]
async fn whoever_runs_the_server_reads_the_reports_and_closes_them() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let nothing = operator(&store, &clock, Action::Reports { all: false }).await;
    assert_eq!(nothing.said, "No report is open.");
    assert!(nothing.found);

    // Three reports with identifiers of this test's choosing, so that what
    // is printed is the same every time: about a flight, a pilot's page
    // and a live track.
    let account = |handle: &'static str| {
        let store = store.clone();
        async move { store.account(handle).await.unwrap().unwrap().id }
    };
    let (demo, mara, jonas) = (
        account("demo").await,
        account("mara").await,
        account("jonas").await,
    );
    let flight = first_public_flight(&router, "mara").await;
    let made_at = adapters::recorded_at();
    let reports = [
        NewReport {
            id: "0000000000000000000000rep1".to_owned(),
            created_at: made_at - Duration::hours(26),
            reporter_id: demo.clone(),
            about_account_id: mara.clone(),
            subject: venti_store::ReportSubject::Flight(flight.to_string()),
            reason: "not_a_flight".to_owned(),
            text: Some("I flew there that day.\nThis is my track, not hers.".to_owned()),
        },
        NewReport {
            id: "0000000000000000000000rep2".to_owned(),
            created_at: made_at - Duration::hours(3),
            reporter_id: jonas,
            about_account_id: mara,
            subject: venti_store::ReportSubject::Pilot,
            reason: "impersonation".to_owned(),
            text: None,
        },
        NewReport {
            id: "0000000000000000000000rep3".to_owned(),
            created_at: made_at - Duration::minutes(10),
            reporter_id: account("mara").await,
            about_account_id: demo,
            subject: venti_store::ReportSubject::LiveTrack(DEMO_LIVE_FLYING.to_owned()),
            reason: "other".to_owned(),
            text: Some("Nobody is flying there. It is night.".to_owned()),
        },
    ];
    for new in &reports {
        assert_eq!(store.save_report(new).await.unwrap(), ReportSaved::Stored);
    }

    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(listed.found);
    assert_golden_text("operator_reports.txt", &format!("{}\n", listed.said));
    // It names who reported, what, and why, and gives the links.
    for expected in [
        "3 reports are open, oldest first.",
        "Report 0000000000000000000000rep1, open, made 2026-11-17 17:05 UTC",
        &format!("flight {flight} of mara (Mara Muster); now public"),
        &format!("https://venti.example/f/{flight}"),
        "  Reason:       not_a_flight",
        "  Text:         I flew there that day.\n                This is my track, not hers.",
        "  Reported by:  demo",
        "the page of mara (Mara Muster)",
        "live track 11111111111111111111111111 of demo (Demo Pilot); now on everyone's map",
        "https://venti.example/live/11111111111111111111111111",
    ] {
        assert!(
            listed.said.contains(expected),
            "{expected}\n{}",
            listed.said
        );
    }

    // Closing one with a note. It leaves the list of open reports, and
    // nothing happens to the flight.
    let close = |id: &str, note: &str| Action::CloseReport {
        id: id.to_owned(),
        note: note.to_owned(),
    };
    clock.advance(Duration::minutes(5));
    let report1 = "0000000000000000000000rep1";
    let closed = operator(
        &store,
        &clock,
        close(report1, "Compared the two files: hers."),
    )
    .await;
    assert!(closed.found, "{}", closed.said);
    assert!(closed.said.contains("is closed"));
    assert!(
        closed
            .said
            .contains("changes nothing about what was reported")
    );
    assert_eq!(first_public_flight(&router, "mara").await, flight);
    let open = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(open.said.starts_with("2 reports are open"));
    assert!(!open.said.contains(report1));
    let all = operator(&store, &clock, Action::Reports { all: true }).await;
    assert!(
        all.said
            .starts_with("3 reports, 2 of them open, oldest first.")
    );
    assert!(all.said.contains(
        "Report 0000000000000000000000rep1, closed 2026-11-18 19:10 UTC, made 2026-11-17 17:05 UTC"
    ));
    assert!(
        all.said
            .contains("  Note:         Compared the two files: hers.")
    );

    // Closing it again says how it was closed; what is no report, or no
    // note, is said to be none.
    let again = operator(&store, &clock, close(report1, "Another note.")).await;
    assert!(again.found);
    assert!(
        again
            .said
            .contains("was closed on 2026-11-18 19:10 UTC already")
    );
    assert!(again.said.contains("Compared the two files: hers."));
    for no_report in ["0000000000000000000000rep9", "report1", ""] {
        let missing = operator(&store, &clock, close(no_report, "A note.")).await;
        assert!(!missing.found, "{no_report:?}: {}", missing.said);
    }
    for no_note in ["", "   ", &"x".repeat(1001)] {
        let closing = close("0000000000000000000000rep2", no_note);
        let refused = perform(&*store, &clock, site(), &closing).await;
        let said = refused.unwrap_err();
        assert!(
            said.contains("a note has from 1 to 1000 characters"),
            "{said}"
        );
    }

    // What a pilot wrote is printed without anything a terminal acts on.
    let hostile = NewReport {
        id: "0000000000000000000000rep4".to_owned(),
        created_at: made_at,
        text: Some("first\u{1b}[2J\rline\nsecond\u{7} line".to_owned()),
        ..reports[1].clone()
    };
    store
        .close_report("0000000000000000000000rep2", "Done.", Clock::now(&clock))
        .await
        .unwrap();
    assert_eq!(
        store.save_report(&hostile).await.unwrap(),
        ReportSaved::Stored
    );
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(
        listed
            .said
            .contains("  Text:         first?[2J?line\n                second? line")
    );
    assert!(!listed.said.contains('\u{1b}') && !listed.said.contains('\u{7}'));
}

// --- Taking a flight down ----------------------------------------------------------

#[tokio::test]
async fn a_flight_taken_down_is_private_and_its_pilot_cannot_share_it_again() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let mine: FlightsResponse = ask(Method::GET, FLIGHTS_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let flight = mine.flights[0].clone();
    assert_eq!(flight.visibility, Visibility::Public);
    assert_eq!(flight.taken_down_at, None);
    let no_flight: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
    let nothing = [
        observable(&get(&router, &flight_path(&no_flight)).await),
        observable(&get(&router, &share_path(&no_flight)).await),
    ];

    // By the link someone reported, as it would be pasted.
    let link = format!("{PUBLIC_URL}/f/{}", flight.id);
    let taken = operator(&store, &clock, Action::TakeDown { flight: link }).await;
    assert!(taken.found, "{}", taken.said);
    assert!(taken.said.contains(&format!(
        "Flight {} of demo (Demo Pilot) is taken down",
        flight.id
    )));
    assert!(taken.said.contains("It was public and is private now"));
    assert!(
        taken
            .said
            .contains("Nothing was deleted, and no report was closed")
    );

    // To everyone else the flight is no flight, in every answer and every
    // list, exactly as an address that never was one.
    let gone = [
        observable(&get(&router, &flight_path(&flight.id)).await),
        observable(&get(&router, &share_path(&flight.id)).await),
    ];
    assert_eq!(gone, nothing);
    assert_eq!(
        get(&router, &share_image_path(&flight.id)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&router, &flight_file_path(&flight.id)).await.status,
        StatusCode::NOT_FOUND
    );
    assert!(!public_flights(&router).await.contains(&flight.id));
    get(&router, "/v1/pilots/demo")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);

    // Its pilot still has it, and is told what was done and since when.
    let shown = ask(Method::GET, &flight_path(&flight.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let answer: FlightResponse = shown.ok(StatusCode::OK);
    assert_eq!(answer.flight.visibility, Visibility::Private);
    assert_eq!(answer.flight.taken_down_at, Some(adapters::recorded_at()));
    let mut example = answer.clone();
    example.flight.outline.truncate(2);
    assert_golden(&golden("flight_taken_down.json"), &example);
    assert!(
        shown
            .text()
            .contains(r#""taken_down_at":"2026-11-18T19:05:00Z""#)
    );
    let own_file = ask(Method::GET, &flight_file_path(&flight.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(own_file.status, StatusCode::OK);
    // The pilot's other devices hear of it as of any change.
    let changes: FlightChangesResponse = ask(Method::GET, FLIGHT_CHANGES_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let changed = changes.flights.iter().find(|f| f.id == flight.id).unwrap();
    assert_eq!(changed.taken_down_at, Some(adapters::recorded_at()));
    // No other flight says anything of the kind.
    assert!(
        !ask(Method::GET, FLIGHTS_PATH)
            .token(DEMO_TOKEN)
            .send(&router)
            .await
            .text()
            .replacen("taken_down_at", "", 1)
            .contains("taken_down_at")
    );

    // The pilot cannot share it again, in either way; asking for what it
    // is, private, is fine.
    for wanted in [Visibility::Public, Visibility::Unlisted] {
        let refused = patch(&router, DEMO_TOKEN, &flight.id, wanted).await;
        let said = refused.error(StatusCode::FORBIDDEN, ErrorCode::FlightTakenDown);
        assert!(
            said.contains("whoever runs this server has taken the flight down"),
            "{said}"
        );
    }
    let same: FlightResponse = patch(&router, DEMO_TOKEN, &flight.id, Visibility::Private)
        .await
        .ok(StatusCode::OK);
    assert_eq!(same.flight, answer.flight);
    assert_eq!(
        get(&router, &share_path(&flight.id)).await.status,
        StatusCode::NOT_FOUND
    );

    // Nor by deleting the flight and uploading its file again: it comes
    // back under its address, taken down since the same moment.
    let own_file = own_file.body;
    let deleted = ask(Method::DELETE, &flight_path(&flight.id))
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    clock.advance(Duration::hours(1));
    let back = upload(&router, DEMO_TOKEN, &own_file).await;
    assert!(back.created);
    assert_eq!(back.flight.id, flight.id);
    assert_eq!(back.flight.taken_down_at, Some(adapters::recorded_at()));
    patch(&router, DEMO_TOKEN, &flight.id, Visibility::Public)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::FlightTakenDown);

    // Taking it down again changes nothing. Released, it is still private,
    // and its pilot may share it.
    let again = Action::TakeDown {
        flight: flight.id.to_string(),
    };
    let again = operator(&store, &clock, again).await;
    assert!(again.found);
    assert!(
        again
            .said
            .contains("was taken down on 2026-11-18 19:05 UTC already")
    );
    let released = Action::Release {
        flight: flight.id.to_string(),
    };
    let released = operator(&store, &clock, released).await;
    assert!(released.found, "{}", released.said);
    assert!(released.said.contains("is released"));
    let now = own_flight(&router, DEMO_TOKEN, &flight.id).await;
    assert_eq!(
        (now.visibility, now.taken_down_at),
        (Visibility::Private, None)
    );
    assert_eq!(
        get(&router, &flight_path(&flight.id)).await.status,
        StatusCode::NOT_FOUND
    );
    set_visibility(&router, DEMO_TOKEN, &flight.id, Visibility::Public).await;
    assert!(public_flights(&router).await.contains(&flight.id));
    let not_down = Action::Release {
        flight: flight.id.to_string(),
    };
    let not_down = operator(&store, &clock, not_down).await;
    assert!(
        not_down
            .said
            .contains("is not taken down. Nothing changed.")
    );

    // What is no flight is said to be none, and a failure for a script.
    for unknown in [no_flight.to_string(), "demo".to_owned(), String::new()] {
        for action in [
            Action::TakeDown {
                flight: unknown.clone(),
            },
            Action::Release {
                flight: unknown.clone(),
            },
        ] {
            let outcome = operator(&store, &clock, action).await;
            assert!(!outcome.found, "{unknown:?}: {}", outcome.said);
        }
    }
}

// --- Suspending an account's publishing ----------------------------------------------

#[tokio::test]
async fn an_account_that_may_not_publish_is_told_so_and_publishes_nothing() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let mine: FlightsResponse = ask(Method::GET, FLIGHTS_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    let [public, unlisted, private] = &mine.flights[..] else {
        panic!("the demo account has three flights");
    };
    assert_eq!(gliders_on_the_map(&router).await.len(), 2);
    assert_eq!(public_flights(&router).await.len(), 5);

    let suspended = Action::SuspendPublishing {
        handle: "Demo".to_owned(),
    };
    let suspended = operator(&store, &clock, suspended).await;
    assert!(suspended.found, "{}", suspended.said);
    assert!(
        suspended.said.starts_with(
            "demo (Demo Pilot) may not publish from now on.\n  \
             Public flights that are for those with their link now: 1\n  \
             Live tracks taken off everyone's map, for those with their link now: 2\n"
        ),
        "{}",
        suspended.said
    );
    assert!(suspended.said.contains("Nothing was deleted."));
    assert_golden_text("operator_suspend.txt", &format!("{}\n", suspended.said));

    // The account says so to its pilot, here and when signing in.
    let shown = ask(Method::GET, ACCOUNT_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await;
    let account: AccountResponse = shown.ok(StatusCode::OK);
    assert_eq!(
        account.account.publishing_suspended_at,
        Some(adapters::recorded_at())
    );
    assert_golden(&golden("account_suspended.json"), &account);
    let signed_in: SignInResponse = try_sign_in(&router, DEMO_HANDLE, DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(signed_in.account, account.account);

    // Nothing of it is in a list that others read.
    assert_eq!(public_flights(&router).await.len(), 4);
    assert!(!public_flights(&router).await.contains(&public.id));
    get(&router, "/v1/pilots/demo")
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::PilotNotFound);
    assert!(gliders_on_the_map(&router).await.is_empty());
    // Whoever has a link still sees what the link shows: the flight that
    // was public is shared by link now, and so are the live tracks.
    let by_link = get(&router, &flight_path(&public.id)).await;
    let answer: FlightResponse = by_link.ok(StatusCode::OK);
    assert_eq!(answer.flight.visibility, Visibility::Unlisted);
    assert_eq!(by_link.header(header::CACHE_CONTROL), "private, no-cache");
    assert_eq!(
        get(&router, &share_path(&public.id)).await.status,
        StatusCode::OK
    );
    let track = get(&router, &format!("/v1/live/{DEMO_LIVE_FLYING}")).await;
    assert!(track.text().contains(r#""visibility":"link""#));
    // The others are as they were.
    assert_eq!(
        own_flight(&router, DEMO_TOKEN, &unlisted.id).await,
        *unlisted
    );
    assert_eq!(own_flight(&router, DEMO_TOKEN, &private.id).await, *private);
    get(&router, "/v1/pilots/mara")
        .await
        .ok::<PilotResponse>(StatusCode::OK);

    // It can make nothing public, and is told who stands in the way.
    for flight in [public, unlisted, private] {
        let refused = patch(&router, DEMO_TOKEN, &flight.id, Visibility::Public).await;
        let said = refused.error(StatusCode::FORBIDDEN, ErrorCode::PublishingSuspended);
        assert!(
            said.contains("suspended this account's publishing"),
            "{said}"
        );
    }
    // Everything else works: sharing by link, taking back, uploading.
    set_visibility(&router, DEMO_TOKEN, &private.id, Visibility::Unlisted).await;
    set_visibility(&router, DEMO_TOKEN, &private.id, Visibility::Private).await;
    assert!(
        upload(&router, DEMO_TOKEN, &sample_file_on("010926"))
            .await
            .created
    );

    // A live track it sends as public is kept, for those with its link:
    // the answer says what the track is, and the map does not show it.
    let new_track = "4".repeat(26);
    let body = positions(&new_track, LiveVisibility::Public, &[20, 10]);
    let sent: LivePositionsResponse = send_positions(&router, DEMO_TOKEN, &body)
        .await
        .ok(StatusCode::CREATED);
    assert_eq!(sent.track.visibility, LiveVisibility::Link);
    assert_eq!((sent.stored, sent.track.fixes), (2, 2));
    let more = positions(DEMO_LIVE_FLYING, LiveVisibility::Public, &[0]);
    let sent: LivePositionsResponse = send_positions(&router, DEMO_TOKEN, &more)
        .await
        .ok(StatusCode::OK);
    assert_eq!(sent.track.visibility, LiveVisibility::Link);
    assert!(gliders_on_the_map(&router).await.is_empty());

    // Suspending again keeps the moment; resuming gives the ability back
    // and publishes nothing.
    clock.advance(Duration::hours(2));
    let again = Action::SuspendPublishing {
        handle: "demo".to_owned(),
    };
    let again = operator(&store, &clock, again).await;
    assert!(again.said.starts_with(
        "demo (Demo Pilot) may not publish since 2026-11-18 19:05 UTC already.\n  \
         Public flights that are for those with their link now: 0\n"
    ));
    let resumed = Action::ResumePublishing {
        handle: "@demo".to_owned(),
    };
    let resumed = operator(&store, &clock, resumed).await;
    assert!(resumed.found);
    assert!(
        resumed
            .said
            .contains("may publish again. Nothing was made public")
    );
    let account: AccountResponse = ask(Method::GET, ACCOUNT_PATH)
        .token(DEMO_TOKEN)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert_eq!(account.account.publishing_suspended_at, None);
    assert_eq!(public_flights(&router).await.len(), 4);
    assert!(gliders_on_the_map(&router).await.is_empty());
    set_visibility(&router, DEMO_TOKEN, &public.id, Visibility::Public).await;
    assert_eq!(public_flights(&router).await.len(), 5);
    let not_suspended = Action::ResumePublishing {
        handle: "demo".to_owned(),
    };
    let not_suspended = operator(&store, &clock, not_suspended).await;
    assert!(
        not_suspended
            .said
            .contains("was not suspended from publishing")
    );

    // A handle nobody has, and text that is none.
    for unknown in ["nobody", "x' OR '1'='1", ""] {
        for action in [
            Action::SuspendPublishing {
                handle: unknown.to_owned(),
            },
            Action::ResumePublishing {
                handle: unknown.to_owned(),
            },
        ] {
            let outcome = operator(&store, &clock, action).await;
            assert!(!outcome.found, "{unknown:?}: {}", outcome.said);
            assert!(outcome.said.contains("There is no account with the handle"));
        }
    }
}

// --- Removing a live track -------------------------------------------------------------

#[tokio::test]
async fn a_live_track_whoever_runs_the_server_removed_is_gone_for_good() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let path = format!("/v1/live/{DEMO_LIVE_FLYING}");
    assert_eq!(get(&router, &path).await.status, StatusCode::OK);
    let nothing = observable(&get(&router, &format!("/v1/live/{}", "c".repeat(26))).await);

    // By the link that was reported, with what a browser adds to it.
    let link = format!("{PUBLIC_URL}/live/{DEMO_LIVE_FLYING}?renew=off");
    let removed = operator(&store, &clock, Action::RemoveLiveTrack { track: link }).await;
    assert!(removed.found, "{}", removed.said);
    assert!(removed.said.starts_with(
        "The live track of demo (Demo Pilot) is removed with its 1201 positions. It was on \
         everyone's map."
    ));
    assert!(
        removed
            .said
            .contains("venti-server suspend-publishing demo")
    );

    // It answers as a track that never was, to everyone, and it is on no
    // map.
    assert_eq!(observable(&get(&router, &path).await), nothing);
    let page = get(&router, &format!("/live/{DEMO_LIVE_FLYING}")).await;
    assert_eq!(page.status, StatusCode::NOT_FOUND);
    assert_eq!(gliders_on_the_map(&router).await.len(), 1);
    // The pilot's app is told that the identifier is used up.
    let more = positions(DEMO_LIVE_FLYING, LiveVisibility::Public, &[0]);
    send_positions(&router, DEMO_TOKEN, &more)
        .await
        .error(StatusCode::GONE, ErrorCode::LiveTrackGone);
    // A track shared by link is removed the same way.
    let by_link = Action::RemoveLiveTrack {
        track: DEMO_LIVE_LINK.to_owned(),
    };
    let removed = operator(&store, &clock, by_link).await;
    assert!(removed.said.contains("It was for those with its link."));

    // There is nothing to remove twice, and what is no identifier is said
    // to be none.
    for unknown in [
        DEMO_LIVE_FLYING.to_owned(),
        "c".repeat(26),
        "demo".to_owned(),
    ] {
        let action = Action::RemoveLiveTrack {
            track: unknown.clone(),
        };
        let outcome = operator(&store, &clock, action).await;
        assert!(!outcome.found, "{unknown:?}: {}", outcome.said);
    }
}

// --- A store in trouble ---------------------------------------------------------------

#[tokio::test]
async fn a_database_that_is_down_is_said_to_be_down() {
    let clock = TestClock::new();
    let store = Arc::new(TroubledStore::down());
    let api = Api::new(
        store.clone(),
        Arc::new(clock.clone()),
        std::iter::empty(),
        settings(),
    );
    let router = api.router(&Cors::Any);
    let token = format!("venti_{}", "7".repeat(52));
    let no_flight: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
    let answer = report(
        &router,
        &token,
        &about_flight(&no_flight, ReportReason::Spam, None),
    )
    .await;
    answer.error(StatusCode::SERVICE_UNAVAILABLE, ErrorCode::Unavailable);

    // The operator's commands say so too, and claim nothing.
    for action in [
        Action::Reports { all: false },
        Action::TakeDown {
            flight: no_flight.to_string(),
        },
        Action::SuspendPublishing {
            handle: "demo".to_owned(),
        },
        Action::RemoveLiveTrack {
            track: "c".repeat(26),
        },
    ] {
        let failed = perform(&*store, &clock, site(), &action).await.unwrap_err();
        assert!(failed.starts_with("the database failed"), "{failed}");
    }
    // The development mode's server, by contrast, answers.
    let router = recorded().await;
    assert_eq!(get(&router, "/v1/health").await.status, StatusCode::OK);
}

// --- The operator's guide ---------------------------------------------------------------

#[test]
fn the_operators_guide_shows_the_commands_as_they_are() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/README.md");
    let guide = std::fs::read_to_string(path).unwrap();
    // What `reports` prints is in the guide as the tests keep it.
    let printed = std::fs::read_to_string(golden("operator_reports.txt")).unwrap();
    assert!(
        guide.contains(printed.trim_end()),
        "deploy/README.md does not show operator_reports.txt as the tests keep it; copy the \
         file from crates/server/tests/golden into the document"
    );
    // Every command the program has for this is in the guide, and the
    // guide names none the program lacks.
    let commands = [
        "reports",
        "close-report",
        "take-down",
        "release",
        "remove-live-track",
        "suspend-publishing",
        "resume-publishing",
    ];
    for command in commands {
        let in_the_guide = format!("docker compose exec server venti-server {command}");
        assert!(guide.contains(&in_the_guide), "{in_the_guide}");
        let in_the_help = format!("\n  {command} ");
        assert!(venti_server::cli::USAGE.contains(&in_the_help), "{command}");
    }
    let in_the_guide = guide
        .lines()
        .filter_map(|line| line.strip_prefix("docker compose exec server venti-server "))
        .filter_map(|rest| rest.split_whitespace().next());
    for command in in_the_guide {
        let in_the_help = format!("\n  {command} ");
        assert!(
            venti_server::cli::USAGE.contains(&in_the_help),
            "the guide names `{command}`, which the program does not have"
        );
    }
}
