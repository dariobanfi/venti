//! The protocol, request by request, without a server.
//!
//! Each test hands [`Server::read`] an HTTP request as a client would send
//! it and looks at what comes back. The answers that a client has to be
//! able to read, one for each revision that shapes them differently, are
//! kept as golden files in `tests/golden`; `docs/mcp.md` says how they were
//! checked against the specification's own schema. Rewrite them with
//! `VENTI_UPDATE_GOLDEN=1` and review the diff.

use std::path::PathBuf;

use serde_json::{Value, json};
use venti_mcp::{
    Answer, Calls, HEADER_MISMATCH, HttpRequest, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    Implementation, MAX_BATCH, METHOD_NOT_FOUND, Origins, PARSE_ERROR, Read, Revision, Server,
    Tool, ToolResult, UNSUPPORTED_PROTOCOL_VERSION,
};

// --- The scene ----------------------------------------------------------------

const MODERN: &str = "2026-07-28";
const LEGACY: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
const JSON: (&str, &str) = ("content-type", "application/json");
const ACCEPT: (&str, &str) = ("accept", "application/json, text/event-stream");

fn server() -> Server {
    let tool = |name: &str, description: &str, schema: Value| Tool {
        name: name.to_owned(),
        description: description.to_owned(),
        input_schema: schema,
        read_only: true,
        open_world: false,
    };
    Server::new(
        Implementation {
            name: "example".to_owned(),
            title: "Example Server".to_owned(),
            version: "1.2.3".to_owned(),
        },
        "Answers questions about the weather.",
        vec![
            tool(
                "get_weather",
                "Get current weather information for a location.",
                json!({
                    "type": "object",
                    "properties": {
                        "location": {"type": "string", "description": "City name or zip code"}
                    },
                    "required": ["location"],
                    "additionalProperties": false
                }),
            ),
            tool(
                "get_time",
                "The current time.",
                json!({"type": "object", "additionalProperties": false}),
            ),
        ],
        Origins::Only(vec!["https://app.example".to_owned()]),
    )
}

fn post(headers: &[(&str, &str)], body: &Value) -> Read {
    let body = body.to_string();
    post_bytes(headers, body.as_bytes())
}

fn post_bytes(headers: &[(&str, &str)], body: &[u8]) -> Read {
    let mut all = vec![JSON, ACCEPT];
    all.extend_from_slice(headers);
    server().read(&HttpRequest {
        method: "POST",
        headers: &all,
        body,
    })
}

fn answered(read: Read) -> Answer {
    match read {
        Read::Answer(answer) => answer,
        Read::Calls(calls) => panic!("expected an answer, got calls: {:?}", calls.calls()),
    }
}

fn calls(read: Read) -> Calls {
    match read {
        Read::Calls(calls) => calls,
        Read::Answer(answer) => panic!(
            "expected calls, got {} {}",
            answer.status,
            String::from_utf8_lossy(&answer.body)
        ),
    }
}

fn ok(text: &str) -> ToolResult {
    ToolResult {
        text: text.to_owned(),
        is_error: false,
    }
}

/// Checks the status of an answer and returns its JSON body.
fn body(answer: &Answer, status: u16) -> Value {
    assert_eq!(
        answer.status,
        status,
        "{}",
        String::from_utf8_lossy(&answer.body)
    );
    assert!(
        answer
            .headers
            .contains(&("content-type", "application/json".to_owned()))
    );
    answer.body_json().expect("a JSON body")
}

/// Checks that an answer is the error with `code`, and returns its message.
fn error(answer: &Answer, status: u16, code: i64) -> String {
    let body = body(answer, status);
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["error"]["code"], code, "{body}");
    assert!(body.get("result").is_none());
    body["error"]["message"].as_str().unwrap().to_owned()
}

/// A request as the newest revision writes it: the revision and what the
/// client can do travel in `_meta`.
fn modern(id: Value, method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": MODERN,
        "io.modelcontextprotocol/clientInfo": {"name": "ExampleClient", "version": "1.0.0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// The headers the newest revision asks for with a request.
fn modern_headers(method: &str) -> Vec<(&str, &str)> {
    vec![("mcp-protocol-version", MODERN), ("mcp-method", method)]
}

fn legacy(id: Value, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn initialize(revision: &str) -> Value {
    legacy(
        json!(1),
        "initialize",
        json!({
            "protocolVersion": revision,
            "capabilities": {"roots": {"listChanged": true}, "sampling": {}},
            "clientInfo": {"name": "ExampleClient", "version": "1.0.0"}
        }),
    )
}

/// Compares an answer with a golden file, or rewrites the file when
/// `VENTI_UPDATE_GOLDEN` is set.
fn assert_golden(name: &str, answer: &Answer) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let value = json!({"status": answer.status, "body": answer.body_json()});
    let mut actual = serde_json::to_string_pretty(&value).unwrap();
    actual.push('\n');
    if std::env::var_os("VENTI_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun with VENTI_UPDATE_GOLDEN=1 to create it",
            path.display()
        )
    });
    assert!(
        expected == actual,
        "output differs from {}\nrun with VENTI_UPDATE_GOLDEN=1 to accept the new output",
        path.display()
    );
}

// --- Before a body is read ------------------------------------------------------

#[test]
fn a_page_of_an_unexpected_origin_is_refused() {
    let list = legacy(json!(1), "tools/list", json!({}));
    // Programs that are not browsers send no Origin.
    body(&answered(post(&[], &list)), 200);
    // The page the server expects, however its browser spells it.
    for origin in ["https://app.example", "HTTPS://APP.EXAMPLE"] {
        body(&answered(post(&[("Origin", origin)], &list)), 200);
    }
    for origin in [
        "https://evil.example",
        "http://app.example",
        "https://app.example:8443",
        "https://app.example.evil.example",
        "http://localhost:6274",
        "null",
        "",
    ] {
        let refused = answered(post(&[("origin", origin)], &list));
        let message = error(&refused, 403, 403);
        assert!(message.contains("origin"), "{origin}: {message}");
        assert!(refused.body_json().unwrap().get("id").is_none());
    }
    // The origin is looked at whatever the method is.
    let listening = server().read(&HttpRequest {
        method: "GET",
        headers: &[("origin", "https://evil.example")],
        body: b"",
    });
    error(&answered(listening), 403, 403);
}

#[test]
fn a_server_on_a_developers_machine_takes_pages_of_that_machine_only() {
    let local = Server::new(
        Implementation {
            name: "example".to_owned(),
            title: "Example".to_owned(),
            version: "1".to_owned(),
        },
        "",
        Vec::new(),
        Origins::Local,
    );
    let text = legacy(json!(1), "tools/list", json!({})).to_string();
    let from = |origin: &str| {
        let request = HttpRequest {
            method: "POST",
            headers: &[JSON, ("origin", origin)],
            body: text.as_bytes(),
        };
        answered(local.read(&request)).status
    };

    // Whatever port a tool on this machine is served from.
    for origin in [
        "http://localhost:6274",
        "http://localhost",
        "http://127.0.0.1:5173",
        "https://127.0.0.1",
        "http://[::1]:8080",
        "HTTP://LOCALHOST:6274",
    ] {
        assert_eq!(from(origin), 200, "{origin}");
    }
    // A page from elsewhere, whatever name it gives itself. A name that
    // was made to lead to this machine is still not this machine's.
    for origin in [
        "http://evil.example.com",
        "https://localhost.evil.example",
        "http://localhost.evil.example:6274",
        "http://127.0.0.1.evil.example",
        "http://127.0.0.10",
        "http://localhost:",
        "http://localhost:80a",
        "http://localhost/path",
        "ftp://localhost",
        "localhost",
        "null",
        "",
    ] {
        assert_eq!(from(origin), 403, "{origin}");
    }
}

#[test]
fn only_post_is_answered_and_there_is_no_stream_and_no_session_to_end() {
    for method in ["GET", "DELETE", "PUT", "PATCH", "HEAD", "get"] {
        // As a client of an older revision asks for a stream, or ends its
        // session, and as one of the newest would if it tried.
        for headers in [
            vec![("accept", "text/event-stream")],
            vec![
                ("accept", "text/event-stream"),
                ("mcp-protocol-version", "2025-11-25"),
                ("mcp-session-id", "1868a90c"),
                ("last-event-id", "7"),
            ],
            vec![("mcp-protocol-version", MODERN)],
        ] {
            let refused = answered(server().read(&HttpRequest {
                method,
                headers: &headers,
                body: b"",
            }));
            let message = error(&refused, 405, 405);
            assert!(message.contains("POST"), "{message}");
            assert!(refused.headers.contains(&("allow", "POST".to_owned())));
        }
    }
}

#[test]
fn a_body_must_be_said_to_be_json_and_the_client_must_take_json() {
    let list = legacy(json!(1), "tools/list", json!({})).to_string();
    let send = |headers: &[(&str, &str)]| {
        answered(server().read(&HttpRequest {
            method: "POST",
            headers,
            body: list.as_bytes(),
        }))
    };

    // What a form on another site can send without asking first.
    for content_type in [
        "text/plain",
        "application/x-www-form-urlencoded",
        "multipart/form-data",
    ] {
        let message = error(&send(&[("content-type", content_type)]), 415, 415);
        assert!(message.contains("application/json"), "{message}");
    }
    error(&send(&[]), 415, 415);
    body(
        &send(&[("Content-Type", "application/json; charset=utf-8")]),
        200,
    );

    // No Accept header means anything will do.
    body(&send(&[JSON]), 200);
    body(&send(&[JSON, ("accept", "*/*")]), 200);
    let message = error(&send(&[JSON, ("accept", "text/event-stream")]), 406, 406);
    assert!(message.contains("application/json"), "{message}");
}

#[test]
fn a_body_that_is_not_a_message_is_refused_as_its_revision_writes_errors() {
    // Not JSON at all. The two oldest revisions want an id in every error
    // and get JSON-RPC's null; the others leave it out.
    for (headers, id) in [
        (vec![], Some(Value::Null)),
        (
            vec![("mcp-protocol-version", "2025-06-18")],
            Some(Value::Null),
        ),
        (vec![("mcp-protocol-version", "2025-11-25")], None),
        (vec![("mcp-protocol-version", MODERN)], None),
        (vec![("mcp-protocol-version", "1900-01-01")], None),
    ] {
        let refused = answered(post_bytes(&headers, b"{\"jsonrpc\": \"2.0\", "));
        error(&refused, 400, PARSE_ERROR);
        assert_eq!(refused.body_json().unwrap().get("id").cloned(), id);
    }
    assert!(matches!(post_bytes(&[], b""), Read::Answer(a) if a.status == 400));

    // JSON, and no message.
    for value in [
        json!("tools/list"),
        json!(7),
        json!(null),
        json!({}),
        json!({"jsonrpc": "2.0", "id": 1}),
        json!({"jsonrpc": "1.0", "id": 1, "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": null, "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": 1.5, "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": []}),
    ] {
        for headers in [vec![], modern_headers("ping")] {
            let message = error(&answered(post(&headers, &value)), 400, INVALID_REQUEST);
            assert!(message.starts_with("Invalid request: "), "{message}");
        }
    }
    // The id of a refused message is named when it has a usable one.
    let refused = answered(post(&[], &json!({"jsonrpc": "2.0", "id": "x"})));
    assert_eq!(refused.body_json().unwrap()["id"], "x");

    // A client answers nothing, because the server asks nothing.
    let response = json!({"jsonrpc": "2.0", "id": 1, "result": {}});
    for headers in [vec![], vec![("mcp-protocol-version", MODERN)]] {
        let message = error(&answered(post(&headers, &response)), 400, INVALID_REQUEST);
        assert!(message.contains("expects no responses"), "{message}");
    }
}

// --- The revisions that begin with initialize --------------------------------------

#[test]
fn initialize_answers_with_the_revision_asked_for_if_it_is_spoken() {
    for revision in LEGACY {
        let answer = answered(post(&[], &initialize(revision)));
        let result = &body(&answer, 200)["result"];
        assert_eq!(result["protocolVersion"], revision);
        assert_eq!(result["capabilities"], json!({"tools": {}}));
        assert_eq!(result["serverInfo"]["name"], "example");
        assert_eq!(result["serverInfo"]["version"], "1.2.3");
        // A title beside the name came with 2025-06-18.
        let titled = revision != "2025-03-26";
        assert_eq!(result["serverInfo"].get("title").is_some(), titled);
        assert_eq!(
            result["instructions"],
            "Answers questions about the weather."
        );
        // Nothing of the newest revision leaks into an older answer.
        assert!(result.get("resultType").is_none() && result.get("_meta").is_none());
        // No session is made: a client that gets no id sends none.
        assert!(
            answer
                .headers
                .iter()
                .all(|(name, _)| !name.contains("session"))
        );
        assert_golden(&format!("initialize_{revision}.json"), &answer);
    }
}

#[test]
fn initialize_offers_the_newest_of_its_kind_for_a_revision_that_is_not_spoken() {
    // Older than any here, newer than any, the revision that has no
    // initialize, and nonsense.
    for wanted in ["2024-11-05", "2099-01-01", MODERN, "1.0.0", ""] {
        let answer = answered(post(&[], &initialize(wanted)));
        let result = &body(&answer, 200)["result"];
        assert_eq!(result["protocolVersion"], "2025-11-25", "{wanted}");
    }

    // The header plays no part: a client that names in it the revision it
    // hopes for is answered from the body all the same.
    for header in ["2025-06-18", "2024-11-05", "something"] {
        let headers = [("mcp-protocol-version", header)];
        let answer = answered(post(&headers, &initialize("2025-06-18")));
        assert_eq!(
            body(&answer, 200)["result"]["protocolVersion"],
            "2025-06-18"
        );
    }

    // Without the one thing the answer depends on.
    let bare = legacy(json!(1), "initialize", json!({"capabilities": {}}));
    let message = error(&answered(post(&[], &bare)), 200, INVALID_PARAMS);
    assert!(message.contains("protocolVersion"), "{message}");
    let none = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
    error(&answered(post(&[], &none)), 200, INVALID_PARAMS);
}

#[test]
fn what_a_client_only_tells_is_accepted_without_a_body() {
    for method in [
        "notifications/initialized",
        "notifications/cancelled",
        "notifications/roots/list_changed",
        "notifications/unheard-of",
    ] {
        let told = json!({"jsonrpc": "2.0", "method": method, "params": {"requestId": 1}});
        for headers in [
            vec![],
            vec![("mcp-protocol-version", "2025-11-25")],
            vec![("mcp-protocol-version", MODERN)],
        ] {
            let answer = answered(post(&headers, &told));
            assert_eq!(answer.status, 202, "{method}");
            assert!(answer.body.is_empty());
            assert!(
                answer
                    .headers
                    .iter()
                    .all(|(name, _)| *name != "content-type")
            );
        }
    }
}

#[test]
fn the_older_revisions_list_and_call_tools_without_the_newest_ones_fields() {
    let headers: [&[(&str, &str)]; 4] = [
        &[],
        &[("mcp-protocol-version", "2025-03-26")],
        &[("mcp-protocol-version", "2025-06-18")],
        // A session id from a client that expects one is ignored.
        &[
            ("MCP-Protocol-Version", "2025-11-25"),
            ("Mcp-Session-Id", "1868a90c"),
        ],
    ];
    for headers in headers {
        let list = answered(post(headers, &legacy(json!(2), "tools/list", json!({}))));
        let result = &body(&list, 200)["result"];
        let names: Vec<&str> = result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["get_weather", "get_time"]);
        let tool = &result["tools"][0];
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint": true, "openWorldHint": false})
        );
        for newer in ["resultType", "ttlMs", "cacheScope", "_meta", "nextCursor"] {
            assert!(result.get(newer).is_none(), "{newer}");
        }
        assert_golden("tools_list_legacy.json", &list);

        let call = legacy(
            json!("call-1"),
            "tools/call",
            json!({"name": "get_weather", "arguments": {"location": "New York"}}),
        );
        let waiting = calls(post(headers, &call));
        assert_eq!(waiting.calls().len(), 1);
        assert_eq!(waiting.calls()[0].name, "get_weather");
        assert_eq!(
            Value::Object(waiting.calls()[0].arguments.clone()),
            json!({"location": "New York"})
        );
        let answer = waiting.finish(vec![ok("72°F, partly cloudy")]);
        assert_eq!(
            body(&answer, 200),
            json!({
                "jsonrpc": "2.0",
                "id": "call-1",
                "result": {
                    "content": [{"type": "text", "text": "72°F, partly cloudy"}],
                    "isError": false
                }
            })
        );
        assert_golden("tools_call_legacy.json", &answer);
    }

    // The list has one page. A ping is answered with nothing.
    let no_params = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"});
    body(&answered(post(&[], &no_params)), 200);
    let null_cursor = legacy(json!(3), "tools/list", json!({"cursor": null}));
    body(&answered(post(&[], &null_cursor)), 200);
    let ping = json!({"jsonrpc": "2.0", "id": "123", "method": "ping"});
    assert_eq!(
        body(&answered(post(&[], &ping)), 200),
        json!({"jsonrpc": "2.0", "id": "123", "result": {}})
    );
}

#[test]
fn a_tool_that_fails_is_an_answer_and_a_call_that_cannot_be_made_is_an_error() {
    // The tool ran and has to say no: the model reads why.
    let call = legacy(json!(4), "tools/call", json!({"name": "get_time"}));
    let waiting = calls(post(&[], &call));
    assert!(
        waiting.calls()[0].arguments.is_empty(),
        "no arguments is none"
    );
    let failed = waiting.finish(vec![ToolResult {
        text: "The clock has stopped.".to_owned(),
        is_error: true,
    }]);
    let result = &body(&failed, 200)["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(result["content"][0]["text"], "The clock has stopped.");

    // The call never reached a tool. In the older revisions the error is
    // the response, and travels as one.
    for (params, said) in [
        (
            json!({"name": "launch_missiles"}),
            "Unknown tool: launch_missiles",
        ),
        (json!({"arguments": {}}), "'name' must be a string"),
        (json!({"name": 7}), "'name' must be a string"),
        (
            json!({"name": "get_time", "arguments": [1]}),
            "'arguments' must be an object",
        ),
        (
            json!({"name": "get_time", "arguments": "now"}),
            "'arguments' must be an object",
        ),
    ] {
        let answer = answered(post(&[], &legacy(json!(5), "tools/call", params)));
        let message = error(&answer, 200, INVALID_PARAMS);
        assert!(message.contains(said), "{message}");
        assert_eq!(answer.body_json().unwrap()["id"], 5);
    }
    let cursor = legacy(
        json!(6),
        "tools/list",
        json!({"cursor": "eyJwYWdlIjogM30="}),
    );
    let message = error(&answered(post(&[], &cursor)), 200, INVALID_PARAMS);
    assert!(message.contains("cursor"), "{message}");

    // A long name from a client is not repeated in full.
    let long = "x".repeat(5000);
    let answer = answered(post(
        &[],
        &legacy(json!(7), "tools/call", json!({"name": long})),
    ));
    assert!(error(&answer, 200, INVALID_PARAMS).len() < 100);
}

#[test]
fn what_the_server_does_not_offer_is_method_not_found() {
    for method in [
        "resources/list",
        "resources/read",
        "resources/templates/list",
        "resources/subscribe",
        "prompts/list",
        "prompts/get",
        "logging/setLevel",
        "completion/complete",
        "tasks/list",
        // The newest revision's own method, asked the old way.
        "server/discover",
        "subscriptions/listen",
        "notifications/initialized",
        "",
    ] {
        let answer = answered(post(&[], &legacy(json!(8), method, json!({}))));
        let message = error(&answer, 200, METHOD_NOT_FOUND);
        assert!(message.starts_with("Method not found"), "{message}");
    }
}

// --- Batches, which only the oldest revision has ------------------------------------

#[test]
fn a_batch_is_answered_message_by_message_in_order() {
    let batch = json!([
        legacy(json!(1), "ping", json!({})),
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        legacy(json!("list"), "tools/list", json!({})),
        legacy(json!(3), "resources/list", json!({})),
        "nonsense",
        {"jsonrpc": "2.0", "id": 5},
        initialize("2025-03-26"),
    ]);
    for headers in [vec![], vec![("mcp-protocol-version", "2025-03-26")]] {
        let answer = answered(post(&headers, &batch));
        let responses = body(&answer, 200);
        let responses = responses.as_array().unwrap();
        // One response per request, none for the notification.
        assert_eq!(responses.len(), 6);
        assert_eq!(
            responses[0],
            json!({"jsonrpc": "2.0", "id": 1, "result": {}})
        );
        assert_eq!(responses[1]["id"], "list");
        assert_eq!(responses[1]["result"]["tools"].as_array().unwrap().len(), 2);
        assert_eq!(responses[2]["error"]["code"], METHOD_NOT_FOUND);
        // What is no message has no id to answer to.
        assert_eq!(responses[3]["id"], Value::Null);
        assert_eq!(responses[3]["error"]["code"], INVALID_REQUEST);
        assert_eq!(responses[4]["id"], 5);
        assert_eq!(responses[4]["error"]["code"], INVALID_REQUEST);
        // A conversation cannot begin in the middle of a batch.
        assert_eq!(responses[5]["id"], 1);
        assert_eq!(responses[5]["error"]["code"], INVALID_REQUEST);
        let message = responses[5]["error"]["message"].as_str().unwrap();
        assert!(message.contains("initialize must not be part of a batch"));
    }
}

#[test]
fn the_tools_of_a_batch_run_in_order_and_their_results_find_their_requests() {
    let batch = json!([
        legacy(json!("a"), "tools/call", json!({"name": "get_time"})),
        legacy(json!("b"), "ping", json!({})),
        legacy(
            json!("c"),
            "tools/call",
            json!({"name": "get_weather", "arguments": {"location": "Lenggries"}})
        ),
        legacy(json!("d"), "tools/call", json!({"name": "nothing"})),
    ]);
    let waiting = calls(post(&[], &batch));
    let names: Vec<&str> = waiting.calls().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["get_time", "get_weather"]);

    let answer = waiting.finish(vec![ok("12:00"), ok("Föhn")]);
    let responses = body(&answer, 200);
    let ids: Vec<&str> = responses
        .as_array()
        .unwrap()
        .iter()
        .map(|response| response["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["a", "b", "c", "d"]);
    assert_eq!(responses[0]["result"]["content"][0]["text"], "12:00");
    assert_eq!(responses[1]["result"], json!({}));
    assert_eq!(responses[2]["result"]["content"][0]["text"], "Föhn");
    assert_eq!(responses[3]["error"]["code"], INVALID_PARAMS);
    assert_golden("batch_2025-03-26.json", &answer);
}

#[test]
fn a_batch_of_things_told_is_accepted_and_a_batch_that_is_none_is_refused() {
    let told = json!([
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 1, "result": {}},
    ]);
    let answer = answered(post(&[], &told));
    assert_eq!((answer.status, answer.body.len()), (202, 0));

    let message = error(&answered(post(&[], &json!([]))), 400, INVALID_REQUEST);
    assert!(message.contains("empty batch"), "{message}");

    let ping = legacy(json!(1), "ping", json!({}));
    let most = Value::Array(vec![ping.clone(); MAX_BATCH]);
    assert_eq!(
        body(&answered(post(&[], &most)), 200)
            .as_array()
            .unwrap()
            .len(),
        MAX_BATCH
    );
    let too_many = Value::Array(vec![ping.clone(); MAX_BATCH + 1]);
    let message = error(&answered(post(&[], &too_many)), 400, INVALID_REQUEST);
    assert!(message.contains("at most"), "{message}");

    // The later revisions took batches out again.
    let batch = json!([ping]);
    for revision in ["2025-06-18", "2025-11-25", MODERN] {
        let headers = [("mcp-protocol-version", revision)];
        let message = error(&answered(post(&headers, &batch)), 400, INVALID_REQUEST);
        assert!(message.contains("no batches"), "{revision}: {message}");
    }
    let headers = [("mcp-protocol-version", "1900-01-01")];
    error(
        &answered(post(&headers, &batch)),
        400,
        UNSUPPORTED_PROTOCOL_VERSION,
    );
}

// --- The revision in which every request stands alone --------------------------------

#[test]
fn discover_says_which_revisions_are_spoken_and_what_the_server_has() {
    let request = modern(json!("discover-1"), "server/discover", json!({}));
    let answer = answered(post(&modern_headers("server/discover"), &request));

    let response = body(&answer, 200);
    assert_eq!(response["id"], "discover-1");
    let result = &response["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(
        result["supportedVersions"],
        json!(["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"])
    );
    assert_eq!(result["capabilities"], json!({"tools": {}}));
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"],
        json!({"name": "example", "title": "Example Server", "version": "1.2.3"})
    );
    assert_eq!(
        result["instructions"],
        "Answers questions about the weather."
    );
    // The same for everyone, and good for an hour.
    assert_eq!(result["ttlMs"], 3_600_000);
    assert_eq!(result["cacheScope"], "public");
    assert_golden("discover_2026-07-28.json", &answer);
}

#[test]
fn the_newest_revision_lists_and_calls_tools_with_its_own_fields() {
    let list = modern(json!(1), "tools/list", json!({}));
    let answer = answered(post(&modern_headers("tools/list"), &list));
    let result = &body(&answer, 200)["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["tools"].as_array().unwrap().len(), 2);
    assert_eq!(result["ttlMs"], 3_600_000);
    assert_eq!(result["cacheScope"], "public");
    assert!(result.get("nextCursor").is_none());
    assert!(result["_meta"]["io.modelcontextprotocol/serverInfo"].is_object());
    assert_golden("tools_list_2026-07-28.json", &answer);

    // The request of the specification's transport page, as it stands there.
    let call = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "get_weather",
            "arguments": {"location": "Seattle, WA"},
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {"name": "ExampleClient", "version": "1.0.0"},
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }
    });
    let headers = [
        ("MCP-Protocol-Version", "2026-07-28"),
        ("Mcp-Method", "tools/call"),
        ("Mcp-Name", "get_weather"),
        // What an older client would send is ignored, not answered.
        ("Mcp-Session-Id", "1868a90c"),
        ("Last-Event-ID", "7"),
    ];
    let waiting = calls(post(&headers, &call));
    assert_eq!(waiting.calls()[0].name, "get_weather");
    // What belongs to the protocol does not reach the tool.
    assert_eq!(
        Value::Object(waiting.calls()[0].arguments.clone()),
        json!({"location": "Seattle, WA"})
    );
    let answer = waiting.finish(vec![ok("Rain.")]);
    assert_eq!(
        body(&answer, 200)["result"],
        json!({
            "resultType": "complete",
            "content": [{"type": "text", "text": "Rain."}],
            "isError": false,
            "_meta": {"io.modelcontextprotocol/serverInfo": {
                "name": "example", "title": "Example Server", "version": "1.2.3"
            }}
        })
    );
    assert!(
        answer
            .headers
            .iter()
            .all(|(name, _)| !name.contains("session"))
    );
    assert_golden("tools_call_2026-07-28.json", &answer);
}

#[test]
fn the_headers_of_a_request_must_say_what_its_body_says() {
    let list = modern(json!(1), "tools/list", json!({}));
    let call = modern(
        json!(2),
        "tools/call",
        json!({"name": "get_weather", "arguments": {}}),
    );
    let version = ("mcp-protocol-version", MODERN);
    let mismatch = |headers: &[(&str, &str)], request: &Value, said: &str| {
        let answer = answered(post(headers, request));
        let message = error(&answer, 400, HEADER_MISMATCH);
        assert!(message.starts_with("Header mismatch: "), "{message}");
        assert!(message.contains(said), "{message}");
        // The request is named, so that the client can match the refusal.
        assert_eq!(answer.body_json().unwrap()["id"], request["id"]);
    };

    mismatch(&[version], &list, "Mcp-Method header is missing");
    mismatch(
        &[version, ("mcp-method", "tools/call")],
        &list,
        "Mcp-Method header value 'tools/call' does not match body value 'tools/list'",
    );
    // Methods are compared as written.
    mismatch(
        &[version, ("mcp-method", "Tools/List")],
        &list,
        "does not match",
    );

    let method = ("mcp-method", "tools/call");
    mismatch(&[version, method], &call, "Mcp-Name header is missing");
    mismatch(
        &[version, method, ("mcp-name", "get_time")],
        &call,
        "Mcp-Name header value 'get_time' does not match body value 'get_weather'",
    );
    mismatch(
        &[version, method, ("mcp-name", "=?base64?not base64!?=")],
        &call,
        "cannot be decoded",
    );
    // A name that does not fit a header travels wrapped, and is unwrapped
    // before it is compared: "get_weather" here.
    let wrapped = ("mcp-name", "=?base64?Z2V0X3dlYXRoZXI=?=");
    assert_eq!(
        calls(post(&[version, method, wrapped], &call))
            .calls()
            .len(),
        1
    );

    // The revision is named twice as well, and both must be there and agree.
    let methods = ("mcp-method", "tools/list");
    mismatch(&[methods], &list, "MCP-Protocol-Version header is missing");
    mismatch(
        &[("mcp-protocol-version", "2025-11-25"), methods],
        &list,
        "MCP-Protocol-Version header value '2025-11-25' does not match body value '2026-07-28'",
    );
    let mut older = list.clone();
    older["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2025-11-25");
    mismatch(
        &[version, methods],
        &older,
        "does not match body value '2025-11-25'",
    );
}

#[test]
fn a_request_of_the_newest_revision_must_carry_what_it_stands_on() {
    let headers = modern_headers("tools/list");
    let invalid = |request: Value, said: &str| {
        let message = error(&answered(post(&headers, &request)), 400, INVALID_PARAMS);
        assert!(message.contains(said), "{message}");
    };
    let version = "io.modelcontextprotocol/protocolVersion";
    let capabilities = "io.modelcontextprotocol/clientCapabilities";

    // The older way of asking, under the newest revision's header.
    invalid(legacy(json!(1), "tools/list", json!({})), version);
    invalid(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        version,
    );
    invalid(
        legacy(json!(1), "tools/list", json!({"_meta": {}})),
        version,
    );
    invalid(
        legacy(json!(1), "tools/list", json!({"_meta": "none"})),
        version,
    );

    let mut request = modern(json!(1), "tools/list", json!({}));
    request["params"]["_meta"]
        .as_object_mut()
        .unwrap()
        .remove(capabilities);
    invalid(request.clone(), capabilities);
    request["params"]["_meta"][capabilities] = json!("all of them");
    invalid(request, capabilities);

    // Who the client is, is optional.
    let mut anonymous = modern(json!(1), "tools/list", json!({}));
    anonymous["params"]["_meta"]
        .as_object_mut()
        .unwrap()
        .remove("io.modelcontextprotocol/clientInfo");
    body(&answered(post(&headers, &anonymous)), 200);
}

#[test]
fn a_revision_that_is_not_spoken_is_refused_with_those_that_are() {
    let mut request = modern(json!(1), "tools/list", json!({}));
    for requested in ["1900-01-01", "2024-11-05", "2027-01-01", "draft", ""] {
        request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!(requested);
        let headers = [
            ("mcp-protocol-version", requested),
            ("mcp-method", "tools/list"),
        ];
        let answer = answered(post(&headers, &request));
        let message = error(&answer, 400, UNSUPPORTED_PROTOCOL_VERSION);
        assert_eq!(message, "Unsupported protocol version");
        let response = answer.body_json().unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(
            response["error"]["data"],
            json!({
                "supported": ["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"],
                "requested": requested
            })
        );
        if requested == "1900-01-01" {
            assert_golden("unsupported_version.json", &answer);
        }
    }
    // The same for a client of an old revision that names it in the header.
    let headers = [("mcp-protocol-version", "2024-11-05")];
    let old = legacy(json!(1), "tools/list", json!({}));
    error(
        &answered(post(&headers, &old)),
        400,
        UNSUPPORTED_PROTOCOL_VERSION,
    );
}

#[test]
fn errors_of_the_newest_revision_have_the_status_it_names() {
    let not_found = |method: &str| {
        let request = modern(json!(1), method, json!({}));
        let answer = answered(post(&modern_headers(method), &request));
        error(&answer, 404, METHOD_NOT_FOUND)
    };
    for method in ["resources/list", "prompts/list", "subscriptions/listen"] {
        assert_eq!(not_found(method), format!("Method not found: {method}"));
    }
    // A client that begins the old way under the new header is told what
    // would work: it has nothing else to go on.
    for method in ["initialize", "ping"] {
        let message = not_found(method);
        assert!(message.contains("every request stands alone"), "{message}");
        assert!(
            message.contains("2025-11-25, 2025-06-18, 2025-03-26"),
            "{message}"
        );
    }

    // Where the revision names no status, the request was a bad one.
    let unknown = modern(json!(2), "tools/call", json!({"name": "nothing"}));
    let headers = [
        ("mcp-protocol-version", MODERN),
        ("mcp-method", "tools/call"),
        ("mcp-name", "nothing"),
    ];
    let message = error(&answered(post(&headers, &unknown)), 400, INVALID_PARAMS);
    assert_eq!(message, "Unknown tool: nothing");
    let cursor = modern(json!(3), "tools/list", json!({"cursor": "abc"}));
    error(
        &answered(post(&modern_headers("tools/list"), &cursor)),
        400,
        INVALID_PARAMS,
    );
    // Without a name there is no header to hold it to.
    let nameless = modern(json!(4), "tools/call", json!({"arguments": {}}));
    let message = error(
        &answered(post(&modern_headers("tools/call"), &nameless)),
        400,
        INVALID_PARAMS,
    );
    assert!(message.contains("'name' must be a string"), "{message}");
}

// --- When whoever runs the tools gets it wrong ----------------------------------------

#[test]
fn a_call_without_a_result_is_an_internal_error_and_never_a_panic() {
    let call = legacy(json!(1), "tools/call", json!({"name": "get_time"}));
    let answer = calls(post(&[], &call)).finish(Vec::new());
    error(&answer, 200, INTERNAL_ERROR);

    let call = modern(json!(1), "tools/call", json!({"name": "get_time"}));
    let headers = [
        ("mcp-protocol-version", MODERN),
        ("mcp-method", "tools/call"),
        ("mcp-name", "get_time"),
    ];
    let answer = calls(post(&headers, &call)).finish(Vec::new());
    error(&answer, 500, INTERNAL_ERROR);

    // One result short in a batch: the second call has none. One too many
    // is ignored.
    let batch = json!([
        legacy(json!("a"), "tools/call", json!({"name": "get_time"})),
        legacy(json!("b"), "tools/call", json!({"name": "get_time"})),
    ]);
    let responses = body(&calls(post(&[], &batch)).finish(vec![ok("12:00")]), 200);
    assert_eq!(responses[0]["result"]["content"][0]["text"], "12:00");
    assert_eq!(responses[1]["error"]["code"], INTERNAL_ERROR);
    let call = legacy(json!(1), "tools/call", json!({"name": "get_time"}));
    let answer = calls(post(&[], &call)).finish(vec![ok("12:00"), ok("13:00")]);
    assert_eq!(body(&answer, 200)["result"]["content"][0]["text"], "12:00");
}

#[test]
fn every_revision_is_answered_at_the_one_endpoint() {
    // The same question in each revision's way of asking.
    for revision in Revision::ALL {
        let name = revision.as_str();
        let answer = if revision.is_legacy() {
            let headers = [("mcp-protocol-version", name)];
            answered(post(&headers, &legacy(json!(1), "tools/list", json!({}))))
        } else {
            let request = modern(json!(1), "tools/list", json!({}));
            answered(post(&modern_headers("tools/list"), &request))
        };
        let result = &body(&answer, 200)["result"];
        assert_eq!(result["tools"].as_array().unwrap().len(), 2, "{name}");
        assert_eq!(result.get("resultType").is_some(), !revision.is_legacy());
    }
}
