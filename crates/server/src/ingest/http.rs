use std::time::Duration;

use venti_sources::{Fetch, FetchError, Request};

/// Fetches over HTTPS.
pub struct HttpFetch {
    client: reqwest::Client,
}

impl HttpFetch {
    /// Creates the client.
    ///
    /// `user_agent` is sent with every request. Some providers ask clients
    /// to name a contact there; what to put in it is the operator's
    /// decision, so the default names only the program and its version.
    pub fn new(user_agent: &str) -> Result<Self, String> {
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
