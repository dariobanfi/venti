//! JSON-RPC messages: telling a request from a notification from a
//! response, and the errors the protocol answers with.

use serde_json::{Map, Value, json};

use crate::Revision;

/// The body is not JSON.
pub const PARSE_ERROR: i64 = -32700;
/// The body is JSON and not a message the protocol knows.
pub const INVALID_REQUEST: i64 = -32600;
/// The server has no such method.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// The method exists and its parameters cannot be used: an unknown tool,
/// a missing field, a cursor the server never gave out.
pub const INVALID_PARAMS: i64 = -32602;
/// The server failed.
pub const INTERNAL_ERROR: i64 = -32603;
/// A header of the request is missing or says something else than the
/// body. Defined by revision 2026-07-28.
pub const HEADER_MISMATCH: i64 = -32020;
/// The request names a revision of the protocol the server does not speak.
/// Defined by revision 2026-07-28.
pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// An error as JSON-RPC carries it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    pub(crate) data: Option<Value>,
}

impl RpcError {
    pub(crate) fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub(crate) fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// The error as the response to the request with `id`. `revision` is
    /// the revision the request spoke, if that could be told: it decides
    /// what stands for an `id` that could not be read.
    pub(crate) fn response(&self, id: Option<&Value>, revision: Option<Revision>) -> Value {
        let mut error = Map::new();
        error.insert("code".to_owned(), json!(self.code));
        error.insert("message".to_owned(), json!(self.message));
        if let Some(data) = &self.data {
            error.insert("data".to_owned(), data.clone());
        }
        let mut response = Map::new();
        response.insert("jsonrpc".to_owned(), json!("2.0"));
        match id {
            Some(id) => {
                response.insert("id".to_owned(), id.clone());
            }
            // The older revisions require the field; JSON-RPC puts `null`
            // there. The newer ones leave it out.
            None if revision.is_some_and(|revision| !revision.may_omit_error_id()) => {
                response.insert("id".to_owned(), Value::Null);
            }
            None => {}
        }
        response.insert("error".to_owned(), Value::Object(error));
        Value::Object(response)
    }
}

/// A result as the response to the request with `id`.
pub(crate) fn result_response(id: &Value, result: Map<String, Value>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// One JSON-RPC message from a client.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Message<'a> {
    /// Something is asked, and an answer with the same `id` is due.
    Request {
        id: &'a Value,
        method: &'a str,
        params: Option<&'a Map<String, Value>>,
    },
    /// Something is told, and no answer is due.
    Notification { method: &'a str },
    /// An answer to something the server asked. This server asks nothing.
    Response,
}

/// A body, or a part of one, that is no message.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NotAMessage<'a> {
    /// The `id` it carries, if it has a usable one, so that the refusal
    /// can name it.
    pub(crate) id: Option<&'a Value>,
    pub(crate) problem: &'static str,
}

/// Reads one message.
///
/// The protocol narrows JSON-RPC: an `id` is a string or a whole number,
/// never `null`, and parameters are an object, never a list.
pub(crate) fn read(value: &Value) -> Result<Message<'_>, NotAMessage<'_>> {
    let no = |id, problem| Err(NotAMessage { id, problem });
    let Some(object) = value.as_object() else {
        return no(None, "a message must be a JSON object");
    };
    let id = match object.get("id") {
        None => None,
        Some(id) if id.is_string() || id.is_i64() || id.is_u64() => Some(id),
        Some(_) => return no(None, "an id must be a string or a whole number"),
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return no(id, "jsonrpc must be \"2.0\"");
    }
    let method = match object.get("method") {
        Some(Value::String(method)) => method.as_str(),
        Some(_) => return no(id, "method must be a string"),
        None if object.contains_key("result") || object.contains_key("error") => {
            return Ok(Message::Response);
        }
        None => return no(id, "a message needs a method"),
    };
    let params = match object.get("params") {
        None => None,
        Some(Value::Object(params)) => Some(params),
        Some(_) => return no(id, "params must be an object"),
    };
    Ok(match id {
        Some(id) => Message::Request { id, method, params },
        None => Message::Notification { method },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_notifications_and_responses_are_told_apart() {
        let request = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/list", "params": {}});
        assert!(matches!(
            read(&request),
            Ok(Message::Request { id, method: "tools/list", params: Some(_) }) if id == &json!(7)
        ));
        let bare = json!({"jsonrpc": "2.0", "id": "a", "method": "ping"});
        assert!(matches!(
            read(&bare),
            Ok(Message::Request { params: None, .. })
        ));
        let told = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert_eq!(
            read(&told),
            Ok(Message::Notification {
                method: "notifications/initialized"
            })
        );
        for response in [
            json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
            json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -1, "message": "no"}}),
        ] {
            assert_eq!(read(&response), Ok(Message::Response));
        }
    }

    #[test]
    fn what_is_no_message_is_refused_with_its_id_if_it_has_one() {
        let refused = |value: Value| read(&value).map(|_| ()).unwrap_err().problem.to_owned();
        assert!(refused(json!("tools/list")).contains("JSON object"));
        assert!(refused(json!([])).contains("JSON object"));
        assert!(refused(json!({"id": 1, "method": "ping"})).contains("jsonrpc"));
        assert!(refused(json!({"jsonrpc": "1.0", "id": 1, "method": "ping"})).contains("jsonrpc"));
        assert!(refused(json!({"jsonrpc": "2.0", "id": 1})).contains("needs a method"));
        assert!(refused(json!({"jsonrpc": "2.0", "id": 1, "method": 5})).contains("string"));
        assert!(
            refused(json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": [1]}))
                .contains("object")
        );
        // The protocol's ids are narrower than JSON-RPC's.
        for id in [json!(null), json!(1.5), json!(true), json!({}), json!([1])] {
            let value = json!({"jsonrpc": "2.0", "id": id, "method": "ping"});
            let refusal = read(&value).unwrap_err();
            assert!(refusal.problem.contains("an id must be"), "{id}");
            assert_eq!(refusal.id, None);
        }
        // A usable id is handed back for the refusal.
        let value = json!({"jsonrpc": "2.0", "id": "x"});
        assert_eq!(read(&value).unwrap_err().id, Some(&json!("x")));
    }

    #[test]
    fn an_error_names_its_request_or_what_the_revision_puts_in_its_place() {
        let error = RpcError::new(INVALID_PARAMS, "Unknown tool: nothing")
            .with_data(json!({"tool": "nothing"}));
        assert_eq!(
            error.response(Some(&json!("call-1")), Some(Revision::V2026_07_28)),
            json!({
                "jsonrpc": "2.0",
                "id": "call-1",
                "error": {"code": -32602, "message": "Unknown tool: nothing", "data": {"tool": "nothing"}}
            })
        );

        let unread = RpcError::new(PARSE_ERROR, "not JSON");
        for (revision, id) in [
            (Some(Revision::V2025_03_26), Some(Value::Null)),
            (Some(Revision::V2025_06_18), Some(Value::Null)),
            (Some(Revision::V2025_11_25), None),
            (Some(Revision::V2026_07_28), None),
            (None, None),
        ] {
            let response = unread.response(None, revision);
            assert_eq!(response.get("id").cloned(), id, "{revision:?}");
            assert!(response["error"].get("data").is_none());
        }
    }
}
