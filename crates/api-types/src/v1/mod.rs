//! Version 1 of the API.
//!
//! | Path | Method | Request | Response |
//! |---|---|---|---|
//! | [`HEALTH_PATH`] | GET | | [`HealthResponse`] |
//! | [`STATIONS_PATH`] | GET | [`StationsRequest`] | [`StationsResponse`] |
//! | [`observations_path`] | GET | [`ObservationsRequest`] | [`ObservationsResponse`] |
//! | [`AIRSPACE_PATH`] | GET | [`AirspaceRequest`] | [`AirspaceResponse`] |
//! | [`SITES_PATH`] | GET | [`SitesRequest`] | [`SitesResponse`] |
//! | [`FORECAST_PATH`] | GET | [`ForecastRequest`] | [`ForecastResponse`] |
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
//! | [`PUBLIC_FLIGHTS_PATH`] | GET | [`PublicFlightsRequest`] | [`FlightsResponse`] |
//! | [`RANKING_PATH`] | GET | [`RankingRequest`] | [`RankingResponse`] |
//! | [`LIVE_POSITIONS_PATH`] | POST | [`LivePositionsRequest`] | [`LivePositionsResponse`] |
//! | [`LIVE_PATH`] | GET | [`LiveTracksRequest`] | [`LiveTracksResponse`] |
//! | [`live_track_path`] | GET | [`LiveTrackRequest`] | [`LiveTrackResponse`] |
//! | [`live_track_path`] | DELETE | | nothing |
//! | [`REPORTS_PATH`] | POST | [`ReportRequest`] | [`ReportResponse`] |
//! | [`GROUPS_PATH`] | POST | [`CreateGroupRequest`] | [`CreateGroupResponse`] |
//! | [`GROUPS_PATH`] | GET | | [`GroupsResponse`] |
//! | [`GROUP_JOIN_PATH`] | POST | [`JoinGroupRequest`] | [`JoinGroupResponse`] |
//! | [`group_path`] | GET, DELETE | | [`GroupResponse`], nothing |
//! | [`group_path`] | PATCH | [`UpdateGroupRequest`] | [`GroupResponse`] |
//! | [`group_invite_path`] | POST | | [`GroupInviteResponse`] |
//! | [`group_keeper_path`] | PUT | [`HandOverGroupRequest`] | [`GroupResponse`] |
//! | [`group_member_path`] | DELETE | | nothing |
//! | [`group_plans_path`] | POST | [`PlanRequest`] | [`PlanResponse`] |
//! | [`group_plans_path`] | GET | [`PlansRequest`] | [`PlansResponse`] |
//! | [`plan_path`] | PUT | [`PlanRequest`] | [`PlanResponse`] |
//! | [`plan_path`] | DELETE | | nothing |
//! | [`plan_in_path`] | PUT, DELETE | | [`PlanResponse`] |
//! | [`PLAN_BOARD_PATH`] | GET | [`PlanBoardRequest`] | [`PlanBoardResponse`] |
//! | [`MAPS_PATH`] | GET | | [`MapsResponse`] |
//! | [`map_file_path`] | GET | | the file, or the range of it asked for |
//!
//! A request that cannot be answered gets an [`ErrorResponse`] with a 4xx or
//! 5xx status.
//!
//! Everything about an account is asked for with a session token in the
//! `Authorization: Bearer` header. The server sets no cookies and reads
//! none.
//!
//! Beside the API the server answers [`share_path`], a page about one
//! flight for people and for the previews that chat apps make of a link,
//! and [`live_page_path`], a page about one live track.
//!
//! Units and conventions are those of `venti-domain`: UTC, WGS84 degrees,
//! metres above mean sea level, metres per second, and wind direction as the
//! direction the wind comes from.

mod account;
mod airspace;
mod attribution;
mod error;
mod flights;
mod forecast;
mod groups;
mod health;
mod live;
mod maps;
mod observations;
mod report;
mod request;
mod sites;
mod stations;
mod text;
mod thermals;

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
    MAX_PAGE_SIZE, Pilot, PilotResponse, RANKING_COUNTED, RankedPilot, RankingResponse, RouteKind,
    UpdateFlightRequest, UploadResponse, Visibility,
};
pub use forecast::{ForecastLayer, ForecastResponse};
pub use groups::{
    CreateGroupRequest, CreateGroupResponse, Group, GroupId, GroupIdError, GroupInviteCode,
    GroupInviteResponse, GroupResponse, GroupTextError, GroupsResponse, HandOverGroupRequest,
    JoinGroupRequest, JoinGroupResponse, MAX_BOARD_PLANS, MAX_GROUP_NAME_LENGTH,
    MAX_PLAN_NOTE_LENGTH, MAX_PLAN_PLACE_LENGTH, Plan, PlanBoardResponse, PlanId, PlanIdError,
    PlanRequest, PlanResponse, PlansResponse, TimeOfDay, TimeOfDayError, UpdateGroupRequest,
    parse_group_name, parse_plan_note, parse_plan_place,
};
pub use health::{
    ClientAddressesHealth, ForecastsHealth, HealthResponse, HealthStatus, ModelHealth,
    RegistrationMode, SourceHealth, ThermalMapHealth, ThermalMapModelHealth,
};
pub use live::{
    LIVE_LISTED_MINUTES, LiveFix, LivePositionsRequest, LivePositionsResponse, LiveTrack,
    LiveTrackId, LiveTrackIdError, LiveTrackResponse, LiveTracksResponse, LiveVisibility,
    MAX_LIVE_BATCH_FIXES, MAX_LIVE_BODY_BYTES, MAX_LIVE_FIXES, MAX_LIVE_TRACKS,
};
pub use maps::{
    MAPS_PATH, MAX_MAP_NAME_LENGTH, MapCredit, MapFile, MapFileRole, MapRegion, MapsResponse,
    is_map_name, map_file_path,
};
pub use observations::{ObservationsResponse, Reading};
pub use report::{
    MAX_REPORT_TEXT_LENGTH, ReportReason, ReportRequest, ReportResponse, ReportSubject,
    ReportTextError, ReportedPlan, parse_report_text,
};
pub use request::{
    ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNTS_PATH, AIRSPACE_PATH, AirspaceRequest, BboxError,
    DayError, FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FORECAST_PATH, FlightChangesRequest, FlightSort,
    FlightSortError, FlightsRequest, ForecastRequest, GROUP_JOIN_PATH, GROUPS_PATH, HEALTH_PATH,
    LIVE_PATH, LIVE_POSITIONS_PATH, LiveTrackRequest, LiveTracksRequest, ObservationsRequest,
    PLAN_BOARD_PATH, PUBLIC_FLIGHTS_PATH, PilotRequest, PlanBoardRequest, PlanDays, PlansRequest,
    PositionError, PublicFlightsRequest, RANKING_PATH, REPORTS_PATH, RankingRequest, SESSIONS_PATH,
    SITES_PATH, STATIONS_PATH, SitesRequest, StationsRequest, TimeError, flight_file_path,
    flight_path, format_bbox, format_day, format_time, group_invite_path, group_keeper_path,
    group_member_path, group_path, group_plans_path, live_page_path, live_track_path,
    observations_path, parse_bbox, parse_day, parse_position, parse_time, pilot_path, plan_in_path,
    plan_path, session_path, share_image_path, share_path,
};
pub use sites::{SiteLayer, SitesResponse};
pub use stations::{Station, StationWithLatest, StationsResponse};
pub use thermals::{
    THERMAL_GRID_DEG, THERMALS_PATH, ThermalGrid, ThermalKind, ThermalMapDay, ThermalMapResponse,
    ThermalReason, ThermalVerdict,
};

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

/// The most flying sites one response carries. A box that holds more is
/// refused with [`ErrorCode::BoxTooLarge`], as for airspace: a cut answer
/// would show places without sites that have them.
pub const MAX_SITES: usize = 5000;

/// The step of the grid a forecast's place is moved to, in degrees of
/// latitude and of longitude: about 2.2 km by 1.5 km at 47° north, the
/// width of a cell of the finest weather model in use. A forecast is the
/// same for every place in a cell of this grid, so that an answer can be
/// reused.
pub const FORECAST_GRID_DEG: f64 = 0.02;
