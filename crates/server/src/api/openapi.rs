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
    ACCESS_TOKEN_PREFIX, ACCOUNT_PASSWORD_PATH, ACCOUNT_PATH, ACCOUNT_TOKENS_PATH, ACCOUNTS_PATH,
    AIRSPACE_PATH, FLIGHT_CHANGES_PATH, FLIGHTS_PATH, FORECAST_PATH, GROUP_JOIN_PATH, GROUPS_PATH,
    HEALTH_PATH, KEPT_SITE_CHANGES_PATH, LIVE_PATH, LIVE_POSITIONS_PATH, MAPS_PATH,
    PLAN_BOARD_PATH, PUBLIC_FLIGHT_CHANGES_PATH, PUBLIC_FLIGHTS_PATH, RADAR_PATH, RANKING_PATH,
    REPORTS_PATH, SESSIONS_PATH, SITE_CHANGES_PATH, SITES_PATH, STATIONS_PATH, Scope,
    THERMAL_HOURS_PATH, THERMALS_PATH,
};

use super::cache;

/// Where the description is served.
pub const OPENAPI_PATH: &str = "/v1/openapi.json";

/// The schemas of the records that go over the wire, by the name of
/// their type in `venti-api-types`.
const SCHEMAS: &str = include_str!("openapi_schemas.json");

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
        AccountRead, FlightsRead, FlightsWrite, GroupsRead, GroupsWrite, KeptSitesRead,
        KeptSitesWrite, LiveWrite, ReportsWrite, SitesWrite,
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
        .query(&["date!", "layer", "height"])
        .json("ThermalHoursResponse"),
        get(
            RADAR_PATH,
            "The rain radar frames the server holds, in layers by source",
        )
        .json("RadarResponse"),
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
        delete(ACCOUNT_PATH, "Delete the account and what goes with it")
            .session()
            .takes("DeleteAccountRequest")
            .nothing(),
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
        patch("/v1/flights/{id}", "Change who may see a flight")
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
        get(PUBLIC_FLIGHTS_PATH, "Everyone's public flights")
            .docs("GET /v1/flights/public?pilot=&bbox=&from=&to=&sort=&after=&limit=")
            .query(&[
                "pilot",
                "bbox",
                "from:time",
                "to:time",
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
        .docs("GET /v1/ranking?bbox=&from=&to=&limit=")
        .query(&["bbox", "from:time", "to:time", "limit"])
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
        patch("/v1/groups/{id}", "Rename a group")
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
        "layer" => (
            "A layer of the thermal map's hours; the error for an unknown one lists those there are.",
            json!({"type": "string"}),
        ),
        "height" => (
            "A height of the wind layer; the error for an unknown one lists those there are.",
            json!({"type": "string"}),
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
                response.insert(
                    "content".to_owned(),
                    json!({media_type: {"schema": schema}}),
                );
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
    };
    if let Some(body) = body {
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
