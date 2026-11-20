//! Everyone's public flights in one list: what pilots flew, newest first
//! or best first, anywhere or in a part of the map, on any day or in a
//! span of time.
//!
//! The list holds public flights and nothing else, and it is the same
//! whoever asks: a pilot who is signed in sees no flight here that a
//! stranger does not see, their own included. A flight that is private or
//! shared by link is in no page, behind no filter and after no cursor; the
//! answer has no count from which one could be told.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{
    ErrorCode, FlightId, FlightSort, FlightsResponse, parse_bbox, parse_time,
};
use venti_domain::Timestamp;
use venti_store::{PublicFlights, PublicOrder, ScoreKey, Store, Visibility};

use super::cache::Keeper;
use super::error::ApiError;
use super::flights::{page_key, page_token};
use super::log::Sparse;
use super::{Api, body, cache, wire};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
    from: Option<String>,
    to: Option<String>,
    sort: Option<String>,
    after: Option<String>,
    limit: Option<String>,
}

fn invalid_cursor() -> ApiError {
    ApiError::bad_request(
        ErrorCode::InvalidCursor,
        "after must be the next of an earlier answer to the same question",
    )
}

/// Where the next page of a list sorted by points starts, as text for the
/// client to hand back: the points of the last flight and its address. The
/// letter in front keeps it apart from the token of a list sorted by
/// start, so that neither is taken for the other.
fn score_token(key: &ScoreKey) -> String {
    format!("p{}_{}", key.0, key.1)
}

fn score_key(token: &str) -> Result<ScoreKey, ApiError> {
    token
        .strip_prefix('p')
        .and_then(|rest| rest.split_once('_'))
        .and_then(|(points, id)| {
            // Text reads as a number also when it says "inf" or "NaN".
            let points: f64 = points
                .parse()
                .ok()
                .filter(|points: &f64| points.is_finite())?;
            let id: FlightId = id.parse().ok()?;
            Some((points, id.as_str().to_owned()))
        })
        .ok_or_else(invalid_cursor)
}

/// A moment a request names, under the name of its parameter.
fn moment(name: &str, text: Option<&str>) -> Result<Option<Timestamp>, ApiError> {
    text.map(|text| {
        parse_time(text).map_err(|error| {
            ApiError::bad_request(ErrorCode::InvalidTime, format!("{name}: {error}"))
        })
    })
    .transpose()
}

/// `GET /v1/flights/public`: one page of everyone's public flights.
///
/// Like a pilot's page, the answer may be kept and is checked before every
/// reuse: a flight that is made private leaves the list at once.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let limit = body::page_size(params.limit.as_deref())?;
    let area = params
        .bbox
        .as_deref()
        .map(|bbox| {
            parse_bbox(bbox).map_err(|error| {
                ApiError::bad_request(ErrorCode::InvalidBbox, format!("{error}; {BBOX_HELP}"))
            })
        })
        .transpose()?;
    let from = moment("from", params.from.as_deref())?;
    let to = moment("to", params.to.as_deref())?;
    if from.zip(to).is_some_and(|(from, to)| from >= to) {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRange,
            "from must lie before to: the list holds the flights that started at or after \
             from and before to",
        ));
    }
    let sort = match params.sort.as_deref() {
        None => FlightSort::default(),
        Some(text) => text
            .parse()
            .map_err(|error: venti_api_types::v1::FlightSortError| {
                ApiError::bad_request(ErrorCode::InvalidSort, error.to_string())
            })?,
    };
    // A cursor belongs to the order it was given out for; one of the other
    // order does not read as one of this.
    let after = params.after.as_deref();
    let order = match sort {
        FlightSort::Newest => PublicOrder::Newest(after.map(page_key).transpose()?),
        FlightSort::Score => PublicOrder::Best(after.map(score_key).transpose()?),
    };
    let which = PublicFlights {
        area,
        from,
        before: to,
    };

    // One more than asked for, to know whether a page follows.
    let mut flights = api.store.public_flights(&which, &order, limit + 1).await?;
    // The store hands out public flights only. Looking again costs
    // nothing, and this list is the one place where a mistake there would
    // show a flight to everyone.
    if flights
        .iter()
        .any(|flight| flight.visibility != Visibility::Public)
    {
        static GATE: Sparse = Sparse::new();
        GATE.error("the store listed a flight that is not public among the public ones");
        return Err(ApiError::internal());
    }
    let more = flights.len() > limit;
    flights.truncate(limit);
    let next = flights.last().filter(|_| more).and_then(|last| match sort {
        FlightSort::Newest => Some(page_token(&last.key())),
        FlightSort::Score => last.score_key().as_ref().map(score_token),
    });
    let answer = FlightsResponse {
        flights: wire::flights(&flights)?,
        next,
    };
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_for_a_list_by_points_names_the_last_flight_and_reads_back() {
        let id = "0123456789abcdefghjkmnpqrs".to_owned();
        for points in [0.0, 11.98, 104.37, 0.1 + 0.2, 1234.5] {
            let key = (points, id.clone());
            assert_eq!(score_key(&score_token(&key)).unwrap(), key, "{points}");
        }
        assert_eq!(
            score_token(&(11.98, id.clone())),
            "p11.98_0123456789abcdefghjkmnpqrs"
        );

        for bad in [
            "",
            "p",
            "p_",
            "p11.98",
            "p11.98_",
            "p11.98_short",
            "11.98_0123456789abcdefghjkmnpqrs",
            // The token of a list sorted by start.
            "1783161000000000_0123456789abcdefghjkmnpqrs",
            "pNaN_0123456789abcdefghjkmnpqrs",
            "pinf_0123456789abcdefghjkmnpqrs",
            "p-inf_0123456789abcdefghjkmnpqrs",
            "p1e999_0123456789abcdefghjkmnpqrs",
            "pmany_0123456789abcdefghjkmnpqrs",
            "p11.98_0123456789ABCDEFGHJKMNPQRS",
            "p11.98_0123456789abcdefghjkmnpqrs' OR '1'='1",
        ] {
            assert!(score_key(bad).is_err(), "{bad:?}");
        }
        // And the other way round: a token by points is none by start.
        assert!(page_key("p11.98_0123456789abcdefghjkmnpqrs").is_err());
    }
}
