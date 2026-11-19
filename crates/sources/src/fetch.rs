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
    /// The answer is larger than the source's manifest allows. Reading
    /// stopped there.
    #[error("the answer is larger than the {} kB the source's manifest allows", limit.div_ceil(1024))]
    TooLarge {
        /// The most bytes the manifest allows.
        limit: usize,
    },
    /// The request was not made, or its answer not followed, because it
    /// would have left what the fetcher may do: an address that is not
    /// HTTPS, a redirect to another host.
    #[error("not fetched: {0}")]
    Refused(String),
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

    /// Performs the request for a source whose manifest allows an answer
    /// of at most `max_bytes`, and fails with [`FetchError::TooLarge`] for
    /// a larger one. This is what the pipeline calls.
    ///
    /// A fetcher that reads from a network implements this and stops
    /// reading at the bound. One that answers from memory need not: here
    /// the bound is applied to what [`get`](Self::get) returned, so a test
    /// with recorded payloads meets the same refusal.
    fn get_within(
        &self,
        request: &Request,
        max_bytes: usize,
    ) -> impl Future<Output = Result<Fetched, FetchError>> + Send {
        async move {
            let fetched = self.get(request).await?;
            if fetched.body.len() > max_bytes {
                return Err(FetchError::TooLarge { limit: max_bytes });
            }
            Ok(fetched)
        }
    }
}
