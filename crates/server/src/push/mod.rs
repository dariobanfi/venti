//! Web Push: telling a pilot's browsers when an alarm fires, even while
//! no Venti page is open.
//!
//! A browser subscribes on the account (`POST /v1/account/push`) and is
//! given an endpoint at its vendor's push service. When an alarm fires,
//! [`crate::alerts`] hands what fired to [`Pusher::fired`], which puts it
//! on a bounded queue and returns: the check never waits for a push
//! service. A task of its own reads the queue, and for each subscription
//! of the account encrypts the message for that browser (RFC 8291), signs
//! the request with the server's key (VAPID, RFC 8292) and sends it
//! (RFC 8030) with a time to live of [`PUSH_TTL_SECONDS`] and
//! `Urgency: normal`.
//!
//! - **Where it sends.** An endpoint comes from a browser, so it is an
//!   address a client gave: the server sends only to HTTPS addresses of
//!   the push services in [`PUSH_SERVICES`], follows no redirect, and
//!   reads no more of an answer than its status. A browser whose push
//!   service is not listed is refused when it subscribes.
//! - **What it keeps.** A `404` or `410` from the push service means the
//!   subscription is gone: it is removed. A `429`, a `5xx` or a failed
//!   connection is tried again after [`PushSettings::retries`]; any other
//!   answer is a line in the log. Nothing else is kept: what fired is on
//!   the account anyway, for the apps to fetch.
//! - **The key.** The server's key pair is made when it first starts
//!   with a database, and kept in it ([`key_of`]). It is never logged.

pub mod crypto;

use std::sync::Arc;
use std::time::Duration;

use reqwest::{StatusCode, Url};
use tokio::sync::{Semaphore, mpsc};
use venti_api_types::v1::{FiredAlert, MAX_PUSH_ENDPOINT_LENGTH, PUSH_TTL_SECONDS, PushMessage};
use venti_store::{PushSubscription, Store};

use crate::clock::Clock;
pub use crypto::ServerKey;

/// The push services the server sends to, by host: a host is one of them
/// or ends in `.` and one of them. Chrome, Edge's Chromium and most
/// others use Google's; Firefox Mozilla's; Safari Apple's; older Edge
/// Microsoft's. *To verify* when a browser that uses another appears: it
/// is refused with `invalid_push_subscription` until its host is here.
pub const PUSH_SERVICES: &[&str] = &[
    "fcm.googleapis.com",
    "updates.push.services.mozilla.com",
    "push.apple.com",
    "notify.windows.com",
];

/// How many messages wait for the sender at most. Beyond it a message is
/// left out, with a line in the log.
const QUEUE: usize = 1024;

/// How many requests to push services are under way at once.
const AT_ONCE: usize = 8;

/// How long a connection to a push service may take to come about, and
/// a whole request.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether an endpoint is one the server sends to: HTTPS, on the usual
/// port, to a host of [`PUSH_SERVICES`], no longer than
/// [`MAX_PUSH_ENDPOINT_LENGTH`]. The reason if not.
pub fn push_service(endpoint: &str) -> Result<Url, &'static str> {
    if endpoint.len() > MAX_PUSH_ENDPOINT_LENGTH {
        return Err("is longer than the server keeps");
    }
    let url = Url::parse(endpoint).map_err(|_| "is not an address")?;
    if url.scheme() != "https" || url.port().is_some_and(|port| port != 443) {
        return Err("is not an HTTPS address");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("carries a user name or password");
    }
    // An address by number is never one of the push services' names.
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return Err("names no push service");
    };
    let known = PUSH_SERVICES.iter().any(|service| {
        host == *service
            || host
                .strip_suffix(service)
                .is_some_and(|rest| rest.ends_with('.'))
    });
    if known {
        Ok(url)
    } else {
        Err("is not of a push service this server sends to")
    }
}

/// The origin of an address, as the `aud` of a VAPID token.
fn origin(url: &Url) -> String {
    url.origin().ascii_serialization()
}

/// How the sender works.
#[derive(Debug, Clone)]
pub struct PushSettings {
    /// The User-Agent of its requests.
    pub user_agent: String,
    /// Whom a push service can reach about the server: the `sub` of the
    /// VAPID token, an `https:` or `mailto:` address. Some push services
    /// refuse a token without one *(to verify: Apple's)*.
    pub contact: Option<String>,
    /// How long to wait before each further try of a message the push
    /// service could not take.
    pub retries: Vec<Duration>,
    /// Origins that may be sent to without HTTPS and without being a
    /// push service: for tests, with a push service of their own.
    pub plain_http_to: Vec<String>,
}

impl PushSettings {
    /// The settings of a server: two more tries, after half a minute and
    /// after five.
    pub fn new(user_agent: &str, contact: Option<String>) -> Self {
        Self {
            user_agent: user_agent.to_owned(),
            contact,
            retries: vec![Duration::from_secs(30), Duration::from_secs(300)],
            plain_http_to: Vec::new(),
        }
    }
}

/// A message on its way to an account's browsers.
#[derive(Debug, Clone)]
struct Job {
    account_id: String,
    message: PushMessage,
}

/// The way in to the sender: cheap to clone, never waits.
#[derive(Debug, Clone)]
pub struct Pusher {
    queue: mpsc::Sender<Job>,
}

/// What a push service made of a request.
enum Sent {
    Taken,
    /// The subscription is no more.
    Gone,
    /// Try again later.
    Later(String),
    /// Not again.
    Refused(String),
}

struct Sender<S> {
    store: Arc<S>,
    key: ServerKey,
    clock: Arc<dyn Clock>,
    settings: PushSettings,
    client: reqwest::Client,
}

impl Pusher {
    /// Starts the sender, on the runtime this is called on. It ends when
    /// the last [`Pusher`] is dropped.
    pub fn start<S: Store + 'static>(
        store: Arc<S>,
        key: ServerKey,
        clock: Arc<dyn Clock>,
        settings: PushSettings,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .user_agent(&settings.user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("cannot create the HTTP client for Web Push: {e}"))?;
        let (queue, mut jobs) = mpsc::channel(QUEUE);
        let sender = Arc::new(Sender {
            store,
            key,
            clock,
            settings,
            client,
        });
        tokio::spawn(async move {
            let at_once = Arc::new(Semaphore::new(AT_ONCE));
            while let Some(job) = jobs.recv().await {
                sender.clone().job(job, &at_once).await;
            }
        });
        Ok(Self { queue })
    }

    /// Tells the account's browsers what fired, without waiting.
    pub fn fired(&self, account_id: &str, fired: &FiredAlert) {
        let job = Job {
            account_id: account_id.to_owned(),
            message: PushMessage::of(fired),
        };
        if self.queue.try_send(job).is_err() {
            tracing::warn!("web push: the queue is full; a message was left out");
        }
    }
}

impl<S: Store + 'static> Sender<S> {
    async fn job(self: Arc<Self>, job: Job, at_once: &Arc<Semaphore>) {
        let subscriptions = match self.store.push_subscriptions(&job.account_id).await {
            Ok(subscriptions) => subscriptions,
            Err(error) => {
                tracing::warn!("web push: the subscriptions could not be read: {error}");
                return;
            }
        };
        let Ok(body) = serde_json::to_vec(&job.message) else {
            return;
        };
        for subscription in subscriptions {
            // Waits while as many are under way as may be: the queue
            // holds the rest.
            let Ok(turn) = at_once.clone().acquire_owned().await else {
                return;
            };
            let sender = self.clone();
            let body = body.clone();
            tokio::spawn(async move {
                sender.deliver(&subscription, &body).await;
                drop(turn);
            });
        }
    }

    async fn deliver(&self, subscription: &PushSubscription, message: &[u8]) {
        let host = Url::parse(&subscription.endpoint)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        let mut waits = self.settings.retries.iter();
        loop {
            match self.send(subscription, message).await {
                Sent::Taken => return,
                Sent::Gone => {
                    if let Err(error) = self
                        .store
                        .drop_push_subscription(&subscription.endpoint)
                        .await
                    {
                        tracing::warn!("web push: a gone subscription stays: {error}");
                    }
                    tracing::info!("web push: {host} says a subscription is gone; removed");
                    return;
                }
                Sent::Refused(why) => {
                    tracing::warn!("web push: {host} refused a message: {why}");
                    return;
                }
                Sent::Later(why) => match waits.next() {
                    Some(wait) => tokio::time::sleep(*wait).await,
                    None => {
                        tracing::warn!("web push: {host} did not take a message: {why}");
                        return;
                    }
                },
            }
        }
    }

    fn allowed(&self, endpoint: &str) -> Result<Url, &'static str> {
        let url = Url::parse(endpoint).map_err(|_| "is not an address")?;
        if url.scheme() == "http"
            && self
                .settings
                .plain_http_to
                .iter()
                .any(|allowed| *allowed == origin(&url))
        {
            return Ok(url);
        }
        push_service(endpoint)
    }

    async fn send(&self, subscription: &PushSubscription, message: &[u8]) -> Sent {
        let url = match self.allowed(&subscription.endpoint) {
            Ok(url) => url,
            Err(why) => return Sent::Refused(format!("the endpoint {why}")),
        };
        let keys = crypto::from_base64url(&subscription.p256dh)
            .zip(crypto::from_base64url(&subscription.auth));
        let Some((p256dh, auth)) = keys else {
            return Sent::Refused("the browser's keys are not base64url".to_owned());
        };
        let body = match crypto::encrypt(&p256dh, &auth, message) {
            Ok(body) => body,
            Err(error) => return Sent::Refused(error.to_string()),
        };
        let authorization = self.key.authorization(
            &origin(&url),
            self.settings.contact.as_deref(),
            self.clock.now(),
        );
        let answer = self
            .client
            .post(url)
            .header("TTL", PUSH_TTL_SECONDS.to_string())
            .header("Urgency", "normal")
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", authorization)
            .body(body)
            .send()
            .await;
        match answer {
            Err(error) => Sent::Later(error.without_url().to_string()),
            Ok(answer) => match answer.status() {
                status if status.is_success() => Sent::Taken,
                StatusCode::NOT_FOUND | StatusCode::GONE => Sent::Gone,
                status if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
                    Sent::Later(status.to_string())
                }
                status => Sent::Refused(status.to_string()),
            },
        }
    }
}

/// The server's key pair for Web Push: the one the store keeps, or a new
/// one, kept, if it has none. Of two servers that start at once with one
/// database, both end with the same.
pub async fn key_of<S: Store>(store: &S, clock: &dyn Clock) -> Result<ServerKey, String> {
    let text = match store.push_key().await.map_err(|e| e.to_string())? {
        Some(text) => text,
        None => {
            let new = ServerKey::generate().map_err(|e| e.to_string())?;
            let kept = store
                .keep_push_key(&new.to_text(), clock.now())
                .await
                .map_err(|e| e.to_string())?;
            tracing::info!("web push: made the server's key pair and kept it in the database");
            kept
        }
    };
    ServerKey::from_text(&text).map_err(|_| "the database's key for Web Push is not one".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_addresses_of_the_push_services_are_taken() {
        for good in [
            "https://fcm.googleapis.com/fcm/send/abc:def",
            "https://updates.push.services.mozilla.com/wpush/v2/gAAA",
            "https://web.push.apple.com/QGuQ",
            "https://wns2-par02p.notify.windows.com/w/?token=x",
        ] {
            assert!(push_service(good).is_ok(), "{good}");
        }
        for bad in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://fcm.googleapis.com:8443/fcm/send/abc",
            "https://evilfcm.googleapis.com.example/x",
            "https://notpush.apple.com/x",
            "https://127.0.0.1/x",
            "https://user:pw@fcm.googleapis.com/x",
            "https://example.com/push",
            "not an address",
        ] {
            assert!(push_service(bad).is_err(), "{bad}");
        }
        let long = format!("https://fcm.googleapis.com/{}", "a".repeat(1024));
        assert!(push_service(&long).is_err());
    }
}
