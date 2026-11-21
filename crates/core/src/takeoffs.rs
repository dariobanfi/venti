//! What public flights say about where pilots take off: the flights that
//! began at a site, the places where several pilots took off and no site
//! stands (candidates), and the offer to add such a place to a pilot who
//! took off there.
//!
//! The server counts (`docs/api.md`, "What public flights say about a
//! site"); the core reads its answers into records with their words, and
//! states two rules of its own once for the three apps:
//!
//! - **The season** of a site or a candidate ([`season_in_words`]): with
//!   fewer than [`SEASON_MIN_FLIGHTS`] flights nothing is said. Otherwise
//!   the shortest run of months in a row, round the turn of the year,
//!   that holds at least three quarters of the flights: one month is
//!   "Flown mostly in July", two to eight are "Flown mostly from May to
//!   August", nine or more "Flown all through the year". Of runs as short
//!   as each other the one with more flights, then the one that begins
//!   earlier in the year.
//! - **Who is offered to add a candidate** ([`Core::candidate_for_pilot`]):
//!   a pilot one of whose own logbook flights began within the answer's
//!   `clear_of_sites_m` (500 m) of it, as [`crate::site_offers`] says that
//!   pilot may add a site, and who has not said "Not this one" of it on
//!   this device.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_api_types::v1::{
    self as api, FlightTerms, MAX_CANDIDATE_BOX_DEG, SiteCandidate, SiteCandidatesResponse,
    SiteFlightsResponse,
};
use venti_domain::{BoundingBox, GeoPoint, SiteId};

use crate::airspace::distance_in_words;
use crate::live::one_line;
use crate::server::{answer, code, refusal};
use crate::site_edit::{invalid, site_draft_new};
use crate::site_offers::{SiteConfirmFlight, adding};
use crate::sites::author_in_words;
use crate::{Coordinate, Core, CoreError, SiteAuthor, SiteDraftView, SiteKind};

/// The least number of flights a season is said for. Fewer say when some
/// pilots happened to fly, not when a site is flown (an estimate, the
/// owner's to change).
pub const SEASON_MIN_FLIGHTS: u32 = 5;

/// The share of the flights the months of a season hold, at least: three
/// quarters, as a numerator over four.
const SEASON_SHARE_QUARTERS: u32 = 3;

/// How many places a pilot's "Not this one" is kept for on the device;
/// the oldest is forgotten first.
const SET_ASIDE_KEPT: usize = 200;

/// The file the places a pilot set aside are kept in.
const SET_ASIDE_FILE: &str = "candidates-set-aside.json";

pub(crate) const MONTHS: [(&str, &str); 12] = [
    ("Jan", "January"),
    ("Feb", "February"),
    ("Mar", "March"),
    ("Apr", "April"),
    ("May", "May"),
    ("Jun", "June"),
    ("Jul", "July"),
    ("Aug", "August"),
    ("Sep", "September"),
    ("Oct", "October"),
    ("Nov", "November"),
    ("Dec", "December"),
];

// --- Records -------------------------------------------------------------------

/// Under what the flights an answer is made from are given: the answer's
/// `terms`, as the server words them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PublicFlightTerms {
    /// The statement, in words for a person: shown as it is.
    #[uniffi(default = "")]
    pub text: String,
    /// The licence the flights are given under, by its name. Absent while
    /// there is none: nothing but reading is granted.
    #[uniffi(default = None)]
    pub licence: Option<String>,
    /// Whether Venti's terms have settled the question. `false`: the
    /// statement is a holding one.
    #[uniffi(default = false)]
    pub decided: bool,
}

/// One month of the year in a row of twelve.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlownMonth {
    /// Its short name: "Jan".
    #[uniffi(default = "")]
    pub name: String,
    /// How many of the flights began in it, all years together.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// Its flights over those of the busiest month, 0 to 1: how tall a
    /// bar is drawn. 0 for every month when nothing was flown.
    #[uniffi(default = 0.0)]
    pub share: f64,
    /// The month in words, for a reader of the screen: "July: 2
    /// flights".
    #[uniffi(default = "")]
    pub words: String,
}

/// One of a site's best public flights, as a row of a list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteBestFlightLine {
    /// The flight's address on the server: for [`crate::public_flight_path`].
    #[uniffi(default = "")]
    pub id: String,
    /// Its pilot, as the list of public flights names one; the name on
    /// one line.
    pub pilot: SiteAuthor,
    /// The day it was flown at the site, `YYYY-MM-DD`.
    #[uniffi(default = "")]
    pub day: String,
    /// The day in words: "Sat 9 May 2026".
    #[uniffi(default = "")]
    pub day_in_words: String,
    /// The scoring distance, metres.
    #[uniffi(default = 0.0)]
    pub distance_m: f64,
    /// The distance as the app writes distances: "12.0 km".
    #[uniffi(default = "")]
    pub distance_words: String,
    /// The points.
    #[uniffi(default = 0.0)]
    pub points: f64,
    /// The points in words: "12.0 points".
    #[uniffi(default = "")]
    pub points_words: String,
    /// The row in a line: "Lena Probe (@lena), Sat 9 May 2026: 12.0 km,
    /// 12.0 points".
    #[uniffi(default = "")]
    pub words: String,
    /// The path of the flight's page on the server: "/f/…".
    #[uniffi(default = "")]
    pub page_path: String,
    /// The path to fetch its file from, to open the flight.
    #[uniffi(default = "")]
    pub file_path: String,
}

/// What the public flights that began at a site say about it: the
/// answer of [`site_flights_path`], read by [`site_flights`].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteFlights {
    /// The site.
    #[uniffi(default = "")]
    pub site_id: String,
    /// Whether the site is one to take off from, by its own record. A
    /// page shows nothing of this answer for one that is not: every
    /// count is 0 and every line empty.
    #[uniffi(default = false)]
    pub is_takeoff: bool,
    /// How many public flights began there.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// How many pilots flew them.
    #[uniffi(default = 0)]
    pub pilots: u32,
    /// The day of the newest, `YYYY-MM-DD`. Absent without a flight.
    #[uniffi(default = None)]
    pub latest_day: Option<String>,
    /// The count line: "14 public flights by 6 pilots began here, last on
    /// 12 Aug 2026"; "1 public flight began here, on 12 Aug 2026". Empty
    /// without a flight.
    #[uniffi(default = "")]
    pub words: String,
    /// What to say of a take-off no public flight began at: "No public
    /// flight began here yet." Empty otherwise, and for a site that is
    /// not a take-off.
    #[uniffi(default = "")]
    pub nobody_words: String,
    /// What is counted, under the count: "Public flights that took off
    /// within 500 m of this site." Empty for a site that is not a
    /// take-off.
    #[uniffi(default = "")]
    pub about: String,
    /// The twelve months, January first. Empty for a site that is not a
    /// take-off.
    #[uniffi(default = [])]
    pub months: Vec<FlownMonth>,
    /// The season in a sentence: "Flown mostly from May to August".
    /// Empty with fewer than [`SEASON_MIN_FLIGHTS`] flights.
    #[uniffi(default = "")]
    pub season_words: String,
    /// The best flights by points, the best first: at most five.
    #[uniffi(default = [])]
    pub best: Vec<SiteBestFlightLine>,
    /// Under what the flights are given.
    pub terms: PublicFlightTerms,
}

/// A place where the public flights of several pilots began and no site
/// stands. It names no pilot and no flight.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteCandidateView {
    /// The middle of the take-offs, rounded to 11 m. It names the place
    /// and is no fix of any flight.
    pub position: Coordinate,
    /// The mean height of the take-offs, metres above sea level, rounded
    /// to 10 m.
    #[uniffi(default = 0)]
    pub altitude_m: i32,
    /// The height in words: "1620 m".
    #[uniffi(default = "")]
    pub altitude_words: String,
    /// How many public flights began there.
    #[uniffi(default = 0)]
    pub flights: u32,
    /// How many pilots flew them: at least three.
    #[uniffi(default = 0)]
    pub pilots: u32,
    /// The day of the newest, `YYYY-MM-DD`.
    #[uniffi(default = "")]
    pub latest_day: String,
    /// The twelve months, January first.
    #[uniffi(default = [])]
    pub months: Vec<FlownMonth>,
    /// The season in a sentence, as for a site. Often empty: a candidate
    /// has few flights.
    #[uniffi(default = "")]
    pub season_words: String,
    /// The place in words: "Pilots take off here: 4 pilots, 9 public
    /// flights, last on 3 Sep 2026. No site names this place."
    #[uniffi(default = "")]
    pub words: String,
}

/// The places in a part of the map where pilots take off and no site
/// stands: the answer of [`site_candidates_path`], read by
/// [`site_candidates`]. A shell hands it back to
/// [`Core::candidate_for_pilot`].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteCandidates {
    /// The places, the one with the most pilots first.
    #[uniffi(default = [])]
    pub candidates: Vec<SiteCandidateView>,
    /// Whether the part of the map holds more than are listed.
    #[uniffi(default = false)]
    pub more: bool,
    /// What to say when it does: "More places lie here than one answer
    /// lists. Zoom in to see them all." Empty otherwise.
    #[uniffi(default = "")]
    pub more_words: String,
    /// How far from a candidate no site stands, metres: the server's
    /// `clear_of_sites_m`, 500.
    #[uniffi(default = 500.0)]
    pub clear_of_sites_m: f64,
    /// Under what the flights they are made from are given.
    pub terms: PublicFlightTerms,
}

/// A candidate offered to a pilot who took off there: the one answer of
/// [`Core::candidate_for_pilot`] for one place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CandidateOffer {
    /// The place. Hand it to [`Core::candidate_set_aside`] for "Not this
    /// one".
    pub candidate: SiteCandidateView,
    /// The pilot's newest flight of the logbook that began there: its
    /// identifier and "Your flight from 11:42, 1 h 05 min".
    pub flight: SiteConfirmFlight,
    /// The day of that flight, the pilot's own, `YYYY-MM-DD`.
    #[uniffi(default = "")]
    pub day: String,
    /// The entry in a sentence: "You took off here on 3 Sep 2026. Add it
    /// as a place pilots take off from?"; for a partner "… Add it as a
    /// site?".
    #[uniffi(default = "")]
    pub words: String,
    /// The button that opens the form with `draft`: "Add this place".
    #[uniffi(default = "")]
    pub add_words: String,
    /// The button that sets the place aside on this device: "Not this
    /// one".
    #[uniffi(default = "")]
    pub dismiss_words: String,
    /// A new site at the pilot's own take-off, with its height and the
    /// use "take-off", for the form of adding a site
    /// ([`crate::site_add_request`]). Not the candidate's rounded place.
    pub draft: SiteDraftView,
}

/// The words around what flights say about a site that are the same in
/// the three apps.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteFlightsWords {
    /// The title over a site's section: "Public flights from here".
    #[uniffi(default = "")]
    pub title: String,
    /// The title over the twelve months: "When it is flown".
    #[uniffi(default = "")]
    pub months_title: String,
    /// The title over the best flights: "The best flights from here".
    #[uniffi(default = "")]
    pub best_title: String,
    /// The switch of the map's layer of candidates: "Take-offs without a
    /// site".
    #[uniffi(default = "")]
    pub layer: String,
    /// What it shows, under it: "Places where the public flights of at
    /// least 3 pilots began and no site stands. …".
    #[uniffi(default = "")]
    pub layer_about: String,
    /// The title of one candidate, on the map or in a card: "Pilots take
    /// off here".
    #[uniffi(default = "")]
    pub candidate_title: String,
}

// --- Words ---------------------------------------------------------------------

/// The words around what flights say about a site.
#[uniffi::export]
pub fn site_flights_words() -> SiteFlightsWords {
    SiteFlightsWords {
        title: "Public flights from here".to_owned(),
        months_title: "When it is flown".to_owned(),
        best_title: "The best flights from here".to_owned(),
        layer: "Take-offs without a site".to_owned(),
        layer_about: format!(
            "Places where the public flights of at least {} pilots began and no site stands. \
             Not checked by anyone: a place on the map is not permission to fly there.",
            api::CANDIDATE_MIN_PILOTS
        ),
        candidate_title: "Pilots take off here".to_owned(),
    }
}

pub(crate) fn terms(terms: &FlightTerms) -> PublicFlightTerms {
    PublicFlightTerms {
        text: terms.text.clone(),
        licence: terms.licence.clone(),
        decided: terms.decided,
    }
}

pub(crate) fn count(number: u32, one: &str, many: &str) -> String {
    if number == 1 {
        format!("1 {one}")
    } else {
        format!("{number} {many}")
    }
}

/// "12 Aug 2026".
fn short_day(day: NaiveDate) -> String {
    day.format("%-d %b %Y").to_string()
}

pub(crate) fn months(by_month: &[u32; 12]) -> Vec<FlownMonth> {
    let most = by_month.iter().copied().max().unwrap_or(0);
    MONTHS
        .iter()
        .zip(by_month)
        .map(|(&(short, long), &flights)| FlownMonth {
            name: short.to_owned(),
            flights,
            share: if most == 0 {
                0.0
            } else {
                f64::from(flights) / f64::from(most)
            },
            words: format!("{long}: {}", count(flights, "flight", "flights")),
        })
        .collect()
}

/// The season of twelve counts by month, by the rule of the module's
/// head. Empty with fewer than [`SEASON_MIN_FLIGHTS`] flights.
pub(crate) fn season_in_words(by_month: &[u32; 12]) -> String {
    let total: u32 = by_month.iter().sum();
    if total < SEASON_MIN_FLIGHTS {
        return String::new();
    }
    let run = |first: usize, length: usize| -> u32 {
        (0..length).map(|step| by_month[(first + step) % 12]).sum()
    };
    let enough = |flights: u32| flights * 4 >= total * SEASON_SHARE_QUARTERS;
    let Some((length, first)) = (1..=12).find_map(|length| {
        (0..12)
            .filter(|&first| enough(run(first, length)))
            // The most flights, then the earliest first month.
            .max_by_key(|&first| (run(first, length), std::cmp::Reverse(first)))
            .map(|first| (length, first))
    }) else {
        return String::new();
    };
    let name = |month: usize| MONTHS[month % 12].1;
    match length {
        1 => format!("Flown mostly in {}", name(first)),
        2..=8 => format!(
            "Flown mostly from {} to {}",
            name(first),
            name(first + length - 1)
        ),
        _ => "Flown all through the year".to_owned(),
    }
}

/// A refusal of a read of public flights, in words.
fn flights_refusal(status: u16, body: &[u8]) -> CoreError {
    let message = match (status, code(body).as_deref()) {
        (429, _) | (_, Some("rate_limited")) => {
            "Public flights were read from here as often as one may in an hour. Try again later."
                .to_owned()
        }
        (_, Some("site_not_found")) => "There is no such site.".to_owned(),
        (_, Some("site_gone")) => "This site was removed: it is on no map.".to_owned(),
        (_, Some("invalid_bbox")) => {
            "The app could not ask the server about this part of the map.".to_owned()
        }
        (_, Some("box_too_large")) => ZOOM_IN.to_owned(),
        _ => return refusal(status, body),
    };
    CoreError::Server { message }
}

fn read<T: serde::de::DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, CoreError> {
    if (200..300).contains(&status) {
        answer(status, body)
    } else {
        Err(flights_refusal(status, body))
    }
}

const ZOOM_IN: &str = "Zoom in to see the places where pilots take off and no site stands.";

// --- A site's flights ------------------------------------------------------------

/// The path to `GET` for what the public flights that began at a site say
/// about it. The site is one of any layer: a source's, the partners' or
/// the crowdsourced one. No token.
///
/// # Errors
///
/// An identifier that is not a site's.
#[uniffi::export]
pub fn site_flights_path(id: String) -> Result<String, CoreError> {
    let id: SiteId = id
        .parse()
        .map_err(|_| invalid("This is not a site the app knows."))?;
    Ok(api::site_flights_path(&id))
}

/// Reads the server's answer to [`site_flights_path`] for a site's page.
///
/// # Errors
///
/// The server's refusal in words: there is no such site, it was removed,
/// or this address read public flights as often as one may in an hour.
#[uniffi::export]
pub fn site_flights(status: u16, body: Vec<u8>) -> Result<SiteFlights, CoreError> {
    let response: SiteFlightsResponse = read(status, &body)?;
    let takeoff = response.takeoff;
    let flown = takeoff && response.flights > 0;
    let words = match (flown, response.latest_day) {
        (true, Some(day)) if response.flights == 1 => {
            format!("1 public flight began here, on {}", short_day(day))
        }
        (true, Some(day)) => format!(
            "{} by {} began here, last on {}",
            count(response.flights, "public flight", "public flights"),
            count(response.pilots, "pilot", "pilots"),
            short_day(day)
        ),
        (true, None) => format!(
            "{} by {} began here",
            count(response.flights, "public flight", "public flights"),
            count(response.pilots, "pilot", "pilots"),
        ),
        (false, _) => String::new(),
    };
    let best = if takeoff {
        response.best.iter().map(best_line).collect()
    } else {
        Vec::new()
    };
    Ok(SiteFlights {
        site_id: response.id.to_string(),
        is_takeoff: takeoff,
        flights: if takeoff { response.flights } else { 0 },
        pilots: if takeoff { response.pilots } else { 0 },
        latest_day: response
            .latest_day
            .filter(|_| flown)
            .map(|day| day.format("%Y-%m-%d").to_string()),
        words,
        nobody_words: if takeoff && !flown {
            "No public flight began here yet.".to_owned()
        } else {
            String::new()
        },
        about: if takeoff {
            format!(
                "Public flights that took off within {:.0} m of this site.",
                response.within_m
            )
        } else {
            String::new()
        },
        months: if takeoff {
            months(&response.by_month)
        } else {
            Vec::new()
        },
        season_words: if takeoff {
            season_in_words(&response.by_month)
        } else {
            String::new()
        },
        best,
        terms: terms(&response.terms),
    })
}

fn best_line(flight: &api::SiteBestFlight) -> SiteBestFlightLine {
    let pilot = SiteAuthor {
        handle: flight.pilot.handle.to_string(),
        name: one_line(&flight.pilot.display_name, 40),
    };
    let day_in_words = flight.day.format("%a %-d %b %Y").to_string();
    let distance_words = distance_in_words(flight.score.distance_m);
    let points_words = format!("{:.1} points", flight.score.points);
    SiteBestFlightLine {
        id: flight.id.to_string(),
        words: format!(
            "{}, {day_in_words}: {distance_words}, {points_words}",
            author_in_words(&pilot)
        ),
        pilot,
        day: flight.day.format("%Y-%m-%d").to_string(),
        day_in_words,
        distance_m: flight.score.distance_m,
        distance_words,
        points: flight.score.points,
        points_words,
        page_path: api::share_path(&flight.id),
        file_path: api::flight_file_path(&flight.id),
    }
}

// --- Candidates ------------------------------------------------------------------

/// The path and query to `GET` for the places in a part of the map where
/// pilots take off and no site stands. No token.
///
/// # Errors
///
/// A part of the map wider or taller than the server answers for (2
/// degrees), in words to show over the map: "Zoom in to see the places
/// where pilots take off and no site stands."
#[uniffi::export]
pub fn site_candidates_path(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
) -> Result<String, CoreError> {
    let (south, west, north, east) = (
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    );
    let bbox = BoundingBox::new(south, west, north, east).map_err(|_| CoreError::Server {
        message: "The app could not ask the server about this part of the map.".to_owned(),
    })?;
    if north - south > MAX_CANDIDATE_BOX_DEG || east - west > MAX_CANDIDATE_BOX_DEG {
        return Err(CoreError::Server {
            message: ZOOM_IN.to_owned(),
        });
    }
    Ok(api::site_candidates_path(&bbox))
}

/// Reads the server's answer to [`site_candidates_path`].
///
/// # Errors
///
/// The server's refusal in words: the part of the map is too large, or
/// this address read public flights as often as one may in an hour.
#[uniffi::export]
pub fn site_candidates(status: u16, body: Vec<u8>) -> Result<SiteCandidates, CoreError> {
    let response: SiteCandidatesResponse = read(status, &body)?;
    Ok(SiteCandidates {
        candidates: response.candidates.iter().map(candidate).collect(),
        more: response.more,
        more_words: if response.more {
            "More places lie here than one answer lists. Zoom in to see them all.".to_owned()
        } else {
            String::new()
        },
        clear_of_sites_m: response.clear_of_sites_m,
        terms: terms(&response.terms),
    })
}

fn candidate(place: &SiteCandidate) -> SiteCandidateView {
    SiteCandidateView {
        position: Coordinate {
            lat: place.position.lat(),
            lon: place.position.lon(),
        },
        altitude_m: place.altitude_m,
        altitude_words: format!("{} m", place.altitude_m),
        flights: place.flights,
        pilots: place.pilots,
        latest_day: place.latest_day.format("%Y-%m-%d").to_string(),
        months: months(&place.by_month),
        season_words: season_in_words(&place.by_month),
        words: format!(
            "Pilots take off here: {}, {}, last on {}. No site names this place.",
            count(place.pilots, "pilot", "pilots"),
            count(place.flights, "public flight", "public flights"),
            short_day(place.latest_day)
        ),
    }
}

// --- "Add this place" ----------------------------------------------------------

/// The places a pilot set aside, as the device keeps them.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SetAside {
    /// Latitude and longitude, the newest last.
    places: Vec<[f64; 2]>,
}

fn storage_error(error: std::io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The places set aside could not be read or written: {error}."),
    }
}

fn near(a: Coordinate, b: Coordinate, within_m: f64) -> bool {
    match (GeoPoint::new(a.lat, a.lon), GeoPoint::new(b.lat, b.lon)) {
        (Ok(a), Ok(b)) => a.distance_m(b) <= within_m,
        _ => false,
    }
}

#[uniffi::export]
impl Core {
    /// The candidates of an answer that the pilot of this device took off
    /// at, each with the pilot's newest flight from there, the entry's
    /// words and a draft for adding the place at the pilot's own take-off.
    ///
    /// A flight began at a candidate when the start of its track lies
    /// within the answer's `clear_of_sites_m` (500 m) of it: the
    /// distance within which, on the server, a flight began at a site,
    /// and within which no site stands. Its take-off and height are the
    /// flight's analysis; where the analysis finds no take-off, the start
    /// of the track and no height. A flight's file that the storage
    /// cannot give fails the call, as [`Core::flight_analysis`] does: in
    /// the browser, the page fetches it and asks again.
    ///
    /// Empty where this pilot may add no site ([`Core::site_adding`]:
    /// signed out, suspended, or a server that takes no places from
    /// pilots), and for a place set aside with
    /// [`Core::candidate_set_aside`]. `takes_sites_from_pilots` is what
    /// the server's status says. `utc_offset_s` is how far the device's
    /// clock is ahead of UTC, in seconds, for the day and the hour of
    /// the pilot's flight. A phone shows nothing to a partner, as it
    /// shows them no entry of [`Core::site_adding`].
    pub fn candidate_for_pilot(
        &self,
        candidates: SiteCandidates,
        takes_sites_from_pilots: bool,
        utc_offset_s: i32,
    ) -> Result<Vec<CandidateOffer>, CoreError> {
        let adding = adding(&self.site_actor()?, takes_sites_from_pilots);
        if !adding.may_add || candidates.candidates.is_empty() {
            return Ok(Vec::new());
        }
        let set_aside = self.set_aside()?;
        let within_m = candidates.clear_of_sites_m;
        // The newest first.
        let flights = self.logbook.flights();
        let mut offers = Vec::new();
        for place in candidates.candidates {
            if set_aside
                .places
                .iter()
                .any(|&[lat, lon]| near(Coordinate { lat, lon }, place.position, within_m))
            {
                continue;
            }
            let Some((id, summary)) = flights.iter().find(|(_, summary)| {
                summary.outline.first().is_some_and(|&[lat, lon]| {
                    near(Coordinate { lat, lon }, place.position, within_m)
                })
            }) else {
                continue;
            };
            let Some(began) = chrono::DateTime::from_timestamp(
                summary.start_unix_s.saturating_add(i64::from(utc_offset_s)),
                0,
            ) else {
                continue;
            };
            let [lat, lon] = summary.outline[0];
            let text = self.logbook.flight_text(id)?;
            let takeoff = crate::flight_analysis_from_file(text)
                .ok()
                .and_then(|analysis| analysis.takeoff);
            let draft = SiteDraftView {
                elevation_m: takeoff.as_ref().map(|takeoff| takeoff.altitude_m.round()),
                uses: vec![SiteKind::Takeoff],
                ..site_draft_new(
                    takeoff.map_or(Coordinate { lat, lon }, |takeoff| takeoff.position),
                )
            };
            let minutes = summary.duration_s.max(0) / 60;
            let day = began.date_naive();
            offers.push(CandidateOffer {
                flight: SiteConfirmFlight {
                    id: id.clone(),
                    words: format!(
                        "Your flight from {}, {} h {:02} min",
                        began.format("%H:%M"),
                        minutes / 60,
                        minutes % 60
                    ),
                },
                day: day.format("%Y-%m-%d").to_string(),
                words: format!(
                    "You took off here on {}. Add it as {}?",
                    short_day(day),
                    if adding.as_partner {
                        "a site"
                    } else {
                        "a place pilots take off from"
                    }
                ),
                add_words: "Add this place".to_owned(),
                dismiss_words: "Not this one".to_owned(),
                draft,
                candidate: place,
            });
        }
        Ok(offers)
    }

    /// Says "Not this one" of a candidate on this device: no answer
    /// offers it, or a candidate within 500 m of it, to this pilot
    /// again. The device keeps the newest 200 places; nothing goes to the
    /// server, which keeps no candidate.
    pub fn candidate_set_aside(&self, position: Coordinate) -> Result<(), CoreError> {
        let mut set_aside = self.set_aside()?;
        set_aside.places.push([position.lat, position.lon]);
        let extra = set_aside.places.len().saturating_sub(SET_ASIDE_KEPT);
        set_aside.places.drain(..extra);
        let bytes = serde_json::to_vec(&set_aside).map_err(|error| CoreError::Storage {
            message: format!("The places set aside could not be written: {error}."),
        })?;
        self.storage
            .write(SET_ASIDE_FILE, &bytes)
            .map_err(storage_error)
    }
}

impl Core {
    fn set_aside(&self) -> Result<SetAside, CoreError> {
        let Some(bytes) = self.storage.read(SET_ASIDE_FILE).map_err(storage_error)? else {
            return Ok(SetAside::default());
        };
        // The file is the app's own writing. Should it be damaged, nothing
        // is set aside.
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempFolder;

    fn golden(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    fn edited(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut answer: serde_json::Value = serde_json::from_slice(&golden(name)).unwrap();
        edit(&mut answer);
        answer.to_string().into_bytes()
    }

    fn words(result: Result<impl std::fmt::Debug, CoreError>) -> String {
        match result {
            Err(CoreError::Server { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    fn refused(code: &str) -> Vec<u8> {
        format!(r#"{{"error":{{"code":"{code}","message":"for developers"}}}}"#).into_bytes()
    }

    #[test]
    fn a_sites_flights_are_a_count_twelve_months_and_the_best_in_lines() {
        assert_eq!(
            site_flights_path("osm-free-flying:node/5964676196".to_owned()).unwrap(),
            "/v1/sites/osm-free-flying:node/5964676196/flights"
        );
        assert!(site_flights_path("no site".to_owned()).is_err());

        let read = site_flights(200, golden("site_flights.json")).unwrap();
        assert_eq!(read.site_id, "venti:0000000000000000000000s001");
        assert!(read.is_takeoff);
        assert_eq!((read.flights, read.pilots), (3, 2));
        assert_eq!(read.latest_day.as_deref(), Some("2026-07-11"));
        assert_eq!(
            read.words,
            "3 public flights by 2 pilots began here, last on 11 Jul 2026"
        );
        assert_eq!(read.nobody_words, "");
        assert_eq!(
            read.about,
            "Public flights that took off within 500 m of this site."
        );
        let months: Vec<(&str, u32, f64)> = read
            .months
            .iter()
            .map(|month| (month.name.as_str(), month.flights, month.share))
            .collect();
        assert_eq!(months.len(), 12);
        assert_eq!(months[0], ("Jan", 0, 0.0));
        assert_eq!(months[4], ("May", 1, 0.5));
        assert_eq!(months[6], ("Jul", 2, 1.0));
        assert_eq!(read.months[6].words, "July: 2 flights");
        assert_eq!(read.months[4].words, "May: 1 flight");
        // Three flights are too few for a season.
        assert_eq!(read.season_words, "");

        assert_eq!(read.best.len(), 3);
        let best = &read.best[0];
        assert_eq!(best.id, "ww5qkxhxaedefvypb3wy3v6m3m");
        assert_eq!(best.pilot.handle, "lena");
        assert_eq!(best.pilot.name, "Lena Probe");
        assert_eq!(best.day, "2026-05-09");
        assert_eq!(best.day_in_words, "Sat 9 May 2026");
        assert_eq!(best.distance_words, "12.0 km");
        assert_eq!(best.points_words, "12.0 points");
        assert_eq!(
            best.words,
            "Lena Probe (@lena), Sat 9 May 2026: 12.0 km, 12.0 points"
        );
        assert_eq!(best.page_path, "/f/ww5qkxhxaedefvypb3wy3v6m3m");
        assert!(best.file_path.contains("ww5qkxhxaedefvypb3wy3v6m3m"));
        assert_eq!(read.best[2].day, "2026-07-11");

        assert!(
            read.terms
                .text
                .starts_with("These flights are their pilots'.")
        );
        assert_eq!(read.terms.licence, None);
        assert!(!read.terms.decided);
    }

    #[test]
    fn one_flight_none_and_a_site_that_is_no_take_off_have_their_own_words() {
        let one = site_flights(
            200,
            edited("site_flights.json", |answer| {
                answer["flights"] = 1.into();
                answer["pilots"] = 1.into();
            }),
        )
        .unwrap();
        assert_eq!(one.words, "1 public flight began here, on 11 Jul 2026");

        let none = |takeoff: bool| {
            site_flights(
                200,
                edited("site_flights.json", |answer| {
                    answer["takeoff"] = takeoff.into();
                    answer["flights"] = 0.into();
                    answer["pilots"] = 0.into();
                    answer["by_month"] = serde_json::to_value([0_u32; 12]).unwrap();
                    answer["best"] = serde_json::json!([]);
                    answer.as_object_mut().unwrap().remove("latest_day");
                }),
            )
            .unwrap()
        };
        let quiet = none(true);
        assert_eq!(quiet.words, "");
        assert_eq!(quiet.nobody_words, "No public flight began here yet.");
        assert_eq!(quiet.latest_day, None);
        assert_eq!(quiet.months.len(), 12);
        assert!(quiet.months.iter().all(|month| month.share == 0.0));

        let landing = none(false);
        assert!(!landing.is_takeoff);
        assert_eq!(
            (
                landing.words.as_str(),
                landing.nobody_words.as_str(),
                landing.about.as_str()
            ),
            ("", "", "")
        );
        assert!(landing.months.is_empty() && landing.best.is_empty());
    }

    #[test]
    fn refusals_of_public_flights_are_in_words() {
        assert_eq!(
            words(site_flights(404, refused("site_not_found"))),
            "There is no such site."
        );
        assert_eq!(
            words(site_flights(410, refused("site_gone"))),
            "This site was removed: it is on no map."
        );
        assert!(
            words(site_flights(429, refused("rate_limited")))
                .starts_with("Public flights were read from here as often as one may")
        );
        assert_eq!(
            words(site_candidates(400, refused("box_too_large"))),
            ZOOM_IN
        );
        assert_eq!(
            words(site_candidates(400, refused("invalid_bbox"))),
            "The app could not ask the server about this part of the map."
        );
        assert!(words(site_candidates(200, b"{}".to_vec())).contains("could not be read"));
    }

    #[test]
    fn the_season_is_the_shortest_run_of_months_with_three_quarters_of_the_flights() {
        let season = |by_month: [u32; 12]| season_in_words(&by_month);
        assert_eq!(
            season([0, 0, 0, 0, 3, 5, 8, 6, 1, 0, 0, 0]),
            "Flown mostly from June to August"
        );
        assert_eq!(
            season([0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0]),
            "Flown mostly in July"
        );
        // Round the turn of the year, as south of the equator.
        assert_eq!(
            season([4, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3]),
            "Flown mostly from December to February"
        );
        assert_eq!(season([1; 12]), "Flown all through the year");
        // Of two runs as short, the one with more flights, then the
        // earlier one.
        assert_eq!(
            season([3, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0]),
            "Flown mostly from January to April"
        );
        assert_eq!(
            season([0, 0, 5, 0, 0, 0, 0, 0, 5, 0, 0, 0]),
            "Flown mostly from March to September"
        );
        assert_eq!(season([0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0]), "");
    }

    #[test]
    fn candidates_are_places_in_words_from_one_answer() {
        assert_eq!(
            site_candidates_path(47.25, 10.92, 47.27, 10.95).unwrap(),
            "/v1/sites/candidates?bbox=10.92,47.25,10.95,47.27"
        );
        assert_eq!(words(site_candidates_path(46.0, 10.0, 48.5, 11.0)), ZOOM_IN);
        assert!(site_candidates_path(48.0, 10.0, 47.0, 11.0).is_err());

        let read = site_candidates(200, golden("site_candidates.json")).unwrap();
        assert!(!read.more);
        assert_eq!(read.more_words, "");
        assert_eq!(read.clear_of_sites_m, 500.0);
        assert!(!read.terms.decided);
        assert_eq!(read.candidates.len(), 1);
        let place = &read.candidates[0];
        assert_eq!(
            place.position,
            Coordinate {
                lat: 47.2612,
                lon: 10.9312
            }
        );
        assert_eq!(
            (place.altitude_m, place.altitude_words.as_str()),
            (1620, "1620 m")
        );
        assert_eq!((place.flights, place.pilots), (3, 3));
        assert_eq!(place.latest_day, "2026-06-07");
        assert_eq!(place.months[5].flights, 2);
        assert_eq!(place.season_words, "");
        assert_eq!(
            place.words,
            "Pilots take off here: 3 pilots, 3 public flights, last on 7 Jun 2026. No site \
             names this place."
        );

        let more = site_candidates(
            200,
            edited("site_candidates.json", |answer| {
                answer["more"] = true.into()
            }),
        )
        .unwrap();
        assert!(more.more_words.starts_with("More places lie here"));
    }

    #[test]
    fn a_pilot_who_took_off_at_a_candidate_is_offered_to_add_it_once() {
        use venti_flight::igc;
        use venti_flight::testing::synthetic_flight;

        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let id = core
            .import_flight(igc::write(&synthetic_flight()))
            .unwrap()
            .id;
        let (_, summary) = core
            .logbook
            .flights()
            .into_iter()
            .find(|(one, _)| *one == id)
            .unwrap();
        let [lat, lon] = summary.outline[0];
        let at = |north_m: f64| {
            let mut answer = site_candidates(200, golden("site_candidates.json")).unwrap();
            answer.candidates[0].position = Coordinate {
                lat: lat + north_m / 111_320.0,
                lon,
            };
            answer
        };

        // Signed out: nothing to offer.
        assert_eq!(core.candidate_for_pilot(at(100.0), true, 0).unwrap(), []);

        core.signed_in(
            "http://127.0.0.1:8080".to_owned(),
            201,
            golden("sign_in.json"),
        )
        .unwrap();
        // The development mode's pilot is a partner; first as any pilot.
        let path = folder.path().join("account.json");
        let partner = |edits_sites: bool| {
            let mut stored: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            stored["edits_sites"] = edits_sites.into();
            std::fs::write(&path, stored.to_string()).unwrap();
        };
        partner(false);
        // A server that takes no places from pilots, and a place too far.
        assert_eq!(core.candidate_for_pilot(at(100.0), false, 0).unwrap(), []);
        assert_eq!(core.candidate_for_pilot(at(600.0), true, 0).unwrap(), []);

        let offers = core.candidate_for_pilot(at(100.0), true, 0).unwrap();
        assert_eq!(offers.len(), 1);
        let offer = &offers[0];
        let began = chrono::DateTime::from_timestamp(summary.start_unix_s, 0).unwrap();
        assert_eq!(offer.flight.id, id);
        assert!(
            offer
                .flight
                .words
                .starts_with(&format!("Your flight from {}", began.format("%H:%M")))
        );
        assert_eq!(offer.day, began.format("%Y-%m-%d").to_string());
        assert_eq!(
            offer.words,
            format!(
                "You took off here on {}. Add it as a place pilots take off from?",
                began.format("%-d %b %Y")
            )
        );
        assert_eq!(offer.add_words, "Add this place");
        assert_eq!(offer.dismiss_words, "Not this one");
        // The draft is at the pilot's own take-off, with its height.
        let analysis = core.flight_analysis(id.clone()).unwrap();
        let takeoff = analysis.takeoff.unwrap();
        assert_eq!(offer.draft.position, takeoff.position);
        assert_eq!(offer.draft.elevation_m, Some(takeoff.altitude_m.round()));
        assert_eq!(offer.draft.uses, [SiteKind::Takeoff]);
        assert_eq!(offer.draft.site_id, None);
        assert_ne!(offer.draft.position, offer.candidate.position);

        // A partner adds a site.
        partner(true);
        let offers = core.candidate_for_pilot(at(100.0), false, 0).unwrap();
        assert!(offers[0].words.ends_with("Add it as a site?"));

        // "Not this one" holds for the place and its neighbours, on this
        // device, after the app starts again.
        core.candidate_set_aside(at(100.0).candidates[0].position)
            .unwrap();
        assert_eq!(core.candidate_for_pilot(at(100.0), true, 0).unwrap(), []);
        assert_eq!(core.candidate_for_pilot(at(300.0), true, 0).unwrap(), []);
        drop(core);
        let again = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        assert_eq!(again.candidate_for_pilot(at(100.0), true, 0).unwrap(), []);
    }
}
