//! How requests are written: paths, and the text form of their parameters.
//!
//! Both sides use these functions, the client to build a request and the
//! server to read it, so there is one definition of the format.

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use venti_domain::{BoundingBox, GeoError, GeoPoint, StationId, Timestamp};

use super::{FlightId, GroupId, Handle, LiveTrackId, PlanId};

/// Path of the health endpoint.
pub const HEALTH_PATH: &str = "/v1/health";

/// Path of the station list.
pub const STATIONS_PATH: &str = "/v1/stations";

/// Path of one station's observations.
pub fn observations_path(station: &StationId) -> String {
    format!(
        "{STATIONS_PATH}/{}/observations",
        encode(&station.to_string())
    )
}

/// A request for the stations inside a box, each with its newest observation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationsRequest {
    /// The box to list.
    pub bbox: BoundingBox,
}

impl StationsRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        format!("{STATIONS_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// A request for what one station measured in a window of time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationsRequest {
    /// The station.
    pub station: StationId,
    /// Start of the window, inclusive. The server's default is
    /// [`DEFAULT_WINDOW_HOURS`](super::DEFAULT_WINDOW_HOURS) before `to`.
    pub from: Option<Timestamp>,
    /// End of the window, inclusive. The server's default is its current time.
    pub to: Option<Timestamp>,
}

impl ObservationsRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            observations_path(&self.station),
            [
                ("from", self.from.map(format_time)),
                ("to", self.to.map(format_time)),
            ],
        )
    }
}

/// Path of the airspace inside a box.
pub const AIRSPACE_PATH: &str = "/v1/airspace";

/// A request for the airspace that touches a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirspaceRequest {
    /// The box to list.
    pub bbox: BoundingBox,
}

impl AirspaceRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        format!("{AIRSPACE_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// Path of the flying sites inside a box.
pub const SITES_PATH: &str = "/v1/sites";

/// A request for the flying sites inside a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SitesRequest {
    /// The box to list.
    pub bbox: BoundingBox,
}

impl SitesRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        format!("{SITES_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// Path of the forecast for a place.
pub const FORECAST_PATH: &str = "/v1/forecast";

/// A request for what the weather models say about one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForecastRequest {
    /// The place. The server answers for the nearest point of a grid of
    /// [`FORECAST_GRID_DEG`](super::FORECAST_GRID_DEG) degrees, which the
    /// answer names.
    pub point: GeoPoint,
}

impl ForecastRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        format!(
            "{FORECAST_PATH}?lat={}&lon={}",
            self.point.lat(),
            self.point.lon()
        )
    }
}

/// Why the `lat` and `lon` parameters were rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PositionError {
    /// One of the two is missing.
    #[error("the {0} parameter is missing")]
    Missing(&'static str),
    /// One of the two is not a number. Holds the start of what was sent.
    #[error("{0:?} is not a number of degrees")]
    NotANumber(String),
    /// The numbers are not a place on the globe.
    #[error("not a position: {0}")]
    Geo(#[from] GeoError),
}

/// Reads a place written as two parameters in degrees, `lat` and `lon`.
pub fn parse_position(lat: Option<&str>, lon: Option<&str>) -> Result<GeoPoint, PositionError> {
    let number = |name: &'static str, text: Option<&str>| {
        let text = text.ok_or(PositionError::Missing(name))?.trim();
        // Whatever was sent in place of a number is not repeated back at
        // length: the longest number of degrees anyone writes is shorter.
        text.parse::<f64>()
            .map_err(|_| PositionError::NotANumber(text.chars().take(24).collect()))
    };
    Ok(GeoPoint::new(number("lat", lat)?, number("lon", lon)?)?)
}

/// Path that creates an account.
pub const ACCOUNTS_PATH: &str = "/v1/accounts";

/// Path of the account a request is signed in to.
pub const ACCOUNT_PATH: &str = "/v1/account";

/// Path that changes the password of the account a request is signed in to.
pub const ACCOUNT_PASSWORD_PATH: &str = "/v1/account/password";

/// Path of an account's sessions: signing in, listing them, and signing out
/// everywhere.
pub const SESSIONS_PATH: &str = "/v1/sessions";

/// Path of one session, by the identifier the session list gives.
pub fn session_path(id: &str) -> String {
    format!("{SESSIONS_PATH}/{}", encode(id))
}

/// Path of an account's flights: uploading one, and listing them.
pub const FLIGHTS_PATH: &str = "/v1/flights";

/// Path of what changed among an account's flights.
pub const FLIGHT_CHANGES_PATH: &str = "/v1/flights/changes";

/// Path of one flight.
pub fn flight_path(flight: &FlightId) -> String {
    format!("{FLIGHTS_PATH}/{flight}")
}

/// Path of one flight's file, exactly as it was uploaded.
pub fn flight_file_path(flight: &FlightId) -> String {
    format!("{FLIGHTS_PATH}/{flight}/igc")
}

/// Path of a pilot's public page data.
pub fn pilot_path(pilot: &Handle) -> String {
    format!("/v1/pilots/{pilot}")
}

/// Path of the page about one flight that is meant for sharing. It is not
/// part of the versioned API: it answers HTML, for people and for the
/// previews chat apps make of a link.
pub fn share_path(flight: &FlightId) -> String {
    format!("/f/{flight}")
}

/// Path of the picture that goes with a flight's share page.
pub fn share_image_path(flight: &FlightId) -> String {
    format!("/f/{flight}/preview.png")
}

/// Path of the public live tracks in a box.
pub const LIVE_PATH: &str = "/v1/live";

/// Path that takes the fixes of a live track.
pub const LIVE_POSITIONS_PATH: &str = "/v1/live/positions";

/// Path of one live track: reading it, and deleting it.
pub fn live_track_path(track: &LiveTrackId) -> String {
    format!("{LIVE_PATH}/{track}")
}

/// Path of the page about one live track that is meant for sharing. Like
/// [`share_path`] it is not part of the versioned API: it answers HTML, for
/// people without the app.
pub fn live_page_path(track: &LiveTrackId) -> String {
    format!("/live/{track}")
}

/// A request for the public live tracks whose newest fix lies in a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveTracksRequest {
    /// The box to list.
    pub bbox: BoundingBox,
}

impl LiveTracksRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        format!("{LIVE_PATH}?bbox={}", format_bbox(&self.bbox))
    }
}

/// A request for one live track and its fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveTrackRequest {
    /// The track.
    pub id: LiveTrackId,
    /// The `cursor` of the answer before: only fixes that arrived after
    /// that answer are returned. `None` asks for every fix.
    pub since: Option<String>,
    /// Only fixes later than this moment are returned. `None` sets no
    /// bound.
    pub after: Option<Timestamp>,
    /// How many fixes to return, at most
    /// [`MAX_LIVE_FIXES`](super::MAX_LIVE_FIXES), which is also the
    /// server's default.
    pub limit: Option<usize>,
}

impl LiveTrackRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            live_track_path(&self.id),
            [
                ("since", self.since.clone()),
                ("after", self.after.map(format_time)),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// A request for one page of the signed-in account's flights, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlightsRequest {
    /// The `next` of the page before. `None` asks for the first page.
    pub after: Option<String>,
    /// How many flights to return, at most
    /// [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE). The server's default is
    /// [`DEFAULT_PAGE_SIZE`](super::DEFAULT_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl FlightsRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            FLIGHTS_PATH.to_owned(),
            [
                ("after", self.after.clone()),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// A request for what changed among the signed-in account's flights.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlightChangesRequest {
    /// The `cursor` of the answer before. `None` asks for everything.
    pub since: Option<String>,
    /// How many changes to return, at most
    /// [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl FlightChangesRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            FLIGHT_CHANGES_PATH.to_owned(),
            [
                ("since", self.since.clone()),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// A request for a pilot and one page of their public flights, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotRequest {
    /// The pilot.
    pub handle: Handle,
    /// The `next` of the page before. `None` asks for the first page.
    pub after: Option<String>,
    /// How many flights to return, at most
    /// [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl PilotRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        with_query(
            pilot_path(&self.handle),
            [
                ("after", self.after.clone()),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// Path that takes a report about a flight, a pilot's page, a live
/// track, a group or a plan. What is reported is named in the body, not in
/// the path: the identifier of a live track is the key to it, a group is
/// nobody's business but its members', and neither belongs in a log.
pub const REPORTS_PATH: &str = "/v1/reports";

/// Path of the groups a pilot is in: making one, and listing them.
pub const GROUPS_PATH: &str = "/v1/groups";

/// Path that takes an invite code and makes its sender a member of the
/// code's group. The code is sent in the body, not in the path: whoever
/// has it can come into the group, and a path is what a log names.
pub const GROUP_JOIN_PATH: &str = "/v1/groups/join";

/// Path of the plans of every group a pilot is in.
pub const PLAN_BOARD_PATH: &str = "/v1/groups/plans";

/// Path of one group: reading it, renaming it and deleting it.
pub fn group_path(group: &GroupId) -> String {
    format!("{GROUPS_PATH}/{group}")
}

/// Path that makes a group a new invite code.
pub fn group_invite_path(group: &GroupId) -> String {
    format!("{GROUPS_PATH}/{group}/invite")
}

/// Path that hands the keeping of a group to another member.
pub fn group_keeper_path(group: &GroupId) -> String {
    format!("{GROUPS_PATH}/{group}/keeper")
}

/// Path of one member of a group: for leaving the group, and for the
/// keeper to remove a member.
pub fn group_member_path(group: &GroupId, member: &Handle) -> String {
    format!("{GROUPS_PATH}/{group}/members/{member}")
}

/// Path of a group's plans: posting one, and listing them.
pub fn group_plans_path(group: &GroupId) -> String {
    format!("{GROUPS_PATH}/{group}/plans")
}

/// Path of one plan: changing it and deleting it.
pub fn plan_path(group: &GroupId, plan: &PlanId) -> String {
    format!("{GROUPS_PATH}/{group}/plans/{plan}")
}

/// Path of a pilot's word on a plan: saying that they are in, and taking
/// that back.
pub fn plan_in_path(group: &GroupId, plan: &PlanId) -> String {
    format!("{GROUPS_PATH}/{group}/plans/{plan}/in")
}

/// A request for the plans of one group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlansRequest {
    /// The group.
    pub group: GroupId,
    /// Which days.
    pub days: PlanDays,
}

impl PlansRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        self.days.query(group_plans_path(&self.group))
    }
}

/// A request for the plans of every group the pilot is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanBoardRequest {
    /// Which days.
    pub days: PlanDays,
}

impl PlanBoardRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        self.days.query(PLAN_BOARD_PATH.to_owned())
    }
}

/// The days a list of plans is asked for. One day, which is what a board
/// of "who flies where today" shows, is that day as both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanDays {
    /// The first day wanted. `None` leaves it to the server, which starts
    /// at the date its own clock shows in UTC. Away from Greenwich that
    /// can be the day before or after the pilot's own, so a client names
    /// the day it means.
    pub from: Option<NaiveDate>,
    /// The last day wanted. `None` asks for every day from the first on.
    pub to: Option<NaiveDate>,
}

impl PlanDays {
    fn query(&self, path: String) -> String {
        with_query(
            path,
            [
                ("from", self.from.map(format_day)),
                ("to", self.to.map(format_day)),
            ],
        )
    }
}

/// A `from` or `to` parameter that is not a date.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a date written YYYY-MM-DD, such as 2026-11-18")]
pub struct DayError(pub String);

/// Writes a day as `YYYY-MM-DD`.
pub fn format_day(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// Reads a day written as `YYYY-MM-DD`.
pub fn parse_day(text: &str) -> Result<NaiveDate, DayError> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        // Only as this crate writes a day: the parser alone also takes
        // "2026-1-5" and "+2026-01-05".
        .filter(|day| format_day(*day) == text)
        // Whatever was sent in place of a date is not repeated back at
        // length.
        .ok_or_else(|| DayError(text.chars().take(24).collect()))
}

/// Path of everyone's public flights in one list.
pub const PUBLIC_FLIGHTS_PATH: &str = "/v1/flights/public";

/// The order of a list of public flights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FlightSort {
    /// Newest first, by the start of the flight. The server's default.
    #[default]
    Newest,
    /// Most points first. A flight too short to score is not in this list.
    Score,
}

impl FlightSort {
    /// The value of the `sort` parameter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::Score => "score",
        }
    }
}

/// Text that names no order of a list of public flights.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("sort must be one of newest, score")]
pub struct FlightSortError;

impl std::str::FromStr for FlightSort {
    type Err = FlightSortError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        [Self::Newest, Self::Score]
            .into_iter()
            .find(|sort| sort.as_str() == text)
            .ok_or(FlightSortError)
    }
}

/// A request for one page of everyone's public flights.
///
/// Every part narrows the list or orders it; none can widen it beyond the
/// flights their pilots made public.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PublicFlightsRequest {
    /// Only flights that took off inside this box. `None` asks for flights
    /// from anywhere.
    pub bbox: Option<BoundingBox>,
    /// Only flights that started at this moment or later.
    pub from: Option<Timestamp>,
    /// Only flights that started before this moment. A day is asked for
    /// with its first moment as `from` and the next day's as `to`.
    pub to: Option<Timestamp>,
    /// The order. `None` leaves it to the server, which lists the newest
    /// first.
    pub sort: Option<FlightSort>,
    /// The `next` of the page before, which was asked for with the same
    /// other values. `None` asks for the first page.
    pub after: Option<String>,
    /// How many flights to return, at most
    /// [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE). The server's default is
    /// [`DEFAULT_PAGE_SIZE`](super::DEFAULT_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl PublicFlightsRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        // A box is written with its commas, as the other requests write it.
        let path = match &self.bbox {
            Some(bbox) => format!("{PUBLIC_FLIGHTS_PATH}?bbox={}", format_bbox(bbox)),
            None => PUBLIC_FLIGHTS_PATH.to_owned(),
        };
        with_query(
            path,
            [
                ("from", self.from.map(format_time)),
                ("to", self.to.map(format_time)),
                ("sort", self.sort.map(|sort| sort.as_str().to_owned())),
                ("after", self.after.clone()),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// Path of the ranking of pilots by their best public flights.
pub const RANKING_PATH: &str = "/v1/ranking";

/// A request for the pilots of everyone's public flights, ranked by the sum
/// of the points of their best.
///
/// A season is asked for as a span of time, as a day is of the list of
/// flights: the server knows no calendar of its own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RankingRequest {
    /// Only flights that took off inside this box count. `None` counts
    /// flights from anywhere.
    pub bbox: Option<BoundingBox>,
    /// Only flights that started at this moment or later count.
    pub from: Option<Timestamp>,
    /// Only flights that started before this moment count.
    pub to: Option<Timestamp>,
    /// How many pilots to return from the top, at most
    /// [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE). The server's default is
    /// [`DEFAULT_PAGE_SIZE`](super::DEFAULT_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl RankingRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        let path = match &self.bbox {
            Some(bbox) => format!("{RANKING_PATH}?bbox={}", format_bbox(bbox)),
            None => RANKING_PATH.to_owned(),
        };
        with_query(
            path,
            [
                ("from", self.from.map(format_time)),
                ("to", self.to.map(format_time)),
                ("limit", self.limit.map(|limit| limit.to_string())),
            ],
        )
    }
}

/// Appends the parameters that have a value to a path, which may have a
/// query string already.
fn with_query<const N: usize>(mut path: String, parameters: [(&str, Option<String>); N]) -> String {
    let mut separator = if path.contains('?') { '&' } else { '?' };
    for (name, value) in parameters {
        if let Some(value) = value {
            path.push(separator);
            path.push_str(name);
            path.push('=');
            path.push_str(&encode(&value));
            separator = '&';
        }
    }
    path
}

/// Why a `bbox` parameter was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BboxError {
    /// The parameter does not have four comma-separated parts.
    #[error("bbox needs four numbers, west,south,east,north, and has {0}")]
    Count(usize),
    /// A part is not a number.
    #[error("bbox part {0:?} is not a number")]
    NotANumber(String),
    /// The numbers do not form a box.
    #[error("bbox is not a box: {0}")]
    Geo(#[from] GeoError),
}

/// Writes a box as `west,south,east,north`, the order web maps use.
pub fn format_bbox(bbox: &BoundingBox) -> String {
    format!(
        "{},{},{},{}",
        bbox.west(),
        bbox.south(),
        bbox.east(),
        bbox.north()
    )
}

/// Reads a box written as `west,south,east,north` in degrees.
pub fn parse_bbox(text: &str) -> Result<BoundingBox, BboxError> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let [west, south, east, north] = parts[..] else {
        return Err(BboxError::Count(parts.len()));
    };
    let number = |part: &str| {
        part.parse::<f64>()
            .map_err(|_| BboxError::NotANumber(part.to_owned()))
    };
    Ok(BoundingBox::new(
        number(south)?,
        number(west)?,
        number(north)?,
        number(east)?,
    )?)
}

/// A time parameter that is not an RFC 3339 time.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{0:?} is not an RFC 3339 time such as 2026-11-18T18:00:00Z \
     (a '+' in an offset must be written as %2B in a URL)"
)]
pub struct TimeError(pub String);

/// Writes a time as RFC 3339 in UTC, for example `2026-11-18T18:00:00Z`.
///
/// The `Z` form needs no escaping in a query string, unlike `+00:00`.
pub fn format_time(time: Timestamp) -> String {
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// Reads an RFC 3339 time with any offset.
pub fn parse_time(text: &str) -> Result<Timestamp, TimeError> {
    DateTime::parse_from_rfc3339(text)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| TimeError(text.to_owned()))
}

/// Escapes text for use as one segment of a URL path or one value of a
/// query string.
///
/// Station identifiers contain a provider's own station key, which may hold
/// any character. Letters, digits, `-`, `.`, `_`, `~` and `:` stay as they
/// are; every other byte becomes `%XX`.
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~:".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use venti_domain::SourceId;

    use super::*;

    fn evening() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 18, 0, 0).unwrap()
    }

    #[test]
    fn bbox_round_trips_in_west_south_east_north_order() {
        let alps = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        assert_eq!(format_bbox(&alps), "9.4,46.3,17.2,48.5");
        assert_eq!(parse_bbox("9.4,46.3,17.2,48.5"), Ok(alps));
        assert_eq!(parse_bbox(" 9.4, 46.3 ,17.2,48.5"), Ok(alps));
    }

    #[test]
    fn bbox_errors_say_what_is_wrong() {
        assert_eq!(parse_bbox("9.4,46.3,17.2"), Err(BboxError::Count(3)));
        assert_eq!(parse_bbox(""), Err(BboxError::Count(1)));
        assert_eq!(
            parse_bbox("9.4,46.3,east,48.5"),
            Err(BboxError::NotANumber("east".to_owned()))
        );
        // South and north swapped.
        assert_eq!(
            parse_bbox("9.4,48.5,17.2,46.3"),
            Err(BboxError::Geo(GeoError::InvertedBox))
        );
        // Rust reads "NaN" as a number; the box refuses it.
        assert!(matches!(
            parse_bbox("9.4,46.3,17.2,NaN"),
            Err(BboxError::Geo(GeoError::Latitude(_)))
        ));
        assert_eq!(
            parse_bbox("9.4,46.3,17.2,91"),
            Err(BboxError::Geo(GeoError::Latitude(91.0)))
        );
    }

    #[test]
    fn times_round_trip_and_accept_any_offset() {
        assert_eq!(format_time(evening()), "2026-11-18T18:00:00Z");
        assert_eq!(parse_time("2026-11-18T18:00:00Z"), Ok(evening()));
        assert_eq!(parse_time("2026-11-18T19:00:00+01:00"), Ok(evening()));
        assert!(parse_time("2026-11-18").is_err());
        assert!(parse_time("yesterday").is_err());
    }

    #[test]
    fn requests_build_their_path_and_query() {
        let stations = StationsRequest {
            bbox: BoundingBox::new(47.2, 10.5, 47.8, 11.9).unwrap(),
        };
        assert_eq!(
            stations.path_and_query(),
            "/v1/stations?bbox=10.5,47.2,11.9,47.8"
        );

        let mut observations = ObservationsRequest {
            station: "dwd-10min:05792".parse().unwrap(),
            from: None,
            to: None,
        };
        assert_eq!(
            observations.path_and_query(),
            "/v1/stations/dwd-10min:05792/observations"
        );
        observations.to = Some(evening());
        assert_eq!(
            observations.path_and_query(),
            "/v1/stations/dwd-10min:05792/observations?to=2026-11-18T18:00:00Z"
        );
        observations.from = Some(evening() - chrono::Duration::hours(6));
        assert_eq!(
            observations.path_and_query(),
            "/v1/stations/dwd-10min:05792/observations\
             ?from=2026-11-18T12:00:00Z&to=2026-11-18T18:00:00Z"
        );

        let airspace = AirspaceRequest {
            bbox: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
        };
        assert_eq!(
            airspace.path_and_query(),
            "/v1/airspace?bbox=9.4,46.3,17.2,48.5"
        );

        let sites = SitesRequest {
            bbox: BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap(),
        };
        assert_eq!(sites.path_and_query(), "/v1/sites?bbox=11.6,47.55,12,47.85");

        let forecast = ForecastRequest {
            point: GeoPoint::new(47.6652, 11.7954).unwrap(),
        };
        assert_eq!(
            forecast.path_and_query(),
            "/v1/forecast?lat=47.6652&lon=11.7954"
        );
    }

    #[test]
    fn a_position_is_two_numbers_that_lie_on_the_globe() {
        let wallberg = GeoPoint::new(47.6652, 11.7954).unwrap();
        assert_eq!(
            parse_position(Some("47.6652"), Some("11.7954")),
            Ok(wallberg)
        );
        assert_eq!(
            parse_position(Some(" 47.6652 "), Some("11.7954")),
            Ok(wallberg)
        );
        assert_eq!(
            parse_position(None, Some("11.7954")),
            Err(PositionError::Missing("lat"))
        );
        assert_eq!(
            parse_position(Some("47.6652"), None),
            Err(PositionError::Missing("lon"))
        );
        assert_eq!(
            parse_position(Some("north"), Some("11.7954")),
            Err(PositionError::NotANumber("north".to_owned()))
        );
        // Latitude and longitude the wrong way round, for a place far east.
        assert_eq!(
            parse_position(Some("138.2"), Some("36.2")),
            Err(PositionError::Geo(GeoError::Latitude(138.2)))
        );
        // Rust reads "NaN" and "inf" as numbers; a position refuses them.
        assert!(matches!(
            parse_position(Some("NaN"), Some("11.8")),
            Err(PositionError::Geo(_))
        ));
        assert!(matches!(
            parse_position(Some("47.7"), Some("inf")),
            Err(PositionError::Geo(_))
        ));
    }

    #[test]
    fn account_and_flight_requests_build_their_paths() {
        let flight: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
        assert_eq!(
            flight_path(&flight),
            "/v1/flights/0123456789abcdefghjkmnpqrs"
        );
        assert_eq!(
            flight_file_path(&flight),
            "/v1/flights/0123456789abcdefghjkmnpqrs/igc"
        );
        assert_eq!(share_path(&flight), "/f/0123456789abcdefghjkmnpqrs");
        assert_eq!(
            share_image_path(&flight),
            "/f/0123456789abcdefghjkmnpqrs/preview.png"
        );
        assert_eq!(session_path("abc"), "/v1/sessions/abc");
        // An identifier from somewhere else cannot leave its path segment.
        assert_eq!(session_path("../account"), "/v1/sessions/..%2Faccount");

        assert_eq!(FlightsRequest::default().path_and_query(), "/v1/flights");
        let page = FlightsRequest {
            after: Some("1783161000_0123456789abcdefghjkmnpqrs".to_owned()),
            limit: Some(20),
        };
        assert_eq!(
            page.path_and_query(),
            "/v1/flights?after=1783161000_0123456789abcdefghjkmnpqrs&limit=20"
        );

        assert_eq!(
            FlightChangesRequest::default().path_and_query(),
            "/v1/flights/changes"
        );
        let changes = FlightChangesRequest {
            since: Some("a b&c".to_owned()),
            limit: None,
        };
        assert_eq!(
            changes.path_and_query(),
            "/v1/flights/changes?since=a%20b%26c"
        );

        let pilot = PilotRequest {
            handle: Handle::new("Demo").unwrap(),
            after: None,
            limit: Some(5),
        };
        assert_eq!(pilot.path_and_query(), "/v1/pilots/demo?limit=5");
    }

    #[test]
    fn a_request_for_a_ranking_names_only_what_was_asked_for() {
        assert_eq!(RankingRequest::default().path_and_query(), "/v1/ranking");
        let year = RankingRequest {
            from: Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
            to: Some(Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap()),
            ..RankingRequest::default()
        };
        assert_eq!(
            year.path_and_query(),
            "/v1/ranking?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z"
        );
        let here = RankingRequest {
            bbox: Some(BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap()),
            limit: Some(20),
            ..year
        };
        assert_eq!(
            here.path_and_query(),
            "/v1/ranking?bbox=11.6,47.55,12,47.85\
             &from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z&limit=20"
        );
    }

    #[test]
    fn a_request_for_public_flights_names_only_what_was_asked_for() {
        assert_eq!(
            PublicFlightsRequest::default().path_and_query(),
            "/v1/flights/public"
        );
        let day = PublicFlightsRequest {
            from: Some(evening() - chrono::Duration::hours(18)),
            to: Some(evening() + chrono::Duration::hours(6)),
            sort: Some(FlightSort::Score),
            ..PublicFlightsRequest::default()
        };
        assert_eq!(
            day.path_and_query(),
            "/v1/flights/public?from=2026-11-18T00:00:00Z&to=2026-11-19T00:00:00Z&sort=score"
        );
        let here = PublicFlightsRequest {
            bbox: Some(BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap()),
            after: Some("p11.98_0123456789abcdefghjkmnpqrs".to_owned()),
            limit: Some(20),
            ..day
        };
        assert_eq!(
            here.path_and_query(),
            "/v1/flights/public?bbox=11.6,47.55,12,47.85\
             &from=2026-11-18T00:00:00Z&to=2026-11-19T00:00:00Z&sort=score\
             &after=p11.98_0123456789abcdefghjkmnpqrs&limit=20"
        );

        // The names of the orders are the values of the parameter.
        for sort in [FlightSort::Newest, FlightSort::Score] {
            assert_eq!(sort.as_str().parse(), Ok(sort));
        }
        assert_eq!(FlightSort::default(), FlightSort::Newest);
        for unknown in ["", "Newest", "points", "newest,score"] {
            assert_eq!(unknown.parse::<FlightSort>(), Err(FlightSortError));
        }
    }

    #[test]
    fn group_and_plan_requests_build_their_paths() {
        let group: GroupId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
        let plan: PlanId = "srqpnmkjhgfedcba9876543210".parse().unwrap();
        assert_eq!(group_path(&group), "/v1/groups/0123456789abcdefghjkmnpqrs");
        assert_eq!(
            group_invite_path(&group),
            "/v1/groups/0123456789abcdefghjkmnpqrs/invite"
        );
        assert_eq!(
            group_keeper_path(&group),
            "/v1/groups/0123456789abcdefghjkmnpqrs/keeper"
        );
        assert_eq!(
            group_member_path(&group, &Handle::new("Mara").unwrap()),
            "/v1/groups/0123456789abcdefghjkmnpqrs/members/mara"
        );
        assert_eq!(
            plan_path(&group, &plan),
            "/v1/groups/0123456789abcdefghjkmnpqrs/plans/srqpnmkjhgfedcba9876543210"
        );
        assert_eq!(
            plan_in_path(&group, &plan),
            "/v1/groups/0123456789abcdefghjkmnpqrs/plans/srqpnmkjhgfedcba9876543210/in"
        );

        let day = |day: u32| NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        let mut plans = PlansRequest {
            group,
            days: PlanDays::default(),
        };
        assert_eq!(
            plans.path_and_query(),
            "/v1/groups/0123456789abcdefghjkmnpqrs/plans"
        );
        plans.days.from = Some(day(18));
        assert_eq!(
            plans.path_and_query(),
            "/v1/groups/0123456789abcdefghjkmnpqrs/plans?from=2026-11-18"
        );
        assert_eq!(
            PlanBoardRequest::default().path_and_query(),
            "/v1/groups/plans"
        );
        // The board of one day.
        let today = PlanBoardRequest {
            days: PlanDays {
                from: Some(day(18)),
                to: Some(day(18)),
            },
        };
        assert_eq!(
            today.path_and_query(),
            "/v1/groups/plans?from=2026-11-18&to=2026-11-18"
        );
    }

    #[test]
    fn a_day_is_read_only_as_it_is_written() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        assert_eq!(format_day(day), "2026-01-05");
        assert_eq!(parse_day("2026-01-05"), Ok(day));
        for bad in [
            "",
            "2026-1-5",
            "+2026-01-05",
            " 2026-01-05",
            "2026-01-05 ",
            "2026-02-30",
            "05.01.2026",
            "2026-01-05T00:00:00Z",
            "today",
        ] {
            assert_eq!(parse_day(bad), Err(DayError(bad.to_owned())), "{bad:?}");
        }
        // What stands in a date's place is not repeated at length.
        let long = "x".repeat(500);
        assert_eq!(parse_day(&long), Err(DayError("x".repeat(24))));
    }

    #[test]
    fn live_requests_build_their_paths() {
        let track: LiveTrackId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
        assert_eq!(
            live_track_path(&track),
            "/v1/live/0123456789abcdefghjkmnpqrs"
        );
        assert_eq!(live_page_path(&track), "/live/0123456789abcdefghjkmnpqrs");

        let list = LiveTracksRequest {
            bbox: BoundingBox::new(47.55, 11.6, 47.85, 12.0).unwrap(),
        };
        assert_eq!(list.path_and_query(), "/v1/live?bbox=11.6,47.55,12,47.85");

        let mut one = LiveTrackRequest {
            id: track,
            since: None,
            after: None,
            limit: None,
        };
        assert_eq!(one.path_and_query(), "/v1/live/0123456789abcdefghjkmnpqrs");
        one.since = Some("1200".to_owned());
        one.after = Some(evening());
        one.limit = Some(500);
        assert_eq!(
            one.path_and_query(),
            "/v1/live/0123456789abcdefghjkmnpqrs\
             ?since=1200&after=2026-11-18T18:00:00Z&limit=500"
        );
    }

    #[test]
    fn unusual_station_keys_are_escaped_in_the_path() {
        let source = SourceId::new("example").unwrap();
        let station = StationId::new(source, "Hütte 7/b").unwrap();
        assert_eq!(
            observations_path(&station),
            "/v1/stations/example:H%C3%BCtte%207%2Fb/observations"
        );
    }
}
