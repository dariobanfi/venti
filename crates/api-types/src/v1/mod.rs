//! Version 1 of the API.
//!
//! | Path | Response |
//! |---|---|
//! | [`HEALTH_PATH`] | [`HealthResponse`] |
//! | [`STATIONS_PATH`] | [`StationsResponse`] |
//! | [`observations_path`] | [`ObservationsResponse`] |
//!
//! A request that cannot be answered gets an [`ErrorResponse`] with a 4xx or
//! 5xx status.
//!
//! Units and conventions are those of `venti-domain`: UTC, WGS84 degrees,
//! metres above mean sea level, metres per second, and wind direction as the
//! direction the wind comes from.

mod attribution;
mod error;
mod health;
mod observations;
mod request;
mod stations;

pub use attribution::Attribution;
pub use error::{ErrorBody, ErrorCode, ErrorResponse};
pub use health::{HealthResponse, HealthStatus, SourceHealth};
pub use observations::{ObservationsResponse, Reading};
pub use request::{
    BboxError, HEALTH_PATH, ObservationsRequest, STATIONS_PATH, StationsRequest, TimeError,
    format_bbox, format_time, observations_path, parse_bbox, parse_time,
};
pub use stations::{Station, StationWithLatest, StationsResponse};

/// The window of observations returned when a request names no start, in hours.
pub const DEFAULT_WINDOW_HOURS: i64 = 24;

/// The longest window of observations one request may ask for, in days.
pub const MAX_WINDOW_DAYS: i64 = 7;

/// The most stations one response lists. A box that holds more gets the
/// first ones by source and identifier, and [`StationsResponse::truncated`]
/// is set.
pub const MAX_STATIONS: usize = 5000;
