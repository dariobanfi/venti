//! Anthropic's Messages API, as documented at platform.claude.com/docs,
//! read 2026-11-19: the `2026-10-21` version of the API.

use serde_json::{Value, json};

use crate::{Block, Conversation, HttpRequest, LlmError, Message, Provider, Reply, Role, Stop};

/// The version of the API this code is written against.
const VERSION: &str = "2026-10-21";

/// Anthropic's Messages API.
#[derive(Debug, Clone, Copy, Default)]
pub struct Anthropic;

impl Provider for Anthropic {
    fn request(&self, key: &str, conversation: &Conversation<'_>) -> HttpRequest {
        let tools: Vec<Value> = conversation
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema,
                })
            })
            .collect();
        let body = json!({
            "model": conversation.model,
            "max_tokens": conversation.max_tokens,
            "system": conversation.system,
            "tools": tools,
            "messages": conversation.messages.iter().map(message).collect::<Vec<_>>(),
            // Lets the provider reuse its reading of the instructions, the
            // tools and the earlier turns between requests, which makes
            // every round of a tool loop cheaper for the pilot.
            "cache_control": {"type": "ephemeral"},
        });
        HttpRequest {
            url: "https://api.anthropic.com/v1/messages".to_owned(),
            headers: vec![
                ("x-api-key".to_owned(), key.to_owned()),
                ("anthropic-version".to_owned(), VERSION.to_owned()),
                ("content-type".to_owned(), "application/json".to_owned()),
            ],
            body: body.to_string(),
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
        if !(200..300).contains(&status) || reply["type"] == "error" {
            return Err(failure(status, &reply));
        }

        let content = reply["content"]
            .as_array()
            .ok_or_else(|| LlmError::Malformed("the reply has no content".to_owned()))?;
        let blocks = content
            .iter()
            .filter_map(|block| match block["type"].as_str()? {
                "text" => Some(Block::Text(block["text"].as_str()?.to_owned())),
                "tool_use" => Some(Block::ToolCall {
                    id: block["id"].as_str()?.to_owned(),
                    name: block["name"].as_str()?.to_owned(),
                    input: block["input"].clone(),
                }),
                // Reasoning and other parts are kept in `verbatim` only.
                _ => None,
            })
            .collect();

        Ok(Reply {
            message: Message {
                role: Role::Assistant,
                blocks,
                verbatim: Some(reply["content"].clone()),
            },
            stop: match reply["stop_reason"].as_str().unwrap_or_default() {
                "end_turn" | "stop_sequence" => Stop::Finished,
                "tool_use" => Stop::ToolUse,
                "max_tokens" | "model_context_window_exceeded" => Stop::TooLong,
                "refusal" => Stop::Refused,
                other => Stop::Other(other.to_owned()),
            },
            usage: reply["usage"]["input_tokens"]
                .as_u64()
                .zip(reply["usage"]["output_tokens"].as_u64()),
        })
    }
}

/// One message in the API's form.
fn message(message: &Message) -> Value {
    let role = match message.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    if let Some(verbatim) = &message.verbatim {
        return json!({"role": role, "content": verbatim});
    }
    let content: Vec<Value> = message
        .blocks
        .iter()
        .map(|block| match block {
            Block::Text(text) => json!({"type": "text", "text": text}),
            Block::ToolCall { id, name, input } => {
                json!({"type": "tool_use", "id": id, "name": name, "input": input})
            }
            Block::ToolResult {
                call_id,
                content,
                is_error,
            } => json!({
                "type": "tool_result",
                "tool_use_id": call_id,
                "content": content,
                "is_error": is_error,
            }),
        })
        .collect();
    json!({"role": role, "content": content})
}

/// The error for a reply that reports failure.
fn failure(status: u16, reply: &Value) -> LlmError {
    let kind = reply["error"]["type"].as_str().unwrap_or_default();
    let said = reply["error"]["message"]
        .as_str()
        .map_or_else(|| format!("HTTP status {status}"), str::to_owned);
    match (status, kind) {
        (401 | 403, _) | (_, "authentication_error" | "permission_error") => LlmError::BadKey,
        (402 | 429, _) | (_, "rate_limit_error" | "billing_error") => LlmError::Limited(said),
        (500.., _) | (_, "overloaded_error" | "api_error") => LlmError::Unavailable(said),
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

    #[test]
    fn the_request_is_what_the_api_documents_and_carries_the_key_only_in_a_header() {
        let tools = tools();
        let messages = [Message::user("How many flights did I fly in 2026?")];
        let request = Anthropic.request(
            "sk-ant-test",
            &Conversation {
                model: "claude-fable-5-2",
                system: "You are the assistant in a paragliding app.",
                tools: &tools,
                messages: &messages,
                max_tokens: 2048,
            },
        );

        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        assert!(
            request
                .headers
                .contains(&("x-api-key".to_owned(), "sk-ant-test".to_owned()))
        );
        assert!(
            request
                .headers
                .contains(&("anthropic-version".to_owned(), "2026-10-21".to_owned()))
        );
        assert!(!request.body.contains("sk-ant-test"));

        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["model"], "claude-fable-5-2");
        assert_eq!(body["max_tokens"], 2048);
        assert_eq!(
            body["system"],
            "You are the assistant in a paragliding app."
        );
        assert_eq!(body["tools"][0]["name"], "list_flights");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(
            body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "How many flights did I fly in 2026?"}]}])
        );
        // The 2026-10-21 version rejects sampling parameters.
        for removed in ["temperature", "top_p", "top_k"] {
            assert!(body.get(removed).is_none(), "{removed}");
        }
    }

    #[test]
    fn a_reply_that_calls_a_tool_is_read_and_sent_back_unchanged_with_the_result() {
        let reply = Anthropic
            .parse(
                200,
                include_bytes!("../tests/fixtures/anthropic-tool-use.json"),
            )
            .unwrap();

        assert_eq!(reply.stop, Stop::ToolUse);
        assert_eq!(reply.usage, Some((1840, 96)));
        assert_eq!(reply.message.text(), "I'll look at your 2026 flights.");
        let call = reply
            .message
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::ToolCall { id, name, input } => Some((id, name, input)),
                _ => None,
            })
            .unwrap();
        assert_eq!(call.0, "toolu_01A09q90qw90lq917835lq9");
        assert_eq!(call.1, "list_flights");
        assert_eq!(call.2["year"], 2026);

        // The next request returns the assistant's message as it came,
        // reasoning included, followed by the tool's result.
        let messages = [
            Message::user("How many flights did I fly in 2026?"),
            reply.message.clone(),
            Message::tool_results(vec![Block::ToolResult {
                call_id: call.0.clone(),
                content: "{\"flights\":[]}".to_owned(),
                is_error: false,
            }]),
        ];
        let tools = tools();
        let request = Anthropic.request(
            "key",
            &Conversation {
                model: "claude-fable-5-2",
                system: "",
                tools: &tools,
                messages: &messages,
                max_tokens: 1024,
            },
        );
        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][1]["content"][0]["type"], "thinking");
        assert_eq!(body["messages"][1]["content"][2]["type"], "tool_use");
        assert_eq!(
            body["messages"][2],
            json!({"role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "toolu_01A09q90qw90lq917835lq9",
                "content": "{\"flights\":[]}",
                "is_error": false,
            }]})
        );
    }

    #[test]
    fn a_final_reply_is_read() {
        let reply = Anthropic
            .parse(
                200,
                include_bytes!("../tests/fixtures/anthropic-end-turn.json"),
            )
            .unwrap();
        assert_eq!(reply.stop, Stop::Finished);
        assert_eq!(
            reply.message.text(),
            "You flew 14 flights in 2026, 31 hours in the air."
        );
    }

    #[test]
    fn failures_are_told_apart_by_what_the_pilot_can_do_about_them() {
        let error = |status: u16, kind: &str| {
            let body = json!({"type": "error", "error": {"type": kind, "message": "said so"}});
            Anthropic
                .parse(status, body.to_string().as_bytes())
                .unwrap_err()
        };
        assert_eq!(error(401, "authentication_error"), LlmError::BadKey);
        assert_eq!(
            error(429, "rate_limit_error"),
            LlmError::Limited("said so".to_owned())
        );
        assert_eq!(
            error(529, "overloaded_error"),
            LlmError::Unavailable("said so".to_owned())
        );
        assert_eq!(
            error(400, "invalid_request_error"),
            LlmError::Rejected("said so".to_owned())
        );

        // A gateway's HTML error page instead of JSON.
        assert_eq!(
            Anthropic
                .parse(502, b"<html>Bad Gateway</html>")
                .unwrap_err(),
            LlmError::Unavailable("HTTP status 502".to_owned())
        );
        assert!(matches!(
            Anthropic.parse(200, b"not json").unwrap_err(),
            LlmError::Malformed(_)
        ));
        assert!(matches!(
            Anthropic.parse(200, b"{}").unwrap_err(),
            LlmError::Malformed(_)
        ));
    }

    #[test]
    fn other_stop_reasons_are_reported() {
        let reply = |reason: &str| {
            let body = json!({"content": [{"type": "text", "text": "…"}], "stop_reason": reason});
            Anthropic
                .parse(200, body.to_string().as_bytes())
                .unwrap()
                .stop
        };
        assert_eq!(reply("max_tokens"), Stop::TooLong);
        assert_eq!(reply("refusal"), Stop::Refused);
        assert_eq!(reply("pause_turn"), Stop::Other("pause_turn".to_owned()));
    }
}
