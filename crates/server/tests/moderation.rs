//! Reporting what others share, and what whoever runs the server does
//! about it: through the real router for what pilots do and see, and
//! through the operator's commands for the rest.
//!
//! What must not happen is tested here too: a report that tells of
//! something its reporter may not see, a group one is not in among them,
//! anything that tells a pilot or the members of a group that they were
//! reported, a flight that was taken down coming back, and an account
//! that may not publish getting something into a list.

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::{Duration, NaiveDate};
use venti_api_types::v1::{
    ACCOUNT_PATH, AccountResponse, CreateGroupRequest, CreateGroupResponse, ErrorCode,
    FLIGHT_CHANGES_PATH, FLIGHTS_PATH, Flight, FlightChangesResponse, FlightId, FlightResponse,
    FlightsResponse, GROUP_JOIN_PATH, GROUPS_PATH, GroupId, GroupInviteCode, GroupResponse,
    GroupsResponse, Handle, JoinGroupRequest, JoinGroupResponse, LIVE_POSITIONS_PATH, LiveFix,
    LivePositionsRequest, LivePositionsResponse, LiveTracksResponse, LiveVisibility,
    MAX_REPORT_TEXT_LENGTH, PLAN_BOARD_PATH, PUBLIC_FLIGHTS_PATH, PilotResponse, Plan,
    PlanBoardResponse, PlanRequest, PlanResponse, PlansResponse, REPORTS_PATH, ReportReason,
    ReportRequest, ReportResponse, ReportSubject, ReportedPlan, ReportedSite, SESSIONS_PATH,
    SignInResponse, UpdateFlightRequest, UpdateFlightsRequest, UpdateGroupRequest, Visibility,
    flight_file_path, flight_path, group_member_path, group_path, group_plans_path, plan_in_path,
    plan_path, share_image_path, share_path,
};
use venti_domain::{GeoPoint, SiteId, SiteUse, WindSector};
use venti_server::accounts::Limits;
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::Clock;
use venti_server::dev::{
    DEMO_GROUP, DEMO_GROUP_CODE, DEMO_GROUP_NAME, DEMO_HANDLE, DEMO_LIVE_FLYING, DEMO_LIVE_LINK,
    DEMO_OTHER_GROUP, DEMO_OTHER_PLAN, DEMO_PASSWORD, DEMO_PILOTS, DEMO_PLANS, DEMO_SITES,
    DEMO_TOKEN,
};
use venti_server::live;
use venti_server::moderation::{Action, Outcome, Site, perform};
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;
use venti_store::{
    MemoryStore, NewPilotSite, NewReport, PilotSite, ReportListing, ReportSaved, SiteAct,
    SiteRevising, SiteVia, SiteWrite, Store,
};

mod common;

use common::{
    Answer, PUBLIC_URL, TestClock, ask, assert_golden_text, get, golden, recorded,
    recorded_with_its_store, sample_file, sample_file_on, server, set_visibility, settings,
    sign_up, sign_up_private, try_sign_in, upload,
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
        plan_days: 2,
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
        .json(&UpdateFlightRequest {
            visibility: Some(visibility),
            ..UpdateFlightRequest::default()
        })
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
        // What a token or an address has left of its budget says how often
        // it asked, and nothing of a report.
        .filter(|(name, _)| !name.as_str().starts_with("ratelimit"))
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
    assert_eq!(first.about.as_ref().unwrap().handle, mara);
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
            .all(|report| report.about.as_ref().unwrap().handle == DEMO_HANDLE)
    );
    assert_eq!(public_flights(&router).await.len(), 11);
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
    let dario = sign_up_private(&router, "dario").await.token;
    let anna = sign_up_private(&router, "anna").await.token;
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

    // Five reports with identifiers of this test's choosing, so that what
    // is printed is the same every time: about a flight, a pilot's page, a
    // live track, a group and a plan. The last two carry the words that
    // were reported, as the endpoint puts them there.
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
    let plan = store
        .any_plan(DEMO_PLANS[1], NaiveDate::MIN)
        .await
        .unwrap()
        .unwrap();
    let reports = [
        NewReport {
            id: "0000000000000000000000rep1".to_owned(),
            created_at: made_at - Duration::hours(26),
            reporter_id: demo.clone(),
            about_account_id: Some(mara.clone()),
            subject: venti_store::ReportSubject::Flight(flight.to_string()),
            reason: "not_a_flight".to_owned(),
            text: Some("I flew there that day.\nThis is my track, not hers.".to_owned()),
        },
        NewReport {
            id: "0000000000000000000000rep2".to_owned(),
            created_at: made_at - Duration::hours(3),
            reporter_id: jonas,
            about_account_id: Some(mara),
            subject: venti_store::ReportSubject::Pilot,
            reason: "impersonation".to_owned(),
            text: None,
        },
        NewReport {
            id: "0000000000000000000000rep3".to_owned(),
            created_at: made_at - Duration::minutes(10),
            reporter_id: account("mara").await,
            about_account_id: Some(demo.clone()),
            subject: venti_store::ReportSubject::LiveTrack(DEMO_LIVE_FLYING.to_owned()),
            reason: "other".to_owned(),
            text: Some("Nobody is flying there. It is night.".to_owned()),
        },
        NewReport {
            id: "0000000000000000000000rep4".to_owned(),
            created_at: made_at - Duration::minutes(6),
            reporter_id: account("jonas").await,
            about_account_id: Some(demo.clone()),
            subject: venti_store::ReportSubject::Group {
                id: DEMO_GROUP.to_owned(),
                name: DEMO_GROUP_NAME.to_owned(),
            },
            reason: "impersonation".to_owned(),
            text: Some("This is not the club's group. The club has none here.".to_owned()),
        },
        NewReport {
            id: "0000000000000000000000rep5".to_owned(),
            created_at: made_at - Duration::minutes(2),
            reporter_id: demo,
            about_account_id: Some(account("mara").await),
            subject: venti_store::ReportSubject::Plan {
                id: plan.id.clone(),
                group_id: plan.group_id.clone(),
                place: plan.details.place.clone(),
                note: plan.details.note.clone(),
            },
            reason: "spam".to_owned(),
            text: Some("An advertisement, not a plan.".to_owned()),
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
        "5 reports are open, oldest first.",
        "Report 0000000000000000000000rep1, open, made 2026-11-17 17:05 UTC",
        &format!("flight {flight} of mara (Mara Muster); now public"),
        &format!("https://venti.example/f/{flight}"),
        "  Reason:       not_a_flight",
        "  Text:         I flew there that day.\n                This is my track, not hers.",
        "  Reported by:  demo",
        "the page of mara (Mara Muster)",
        "live track 11111111111111111111111111 of demo (Demo Pilot); now on everyone's map",
        "https://venti.example/live/11111111111111111111111111",
        // A group and a plan: what was reported, and what it is now.
        "group 44444444444444444444444444, which demo (Demo Pilot) kept when it was reported",
        "  Name then:    Mustergruppe\n  Now:          the same name; kept by demo (Demo Pilot), 3 members",
        "plan 77777777777777777777777777 of mara (Mara Muster), in group 44444444444444444444444444",
        "  Place then:   Beispielwiese\n  Note then:    A made-up plan. Ground handling if the wind allows, coffee if not.",
        "  Now:          the same words, for 2026-11-19 at 09:00; its group is called \"Mustergruppe\"",
        "  Reported by:  jonas",
        "rename-group, remove-plan, remove-group, remove-comment and close-report",
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
    assert!(open.said.starts_with("4 reports are open"));
    assert!(!open.said.contains(report1));
    let all = operator(&store, &clock, Action::Reports { all: true }).await;
    assert!(
        all.said
            .starts_with("5 reports, 4 of them open, oldest first.")
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
        id: "0000000000000000000000rep6".to_owned(),
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
    assert_eq!(public_flights(&router).await.len(), 11);

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
    assert_eq!(public_flights(&router).await.len(), 10);
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
    // Not all at once either, and nothing changes.
    let refused = ask(Method::PATCH, FLIGHTS_PATH)
        .token(DEMO_TOKEN)
        .json(&UpdateFlightsRequest {
            visibility: Visibility::Public,
        })
        .send(&router)
        .await;
    let said = refused.error(StatusCode::FORBIDDEN, ErrorCode::PublishingSuspended);
    assert!(said.contains("nothing was changed"), "{said}");
    assert_eq!(
        own_flight(&router, DEMO_TOKEN, &unlisted.id).await,
        *unlisted
    );
    // Everything else works: sharing by link, taking back, uploading. An
    // upload is private, whatever the account's setting says.
    set_visibility(&router, DEMO_TOKEN, &private.id, Visibility::Unlisted).await;
    set_visibility(&router, DEMO_TOKEN, &private.id, Visibility::Private).await;
    assert!(account.account.new_flights_public);
    let uploaded = upload(&router, DEMO_TOKEN, &sample_file_on("010926")).await;
    assert!(uploaded.created);
    assert_eq!(uploaded.flight.visibility, Visibility::Private);

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
    assert_eq!(public_flights(&router).await.len(), 10);
    assert!(gliders_on_the_map(&router).await.is_empty());
    set_visibility(&router, DEMO_TOKEN, &public.id, Visibility::Public).await;
    assert_eq!(public_flights(&router).await.len(), 11);
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

// --- Reporting a group and a plan ---------------------------------------------------------

/// An identifier of the right form that nothing has.
const NOBODYS: &str = "0123456789abcdefghjkmnpqrs";

fn about_group(group: &str) -> ReportRequest {
    about(ReportSubject::Group(group.to_owned()))
}

fn about_plan(group: &str, plan: &str) -> ReportRequest {
    about(ReportSubject::Plan(ReportedPlan {
        group: group.to_owned(),
        id: plan.to_owned(),
    }))
}

/// Makes a group and returns it with its invite code.
async fn make_group(router: &Router, token: &str, name: &str) -> CreateGroupResponse {
    let request = CreateGroupRequest {
        name: name.to_owned(),
    };
    ask(Method::POST, GROUPS_PATH)
        .token(token)
        .json(&request)
        .send(router)
        .await
        .ok(StatusCode::CREATED)
}

async fn try_join(router: &Router, token: &str, code: &str) -> Answer {
    let request = JoinGroupRequest {
        code: GroupInviteCode::new(code),
    };
    ask(Method::POST, GROUP_JOIN_PATH)
        .token(token)
        .json(&request)
        .send(router)
        .await
}

async fn come_in(router: &Router, token: &str, code: &GroupInviteCode) {
    let joined: JoinGroupResponse = try_join(router, token, code.expose())
        .await
        .ok(StatusCode::CREATED);
    assert!(joined.joined);
}

/// A plan for a day of November 2026.
fn a_plan(day: u32, place: &str, note: Option<&str>) -> PlanRequest {
    PlanRequest {
        day: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
        place: place.to_owned(),
        position: None,
        meet_at: None,
        note: note.map(str::to_owned),
    }
}

async fn post_plan(router: &Router, token: &str, group: &GroupId, plan: &PlanRequest) -> Plan {
    let posted: PlanResponse = ask(Method::POST, &group_plans_path(group))
        .token(token)
        .json(plan)
        .send(router)
        .await
        .ok(StatusCode::CREATED);
    posted.plan
}

async fn seen_group(router: &Router, token: &str, group: &GroupId) -> GroupResponse {
    ask(Method::GET, &group_path(group))
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK)
}

/// The identifiers of a group's plans, as a member is given them from the
/// first of November on.
async fn plans_of(router: &Router, token: &str, group: &GroupId) -> Vec<String> {
    let path = format!("{}?from=2026-11-01", group_plans_path(group));
    let listed: PlansResponse = ask(Method::GET, &path)
        .token(token)
        .send(router)
        .await
        .ok(StatusCode::OK);
    listed
        .plans
        .into_iter()
        .map(|plan| plan.id.to_string())
        .collect()
}

async fn sign_in(router: &Router, handle: &str) -> String {
    let signed_in: SignInResponse = try_sign_in(router, handle, DEMO_PASSWORD)
        .await
        .ok(StatusCode::CREATED);
    signed_in.token
}

fn demo_group() -> GroupId {
    DEMO_GROUP.parse().unwrap()
}

#[tokio::test]
async fn a_member_reports_a_group_and_a_plan_of_it() {
    let (router, store) = recorded_with_its_store().await;
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);
    let jonas_token = sign_in(&router, jonas).await;
    let group = demo_group();
    let before = (
        seen_group(&router, DEMO_TOKEN, &group).await,
        plans_of(&router, DEMO_TOKEN, &group).await,
    );

    // Jonas reports the group, which the demo account keeps. This is an
    // example of the documentation.
    let request = ReportRequest {
        subject: ReportSubject::Group(DEMO_GROUP.to_owned()),
        reason: ReportReason::Impersonation,
        text: Some("This is not the club's group. The club has none here.".to_owned()),
    };
    assert_golden(&golden("report_group_request.json"), &request);
    let answer = report(&router, &jonas_token, &request).await;
    let stored: ReportResponse = answer.ok(StatusCode::CREATED);
    assert!(stored.created);
    answer.is_private();
    // The demo account reports a plan of Mara's: the other example.
    let request = ReportRequest {
        subject: ReportSubject::Plan(ReportedPlan {
            group: DEMO_GROUP.to_owned(),
            id: DEMO_PLANS[1].to_owned(),
        }),
        reason: ReportReason::Spam,
        text: Some("An advertisement, not a plan.".to_owned()),
    };
    assert_golden(&golden("report_plan_request.json"), &request);
    let answer = report(&router, DEMO_TOKEN, &request).await;
    assert!(answer.ok::<ReportResponse>(StatusCode::CREATED).created);
    answer.is_private();

    // Either reported again is the same report, whatever it says this time.
    for (token, again) in [
        (jonas_token.as_str(), about_group(DEMO_GROUP)),
        (DEMO_TOKEN, about_plan(DEMO_GROUP, DEMO_PLANS[1])),
    ] {
        let known: ReportResponse = report(&router, token, &again).await.ok(StatusCode::OK);
        assert!(!known.created);
    }
    // A keeper may report the group they keep and an author their own
    // plan: each is one more report to read.
    for own in [
        about_group(DEMO_GROUP),
        about_plan(DEMO_GROUP, DEMO_PLANS[0]),
    ] {
        report(&router, DEMO_TOKEN, &own)
            .await
            .ok::<ReportResponse>(StatusCode::CREATED);
    }

    // What waits for whoever runs the server: the group with its name and
    // the account that keeps it, the plan with its words and its author.
    // Nothing else of either was kept.
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 4);
    let of = |reporter: &str, kind: &str| {
        reports
            .iter()
            .find(|report| report.reporter_handle == reporter && report.subject.kind() == kind)
            .unwrap_or_else(|| panic!("no report of {reporter} about a {kind}"))
    };
    let about_the_group = of(jonas, "group");
    assert_eq!(about_the_group.about.as_ref().unwrap().handle, DEMO_HANDLE);
    assert_eq!(
        about_the_group.subject,
        venti_store::ReportSubject::Group {
            id: DEMO_GROUP.to_owned(),
            name: DEMO_GROUP_NAME.to_owned(),
        }
    );
    assert_eq!(about_the_group.reason, "impersonation");
    let about_the_plan = reports
        .iter()
        .find(|report| report.subject.id() == DEMO_PLANS[1])
        .unwrap();
    assert_eq!(about_the_plan.reporter_handle, DEMO_HANDLE);
    assert_eq!(about_the_plan.about.as_ref().unwrap().handle, mara);
    assert_eq!(
        about_the_plan.subject,
        venti_store::ReportSubject::Plan {
            id: DEMO_PLANS[1].to_owned(),
            group_id: DEMO_GROUP.to_owned(),
            place: "Beispielwiese".to_owned(),
            note: Some(
                "A made-up plan. Ground handling if the wind allows, coffee if not.".to_owned()
            ),
        }
    );
    assert_eq!(
        about_the_plan.text.as_deref(),
        Some("An advertisement, not a plan.")
    );
    // And nothing else happened: the group and its plans are as they were.
    let after = (
        seen_group(&router, DEMO_TOKEN, &group).await,
        plans_of(&router, DEMO_TOKEN, &group).await,
    );
    assert_eq!(after, before);

    // A group and a plan are words and no flight: the reason about a
    // flight is none of theirs, nor is a reason nobody knows. The refusal
    // is the same whether there is such a group or not.
    for subject in [
        ReportSubject::Group(DEMO_GROUP.to_owned()),
        ReportSubject::Group(NOBODYS.to_owned()),
        ReportSubject::Plan(ReportedPlan {
            group: DEMO_GROUP.to_owned(),
            id: DEMO_PLANS[2].to_owned(),
        }),
    ] {
        let wrong = ReportRequest {
            subject,
            reason: ReportReason::NotAFlight,
            text: None,
        };
        let said = report(&router, DEMO_TOKEN, &wrong)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
        assert_eq!(
            said,
            "reason must be one of offensive, impersonation, privacy, spam, other"
        );
    }
    let unknown = format!(r#"{{"subject":{{"group":"{DEMO_GROUP}"}},"reason":"airspace"}}"#);
    let said = ask(Method::POST, REPORTS_PATH)
        .token(DEMO_TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(unknown)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    assert!(!said.contains("not_a_flight") && !said.contains(DEMO_GROUP));
    // "Other" says in its text what it is about, as for every report.
    let wordless = ReportRequest {
        reason: ReportReason::Other,
        ..about_plan(DEMO_GROUP, DEMO_PLANS[2])
    };
    report(&router, DEMO_TOKEN, &wordless)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    // Not the JSON a report is: a plan without its group, and a group
    // that is more than an identifier.
    for text in [
        format!(
            r#"{{"subject":{{"plan":"{}"}},"reason":"spam"}}"#,
            DEMO_PLANS[1]
        ),
        format!(
            r#"{{"subject":{{"plan":{{"id":"{}"}}}},"reason":"spam"}}"#,
            DEMO_PLANS[1]
        ),
        format!(r#"{{"subject":{{"group":["{DEMO_GROUP}"]}},"reason":"spam"}}"#),
    ] {
        let said = ask(Method::POST, REPORTS_PATH)
            .token(DEMO_TOKEN)
            .header(header::CONTENT_TYPE, "application/json")
            .bytes(text)
            .send(&router)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
        assert!(!said.contains(DEMO_PLANS[1]) && !said.contains(DEMO_GROUP));
    }
    let open = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(open.len(), 4);
}

#[tokio::test]
async fn a_group_one_is_not_in_is_not_there_to_report() {
    let (router, store, clock) = open_server();
    let keeper = sign_up(&router, "keeper").await.token;
    let member = sign_up(&router, "member").await.token;
    let outsider = sign_up(&router, "outsider").await.token;
    let removed = sign_up(&router, "removed").await.token;
    let leaver = sign_up(&router, "leaver").await.token;
    let made = make_group(&router, &keeper, "Mustergruppe").await;
    for token in [&member, &removed, &leaver] {
        come_in(&router, token, &made.invite_code).await;
    }
    let group = made.group.id.clone();
    let plan = post_plan(
        &router,
        &member,
        &group,
        &a_plan(19, "Musterberg", Some("Bring a radio.")),
    )
    .await;
    // A group of the outsider's own, with a plan: another group's plan.
    let others = make_group(&router, &outsider, "Beispielgruppe").await;
    let elsewhere = post_plan(
        &router,
        &outsider,
        &others.group.id,
        &a_plan(19, "Beispielwiese", None),
    )
    .await;
    // One was in the group and was removed, one was in it and left.
    for (token, handle) in [(&keeper, "removed"), (&leaver, "leaver")] {
        let path = group_member_path(&group, &Handle::new(handle).unwrap());
        let gone = ask(Method::DELETE, &path).token(token).send(&router).await;
        assert_eq!(gone.status, StatusCode::NO_CONTENT);
    }

    for (who, token) in [
        ("an outsider", &outsider),
        ("a removed member", &removed),
        ("a member who left", &leaver),
    ] {
        // The group answers a report exactly as an identifier nothing
        // has: in status, in every header and in every byte. Reporting is
        // no way to find a group.
        let for_the_group = report(&router, token, &about_group(group.as_str())).await;
        let for_no_group = report(&router, token, &about_group(NOBODYS)).await;
        let said = for_the_group.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
        assert!(
            !said.contains(group.as_str()) && !said.contains("Mustergruppe"),
            "{said}"
        );
        assert_eq!(
            observable(&for_the_group),
            observable(&for_no_group),
            "{who}"
        );
        // It is the answer reading the group gives them.
        let reading = ask(Method::GET, &group_path(&group))
            .token(token)
            .send(&router)
            .await;
        assert_eq!(for_the_group.body, reading.body, "{who}");
        // And a plan answers as the group does, whether there is such a
        // plan or not: who is not in a group never learns of one.
        for (in_group, named) in [
            (group.as_str(), plan.id.as_str()),
            (group.as_str(), NOBODYS),
            (group.as_str(), "x"),
            (NOBODYS, plan.id.as_str()),
            (NOBODYS, NOBODYS),
        ] {
            let answer = report(&router, token, &about_plan(in_group, named)).await;
            assert_eq!(
                observable(&answer),
                observable(&for_no_group),
                "{who}: a plan {named} of {in_group}"
            );
        }
    }
    // The comparison means something: to a member the group is there, and
    // a member is told when it has no such plan. A plan of another group
    // is no plan of this one, also for someone who is in both.
    come_in(&router, &member, &others.invite_code).await;
    let no_plan = ask(
        Method::PUT,
        &plan_in_path(&group, &NOBODYS.parse().unwrap()),
    )
    .token(&member)
    .send(&router)
    .await;
    for nothing in [NOBODYS, "x", "", "../account", elsewhere.id.as_str()] {
        let answer = report(&router, &member, &about_plan(group.as_str(), nothing)).await;
        let said = answer.error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
        assert!(!said.contains("account"), "{said}");
        assert_eq!(
            answer.body, no_plan.body,
            "as the plan's own address answers"
        );
    }
    // Text that is no identifier is the identifier of no group.
    for odd in [
        "",
        "x",
        "../account",
        "x' OR '1'='1",
        &NOBODYS.to_uppercase(),
    ] {
        let answer = report(&router, &member, &about_group(odd)).await;
        let said = answer.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
        assert!(!said.contains("account") && !said.contains("OR"), "{said}");
    }
    let kept = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert!(kept.is_empty(), "none of that is a report");

    // A member reports the group and the plan, a keeper too.
    for (token, request) in [
        (&member, about_group(group.as_str())),
        (&member, about_plan(group.as_str(), plan.id.as_str())),
        (&keeper, about_plan(group.as_str(), plan.id.as_str())),
    ] {
        report(&router, token, &request)
            .await
            .ok::<ReportResponse>(StatusCode::CREATED);
    }
    // Whoever comes in with the code is a member, and reports like one.
    let code: venti_api_types::v1::GroupInviteResponse = ask(
        Method::POST,
        &venti_api_types::v1::group_invite_path(&group),
    )
    .token(&keeper)
    .send(&router)
    .await
    .ok(StatusCode::OK);
    come_in(&router, &outsider, &code.invite_code).await;
    report(&router, &outsider, &about_group(group.as_str()))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    // Who leaves is told nothing from then on, of the group or of the
    // report they made.
    let path = group_member_path(&group, &Handle::new("member").unwrap());
    let left = ask(Method::DELETE, &path)
        .token(&member)
        .send(&router)
        .await;
    assert_eq!(left.status, StatusCode::NO_CONTENT);
    let again = report(&router, &member, &about_group(group.as_str())).await;
    again.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    let nothing = report(&router, &member, &about_group(NOBODYS)).await;
    assert_eq!(observable(&again), observable(&nothing));

    // A plan whose day lies further back than plans are kept is gone,
    // also for a report: a plan for the 19th on the 22nd. The member who
    // posted it left, and it went with them; another stands for it.
    let old = post_plan(&router, &keeper, &group, &a_plan(19, "Musterberg", None)).await;
    clock.advance(Duration::days(4));
    let answer = report(
        &router,
        &outsider,
        &about_plan(group.as_str(), old.id.as_str()),
    )
    .await;
    answer.error(StatusCode::NOT_FOUND, ErrorCode::PlanNotFound);
    let reports = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(reports.len(), 4);
}

#[tokio::test]
async fn a_report_about_a_group_counts_like_any_other() {
    let clock = TestClock::new();
    let strict = Settings {
        limits: Limits {
            reports_per_day: 4,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, store) = server(strict, &clock);
    let anna = sign_up(&router, "anna").await.token;
    let dario = sign_up(&router, "dario").await.token;
    let made = make_group(&router, &anna, "Mustergruppe").await;
    let group = made.group.id.clone();
    let plan = post_plan(&router, &anna, &group, &a_plan(19, "Musterberg", None)).await;

    // Four a day, whatever becomes of them: a group one is not in, a plan
    // of it, an identifier nothing has, and a reason that is none for a
    // group. Trying identifiers here is no cheaper than anywhere else.
    report(&router, &dario, &about_group(group.as_str()))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    report(
        &router,
        &dario,
        &about_plan(group.as_str(), plan.id.as_str()),
    )
    .await
    .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    report(&router, &dario, &about_group(NOBODYS))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
    let wrong = ReportRequest {
        reason: ReportReason::NotAFlight,
        ..about_group(group.as_str())
    };
    report(&router, &dario, &wrong)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    // The fifth is refused before it is read, and says nothing of the
    // group: the answer is the same for a group that is there and one
    // that is not.
    let refused = report(&router, &dario, &about_group(group.as_str())).await;
    refused.error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    assert_eq!(refused.header(header::RETRY_AFTER), "86400");
    let nowhere = report(&router, &dario, &about_group(NOBODYS)).await;
    assert_eq!(observable(&refused), observable(&nowhere));
    // Coming into the group does not give the day back.
    come_in(&router, &dario, &made.invite_code).await;
    report(&router, &dario, &about_group(group.as_str()))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    // A member's own day is their own, and holds as many.
    for (request, status) in [
        (about_group(group.as_str()), StatusCode::CREATED),
        (about_group(group.as_str()), StatusCode::OK),
        (
            about_plan(group.as_str(), plan.id.as_str()),
            StatusCode::CREATED,
        ),
        (about_plan(group.as_str(), NOBODYS), StatusCode::NOT_FOUND),
    ] {
        assert_eq!(report(&router, &anna, &request).await.status, status);
    }
    report(&router, &anna, &about_group(group.as_str()))
        .await
        .error(StatusCode::TOO_MANY_REQUESTS, ErrorCode::RateLimited);
    let kept = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(kept.len(), 2);
    // The next day Dario, who is in the group now, reports it.
    clock.advance(Duration::days(1) + Duration::seconds(1));
    report(&router, &dario, &about_group(group.as_str()))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
}

#[tokio::test]
async fn nothing_tells_the_members_of_a_group_that_it_was_reported() {
    let (router, store, clock) = open_server();
    let anna = sign_up(&router, "anna").await.token;
    let bernd = sign_up(&router, "bernd").await.token;
    let dario = sign_up(&router, "dario").await.token;
    let made = make_group(&router, &anna, "Mustergruppe").await;
    for token in [&bernd, &dario] {
        come_in(&router, token, &made.invite_code).await;
    }
    let group = made.group.id.clone();
    let plan = post_plan(
        &router,
        &bernd,
        &group,
        &a_plan(19, "Musterberg", Some("Bring a radio.")),
    )
    .await;

    // Everything the group's keeper and the plan's author can ask the
    // server, each with their own token.
    let mut questions: Vec<(String, &str)> = Vec::new();
    for token in [anna.as_str(), bernd.as_str()] {
        for path in [
            ACCOUNT_PATH.to_owned(),
            SESSIONS_PATH.to_owned(),
            FLIGHTS_PATH.to_owned(),
            FLIGHT_CHANGES_PATH.to_owned(),
            GROUPS_PATH.to_owned(),
            group_path(&group),
            group_plans_path(&group),
            format!("{}?from=2026-11-01", group_plans_path(&group)),
            PLAN_BOARD_PATH.to_owned(),
            format!("{PLAN_BOARD_PATH}?from=2026-11-19&to=2026-11-19"),
        ] {
            questions.push((path, token));
        }
    }
    let ask_all = || async {
        let mut answers = Vec::new();
        for (path, token) in &questions {
            let answer = ask(Method::GET, path).token(token).send(&router).await;
            assert_eq!(answer.status, StatusCode::OK, "{path}");
            answers.push(observable(&answer));
        }
        answers
    };
    let before = ask_all().await;

    // Dario reports the group and the plan, in words that stand nowhere
    // else.
    for subject in [
        ReportSubject::Group(group.to_string()),
        ReportSubject::Plan(ReportedPlan {
            group: group.to_string(),
            id: plan.id.to_string(),
        }),
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
    let open = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(open.len(), 2);

    // Every answer is what it was, in status, headers and body: nothing
    // says that there is a report, let alone whose.
    let after = ask_all().await;
    for ((path, _), (before, after)) in questions.iter().zip(before.iter().zip(&after)) {
        assert_eq!(before, after, "{path}");
        let body = String::from_utf8_lossy(&after.2);
        for told in ["Zanzibar", "impersonation", "report"] {
            assert!(!body.contains(told), "{path} says {told:?}");
        }
    }
    // What they do in the group is answered as before as well: the author
    // says that they are in, and takes it back.
    for method in [Method::DELETE, Method::PUT] {
        let answer = ask(method, &plan_in_path(&group, &plan.id))
            .token(&bernd)
            .send(&router)
            .await;
        let now: PlanResponse = answer.ok(StatusCode::OK);
        assert_eq!(now.plan.place, "Musterberg");
    }
    // Closing the reports tells nobody either.
    for listed in open {
        let closing = Action::CloseReport {
            id: listed.id,
            note: "Looked at it. Nothing to do.".to_owned(),
        };
        assert!(operator(&store, &clock, closing).await.found);
    }
    assert_eq!(ask_all().await, before);
}

// --- What whoever runs the server does about a group ------------------------------------

#[tokio::test]
async fn the_reports_say_what_was_reported_also_when_it_is_changed_or_gone() {
    let (router, store, clock) = open_server();
    let anna = sign_up(&router, "anna").await.token;
    let bernd = sign_up(&router, "bernd").await.token;
    let dario = sign_up(&router, "dario").await.token;
    let made = make_group(&router, &anna, "Zanzibar flyers").await;
    for token in [&bernd, &dario] {
        come_in(&router, token, &made.invite_code).await;
    }
    let group = made.group.id.clone();
    let words = a_plan(
        19,
        "Marmalade hill",
        Some("Bring the quotient.\nAnd a radio."),
    );
    let plan = post_plan(&router, &bernd, &group, &words).await;
    report(&router, &dario, &about_group(group.as_str()))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    clock.advance(Duration::minutes(1));
    let about_the_plan = about_plan(group.as_str(), plan.id.as_str());
    report(&router, &dario, &about_the_plan)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    let listed = || async {
        operator(&store, &clock, Action::Reports { all: false })
            .await
            .said
    };
    let says = |said: &str, expected: &[&str]| {
        for expected in expected {
            assert!(said.contains(expected), "{expected}\n{said}");
        }
    };

    // As reported: the words, who they are whose, and that they still
    // stand.
    let said = listed().await;
    says(
        &said,
        &[
            &format!("group {group}, which anna (Pilot anna) kept when it was reported"),
            "  Name then:    Zanzibar flyers\n  Now:          the same name; kept by anna (Pilot anna), 3 members",
            &format!("plan {} of bernd (Pilot bernd), in group {group}", plan.id),
            "  Place then:   Marmalade hill\n  Note then:    Bring the quotient.\n                And a radio.\n  Now:          the same words, for 2026-11-19; its group is called \"Zanzibar flyers\"",
            "  Reported by:  dario",
        ],
    );
    assert!(!said.contains("Place now:"), "{said}");

    // The keeper gives the group another name and hands it on, and the
    // author makes the plan say something else. The reports keep what was
    // reported, and say beside it what stands there now.
    let renamed = ask(Method::PATCH, &group_path(&group))
        .token(&anna)
        .json(&UpdateGroupRequest {
            name: "Harmless".to_owned(),
        })
        .send(&router)
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    let handed = ask(Method::PUT, &venti_api_types::v1::group_keeper_path(&group))
        .token(&anna)
        .json(&venti_api_types::v1::HandOverGroupRequest {
            handle: "bernd".to_owned(),
        })
        .send(&router)
        .await;
    assert_eq!(handed.status, StatusCode::OK);
    let changed = ask(Method::PUT, &plan_path(&group, &plan.id))
        .token(&bernd)
        .json(&a_plan(20, "Elsewhere", None))
        .send(&router)
        .await;
    assert_eq!(changed.status, StatusCode::OK);
    let said = listed().await;
    says(
        &said,
        &[
            &format!("group {group}, which anna (Pilot anna) kept when it was reported"),
            "  Name then:    Zanzibar flyers\n  Now:          called \"Harmless\"; kept by bernd (Pilot bernd), 3 members",
            "  Place then:   Marmalade hill\n  Note then:    Bring the quotient.\n                And a radio.\n  Now:          other words, for 2026-11-20; its group is called \"Harmless\"\n  Place now:    Elsewhere\n  Reason:",
        ],
    );
    assert!(!said.contains("Note now:"), "{said}");
    // A note that it has now is shown as well.
    let noted = ask(Method::PUT, &plan_path(&group, &plan.id))
        .token(&bernd)
        .json(&a_plan(20, "Elsewhere", Some("Nothing to see.")))
        .send(&router)
        .await;
    assert_eq!(noted.status, StatusCode::OK);
    says(
        &listed().await,
        &["  Place now:    Elsewhere\n  Note now:     Nothing to see.\n  Reason:"],
    );

    // The author deletes the plan, and then the keeper the group. The
    // words that were reported are still there to read.
    let deleted = ask(Method::DELETE, &plan_path(&group, &plan.id))
        .token(&bernd)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    says(
        &listed().await,
        &[
            "  Note then:    Bring the quotient.\n                And a radio.\n  Now:          gone; its group is called \"Harmless\"",
        ],
    );
    let deleted = ask(Method::DELETE, &group_path(&group))
        .token(&bernd)
        .send(&router)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    let said = listed().await;
    assert!(said.starts_with("2 reports are open"), "{said}");
    says(
        &said,
        &[
            "  Name then:    Zanzibar flyers\n  Now:          gone\n",
            "  Place then:   Marmalade hill",
            "  Now:          gone; its group is gone\n",
        ],
    );

    // What a pilot typed is printed without anything a terminal acts on.
    // The API lets none of it in; a report that holds it is made past it.
    let account = |handle: &'static str| {
        let store = store.clone();
        async move { store.account(handle).await.unwrap().unwrap().id }
    };
    let hostile = [
        venti_store::ReportSubject::Group {
            id: "g".repeat(26),
            name: "first\u{1b}[2J\rname".to_owned(),
        },
        venti_store::ReportSubject::Plan {
            id: "p".repeat(26),
            group_id: "g\u{7}".repeat(13),
            place: "a\u{1b}place".to_owned(),
            note: Some("one\u{9b}\ntwo\u{7}".to_owned()),
        },
    ];
    for (number, subject) in hostile.into_iter().enumerate() {
        let new = NewReport {
            id: format!("0000000000000000000000bad{number}"),
            created_at: Clock::now(&clock),
            reporter_id: account("dario").await,
            about_account_id: Some(account("anna").await),
            subject,
            reason: "other".to_owned(),
            text: None,
        };
        assert_eq!(store.save_report(&new).await.unwrap(), ReportSaved::Stored);
    }
    let said = listed().await;
    says(
        &said,
        &[
            "  Name then:    first?[2J?name",
            "  Place then:   a?place\n  Note then:    one?\n                two?",
            "in group g?g?g?",
        ],
    );
    for acted_on in ['\u{1b}', '\u{7}', '\u{9b}', '\r'] {
        assert!(!said.contains(acted_on), "{acted_on:?}");
    }
}

#[tokio::test]
async fn whoever_runs_the_server_looks_at_a_group_renames_it_and_removes_from_it() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);
    let (mara, jonas) = (sign_in(&router, mara).await, sign_in(&router, jonas).await);
    let group = demo_group();
    let members = [DEMO_TOKEN, mara.as_str(), jonas.as_str()];
    // Two reports, to see what the list says of them afterwards.
    report(&router, &jonas, &about_group(DEMO_GROUP))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    report(&router, DEMO_TOKEN, &about_plan(DEMO_GROUP, DEMO_PLANS[1]))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    let before = seen_group(&router, &mara, &group).await;

    // Looking at one group: what its members see, and of who is in on a
    // plan the number.
    let look = Action::Group {
        group: format!(" {DEMO_GROUP}\n"),
    };
    let shown = operator(&store, &clock, look).await;
    assert!(shown.found, "{}", shown.said);
    assert_golden_text("operator_group.txt", &format!("{}\n", shown.said));
    for expected in [
        "Group 44444444444444444444444444, made 2026-10-19 19:05 UTC",
        "  Name:         Mustergruppe",
        "  Kept by:      demo (Demo Pilot)",
        "  Members:      3, in the order they came in\n                demo (Demo Pilot)\n                mara (Mara Muster)\n                jonas (Jonas Beispiel)",
        "  Plans:        3",
        "Plan 66666666666666666666666666, for 2026-11-18 at 10:30, posted by demo (Demo Pilot); 3 pilots are in",
        "  Place:        Musterberg, Startplatz Ost\n  Position:     47.66420, 11.79670\n  Note:         A made-up plan of the development mode.\n                At the lift at half past ten, in the air by noon.",
        "Plan 88888888888888888888888888, for 2026-11-20, posted by jonas (Jonas Beispiel); 2 pilots are in\n  Place:        Musterberg\n",
        "Nothing was changed",
    ] {
        assert!(shown.said.contains(expected), "{expected}\n{}", shown.said);
    }
    assert_eq!(seen_group(&router, &mara, &group).await, before);
    for nothing in [NOBODYS, "demo", "", DEMO_PLANS[0], "x' OR '1'='1"] {
        let look = Action::Group {
            group: nothing.to_owned(),
        };
        let outcome = operator(&store, &clock, look).await;
        assert!(!outcome.found, "{nothing:?}: {}", outcome.said);
    }

    // Another name, cleaned as a keeper's would be.
    let rename = |name: &str| Action::RenameGroup {
        group: DEMO_GROUP.to_owned(),
        name: name.to_owned(),
    };
    let renamed = operator(&store, &clock, rename("  Neuer\tName ")).await;
    assert!(renamed.found, "{}", renamed.said);
    assert!(
        renamed.said.starts_with(
            "Group 44444444444444444444444444, kept by demo (Demo Pilot), was called \
             \"Mustergruppe\" and is called \"Neuer Name\" now, for its 3 members and for \
             nobody else.\n"
        ),
        "{}",
        renamed.said
    );
    assert!(renamed.said.contains("Nobody is told"));
    assert!(renamed.said.contains("no report was closed"));
    // Every member reads the new name, where a group is named, and
    // nothing else about the group changed: nothing says who renamed it.
    let mut expected = before.clone();
    expected.group.name = "Neuer Name".to_owned();
    for token in members {
        assert_eq!(seen_group(&router, token, &group).await, expected);
        let mine: GroupsResponse = ask(Method::GET, GROUPS_PATH)
            .token(token)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert!(mine.groups.contains(&expected.group));
    }
    assert_eq!(plans_of(&router, &jonas, &group).await, DEMO_PLANS);
    // The same name again changes nothing, and what is no name is none.
    let again = operator(&store, &clock, rename("Neuer Name")).await;
    assert!(again.found);
    assert!(
        again
            .said
            .contains("is called \"Neuer Name\" already. Nothing changed.")
    );
    let too_long = "x".repeat(61);
    for no_name in ["", " \n\t ", "\u{200b}", too_long.as_str()] {
        let refused = perform(&*store, &clock, site(), &rename(no_name)).await;
        let said = refused.unwrap_err();
        assert!(said.starts_with("a group's name"), "{said}");
    }
    let nowhere = Action::RenameGroup {
        group: NOBODYS.to_owned(),
        name: "Nobody's".to_owned(),
    };
    assert!(!operator(&store, &clock, nowhere).await.found);
    // Its keeper names it again, as before.
    let back = ask(Method::PATCH, &group_path(&group))
        .token(DEMO_TOKEN)
        .json(&UpdateGroupRequest {
            name: DEMO_GROUP_NAME.to_owned(),
        })
        .send(&router)
        .await;
    assert_eq!(back.ok::<GroupResponse>(StatusCode::OK), before);

    // One plan removed, by its identifier.
    let in_on = |plan: &str| {
        let path = plan_in_path(&group, &plan.parse().unwrap());
        let (router, token) = (router.clone(), jonas.clone());
        async move { ask(Method::PUT, &path).token(&token).send(&router).await }
    };
    let never_was = observable(&in_on(NOBODYS).await);
    let remove = |plan: &str| Action::RemovePlan {
        plan: plan.to_owned(),
    };
    let removed = operator(&store, &clock, remove(DEMO_PLANS[1])).await;
    assert!(removed.found, "{}", removed.said);
    assert!(
        removed.said.starts_with(
            "The plan 77777777777777777777777777 of mara (Mara Muster) in group \
             44444444444444444444444444 is removed: for 2026-11-19, at \"Beispielwiese\"; 2 \
             pilots were in on it.\n"
        ),
        "{}",
        removed.said
    );
    assert!(removed.said.contains("This cannot be undone."));
    assert!(removed.said.contains("no report was closed"));
    // To the group's members it is a plan that is not there, as one that
    // never was: in status, in every header and in every byte.
    assert_eq!(observable(&in_on(DEMO_PLANS[1]).await), never_was);
    for token in members {
        assert_eq!(
            plans_of(&router, token, &group).await,
            [DEMO_PLANS[0], DEMO_PLANS[2]]
        );
        let board: PlanBoardResponse =
            ask(Method::GET, &format!("{PLAN_BOARD_PATH}?from=2026-11-01"))
                .token(token)
                .send(&router)
                .await
                .ok(StatusCode::OK);
        assert!(
            board
                .plans
                .iter()
                .all(|plan| plan.id.as_str() != DEMO_PLANS[1])
        );
    }
    assert_eq!(seen_group(&router, &mara, &group).await, before);
    // Its author posts again, if she likes: nothing stops that.
    post_plan(&router, &mara, &group, &a_plan(21, "Beispielwiese", None)).await;
    // There is nothing to remove twice, and what is no plan is none.
    for nothing in [DEMO_PLANS[1], NOBODYS, DEMO_GROUP, "x", ""] {
        let outcome = operator(&store, &clock, remove(nothing)).await;
        assert!(!outcome.found, "{nothing:?}: {}", outcome.said);
    }
    // A plan of another group goes by its identifier alone.
    let other = operator(&store, &clock, remove(DEMO_OTHER_PLAN)).await;
    assert!(other.found && other.said.contains(DEMO_OTHER_GROUP));
    assert!(other.said.contains("2 pilots were in on it."));

    // The list of reports says what became of what was reported.
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(listed.said.contains(
        "  Place then:   Beispielwiese\n  Note then:    A made-up plan. Ground handling if the \
         wind allows, coffee if not.\n  Now:          gone; its group is called \"Mustergruppe\""
    ));

    // The group removed, with all that is in it.
    let no_group = |token: &str| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            let nobodys = group_path(&NOBODYS.parse().unwrap());
            observable(&ask(Method::GET, &nobodys).token(&token).send(&router).await)
        }
    };
    let remove = |group: &str| Action::RemoveGroup {
        group: group.to_owned(),
    };
    let removed = operator(&store, &clock, remove(DEMO_GROUP)).await;
    assert!(removed.found, "{}", removed.said);
    assert!(
        removed.said.starts_with(
            "Group 44444444444444444444444444 \"Mustergruppe\", kept by demo (Demo Pilot), is \
             removed, with the places of its 3 members in it and its 3 plans. Its invite code \
             lets nobody in.\n"
        ),
        "{}",
        removed.said
    );
    assert!(
        removed
            .said
            .contains("No account was touched, and no report was closed.")
    );
    // To everyone who was in it, it answers as a group that never was, and
    // it is in none of their lists.
    for token in members {
        let gone = ask(Method::GET, &group_path(&group))
            .token(token)
            .send(&router)
            .await;
        gone.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
        assert_eq!(observable(&gone), no_group(token).await);
        let plans = ask(Method::GET, &group_plans_path(&group))
            .token(token)
            .send(&router)
            .await;
        plans.error(StatusCode::NOT_FOUND, ErrorCode::GroupNotFound);
        let mine: GroupsResponse = ask(Method::GET, GROUPS_PATH)
            .token(token)
            .send(&router)
            .await
            .ok(StatusCode::OK);
        assert!(mine.groups.iter().all(|listed| listed.id != group));
    }
    // Its code lets nobody in, and the accounts are as they were: the
    // other group is where it was, with Mara keeping it.
    let newcomer = sign_up(&router, "newcomer").await.token;
    try_join(&router, &newcomer, DEMO_GROUP_CODE)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::InvalidGroupInvite);
    for handle in [DEMO_HANDLE, "mara", "jonas"] {
        assert!(store.account(handle).await.unwrap().is_some(), "{handle}");
    }
    let other = seen_group(&router, &mara, &DEMO_OTHER_GROUP.parse().unwrap()).await;
    assert_eq!(other.group.keeper.handle.as_str(), "mara");
    // The reports are open still, and say that it is gone.
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(listed.said.starts_with("2 reports are open"));
    assert!(
        listed
            .said
            .contains("  Name then:    Mustergruppe\n  Now:          gone\n")
    );
    assert!(
        listed
            .said
            .contains("  Now:          gone; its group is gone\n")
    );
    // There is nothing to remove twice, and what is no group is none.
    for nothing in [DEMO_GROUP, NOBODYS, "demo", DEMO_PLANS[0], ""] {
        let outcome = operator(&store, &clock, remove(nothing)).await;
        assert!(!outcome.found, "{nothing:?}: {}", outcome.said);
        assert!(!outcome.said.contains("is removed"), "{}", outcome.said);
    }
}

#[tokio::test]
async fn a_plan_whose_day_is_gone_is_no_plan_to_whoever_runs_the_server_either() {
    let (router, store, clock) = open_server();
    let anna = sign_up(&router, "anna").await.token;
    let made = make_group(&router, &anna, "Mustergruppe").await;
    let group = made.group.id.clone();
    let old = post_plan(&router, &anna, &group, &a_plan(18, "Musterberg", None)).await;
    let new = post_plan(&router, &anna, &group, &a_plan(21, "Beispielwiese", None)).await;
    report(&router, &anna, &about_plan(group.as_str(), old.id.as_str()))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);

    // Three days later the plan for the 18th is in no answer of the API,
    // whether or not the hourly cleanup has come by. The commands go by
    // the same reckoning: it is not shown, not counted and not removed.
    clock.advance(Duration::days(3));
    assert_eq!(plans_of(&router, &anna, &group).await, [new.id.as_str()]);
    let look = Action::Group {
        group: group.to_string(),
    };
    let shown = operator(&store, &clock, look).await;
    assert!(shown.said.contains("  Plans:        1"), "{}", shown.said);
    assert!(!shown.said.contains(old.id.as_str()), "{}", shown.said);
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(
        listed.said.contains(
            "  Place then:   Musterberg\n  Now:          gone; its group is called \"Mustergruppe\""
        ),
        "{}",
        listed.said
    );
    let remove = Action::RemovePlan {
        plan: old.id.to_string(),
    };
    let outcome = operator(&store, &clock, remove).await;
    assert!(!outcome.found);
    assert!(
        outcome.said.contains("its day is more than 2 days gone by"),
        "{}",
        outcome.said
    );
    let removed = Action::RemoveGroup {
        group: group.to_string(),
    };
    let removed = operator(&store, &clock, removed).await;
    assert!(
        removed
            .said
            .contains("the places of its 1 member in it and its 1 plan."),
        "{}",
        removed.said
    );
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
        Action::Group {
            group: "c".repeat(26),
        },
        Action::RenameGroup {
            group: "c".repeat(26),
            name: "Another name".to_owned(),
        },
        Action::RemovePlan {
            plan: "c".repeat(26),
        },
        Action::RemoveGroup {
            group: "c".repeat(26),
        },
    ] {
        let failed = perform(&*store, &clock, site(), &action).await.unwrap_err();
        assert!(failed.starts_with("the database failed"), "{failed}");
    }
    // The development mode's server, by contrast, answers.
    let router = recorded().await;
    assert_eq!(get(&router, "/v1/health").await.status, StatusCode::OK);
}

// --- Sites that pilots made -------------------------------------------------------------

/// The identifier of the site these tests make, and of a second one. The
/// development mode has neither.
const A_SITE: &str = "venti:0s1te0s1te0s1te0s1te0s1te0";
const ANOTHER_SITE: &str = "venti:0s2te0s2te0s2te0s2te0s2te0";

/// How near to each other the commands let two sites stand, in metres:
/// the number `revert-site` names when it refuses.
const SITES_APART_M: f64 = 20.0;

/// A made-up take-off under an identifier, far from every site the
/// development mode starts with.
fn a_takeoff(id: &str, name: &str) -> venti_domain::Site {
    venti_domain::Site {
        id: id.parse().unwrap(),
        name: Some(name.to_owned()),
        position: GeoPoint::new(46.9, 13.1).unwrap(),
        elevation_m: Some(1840.0),
        uses: vec![SiteUse::Takeoff],
        wind_directions: vec![WindSector {
            from_deg: 180.0,
            to_deg: 270.0,
        }],
        paragliding: Some(true),
        hang_gliding: None,
        official: None,
        discouraged: false,
        url: None,
        website: None,
        description: None,
        fee: None,
        guest_guidelines: None,
        no_flight_time: None,
        outline: None,
    }
}

async fn account_id(store: &MemoryStore, handle: &str) -> String {
    store.account(handle).await.unwrap().unwrap().id
}

/// Adds a site as a pilot does, at the clock's time.
async fn add_site(
    store: &MemoryStore,
    clock: &TestClock,
    author: &str,
    site: venti_domain::Site,
    comment: &str,
) -> PilotSite {
    let new = NewPilotSite {
        site,
        author_id: author.to_owned(),
        at: clock.now(),
        via: SiteVia::Session,
        comment: Some(comment.to_owned()),
    };
    match store.add_pilot_site(&new, SITES_APART_M).await.unwrap() {
        SiteWrite::Written(site) => *site,
        other => panic!("{other:?}"),
    }
}

/// Changes a site as a pilot does, at the clock's time.
async fn change_site(
    store: &MemoryStore,
    clock: &TestClock,
    author: &str,
    site: venti_domain::Site,
    comment: Option<&str>,
) -> PilotSite {
    let revising = SiteRevising {
        id: site.id.clone(),
        author_id: Some(author.to_owned()),
        at: clock.now(),
        via: SiteVia::Session,
        act: SiteAct::Change(Box::new(site)),
        comment: comment.map(str::to_owned),
        expects: None,
    };
    let written = store.revise_pilot_site(&revising, SITES_APART_M).await;
    match written.unwrap() {
        SiteWrite::Written(site) => *site,
        other => panic!("{other:?}"),
    }
}

async fn stored_site(store: &MemoryStore, id: &str) -> PilotSite {
    let id: SiteId = id.parse().unwrap();
    store.pilot_site(&id).await.unwrap().unwrap()
}

#[tokio::test]
async fn whoever_runs_the_server_looks_at_a_site_reverts_it_and_takes_it_down() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let [mara, jonas] = DEMO_PILOTS.map(|pilot| pilot.handle);
    let (mara, jonas) = (
        account_id(&store, mara).await,
        account_id(&store, jonas).await,
    );
    // A pilot who changes the site and then deletes the account.
    sign_up(&router, "gast").await;
    let guest = account_id(&store, "gast").await;

    // The site's story: made by one pilot, corrected by a second,
    // scribbled on by a third, whose account is gone afterwards.
    let made = a_takeoff(A_SITE, "Musterkogel West");
    add_site(
        &store,
        &clock,
        &mara,
        made.clone(),
        "From my own flights there.",
    )
    .await;
    clock.advance(Duration::hours(2));
    let mut corrected = made.clone();
    corrected.elevation_m = Some(1862.0);
    corrected.description =
        Some("A made-up take-off.\nSteep; not for the first flight.".to_owned());
    change_site(
        &store,
        &clock,
        &jonas,
        corrected.clone(),
        Some("Height from my GPS."),
    )
    .await;
    clock.advance(Duration::days(1));
    let mut scribbled = corrected.clone();
    scribbled.name = Some("Nonsense\u{1b}[2J".to_owned());
    scribbled.description = None;
    scribbled.website = Some("https://example.com/buy".to_owned());
    change_site(&store, &clock, &guest, scribbled.clone(), None).await;
    store.delete_account(&guest).await.unwrap();
    clock.advance(Duration::minutes(30));

    // Looking at it: the record as it stands, and every revision.
    let look = |site: &str| Action::Site {
        site: site.to_owned(),
    };
    let shown = operator(&store, &clock, look(&format!(" {A_SITE}\n"))).await;
    assert!(shown.found, "{}", shown.said);
    assert_golden_text("operator_site.txt", &format!("{}\n", shown.said));
    for expected in [
        "Site venti:0s1te0s1te0s1te0s1te0s1te0\n  Layer:        the partners' ",
        "  Name:         Nonsense?[2J\n",
        "  Place:        46.90000, 13.10000\n  Height:       1862 m\n  Uses:         take-off\n",
        "  Wind from:    180° to 270°\n",
        "  Web page:     https://example.com/buy\n",
        "  Made:         2026-11-18 19:05 UTC by mara (Mara Muster)\n",
        "  Stands:       at revision 3\n",
        "History, 3 revisions, the first one first:\n",
        "   1  2026-11-18 19:05 UTC  mara (Mara Muster), through a session made the site: \
         \"From my own flights there.\"\n",
        "   2  2026-11-18 21:05 UTC  jonas (Jonas Beispiel), through a session changed it \
         (elevation_m, description): \"Height from my GPS.\"\n",
        "   3  2026-11-19 21:05 UTC  an account that is gone, through a session changed it \
         (name, website, description)\n",
        "Nothing was changed",
    ] {
        assert!(shown.said.contains(expected), "{expected}\n{}", shown.said);
    }
    // A link that ends in the identifier names the site too.
    let link = format!("{PUBLIC_URL}/v1/sites/{A_SITE}/");
    assert_eq!(operator(&store, &clock, look(&link)).await, shown);
    assert_eq!(stored_site(&store, A_SITE).await.revision, 3);
    // What is no site that pilots made: no identifier, another source's
    // site, a site nobody made.
    for nothing in [
        "",
        "demo",
        NOBODYS,
        "venti:",
        "venti:short",
        "osm-free-flying:node/5964676196",
        "x' OR '1'='1",
        ANOTHER_SITE,
    ] {
        for action in [
            look(nothing),
            Action::RevertSite {
                site: nothing.to_owned(),
                revision: 1,
                comment: None,
            },
            Action::RemoveSite {
                site: nothing.to_owned(),
                comment: None,
            },
        ] {
            let outcome = operator(&store, &clock, action).await;
            assert!(!outcome.found, "{nothing:?}: {}", outcome.said);
        }
    }

    // Back to what the second revision said, as a fourth of the
    // operator's, with a comment cleaned as a pilot's is.
    let revert = |revision: u32, comment: Option<&str>| Action::RevertSite {
        site: A_SITE.to_owned(),
        revision,
        comment: comment.map(str::to_owned),
    };
    let reverted = operator(&store, &clock, revert(2, Some("  Not\ta site's name. "))).await;
    assert!(reverted.found, "{}", reverted.said);
    assert!(
        reverted.said.starts_with(
            "Site venti:0s1te0s1te0s1te0s1te0s1te0 \"Musterkogel West\" says again what its \
             revision 2 said, as its revision 4.\n"
        ),
        "{}",
        reverted.said
    );
    assert!(reverted.said.contains("no report was closed"));
    let now = stored_site(&store, A_SITE).await;
    assert_eq!((now.revision, &now.site), (4, &corrected));
    assert_eq!((&now.changed_by, now.changed_at), (&None, clock.now()));
    assert!(now.stands());
    let shown = operator(&store, &clock, look(A_SITE)).await;
    assert!(
        shown.said.contains(
            "   4  2026-11-19 21:35 UTC  the operator restored revision 2 (name, website, \
             description): \"Not a site's name.\"\n"
        ),
        "{}",
        shown.said
    );
    // The same again changes nothing; a revision it does not have is
    // none; a comment that is too long is refused before anything is
    // looked at.
    let again = operator(&store, &clock, revert(2, None)).await;
    assert!(again.found);
    assert!(again.said.ends_with("Nothing changed."), "{}", again.said);
    let none = operator(&store, &clock, revert(5, None)).await;
    assert!(!none.found);
    assert!(
        none.said
            .contains("has no revision 5: it stands at revision 4"),
        "{}",
        none.said
    );
    let long = "x".repeat(201);
    for action in [
        revert(1, Some(long.as_str())),
        Action::RemoveSite {
            site: A_SITE.to_owned(),
            comment: Some(long.clone()),
        },
    ] {
        let refused = perform(&*store, &clock, site(), &action).await;
        assert!(refused.unwrap_err().starts_with("a comment is one line"));
    }
    assert_eq!(stored_site(&store, A_SITE).await, now);

    // Taken down: gone, and kept.
    clock.advance(Duration::minutes(5));
    let remove = Action::RemoveSite {
        site: A_SITE.to_owned(),
        comment: Some("Private ground; the owner asked.".to_owned()),
    };
    let removed = operator(&store, &clock, remove.clone()).await;
    assert!(removed.found, "{}", removed.said);
    assert!(
        removed.said.starts_with(
            "Site venti:0s1te0s1te0s1te0s1te0s1te0 \"Musterkogel West\" is taken down, as its \
             revision 5. It stood until now.\n"
        ),
        "{}",
        removed.said
    );
    assert!(removed.said.contains(
        "`venti-server revert-site venti:0s1te0s1te0s1te0s1te0s1te0 --revision 4` brings it back"
    ));
    let down = stored_site(&store, A_SITE).await;
    assert_eq!((down.revision, down.taken_down), (5, true));
    assert_eq!(down.removed_at, Some(clock.now()));
    assert_eq!(down.site, corrected);
    let everywhere = venti_domain::BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap();
    let layer = venti_server::pilot_sites::Layer::Partners.source();
    let standing = store
        .pilot_sites_in(&layer, &everywhere, 10_000)
        .await
        .unwrap();
    assert!(standing.iter().all(|site| site.site.id != down.site.id));
    // No pilot brings it back.
    let by_a_pilot = SiteRevising {
        id: A_SITE.parse().unwrap(),
        author_id: Some(mara.clone()),
        at: clock.now(),
        via: SiteVia::Session,
        act: SiteAct::Restore(4),
        comment: None,
        expects: None,
    };
    assert_eq!(
        store.revise_pilot_site(&by_a_pilot, SITES_APART_M).await,
        Ok(SiteWrite::TakenDown)
    );
    // The operator still sees all of it, and taking it down again changes
    // nothing.
    let shown = operator(&store, &clock, look(A_SITE)).await;
    assert!(shown.found);
    for expected in [
        "  Stands:       taken down since 2026-11-19 21:40 UTC, at revision 5; gone to \
         everyone, and only revert-site brings it back\n",
        "   5  2026-11-19 21:40 UTC  the operator removed it: \"Private ground; the owner \
         asked.\"\n",
    ] {
        assert!(shown.said.contains(expected), "{expected}\n{}", shown.said);
    }
    let again = operator(&store, &clock, remove).await;
    assert!(again.found);
    assert!(
        again
            .said
            .ends_with("is taken down already. Nothing changed."),
        "{}",
        again.said
    );
    assert_eq!(stored_site(&store, A_SITE).await, down);

    // Somebody adds a site at the place meanwhile: the one that was taken
    // down does not come back on top of it.
    let other = a_takeoff(ANOTHER_SITE, "Musterkogel");
    add_site(&store, &clock, &mara, other, "The take-off by the cross.").await;
    let blocked = operator(&store, &clock, revert(4, None)).await;
    assert!(!blocked.found);
    assert!(
        blocked.said.starts_with(
            "Nothing changed: as its revision 4 said, site venti:0s1te0s1te0s1te0s1te0s1te0 \
             would stand less than 20 m from site venti:0s2te0s2te0s2te0s2te0s2te0"
        ),
        "{}",
        blocked.said
    );
    assert_eq!(stored_site(&store, A_SITE).await, down);
    // With that one taken down, it does, as a sixth revision.
    let other_down = Action::RemoveSite {
        site: ANOTHER_SITE.to_owned(),
        comment: None,
    };
    assert!(operator(&store, &clock, other_down).await.found);
    clock.advance(Duration::minutes(5));
    let back = operator(&store, &clock, revert(4, Some("Agreed with the owner."))).await;
    assert!(back.found, "{}", back.said);
    assert!(
        back.said
            .contains("as its revision 6. It was taken down and stands again"),
        "{}",
        back.said
    );
    let now = stored_site(&store, A_SITE).await;
    assert_eq!(
        (now.revision, now.taken_down, now.removed_at),
        (6, false, None)
    );
    assert_eq!(now.site, corrected);
    assert_eq!(now.created_by.map(|pilot| pilot.account_id), Some(mara));

    // A site a pilot removed is taken down as one that stood is, and the
    // command says which it was.
    let by_a_pilot = SiteRevising {
        id: A_SITE.parse().unwrap(),
        author_id: Some(jonas),
        at: clock.now(),
        via: SiteVia::Session,
        act: SiteAct::Remove { take_down: false },
        comment: None,
        expects: None,
    };
    let removed = store.revise_pilot_site(&by_a_pilot, SITES_APART_M).await;
    assert!(matches!(removed, Ok(SiteWrite::Written(_))));
    let shown = operator(&store, &clock, look(A_SITE)).await;
    assert!(
        shown.said.contains(
            "  Stands:       removed since 2026-11-19 21:45 UTC, at revision 7; a pilot can \
             bring it back\n"
        ),
        "{}",
        shown.said
    );
    let taken_down = Action::RemoveSite {
        site: A_SITE.to_owned(),
        comment: None,
    };
    let taken_down = operator(&store, &clock, taken_down).await;
    assert!(taken_down.found);
    assert!(
        taken_down
            .said
            .contains("as its revision 8. A pilot had removed it"),
        "{}",
        taken_down.said
    );
}

// --- The operator's guide ---------------------------------------------------------------

#[test]
fn the_operators_guide_shows_the_commands_as_they_are() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/running-a-server.md"
    );
    let guide = std::fs::read_to_string(path).unwrap();
    // What `reports` prints is in the guide as the tests keep it.
    let printed = std::fs::read_to_string(golden("operator_reports.txt")).unwrap();
    assert!(
        guide.contains(printed.trim_end()),
        "docs/running-a-server.md does not show operator_reports.txt as the tests keep it; copy \
         the file from crates/server/tests/golden into the document"
    );
    // So is what `site` prints.
    let printed = std::fs::read_to_string(golden("operator_site.txt")).unwrap();
    assert!(
        guide.contains(printed.trim_end()),
        "docs/running-a-server.md does not show operator_site.txt as the tests keep it; copy \
         the file from crates/server/tests/golden into the document"
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
        "group",
        "rename-group",
        "remove-plan",
        "remove-group",
        "remove-comment",
        "site",
        "revert-site",
        "remove-site",
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

// --- Who may ask as nobody ------------------------------------------------------------------

/// The Rust files under a folder, however deep.
fn sources_under(folder: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources_under(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The store answers a few calls for whoever runs the server without
/// asking who asks: about any flight, any live track, any account, and
/// since 2026-11-21 about any group and any plan. A request handler that
/// reached one of them would show or change a group for someone who is
/// not in it. Only the operator's commands may call them.
#[test]
fn only_the_operators_commands_ask_the_store_as_nobody() {
    let calls = [
        ".any_group(",
        ".plans_of_any_group(",
        ".any_plan(",
        ".rename_any_group(",
        ".remove_plan(",
        ".remove_group(",
        ".take_down_flight(",
        ".release_flight(",
        ".remove_live_track(",
        ".suspend_publishing(",
        ".resume_publishing(",
    ];
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources_under(&folder, &mut files);
    assert!(files.len() > 40, "{} files", files.len());
    let mut commands = 0;
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        if file.ends_with("moderation.rs") {
            // Each of them is something a command does.
            for call in calls {
                assert!(text.contains(call), "no command calls {call}");
            }
            commands += 1;
            continue;
        }
        for call in calls {
            assert!(
                !text.contains(call),
                "{} calls {call}, which is for whoever runs the server",
                file.display()
            );
        }
    }
    assert_eq!(commands, 1, "the commands are in one file");
}

// --- Reports about a site that pilots made -----------------------------------

fn about_site(id: &str, revision: Option<u32>, reason: ReportReason) -> ReportRequest {
    ReportRequest {
        subject: ReportSubject::Site(ReportedSite {
            id: id.to_owned(),
            revision,
        }),
        reason,
        text: None,
    }
}

#[tokio::test]
async fn a_pilot_reports_a_site_that_pilots_made_and_one_revision_of_it() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let petra = sign_up(&router, "petra").await.token;
    let [first, second, _, removed] = DEMO_SITES;

    // The site as it stands: the example of the documentation. The demo
    // account added it; the report names nobody.
    let request = ReportRequest {
        text: Some("The take-off is on the other side of the ridge.".to_owned()),
        ..about_site(first, None, ReportReason::WrongOrDangerous)
    };
    assert_golden(&golden("report_site_request.json"), &request);
    let answer = report(&router, &petra, &request).await;
    assert!(answer.ok::<ReportResponse>(StatusCode::CREATED).created);
    answer.is_private();
    // One revision of another site, by its number; and a site that a
    // pilot removed, whose history is still there to read.
    let revision = about_site(second, Some(2), ReportReason::Offensive);
    let created: ReportResponse = report(&router, &petra, &revision)
        .await
        .ok(StatusCode::CREATED);
    assert!(created.created);
    let gone = about_site(removed, None, ReportReason::NotASite);
    report(&router, DEMO_TOKEN, &gone)
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);
    // The same site again is the same report, whichever revision it names.
    let again = about_site(first, Some(1), ReportReason::Spam);
    let known: ReportResponse = report(&router, &petra, &again).await.ok(StatusCode::OK);
    assert!(!known.created);

    // What waits for whoever runs the server: the site and the revision
    // it stood at or the one that was named, and no account but the
    // reporter's.
    let reports = store.reports(ReportListing::Open, None, 10).await.unwrap();
    assert_eq!(reports.len(), 3);
    assert!(reports.iter().all(|report| report.about.is_none()));
    // The three were made in the same moment: found by what they are about.
    let kept = |id: &str| {
        reports
            .iter()
            .find(|report| report.subject.id() == id)
            .unwrap_or_else(|| panic!("no report about {id}"))
    };
    assert_eq!(
        kept(first).subject,
        venti_store::ReportSubject::Site {
            id: first.to_owned(),
            revision: 2,
        }
    );
    assert_eq!(kept(first).reason, "wrong_or_dangerous");
    assert_eq!(
        kept(first).text.as_deref(),
        Some("The take-off is on the other side of the ridge.")
    );
    assert_eq!(
        kept(second).subject,
        venti_store::ReportSubject::Site {
            id: second.to_owned(),
            revision: 2,
        }
    );
    assert_eq!(kept(removed).reason, "not_a_site");
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(listed.found);
    let block = listed
        .said
        .split("\n\n")
        .find(|block| block.contains(first))
        .expect("the report about the first site");
    assert!(
        block.contains(&format!(
            "site {first}, revision 2; made by pilots and nobody's"
        )),
        "{block}"
    );
    assert!(block.contains("at revision 2, called"), "{block}");
    assert!(
        block.contains(&format!("venti-server site {first}")),
        "{block}"
    );
    assert!(block.contains("Reported by:"), "{block}");
    assert!(
        listed.said.contains("removed, at revision"),
        "{}",
        listed.said
    );

    // A site has the reasons of a site: the one about an account and the
    // one about a flight are none of them. The refusal is the same
    // whether there is such a site or not.
    for id in [first, "venti:99999999999999999999999999"] {
        let wrong = about_site(id, None, ReportReason::NotAFlight);
        let said = report(&router, &petra, &wrong)
            .await
            .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
        assert_eq!(
            said,
            "reason must be one of wrong_or_dangerous, not_a_site, offensive, privacy, spam, \
             other"
        );
    }
    // And the reasons of a site are no reasons about anything else.
    let wrong = ReportRequest {
        reason: ReportReason::NotASite,
        ..about(ReportSubject::Pilot("mara".to_owned()))
    };
    report(&router, &petra, &wrong)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);

    // What is not a site that pilots made is not reported here.
    let spam = |id: &str, revision| about_site(id, revision, ReportReason::Spam);
    let of_a_source = spam("osm-free-flying:way/1080986173", None);
    report(&router, &petra, &of_a_source)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    for nothing in ["venti:99999999999999999999999999", "venti:x", "nothing"] {
        report(&router, &petra, &spam(nothing, None))
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::SiteNotFound);
    }
    for revision in [0, 9] {
        report(&router, DEMO_TOKEN, &spam(second, Some(revision)))
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::SiteRevisionNotFound);
    }
    let all = store.reports(ReportListing::All, None, 10).await.unwrap();
    assert_eq!(all.len(), 3, "nothing of that was kept");

    // A site whoever runs the server took down shows nothing, and is gone
    // for a report as for a reader.
    let taken_down = Action::RemoveSite {
        site: second.to_owned(),
        comment: Some("Made up, for a test.".to_owned()),
    };
    assert!(operator(&store, &clock, taken_down).await.found);
    report(&router, DEMO_TOKEN, &spam(second, None))
        .await
        .error(StatusCode::GONE, ErrorCode::SiteGone);
    // The report that was made about it stays, and says what became of it.
    let listed = operator(&store, &clock, Action::Reports { all: false }).await;
    assert!(listed.said.contains("taken down"), "{}", listed.said);
}

// --- Who Venti's partners for flying sites are --------------------------------

#[tokio::test]
async fn whoever_runs_the_server_names_the_partners_for_flying_sites() {
    let (router, store) = recorded_with_its_store().await;
    let clock = TestClock::new();
    let partners = || operator(&store, &clock, Action::SitePartners);
    let grant = |handle: &str| Action::GrantSites {
        handle: handle.to_owned(),
    };
    let withdraw = |handle: &str| Action::WithdrawSites {
        handle: handle.to_owned(),
    };
    let add = |token: &'static str, lat: f64| {
        let router = router.clone();
        async move {
            let request = serde_json::json!({
                "site": {
                    "name": "Beispielkopf",
                    "position": { "lat": lat, "lon": 11.2 },
                    "uses": ["takeoff"],
                }
            });
            ask(Method::POST, "/v1/sites")
                .token(token)
                .json(&request)
                .send(&router)
                .await
        }
    };

    // The development mode starts with one: its demo account.
    let listed = partners().await;
    assert!(listed.found);
    assert!(listed.said.contains("demo (Demo Pilot)"), "{}", listed.said);
    assert!(!listed.said.contains("mara"), "{}", listed.said);

    // Named: the account says so to its apps, and what it adds from now
    // on is a partner's site.
    let mara = common::try_sign_in(&router, "mara", venti_server::dev::DEMO_PASSWORD).await;
    let mara: venti_api_types::v1::SignInResponse = mara.ok(StatusCode::CREATED);
    assert!(!mara.account.edits_sites);
    let token: &'static str = Box::leak(mara.token.clone().into_boxed_str());
    let before: venti_api_types::v1::SiteResponse = add(token, 47.40).await.ok(StatusCode::CREATED);
    assert_eq!(before.site.id.source().as_str(), "venti-pilots");
    let granted = operator(&store, &clock, grant("@Mara")).await;
    assert!(granted.found, "{}", granted.said);
    assert_golden_text("operator_grant_sites.txt", &format!("{}\n", granted.said));
    let shown: AccountResponse = ask(Method::GET, ACCOUNT_PATH)
        .token(token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(shown.account.edits_sites);
    let after: venti_api_types::v1::SiteResponse = add(token, 47.42).await.ok(StatusCode::CREATED);
    assert_eq!(after.site.id.source().as_str(), "venti");
    // Named again: nothing changes, and the moment is the first one.
    let again = operator(&store, &clock, grant("mara")).await;
    assert!(again.found);
    assert!(
        again.said.contains("already. Nothing changed."),
        "{}",
        again.said
    );
    let listed = partners().await;
    assert_golden_text("operator_site_partners.txt", &format!("{}\n", listed.said));

    // Withdrawn: what the account wrote stays, and it writes no
    // partner's site any more.
    let withdrawn = operator(&store, &clock, withdraw("mara")).await;
    assert!(withdrawn.found);
    assert_golden_text(
        "operator_withdraw_sites.txt",
        &format!("{}\n", withdrawn.said),
    );
    let kept = stored_site(&store, &after.site.id.to_string()).await;
    assert!(kept.stands());
    assert_eq!(kept.created_by.unwrap().handle, "mara");
    ask(Method::DELETE, &format!("/v1/sites/{}", after.site.id))
        .token(token)
        .send(&router)
        .await
        .error(StatusCode::FORBIDDEN, ErrorCode::SitesForPartners);
    let next: venti_api_types::v1::SiteResponse = add(token, 47.44).await.ok(StatusCode::CREATED);
    assert_eq!(next.site.id.source().as_str(), "venti-pilots");
    let once_more = operator(&store, &clock, withdraw("mara")).await;
    assert!(once_more.found);
    assert!(
        once_more.said.contains("was not a partner"),
        "{}",
        once_more.said
    );
    let shown: AccountResponse = ask(Method::GET, ACCOUNT_PATH)
        .token(token)
        .send(&router)
        .await
        .ok(StatusCode::OK);
    assert!(!shown.account.edits_sites);

    // Nobody of that name; and a server on which nobody is one.
    for action in [grant("nobody"), withdraw("nobody"), grant("no handle!")] {
        let missing = operator(&store, &clock, action).await;
        assert!(!missing.found, "{}", missing.said);
        assert!(
            missing.said.starts_with("There is no account"),
            "{}",
            missing.said
        );
    }
    operator(&store, &clock, withdraw("demo")).await;
    let nobody = partners().await;
    assert!(nobody.found);
    assert!(
        nobody.said.starts_with("No account is a partner"),
        "{}",
        nobody.said
    );
}
