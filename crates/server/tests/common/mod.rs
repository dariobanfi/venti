//! What the tests of the HTTP API share: the development mode's router, a
//! server of their own making, and a way to send either requests.

// Each test file uses a part of this.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower::ServiceExt;
use venti_api_types::v1::{
    ACCOUNTS_PATH, CreateAccountRequest, ErrorCode, ErrorResponse, FLIGHTS_PATH, FlightId,
    FlightResponse, Password, SESSIONS_PATH, SignInRequest, SignInResponse, UpdateFlightRequest,
    UploadResponse, Visibility, flight_path,
};
use venti_domain::Timestamp;
use venti_flight::igc;
use venti_flight::testing::synthetic_flight;
use venti_server::accounts::password::Cost;
use venti_server::adapters;
use venti_server::api::{Api, Cors, Settings};
use venti_server::clock::Clock;
use venti_server::dev;
use venti_server::flights::analyser::{Analyser, Where};
use venti_server::flights::share::PublicUrl;
use venti_store::MemoryStore;

/// The origin of the web app in these tests.
pub const WEB_APP: &str = "https://venti.example";

/// The address the server is reached at in these tests.
pub const PUBLIC_URL: &str = "https://venti.example";

/// The development mode's settings as the tests use them: flights are
/// analysed on a thread of the test, and passwords are hashed at almost no
/// cost. Neither changes an answer.
pub fn settings() -> Settings {
    dev::settings(
        PublicUrl::new(PUBLIC_URL).unwrap(),
        Analyser::new(Where::Thread),
        Cost::TESTS_ONLY,
    )
}

/// The router of the development mode, with one web origin allowed.
pub async fn recorded() -> Router {
    let api = dev::recorded_api(settings()).await.unwrap();
    api.router(&Cors::Origins(vec![WEB_APP.to_owned()]))
}

/// A clock a test moves by hand.
#[derive(Debug, Clone)]
pub struct TestClock(Arc<Mutex<Timestamp>>);

impl TestClock {
    /// A clock that shows the moment of the recorded data.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(adapters::recorded_at())))
    }

    pub fn advance(&self, by: chrono::Duration) {
        *self.0.lock().unwrap() += by;
    }
}

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }
}

/// A server with an empty store, a clock the test moves, and the given
/// settings. Returns the store too, for what the API offers no way to do.
pub fn server(settings: Settings, clock: &TestClock) -> (Router, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let api = Api::new(store.clone(), Arc::new(clock.clone()), [], settings);
    (api.router(&Cors::Origins(vec![WEB_APP.to_owned()])), store)
}

pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Answer {
    fn start(&self) -> String {
        String::from_utf8_lossy(&self.body[..self.body.len().min(300)]).into_owned()
    }

    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("{error}: {} {}", self.status, self.start()))
    }

    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.body).expect("the body is text")
    }

    pub fn header(&self, name: header::HeaderName) -> &str {
        self.headers
            .get(&name)
            .unwrap_or_else(|| panic!("no {name} header"))
            .to_str()
            .unwrap()
    }

    /// Every value of a header that may be given several times, joined.
    pub fn header_values(&self, name: header::HeaderName) -> String {
        let values: Vec<&str> = self
            .headers
            .get_all(&name)
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect();
        values.join(", ")
    }

    /// Checks that the answer is an error of the API's one shape.
    pub fn error(&self, status: StatusCode, code: ErrorCode) -> String {
        assert_eq!(self.status, status, "{}", self.start());
        assert!(
            self.header(header::CONTENT_TYPE)
                .starts_with("application/json")
        );
        assert_eq!(self.header(header::CACHE_CONTROL), "no-store");
        let error: ErrorResponse = self.json();
        assert_eq!(error.error.code, code, "{}", error.error.message);
        error.error.message
    }

    /// Checks the status and decodes the body.
    pub fn ok<T: DeserializeOwned>(&self, status: StatusCode) -> T {
        assert_eq!(self.status, status, "{}", self.start());
        self.json()
    }

    /// Checks that the answer was made for one account: nobody may keep it
    /// and it carries no fingerprint of its content.
    pub fn is_private(&self) {
        assert_eq!(self.header(header::CACHE_CONTROL), "no-store");
        assert!(
            self.header_values(header::VARY).contains("authorization"),
            "Vary: {}",
            self.header_values(header::VARY)
        );
        assert!(!self.headers.contains_key(header::ETAG));
        assert!(!self.headers.contains_key(header::SET_COOKIE));
    }
}

pub async fn send(router: &Router, request: Request<Body>) -> Answer {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Answer {
        status,
        headers,
        body,
    }
}

pub async fn get(router: &Router, path: &str) -> Answer {
    get_with(router, path, &[]).await
}

pub async fn get_with(
    router: &Router,
    path: &str,
    headers: &[(header::HeaderName, &str)],
) -> Answer {
    let mut request = Request::builder().method(Method::GET).uri(path);
    for (name, value) in headers {
        request = request.header(name, *value);
    }
    send(router, request.body(Body::empty()).unwrap()).await
}

/// A request in the making.
pub struct Ask {
    request: axum::http::request::Builder,
    body: Body,
}

/// Starts a request.
pub fn ask(method: Method, path: &str) -> Ask {
    Ask {
        request: Request::builder().method(method).uri(path),
        body: Body::empty(),
    }
}

impl Ask {
    /// Signs the request with a session token.
    pub fn token(self, token: &str) -> Self {
        self.header(header::AUTHORIZATION, &format!("Bearer {token}"))
    }

    pub fn header(mut self, name: header::HeaderName, value: &str) -> Self {
        self.request = self.request.header(name, value);
        self
    }

    /// Gives the request a JSON body.
    pub fn json(mut self, body: &impl Serialize) -> Self {
        self.body = Body::from(serde_json::to_vec(body).unwrap());
        self.header(header::CONTENT_TYPE, "application/json")
    }

    /// Gives the request a body of plain bytes.
    pub fn bytes(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Body::from(body.into());
        self
    }

    /// Makes the request arrive over a connection from the address `peer`.
    pub fn from(mut self, peer: &str) -> Self {
        let address: SocketAddr = format!("{peer}:50000")
            .parse()
            .or_else(|_| format!("[{peer}]:50000").parse())
            .unwrap();
        self.request = self.request.extension(ConnectInfo(address));
        self
    }

    pub async fn send(self, router: &Router) -> Answer {
        send(router, self.request.body(self.body).unwrap()).await
    }
}

pub fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Compares text with a golden file, or rewrites the file when
/// `VENTI_UPDATE_GOLDEN` is set. For answers that are not JSON.
pub fn assert_golden_text(name: &str, actual: &str) {
    let path = golden(name);
    if std::env::var_os("VENTI_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun with VENTI_UPDATE_GOLDEN=1 to create it",
            path.display()
        )
    });
    if expected != actual {
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .map_or_else(
                || expected.lines().count().min(actual.lines().count()) + 1,
                |i| i + 1,
            );
        panic!(
            "output differs from {} at line {line}\n\
             run with VENTI_UPDATE_GOLDEN=1 to accept the new output",
            path.display()
        );
    }
}

// --- Accounts and flights -----------------------------------------------------

/// A password long enough to be accepted.
pub const PASSWORD: &str = "correct horse battery";

/// The sample flight as a file.
pub fn sample_file() -> Vec<u8> {
    igc::write(&synthetic_flight()).into_bytes()
}

/// The sample flight flown on another day (written `ddmmyy`), which makes
/// it another flight.
pub fn sample_file_on(day: &str) -> Vec<u8> {
    let text = igc::write(&synthetic_flight());
    assert!(text.contains("HFDTEDATE:040726"));
    text.replace("HFDTEDATE:040726", &format!("HFDTEDATE:{day}"))
        .into_bytes()
}

/// Creates an account with [`PASSWORD`] on a server that takes anyone, and
/// returns what the server answered.
pub async fn sign_up(router: &Router, handle: &str) -> SignInResponse {
    let request = CreateAccountRequest {
        handle: handle.to_owned(),
        display_name: format!("Pilot {handle}"),
        password: Password::new(PASSWORD),
        invite: None,
        device: Some("Test device".to_owned()),
    };
    ask(Method::POST, ACCOUNTS_PATH)
        .json(&request)
        .send(router)
        .await
        .ok(StatusCode::CREATED)
}

/// Tries to sign in and returns the server's answer, whatever it is.
pub async fn try_sign_in(router: &Router, handle: &str, password: &str) -> Answer {
    let request = SignInRequest {
        handle: handle.to_owned(),
        password: Password::new(password),
        device: None,
    };
    ask(Method::POST, SESSIONS_PATH)
        .json(&request)
        .send(router)
        .await
}

/// Uploads a flight file for the account a token belongs to.
pub async fn upload(router: &Router, token: &str, file: &[u8]) -> UploadResponse {
    let answer = ask(Method::POST, FLIGHTS_PATH)
        .token(token)
        .bytes(file)
        .send(router)
        .await;
    assert!(
        answer.status == StatusCode::CREATED || answer.status == StatusCode::OK,
        "{} {}",
        answer.status,
        answer.start()
    );
    answer.json()
}

/// Sets who may see a flight.
pub async fn set_visibility(
    router: &Router,
    token: &str,
    flight: &FlightId,
    visibility: Visibility,
) {
    let answer = ask(Method::PATCH, &flight_path(flight))
        .token(token)
        .json(&UpdateFlightRequest { visibility })
        .send(router)
        .await;
    let updated: FlightResponse = answer.ok(StatusCode::OK);
    assert_eq!(updated.flight.visibility, visibility);
}
