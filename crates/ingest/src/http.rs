use std::time::Duration;

use venti_sources::{Fetch, FetchError, Request};

/// Fetches over HTTPS.
pub(crate) struct HttpFetch {
    client: reqwest::Client,
}

impl HttpFetch {
    /// Creates the client.
    ///
    /// The `User-Agent` comes from the `VENTI_USER_AGENT` environment variable
    /// when it is set. Some providers ask clients to name a contact there;
    /// what to put in it is the operator's decision, so the default names only
    /// the program and its version.
    pub(crate) fn new() -> Result<Self, String> {
        let user_agent = std::env::var("VENTI_USER_AGENT")
            .unwrap_or_else(|_| concat!("venti-ingest/", env!("CARGO_PKG_VERSION")).to_owned());
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| format!("cannot create the HTTP client: {e}"))?;
        Ok(Self { client })
    }
}

impl Fetch for HttpFetch {
    async fn get(&self, request: &Request) -> Result<Vec<u8>, FetchError> {
        let transport = |e: reqwest::Error| FetchError::Transport(e.to_string());
        let response = self
            .client
            .get(&request.url)
            .send()
            .await
            .map_err(transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        Ok(response.bytes().await.map_err(transport)?.to_vec())
    }
}
