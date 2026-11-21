//! Identifiers, session tokens and invite codes: how they are made, how
//! they are written, and what of them is kept.
//!
//! Everything random here comes from the operating system. A token or an
//! invite code is shown once, to whoever it is for; the server keeps its
//! SHA-256 hash. That plain hash is enough because these are long random
//! values that nobody can guess. A password is not, and goes through
//! [`super::password`] instead.

use std::fmt;

use sha2::{Digest, Sha256};
use venti_api_types::v1::ACCESS_TOKEN_PREFIX;
use venti_store::SecretHash;

/// The characters identifiers and tokens are written in: the digits and the
/// alphabet without `i`, `l`, `o` and `u`. Five bits each, lower case, safe
/// in a path without escaping.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// What a session token starts with, so that one found in a log or a
/// repository can be recognised for what it is.
const TOKEN_PREFIX: &str = "venti_";

/// How many characters of a token follow the prefix: 256 random bits.
const TOKEN_LENGTH: usize = 52;

/// The operating system could not supply random bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the operating system supplied no random bytes")]
pub struct NoRandomness;

/// Writes bytes in [`ALPHABET`], five bits to a character, first bit first.
/// The last character is padded with zero bits.
fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            text.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
    }
    if bits > 0 {
        text.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
    }
    text
}

fn random<const N: usize>() -> Result<[u8; N], NoRandomness> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes).map_err(|_| NoRandomness)?;
    Ok(bytes)
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}

/// A new identifier for an account or a session: 128 random bits as 26
/// characters.
pub fn random_id() -> Result<String, NoRandomness> {
    Ok(encode(&random::<16>()?))
}

/// Whether text has the form of an identifier made by [`random_id`]. One
/// that has not is the identifier of nothing, which is known without
/// asking the database.
pub fn is_id(text: &str) -> bool {
    text.len() == ID_LENGTH && text.bytes().all(|byte| ALPHABET.contains(&byte))
}

/// How many characters an identifier has: 128 bits, five to a character.
const ID_LENGTH: usize = 26;

/// The address of an account's flight with a given identity: 26 characters.
///
/// It is worked out, not drawn, so the same flight uploaded again by the
/// same account gets the same address, also after it was deleted in
/// between. It cannot be worked out by anyone else: the account's
/// identifier is 128 random bits that never leave the server. So knowing a
/// flight's file is not knowing the link to it.
pub fn flight_id(account_id: &str, identity: &str) -> String {
    let hash = sha256(&[
        b"venti flight address 1\0",
        account_id.as_bytes(),
        b"\0",
        identity.as_bytes(),
    ]);
    encode(&hash[..16])
}

/// A session token: what a device sends to act for an account.
///
/// It does not print itself.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// A new token: 256 random bits.
    pub fn new() -> Result<Self, NoRandomness> {
        Ok(Self(format!("{TOKEN_PREFIX}{}", encode(&random::<32>()?))))
    }

    /// Reads a token a client sent. `None` for text that cannot be one,
    /// which saves asking the database about it.
    pub fn parse(text: &str) -> Option<Self> {
        let rest = text.strip_prefix(TOKEN_PREFIX)?;
        let well_formed = rest.len() == TOKEN_LENGTH && rest.bytes().all(|b| ALPHABET.contains(&b));
        well_formed.then(|| Self(text.to_owned()))
    }

    /// The token, for handing to its device once and nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// What the store keeps of the token.
    pub fn hash(&self) -> SecretHash {
        SecretHash(sha256(&[b"venti session token 1\0", self.0.as_bytes()]))
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<hidden>)")
    }
}

/// The secret of a token a pilot gave to another program: what that
/// program sends to act for the account, within the token's scopes.
///
/// It is written as a session's token is, 256 random bits after a prefix,
/// and the prefix is another: [`ACCESS_TOKEN_PREFIX`]. A session's token
/// cannot be read as one of these or the other way round, since `_` is not
/// in the alphabet and the two differ in length, and the two are hashed
/// apart.
///
/// It does not print itself.
#[derive(Clone, PartialEq, Eq)]
pub struct AccessSecret(String);

impl AccessSecret {
    /// A new secret: 256 random bits.
    pub fn new() -> Result<Self, NoRandomness> {
        Ok(Self(format!(
            "{ACCESS_TOKEN_PREFIX}{}",
            encode(&random::<32>()?)
        )))
    }

    /// Reads a secret a client sent. `None` for text that cannot be one,
    /// which saves asking the database about it.
    pub fn parse(text: &str) -> Option<Self> {
        let rest = text.strip_prefix(ACCESS_TOKEN_PREFIX)?;
        let well_formed = rest.len() == TOKEN_LENGTH && rest.bytes().all(|b| ALPHABET.contains(&b));
        well_formed.then(|| Self(text.to_owned()))
    }

    /// The secret, for showing to the pilot once and nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// What the store keeps of the secret.
    pub fn hash(&self) -> SecretHash {
        SecretHash(sha256(&[b"venti access token 1\0", self.0.as_bytes()]))
    }
}

impl fmt::Debug for AccessSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessSecret(<hidden>)")
    }
}

/// An invite code: sixteen characters in four groups, such as
/// `7k3m-9q2x-4r8t-1v6w`. 80 random bits, short enough to read out. One
/// kind creates an account, another lets a pilot into a group.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteCode(String);

impl InviteCode {
    /// A new code.
    pub fn new() -> Result<Self, NoRandomness> {
        Ok(Self(encode(&random::<10>()?)))
    }

    /// Reads a code as a person typed it: hyphens and spaces are dropped
    /// and upper case becomes lower case. `None` for text that cannot be a
    /// code.
    pub fn parse(text: &str) -> Option<Self> {
        let code: String = text
            .chars()
            .filter(|c| !matches!(c, '-' | ' '))
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let well_formed = code.len() == 16 && code.bytes().all(|b| ALPHABET.contains(&b));
        well_formed.then_some(Self(code))
    }

    /// What the store keeps of the code.
    pub fn hash(&self) -> SecretHash {
        SecretHash(sha256(&[b"venti invite code 1\0", self.0.as_bytes()]))
    }

    /// What the store keeps of the code when it is the invite code of a
    /// group. Such a code has the form of one that creates an account and
    /// is another thing: the two are hashed apart, so that neither can
    /// stand in for the other.
    pub fn group_hash(&self) -> SecretHash {
        SecretHash(sha256(&[b"venti group invite 1\0", self.0.as_bytes()]))
    }
}

impl fmt::Display for InviteCode {
    /// The code in four groups, for giving to the person invited.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (a, rest) = self.0.split_at(4);
        let (b, rest) = rest.split_at(4);
        let (c, d) = rest.split_at(4);
        write!(f, "{a}-{b}-{c}-{d}")
    }
}

impl fmt::Debug for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InviteCode(<hidden>)")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use venti_api_types::v1::FlightId;

    use super::*;

    #[test]
    fn bytes_are_written_five_bits_to_a_character() {
        assert_eq!(encode(&[]), "");
        assert_eq!(encode(&[0]), "00");
        assert_eq!(encode(&[0xff]), "zw");
        // 00000 00100 00100 00011 0000(0)
        assert_eq!(encode(&[0x01, 0x08, 0x30]), "04430");
        assert_eq!(encode(&[0xff; 5]), "zzzzzzzz");
        assert_eq!(encode(&[0; 16]).len(), 26);
        assert_eq!(encode(&[0; 32]).len(), 52);
        assert_eq!(encode(&[0; 10]).len(), 16);
    }

    #[test]
    fn identifiers_are_26_characters_and_never_repeat() {
        let ids: BTreeSet<String> = (0..200).map(|_| random_id().unwrap()).collect();
        assert_eq!(ids.len(), 200);
        for id in &ids {
            // The form the API's types check.
            assert!(id.parse::<FlightId>().is_ok(), "{id}");
        }
    }

    #[test]
    fn a_flights_address_follows_from_account_and_identity_alone() {
        let identity = "6c55247b80a34b03fa0bb3bd38173c59";
        let address = flight_id("0123456789abcdefghjkmnpqrs", identity);

        assert_eq!(address, flight_id("0123456789abcdefghjkmnpqrs", identity));
        assert!(address.parse::<FlightId>().is_ok(), "{address}");
        // Another account, or another flight, is another address.
        assert_ne!(address, flight_id("0123456789abcdefghjkmnpqrt", identity));
        assert_ne!(
            address,
            flight_id(
                "0123456789abcdefghjkmnpqrs",
                "6c55247b80a34b03fa0bb3bd38173c5a"
            )
        );
        // The parts cannot be shifted into each other.
        assert_ne!(flight_id("ab", "c"), flight_id("a", "bc"));
        // Links that were handed out depend on this value staying as it is.
        assert_eq!(address, "5hr238ce689tq69watgzgyzw68");
    }

    #[test]
    fn a_token_is_recognised_by_its_form_and_kept_as_a_hash() {
        let token = Token::new().unwrap();
        let text = token.expose().to_owned();

        assert!(text.starts_with("venti_"));
        assert_eq!(text.len(), 58);
        assert_eq!(Token::parse(&text), Some(token.clone()));
        assert_ne!(Token::new().unwrap(), token);
        assert_eq!(token.hash(), Token::parse(&text).unwrap().hash());
        assert_ne!(token.hash(), Token::new().unwrap().hash());

        for bad in [
            "",
            "venti_",
            &text[..57],
            &format!("{text}0"),
            &text.to_uppercase(),
            &text.replace("venti_", "other_"),
            &format!("venti_{}", "u".repeat(52)),
            &format!(" {text}"),
        ] {
            assert_eq!(Token::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_token_for_a_program_is_told_from_a_sessions_by_its_form() {
        let secret = AccessSecret::new().unwrap();
        let text = secret.expose().to_owned();

        assert!(text.starts_with("venti_pat_"));
        assert_eq!(text.len(), 62);
        assert_eq!(AccessSecret::parse(&text), Some(secret.clone()));
        assert_ne!(AccessSecret::new().unwrap(), secret);
        assert_eq!(secret.hash(), AccessSecret::parse(&text).unwrap().hash());
        assert_ne!(secret.hash(), AccessSecret::new().unwrap().hash());
        assert_eq!(format!("{secret:?}"), "AccessSecret(<hidden>)");

        // Neither kind can be read as the other, and the same characters
        // would not hash alike.
        assert_eq!(Token::parse(&text), None);
        let session = Token::new().unwrap();
        assert_eq!(AccessSecret::parse(session.expose()), None);
        let as_session = sha256(&[b"venti session token 1\0", text.as_bytes()]);
        assert_ne!(secret.hash().0, as_session);

        for bad in [
            "",
            "venti_pat_",
            &text[..61],
            &format!("{text}0"),
            &text.to_uppercase(),
            &text.replace("venti_pat_", "venti_"),
            &format!("venti_pat_{}", "u".repeat(52)),
            &format!(" {text}"),
        ] {
            assert_eq!(AccessSecret::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn tokens_and_invite_codes_do_not_print_themselves() {
        let token = Token::new().unwrap();
        assert_eq!(format!("{token:?}"), "Token(<hidden>)");
        let code = InviteCode::new().unwrap();
        assert_eq!(format!("{code:?}"), "InviteCode(<hidden>)");
    }

    #[test]
    fn an_invite_code_is_read_as_a_person_types_it() {
        let code = InviteCode::new().unwrap();
        let shown = code.to_string();

        assert_eq!(shown.len(), 19);
        assert_eq!(shown.matches('-').count(), 3);
        assert_eq!(InviteCode::parse(&shown), Some(code.clone()));
        assert_eq!(InviteCode::parse(&shown.to_uppercase()), Some(code.clone()));
        assert_eq!(
            InviteCode::parse(&shown.replace('-', " ")),
            Some(code.clone())
        );
        assert_eq!(
            InviteCode::parse(&shown.replace('-', "")),
            Some(code.clone())
        );
        assert_eq!(
            InviteCode::parse(&shown).unwrap().hash(),
            code.hash(),
            "however it is typed, it is the same invite"
        );

        // The invite code of a group is kept apart from one for an account.
        assert_ne!(code.group_hash(), code.hash());
        assert_eq!(
            InviteCode::parse(&shown.to_uppercase())
                .unwrap()
                .group_hash(),
            code.group_hash()
        );
        assert_ne!(code.group_hash(), InviteCode::new().unwrap().group_hash());

        assert_eq!(InviteCode::parse(""), None);
        assert_eq!(InviteCode::parse(&shown[..18]), None);
        assert_eq!(InviteCode::parse("uuuu-uuuu-uuuu-uuuu"), None);
        assert_eq!(InviteCode::parse("7k3m-9q2x-4r8t-1v6w-0000"), None);
    }
}
