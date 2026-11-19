use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::header::LAST_MODIFIED;
use venti_domain::Timestamp;
use venti_sources::{Fetch, FetchError, Fetched, Request};

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
    async fn get(&self, request: &Request) -> Result<Fetched, FetchError> {
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
        let last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .and_then(http_date);
        Ok(Fetched {
            body: response.bytes().await.map_err(transport)?.to_vec(),
            last_modified,
        })
    }
}

/// Reads the date of a `Last-Modified` header, such as
/// `Thu, 19 Nov 2026 01:57:33 GMT`. `None` for anything else: a date that
/// cannot be read is no date.
fn http_date(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc2822(text.trim())
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn the_date_a_provider_puts_on_a_file_is_read() {
        // As openAIP's file store sent it on 2026-11-19.
        assert_eq!(
            http_date("Thu, 19 Nov 2026 01:57:33 GMT"),
            Some(Utc.with_ymd_and_hms(2026, 11, 19, 1, 57, 33).unwrap())
        );
        assert_eq!(http_date("yesterday"), None);
        assert_eq!(http_date(""), None);
    }
}
