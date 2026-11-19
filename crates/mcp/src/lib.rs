//! The Model Context Protocol, as a server speaks it over HTTP.
//!
//! MCP is how an assistant such as Claude or ChatGPT uses tools that
//! somebody else's server offers. A pilot adds Venti's server to their own
//! assistant as a *connector*, and the assistant can then ask it for the
//! wind at a place. This crate is the protocol and nothing else: it reads
//! one HTTP request into what is asked, and builds the answer. It does no
//! I/O, knows nothing about wind or flying, and depends on no other crate
//! of the workspace. Whoever uses it owns the socket and runs the tools.
//!
//! ```
//! use serde_json::json;
//! use venti_mcp::{HttpRequest, Implementation, Origins, Read, Server, Tool, ToolResult};
//!
//! let server = Server::new(
//!     Implementation {
//!         name: "example".to_owned(),
//!         title: "Example".to_owned(),
//!         version: "1.0.0".to_owned(),
//!     },
//!     "Answers with the time.",
//!     vec![Tool {
//!         name: "time".to_owned(),
//!         description: "The current time.".to_owned(),
//!         input_schema: json!({"type": "object", "additionalProperties": false}),
//!         read_only: true,
//!         open_world: false,
//!     }],
//!     // No page in a browser may call this server.
//!     Origins::Only(Vec::new()),
//! );
//!
//! let body = br#"{"jsonrpc": "2.0", "id": 1, "method": "tools/call",
//!                 "params": {"name": "time", "arguments": {}}}"#;
//! let request = HttpRequest {
//!     method: "POST",
//!     headers: &[("content-type", "application/json")],
//!     body,
//! };
//! let answer = match server.read(&request) {
//!     Read::Answer(answer) => answer,
//!     Read::Calls(calls) => {
//!         assert_eq!(calls.calls()[0].name, "time");
//!         calls.finish(vec![ToolResult {
//!             text: "12:00".to_owned(),
//!             is_error: false,
//!         }])
//!     }
//! };
//! assert_eq!(answer.status, 200);
//! ```
//!
//! # What is spoken
//!
//! The *Streamable HTTP* transport, in four dated revisions of the
//! specification at <https://modelcontextprotocol.io/specification>
//! ([`Revision`]):
//!
//! | Revision | How a conversation begins |
//! |---|---|
//! | 2026-07-28 | It does not: every request names its revision and stands alone |
//! | 2025-11-25, 2025-06-18, 2025-03-26 | With an `initialize` request |
//!
//! Both kinds are answered at the one endpoint, by what each request says
//! about itself:
//!
//! - A request whose `MCP-Protocol-Version` header names 2026-07-28 is one
//!   of the new kind. Its other headers (`Mcp-Method`, and `Mcp-Name` for a
//!   tool call) must agree with its body, and the body's `_meta` must name
//!   the same revision.
//! - A request whose header names one of the three older revisions is of
//!   that revision.
//! - A request without the header is taken for 2025-03-26, whose clients
//!   send none, as the later revisions ask servers to do.
//! - `initialize` is answered from the revision its body asks for: with
//!   that one if it is one of the three, and with 2025-11-25 if not.
//! - Any other revision is refused with the list of these four.
//!
//! # What is offered
//!
//! Tools, and nothing else: `tools/list` and `tools/call`, with
//! `initialize` and `ping` in the older revisions and `server/discover` in
//! the newest. Every other method is answered "method not found".
//!
//! Every answer is one JSON object, or a list of them for a batch, which
//! only 2025-03-26 has. The server keeps no session and opens no stream: a
//! `GET` or `DELETE` gets `405`, which is how a server says so. It asks
//! nothing of the client, so a response sent to it is refused, and a
//! notification is accepted with `202` and has no effect.
//!
//! # What is refused before a body is read
//!
//! - A request with an `Origin` header that is not allowed: `403`. See
//!   [`Origins`].
//! - Anything but `POST`: `405`.
//! - A body that is not said to be `application/json`: `415`. A page on
//!   another site cannot send that without asking first, so this is also
//!   what keeps a browser from being used to reach the server.
//! - A client that will not take JSON for an answer: `406`.
//!
//! These carry a JSON-RPC error whose code is the HTTP status. The codes
//! the protocol itself defines are in this crate's constants.
//!
//! # HTTP status of an error
//!
//! The older revisions deliver the error of an understood request as its
//! response, with `200`. The newest names a status for most errors (`400`
//! for a bad request, a missing header or an unknown revision; `404` for
//! an unknown method) and none for some: an unknown tool, a cursor never
//! given out. Those get `400` here. *To verify* against other servers once
//! any exist for that revision.

mod http;
mod message;
mod revision;
mod server;

pub use http::{Answer, HttpRequest};
pub use message::{
    HEADER_MISMATCH, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND,
    PARSE_ERROR, UNSUPPORTED_PROTOCOL_VERSION,
};
pub use revision::Revision;
pub use server::{
    Calls, Implementation, MAX_BATCH, Origins, Read, Server, Tool, ToolCall, ToolResult,
};
