//! How requests are written: paths, and the text form of their parameters.
//!
//! Both sides use these functions, the client to build a request and the
//! server to read it, so there is one definition of the format.

use chrono::{DateTime, SecondsFormat, Utc};
use venti_domain::{BoundingBox, GeoError, StationId, Timestamp};

/// Path of the health endpoint.
pub const HEALTH_PATH: &str = "/v1/health";

/// Path of the station list.
pub const STATIONS_PATH: &str = "/v1/stations";

/// Path of one station's observations.
pub fn observations_path(station: &StationId) -> String {
    format!(
        "{STATIONS_PATH}/{}/observations",
        encode_path_segment(&station.to_string())
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
        let mut text = observations_path(&self.station);
        let parameters = [("from", self.from), ("to", self.to)];
        let mut separator = '?';
        for (name, time) in parameters {
            if let Some(time) = time {
                text.push(separator);
                text.push_str(name);
                text.push('=');
                text.push_str(&format_time(time));
                separator = '&';
            }
        }
        text
    }
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

/// Escapes text for use as one segment of a URL path.
///
/// Station identifiers contain a provider's own station key, which may hold
/// any character. Letters, digits, `-`, `.`, `_`, `~` and `:` stay as they
/// are; every other byte becomes `%XX`.
fn encode_path_segment(text: &str) -> String {
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
