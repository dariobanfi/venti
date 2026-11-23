//! Version 1 of the API.
//!
//! | Path | Method | Request | Response |
//! |---|---|---|---|
//! | [`HEALTH_PATH`] | GET | | [`HealthResponse`] |
//! | [`STATIONS_PATH`] | GET | [`StationsRequest`] | [`StationsResponse`] |
//! | [`observations_path`] | GET | [`ObservationsRequest`] | [`ObservationsResponse`] |
//! | [`AIRSPACE_PATH`] | GET | [`AirspaceRequest`] | [`AirspaceResponse`] |
//! | [`SITES_PATH`] | GET | [`SitesRequest`] | [`SitesResponse`] |
//! | [`CABLES_PATH`] | GET | [`CablesRequest`] | [`CablesResponse`] |
//! | [`FORECAST_PATH`] | GET | [`ForecastRequest`] | [`ForecastResponse`] |
//! | [`ACCOUNTS_PATH`] | POST | [`CreateAccountRequest`] | [`SignInResponse`] |
//! | [`SESSIONS_PATH`] | POST | [`SignInRequest`] | [`SignInResponse`] |
//! | [`AUTH_GOOGLE_PATH`] | POST | [`GoogleSignInRequest`] | [`SignInResponse`] (201) or [`GoogleSignUpResponse`] (200) |
//! | [`AUTH_GOOGLE_ACCOUNT_PATH`] | POST | [`CreateGoogleAccountRequest`] | [`SignInResponse`] |
//! | [`ACCOUNT_GOOGLE_PATH`] | POST | [`LinkGoogleRequest`] | [`AccountResponse`] |
//! | [`ACCOUNT_GOOGLE_PATH`] | DELETE | | nothing |
//! | [`DEV_GOOGLE_ID_TOKEN_PATH`] | GET | `email`, `name` | [`DevGoogleIdTokenResponse`] (development mode only) |
//! | [`SESSIONS_PATH`] | GET, DELETE | | [`SessionsResponse`], nothing |
//! | [`session_path`] | DELETE | | nothing |
//! | [`ACCOUNT_PATH`] | GET | | [`AccountResponse`] |
//! | [`ACCOUNT_PATH`] | PATCH | [`UpdateAccountRequest`] | [`AccountResponse`] |
//! | [`ACCOUNT_PATH`] | DELETE | [`DeleteAccountRequest`] | nothing |
//! | [`ACCOUNT_ARCHIVE_PATH`] | GET | | a ZIP file; [`ArchiveIndex`] is its `index.json` |
//! | [`ACCOUNT_PASSWORD_PATH`] | PUT | [`ChangePasswordRequest`] | nothing |
//! | [`ACCOUNT_TOKENS_PATH`] | POST | [`CreateAccessTokenRequest`] | [`CreateAccessTokenResponse`] |
//! | [`ACCOUNT_TOKENS_PATH`] | GET | | [`AccessTokensResponse`] |
//! | [`account_token_path`] | DELETE | | nothing |
//! | [`oauth_request_path`] | GET | | [`AuthorizationRequestResponse`] |
//! | [`oauth_request_path`] | POST | [`AnswerAuthorizationRequest`] | [`AuthorizationAnswerResponse`] |
//! | [`KEPT_SITE_CHANGES_PATH`] | GET | [`KeptSiteChangesRequest`] | [`KeptSiteChangesResponse`] |
//! | [`kept_site_path`] | PUT | [`KeepSiteRequest`] | [`KeptSiteResponse`] |
//! | [`kept_site_path`] | DELETE | | nothing |
//! | [`ALERTS_PATH`] | GET | | [`AlertsResponse`] |
//! | [`ALERTS_PATH`] | POST | [`AlertRequest`] | [`AlertResponse`] |
//! | [`alert_path`] | PUT | [`AlertRequest`] | [`AlertResponse`] |
//! | [`alert_path`] | DELETE | | nothing |
//! | [`ALERTS_FIRED_PATH`] | GET | [`FiredAlertsRequest`] | [`FiredAlertsResponse`] |
//! | [`ACCOUNT_UNREAD_PATH`] | GET | | [`UnreadResponse`] |
//! | [`ACCOUNT_UNREAD_PATH`] | PUT | [`SeenRequest`], or nothing | [`UnreadResponse`] |
//! | [`FLIGHTS_PATH`] | POST | the flight file | [`UploadResponse`] |
//! | [`FLIGHTS_PATH`] | GET | [`FlightsRequest`] | [`FlightsResponse`] |
//! | [`FLIGHTS_PATH`] | PATCH | [`UpdateFlightsRequest`] | [`UpdateFlightsResponse`] |
//! | [`FLIGHT_CHANGES_PATH`] | GET | [`FlightChangesRequest`] | [`FlightChangesResponse`] |
//! | [`flight_path`] | GET, DELETE | | [`FlightResponse`], nothing |
//! | [`flight_path`] | PATCH | [`UpdateFlightRequest`] | [`FlightResponse`] |
//! | [`flight_file_path`] | GET | | the flight file |
//! | [`pilot_path`] | GET | [`PilotRequest`] | [`PilotResponse`] |
//! | [`PUBLIC_FLIGHTS_PATH`] | GET | [`PublicFlightsRequest`] | [`FlightsResponse`] |
//! | [`RANKING_PATH`] | GET | [`RankingRequest`] | [`RankingResponse`] |
//! | [`PILOTS_PATH`] | GET | [`PilotsRequest`] | [`PilotsResponse`] |
//! | [`pilot_statistics_path`] | GET | | [`PilotStatisticsResponse`] |
//! | [`ACCOUNT_STATISTICS_PATH`] | GET | | [`PilotStatisticsResponse`] |
//! | [`PILOTS_NEAR_PATH`] | GET | [`PilotsNearRequest`] | [`PilotsNearResponse`] |
//! | [`ACCOUNT_PICTURE_PATH`] | PUT | a JPEG, PNG or WebP picture | [`AccountResponse`] |
//! | [`ACCOUNT_PICTURE_PATH`] | DELETE | | [`AccountResponse`] |
//! | [`picture_path`] | GET | | the picture, WebP |
//! | [`ACCOUNT_HABITS_PATH`] | GET | | [`HabitsResponse`] |
//! | [`LIVE_POSITIONS_PATH`] | POST | [`LivePositionsRequest`] | [`LivePositionsResponse`] |
//! | [`LIVE_PATH`] | GET | [`LiveTracksRequest`] | [`LiveTracksResponse`] |
//! | [`LIVE_OTHERS_PATH`] | GET | [`LiveOthersRequest`] | [`LiveOthersResponse`] |
//! | [`LIVE_CLIMBS_PATH`] | GET | [`LiveClimbsRequest`] | [`LiveClimbsResponse`] |
//! | [`LIVE_SITES_PATH`] | GET | [`LiveSitesRequest`] | [`LiveSitesResponse`] |
//! | [`FLOWN_THERMALS_PATH`] | GET | [`FlownThermalsRequest`] | [`FlownThermalsResponse`] |
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
//! | [`OPEN_PLANS_PATH`] | POST | [`OpenPlanRequest`] | [`OpenPlanResponse`] |
//! | [`OPEN_PLANS_PATH`] | GET | [`OpenPlansRequest`] | [`OpenPlansResponse`] |
//! | [`open_plan_path`] | GET, DELETE | | [`OpenPlanResponse`], nothing |
//! | [`open_plan_path`] | PUT | [`OpenPlanRequest`] | [`OpenPlanResponse`] |
//! | [`open_plan_in_path`] | PUT, DELETE | | [`OpenPlanResponse`] |
//! | [`ACCOUNT_OPEN_PLANS_PATH`] | GET | | [`OpenPlansResponse`] |
//! | [`RADAR_PATH`] | GET | | [`RadarResponse`] |
//! | [`FOEHN_PATH`] | GET | | [`FoehnResponse`] |
//! | [`radar_frame_path`] | GET | | the frame's picture, a PNG |
//! | [`DUMPS_PATH`] | GET | | [`DumpsResponse`] |
//! | [`sites_dump_path`] | GET | | a layer of Venti's own sites as a file |
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
mod air;
mod airspace;
mod alerts;
mod archive;
mod attribution;
mod authorization;
mod cables;
mod dumps;
mod error;
mod flights;
mod foehn;
mod forecast;
mod groups;
mod health;
mod help;
mod kept_sites;
mod live;
mod maps;
mod observations;
mod open_plans;
mod patch;
mod phones;
mod pictures;
mod push;
mod radar;
mod report;
mod request;
mod route;
mod sites;
mod social;
mod stations;
mod takeoffs;
mod text;
mod thermals;
mod token;

pub use account::{
    Account, AccountResponse, ChangePasswordRequest, CreateAccountRequest,
    CreateGoogleAccountRequest, DeleteAccountRequest, DevGoogleIdTokenResponse, DisplayNameError,
    GoogleSignInRequest, GoogleSignUpResponse, Handle, HandleError, LinkGoogleRequest,
    MAX_DISPLAY_NAME_LENGTH, Password, PasswordError, Session, SessionsResponse, SignInRequest,
    SignInResponse, UpdateAccountRequest, parse_display_name,
};
pub use air::{
    FLOWN_CELL_DEG, FLOWN_MIN_PILOTS, FLOWN_THERMALS_PATH, FlownThermalCell, FlownThermalsRequest,
    FlownThermalsResponse, LIVE_CLIMB_CELL_DEG, LIVE_CLIMB_HEIGHT_STEP_M, LIVE_CLIMB_MINUTES,
    LIVE_CLIMBS_PATH, LIVE_SITES_PATH, LiveClimb, LiveClimbsLayer, LiveClimbsRequest,
    LiveClimbsResponse, LiveSitesLayer, LiveSitesRequest, LiveSitesResponse, MAX_FLOWN_BOX_DEG,
    MAX_FLOWN_CELLS, MAX_LIVE_CLIMBS, MAX_LIVE_SITES, SITE_AIR_HEARD_MINUTES, SITE_AIR_WITHIN_M,
    SITE_TAKEOFF_MINUTES, SITE_TAKEOFF_WITHIN_M, SiteAir, VENTI_LIVE_SOURCE, cell_middle,
    parse_flown_hours,
};
pub use airspace::{AirspaceLayer, AirspaceResponse};
pub use alerts::{
    ALERT_READING_FRESH_MINUTES, ALERT_REST_HOURS, ALERTS_FIRED_PATH, ALERTS_PATH, Alert,
    AlertFault, AlertHours, AlertReading, AlertRequest, AlertResponse, AlertRule, AlertSector,
    AlertsResponse, DayAlert, DayAtLeast, FIRED_KEPT_DAYS, FiredAlert, FiredAlertsRequest,
    FiredAlertsResponse, FiredBecause, MAX_ALERT_KMH, MAX_ALERT_NAME_LENGTH,
    MAX_ALERT_PLACE_LENGTH, MAX_ALERTS, MAX_DAYS_AHEAD, MAX_PILOTS_ALERT_PILOTS,
    MAX_PILOTS_REST_HOURS, PILOTS_ALERT_DEFAULT_PILOTS, PILOTS_ALERT_DEFAULT_REST_HOURS,
    PilotsAlert, WindAlert, alert_clock, alert_day, alert_path, weekday_name,
};
pub use archive::{
    ACCOUNT_ARCHIVE_PATH, ARCHIVE_FORMAT, ARCHIVE_INDEX_FILE, ARCHIVE_MEDIA_TYPE, ArchiveAlerts,
    ArchiveFile, ArchiveFlight, ArchiveIndex, ArchiveLicence, ArchiveReport, ArchiveReports,
    ArchiveSiteConfirmation, ArchiveSites, MAX_ARCHIVE_FILE_BYTES,
};
pub use attribution::Attribution;
pub use authorization::{
    AUTHORIZATION_REQUEST_PREFIX, AnswerAuthorizationRequest, AuthorizationAnswerResponse,
    AuthorizationRequestResponse, OAUTH_REQUESTS_PATH, is_authorization_request_id,
    oauth_request_path,
};
pub use cables::{CABLES_PATH, CableLayer, CablesRequest, CablesResponse, MAX_CABLES};
pub use dumps::{
    CSV_MEDIA_TYPE, DUMPS_PATH, DumpContent, DumpFile, DumpFormat, DumpLayer, DumpStamp,
    DumpsResponse, GEOJSON_MEDIA_TYPE, JSON_LINES_MEDIA_TYPE, SITES_CSV_COLUMNS, SiteHistoryHead,
    SiteHistoryLine, sites_dump_path,
};
pub use error::{ErrorBody, ErrorCode, ErrorResponse, ErrorSite};
pub use flights::{
    DEFAULT_PAGE_SIZE, DeletedFlight, FLIGHT_TERMS_UNDECIDED, Flight, FlightChangesResponse,
    FlightId, FlightIdError, FlightPlace, FlightResponse, FlightScore, FlightSite, FlightTerms,
    FlightTrack, FlightsResponse, GliderClass, MAX_FLIGHT_FILE_BYTES, MAX_PAGE_SIZE,
    MAX_TRACK_POINTS, Pilot, PilotResponse, PublicFlightChangesResponse, RANKING_COUNTED,
    RankedPilot, RankingResponse, RouteKind, TAKEOFF_SITE_WITHIN_M, TrackLine, TrackProperties,
    UpdateFlightRequest, UpdateFlightsRequest, UpdateFlightsResponse, UploadResponse, Visibility,
    WithdrawnFlight, country_code, is_a_takeoff, takeoff_site,
};
pub use flights::{
    FlightHabits, FlownSite, FoundPilot, HabitsResponse, HighestFlight, LongestFlight,
    PILOT_SEARCH_MAX_CHARS, PILOT_SEARCH_MIN_CHARS, PILOT_SITE_STEP_DEG, PILOT_TOP_SITES,
    PilotSeason, PilotStatisticsResponse, PilotTotals, PilotsResponse, RankingMonth, RankingYear,
    YearMonth, YearMonthError,
};
pub use foehn::{
    FOEHN_PATH, FoehnLevel, FoehnPair, FoehnPoint, FoehnPressure, FoehnReducedBy, FoehnResponse,
    FoehnSide, FoehnStation, FoehnUnknown,
};
pub use forecast::{ForecastLayer, ForecastNearby, ForecastResponse, NearbyHour, NearbyStrongest};
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
    PhonePushHealth, RadarHealth, RegistrationMode, SitesFromPilots, SourceHealth,
    ThermalMapHealth, ThermalMapModelHealth,
};
pub use help::{
    HELP_EMERGENCY_WORDS, HELP_OPEN_HOURS, HELP_PATH, HELP_RADIUS_KM, HELP_REQUESTS_PER_DAY,
    HelpAnswerBody, HelpListResponse, HelpRequest, HelpRequestBody, HelpResponse,
    MAX_HELP_NOTE_LENGTH, MAX_HELP_RECIPIENTS, help_answer_path, help_answer_words, help_path,
    help_request_words, parse_help_note,
};
pub use kept_sites::{
    KEPT_SITE_CHANGES_PATH, KEPT_SITES_PATH, KeepSiteRequest, KeptSite, KeptSiteChangesRequest,
    KeptSiteChangesResponse, KeptSiteResponse, MAX_KEPT_CREDIT_LENGTH, MAX_KEPT_SITE_BODY_BYTES,
    MAX_KEPT_SITES, kept_site_path,
};
pub use live::{
    AircraftKind, LIVE_LISTED_MINUTES, LIVE_OTHERS_LISTED_MINUTES, LiveFix, LiveOthersLayer,
    LiveOthersResponse, LivePositionsRequest, LivePositionsResponse, LiveTrack, LiveTrackId,
    LiveTrackIdError, LiveTrackResponse, LiveTracksResponse, LiveVisibility, MAX_LIVE_BATCH_FIXES,
    MAX_LIVE_BODY_BYTES, MAX_LIVE_FIXES, MAX_LIVE_OTHERS, MAX_LIVE_OTHERS_BOX_DEG, MAX_LIVE_TRACKS,
    OtherAircraft,
};
pub use maps::{
    MAPS_PATH, MAX_MAP_NAME_LENGTH, MapCredit, MapFile, MapFileRole, MapRegion, MapsResponse,
    is_map_name, map_file_path,
};
pub use observations::{ObservationsResponse, Reading};
pub use open_plans::{
    ACCOUNT_OPEN_PLANS_PATH, DEFAULT_OPEN_PLANS_RADIUS_KM, MAX_OPEN_PLAN_NOTE_LENGTH,
    MAX_OPEN_PLAN_SEATS, MAX_OPEN_PLANS, MAX_OPEN_PLANS_RADIUS_KM, OPEN_PLAN_DAYS_AHEAD,
    OPEN_PLANS_DAYS, OPEN_PLANS_PATH, OpenPlan, OpenPlanRequest, OpenPlanResponse,
    OpenPlansRequest, OpenPlansResponse, open_plan_in_path, open_plan_path, open_plan_words,
    parse_open_plan_note,
};
pub use phones::{
    DEFAULT_NEARBY_RADIUS_KM, DEVICES_PATH, DeviceRemoveRequest, DeviceRequest,
    MAX_DEVICE_TOKEN_LENGTH, MAX_DEVICES, MAX_NEARBY_RADIUS_KM, NEARBY_PATH, NearbyCircle,
    NearbySettings, PHONE_PUSH_TTL_SECONDS, POSITION_GRID_DEG, POSITION_KEPT_MINUTES,
    POSITION_PATH, PhoneMessage, PhoneMessageKind, PositionRequest, PushPlatform, coarse_position,
};
pub use pictures::{
    ACCOUNT_PICTURE_PATH, DEFAULT_PILOTS_NEAR, DEFAULT_PILOTS_NEAR_RADIUS_KM, MAX_PICTURE_BYTES,
    MAX_PICTURE_PIXELS, MAX_PICTURE_SIDE, MAX_PILOTS_NEAR, MAX_PILOTS_NEAR_RADIUS_KM,
    MIN_PICTURE_SIDE, NearPilot, PICTURE_MEDIA_TYPE, PICTURE_MEDIA_TYPES, PICTURE_SIDE,
    PICTURES_PER_DAY, PILOTS_NEAR_MONTHS, PILOTS_NEAR_PATH, PILOTS_NEAR_TOP_SITES,
    PilotsNearRequest, PilotsNearResponse, picture_path,
};
pub use push::{
    MAX_PUSH_ENDPOINT_LENGTH, MAX_PUSH_SUBSCRIPTIONS, PUSH_KEY_PATH, PUSH_SUBSCRIPTION_PATH,
    PUSH_TTL_SECONDS, PushKeyResponse, PushKeys, PushMessage, PushRemoveRequest,
    PushSubscriptionRequest, alert_utc_offset_s, fired_words,
};
pub use radar::{
    RADAR_PATH, RadarFrameRef, RadarLayer, RadarResponse, format_radar_time, parse_radar_time,
    radar_frame_path,
};
pub use report::{
    MAX_REPORT_TEXT_LENGTH, ReportReason, ReportRequest, ReportResponse, ReportSubject,
    ReportTextError, ReportedComment, ReportedPlan, ReportedSite, parse_report_text,
};
pub use request::{
    ACCOUNT_GOOGLE_PATH, ACCOUNT_HABITS_PATH, ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH,
    ACCOUNT_STATISTICS_PATH, ACCOUNT_TOKENS_PATH, ACCOUNTS_PATH, AIRSPACE_PATH,
    AUTH_GOOGLE_ACCOUNT_PATH, AUTH_GOOGLE_PATH, AirspaceRequest, BboxError,
    DEV_GOOGLE_ID_TOKEN_PATH, DayError, FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FORECAST_PATH,
    FlightChangesRequest, FlightSort, FlightSortError, FlightsRequest, ForecastRequest,
    GROUP_JOIN_PATH, GROUPS_PATH, HEALTH_PATH, LIVE_OTHERS_PATH, LIVE_PATH, LIVE_POSITIONS_PATH,
    LiveOthersRequest, LiveTrackRequest, LiveTracksRequest, ObservationsRequest, PILOTS_PATH,
    PLAN_BOARD_PATH, PUBLIC_FLIGHT_CHANGES_PATH, PUBLIC_FLIGHTS_PATH, PilotRequest, PilotsRequest,
    PlanBoardRequest, PlanDays, PlansRequest, PositionError, PublicFlightChangesRequest,
    PublicFlightsRequest, RANKING_PATH, REPORTS_PATH, RankingRequest, SESSIONS_PATH,
    SITE_CANDIDATES_PATH, SITE_CHANGES_PATH, SITES_PATH, STATIONS_PATH, SitesRequest,
    StationsRequest, TimeError, account_token_path, flight_file_path, flight_path,
    flight_track_path, format_bbox, format_day, format_time, group_invite_path, group_keeper_path,
    group_member_path, group_path, group_plans_path, live_page_path, live_track_path,
    observations_path, parse_bbox, parse_day, parse_position, parse_time, pilot_path,
    pilot_statistics_path, plan_in_path, plan_path, session_path, share_image_path, share_path,
    site_candidates_path, site_confirmation_path, site_confirmations_path, site_flights_path,
    site_history_path, site_path, site_revisions_path, sites_path,
};
pub use route::{
    ROUTE_POINTS_AT_MOST, ROUTE_WEATHER_PATH, RoutePointsError, RouteWeatherHour,
    RouteWeatherPoint, RouteWeatherRequest, RouteWeatherResponse, RouteWindAloft,
    parse_route_points,
};
pub use sites::{
    AddSiteRequest, CONFIRMATION_DAYS_BACK, CONFIRMATION_FLIGHT_WITHIN_M, ChangeSiteRequest,
    ConfirmSiteRequest, DEFAULT_SITE_LAYERS, MAX_SITE_COMMENT_LENGTH, MAX_SITE_CONFIRMATIONS,
    RemoveSiteRequest, RestoreSiteRequest, SAME_PLACE_M, SamePlace, SiteChangesResponse,
    SiteConfirmation, SiteConfirmationsResponse, SiteConfirmed, SiteDraft, SiteHistoryResponse,
    SiteLayer, SitePilot, SiteRecord, SiteRemoval, SiteResponse, SiteRevision, SiteRevisionKind,
    SiteStanding, SiteVia, SitesResponse, VENTI_PILOTS_SITES_SOURCE, VENTI_SITES_LICENCE,
    VENTI_SITES_LICENCE_URL, VENTI_SITES_SOURCE, parse_site_comment,
};
pub use social::{
    ACCOUNT_FEED_PATH, ACCOUNT_FOLLOWING_PATH, ACCOUNT_NOTIFICATIONS_PATH, ACCOUNT_UNREAD_PATH,
    ArchiveCheer, ArchiveComment, ArchiveSocial, COMMENTS_PER_DAY, Cheer, CheerResponse,
    CheersResponse, Comment, CommentError, CommentRequest, CommentResponse, CommentsOpen,
    CommentsResponse, FOLLOWS_AND_CHEERS_PER_DAY, FeedFlight, FeedResponse, FollowResponse,
    FollowedPilot, FollowsResponse, MAX_COMMENT_LENGTH, MAX_FOLLOWS, NOTIFICATIONS_KEPT_DAYS,
    Notification, NotificationKind, NotificationsResponse, NotifiedFlight, SeenRequest,
    UNREAD_COUNTED_UP_TO, UnreadResponse, feed_path, flight_cheer_path, flight_cheers_path,
    flight_comment_path, flight_comments_path, notifications_path, parse_comment,
    pilot_follow_path, pilot_followers_path, pilot_following_path, social_page_path,
};
pub use stations::{Station, StationWithLatest, StationsResponse};
pub use takeoffs::{
    CANDIDATE_CELL_DEG, CANDIDATE_CLEAR_OF_SITES_M, CANDIDATE_MIN_PILOTS,
    CANDIDATE_POSITION_STEP_DEG, MAX_CANDIDATE_BOX_DEG, MAX_SITE_CANDIDATES, SITE_BEST_FLIGHTS,
    SITE_FLIGHTS_WITHIN_M, SiteBestFlight, SiteCandidate, SiteCandidatesResponse,
    SiteFlightsResponse,
};
pub use thermals::{
    THERMAL_GRID_DEG, THERMAL_HOURS_PATH, THERMALS_PATH, ThermalGrid, ThermalHoursRequest,
    ThermalHoursResponse, ThermalKind, ThermalLayer, ThermalMapDay, ThermalMapHour,
    ThermalMapResponse, ThermalReason, ThermalVerdict,
};
pub use token::{
    ACCESS_TOKEN_PREFIX, AccessToken, AccessTokensResponse, CreateAccessTokenRequest,
    CreateAccessTokenResponse, MAX_TOKEN_NAME_LENGTH, Scope, TokenClient, TokenNameError,
    parse_token_name,
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
