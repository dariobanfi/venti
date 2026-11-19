//! The one place from which the server asks anyone else for anything.
//!
//! The adapters say which addresses to fetch, and some of those they read
//! out of what a provider sent: a list of files, a link to the next page.
//! A provider's answer can also come from someone else, through a
//! redirect. So the fetcher trusts neither and keeps to bounds of its own:
//!
//! - **HTTPS and nothing else**, except to the addresses whoever runs the
//!   server wrote into its configuration with `http://`: a forecast service
//!   or an Overpass instance of their own, beside the server on a private
//!   network. See [`HttpFetch::plain_http_to`].
//! - **No redirect to another host.** A redirect is followed a few times
//!   if it stays with the same scheme, host and port. One that leads
//!   elsewhere is a failed request; so is one from HTTPS to plain HTTP.
//!   Without this a provider, or whoever answers for it, could point the
//!   server at an address inside its own network.
//! - **A bound on the size of an answer**, from the source's manifest. The
//!   body is read piece by piece and reading stops at the bound; it is
//!   never taken into memory whole first.
//! - **A bound on time**: ten seconds to connect, two minutes for a whole
//!   request.
//!
//! Nothing a client sends is ever an address the server fetches. The one
//! request that makes it fetch at all is the one for a forecast, and what
//! it fetches then is the forecast service the configuration names, for
//! two numbers.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::header::LAST_MODIFIED;
use reqwest::redirect::Policy;
use reqwest::{Response, Url};
use venti_domain::Timestamp;
use venti_sources::{DEFAULT_MAX_RESPONSE_KB, Fetch, FetchError, Fetched, Request};

/// How long a connection may take to come about.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a whole request may take, the body included. An Overpass
/// instance may work on a region's query for a minute and more before it
/// answers.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// How many redirects within one host are followed.
const MAX_REDIRECTS: usize = 3;

/// Fetches over HTTPS.
pub struct HttpFetch {
    client: reqwest::Client,
    /// The origins that may be asked without encryption.
    plain_http: Vec<Origin>,
}

/// A scheme, a host and a port: what a browser calls an origin.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

impl Origin {
    fn of(url: &Url) -> Option<Self> {
        Some(Self {
            scheme: url.scheme().to_owned(),
            host: url.host_str()?.to_ascii_lowercase(),
            port: url.port_or_known_default(),
        })
    }
}

impl HttpFetch {
    /// Creates the client.
    ///
    /// `user_agent` is sent with every request. Some providers ask clients
    /// to name a contact there; what to put in it is the operator's
    /// decision, so the default names only the program and its version.
    pub fn new(user_agent: &str) -> Result<Self, String> {
        let redirects = Policy::custom(|attempt| {
            // Where the request set out for, and where it is sent now.
            let same_place = attempt
                .previous()
                .first()
                .and_then(Origin::of)
                .zip(Origin::of(attempt.url()))
                .is_some_and(|(first, next)| first == next);
            if !same_place {
                // Hand the redirect back as the answer; `get_within` turns
                // it into a refusal that names where it led.
                attempt.stop()
            } else if attempt.previous().len() > MAX_REDIRECTS {
                attempt.error("more redirects than are followed")
            } else {
                attempt.follow()
            }
        });
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(redirects)
            .build()
            .map_err(|e| format!("cannot create the HTTP client: {e}"))?;
        Ok(Self {
            client,
            plain_http: Vec::new(),
        })
    }

    /// Lets the fetcher ask the given addresses without encryption, if
    /// they begin with `http://`.
    ///
    /// These are addresses from the server's configuration and from
    /// nowhere else: a service whoever runs the server keeps beside it,
    /// reached over a network of their own. An address that begins with
    /// `https://` needs no mention and changes nothing. One that is no
    /// address is an error, with the reason.
    pub fn plain_http_to<'a>(
        mut self,
        addresses: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, String> {
        for address in addresses {
            let url = Url::parse(address).map_err(|e| format!("{address:?}: {e}"))?;
            let origin = Origin::of(&url).ok_or_else(|| format!("{address:?} names no host"))?;
            if origin.scheme == "http" && !self.plain_http.contains(&origin) {
                self.plain_http.push(origin);
            }
        }
        Ok(self)
    }

    /// The address of a request, if the fetcher may ask it.
    fn allowed(&self, address: &str) -> Result<Url, FetchError> {
        let refused = |why: &str| FetchError::Refused(format!("{address}: {why}"));
        let url = Url::parse(address).map_err(|_| refused("this is not an address"))?;
        let origin = Origin::of(&url).ok_or_else(|| refused("it names no host"))?;
        match origin.scheme.as_str() {
            "https" => Ok(url),
            "http" if self.plain_http.contains(&origin) => Ok(url),
            "http" => Err(refused(
                "only HTTPS is fetched, and plain HTTP from the addresses the server's \
                 configuration names",
            )),
            _ => Err(refused("only HTTPS is fetched")),
        }
    }
}

impl Fetch for HttpFetch {
    async fn get(&self, request: &Request) -> Result<Fetched, FetchError> {
        let max_bytes = usize::try_from(u64::from(DEFAULT_MAX_RESPONSE_KB) * 1024);
        self.get_within(request, max_bytes.unwrap_or(usize::MAX))
            .await
    }

    async fn get_within(&self, request: &Request, max_bytes: usize) -> Result<Fetched, FetchError> {
        let transport = |e: reqwest::Error| FetchError::Transport(e.to_string());
        let url = self.allowed(&request.url)?;
        let mut response = self.client.get(url).send().await.map_err(transport)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(redirected_elsewhere(&response));
        }
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        let last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .and_then(http_date);

        // What the answer says about its own length saves reading any of
        // it; what it does not say, or says wrongly, is found out below.
        let too_large = FetchError::TooLarge { limit: max_bytes };
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(too_large);
        }
        let mut body = Vec::new();
        while let Some(piece) = response.chunk().await.map_err(transport)? {
            if body.len() + piece.len() > max_bytes {
                // Dropping the response closes the connection: the rest
                // is never read.
                return Err(too_large);
            }
            body.extend_from_slice(&piece);
        }
        Ok(Fetched {
            body,
            last_modified,
        })
    }
}

/// The refusal for a redirect that was not followed, with where it led.
fn redirected_elsewhere(response: &Response) -> FetchError {
    let to = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("an address it does not name");
    // Only the place: a query can hold what a provider meant for us alone.
    let to = to.split(['?', '#']).next().unwrap_or(to);
    FetchError::Refused(format!(
        "{} answers with a redirect to {to}; a redirect is followed only within the same \
         scheme, host and port",
        response
            .url()
            .as_str()
            .split('?')
            .next()
            .unwrap_or_default()
    ))
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
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::TimeZone;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

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

    /// A provider on this machine, speaking plain HTTP: it answers each
    /// path as [`answer`] says, and counts the bytes of body it managed to
    /// send before the fetcher went away.
    struct Provider {
        address: SocketAddr,
        sent: Arc<AtomicUsize>,
    }

    impl Provider {
        /// Starts one. `elsewhere` is where its redirect to another host
        /// leads.
        async fn start(elsewhere: Option<SocketAddr>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let sent = Arc::new(AtomicUsize::new(0));
            let counter = sent.clone();
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(answer(stream, elsewhere, counter.clone()));
                }
            });
            Self { address, sent }
        }

        fn url(&self, path: &str) -> String {
            format!("http://{}{path}", self.address)
        }

        fn origin(&self) -> String {
            self.url("")
        }
    }

    async fn answer(mut stream: TcpStream, elsewhere: Option<SocketAddr>, sent: Arc<AtomicUsize>) {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.unwrap_or(0) == 0 {
                return;
            }
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
        let block = [b'x'; 16 * 1024];
        match path.as_str() {
            "/file" => {
                let text = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\
                            Last-Modified: Thu, 19 Nov 2026 01:57:33 GMT\r\n\r\nhello";
                let _ = stream.write_all(text.as_bytes()).await;
            }
            // Says how large it is: sixteen megabytes.
            "/says-it-is-large" => {
                let text =
                    "HTTP/1.1 200 OK\r\nContent-Length: 16777216\r\nConnection: close\r\n\r\n";
                if stream.write_all(text.as_bytes()).await.is_err() {
                    return;
                }
                for _ in 0..1024 {
                    if stream.write_all(&block).await.is_err() {
                        return;
                    }
                    sent.fetch_add(block.len(), Ordering::SeqCst);
                }
            }
            // Says nothing about its size and never ends.
            "/endless" => {
                let text = "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n";
                if stream.write_all(text.as_bytes()).await.is_err() {
                    return;
                }
                loop {
                    if stream.write_all(&block).await.is_err() {
                        return;
                    }
                    sent.fetch_add(block.len(), Ordering::SeqCst);
                }
            }
            "/moved-within" => {
                let text = "HTTP/1.1 302 Found\r\nLocation: /file\r\nContent-Length: 0\r\n\
                            Connection: close\r\n\r\n";
                let _ = stream.write_all(text.as_bytes()).await;
            }
            "/moved-in-circles" => {
                let text = "HTTP/1.1 302 Found\r\nLocation: /moved-in-circles\r\n\
                            Content-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(text.as_bytes()).await;
            }
            "/moved-elsewhere" => {
                let to = elsewhere.expect("the test names another host");
                let text = format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{to}/file?key=secret\r\n\
                     Content-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(text.as_bytes()).await;
            }
            _ => {
                let text =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(text.as_bytes()).await;
            }
        }
        let _ = stream.shutdown().await;
    }

    fn request(url: String) -> Request {
        Request::new("test", url)
    }

    /// A fetcher that may ask the given providers without encryption, as
    /// if whoever runs the server had named them in its configuration.
    fn fetcher(trusted: &[&Provider]) -> HttpFetch {
        let origins: Vec<String> = trusted.iter().map(|provider| provider.origin()).collect();
        HttpFetch::new("venti-server/test")
            .unwrap()
            .plain_http_to(origins.iter().map(String::as_str))
            .unwrap()
    }

    #[tokio::test]
    async fn a_file_is_fetched_with_the_date_its_provider_gives_it() {
        let provider = Provider::start(None).await;
        let fetch = fetcher(&[&provider]);

        let fetched = fetch.get(&request(provider.url("/file"))).await.unwrap();

        assert_eq!(fetched.body, b"hello");
        assert_eq!(
            fetched.last_modified,
            Some(Utc.with_ymd_and_hms(2026, 11, 19, 1, 57, 33).unwrap())
        );
        // Exactly as large as allowed is allowed.
        let exact = fetch.get_within(&request(provider.url("/file")), 5).await;
        assert_eq!(exact.unwrap().body, b"hello");
        let missing = fetch.get(&request(provider.url("/nothing"))).await;
        assert_eq!(missing, Err(FetchError::Status(404)));
    }

    #[tokio::test]
    async fn an_answer_larger_than_the_manifest_allows_is_not_read() {
        let provider = Provider::start(None).await;
        let fetch = fetcher(&[&provider]);
        let limit = 64 * 1024;
        let too_large = Err(FetchError::TooLarge { limit });

        // One that says so itself is refused on its word.
        let said = fetch
            .get_within(&request(provider.url("/says-it-is-large")), limit)
            .await;
        assert_eq!(said, too_large);
        // One that never ends is read as far as the bound and no further:
        // the provider gets rid of what the connection's buffers hold, and
        // then finds nobody listening.
        let endless = fetch
            .get_within(&request(provider.url("/endless")), limit)
            .await;
        assert_eq!(endless, too_large);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let sent = provider.sent.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            provider.sent.load(Ordering::SeqCst),
            sent,
            "it is still sending"
        );
        assert!(sent < 16 * 1024 * 1024, "{sent} bytes were taken");
        // One byte too many is too many.
        let short = fetch.get_within(&request(provider.url("/file")), 4).await;
        assert_eq!(short, Err(FetchError::TooLarge { limit: 4 }));
    }

    #[tokio::test]
    async fn a_redirect_is_followed_within_one_host_and_nowhere_else() {
        let other = Provider::start(None).await;
        let provider = Provider::start(Some(other.address)).await;
        // Both may be asked without encryption: what stops the redirect
        // is that it leaves the host, not where it leads.
        let fetch = fetcher(&[&provider, &other]);

        let within = fetch.get(&request(provider.url("/moved-within"))).await;
        assert_eq!(within.unwrap().body, b"hello");

        let elsewhere = fetch.get(&request(provider.url("/moved-elsewhere"))).await;
        let Err(FetchError::Refused(why)) = elsewhere else {
            panic!("followed to another host: {elsewhere:?}");
        };
        assert!(why.contains("redirect"), "{why}");
        assert!(why.contains(&other.address.to_string()), "{why}");
        // What a provider puts into the address it sends us to stays out
        // of the log.
        assert!(!why.contains("secret"), "{why}");

        let circles = fetch.get(&request(provider.url("/moved-in-circles"))).await;
        assert!(
            matches!(circles, Err(FetchError::Transport(_))),
            "{circles:?}"
        );
    }

    #[tokio::test]
    async fn only_https_is_fetched_unless_the_configuration_names_the_address() {
        let provider = Provider::start(None).await;
        let strict = HttpFetch::new("venti-server/test").unwrap();

        // Not asked at all: the provider is there and would answer.
        let plain = strict.get(&request(provider.url("/file"))).await;
        let Err(FetchError::Refused(why)) = plain else {
            panic!("fetched without encryption: {plain:?}");
        };
        assert!(why.contains("only HTTPS"), "{why}");
        for address in [
            "ftp://opendata.example/file",
            "file:///etc/passwd",
            "opendata.example/file",
            "",
        ] {
            let refused = strict.get(&request(address.to_owned())).await;
            assert!(
                matches!(refused, Err(FetchError::Refused(_))),
                "{address}: {refused:?}"
            );
        }

        // Named in the configuration with http://, it is asked.
        let trusting = fetcher(&[&provider]);
        assert!(trusting.get(&request(provider.url("/file"))).await.is_ok());
        // Another port of the same machine was not named.
        let other = Provider::start(None).await;
        let refused = trusting.get(&request(other.url("/file"))).await;
        assert!(
            matches!(refused, Err(FetchError::Refused(_))),
            "{refused:?}"
        );
        // An address named with https:// allows nothing in plain.
        let https_named = HttpFetch::new("venti-server/test")
            .unwrap()
            .plain_http_to([format!("https://{}", provider.address).as_str()])
            .unwrap();
        let refused = https_named.get(&request(provider.url("/file"))).await;
        assert!(
            matches!(refused, Err(FetchError::Refused(_))),
            "{refused:?}"
        );
        // What is no address is said at start, not found out in a run.
        assert!(
            HttpFetch::new("venti-server/test")
                .unwrap()
                .plain_http_to(["open-meteo:8080/v1"])
                .is_err()
        );
    }
}
