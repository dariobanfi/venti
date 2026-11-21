//! What public flights say about where pilots take off: the flights that
//! began at a site, and the places where several pilots took off and no
//! site stands.
//!
//! Both are made from flights their pilots made public and from nothing
//! else, anew for every answer: a flight that is made private, shared by
//! link only, taken down or deleted is in no count of the next answer.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, SiteId};

use super::{CONFIRMATION_FLIGHT_WITHIN_M, FlightId, FlightScore, FlightTerms, Pilot};

/// How far from a site a flight may have taken off to count as one that
/// began there, in metres. It is the distance a flight may lie from a
/// site it is named with to confirm it, and the same number on purpose:
/// "this flight began at this site" means one thing on the server.
///
/// A site is one point, a take-off is a meadow, and where a recording
/// first shows the glider flying is some way down the slope. A landing
/// field lies further from its take-off than this almost everywhere; where
/// one does not, [`SiteFlightsResponse::takeoff`] keeps the take-off's
/// flights off the landing field's answer.
pub const SITE_FLIGHTS_WITHIN_M: f64 = CONFIRMATION_FLIGHT_WITHIN_M;

/// How many of a site's best flights its answer names.
pub const SITE_BEST_FLIGHTS: usize = 5;

/// The least number of different pilots whose public flights began at one
/// place for it to be offered as a candidate. One pilot's flights say
/// where that pilot takes off, which may be their own field; two may be a
/// pilot and a friend. Three who each chose to publish a flight from
/// there say that pilots take off there (an estimate, the owner's to
/// change).
pub const CANDIDATE_MIN_PILOTS: u32 = 3;

/// How far from the middle of a candidate's take-offs no site of any
/// layer stands, in metres: within [`SITE_FLIGHTS_WITHIN_M`] of a site, a
/// flight began at that site.
pub const CANDIDATE_CLEAR_OF_SITES_M: f64 = SITE_FLIGHTS_WITHIN_M;

/// The side of a cell of the grid in which a candidate's flights began,
/// in degrees of latitude and of longitude: 278 m from north to south,
/// and 190 m from east to west at 47 degrees north.
pub const CANDIDATE_CELL_DEG: f64 = 0.0025;

/// What a candidate's position is rounded to, in degrees: 11 m from north
/// to south. It names the place, and is no fix of any flight.
pub const CANDIDATE_POSITION_STEP_DEG: f64 = 0.0001;

/// The most candidates one answer carries: those with the most pilots.
/// [`SiteCandidatesResponse::more`] says that the box holds more.
pub const MAX_SITE_CANDIDATES: usize = 200;

/// The most a box that candidates are asked for in may span, in degrees
/// of latitude and of longitude each: some 220 km by 150 km at 47 degrees
/// north. A wider one is refused with
/// [`ErrorCode::BoxTooLarge`](super::ErrorCode::BoxTooLarge): what an
/// answer costs the server grows with the public flights its box holds.
pub const MAX_CANDIDATE_BOX_DEG: f64 = 2.0;

/// The answer of `GET /v1/sites/{id}/flights`: what the public flights
/// that began at a site say about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteFlightsResponse {
    /// The site.
    pub id: SiteId,
    /// How far from the site's position a flight may have taken off to be
    /// counted, in metres: [`SITE_FLIGHTS_WITHIN_M`].
    pub within_m: f64,
    /// Whether the site is one to take off from, by its own record:
    /// `takeoff` or `towing` is among its uses, or it names no use. For a
    /// site that is not (a landing field, a training slope) nothing is
    /// counted: every count is 0 and `best` is empty, since the flights
    /// that began near a landing field are those of a take-off beside it.
    pub takeoff: bool,
    /// How many public flights began there.
    pub flights: u32,
    /// How many pilots flew them.
    pub pilots: u32,
    /// The day of the newest of them, at the site: the day by the sun,
    /// which is the pilot's calendar day wherever the site is. Left out
    /// without a flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_day: Option<NaiveDate>,
    /// How many of them began in each month of the year, January first,
    /// all years together: twelve numbers that add up to `flights`.
    pub by_month: [u32; 12],
    /// The best of them by points, the best first, as
    /// `GET /v1/flights/public?sort=score` orders flights: at most
    /// [`SITE_BEST_FLIGHTS`]. A flight too short to score is not among
    /// them.
    pub best: Vec<SiteBestFlight>,
    /// Under what the flights are given.
    pub terms: FlightTerms,
}

/// One of a site's best public flights: what names it. Everything else of
/// it is at `GET /v1/flights/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteBestFlight {
    /// The flight's address on the server.
    pub id: FlightId,
    /// Its pilot, as the list of everyone's public flights names one.
    pub pilot: Pilot,
    /// The day it was flown, at the site.
    pub day: NaiveDate,
    /// How it scores: the distance and the points.
    pub score: FlightScore,
}

/// The answer of `GET /v1/sites/candidates`: the places in a box where
/// pilots take off and no site of any layer stands.
///
/// A candidate is not a site and the server makes none of one. It is
/// something to show a pilot who flew there: "pilots take off here and no
/// site names it: add it?", with the place and the height from the pilot's
/// own flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteCandidatesResponse {
    /// The candidates: the one with the most pilots first, then the one
    /// with the most flights. At most [`MAX_SITE_CANDIDATES`].
    pub candidates: Vec<SiteCandidate>,
    /// Whether the box holds more candidates than are listed.
    pub more: bool,
    /// The least number of pilots a candidate has:
    /// [`CANDIDATE_MIN_PILOTS`].
    pub min_pilots: u32,
    /// The side of the cell in which a candidate's flights began, in
    /// degrees: [`CANDIDATE_CELL_DEG`].
    pub cell_deg: f64,
    /// How far from a candidate no site stands, in metres:
    /// [`CANDIDATE_CLEAR_OF_SITES_M`].
    pub clear_of_sites_m: f64,
    /// Under what the flights the candidates are made from are given.
    pub terms: FlightTerms,
}

/// A place where the public flights of several pilots began and no site
/// stands. It names no pilot and no flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteCandidate {
    /// The middle of the take-offs, rounded to
    /// [`CANDIDATE_POSITION_STEP_DEG`].
    pub position: GeoPoint,
    /// The mean of the heights the flights took off at, satellite
    /// altitude in metres above mean sea level, rounded to 10 m.
    pub altitude_m: i32,
    /// How many public flights began there.
    pub flights: u32,
    /// How many pilots flew them: at least [`CANDIDATE_MIN_PILOTS`].
    pub pilots: u32,
    /// The day of the newest of them, at the place.
    pub latest_day: NaiveDate,
    /// How many of them began in each month of the year, January first,
    /// all years together.
    pub by_month: [u32; 12],
}
