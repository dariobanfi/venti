//! Apple's push service, APNs, for the iPhone app.
//!
//! A request is an HTTP/2 `POST` to `/3/device/{token}` on Apple's host,
//! signed with a JWT (ES256) that the key whoever runs the server made on
//! Apple's developer site signs: the `.p8` file, its key ID and the team
//! ID. Apple takes one token for up to an hour and refuses one renewed
//! more often than every twenty minutes, so a token is made once and used
//! for [`TOKEN_MINUTES`]. Apple's documentation, "Sending notification
//! requests to APNs" and "Establishing a token-based connection to APNs"
//! (read 2026-11-23, *to verify against a real key*).
//!
//! reqwest speaks HTTP/2 to an HTTPS host that offers it (ALPN), which
//! Apple's does; a test's plain-HTTP fake is spoken to in HTTP/1.1.

use std::sync::Mutex;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::SecretKey;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use reqwest::StatusCode;
use serde_json::json;
use venti_api_types::v1::{PHONE_PUSH_TTL_SECONDS, PhoneMessage, PhoneMessageKind};
use venti_domain::Timestamp;

use super::Sent;

/// Apple's host for apps from the App Store and TestFlight.
pub const PRODUCTION: &str = "https://api.push.apple.com";

/// Apple's host for apps built for development.
pub const SANDBOX: &str = "https://api.sandbox.push.apple.com";

/// For how long one token is used, in minutes.
const TOKEN_MINUTES: i64 = 40;

/// The most of an answer's body that is read.
const MOST_READ: usize = 4096;

/// What signs the requests: the key, its ID, the team's, and the app's
/// bundle ID as the topic.
pub struct ApnsKey {
    key: SigningKey,
    key_id: String,
    team_id: String,
    topic: String,
}

impl std::fmt::Debug for ApnsKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApnsKey")
            .field("key_id", &self.key_id)
            .field("team_id", &self.team_id)
            .field("topic", &self.topic)
            .finish_non_exhaustive()
    }
}

impl ApnsKey {
    /// The key from the text of a `.p8` file. The reason if it is none;
    /// the reason never repeats the file.
    pub fn from_pem(pem: &str, key_id: &str, team_id: &str, topic: &str) -> Result<Self, String> {
        let secret = SecretKey::from_pkcs8_pem(pem.trim())
            .map_err(|_| "is not an APNs key: a PKCS #8 file of a P-256 key".to_owned())?;
        Ok(Self {
            key: SigningKey::from(&secret),
            key_id: key_id.to_owned(),
            team_id: team_id.to_owned(),
            topic: topic.to_owned(),
        })
    }

    /// The JWT of a provider token, made at `now`.
    pub fn token(&self, now: Timestamp) -> String {
        let b64 = |bytes: &[u8]| URL_SAFE_NO_PAD.encode(bytes);
        let header = b64(json!({"alg": "ES256", "kid": self.key_id})
            .to_string()
            .as_bytes());
        let claims = b64(json!({"iss": self.team_id, "iat": now.timestamp()})
            .to_string()
            .as_bytes());
        let signed = format!("{header}.{claims}");
        let signature: Signature = self.key.sign(signed.as_bytes());
        format!("{signed}.{}", b64(&signature.to_bytes()))
    }
}

/// The sender to APNs.
pub struct Apns {
    key: ApnsKey,
    production: String,
    sandbox: String,
    /// The token in use, and when it was made.
    token: Mutex<Option<(Timestamp, String)>>,
}

impl std::fmt::Debug for Apns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Apns")
            .field("key", &self.key)
            .field("production", &self.production)
            .finish_non_exhaustive()
    }
}

/// The payload of a message: Apple's `aps`, and what it is about beside
/// it.
pub fn payload(message: &PhoneMessage) -> serde_json::Value {
    json!({
        "aps": {
            "alert": {"title": message.title, "body": message.body},
            "sound": "default",
        },
        "kind": message.kind.as_str(),
        "id": message.id,
        "sent_at": message.sent_at,
    })
}

/// Whether a message is to be shown at once: a call for help and its
/// answer are; the rest may wait for the phone's next good moment.
pub fn urgent(kind: PhoneMessageKind) -> bool {
    matches!(
        kind,
        PhoneMessageKind::HelpRequest | PhoneMessageKind::HelpAnswer
    )
}

/// Reads at most [`MOST_READ`] bytes of an answer.
pub(super) async fn read_some(mut answer: reqwest::Response) -> Vec<u8> {
    let mut read = Vec::new();
    while let Ok(Some(chunk)) = answer.chunk().await {
        read.extend_from_slice(&chunk);
        if read.len() >= MOST_READ {
            read.truncate(MOST_READ);
            break;
        }
    }
    read
}

impl Apns {
    /// A sender with `key` to Apple's two hosts, or to others in tests.
    pub fn new(key: ApnsKey, production: &str, sandbox: &str) -> Self {
        Self {
            key,
            production: production.trim_end_matches('/').to_owned(),
            sandbox: sandbox.trim_end_matches('/').to_owned(),
            token: Mutex::new(None),
        }
    }

    fn token(&self, now: Timestamp, renew: bool) -> String {
        let mut held = self
            .token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fresh = held.as_ref().filter(|(made, _)| {
            !renew && now - *made < chrono::Duration::minutes(TOKEN_MINUTES) && *made <= now
        });
        if let Some((_, token)) = fresh {
            return token.clone();
        }
        let token = self.key.token(now);
        *held = Some((now, token.clone()));
        token
    }

    /// Sends `message` to one phone.
    pub(super) async fn send(
        &self,
        client: &reqwest::Client,
        token: &str,
        sandbox: bool,
        message: &PhoneMessage,
        now: Timestamp,
    ) -> Sent {
        // A token is hexadecimal: the API takes no other, so it cannot
        // change the path.
        if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Sent::Gone;
        }
        let host = if sandbox {
            &self.sandbox
        } else {
            &self.production
        };
        let expiration = now.timestamp() + i64::from(PHONE_PUSH_TTL_SECONDS);
        let priority = if urgent(message.kind) { "10" } else { "5" };
        let answer = client
            .post(format!("{host}/3/device/{token}"))
            .header(
                "authorization",
                format!("bearer {}", self.token(now, false)),
            )
            .header("apns-topic", &self.key.topic)
            .header("apns-push-type", "alert")
            .header("apns-priority", priority)
            .header("apns-expiration", expiration.to_string())
            .header("content-type", "application/json")
            .body(payload(message).to_string())
            .send()
            .await;
        let answer = match answer {
            Err(error) => return Sent::Later(error.without_url().to_string()),
            Ok(answer) => answer,
        };
        let status = answer.status();
        if status.is_success() {
            return Sent::Taken;
        }
        let body = read_some(answer).await;
        let reason = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|body| body.get("reason")?.as_str().map(str::to_owned))
            .unwrap_or_default();
        match (status, reason.as_str()) {
            (StatusCode::GONE, _)
            | (_, "BadDeviceToken" | "Unregistered" | "DeviceTokenNotForTopic") => Sent::Gone,
            (StatusCode::FORBIDDEN, "ExpiredProviderToken") => {
                // Made anew for the next try.
                self.token(now, true);
                Sent::Later(format!("{status} {reason}"))
            }
            (status, _) if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
                Sent::Later(format!("{status} {reason}"))
            }
            (status, reason) => Sent::Refused(format!("{status} {reason}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use p256::ecdsa::VerifyingKey;
    use p256::ecdsa::signature::Verifier;
    use p256::pkcs8::EncodePrivateKey;

    use super::*;

    #[test]
    fn a_provider_token_is_signed_with_the_key_and_names_it() {
        let secret = SecretKey::from_slice(&[7; 32]).unwrap();
        let pem = secret.to_pkcs8_pem(Default::default()).unwrap();
        let key = ApnsKey::from_pem(&pem, "ABC123DEFG", "TEAM456789", "fly.venti.app").unwrap();
        let now = Utc.with_ymd_and_hms(2026, 11, 23, 10, 0, 0).unwrap();
        let token = key.token(now);
        let parts: Vec<&str> = token.split('.').collect();
        let read = |part: &str| -> serde_json::Value {
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
        };
        assert_eq!(read(parts[0]), json!({"alg": "ES256", "kid": "ABC123DEFG"}));
        assert_eq!(
            read(parts[1]),
            json!({"iss": "TEAM456789", "iat": now.timestamp()})
        );
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        VerifyingKey::from(&SigningKey::from(&secret))
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
        assert!(ApnsKey::from_pem("not a key", "a", "b", "c").is_err());
        assert!(!format!("{key:?}").contains("BEGIN"));
    }
}
