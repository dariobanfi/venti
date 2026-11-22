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
//! that asks, or the token ([`Api::count_public_read`]).

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use venti_api_types::v1::{
    self, CANDIDATE_CELL_DEG, CANDIDATE_CLEAR_OF_SITES_M, CANDIDATE_MIN_PILOTS, ErrorCode,
    FlightSite, FlightTerms, MAX_CANDIDATE_BOX_DEG, MAX_SITE_CANDIDATES, SITE_BEST_FLIGHTS,
    SITE_FLIGHTS_WITHIN_M, SiteBestFlight, SiteCandidate, SiteCandidatesResponse,
    SiteFlightsResponse, TAKEOFF_SITE_WITHIN_M, parse_bbox,
};
use venti_domain::{BoundingBox, GeoPoint, Site, SiteId, SiteUse, SourceId};
use venti_sources::Kind;
use venti_store::{CandidateRule, Flight, FlightsFrom, Near, Store, Visibility, solar_day};

use crate::pilot_sites::Layer;

use super::budget::Reader;
use super::cache::Keeper;
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, cache, wire};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

/// The site a flight took off from, of the standing sites near where it
/// took off ([`TAKEOFF_SITE_WITHIN_M`]): of those that are take-offs and
/// have a name, a partner's (`venti`) before a place from pilots
/// (`venti-pilots`) before a source's (OpenStreetMap's), and of one
/// layer the nearest; of two as near, the smaller identifier.
/// It comes with where the site stands.
fn takeoff_site(takeoff: GeoPoint, near: &[Site]) -> Option<(FlightSite, GeoPoint)> {
    let layer = |site: &Site| match Layer::of(&site.id) {
        Some(Layer::Partners) => 0,
        Some(Layer::Pilots) => 1,
        None => 2,
    };
    near.iter()
        .filter(|site| is_a_takeoff(site))
        .filter_map(|site| {
            let name = site.name.as_deref().map(str::trim)?;
            (!name.is_empty()).then_some((site, name))
        })
        .min_by(|(a, _), (b, _)| {
            layer(a)
                .cmp(&layer(b))
                .then(
                    a.position
                        .distance_m(takeoff)
                        .total_cmp(&b.position.distance_m(takeoff)),
                )
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|(site, name)| {
            (
                FlightSite {
                    id: site.id.clone(),
                    name: name.to_owned(),
                },
                site.position,
            )
        })
}

/// Whether a site is one to take off from, by its own record: on foot or
/// on a tow line, or it names no use at all. A landing field and a
/// training slope are not.
pub(super) fn is_a_takeoff(site: &Site) -> bool {
    site.uses.is_empty()
        || site
            .uses
            .iter()
            .any(|usage| matches!(usage, SiteUse::Takeoff | SiteUse::Towing))
}

impl<S: Store> Api<S> {
    /// Flights as an answer shows them, each with the site it took off
    /// from ([`takeoff_site`]). The sites are looked up for the whole
    /// page in one question to the store, as they stand now.
    pub(crate) async fn shown_flights(
        &self,
        flights: &[Flight],
    ) -> Result<Vec<v1::Flight>, ApiError> {
        let mut shown = wire::flights(flights)?;
        let takeoffs: Vec<(usize, GeoPoint)> = flights
            .iter()
            .enumerate()
            .filter_map(|(at, flight)| Some((at, flight.derived.takeoff?.position)))
            .collect();
        if takeoffs.is_empty() {
            return Ok(shown);
        }
        let places: Vec<GeoPoint> = takeoffs.iter().map(|&(_, place)| place).collect();
        let sites = self.takeoff_sites(&places).await?;
        for ((at, _), site) in takeoffs.into_iter().zip(sites) {
            shown[at].takeoff_site = site;
        }
        Ok(shown)
    }

    /// The site each of some take-offs belongs to ([`takeoff_site`]), in
    /// their order, looked up in one question to the store.
    pub(crate) async fn takeoff_sites(
        &self,
        places: &[GeoPoint],
    ) -> Result<Vec<Option<FlightSite>>, ApiError> {
        Ok(self
            .placed_takeoff_sites(places)
            .await?
            .into_iter()
            .map(|placed| placed.map(|(site, _)| site))
            .collect())
    }

    /// [`Api::takeoff_sites`], each with where the site stands.
    pub(crate) async fn placed_takeoff_sites(
        &self,
        places: &[GeoPoint],
    ) -> Result<Vec<Option<(FlightSite, GeoPoint)>>, ApiError> {
        if places.is_empty() {
            return Ok(Vec::new());
        }
        let near = self
            .store
            .sites_near(&self.site_sources(), places, TAKEOFF_SITE_WITHIN_M)
            .await?;
        Ok(places
            .iter()
            .zip(near)
            .map(|(&place, sites)| takeoff_site(place, &sites))
            .collect())
    }

    /// One flight as an answer shows it: [`Api::shown_flights`].
    pub(crate) async fn shown_flight(&self, flight: &Flight) -> Result<v1::Flight, ApiError> {
        self.shown_flights(std::slice::from_ref(flight))
            .await?
            .pop()
            .ok_or_else(ApiError::internal)
    }

    /// Where the public flights that began at a site took off, for a list
    /// asked for by `site`: within [`SITE_FLIGHTS_WITHIN_M`] of it, as
    /// `GET /v1/sites/{id}/flights` counts them. `None` for a site that
    /// is no take-off, from which no flight began.
    pub(crate) async fn began_at_site(&self, id: &str) -> Result<Option<Near>, ApiError> {
        let id: SiteId = id.parse().map_err(|_| {
            ApiError::missing(
                ErrorCode::SiteNotFound,
                "there is no site with this identifier",
            )
        })?;
        let site = self.site_of_any_layer(&id).await?;
        Ok(is_a_takeoff(&site).then_some(Near {
            place: site.position,
            within_m: SITE_FLIGHTS_WITHIN_M,
        }))
    }

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
        reader: &Reader,
        headers: &HeaderMap,
        id: SiteId,
        site: &Site,
    ) -> Result<Response, ApiError> {
        self.count_public_read(reader)?;
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
    reader: Reader,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    api.count_public_read(&reader)?;
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

    fn site(id: &str, name: Option<&str>, lat: f64, uses: &[&str]) -> Site {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": name,
            "position": {"lat": lat, "lon": 11.0},
            "uses": uses,
        }))
        .unwrap()
    }

    /// Of the take-offs near a flight's, a partner's comes before a place
    /// from pilots, which comes before a source's; of one layer the
    /// nearest. A landing field and a site without a name are none.
    #[test]
    fn a_flight_takes_off_from_the_partners_site_before_the_nearer_others() {
        let takeoff = GeoPoint::new(47.0, 11.0).unwrap();
        let osm = site("osm:node/1", Some("Musterkogel"), 47.0, &["takeoff"]);
        let pilots_far = site("venti-pilots:b", Some("Musterkogel Ost"), 47.003, &[]);
        let pilots_near = site(
            "venti-pilots:a",
            Some("Musterkogel West"),
            47.001,
            &["takeoff"],
        );
        let partners = site("venti:c", Some("Musterkogel Nord"), 47.004, &["towing"]);
        let landing = site("venti:d", Some("Landeplatz"), 47.0, &["landing"]);
        let nameless = site("venti:e", None, 47.0, &["takeoff"]);
        let named = |sites: &[Site]| takeoff_site(takeoff, sites).map(|(site, _)| site.name);

        let all = [
            osm.clone(),
            pilots_far.clone(),
            pilots_near.clone(),
            partners,
            landing.clone(),
            nameless.clone(),
        ];
        assert_eq!(named(&all).as_deref(), Some("Musterkogel Nord"));
        assert_eq!(
            named(&all[..3]).as_deref(),
            Some("Musterkogel West"),
            "of one layer the nearest"
        );
        assert_eq!(
            named(&[osm.clone(), landing.clone()]).as_deref(),
            Some("Musterkogel")
        );
        assert_eq!(named(&[landing, nameless]), None);
        assert_eq!(
            takeoff_site(takeoff, &[osm])
                .map(|(site, _)| site.id.to_string())
                .as_deref(),
            Some("osm:node/1")
        );
    }

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
