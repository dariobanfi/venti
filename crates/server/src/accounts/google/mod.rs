//! Sign in with Google: checking an ID token that Google signed, and the
//! short-lived token that carries a first Google sign-in on to choosing a
//! handle.
//!
//! An ID token is a JSON Web Token that Google signs with RS256. It is
//! taken as Google's guide "Verify the Google ID token on your server
//! side" says (read 2026-11-21): the signature with one of Google's keys,
//! which Google publishes as a JWKS at [`GOOGLE_CERTS_URL`] and changes
//! from time to time (the answer's `Cache-Control` says how long a set
//! holds); `iss` is `accounts.google.com` or `https://accounts.google.com`;
//! `aud` is one of this server's own client IDs; `exp` has not passed. And
//! Venti takes only a Google account whose email Google has verified.
//!
//! What is checked before Google's keys are needed (the form, the issuer,
//! the audience, the time) is checked first: a token that cannot be good
//! never makes the server ask Google for anything.
//!
//! No client secret is involved: a client ID is no secret, and a server
//! that only checks ID tokens needs none.

pub mod dev;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aes_gcm::Aes128Gcm;
use aes_gcm::aead::{Aead, KeyInit};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::Verifier as _;
use rsa::{BoxedUint, RsaPublicKey};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use venti_domain::Timestamp;

/// Where Google publishes the keys it signs ID tokens with.
pub const GOOGLE_CERTS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// The two ways Google writes itself as the issuer.
const ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];

/// How far the server's clock and Google's may differ.
const SKEW_S: i64 = 60;

/// The most a request waits for Google's keys, its own wait for another
/// request's fetch included.
const KEYS_WAIT: Duration = Duration::from_secs(4);

/// How long a fetch of the keys may take.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// A key that the set at hand does not have makes the server fetch the
/// set again, but not more often than this: a stranger who sends tokens
/// with made-up key identifiers must not make it ask Google on every
/// request.
const REFETCH_AFTER: Duration = Duration::from_secs(60);

/// How long a set of keys holds when Google says nothing, and the bounds on
/// what it says.
const DEFAULT_KEYS_LIFE: Duration = Duration::from_secs(3600);
const SHORTEST_KEYS_LIFE: Duration = Duration::from_secs(60);
const LONGEST_KEYS_LIFE: Duration = Duration::from_secs(24 * 3600);

/// A longer token is not looked at. Google's are about 1,200 bytes.
const MAX_TOKEN_BYTES: usize = 8 * 1024;

/// The smallest RSA key taken, in bits. Google's are 2048.
const MIN_KEY_BITS: u32 = 2048;

/// How long a pilot has to choose a handle after a first sign-in with
/// Google.
pub const SIGNUP_LIFE_S: i64 = 15 * 60;

/// What a sign-up token begins with.
const SIGNUP_PREFIX: &str = "venti_signup_";

/// Google's keys by their identifier.
pub(crate) type KeySet = BTreeMap<String, RsaPublicKey>;

/// Who signed in with Google.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Google's identifier of the Google account, which never changes.
    pub subject: String,
    /// The Google account's email, verified by Google.
    pub email: String,
    /// The name the Google account gives, if it gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Why an ID token was not taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// It is not a good ID token for this server, and never will be.
    #[error("{0}")]
    Invalid(&'static str),
    /// Google's keys could not be had: nothing can be said of the token.
    #[error("Google's keys could not be fetched")]
    Unavailable,
}

/// The server's side of signing in with Google.
pub struct GoogleSignIn {
    /// The OAuth client IDs of the server's apps: the web's, Android's and
    /// the iPhone's. A token for any other is refused.
    client_ids: Vec<String>,
    keys: Keys,
    /// Seals sign-up tokens. Made when the process starts: a sign-up that
    /// is under way when the server restarts begins again with Google.
    sealer: Aes128Gcm,
    /// The development mode's own "Google", if this is the development
    /// mode.
    dev: Option<dev::DevIssuer>,
}

enum Keys {
    /// Google's, fetched and kept.
    Google {
        http: reqwest::Client,
        url: String,
        cached: Mutex<Cached>,
    },
    /// A set given when the verifier was made: the development mode's and
    /// the tests'.
    Fixed(Arc<KeySet>),
}

#[derive(Default)]
struct Cached {
    keys: KeySet,
    fresh_until: Option<Instant>,
    fetched_at: Option<Instant>,
}

impl std::fmt::Debug for GoogleSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleSignIn")
            .field("client_ids", &self.client_ids)
            .field("development", &self.dev.is_some())
            .finish_non_exhaustive()
    }
}

impl GoogleSignIn {
    /// Signing in with Google for these client IDs, with Google's keys.
    ///
    /// # Errors
    ///
    /// When the HTTP client or the sealing key cannot be made.
    pub fn google(client_ids: Vec<String>, user_agent: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(FETCH_TIMEOUT)
            .timeout(FETCH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("cannot create the HTTP client for Google's keys: {e}"))?;
        Self::with_keys(
            client_ids,
            Keys::Google {
                http,
                url: GOOGLE_CERTS_URL.to_owned(),
                cached: Mutex::new(Cached::default()),
            },
            None,
        )
    }

    /// The development mode's: it takes the tokens of its own "Google"
    /// ([`dev::DevIssuer`]) for [`dev::CLIENT_ID`], and no token of Google's.
    ///
    /// # Errors
    ///
    /// When the sealing key cannot be made.
    pub fn development() -> Result<Self, String> {
        let issuer = dev::DevIssuer::new();
        Self::with_keys(
            vec![dev::CLIENT_ID.to_owned()],
            Keys::Fixed(Arc::new(issuer.keys())),
            Some(issuer),
        )
    }

    fn with_keys(
        client_ids: Vec<String>,
        keys: Keys,
        dev: Option<dev::DevIssuer>,
    ) -> Result<Self, String> {
        let mut key = [0_u8; 16];
        getrandom::fill(&mut key).map_err(|_| "no randomness for the sealing key".to_owned())?;
        let sealer = Aes128Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
        Ok(Self {
            client_ids,
            keys,
            sealer,
            dev,
        })
    }

    /// The development mode's "Google", if this is the development mode.
    pub(crate) fn dev_issuer(&self) -> Option<&dev::DevIssuer> {
        self.dev.as_ref()
    }

    /// Who an ID token says signed in, if it is good for this server at
    /// `now`.
    pub async fn verify(&self, id_token: &str, now: Timestamp) -> Result<Identity, Refusal> {
        let token = Parsed::read(id_token)?;
        let claims = &token.claims;
        if !ISSUERS.contains(&claims.iss.as_str()) {
            return Err(Refusal::Invalid("the token is not Google's"));
        }
        if !self.client_ids.contains(&claims.aud) {
            return Err(Refusal::Invalid("the token is for another app"));
        }
        if claims.exp + SKEW_S <= now.timestamp() {
            return Err(Refusal::Invalid("the token has expired"));
        }
        if claims.iat.is_some_and(|iat| iat - SKEW_S > now.timestamp()) {
            return Err(Refusal::Invalid("the token is from the future"));
        }
        let key = self.key(&token.kid).await?;
        let signature = Signature::try_from(token.signature.as_slice())
            .map_err(|_| Refusal::Invalid("the token's signature cannot be read"))?;
        VerifyingKey::<Sha256>::new(key)
            .verify(token.signed.as_bytes(), &signature)
            .map_err(|_| Refusal::Invalid("the token's signature is not Google's"))?;
        if !claims.email_verified.is_true() {
            return Err(Refusal::Invalid(
                "Google has not verified the email of this Google account",
            ));
        }
        let email = claims
            .email
            .clone()
            .filter(|email| !email.trim().is_empty());
        let Some(email) = email else {
            return Err(Refusal::Invalid("the token names no email"));
        };
        if claims.sub.is_empty() || claims.sub.len() > 255 {
            return Err(Refusal::Invalid("the token names no Google account"));
        }
        Ok(Identity {
            subject: claims.sub.clone(),
            email,
            name: claims.name.clone().filter(|name| !name.trim().is_empty()),
        })
    }

    /// The key with this identifier.
    async fn key(&self, kid: &str) -> Result<RsaPublicKey, Refusal> {
        match &self.keys {
            Keys::Fixed(keys) => keys.get(kid).cloned().ok_or(Refusal::Invalid(
                "the token is signed with a key Google does not have",
            )),
            Keys::Google { http, url, cached } => {
                tokio::time::timeout(KEYS_WAIT, google_key(http, url, cached, kid))
                    .await
                    .unwrap_or(Err(Refusal::Unavailable))
            }
        }
    }

    /// A sign-up token for this identity, good for [`SIGNUP_LIFE_S`] from
    /// `now`. It is sealed: whoever holds it can neither read nor change
    /// what it says, and only this process can open it.
    pub fn signup_token(&self, identity: &Identity, now: Timestamp) -> Result<String, String> {
        let sealed = Sealed {
            identity: identity.clone(),
            until: now.timestamp() + SIGNUP_LIFE_S,
        };
        let plain = serde_json::to_vec(&sealed).map_err(|e| e.to_string())?;
        let mut nonce = [0_u8; 12];
        getrandom::fill(&mut nonce).map_err(|_| "no randomness".to_owned())?;
        let mut out = nonce.to_vec();
        out.extend(
            self.sealer
                .encrypt(&nonce.into(), plain.as_slice())
                .map_err(|_| "the sign-up could not be sealed".to_owned())?,
        );
        Ok(format!("{SIGNUP_PREFIX}{}", b64(&out)))
    }

    /// The identity a sign-up token carries, if it is one of this
    /// process's and still good at `now`.
    pub fn read_signup(&self, token: &str, now: Timestamp) -> Option<Identity> {
        let bytes = token.trim().strip_prefix(SIGNUP_PREFIX)?;
        if bytes.len() > MAX_TOKEN_BYTES {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(bytes).ok()?;
        if bytes.len() < 12 {
            return None;
        }
        let (nonce, sealed) = bytes.split_at(12);
        let nonce: [u8; 12] = nonce.try_into().ok()?;
        let plain = self.sealer.decrypt(&nonce.into(), sealed).ok()?;
        let sealed: Sealed = serde_json::from_slice(&plain).ok()?;
        (sealed.until > now.timestamp()).then_some(sealed.identity)
    }
}

/// What a sign-up token holds.
#[derive(Serialize, Deserialize)]
struct Sealed {
    identity: Identity,
    until: i64,
}

/// The key with this identifier from Google's set: the set at hand while
/// it holds, fetched again when it no longer does, or when it lacks the
/// key and was not fetched within [`REFETCH_AFTER`]. A set that could not
/// be fetched again is used while it has the key.
async fn google_key(
    http: &reqwest::Client,
    url: &str,
    cached: &Mutex<Cached>,
    kid: &str,
) -> Result<RsaPublicKey, Refusal> {
    // One request fetches; the others wait for it and then find the set.
    let mut cached = cached.lock().await;
    let now = Instant::now();
    let stale = cached.fresh_until.is_none_or(|until| until <= now);
    let lacking = !cached.keys.contains_key(kid);
    let may_refetch = cached
        .fetched_at
        .is_none_or(|at| now.duration_since(at) >= REFETCH_AFTER);
    let mut failed = false;
    if stale || (lacking && may_refetch) {
        cached.fetched_at = Some(now);
        match fetch(http, url).await {
            Ok((keys, life)) => {
                cached.keys = keys;
                cached.fresh_until = Some(now + life);
            }
            Err(error) => {
                tracing::warn!(%error, "Google's keys for sign-in could not be fetched");
                failed = true;
            }
        }
    }
    match cached.keys.get(kid) {
        Some(key) => Ok(key.clone()),
        None if failed => Err(Refusal::Unavailable),
        None => Err(Refusal::Invalid(
            "the token is signed with a key Google does not have",
        )),
    }
}

async fn fetch(http: &reqwest::Client, url: &str) -> Result<(KeySet, Duration), String> {
    let response = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Google answered {}", response.status()));
    }
    let life = response
        .headers()
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .and_then(max_age)
        .unwrap_or(DEFAULT_KEYS_LIFE)
        .clamp(SHORTEST_KEYS_LIFE, LONGEST_KEYS_LIFE);
    let body = response.bytes().await.map_err(|e| e.to_string())?;
    let keys = key_set(&body)?;
    Ok((keys, life))
}

/// The `max-age` of a `Cache-Control` header.
fn max_age(header: &str) -> Option<Duration> {
    header.split(',').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("max-age")
            .then(|| value.trim().trim_matches('"').parse().ok())
            .flatten()
            .map(Duration::from_secs)
    })
}

/// The RSA signing keys of a JWKS, by identifier. A key of another kind is
/// left out.
pub(crate) fn key_set(jwks: &[u8]) -> Result<KeySet, String> {
    #[derive(Deserialize)]
    struct Jwks {
        keys: Vec<Jwk>,
    }
    #[derive(Deserialize)]
    struct Jwk {
        kid: Option<String>,
        kty: String,
        alg: Option<String>,
        n: Option<String>,
        e: Option<String>,
    }
    let jwks: Jwks = serde_json::from_slice(jwks).map_err(|e| format!("not a JWKS: {e}"))?;
    let mut keys = KeySet::new();
    for jwk in jwks.keys {
        let (Some(kid), Some(n), Some(e)) = (jwk.kid, jwk.n, jwk.e) else {
            continue;
        };
        if jwk.kty != "RSA" || jwk.alg.as_deref().is_some_and(|alg| alg != "RS256") {
            continue;
        }
        let (Ok(n), Ok(e)) = (URL_SAFE_NO_PAD.decode(n), URL_SAFE_NO_PAD.decode(e)) else {
            continue;
        };
        let n = BoxedUint::from_be_slice_vartime(&n);
        if n.bits_vartime() < MIN_KEY_BITS {
            continue;
        }
        if let Ok(key) = RsaPublicKey::new(n, BoxedUint::from_be_slice_vartime(&e)) {
            keys.insert(kid, key);
        }
    }
    if keys.is_empty() {
        return Err("the JWKS has no RSA key".to_owned());
    }
    Ok(keys)
}

/// An ID token taken apart.
struct Parsed {
    kid: String,
    claims: Claims,
    /// The header and the claims as they were signed.
    signed: String,
    signature: Vec<u8>,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    sub: String,
    exp: i64,
    #[serde(default)]
    iat: Option<i64>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Verified,
    #[serde(default)]
    name: Option<String>,
}

/// `email_verified`, which Google writes as `true` and has written as
/// `"true"`.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Verified {
    Flag(bool),
    Word(String),
    #[default]
    Missing,
}

impl Verified {
    fn is_true(&self) -> bool {
        match self {
            Self::Flag(flag) => *flag,
            Self::Word(word) => word == "true",
            Self::Missing => false,
        }
    }
}

impl Parsed {
    fn read(token: &str) -> Result<Self, Refusal> {
        let malformed = Refusal::Invalid("this is not an ID token");
        let token = token.trim();
        if token.len() > MAX_TOKEN_BYTES {
            return Err(malformed);
        }
        let mut parts = token.split('.');
        let (Some(header), Some(claims), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(malformed);
        };
        let decode = |part: &str| URL_SAFE_NO_PAD.decode(part).map_err(|_| malformed.clone());
        let header: Header =
            serde_json::from_slice(&decode(header)?).map_err(|_| malformed.clone())?;
        if header.alg != "RS256" {
            return Err(Refusal::Invalid("the token is not signed as Google signs"));
        }
        let kid = header.kid.ok_or(malformed.clone())?;
        let claims_json = decode(claims)?;
        let claims: Claims = serde_json::from_slice(&claims_json)
            .map_err(|_| Refusal::Invalid("the token does not say what an ID token says"))?;
        let signed_len = token.len() - signature.len() - 1;
        Ok(Self {
            kid,
            claims,
            signed: token[..signed_len].to_owned(),
            signature: decode(signature)?,
        })
    }
}

/// Base64url without padding, as JSON Web Tokens write it.
pub(crate) fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A handle to suggest to a pilot who signs in with Google for the first
/// time, made from the email's local part, else from the name: what is
/// left of it within the handle's rules, or `None`.
pub fn handle_from(identity: &Identity) -> Option<String> {
    use unicode_normalization::UnicodeNormalization as _;
    let local = identity.email.split('@').next().unwrap_or_default();
    let candidates = [Some(local), identity.name.as_deref()];
    candidates.into_iter().flatten().find_map(|text| {
        // Letters with marks lose their marks: "Jürgen" is "jurgen".
        let plain: String = text
            .nfkd()
            .filter(char::is_ascii)
            .map(|c| c.to_ascii_lowercase())
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let mut handle = String::new();
        for c in plain.chars() {
            if handle.is_empty() && !c.is_ascii_lowercase() {
                continue;
            }
            if c == '_' && handle.ends_with('_') {
                continue;
            }
            handle.push(c);
        }
        handle.truncate(venti_api_types::v1::Handle::MAX_LENGTH);
        let handle = handle.trim_end_matches('_').to_owned();
        venti_api_types::v1::Handle::new(&handle)
            .ok()
            .map(|handle| handle.as_str().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::dev::{CLIENT_ID, DevIssuer};
    use super::*;

    fn at() -> Timestamp {
        crate::adapters::recorded_at()
    }

    fn claims() -> serde_json::Value {
        json!({
            "iss": "https://accounts.google.com",
            "aud": CLIENT_ID,
            "sub": "110169484474386276334",
            "email": "anna@example.com",
            "email_verified": true,
            "name": "Anna Alpin",
            "iat": at().timestamp() - 10,
            "exp": at().timestamp() + 3590,
        })
    }

    #[tokio::test]
    async fn a_good_token_names_the_google_account() {
        let google = GoogleSignIn::development().unwrap();
        let token = DevIssuer::new().sign(&claims());
        let identity = google.verify(&token, at()).await.unwrap();
        assert_eq!(
            identity,
            Identity {
                subject: "110169484474386276334".to_owned(),
                email: "anna@example.com".to_owned(),
                name: Some("Anna Alpin".to_owned()),
            }
        );
        // Google's own way of writing the issuer, and of writing a flag.
        let mut other = claims();
        other["iss"] = json!("accounts.google.com");
        other["email_verified"] = json!("true");
        assert!(
            google
                .verify(&DevIssuer::new().sign(&other), at())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_token_is_refused_for_everything_that_is_not_as_it_must_be() {
        let google = GoogleSignIn::development().unwrap();
        let issuer = DevIssuer::new();
        let changed = |name: &str, value: serde_json::Value| {
            let mut claims = claims();
            claims[name] = value;
            issuer.sign(&claims)
        };
        for (what, token) in [
            ("issuer", changed("iss", json!("https://evil.example"))),
            (
                "audience",
                changed("aud", json!("someone-else.apps.googleusercontent.com")),
            ),
            ("expired", changed("exp", json!(at().timestamp() - 61))),
            ("future", changed("iat", json!(at().timestamp() + 3600))),
            ("unverified", changed("email_verified", json!(false))),
            ("no email", changed("email", json!(""))),
            ("no subject", changed("sub", json!(""))),
        ] {
            let refused = google.verify(&token, at()).await.unwrap_err();
            assert!(
                matches!(refused, Refusal::Invalid(_)),
                "{what}: {refused:?}"
            );
        }
        // Within the minute of skew an expired token still holds.
        assert!(
            google
                .verify(&changed("exp", json!(at().timestamp() - 30)), at())
                .await
                .is_ok()
        );

        // A signature that is not of the claims.
        let good = issuer.sign(&claims());
        let (signed, _) = good.rsplit_once('.').unwrap();
        let other = issuer.sign(&json!({"x": 1}));
        let (_, signature) = other.rsplit_once('.').unwrap();
        let forged = format!("{signed}.{signature}");
        assert!(google.verify(&forged, at()).await.is_err());
        // Claims changed after signing.
        let (header, rest) = good.split_once('.').unwrap();
        let (_, signature) = rest.split_once('.').unwrap();
        let mut more = claims();
        more["sub"] = json!("1");
        let tampered = format!("{header}.{}.{signature}", b64(more.to_string().as_bytes()));
        assert!(google.verify(&tampered, at()).await.is_err());
        // Another algorithm, and no algorithm.
        for alg in ["none", "HS256"] {
            let header = b64(json!({"alg": alg, "kid": dev::KID}).to_string().as_bytes());
            let token = format!("{header}.{}.", b64(claims().to_string().as_bytes()));
            assert!(google.verify(&token, at()).await.is_err(), "{alg}");
        }
        // Not a token at all.
        for junk in ["", "a.b", "a.b.c", "a.b.c.d", &"x".repeat(9000)] {
            assert!(google.verify(junk, at()).await.is_err(), "{junk}");
        }
    }

    #[tokio::test]
    async fn a_key_google_does_not_have_signs_nothing() {
        let header = b64(json!({"alg": "RS256", "kid": "other"})
            .to_string()
            .as_bytes());
        let good = DevIssuer::new().sign(&claims());
        let (_, rest) = good.split_once('.').unwrap();
        let token = format!("{header}.{rest}");
        let google = GoogleSignIn::development().unwrap();
        assert!(matches!(
            google.verify(&token, at()).await,
            Err(Refusal::Invalid(_))
        ));
    }

    #[test]
    fn a_jwks_is_read_as_google_publishes_it() {
        let jwks = DevIssuer::new().jwks();
        let keys = key_set(jwks.to_string().as_bytes()).unwrap();
        assert!(keys.contains_key(dev::KID));
        assert!(key_set(br#"{"keys": [{"kid": "a", "kty": "EC"}]}"#).is_err());
        assert!(key_set(b"not json").is_err());
        assert_eq!(
            max_age("public, max-age=19569, must-revalidate, no-transform"),
            Some(Duration::from_secs(19569))
        );
        assert_eq!(max_age("no-cache"), None);
    }

    #[test]
    fn a_signup_token_carries_the_identity_for_a_quarter_of_an_hour_and_no_more() {
        let google = GoogleSignIn::development().unwrap();
        let identity = Identity {
            subject: "1".to_owned(),
            email: "anna@example.com".to_owned(),
            name: None,
        };
        let token = google.signup_token(&identity, at()).unwrap();
        assert!(!token.contains("anna"));
        assert_eq!(google.read_signup(&token, at()), Some(identity.clone()));
        let later = at() + chrono::Duration::seconds(SIGNUP_LIFE_S);
        assert_eq!(google.read_signup(&token, later), None);
        // Another process's, and one that was changed, are nobody's.
        let other = GoogleSignIn::development().unwrap();
        assert_eq!(other.read_signup(&token, at()), None);
        let mut changed = token.clone();
        changed.pop();
        changed.push(if token.ends_with('A') { 'B' } else { 'A' });
        assert_eq!(google.read_signup(&changed, at()), None);
        assert_eq!(google.read_signup("venti_signup_", at()), None);
        assert_eq!(google.read_signup("", at()), None);
    }

    #[test]
    fn a_handle_is_suggested_from_the_email_or_the_name() {
        let suggest = |email: &str, name: Option<&str>| {
            handle_from(&Identity {
                subject: "1".to_owned(),
                email: email.to_owned(),
                name: name.map(str::to_owned),
            })
        };
        assert_eq!(
            suggest("anna.alpin@example.com", None).as_deref(),
            Some("anna_alpin")
        );
        assert_eq!(
            suggest("Jürgen.Ö+fly@example.com", None).as_deref(),
            Some("jurgen_o_fly")
        );
        assert_eq!(
            suggest("42@example.com", Some("Dario B")).as_deref(),
            Some("dario_b")
        );
        assert_eq!(
            suggest("a_very_long_local_part_indeed@example.com", None).as_deref(),
            Some("a_very_long_local_pa")
        );
        assert_eq!(suggest("x@example.com", Some("山田")), None);
    }
}
