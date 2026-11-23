//! Push to the phone apps: Apple's APNs for iPhones ([`super::apns`]) and
//! Google's FCM for Android ([`super::fcm`]).
//!
//! What concerns some pilots is handed to [`Phones::tell`] with their
//! accounts, which puts it on a bounded queue and returns: nothing waits
//! for a push service. A task of its own reads the queue, finds the phones
//! of those accounts, and sends to each through its platform's service.
//!
//! - **Configured or not.** Whoever runs the server gives the APNs key and
//!   the Firebase service account as files ([`crate::config::PushConfig`]).
//!   A platform without them is not sent to, and the health answer says
//!   so; the rest of the server works as before. Neither file is logged.
//! - **What it keeps.** A push service that says a token is no more
//!   (APNs `410` or `BadDeviceToken`, FCM `404`) has the phone removed. A
//!   `429`, a `5xx` or a failed connection is tried again after
//!   [`PhoneSettings::retries`]; any other answer is a line in the log
//!   without the token.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc};
use venti_api_types::v1::PhoneMessage;
use venti_store::{PhoneDevice, Store};

use super::Sent;
use super::apns::{self, Apns, ApnsKey};
use super::fcm::{self, Fcm, ServiceAccount};
use crate::clock::Clock;
use crate::config::PushConfig;

/// How many messages wait for the sender at most.
const QUEUE: usize = 1024;

/// How many requests to push services are under way at once.
const AT_ONCE: usize = 8;

/// How many accounts' phones are read at once.
const ACCOUNTS_AT_ONCE: usize = 500;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How the sender works, and where Apple's and Google's services are.
#[derive(Debug, Clone)]
pub struct PhoneSettings {
    /// The User-Agent of its requests.
    pub user_agent: String,
    /// How long to wait before each further try of a message a push
    /// service could not take.
    pub retries: Vec<Duration>,
    /// APNs, for apps from the App Store and TestFlight.
    pub apns_production: String,
    /// APNs, for apps built for development.
    pub apns_sandbox: String,
    /// FCM.
    pub fcm: String,
    /// Whether addresses without HTTPS may be spoken to: for tests, with
    /// push services of their own.
    pub plain_http: bool,
}

impl PhoneSettings {
    /// The settings of a server: Apple's and Google's hosts, and two more
    /// tries, after half a minute and after five.
    pub fn new(user_agent: &str) -> Self {
        Self {
            user_agent: user_agent.to_owned(),
            retries: vec![Duration::from_secs(30), Duration::from_secs(300)],
            apns_production: apns::PRODUCTION.to_owned(),
            apns_sandbox: apns::SANDBOX.to_owned(),
            fcm: fcm::FCM.to_owned(),
            plain_http: false,
        }
    }
}

/// What signs the requests to each platform's service, where given.
#[derive(Debug, Default)]
pub struct PhoneKeys {
    /// The APNs key.
    pub apns: Option<ApnsKey>,
    /// The Firebase project's service account.
    pub fcm: Option<ServiceAccount>,
}

/// Reads the files the configuration names. A file named but unreadable,
/// or not what it should be, is an error: the server does not start
/// believing it sends what it cannot. The error names the setting and the
/// path, never what the file holds.
pub fn keys_from(config: &PushConfig) -> Result<PhoneKeys, String> {
    let read = |path: &Path, setting: &str| {
        std::fs::read_to_string(path)
            .map_err(|error| format!("{setting}: cannot read {}: {error}", path.display()))
    };
    let apns = match &config.apns {
        None => None,
        Some(apns) => {
            let text = read(&apns.key_file, "apns_key_file")?;
            Some(
                ApnsKey::from_pem(&text, &apns.key_id, &apns.team_id, &apns.topic).map_err(
                    |problem| format!("apns_key_file: {} {problem}", apns.key_file.display()),
                )?,
            )
        }
    };
    let fcm = match &config.fcm_service_account_file {
        None => None,
        Some(path) => {
            let text = read(path, "fcm_service_account_file")?;
            Some(ServiceAccount::from_json(&text).map_err(|problem| {
                format!("fcm_service_account_file: {} {problem}", path.display())
            })?)
        }
    };
    Ok(PhoneKeys { apns, fcm })
}

/// A message on its way to some accounts' phones.
#[derive(Debug, Clone)]
struct Job {
    accounts: Vec<String>,
    message: PhoneMessage,
}

/// The way in to the sender: cheap to clone, never waits.
#[derive(Debug, Clone, Default)]
pub struct Phones {
    queue: Option<mpsc::Sender<Job>>,
    apple: bool,
    google: bool,
}

struct Sender<S> {
    store: Arc<S>,
    clock: Arc<dyn Clock>,
    settings: PhoneSettings,
    client: reqwest::Client,
    apns: Option<Apns>,
    fcm: Option<Fcm>,
}

impl Phones {
    /// A server that sends to no phone.
    pub fn off() -> Self {
        Self::default()
    }

    /// Starts the sender for the platforms `keys` has, on the runtime this
    /// is called on. With neither, nothing is started and nothing is sent.
    pub fn start<S: Store + 'static>(
        store: Arc<S>,
        clock: Arc<dyn Clock>,
        keys: PhoneKeys,
        settings: PhoneSettings,
    ) -> Result<Self, String> {
        if keys.apns.is_none() && keys.fcm.is_none() {
            return Ok(Self::off());
        }
        if let Some(account) = &keys.fcm
            && !settings.plain_http
            && !account.token_uri().starts_with("https://")
        {
            return Err(
                "fcm_service_account_file: its token_uri is not an HTTPS address".to_owned(),
            );
        }
        let client = reqwest::Client::builder()
            .user_agent(&settings.user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(!settings.plain_http)
            .build()
            .map_err(|e| format!("cannot create the HTTP client for push to phones: {e}"))?;
        let apple = keys.apns.is_some();
        let google = keys.fcm.is_some();
        let sender = Arc::new(Sender {
            apns: keys
                .apns
                .map(|key| Apns::new(key, &settings.apns_production, &settings.apns_sandbox)),
            fcm: keys.fcm.map(|account| Fcm::new(account, &settings.fcm)),
            store,
            clock,
            settings,
            client,
        });
        let (queue, mut jobs) = mpsc::channel(QUEUE);
        tokio::spawn(async move {
            let at_once = Arc::new(Semaphore::new(AT_ONCE));
            while let Some(job) = jobs.recv().await {
                sender.clone().job(job, &at_once).await;
            }
        });
        Ok(Self {
            queue: Some(queue),
            apple,
            google,
        })
    }

    /// Whether iPhones are sent to.
    pub fn apple(&self) -> bool {
        self.apple
    }

    /// Whether Android phones are sent to.
    pub fn google(&self) -> bool {
        self.google
    }

    /// Tells the phones of `accounts` the `message`, without waiting.
    pub fn tell(&self, accounts: Vec<String>, message: PhoneMessage) {
        let Some(queue) = &self.queue else {
            return;
        };
        if accounts.is_empty() {
            return;
        }
        if queue.try_send(Job { accounts, message }).is_err() {
            tracing::warn!("phone push: the queue is full; a message was left out");
        }
    }
}

impl<S: Store + 'static> Sender<S> {
    async fn job(self: Arc<Self>, job: Job, at_once: &Arc<Semaphore>) {
        for accounts in job.accounts.chunks(ACCOUNTS_AT_ONCE) {
            let devices = match self.store.devices_of(accounts).await {
                Ok(devices) => devices,
                Err(error) => {
                    tracing::warn!("phone push: the phones could not be read: {error}");
                    return;
                }
            };
            for device in devices {
                let Ok(turn) = at_once.clone().acquire_owned().await else {
                    return;
                };
                let sender = self.clone();
                let message = job.message.clone();
                tokio::spawn(async move {
                    sender.deliver(&device, &message).await;
                    drop(turn);
                });
            }
        }
    }

    async fn send(&self, device: &PhoneDevice, message: &PhoneMessage) -> Option<Sent> {
        let now = self.clock.now();
        match device.platform.as_str() {
            "apns" => Some(
                self.apns
                    .as_ref()?
                    .send(&self.client, &device.token, device.sandbox, message, now)
                    .await,
            ),
            "fcm" => Some(
                self.fcm
                    .as_ref()?
                    .send(&self.client, &device.token, message, now)
                    .await,
            ),
            _ => None,
        }
    }

    async fn deliver(&self, device: &PhoneDevice, message: &PhoneMessage) {
        let platform = device.platform.as_str();
        let mut waits = self.settings.retries.iter();
        loop {
            // A platform this server has no key for: its phones wait for
            // one, and are told nothing meanwhile.
            let Some(sent) = self.send(device, message).await else {
                return;
            };
            match sent {
                Sent::Taken => return,
                Sent::Gone => {
                    if let Err(error) = self.store.drop_device(platform, &device.token).await {
                        tracing::warn!("phone push: a phone that is gone stays: {error}");
                    }
                    tracing::info!("phone push: {platform} says a phone is gone; removed");
                    return;
                }
                Sent::Refused(why) => {
                    tracing::warn!("phone push: {platform} refused a message: {why}");
                    return;
                }
                Sent::Later(why) => match waits.next() {
                    Some(wait) => tokio::time::sleep(*wait).await,
                    None => {
                        tracing::warn!("phone push: {platform} did not take a message: {why}");
                        return;
                    }
                },
            }
        }
    }
}
