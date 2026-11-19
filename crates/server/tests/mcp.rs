//! Tests of the connector: the Model Context Protocol through the real
//! router, over what the development mode serves.
//!
//! The protocol is tested in `venti-mcp`, against the specification's own
//! examples and schema. What is tested here is what the server makes of it:
//! which tools it offers, that a tool answers with the text the app's own
//! assistant reads, who may call, and what a call may cost. Some answers
//! are kept as golden files, which are also the examples in `docs/mcp.md`.

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderName, Method, StatusCode, header};
use serde_json::{Value, json};
use venti_server::accounts::Limits;
use venti_server::accounts::client::TrustedProxies;
use venti_server::adapters;
use venti_server::api::{Api, Cors, MCP_PATH, Settings};
use venti_server::dev;
use venti_server::flights::share::PublicUrl;
use venti_sources::testing::assert_golden;
use venti_store::testing::TroubledStore;
use venti_store::{MemoryStore, Store};
use venti_tools::{ToolError, airspace, wind};

mod common;

use common::{Answer, Ask, TestClock, WEB_APP, ask, get, golden, recorded, settings};

// --- Helpers ----------------------------------------------------------------

const VERSION: HeaderName = HeaderName::from_static("mcp-protocol-version");
const METHOD: HeaderName = HeaderName::from_static("mcp-method");
const NAME: HeaderName = HeaderName::from_static("mcp-name");
const FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// The revision in which every request stands alone.
const NEWEST: &str = "2026-07-28";

/// The revisions that begin with `initialize`, oldest first.
const OLDER: [&str; 3] = ["2025-03-26", "2025-06-18", "2025-11-25"];

/// What clients of every revision say they take for an answer.
const ACCEPT: &str = "application/json, text/event-stream";

fn message(id: u64, method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// A request as a client of an older revision sends it once the
/// conversation has begun. The oldest revision's clients name no revision.
fn older(revision: &str, method: &str, params: Value) -> Ask {
    let request = ask(Method::POST, MCP_PATH).header(header::ACCEPT, ACCEPT);
    let request = if revision == OLDER[0] {
        request
    } else {
        request.header(VERSION, revision)
    };
    request.json(&message(1, method, &params))
}

/// A request of the newest revision: it names its revision, its method and
/// the tool it calls in headers, and describes its client in `_meta`.
fn newest(method: &str, mut params: Value) -> Ask {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": NEWEST,
        "io.modelcontextprotocol/clientInfo": {"name": "test-client", "version": "1.0.0"},
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    let mut request = ask(Method::POST, MCP_PATH)
        .header(header::ACCEPT, ACCEPT)
        .header(VERSION, NEWEST)
        .header(METHOD, method);
    if let Some(name) = params.get("name").and_then(Value::as_str) {
        request = request.header(NAME, name);
    }
    request.json(&message(1, method, &params))
}

/// A request in any revision.
fn in_revision(revision: &str, method: &str, params: Value) -> Ask {
    if revision == NEWEST {
        newest(method, params)
    } else {
        older(revision, method, params)
    }
}

fn call(name: &str, arguments: Value) -> Value {
    json!({"name": name, "arguments": arguments})
}

/// The result of a request that worked.
fn result_of(answer: &Answer) -> Value {
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    let body: Value = answer.json();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert!(body.get("error").is_none(), "{body}");
    body["result"].clone()
}

/// The text of a tool call's result, and whether the call failed.
fn tool_result(answer: &Answer) -> (String, bool) {
    let result = result_of(answer);
    let content = result["content"].as_array().expect("a tool result");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "text");
    let text = content[0]["text"].as_str().unwrap().to_owned();
    (text, result["isError"].as_bool().unwrap())
}

/// The error of a request the protocol refused: its code and its message.
fn error_of(answer: &Answer, status: StatusCode) -> (i64, String) {
    assert_eq!(answer.status, status, "{}", answer.text());
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(answer.header(header::CACHE_CONTROL), "no-store");
    let body: Value = answer.json();
    assert_eq!(body["jsonrpc"], "2.0");
    assert!(body.get("result").is_none(), "{body}");
    let error = &body["error"];
    (
        error["code"].as_i64().unwrap(),
        error["message"].as_str().unwrap().to_owned(),
    )
}

/// What the app's own assistant reads for a call: the tool's request sent
/// to the API, and the tool's own function over the API's answer.
async fn in_the_app(router: &Router, name: &str, input: &Value) -> Result<String, ToolError> {
    let now = adapters::recorded_at();
    if wind::needs_server(name) {
        let asked = get(router, &wind::request(name, input)?).await;
        wind::result(name, input, asked.status.as_u16(), &asked.body, now)
    } else {
        let asked = get(router, &airspace::request(name, input)?).await;
        airspace::result(name, input, asked.status.as_u16(), &asked.body, now)
    }
}

/// The calls the tests make: wind around the Zugspitze, the last hours of
/// the station on its summit, and the airspace over Hochfilzen.
fn calls() -> [(&'static str, Value, &'static str); 3] {
    [
        (
            "wind_stations",
            json!({"latitude": 47.42, "longitude": 10.98, "radius_km": 60}),
            "mcp_wind_stations.json",
        ),
        (
            "station_history",
            json!({"station_id": "dwd-10min:05792", "hours": 2}),
            "mcp_station_history.json",
        ),
        (
            "airspace_at",
            json!({"latitude": 47.485, "longitude": 12.655, "radius_km": 15}),
            "mcp_airspace_at.json",
        ),
    ]
}

/// A server over an empty store whose limits a test can reach, with every
/// source of the registry, so that it offers every tool.
fn limited(clock: &TestClock, per_address: u32, proxies: &[&str]) -> Router {
    let settings = Settings {
        limits: Limits {
            connector_requests_per_address: per_address,
            ..Limits::default()
        },
        trusted_proxies: TrustedProxies::parse(proxies).unwrap(),
        ..settings()
    };
    over(MemoryStore::new(), clock, settings)
}

fn over<S: Store>(store: S, clock: &TestClock, settings: Settings) -> Router {
    let registry = adapters::registry();
    Api::new(
        Arc::new(store),
        Arc::new(clock.clone()),
        registry.manifests(),
        settings,
    )
    .router(&Cors::Origins(vec![WEB_APP.to_owned()]))
}

fn list() -> Ask {
    older("2025-11-25", "tools/list", json!({}))
}

// --- The tools ----------------------------------------------------------------

#[tokio::test]
async fn the_tools_are_the_apps_own_definitions() {
    let router = recorded().await;
    let defined: Vec<&venti_tools::Tool> = wind::SERVER_TOOLS
        .iter()
        .chain(&airspace::AIRSPACE_TOOLS)
        .collect();

    for revision in OLDER.into_iter().chain([NEWEST]) {
        let answer = in_revision(revision, "tools/list", json!({}))
            .send(&router)
            .await;
        let result = result_of(&answer);
        let listed = result["tools"].as_array().unwrap();

        let names: Vec<&str> = listed
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["wind_stations", "station_history", "airspace_at"]);
        for (tool, definition) in listed.iter().zip(&defined) {
            assert_eq!(tool["description"], definition.description, "{revision}");
            assert_eq!(tool["inputSchema"], definition.input_schema());
            // Every tool only reads, and reads the server's own data.
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert_eq!(tool["annotations"]["openWorldHint"], false);
        }
        // Nothing of a pilot's own is offered: the logbook's tools are the
        // app's, and stay there.
        for private in venti_tools::TOOLS {
            assert!(!names.contains(&private.name), "{}", private.name);
        }
        // The newest revision says how long the list may be kept.
        if revision == NEWEST {
            assert_eq!(result["resultType"], "complete");
            assert_eq!(result["ttlMs"], 3_600_000);
            assert_eq!(result["cacheScope"], "public");
        } else {
            assert!(result.get("resultType").is_none());
        }
    }
}

#[tokio::test]
async fn a_tool_answers_with_the_text_the_apps_assistant_reads() {
    let router = recorded().await;

    for (name, input, kept) in calls() {
        let in_app = in_the_app(&router, name, &input).await.unwrap();

        for revision in OLDER.into_iter().chain([NEWEST]) {
            let answer = in_revision(revision, "tools/call", call(name, input.clone()))
                .send(&router)
                .await;
            let (text, failed) = tool_result(&answer);
            assert!(!failed, "{name} in {revision}: {text}");
            assert_eq!(text, in_app, "{name} in {revision}");
        }

        // What the model reads, as the documentation shows it.
        let read: Value = serde_json::from_str(&in_app).unwrap();
        assert_golden(&golden(kept), &read);
    }
}

#[tokio::test]
async fn an_airspace_answer_carries_its_caveats_and_its_source() {
    let router = recorded().await;
    let (name, input, _) = calls().into_iter().nth(2).unwrap();

    let answer = newest("tools/call", call(name, input)).send(&router).await;
    let (text, _) = tool_result(&answer);
    let read: Value = serde_json::from_str(&text).unwrap();

    let names: Vec<&str> = read["over_the_place"]
        .as_array()
        .unwrap()
        .iter()
        .map(|airspace| airspace["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"LOR8 (HOCHFILZEN)"), "{names:?}");
    // Said in the result itself, where no model can leave it behind.
    let caveats = read["caveats"].to_string();
    for said in ["not certified", "not a clearance", "active today"] {
        assert!(caveats.contains(said), "{said}: {caveats}");
    }
    let data = &read["data"][0];
    assert!(data["source"].as_str().unwrap().contains("openAIP"));
    assert!(data["notice"].as_str().unwrap().contains("not certified"));
    assert!(data["coverage"].as_str().unwrap().contains("DE, AT"));
    assert_eq!(data["fetched_days_ago"], 0);

    // North of what the server collects for, the same question gets no
    // airspace and is told that this means nothing.
    let hamburg = call("airspace_at", json!({"latitude": 53.6, "longitude": 9.9}));
    let answer = newest("tools/call", hamburg).send(&router).await;
    let (text, failed) = tool_result(&answer);
    assert!(!failed);
    let read: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(read["over_the_place"], json!([]));
    assert!(
        read["note"]
            .as_str()
            .unwrap()
            .contains("holds no airspace data for this place")
    );
}

#[tokio::test]
async fn a_call_that_cannot_be_answered_is_a_result_that_says_why() {
    let router = recorded().await;

    for revision in ["2025-11-25", NEWEST] {
        // A station nobody has: what the app's assistant is told, word for
        // word.
        let unknown = json!({"station_id": "dwd-10min:99999"});
        let in_app = in_the_app(&router, "station_history", &unknown).await;
        let said = venti_tools::failure(&in_app.unwrap_err());
        let asked = call("station_history", unknown);
        let answer = in_revision(revision, "tools/call", asked)
            .send(&router)
            .await;
        assert_eq!(tool_result(&answer), (said.clone(), true));
        assert!(said.contains("Use an id from wind_stations"), "{said}");

        // Arguments the tool cannot use are the tool's to explain.
        for (name, arguments) in [
            ("wind_stations", json!({"latitude": 95, "longitude": 11})),
            ("wind_stations", json!({})),
            ("airspace_at", json!({"latitude": "47", "longitude": 11})),
            ("station_history", json!({"station_id": 5792})),
        ] {
            let in_app = in_the_app(&router, name, &arguments).await.unwrap_err();
            assert!(matches!(in_app, ToolError::BadArguments(_)), "{in_app}");
            let answer = in_revision(revision, "tools/call", call(name, arguments))
                .send(&router)
                .await;
            let (text, failed) = tool_result(&answer);
            assert!(failed, "{text}");
            assert_eq!(text, venti_tools::failure(&in_app));
        }
    }

    // A tool that does not exist is the protocol's to refuse, each
    // revision in its own way.
    let nothing = call("logbook_summary", json!({}));
    let answer = older("2025-11-25", "tools/call", nothing.clone())
        .send(&router)
        .await;
    let (code, said) = error_of(&answer, StatusCode::OK);
    assert_eq!(
        (code, said.as_str()),
        (-32602, "Unknown tool: logbook_summary")
    );
    let answer = newest("tools/call", nothing).send(&router).await;
    assert_eq!(error_of(&answer, StatusCode::BAD_REQUEST).0, -32602);
}

#[tokio::test]
async fn an_argument_is_never_an_address_to_fetch() {
    // The development mode has no network, and neither has anything the
    // connector is made of. What a tool takes is two numbers, a radius, an
    // identifier and a count; an address in their place is one more value
    // that is not one of those.
    let router = recorded().await;
    let metadata = "http://169.254.169.254/latest/meta-data/";

    let asked = call("station_history", json!({"station_id": metadata}));
    let answer = newest("tools/call", asked).send(&router).await;
    let (text, failed) = tool_result(&answer);
    assert!(failed);
    assert!(text.contains("There is no station with this id"), "{text}");

    // An address where a number belongs is a bad argument, and is not
    // repeated back.
    let asked = call(
        "wind_stations",
        json!({"latitude": metadata, "longitude": 11}),
    );
    let answer = newest("tools/call", asked).send(&router).await;
    let (text, failed) = tool_result(&answer);
    assert!(failed, "{text}");
    assert!(!text.contains("169.254"), "{text}");

    // An argument the tool does not have is not looked at: the answer is
    // the one without it.
    let place = json!({"latitude": 47.42, "longitude": 10.98});
    let mut with_address = place.clone();
    with_address["url"] = json!(metadata);
    let plain = newest("tools/call", call("wind_stations", place))
        .send(&router)
        .await;
    let extra = newest("tools/call", call("wind_stations", with_address))
        .send(&router)
        .await;
    assert_eq!(tool_result(&plain), tool_result(&extra));
    assert!(!tool_result(&extra).1);

    // And the handlers hold nothing that could fetch: the part of the
    // server that answers requests never names the part that asks
    // providers.
    let api = concat!(env!("CARGO_MANIFEST_DIR"), "/src/api");
    for file in std::fs::read_dir(api).unwrap() {
        let path = file.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        for fetcher in ["reqwest", "HttpFetch", "ingest::", "venti_sources::Fetch"] {
            assert!(
                !source.contains(fetcher),
                "{} names {fetcher}",
                path.display()
            );
        }
    }
}

// --- The two ways a conversation goes ------------------------------------------

#[tokio::test]
async fn the_older_revisions_begin_with_initialize() {
    let router = recorded().await;

    for revision in OLDER {
        // Before `initialize` a client has no revision to name.
        let hello = json!({
            "protocolVersion": revision,
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"},
        });
        let answer = older(OLDER[0], "initialize", hello).send(&router).await;
        let result = result_of(&answer);
        assert_eq!(result["protocolVersion"], revision);
        assert_eq!(result["capabilities"], json!({"tools": {}}));
        assert_eq!(result["serverInfo"]["name"], "venti");
        let instructions = result["instructions"].as_str().unwrap();
        assert!(instructions.contains("never a clearance"), "{instructions}");
        assert!(instructions.contains("knows nothing about the person"));
        // No session is begun: the server remembers nothing of a client.
        assert!(!answer.headers.contains_key("mcp-session-id"));
        if revision == "2025-11-25" {
            let body: Value = answer.json();
            assert_golden(&golden("mcp_initialize.json"), &body);
        }

        // The client says it is ready, which needs no answer.
        let ready = ask(Method::POST, MCP_PATH)
            .header(header::ACCEPT, ACCEPT)
            .json(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .send(&router)
            .await;
        assert_eq!(ready.status, StatusCode::ACCEPTED);
        assert!(ready.body.is_empty());

        let ping = older(revision, "ping", json!({})).send(&router).await;
        assert_eq!(result_of(&ping), json!({}));
    }

    // A client that asks for a revision the server does not speak is
    // offered the newest of this kind, and decides whether that will do.
    let future = json!({"protocolVersion": "2031-01-01", "capabilities": {},
                        "clientInfo": {"name": "test-client", "version": "1.0.0"}});
    let answer = older(OLDER[0], "initialize", future).send(&router).await;
    assert_eq!(result_of(&answer)["protocolVersion"], "2025-11-25");
}

#[tokio::test]
async fn the_newest_revision_needs_no_beginning() {
    let router = recorded().await;

    // What a client may ask first, and need not.
    let answer = newest("server/discover", json!({})).send(&router).await;
    let result = result_of(&answer);
    assert_eq!(
        result["supportedVersions"],
        json!(["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"])
    );
    assert_eq!(result["resultType"], "complete");
    let me = &result["_meta"]["io.modelcontextprotocol/serverInfo"];
    assert_eq!(me["name"], "venti");
    let body: Value = answer.json();
    assert_golden(&golden("mcp_discover.json"), &body);

    // A tool call with nothing before it.
    let (name, input, _) = calls().into_iter().next().unwrap();
    let answer = newest("tools/call", call(name, input)).send(&router).await;
    let result = result_of(&answer);
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["isError"], false);

    // The old beginning under the new revision's name: there is no such
    // method, and the answer says which revisions have it.
    let hello = json!({"protocolVersion": NEWEST, "capabilities": {},
                       "clientInfo": {"name": "test-client", "version": "1.0.0"}});
    let answer = newest("initialize", hello).send(&router).await;
    let (code, said) = error_of(&answer, StatusCode::NOT_FOUND);
    assert_eq!(code, -32601);
    assert!(said.contains("2025-11-25"), "{said}");

    // Headers that do not say what the body says.
    let mut params = call("wind_stations", json!({}));
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": NEWEST,
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    let wrong_tool = ask(Method::POST, MCP_PATH)
        .header(VERSION, NEWEST)
        .header(METHOD, "tools/call")
        .header(NAME, "airspace_at")
        .json(&message(1, "tools/call", &params))
        .send(&router)
        .await;
    assert_eq!(error_of(&wrong_tool, StatusCode::BAD_REQUEST).0, -32020);
    let no_method = ask(Method::POST, MCP_PATH)
        .header(VERSION, NEWEST)
        .json(&message(1, "tools/list", &json!({})))
        .send(&router)
        .await;
    assert_eq!(error_of(&no_method, StatusCode::BAD_REQUEST).0, -32020);

    // A revision nobody speaks here: the answer lists those that are.
    let answer = ask(Method::POST, MCP_PATH)
        .header(VERSION, "2031-01-01")
        .json(&message(1, "tools/list", &json!({})))
        .send(&router)
        .await;
    assert_eq!(error_of(&answer, StatusCode::BAD_REQUEST).0, -32022);
    let body: Value = answer.json();
    assert_eq!(body["error"]["data"]["supported"][0], NEWEST);
}

#[tokio::test]
async fn several_calls_in_one_request_are_for_the_oldest_revision_only() {
    let router = recorded().await;
    let [(wind_tool, wind_input, _), _, (air_tool, air_input, _)] = calls();
    let batch = json!([
        {"jsonrpc": "2.0", "id": "a", "method": "tools/call",
         "params": call(wind_tool, wind_input.clone())},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": "b", "method": "tools/call",
         "params": call(air_tool, air_input.clone())},
        {"jsonrpc": "2.0", "id": "c", "method": "ping"},
    ]);

    let answer = ask(Method::POST, MCP_PATH).json(&batch).send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);
    let responses: Vec<Value> = answer.json();
    let ids: Vec<&str> = responses
        .iter()
        .map(|response| response["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["a", "b", "c"]);
    // Each call got its own tool's answer.
    let text = |response: &Value| response["result"]["content"][0]["text"].clone();
    let wind_text = in_the_app(&router, wind_tool, &wind_input).await.unwrap();
    let air_text = in_the_app(&router, air_tool, &air_input).await.unwrap();
    assert_eq!(text(&responses[0]), wind_text.as_str());
    assert_eq!(text(&responses[1]), air_text.as_str());
    assert_eq!(responses[2]["result"], json!({}));

    // Later revisions took batches out again.
    let answer = ask(Method::POST, MCP_PATH)
        .header(VERSION, "2025-06-18")
        .json(&batch)
        .send(&router)
        .await;
    assert_eq!(error_of(&answer, StatusCode::BAD_REQUEST).0, -32600);

    // No batch is larger than the server says.
    let many: Vec<Value> = (0..17)
        .map(|n| json!({"jsonrpc": "2.0", "id": n, "method": "ping"}))
        .collect();
    let answer = ask(Method::POST, MCP_PATH).json(&many).send(&router).await;
    let (code, said) = error_of(&answer, StatusCode::BAD_REQUEST);
    assert_eq!(code, -32600);
    assert!(said.contains("16 at most"), "{said}");
}

#[tokio::test]
async fn there_is_no_stream_and_no_session() {
    let router = recorded().await;

    // A client that asks for a stream of the server's own messages, or
    // ends a session, is told that there is neither.
    for method in [Method::GET, Method::DELETE, Method::PUT] {
        let answer = ask(method, MCP_PATH)
            .header(header::ACCEPT, "text/event-stream")
            .send(&router)
            .await;
        let (code, _) = error_of(&answer, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(code, 405);
        assert_eq!(answer.header(header::ALLOW), "POST");
    }

    // A client that takes a stream only cannot be answered.
    let answer = ask(Method::POST, MCP_PATH)
        .header(header::ACCEPT, "text/event-stream")
        .json(&message(1, "ping", &json!({})))
        .send(&router)
        .await;
    assert_eq!(error_of(&answer, StatusCode::NOT_ACCEPTABLE).0, 406);

    // A body that is not said to be JSON is not read. A form on somebody
    // else's page can send text; it cannot send this content type.
    let answer = ask(Method::POST, MCP_PATH)
        .header(header::CONTENT_TYPE, "text/plain")
        .bytes(message(1, "ping", &json!({})).to_string())
        .send(&router)
        .await;
    assert_eq!(error_of(&answer, StatusCode::UNSUPPORTED_MEDIA_TYPE).0, 415);

    // A body that is not JSON, and one that is no message.
    let answer = ask(Method::POST, MCP_PATH)
        .header(header::CONTENT_TYPE, "application/json")
        .bytes("{not json")
        .send(&router)
        .await;
    assert_eq!(error_of(&answer, StatusCode::BAD_REQUEST).0, -32700);
    let answer = ask(Method::POST, MCP_PATH)
        .json(&json!({"jsonrpc": "2.0", "id": 1, "result": {}}))
        .send(&router)
        .await;
    assert_eq!(error_of(&answer, StatusCode::BAD_REQUEST).0, -32600);

    // Methods the server does not have: the protocol's own answer.
    for method in ["resources/list", "prompts/list", "sampling/createMessage"] {
        let answer = older("2025-11-25", method, json!({})).send(&router).await;
        assert_eq!(error_of(&answer, StatusCode::OK).0, -32601, "{method}");
        let answer = newest(method, json!({})).send(&router).await;
        assert_eq!(error_of(&answer, StatusCode::NOT_FOUND).0, -32601);
    }
}

// --- Who may call --------------------------------------------------------------

#[tokio::test]
async fn the_connector_knows_nobody() {
    let router = recorded().await;
    let (name, input, _) = calls().into_iter().next().unwrap();

    // A session token changes nothing: it is not looked at.
    let plain = newest("tools/call", call(name, input.clone()))
        .send(&router)
        .await;
    let signed = newest("tools/call", call(name, input))
        .token(dev::DEMO_TOKEN)
        .send(&router)
        .await;
    assert_eq!(plain.body, signed.body);
    assert_eq!(signed.status, StatusCode::OK);
    // Neither is a wrong one refused: there is nothing to sign in to.
    let wrong = list().token("venti_not-a-token").send(&router).await;
    assert_eq!(wrong.status, StatusCode::OK);

    // The server sets no cookie and asks for no sign-in.
    for answer in [&plain, &signed, &wrong] {
        assert!(!answer.headers.contains_key(header::SET_COOKIE));
        assert!(!answer.headers.contains_key(header::WWW_AUTHENTICATE));
    }
    // The demo account's flights are not in anything the tools return.
    let text = String::from_utf8(signed.body).unwrap();
    assert!(!text.contains(dev::DEMO_HANDLE), "{text}");
}

#[tokio::test]
async fn a_page_in_a_browser_may_call_only_from_where_the_api_may_be_called() {
    // The web app's origin and the server's own address.
    let settings = Settings {
        public_url: PublicUrl::new("https://api.venti.example").unwrap(),
        ..settings()
    };
    let router = over(MemoryStore::new(), &TestClock::new(), settings);
    let from = |origin: &'static str| list().header(header::ORIGIN, origin);

    for allowed in [WEB_APP, "https://api.venti.example"] {
        let answer = from(allowed).send(&router).await;
        assert_eq!(answer.status, StatusCode::OK, "{allowed}");
    }
    for refused in [
        "https://evil.example",
        "https://venti.example.evil.example",
        "http://venti.example",
        "http://localhost:5173",
        "null",
    ] {
        let answer = from(refused).send(&router).await;
        let (code, _) = error_of(&answer, StatusCode::FORBIDDEN);
        assert_eq!(code, 403, "{refused}");
    }
    // A program that is not a browser names no origin. That is every
    // assistant, and it is answered.
    assert_eq!(list().send(&router).await.status, StatusCode::OK);

    // A page that may call is told so before it sends the protocol's own
    // headers.
    let preflight = ask(Method::OPTIONS, MCP_PATH)
        .header(header::ORIGIN, WEB_APP)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "content-type, mcp-protocol-version, mcp-method",
        )
        .send(&router)
        .await;
    assert_eq!(preflight.status, StatusCode::OK);
    let headers = preflight.header(header::ACCESS_CONTROL_ALLOW_HEADERS);
    for name in [
        "content-type",
        "mcp-protocol-version",
        "mcp-method",
        "mcp-name",
    ] {
        assert!(headers.contains(name), "{name}: {headers}");
    }
}

#[tokio::test]
async fn the_development_mode_takes_pages_of_the_developers_own_machine() {
    // Its API may be read by any page: it holds test data only. The
    // connector follows the protocol's rule all the same.
    let api = dev::recorded_api(settings()).await.unwrap();
    let router = api.router(&Cors::Any);
    let from = |origin: &'static str| list().header(header::ORIGIN, origin);

    for allowed in [
        "http://localhost:5173",
        "http://127.0.0.1:6274",
        "http://[::1]:3000",
    ] {
        let answer = from(allowed).send(&router).await;
        assert_eq!(answer.status, StatusCode::OK, "{allowed}");
    }
    for refused in ["https://evil.example", "http://localhost.evil.example"] {
        let answer = from(refused).send(&router).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{refused}");
    }
}

// --- What a call may cost ------------------------------------------------------

/// Checks that a request was refused for being one too many.
fn too_many(answer: &Answer) {
    let (code, said) = error_of(answer, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(code, 429);
    assert!(said.contains("Too many requests"), "{said}");
    let wait: u64 = answer.header(header::RETRY_AFTER).parse().unwrap();
    assert!((1..=60).contains(&wait), "{wait}");
}

#[tokio::test]
async fn requests_are_counted_by_the_address_they_come_from() {
    let clock = TestClock::new();
    let router = limited(&clock, 3, &[]);
    let from = |peer: &'static str| list().from(peer);

    for _ in 0..3 {
        let answer = from("203.0.113.5").send(&router).await;
        assert_eq!(answer.status, StatusCode::OK);
    }
    too_many(&from("203.0.113.5").send(&router).await);
    // A request over the limit is not looked at, whatever it is.
    let get = ask(Method::GET, MCP_PATH).from("203.0.113.5");
    too_many(&get.send(&router).await);

    // Somebody else is not held up by it.
    let answer = from("203.0.113.6").send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);
    // What a client writes into a header is not its address.
    let lying = from("203.0.113.5").header(FORWARDED_FOR, "198.51.100.1");
    too_many(&lying.send(&router).await);
    // The rest of the API is not affected.
    let health = ask(Method::GET, "/v1/health").from("203.0.113.5");
    assert_eq!(health.send(&router).await.status, StatusCode::OK);

    // A minute later the address may ask again.
    clock.advance(chrono::Duration::seconds(61));
    let answer = from("203.0.113.5").send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn behind_the_proxy_the_address_is_the_one_the_proxy_names() {
    let clock = TestClock::new();
    let router = limited(&clock, 3, &["172.18.0.2"]);
    let through_caddy =
        |client: &'static str| list().from("172.18.0.2").header(FORWARDED_FOR, client);

    for _ in 0..3 {
        let answer = through_caddy("198.51.100.7").send(&router).await;
        assert_eq!(answer.status, StatusCode::OK);
    }
    too_many(&through_caddy("198.51.100.7").send(&router).await);
    // Everyone comes through the same proxy, and each is counted alone.
    let answer = through_caddy("198.51.100.8").send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn every_call_of_a_batch_counts() {
    let clock = TestClock::new();
    let router = limited(&clock, 4, &[]);
    let calls: Vec<Value> = (0..3)
        .map(|n| {
            let arguments = json!({"latitude": 47.4, "longitude": 11.0});
            json!({"jsonrpc": "2.0", "id": n, "method": "tools/call",
                   "params": call("wind_stations", arguments)})
        })
        .collect();

    let answer = ask(Method::POST, MCP_PATH)
        .from("203.0.113.5")
        .json(&calls)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    let responses: Vec<Value> = answer.json();
    assert_eq!(responses.len(), 3);

    // Three of four are used up: one more request, and that is all.
    let answer = list().from("203.0.113.5").send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);
    too_many(&list().from("203.0.113.5").send(&router).await);
}

#[tokio::test]
async fn a_request_is_bounded_in_size() {
    let router = recorded().await;
    let padded = |bytes: usize| {
        let params = json!({"cursor": null, "padding": "x".repeat(bytes)});
        message(1, "tools/list", &params).to_string()
    };
    let post = |body: String| {
        ask(Method::POST, MCP_PATH)
            .header(header::CONTENT_TYPE, "application/json")
            .bytes(body)
    };

    // A client may describe itself at length.
    let answer = post(padded(60_000)).send(&router).await;
    assert_eq!(answer.status, StatusCode::OK);

    // Past 64 kB the request is refused, whether it says how long it is or
    // not, and in the protocol's shape either way.
    let large = padded(70_000);
    let said = post(large.clone())
        .header(header::CONTENT_LENGTH, &large.len().to_string())
        .send(&router)
        .await;
    assert_eq!(error_of(&said, StatusCode::PAYLOAD_TOO_LARGE).0, 413);
    let unsaid = post(large).send(&router).await;
    assert_eq!(error_of(&unsaid, StatusCode::PAYLOAD_TOO_LARGE).0, 413);
}

#[tokio::test]
async fn a_database_that_is_down_is_a_tool_result_that_says_try_later() {
    let router = over(TroubledStore::down(), &TestClock::new(), settings());

    // What needs no database is answered as ever.
    assert_eq!(list().send(&router).await.status, StatusCode::OK);

    for (name, input, _) in calls() {
        let answer = newest("tools/call", call(name, input)).send(&router).await;
        let (text, failed) = tool_result(&answer);
        assert!(failed);
        assert_eq!(
            text,
            r#"{"error":"The Venti server could not answer (unavailable)."}"#
        );
        // What exactly failed is the operator's business.
        assert!(!text.contains("connection refused"), "{text}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_call_that_takes_too_long_is_given_up() {
    let slow = TroubledStore::slow(std::time::Duration::from_secs(3600));
    let router = over(slow, &TestClock::new(), settings());
    let (name, input, _) = calls().into_iter().next().unwrap();
    let began = tokio::time::Instant::now();

    let answer = newest("tools/call", call(name, input)).send(&router).await;

    let (code, said) = error_of(&answer, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code, 503);
    assert!(said.contains("in time"), "{said}");
    assert_eq!(answer.header(header::RETRY_AFTER), "30");
    assert_eq!(began.elapsed(), std::time::Duration::from_secs(10));
}

// --- The documentation -------------------------------------------------------

/// `docs/mcp.md` says that its examples are the development mode's
/// answers and names the bounds on a request. This is what holds it to
/// that.
#[test]
fn the_documentation_shows_the_connector_as_it_is() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/mcp.md");
    let docs = std::fs::read_to_string(path).unwrap();
    for name in [
        "mcp_initialize.json",
        "mcp_discover.json",
        "mcp_wind_stations.json",
        "mcp_station_history.json",
        "mcp_airspace_at.json",
    ] {
        let answer = std::fs::read_to_string(golden(name)).unwrap();
        assert!(
            docs.contains(answer.trim_end()),
            "docs/mcp.md does not show {name} as the tests keep it; \
             copy the file from crates/server/tests/golden into the document"
        );
    }
    for tool in wind::SERVER_TOOLS.iter().chain(&airspace::AIRSPACE_TOOLS) {
        assert!(
            docs.contains(&format!("| `{}` |", tool.name)),
            "{}",
            tool.name
        );
    }
    // The bounds it names are the ones the server keeps.
    let limits = Limits::default();
    let per_minute = limits.connector_requests_per_address;
    assert_eq!(limits.connector_window, chrono::Duration::minutes(1));
    assert!(docs.contains(&format!("| {per_minute} a minute |")));
    let per_network = limits.connector_requests_per_network;
    assert!(docs.contains(&format!("| {per_network} a minute |")));
    assert!(docs.contains(&format!("| {}, and each counts", venti_mcp::MAX_BATCH)));
    for caveat in airspace::CAVEATS {
        assert!(docs.contains(caveat), "{caveat}");
    }
}
