//! Version 1 of the API.
//!
//! | Path | Method | Request | Response |
//! |---|---|---|---|
//! | [`HEALTH_PATH`] | GET | | [`HealthResponse`] |
//! | [`STATIONS_PATH`] | GET | [`StationsRequest`] | [`StationsResponse`] |
//! | [`observations_path`] | GET | [`ObservationsRequest`] | [`ObservationsResponse`] |
//! | [`AIRSPACE_PATH`] | GET | [`AirspaceRequest`] | [`AirspaceResponse`] |
//! | [`ACCOUNTS_PATH`] | POST | [`CreateAccountRequest`] | [`SignInResponse`] |
//! | [`SESSIONS_PATH`] | POST | [`SignInRequest`] | [`SignInResponse`] |
//! | [`SESSIONS_PATH`] | GET, DELETE | | [`SessionsResponse`], nothing |
//! | [`session_path`] | DELETE | | nothing |
//! | [`ACCOUNT_PATH`] | GET | | [`AccountResponse`] |
//! | [`ACCOUNT_PATH`] | DELETE | [`DeleteAccountRequest`] | nothing |
//! | [`ACCOUNT_PASSWORD_PATH`] | PUT | [`ChangePasswordRequest`] | nothing |
//! | [`FLIGHTS_PATH`] | POST | the flight file | [`UploadResponse`] |
//! | [`FLIGHTS_PATH`] | GET | [`FlightsRequest`] | [`FlightsResponse`] |
//! | [`FLIGHT_CHANGES_PATH`] | GET | [`FlightChangesRequest`] | [`FlightChangesResponse`] |
//! | [`flight_path`] | GET, DELETE | | [`FlightResponse`], nothing |
//! | [`flight_path`] | PATCH | [`UpdateFlightRequest`] | [`FlightResponse`] |
//! | [`flight_file_path`] | GET | | the flight file |
//! | [`pilot_path`] | GET | [`PilotRequest`] | [`PilotResponse`] |
//!
//! A request that cannot be answered gets an [`ErrorResponse`] with a 4xx or
//! 5xx status.
//!
//! Everything about an account is asked for with a session token in the
//! `Authorization: Bearer` header. The server sets no cookies and reads
//! none.
//!
//! Beside the API the server answers [`share_path`], a page about one
//! flight for people and for the previews that chat apps make of a link.
//!
//! Units and conventions are those of `venti-domain`: UTC, WGS84 degrees,
//! metres above mean sea level, metres per second, and wind direction as the
//! direction the wind comes from.

mod account;
mod airspace;
mod attribution;
mod error;
mod flights;
mod health;
mod observations;
mod request;
mod stations;

pub use account::{
    Account, AccountResponse, ChangePasswordRequest, CreateAccountRequest, DeleteAccountRequest,
    DisplayNameError, Handle, HandleError, MAX_DISPLAY_NAME_LENGTH, Password, PasswordError,
    Session, SessionsResponse, SignInRequest, SignInResponse, parse_display_name,
};
pub use airspace::{AirspaceLayer, AirspaceResponse};
pub use attribution::Attribution;
pub use error::{ErrorBody, ErrorCode, ErrorResponse};
pub use flights::{
    DEFAULT_PAGE_SIZE, DeletedFlight, Flight, FlightChangesResponse, FlightId, FlightIdError,
    FlightPlace, FlightResponse, FlightScore, FlightsResponse, MAX_FLIGHT_FILE_BYTES,
    MAX_PAGE_SIZE, Pilot, PilotResponse, RouteKind, UpdateFlightRequest, UploadResponse,
    Visibility,
};
pub use health::{HealthResponse, HealthStatus, SourceHealth};
pub use observations::{ObservationsResponse, Reading};
pub use request::{
    ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNTS_PATH, AIRSPACE_PATH, AirspaceRequest, BboxError,
    FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FlightChangesRequest, FlightsRequest, HEALTH_PATH,
    ObservationsRequest, PilotRequest, SESSIONS_PATH, STATIONS_PATH, StationsRequest, TimeError,
    flight_file_path, flight_path, format_bbox, format_time, observations_path, parse_bbox,
    parse_time, pilot_path, session_path, share_image_path, share_path,
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

/// The most airspaces one response carries. A box that holds more is
/// refused with [`ErrorCode::BoxTooLarge`] and not cut short: a client
/// could take a cut answer for the whole airspace of the box, and a pilot
/// is not warned about airspace that is missing.
pub const MAX_AIRSPACES: usize = 5000;
