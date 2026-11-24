//! Everyone's public flights in one list: what pilots flew, newest first
//! or best first, anywhere or in a part of the map, on any day or in a
//! span of time.
//!
//! The list holds public flights and nothing else, and it is the same
//! whoever asks: a pilot who is signed in sees no flight here that a
//! stranger does not see, their own included. A flight that is private or
//! shared by link is in no page, behind no filter and after no cursor; the
//! answer has no count from which one could be told.
//!
//! Beside the list, what changed in it: the flights that became public or
//! changed while public, and those that are public no more, so that
//! whoever keeps a copy of public flights can keep it true.
//!
//! Both are open to anyone without a token, and bounded by the address
//! that asks or by the token that asks ([`Api::count_public_read`]). Every answer says under
//! what its flights are given ([`FlightTerms`]).

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use venti_api_types::v1::{
    ErrorCode, FlightFilters, FlightId, FlightSort, FlightTerms, FlightsResponse,
    PublicFlightChangesResponse, SearchError, SentFilters, WithdrawnFlight, glider_class_name,
    parse_bbox, parse_time, route_kind_name,
};
use venti_domain::Timestamp;
use venti_store::{
    GliderClass, Near, PublicFlights, PublicOrder, ScoreKey, ScoreKind, Store, Visibility,
};

use super::budget::Reader;
use super::cache::Keeper;
use super::error::ApiError;
use super::flights::{page_key, page_token};
use super::log::Sparse;
use super::{Api, body, cache};

impl<S: Store> Api<S> {
    /// Counts a read of public flights as data for the address that asks
    /// without a token, as [`Api::count_public_read`] does: for the
    /// connector, which looks at no token.
    pub(crate) fn count_public_flight_read(&self, client: IpAddr) -> Result<(), ApiError> {
        self.count_public_read(&Reader::at(client))
    }

    /// The account whose public flights a request's `pilot` means: the
    /// pilot with this handle, if they have a page.
    pub(crate) async fn pilot_asked_for(
        &self,
        pilot: Option<&str>,
    ) -> Result<Option<String>, ApiError> {
        match pilot {
            None => Ok(None),
            Some(handle) => Ok(Some(self.pilot_with_a_page(handle).await?.id)),
        }
    }
}

/// How a list of withdrawn flights names a flight: by the SHA-256 of its
/// address, in hexadecimal. See [`WithdrawnFlight`].
fn address_hash(id: &str) -> String {
    Sha256::digest(id.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

/// The parameters of the filters beyond the box, the days and the site,
/// which the list and the ranking take alike ([`FlightFilters`]).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct FilterParams {
    near: Option<String>,
    radius_km: Option<String>,
    area: Option<String>,
    class: Option<String>,
    kind: Option<String>,
    min_km: Option<String>,
    country: Option<String>,
}

impl FilterParams {
    /// The filters, or why they were refused.
    fn read(&self) -> Result<FlightFilters, ApiError> {
        FlightFilters::parse(&SentFilters {
            near: self.near.as_deref(),
            radius_km: self.radius_km.as_deref(),
            area: self.area.as_deref(),
            class: self.class.as_deref(),
            kind: self.kind.as_deref(),
            min_km: self.min_km.as_deref(),
            country: self.country.as_deref(),
        })
        .map_err(|error| {
            let code = match error {
                SearchError::Class
                | SearchError::Kind
                | SearchError::MinDistance
                | SearchError::Country => ErrorCode::InvalidRequest,
                _ => ErrorCode::InvalidPosition,
            };
            ApiError::bad_request(code, error.to_string())
        })
    }
}

/// What the store is asked for of the filters.
pub(crate) fn narrowed(mut which: PublicFlights, filters: FlightFilters) -> PublicFlights {
    which.near = filters.near.map(|circle| Near {
        place: circle.centre,
        within_m: circle.radius_m(),
    });
    which.drawn = filters.area;
    which.class = filters
        .class
        .and_then(glider_class_name)
        .and_then(GliderClass::from_name);
    which.kind = filters
        .kind
        .and_then(route_kind_name)
        .and_then(ScoreKind::from_name);
    which.min_distance_m = filters.min_km.map(|km| km * 1000.0);
    which.country = filters.country;
    which
}

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    pilot: Option<String>,
    bbox: Option<String>,
    from: Option<String>,
    to: Option<String>,
    site: Option<String>,
    #[serde(flatten)]
    filters: FilterParams,
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

/// Which of everyone's public flights a request means, from its `bbox`,
/// `from`, `to` and filters as they were sent. The ranking is asked in the
/// same words.
pub(super) fn which(
    bbox: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    filters: &FilterParams,
) -> Result<PublicFlights, ApiError> {
    let filters = filters.read()?;
    let area = bbox
        .map(|bbox| {
            parse_bbox(bbox).map_err(|error| {
                ApiError::bad_request(ErrorCode::InvalidBbox, format!("{error}; {BBOX_HELP}"))
            })
        })
        .transpose()?;
    let from = moment("from", from)?;
    let to = moment("to", to)?;
    if from.zip(to).is_some_and(|(from, to)| from >= to) {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidRange,
            "from must lie before to: what is asked for are the flights that started at or \
             after from and before to",
        ));
    }
    Ok(narrowed(
        PublicFlights {
            area,
            from,
            before: to,
            ..PublicFlights::default()
        },
        filters,
    ))
}

/// `GET /v1/flights/public`: one page of everyone's public flights.
///
/// Like a pilot's page, the answer may be kept and is checked before every
/// reuse: a flight that is made private leaves the list at once.
///
/// With `pilot`, the public flights of that pilot only. A pilot who has
/// made no flight public is a pilot nobody has, as on the pilot's page.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let limit = body::page_size(params.limit.as_deref())?;
    let mut which = PublicFlights {
        pilot: api.pilot_asked_for(params.pilot.as_deref()).await?,
        ..which(
            params.bbox.as_deref(),
            params.from.as_deref(),
            params.to.as_deref(),
            &params.filters,
        )?
    };
    let sort = match params.sort.as_deref() {
        None => FlightSort::default(),
        Some(text) => text
            .parse()
            .map_err(|error: venti_api_types::v1::FlightSortError| {
                ApiError::bad_request(ErrorCode::InvalidSort, error.to_string())
            })?,
    };
    if let Some(site) = params.site.as_deref() {
        if which.near.is_some() {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidRequest,
                "site is a place and a distance of its own: send site, or near and radius_km, \
                 and not both",
            ));
        }
        match api.began_at_site(site).await? {
            Some(near) => which.near = Some(near),
            // No flight begins at a site that is no take-off.
            None => {
                let answer = FlightsResponse {
                    flights: Vec::new(),
                    next: None,
                    terms: Some(FlightTerms::current()),
                };
                return Ok(cache::shared_json(&headers, &answer, Keeper::Anyone));
            }
        }
    }
    let answer = api
        .public_flights_page(&which, sort, params.after.as_deref(), limit)
        .await?;
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

impl<S: Store> Api<S> {
    /// A page of everyone's public flights, as `GET /v1/flights/public`
    /// answers it and the connector's `public_flights` does: `which` with
    /// its pilot, in the order of `sort`, behind the cursor `after`.
    /// Counting the read is the caller's.
    pub(crate) async fn public_flights_page(
        &self,
        which: &PublicFlights,
        sort: FlightSort,
        after: Option<&str>,
        limit: usize,
    ) -> Result<FlightsResponse, ApiError> {
        // A cursor belongs to the order it was given out for; one of the other
        // order does not read as one of this.
        let order = match sort {
            FlightSort::Newest => PublicOrder::Newest(after.map(page_key).transpose()?),
            FlightSort::Score => PublicOrder::Best(after.map(score_key).transpose()?),
        };

        // One more than asked for, to know whether a page follows.
        let mut flights = self.store.public_flights(which, &order, limit + 1).await?;
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
        Ok(FlightsResponse {
            flights: self.shown_flights(&flights).await?,
            next,
            terms: Some(FlightTerms::current()),
        })
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChangesParams {
    since: Option<String>,
    pilot: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/flights/public/changes`: what changed in what the public sees
/// of flights since a program last asked, of everyone's or of one
/// pilot's.
///
/// A flight that became public or changed while public is in `flights`,
/// as it is now. A flight that is public no more is in `withdrawn`, by
/// the hash of its address: whoever kept it removes it. A flight that was
/// never public is in neither.
pub(crate) async fn changes<S: Store>(
    State(api): State<Arc<Api<S>>>,
    reader: Reader,
    Query(params): Query<ChangesParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
    let limit = body::page_size(params.limit.as_deref())?;
    let asked: i64 = match params.since.as_deref() {
        None => 0,
        Some(text) => text
            .parse()
            .ok()
            .filter(|since| *since >= 0)
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidCursor,
                    "since must be the cursor of an earlier answer",
                )
            })?,
    };
    // A pilot without a public flight has no page, here as everywhere:
    // whoever followed that pilot removes what they kept of them.
    let pilot = api.pilot_asked_for(params.pilot.as_deref()).await?;
    let pilot = pilot.as_deref();

    let mut changes = api.store.public_flight_changes(pilot, asked, limit).await?;
    // A cursor from beyond the latest change was not given out by this
    // database as it is now. Start again from the beginning, and say so.
    let reset = asked > changes.latest;
    let since = if reset { 0 } else { asked };
    if reset {
        changes = api.store.public_flight_changes(pilot, 0, limit).await?;
    }
    // As in the list: the store hands out public flights only, and a
    // mistake there would show a flight to everyone.
    if changes
        .flights
        .iter()
        .any(|flight| flight.visibility != Visibility::Public)
    {
        static GATE: Sparse = Sparse::new();
        GATE.error("the store listed a flight that is not public among the public changes");
        return Err(ApiError::internal());
    }

    let answer = PublicFlightChangesResponse {
        cursor: changes.last().unwrap_or(since).to_string(),
        flights: api.shown_flights(&changes.flights).await?,
        withdrawn: changes
            .withdrawn
            .iter()
            .map(|withdrawn| WithdrawnFlight {
                id_sha256: address_hash(&withdrawn.id),
                withdrawn_at: withdrawn.withdrawn_at,
            })
            .collect(),
        more: changes.more,
        reset,
        terms: FlightTerms::current(),
    };
    Ok(cache::shared_json(&headers, &answer, Keeper::Anyone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_withdrawn_flight_is_named_by_the_hash_of_its_address() {
        // SHA-256 of the 26 ASCII characters, as any program computes it:
        // `printf 0123456789abcdefghjkmnpqrs | shasum -a 256`.
        let hash = address_hash("0123456789abcdefghjkmnpqrs");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "6aab299c78d0b17077bd60f9ea95ff9c4dc6a4dfa7b3b308d01c7c187cba3709"
        );
    }

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
