//! The connector: the server's tools over the Model Context Protocol.
//!
//! A pilot adds this endpoint to their own Claude or ChatGPT, and that
//! assistant can then ask the server for the wind around a place or the
//! airspace over it. The project pays for no inference: the question is
//! answered by the pilot's assistant, on the pilot's account, and only the
//! tool calls come here.
//!
//! This module is a thin layer. The protocol is the `venti-mcp` crate,
//! which reads a request into what is asked and builds the answer; the
//! tools are those of `venti-tools`, the same definitions the app's own
//! assistant has. What is done here is to run a tool: read the answer the
//! tool's request would have got from the API, straight from the store,
//! and hand it to the tool's own function.
//!
//! **The public tools are open and know nobody.** They read what the API
//! serves to anyone: wind stations, airspace, and the community's data
//! (the flying sites of every layer with their history, what public
//! flights say about a site, the places pilots take off from that no site
//! names, and the public flights). They answer the same with a token and
//! without one, and nothing a pilot keeps private is in any of their
//! answers, whoever asks.
//!
//! **Three tools read one pilot's own logbook** ([`venti_tools::TOOLS`],
//! answered in [`super::logbook`]): `logbook_statistics`, `list_flights`
//! and `get_flight`, the private flights too. They are answered only for
//! a token that the authorization server ([`super::oauth`]) gave out for
//! this connector after the pilot agreed, that has not ended or been
//! withdrawn, and that has the scope `flights:read`; they answer for that
//! token's account and no other. Without such a token the request is
//! answered `401` with a `WWW-Authenticate` header that names where the
//! connector says who gives out tokens for it, as the protocol's
//! authorization chapter asks. A token that is not good is refused
//! whatever the request asks for: its sender believes to act for a pilot
//! and should hear that it does not. A token made for the API is no token
//! for the connector, whatever its scopes, and a token is never passed on
//! to anything else. Adding a site over this protocol is not built.
//!
//! **The same reads as the API's.** Each tool is answered by the method
//! the API's handler calls for the same request (`sites_in`,
//! `site_answer`, `site_history`, `site_flights`, `candidates_in`,
//! `public_flights_page`, `public_flight`), never by a second way to the
//! store, so a rule of who may read what holds for both.
//!
//! **A tool call cannot make the server fetch anything.** The tools take
//! places as numbers, identifiers, a handle, days and a cursor. They read
//! the server's own database and nothing else: this module holds no HTTP
//! client, and no argument is ever treated as an address.
//!
//! **What it costs is bounded**: the size of a request, the time for an
//! answer, the number of messages in a batch, and the number of requests
//! from one address. A tool that reads public flights counts, besides, as
//! the API's read of them does, against the same budget of the address
//! ([`Api::count_public_flight_read`]). A request with a token counts
//! against that token's own budget as well, the one a token for a program
//! has at the API ([`super::budget`]).

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use venti_api_types::v1::{FlightSort, Scope};
use venti_domain::SiteId;
use venti_mcp::{Answer, HttpRequest, Implementation, Origins, Read, Server, ToolCall, ToolResult};
use venti_sources::Kind;
use venti_store::Store;
use venti_store::{Near, PublicFlights};
use venti_tools::airspace::{AIRSPACE_TOOLS, AirspaceCall, airspace_at};
use venti_tools::community::{
    COMMUNITY_TOOLS, CandidatesCall, FlightCall, FlightsCall, HistoryCall as SiteHistoryCall,
    SiteCall, SitesCall,
};
use venti_tools::wind::{
    HistoryCall, SERVER_TOOLS, StationsCall, refusal, station_history, wind_stations,
};
use venti_tools::{Tool, ToolError};

use super::auth::{By, Client, Signed, Viewer};
use super::error::ApiError;
use super::{Api, Cors, ServedSource};
use crate::accounts::client::counting_keys;
use crate::accounts::secret::{OauthKind, OauthSecret};
use crate::accounts::throttle::Counted;
use crate::flights::share::PublicUrl;

/// The path of the connector.
pub const MCP_PATH: &str = "/mcp";

/// The largest request the connector reads. A tool call is a few hundred
/// bytes. The newest revision of the protocol lets a client describe
/// itself in every request, icon included, which is what the room is for.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// What the server tells an assistant about itself, beyond what each tool
/// says.
const INSTRUCTIONS: &str = "Venti is a free app for paraglider pilots. This connector answers \
    from public data the Venti server holds: wind measured by weather stations, airspace, \
    flying sites (OpenStreetMap's, those of Venti's partners, and on request the places from \
    pilots, which are not official), and the flights pilots made public. Give places as \
    coordinates. Say how old a reading is, name the source of the data and pass on each \
    source's licence and notice. Airspace answers are advice from data that is not certified \
    and never a clearance: nothing here knows what is active today. A site is not permission \
    to fly there.";

/// What is added where nobody's own data can be asked for.
const KNOWS_NOBODY: &str = " The connector is open to everyone and knows nothing about the \
    person asking.";

/// What is added where a pilot's own logbook can be asked for.
const KNOWS_WHO_AGREED: &str = " Those tools are open to everyone and know nothing about the \
    person asking. Three more read one pilot's own logbook on Venti, the private flights too: \
    logbook_statistics, list_flights and get_flight. They work only after that pilot has \
    signed in on Venti's own page and agreed, and they only read. What they return is that \
    pilot's own: use it to answer the pilot, and do not pass it on to anyone else unasked.";

/// The scope the logbook's tools ask of a token.
const LOGBOOK_SCOPE: Scope = Scope::FlightsRead;

/// Whether a tool reads one pilot's own logbook.
fn reads_a_logbook(name: &str) -> bool {
    venti_tools::TOOLS.iter().any(|tool| tool.name == name)
}

/// The headers of a request that the protocol reads.
const READ_HEADERS: [&str; 6] = [
    "origin",
    "content-type",
    "accept",
    "mcp-protocol-version",
    "mcp-method",
    "mcp-name",
];

/// The headers a page in a browser may send to the connector, beside
/// those the API allows: the protocol's own.
pub(crate) const REQUEST_HEADERS: [HeaderName; 6] = [
    header::ACCEPT,
    HeaderName::from_static("mcp-protocol-version"),
    HeaderName::from_static("mcp-method"),
    HeaderName::from_static("mcp-name"),
    HeaderName::from_static("mcp-session-id"),
    HeaderName::from_static("last-event-id"),
];

/// The protocol's side of the connector, for a server with these sources.
///
/// The wind tools are offered when the server collects observations and
/// the airspace tool when it collects airspace: a tool that could only
/// ever answer "no data" is not offered. The tools of the community's data
/// are always offered. The tools of a pilot's own logbook are offered where
/// the server gives out tokens for them: everywhere but on a server that
/// is reached over plain HTTP from elsewhere. The list is the same for
/// every caller, with a token or without.
///
/// A browser may call the connector from the pages the API may be called
/// from, and from the server's own address. Assistants do not call from a
/// browser and are not affected. The development mode lets any page read
/// its API, which holds test data only; its connector still follows the
/// protocol's rule and takes pages of the developer's own machine only.
pub(crate) fn protocol<'a>(
    sources: impl IntoIterator<Item = &'a ServedSource> + Clone,
    cors: &Cors,
    public_url: &PublicUrl,
) -> Server {
    let serves = |kind: Kind| {
        sources
            .clone()
            .into_iter()
            .any(|source| source.kinds.contains(&kind))
    };
    let mut tools: Vec<&Tool> = Vec::new();
    if serves(Kind::Observations) {
        tools.extend(&SERVER_TOOLS);
    }
    if serves(Kind::Airspace) {
        tools.extend(&AIRSPACE_TOOLS);
    }
    // The community's data is the server's own: Venti's layers of sites
    // and the public flights are there whatever the sources are.
    tools.extend(&COMMUNITY_TOOLS);
    let logbook = !public_url.is_plain_http_to_elsewhere();
    if logbook {
        tools.extend(&venti_tools::TOOLS);
    }
    let instructions = if logbook {
        format!("{INSTRUCTIONS}{KNOWS_WHO_AGREED}")
    } else {
        format!("{INSTRUCTIONS}{KNOWS_NOBODY}")
    };
    let tools = tools
        .into_iter()
        .map(|tool| venti_mcp::Tool {
            name: tool.name.to_owned(),
            description: tool.description.to_owned(),
            input_schema: tool.input_schema(),
            read_only: tool.read_only,
            // They read the server's own database and reach nowhere else.
            open_world: false,
        })
        .collect();
    let origins = match cors {
        Cors::Any => Origins::Local,
        Cors::Origins(origins) => {
            let mut allowed = origins.clone();
            allowed.push(public_url.as_str().to_owned());
            Origins::Only(allowed)
        }
    };
    Server::new(
        Implementation {
            name: "venti".to_owned(),
            title: "Venti".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        instructions,
        tools,
        origins,
    )
}

/// The connector's endpoint, for every method: the protocol decides what
/// each gets.
pub(crate) async fn endpoint<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    request: Request,
) -> Response {
    // Counted before anything is read: a request over the limit costs a
    // look into a table and nothing else.
    let now = api.clock.now();
    let address = counting_keys(client);
    let by_address = address.counted(
        api.limits.connector_requests_per_address,
        api.limits.connector_requests_per_network,
    );
    if let Err(refused) = api.connector_requests.attempt(&by_address, now) {
        // One line for the address and the window, however often it asks
        // again: a line per refusal is a megabyte of log a second from one
        // client that does not stop. A client refused because the table of
        // addresses is full did nothing itself; the table has said so.
        if refused.first && !refused.shared {
            tracing::warn!(
                client = %client,
                "connector: too many requests from this address or its network; \
                 it is refused until its minute is over, without a further line"
            );
        }
        let seconds = u64::try_from(refused.retry_after.num_seconds())
            .unwrap_or(0)
            .max(1);
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many requests from this address; try again shortly",
            Some(seconds),
        );
    }

    let (parts, body) = request.into_parts();
    // Who asks, where the request says so. A token that is not good is
    // refused before anything else is read.
    let caller = match api.connector_caller(&parts.headers).await {
        Ok(caller) => caller,
        Err(refusal) => return refusal,
    };
    // The size is bounded by the layer around this handler; a body beyond
    // it never gets here whole.
    let Ok(body) = axum::body::to_bytes(body, MAX_BODY_BYTES).await else {
        return refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Payload too large: the request is larger than this server reads",
            None,
        );
    };
    let headers = read_headers(&parts.headers);
    let request = HttpRequest {
        method: parts.method.as_str(),
        headers: &headers,
        body: &body,
    };

    let answer = match api.mcp.read(&request) {
        Read::Answer(answer) => answer,
        Read::Calls(calls) => {
            // A pilot's own logbook is read for a token that was given
            // out for it and for nobody else. A request that asks for it
            // without one is told where a token is got; a batch that asks
            // for it among other things is refused whole.
            if calls.calls().iter().any(|call| reads_a_logbook(&call.name)) {
                match &caller {
                    None => return api.ask_for_a_token(None),
                    Some(signed) if !signed.may(LOGBOOK_SCOPE) => {
                        return api.ask_for_the_scope();
                    }
                    Some(_) => {}
                }
            }
            // A batch is one request with several calls in it; each call
            // after the first counts as a request of its own.
            let further = u32::try_from(calls.calls().len().saturating_sub(1)).unwrap_or(u32::MAX);
            api.connector_requests.count(&by_address, further, now);
            let mut results = Vec::with_capacity(calls.calls().len());
            for call in calls.calls() {
                results.push(api.run_tool(call, client, caller.as_ref()).await);
            }
            calls.finish(results)
        }
    };
    let mut response = respond(answer);
    if caller.is_some() {
        made_for_one_account(&mut response);
    }
    response
}

/// Marks an answer to a request with a token as made for that token's
/// account: nobody may keep it, and nothing may hand it to a request with
/// another token or with none.
fn made_for_one_account(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.append(header::VARY, HeaderValue::from_static("authorization"));
}

impl<S: Store> Api<S> {
    /// Whoever a request to the connector acts for: `Ok(None)` for a
    /// request without an `Authorization` header, which is most of them.
    ///
    /// With one, the request acts for the account of an access token that
    /// the authorization server gave out for this connector, and for
    /// nothing else. Everything else in that header is refused, as the
    /// protocol's chapter asks ("Invalid or expired tokens MUST receive
    /// an HTTP 401 response"): a token that has ended or was withdrawn, a
    /// refresh token, a session's token, a token a pilot made by hand for
    /// the API, whatever its scopes, and a token that was given out for
    /// another address.
    async fn connector_caller(&self, headers: &HeaderMap) -> Result<Option<Signed>, Response> {
        let mut values = headers.get_all(header::AUTHORIZATION).iter();
        let Some(value) = values.next() else {
            return Ok(None);
        };
        let not_good = || {
            self.ask_for_a_token(Some(
                "the token is not one that was given out for this connector, or it has \
                 ended or was withdrawn",
            ))
        };
        if values.next().is_some() || !self.offers_authorization() {
            return Err(not_good());
        }
        let secret = value
            .to_str()
            .ok()
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .and_then(|(_, token)| OauthSecret::parse(OauthKind::Access, token.trim()));
        let Some(secret) = secret else {
            return Err(not_good());
        };
        let now = self.clock.now();
        let used = match self.store.use_access_token(&secret.hash(), now).await {
            Ok(used) => used,
            Err(error) => {
                let error = ApiError::from(error);
                return Err(refuse(
                    error.status(),
                    "The server cannot look the token up right now; try again shortly",
                    Some(30),
                ));
            }
        };
        let Some((token, account)) = used else {
            return Err(not_good());
        };
        // Given out for this connector, and for no other address.
        let connector = self.connector_resource();
        if token
            .client
            .as_ref()
            .is_none_or(|bound| bound.resource != connector)
        {
            return Err(not_good());
        }
        // The token's own budget, the one it would have at the API: it is
        // kept by the token's identifier, which a refresh does not change.
        let by_token = [Counted::new(
            &token.id,
            self.limits.token_requests_per_window,
        )];
        if let Err(refused) = self.token_requests.attempt(&by_token, now) {
            let seconds = u64::try_from(refused.retry_after.num_seconds())
                .unwrap_or(0)
                .max(1);
            return Err(refuse(
                StatusCode::TOO_MANY_REQUESTS,
                "This token has made as many requests as one token may in an hour",
                Some(seconds),
            ));
        }
        Ok(Some(Signed {
            account,
            by: By::Token(token),
        }))
    }

    /// `401`: the request needs a token for this connector and has none,
    /// or (`why`) has one that is not good. The header names the document
    /// that says who gives tokens out for the connector, and the scope to
    /// ask for.
    fn ask_for_a_token(&self, why: Option<&str>) -> Response {
        let scope = LOGBOOK_SCOPE.as_str();
        let mut challenge = String::from("Bearer ");
        if let Some(why) = why {
            challenge.push_str(&format!(
                "error=\"invalid_token\", error_description=\"{why}\", "
            ));
        }
        let message = why.unwrap_or(
            "This tool reads one pilot's own logbook: it needs that pilot's agreement, \
             which the authorization server named in the WWW-Authenticate header gives",
        );
        let mut response = refuse(StatusCode::UNAUTHORIZED, message, None);
        if self.offers_authorization() {
            challenge.push_str(&format!(
                "resource_metadata=\"{}\", scope=\"{scope}\"",
                self.resource_metadata_url()
            ));
        } else {
            challenge.push_str("realm=\"Venti\"");
        }
        if let Ok(challenge) = HeaderValue::from_str(&challenge) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, challenge);
        }
        response
    }

    /// `403`: the token is good and lacks the scope the tool needs.
    fn ask_for_the_scope(&self) -> Response {
        let scope = LOGBOOK_SCOPE.as_str();
        let challenge = format!(
            "Bearer error=\"insufficient_scope\", scope=\"{scope}\", \
             resource_metadata=\"{}\"",
            self.resource_metadata_url()
        );
        let mut response = refuse(
            StatusCode::FORBIDDEN,
            "The token does not allow reading the pilot's flights",
            None,
        );
        if let Ok(challenge) = HeaderValue::from_str(&challenge) {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, challenge);
        }
        made_for_one_account(&mut response);
        response
    }
}

/// The headers the protocol reads, as it wants them. A value that is not
/// text is left out: the protocol then treats the header as missing.
fn read_headers(headers: &HeaderMap) -> Vec<(&'static str, &str)> {
    READ_HEADERS
        .into_iter()
        .filter_map(|name| Some((name, headers.get(name)?.to_str().ok()?)))
        .collect()
}

fn respond(answer: Answer) -> Response {
    let status = StatusCode::from_u16(answer.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, answer.body).into_response();
    for (name, value) in answer.headers {
        if let Ok(value) = HeaderValue::from_str(&value) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(name), value);
        }
    }
    response
}

/// A refusal the server makes before the protocol is asked: too many
/// requests, too large a request, no answer in time. It has the shape of
/// the protocol's own refusals: a JSON-RPC error without an `id`, whose
/// code is the HTTP status.
fn refuse(status: StatusCode, message: &str, retry_after: Option<u64>) -> Response {
    let body = json!({
        "jsonrpc": "2.0",
        "error": {"code": status.as_u16(), "message": message},
    });
    let mut response = (status, axum::Json(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(seconds) = retry_after {
        headers.insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    }
    response
}

/// Gives up on a request to the connector that takes longer than `limit`.
pub(crate) async fn deadline(limit: Duration, request: Request, next: Next) -> Response {
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_) => refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "The server could not answer in time; try again shortly",
            Some(30),
        ),
    }
}

/// Gives every refusal on the connector's path the protocol's shape.
///
/// The handler answers that way already. This catches what the layers
/// around it answer on their own, in plain text: a body that is too large.
pub(crate) async fn rpc_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let status = response.status();
    if is_json || !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    let reason = status.canonical_reason().unwrap_or("The request failed");
    refuse(status, reason, None)
}

impl<S: Store> Api<S> {
    /// Runs one tool and returns what the model reads.
    ///
    /// The result is the text the app's own assistant gets for the same
    /// call: the arguments are read by the tool's own code, the answer is
    /// built by the function that serves the API's endpoint, and the
    /// tool's own function turns it into the result. A call that cannot be
    /// answered is a result too, one that says why.
    async fn run_tool(
        &self,
        call: &ToolCall,
        client: IpAddr,
        caller: Option<&Signed>,
    ) -> ToolResult {
        tracing::debug!(tool = %call.name, "connector: tool call");
        let input = Value::Object(call.arguments.clone());
        match self.tool_result(&call.name, &input, client, caller).await {
            Ok(text) => ToolResult {
                text,
                is_error: false,
            },
            Err(error) => ToolResult {
                text: venti_tools::failure(&error),
                is_error: true,
            },
        }
    }

    async fn tool_result(
        &self,
        name: &str,
        input: &Value,
        client: IpAddr,
        caller: Option<&Signed>,
    ) -> Result<String, ToolError> {
        let now = self.clock.now();
        match name {
            // A pilot's own logbook, for the account the token acts for
            // and no other: no argument names an account. The endpoint
            // has checked the token; a call that got here without one is
            // answered as a call that cannot be answered.
            name if reads_a_logbook(name) => {
                let signed = caller
                    .filter(|signed| signed.may(LOGBOOK_SCOPE))
                    .ok_or_else(|| refusal(401, None))?;
                let owner = Viewer::allowed(signed);
                let logbook = self
                    .logbook_for(&signed.account, &owner, name, input)
                    .await
                    .map_err(refused)?;
                venti_tools::run(&logbook, name, input)
            }
            "wind_stations" => {
                let call = StationsCall::read(input)?;
                let answer = self
                    .stations_in(&call.request().bbox)
                    .await
                    .map_err(refused)?;
                Ok(wind_stations(&answer, call.place, call.radius_km, now).to_string())
            }
            "station_history" => {
                let call = HistoryCall::read(input)?;
                let answer = self
                    .observations_of(&call.station, None, None)
                    .await
                    .map_err(refused)?;
                Ok(station_history(&answer, call.hours, now).to_string())
            }
            "foehn" => {
                let answer = self.foehn().await.map_err(refused)?;
                Ok(venti_tools::wind::foehn(&answer).to_string())
            }
            "airspace_at" => {
                let call = AirspaceCall::read(input)?;
                let answer = self
                    .airspace_in(&call.request().bbox)
                    .await
                    .map_err(refused)?;
                Ok(airspace_at(&answer, call.place, call.radius_km, now).to_string())
            }
            other => self.community_result(other, input, client).await,
        }
    }

    /// The tools of the community's data. Each reads through the method
    /// the API's handler for the same request calls, and a tool that
    /// reads public flights counts as that request does.
    async fn community_result(
        &self,
        name: &str,
        input: &Value,
        client: IpAddr,
    ) -> Result<String, ToolError> {
        use venti_tools::community as tools;
        let now = self.clock.now();
        let site_id = |text: &str| -> Result<SiteId, ToolError> {
            text.parse()
                .map_err(|_| tools::refusal(404, Some("site_not_found")))
        };
        match name {
            "sites_in_box" => {
                let call = SitesCall::read(input)?;
                let answer = self
                    .sites_in(&call.bbox, call.layers().as_deref())
                    .await
                    .map_err(community_refused)?;
                Ok(tools::sites_in_box(&answer, &call, now).to_string())
            }
            "site" => {
                let call = SiteCall::read(input)?;
                let answer = self
                    .site_answer(&site_id(&call.site_id)?)
                    .await
                    .map_err(community_refused)?;
                Ok(tools::site(&answer).to_string())
            }
            "site_history" => {
                let call = SiteHistoryCall::read(input)?;
                let answer = self
                    .site_history(&site_id(&call.site_id)?, call.after, call.limit)
                    .await
                    .map_err(community_refused)?;
                Ok(tools::site_history(&answer).to_string())
            }
            "site_flights" => {
                let call = SiteCall::read(input)?;
                let id = site_id(&call.site_id)?;
                self.count_public_flight_read(client)
                    .map_err(community_refused)?;
                let site = self
                    .site_of_any_layer(&id)
                    .await
                    .map_err(community_refused)?;
                let answer = self
                    .site_flights(id, &site)
                    .await
                    .map_err(community_refused)?;
                Ok(tools::site_flights(&answer).to_string())
            }
            "takeoff_candidates" => {
                let call = CandidatesCall::read(input)?;
                self.count_public_flight_read(client)
                    .map_err(community_refused)?;
                let answer = self
                    .candidates_in(&call.bbox)
                    .await
                    .map_err(community_refused)?;
                Ok(tools::takeoff_candidates(&answer).to_string())
            }
            "public_flights" => {
                let call = FlightsCall::read(input)?;
                self.count_public_flight_read(client)
                    .map_err(community_refused)?;
                let which = PublicFlights {
                    pilot: self
                        .pilot_asked_for(call.pilot.as_deref())
                        .await
                        .map_err(community_refused)?,
                    area: call.bbox,
                    from: call.from,
                    before: call.to,
                    near: call.near.map(|circle| Near {
                        place: circle.centre,
                        within_m: circle.radius_m(),
                    }),
                    ..PublicFlights::default()
                };
                let sort = if call.best {
                    FlightSort::Score
                } else {
                    FlightSort::Newest
                };
                let answer = self
                    .public_flights_page(&which, sort, call.after.as_deref(), call.limit)
                    .await
                    .map_err(community_refused)?;
                Ok(tools::public_flights(&answer, now).to_string())
            }
            "public_flight" => {
                let call = FlightCall::read(input)?;
                let answer = self
                    .public_flight(&call.flight_id)
                    .await
                    .map_err(community_refused)?;
                Ok(tools::public_flight(&answer, now).to_string())
            }
            other => Err(ToolError::UnknownTool(other.to_owned())),
        }
    }
}

/// What the tool tells the model when the API could not answer: the words
/// the app's assistant reads for the same error.
fn refused(error: ApiError) -> ToolError {
    let code = serde_json::to_value(error.code()).ok();
    let code = code.as_ref().and_then(Value::as_str);
    refusal(error.status().as_u16(), code)
}

/// The same for the tools of the community's data, in their words.
fn community_refused(error: ApiError) -> ToolError {
    let code = serde_json::to_value(error.code()).ok();
    let code = code.as_ref().and_then(Value::as_str);
    venti_tools::community::refusal(error.status().as_u16(), code)
}
