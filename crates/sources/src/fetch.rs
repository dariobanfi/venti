use std::future::Future;

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

/// Something that can perform the GET requests adapters ask for.
///
/// The server uses a real HTTP client. Tests use recorded payloads.
pub trait Fetch: Sync {
    /// Performs the request and returns the response body.
    fn get(&self, request: &Request) -> impl Future<Output = Result<Vec<u8>, FetchError>> + Send;
}
