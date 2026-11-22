//! The authorization server's rules that need no store and no clock: what
//! a redirect address may be and when two are the same, what a program may
//! ask for, and whether a verifier answers a challenge.
//!
//! Checked against, on 2026-11-22: the Model Context Protocol's
//! authorization chapter (the draft after revision 2026-07-28, on
//! modelcontextprotocol.io), the OAuth 2.1 draft it names
//! (draft-ietf-oauth-v2-1-13), RFC 7636, RFC 8707 and RFC 7591. They were
//! read through a tool that answers questions about a page; what could not
//! be checked is marked "to verify" in `docs/architecture.md`.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Duration;
use sha2::{Digest, Sha256};
use venti_api_types::v1::Scope;

/// What a program can be allowed: reading the pilot's flights, and nothing
/// else. No scope that writes is ever given out this way.
pub const SCOPES: [Scope; 1] = [Scope::FlightsRead];

/// How long a code is good for: long enough for a program to exchange it
/// over a slow connection, and no longer. OAuth 2.1 recommends ten minutes
/// at most (section 4.1.2).
pub const CODE_LIFE: Duration = Duration::minutes(2);

/// How long an access token is good for before the program has to use its
/// refresh token.
pub const ACCESS_LIFE: Duration = Duration::hours(1);

/// How long a refresh token is good for, and with it the pilot's
/// agreement, counted from the program's latest refresh: an assistant
/// that was not used all winter still reads the logbook in spring. An
/// estimate, and the owner's to set.
pub const REFRESH_LIFE: Duration = Duration::days(180);

/// How long a registered program is kept that holds no agreement and shows
/// no sign of life.
pub const CLIENT_IDLE_LIFE: Duration = Duration::days(30);

/// The most redirect addresses one program registers.
pub const MAX_REDIRECT_URIS: usize = 5;

/// The longest redirect address, in bytes.
pub const MAX_REDIRECT_URI_BYTES: usize = 400;

/// The longest `state` a program may send along, in bytes. It is the
/// program's own and is handed back unread.
pub const MAX_STATE_BYTES: usize = 512;

/// A redirect address that passed the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectUri<'a> {
    text: &'a str,
    /// `https` or `http`.
    scheme: &'a str,
    /// The host as written, with brackets around an IPv6 address.
    host: &'a str,
    port: Option<&'a str>,
    /// Everything after the authority: path and query.
    rest: &'a str,
}

impl<'a> RedirectUri<'a> {
    /// Reads a redirect address, or says why it is none.
    ///
    /// It is `https://` with a host, or `http://` to this very machine
    /// (`127.0.0.1`, `[::1]` or `localhost`), which is how a program on
    /// the pilot's own device receives the answer. No other scheme, so no
    /// script and no app's own scheme; no user name, no fragment, nothing
    /// that is not plain ASCII.
    pub fn parse(text: &'a str) -> Result<Self, &'static str> {
        if text.len() > MAX_REDIRECT_URI_BYTES {
            return Err("a redirect address is at most 400 bytes long");
        }
        let plain = |c: char| c.is_ascii_graphic() && !matches!(c, '"' | '<' | '>' | '\\' | '`');
        if !text.chars().all(plain) {
            return Err("a redirect address is plain ASCII without spaces or quotes");
        }
        if text.contains('#') {
            return Err("a redirect address has no fragment");
        }
        let (scheme, after) = if let Some(after) = text.strip_prefix("https://") {
            ("https", after)
        } else if let Some(after) = text.strip_prefix("http://") {
            ("http", after)
        } else {
            return Err(
                "a redirect address starts with https://, or with http:// for an \
                        address of the device itself",
            );
        };
        let end = after.find(['/', '?']).unwrap_or(after.len());
        let (authority, rest) = after.split_at(end);
        if authority.contains('@') {
            return Err("a redirect address names no user");
        }
        let (host, port) = match authority.strip_prefix('[') {
            // An IPv6 address stands in brackets.
            Some(bracketed) => {
                let (inside, after) = bracketed
                    .split_once(']')
                    .ok_or("a redirect address's host is not well formed")?;
                let port = match after {
                    "" => None,
                    after => Some(
                        after
                            .strip_prefix(':')
                            .ok_or("a redirect address's host is not well formed")?,
                    ),
                };
                (&authority[..inside.len() + 2], port)
            }
            None => match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            },
        };
        if port.is_some_and(|port| {
            port.is_empty() || port.len() > 5 || !port.bytes().all(|digit| digit.is_ascii_digit())
        }) {
            return Err("a redirect address's port is a number");
        }
        let loopback = matches!(host, "127.0.0.1" | "[::1]" | "localhost");
        let named = !host.is_empty()
            && !host.starts_with(['.', '-'])
            && !host.ends_with(['.', '-'])
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
        if !loopback && !named {
            return Err("a redirect address's host is not well formed");
        }
        if scheme == "http" && !loopback {
            return Err(
                "a redirect address uses https, unless it is an address of the \
                        device itself (127.0.0.1, [::1] or localhost)",
            );
        }
        Ok(Self {
            text,
            scheme,
            host,
            port,
            rest,
        })
    }

    /// The address as it was given.
    pub fn as_str(&self) -> &'a str {
        self.text
    }

    /// Whether it leads to the device the browser runs on, over plain
    /// HTTP: the address of a program on that device.
    pub fn is_loopback(&self) -> bool {
        self.scheme == "http"
    }

    /// The host, for showing to the pilot.
    pub fn host(&self) -> &'a str {
        self.host
    }

    /// Scheme, host and port: what a browser calls the origin. Made of
    /// the checked parts, so that it is fit to stand in a header.
    pub fn origin(&self) -> String {
        match self.port {
            Some(port) => format!("{}://{}:{port}", self.scheme, self.host),
            None => format!("{}://{}", self.scheme, self.host),
        }
    }

    /// Whether an address a program registered allows this one.
    ///
    /// The two are the same text, character for character. The one
    /// exception is OAuth 2.1's (section 4.1.1, with RFC 8252 section
    /// 7.3): a program on the pilot's device listens on whatever port is
    /// free that time, so for an address of the device itself the port is
    /// not compared.
    pub fn is_allowed_by(&self, registered: &str) -> bool {
        if self.text == registered {
            return true;
        }
        let Ok(registered) = RedirectUri::parse(registered) else {
            return false;
        };
        self.is_loopback()
            && registered.is_loopback()
            && self.host == registered.host
            && self.rest == registered.rest
    }
}

/// Why the scopes a program asks for are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeRefusal {
    /// A scope that writes: no program is given one this way.
    Writes,
    /// A name that is no scope of this server's, or one that reads what
    /// the connector does not offer.
    NotOffered,
}

/// The scopes a program asks for, from the `scope` parameter: names
/// separated by spaces. Left out, it asks for everything that can be
/// given, which is [`SCOPES`].
pub fn scopes(asked: Option<&str>) -> Result<Vec<Scope>, ScopeRefusal> {
    let Some(asked) = asked else {
        return Ok(SCOPES.to_vec());
    };
    let mut scopes = Vec::new();
    for name in asked.split(' ').filter(|name| !name.is_empty()) {
        let scope = Scope::from_name(name).ok_or(ScopeRefusal::NotOffered)?;
        if !SCOPES.contains(&scope) {
            return Err(if name.ends_with(":write") {
                ScopeRefusal::Writes
            } else {
                ScopeRefusal::NotOffered
            });
        }
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    if scopes.is_empty() {
        return Ok(SCOPES.to_vec());
    }
    Ok(scopes)
}

/// Scopes as the `scope` parameter writes them.
pub fn scope_text(scopes: &[Scope]) -> String {
    let names: Vec<&str> = scopes.iter().map(|scope| scope.as_str()).collect();
    names.join(" ")
}

/// Whether text has the form of an `S256` challenge: the 43 characters
/// that 32 bytes make in base64url without padding (RFC 7636, section
/// 4.2).
pub fn is_challenge(text: &str) -> bool {
    text.len() == 43 && text.bytes().all(is_base64url)
}

fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

/// Whether `verifier` answers `challenge`: it has the form of a verifier
/// (43 to 128 characters of `A-Z a-z 0-9 - . _ ~`; RFC 7636, section 4.1)
/// and its SHA-256 hash, in base64url without padding, is the challenge.
pub fn answers(challenge: &str, verifier: &str) -> bool {
    let well_formed = (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'));
    if !well_formed {
        return false;
    }
    let hashed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    same(hashed.as_bytes(), challenge.as_bytes())
}

/// Whether two texts are the same, in a time that depends on their length
/// and not on where they differ.
pub fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0_u8, |all, (a, b)| all | (a ^ b)) == 0
}

/// Whether a `resource` a program names is the connector at `canonical`
/// (RFC 8707). The scheme and the host may be written in upper case, as
/// the protocol's chapter asks servers to accept; everything else is
/// compared as it is.
pub fn is_resource(asked: &str, canonical: &str) -> bool {
    let lowered = |text: &str| -> Option<String> {
        let (scheme, rest) = text.split_once("://")?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, path) = rest.split_at(end);
        Some(format!(
            "{}://{}{path}",
            scheme.to_ascii_lowercase(),
            authority.to_ascii_lowercase()
        ))
    };
    lowered(asked).is_some_and(|asked| Some(asked) == lowered(canonical))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redirect_address_is_https_or_the_device_itself() {
        for good in [
            "https://claude.ai/api/mcp/auth_callback",
            "https://chatgpt.com/connector_platform_oauth_redirect",
            "https://app.example:8443/cb?x=1",
            "http://127.0.0.1:33418/callback",
            "http://127.0.0.1/callback",
            "http://[::1]:3000/callback",
            "http://localhost:6274/oauth/callback",
        ] {
            let read = RedirectUri::parse(good).unwrap_or_else(|why| panic!("{good}: {why}"));
            assert_eq!(read.as_str(), good);
        }
        for bad in [
            "",
            "claude.ai/callback",
            "http://claude.ai/callback",
            "http://127.0.0.1.evil.example/callback",
            "http://localhost.evil.example/callback",
            "http://192.168.1.10/callback",
            "http://[::2]/callback",
            "https://user@claude.ai/callback",
            "https://claude.ai@evil.example/callback",
            "https://claude.ai/callback#fragment",
            "https://claude.ai/call back",
            "https://claude.ai/\"><script>",
            "https://clau\u{0434}e.ai/callback",
            "https:///callback",
            "https://-claude.ai/callback",
            "https://claude.ai:port/callback",
            "https://claude.ai:/callback",
            "http://[::1/callback",
            "http://[::1]x/callback",
            "javascript:alert(1)",
            "data:text/html,x",
            "cursor://anysphere.cursor-mcp/oauth/callback",
            "file:///etc/passwd",
        ] {
            assert!(RedirectUri::parse(bad).is_err(), "{bad}");
        }
        let long = format!("https://claude.ai/{}", "a".repeat(400));
        assert!(RedirectUri::parse(&long).is_err());
    }

    #[test]
    fn an_origin_is_made_of_the_checked_parts() {
        let origin = |text: &str| RedirectUri::parse(text).unwrap().origin();
        assert_eq!(origin("https://claude.ai/api/cb?x=1"), "https://claude.ai");
        assert_eq!(
            origin("https://app.example:8443/cb"),
            "https://app.example:8443"
        );
        assert_eq!(origin("http://[::1]:3000/cb"), "http://[::1]:3000");
        let local = RedirectUri::parse("http://127.0.0.1:3000/cb").unwrap();
        assert!(local.is_loopback());
        assert_eq!(local.host(), "127.0.0.1");
        assert!(
            !RedirectUri::parse("https://claude.ai/cb")
                .unwrap()
                .is_loopback()
        );
    }

    #[test]
    fn a_redirect_address_is_the_registered_one_to_the_character() {
        let allowed = |asked: &str, registered: &str| {
            RedirectUri::parse(asked).unwrap().is_allowed_by(registered)
        };
        let registered = "https://claude.ai/api/mcp/auth_callback";
        assert!(allowed(registered, registered));
        for other in [
            "https://claude.ai/api/mcp/auth_callbacl",
            "https://claude.ai/api/mcp/auth_callback/",
            "https://claude.ai/api/mcp/auth_callback?x=1",
            "https://claude.ai:443/api/mcp/auth_callback",
            "https://Claude.ai/api/mcp/auth_callback",
            "https://claude.ai.evil.example/api/mcp/auth_callback",
            "https://claude.ai/api/mcp/../mcp/auth_callback",
        ] {
            assert!(!allowed(other, registered), "{other}");
        }

        // A program on the pilot's device: any port, and nothing else
        // may differ.
        let local = "http://127.0.0.1:3000/callback";
        assert!(allowed("http://127.0.0.1:51234/callback", local));
        assert!(allowed("http://127.0.0.1/callback", local));
        for other in [
            "http://127.0.0.1:3000/callbacl",
            "http://localhost:3000/callback",
            "http://[::1]:3000/callback",
            "http://127.0.0.1:3000/callback?x=1",
        ] {
            assert!(!allowed(other, local), "{other}");
        }
        // The port matters everywhere else.
        assert!(!allowed(
            "https://app.example:8444/cb",
            "https://app.example:8443/cb"
        ));
    }

    #[test]
    fn a_program_is_allowed_to_read_flights_and_nothing_that_writes() {
        assert_eq!(scopes(None), Ok(vec![Scope::FlightsRead]));
        assert_eq!(scopes(Some("")), Ok(vec![Scope::FlightsRead]));
        assert_eq!(scopes(Some("flights:read")), Ok(vec![Scope::FlightsRead]));
        assert_eq!(
            scopes(Some("flights:read flights:read")),
            Ok(vec![Scope::FlightsRead])
        );
        // Every scope that writes is refused as one that writes, alone or
        // beside the one that is offered.
        for scope in Scope::ALL {
            let name = scope.as_str();
            if SCOPES.contains(&scope) {
                continue;
            }
            let expected = if name.ends_with(":write") {
                ScopeRefusal::Writes
            } else {
                ScopeRefusal::NotOffered
            };
            assert_eq!(scopes(Some(name)), Err(expected), "{name}");
            let beside = format!("flights:read {name}");
            assert_eq!(scopes(Some(&beside)), Err(expected), "{beside}");
        }
        for unknown in [
            "offline_access",
            "openid",
            "*",
            "flights:read,flights:write",
        ] {
            assert_eq!(scopes(Some(unknown)), Err(ScopeRefusal::NotOffered));
        }
        assert_eq!(scope_text(&SCOPES), "flights:read");
        // Nothing that can be given out writes.
        assert!(SCOPES.iter().all(|scope| scope.as_str().ends_with(":read")));
    }

    #[test]
    fn a_verifier_answers_its_own_challenge_and_no_other() {
        // The example of RFC 7636, appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(is_challenge(challenge));
        assert!(answers(challenge, verifier));

        let mut other = verifier.to_owned();
        other.replace_range(0..1, "e");
        assert!(!answers(challenge, &other));
        // The challenge itself is no verifier for it: that would be the
        // `plain` method, which is not offered.
        assert!(!answers(challenge, challenge));
        for bad in [
            "",
            "short",
            &"a".repeat(42),
            &"a".repeat(129),
            &format!("{verifier} "),
        ] {
            assert!(!answers(challenge, bad), "{bad:?}");
        }
        for bad in [
            "",
            "plain",
            &challenge[..42],
            &format!("{challenge}="),
            &"+".repeat(43),
        ] {
            assert!(!is_challenge(bad), "{bad:?}");
        }
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"abcd"));
    }

    #[test]
    fn a_resource_is_the_connector_and_nothing_near_it() {
        let connector = "https://venti.example/mcp";
        assert!(is_resource(connector, connector));
        assert!(is_resource("HTTPS://Venti.Example/mcp", connector));
        for other in [
            "https://venti.example",
            "https://venti.example/",
            "https://venti.example/mcp/",
            "https://venti.example/MCP",
            "https://venti.example/mcp?x=1",
            "https://venti.example/mcp#x",
            "https://venti.example.evil.example/mcp",
            "http://venti.example/mcp",
            "https://other.example/mcp",
            "venti.example/mcp",
            "",
        ] {
            assert!(!is_resource(other, connector), "{other}");
        }
    }
}
