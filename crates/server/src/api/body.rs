//! Reading what a request carries: a JSON body, and the parameters of a
//! paged list.

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode, header};
use serde::de::DeserializeOwned;
use serde_json::error::Category;
use venti_api_types::v1::{DEFAULT_PAGE_SIZE, ErrorCode, MAX_PAGE_SIZE};

use super::error::ApiError;

/// Reads a request's body as JSON of the shape `T`.
///
/// The request has to say that its body is JSON, with
/// `Content-Type: application/json`. That is not tidiness. A page on any
/// site can make a visitor's browser send a request here with a body of
/// its choosing, as long as the request is one a plain HTML form could
/// have made: such a request is sent without asking the server first. A
/// body typed `text/plain` is of that kind, and one typed
/// `application/json` is not: for that the browser asks first, and the
/// server's answer about which origins may call it stops the request.
/// Without this rule a hostile page could try passwords or invite codes
/// through its visitors' browsers, from their addresses.
///
/// The explanation of a body that cannot be read says where it stops
/// making sense and never what stands there: a body can hold a password,
/// and an error message travels further than a body does.
pub(crate) fn json<T: DeserializeOwned>(headers: &HeaderMap, body: &Bytes) -> Result<T, ApiError> {
    must_say_json(headers)?;
    serde_json::from_slice(body).map_err(|error| {
        let problem = match error.classify() {
            Category::Eof if body.is_empty() => "is empty".to_owned(),
            Category::Syntax | Category::Eof | Category::Io => format!(
                "is not valid JSON at line {} column {}",
                error.line(),
                error.column()
            ),
            Category::Data => format!(
                "does not have the fields and types this endpoint expects \
                 (line {} column {})",
                error.line(),
                error.column()
            ),
        };
        ApiError::bad_request(
            ErrorCode::InvalidRequest,
            format!("the request body {problem}; docs/api.md shows what to send"),
        )
    })
}

/// Refuses a request that does not say its body is JSON; see [`json`].
/// A handler that counts attempts before it reads the body asks this
/// first, so that what a browser was made to send is not counted against
/// the visitor it was sent through.
pub(crate) fn must_say_json(headers: &HeaderMap) -> Result<(), ApiError> {
    if says_json(headers) {
        return Ok(());
    }
    Err(ApiError::new(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ErrorCode::UnsupportedMediaType,
        "this request's body is JSON and must say so: send it with \
         'Content-Type: application/json'",
    ))
}

/// Whether a request names its body's type as `application/json`, once,
/// with or without parameters such as a character set.
fn says_json(headers: &HeaderMap) -> bool {
    let mut types = headers.get_all(header::CONTENT_TYPE).iter();
    let (Some(value), None) = (types.next(), types.next()) else {
        return false;
    };
    value.to_str().is_ok_and(|value| {
        let essence = value.split(';').next().unwrap_or_default().trim();
        essence.eq_ignore_ascii_case("application/json")
    })
}

/// How many entries a request asks for. `None` gives the default.
pub(crate) fn page_size(limit: Option<&str>) -> Result<usize, ApiError> {
    let Some(text) = limit else {
        return Ok(DEFAULT_PAGE_SIZE);
    };
    text.parse()
        .ok()
        .filter(|limit| (1..=MAX_PAGE_SIZE).contains(limit))
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidLimit,
                format!("limit must be a number from 1 to {MAX_PAGE_SIZE}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use venti_api_types::v1::Password;

    use super::*;

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Body {
        handle: String,
        password: Password,
    }

    fn typed(content_type: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
        headers
    }

    fn message(body: &str) -> String {
        let body = Bytes::from(body.to_owned());
        let error = json::<Body>(&typed("application/json"), &body).unwrap_err();
        format!("{error:?}")
    }

    #[test]
    fn a_body_is_read_as_json_only_when_the_request_says_it_is_json() {
        let body = Bytes::from(r#"{"handle":"a","password":"b"}"#);
        for said in [
            "application/json",
            "application/json; charset=utf-8",
            "Application/JSON",
            " application/json ;charset=UTF-8",
        ] {
            assert!(json::<Body>(&typed(said), &body).is_ok(), "{said}");
        }

        // What a form on any web page can send without asking first, and
        // what merely resembles the right type.
        for said in [
            "text/plain",
            "text/plain;charset=UTF-8",
            "application/x-www-form-urlencoded",
            "multipart/form-data; boundary=x",
            "application/json5",
            "application/jsonp",
            "text/json",
            "application/octet-stream",
            "json",
            "",
        ] {
            let error = json::<Body>(&typed(said), &body).unwrap_err();
            assert_eq!(error.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE, "{said}");
            assert_eq!(error.code(), ErrorCode::UnsupportedMediaType);
        }
        // No type at all, and two types.
        let error = json::<Body>(&HeaderMap::new(), &body).unwrap_err();
        assert_eq!(error.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let mut twice = typed("text/plain");
        twice.append(header::CONTENT_TYPE, "application/json".parse().unwrap());
        let error = json::<Body>(&twice, &body).unwrap_err();
        assert_eq!(error.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn a_body_that_cannot_be_read_is_explained_without_quoting_it() {
        let good = Bytes::from(r#"{"handle":"a","password":"b"}"#);
        assert!(json::<Body>(&typed("application/json"), &good).is_ok());

        assert!(message("").contains("is empty"));
        assert!(message("{").contains("not valid JSON at line 1 column 1"));
        assert!(message("hunter2 is my password").contains("not valid JSON"));
        assert!(message(r#"{"handle":"a"}"#).contains("fields and types"));

        // serde would say: invalid type: string "hunter2-hunter2", expected ...
        let wrong_type = message(r#"{"handle":["x"],"password":"hunter2-hunter2"}"#);
        assert!(wrong_type.contains("fields and types"), "{wrong_type}");
        for text in [
            message(r#"{"handle":"hunter2-hunter2","password":7}"#),
            message(r#"{"handle":"a","password":"hunter2-hunter2""#),
            message("\"hunter2-hunter2\""),
            wrong_type,
        ] {
            assert!(!text.contains("hunter2"), "{text}");
        }
    }

    #[test]
    fn a_page_holds_from_one_to_the_most_flights() {
        assert_eq!(page_size(None).unwrap(), DEFAULT_PAGE_SIZE);
        assert_eq!(page_size(Some("1")).unwrap(), 1);
        assert_eq!(page_size(Some("100")).unwrap(), MAX_PAGE_SIZE);
        for bad in ["0", "101", "-1", "ten", "", "1e2", "18446744073709551616"] {
            assert!(page_size(Some(bad)).is_err(), "{bad:?}");
        }
    }
}
