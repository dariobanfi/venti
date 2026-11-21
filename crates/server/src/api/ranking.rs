//! The pilots of everyone's public flights, ranked by the sum of the
//! points of their best flights.
//!
//! It is asked in the words of the list of public flights: a part of the
//! map, a span of time. A season is a span the client names. A month is
//! one too, and has a word of its own, `month`: a calendar month by UTC.
//! Like that list it holds public flights and nothing else and is the same
//! whoever asks.
//!
//! The answer says which years and months have a ranking at all, so that
//! a page offers those and no empty one.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{
    ErrorCode, FlightTerms, RANKING_COUNTED, RankedPilot, RankingMonth, RankingResponse,
    RankingYear, YearMonth,
};
use venti_store::{Month, PublicFlights, Ranked, Store, Visibility};

use super::cache::Keeper;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, body, cache, feed, wire};

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
    from: Option<String>,
    to: Option<String>,
    month: Option<String>,
    limit: Option<String>,
}

/// A sum of points as it is handed out: rounded to hundredths, as the
/// points of a flight are. Three numbers with two decimals each do not
/// always add up to one with two.
pub(super) fn rounded(points: f64) -> f64 {
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

/// The month a request's `month` names.
fn month_asked_for(text: &str) -> Result<Month, ApiError> {
    let month: YearMonth = text.parse().map_err(|error| {
        ApiError::bad_request(ErrorCode::InvalidTime, format!("month: {error}"))
    })?;
    Month::new(month.year(), month.month()).ok_or_else(ApiError::internal)
}

/// The years and months that have a ranking, the newest first, from the
/// months the store counted, the oldest first.
fn periods(months: &[(Month, u32)]) -> Vec<RankingYear> {
    let mut years: Vec<RankingYear> = Vec::new();
    for (month, flights) in months.iter().rev() {
        let Some(named) = YearMonth::new(month.year(), month.month()) else {
            continue;
        };
        if years.last().is_none_or(|year| year.year != month.year()) {
            years.push(RankingYear {
                year: month.year(),
                flights: 0,
                months: Vec::new(),
            });
        }
        if let Some(year) = years.last_mut() {
            year.flights = year.flights.saturating_add(*flights);
            year.months.push(RankingMonth {
                month: named,
                flights: *flights,
            });
        }
    }
    years
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
    let mut which = feed::which(
        params.bbox.as_deref(),
        params.from.as_deref(),
        params.to.as_deref(),
    )?;
    if let Some(month) = params.month.as_deref() {
        if which.from.is_some() || which.before.is_some() {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidRange,
                "month stands for a from and a to: send month, or from and to, and not both",
            ));
        }
        let month = month_asked_for(month)?;
        which.from = Some(month.start());
        which.before = Some(month.next().start());
    }
    // The periods are those of the part of the map, whatever span is
    // asked for: they are what else there is to ask for.
    let anytime = PublicFlights {
        area: which.area,
        ..PublicFlights::default()
    };
    let months = api.store.scored_months(&anytime).await?;

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
        periods: periods(&months),
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

    #[test]
    fn the_periods_are_years_with_their_months_the_newest_first() {
        let month = |year, month| Month::new(year, month).unwrap();
        let said = periods(&[
            (month(2025, 11), 1),
            (month(2026, 5), 2),
            (month(2026, 7), 4),
        ]);
        let short: Vec<(i32, u32, Vec<String>)> = said
            .iter()
            .map(|year| {
                let months = year
                    .months
                    .iter()
                    .map(|month| format!("{} {}", month.month, month.flights))
                    .collect();
                (year.year, year.flights, months)
            })
            .collect();
        assert_eq!(
            short,
            [
                (
                    2026,
                    6,
                    vec!["2026-07 4".to_owned(), "2026-05 2".to_owned()]
                ),
                (2025, 1, vec!["2025-11 1".to_owned()]),
            ]
        );
        assert!(periods(&[]).is_empty());
    }

    #[test]
    fn a_month_is_read_as_a_span_by_utc() {
        let july = month_asked_for("2026-07").unwrap();
        assert_eq!(july.start().to_rfc3339(), "2026-07-01T00:00:00+00:00");
        assert_eq!(
            july.next().start().to_rfc3339(),
            "2026-08-01T00:00:00+00:00"
        );
        for bad in ["2026", "2026-13", "2026-7", "26-07", "2026-07-01", "july"] {
            assert!(month_asked_for(bad).is_err(), "{bad}");
        }
    }
}
