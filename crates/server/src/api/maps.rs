//! The map regions a client can download: the list, and the files.
//!
//! The list is an answer like any other of the API. A file is not: it is
//! some gigabytes that a phone fetches over many minutes, often in several
//! goes. Three things follow, and the router is built for them
//! ([`super::Api::router`]).
//!
//! - **A file is never compressed on its way out.** A phone asks for gzip
//!   by default. A compressed answer has no length, and a download that
//!   broke off could not be taken up again by its range of bytes. The
//!   archives are compressed inside already. So the route of the files
//!   lies outside the layer that compresses the API's answers.
//! - **No deadline ends a download.** The route has no layer that bounds
//!   the time of a request, and the answer is read from the disk as fast
//!   as the client takes it, a piece at a time.
//! - **A part of a file can be asked for** with `Range`, so that a
//!   download goes on where it broke off.
//!
//! A file's address holds the version of its region, so what an address
//! names never changes: the answer may be kept for good, and its `ETag` is
//! the file's SHA-256, the one the list states.

use std::io::SeekFrom;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use venti_api_types::v1::{ErrorCode, MAPS_PATH};
use venti_store::Store;

use super::error::ApiError;
use super::log::Sparse;
use super::{Api, cache};
use crate::maps::CheckedFile;

/// The route of a region's files, as the router names it.
pub(crate) const FILE_ROUTE: &str = "/v1/maps/{region}/{version}/{name}";

/// What may keep a file, and for how long: anyone, for a year, without
/// asking again. The version is part of the address, so the bytes behind
/// an address never change.
const KEPT_FOR_GOOD: &str = "public, max-age=31536000, immutable";

/// How much of a file is read from the disk at a time. Each download
/// holds one such piece in memory.
const PIECE: usize = 256 * 1024;

/// `GET /v1/maps`: the regions whose map a client can download.
///
/// Anyone may read it: the maps are open data. A region appears once the
/// server has read its files, which after a start can take a while.
pub(crate) async fn list<S: Store>(State(api): State<Arc<Api<S>>>, headers: HeaderMap) -> Response {
    cache::json(&headers, &api.maps.list())
}

/// `GET` and `HEAD /v1/maps/{region}/{version}/{name}`: one file of a
/// region, or the part of it that `Range` asks for.
pub(crate) async fn file<S: Store>(
    State(api): State<Arc<Api<S>>>,
    method: Method,
    Path((region, version, name)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let missing = || {
        ApiError::missing(
            ErrorCode::MapNotFound,
            format!(
                "there is no such map file; {MAPS_PATH} lists the regions this server hands \
                 out, each with its version and the paths of its files"
            ),
        )
    };
    // What the request names is looked up, and never opened.
    let checked = api
        .maps
        .file(&region, &version, &name)
        .ok_or_else(missing)?;

    let tag = format!("\"{}\"", checked.sha256);
    if cache::client_has(&headers, &tag) {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        describe(&mut response, &tag);
        return Ok(response);
    }

    let mut file = match open(&checked).await {
        Ok(file) => file,
        Err(NotOpened::Changed(how)) => {
            // The listed hash can no longer be vouched for: a download
            // checked against it would fail at its end. The region is off
            // the list from here on, so this is written once.
            if api.maps.withdraw(&region) {
                tracing::error!(
                    "maps: {region} is no longer handed out: {} {how}. Start the server \
                     again once the region's folder is as its map.toml says",
                    checked.path.display()
                );
            }
            return Err(missing());
        }
        Err(NotOpened::NotNow(error)) => {
            static GATE: Sparse = Sparse::new();
            GATE.error(format_args!(
                "maps: {} cannot be opened: {error}",
                checked.path.display()
            ));
            return Err(ApiError::busy(
                "the server cannot read the file at the moment; try again shortly",
            ));
        }
    };

    let size = checked.bytes;
    // A range is for a download that goes on: only `GET` has one, and
    // only while the file is still the one the client began with.
    let range = (method == Method::GET && continues(&headers, &tag))
        .then(|| headers.get(header::RANGE))
        .flatten();
    let (status, first, length) = match asked(range, size) {
        Asked::Whole => (StatusCode::OK, 0, size),
        Asked::Part { first, last } => (StatusCode::PARTIAL_CONTENT, first, last - first + 1),
        Asked::Beyond => {
            let mut response = ApiError::new(
                StatusCode::RANGE_NOT_SATISFIABLE,
                ErrorCode::RangeNotSatisfiable,
                format!("the file has {size} bytes, and the range asked for begins after them"),
            )
            .into_response();
            let headers = response.headers_mut();
            headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            insert(headers, header::CONTENT_RANGE, &format!("bytes */{size}"));
            return Ok(response);
        }
    };

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        if first > 0 {
            file.seek(SeekFrom::Start(first)).await.map_err(|error| {
                static GATE: Sparse = Sparse::new();
                GATE.error(format_args!(
                    "maps: {} cannot be read from byte {first}: {error}",
                    checked.path.display()
                ));
                ApiError::internal()
            })?;
        }
        // The file as the client takes it, a piece at a time, until its
        // end or until the server is told to stop: a download must not
        // keep the server from stopping, and its client takes it up again
        // by the range it lacks. A small part is read in a small piece.
        let piece = usize::try_from(length).map_or(PIECE, |length| length.clamp(1, PIECE));
        let pieces = ReaderStream::with_capacity(file.take(length), piece);
        Body::from_stream(pieces.take_until(api.maps.stopping()))
    };

    let mut response = (status, body).into_response();
    describe(&mut response, &tag);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Said here and not left to be worked out: the length of what is sent,
    // also where nothing is sent because the request was a `HEAD`.
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    if status == StatusCode::PARTIAL_CONTENT {
        let last = first + length - 1;
        insert(
            headers,
            header::CONTENT_RANGE,
            &format!("bytes {first}-{last}/{size}"),
        );
    }
    Ok(response)
}

/// Why a file that was checked could not be opened.
enum NotOpened {
    /// It is no longer the file that was checked. In words for the log,
    /// what became of it.
    Changed(&'static str),
    /// It could not be opened this time, which says nothing about the
    /// file: the machine is out of something, perhaps.
    NotNow(std::io::Error),
}

/// Opens a file that was checked, if it is still the file that was
/// checked.
async fn open(checked: &CheckedFile) -> Result<tokio::fs::File, NotOpened> {
    let file = match tokio::fs::File::open(&checked.path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(NotOpened::Changed("is no longer there"));
        }
        Err(error) => return Err(NotOpened::NotNow(error)),
    };
    let metadata = file.metadata().await.map_err(NotOpened::NotNow)?;
    if !checked.is_still(metadata.len(), metadata.modified().ok()) {
        return Err(NotOpened::Changed(
            "was changed after the server had read it",
        ));
    }
    Ok(file)
}

/// What every answer about a file says of it: that parts of it can be
/// asked for, its validator, and that it may be kept for good.
fn describe(response: &mut Response, tag: &str) {
    let headers = response.headers_mut();
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(KEPT_FOR_GOOD),
    );
    insert(headers, header::ETAG, tag);
}

fn insert(headers: &mut HeaderMap, name: header::HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

/// Whether a request's `Range` is to be honoured: it names no validator
/// with `If-Range`, or the one it names is this file's.
///
/// A client that began its download with another file must get this one
/// whole, not the end of it glued to the beginning of the other. The
/// server gives its files no date, so a date in `If-Range` matches none.
fn continues(request: &HeaderMap, tag: &str) -> bool {
    match request.get(header::IF_RANGE) {
        None => true,
        Some(value) => value.to_str().is_ok_and(|value| value.trim() == tag),
    }
}

/// What a request asks of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    /// All of it.
    Whole,
    /// The bytes from `first` to `last`, both included and both inside
    /// the file.
    Part { first: u64, last: u64 },
    /// Bytes that begin at or after the file's end.
    Beyond,
}

/// Reads a `Range` header for a file of `size` bytes.
///
/// One range of bytes is understood, in its three forms: `bytes=a-b`,
/// `bytes=a-` and `bytes=-n`, the last `n` bytes. A range that ends after
/// the file ends with the file. Anything else is no range and gets the
/// whole file, as the HTTP standard allows for what a server does not
/// understand: another unit, several ranges at once, an end before the
/// beginning.
fn asked(range: Option<&HeaderValue>, size: u64) -> Asked {
    let Some(range) = range.and_then(|value| value.to_str().ok()) else {
        return Asked::Whole;
    };
    let Some((unit, spec)) = range.trim().split_once('=') else {
        return Asked::Whole;
    };
    if !unit.trim().eq_ignore_ascii_case("bytes") {
        return Asked::Whole;
    }
    let Some((first, last)) = spec.trim().split_once('-') else {
        return Asked::Whole;
    };
    // A number as the standard writes it: digits and nothing else. One
    // too large to hold is larger than any file.
    let number = |text: &str| {
        (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| text.parse::<u64>().unwrap_or(u64::MAX))
    };
    match (first, number(first), last, number(last)) {
        // The last `n` bytes.
        ("", _, _, Some(n)) => match (n, size) {
            (0, _) | (_, 0) => Asked::Beyond,
            _ => Asked::Part {
                first: size.saturating_sub(n),
                last: size - 1,
            },
        },
        // From `first` to the end.
        (_, Some(first), "", _) if first < size => Asked::Part {
            first,
            last: size - 1,
        },
        // From `first` to `last`.
        (_, Some(first), _, Some(last)) if first <= last && first < size => Asked::Part {
            first,
            last: last.min(size - 1),
        },
        (_, Some(first), "", _) if first >= size => Asked::Beyond,
        (_, Some(first), _, Some(last)) if first <= last => Asked::Beyond,
        _ => Asked::Whole,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(text: &str, size: u64) -> Asked {
        asked(Some(&HeaderValue::from_str(text).unwrap()), size)
    }

    fn part(first: u64, last: u64) -> Asked {
        Asked::Part { first, last }
    }

    #[test]
    fn one_range_of_bytes_is_read_in_its_three_forms() {
        assert_eq!(asked(None, 1000), Asked::Whole);
        assert_eq!(range("bytes=0-99", 1000), part(0, 99));
        assert_eq!(range("bytes=500-599", 1000), part(500, 599));
        assert_eq!(range("bytes=0-0", 1000), part(0, 0));
        assert_eq!(range("bytes=999-999", 1000), part(999, 999));
        // To the end.
        assert_eq!(range("bytes=500-", 1000), part(500, 999));
        assert_eq!(range("bytes=0-", 1000), part(0, 999));
        // The last bytes.
        assert_eq!(range("bytes=-100", 1000), part(900, 999));
        assert_eq!(range("bytes=-1", 1000), part(999, 999));
        // The unit in capitals, and spaces around the whole.
        assert_eq!(range(" Bytes=0-99 ", 1000), part(0, 99));
    }

    #[test]
    fn a_range_that_ends_after_the_file_ends_with_the_file() {
        assert_eq!(range("bytes=900-1999", 1000), part(900, 999));
        assert_eq!(range("bytes=-5000", 1000), part(0, 999));
        assert_eq!(
            range("bytes=0-99999999999999999999999999", 1000),
            part(0, 999)
        );
    }

    #[test]
    fn a_range_that_begins_after_the_file_cannot_be_answered() {
        assert_eq!(range("bytes=1000-", 1000), Asked::Beyond);
        assert_eq!(range("bytes=1000-1001", 1000), Asked::Beyond);
        assert_eq!(range("bytes=5000-6000", 1000), Asked::Beyond);
        assert_eq!(
            range("bytes=99999999999999999999999999-", 1000),
            Asked::Beyond
        );
        // No last bytes of nothing, and no last zero bytes.
        assert_eq!(range("bytes=-0", 1000), Asked::Beyond);
        assert_eq!(range("bytes=-10", 0), Asked::Beyond);
        assert_eq!(range("bytes=0-", 0), Asked::Beyond);
    }

    #[test]
    fn what_is_no_single_range_of_bytes_gets_the_whole_file() {
        for other in [
            "bytes=0-99,200-299",
            "bytes=0-99, -5",
            "bytes=99-0",
            "bytes=-",
            "bytes=",
            "bytes",
            "bytes=a-b",
            "bytes=+5-10",
            "bytes=5-+10",
            "bytes=-5-10",
            "bytes=5 - 10",
            "bytes=0x10-0x20",
            "items=0-99",
            "=0-99",
            "",
        ] {
            assert_eq!(range(other, 1000), Asked::Whole, "{other}");
        }
    }

    #[test]
    fn a_range_is_honoured_for_the_file_the_client_began_with() {
        let tag = "\"abc\"";
        let asking = |value: Option<&str>| {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert(header::IF_RANGE, value.parse().unwrap());
            }
            headers
        };
        assert!(continues(&asking(None), tag));
        assert!(continues(&asking(Some("\"abc\"")), tag));
        // Another file, a weak validator, and a date: none is this file.
        assert!(!continues(&asking(Some("\"abd\"")), tag));
        assert!(!continues(&asking(Some("W/\"abc\"")), tag));
        assert!(!continues(
            &asking(Some("Fri, 20 Nov 2026 10:00:00 GMT")),
            tag
        ));
    }
}
