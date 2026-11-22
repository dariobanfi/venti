//! What the tests of the authorization server share: a program that
//! registers itself, a pilot who agrees on the server's page, the exchange
//! of the code, and a call of the connector with the token that came of it.

use axum::Router;
use axum::http::{HeaderName, Method, StatusCode, header};
use serde_json::{Value, json};
use venti_server::api::{AUTHORIZE_PATH, MCP_PATH, REGISTER_PATH, REVOKE_PATH, TOKEN_PATH};

use super::{Answer, PASSWORD, PUBLIC_URL, ask};

/// The verifier and the challenge of RFC 7636, appendix B.
pub const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
pub const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

/// The connector's address on the tests' server: what tokens are for.
pub const CONNECTOR: &str = "https://venti.example/mcp";

/// Where the tests' program wants the pilot sent back to.
pub const REDIRECT: &str = "https://assistant.example/oauth/callback";

/// What the tests' program sends along and expects back unchanged.
pub const STATE: &str = "state-7f3a/+=&?";

const FORM: &str = "application/x-www-form-urlencoded";

/// Text as a value in a query or a form.
pub fn encoded(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The text a value in a query stands for.
pub fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut plain = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap();
            plain.push(u8::from_str_radix(hex, 16).unwrap());
            at += 3;
        } else {
            plain.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(plain).unwrap()
}

/// Names and values as a query or the body of a form.
pub fn form(pairs: &[(&str, String)]) -> String {
    let pairs: Vec<String> = pairs
        .iter()
        .map(|(name, value)| format!("{name}={}", encoded(value)))
        .collect();
    pairs.join("&")
}

/// A program's registration, whatever the server makes of it.
pub async fn register(router: &Router, registration: &Value) -> Answer {
    ask(Method::POST, REGISTER_PATH)
        .json(registration)
        .send(router)
        .await
}

/// Registers a program under `name` that is sent back to `redirect`, and
/// returns the identifier it got.
pub async fn registered_at(router: &Router, name: &str, redirect: &str) -> String {
    let answer = register(
        router,
        &json!({
            "client_name": name,
            "redirect_uris": [redirect],
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
        }),
    )
    .await;
    let registered: Value = answer.ok(StatusCode::CREATED);
    registered["client_id"].as_str().unwrap().to_owned()
}

/// Registers a program that is sent back to [`REDIRECT`].
pub async fn registered(router: &Router, name: &str) -> String {
    registered_at(router, name, REDIRECT).await
}

/// What a program sends the pilot's browser to the page with.
pub type Request = Vec<(&'static str, String)>;

/// A request every part of which is good.
pub fn request(client_id: &str) -> Request {
    vec![
        ("response_type", "code".to_owned()),
        ("client_id", client_id.to_owned()),
        ("redirect_uri", REDIRECT.to_owned()),
        ("scope", "flights:read".to_owned()),
        ("state", STATE.to_owned()),
        ("code_challenge", CHALLENGE.to_owned()),
        ("code_challenge_method", "S256".to_owned()),
        ("resource", CONNECTOR.to_owned()),
    ]
}

/// The request with one part changed or added.
pub fn with(mut request: Request, name: &'static str, value: &str) -> Request {
    request.retain(|(part, _)| *part != name);
    request.push((name, value.to_owned()));
    request
}

/// The request with one part left out.
pub fn without(mut request: Request, name: &str) -> Request {
    request.retain(|(part, _)| *part != name);
    request
}

/// The page the pilot's browser is shown for a request.
pub async fn page(router: &Router, request: &Request) -> Answer {
    ask(Method::GET, &format!("{AUTHORIZE_PATH}?{}", form(request)))
        .send(router)
        .await
}

/// What a browser keeps of the page: the value against forged answers in
/// the form, and the cookie that came with it.
pub struct Shown {
    pub csrf: String,
    pub cookie: String,
}

/// The value of a hidden field of a page.
pub fn hidden(html: &str, name: &str) -> Option<String> {
    let start = format!("<input type=\"hidden\" name=\"{name}\" value=\"");
    let rest = &html[html.find(&start)? + start.len()..];
    Some(rest[..rest.find('"')?].to_owned())
}

pub fn shown(answer: &Answer) -> Shown {
    let csrf = hidden(answer.text(), "csrf").expect("the page has a form");
    let cookie = answer.header(header::SET_COOKIE);
    let cookie = cookie.split(';').next().unwrap().to_owned();
    assert_eq!(cookie, format!("__Host-venti-consent={csrf}"));
    Shown { csrf, cookie }
}

/// A form sent to the page as the pilot's own browser sends it.
pub async fn answer_page(router: &Router, fields: &[(&str, String)], cookie: &str) -> Answer {
    ask(Method::POST, AUTHORIZE_PATH)
        .header(header::CONTENT_TYPE, FORM)
        .header(header::ORIGIN, PUBLIC_URL)
        .header(header::COOKIE, cookie)
        .bytes(form(fields))
        .send(router)
        .await
}

/// The pilot's answer to the page for `request`: the handle and password
/// typed, and the button pressed (`agree` or `refuse`).
pub async fn decide(
    router: &Router,
    request: &Request,
    handle: &str,
    password: &str,
    decision: &str,
) -> Answer {
    let page = page(router, request).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    let shown = shown(&page);
    let mut fields: Vec<(&str, String)> = request.clone();
    fields.push(("csrf", shown.csrf));
    fields.push(("handle", handle.to_owned()));
    fields.push(("password", password.to_owned()));
    fields.push(("decision", decision.to_owned()));
    answer_page(router, &fields, &shown.cookie).await
}

/// What the pilot's browser carries back to the program: the address it
/// is sent to, and what the server added to it.
pub fn sent_back(answer: &Answer) -> (String, Vec<(String, String)>) {
    assert_eq!(answer.status, StatusCode::SEE_OTHER, "{}", answer.text());
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    assert_eq!(answer.header(header::REFERRER_POLICY), "no-referrer");
    let location = answer.header(header::LOCATION);
    let (address, query) = location.split_once('?').expect("a query");
    let carried = query
        .split('&')
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap();
            (name.to_owned(), decoded(value))
        })
        .collect();
    (address.to_owned(), carried)
}

/// One of the things carried back.
pub fn carried<'a>(back: &'a [(String, String)], name: &str) -> Option<&'a str> {
    back.iter()
        .find(|(part, _)| part == name)
        .map(|(_, value)| value.as_str())
}

/// The pilot `handle` signs in on the page with the tests' password and
/// agrees; returns the code the program is sent.
pub async fn agree(router: &Router, client_id: &str, handle: &str) -> String {
    let answer = decide(router, &request(client_id), handle, PASSWORD, "agree").await;
    let (address, back) = sent_back(&answer);
    assert_eq!(address, REDIRECT);
    assert_eq!(carried(&back, "state"), Some(STATE));
    assert_eq!(carried(&back, "iss"), Some(PUBLIC_URL));
    carried(&back, "code").expect("a code").to_owned()
}

/// A request to the token endpoint.
pub async fn token_request(router: &Router, fields: &[(&str, String)]) -> Answer {
    ask(Method::POST, TOKEN_PATH)
        .header(header::CONTENT_TYPE, FORM)
        .bytes(form(fields))
        .send(router)
        .await
}

/// What a program sends to exchange a code, every part of it good.
pub fn exchange_of(client_id: &str, code: &str) -> Request {
    vec![
        ("grant_type", "authorization_code".to_owned()),
        ("client_id", client_id.to_owned()),
        ("code", code.to_owned()),
        ("code_verifier", VERIFIER.to_owned()),
        ("resource", CONNECTOR.to_owned()),
    ]
}

/// What a program sends to use a refresh token.
pub fn refresh_of(client_id: &str, refresh: &str) -> Request {
    vec![
        ("grant_type", "refresh_token".to_owned()),
        ("client_id", client_id.to_owned()),
        ("refresh_token", refresh.to_owned()),
        ("resource", CONNECTOR.to_owned()),
    ]
}

/// The tokens a program holds.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
}

/// Reads the tokens out of an answer of the token endpoint that gave some.
pub fn tokens_of(answer: &Answer) -> Tokens {
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    let body: Value = answer.json();
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], 3600);
    assert_eq!(body["scope"], "flights:read");
    Tokens {
        access: body["access_token"].as_str().unwrap().to_owned(),
        refresh: body["refresh_token"].as_str().unwrap().to_owned(),
    }
}

/// Checks that the token endpoint refused, with OAuth's code for why.
pub fn refused(answer: &Answer, status: StatusCode, code: &str) {
    assert_eq!(answer.status, status, "{}", answer.text());
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    let body: Value = answer.json();
    assert_eq!(body["error"], code, "{body}");
    assert!(body.get("access_token").is_none());
}

/// The whole way: the pilot `handle` agrees to the program `client_id`,
/// and the program exchanges its code.
pub async fn connect(router: &Router, client_id: &str, handle: &str) -> Tokens {
    let code = agree(router, client_id, handle).await;
    tokens_of(&token_request(router, &exchange_of(client_id, &code)).await)
}

/// A program gives a token up.
pub async fn revoke(router: &Router, client_id: &str, token: &str) -> Answer {
    ask(Method::POST, REVOKE_PATH)
        .header(header::CONTENT_TYPE, FORM)
        .bytes(form(&[
            ("client_id", client_id.to_owned()),
            ("token", token.to_owned()),
        ]))
        .send(router)
        .await
}

// --- The connector ------------------------------------------------------------

const VERSION: HeaderName = HeaderName::from_static("mcp-protocol-version");
const METHOD: HeaderName = HeaderName::from_static("mcp-method");
const NAME: HeaderName = HeaderName::from_static("mcp-name");

/// A request to the connector in the protocol's newest revision, with a
/// bearer token or without.
pub async fn connector(
    router: &Router,
    token: Option<&str>,
    method: &str,
    params: Value,
) -> Answer {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "test-client", "version": "1.0.0"},
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    let mut request = ask(Method::POST, MCP_PATH)
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header(VERSION, "2026-07-28")
        .header(METHOD, method);
    if let Some(name) = params.get("name").and_then(Value::as_str) {
        request = request.header(NAME, name);
    }
    if let Some(token) = token {
        request = request.token(token);
    }
    request
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send(router)
        .await
}

/// A tool call, with a bearer token or without.
pub async fn call(router: &Router, token: Option<&str>, tool: &str, arguments: Value) -> Answer {
    let params = json!({"name": tool, "arguments": arguments});
    connector(router, token, "tools/call", params).await
}

/// The text of a tool call's result, and whether the call failed.
pub fn tool_text(answer: &Answer) -> (String, bool) {
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    let body: Value = answer.json();
    let result = &body["result"];
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no tool result: {body}"))
        .to_owned();
    (text, result["isError"].as_bool().unwrap())
}

/// The result of a tool call that worked, which is JSON.
pub fn tool_json(answer: &Answer) -> Value {
    let (text, failed) = tool_text(answer);
    assert!(!failed, "{text}");
    serde_json::from_str(&text).unwrap()
}

/// Checks that the connector asked for a token: `401`, with the header
/// that names where the connector says who gives tokens out for it.
pub fn asks_for_a_token(answer: &Answer) -> String {
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{}", answer.text());
    let challenge = answer.header(header::WWW_AUTHENTICATE).to_owned();
    assert!(challenge.starts_with("Bearer "), "{challenge}");
    assert!(
        challenge.contains(
            "resource_metadata=\"https://venti.example/.well-known/oauth-protected-resource/mcp\""
        ),
        "{challenge}"
    );
    assert!(challenge.contains("scope=\"flights:read\""), "{challenge}");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    challenge
}
