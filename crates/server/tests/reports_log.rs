//! What the server writes to its log while pilots report things, and what
//! it must never write there: who reported, whom, what, why, or a word of
//! what the reporter wrote. A report is one person's word about another,
//! and for a live track or a flight shared by link the thing reported is
//! named by the key to it.
//!
//! Like `live_log.rs`, this test has a file, and so a process, to itself: it
//! collects the log of its own thread, and the logging library decides once
//! per log statement whether anyone listens.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use venti_api_types::v1::{
    ErrorCode, LIVE_POSITIONS_PATH, LiveFix, LivePositionsRequest, LiveVisibility, REPORTS_PATH,
    ReportReason, ReportRequest, ReportResponse, ReportSubject, Visibility,
};
use venti_domain::GeoPoint;
use venti_server::accounts::Limits;
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::FixedClock;
use venti_server::moderation::{Action, Site, perform};
use venti_store::testing::TroubledStore;

mod common;

use common::{TestClock, ask, sample_file, server, set_visibility, settings, sign_up, upload};

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

/// Words that stand nowhere else in this test or in the server.
const WORDS: &str = "Zanzibar marmalade quotient";

#[tokio::test]
async fn nothing_of_a_report_reaches_the_log() {
    let clock = TestClock::new();
    let strict = Settings {
        limits: Limits {
            reports_per_day: 12,
            ..Limits::default()
        },
        ..settings()
    };
    let (router, store) = server(strict, &clock);
    // Two accounts with handles that are no words of the log's own, a
    // flight shared by link and a live track shared by link. All of that
    // is in place before anyone listens: the log may say that an account
    // was created.
    let reporter = sign_up(&router, "xq_reporter").await.token;
    let reported = sign_up(&router, "xq_reported").await.token;
    let flight = upload(&router, &reported, &sample_file()).await.flight;
    set_visibility(&router, &reported, &flight.id, Visibility::Unlisted).await;
    let track = "b".repeat(26);
    let positions = LivePositionsRequest {
        id: track.clone(),
        visibility: LiveVisibility::Link,
        landed: false,
        fixes: vec![LiveFix {
            time: adapters::recorded_at() - Duration::seconds(30),
            position: GeoPoint::new(47.66, 11.8).unwrap(),
            altitude_m: 1600,
            ground_speed_ms: None,
            climb_ms: None,
            course_deg: None,
        }],
    };
    let begun = ask(Method::POST, LIVE_POSITIONS_PATH)
        .token(&reported)
        .json(&positions)
        .send(&router)
        .await;
    assert_eq!(begun.status, StatusCode::CREATED);

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
    let listener = tracing::Dispatch::new(subscriber);
    let listening = tracing::dispatcher::set_default(&listener);

    let send = |token: &str, request: ReportRequest| {
        let (router, token) = (router.clone(), token.to_owned());
        async move {
            ask(Method::POST, REPORTS_PATH)
                .token(&token)
                .json(&request)
                .send(&router)
                .await
        }
    };
    let about = |subject: ReportSubject, reason: ReportReason| ReportRequest {
        subject,
        reason,
        text: Some(WORDS.to_owned()),
    };

    // Reports that are stored: about the flight, the live track, and the
    // reported pilot's page once there is one; the same again.
    let subjects = [
        ReportSubject::Flight(flight.id.to_string()),
        ReportSubject::LiveTrack(track.clone()),
    ];
    for subject in &subjects {
        let stored = send(
            &reporter,
            about(subject.clone(), ReportReason::Impersonation),
        )
        .await;
        assert!(stored.ok::<ReportResponse>(StatusCode::CREATED).created);
        let again = send(
            &reporter,
            about(subject.clone(), ReportReason::Impersonation),
        )
        .await;
        assert!(!again.ok::<ReportResponse>(StatusCode::OK).created);
    }
    // The pilot publishes the flight, so that there is a page to report.
    // Nobody listens meanwhile: a request about a flight has the flight's
    // address in its path, and the log names every request's path. That
    // is the log's way with flights and no part of a report.
    drop(listening);
    set_visibility(&router, &reported, &flight.id, Visibility::Public).await;
    let _listening = tracing::dispatcher::set_default(&listener);
    let page = ReportSubject::Pilot("xq_reported".to_owned());
    send(&reporter, about(page, ReportReason::Impersonation))
        .await
        .ok::<ReportResponse>(StatusCode::CREATED);

    // Reports that are not: about nothing, with a reason nobody knows,
    // with too many words, in a body that is no report and names the
    // flight, and from nobody.
    let nothing = ReportSubject::LiveTrack("c".repeat(26));
    send(&reporter, about(nothing, ReportReason::Impersonation))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::LiveTrackNotFound);
    let unknown = format!(
        r#"{{"subject":{{"flight":"{}"}},"reason":"quotient","text":"{WORDS}"}}"#,
        flight.id
    );
    let said = ask(Method::POST, REPORTS_PATH)
        .token(&reporter)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(unknown)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    assert!(!said.contains("quotient"), "{said}");
    let wordy = ReportRequest {
        text: Some(WORDS.repeat(50)),
        ..about(subjects[0].clone(), ReportReason::Spam)
    };
    send(&reporter, wordy)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidReport);
    let malformed = format!(
        r#"{{"subject":{{"flight":"{}"}},"text":"{WORDS}","reason":"#,
        flight.id
    );
    let said = ask(Method::POST, REPORTS_PATH)
        .token(&reporter)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes(malformed)
        .send(&router)
        .await
        .error(StatusCode::BAD_REQUEST, ErrorCode::InvalidRequest);
    assert!(
        !said.contains(flight.id.as_str()) && !said.contains("Zanzibar"),
        "{said}"
    );
    let answer = ask(Method::POST, REPORTS_PATH)
        .json(&about(subjects[1].clone(), ReportReason::Privacy))
        .send(&router)
        .await;
    answer.error(StatusCode::UNAUTHORIZED, ErrorCode::Unauthorized);

    // More than an account may send in a day.
    let mut refused = 0;
    for _ in 0..12 {
        let answer = send(&reporter, about(subjects[0].clone(), ReportReason::Spam)).await;
        refused += usize::from(answer.status == StatusCode::TOO_MANY_REQUESTS);
    }
    assert!(refused >= 4, "{refused} were refused");

    // What whoever runs the server does prints to their own terminal, and
    // goes to no log either.
    let site = Site {
        public_url: Some(common::PUBLIC_URL),
        live_hours: 48,
    };
    let listed = perform(&*store, &clock, site, &Action::Reports { all: true })
        .await
        .unwrap();
    assert!(listed.said.contains(WORDS) && listed.said.contains("xq_reporter"));
    for action in [
        Action::TakeDown {
            flight: flight.id.to_string(),
        },
        Action::RemoveLiveTrack {
            track: track.clone(),
        },
        Action::SuspendPublishing {
            handle: "xq_reported".to_owned(),
        },
    ] {
        assert!(perform(&*store, &clock, site, &action).await.unwrap().found);
    }

    // And a database that is away, which the log does hear about.
    let api = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(FixedClock(adapters::recorded_at())),
        [],
        settings(),
    );
    let troubled = api.router(&Cors::Any);
    let answer = ask(Method::POST, REPORTS_PATH)
        .token(&format!("venti_{}", "7".repeat(52)))
        .json(&about(subjects[0].clone(), ReportReason::Spam))
        .send(&troubled)
        .await;
    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);

    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    // The log did record all this: the requests by their path, which
    // names nothing, and the trouble with the database.
    for expected in ["path=/v1/reports}", "the store is unavailable"] {
        assert!(log.contains(expected), "the log lacks {expected:?}:\n{log}");
    }
    // It holds nothing of a report.
    for (what, secret) in [
        ("the reporter's handle", "xq_reporter"),
        ("the reported pilot's handle", "xq_reported"),
        ("the address of the reported flight", flight.id.as_str()),
        ("the identifier of the reported live track", track.as_str()),
        (
            "an identifier that was reported in vain",
            "cccccccccccccccccccccccccc",
        ),
        ("a word of the reporter's text", "Zanzibar"),
        ("a word of the reporter's text", "marmalade"),
        ("a reason", "impersonation"),
        ("a reason that is none", "quotient"),
    ] {
        assert!(!log.contains(secret), "{what} is in the log:\n{log}");
    }
}
