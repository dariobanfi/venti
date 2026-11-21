//! The cryptography of Web Push: the message encrypted for one browser
//! (RFC 8291, the `aes128gcm` content coding of RFC 8188), and the
//! server's signature on its request to the push service (VAPID, RFC
//! 8292), both on P-256.
//!
//! The server's key is a secret: [`ServerKey`] shows its public half when
//! printed, and its text ([`ServerKey::to_text`]) goes to the store and
//! nowhere else.

use std::fmt;

use aes_gcm::Aes128Gcm;
use aes_gcm::aead::{Aead, KeyInit};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hkdf::Hkdf;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use serde_json::json;
use sha2::Sha256;
use venti_domain::Timestamp;

/// The record size the message announces. One record holds all of it.
const RECORD_SIZE: u32 = 4096;

/// The most plaintext one record holds: the record size less the
/// header's 86 bytes, the delimiter and the tag (RFC 8291, section 4).
pub const MAX_PLAINTEXT: usize = 3993;

/// How long a VAPID signature is good for: twelve hours, half of the
/// most RFC 8292 allows.
const SIGNATURE_HOURS: i64 = 12;

/// What went wrong in the cryptography.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    /// The browser's public key is not a point on P-256.
    #[error("the browser's key is not a P-256 public key")]
    BrowserKey,
    /// The browser's authentication secret is not 16 bytes.
    #[error("the browser's authentication secret is not 16 bytes")]
    AuthSecret,
    /// The message is longer than one record holds.
    #[error("the message is longer than {MAX_PLAINTEXT} bytes")]
    TooLong,
    /// The text of a key is not one.
    #[error("the text is not a P-256 private key")]
    ServerKey,
    /// The system gave no randomness.
    #[error("the system gave no randomness")]
    NoRandomness,
}

/// Bytes from base64url, with or without padding.
pub fn from_base64url(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).ok()
}

fn base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn random<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| CryptoError::NoRandomness)?;
    Ok(bytes)
}

fn random_secret() -> Result<SecretKey, CryptoError> {
    // A random 32 bytes are a scalar of P-256 but for about one in 2^32.
    loop {
        if let Ok(secret) = SecretKey::from_slice(&random::<32>()?) {
            return Ok(secret);
        }
    }
}

fn uncompressed(public: &PublicKey) -> Vec<u8> {
    public.to_sec1_point(false).as_bytes().to_vec()
}

/// A browser's public key, checked to be on the curve, as RFC 8291 asks.
pub fn browser_key(p256dh: &[u8]) -> Result<PublicKey, CryptoError> {
    if p256dh.len() != 65 || p256dh[0] != 4 {
        return Err(CryptoError::BrowserKey);
    }
    PublicKey::from_sec1_bytes(p256dh).map_err(|_| CryptoError::BrowserKey)
}

/// Encrypts a message for a browser, with a new key pair and salt of the
/// server's own for this one message: the body of the request to the
/// push service, with `Content-Encoding: aes128gcm`.
pub fn encrypt(p256dh: &[u8], auth: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    encrypt_with(p256dh, auth, plaintext, &random_secret()?, random::<16>()?)
}

/// [`encrypt`], with the sender's key and the salt given: for the test
/// vector of RFC 8291.
fn encrypt_with(
    p256dh: &[u8],
    auth: &[u8],
    plaintext: &[u8],
    sender: &SecretKey,
    salt: [u8; 16],
) -> Result<Vec<u8>, CryptoError> {
    let browser = browser_key(p256dh)?;
    if auth.len() != 16 {
        return Err(CryptoError::AuthSecret);
    }
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(CryptoError::TooLong);
    }
    let sender_public = uncompressed(&sender.public_key());
    let shared = p256::ecdh::diffie_hellman(sender.to_nonzero_scalar(), browser.as_affine());

    // The shared and the authentication secret together (section 3.3).
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(p256dh);
    key_info.extend_from_slice(&sender_public);
    let mut ikm = [0; 32];
    Hkdf::<Sha256>::new(Some(auth), shared.raw_secret_bytes())
        .expand(&key_info, &mut ikm)
        .map_err(|_| CryptoError::TooLong)?;

    // The key and nonce of RFC 8188.
    let content = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut key = [0; 16];
    let mut nonce = [0; 12];
    content
        .expand(b"Content-Encoding: aes128gcm\0", &mut key)
        .map_err(|_| CryptoError::TooLong)?;
    content
        .expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| CryptoError::TooLong)?;

    // One record: the message and the delimiter of the last record.
    let mut record = plaintext.to_vec();
    record.push(2);
    let cipher = Aes128Gcm::new_from_slice(&key).map_err(|_| CryptoError::TooLong)?;
    let sealed = cipher
        .encrypt(&nonce.into(), record.as_slice())
        .map_err(|_| CryptoError::TooLong)?;

    // The header: salt, record size, the length of the key id and the
    // key id, which is the sender's public key.
    let mut body = Vec::with_capacity(86 + sealed.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(65);
    body.extend_from_slice(&sender_public);
    body.extend_from_slice(&sealed);
    Ok(body)
}

/// The server's own key pair for Web Push.
#[derive(Clone)]
pub struct ServerKey {
    secret: SecretKey,
}

impl fmt::Debug for ServerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerKey")
            .field("public", &self.public_text())
            .finish_non_exhaustive()
    }
}

impl ServerKey {
    /// A new key pair.
    pub fn generate() -> Result<Self, CryptoError> {
        Ok(Self {
            secret: random_secret()?,
        })
    }

    /// The key pair whose private key [`ServerKey::to_text`] wrote.
    pub fn from_text(text: &str) -> Result<Self, CryptoError> {
        let bytes = from_base64url(text).ok_or(CryptoError::ServerKey)?;
        let secret = SecretKey::from_slice(&bytes).map_err(|_| CryptoError::ServerKey)?;
        Ok(Self { secret })
    }

    /// The private key as text: base64url of its 32 bytes. A secret.
    pub fn to_text(&self) -> String {
        base64url(&self.secret.to_bytes())
    }

    /// The public key, uncompressed, base64url: what a browser is given
    /// as `applicationServerKey`.
    pub fn public_text(&self) -> String {
        base64url(&uncompressed(&self.secret.public_key()))
    }

    /// The JWT of RFC 8292 for a push service, by the origin of its
    /// endpoint: signed with ES256, good until twelve hours after `now`.
    /// `contact` is the `sub` claim, a `mailto:` or `https:` address.
    pub fn token(&self, audience: &str, contact: Option<&str>, now: Timestamp) -> String {
        let header = base64url(br#"{"typ":"JWT","alg":"ES256"}"#);
        let mut claims = json!({
            "aud": audience,
            "exp": (now + chrono::Duration::hours(SIGNATURE_HOURS)).timestamp(),
        });
        if let Some(contact) = contact {
            claims["sub"] = json!(contact);
        }
        let claims = base64url(claims.to_string().as_bytes());
        let signed = format!("{header}.{claims}");
        let signature: Signature = SigningKey::from(&self.secret).sign(signed.as_bytes());
        format!("{signed}.{}", base64url(&signature.to_bytes()))
    }

    /// The `Authorization` header of a request to a push service.
    pub fn authorization(&self, audience: &str, contact: Option<&str>, now: Timestamp) -> String {
        format!(
            "vapid t={}, k={}",
            self.token(audience, contact, now),
            self.public_text()
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use p256::ecdsa::VerifyingKey;
    use p256::ecdsa::signature::Verifier;

    use super::*;

    fn b64(text: &str) -> Vec<u8> {
        from_base64url(&text.replace([' ', '\n'], "")).unwrap()
    }

    /// RFC 8291, section 5, and appendix A.
    #[test]
    fn the_message_of_rfc_8291_is_encrypted_as_the_rfc_has_it() {
        let plaintext = b"When I grow up, I want to be a watermelon";
        let auth = b64("BTBZMqHH6r4Tts7J_aSIgg");
        let receiver = b64("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcx
             aOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4");
        let sender =
            SecretKey::from_slice(&b64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw")).unwrap();
        assert_eq!(
            uncompressed(&sender.public_key()),
            b64("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIg
                 Dll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8")
        );
        let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let body = encrypt_with(&receiver, &auth, plaintext, &sender, salt).unwrap();
        assert_eq!(
            body,
            b64(
                "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml
                 mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT
                 pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
            )
        );
    }

    #[test]
    fn a_key_that_is_not_on_the_curve_or_a_short_secret_is_refused() {
        let auth = [0; 16];
        let mut off_curve = vec![4; 65];
        off_curve[64] = 5;
        assert_eq!(
            encrypt(&off_curve, &auth, b"x"),
            Err(CryptoError::BrowserKey)
        );
        let browser = uncompressed(&random_secret().unwrap().public_key());
        assert_eq!(
            encrypt(&browser, &[0; 15], b"x"),
            Err(CryptoError::AuthSecret)
        );
        assert_eq!(
            encrypt(&browser, &auth, &[0; MAX_PLAINTEXT + 1]),
            Err(CryptoError::TooLong)
        );
        // Each message has a salt and a key of its own.
        assert_ne!(
            encrypt(&browser, &auth, b"x").unwrap(),
            encrypt(&browser, &auth, b"x").unwrap()
        );
    }

    #[test]
    fn the_vapid_token_is_an_es256_jwt_for_the_push_service() {
        let key = ServerKey::generate().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap();
        let token = key.token(
            "https://fcm.googleapis.com",
            Some("https://venti.example"),
            now,
        );
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: serde_json::Value =
            serde_json::from_slice(&from_base64url(parts[0]).unwrap()).unwrap();
        assert_eq!(header, json!({"typ": "JWT", "alg": "ES256"}));
        let claims: serde_json::Value =
            serde_json::from_slice(&from_base64url(parts[1]).unwrap()).unwrap();
        assert_eq!(
            claims,
            json!({
                "aud": "https://fcm.googleapis.com",
                "exp": now.timestamp() + 12 * 3600,
                "sub": "https://venti.example",
            })
        );
        // The signature is r and s, 64 bytes, by the key the header names.
        let signature = Signature::from_slice(&from_base64url(parts[2]).unwrap()).unwrap();
        let public = b64(&key.public_text());
        assert_eq!(public.len(), 65);
        VerifyingKey::from_sec1_bytes(&public)
            .unwrap()
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
        let header = key.authorization("https://fcm.googleapis.com", None, now);
        assert!(header.starts_with("vapid t=ey"));
        assert!(header.ends_with(&format!(", k={}", key.public_text())));
    }

    #[test]
    fn the_key_comes_back_from_its_text_and_is_not_printed() {
        let key = ServerKey::generate().unwrap();
        let again = ServerKey::from_text(&key.to_text()).unwrap();
        assert_eq!(again.public_text(), key.public_text());
        assert!(!format!("{key:?}").contains(&key.to_text()));
        assert_eq!(
            ServerKey::from_text("not a key").unwrap_err(),
            CryptoError::ServerKey
        );
    }
}
