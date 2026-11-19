//! What a server answers to each request.

use serde_json::{Map, Value, json};

use crate::http::{Answer, HttpRequest, accepts_json, is_json, unwrap_header_value};
use crate::message::{
    HEADER_MISMATCH, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, Message,
    PARSE_ERROR, RpcError, UNSUPPORTED_PROTOCOL_VERSION, result_response,
};
use crate::{Revision, message};

/// The most messages one batch may hold. A batch is one HTTP request, and
/// whoever bounds requests by their number bounds batches by this.
pub const MAX_BATCH: usize = 16;

/// How long a client may keep the answer to `server/discover` and
/// `tools/list` before asking again, in milliseconds: an hour. Both change
/// only when the server is replaced by a newer one.
const LIST_TTL_MS: u64 = 3_600_000;

/// The key under which the newest revision carries the revision a request
/// speaks, in the request's `_meta`.
const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// The key of what the client can do, in the request's `_meta`.
const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
/// The key under which a result names the server, in its `_meta`.
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// The longest piece of a client's own text that an error repeats.
const QUOTED_CHARS: usize = 64;

/// What a server says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implementation {
    /// The name programs know it by.
    pub name: String,
    /// The name people are shown.
    pub title: String,
    /// Its version.
    pub version: String,
}

/// A tool as the protocol lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// The name the model calls it by.
    pub name: String,
    /// What it does and when to use it, written for the model.
    pub description: String,
    /// A JSON Schema of its arguments, of type `object`.
    pub input_schema: Value,
    /// Whether it only reads. Clients ask their user before running a tool
    /// that says it does not.
    pub read_only: bool,
    /// Whether it reaches out into the world beyond the server: the web,
    /// other services. A tool that reads the server's own data does not.
    pub open_world: bool,
}

/// Which web pages may call the server from a browser.
///
/// A browser names the page a request comes from in the `Origin` header,
/// and programs that are not browsers send none. The protocol asks servers
/// to refuse an origin they do not expect, so that a page on one site
/// cannot use a visitor's browser to reach a server on their own machine or
/// network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origins {
    /// Pages served from the machine the browser runs on: `localhost`,
    /// `127.0.0.1` or `[::1]`, on any port. For a server on a developer's
    /// own machine, where the tools that call it are served from there too
    /// and their ports are not known beforehand.
    Local,
    /// Only pages of these origins, each written as a browser sends it:
    /// `https://venti.example`, without a path.
    Only(Vec<String>),
}

impl Origins {
    fn allow(&self, origin: &str) -> bool {
        match self {
            Self::Local => is_local(origin),
            Self::Only(origins) => origins
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(origin)),
        }
    }
}

/// Whether an origin names this machine: `http://localhost:5173`,
/// `http://127.0.0.1`, `https://[::1]:8443`.
fn is_local(origin: &str) -> bool {
    let origin = origin.to_ascii_lowercase();
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    ["localhost", "127.0.0.1", "[::1]"].into_iter().any(|host| {
        authority.strip_prefix(host).is_some_and(|rest| {
            rest.is_empty()
                || rest.strip_prefix(':').is_some_and(|port| {
                    !port.is_empty() && port.bytes().all(|digit| digit.is_ascii_digit())
                })
        })
    })
}

/// A tool the client asks to be run. The tool exists, and its arguments
/// are a JSON object; whether they are the ones it needs is for the tool
/// to say.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// The tool's name.
    pub name: String,
    /// The arguments the model gave.
    pub arguments: Map<String, Value>,
}

/// What a tool made of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// The text the model reads.
    pub text: String,
    /// Whether the call failed. The text then says why, so that the model
    /// can correct itself: a failed call is an answer, not a protocol error.
    pub is_error: bool,
}

/// What a request comes to.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    /// The answer.
    Answer(Answer),
    /// Tools to run before there is an answer: see [`Calls`].
    Calls(Calls),
}

/// A request that waits for tools to run.
///
/// Run each of [`Calls::calls`] and hand the results to [`Calls::finish`],
/// in the same order.
#[derive(Debug, Clone, PartialEq)]
pub struct Calls {
    calls: Vec<ToolCall>,
    pending: Pending,
}

impl Calls {
    /// The tools to run. One, unless the request was a batch.
    pub fn calls(&self) -> &[ToolCall] {
        &self.calls
    }

    /// The answer, from one result for each call in the order of
    /// [`Calls::calls`]. A call without a result is answered with an
    /// internal error.
    pub fn finish(self, results: Vec<ToolResult>) -> Answer {
        let mut results = results.into_iter();
        let pending = self.pending;
        let responses: Vec<Value> = pending
            .slots
            .into_iter()
            .map(|slot| match slot {
                Slot::Ready(response) => response,
                Slot::Waiting(id) => match results.next() {
                    Some(result) => {
                        let mut body = Map::new();
                        body.insert(
                            "content".to_owned(),
                            json!([{"type": "text", "text": result.text}]),
                        );
                        body.insert("isError".to_owned(), json!(result.is_error));
                        let body = decorate(body, pending.revision, &pending.server_info);
                        result_response(&id, body)
                    }
                    None => RpcError::new(INTERNAL_ERROR, "the tool did not answer")
                        .response(Some(&id), Some(pending.revision)),
                },
            })
            .collect();
        match pending.shape {
            Shape::Batch => Answer::json(200, &Value::Array(responses)),
            Shape::Single => {
                let response = responses.into_iter().next().unwrap_or(Value::Null);
                // A missing result is the one error that can arise here.
                let failed = response.get("error").is_some();
                let status = if failed && !pending.revision.is_legacy() {
                    500
                } else {
                    200
                };
                Answer::json(status, &response)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Pending {
    revision: Revision,
    shape: Shape,
    slots: Vec<Slot>,
    server_info: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Single,
    Batch,
}

/// One response of an answer: finished, or waiting for a tool.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Ready(Value),
    /// The `id` of the request that called the tool.
    Waiting(Value),
}

/// What became of one request.
enum Handled {
    Result(Map<String, Value>),
    Error(RpcError),
    Call(ToolCall),
}

/// A server as the protocol sees it: who it is and which tools it has.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    info: Implementation,
    instructions: String,
    tools: Vec<Tool>,
    origins: Origins,
}

impl Server {
    /// A server with these tools. `instructions` tell a model what the
    /// server is for, beyond what the tools say about themselves.
    pub fn new(
        info: Implementation,
        instructions: impl Into<String>,
        tools: Vec<Tool>,
        origins: Origins,
    ) -> Self {
        Self {
            info,
            instructions: instructions.into(),
            tools,
            origins,
        }
    }

    /// The tools, in the order they are listed.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// Reads one HTTP request to the MCP endpoint.
    ///
    /// Everything the protocol decides is decided here: whether the request
    /// may be answered at all, which revision it speaks, what it asks, and
    /// what the answer is. What is left to the caller is to run tools.
    pub fn read(&self, request: &HttpRequest<'_>) -> Read {
        if let Some(refusal) = self.refuse_before_reading(request) {
            return Read::Answer(refusal);
        }
        let named = request.header("mcp-protocol-version");
        // The revision whose rules shape an error about a body that could
        // not be read: the one the header names, and the oldest for a
        // request that names none, as its clients do not.
        let spoken = match named {
            None => Some(Revision::UNNAMED),
            Some(named) => Revision::parse(named),
        };
        let Ok(body) = serde_json::from_slice::<Value>(request.body) else {
            let error = RpcError::new(PARSE_ERROR, "Parse error: the body is not JSON");
            return answer(400, &error.response(None, spoken));
        };

        if let Value::Array(messages) = &body {
            return match (named, spoken) {
                (_, Some(revision)) if revision.has_batches() => self.batch(messages),
                (Some(named), None) => unsupported(named, None),
                (_, revision) => {
                    let error = RpcError::new(
                        INVALID_REQUEST,
                        "Invalid request: this revision of the protocol has no batches; \
                         send one message per request",
                    );
                    answer(400, &error.response(None, revision))
                }
            };
        }

        let message = match message::read(&body) {
            Ok(message) => message,
            Err(no) => {
                let error =
                    RpcError::new(INVALID_REQUEST, format!("Invalid request: {}", no.problem));
                return answer(400, &error.response(no.id, spoken));
            }
        };
        let (id, is_initialize) = match &message {
            Message::Request { id, method, .. } => (Some(*id), *method == "initialize"),
            _ => (None, false),
        };

        // A body that names its revision is held to the header first: the
        // two must never disagree, whatever the revision is.
        if let Some(declared) = declared_revision(&message) {
            let problem = match named {
                None => Some("the MCP-Protocol-Version header is missing".to_owned()),
                Some(named) if named != declared => Some(format!(
                    "MCP-Protocol-Version header value '{}' does not match body value '{}'",
                    quoted(named),
                    quoted(declared)
                )),
                Some(_) => None,
            };
            if let Some(problem) = problem {
                return header_mismatch(&problem, id);
            }
        }

        let revision = match (named, spoken) {
            (_, Some(revision)) => revision,
            // `initialize` settles the revision from its body: whatever
            // the header says, the answer is an offer the client can take.
            (Some(_), None) if is_initialize => Revision::UNNAMED,
            (Some(named), None) => return unsupported(named, id),
            (None, None) => Revision::UNNAMED,
        };
        if revision.is_legacy() {
            self.legacy(&message, revision)
        } else {
            self.modern(request, &message, revision)
        }
    }

    /// Refusals that need no look at the body.
    fn refuse_before_reading(&self, request: &HttpRequest<'_>) -> Option<Answer> {
        let refuse = |status: u16, message: &str| {
            let error = RpcError::new(i64::from(status), message);
            Answer::json(status, &error.response(None, None))
        };
        if let Some(origin) = request.header("origin")
            && !self.origins.allow(origin)
        {
            return Some(refuse(
                403,
                "Forbidden: this server does not answer pages of this origin",
            ));
        }
        if !request.method.eq_ignore_ascii_case("POST") {
            // No stream to listen on and no session to end: every answer
            // is the answer to a POST.
            let refusal = refuse(
                405,
                "Method not allowed: this endpoint answers POST and offers no stream",
            );
            return Some(refusal.with_header("allow", "POST"));
        }
        if !request.header("content-type").is_some_and(is_json) {
            return Some(refuse(
                415,
                "Unsupported media type: send the message as application/json",
            ));
        }
        if request
            .header("accept")
            .is_some_and(|accept| !accepts_json(accept))
        {
            return Some(refuse(
                406,
                "Not acceptable: this server answers in application/json only",
            ));
        }
        None
    }

    /// A message of a revision that begins with `initialize`.
    fn legacy(&self, message: &Message<'_>, revision: Revision) -> Read {
        match *message {
            Message::Response => unexpected_response(revision),
            Message::Notification { .. } => Read::Answer(Answer::accepted()),
            Message::Request { id, method, params } => {
                match self.handle(revision, method, params) {
                    Handled::Result(result) => {
                        let result = decorate(result, revision, &self.server_info(revision));
                        answer(200, &result_response(id, result))
                    }
                    // The error is the response to a request that was
                    // understood as one, and travels as responses do.
                    Handled::Error(error) => answer(200, &error.response(Some(id), Some(revision))),
                    Handled::Call(call) => self.waiting(revision, Shape::Single, id, call),
                }
            }
        }
    }

    /// A message of the revision in which every request stands alone.
    fn modern(&self, request: &HttpRequest<'_>, message: &Message<'_>, revision: Revision) -> Read {
        let (id, method, params) = match *message {
            Message::Response => return unexpected_response(revision),
            // The revision defines no notification a client sends over
            // HTTP, and no headers for one. Accepting it costs nothing.
            Message::Notification { .. } => return Read::Answer(Answer::accepted()),
            Message::Request { id, method, params } => (id, method, params),
        };

        match request.header("mcp-method") {
            None => return header_mismatch("the Mcp-Method header is missing", Some(id)),
            Some(named) if named != method => {
                let problem = format!(
                    "Mcp-Method header value '{}' does not match body value '{}'",
                    quoted(named),
                    quoted(method)
                );
                return header_mismatch(&problem, Some(id));
            }
            Some(_) => {}
        }

        let invalid = |what: &str| {
            let error = RpcError::new(INVALID_PARAMS, format!("Invalid params: {what}"));
            answer(400, &error.response(Some(id), Some(revision)))
        };
        let meta = params
            .and_then(|params| params.get("_meta"))
            .and_then(Value::as_object);
        // That it equals the header was checked before the revision was chosen.
        if !meta.is_some_and(|meta| {
            meta.get(META_PROTOCOL_VERSION)
                .is_some_and(Value::is_string)
        }) {
            return invalid(&format!("_meta lacks \"{META_PROTOCOL_VERSION}\""));
        }
        if !meta.is_some_and(|meta| {
            meta.get(META_CLIENT_CAPABILITIES)
                .is_some_and(Value::is_object)
        }) {
            return invalid(&format!("_meta lacks \"{META_CLIENT_CAPABILITIES}\""));
        }

        // A tool's name travels twice, and the two must agree. A call
        // without a usable name is refused for that further down.
        let tool = params
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str);
        if let (true, Some(tool)) = (method == "tools/call", tool) {
            let problem = match request.header("mcp-name").map(unwrap_header_value) {
                None => Some("the Mcp-Name header is missing".to_owned()),
                Some(None) => Some("the Mcp-Name header cannot be decoded".to_owned()),
                Some(Some(named)) if named != tool => Some(format!(
                    "Mcp-Name header value '{}' does not match body value '{}'",
                    quoted(&named),
                    quoted(tool)
                )),
                Some(Some(_)) => None,
            };
            if let Some(problem) = problem {
                return header_mismatch(&problem, Some(id));
            }
        }

        match self.handle(revision, method, params) {
            Handled::Result(result) => {
                let result = decorate(result, revision, &self.server_info(revision));
                answer(200, &result_response(id, result))
            }
            Handled::Error(error) => answer(
                modern_status(error.code),
                &error.response(Some(id), Some(revision)),
            ),
            Handled::Call(call) => self.waiting(revision, Shape::Single, id, call),
        }
    }

    /// Several messages in one body, which the oldest revision allows.
    fn batch(&self, messages: &[Value]) -> Read {
        let revision = Revision::UNNAMED;
        let refuse = |problem: String| {
            let error = RpcError::new(INVALID_REQUEST, format!("Invalid request: {problem}"));
            answer(400, &error.response(None, Some(revision)))
        };
        if messages.is_empty() {
            return refuse("an empty batch".to_owned());
        }
        if messages.len() > MAX_BATCH {
            return refuse(format!(
                "a batch of {} messages; this server takes {MAX_BATCH} at most",
                messages.len()
            ));
        }

        let mut slots = Vec::new();
        let mut calls = Vec::new();
        for value in messages {
            let (id, method, params) = match message::read(value) {
                Ok(Message::Request { id, method, params }) => (id, method, params),
                // Nothing is due for what is only told.
                Ok(Message::Notification { .. } | Message::Response) => continue,
                Err(no) => {
                    let error =
                        RpcError::new(INVALID_REQUEST, format!("Invalid request: {}", no.problem));
                    slots.push(Slot::Ready(error.response(no.id, Some(revision))));
                    continue;
                }
            };
            if method == "initialize" {
                let error = RpcError::new(
                    INVALID_REQUEST,
                    "Invalid request: initialize must not be part of a batch",
                );
                slots.push(Slot::Ready(error.response(Some(id), Some(revision))));
                continue;
            }
            slots.push(match self.handle(revision, method, params) {
                Handled::Result(result) => {
                    let result = decorate(result, revision, &self.server_info(revision));
                    Slot::Ready(result_response(id, result))
                }
                Handled::Error(error) => Slot::Ready(error.response(Some(id), Some(revision))),
                Handled::Call(call) => {
                    calls.push(call);
                    Slot::Waiting(id.clone())
                }
            });
        }

        if slots.is_empty() {
            return Read::Answer(Answer::accepted());
        }
        let pending = Pending {
            revision,
            shape: Shape::Batch,
            slots,
            server_info: self.server_info(revision),
        };
        let calls = Calls { calls, pending };
        if calls.calls.is_empty() {
            Read::Answer(calls.finish(Vec::new()))
        } else {
            Read::Calls(calls)
        }
    }

    fn waiting(&self, revision: Revision, shape: Shape, id: &Value, call: ToolCall) -> Read {
        Read::Calls(Calls {
            calls: vec![call],
            pending: Pending {
                revision,
                shape,
                slots: vec![Slot::Waiting(id.clone())],
                server_info: self.server_info(revision),
            },
        })
    }

    /// Answers one request, or says which tool has to run for the answer.
    fn handle(
        &self,
        revision: Revision,
        method: &str,
        params: Option<&Map<String, Value>>,
    ) -> Handled {
        match (method, revision.is_legacy()) {
            ("initialize", true) => self.initialize(params),
            ("ping", true) => Handled::Result(Map::new()),
            ("server/discover", false) => Handled::Result(self.discover()),
            ("tools/list", _) => self.list_tools(revision, params),
            ("tools/call", _) => self.call_tool(params),
            // A client that begins the old way with the new revision's
            // header has nothing to fall back on but this message.
            ("initialize" | "ping", false) => Handled::Error(RpcError::new(
                METHOD_NOT_FOUND,
                format!(
                    "Method not found: revision {} of the protocol has no {method}; every \
                     request stands alone. This server also speaks {}, which begin with \
                     initialize",
                    revision.as_str(),
                    legacy_names().join(", ")
                ),
            )),
            _ => Handled::Error(RpcError::new(
                METHOD_NOT_FOUND,
                format!("Method not found: {}", quoted(method)),
            )),
        }
    }

    /// `initialize`: the client names the revision it would like, and the
    /// server answers with that one if it speaks it, and with the newest it
    /// has of this kind if not. The client decides whether that will do.
    fn initialize(&self, params: Option<&Map<String, Value>>) -> Handled {
        let Some(wanted) = params
            .and_then(|params| params.get("protocolVersion"))
            .and_then(Value::as_str)
        else {
            return Handled::Error(RpcError::new(
                INVALID_PARAMS,
                "Invalid params: initialize needs a protocolVersion",
            ));
        };
        let revision = Revision::parse(wanted)
            .filter(|revision| revision.is_legacy())
            .unwrap_or(Revision::NEWEST_LEGACY);

        let mut result = Map::new();
        result.insert("protocolVersion".to_owned(), json!(revision.as_str()));
        result.insert("capabilities".to_owned(), capabilities());
        result.insert("serverInfo".to_owned(), self.server_info(revision));
        result.insert("instructions".to_owned(), json!(self.instructions));
        Handled::Result(result)
    }

    /// `server/discover`: which revisions the server speaks and what it has.
    fn discover(&self) -> Map<String, Value> {
        let mut result = Map::new();
        result.insert("supportedVersions".to_owned(), json!(Revision::names()));
        result.insert("capabilities".to_owned(), capabilities());
        result.insert("instructions".to_owned(), json!(self.instructions));
        cacheable(&mut result);
        result
    }

    fn list_tools(&self, revision: Revision, params: Option<&Map<String, Value>>) -> Handled {
        // The list fits one page, so no cursor was ever given out.
        if params.is_some_and(|params| params.get("cursor").is_some_and(|cursor| !cursor.is_null()))
        {
            return Handled::Error(RpcError::new(
                INVALID_PARAMS,
                "Invalid params: unknown cursor; this server lists its tools on one page",
            ));
        }
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                    "annotations": {
                        "readOnlyHint": tool.read_only,
                        "openWorldHint": tool.open_world,
                    },
                })
            })
            .collect();
        let mut result = Map::new();
        result.insert("tools".to_owned(), json!(tools));
        if !revision.is_legacy() {
            cacheable(&mut result);
        }
        Handled::Result(result)
    }

    fn call_tool(&self, params: Option<&Map<String, Value>>) -> Handled {
        let invalid = |problem: String| {
            Handled::Error(RpcError::new(
                INVALID_PARAMS,
                format!("Invalid params for tools/call: {problem}"),
            ))
        };
        let Some(name) = params
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str)
        else {
            return invalid("'name' must be a string".to_owned());
        };
        let arguments = match params.and_then(|params| params.get("arguments")) {
            None => Map::new(),
            Some(Value::Object(arguments)) => arguments.clone(),
            Some(_) => return invalid("'arguments' must be an object".to_owned()),
        };
        if !self.tools.iter().any(|tool| tool.name == name) {
            return Handled::Error(RpcError::new(
                INVALID_PARAMS,
                format!("Unknown tool: {}", quoted(name)),
            ));
        }
        Handled::Call(ToolCall {
            name: name.to_owned(),
            arguments,
        })
    }

    /// The server's name and version as a revision writes them.
    fn server_info(&self, revision: Revision) -> Value {
        let mut info = Map::new();
        info.insert("name".to_owned(), json!(self.info.name));
        if revision.has_titles() {
            info.insert("title".to_owned(), json!(self.info.title));
        }
        info.insert("version".to_owned(), json!(self.info.version));
        Value::Object(info)
    }
}

/// What the server can do: it has tools, and their list does not change
/// while it runs.
fn capabilities() -> Value {
    json!({"tools": {}})
}

/// Marks a result as one a client may keep for a while, and anyone may
/// share: it is the same for every caller.
fn cacheable(result: &mut Map<String, Value>) {
    result.insert("ttlMs".to_owned(), json!(LIST_TTL_MS));
    result.insert("cacheScope".to_owned(), json!("public"));
}

/// Adds to a result what the newest revision asks of every result: that it
/// is complete, and who the server is.
fn decorate(
    mut result: Map<String, Value>,
    revision: Revision,
    server_info: &Value,
) -> Map<String, Value> {
    if !revision.is_legacy() {
        result.insert("resultType".to_owned(), json!("complete"));
        result.insert("_meta".to_owned(), json!({META_SERVER_INFO: server_info}));
    }
    result
}

/// The revision a message says it speaks, in the `_meta` of its
/// parameters. Only the newest revision's requests say so.
fn declared_revision<'a>(message: &Message<'a>) -> Option<&'a str> {
    match message {
        Message::Request { params, .. } => params
            .and_then(|params| params.get("_meta"))
            .and_then(|meta| meta.get(META_PROTOCOL_VERSION))
            .and_then(Value::as_str),
        _ => None,
    }
}

/// The HTTP status of an error in the newest revision, which names one
/// for most. Where it names none (an unknown tool, a cursor never given
/// out) the request was a bad one all the same.
fn modern_status(code: i64) -> u16 {
    match code {
        METHOD_NOT_FOUND => 404,
        INTERNAL_ERROR => 500,
        _ => 400,
    }
}

fn answer(status: u16, body: &Value) -> Read {
    Read::Answer(Answer::json(status, body))
}

fn header_mismatch(problem: &str, id: Option<&Value>) -> Read {
    let error = RpcError::new(HEADER_MISMATCH, format!("Header mismatch: {problem}"));
    // Only a request of the newest revision can get here.
    answer(400, &error.response(id, Some(Revision::V2026_07_28)))
}

/// The answer to a revision the server does not speak. It lists those it
/// does, which is what a client needs to try again.
fn unsupported(named: &str, id: Option<&Value>) -> Read {
    let error = RpcError::new(UNSUPPORTED_PROTOCOL_VERSION, "Unsupported protocol version")
        .with_data(json!({"supported": Revision::names(), "requested": quoted(named)}));
    answer(400, &error.response(id, None))
}

fn unexpected_response(revision: Revision) -> Read {
    let error = RpcError::new(
        INVALID_REQUEST,
        "Invalid request: this server asks nothing of its clients and expects no responses",
    );
    answer(400, &error.response(None, Some(revision)))
}

fn legacy_names() -> Vec<&'static str> {
    Revision::ALL
        .into_iter()
        .filter(|revision| revision.is_legacy())
        .map(Revision::as_str)
        .collect()
}

/// A piece of the client's own text, short enough to repeat in an error.
fn quoted(text: &str) -> String {
    text.chars().take(QUOTED_CHARS).collect()
}
