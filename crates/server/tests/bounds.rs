//! The bounds the documents state are the server's own.
//!
//! Every number `docs/api.md` and `docs/mcp.md` give as a bound, and every
//! `maxLength`, `maxItems` and `maximum` of a request in the API's
//! description, is written here once, next to the constant the server
//! enforces it with. A constant that changes and a document that is not
//! changed with it fail here, and so does a document that is reworded
//! without this file: the sentence is then said again here, with its
//! number taken from the constant.
//!
//! The reads that spend an address's budget are asked here one by one
//! through the development mode's router, and each must answer with the
//! address's budget in `RateLimit`; the document must list each of them,
//! and say how many there are.

mod common;

use std::path::PathBuf;

use axum::http::{Method, StatusCode, header};
use serde_json::Value;
use venti_api_types::v1::{self, Handle, Password};
use venti_server::accounts::Limits;
use venti_server::api;
use venti_server::dev::{DEMO_HANDLE, DEMO_OPEN_PLANS, DEMO_SITES, DEMO_SITES_BBOX};
use venti_tools::{airspace, community, sites, wind};

use common::{ask, recorded};

fn repository_file(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn document(path: &str) -> String {
    std::fs::read_to_string(repository_file(path)).unwrap()
}

/// A number as the documents write it: 3,600 and 50,000, but 5000 where a
/// document writes it so.
fn grouped(n: impl Into<u64>) -> String {
    let digits = n.into().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A small number as a word, as the documents write those.
fn word(n: usize) -> &'static str {
    [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ][n]
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn assert_says(path: &str, text: &str, stated: &[String]) {
    let missing: Vec<&String> = stated.iter().filter(|s| !text.contains(*s)).collect();
    assert!(
        missing.is_empty(),
        "{path} does not say, as the server's constants have it:\n{}",
        missing
            .iter()
            .map(|s| format!("  {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// --- The reads that spend an address's budget ---------------------------------

/// Every request the address's budget is spent by, with the heading of
/// `docs/api.md` that describes it.
const COUNTED: [(&str, &str); 16] = [
    (
        "/v1/flights/public",
        "get-v1flightspublicpilotbboxfromtosortafterlimit",
    ),
    (
        "/v1/flights/public/changes",
        "get-v1flightspublicchangessincepilotlimit",
    ),
    ("/v1/flights/{flight}/track", "get-v1flightsidtrack"),
    ("/v1/sites/{site}/flights", "get-v1sitesidflights"),
    (
        "/v1/sites/candidates?bbox={bbox}",
        "get-v1sitescandidatesbboxwestsoutheastnorth",
    ),
    (
        "/v1/thermals/flown?bbox={bbox}",
        "get-v1thermalsflownbboxmonthhours",
    ),
    ("/v1/pilots?q={handle}", "get-v1pilotsqlimit"),
    (
        "/v1/pilots/near?lat=47.3&lon=10.9",
        "get-v1pilotsnearlatlonradius_kmlimit",
    ),
    (
        "/v1/pilots/{handle}/statistics",
        "get-v1pilotshandlestatistics",
    ),
    (
        "/v1/pilots/{handle}/followers",
        "get-v1pilotshandlefollowersafterlimit",
    ),
    (
        "/v1/pilots/{handle}/following",
        "get-v1pilotshandlefollowingafterlimit",
    ),
    (
        "/v1/flights/{flight}/cheers",
        "get-v1flightsidcheersafterlimit",
    ),
    (
        "/v1/flights/{flight}/comments",
        "get-v1flightsidcommentsafterlimit",
    ),
    ("/v1/ranking", "get-v1rankingbboxfromtomonthlimit"),
    (
        "/v1/open-plans?lat=47.3&lon=10.9",
        "get-v1open-planslatlonradius_kmfromto",
    ),
    (
        "/v1/open-plans/{plan}",
        "get-put-and-delete-v1open-plansplan",
    ),
];

/// Reads open to anyone that the document says are not counted.
const NOT_COUNTED: [&str; 3] = [
    "/v1/flights/{flight}",
    "/v1/pilots/{handle}",
    "/v1/sites/{site}",
];

#[tokio::test]
async fn every_read_the_document_counts_spends_the_address_budget_and_no_other_does() {
    let router = recorded().await;
    let page = ask(Method::GET, v1::PUBLIC_FLIGHTS_PATH)
        .send(&router)
        .await;
    let flights: Value = page.json();
    let flight = flights["flights"][0]["id"].as_str().unwrap().to_owned();
    let path = |template: &str| {
        template
            .replace("{flight}", &flight)
            .replace("{site}", DEMO_SITES[0])
            .replace("{bbox}", DEMO_SITES_BBOX)
            .replace("{handle}", DEMO_HANDLE)
            .replace("{plan}", DEMO_OPEN_PLANS[0])
    };
    let budget = header::HeaderName::from_static("ratelimit");
    for (template, _) in COUNTED {
        let asked = ask(Method::GET, &path(template)).send(&router).await;
        assert_eq!(asked.status, StatusCode::OK, "{template}: {}", asked.text());
        assert!(
            asked.header(budget.clone()).starts_with("\"address\";"),
            "{template} does not spend the address's budget"
        );
    }
    for template in NOT_COUNTED {
        let asked = ask(Method::GET, &path(template)).send(&router).await;
        assert_eq!(asked.status, StatusCode::OK, "{template}: {}", asked.text());
        assert!(
            !asked.headers.contains_key(&budget),
            "{template} spends a budget the document does not name"
        );
    }

    let docs = document("docs/api.md");
    let list = docs
        .lines()
        .find(|line| line.contains("reads are open to anyone and counted together"))
        .expect("docs/api.md lists the reads that spend an address's budget");
    let stated = format!(
        "{} reads are open to anyone and counted together",
        capitalised(word(COUNTED.len()))
    );
    assert!(
        list.starts_with(&stated),
        "docs/api.md does not say: {stated}"
    );
    for (template, heading) in COUNTED {
        let link = format!("](#{heading})");
        // This list is the page the paragraph stands under.
        let named = list.contains(&link) || template == "/v1/flights/public";
        assert!(
            named,
            "docs/api.md's list does not link {template} ({heading})"
        );
    }
}

// --- What docs/api.md states --------------------------------------------------

#[test]
fn the_api_document_states_the_servers_bounds() {
    let limits = Limits::default();
    let window = |duration: chrono::Duration| duration.num_seconds();
    let stated = vec![
        // The budgets.
        format!(
            "a token for a program makes [{} requests an hour](#limits)",
            grouped(limits.token_requests_per_window)
        ),
        format!(
            "**A token for a program** makes at most **{} requests an hour**",
            grouped(limits.token_requests_per_window)
        ),
        format!(
            "RateLimit-Policy: \"token\";q={};w={}",
            limits.token_requests_per_window,
            window(limits.token_window)
        ),
        format!(
            "[{} reads an hour per address](#how-often-an-address-may-read)",
            limits.public_flight_reads_per_address
        ),
        format!(
            "RateLimit-Policy: \"address\";q={};w={}",
            limits.public_flight_reads_per_address,
            window(limits.public_flight_window)
        ),
        format!(
            "RateLimit-Policy: \"account\";q={};w={}",
            limits.public_flight_reads_per_account,
            window(limits.public_flight_window)
        ),
        format!(
            "counted for the signed-in account, {} an hour",
            grouped(limits.public_flight_reads_per_account)
        ),
        format!(
            "and {} an hour for a signed-in pilot's session",
            grouped(limits.public_flight_reads_per_account)
        ),
        format!(
            "An address may make **{} of them in an hour**",
            limits.public_flight_reads_per_address
        ),
        format!(
            "one /48 network together gets {}",
            grouped(limits.public_flight_reads_per_network)
        ),
        format!(
            "**{} an hour** whatever address the pilot reads from",
            grouped(limits.public_flight_reads_per_account)
        ),
        format!(
            "A page holds at most {} flights or changes",
            v1::MAX_PAGE_SIZE
        ),
        format!(
            "a track at most {} positions",
            grouped(v1::MAX_TRACK_POINTS as u64)
        ),
        // A request's size and time.
        format!("a body of more than {} kB", api::MAX_BODY_BYTES / 1024),
        format!(
            "could not answer within {} seconds; for [an account's archive](#get-v1accountarchive), within {}",
            word(api::DEADLINE.as_secs() as usize),
            api::ARCHIVE_DEADLINE.as_secs()
        ),
    ];
    assert_says("docs/api.md", &document("docs/api.md"), &stated);
}

// --- What docs/mcp.md states --------------------------------------------------

#[test]
fn the_connector_document_states_the_servers_bounds() {
    let limits = Limits::default();
    // The document says "a minute" for the connector's window.
    assert_eq!(limits.connector_window, chrono::Duration::minutes(1));
    let stated = vec![
        // What a request may cost.
        format!(
            "| The size of a request | {} kB | `413` |",
            api::MCP_MAX_BODY_BYTES / 1024
        ),
        format!(
            "| The time for an answer | {} seconds |",
            api::DEADLINE.as_secs()
        ),
        format!(
            "| Requests from one address | {} a minute |",
            limits.connector_requests_per_address
        ),
        format!(
            "| Requests from one IPv6 /48 network | {} a minute |",
            limits.connector_requests_per_network
        ),
        format!(
            "| Messages in one request | {}, every message of a batch counted",
            venti_mcp::MAX_BATCH
        ),
        format!(
            "| Requests with one token | {} an hour",
            grouped(limits.token_requests_per_window)
        ),
        format!(
            "[{} an hour](api.md#how-often-an-address-may-read), {} for an IPv6 /48 network",
            limits.public_flight_reads_per_address,
            grouped(limits.public_flight_reads_per_network)
        ),
        format!(
            "a box with more than {} sites is refused; the result lists the nearest {}",
            v1::MAX_SITES,
            community::MAX_SITES
        ),
        format!(
            "The API's: {} degrees of latitude and of longitude",
            v1::MAX_CANDIDATE_BOX_DEG
        ),
        format!(
            "The server counts {} addresses at a time",
            grouped(limits.watched_keys as u64)
        ),
        // The tools' arguments.
        format!(
            "`radius_km`, {} if left out, {} at most |",
            wind::DEFAULT_RADIUS_KM,
            wind::MAX_RADIUS_KM
        ),
        format!(
            "`hours`, {} if left out, {} at most |",
            wind::DEFAULT_HOURS,
            wind::MAX_HOURS
        ),
        format!(
            "`radius_km`, {} if left out, {} at most |",
            airspace::DEFAULT_RADIUS_KM,
            airspace::MAX_RADIUS_KM
        ),
        format!("the nearest {} to the box's middle", community::MAX_SITES),
        format!(
            "`site_id`; `after`; `limit`, {} if left out, {} at most |",
            community::DEFAULT_REVISIONS,
            community::MAX_REVISIONS
        ),
        format!(
            "public flights that began within {} m of a site",
            v1::SITE_FLIGHTS_WITHIN_M
        ),
        format!(
            "`north`, {} degrees each way at most |",
            v1::MAX_CANDIDATE_BOX_DEG
        ),
        format!(
            "`limit`, {} if left out, {} at most; `after` |",
            community::DEFAULT_FLIGHTS,
            community::MAX_FLIGHTS
        ),
        format!(
            "| `year`; `limit`, {} if left out, {} at most |",
            venti_tools::DEFAULT_LIMIT,
            venti_tools::MAX_LIMIT
        ),
        // What a result holds.
        format!(
            "| Stations that `wind_stations` lists | {}, the nearest |",
            wind::MAX_STATIONS
        ),
        format!("| Hours of `station_history` | {} |", wind::MAX_HOURS),
        format!(
            "| Airspaces that `airspace_at` lists near the place, beside those over it | {} |",
            airspace::MAX_NEARBY
        ),
        format!(
            "| Sites that `sites_in_box` lists | {}, the nearest to the box's middle |",
            community::MAX_SITES
        ),
        format!(
            "| Revisions of `site_history` | {} |",
            community::MAX_REVISIONS
        ),
        format!(
            "| Flights of `public_flights` | {} |",
            community::MAX_FLIGHTS
        ),
        format!("| Flights of `list_flights` | {} |", venti_tools::MAX_LIMIT),
        format!(
            "| Days from `from_date` to `to_date` of `public_flights` | {} |",
            community::MAX_DAYS
        ),
        format!(
            "| A cursor (`after`) | {} characters |",
            community::MAX_CURSOR_CHARS
        ),
        format!("| Layers that a result describes | {} |", wind::MAX_LAYERS),
        format!("| Credits that a result lists | {} |", wind::MAX_CREDITS),
        format!(
            "| Spans of wind direction of one site | {} |",
            community::MAX_SECTORS
        ),
        format!(
            "| A name: of a station, an airspace, a site, a pilot, a provider, a glider | {} characters |",
            wind::MAX_NAME_CHARS
        ),
        format!(
            "| An airspace's remark | {} characters |",
            airspace::MAX_REMARK_CHARS
        ),
        format!(
            "| A site's description, fee and time without flying, a revision's line | {} characters |",
            community::MAX_TEXT_CHARS
        ),
        format!(
            "| A credit's text | {} characters |",
            wind::MAX_CREDIT_CHARS
        ),
        format!(
            "| A licence's name | {} characters |",
            wind::MAX_LICENCE_CHARS
        ),
        format!(
            "| A credit's link | {} characters, plain `https`",
            sites::MAX_URL_CHARS
        ),
        format!(
            "| A source's notice | {} characters |",
            wind::MAX_NOTICE_CHARS
        ),
        format!(
            "| The terms of flights | {} characters |",
            wind::MAX_NOTICE_CHARS * 2
        ),
        format!(
            "| A source's, a layer's or a model's identifier | {} characters |",
            wind::MAX_ID_CHARS
        ),
        // The same in the words of "Airspace is advice".
        format!(
            "at most {} characters of a name and {} of a remark",
            airspace::MAX_NAME_CHARS,
            airspace::MAX_REMARK_CHARS
        ),
        format!(
            "a station's name is one line of at most {} characters; a credit's text one of at most {}, its licence's name at most {} and its link at most {} plain characters",
            wind::MAX_NAME_CHARS,
            wind::MAX_CREDIT_CHARS,
            wind::MAX_LICENCE_CHARS,
            sites::MAX_URL_CHARS
        ),
        format!("a source's notice at most {}", wind::MAX_NOTICE_CHARS),
        format!(
            "a result lists {} credits at the most",
            word(wind::MAX_CREDITS)
        ),
    ];
    // The names of every kind are cut at one length.
    assert_eq!(airspace::MAX_NAME_CHARS, wind::MAX_NAME_CHARS);
    assert_eq!(community::MAX_NAME_CHARS, wind::MAX_NAME_CHARS);
    assert_says("docs/mcp.md", &document("docs/mcp.md"), &stated);
}

// --- What the API's description states ----------------------------------------

#[test]
fn the_description_bounds_each_request_as_the_server_does() {
    let description: Value = serde_json::from_str(&document("docs/openapi.json")).unwrap();
    let schemas = &description["components"]["schemas"];
    let site_comment = v1::MAX_SITE_COMMENT_LENGTH;
    // (record, field, keyword, the server's bound)
    let bounds: Vec<(&str, &str, &str, u64)> = vec![
        (
            "AddSiteRequest",
            "comment",
            "maxLength",
            site_comment as u64,
        ),
        (
            "ChangeSiteRequest",
            "comment",
            "maxLength",
            site_comment as u64,
        ),
        (
            "RemoveSiteRequest",
            "comment",
            "maxLength",
            site_comment as u64,
        ),
        (
            "RestoreSiteRequest",
            "comment",
            "maxLength",
            site_comment as u64,
        ),
        (
            "ConfirmSiteRequest",
            "note",
            "maxLength",
            site_comment as u64,
        ),
        (
            "AlertRequest",
            "name",
            "maxLength",
            v1::MAX_ALERT_NAME_LENGTH as u64,
        ),
        (
            "CommentRequest",
            "text",
            "maxLength",
            v1::MAX_COMMENT_LENGTH as u64,
        ),
        (
            "CreateAccessTokenRequest",
            "name",
            "maxLength",
            v1::MAX_TOKEN_NAME_LENGTH as u64,
        ),
        (
            "CreateAccountRequest",
            "handle",
            "minLength",
            Handle::MIN_LENGTH as u64,
        ),
        (
            "CreateAccountRequest",
            "handle",
            "maxLength",
            Handle::MAX_LENGTH as u64,
        ),
        (
            "CreateAccountRequest",
            "display_name",
            "maxLength",
            v1::MAX_DISPLAY_NAME_LENGTH as u64,
        ),
        (
            "CreateAccountRequest",
            "password",
            "minLength",
            Password::MIN_LENGTH as u64,
        ),
        (
            "CreateAccountRequest",
            "password",
            "maxLength",
            Password::MAX_LENGTH as u64,
        ),
        (
            "ChangePasswordRequest",
            "new_password",
            "minLength",
            Password::MIN_LENGTH as u64,
        ),
        (
            "ChangePasswordRequest",
            "new_password",
            "maxLength",
            Password::MAX_LENGTH as u64,
        ),
        (
            "CreateGoogleAccountRequest",
            "handle",
            "minLength",
            Handle::MIN_LENGTH as u64,
        ),
        (
            "CreateGoogleAccountRequest",
            "handle",
            "maxLength",
            Handle::MAX_LENGTH as u64,
        ),
        (
            "CreateGoogleAccountRequest",
            "display_name",
            "maxLength",
            v1::MAX_DISPLAY_NAME_LENGTH as u64,
        ),
        (
            "CreateGroupRequest",
            "name",
            "maxLength",
            v1::MAX_GROUP_NAME_LENGTH as u64,
        ),
        (
            "CreateGroupRequest",
            "description",
            "maxLength",
            v1::MAX_GROUP_DESCRIPTION_LENGTH as u64,
        ),
        (
            "UpdateGroupRequest",
            "name",
            "maxLength",
            v1::MAX_GROUP_NAME_LENGTH as u64,
        ),
        (
            "UpdateGroupRequest",
            "description",
            "maxLength",
            v1::MAX_GROUP_DESCRIPTION_LENGTH as u64,
        ),
        (
            "DeviceRequest",
            "token",
            "maxLength",
            v1::MAX_DEVICE_TOKEN_LENGTH as u64,
        ),
        (
            "OpenPlanRequest",
            "place",
            "maxLength",
            v1::MAX_PLAN_PLACE_LENGTH as u64,
        ),
        (
            "OpenPlanRequest",
            "note",
            "maxLength",
            v1::MAX_OPEN_PLAN_NOTE_LENGTH as u64,
        ),
        (
            "OpenPlanRequest",
            "seats",
            "maximum",
            u64::from(v1::MAX_OPEN_PLAN_SEATS),
        ),
        (
            "PlanRequest",
            "place",
            "maxLength",
            v1::MAX_PLAN_PLACE_LENGTH as u64,
        ),
        (
            "PlanRequest",
            "note",
            "maxLength",
            v1::MAX_PLAN_NOTE_LENGTH as u64,
        ),
        (
            "PushSubscriptionRequest",
            "endpoint",
            "maxLength",
            v1::MAX_PUSH_ENDPOINT_LENGTH as u64,
        ),
        (
            "ReportRequest",
            "text",
            "maxLength",
            v1::MAX_REPORT_TEXT_LENGTH as u64,
        ),
        (
            "MessageRequest",
            "text",
            "maxLength",
            v1::MAX_MESSAGE_LENGTH as u64,
        ),
        (
            "MessageRequest",
            "pictures",
            "maxItems",
            v1::MAX_MESSAGE_PICTURES as u64,
        ),
        (
            "MessageRequest",
            "links",
            "maxItems",
            v1::MAX_MESSAGE_LINKS as u64,
        ),
    ];
    for (record, field, keyword, bound) in bounds {
        let schema = &schemas[record]["properties"][field];
        // A field that may be null bounds its other form.
        let described = schema[keyword].as_u64().or_else(|| {
            schema["anyOf"]
                .as_array()?
                .iter()
                .find_map(|form| form[keyword].as_u64())
        });
        assert_eq!(
            described,
            Some(bound),
            "docs/openapi.json: {record}.{field} has no {keyword} of {bound}, \
             which the server holds it to"
        );
    }
}
