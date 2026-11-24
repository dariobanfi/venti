//! The description of `/v1` that a program reads: an OpenAPI 3.1 document,
//! served at [`OPENAPI_PATH`] and kept in the repository as
//! `docs/openapi.json`.
//!
//! It is made here, from two things that are written by hand:
//!
//! - **The table of operations** in [`operations`]: one line of Rust for
//!   every method on every path, with who may ask, the parameters, the
//!   body and the answer. The paths are the constants the router uses and
//!   the scopes are [`Scope`]'s own names, so neither is written twice.
//! - **The schemas** in `openapi_schemas.json`: one for every record of
//!   `venti-api-types` that goes over the wire, in the part of JSON Schema
//!   that the tests' own validator reads.
//!
//! Nothing derives a schema from the Rust types: that would be a new
//! dependency in two crates, and `docs/architecture.md` says why it was
//! not taken. What holds the description to the code instead is three
//! tests in `crates/server/tests/openapi.rs`: every route of the router is
//! described and every described operation is routed; every golden answer
//! in `crates/server/tests/golden` is valid against the schema of its
//! operation, with no field the schema does not name; and the document the
//! server hands out is the file in the repository.
//!
//! What the document does not say, `docs/api.md` does: when an error comes
//! with which status, what every field means, the bounds. Every operation
//! names its heading there (`x-venti-docs`).

use std::sync::LazyLock;

use axum::http::HeaderMap;
use axum::response::Response;
use serde_json::{Map, Value, json};
use venti_api_types::v1::{
    ACCESS_TOKEN_PREFIX, ACCOUNT_ARCHIVE_PATH, ACCOUNT_FEED_PATH, ACCOUNT_FOLLOWING_PATH,
    ACCOUNT_GOOGLE_PATH, ACCOUNT_HABITS_PATH, ACCOUNT_NOTIFICATIONS_PATH, ACCOUNT_PASSWORD_PATH,
    ACCOUNT_PATH, ACCOUNT_STATISTICS_PATH, ACCOUNT_TOKENS_PATH, ACCOUNT_UNREAD_PATH, ACCOUNTS_PATH,
    AIRSPACE_PATH, ALERTS_FIRED_PATH, ALERTS_PATH, ARCHIVE_MEDIA_TYPE, AUTH_GOOGLE_ACCOUNT_PATH,
    AUTH_GOOGLE_PATH, CABLES_PATH, CSV_MEDIA_TYPE, DEV_GOOGLE_ID_TOKEN_PATH, DUMPS_PATH,
    FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FORECAST_PATH, GEOJSON_MEDIA_TYPE, GROUP_JOIN_PATH,
    GROUPS_PATH, HEALTH_PATH, JSON_LINES_MEDIA_TYPE, KEPT_SITE_CHANGES_PATH, LIVE_OTHERS_PATH,
    LIVE_PATH, LIVE_POSITIONS_PATH, MAPS_PATH, PILOTS_PATH, PLAN_BOARD_PATH,
    PUBLIC_FLIGHT_CHANGES_PATH, PUBLIC_FLIGHTS_PATH, PUSH_KEY_PATH, PUSH_SUBSCRIPTION_PATH,
    RADAR_PATH, RANKING_PATH, REGION_FORECAST_PATH, REGIONS_PATH, REPORTS_PATH, ROUTE_WEATHER_PATH,
    SESSIONS_PATH, SITE_CANDIDATES_PATH, SITE_CHANGES_PATH, SITES_PATH, STATIONS_PATH, Scope,
    THERMAL_HOURS_PATH, THERMALS_PATH,
};
use venti_api_types::v1::{FLOWN_THERMALS_PATH, LIVE_CLIMBS_PATH, LIVE_SITES_PATH};

use super::cache;

/// Where the description is served.
pub const OPENAPI_PATH: &str = "/v1/openapi.json";

/// The schemas of the records that go over the wire, by the name of
/// their type in `venti-api-types`.
const SCHEMAS: &str = include_str!("openapi_schemas.json");

/// The examples of request bodies and answers, by operation (`get
/// /v1/health`) and part (`request`, or the status), each by the name of
/// the golden file it is. Written by the test
/// `the_examples_are_the_golden_answers` in `crates/server/tests/openapi.rs`
/// from the golden files and the table that names each one's operation,
/// so that every example is an answer the development mode gave.
const EXAMPLES: &str = include_str!("openapi_examples.json");

/// The groups the operations are shown in, in order, each with what it
/// holds and the beginnings of the paths that are its. An operation is in
/// the group whose beginning of a path is the longest that its path
/// begins with.
const TAGS: &[(&str, &str, &[&str])] = &[
    (
        "Server",
        "How the server and its sources are doing, and this description.",
        &["/v1/health", "/v1/openapi.json", "/v1/dev/"],
    ),
    (
        "Weather",
        "Stations, forecasts, thermals, the forecast by region, the weather along a route, rain radar and foehn.",
        &[
            "/v1/stations",
            "/v1/forecast",
            "/v1/thermals",
            "/v1/regions",
            "/v1/route/weather",
            "/v1/radar",
            "/v1/foehn",
        ],
    ),
    (
        "Airspace and cables",
        "Airspace and the cables hung in the air, in layers by source.",
        &["/v1/airspace", "/v1/cables"],
    ),
    (
        "Sites",
        "Flying sites of every layer, those of Venti's partners and pilots with their history, \
         what public flights say of a site, and the dumps of Venti's own layers.",
        &["/v1/sites", "/v1/dumps"],
    ),
    (
        "Maps",
        "Map files for regions and map imagery.",
        &["/v1/maps", "/v1/imagery"],
    ),
    (
        "Accounts",
        "Accounts, sign-in, sessions, pilots' pictures, tokens for other programs, an \
         assistant's agreement and everything a pilot gave.",
        &[
            "/v1/accounts",
            "/v1/account",
            "/v1/sessions",
            "/v1/auth",
            "/v1/oauth",
            "/v1/pictures",
        ],
    ),
    (
        "Flights",
        "The pilot's own flights: the logbook on the account, its changes, statistics and how \
         each flight was flown; one flight, its file and its track.",
        &[
            "/v1/flights",
            "/v1/account/statistics",
            "/v1/account/habits",
        ],
    ),
    (
        "Public flights",
        "Everyone's public flights as data, pilots' pages and statistics, and the ranking.",
        &["/v1/flights/public", "/v1/pilots", "/v1/ranking"],
    ),
    (
        "Pilots among each other",
        "Following pilots, cheers and comments, the feed and notifications.",
        &[
            "/v1/pilots/{handle}/follow",
            "/v1/flights/{id}/cheer",
            "/v1/flights/{id}/comments",
            "/v1/account/feed",
            "/v1/account/following",
            "/v1/account/notifications",
            "/v1/account/unread",
        ],
    ),
    (
        "Kept sites",
        "The sites a pilot keeps, in step across their devices.",
        &["/v1/account/kept-sites"],
    ),
    (
        "Alarms",
        "Alarms for wind and pilots, and Web Push.",
        &["/v1/account/alerts", "/v1/account/push", "/v1/push"],
    ),
    (
        "Phones and help",
        "Phones registered for push, open plans and help near the pilot, and asking pilots \
         nearby for a lift after landing.",
        &[
            "/v1/account/devices",
            "/v1/account/nearby",
            "/v1/account/position",
            "/v1/help",
        ],
    ),
    (
        "Live tracks",
        "Live positions in the air, and what they say about climbs and sites.",
        &["/v1/live"],
    ),
    (
        "Groups and plans",
        "Groups of pilots and their plans for a flying day, and plans open to everyone flying from an area.",
        &["/v1/groups", "/v1/open-plans", "/v1/account/open-plans"],
    ),
    (
        "Reports",
        "Telling whoever runs the server of something that should not be there.",
        &["/v1/reports"],
    ),
];

/// The group of an operation on this path. `None` only for a path no
/// group begins, which a test rules out.
fn tag_of(path: &str) -> Option<&'static str> {
    TAGS.iter()
        .flat_map(|(tag, _, beginnings)| beginnings.iter().map(move |begins| (*tag, *begins)))
        .filter(|(_, begins)| path.starts_with(begins))
        .max_by_key(|(_, begins)| begins.len())
        .map(|(tag, _)| tag)
}

/// The examples of one part of an operation, as the media type's
/// `examples` holds them, if there are any.
fn examples_of(examples: &Value, op: &Op, part: &str) -> Option<Value> {
    let found = examples
        .get(format!("{} {}", op.method, op.path))?
        .get(part)?
        .as_object()?;
    Some(Value::Object(
        found
            .iter()
            .map(|(name, value)| (name.clone(), json!({"value": value})))
            .collect(),
    ))
}

/// The examples, read once.
fn examples() -> &'static Value {
    static EXAMPLE_FILE: LazyLock<Value> = LazyLock::new(|| {
        serde_json::from_str(EXAMPLES).unwrap_or_else(|error| {
            // The file is part of the program; a test writes it.
            tracing::error!(%error, "the examples of the API's description cannot be read");
            json!({})
        })
    });
    &EXAMPLE_FILE
}

/// `GET /v1/openapi.json`: the description, the same for everyone.
pub(crate) async fn show(headers: HeaderMap) -> Response {
    cache::slow_json(&headers, document())
}

/// The description, made once.
pub fn document() -> &'static Value {
    static DOCUMENT: LazyLock<Value> = LazyLock::new(build);
    &DOCUMENT
}

/// Who may ask.
#[derive(Clone, Copy)]
enum Who {
    /// Anyone, with no token.
    Anyone,
    /// The pilot: with a session, or a token that has the scope.
    With(Scope),
    /// The pilot's own sign-in only; no token for a program.
    Session,
    /// Anyone for what its pilot shares; the pilot, with a session or a
    /// token that has the scope, for what is private.
    SharedOr(Scope),
}

/// What a request carries.
#[derive(Clone, Copy)]
enum Takes {
    Nothing,
    /// A JSON body of this schema.
    Json(&'static str),
    /// A JSON body of this schema, which may be left out.
    JsonIfAny(&'static str),
    /// A flight file as it is.
    File,
    /// A picture as it is: a JPEG, a PNG or a WebP.
    Picture,
}

/// What an answer carries when the request succeeds.
#[derive(Clone, Copy)]
enum Gives {
    Nothing,
    /// JSON of this schema.
    Json(&'static str),
    /// JSON of this schema under a media type of its own.
    JsonAs(&'static str, &'static str),
    /// Bytes of this media type, and what they are.
    Bytes(&'static str, &'static str),
}

/// One method on one path.
struct Op {
    method: &'static str,
    path: &'static str,
    summary: &'static str,
    /// The heading of `docs/api.md` that describes it.
    docs: &'static str,
    who: Who,
    query: &'static [&'static str],
    takes: Takes,
    /// The statuses of success, the usual one first.
    statuses: &'static [u16],
    gives: Gives,
}

impl Op {
    fn new(method: &'static str, path: &'static str, summary: &'static str) -> Self {
        Self {
            method,
            path,
            summary,
            docs: "",
            who: Who::Anyone,
            query: &[],
            takes: Takes::Nothing,
            statuses: &[200],
            gives: Gives::Nothing,
        }
    }

    /// The heading of `docs/api.md`, where it is not `METHOD path`.
    fn docs(mut self, heading: &'static str) -> Self {
        self.docs = heading;
        self
    }

    fn query(mut self, query: &'static [&'static str]) -> Self {
        self.query = query;
        self
    }

    fn with(mut self, scope: Scope) -> Self {
        self.who = Who::With(scope);
        self
    }

    fn session(mut self) -> Self {
        self.who = Who::Session;
        self
    }

    fn shared_or(mut self, scope: Scope) -> Self {
        self.who = Who::SharedOr(scope);
        self
    }

    fn takes(mut self, schema: &'static str) -> Self {
        self.takes = Takes::Json(schema);
        self
    }

    fn takes_if_any(mut self, schema: &'static str) -> Self {
        self.takes = Takes::JsonIfAny(schema);
        self
    }

    fn takes_a_file(mut self) -> Self {
        self.takes = Takes::File;
        self
    }

    fn takes_a_picture(mut self) -> Self {
        self.takes = Takes::Picture;
        self
    }

    fn json(mut self, schema: &'static str) -> Self {
        self.gives = Gives::Json(schema);
        self
    }

    fn json_as(mut self, media_type: &'static str, schema: &'static str) -> Self {
        self.gives = Gives::JsonAs(media_type, schema);
        self
    }

    fn bytes(mut self, media_type: &'static str, what: &'static str) -> Self {
        self.gives = Gives::Bytes(media_type, what);
        self
    }

    fn statuses(mut self, statuses: &'static [u16]) -> Self {
        self.statuses = statuses;
        self
    }

    fn nothing(mut self) -> Self {
        self.statuses = &[204];
        self.gives = Gives::Nothing;
        self
    }
}

fn get(path: &'static str, summary: &'static str) -> Op {
    Op::new("get", path, summary)
}

fn post(path: &'static str, summary: &'static str) -> Op {
    Op::new("post", path, summary)
}

fn put(path: &'static str, summary: &'static str) -> Op {
    Op::new("put", path, summary)
}

fn patch(path: &'static str, summary: &'static str) -> Op {
    Op::new("patch", path, summary)
}

fn delete(path: &'static str, summary: &'static str) -> Op {
    Op::new("delete", path, summary)
}

/// Every operation of `/v1`, in the order of `docs/api.md`.
///
/// A query parameter is named as in [`query_parameter`]: its name, and
/// after a colon what it is where the name alone does not say.
#[allow(clippy::too_many_lines)]
fn operations() -> Vec<Op> {
    use Scope::{
        AccountRead, AlertsRead, AlertsWrite, FlightsRead, FlightsWrite, GroupsRead, GroupsWrite,
        KeptSitesRead, KeptSitesWrite, LiveWrite, ReportsWrite, SitesWrite, SocialRead,
        SocialWrite,
    };
    const PAGE: &[&str] = &["after:cursor", "limit"];
    const SINCE: &[&str] = &["since:cursor", "limit"];
    const DAYS: &[&str] = &["from:day", "to:day"];
    vec![
        // What anyone may read.
        get(
            HEALTH_PATH,
            "How the server and each of its sources are doing",
        )
        .json("HealthResponse"),
        get(OPENAPI_PATH, "This description").bytes("application/json", "An OpenAPI 3.1 document"),
        get(
            STATIONS_PATH,
            "The weather stations of a box, each with its latest reading",
        )
        .docs("GET /v1/stations?bbox=west,south,east,north")
        .query(&["bbox!"])
        .json("StationsResponse"),
        get(
            "/v1/stations/{id}/observations",
            "What one station measured over a span of time",
        )
        .docs("GET /v1/stations/{id}/observations?from=&to=")
        .query(&["from:time", "to:time"])
        .json("ObservationsResponse"),
        get(AIRSPACE_PATH, "The airspace of a box, in layers by source")
            .docs("GET /v1/airspace?bbox=west,south,east,north")
            .query(&["bbox!"])
            .json("AirspaceResponse"),
        get(CABLES_PATH, "The cables of a box, in layers by source")
            .docs("GET /v1/cables?bbox=west,south,east,north")
            .query(&["bbox!"])
            .json("CablesResponse"),
        get(SITES_PATH, "The flying sites of a box, in layers by source")
            .docs("GET /v1/sites?bbox=west,south,east,north")
            .query(&["bbox!", "layers"])
            .json("SitesResponse"),
        post(
            SITES_PATH,
            "Add a site: a partner's to the layer `venti`, any other pilot's to `venti-pilots`",
        )
        .with(SitesWrite)
        .takes("AddSiteRequest")
        .statuses(&[201])
        .json("SiteResponse"),
        get(
            SITE_CHANGES_PATH,
            "What changed in one of Venti's own two layers of sites, removals included",
        )
        .docs("GET /v1/sites/changes?layer=&since=&limit=")
        .query(&["layer:sites", "since:cursor", "limit"])
        .json("SiteChangesResponse"),
        get("/v1/sites/{id}", "One site of any layer").json("SiteResponse"),
        put(
            "/v1/sites/{id}",
            "Correct a site of one of Venti's own layers",
        )
        .with(SitesWrite)
        .takes("ChangeSiteRequest")
        .json("SiteResponse"),
        delete(
            "/v1/sites/{id}",
            "Remove a site of one of Venti's own layers, as a new revision",
        )
        .with(SitesWrite)
        .takes_if_any("RemoveSiteRequest")
        .nothing(),
        get(
            "/v1/sites/{id}/history",
            "The revisions of a site of one of Venti's own layers",
        )
        .docs("GET /v1/sites/{id}/history?after=&limit=")
        .query(&["after:revision", "limit"])
        .json("SiteHistoryResponse"),
        post(
            "/v1/sites/{id}/revisions",
            "Put a site back to an earlier revision",
        )
        .with(SitesWrite)
        .takes("RestoreSiteRequest")
        .json("SiteResponse"),
        put(
            "/v1/sites/{id}/confirmation",
            "Say \"I took off here and it was good\"; again to move its day",
        )
        .with(SitesWrite)
        .takes("ConfirmSiteRequest")
        .json("SiteResponse"),
        delete(
            "/v1/sites/{id}/confirmation",
            "Take the caller's own confirmation of a site back",
        )
        .with(SitesWrite)
        .nothing(),
        get(
            "/v1/sites/{id}/confirmations",
            "Who says they took off at a site, the newest day first",
        )
        .json("SiteConfirmationsResponse"),
        get(
            "/v1/sites/{id}/flights",
            "What the public flights that began at a site of any layer say about it",
        )
        .json("SiteFlightsResponse"),
        get(
            SITE_CANDIDATES_PATH,
            "The places in a box where pilots take off and no site stands",
        )
        .docs("GET /v1/sites/candidates?bbox=west,south,east,north")
        .query(&["bbox!"])
        .json("SiteCandidatesResponse"),
        get(
            DUMPS_PATH,
            "The dumps of Venti's own two layers of sites: every file with its licence",
        )
        .json("DumpsResponse"),
        get(
            "/v1/dumps/sites/{layer}.geojson",
            "The sites of one of Venti's own layers as they stand, as GeoJSON",
        )
        .bytes(
            GEOJSON_MEDIA_TYPE,
            "A FeatureCollection that names its licence, its layer and its cursor",
        ),
        get(
            "/v1/dumps/sites/{layer}.csv",
            "The sites of one of Venti's own layers as they stand, as CSV",
        )
        .bytes(
            CSV_MEDIA_TYPE,
            "A line of column names and a line for every site",
        ),
        get(
            "/v1/dumps/sites/{layer}.history.jsonl",
            "Every site of one of Venti's own layers with every revision, as JSON Lines",
        )
        .bytes(
            JSON_LINES_MEDIA_TYPE,
            "A first line that names the licence, the layer and the cursor, and a line for \
             every site",
        ),
        get(FORECAST_PATH, "The forecast of a place, in layers by model")
            .docs("GET /v1/forecast?lat=&lon=")
            .query(&["lat!", "lon!"])
            .json("ForecastResponse"),
        get(THERMALS_PATH, "The thermal map: every cell's days").json("ThermalMapResponse"),
        get(
            THERMAL_HOURS_PATH,
            "One day of the thermal map hour by hour, a layer at a time",
        )
        .docs("GET /v1/thermals/hours?date=&layer=&height=")
        .query(&["date!", "layer", "height", "above_ground"])
        .json("ThermalHoursResponse"),
        get(
            REGIONS_PATH,
            "The regions of the forecast by region: outlines, names and labels, as GeoJSON",
        )
        .json("RegionsResponse"),
        get(
            REGION_FORECAST_PATH,
            "One day of the thermal map read region by region, for the day and each hour",
        )
        .docs("GET /v1/regions/forecast?date=")
        .query(&["date:region-forecast"])
        .json("RegionForecastResponse"),
        get(
            FLOWN_THERMALS_PATH,
            "Where thermals have been: the cells in which several pilots' public flights climbed",
        )
        .docs("GET /v1/thermals/flown?bbox=&month=&hours=")
        .query(&["bbox!", "month:of-year", "hours:by-the-sun"])
        .json("FlownThermalsResponse"),
        get(
            ROUTE_WEATHER_PATH,
            "The thermal map's hours of a day at each point of a route",
        )
        .docs("GET /v1/route/weather?date=&points=")
        .query(&["date!", "points!"])
        .json("RouteWeatherResponse"),
        get(
            RADAR_PATH,
            "The rain radar frames the server holds, in layers by source",
        )
        .json("RadarResponse"),
        get(
            venti_api_types::v1::FOEHN_PATH,
            "Foehn: the pressure difference across the Alps' main ridge for each pair of stations",
        )
        .json("FoehnResponse"),
        get(
            "/v1/radar/{source}/{time}.png",
            "One radar frame as a picture",
        )
        .bytes("image/png", "The frame"),
        get(MAPS_PATH, "The map regions the server hands out").json("MapsResponse"),
        get(
            "/v1/maps/{region}/{version}/{name}",
            "A file of a map region, or the range asked for",
        )
        .docs("GET and HEAD /v1/maps/{region}/{version}/{name}")
        .statuses(&[200, 206])
        .bytes("application/octet-stream", "The file, never compressed"),
        get(
            "/v1/imagery/{source}/{z}/{x}/{y}",
            "A tile of map imagery, passed on from its provider",
        )
        .statuses(&[200, 204])
        .bytes("image/*", "The tile; 204 where the source has none to show"),
        // Accounts and sessions.
        post(ACCOUNTS_PATH, "Create an account and sign in")
            .takes("CreateAccountRequest")
            .statuses(&[201])
            .json("SignInResponse"),
        post(SESSIONS_PATH, "Sign in with handle and password")
            .takes("SignInRequest")
            .statuses(&[201])
            .json("SignInResponse"),
        post(AUTH_GOOGLE_PATH, "Sign in with a Google ID token")
            .takes("GoogleSignInRequest")
            .statuses(&[200, 201])
            .json("GoogleSignInAnswer"),
        post(
            AUTH_GOOGLE_ACCOUNT_PATH,
            "Create an account after a first sign-in with Google, and sign in",
        )
        .takes("CreateGoogleAccountRequest")
        .statuses(&[201])
        .json("SignInResponse"),
        post(ACCOUNT_GOOGLE_PATH, "Link the account to a Google account")
            .docs("POST and DELETE /v1/account/google")
            .session()
            .takes("LinkGoogleRequest")
            .json("AccountResponse"),
        delete(
            ACCOUNT_GOOGLE_PATH,
            "Unlink the account from its Google account",
        )
        .docs("POST and DELETE /v1/account/google")
        .session()
        .nothing(),
        get(
            DEV_GOOGLE_ID_TOKEN_PATH,
            "A made-up Google ID token, in the development mode only",
        )
        .docs("GET /v1/dev/google-id-token?email=&name=")
        .query(&["email!", "name"])
        .json("DevGoogleIdTokenResponse"),
        get(SESSIONS_PATH, "The account's sessions")
            .session()
            .json("SessionsResponse"),
        delete(SESSIONS_PATH, "Sign out everywhere")
            .docs("DELETE /v1/sessions/{id} and DELETE /v1/sessions")
            .session()
            .nothing(),
        delete("/v1/sessions/{id}", "End one session")
            .docs("DELETE /v1/sessions/{id} and DELETE /v1/sessions")
            .session()
            .nothing(),
        get(ACCOUNT_PATH, "The account")
            .with(AccountRead)
            .json("AccountResponse"),
        patch(
            ACCOUNT_PATH,
            "Change the pilot's settings: whether new flights are public, the pilot's country, \
             the glider's class",
        )
        .with(FlightsWrite)
        .takes("UpdateAccountRequest")
        .json("AccountResponse"),
        delete(ACCOUNT_PATH, "Delete the account and what goes with it")
            .session()
            .takes("DeleteAccountRequest")
            .nothing(),
        get(
            ACCOUNT_ARCHIVE_PATH,
            "Everything the account gave, as one ZIP file",
        )
        .session()
        .bytes(
            ARCHIVE_MEDIA_TYPE,
            "A ZIP file. Its index.json (ArchiveIndex) names every file with the record it holds",
        ),
        put(ACCOUNT_PASSWORD_PATH, "Change the password")
            .session()
            .takes("ChangePasswordRequest")
            .nothing(),
        post(
            ACCOUNT_TOKENS_PATH,
            "Make a token for another program; its secret is shown once",
        )
        .session()
        .takes("CreateAccessTokenRequest")
        .statuses(&[201])
        .json("CreateAccessTokenResponse"),
        get(
            ACCOUNT_TOKENS_PATH,
            "The account's tokens for other programs",
        )
        .session()
        .json("AccessTokensResponse"),
        delete("/v1/account/tokens/{id}", "Withdraw a token")
            .session()
            .nothing(),
        get(
            "/v1/oauth/requests/{id}",
            "A request for the pilot's agreement that a program may read for them",
        )
        .json("AuthorizationRequestResponse"),
        post(
            "/v1/oauth/requests/{id}",
            "Answer such a request: agree or refuse; the answer is where the browser goes",
        )
        .session()
        .takes("AnswerAuthorizationRequest")
        .json("AuthorizationAnswerResponse"),
        get(
            KEPT_SITE_CHANGES_PATH,
            "What changed among the sites the pilot keeps as their own",
        )
        .docs("GET /v1/account/kept-sites/changes?since=&limit=")
        .with(KeptSitesRead)
        .query(SINCE)
        .json("KeptSiteChangesResponse"),
        put(
            "/v1/account/kept-sites/{id}",
            "Keep a site as the pilot's own",
        )
        .with(KeptSitesWrite)
        .takes("KeepSiteRequest")
        .json("KeptSiteResponse"),
        delete("/v1/account/kept-sites/{id}", "Forget a kept site")
            .with(KeptSitesWrite)
            .nothing(),
        get(ALERTS_PATH, "The pilot's alarms")
            .docs("GET /v1/account/alerts")
            .with(AlertsRead)
            .json("AlertsResponse"),
        post(ALERTS_PATH, "Set an alarm")
            .docs("POST /v1/account/alerts")
            .with(AlertsWrite)
            .takes("AlertRequest")
            .statuses(&[201])
            .json("AlertResponse"),
        put("/v1/account/alerts/{id}", "Change an alarm")
            .docs("PUT /v1/account/alerts/{id}")
            .with(AlertsWrite)
            .takes("AlertRequest")
            .json("AlertResponse"),
        delete("/v1/account/alerts/{id}", "Remove an alarm")
            .docs("DELETE /v1/account/alerts/{id}")
            .with(AlertsWrite)
            .nothing(),
        get(ALERTS_FIRED_PATH, "What the pilot's alarms fired")
            .docs("GET /v1/account/alerts/fired?since=&limit=")
            .with(AlertsRead)
            .query(SINCE)
            .json("FiredAlertsResponse"),
        post(PUSH_SUBSCRIPTION_PATH, "Subscribe a browser to Web Push")
            .with(AlertsWrite)
            .takes("PushSubscriptionRequest")
            .nothing(),
        delete(PUSH_SUBSCRIPTION_PATH, "Remove a browser's subscription")
            .with(AlertsWrite)
            .takes("PushRemoveRequest")
            .nothing(),
        get(PUSH_KEY_PATH, "The server's public key for Web Push").json("PushKeyResponse"),
        // Phones, what pilots nearby do, and help after landing.
        post(
            venti_api_types::v1::DEVICES_PATH,
            "Register this phone for push through APNs or FCM",
        )
        .docs("POST and DELETE /v1/account/devices")
        .session()
        .takes("DeviceRequest")
        .nothing(),
        delete(venti_api_types::v1::DEVICES_PATH, "Remove a phone")
            .docs("POST and DELETE /v1/account/devices")
            .session()
            .takes("DeviceRemoveRequest")
            .nothing(),
        get(
            venti_api_types::v1::NEARBY_PATH,
            "The settings for open plans nearby and being asked for help",
        )
        .docs("GET and PUT /v1/account/nearby")
        .session()
        .json("NearbySettings"),
        put(
            venti_api_types::v1::NEARBY_PATH,
            "Set the settings for open plans nearby and being asked for help",
        )
        .docs("GET and PUT /v1/account/nearby")
        .session()
        .takes("NearbySettings")
        .json("NearbySettings"),
        put(
            venti_api_types::v1::POSITION_PATH,
            "Tell the phone's coarse position, for being asked for help",
        )
        .docs("PUT and DELETE /v1/account/position")
        .session()
        .takes("PositionRequest")
        .nothing(),
        delete(
            venti_api_types::v1::POSITION_PATH,
            "Forget the phone's position",
        )
        .docs("PUT and DELETE /v1/account/position")
        .session()
        .nothing(),
        post(
            venti_api_types::v1::HELP_PATH,
            "Ask the pilots nearby for a lift or a hand after landing",
        )
        .session()
        .takes("HelpRequestBody")
        .statuses(&[201])
        .json("HelpResponse"),
        get(
            venti_api_types::v1::HELP_PATH,
            "The pilot's own requests for help and those they were asked",
        )
        .session()
        .json("HelpListResponse"),
        get("/v1/help/{id}", "One request for help")
            .docs("GET and DELETE /v1/help/{id}")
            .session()
            .json("HelpResponse"),
        delete("/v1/help/{id}", "Close the pilot's own request for help")
            .docs("GET and DELETE /v1/help/{id}")
            .session()
            .nothing(),
        put("/v1/help/{id}/answer", "Say whether the pilot can help")
            .session()
            .takes("HelpAnswerBody")
            .json("HelpResponse"),
        // Flights.
        post(
            FLIGHTS_PATH,
            "Upload a flight file; 200 where the account has the flight already",
        )
        .with(FlightsWrite)
        .takes_a_file()
        .statuses(&[201, 200])
        .json("UploadResponse"),
        get(FLIGHTS_PATH, "The pilot's own flights, newest first")
            .docs("GET /v1/flights?after=&limit=")
            .with(FlightsRead)
            .query(PAGE)
            .json("FlightsResponse"),
        patch(
            FLIGHTS_PATH,
            "Make every flight of the pilot public, or every one private",
        )
        .with(FlightsWrite)
        .takes("UpdateFlightsRequest")
        .json("UpdateFlightsResponse"),
        get(
            FLIGHT_CHANGES_PATH,
            "What changed among the pilot's own flights, deletions included",
        )
        .docs("GET /v1/flights/changes?since=&limit=")
        .with(FlightsRead)
        .query(SINCE)
        .json("FlightChangesResponse"),
        get("/v1/flights/{id}", "One flight")
            .shared_or(FlightsRead)
            .json("FlightResponse"),
        patch(
            "/v1/flights/{id}",
            "Change who may see a flight, or the class of its glider",
        )
        .with(FlightsWrite)
        .takes("UpdateFlightRequest")
        .json("FlightResponse"),
        delete("/v1/flights/{id}", "Delete a flight")
            .with(FlightsWrite)
            .nothing(),
        get(
            "/v1/flights/{id}/igc",
            "A flight's file as its pilot uploaded it",
        )
        .shared_or(FlightsRead)
        .bytes("application/octet-stream", "The IGC file"),
        get("/v1/flights/{id}/track", "A flight's track as GeoJSON")
            .shared_or(FlightsRead)
            .json_as("application/geo+json", "FlightTrack"),
        get(
            "/v1/pilots/{handle}",
            "A pilot's page: their public flights, newest first",
        )
        .docs("GET /v1/pilots/{handle}?after=&limit=")
        .query(PAGE)
        .json("PilotResponse"),
        get(
            PILOTS_PATH,
            "The pilots with public flights whose handle or name holds a text",
        )
        .docs("GET /v1/pilots?q=&limit=")
        .query(&["q!", "limit"])
        .json("PilotsResponse"),
        get(
            "/v1/pilots/{handle}/statistics",
            "What a pilot's public flights add up to, all together and year by year",
        )
        .json("PilotStatisticsResponse"),
        get(
            ACCOUNT_STATISTICS_PATH,
            "What all the account's flights add up to, the private ones too",
        )
        .with(FlightsRead)
        .json("PilotStatisticsResponse"),
        get(
            venti_api_types::v1::PILOTS_NEAR_PATH,
            "The pilots whose public flights took off near a place",
        )
        .docs("GET /v1/pilots/near?lat=&lon=&radius_km=&limit=")
        .query(&["lat!", "lon!", "radius_km", "limit"])
        .json("PilotsNearResponse"),
        put(
            venti_api_types::v1::ACCOUNT_PICTURE_PATH,
            "Send the pilot's own picture",
        )
        .docs("PUT and DELETE /v1/account/picture")
        .session()
        .takes_a_picture()
        .json("AccountResponse"),
        delete(
            venti_api_types::v1::ACCOUNT_PICTURE_PATH,
            "Remove the pilot's picture",
        )
        .docs("PUT and DELETE /v1/account/picture")
        .session()
        .json("AccountResponse"),
        get("/v1/pictures/{id}", "A pilot's picture")
            .bytes("image/webp", "The picture, 256 by 256 pixels"),
        get(
            ACCOUNT_HABITS_PATH,
            "How each of the account's flights was flown, in figures, the private ones too",
        )
        .with(FlightsRead)
        .json("HabitsResponse"),
        // Pilots among each other.
        put("/v1/pilots/{handle}/follow", "Follow a pilot")
            .with(SocialWrite)
            .json("FollowResponse"),
        delete("/v1/pilots/{handle}/follow", "Follow a pilot no more")
            .with(SocialWrite)
            .statuses(&[200])
            .json("FollowResponse"),
        get(
            "/v1/pilots/{handle}/followers",
            "Who follows a pilot, the newest first, and how many do",
        )
        .docs("GET /v1/pilots/{handle}/followers?after=&limit=")
        .query(PAGE)
        .json("FollowsResponse"),
        get(
            "/v1/pilots/{handle}/following",
            "Whom a pilot follows, the newest first, and how many",
        )
        .docs("GET /v1/pilots/{handle}/following?after=&limit=")
        .query(PAGE)
        .json("FollowsResponse"),
        get(ACCOUNT_FOLLOWING_PATH, "Everyone the account follows")
            .docs("GET /v1/account/following?after=&limit=")
            .with(SocialRead)
            .query(PAGE)
            .json("FollowsResponse"),
        get(
            ACCOUNT_FEED_PATH,
            "The public flights of the pilots the account follows, newest first",
        )
        .docs("GET /v1/account/feed?before=&limit=")
        .with(SocialRead)
        .query(&["before:cursor", "limit"])
        .json("FeedResponse"),
        put("/v1/flights/{id}/cheer", "Cheer a public flight")
            .with(SocialWrite)
            .json("CheerResponse"),
        delete("/v1/flights/{id}/cheer", "Take a cheer back")
            .with(SocialWrite)
            .statuses(&[200])
            .json("CheerResponse"),
        get(
            "/v1/flights/{id}/cheers",
            "Who cheered a public flight, the newest first, and how many did",
        )
        .docs("GET /v1/flights/{id}/cheers?after=&limit=")
        .query(PAGE)
        .json("CheersResponse"),
        get(
            "/v1/flights/{id}/comments",
            "The comments under a public flight, the oldest first",
        )
        .docs("GET /v1/flights/{id}/comments?after=&limit=")
        .query(PAGE)
        .json("CommentsResponse"),
        post(
            "/v1/flights/{id}/comments",
            "Write a comment under a public flight",
        )
        .with(SocialWrite)
        .takes("CommentRequest")
        .statuses(&[201])
        .json("CommentResponse"),
        patch(
            "/v1/flights/{id}/comments",
            "Say whether a flight of the pilot's own takes comments",
        )
        .with(FlightsWrite)
        .takes("CommentsOpen")
        .json("CommentsOpen"),
        delete(
            "/v1/flights/{id}/comments/{comment}",
            "Remove a comment: its author, or the flight's pilot",
        )
        .with(SocialWrite)
        .nothing(),
        get(ACCOUNT_NOTIFICATIONS_PATH, "What the account was told")
            .docs("GET /v1/account/notifications?since=&limit=")
            .with(SocialRead)
            .query(SINCE)
            .json("NotificationsResponse"),
        get(
            ACCOUNT_UNREAD_PATH,
            "How much is new for the pilot, on every device",
        )
        .docs("GET and PUT /v1/account/unread")
        .with(SocialRead)
        .json("UnreadResponse"),
        put(
            ACCOUNT_UNREAD_PATH,
            "The pilot saw the notifications and the alarms",
        )
        .docs("GET and PUT /v1/account/unread")
        .with(SocialWrite)
        .takes_if_any("SeenRequest")
        .json("UnreadResponse"),
        get(PUBLIC_FLIGHTS_PATH, "Everyone's public flights")
            .docs("GET /v1/flights/public?pilot=&bbox=&from=&to=&sort=&after=&limit=")
            .query(&[
                "pilot",
                "bbox",
                "from:time",
                "to:time",
                "site",
                "near",
                "radius_km:search",
                "area",
                "class",
                "kind",
                "min_km",
                "country",
                "sort",
                "after:cursor",
                "limit",
            ])
            .json("FlightsResponse"),
        get(
            PUBLIC_FLIGHT_CHANGES_PATH,
            "What changed among the public flights: new, changed and withdrawn",
        )
        .docs("GET /v1/flights/public/changes?since=&pilot=&limit=")
        .query(&["since:cursor", "pilot", "limit"])
        .json("PublicFlightChangesResponse"),
        get(
            RANKING_PATH,
            "Pilots by the sum of their best public flights",
        )
        .docs("GET /v1/ranking?bbox=&from=&to=&month=&limit=")
        .query(&[
            "bbox",
            "from:time",
            "to:time",
            "month",
            "near",
            "radius_km:search",
            "area",
            "class",
            "kind",
            "min_km",
            "country",
            "limit",
        ])
        .json("RankingResponse"),
        // Live tracks.
        post(
            LIVE_POSITIONS_PATH,
            "Send fixes of a live track; 201 where they begin one",
        )
        .with(LiveWrite)
        .takes("LivePositionsRequest")
        .statuses(&[200, 201])
        .json("LivePositionsResponse"),
        get(
            LIVE_PATH,
            "Who is in the air: the public live tracks of a box",
        )
        .docs("GET /v1/live?bbox=west,south,east,north")
        .query(&["bbox!"])
        .json("LiveTracksResponse"),
        get(
            LIVE_OTHERS_PATH,
            "Who else is in the air: the aircraft other networks heard in a box",
        )
        .docs("GET /v1/live/others?bbox=west,south,east,north")
        .query(&["bbox!"])
        .json("LiveOthersResponse"),
        get(
            LIVE_CLIMBS_PATH,
            "Where pilots climb now: the thermals gliders circled in during the last minutes",
        )
        .docs("GET /v1/live/climbs?bbox=west,south,east,north")
        .query(&["bbox!"])
        .json("LiveClimbsResponse"),
        get(
            LIVE_SITES_PATH,
            "How many pilots are in the air at the flying sites of a box",
        )
        .docs("GET /v1/live/sites?bbox=west,south,east,north")
        .query(&["bbox!"])
        .json("LiveSitesResponse"),
        get("/v1/live/{id}", "One live track and its fixes")
            .docs("GET /v1/live/{id}?since=&after=&limit=")
            .query(&["since:cursor", "after:time", "limit:fixes"])
            .json("LiveTrackResponse"),
        delete("/v1/live/{id}", "End and delete a live track")
            .with(LiveWrite)
            .nothing(),
        // Groups and plans.
        post(GROUPS_PATH, "Make a group")
            .with(GroupsWrite)
            .takes("CreateGroupRequest")
            .statuses(&[201])
            .json("CreateGroupResponse"),
        get(GROUPS_PATH, "The groups the pilot is in")
            .with(GroupsRead)
            .json("GroupsResponse"),
        post(
            GROUP_JOIN_PATH,
            "Come into a group with its invite code; 200 for a member",
        )
        .with(GroupsWrite)
        .takes("JoinGroupRequest")
        .statuses(&[201, 200])
        .json("JoinGroupResponse"),
        get(PLAN_BOARD_PATH, "The plans of all of the pilot's groups")
            .docs("GET /v1/groups/plans?from=&to=")
            .with(GroupsRead)
            .query(DAYS)
            .json("PlanBoardResponse"),
        get("/v1/groups/{id}", "A group and its members")
            .with(GroupsRead)
            .json("GroupResponse"),
        patch(
            "/v1/groups/{id}",
            "Change a group's name, description, home or kind",
        )
        .with(GroupsWrite)
        .takes("UpdateGroupRequest")
        .json("GroupResponse"),
        delete("/v1/groups/{id}", "Delete a group")
            .with(GroupsWrite)
            .nothing(),
        post("/v1/groups/{id}/invite", "Replace a group's invite code")
            .with(GroupsWrite)
            .json("GroupInviteResponse"),
        put(
            "/v1/groups/{id}/keeper",
            "Hand the keeping of a group to another member",
        )
        .with(GroupsWrite)
        .takes("HandOverGroupRequest")
        .json("GroupResponse"),
        delete(
            "/v1/groups/{id}/members/{handle}",
            "Leave a group, or remove a member",
        )
        .with(GroupsWrite)
        .nothing(),
        post("/v1/groups/{id}/plans", "Post a plan for a flying day")
            .with(GroupsWrite)
            .takes("PlanRequest")
            .statuses(&[201])
            .json("PlanResponse"),
        get("/v1/groups/{id}/plans", "A group's plans")
            .docs("GET /v1/groups/{id}/plans?from=&to=")
            .with(GroupsRead)
            .query(DAYS)
            .json("PlansResponse"),
        put("/v1/groups/{id}/plans/{plan}", "Change a plan")
            .with(GroupsWrite)
            .takes("PlanRequest")
            .json("PlanResponse"),
        delete("/v1/groups/{id}/plans/{plan}", "Delete a plan")
            .with(GroupsWrite)
            .nothing(),
        put("/v1/groups/{id}/plans/{plan}/in", "Say \"I am in\"")
            .docs("PUT and DELETE /v1/groups/{id}/plans/{plan}/in")
            .with(GroupsWrite)
            .json("PlanResponse"),
        delete("/v1/groups/{id}/plans/{plan}/in", "Take \"I am in\" back")
            .docs("PUT and DELETE /v1/groups/{id}/plans/{plan}/in")
            .with(GroupsWrite)
            .json("PlanResponse"),
        // Groups as communities with a chat.
        get(
            venti_api_types::v1::GROUPS_FIND_PATH,
            "Public groups near a place and by name",
        )
        .docs("GET /v1/groups/find?lat=&lon=&q=&limit=")
        .with(GroupsRead)
        .query(&["lat", "lon", "q:group", "limit:groups"])
        .json("GroupsFoundResponse"),
        get(
            venti_api_types::v1::GROUP_INVITES_PATH,
            "The pilot's invites into private groups",
        )
        .with(GroupsRead)
        .json("GroupInvitesResponse"),
        get("/v1/groups/pictures/{picture}", "A picture sent to a group").bytes(
            "image/webp",
            "The picture, its longer side at most 1600 pixels",
        ),
        post(
            "/v1/groups/{id}/members",
            "Come into a public group, or one the pilot was invited into; 200 for a member",
        )
        .with(GroupsWrite)
        .statuses(&[201, 200])
        .json("JoinGroupResponse"),
        post(
            "/v1/groups/{id}/invites",
            "Invite a pilot into a group by handle",
        )
        .with(GroupsWrite)
        .takes("InviteRequest")
        .statuses(&[201])
        .json("GroupInviteAnswer"),
        delete(
            "/v1/groups/{id}/invites/{handle}",
            "Take an invite back, or decline it",
        )
        .with(GroupsWrite)
        .nothing(),
        put("/v1/groups/{id}/admins/{handle}", "Make a member an admin")
            .docs("PUT and DELETE /v1/groups/{id}/admins/{handle}")
            .with(GroupsWrite)
            .nothing(),
        delete(
            "/v1/groups/{id}/admins/{handle}",
            "Make an admin a member again",
        )
        .docs("PUT and DELETE /v1/groups/{id}/admins/{handle}")
        .with(GroupsWrite)
        .nothing(),
        put("/v1/groups/{id}/picture", "Give a group its picture")
            .docs("PUT and DELETE /v1/groups/{id}/picture")
            .with(GroupsWrite)
            .takes_a_picture()
            .json("GroupResponse"),
        delete("/v1/groups/{id}/picture", "Remove a group's picture")
            .docs("PUT and DELETE /v1/groups/{id}/picture")
            .with(GroupsWrite)
            .json("GroupResponse"),
        post("/v1/groups/{id}/pictures", "Send a picture for a message")
            .session()
            .takes_a_picture()
            .statuses(&[201])
            .json("SentPictureResponse"),
        get(
            "/v1/groups/{id}/messages",
            "A page of a group's messages, or what is new since an answer",
        )
        .docs("GET /v1/groups/{id}/messages?before=&since=&limit=")
        .session()
        .query(&["before:message", "since:message", "limit"])
        .json("MessagesResponse"),
        post("/v1/groups/{id}/messages", "Post a message to a group")
            .session()
            .takes("MessageRequest")
            .statuses(&[201])
            .json("MessageResponse"),
        delete("/v1/groups/{id}/messages/{message}", "Remove a message")
            .session()
            .json("MessageResponse"),
        patch("/v1/groups/{id}/me", "Mute a group, or mark it read")
            .session()
            .takes("GroupMeRequest")
            .json("GroupMeResponse"),
        // Open plans.
        post(
            venti_api_types::v1::OPEN_PLANS_PATH,
            "Post a plan open to everyone flying from the area",
        )
        .with(GroupsWrite)
        .takes("OpenPlanRequest")
        .statuses(&[201])
        .json("OpenPlanResponse"),
        get(
            venti_api_types::v1::OPEN_PLANS_PATH,
            "The open plans near a place for the coming days",
        )
        .docs("GET /v1/open-plans?lat=&lon=&radius_km=&from=&to=")
        .query(&["lat!", "lon!", "radius_km", "from:day", "to:day"])
        .json("OpenPlansResponse"),
        get("/v1/open-plans/{plan}", "One open plan")
            .docs("GET, PUT and DELETE /v1/open-plans/{plan}")
            .json("OpenPlanResponse"),
        put("/v1/open-plans/{plan}", "Change an open plan")
            .docs("GET, PUT and DELETE /v1/open-plans/{plan}")
            .with(GroupsWrite)
            .takes("OpenPlanRequest")
            .json("OpenPlanResponse"),
        delete("/v1/open-plans/{plan}", "Delete an open plan")
            .docs("GET, PUT and DELETE /v1/open-plans/{plan}")
            .with(GroupsWrite)
            .nothing(),
        put(
            "/v1/open-plans/{plan}/in",
            "Say \"I am in\" on an open plan",
        )
        .docs("PUT and DELETE /v1/open-plans/{plan}/in")
        .with(GroupsWrite)
        .json("OpenPlanResponse"),
        delete(
            "/v1/open-plans/{plan}/in",
            "Take \"I am in\" back on an open plan",
        )
        .docs("PUT and DELETE /v1/open-plans/{plan}/in")
        .with(GroupsWrite)
        .json("OpenPlanResponse"),
        get(
            venti_api_types::v1::ACCOUNT_OPEN_PLANS_PATH,
            "The open plans the pilot posted or is in on",
        )
        .with(GroupsRead)
        .json("OpenPlansResponse"),
        // Reports.
        post(
            REPORTS_PATH,
            "Report something to whoever runs the server; 200 for one sent before",
        )
        .with(ReportsWrite)
        .takes("ReportRequest")
        .statuses(&[201, 200])
        .json("ReportResponse"),
    ]
}

/// A query parameter by its name in [`operations`]: what it means and its
/// schema. A `!` after the name marks one a request must carry.
fn query_parameter(spec: &str) -> Value {
    let required = spec.ends_with('!');
    let spec = spec.trim_end_matches('!');
    let name = spec.split(':').next().unwrap_or(spec);
    let (description, schema) = match spec {
        "bbox" => (
            "A box as west,south,east,north in WGS84 degrees, for example 10.5,47.2,11.9,47.8.",
            json!({"type": "string"}),
        ),
        "lat" => ("Latitude in WGS84 degrees.", json!({"type": "number"})),
        "lon" => ("Longitude in WGS84 degrees.", json!({"type": "number"})),
        "from:time" => (
            "The start of the span, an RFC 3339 time in UTC.",
            json!({"type": "string", "format": "date-time"}),
        ),
        "to:time" => (
            "The end of the span, an RFC 3339 time in UTC.",
            json!({"type": "string", "format": "date-time"}),
        ),
        "from:day" => (
            "The first day, written YYYY-MM-DD.",
            json!({"type": "string", "format": "date"}),
        ),
        "to:day" => (
            "The last day, written YYYY-MM-DD.",
            json!({"type": "string", "format": "date"}),
        ),
        "date" => (
            "A day the thermal map has, written YYYY-MM-DD.",
            json!({"type": "string", "format": "date"}),
        ),
        "points" => (
            "The points of a route in the order they are flown, each lat,lon in WGS84 degrees, joined by ;, for example 47.6642,11.7967;47.7,11.9. At most 25.",
            json!({"type": "string"}),
        ),
        "layer" => (
            "A layer of the thermal map's hours; the error for an unknown one lists those there are.",
            json!({"type": "string"}),
        ),
        "height" => (
            "A height of the wind layer; the error for an unknown one lists those there are.",
            json!({"type": "string"}),
        ),
        "above_ground" => (
            "A height above the model's ground of the wind layer, in place of height; the error for an unknown one lists those there are.",
            json!({"type": "string"}),
        ),
        "date:region-forecast" => (
            "A day the thermal map has, written YYYY-MM-DD. Without it: today, or the map's first day where it has no today.",
            json!({"type": "string", "format": "date"}),
        ),
        "layers" => (
            "The layers wanted, by name and separated by commas, for example osm-free-flying,venti,venti-pilots; `default` stands for those a request that names none gets. Without it: every source's layer and `venti`, and not the crowdsourced `venti-pilots`.",
            json!({"type": "string"}),
        ),
        "layer:sites" => (
            "Which of Venti's own two layers: `venti`, the partners', which a request that names none gets, or `venti-pilots`, the crowdsourced one. Each has cursors of its own.",
            json!({"type": "string", "enum": ["venti", "venti-pilots"]}),
        ),
        "after:cursor" => (
            "Paging: the `next` of the page before, as the server gave it. Left out for the first page. A page is followed until an answer has no `next`.",
            json!({"type": "string"}),
        ),
        "since:cursor" => (
            "Following changes: the `cursor` of the answer before, as the server gave it. Left out to begin with everything. While an answer says `more`, ask again at once with its `cursor`; `reset` says the cursor was not this server's.",
            json!({"type": "string"}),
        ),
        "after:revision" => (
            "The number of the last revision of the page before.",
            json!({"type": "integer"}),
        ),
        "after:time" => (
            "Only fixes after this RFC 3339 time.",
            json!({"type": "string", "format": "date-time"}),
        ),
        "limit" => (
            "The most entries one answer carries, 1 to 100.",
            json!({"type": "integer", "minimum": 1, "maximum": 100}),
        ),
        "limit:fixes" => (
            "The most fixes one answer carries, 1 to 5000.",
            json!({"type": "integer", "minimum": 1, "maximum": 5000}),
        ),
        "pilot" => ("A pilot's handle.", json!({"type": "string"})),
        "q:group" => (
            "Text a group's name or the place of its home contains, in any case, at most 60 characters.",
            json!({"type": "string", "maxLength": 60}),
        ),
        "limit:groups" => (
            "The most groups the answer carries, 1 to 50, 20 if left out.",
            json!({"type": "integer", "minimum": 1, "maximum": 50}),
        ),
        "before:message" => (
            "The `older` of the answer before: the page of messages before it. Left out for the newest.",
            json!({"type": "string"}),
        ),
        "since:message" => (
            "The `since` of an earlier answer: what was posted or changed after it. While an answer says `more`, ask again at once with its `since`.",
            json!({"type": "string"}),
        ),
        "radius_km" => (
            "How far around the place, in whole kilometres: 1 to 100, 25 if left out.",
            json!({"type": "integer"}),
        ),
        "near" => (
            "The centre of a circle the take-off lies in, lat,lon in WGS84 degrees, for example 47.2654,11.3933. With radius_km; not with site.",
            json!({"type": "string", "maxLength": 40, "pattern": "^-?[0-9.]+,-?[0-9.]+$"}),
        ),
        "radius_km:search" => (
            "The radius of the circle of near, in kilometres, 1 to 100, edge included.",
            json!({"type": "number", "minimum": 1, "maximum": 100}),
        ),
        "area" => (
            "An area drawn on the map the take-off lies in, edges included: its corners in the order drawn, lat,lon,lat,lon,… in WGS84 degrees, three to 50 corners, edges that do not cross, within 180 degrees of longitude.",
            json!({"type": "string", "maxLength": 1100}),
        ),
        "class" => (
            "Only flights flown on a glider of this class.",
            json!({"type": "string", "enum": ["en_a", "en_b", "en_c", "en_d", "ccc"]}),
        ),
        "kind" => (
            "Only flights scored as this kind of route.",
            json!({"type": "string", "enum": ["free_flight", "free_triangle", "fai_triangle"]}),
        ),
        "min_km" => (
            "Only flights whose scoring distance is at least this many kilometres, 0 to 1000. A flight too short to score has none.",
            json!({"type": "number", "minimum": 0, "maximum": 1000}),
        ),
        "country" => (
            "Only flights that took off in this country, an ISO 3166-1 alpha-2 code, in either case.",
            json!({"type": "string", "minLength": 2, "maxLength": 2, "pattern": "^[A-Za-z]{2}$"}),
        ),
        "q" => (
            "A part of a pilot's handle or display name, 2 to 60 characters, in any case.",
            json!({"type": "string"}),
        ),
        "month:of-year" => (
            "A month of any year, a number from 1 to 12: only the climbs of that month.",
            json!({"type": "integer", "minimum": 1, "maximum": 12}),
        ),
        "hours:by-the-sun" => (
            "Hours of the day by the sun at the place (UTC and four minutes for each degree east), from the first to before the second, for example 11-15: only the climbs that began between them.",
            json!({"type": "string"}),
        ),
        "month" => (
            "A calendar month by UTC, written YYYY-MM: a `from` and a `to` in one word, and not sent with either.",
            json!({"type": "string"}),
        ),
        "site" => (
            "A site's identifier, of any layer: the flights that took off within 500 m of it.",
            json!({"type": "string"}),
        ),
        "sort" => (
            "The order of the list.",
            json!({"type": "string", "enum": ["newest", "score"]}),
        ),
        _ => ("", json!({})),
    };
    json!({
        "name": name,
        "in": "query",
        "required": required,
        "description": description,
        "schema": schema,
    })
}

/// The parameters a path names between braces.
fn path_parameters(path: &str) -> Vec<Value> {
    path.split('{')
        .skip(1)
        .filter_map(|rest| rest.split('}').next())
        .map(|name| {
            let description = match name {
                "id" => "The identifier, as an answer of the server gave it.",
                "handle" => "A pilot's handle.",
                "layer" => "One of Venti's own two layers of sites: `venti` or `venti-pilots`.",
                "plan" => "The plan's identifier.",
                "source" => "The source's identifier, as the list names it.",
                "time" => "The frame's time, as the list of frames writes it.",
                "region" | "version" | "name" => "As the list of map regions names it.",
                "z" | "x" | "y" => "The tile's zoom, column and row.",
                _ => "",
            };
            json!({
                "name": name,
                "in": "path",
                "required": true,
                "description": description,
                "schema": {"type": "string"},
            })
        })
        .collect()
}

fn reference(schema: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{schema}")})
}

/// What the description says of who may ask: the standard `security`,
/// and the same in words.
fn security(who: Who) -> (Value, String) {
    let scoped = |scope: Scope| json!({"token": [scope.as_str()]});
    match who {
        Who::Anyone => (json!([]), "Anyone; no token.".to_owned()),
        Who::With(scope) => (
            json!([{"session": []}, scoped(scope)]),
            format!(
                "The pilot: a session's token, or a token for a program with the scope `{scope}`."
            ),
        ),
        Who::Session => (
            json!([{"session": []}]),
            "The pilot's own sign-in only: a session's token. No token for a program.".to_owned(),
        ),
        Who::SharedOr(scope) => (
            json!([{}, {"session": []}, scoped(scope)]),
            format!(
                "Anyone for a flight that is public or shared by link, with no token. For the pilot's own private flight: a session's token, or a token for a program with the scope `{scope}`."
            ),
        ),
    }
}

fn operation(op: &Op) -> Value {
    let (security, who) = security(op.who);
    let mut parameters = path_parameters(op.path);
    parameters.extend(op.query.iter().map(|spec| query_parameter(spec)));

    let mut responses = Map::new();
    for (place, status) in op.statuses.iter().enumerate() {
        let mut response = Map::new();
        let description = match (status, place) {
            (204, _) => "Done; no body.",
            (206, _) => "The range asked for.",
            (_, 0) => "The answer.",
            _ => "The answer, where the summary says so.",
        };
        response.insert("description".to_owned(), json!(description));
        if *status != 204 {
            let content = match op.gives {
                Gives::Nothing => None,
                Gives::Json(schema) => Some(("application/json", reference(schema))),
                Gives::JsonAs(media_type, schema) => Some((media_type, reference(schema))),
                Gives::Bytes(media_type, what) => Some((
                    media_type,
                    json!({"type": "string", "format": "binary", "description": what}),
                )),
            };
            if let Some((media_type, schema)) = content {
                let mut described = json!({"schema": schema});
                if let Some(found) = examples_of(examples(), op, &status.to_string()) {
                    described["examples"] = found;
                }
                response.insert("content".to_owned(), json!({media_type: described}));
            }
        }
        responses.insert(status.to_string(), Value::Object(response));
    }
    if !matches!(op.who, Who::Anyone) {
        responses.insert(
            "401".to_owned(),
            json!({"$ref": "#/components/responses/Unauthorized"}),
        );
        responses.insert(
            "403".to_owned(),
            json!({"$ref": "#/components/responses/Forbidden"}),
        );
    }
    responses.insert(
        "default".to_owned(),
        json!({"$ref": "#/components/responses/Error"}),
    );

    let mut described = Map::new();
    if let Some(tag) = tag_of(op.path) {
        described.insert("tags".to_owned(), json!([tag]));
    }
    described.insert("summary".to_owned(), json!(op.summary));
    described.insert("description".to_owned(), json!(who));
    described.insert("operationId".to_owned(), json!(operation_id(op)));
    described.insert("security".to_owned(), security);
    if !parameters.is_empty() {
        described.insert("parameters".to_owned(), json!(parameters));
    }
    let body = match op.takes {
        Takes::Nothing => None,
        Takes::Json(schema) => Some(json!({
            "required": true,
            "content": {"application/json": {"schema": reference(schema)}},
        })),
        Takes::JsonIfAny(schema) => Some(json!({
            "required": false,
            "content": {"application/json": {"schema": reference(schema)}},
        })),
        Takes::File => Some(json!({
            "required": true,
            "description": "The flight file as it is, of any media type: an IGC file.",
            "content": {"application/octet-stream": {"schema": {"type": "string", "format": "binary"}}},
        })),
        Takes::Picture => {
            let binary = json!({"schema": {"type": "string", "format": "binary"}});
            let content: Map<String, Value> = venti_api_types::v1::PICTURE_MEDIA_TYPES
                .iter()
                .map(|media| ((*media).to_owned(), binary.clone()))
                .collect();
            Some(json!({
                "required": true,
                "description": "The picture as it is: a JPEG, a PNG or a WebP.",
                "content": content,
            }))
        }
    };
    if let Some(mut body) = body {
        if let (Some(found), Some(json)) = (
            examples_of(examples(), op, "request"),
            body.pointer_mut("/content/application~1json"),
        ) {
            json["examples"] = found;
        }
        described.insert("requestBody".to_owned(), body);
    }
    described.insert("responses".to_owned(), Value::Object(responses));
    let docs = if op.docs.is_empty() {
        format!("{} {}", op.method.to_uppercase(), op.path)
    } else {
        op.docs.to_owned()
    };
    described.insert("x-venti-docs".to_owned(), json!(docs));
    Value::Object(described)
}

/// A name for generated clients: the method and the path's words.
fn operation_id(op: &Op) -> String {
    let mut id = op.method.to_owned();
    for part in op.path.split(['/', '.', '-']).skip(2) {
        let part = part.trim_matches(['{', '}']);
        if part.is_empty() {
            continue;
        }
        id.push('_');
        id.push_str(part);
    }
    id
}

fn build() -> Value {
    let schemas: Value = serde_json::from_str(SCHEMAS).unwrap_or_else(|error| {
        // The file is part of the program; a test reads it too.
        tracing::error!(%error, "the schemas of the API's description cannot be read");
        json!({})
    });

    let mut paths: Map<String, Value> = Map::new();
    for op in operations() {
        let entry = paths.entry(op.path.to_owned()).or_insert_with(|| json!({}));
        if let Some(entry) = entry.as_object_mut() {
            entry.insert(op.method.to_owned(), operation(&op));
        }
    }

    let scopes: Vec<&str> = Scope::ALL.into_iter().map(Scope::as_str).collect();
    let error = |description: &str| {
        json!({
            "description": description,
            "content": {"application/json": {"schema": reference("ErrorResponse")}},
        })
    };
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Venti",
            "version": "v1",
            "summary": "The HTTP API of the Venti server, version 1.",
            "description": concat!(
                "Made by the server from its own table of routes and held to it by tests. ",
                "docs/api.md in the repository says in words what every field means, when ",
                "each error comes and what the bounds are; `x-venti-docs` on an operation ",
                "names its heading there, and \"Writing a program against Venti\" is where ",
                "to begin.\n\n",
                "Within `/v1` the format only grows: an answer may gain fields and a list ",
                "of known values may gain entries, so a client ignores what it does not ",
                "know. No schema here closes a record against further fields. Where a ",
                "schema has `x-venti-values`, those are the values there are today, and ",
                "others may come.\n\n",
                "A value that is not known is left out of an answer: a field is never null. ",
                "Null stands only inside a list with an entry per cell of the thermal map's ",
                "grid, for a cell without a value; its schema says so.\n\n",
                "The data of others is handed out under its sources' licences: every ",
                "answer that carries such data names them in `attributions`, and answers ",
                "with flights of pilots carry `terms`.",
            ),
        },
        "servers": [{"url": "/", "description": "The server the description was read from"}],
        "tags": TAGS
            .iter()
            .map(|(name, description, _)| json!({"name": name, "description": description}))
            .collect::<Vec<_>>(),
        "paths": paths,
        "components": {
            "schemas": schemas,
            "securitySchemes": {
                "session": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": concat!(
                        "The token of a sign-in (`POST /v1/sessions`), sent as ",
                        "`Authorization: Bearer …`. It may do everything the account may. ",
                        "The server sets no cookies and reads none.",
                    ),
                },
                "token": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": format!(
                        "A token a pilot made for another program (`POST /v1/account/tokens`), \
                         sent as `Authorization: Bearer {ACCESS_TOKEN_PREFIX}…`. It may do what \
                         its scopes say; an operation names the scope it needs."
                    ),
                    "x-venti-scopes": scopes,
                },
            },
            "responses": {
                "Error": error(
                    "Any 4xx or 5xx: every error has this body. docs/api.md lists the codes \
                     with their statuses.",
                ),
                "Unauthorized": error(
                    "`unauthorized`: the request needs an account and carries no valid token.",
                ),
                "Forbidden": error(
                    "`insufficient_scope`, naming the scope in `scope`; `session_required`; or \
                     another refusal the operation has.",
                ),
            },
        },
    })
}
