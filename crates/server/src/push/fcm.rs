//! Google's Firebase Cloud Messaging, HTTP v1, for the Android app.
//!
//! A message is a `POST` to `/v1/projects/{project}/messages:send`, with an
//! OAuth 2.0 access token of the Firebase project's service account. The
//! server makes that token itself: it signs a JWT (RS256) with the
//! service account's private key and trades it at the account's
//! `token_uri` for an access token (RFC 7523), which it uses until a few
//! minutes before it expires. Google's documentation, "Send a message
//! using FCM HTTP v1 API", "Authorize send requests" and "Using OAuth 2.0
//! for Server to Server Applications" (read 2026-11-23, *to verify against
//! a real project*).

use std::sync::Mutex;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::StatusCode;
use rsa::RsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer};
use serde::Deserialize;
use serde_json::json;
use venti_api_types::v1::{PHONE_PUSH_TTL_SECONDS, PhoneMessage};
use venti_domain::Timestamp;

use super::Sent;
use super::apns::{read_some, urgent};

/// Google's host for FCM.
pub const FCM: &str = "https://fcm.googleapis.com";

/// What the access token may do: send messages.
const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// How long the assertion is good for, in seconds: Google takes an hour
/// at most.
const ASSERTION_SECONDS: i64 = 3600;

/// How long before it expires an access token is no longer used.
const MARGIN_SECONDS: i64 = 300;

/// The service account's file as Google hands it out, of which the
/// server reads these fields.
#[derive(Deserialize)]
struct File {
    project_id: String,
    client_email: String,
    private_key: String,
    #[serde(default = "google_token_uri")]
    token_uri: String,
}

fn google_token_uri() -> String {
    "https://oauth2.googleapis.com/token".to_owned()
}

/// The Firebase project's service account: who signs, with what, and
/// where the access token is asked for.
pub struct ServiceAccount {
    project_id: String,
    client_email: String,
    token_uri: String,
    key: SigningKey<Sha256>,
}

impl std::fmt::Debug for ServiceAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceAccount")
            .field("project_id", &self.project_id)
            .field("client_email", &self.client_email)
            .field("token_uri", &self.token_uri)
            .finish_non_exhaustive()
    }
}

impl ServiceAccount {
    /// The account from the text of its JSON file. The reason if it is
    /// none; the reason never repeats the file.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: File = serde_json::from_str(text).map_err(|_| {
            "is not a service account's JSON file: it needs project_id, client_email and \
             private_key"
                .to_owned()
        })?;
        let key = RsaPrivateKey::from_pkcs8_pem(file.private_key.trim())
            .map_err(|_| "has a private_key that is not an RSA key in PKCS #8".to_owned())?;
        let plain = |text: &str| {
            !text.is_empty()
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.@".contains(&byte))
        };
        if !plain(&file.project_id) || !plain(&file.client_email) {
            return Err("has a project_id or client_email that is not one".to_owned());
        }
        Ok(Self {
            project_id: file.project_id,
            client_email: file.client_email,
            token_uri: file.token_uri,
            key: SigningKey::new(key),
        })
    }

    /// The address access tokens are asked for at.
    pub fn token_uri(&self) -> &str {
        &self.token_uri
    }

    /// The signed JWT that is traded for an access token, made at `now`.
    pub fn assertion(&self, now: Timestamp) -> String {
        let b64 = |bytes: &[u8]| URL_SAFE_NO_PAD.encode(bytes);
        let header = b64(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = json!({
            "iss": self.client_email,
            "scope": SCOPE,
            "aud": self.token_uri,
            "iat": now.timestamp(),
            "exp": now.timestamp() + ASSERTION_SECONDS,
        });
        let claims = b64(claims.to_string().as_bytes());
        let signed = format!("{header}.{claims}");
        let signature = self.key.sign(signed.as_bytes());
        format!("{signed}.{}", b64(&signature.to_bytes()))
    }
}

/// What the token endpoint answers.
#[derive(Deserialize)]
struct Granted {
    access_token: String,
    expires_in: i64,
}

/// The sender to FCM.
pub struct Fcm {
    account: ServiceAccount,
    send_url: String,
    /// The access token in use, and until when it is used.
    access: Mutex<Option<(Timestamp, String)>>,
}

impl std::fmt::Debug for Fcm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fcm")
            .field("account", &self.account)
            .field("send_url", &self.send_url)
            .finish_non_exhaustive()
    }
}

/// The body of a message to one phone.
pub fn body(token: &str, message: &PhoneMessage) -> serde_json::Value {
    json!({
        "message": {
            "token": token,
            "notification": {"title": message.title, "body": message.body},
            "data": {
                "kind": message.kind.as_str(),
                "id": message.id,
                "sent_at": message.sent_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            },
            "android": {
                "priority": if urgent(message.kind) { "HIGH" } else { "NORMAL" },
                "ttl": format!("{PHONE_PUSH_TTL_SECONDS}s"),
            },
        }
    })
}

impl Fcm {
    /// A sender for `account` to FCM at `base`: Google's, or another in
    /// tests.
    pub fn new(account: ServiceAccount, base: &str) -> Self {
        let send_url = format!(
            "{}/v1/projects/{}/messages:send",
            base.trim_end_matches('/'),
            account.project_id
        );
        Self {
            account,
            send_url,
            access: Mutex::new(None),
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Option<(Timestamp, String)>> {
        self.access
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn access_token(&self, client: &reqwest::Client, now: Timestamp) -> Result<String, Sent> {
        if let Some((until, token)) = self.held().as_ref()
            && *until > now
        {
            return Ok(token.clone());
        }
        // The grant type's colons are written out; the assertion is
        // base64url and dots, which a form carries as they are.
        let form = format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}",
            self.account.assertion(now)
        );
        let answer = client
            .post(&self.account.token_uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form)
            .send()
            .await
            .map_err(|error| Sent::Later(format!("token: {}", error.without_url())))?;
        let status = answer.status();
        let body = read_some(answer).await;
        if !status.is_success() {
            return Err(
                if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                    Sent::Later(format!("token: {status}"))
                } else {
                    Sent::Refused(format!("token: {status}"))
                },
            );
        }
        let granted: Granted = serde_json::from_slice(&body)
            .map_err(|_| Sent::Refused("token: an answer that is not a token".to_owned()))?;
        let until = now + chrono::Duration::seconds(granted.expires_in - MARGIN_SECONDS);
        *self.held() = Some((until, granted.access_token.clone()));
        Ok(granted.access_token)
    }

    /// Sends `message` to one phone.
    pub(super) async fn send(
        &self,
        client: &reqwest::Client,
        token: &str,
        message: &PhoneMessage,
        now: Timestamp,
    ) -> Sent {
        let access = match self.access_token(client, now).await {
            Ok(access) => access,
            Err(sent) => return sent,
        };
        let answer = client
            .post(&self.send_url)
            .header("authorization", format!("Bearer {access}"))
            .header("content-type", "application/json")
            .body(body(token, message).to_string())
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
        let read = read_some(answer).await;
        let text = String::from_utf8_lossy(&read);
        match status {
            // The token is no more: the app was removed, or the token
            // replaced.
            StatusCode::NOT_FOUND => Sent::Gone,
            StatusCode::FORBIDDEN if text.contains("SENDER_ID_MISMATCH") => Sent::Gone,
            StatusCode::UNAUTHORIZED => {
                *self.held() = None;
                Sent::Later(status.to_string())
            }
            status if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
                Sent::Later(status.to_string())
            }
            status => Sent::Refused(status.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    use super::*;
    use crate::accounts::google::dev;

    #[test]
    fn the_assertion_is_signed_with_the_service_accounts_key() {
        let file = json!({
            "type": "service_account",
            "project_id": "venti-test",
            "client_email": "push@venti-test.iam.gserviceaccount.com",
            "private_key": dev::test_private_key_pem(),
        });
        let account = ServiceAccount::from_json(&file.to_string()).unwrap();
        assert_eq!(account.token_uri(), "https://oauth2.googleapis.com/token");
        let now = Utc.with_ymd_and_hms(2026, 11, 23, 10, 0, 0).unwrap();
        let assertion = account.assertion(now);
        let parts: Vec<&str> = assertion.split('.').collect();
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["iss"], "push@venti-test.iam.gserviceaccount.com");
        assert_eq!(claims["scope"], SCOPE);
        assert_eq!(claims["exp"], now.timestamp() + 3600);
        let signature =
            Signature::try_from(URL_SAFE_NO_PAD.decode(parts[2]).unwrap().as_slice()).unwrap();
        VerifyingKey::<Sha256>::new(dev::test_public_key())
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
        assert!(!format!("{account:?}").contains("PRIVATE"));
        assert!(ServiceAccount::from_json("{}").is_err());
    }
}
