//! The chat completions format: OpenAI's API, which many other services
//! and the servers people run for local models speak as well.
//!
//! Written against OpenAI's own description of it (the OpenAPI document at
//! github.com/openai/openai-openapi, read 2026-11-19): the request with
//! `messages` and `tools`, a reply's `choices[0].message` with
//! `tool_calls`, and the `tool` messages that answer them. Other services
//! say they follow it; where they differ is to verify, service by service.
//! The one choice that could matter: the limit on a reply's length is sent
//! as `max_completion_tokens`, the name the description gives now, and not
//! as `max_tokens`, which it marks as deprecated and which OpenAI's
//! reasoning models refuse.

use serde_json::{Map, Value, json};

use crate::{Block, Conversation, HttpRequest, LlmError, Message, Provider, Reply, Role, Stop};

/// A service that speaks the chat completions format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCompletions {
    /// The address the API lives under, without a slash at the end:
    /// `https://api.openai.com/v1`. The request goes to
    /// `/chat/completions` below it.
    pub base_url: String,
}

impl ChatCompletions {
    /// A service by the address its API lives under. A slash at the end
    /// is left off.
    pub fn at(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
        }
    }
}

impl Provider for ChatCompletions {
    fn request(&self, key: &str, conversation: &Conversation<'_>) -> HttpRequest {
        let tools: Vec<Value> = conversation
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.input_schema,
                    },
                })
            })
            .collect();
        let mut messages = vec![json!({"role": "system", "content": conversation.system})];
        messages.extend(conversation.messages.iter().flat_map(wire_messages));
        let mut body = Map::new();
        body.insert("model".to_owned(), json!(conversation.model));
        body.insert("messages".to_owned(), Value::Array(messages));
        // An empty list of tools is refused by some services.
        if !tools.is_empty() {
            body.insert("tools".to_owned(), Value::Array(tools));
        }
        body.insert(
            "max_completion_tokens".to_owned(),
            json!(conversation.max_tokens),
        );
        HttpRequest {
            url: format!("{}/chat/completions", self.base_url),
            headers: vec![
                ("authorization".to_owned(), format!("Bearer {key}")),
                ("content-type".to_owned(), "application/json".to_owned()),
            ],
            body: Value::Object(body).to_string(),
        }
    }

    fn parse(&self, status: u16, body: &[u8]) -> Result<Reply, LlmError> {
        let reply: Value = serde_json::from_slice(body).map_err(|error| {
            if (200..300).contains(&status) {
                LlmError::Malformed(error.to_string())
            } else {
                failure(status, &Value::Null)
            }
        })?;
        if !(200..300).contains(&status) || reply.get("error").is_some_and(|error| !error.is_null())
        {
            return Err(failure(status, &reply));
        }

        let choice = &reply["choices"][0];
        let said = choice["message"]
            .as_object()
            .ok_or_else(|| LlmError::Malformed("the reply has no message".to_owned()))?;
        let mut blocks = Vec::new();
        if let Some(text) = said.get("content").and_then(Value::as_str)
            && !text.is_empty()
        {
            blocks.push(Block::Text(text.to_owned()));
        }
        let calls = said
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for call in calls {
            let (Some(id), Some(name)) = (call["id"].as_str(), call["function"]["name"].as_str())
            else {
                continue;
            };
            // The arguments come as JSON in a text, and the description
            // warns that a model does not always write valid JSON. What
            // cannot be read is handed on as no arguments at all: the tool
            // then says what it is missing, and the model can try again.
            let input = call["function"]["arguments"]
                .as_str()
                .and_then(|arguments| serde_json::from_str(arguments).ok())
                .unwrap_or(Value::Null);
            blocks.push(Block::ToolCall {
                id: id.to_owned(),
                name: name.to_owned(),
                input,
            });
        }
        let has_calls = blocks
            .iter()
            .any(|block| matches!(block, Block::ToolCall { .. }));

        Ok(Reply {
            message: Message {
                role: Role::Assistant,
                blocks,
                // The message as it came, to send back as it was: a
                // service may have put more into it than is read here.
                verbatim: Some(Value::Object(said.clone())),
            },
            stop: match choice["finish_reason"].as_str().unwrap_or_default() {
                // Some services say "stop" although they ask for tools.
                _ if has_calls => Stop::ToolUse,
                "stop" => Stop::Finished,
                "tool_calls" | "function_call" => Stop::ToolUse,
                "length" => Stop::TooLong,
                "content_filter" => Stop::Refused,
                other => Stop::Other(other.to_owned()),
            },
            usage: reply["usage"]["prompt_tokens"]
                .as_u64()
                .zip(reply["usage"]["completion_tokens"].as_u64()),
        })
    }
}

/// One message of the conversation as the messages of the format: the
/// results of several tools are a message each.
fn wire_messages(message: &Message) -> Vec<Value> {
    // What the service sent is sent back as it was, if it has the shape
    // of this format's messages. A reply kept from another provider does
    // not, and is built from its parts instead.
    if let Some(verbatim) = &message.verbatim
        && verbatim.get("role").is_some()
    {
        return vec![verbatim.clone()];
    }
    let text = message.text();
    match message.role {
        Role::User => {
            let mut wire: Vec<Value> = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    Block::ToolResult {
                        call_id,
                        content,
                        is_error,
                    } => Some(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        // The format has no field for a failed call; the
                        // content says so itself.
                        "content": if *is_error {
                            format!("The tool failed: {content}")
                        } else {
                            content.clone()
                        },
                    })),
                    _ => None,
                })
                .collect();
            if !text.is_empty() {
                wire.push(json!({"role": "user", "content": text}));
            }
            wire
        }
        Role::Assistant => {
            let calls: Vec<Value> = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    Block::ToolCall { id, name, input } => Some(json!({
                        "id": id,
                        "type": "function",
                        "function": {"name": name, "arguments": input.to_string()},
                    })),
                    _ => None,
                })
                .collect();
            let mut said = Map::new();
            said.insert("role".to_owned(), json!("assistant"));
            said.insert(
                "content".to_owned(),
                if text.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                },
            );
            if !calls.is_empty() {
                said.insert("tool_calls".to_owned(), Value::Array(calls));
            }
            vec![Value::Object(said)]
        }
    }
}

/// The error for a reply that reports failure.
fn failure(status: u16, reply: &Value) -> LlmError {
    // Most services put a message into an object; some put a text there.
    let error = &reply["error"];
    let said = error["message"]
        .as_str()
        .or_else(|| error.as_str())
        .map_or_else(|| format!("HTTP status {status}"), str::to_owned);
    let kind = error["type"].as_str().unwrap_or_default();
    let code = error["code"].as_str().unwrap_or_default();
    match status {
        401 | 403 => LlmError::BadKey,
        402 | 429 => LlmError::Limited(said),
        500.. => LlmError::Unavailable(said),
        _ if code == "invalid_api_key" || kind == "authentication_error" => LlmError::BadKey,
        _ if code == "insufficient_quota" || kind == "insufficient_quota" => {
            LlmError::Limited(said)
        }
        _ => LlmError::Rejected(said),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolSpec;

    fn tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "list_flights".to_owned(),
            description: "Lists the pilot's flights.".to_owned(),
            input_schema: json!({"type": "object", "properties": {"year": {"type": "integer"}}}),
        }]
    }

    fn conversation<'a>(tools: &'a [ToolSpec], messages: &'a [Message]) -> Conversation<'a> {
        Conversation {
            model: "some-model",
            system: "You are the assistant in a paragliding app.",
            tools,
            messages,
            max_tokens: 2048,
        }
    }

    #[test]
    fn the_request_is_what_the_format_describes_and_carries_the_key_only_in_a_header() {
        let tools = tools();
        let messages = [Message::user("How many flights did I fly in 2026?")];
        let request = ChatCompletions::at(" https://api.example.org/v1/ ")
            .request("sk-test", &conversation(&tools, &messages));

        assert_eq!(request.url, "https://api.example.org/v1/chat/completions");
        assert!(
            request
                .headers
                .contains(&("authorization".to_owned(), "Bearer sk-test".to_owned()))
        );
        assert!(!request.body.contains("sk-test"));

        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["model"], "some-model");
        assert_eq!(body["max_completion_tokens"], 2048);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(
            body["messages"],
            json!([
                {"role": "system", "content": "You are the assistant in a paragliding app."},
                {"role": "user", "content": "How many flights did I fly in 2026?"}
            ])
        );
        assert_eq!(
            body["tools"],
            json!([{"type": "function", "function": {
                "name": "list_flights",
                "description": "Lists the pilot's flights.",
                "parameters": {"type": "object", "properties": {"year": {"type": "integer"}}}
            }}])
        );

        // Without tools the list is left out.
        let plain = ChatCompletions::at("https://api.example.org/v1")
            .request("sk-test", &conversation(&[], &messages));
        let body: Value = serde_json::from_str(&plain.body).unwrap();
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn a_reply_that_asks_for_tools_is_read_and_sent_back_as_it_came() {
        let reply = json!({
            "id": "chatcmpl-1", "object": "chat.completion", "model": "some-model",
            "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
                "role": "assistant", "content": null, "refusal": null,
                "tool_calls": [
                    {"id": "call_1", "type": "function",
                     "function": {"name": "list_flights", "arguments": "{\"year\": 2026}"}},
                    {"id": "call_2", "type": "function",
                     "function": {"name": "list_flights", "arguments": "{not json"}}
                ]
            }}],
            "usage": {"prompt_tokens": 120, "completion_tokens": 18, "total_tokens": 138}
        });
        let service = ChatCompletions::at("https://api.example.org/v1");
        let read = service.parse(200, reply.to_string().as_bytes()).unwrap();
        assert_eq!(read.stop, Stop::ToolUse);
        assert_eq!(read.usage, Some((120, 18)));
        assert_eq!(
            read.message.blocks,
            vec![
                Block::ToolCall {
                    id: "call_1".to_owned(),
                    name: "list_flights".to_owned(),
                    input: json!({"year": 2026}),
                },
                // Arguments that are not JSON are handed on as none.
                Block::ToolCall {
                    id: "call_2".to_owned(),
                    name: "list_flights".to_owned(),
                    input: Value::Null,
                },
            ]
        );

        // The next request: the reply as it came, then a message for each
        // tool's result, a failed one saying so.
        let tools = tools();
        let messages = [
            Message::user("How many flights did I fly in 2026?"),
            read.message.clone(),
            Message::tool_results(vec![
                Block::ToolResult {
                    call_id: "call_1".to_owned(),
                    content: "{\"flights\": 3}".to_owned(),
                    is_error: false,
                },
                Block::ToolResult {
                    call_id: "call_2".to_owned(),
                    content: "bad arguments".to_owned(),
                    is_error: true,
                },
            ]),
        ];
        let request = service.request("sk-test", &conversation(&tools, &messages));
        let body: Value = serde_json::from_str(&request.body).unwrap();
        let sent = body["messages"].as_array().unwrap();
        assert_eq!(sent.len(), 5);
        assert_eq!(sent[2], reply["choices"][0]["message"]);
        assert_eq!(
            sent[3],
            json!({"role": "tool", "tool_call_id": "call_1", "content": "{\"flights\": 3}"})
        );
        assert_eq!(
            sent[4],
            json!({"role": "tool", "tool_call_id": "call_2", "content": "The tool failed: bad arguments"})
        );
    }

    #[test]
    fn an_answer_in_words_is_read_with_why_it_stopped() {
        let service = ChatCompletions::at("https://api.example.org/v1");
        let reply = |finish: &str| {
            json!({"choices": [{"finish_reason": finish,
                                "message": {"role": "assistant", "content": "Three flights."}}]})
            .to_string()
        };
        let read = service.parse(200, reply("stop").as_bytes()).unwrap();
        assert_eq!(read.message.text(), "Three flights.");
        assert_eq!(read.stop, Stop::Finished);
        assert_eq!(read.usage, None);
        assert_eq!(
            service.parse(200, reply("length").as_bytes()).unwrap().stop,
            Stop::TooLong
        );
        assert_eq!(
            service
                .parse(200, reply("content_filter").as_bytes())
                .unwrap()
                .stop,
            Stop::Refused
        );
        assert_eq!(
            service.parse(200, reply("else").as_bytes()).unwrap().stop,
            Stop::Other("else".to_owned())
        );

        // A service that says "stop" while asking for a tool wants the tool.
        let both = json!({"choices": [{"finish_reason": "stop", "message": {
            "role": "assistant", "content": "",
            "tool_calls": [{"id": "c", "type": "function",
                            "function": {"name": "list_flights", "arguments": "{}"}}]}}]});
        assert_eq!(
            service
                .parse(200, both.to_string().as_bytes())
                .unwrap()
                .stop,
            Stop::ToolUse
        );
    }

    #[test]
    fn a_message_built_here_or_kept_from_another_provider_is_put_into_the_formats_shape() {
        // An assistant's message without the service's own copy of it:
        // with the shape of another provider's content, or with none.
        let foreign = Message {
            role: Role::Assistant,
            blocks: vec![
                Block::Text("Let me look.".to_owned()),
                Block::ToolCall {
                    id: "toolu_1".to_owned(),
                    name: "list_flights".to_owned(),
                    input: json!({"year": 2026}),
                },
            ],
            verbatim: Some(json!([{"type": "text", "text": "Let me look."}])),
        };
        let messages = [Message::user("Hello"), foreign];
        let request = ChatCompletions::at("https://api.example.org/v1")
            .request("sk-test", &conversation(&[], &messages));
        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(
            body["messages"][2],
            json!({"role": "assistant", "content": "Let me look.", "tool_calls": [
                {"id": "toolu_1", "type": "function",
                 "function": {"name": "list_flights", "arguments": "{\"year\":2026}"}}
            ]})
        );
    }

    #[test]
    fn failures_are_told_apart_by_what_the_pilot_can_do_about_them() {
        let service = ChatCompletions::at("https://api.example.org/v1");
        let error = |message: &str, kind: &str| {
            json!({"error": {"message": message, "type": kind, "code": null}}).to_string()
        };
        assert_eq!(
            service.parse(
                401,
                error("Incorrect API key provided", "invalid_request_error").as_bytes()
            ),
            Err(LlmError::BadKey)
        );
        assert_eq!(
            service.parse(
                429,
                error("Rate limit reached", "rate_limit_error").as_bytes()
            ),
            Err(LlmError::Limited("Rate limit reached".to_owned()))
        );
        assert_eq!(
            service.parse(503, b"<html>Service Unavailable</html>"),
            Err(LlmError::Unavailable("HTTP status 503".to_owned()))
        );
        assert_eq!(
            service.parse(
                400,
                error("Unknown parameter", "invalid_request_error").as_bytes()
            ),
            Err(LlmError::Rejected("Unknown parameter".to_owned()))
        );
        // Some services put a text where others put an object, and some
        // answer 200 with an error in the body.
        assert_eq!(
            service.parse(400, br#"{"error": "model not found"}"#),
            Err(LlmError::Rejected("model not found".to_owned()))
        );
        assert_eq!(
            service.parse(
                200,
                error("You exceeded your quota", "insufficient_quota").as_bytes()
            ),
            Err(LlmError::Limited("You exceeded your quota".to_owned()))
        );
        assert!(matches!(
            service.parse(200, b"not json"),
            Err(LlmError::Malformed(_))
        ));
        assert!(matches!(
            service.parse(200, br#"{"choices": []}"#),
            Err(LlmError::Malformed(_))
        ));
    }
}
