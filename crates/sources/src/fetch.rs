use std::future::Future;

use venti_domain::Timestamp;

use crate::Request;

/// Why a request produced no usable body.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// The server answered with a status other than success.
    #[error("HTTP status {0}")]
    Status(u16),
    /// The request did not complete.
    #[error("transport error: {0}")]
    Transport(String),
}

/// What a provider sent in answer to a request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fetched {
    /// The response body.
    pub body: Vec<u8>,
    /// When the provider last changed what it sent, by its own word: the
    /// `Last-Modified` header of the response. `None` when it does not say.
    pub last_modified: Option<Timestamp>,
}

impl From<Vec<u8>> for Fetched {
    /// A body the provider put no date on.
    fn from(body: Vec<u8>) -> Self {
        Self {
            body,
            last_modified: None,
        }
    }
}

/// Something that can perform the GET requests adapters ask for.
///
/// The server uses a real HTTP client. Tests use recorded payloads.
pub trait Fetch: Sync {
    /// Performs the request and returns what came back.
    fn get(&self, request: &Request) -> impl Future<Output = Result<Fetched, FetchError>> + Send;
}
