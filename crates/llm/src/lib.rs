//! Language model providers for the in-app assistant.
//!
//! The project pays for no inference: the pilot brings a key for their own
//! account with a provider, and the app talks to that provider directly from
//! the device. This crate knows each provider's wire format. Like the data
//! source adapters it does no I/O: it builds the HTTP request and reads the
//! reply, and whoever calls it sends the bytes. That keeps the key out of
//! everything except one request header, and makes every provider testable
//! from recorded replies.
//!
//! A conversation is a list of [`Message`]s made of [`Block`]s: text, a call
//! to one of the app's tools, or a tool's result. [`Provider::request`] turns
//! a conversation into a request and [`Provider::parse`] turns the reply into
//! the assistant's next message. The loop that runs the tools between the
//! two lives with the tools, in the client core.
//!
//! Providers so far: Anthropic's Messages API, and the chat completions
//! format that OpenAI defined and many other services and local model
//! servers speak.

mod anthropic;
mod chat_completions;

pub use anthropic::Anthropic;
pub use chat_completions::ChatCompletions;
use serde_json::Value;

/// An HTTP request for whoever does the I/O to send as a `POST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Where to send it.
    pub url: String,
    /// The headers, the pilot's key among them.
    pub headers: Vec<(String, String)>,
    /// The JSON body.
    pub body: String,
}

/// Who is speaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The pilot, or the app on the pilot's side (tool results).
    User,
    /// The model.
    Assistant,
}

/// One part of a message.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Words.
    Text(String),
    /// The model asks for one of the app's tools to be run.
    ToolCall {
        /// The provider's identifier for this call, to match the result.
        id: String,
        /// The tool's name.
        name: String,
        /// The arguments, as the tool's schema describes them.
        input: Value,
    },
    /// What a tool returned.
    ToolResult {
        /// The identifier of the call this answers.
        call_id: String,
        /// The result as text, usually JSON.
        content: String,
        /// Whether the tool failed; the content then says why.
        is_error: bool,
    },
}

/// One message of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Who is speaking.
    pub role: Role,
    /// What is said.
    pub blocks: Vec<Block>,
    /// For a message that came from a provider: its content exactly as the
    /// provider sent it. It is sent back unchanged in later requests,
    /// because a reply can hold parts this crate does not model (such as the
    /// model's reasoning) that the provider wants returned as they were.
    pub verbatim: Option<Value>,
}

impl Message {
    /// A message from the pilot.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            blocks: vec![Block::Text(text.into())],
            verbatim: None,
        }
    }

    /// The results of the tools the model called, sent back on the pilot's
    /// side as the providers require.
    pub fn tool_results(results: Vec<Block>) -> Self {
        Self {
            role: Role::User,
            blocks: results,
            verbatim: None,
        }
    }

    /// The message's words, without tool traffic.
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    /// Its name.
    pub name: String,
    /// What it does and when to use it, for the model.
    pub description: String,
    /// A JSON Schema of its arguments.
    pub input_schema: Value,
}

/// Everything a provider needs for one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation<'a> {
    /// The provider's name for the model to use.
    pub model: &'a str,
    /// Standing instructions.
    pub system: &'a str,
    /// The tools on offer.
    pub tools: &'a [ToolSpec],
    /// The conversation so far, ending with a message on the pilot's side.
    pub messages: &'a [Message],
    /// The most tokens the reply may have.
    pub max_tokens: u32,
}

/// Why the model stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// It finished what it had to say.
    Finished,
    /// It wants tools run and their results sent back.
    ToolUse,
    /// It ran into the token limit; the reply is cut off.
    TooLong,
    /// It declined to answer.
    Refused,
    /// Anything else the provider reports, by its own name.
    Other(String),
}

/// A provider's reply to one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    /// The assistant's message.
    pub message: Message,
    /// Why it stopped.
    pub stop: Stop,
    /// Tokens read and written, if the provider reports them. The pilot
    /// pays for these.
    pub usage: Option<(u64, u64)>,
}

/// Why a reply could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    /// The provider refused the key.
    #[error("the provider did not accept the key")]
    BadKey,
    /// The account is out of credit or over its rate limit.
    #[error("the provider is limiting this account: {0}")]
    Limited(String),
    /// The provider is overloaded or down. Trying again later may work.
    #[error("the provider is not available right now: {0}")]
    Unavailable(String),
    /// The provider rejected the request for another reason.
    #[error("the provider rejected the request: {0}")]
    Rejected(String),
    /// The reply was not in the form the provider documents.
    #[error("the provider's reply could not be read: {0}")]
    Malformed(String),
}

/// One provider's wire format.
pub trait Provider {
    /// The request for a conversation. `key` is the pilot's credential; it
    /// goes into a header and nowhere else.
    fn request(&self, key: &str, conversation: &Conversation<'_>) -> HttpRequest;

    /// Reads the provider's answer to such a request.
    fn parse(&self, status: u16, body: &[u8]) -> Result<Reply, LlmError>;
}
