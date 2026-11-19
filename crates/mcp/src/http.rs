//! The HTTP side of the protocol: what of a request it reads, and what an
//! answer consists of. Whoever uses this crate owns the socket; these are
//! plain values.

use serde_json::Value;

/// One HTTP request to the MCP endpoint, as far as the protocol reads it.
#[derive(Debug, Clone, Copy)]
pub struct HttpRequest<'a> {
    /// The method, for example `POST`.
    pub method: &'a str,
    /// The headers, by name and value. Names are compared without regard
    /// to case, as HTTP asks. A header given twice counts by its first
    /// value.
    pub headers: &'a [(&'a str, &'a str)],
    /// The body.
    pub body: &'a [u8],
}

impl HttpRequest<'_> {
    /// The value of a header, without the blanks around it.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim())
    }
}

/// The HTTP answer to a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The status code.
    pub status: u16,
    /// Headers to send, by lower-case name.
    pub headers: Vec<(&'static str, String)>,
    /// The body: one JSON value, or nothing where the protocol asks for an
    /// answer without one.
    pub body: Vec<u8>,
}

impl Answer {
    /// An answer whose body is one JSON value.
    pub(crate) fn json(status: u16, body: &Value) -> Self {
        Self {
            status,
            headers: vec![
                ("content-type", "application/json".to_owned()),
                // An answer is made for one request; nothing along the way
                // has a use for keeping it.
                ("cache-control", "no-store".to_owned()),
            ],
            body: body.to_string().into_bytes(),
        }
    }

    /// `202 Accepted` with no body: the answer to a notification.
    pub(crate) fn accepted() -> Self {
        Self {
            status: 202,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Adds a header.
    pub(crate) fn with_header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.to_owned()));
        self
    }

    /// The body as JSON. `None` for an answer without a body.
    pub fn body_json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

/// Whether a `Content-Type` says that the body is JSON. Parameters such as
/// a character set are ignored.
pub(crate) fn is_json(content_type: &str) -> bool {
    media_type(content_type).eq_ignore_ascii_case("application/json")
}

/// Whether an `Accept` header allows an answer in JSON. A range that the
/// client marked as unwanted (`q=0`) does not count.
pub(crate) fn accepts_json(accept: &str) -> bool {
    accept.split(',').any(|range| {
        let unwanted = range
            .split(';')
            .skip(1)
            .filter_map(|parameter| parameter.trim().strip_prefix("q="))
            .any(|quality| quality.trim().parse::<f64>().is_ok_and(|q| q <= 0.0));
        let media = media_type(range).to_ascii_lowercase();
        !unwanted && matches!(media.as_str(), "application/json" | "application/*" | "*/*")
    })
}

/// A media type without its parameters and the blanks around it.
fn media_type(text: &str) -> &str {
    text.split(';').next().unwrap_or_default().trim()
}

/// Decodes a header value that may be wrapped as `=?base64?...?=`.
///
/// The newest revision of the protocol sends a name that does not fit a
/// header as it is (anything outside visible ASCII, or with blanks at
/// either end) in this form, the bytes being the name in UTF-8. `None` for
/// a value that has the wrapping and cannot be decoded.
pub(crate) fn unwrap_header_value(value: &str) -> Option<String> {
    match value
        .strip_prefix("=?base64?")
        .and_then(|rest| rest.strip_suffix("?="))
    {
        Some(encoded) => String::from_utf8(base64(encoded)?).ok(),
        None => Some(value.to_owned()),
    }
}

/// Decodes standard Base64, with or without its padding.
fn base64(text: &str) -> Option<Vec<u8>> {
    let digits = text.trim_end_matches('=');
    if text.len() - digits.len() > 2 || digits.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(digits.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0u32);
    for digit in digits.bytes() {
        let value = match digit {
            b'A'..=b'Z' => digit - b'A',
            b'a'..=b'z' => digit - b'a' + 26,
            b'0'..=b'9' => digit - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_are_found_whatever_their_case() {
        let request = HttpRequest {
            method: "POST",
            headers: &[
                ("MCP-Protocol-Version", " 2026-07-28 "),
                ("mcp-method", "tools/list"),
                ("Mcp-Method", "tools/call"),
            ],
            body: b"",
        };
        assert_eq!(request.header("mcp-protocol-version"), Some("2026-07-28"));
        assert_eq!(request.header("Mcp-Method"), Some("tools/list"));
        assert_eq!(request.header("mcp-name"), None);
    }

    #[test]
    fn json_is_told_from_other_content() {
        assert!(is_json("application/json"));
        assert!(is_json("Application/JSON; charset=utf-8"));
        assert!(!is_json("text/plain"));
        assert!(!is_json("application/x-www-form-urlencoded"));
        assert!(!is_json("application/json-seq"));
        assert!(!is_json(""));

        // What the protocol asks clients to send.
        assert!(accepts_json("application/json, text/event-stream"));
        assert!(accepts_json("text/event-stream, application/json;q=0.5"));
        assert!(accepts_json("*/*"));
        assert!(accepts_json("application/*"));
        assert!(!accepts_json("text/event-stream"));
        assert!(!accepts_json("text/event-stream, application/json;q=0"));
        assert!(!accepts_json(""));
    }

    #[test]
    fn a_wrapped_header_value_is_decoded() {
        // The examples of the specification's transport page.
        for (header, value) in [
            ("us-west1", "us-west1"),
            ("=?base64?SGVsbG8sIOS4lueVjA==?=", "Hello, 世界"),
            ("=?base64?IHBhZGRlZCA=?=", " padded "),
            ("=?base64?bGluZTEKbGluZTI=?=", "line1\nline2"),
            ("=?base64?PT9iYXNlNjQ/bGl0ZXJhbD89?=", "=?base64?literal?="),
            // Without padding, as some encoders write it.
            ("=?base64?SGVsbG8sIOS4lueVjA?=", "Hello, 世界"),
            ("=?base64??=", ""),
        ] {
            assert_eq!(unwrap_header_value(header).as_deref(), Some(value));
        }
        for broken in [
            "=?base64?not base64!?=",
            "=?base64?A?=",
            "=?base64?QUJD====?=",
            // Bytes that are not text.
            "=?base64?/w==?=",
        ] {
            assert_eq!(unwrap_header_value(broken), None, "{broken}");
        }
        // Only the exact wrapping counts.
        assert_eq!(
            unwrap_header_value("=?BASE64?QQ==?=").as_deref(),
            Some("=?BASE64?QQ==?=")
        );
    }
}
