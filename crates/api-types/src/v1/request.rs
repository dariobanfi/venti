//! How requests are written: paths, and the text form of their parameters.
//!
//! Both sides use these functions, the client to build a request and the
//! server to read it, so there is one definition of the format.

use chrono::{DateTime, SecondsFormat, Utc};
use venti_domain::{BoundingBox, GeoError, StationId, Timestamp};

use super::{FlightId, Handle};

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

/// Appends the parameters that have a value to a path.
fn with_query<const N: usize>(mut path: String, parameters: [(&str, Option<String>); N]) -> String {
    let mut separator = '?';
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
    fn unusual_station_keys_are_escaped_in_the_path() {
        let source = SourceId::new("example").unwrap();
        let station = StationId::new(source, "Hütte 7/b").unwrap();
        assert_eq!(
            observations_path(&station),
            "/v1/stations/example:H%C3%BCtte%207%2Fb/observations"
        );
    }
}
