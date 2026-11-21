//! The development mode's own "Google": a key pair that signs ID tokens
//! for made-up pilots, so that the apps' UI tests and the server's own
//! tests can sign in "with Google" without Google.
//!
//! **TEST-ONLY KEY.** The two primes below are of an RSA key made for this
//! file and for nothing else (2026-11-21, `openssl genrsa 2048`). It is no
//! secret: it is published here on purpose. A server trusts it only in the
//! development mode and in tests, which give the verifier this key in
//! place of Google's; a server started from its configuration never does.

use rsa::pkcs1v15::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::traits::PublicKeyParts;
use rsa::{BoxedUint, RsaPrivateKey, RsaPublicKey};
use serde_json::json;
use sha2::Digest as _;
use venti_domain::Timestamp;

use super::{KeySet, b64};

/// The key's identifier, as a JWKS names it.
pub const KID: &str = "venti-development-only";

/// The client ID the development mode's tokens are for, and that the
/// development mode takes.
pub const CLIENT_ID: &str = "venti-development.apps.googleusercontent.com";

const P: &[&str] = &[
    "ebc6e4aaac693834d3708962bdeef0604aa95a9b44ca8a5b177811799df34700",
    "9c93b2da611ce3c26134d99d746f542909eb1e4f4b2761df581f53ea7c6cea25",
    "f15167157826b58433678ff5b1227dd877be18ee03ef728e0cbd15bc626d471c",
    "cd3955b06248a07912a8fb4df71daa2d95069a98788e95354c5a4e5a2c8d98f3",
];

const Q: &[&str] = &[
    "e5443132d06513f9fc673a08073317b7912c9d41f3f75af6be955ce01ffb160e",
    "f70b2fdf6de78cb9921a53d23515fd9bdd952f10d12ed8b7b7c463a05cc1eb83",
    "738a7374ec16c7d18a5c69882cd30834c189debf2c1087430bde25e8025cee8b",
    "bac01b8474484bee1eb442adc9ea273560084f58eeebc76428ac9d2ec110beb1",
];

fn number(parts: &[&str]) -> BoxedUint {
    let hex: String = parts.concat();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect();
    BoxedUint::from_be_slice_vartime(&bytes)
}

/// Signs ID tokens as Google would, with the test-only key.
#[derive(Debug, Clone)]
pub struct DevIssuer {
    key: SigningKey<Sha256>,
    public: RsaPublicKey,
}

impl DevIssuer {
    /// The issuer with the test-only key.
    ///
    /// # Panics
    ///
    /// Never: the primes are fixed and good.
    #[must_use]
    pub fn new() -> Self {
        let private = RsaPrivateKey::from_p_q(number(P), number(Q), BoxedUint::from(65_537_u32))
            .expect("the test-only key is a good key");
        let public = private.to_public_key();
        Self {
            key: SigningKey::new(private),
            public,
        }
    }

    /// The key set a verifier is given in place of Google's.
    pub(crate) fn keys(&self) -> KeySet {
        KeySet::from([(KID.to_owned(), self.public.clone())])
    }

    /// The key set as a JWKS, as Google publishes its own.
    #[must_use]
    pub fn jwks(&self) -> serde_json::Value {
        json!({ "keys": [{
            "kid": KID,
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "n": b64(&self.public.n().to_be_bytes_trimmed_vartime()),
            "e": b64(&self.public.e().to_be_bytes_trimmed_vartime()),
        }]})
    }

    /// A token with these claims, signed.
    #[must_use]
    pub fn sign(&self, claims: &serde_json::Value) -> String {
        let header = json!({ "alg": "RS256", "kid": KID, "typ": "JWT" });
        let input = format!(
            "{}.{}",
            b64(header.to_string().as_bytes()),
            b64(claims.to_string().as_bytes())
        );
        let signature = self.key.sign(input.as_bytes());
        format!("{input}.{}", b64(&signature.to_bytes()))
    }

    /// An ID token for the made-up Google account with this email, for an
    /// hour from `now`, for [`CLIENT_ID`]. The subject is made from the
    /// email, so the same email is always the same Google account.
    #[must_use]
    pub fn id_token(&self, email: &str, name: Option<&str>, now: Timestamp) -> String {
        let mut claims = json!({
            "iss": "https://accounts.google.com",
            "aud": CLIENT_ID,
            "azp": CLIENT_ID,
            "sub": subject_of(email),
            "email": email,
            "email_verified": true,
            "iat": now.timestamp(),
            "exp": now.timestamp() + 3600,
        });
        if let Some(name) = name {
            claims["name"] = json!(name);
        }
        self.sign(&claims)
    }
}

impl Default for DevIssuer {
    fn default() -> Self {
        Self::new()
    }
}

/// The made-up subject of the made-up Google account with this email: 21
/// digits, as Google's are.
#[must_use]
pub fn subject_of(email: &str) -> String {
    let hash = sha2::Sha256::digest(email.trim().to_lowercase().as_bytes());
    let mut digits = String::from("1");
    for byte in hash.iter().take(20) {
        digits.push(char::from(b'0' + byte % 10));
    }
    digits
}
