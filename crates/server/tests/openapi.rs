//! What holds the description of the API (`GET /v1/openapi.json`,
//! `docs/openapi.json`) to the server.
//!
//! - The document the server hands out is the file in the repository.
//! - Every route the router has is described, and every described
//!   operation is routed; who may ask is as the description says.
//! - Every golden answer in `tests/golden` is valid against the schema the
//!   description gives for its operation, with no field the schema does
//!   not name.
//!
//! The validator below reads the part of JSON Schema the description
//! uses and refuses a schema that uses more. What it does not look at is
//! said on [`KEYWORDS_NOT_CHECKED`].

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use serde_json::Value;
use venti_api_types::v1::{ErrorCode, ErrorResponse, Scope};
use venti_server::api::{Cors, OPENAPI_PATH};
use venti_server::dev;
use venti_sources::testing::assert_golden;

use common::{WEB_APP, ask, get, golden, settings};

/// The methods the description speaks of. `HEAD` is answered wherever
/// `GET` is and is described nowhere; `OPTIONS` belongs to browsers.
const METHODS: [Method; 5] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
];

/// The routes that are not part of `/v1` and not in its description: the
/// pages for people, the connector, which `docs/mcp.md` describes, and
/// the authorization server, whose endpoints answer in OAuth's own shapes
/// and describe themselves in the two documents under `/.well-known`
/// (`docs/api.md`, "Connecting an assistant to a pilot's logbook").
const NOT_OF_THE_API: [&str; 11] = [
    "/.well-known/oauth-authorization-server",
    "/.well-known/oauth-protected-resource",
    "/.well-known/oauth-protected-resource/mcp",
    "/f/{id}",
    "/f/{id}/preview.png",
    "/live/{id}",
    "/mcp",
    "/oauth/authorize",
    "/oauth/register",
    "/oauth/revoke",
    "/oauth/token",
];

async fn router_and_routes() -> (Router, Vec<&'static str>) {
    let api = dev::recorded_api(settings()).await.unwrap();
    api.router_and_routes(&Cors::Origins(vec![WEB_APP.to_owned()]))
}

async fn served(router: &Router) -> Value {
    get(router, OPENAPI_PATH).await.ok(StatusCode::OK)
}

fn repository_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// One described operation.
struct Described {
    method: Method,
    path: String,
    operation: Value,
}

fn described(document: &Value) -> Vec<Described> {
    let mut all = Vec::new();
    for (path, methods) in document["paths"].as_object().unwrap() {
        for (method, operation) in methods.as_object().unwrap() {
            all.push(Described {
                method: method.to_uppercase().parse().unwrap(),
                path: path.clone(),
                operation: operation.clone(),
            });
        }
    }
    all
}

/// A path with something in the place of every parameter.
fn filled(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            let Some(open) = segment.find('{') else {
                return segment.to_owned();
            };
            let close = segment.find('}').unwrap();
            let name = segment[open + 1..close].trim_start_matches('*');
            let sample = match name {
                "handle" => "demo",
                "source" => "dwd-radar",
                "time" | "frame" => "20261121T1310Z",
                "z" | "x" | "y" => "1",
                "region" | "version" | "name" => "sample",
                _ => "venti:0123456789abcdefghjkmnpqrs",
            };
            format!("{}{sample}{}", &segment[..open], &segment[close + 1..])
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether the router's `route` is the one a described `path` is answered
/// by, and how many of its segments are fixed words: of two routes that
/// fit, the router takes the one with more.
fn fits(route: &str, path: &str) -> Option<usize> {
    let route: Vec<&str> = route.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    let mut fixed = 0;
    for (place, segment) in route.iter().enumerate() {
        if segment.starts_with("{*") {
            return (path.len() > place).then_some(fixed);
        }
        let theirs = path.get(place)?;
        if segment.starts_with('{') {
            continue;
        }
        if segment != theirs {
            return None;
        }
        fixed += 1;
    }
    (route.len() == path.len()).then_some(fixed)
}

struct Probe {
    status: StatusCode,
    /// Whether the answer is the router's own "no such path".
    no_such_path: bool,
}

async fn probe(router: &Router, method: &Method, path: &str, with_body: bool) -> Probe {
    let mut request = ask(method.clone(), path);
    if with_body {
        request = request.json(&serde_json::json!({}));
    }
    let answer = request.send(router).await;
    let no_such_path = answer.status == StatusCode::NOT_FOUND
        && serde_json::from_slice::<ErrorResponse>(&answer.body).is_ok_and(|error| {
            error.error.code == ErrorCode::NotFound
                && error.error.message.contains("is not part of the API")
        });
    Probe {
        status: answer.status,
        no_such_path,
    }
}

#[tokio::test]
async fn the_served_description_is_the_file_in_the_repository() {
    let (router, _) = router_and_routes().await;
    let answer = get(&router, OPENAPI_PATH).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert!(
        answer
            .header(header::CONTENT_TYPE)
            .starts_with("application/json")
    );
    assert!(answer.headers.contains_key(header::ETAG));
    let document: Value = answer.json();
    assert_eq!(document["openapi"], "3.1.0");
    // Rewritten with VENTI_UPDATE_GOLDEN=1, as the golden answers are.
    assert_golden(&repository_file("docs/openapi.json"), &document);
}

#[tokio::test]
async fn every_route_is_described_and_every_described_operation_is_routed() {
    let (router, routes) = router_and_routes().await;
    let document = served(&router).await;
    let described = described(&document);

    let (of_the_api, others): (Vec<&str>, Vec<&str>) =
        routes.iter().partition(|route| route.starts_with("/v1/"));
    assert_eq!(
        others, NOT_OF_THE_API,
        "a route outside /v1 is new or gone: name it here, or describe it"
    );

    // The route each described path is answered by.
    let mut by_route: BTreeMap<&str, Vec<&Described>> = BTreeMap::new();
    for operation in &described {
        let route = of_the_api
            .iter()
            .filter_map(|route| fits(route, &operation.path).map(|fixed| (fixed, *route)))
            .max()
            .unwrap_or_else(|| {
                panic!(
                    "the description has {} {}, and the router has no route for it",
                    operation.method, operation.path
                )
            })
            .1;
        by_route.entry(route).or_default().push(operation);
    }

    // Described, so routed: the request reaches a handler.
    for operation in &described {
        let asked = probe(
            &router,
            &operation.method,
            &filled(&operation.path),
            operation.operation.get("requestBody").is_some(),
        )
        .await;
        assert!(
            !asked.no_such_path && asked.status != StatusCode::METHOD_NOT_ALLOWED,
            "the description has {} {}, and the server answers {}",
            operation.method,
            operation.path,
            asked.status
        );
        // And who may ask is as the description says: what it calls
        // open is answered without a token, and nothing else is.
        let open = operation.operation["security"]
            .as_array()
            .unwrap()
            .iter()
            .all(|way| way.as_object().unwrap().is_empty())
            || operation.operation["security"]
                .as_array()
                .unwrap()
                .iter()
                .any(|way| way.as_object().unwrap().is_empty());
        assert_eq!(
            asked.status != StatusCode::UNAUTHORIZED,
            open,
            "{} {} without a token is answered {}",
            operation.method,
            operation.path,
            asked.status
        );
    }

    // Routed, so described: every route, and every method it answers.
    for route in &of_the_api {
        let operations = by_route.get(route).unwrap_or_else(|| {
            panic!("the router has {route}, and the description has no path for it")
        });
        let mut addresses: Vec<String> = operations.iter().map(|op| filled(&op.path)).collect();
        addresses.push(filled(route));
        for method in METHODS {
            if operations.iter().any(|op| op.method == method) {
                continue;
            }
            for address in &addresses {
                let asked = probe(&router, &method, address, false).await;
                assert_eq!(
                    asked.status,
                    StatusCode::METHOD_NOT_ALLOWED,
                    "the router answers {method} on {route} ({address}), \
                     and the description does not have it"
                );
            }
        }
    }
}

#[tokio::test]
async fn the_description_names_the_scopes_as_the_documentation_does() {
    let (router, _) = router_and_routes().await;
    let document = served(&router).await;
    let docs = std::fs::read_to_string(repository_file("docs/api.md")).unwrap();

    let mut scoped = 0;
    let mut for_the_session = 0;
    for operation in described(&document) {
        let ways = operation.operation["security"].as_array().unwrap().clone();
        let scope = ways
            .iter()
            .find_map(|way| way.get("token"))
            .map(|scopes| scopes[0].as_str().unwrap().to_owned());
        let shared = ways.iter().any(|way| way.as_object().unwrap().is_empty());
        let request = format!("`{} {}`", operation.method, operation.path);
        match scope {
            // The three reads of a flight share one row of their own.
            Some(scope) if shared => {
                assert_eq!(scope, Scope::FlightsRead.as_str());
                assert!(docs.contains(&format!("{request}, `GET")) || docs.contains(&request));
            }
            Some(scope) => {
                assert!(Scope::from_name(&scope).is_some(), "{scope}");
                let row = format!("| {request} | `{scope}` |");
                assert!(docs.contains(&row), "docs/api.md lacks the row: {row}");
                scoped += 1;
            }
            None if !ways.is_empty() => {
                let row = format!("| {request} | the session only |");
                assert!(docs.contains(&row), "docs/api.md lacks the row: {row}");
                for_the_session += 1;
            }
            None => {}
        }
    }
    // And the reverse: the table has no row the description lacks.
    let rows = |ending: &str| {
        docs.lines()
            .filter(|line| line.starts_with("| `") && line.ends_with(ending))
            .count()
    };
    assert_eq!(
        for_the_session,
        rows("| the session only |"),
        "the session's own requests"
    );
    let rows_with_a_scope = docs
        .lines()
        .filter(|line| {
            line.starts_with("| `")
                && line.contains(" /v1/")
                && Scope::ALL
                    .iter()
                    .any(|scope| line.ends_with(&format!("| `{scope}` |")))
        })
        .count();
    assert_eq!(scoped, rows_with_a_scope, "the requests that need a scope");

    // The scopes the description lists are the scopes there are.
    let listed: Vec<&str> = document["components"]["securitySchemes"]["token"]["x-venti-scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scope| scope.as_str().unwrap())
        .collect();
    let all: Vec<&str> = Scope::ALL.into_iter().map(Scope::as_str).collect();
    assert_eq!(listed, all);
    let in_the_schema: Vec<&str> = document["components"]["schemas"]["Scope"]["x-venti-values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scope| scope.as_str().unwrap())
        .collect();
    assert_eq!(in_the_schema, all);
}

#[tokio::test]
async fn the_description_points_into_the_documentation_and_knows_its_error_codes() {
    let (router, _) = router_and_routes().await;
    let document = served(&router).await;
    let docs = std::fs::read_to_string(repository_file("docs/api.md")).unwrap();
    let headings: BTreeSet<String> = docs
        .lines()
        .filter(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().replace('`', ""))
        .collect();
    for operation in described(&document) {
        let heading = operation.operation["x-venti-docs"].as_str().unwrap();
        assert!(
            headings.contains(heading),
            "docs/api.md has no heading \"{heading}\" for {} {}",
            operation.method,
            operation.path
        );
    }

    // And the other way: every request that docs/api.md gives a heading
    // is described. This is what finds a new path that a handler of a
    // wildcard route answers (`/v1/sites/{id}/confirmations`), which the
    // router's list of routes cannot show.
    let in_the_description: BTreeSet<(String, String)> = described(&document)
        .iter()
        .map(|operation| (operation.method.to_string(), operation.path.clone()))
        .collect();
    for heading in &headings {
        let mut methods: Vec<&str> = Vec::new();
        for word in heading.split_whitespace() {
            if ["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&word) {
                methods.push(word);
            } else if word.starts_with("/v1/") {
                let path = word.split('?').next().unwrap_or(word);
                for method in methods.drain(..) {
                    assert!(
                        in_the_description.contains(&(method.to_owned(), path.to_owned())),
                        "docs/api.md has the heading \"{heading}\", \
                         and the description has no {method} {path}"
                    );
                }
            }
        }
    }

    // Every code the description lists is one the server's types know,
    // and docs/api.md has it. That the list is complete is not held here:
    // the type cannot be asked for its variants.
    for code in document["components"]["schemas"]["ErrorCode"]["x-venti-values"]
        .as_array()
        .unwrap()
    {
        let known: ErrorCode = serde_json::from_value(code.clone()).unwrap();
        assert_ne!(known, ErrorCode::Unknown, "{code}");
        let written = format!("`{}`", code.as_str().unwrap());
        assert!(docs.contains(&written), "docs/api.md lacks {written}");
    }
}

// --- The golden answers against the schemas ---------------------------------

/// Which part of an operation a golden file is.
enum Part {
    /// The answer with this status.
    Answer(u16),
    /// The request's body.
    Request,
    /// An error: every operation has the one shape for it.
    Error,
}

use Part::{Answer, Error, Request};

/// Every golden file that is JSON of `/v1`: the operation it belongs to
/// and which part of it.
const GOLDEN: &[(&str, &str, &str, Part)] = &[
    ("health.json", "get", "/v1/health", Answer(200)),
    ("stations.json", "get", "/v1/stations", Answer(200)),
    (
        "observations.json",
        "get",
        "/v1/stations/{id}/observations",
        Answer(200),
    ),
    ("airspace.json", "get", "/v1/airspace", Answer(200)),
    ("airspace_none.json", "get", "/v1/airspace", Answer(200)),
    ("cables.json", "get", "/v1/cables", Answer(200)),
    ("sites.json", "get", "/v1/sites", Answer(200)),
    ("sites_venti.json", "get", "/v1/sites", Answer(200)),
    ("sites_venti_pilots.json", "get", "/v1/sites", Answer(200)),
    ("site.json", "get", "/v1/sites/{id}", Answer(200)),
    (
        "site_of_a_source.json",
        "get",
        "/v1/sites/{id}",
        Answer(200),
    ),
    (
        "site_history.json",
        "get",
        "/v1/sites/{id}/history",
        Answer(200),
    ),
    (
        "site_flights.json",
        "get",
        "/v1/sites/{id}/flights",
        Answer(200),
    ),
    (
        "public_flights_at_site.json",
        "get",
        "/v1/flights/public",
        Answer(200),
    ),
    (
        "site_candidates.json",
        "get",
        "/v1/sites/candidates",
        Answer(200),
    ),
    ("site_changes.json", "get", "/v1/sites/changes", Answer(200)),
    (
        "site_changes_pilots.json",
        "get",
        "/v1/sites/changes",
        Answer(200),
    ),
    ("dumps.json", "get", "/v1/dumps", Answer(200)),
    ("site_add_request.json", "post", "/v1/sites", Request),
    ("site_written.json", "post", "/v1/sites", Answer(201)),
    ("site_change_request.json", "put", "/v1/sites/{id}", Request),
    (
        "site_restore_request.json",
        "post",
        "/v1/sites/{id}/revisions",
        Request,
    ),
    ("site_too_close.json", "post", "/v1/sites", Error),
    (
        "error_sites_for_partners.json",
        "put",
        "/v1/sites/{id}",
        Error,
    ),
    (
        "site_confirm_request.json",
        "put",
        "/v1/sites/{id}/confirmation",
        Request,
    ),
    (
        "site_confirmations.json",
        "get",
        "/v1/sites/{id}/confirmations",
        Answer(200),
    ),
    ("forecast.json", "get", "/v1/forecast", Answer(200)),
    ("thermals.json", "get", "/v1/thermals", Answer(200)),
    (
        "thermal_hours.json",
        "get",
        "/v1/thermals/hours",
        Answer(200),
    ),
    (
        "thermal_hours_wind.json",
        "get",
        "/v1/thermals/hours",
        Answer(200),
    ),
    (
        "route_weather.json",
        "get",
        "/v1/route/weather",
        Answer(200),
    ),
    ("radar.json", "get", "/v1/radar", Answer(200)),
    ("foehn.json", "get", "/v1/foehn", Answer(200)),
    ("radar_two_layers.json", "get", "/v1/radar", Answer(200)),
    ("maps.json", "get", "/v1/maps", Answer(200)),
    ("sign_in.json", "post", "/v1/sessions", Answer(201)),
    (
        "google_sign_up.json",
        "post",
        "/v1/auth/google",
        Answer(200),
    ),
    (
        "dev_google_id_token.json",
        "get",
        "/v1/dev/google-id-token",
        Answer(200),
    ),
    ("sessions.json", "get", "/v1/sessions", Answer(200)),
    ("account.json", "get", "/v1/account", Answer(200)),
    ("account_suspended.json", "get", "/v1/account", Answer(200)),
    (
        "account_settings_request.json",
        "patch",
        "/v1/account",
        Request,
    ),
    ("account_settings.json", "patch", "/v1/account", Answer(200)),
    ("tokens.json", "get", "/v1/account/tokens", Answer(200)),
    (
        "token_create_request.json",
        "post",
        "/v1/account/tokens",
        Request,
    ),
    (
        "token_created.json",
        "post",
        "/v1/account/tokens",
        Answer(201),
    ),
    (
        "oauth_request.json",
        "get",
        "/v1/oauth/requests/{id}",
        Answer(200),
    ),
    (
        "oauth_request_answer.json",
        "post",
        "/v1/oauth/requests/{id}",
        Request,
    ),
    (
        "oauth_request_answered.json",
        "post",
        "/v1/oauth/requests/{id}",
        Answer(200),
    ),
    ("error_insufficient_scope.json", "get", "/v1/groups", Error),
    ("error_missing_bbox.json", "get", "/v1/stations", Error),
    (
        "kept_site_changes.json",
        "get",
        "/v1/account/kept-sites/changes",
        Answer(200),
    ),
    (
        "kept_site_keep_request.json",
        "put",
        "/v1/account/kept-sites/{id}",
        Request,
    ),
    (
        "kept_site.json",
        "put",
        "/v1/account/kept-sites/{id}",
        Answer(200),
    ),
    ("alerts.json", "get", "/v1/account/alerts", Answer(200)),
    (
        "alert_request.json",
        "put",
        "/v1/account/alerts/{id}",
        Request,
    ),
    ("alert.json", "put", "/v1/account/alerts/{id}", Answer(200)),
    (
        "alerts_fired.json",
        "get",
        "/v1/account/alerts/fired",
        Answer(200),
    ),
    (
        "followers.json",
        "get",
        "/v1/pilots/{handle}/followers",
        Answer(200),
    ),
    (
        "follow.json",
        "put",
        "/v1/pilots/{handle}/follow",
        Answer(200),
    ),
    ("feed.json", "get", "/v1/account/feed", Answer(200)),
    ("cheer.json", "put", "/v1/flights/{id}/cheer", Answer(200)),
    ("cheers.json", "get", "/v1/flights/{id}/cheers", Answer(200)),
    (
        "comments.json",
        "get",
        "/v1/flights/{id}/comments",
        Answer(200),
    ),
    (
        "comment_request.json",
        "post",
        "/v1/flights/{id}/comments",
        Request,
    ),
    (
        "comment.json",
        "post",
        "/v1/flights/{id}/comments",
        Answer(201),
    ),
    (
        "comments_open.json",
        "patch",
        "/v1/flights/{id}/comments",
        Answer(200),
    ),
    (
        "notifications.json",
        "get",
        "/v1/account/notifications",
        Answer(200),
    ),
    ("push_key.json", "get", "/v1/push/key", Answer(200)),
    (
        "push_subscribe_request.json",
        "post",
        "/v1/account/push",
        Request,
    ),
    (
        "push_remove_request.json",
        "delete",
        "/v1/account/push",
        Request,
    ),
    ("upload.json", "post", "/v1/flights", Answer(201)),
    ("flights.json", "get", "/v1/flights", Answer(200)),
    (
        "flights_visibility_request.json",
        "patch",
        "/v1/flights",
        Request,
    ),
    (
        "flights_visibility.json",
        "patch",
        "/v1/flights",
        Answer(200),
    ),
    (
        "flight_changes.json",
        "get",
        "/v1/flights/changes",
        Answer(200),
    ),
    ("flight.json", "get", "/v1/flights/{id}", Answer(200)),
    (
        "flight_taken_down.json",
        "get",
        "/v1/flights/{id}",
        Answer(200),
    ),
    (
        "flight_track.json",
        "get",
        "/v1/flights/{id}/track",
        Answer(200),
    ),
    ("pilot.json", "get", "/v1/pilots/{handle}", Answer(200)),
    (
        "public_flights.json",
        "get",
        "/v1/flights/public",
        Answer(200),
    ),
    (
        "public_flights_best.json",
        "get",
        "/v1/flights/public",
        Answer(200),
    ),
    (
        "public_flight_changes.json",
        "get",
        "/v1/flights/public/changes",
        Answer(200),
    ),
    ("ranking.json", "get", "/v1/ranking", Answer(200)),
    ("pilots.json", "get", "/v1/pilots", Answer(200)),
    (
        "pilot_statistics.json",
        "get",
        "/v1/pilots/{handle}/statistics",
        Answer(200),
    ),
    ("live.json", "get", "/v1/live", Answer(200)),
    ("live_others.json", "get", "/v1/live/others", Answer(200)),
    ("live_climbs.json", "get", "/v1/live/climbs", Answer(200)),
    ("live_sites.json", "get", "/v1/live/sites", Answer(200)),
    (
        "thermals_flown.json",
        "get",
        "/v1/thermals/flown",
        Answer(200),
    ),
    (
        "alert_pilots_request.json",
        "post",
        "/v1/account/alerts",
        Request,
    ),
    (
        "alerts_fired_pilots.json",
        "get",
        "/v1/account/alerts/fired",
        Answer(200),
    ),
    ("live_track.json", "get", "/v1/live/{id}", Answer(200)),
    (
        "live_positions_request.json",
        "post",
        "/v1/live/positions",
        Request,
    ),
    (
        "live_positions.json",
        "post",
        "/v1/live/positions",
        Answer(200),
    ),
    ("report_request.json", "post", "/v1/reports", Request),
    ("report_group_request.json", "post", "/v1/reports", Request),
    ("report_plan_request.json", "post", "/v1/reports", Request),
    ("report_site_request.json", "post", "/v1/reports", Request),
    ("report.json", "post", "/v1/reports", Answer(201)),
    ("groups.json", "get", "/v1/groups", Answer(200)),
    ("group.json", "get", "/v1/groups/{id}", Answer(200)),
    ("group_create_request.json", "post", "/v1/groups", Request),
    ("group_created.json", "post", "/v1/groups", Answer(201)),
    (
        "group_join_request.json",
        "post",
        "/v1/groups/join",
        Request,
    ),
    ("group_joined.json", "post", "/v1/groups/join", Answer(201)),
    (
        "group_invite.json",
        "post",
        "/v1/groups/{id}/invite",
        Answer(200),
    ),
    ("plans.json", "get", "/v1/groups/{id}/plans", Answer(200)),
    ("plan_board.json", "get", "/v1/groups/plans", Answer(200)),
    (
        "plan_request.json",
        "post",
        "/v1/groups/{id}/plans",
        Request,
    ),
    ("plan.json", "post", "/v1/groups/{id}/plans", Answer(201)),
];

/// The schemas no golden file reaches. Each is a record of a request
/// that the tests build in Rust and keep no file of, or a part of a
/// record that the recorded data does not have. Nothing holds these to
/// the code but their reading; a golden file for one takes it off this
/// list.
const SCHEMAS_NO_GOLDEN_FILE_REACHES: &[&str] = &[
    "ChangePasswordRequest",
    "Circle",
    "ClientAddressesHealth",
    "CreateAccountRequest",
    "CreateGoogleAccountRequest",
    "DayTime",
    "DeleteAccountRequest",
    "DeletedFlight",
    "Frequency",
    "GoogleSignInRequest",
    "HandOverGroupRequest",
    "LinkGoogleRequest",
    "OperatingHours",
    "RemoveSiteRequest",
    "SignInRequest",
    "TokenClient",
    "UpdateFlightRequest",
    "UpdateGroupRequest",
];

/// The keywords of a schema that are read past: they say something to a
/// person or to a generator, and no golden answer is checked against
/// them.
const KEYWORDS_NOT_CHECKED: [&str; 1] = ["description"];

struct Validator<'a> {
    document: &'a Value,
    reached: BTreeSet<String>,
    problems: Vec<String>,
}

impl Validator<'_> {
    fn problem(&mut self, at: &str, what: impl std::fmt::Display) {
        self.problems.push(format!("{at}: {what}"));
    }

    fn check(&mut self, schema: &Value, value: &Value, at: &str) {
        let Some(schema) = schema.as_object() else {
            return self.problem(at, "the schema is not an object");
        };
        if let Some(reference) = schema.get("$ref") {
            let name = reference
                .as_str()
                .and_then(|path| path.strip_prefix("#/components/schemas/"));
            let Some(name) = name else {
                return self.problem(at, format!("a reference that leads nowhere: {reference}"));
            };
            let document = self.document;
            let target = &document["components"]["schemas"][name];
            if target.is_null() {
                return self.problem(at, format!("no schema is called {name}"));
            }
            self.reached.insert(name.to_owned());
            return self.check(target, value, at);
        }
        for keyword in schema.keys() {
            let read = [
                "type",
                "anyOf",
                "properties",
                "required",
                "additionalProperties",
                "items",
                "enum",
                "format",
                "x-venti-values",
            ];
            if !read.contains(&keyword.as_str())
                && !KEYWORDS_NOT_CHECKED.contains(&keyword.as_str())
            {
                self.problem(
                    at,
                    format!("the schema uses `{keyword}`, which this validator does not read"),
                );
            }
        }
        if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
            // Valid against one of them; the problems of the last are told.
            let before = self.problems.len();
            for branch in branches {
                self.problems.truncate(before);
                self.check(branch, value, at);
                if self.problems.len() == before {
                    break;
                }
            }
            return;
        }
        if let Some(kind) = schema.get("type") {
            let fits = match kind.as_str() {
                Some("null") => value.is_null(),
                Some("string") => value.is_string(),
                Some("boolean") => value.is_boolean(),
                Some("number") => value.is_number(),
                Some("integer") => value.is_i64() || value.is_u64(),
                Some("array") => value.is_array(),
                Some("object") => value.is_object(),
                _ => false,
            };
            if !fits {
                return self.problem(at, format!("not of the type {kind}: {}", short(value)));
            }
        } else if value.is_null() {
            return self.problem(at, "null, where the schema does not say it may be");
        }
        for closed in ["enum", "x-venti-values"] {
            if let Some(values) = schema.get(closed).and_then(Value::as_array)
                && !values.contains(value)
            {
                self.problem(
                    at,
                    format!("{} is not among the values listed", short(value)),
                );
            }
        }
        if let (Some(format), Some(text)) =
            (schema.get("format").and_then(Value::as_str), value.as_str())
        {
            let fits = match format {
                "date-time" => chrono::DateTime::parse_from_rfc3339(text).is_ok(),
                "date" => text.parse::<chrono::NaiveDate>().is_ok(),
                _ => {
                    self.problem(at, format!("the format `{format}` is not one this reads"));
                    true
                }
            };
            if !fits {
                self.problem(at, format!("{text} is not a {format}"));
            }
        }
        if let Some(fields) = value.as_object() {
            let properties = schema.get("properties").and_then(Value::as_object);
            let further = schema.get("additionalProperties");
            for name in schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if !fields.contains_key(name.as_str().unwrap_or_default()) {
                    self.problem(at, format!("lacks {name}, which the schema requires"));
                }
            }
            for (name, field) in fields {
                let at = format!("{at}.{name}");
                match (properties.and_then(|all| all.get(name)), further) {
                    (Some(schema), _) | (None, Some(schema)) => self.check(schema, field, &at),
                    // A schema that says nothing of its fields takes any.
                    (None, None) if properties.is_none() && schema.get("type").is_none() => {}
                    (None, None) => self.problem(&at, "a field the schema does not name"),
                }
            }
        }
        if let (Some(entries), Some(items)) = (value.as_array(), schema.get("items")) {
            for (place, entry) in entries.iter().enumerate() {
                // One problem of a kind per list is enough to read.
                let before = self.problems.len();
                self.check(items, entry, &format!("{at}[{place}]"));
                if self.problems.len() > before {
                    break;
                }
            }
        }
    }
}

fn short(value: &Value) -> String {
    let text = value.to_string();
    text.chars().take(80).collect()
}

/// The schema the description gives for a part of an operation.
fn schema_of<'a>(document: &'a Value, method: &str, path: &str, part: &Part) -> &'a Value {
    let operation = &document["paths"][path][method];
    assert!(
        operation.is_object(),
        "the description has no {method} {path}"
    );
    let content = match part {
        Answer(status) => &operation["responses"][status.to_string()]["content"],
        Request => &operation["requestBody"]["content"],
        Error => {
            assert_eq!(
                operation["responses"]["default"]["$ref"],
                "#/components/responses/Error"
            );
            &document["components"]["responses"]["Error"]["content"]
        }
    };
    let schema = content
        .as_object()
        .and_then(|types| types.values().next())
        .map(|media| &media["schema"]);
    schema.unwrap_or_else(|| panic!("the description gives {method} {path} no schema there"))
}

#[tokio::test]
async fn every_golden_answer_is_valid_against_its_schema() {
    let (router, _) = router_and_routes().await;
    let document = served(&router).await;
    let mut validator = Validator {
        document: &document,
        reached: BTreeSet::new(),
        problems: Vec::new(),
    };

    for (file, method, path, part) in GOLDEN {
        let text = std::fs::read_to_string(golden(file)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let schema = schema_of(&document, method, path, part);
        validator.check(schema, &value, file);
    }
    assert!(
        validator.problems.is_empty(),
        "golden answers that the description does not describe:\n{}",
        validator.problems.join("\n")
    );

    // Every golden file is in the table, or is not JSON of /v1.
    let known: BTreeSet<&str> = GOLDEN.iter().map(|(file, ..)| *file).collect();
    for entry in std::fs::read_dir(golden("")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        // A file of a pilot's archive is no answer of an operation: the
        // archive's own test holds it.
        let of_the_api =
            name.ends_with(".json") && !name.starts_with("mcp_") && !name.starts_with("archive_");
        assert!(
            !of_the_api || known.contains(name.as_str()),
            "{name} is a golden answer that is checked against no schema: \
             name its operation in GOLDEN"
        );
    }

    // Every schema is reached by some golden file, or is named as not.
    let all: BTreeSet<String> = document["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let not_reached: Vec<&str> = all
        .difference(&validator.reached)
        .map(String::as_str)
        .collect();
    assert_eq!(
        not_reached, SCHEMAS_NO_GOLDEN_FILE_REACHES,
        "the schemas no golden file reaches"
    );
}

#[tokio::test]
async fn every_reference_in_the_description_leads_somewhere() {
    let (router, _) = router_and_routes().await;
    let document = served(&router).await;
    fn walk(document: &Value, value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                if let Some(reference) = fields.get("$ref").and_then(Value::as_str) {
                    let pointer = reference.trim_start_matches('#');
                    if document.pointer(pointer).is_none() {
                        found.push(reference.to_owned());
                    }
                }
                for field in fields.values() {
                    walk(document, field, found);
                }
            }
            Value::Array(entries) => {
                for entry in entries {
                    walk(document, entry, found);
                }
            }
            _ => {}
        }
    }
    let mut lost = Vec::new();
    walk(&document, &document, &mut lost);
    assert!(lost.is_empty(), "{lost:?}");
}
