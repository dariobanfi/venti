//! What public flights say about where pilots take off: the flights that
//! began at a site, and the places where several pilots took off and no
//! site of any layer stands.
//!
//! "Yes, I started here and it was good" is what a pilot says of a place
//! from pilots ([`super::sites`]). A public flight says the first half of
//! it without anyone typing it, and this is where the server reads that.
//!
//! **Public flights and nothing else.** A flight that is private, shared
//! by link, taken down or deleted is in no count here, no month and no
//! list, and nothing is kept between two answers: a flight that leaves
//! the public is gone from the next one. The store counts public flights
//! only; what it hands back is looked at again here, as the list of
//! everyone's public flights does ([`super::feed`]).
//!
//! **A candidate names a place, not a person.** It is offered only where
//! the public flights of at least [`CANDIDATE_MIN_PILOTS`] different
//! pilots began in one cell of a fixed grid: one pilot's flights say
//! where that pilot takes off, which may be the field behind their house.
//! It carries no pilot and no flight, its position is the middle of the
//! take-offs rounded to [`CANDIDATE_POSITION_STEP_DEG`], and its height is
//! rounded to 10 m. Everything it is made from is already in the list of
//! public flights with its pilot's name; what a candidate adds is the
//! word that pilots take off there, which is why the number of pilots is
//! the guard. `docs/architecture.md` has the rule in its decision log.
//!
//! **The server makes no site of a candidate.** It is something an app
//! shows a pilot who flew there, who adds the site with the place and
//! the height of their own flight, or does not.
//!
//! **What an answer costs** is one question over the index of where
//! public flights took off (`flights_public_takeoff_idx`), and for
//! candidates a second one for the sites near them: nothing is kept in
//! the server's memory and no maintenance step makes anything. A site's
//! answer reads the flights within some hundred metres of one point.
//! Candidates read every public flight of the box, so the box may span
//! [`MAX_CANDIDATE_BOX_DEG`] degrees at most, and an answer may be reused
//! for a minute. Both count as reads of public flights for the address
//! that asks ([`Api::count_public_flight_read`]).

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{
    CANDIDATE_CELL_DEG, CANDIDATE_CLEAR_OF_SITES_M, CANDIDATE_MIN_PILOTS, ErrorCode, FlightTerms,
    MAX_CANDIDATE_BOX_DEG, MAX_SITE_CANDIDATES, SITE_BEST_FLIGHTS, SITE_FLIGHTS_WITHIN_M,
    SiteBestFlight, SiteCandidate, SiteCandidatesResponse, SiteFlightsResponse, parse_bbox,
};
use venti_domain::{BoundingBox, Site, SiteId, SiteUse, SourceId};
use venti_sources::Kind;
use venti_store::{CandidateRule, FlightsFrom, Store, Visibility, solar_day};

use super::auth::Client;
use super::cache::Keeper;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, cache, wire};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

/// Whether a site is one to take off from, by its own record: on foot or
/// on a tow line, or it names no use at all. A landing field and a
/// training slope are not.
fn is_a_takeoff(site: &Site) -> bool {
    site.uses.is_empty()
        || site
            .uses
            .iter()
            .any(|usage| matches!(usage, SiteUse::Takeoff | SiteUse::Towing))
}

impl<S: Store> Api<S> {
    /// The sources this server serves flying sites of.
    pub(super) fn site_sources(&self) -> Vec<SourceId> {
        self.sources
            .iter()
            .filter(|(_, served)| served.kinds.contains(&Kind::Sites))
            .map(|(source, _)| source.clone())
            .collect()
    }

    /// `GET /v1/sites/{id}/flights`: what the public flights that began
    /// within [`SITE_FLIGHTS_WITHIN_M`] of a site say about it. `site` is
    /// the site as it stands, of whichever layer.
    ///
    /// The answer names flights and their pilots, so it is kept as the
    /// list of public flights is: checked with the server before every
    /// reuse, so that a flight made private is gone from it at once.
    pub(super) async fn flights_at(
        &self,
        client: IpAddr,
        headers: &HeaderMap,
        id: SiteId,
        site: &Site,
    ) -> Result<Response, ApiError> {
        self.count_public_flight_read(client)?;
        let answer = self.site_flights(id, site).await?;
        Ok(cache::shared_json(headers, &answer, Keeper::Anyone))
    }

    /// What the public flights that began at a site say about it, as
    /// [`Api::flights_at`] answers it and the connector's `site_flights`
    /// does. Counting the read is the caller's.
    pub(crate) async fn site_flights(
        &self,
        id: SiteId,
        site: &Site,
    ) -> Result<SiteFlightsResponse, ApiError> {
        let takeoff = is_a_takeoff(site);
        let said = if takeoff {
            self.store
                .flights_from(site.position, SITE_FLIGHTS_WITHIN_M, SITE_BEST_FLIGHTS)
                .await?
        } else {
            FlightsFrom {
                flights: 0,
                pilots: 0,
                latest_day: None,
                by_month: [0; 12],
                best: Vec::new(),
            }
        };
        // The store names public flights only. Looking again costs
        // nothing, and a mistake there would show a flight to everyone.
        if said
            .best
            .iter()
            .any(|flight| flight.visibility != Visibility::Public)
        {
            static GATE: Sparse = Sparse::new();
            GATE.error("the store named a flight that is not public among a site's best");
            return Err(ApiError::internal());
        }
        let best = said
            .best
            .iter()
            .map(|flight| {
                let shown = wire::flight(flight)?;
                // A flight is among the best because it scores and was
                // found by where it took off.
                let (Some(score), Some(takeoff)) = (shown.score, flight.derived.takeoff) else {
                    return Err(ApiError::internal());
                };
                Ok(SiteBestFlight {
                    id: shown.id,
                    pilot: shown.pilot,
                    day: solar_day(flight.derived.start, takeoff.position.lon()),
                    score,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        let answer = SiteFlightsResponse {
            id,
            within_m: SITE_FLIGHTS_WITHIN_M,
            takeoff,
            flights: said.flights,
            pilots: said.pilots,
            latest_day: said.latest_day,
            by_month: said.by_month,
            best,
            terms: FlightTerms::current(),
        };
        Ok(answer)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
}

/// `GET /v1/sites/candidates?bbox=west,south,east,north`: the places in a
/// box where pilots take off and no site stands.
///
/// An answer names no pilot and no flight and may be reused for a minute,
/// as the answers about sites that change through the API are: a flight
/// that leaves the public, or a site that is added, shows a minute later
/// at most.
pub(crate) async fn candidates<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_flight_read(client)?;
    let bbox = params.bbox.ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidBbox,
            format!("the bbox parameter is missing; {BBOX_HELP}"),
        )
    })?;
    let area = parse_bbox(&bbox).map_err(|error| {
        ApiError::bad_request(ErrorCode::InvalidBbox, format!("{error}; {BBOX_HELP}"))
    })?;
    let answer = api.candidates_in(&area).await?;
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The candidates in a box, as `GET /v1/sites/candidates` answers them
    /// and the connector's `takeoff_candidates` does: a box of at most
    /// [`MAX_CANDIDATE_BOX_DEG`] each way. Counting the read is the
    /// caller's.
    pub(crate) async fn candidates_in(
        &self,
        area: &BoundingBox,
    ) -> Result<SiteCandidatesResponse, ApiError> {
        if area.north() - area.south() > MAX_CANDIDATE_BOX_DEG
            || area.east() - area.west() > MAX_CANDIDATE_BOX_DEG
        {
            return Err(ApiError::bad_request(
                ErrorCode::BoxTooLarge,
                format!(
                    "candidates are asked for in a box of at most {MAX_CANDIDATE_BOX_DEG} degrees \
                     of latitude and of longitude; ask for it in several smaller boxes"
                ),
            ));
        }

        let rule = CandidateRule {
            min_pilots: CANDIDATE_MIN_PILOTS,
            clear_m: CANDIDATE_CLEAR_OF_SITES_M,
        };
        // One more than an answer carries, to know whether the box holds more.
        let mut found = self
            .store
            .takeoff_candidates(&self.site_sources(), area, &rule, MAX_SITE_CANDIDATES + 1)
            .await?;
        // The store offers no place with fewer pilots. Looking again costs
        // nothing, and a mistake there would point at one pilot's field.
        if found
            .iter()
            .any(|candidate| candidate.pilots < CANDIDATE_MIN_PILOTS)
        {
            static GATE: Sparse = Sparse::new();
            GATE.error("the store offered a candidate with fewer pilots than a candidate has");
            return Err(ApiError::internal());
        }
        let more = found.len() > MAX_SITE_CANDIDATES;
        found.truncate(MAX_SITE_CANDIDATES);
        let answer = SiteCandidatesResponse {
            candidates: found
                .into_iter()
                .map(|candidate| SiteCandidate {
                    position: candidate.position,
                    altitude_m: candidate.altitude_m,
                    flights: candidate.flights,
                    pilots: candidate.pilots,
                    latest_day: candidate.latest_day,
                    by_month: candidate.by_month,
                })
                .collect(),
            more,
            min_pilots: CANDIDATE_MIN_PILOTS,
            cell_deg: CANDIDATE_CELL_DEG,
            clear_of_sites_m: CANDIDATE_CLEAR_OF_SITES_M,
            terms: FlightTerms::current(),
        };
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use venti_api_types::v1::CANDIDATE_POSITION_STEP_DEG;
    use venti_store::{CANDIDATE_STEPS_PER_DEGREE, CELLS_PER_DEGREE};

    use super::*;

    /// The answer says the grid and the rounding in the API's words; the
    /// store works with its own. They are the same numbers.
    #[test]
    #[allow(clippy::float_cmp)]
    fn what_the_answer_says_of_the_grid_is_what_the_store_counts_in() {
        assert_eq!(1.0 / CELLS_PER_DEGREE, CANDIDATE_CELL_DEG);
        assert_eq!(
            1.0 / CANDIDATE_STEPS_PER_DEGREE,
            CANDIDATE_POSITION_STEP_DEG
        );
    }
}
