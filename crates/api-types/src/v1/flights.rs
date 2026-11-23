//! A pilot's flights as the server keeps them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, SiteId, Timestamp};

use super::Handle;

/// The largest flight file the server accepts, in bytes.
///
/// A fix is a line of 37 to about 55 bytes, so twelve hours at one fix a
/// second, longer than any paraglider has stayed up, come to between 1.6
/// and 2.4 MB. Four are room for that and for whatever else a recorder
/// writes, and little enough that reading one cannot strain the server.
pub const MAX_FLIGHT_FILE_BYTES: usize = 4 * 1024 * 1024;

/// How many flights a list holds when the request names no limit.
pub const DEFAULT_PAGE_SIZE: usize = 50;

/// The most flights one request may ask for.
pub const MAX_PAGE_SIZE: usize = 100;

/// The address of one flight on the server: 26 lower-case letters and
/// digits, as in `/v1/flights/{id}` and in a share link.
///
/// It is not the flight's identity. Two accounts that upload the same
/// recording hold two flights with two addresses, and an address cannot be
/// worked out from the file, so a link to an unlisted flight is known only
/// to those it was given to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FlightId(String);

/// Text that does not have the form of a flight's address.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a flight's address is 26 lower-case letters and digits")]
pub struct FlightIdError;

impl FlightId {
    /// How many characters an address has.
    pub const LENGTH: usize = 26;
    /// The characters an address is written in: the digits and the
    /// alphabet without `i`, `l`, `o` and `u`, which are easily misread.
    pub const ALPHABET: &'static str = "0123456789abcdefghjkmnpqrstvwxyz";

    /// The address as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FlightId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for FlightId {
    type Err = FlightIdError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let well_formed =
            text.len() == Self::LENGTH && text.chars().all(|c| Self::ALPHABET.contains(c));
        if well_formed {
            Ok(Self(text.to_owned()))
        } else {
            Err(FlightIdError)
        }
    }
}

impl TryFrom<String> for FlightId {
    type Error = FlightIdError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<FlightId> for String {
    fn from(id: FlightId) -> Self {
        id.0
    }
}

/// Who may see a flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Only its pilot. To everyone else the flight does not exist. A new
    /// flight starts like this when its pilot keeps new flights private
    /// ([`super::Account::new_flights_public`]), and public otherwise.
    Private,
    /// Anyone who has its link. It is not listed anywhere.
    Unlisted,
    /// Anyone. It is listed on its pilot's page.
    Public,
    /// A setting this version of the client does not know. Treat the flight
    /// as visible to more people than its pilot, and do not send this back.
    #[serde(other)]
    Unknown,
}

/// The pilot a flight belongs to, as everyone may see them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pilot {
    /// The pilot's handle.
    pub handle: Handle,
    /// The name the pilot is shown by.
    pub display_name: String,
    /// The path of the pilot's picture ([`super::picture_path`]), if they
    /// have one and the answer names it. Left out otherwise: an app then
    /// shows the pilot's initials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    /// The pilot's country, an ISO 3166-1 alpha-2 code in upper case, if
    /// they set one ([`super::Account::country`]). It is said with their
    /// flights and on their page; left out where the pilot set none, and
    /// in answers that are not about flights (a group's members, a live
    /// track).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

/// The class a paraglider is certified in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GliderClass {
    /// EN A.
    EnA,
    /// EN B.
    EnB,
    /// EN C.
    EnC,
    /// EN D.
    EnD,
    /// CCC, the competition class.
    Ccc,
    /// A class this version of the client does not know. Do not send it
    /// back.
    #[serde(other)]
    Unknown,
}

/// Whether text is a country as the API takes one: two letters, an ISO
/// 3166-1 alpha-2 code. Returns it in upper case. Whether the code is one
/// ISO has assigned is not checked.
pub fn country_code(text: &str) -> Option<String> {
    let text = text.trim();
    (text.len() == 2 && text.bytes().all(|byte| byte.is_ascii_alphabetic()))
        .then(|| text.to_ascii_uppercase())
}

/// One flight.
///
/// The server keeps the flight file exactly as it was uploaded and works
/// everything else here out from it, with the code the apps use on the
/// device. Like there, the numbers describe the flown part of the
/// recording: from take-off to landing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flight {
    /// The flight's address on the server.
    pub id: FlightId,
    /// The flight's identity: a hash of its fixes, 32 hexadecimal
    /// characters. It is the identifier the apps' logbooks use, so a device
    /// matches the server's flights with its own by this value.
    pub identity: String,
    /// The pilot the flight belongs to.
    pub pilot: Pilot,
    /// Who may see the flight.
    pub visibility: Visibility,
    /// When the server first received the flight.
    pub uploaded_at: Timestamp,
    /// When the flown part begins: the take-off, or the first fix of a
    /// recording that never shows the glider flying.
    pub start: Timestamp,
    /// Seconds from take-off to landing, or to the end of a recording that
    /// stops in the air. For a recording that never shows the glider
    /// flying, the length of the recording.
    pub airtime_s: i64,
    /// Where and when the glider took off. Left out when the recording
    /// never shows it flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff: Option<FlightPlace>,
    /// Where and when it landed. Left out when the recording ends in the
    /// air or never shows the glider flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landing: Option<FlightPlace>,
    /// The site the glider took off from: the standing take-off nearest
    /// to [`Flight::takeoff`] within [`TAKEOFF_SITE_WITHIN_M`], of any
    /// layer the server serves, a partner's before a place from pilots
    /// before a source's. Left out when no such site stands, and for a
    /// recording that never shows the glider flying.
    ///
    /// The server looks it up with every answer, so it is the site and
    /// its name as they stand now: a site that is renamed or removed is
    /// so here too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff_site: Option<FlightSite>,
    /// The country the glider took off in, an ISO 3166-1 alpha-2 code, as
    /// the server finds it with every answer from the country outlines it
    /// has. Left out where the take-off is in no country it has an
    /// outline of (in November 2026 it has Austria's and France's), and
    /// for a recording that never shows the glider flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff_country: Option<String>,
    /// Distance along the track, metres.
    pub track_length_m: f64,
    /// Highest satellite altitude, metres above mean sea level.
    pub max_altitude_m: i32,
    /// Best climb averaged over half a minute, metres per second.
    pub max_climb_ms: f64,
    /// How the flight scores as a cross-country flight. Left out for a
    /// track too short to score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<FlightScore>,
    /// The glider, as the flight file names it. Text from the file: show it
    /// as text and never as markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glider: Option<String>,
    /// The class of the glider it was flown on: the pilot's setting
    /// ([`super::Account::glider_class`]) when it was uploaded, or what
    /// the pilot set on the flight later. Left out while it is not known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glider_class: Option<GliderClass>,
    /// The shape of the track for a thumbnail: at most 96 points, each a
    /// latitude and a longitude in degrees. Circles in thermals are
    /// smoothed away; nothing may be measured on it.
    pub outline: Vec<[f64; 2]>,
    /// The size of the flight file in bytes.
    pub file_bytes: u32,
    /// Which version of the server's analysis produced these numbers. The
    /// server works them out again when its analysis improves; the flight
    /// then shows up as changed.
    pub analysis_version: u32,
    /// Since when the flight is taken down, if whoever runs the server has
    /// done that. Left out otherwise, which is the usual case.
    ///
    /// A flight that is taken down is private, and only its pilot is ever
    /// shown this. While it is there the flight cannot be shared: asking
    /// for `unlisted` or `public` is refused, and deleting the flight and
    /// uploading its file again brings it back taken down. Only the
    /// operator can lift it. An app says so on the flight, in place of the
    /// choice of who sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_down_at: Option<Timestamp>,
    /// Since when the flight is public, without a break. Left out while
    /// it is not public.
    ///
    /// A flight that was withdrawn and made public again has the moment
    /// of the second time. For a flight that was public before the server
    /// kept this (2026-11-21) it is the moment of the upload: the earliest
    /// it can have been.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
}

/// Under what a flight that is not the asker's own is given: the one
/// statement every answer with such flights carries.
///
/// **Not decided.** Under what terms others may reuse a flight a pilot
/// made public is the owner's decision still to come
/// (`docs/data-platform.md`, "For the owner", 5). Until then the statement
/// says the least: the flights are their pilots', and reuse needs the
/// pilot's leave. [`FlightTerms::current`] is the one place that says it;
/// when the decision is made, it changes there and nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightTerms {
    /// The statement, in words for a person.
    pub text: String,
    /// The licence the flights are given under, by its name, for example
    /// `CC BY 4.0`. Left out while there is none: then nothing but
    /// reading is granted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    /// Whether Venti's terms have settled the question. `false` means the
    /// statement is a holding one and will change; a program that keeps
    /// flights should read it again with every answer.
    pub decided: bool,
}

/// The statement of [`FlightTerms`] while the terms are not decided.
pub const FLIGHT_TERMS_UNDECIDED: &str = "These flights are their pilots'. They are shown here \
     for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs \
     its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn \
     is to be removed from every copy.";

impl FlightTerms {
    /// What flights are given under today. TO DECIDE (the owner): the
    /// terms under which a public flight may be reused; this is the
    /// holding statement until then.
    pub fn current() -> Self {
        Self {
            text: FLIGHT_TERMS_UNDECIDED.to_owned(),
            licence: None,
            decided: false,
        }
    }
}

/// How far from a flight's take-off a site may stand to be the site the
/// flight took off from, in metres: as far as a site's flights reach
/// ([`SITE_FLIGHTS_WITHIN_M`](super::SITE_FLIGHTS_WITHIN_M)).
pub const TAKEOFF_SITE_WITHIN_M: f64 = super::SITE_FLIGHTS_WITHIN_M;

/// The site a flight took off from, by its identifier and its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightSite {
    /// The site's identifier, as `GET /v1/sites/{id}` reads it.
    pub id: SiteId,
    /// The site's name as it stands. Text from a source or from pilots:
    /// show it as text and never as markup.
    pub name: String,
}

/// A moment at one end of a flight.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightPlace {
    /// When.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Satellite altitude, metres above mean sea level.
    pub altitude_m: i32,
}

/// How a flight scores under the World XContest's rules, as this project
/// reads them. Until a flight has been compared with XContest's own result
/// the points are ours, not the league's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightScore {
    /// What the flight is scored as.
    pub kind: RouteKind,
    /// For a triangle, whether start and finish lie within 5% of its
    /// perimeter of each other. Always `false` for a free flight.
    pub closed: bool,
    /// The scoring distance, metres.
    pub distance_m: f64,
    /// The points.
    pub points: f64,
}

/// What a flight is scored as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// The longest distance through up to three turnpoints.
    FreeFlight,
    /// A triangle that is not an FAI triangle.
    FreeTriangle,
    /// A triangle whose shortest leg is at least 28% of its perimeter.
    FaiTriangle,
    /// A kind this version of the client does not know.
    #[serde(other)]
    Unknown,
}

/// The answer of `GET /v1/flights/{id}` and of `PATCH /v1/flights/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightResponse {
    /// The flight.
    pub flight: Flight,
    /// Under what the flight is given, when it is public or shared by
    /// link. Left out for a private flight, which only its pilot reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<FlightTerms>,
}

/// The answer of `POST /v1/flights`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadResponse {
    /// The flight as the server now holds it.
    pub flight: Flight,
    /// `true` when this upload created the flight, `false` when the account
    /// already had it. Either way the upload succeeded.
    pub created: bool,
}

/// The answer of `GET /v1/flights`, one page of the account's flights, and
/// of `GET /v1/flights/public`, one page of everyone's public flights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightsResponse {
    /// The flights of this page, in the order that was asked for: newest
    /// first unless the request said otherwise.
    pub flights: Vec<Flight>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// Under what the flights are given, in a list of everyone's public
    /// flights. Left out of the list of the account's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<FlightTerms>,
}

/// How many of a pilot's flights count in a ranking: the best three.
pub const RANKING_COUNTED: usize = 3;

/// The answer of `GET /v1/ranking`: the pilots of everyone's public
/// flights by the sum of the points of their best flights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankingResponse {
    /// How many of a pilot's flights count at most:
    /// [`RANKING_COUNTED`], said in the answer so that a client words the
    /// rule from what the server did.
    pub counted: u32,
    /// The pilots, the one with the most points first, and by handle among
    /// pilots with the same points.
    pub pilots: Vec<RankedPilot>,
    /// The years and months that have a ranking: those in which a public
    /// flight that scores started, in the part of the map that was asked
    /// for and whatever span was. The newest year first. Empty from a
    /// server that does not say.
    #[serde(default)]
    pub periods: Vec<RankingYear>,
    /// The first moment of the span the ranking counts, as `from` or
    /// `month` asked for it. Left out for a ranking of all time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Timestamp>,
    /// The first moment after that span, as `to` or `month` asked for it.
    /// Left out when the span has no end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Timestamp>,
    /// The month the ranking is of, when `month` asked for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<YearMonth>,
    /// Under what the flights are given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<FlightTerms>,
}

/// A calendar month by UTC, written `2026-07`: what a ranking's `month`
/// is asked with and what its periods are named by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct YearMonth {
    year: i32,
    month: u32,
}

/// Why text is not a [`YearMonth`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a month: write it as YYYY-MM, for example 2026-07")]
pub struct YearMonthError(String);

impl YearMonth {
    /// A month of a year from 0 to 9999, by its number from 1 to 12.
    pub fn new(year: i32, month: u32) -> Option<Self> {
        ((0..=9999).contains(&year) && (1..=12).contains(&month)).then_some(Self { year, month })
    }

    /// The year.
    pub fn year(&self) -> i32 {
        self.year
    }

    /// The month of the year, 1 to 12.
    pub fn month(&self) -> u32 {
        self.month
    }
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl FromStr for YearMonth {
    type Err = YearMonthError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let digits = |part: &str, length: usize| {
            (part.len() == length && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| part.parse::<u32>().ok())
                .flatten()
        };
        text.split_once('-')
            .and_then(|(year, month)| {
                Self::new(i32::try_from(digits(year, 4)?).ok()?, digits(month, 2)?)
            })
            .ok_or_else(|| YearMonthError(text.to_owned()))
    }
}

impl TryFrom<String> for YearMonth {
    type Error = YearMonthError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<YearMonth> for String {
    fn from(month: YearMonth) -> Self {
        month.to_string()
    }
}

/// A year that has a ranking, with its months that have one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankingYear {
    /// The year, by UTC.
    pub year: i32,
    /// How many public flights that score started in it.
    pub flights: u32,
    /// Its months that have such a flight, the newest first.
    pub months: Vec<RankingMonth>,
}

/// A month that has a ranking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankingMonth {
    /// The month, as `GET /v1/ranking?month=` takes it.
    pub month: YearMonth,
    /// How many public flights that score started in it.
    pub flights: u32,
}

/// The fewest characters a search for pilots takes.
pub const PILOT_SEARCH_MIN_CHARS: usize = 2;

/// The most characters a search for pilots takes.
pub const PILOT_SEARCH_MAX_CHARS: usize = 60;

/// The answer of `GET /v1/pilots?q=`: the pilots with a public flight
/// whose handle or display name holds the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotsResponse {
    /// The pilots: those whose handle or name begins with the text first,
    /// then the pilot with the most public flights, then by handle.
    pub pilots: Vec<FoundPilot>,
}

/// A pilot a search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundPilot {
    /// The pilot.
    pub pilot: Pilot,
    /// How many public flights the pilot has.
    pub flights: u32,
}

/// How many of the take-off sites a pilot flew from most their totals
/// name.
pub const PILOT_TOP_SITES: usize = 5;

/// What a take-off is rounded to before the site it belongs to is looked
/// up for a pilot's totals, in degrees: 11 m from north to south.
pub const PILOT_SITE_STEP_DEG: f64 = 0.0001;

/// The answer of `GET /v1/pilots/{handle}/statistics`: what a pilot's
/// public flights add up to, all together and year by year.
///
/// It is made from the flights the pilot made public and from nothing
/// else, anew for every answer. Years and months are those of UTC, as a
/// ranking's are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotStatisticsResponse {
    /// The pilot.
    pub pilot: Pilot,
    /// How many of a pilot's flights count for `points` at most:
    /// [`RANKING_COUNTED`], as in a ranking.
    pub counted: u32,
    /// All public flights together.
    pub overall: PilotTotals,
    /// Each year the pilot has a public flight in, the newest first.
    pub seasons: Vec<PilotSeason>,
    /// Under what the flights are given.
    pub terms: FlightTerms,
}

/// A year of a pilot's public flights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotSeason {
    /// The year, by UTC.
    pub year: i32,
    /// What its flights add up to.
    pub totals: PilotTotals,
}

/// What some of a pilot's public flights add up to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotTotals {
    /// How many flights.
    pub flights: u32,
    /// How many of them score.
    pub scored: u32,
    /// The scoring distances of those that score, added up, in metres.
    pub distance_m: f64,
    /// The sum of the points of the best `counted` of them, rounded to
    /// hundredths: what a ranking of the same span gives the pilot.
    pub points: f64,
    /// The airtime of all of them, added up, in seconds.
    pub airtime_s: i64,
    /// The flight with the longest scoring distance. Left out when none
    /// scores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longest: Option<LongestFlight>,
    /// The flight that went highest. Left out without a flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highest: Option<HighestFlight>,
    /// The take-off sites flown from most, the one with the most flights
    /// first and by identifier among sites with as many: at most
    /// [`PILOT_TOP_SITES`]. A flight that took off where no site with a
    /// name stands is in none.
    pub sites: Vec<FlownSite>,
    /// How many of the flights began in each month of the year, January
    /// first, all years together: twelve numbers that add up to
    /// `flights`.
    pub by_month: [u32; 12],
}

/// The longest of some flights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongestFlight {
    /// The flight's address on the server.
    pub id: FlightId,
    /// Its scoring distance, metres.
    pub distance_m: f64,
}

/// The highest of some flights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighestFlight {
    /// The flight's address on the server.
    pub id: FlightId,
    /// Its highest satellite altitude, metres above mean sea level.
    pub max_altitude_m: i32,
}

/// A take-off site with how often a pilot flew from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlownSite {
    /// The site, as a flight names the one it took off from.
    pub site: FlightSite,
    /// Where the site stands, as its record has it: for a map and a link.
    /// Left out by a server that does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<GeoPoint>,
    /// How many of the flights took off from it.
    pub flights: u32,
}

/// A pilot's place in a ranking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedPilot {
    /// The pilot.
    pub pilot: Pilot,
    /// The sum of the points of the flights that count, rounded to
    /// hundredths as a flight's own points are.
    pub points: f64,
    /// How many of the pilot's public flights among those asked for scored
    /// at all, those that count among them.
    pub scored: u32,
    /// The flights that count, the best first: no more than `counted`.
    pub flights: Vec<Flight>,
}

/// The body of `PATCH /v1/flights/{id}`: what changes of one of the
/// account's flights. A field that is left out stays as it is; at least
/// one is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UpdateFlightRequest {
    /// Who may see the flight from now on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    /// The class of the glider the flight was flown on; `null` says it is
    /// not known.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "super::patch"
    )]
    pub glider_class: Option<Option<GliderClass>>,
}

/// The body of `PATCH /v1/flights`: who may see every flight of the
/// account from now on, `public` or `private`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateFlightsRequest {
    /// `public` or `private`. A flight shared by link is set like any
    /// other; making all of them `unlisted` is not offered.
    pub visibility: Visibility,
}

/// The answer of `PATCH /v1/flights`: how the account's flights stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateFlightsResponse {
    /// What was asked for.
    pub visibility: Visibility,
    /// How many of the account's flights are so now, those that were so
    /// before included.
    pub flights: u32,
    /// How many of them the request changed.
    pub changed: u32,
    /// How many stay private because whoever runs the server took them
    /// down. Only ever more than 0 when `public` was asked for.
    pub taken_down: u32,
}

/// The answer of `GET /v1/flights/changes`: what happened to the account's
/// flights since the device last asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightChangesResponse {
    /// Flights that are new or changed, each as it is now.
    pub flights: Vec<Flight>,
    /// Flights that were deleted.
    pub deleted: Vec<DeletedFlight>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more changes are waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server did not recognise the request's `since`, for
    /// example after it was restored from a backup, and therefore started
    /// again from the beginning. The device should then compare its whole
    /// logbook with the server's flights instead of applying changes only.
    pub reset: bool,
}

/// A flight that is no longer on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedFlight {
    /// The address the flight had.
    pub id: FlightId,
    /// The identity of the flight, to find it in the device's logbook.
    pub identity: String,
}

/// The answer of `GET /v1/pilots/{handle}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PilotResponse {
    /// The pilot.
    pub pilot: Pilot,
    /// One page of the pilot's public flights, newest first.
    pub flights: Vec<Flight>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// Under what the flights are given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<FlightTerms>,
}

/// The answer of `GET /v1/flights/public/changes`: what changed in what
/// the public sees of flights since a program last asked.
///
/// Whoever keeps a copy of public flights follows this: it stores every
/// flight of `flights` over the one it has, removes every flight that
/// `withdrawn` names, and remembers `cursor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicFlightChangesResponse {
    /// Flights that became public, or changed while public, each as it is
    /// now.
    pub flights: Vec<Flight>,
    /// Flights that are public no more.
    pub withdrawn: Vec<WithdrawnFlight>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more changes are waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server did not recognise the request's `since`, for
    /// example after it was restored from a backup, and therefore started
    /// again from the beginning. Whoever keeps a copy then replaces it
    /// with what the answers from here on hold.
    pub reset: bool,
    /// Under what the flights are given.
    pub terms: FlightTerms,
}

/// A flight that was public and is not: its pilot made it private or
/// shared it by link only, or deleted it or the account, or whoever runs
/// the server took it down.
///
/// It is named by the SHA-256 of its address and not by the address: a
/// flight that is now shared by link is read by whoever has its address,
/// and this list is not to hand that address to everyone. Whoever holds
/// the flight holds its address and finds it by the hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithdrawnFlight {
    /// The SHA-256 of the flight's `id`, as 64 lower-case hexadecimal
    /// characters: of the 26 characters of the address, as ASCII.
    pub id_sha256: String,
    /// When it was withdrawn. Left out for a flight that went with its
    /// pilot's account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withdrawn_at: Option<Timestamp>,
}

/// The most points a track holds: [`FlightTrack`].
pub const MAX_TRACK_POINTS: usize = 50_000;

/// The answer of `GET /v1/flights/{id}/track`: the flight's track as a
/// GeoJSON Feature (RFC 7946) whose geometry is a LineString.
///
/// The media type is `application/geo+json`. `terms` is a member of the
/// server's own, which the format allows beside its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightTrack {
    /// `Feature`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The flight's address.
    pub id: FlightId,
    /// The line.
    pub geometry: TrackLine,
    /// What belongs to the line.
    pub properties: TrackProperties,
    /// Under what the track is given, when the flight is public or shared
    /// by link. Left out for a private flight, which only its pilot reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<FlightTerms>,
}

/// The line of a [`FlightTrack`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackLine {
    /// `LineString`.
    #[serde(rename = "type")]
    pub kind: String,
    /// One position per fix, in the order of the recording: longitude and
    /// latitude in degrees (WGS 84), and the satellite altitude in metres
    /// as the file gives it.
    pub coordinates: Vec<[f64; 3]>,
}

/// What belongs to the line of a [`FlightTrack`]. The lists have one
/// entry per position of the line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackProperties {
    /// The time of the first position.
    pub start: Timestamp,
    /// For each position, the seconds from `start` to its fix.
    pub times_s: Vec<i64>,
    /// For each position, the pressure altitude in metres as the file
    /// gives it. Left out for a recording without a pressure sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_altitudes_m: Option<Vec<i32>>,
    /// How many fixes the recording holds.
    pub fixes: usize,
    /// Every how many fixes a position was taken: 1 when the line holds
    /// every fix, more for a recording of more than
    /// [`MAX_TRACK_POINTS`] fixes. The last fix is always there.
    pub every: usize,
    /// The seconds from `start` to the take-off the analysis found. Left
    /// out when the recording never shows the glider flying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff_s: Option<i64>,
    /// The seconds from `start` to the landing the analysis found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landing_s: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flight_address_is_26_characters_of_its_alphabet() {
        let id = "0123456789abcdefghjkmnpqrs";
        assert_eq!(id.parse::<FlightId>().unwrap().as_str(), id);
        assert_eq!(FlightId::ALPHABET.len(), 32);

        for bad in [
            "",
            "0123456789abcdefghjkmnpqr",   // one short
            "0123456789abcdefghjkmnpqrst", // one long
            "0123456789ABCDEFGHJKMNPQRS",  // upper case
            "0123456789abcdefghijkmnpqr",  // an i
            "../../../../../etc/passwd/",
            "0123456789abcdefghjkmnpq%2f",
            "0123456789abcdefghjkmnpqrś",
        ] {
            assert_eq!(bad.parse::<FlightId>(), Err(FlightIdError), "{bad:?}");
        }
        assert!(serde_json::from_str::<FlightId>(r#""changes""#).is_err());
    }

    #[test]
    fn settings_a_client_does_not_know_still_decode() {
        let visibility: Visibility = serde_json::from_str(r#""followers""#).unwrap();
        assert_eq!(visibility, Visibility::Unknown);
        let kind: RouteKind = serde_json::from_str(r#""flat_triangle""#).unwrap();
        assert_eq!(kind, RouteKind::Unknown);
        assert_eq!(
            serde_json::to_string(&Visibility::Unlisted).unwrap(),
            r#""unlisted""#
        );
        assert_eq!(
            serde_json::to_string(&RouteKind::FaiTriangle).unwrap(),
            r#""fai_triangle""#
        );
    }

    #[test]
    fn what_is_not_known_about_a_flight_is_left_out() {
        let start = chrono::DateTime::from_timestamp(1_783_161_000, 0).unwrap();
        let flight = Flight {
            id: "0123456789abcdefghjkmnpqrs".parse().unwrap(),
            identity: "00112233445566778899aabbccddeeff".to_owned(),
            pilot: Pilot {
                handle: Handle::new("demo").unwrap(),
                display_name: "Demo Pilot".to_owned(),
                country: None,
                picture: None,
            },
            visibility: Visibility::Private,
            uploaded_at: start,
            start,
            airtime_s: 60,
            takeoff: None,
            landing: None,
            takeoff_site: None,
            takeoff_country: None,
            track_length_m: 600.0,
            max_altitude_m: 1620,
            max_climb_ms: 0.0,
            score: None,
            glider: None,
            glider_class: None,
            outline: vec![[47.6642, 11.7967], [47.67, 11.79]],
            file_bytes: 2300,
            analysis_version: 1,
            taken_down_at: None,
            published_at: None,
        };

        let json = serde_json::to_value(&flight).unwrap();
        let object = json.as_object().unwrap();

        for absent in [
            "takeoff",
            "landing",
            "score",
            "glider",
            "glider_class",
            "takeoff_country",
            "taken_down_at",
            "published_at",
        ] {
            assert!(!object.contains_key(absent), "{absent}");
        }
        assert_eq!(json["outline"][0][0], 47.6642);
        assert_eq!(json["pilot"]["handle"], "demo");
        assert_eq!(serde_json::from_value::<Flight>(json).unwrap(), flight);
    }

    #[test]
    fn a_change_of_a_flight_names_what_changes_and_null_clears_the_class() {
        let read = |text: &str| serde_json::from_str::<UpdateFlightRequest>(text).unwrap();
        assert_eq!(
            read(r#"{"visibility":"public"}"#),
            UpdateFlightRequest {
                visibility: Some(Visibility::Public),
                glider_class: None,
            }
        );
        assert_eq!(
            read(r#"{"glider_class":"en_b"}"#).glider_class,
            Some(Some(GliderClass::EnB))
        );
        assert_eq!(read(r#"{"glider_class":null}"#).glider_class, Some(None));
        assert_eq!(read("{}"), UpdateFlightRequest::default());
        let clear = UpdateFlightRequest {
            visibility: None,
            glider_class: Some(None),
        };
        assert_eq!(
            serde_json::to_string(&clear).unwrap(),
            r#"{"glider_class":null}"#
        );
        assert_eq!(
            serde_json::to_string(&GliderClass::Ccc).unwrap(),
            r#""ccc""#
        );
        assert_eq!(
            serde_json::from_str::<GliderClass>(r#""tandem""#).unwrap(),
            GliderClass::Unknown
        );
        assert_eq!(country_code(" de ").as_deref(), Some("DE"));
        assert_eq!(country_code("DEU"), None);
        assert_eq!(country_code("D1"), None);
    }
}
