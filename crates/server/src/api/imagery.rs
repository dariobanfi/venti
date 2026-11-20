//! `GET /v1/imagery/{source}/{z}/{x}/{y}`: one tile of map imagery that
//! the server passes on ([`crate::imagery`]).
//!
//! The address is a map's own: zoom, column, row, as MapLibre writes
//! `{z}/{x}/{y}`. Anything but a source the server has and three plain
//! numbers that name a tile is "not found", before the disk or the network
//! is touched.
//!
//! | Answer | When | May be kept |
//! |---|---|---|
//! | `200`, `image/jpeg` | the provider's tile, wholly inside | a week |
//! | `200`, `image/png` | a tile on the edge, cut | a week |
//! | `204` | nothing here: outside, beyond the provider's zooms, or cut to nothing | a week |
//! | `502` | the provider failed or sent what is not a tile | a minute |
//! | `504` | the provider did not answer in time | a minute |
//! | `404` | not an address of a tile, or no such source | |

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::ErrorCode;
use venti_sources::TileAddress;
use venti_store::Store;

use super::Api;
use super::error::ApiError;
use crate::imagery::Answer;

/// The route, as the router names it.
pub(crate) const ROUTE: &str = "/v1/imagery/{source}/{z}/{x}/{y}";

/// How long a tile, or the word that there is none, may be kept: a week.
/// The provider's own tiles say 14 days; the server keeps them longer.
const KEPT_A_WEEK: &str = "public, max-age=604800";

/// How long a failure may be kept: a minute, so that a map asks again
/// soon and nothing caches a provider's bad hour.
const KEPT_A_MINUTE: &str = "public, max-age=60";

/// The most digits a number of a tile's address has: a column at zoom 24
/// has eight.
const MAX_DIGITS: usize = 8;

/// Reads a number of a tile's address: digits and nothing else.
fn number(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > MAX_DIGITS || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The tile a request names, if it names one.
fn tile_of(z: &str, x: &str, y: &str) -> Option<TileAddress> {
    let z = u8::try_from(number(z)?).ok()?;
    TileAddress::new(z, number(x)?, number(y)?)
}

/// `GET /v1/imagery/{source}/{z}/{x}/{y}`.
pub(crate) async fn tile<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Path((source, z, x, y)): Path<(String, String, String, String)>,
) -> Response {
    let not_found = || {
        ApiError::missing(
            ErrorCode::NotFound,
            "there is no such tile: the address is /v1/imagery/{source}/{z}/{x}/{y}, with a \
             source this server has and the zoom, column and row of a tile",
        )
        .into_response()
    };
    let Some(tile) = tile_of(&z, &x, &y) else {
        return not_found();
    };
    let Some(imagery) = &api.imagery else {
        return not_found();
    };
    let Some(answer) = imagery.answer(&source, tile).await else {
        return not_found();
    };
    let (mut response, kept) = match answer {
        Answer::Tile { media_type, bytes } => (
            (
                [(header::CONTENT_TYPE, HeaderValue::from_static(media_type))],
                bytes,
            )
                .into_response(),
            KEPT_A_WEEK,
        ),
        Answer::Nothing => (StatusCode::NO_CONTENT.into_response(), KEPT_A_WEEK),
        Answer::Failed(why) => {
            tracing::warn!("imagery: {source} {tile}: {why}");
            (
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    ErrorCode::Unavailable,
                    "the provider of this imagery did not send the tile; try again shortly",
                )
                .into_response(),
                KEPT_A_MINUTE,
            )
        }
        Answer::TimedOut => (
            ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                ErrorCode::Timeout,
                "the provider of this imagery did not answer in time; try again shortly",
            )
            .into_response(),
            KEPT_A_MINUTE,
        ),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(kept));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_three_plain_numbers_of_a_tile() {
        assert_eq!(
            tile_of("15", "17448", "11446"),
            TileAddress::new(15, 17448, 11446)
        );
        assert!(tile_of("0", "0", "0").is_some());
        for (z, x, y) in [
            ("15", "17448", ""),
            ("15", "+17448", "11446"),
            ("15", "-1", "11446"),
            ("15", "17448", "11446.png"),
            ("15", "1e4", "1"),
            ("15", "32768", "1"),
            ("25", "0", "0"),
            ("256", "0", "0"),
            ("15", "123456789", "1"),
            ("x", "1", "1"),
        ] {
            assert_eq!(tile_of(z, x, y), None, "{z}/{x}/{y}");
        }
    }
}
