//! The pilots of everyone's public flights, ranked by the sum of the
//! points of their best flights.
//!
//! It is asked in the words of the list of public flights: a part of the
//! map, a span of time. A season is a span the client names; the server
//! knows no calendar. Like that list it holds public flights and nothing
//! else and is the same whoever asks.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{FlightTerms, RANKING_COUNTED, RankedPilot, RankingResponse};
use venti_store::{Ranked, Store, Visibility};

use super::cache::Keeper;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, body, cache, feed, wire};

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<String>,
}

/// A sum of points as it is handed out: rounded to hundredths, as the
/// points of a flight are. Three numbers with two decimals each do not
/// always add up to one with two.
fn rounded(points: f64) -> f64 {
    (points * 100.0).round() / 100.0
}

fn pilot(ranked: &Ranked) -> Result<RankedPilot, ApiError> {
    Ok(RankedPilot {
        pilot: wire::pilot(&ranked.pilot_handle, &ranked.pilot_name)?,
        points: rounded(ranked.points),
        scored: ranked.scored,
        flights: wire::flights(&ranked.flights)?,
    })
}

/// `GET /v1/ranking`: the pilots with the most points, from the top.
///
/// Like the list of public flights, the answer may be kept and is checked
/// before every reuse: a flight that is made private counts no longer at
/// once.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let limit = body::page_size(params.limit.as_deref())?;
    let which = feed::which(
        params.bbox.as_deref(),
        params.from.as_deref(),
        params.to.as_deref(),
    )?;

    let ranked = api.store.ranking(&which, RANKING_COUNTED, limit).await?;
    // The store counts public flights only. Looking again costs nothing,
    // and a mistake there would show a flight to everyone.
    if ranked
        .iter()
        .flat_map(|pilot| &pilot.flights)
        .any(|flight| flight.visibility != Visibility::Public)
    {
        static GATE: Sparse = Sparse::new();
        GATE.error("the store counted a flight that is not public in the ranking");
        return Err(ApiError::internal());
    }
    let answer = RankingResponse {
        counted: u32::try_from(RANKING_COUNTED).unwrap_or(u32::MAX),
        pilots: ranked.iter().map(pilot).collect::<Result<_, _>>()?,
        terms: Some(FlightTerms::current()),
    };
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sum_of_points_is_handed_out_in_hundredths() {
        assert_eq!(rounded(11.98 + 10.42), 22.4);
        assert_eq!(rounded(0.1 + 0.2), 0.3);
        assert_eq!(rounded(104.37), 104.37);
        assert_eq!(rounded(0.0), 0.0);
    }
}
