//! Reading what a request carries: a JSON body, and the parameters of a
//! paged list.

use axum::body::Bytes;
use serde::de::DeserializeOwned;
use serde_json::error::Category;
use venti_api_types::v1::{DEFAULT_PAGE_SIZE, ErrorCode, MAX_PAGE_SIZE};

use super::error::ApiError;

/// Reads a request's body as JSON of the shape `T`.
///
/// The explanation says where the body stops making sense and never what
/// stands there: a body can hold a password, and an error message travels
/// further than a body does.
pub(crate) fn json<T: DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
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

    fn message(body: &str) -> String {
        let error = json::<Body>(&Bytes::from(body.to_owned())).unwrap_err();
        format!("{error:?}")
    }

    #[test]
    fn a_body_that_cannot_be_read_is_explained_without_quoting_it() {
        assert!(json::<Body>(&Bytes::from(r#"{"handle":"a","password":"b"}"#)).is_ok());

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
