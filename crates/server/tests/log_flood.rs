//! How much the server writes to its log when someone keeps asking for
//! what it refuses, or keeps it from answering.
//!
//! Like `log.rs`, this test has a file, and so a process, to itself: it
//! collects the log of its own thread, and the gates that keep the log
//! short are the process's.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode, header};
use venti_server::accounts::Limits;
use venti_server::api::{Api, Cors, MCP_PATH, Settings};
use venti_store::testing::TroubledStore;

mod common;

use common::{Answer, TestClock, ask, get, server, settings};

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

impl Captured {
    /// The lines logged so far that contain `text`.
    fn lines_with(&self, text: &str) -> Vec<String> {
        let log = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
        log.lines()
            .filter(|line| line.contains(text))
            .map(str::to_owned)
            .collect()
    }

    fn bytes(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

#[tokio::test]
async fn a_client_that_does_not_stop_costs_a_line_and_not_one_per_request() {
    let captured = Captured::default();
    let writer = captured.clone();
    // The level a server runs at unless it is told otherwise.
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let clock = TestClock::new();
    let limits = Limits {
        connector_requests_per_address: 5,
        ..Limits::none()
    };
    let settings = Settings {
        limits,
        ..settings()
    };
    let router = server(settings, &clock).0;

    // --- The connector, asked far more often than it answers -----------------
    let flood = |peer: &'static str, times: usize| {
        let router = router.clone();
        async move {
            let mut refused = 0;
            for _ in 0..times {
                let answer: Answer = ask(Method::POST, MCP_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .bytes(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
                    .from(peer)
                    .send(&router)
                    .await;
                match answer.status {
                    StatusCode::OK => {}
                    StatusCode::TOO_MANY_REQUESTS => refused += 1,
                    other => panic!("unexpected {other}"),
                }
            }
            refused
        }
    };
    const REFUSAL: &str = "connector: too many requests";

    assert_eq!(flood("203.0.113.9", 1005).await, 1000);
    let lines = captured.lines_with(REFUSAL);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains("203.0.113.9"), "{}", lines[0]);

    // Another address is news; the first one asking again is not.
    assert_eq!(flood("198.51.100.7", 505).await, 500);
    assert_eq!(flood("203.0.113.9", 500).await, 500);
    assert_eq!(captured.lines_with(REFUSAL).len(), 2);

    // A minute later the window is a new one, and so is its first refusal.
    clock.advance(chrono::Duration::minutes(1));
    assert_eq!(flood("203.0.113.9", 505).await, 500);
    assert_eq!(captured.lines_with(REFUSAL).len(), 3);

    // --- A server whose database is away, asked a thousand times ---------------
    let troubled = Api::new(
        Arc::new(TroubledStore::down()),
        Arc::new(TestClock::new()),
        std::iter::empty(),
        common::settings(),
    )
    .router(&Cors::Any);
    for n in 0..1000 {
        let path = [
            "/v1/health",
            "/v1/pilots/dario",
            "/f/0123456789abcdefghjkmnpqrs",
        ][n % 3];
        let answer = get(&troubled, path).await;
        assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    }
    // The first of each kind is written at once, and the rest are counted
    // for a later line. A gate opens every ten seconds; the test takes far
    // less, but a slow machine may see one open once more.
    for kind in ["store:", "a request failed"] {
        let lines = captured.lines_with(kind);
        assert!((1..=3).contains(&lines.len()), "{kind}: {lines:#?}");
    }
    assert!(captured.lines_with("a request failed")[0].contains("503"));

    // 2,500 refusals and 1,000 failures: a few lines, not a few thousand.
    assert!(
        captured.bytes() < 4096,
        "{} bytes of log:\n{}",
        captured.bytes(),
        captured.lines_with("").join("\n")
    );
}
