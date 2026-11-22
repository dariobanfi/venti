//! The authorization server: how a pilot's own assistant comes to read
//! the pilot's logbook over the connector without the pilot copying a
//! secret (OAuth 2.1 with PKCE, as the Model Context Protocol's
//! authorization chapter asks for it).
//!
//! The server is two things here. It is the **resource**: the connector
//! at `/mcp`, which says in `/.well-known/oauth-protected-resource` who
//! gives out tokens for it. And it is that **authorization server**, with
//! its own description at `/.well-known/oauth-authorization-server`:
//!
//! - `POST /oauth/register`: a program registers itself (RFC 7591). It
//!   gets an identifier and no secret: every program is a public client.
//! - `GET /oauth/authorize`: the page on which the pilot signs in, sees
//!   which program asks and what it may read, and agrees or refuses;
//!   `POST` is the pilot's answer.
//! - `POST /oauth/token`: the program exchanges the code for an access
//!   token and a refresh token, and later a refresh token for new ones.
//! - `POST /oauth/revoke`: the program gives its tokens up (RFC 7009).
//!
//! **What is given out reads and never writes** ([`SCOPES`]), is bound to
//! the program and to the connector's address, and is listed among the
//! pilot's tokens, where the pilot withdraws it like any other.
//!
//! **The server fetches nothing here.** A program is known by what it
//! registered, never by an address the server would have to look up.
//!
//! **Nobody is sent anywhere without the pilot's own click.** Every
//! program registers itself, so no redirect address is one the server
//! trusts. A request that cannot be agreed to is explained on a page; the
//! way back to the program is a link on it. The pilot's browser is
//! redirected only as the answer to the pilot pressing a button.
//!
//! **The page is protected as a page that takes a password must be.** It
//! is shown in no frame, runs no script and loads nothing. Its form is
//! taken only with a value that the same browser was given in a cookie
//! when it loaded the page: the one cookie this server sets. It names no
//! account, lives a quarter of an hour, and is read by this page alone.
//! The password is checked with the limits the sign-in has.
//!
//! **Nothing of a request reaches the log**: no code, no token, no
//! verifier, no `state`, no name a program gave itself.

use std::net::IpAddr;
use std::sync::{Arc, LazyLock};

use axum::Form;
use axum::body::Bytes;
use axum::extract::rejection::{FormRejection, QueryRejection};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use venti_api_types::v1::{Handle, Password, Scope, parse_token_name};
use venti_store::{
    AccessToken, AuthorizationCode, NewGrant, OauthClient, Redeemed, Refreshed, Rotation, Store,
    TokenClient,
};

use super::auth::Client;
use super::error::ApiError;
use super::mcp::MCP_PATH;
use super::{Api, cache};
use crate::accounts::client::counting_keys;
use crate::accounts::oauth::{
    ACCESS_LIFE, CODE_LIFE, MAX_REDIRECT_URIS, MAX_STATE_BYTES, REFRESH_LIFE, RedirectUri, SCOPES,
    ScopeRefusal, answers, is_challenge, is_resource, same, scope_text, scopes,
};
use crate::accounts::secret::{OauthKind, OauthSecret, is_id, random_id};
use crate::accounts::throttle::Refused;
use crate::flights::share::{self, Esc};

/// Where the connector says who gives out tokens for it (RFC 9728), at
/// the root and, as that document asks for a resource with a path, with
/// the connector's path after it.
pub const PROTECTED_RESOURCE_PATH: &str = "/.well-known/oauth-protected-resource";
/// The same document under the connector's own path.
pub const PROTECTED_RESOURCE_MCP_PATH: &str = "/.well-known/oauth-protected-resource/mcp";
/// Where the authorization server describes itself (RFC 8414).
pub const AUTHORIZATION_SERVER_PATH: &str = "/.well-known/oauth-authorization-server";
/// Where a program registers itself.
pub const REGISTER_PATH: &str = "/oauth/register";
/// The page on which a pilot agrees or refuses.
pub const AUTHORIZE_PATH: &str = "/oauth/authorize";
/// Where a program exchanges a code or a refresh token.
pub const TOKEN_PATH: &str = "/oauth/token";
/// Where a program gives its tokens up.
pub const REVOKE_PATH: &str = "/oauth/revoke";

const HTML: &str = "text/html; charset=utf-8";

/// How long the page's cookie lives: the time a pilot has to answer.
const CONSENT_SECONDS: u32 = 900;

/// What a program is called that gave itself no name.
const UNNAMED: &str = "A program without a name";

impl<S: Store> Api<S> {
    /// The connector's address, which is what tokens are given out for
    /// and the only place they are taken.
    pub(crate) fn connector_resource(&self) -> String {
        format!("{}{MCP_PATH}", self.public_url.as_str())
    }

    /// The address of the document that says who gives out tokens for the
    /// connector, as the connector names it when it asks for one.
    pub(crate) fn resource_metadata_url(&self) -> String {
        format!("{}{PROTECTED_RESOURCE_MCP_PATH}", self.public_url.as_str())
    }

    /// Whether this server gives out tokens at all. One that is reached
    /// over plain HTTP from elsewhere does not: a password and every
    /// token would travel in the clear.
    pub(crate) fn offers_authorization(&self) -> bool {
        !self.public_url.is_plain_http_to_elsewhere()
    }

    fn at(&self, path: &str) -> String {
        format!("{}{path}", self.public_url.as_str())
    }

    /// Counts a request to the authorization server for its address.
    fn count_oauth_request(&self, client: IpAddr) -> Result<(), Refused> {
        let address = counting_keys(client);
        let by_address = address.counted(
            self.limits.oauth_requests_per_address,
            self.limits.oauth_requests_per_network,
        );
        self.oauth_requests.attempt(&by_address, self.clock.now())
    }

    /// The cookie's name. Over HTTPS it has the prefix that makes a
    /// browser take it only from this very host, over HTTPS, for every
    /// path: nothing else can plant one.
    fn consent_cookie(&self) -> &'static str {
        if self.public_url.as_str().starts_with("https://") {
            "__Host-venti-consent"
        } else {
            "venti-consent"
        }
    }

    fn set_consent_cookie(&self, response: &mut Response, value: &str, seconds: u32) {
        let secure = if self.public_url.as_str().starts_with("https://") {
            "; Secure"
        } else {
            ""
        };
        let cookie = format!(
            "{}={value}; Path=/; Max-Age={seconds}; HttpOnly; SameSite=Strict{secure}",
            self.consent_cookie()
        );
        if let Ok(cookie) = HeaderValue::from_str(&cookie) {
            response.headers_mut().insert(header::SET_COOKIE, cookie);
        }
    }

    /// The value of the page's cookie in a request, if it carries one of
    /// the form the server gives out.
    fn consent_cookie_of<'a>(&self, headers: &'a HeaderMap) -> Option<&'a str> {
        let name = self.consent_cookie();
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(cookie, _)| *cookie == name)
            .map(|(_, value)| value)
            .filter(|value| is_id(value))
    }
}

// --- What the server says of itself --------------------------------------

fn metadata(value: &Value) -> Response {
    let mut response = (StatusCode::OK, axum::Json(value)).into_response();
    // The same for everyone, and it changes only with the server.
    cache::set(&mut response, header::CACHE_CONTROL, "public, max-age=3600");
    response
}

fn not_offered() -> Response {
    oauth_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "this server is not reached over HTTPS and gives out no tokens",
    )
}

/// `GET /.well-known/oauth-protected-resource` and the same under the
/// connector's path: what the connector is and who gives out tokens for
/// it (RFC 9728).
pub(crate) async fn protected_resource<S: Store>(State(api): State<Arc<Api<S>>>) -> Response {
    if !api.offers_authorization() {
        return not_offered();
    }
    metadata(&json!({
        "resource": api.connector_resource(),
        "authorization_servers": [api.public_url.as_str()],
        "scopes_supported": [scope_text(&SCOPES)],
        "bearer_methods_supported": ["header"],
        "resource_name": "Venti",
    }))
}

/// `GET /.well-known/oauth-authorization-server`: where the authorization
/// server's endpoints are and what it can do (RFC 8414).
pub(crate) async fn authorization_server<S: Store>(State(api): State<Arc<Api<S>>>) -> Response {
    if !api.offers_authorization() {
        return not_offered();
    }
    let scopes: Vec<&str> = SCOPES.iter().map(|scope| scope.as_str()).collect();
    metadata(&json!({
        "issuer": api.public_url.as_str(),
        "authorization_endpoint": api.at(AUTHORIZE_PATH),
        "token_endpoint": api.at(TOKEN_PATH),
        "registration_endpoint": api.at(REGISTER_PATH),
        "revocation_endpoint": api.at(REVOKE_PATH),
        "scopes_supported": scopes,
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"],
        "authorization_response_iss_parameter_supported": true,
        // A program is known by what it registered. The server fetches
        // no address that a program names.
        "client_id_metadata_document_supported": false,
    }))
}

// --- Answers to programs ---------------------------------------------------

/// An error as OAuth's endpoints for programs write it: a code and a
/// sentence, never anything the request held.
fn oauth_error(status: StatusCode, code: &str, description: &str) -> Response {
    let body = json!({"error": code, "error_description": description});
    let mut response = (status, axum::Json(body)).into_response();
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    cache::set(&mut response, header::PRAGMA, "no-cache");
    response
}

fn invalid_request(description: &str) -> Response {
    oauth_error(StatusCode::BAD_REQUEST, "invalid_request", description)
}

fn invalid_grant() -> Response {
    // One answer for a code or a refresh token that is unknown, used,
    // ended, another program's, or met with the wrong verifier: which it
    // was is nothing its sender should learn.
    oauth_error(
        StatusCode::BAD_REQUEST,
        "invalid_grant",
        "the code or the refresh token is not good: it is unknown, was used, has ended, \
         belongs to another program, or the verifier does not answer its challenge",
    )
}

fn invalid_client() -> Response {
    oauth_error(
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        "no program is registered under this client_id; programs here are public clients \
         that send their client_id and no secret",
    )
}

fn invalid_target() -> Response {
    oauth_error(
        StatusCode::BAD_REQUEST,
        "invalid_target",
        "resource must be this server's connector, as its protected resource metadata \
         names it",
    )
}

fn too_many(refused: &Refused) -> Response {
    let seconds = u64::try_from(refused.retry_after.num_seconds())
        .unwrap_or(0)
        .max(1);
    let mut response = oauth_error(
        StatusCode::TOO_MANY_REQUESTS,
        "too_many_requests",
        "too many requests from this address; try again shortly",
    );
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    response
}

fn is_form(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|kind| {
            kind.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
}

// --- Registration ----------------------------------------------------------

/// What a program says of itself when it registers (RFC 7591). What is
/// not named here is not read: an address of a logo or of terms is
/// neither kept nor ever fetched.
#[derive(Deserialize)]
struct Registration {
    redirect_uris: Option<Vec<String>>,
    client_name: Option<String>,
    token_endpoint_auth_method: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
    scope: Option<String>,
}

fn invalid_metadata(description: &str) -> Response {
    oauth_error(
        StatusCode::BAD_REQUEST,
        "invalid_client_metadata",
        description,
    )
}

/// `POST /oauth/register`: a program registers itself and gets its
/// identifier.
pub(crate) async fn register<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if !api.offers_authorization() {
        return Ok(not_offered());
    }
    // A body that does not say it is JSON is what a page elsewhere can
    // make a visitor's browser send: no attempt, and not counted as one.
    if !is_json(&headers) {
        return Ok(invalid_metadata(
            "send the registration as application/json",
        ));
    }
    let now = api.clock.now();
    let address = counting_keys(client);
    let by_address = address.counted(
        api.limits.client_registrations_per_address,
        api.limits.client_registrations_per_network,
    );
    if let Err(refused) = api.client_registrations.attempt(&by_address, now) {
        return Ok(too_many(&refused));
    }
    let Ok(asked) = serde_json::from_slice::<Registration>(&body) else {
        return Ok(invalid_metadata(
            "the registration is not the JSON object RFC 7591 describes",
        ));
    };

    let redirect_uris = asked.redirect_uris.unwrap_or_default();
    if redirect_uris.is_empty() || redirect_uris.len() > MAX_REDIRECT_URIS {
        return Ok(oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uris must name one to five addresses",
        ));
    }
    for uri in &redirect_uris {
        if let Err(why) = RedirectUri::parse(uri) {
            return Ok(oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_redirect_uri",
                why,
            ));
        }
    }
    if asked
        .token_endpoint_auth_method
        .is_some_and(|method| method != "none")
    {
        return Ok(invalid_metadata(
            "token_endpoint_auth_method must be none: a program gets no secret here",
        ));
    }
    let known = |given: Option<Vec<String>>, allowed: &[&str]| {
        given.is_none_or(|given| given.iter().all(|one| allowed.contains(&one.as_str())))
    };
    if !known(asked.grant_types, &["authorization_code", "refresh_token"]) {
        return Ok(invalid_metadata(
            "grant_types may name authorization_code and refresh_token",
        ));
    }
    if !known(asked.response_types, &["code"]) {
        return Ok(invalid_metadata("response_types may name code"));
    }
    if scopes(asked.scope.as_deref()).is_err() {
        return Ok(invalid_metadata(
            "scope may name flights:read; nothing that writes is given out",
        ));
    }
    let name = match asked.client_name.as_deref().map(str::trim) {
        None | Some("") => UNNAMED.to_owned(),
        // What a pilot's own name for a token may be, a program's name
        // for itself may be: one line, sixty characters, nothing in it
        // that does not show.
        Some(name) => match parse_token_name(name) {
            Ok(name) => name,
            Err(_) => {
                return Ok(invalid_metadata(
                    "client_name must be one line of at most 60 characters that all show",
                ));
            }
        },
    };

    let registered = OauthClient {
        id: random_id()?,
        name,
        redirect_uris,
        created_at: now,
        last_used_at: None,
    };
    if !api
        .store
        .create_oauth_client(&registered, api.limits.oauth_clients)
        .await?
    {
        return Ok(oauth_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "temporarily_unavailable",
            "the server holds as many registered programs as it keeps; try again later",
        ));
    }
    let body = json!({
        "client_id": registered.id,
        "client_id_issued_at": now.timestamp(),
        "client_name": registered.name,
        "redirect_uris": registered.redirect_uris,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "scope": scope_text(&SCOPES),
    });
    let mut response = (StatusCode::CREATED, axum::Json(body)).into_response();
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    Ok(response)
}

// --- The page on which a pilot agrees --------------------------------------

/// What a program asks the pilot's browser to bring along.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct AuthorizeParams {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    resource: Option<String>,
}

/// The pilot's answer: the request again, as the page carried it, and
/// what the pilot typed and pressed.
#[derive(Deserialize)]
pub(crate) struct Answered {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    resource: Option<String>,
    csrf: Option<String>,
    handle: Option<String>,
    password: Option<String>,
    decision: Option<String>,
}

impl Answered {
    fn split(self) -> (AuthorizeParams, PilotsAnswer) {
        let params = AuthorizeParams {
            response_type: self.response_type,
            client_id: self.client_id,
            redirect_uri: self.redirect_uri,
            scope: self.scope,
            state: self.state,
            code_challenge: self.code_challenge,
            code_challenge_method: self.code_challenge_method,
            resource: self.resource,
        };
        let answer = PilotsAnswer {
            csrf: self.csrf,
            handle: self.handle,
            password: self.password,
            decision: self.decision,
        };
        (params, answer)
    }
}

struct PilotsAnswer {
    csrf: Option<String>,
    handle: Option<String>,
    password: Option<String>,
    decision: Option<String>,
}

/// A request that a pilot can agree to: every part of it checked.
struct Asked {
    client: OauthClient,
    /// The address the pilot is sent back to: one the program registered.
    redirect_uri: String,
    redirect_origin: String,
    /// Whether that address is a program on the pilot's own device.
    loopback: bool,
    state: Option<String>,
    scopes: Vec<Scope>,
    challenge: String,
}

/// Why a request cannot be agreed to.
enum Unusable {
    /// Nothing is known that the pilot could be sent back to: the pilot
    /// is told, and nobody else.
    Stop(StatusCode, &'static str),
    /// The program asked for something it cannot have. The pilot is told,
    /// and the page offers the way back, which tells the program.
    Back {
        redirect_uri: String,
        host: String,
        state: Option<String>,
        error: &'static str,
        description: &'static str,
    },
}

impl<S: Store> Api<S> {
    /// Checks a request for the pilot's agreement, in the order OAuth 2.1
    /// asks for (section 4.1.2.1): first the program and the address the
    /// pilot would be sent back to, and only once both are good anything
    /// that is answered by way of that address.
    async fn asked(&self, params: &AuthorizeParams) -> Result<Result<Asked, Unusable>, ApiError> {
        let stop = |message| Ok(Err(Unusable::Stop(StatusCode::BAD_REQUEST, message)));
        let client = match params.client_id.as_deref().filter(|id| is_id(id)) {
            Some(id) => self.store.oauth_client(id).await?,
            None => None,
        };
        let Some(client) = client else {
            return stop(
                "The program that sent you here is not registered with Venti, or no longer \
                 is. Go back to the program and connect it to Venti again.",
            );
        };
        let redirect = params
            .redirect_uri
            .as_deref()
            .and_then(|uri| RedirectUri::parse(uri).ok())
            .filter(|uri| {
                client
                    .redirect_uris
                    .iter()
                    .any(|registered| uri.is_allowed_by(registered))
            });
        let Some(redirect) = redirect else {
            return stop(
                "The program that sent you here wants you sent back to an address it did not \
                 register with Venti. Nothing was agreed to. If you did not start this \
                 yourself, close this page.",
            );
        };

        let state = params.state.clone();
        let state_fits = state
            .as_deref()
            .is_none_or(|state| state.len() <= MAX_STATE_BYTES);
        let back = |error, description| {
            Ok(Err(Unusable::Back {
                redirect_uri: redirect.as_str().to_owned(),
                host: redirect.host().to_owned(),
                // A state that is too long is not carried back either.
                state: state.clone().filter(|_| state_fits),
                error,
                description,
            }))
        };
        if !state_fits {
            return back("invalid_request", "state is longer than 512 bytes");
        }
        if params.response_type.as_deref() != Some("code") {
            return back("unsupported_response_type", "response_type must be code");
        }
        if params.code_challenge_method.as_deref() != Some("S256") {
            return back("invalid_request", "code_challenge_method must be S256");
        }
        let Some(challenge) = params.code_challenge.clone().filter(|c| is_challenge(c)) else {
            return back(
                "invalid_request",
                "code_challenge must be the S256 challenge of RFC 7636",
            );
        };
        let resource = self.connector_resource();
        if !params
            .resource
            .as_deref()
            .is_some_and(|asked| is_resource(asked, &resource))
        {
            return back(
                "invalid_target",
                "resource must be this server's connector, as its protected resource \
                 metadata names it",
            );
        }
        let scopes = match scopes(params.scope.as_deref()) {
            Ok(scopes) => scopes,
            Err(ScopeRefusal::Writes) => {
                return back(
                    "invalid_scope",
                    "no scope that writes is given out this way; ask for flights:read",
                );
            }
            Err(ScopeRefusal::NotOffered) => {
                return back(
                    "invalid_scope",
                    "the scope that can be asked for is flights:read",
                );
            }
        };
        Ok(Ok(Asked {
            redirect_uri: redirect.as_str().to_owned(),
            redirect_origin: redirect.origin(),
            loopback: redirect.is_loopback(),
            client,
            state,
            scopes,
            challenge,
        }))
    }

    /// The address the pilot's browser goes back to, with `answer` and
    /// what OAuth adds to every answer: the program's `state`, and who
    /// answers (`iss`, RFC 9207).
    fn way_back(&self, redirect_uri: &str, state: Option<&str>, answer: &[(&str, &str)]) -> String {
        let mut url = redirect_uri.to_owned();
        let mut separator = if redirect_uri.contains('?') { '&' } else { '?' };
        let state = state.map(|state| ("state", state));
        let iss = ("iss", self.public_url.as_str());
        for (name, value) in answer.iter().copied().chain(state).chain([iss]) {
            url.push(separator);
            url.push_str(name);
            url.push('=');
            url.push_str(&percent_encoded(value));
            separator = '&';
        }
        url
    }
}

/// Text as a value in a query: everything but letters, digits and
/// `-._~` is escaped.
fn percent_encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The page's style: the share pages' own, and what a form needs.
static STYLE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}\
.asks {{ margin: 0 0 1.5rem; border-top: 1px solid var(--line); }}
.asks div {{ padding: 0.625rem 0; border-bottom: 1px solid var(--line); }}
.asks dt {{ color: var(--quiet); font-size: 0.875rem; }}
.asks dd {{ margin: 0; overflow-wrap: anywhere; }}
.alert {{ margin: 0 0 1.5rem; padding: 0.75rem 1rem; border-radius: 0.75rem; background: var(--tint); }}
label {{ display: block; margin: 0 0 1rem; color: var(--quiet); font-size: 0.875rem; }}
input {{ display: block; width: 100%; margin-top: 0.25rem; padding: 0.625rem 0.75rem;
  border: 1px solid var(--line); border-radius: 0.75rem; background: Canvas; color: CanvasText;
  font: inherit; }}
button {{ font: inherit; cursor: pointer; }}
button.button {{ border: 0; margin: 0 0.75rem 0.75rem 0; }}
button.plain {{ padding: 0.75rem 1.25rem; border-radius: 0.75rem; border: 1px solid var(--line);
  background: Canvas; color: CanvasText; font-weight: 600; }}
input:focus-visible, button:focus-visible {{ outline: 3px solid CanvasText; outline-offset: 3px; }}
",
        share::STYLE
    )
});

/// What every page here carries: it is shown in no frame, loads and runs
/// nothing but its own style, is kept by nobody and listed nowhere, and
/// its address travels to no other site.
fn dress(mut response: Response, form_goes_to: Option<&str>) -> Response {
    cache::set(&mut response, header::CONTENT_TYPE, HTML);
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    cache::set(&mut response, header::X_FRAME_OPTIONS, "DENY");
    cache::set(&mut response, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // Not `no-referrer`: with that a browser sends `Origin: null` with
    // the page's own form, and the origin is one of the two things the
    // form is held to.
    cache::set(&mut response, header::REFERRER_POLICY, "same-origin");
    cache::set(
        &mut response,
        HeaderName::from_static("x-robots-tag"),
        "noindex",
    );
    let policy = share::policy_for(&STYLE);
    // A browser holds a form to this list also when the answer to it is a
    // redirect: the form goes to this server, and from there to the
    // address the program registered.
    let policy = match form_goes_to {
        Some(origin) => policy.replace(
            "form-action 'none'",
            &format!("form-action 'self' {origin}"),
        ),
        None => policy,
    };
    if let Ok(policy) = HeaderValue::from_str(&policy) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>{} · Venti</title>\n\
         <style>{}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">Venti</a></header>\n<main>\n\
         {body}\
         </main>\n</body>\n</html>\n",
        Esc(title),
        STYLE.as_str()
    )
}

/// A page that says why nothing was agreed to. `message` is the server's
/// own words: nothing a request held is ever shown here.
fn stop_page(status: StatusCode, message: &str) -> Response {
    let body = format!(
        "<h1>Nothing was agreed to</h1>\n<p class=\"byline\">{}</p>\n",
        Esc(message)
    );
    dress(
        (status, page("Nothing was agreed to", &body)).into_response(),
        None,
    )
}

fn busy_page(refused: &Refused) -> Response {
    let seconds = u64::try_from(refused.retry_after.num_seconds())
        .unwrap_or(0)
        .max(1);
    let mut response = stop_page(
        StatusCode::TOO_MANY_REQUESTS,
        "Too many requests came from your address. Wait a minute, then go back to the \
         program and start again.",
    );
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    response
}

impl<S: Store> Api<S> {
    /// The answer for a request that cannot be agreed to.
    fn unusable(&self, unusable: Unusable) -> Response {
        match unusable {
            Unusable::Stop(status, message) => stop_page(status, message),
            Unusable::Back {
                redirect_uri,
                host,
                state,
                error,
                description,
            } => {
                let link = self.way_back(
                    &redirect_uri,
                    state.as_deref(),
                    &[("error", error), ("error_description", description)],
                );
                let body = format!(
                    "<h1>Nothing was agreed to</h1>\n\
                     <p class=\"byline\">The program that sent you here asked for something \
                     Venti does not give: {}. That is the program's mistake, not yours. The \
                     link below takes you back and tells it so.</p>\n\
                     <p><a class=\"button\" href=\"{}\" rel=\"noreferrer\">Back to {}</a></p>\n",
                    Esc(description),
                    Esc(&link),
                    Esc(&host),
                );
                dress(
                    (
                        StatusCode::BAD_REQUEST,
                        page("Nothing was agreed to", &body),
                    )
                        .into_response(),
                    None,
                )
            }
        }
    }

    /// The page on which the pilot agrees or refuses, with a fresh value
    /// against forged answers in the form and in the cookie.
    fn consent_page(
        &self,
        status: StatusCode,
        asked: &Asked,
        params: &AuthorizeParams,
        message: Option<&str>,
    ) -> Result<Response, ApiError> {
        let csrf = random_id()?;
        let mut body = String::new();
        body.push_str("<h1>Let this program read your flights?</h1>\n");
        body.push_str(&format!(
            "<p class=\"byline\">A program that calls itself <b>“{}”</b> asks to read your \
             Venti logbook. The name is the program's own claim: Venti has not checked who \
             it is.</p>\n",
            Esc(&asked.client.name)
        ));
        body.push_str(&format!(
            "<dl class=\"asks\">\n\
             <div><dt>It may read</dt><dd>Your flights, the private ones and those you share \
             by link too: when and where you flew, how long, how far and how high, the score \
             and your glider.</dd></div>\n\
             <div><dt>It may not</dt><dd>Change, delete or upload anything, or see anything \
             else of your account.</dd></div>\n\
             <div><dt>You are sent back to</dt><dd>{}</dd></div>\n\
             <div><dt>For how long</dt><dd>Until you withdraw it: it is listed in your Venti \
             account among the programs with access. It also ends by itself when the program \
             has not used it for {} days.</dd></div>\n\
             </dl>\n",
            Esc(&asked.redirect_uri),
            REFRESH_LIFE.num_days(),
        ));
        if asked.loopback {
            body.push_str(
                "<p class=\"alert\">The address you are sent back to is a program on the \
                 device this browser runs on, and Venti cannot tell which. Agree only if \
                 you started this yourself a moment ago.</p>\n",
            );
        }
        if let Some(message) = message {
            body.push_str(&format!(
                "<p class=\"alert\" role=\"alert\">{}</p>\n",
                Esc(message)
            ));
        }
        body.push_str(&format!(
            "<form method=\"post\" action=\"{AUTHORIZE_PATH}\">\n"
        ));
        let carried = [
            ("response_type", params.response_type.as_deref()),
            ("client_id", params.client_id.as_deref()),
            ("redirect_uri", params.redirect_uri.as_deref()),
            ("scope", params.scope.as_deref()),
            ("state", params.state.as_deref()),
            ("code_challenge", params.code_challenge.as_deref()),
            (
                "code_challenge_method",
                params.code_challenge_method.as_deref(),
            ),
            ("resource", params.resource.as_deref()),
            ("csrf", Some(csrf.as_str())),
        ];
        for (name, value) in carried {
            if let Some(value) = value {
                body.push_str(&format!(
                    "<input type=\"hidden\" name=\"{name}\" value=\"{}\">\n",
                    Esc(value)
                ));
            }
        }
        body.push_str(
            "<label>Your handle\n<input name=\"handle\" autocomplete=\"username\" \
             autocapitalize=\"none\" spellcheck=\"false\" maxlength=\"64\"></label>\n\
             <label>Your password\n<input name=\"password\" type=\"password\" \
             autocomplete=\"current-password\" maxlength=\"256\"></label>\n\
             <p><button class=\"button\" name=\"decision\" value=\"agree\">Sign in and \
             agree</button>\n\
             <button class=\"plain\" name=\"decision\" value=\"refuse\" \
             formnovalidate>Refuse</button></p>\n\
             </form>\n",
        );
        if self.google.is_some() {
            body.push_str(
                "<p class=\"byline\">An account that signs in with Google and has no \
                 password cannot agree on this page yet.</p>\n",
            );
        }
        let html = page("Let a program read your flights?", &body);
        let mut response = dress((status, html).into_response(), Some(&asked.redirect_origin));
        self.set_consent_cookie(&mut response, &csrf, CONSENT_SECONDS);
        Ok(response)
    }
}

/// `GET /oauth/authorize`: the page on which a pilot agrees or refuses.
pub(crate) async fn authorize<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    params: Result<Query<AuthorizeParams>, QueryRejection>,
) -> Result<Response, ApiError> {
    if !api.offers_authorization() {
        return Ok(not_offered());
    }
    if let Err(refused) = api.count_oauth_request(client) {
        return Ok(busy_page(&refused));
    }
    let Ok(Query(params)) = params else {
        return Ok(stop_page(
            StatusCode::BAD_REQUEST,
            "The link that brought you here is not one a program makes to ask for access: \
             it is incomplete or says something twice. Go back to the program and start \
             again.",
        ));
    };
    match api.asked(&params).await? {
        Ok(asked) => api.consent_page(StatusCode::OK, &asked, &params, None),
        Err(unusable) => Ok(api.unusable(unusable)),
    }
}

/// `POST /oauth/authorize`: the pilot's answer.
pub(crate) async fn decide<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    headers: HeaderMap,
    answered: Result<Form<Answered>, FormRejection>,
) -> Result<Response, ApiError> {
    if !api.offers_authorization() {
        return Ok(not_offered());
    }
    if let Err(refused) = api.count_oauth_request(client) {
        return Ok(busy_page(&refused));
    }
    let forged = || {
        stop_page(
            StatusCode::FORBIDDEN,
            "This answer did not come from the page Venti showed you, or the page was open \
             for more than a quarter of an hour. Nothing was agreed to. Go back to the \
             program and start again.",
        )
    };
    // A browser names the page a form was sent from. One that names
    // another page than Venti's own is not the pilot's answer.
    let from_elsewhere = headers
        .get_all(header::ORIGIN)
        .iter()
        .any(|origin| origin.as_bytes() != api.public_url.as_str().as_bytes());
    if from_elsewhere {
        return Ok(forged());
    }
    let Ok(Form(answered)) = answered else {
        return Ok(stop_page(
            StatusCode::BAD_REQUEST,
            "This is not an answer the page Venti showed you could have sent. Nothing was \
             agreed to.",
        ));
    };
    let (params, answer) = answered.split();
    // The form is taken only with the value this browser was given in a
    // cookie when it loaded the page. A page elsewhere can make a
    // visitor's browser send a form here, but it can neither read that
    // value nor make the browser send the cookie along.
    let in_cookie = api.consent_cookie_of(&headers);
    let in_form = answer.csrf.as_deref().filter(|value| is_id(value));
    let ours = matches!((in_cookie, in_form), (Some(cookie), Some(form))
        if same(cookie.as_bytes(), form.as_bytes()));
    if !ours {
        return Ok(forged());
    }
    let asked = match api.asked(&params).await? {
        Ok(asked) => asked,
        Err(unusable) => return Ok(api.unusable(unusable)),
    };

    let agreed = match answer.decision.as_deref() {
        Some("agree") => true,
        Some("refuse") => false,
        _ => {
            return Ok(stop_page(
                StatusCode::BAD_REQUEST,
                "This is not an answer the page Venti showed you could have sent. Nothing \
                 was agreed to.",
            ));
        }
    };
    if !agreed {
        let back = api.way_back(
            &asked.redirect_uri,
            asked.state.as_deref(),
            &[
                ("error", "access_denied"),
                ("error_description", "the pilot refused"),
            ],
        );
        return Ok(api.send_back(&back));
    }

    // Signing in, as `POST /v1/sessions` does it and with its limits:
    // text that cannot be a handle is the handle of no account, and is
    // checked like any other so that it takes as long.
    let handle = Handle::new(answer.handle.as_deref().unwrap_or_default()).ok();
    let password = Password::new(answer.password.unwrap_or_default());
    let account = match api.check_password(handle.as_ref(), &password, client).await {
        Ok(Some(account)) => account,
        Ok(None) => {
            return api.consent_page(
                StatusCode::UNAUTHORIZED,
                &asked,
                &params,
                Some("The handle or the password is not correct. Nothing was agreed to."),
            );
        }
        Err(error) if error.status() == StatusCode::TOO_MANY_REQUESTS => {
            return api.consent_page(
                StatusCode::TOO_MANY_REQUESTS,
                &asked,
                &params,
                Some(
                    "Too many wrong passwords were sent for this handle or from your \
                     address. Nothing was agreed to. Try again in an hour.",
                ),
            );
        }
        Err(error) if error.status() == StatusCode::SERVICE_UNAVAILABLE => {
            return api.consent_page(
                StatusCode::SERVICE_UNAVAILABLE,
                &asked,
                &params,
                Some("Venti is busy right now. Nothing was agreed to. Try again in a moment."),
            );
        }
        Err(error) => return Err(error),
    };

    // An account that is full is told here, where its pilot reads it,
    // and not only to the program later. Agreeing again with a program
    // the account has an agreement with replaces that one.
    let held = api.store.access_tokens(&account.id).await?;
    let others = held
        .iter()
        .filter(|token| {
            token
                .client
                .as_ref()
                .is_none_or(|bound| bound.client_id != asked.client.id)
        })
        .count();
    if others >= api.limits.tokens_per_account {
        return api.consent_page(
            StatusCode::CONFLICT,
            &asked,
            &params,
            Some(
                "Your account already has as many programs with access as it may have. \
                 Withdraw one in your Venti account first. Nothing was agreed to.",
            ),
        );
    }

    let now = api.clock.now();
    let code = OauthSecret::new(OauthKind::Code)?;
    let record = AuthorizationCode {
        client_id: asked.client.id.clone(),
        account_id: account.id.clone(),
        redirect_uri: asked.redirect_uri.clone(),
        scopes: asked
            .scopes
            .iter()
            .map(|scope| scope.as_str().to_owned())
            .collect(),
        resource: api.connector_resource(),
        challenge: asked.challenge.clone(),
        created_at: now,
        expires_at: now + CODE_LIFE,
    };
    if !api
        .store
        .create_authorization_code(&code.hash(), &record)
        .await?
    {
        return Ok(stop_page(
            StatusCode::CONFLICT,
            "The program that sent you here is no longer registered with Venti. Nothing \
             was agreed to. Go back to the program and connect it again.",
        ));
    }
    // The program's name is its own word and stays out of the log; its
    // identifier is the server's.
    tracing::info!(
        account = %account.handle,
        program = %asked.client.id,
        "a pilot agreed that a program may read their flights"
    );
    let back = api.way_back(
        &asked.redirect_uri,
        asked.state.as_deref(),
        &[("code", code.expose())],
    );
    Ok(api.send_back(&back))
}

impl<S: Store> Api<S> {
    /// Sends the pilot's browser back to the program, as the answer to
    /// the pilot's own press of a button, and takes the page's cookie
    /// away again.
    fn send_back(&self, to: &str) -> Response {
        let Ok(location) = HeaderValue::from_str(to) else {
            return stop_page(
                StatusCode::BAD_REQUEST,
                "The address the program wants you sent back to cannot be written into an \
                 answer. Nothing was agreed to.",
            );
        };
        let mut response = StatusCode::SEE_OTHER.into_response();
        response.headers_mut().insert(header::LOCATION, location);
        cache::set(&mut response, header::CACHE_CONTROL, "no-store");
        cache::set(&mut response, header::REFERRER_POLICY, "no-referrer");
        cache::set(&mut response, header::X_FRAME_OPTIONS, "DENY");
        self.set_consent_cookie(&mut response, "", 0);
        response
    }
}

// --- The exchange ------------------------------------------------------------

/// What a program sends to exchange a code or a refresh token.
#[derive(Deserialize)]
pub(crate) struct Exchange {
    grant_type: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    code: Option<String>,
    code_verifier: Option<String>,
    redirect_uri: Option<String>,
    resource: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

fn tokens(access: &OauthSecret, refresh: &OauthSecret, token: &AccessToken) -> Response {
    let scopes: Vec<Scope> = token
        .scopes
        .iter()
        .filter_map(|name| Scope::from_name(name))
        .collect();
    let body = json!({
        "access_token": access.expose(),
        "token_type": "Bearer",
        "expires_in": ACCESS_LIFE.num_seconds(),
        "refresh_token": refresh.expose(),
        "scope": scope_text(&scopes),
    });
    let mut response = (StatusCode::OK, axum::Json(body)).into_response();
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    cache::set(&mut response, header::PRAGMA, "no-cache");
    response
}

impl<S: Store> Api<S> {
    /// The registered program a request names, if it names one and says
    /// nothing that only a program with a secret would say.
    async fn public_client(
        &self,
        headers: &HeaderMap,
        client_id: Option<&str>,
        client_secret: Option<&str>,
    ) -> Result<Option<OauthClient>, ApiError> {
        // A program here has no secret. One that sends one, in the body
        // or in the header, takes this for another server.
        if client_secret.is_some() || headers.contains_key(header::AUTHORIZATION) {
            return Ok(None);
        }
        match client_id.filter(|id| is_id(id)) {
            Some(id) => Ok(self.store.oauth_client(id).await?),
            None => Ok(None),
        }
    }
}

/// `POST /oauth/token`: a code for the first tokens, or a refresh token
/// for the next.
pub(crate) async fn token<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    headers: HeaderMap,
    exchange: Result<Form<Exchange>, FormRejection>,
) -> Result<Response, ApiError> {
    if !api.offers_authorization() {
        return Ok(not_offered());
    }
    if let Err(refused) = api.count_oauth_request(client) {
        return Ok(too_many(&refused));
    }
    let Ok(Form(exchange)) = exchange else {
        return Ok(invalid_request(
            "send the request as application/x-www-form-urlencoded, each parameter once",
        ));
    };
    let registered = api
        .public_client(
            &headers,
            exchange.client_id.as_deref(),
            exchange.client_secret.as_deref(),
        )
        .await?;
    let Some(registered) = registered else {
        return Ok(invalid_client());
    };
    let now = api.clock.now();
    let connector = api.connector_resource();

    match exchange.grant_type.as_deref() {
        Some("authorization_code") => {
            let (Some(code), Some(verifier)) = (&exchange.code, &exchange.code_verifier) else {
                return Ok(invalid_request("code and code_verifier are required"));
            };
            // The protocol has a program name the connector here as well
            // as on the page; one that does not is not guessed at.
            let Some(resource) = exchange.resource.as_deref() else {
                return Ok(invalid_target());
            };
            if !is_resource(resource, &connector) {
                return Ok(invalid_target());
            }
            let Some(code) = OauthSecret::parse(OauthKind::Code, code) else {
                return Ok(invalid_grant());
            };
            let Some(stored) = api.store.authorization_code(&code.hash(), now).await? else {
                return Ok(invalid_grant());
            };
            // Everything is checked before the code is touched: whoever
            // holds a code and not its verifier neither gets tokens nor
            // takes anything from the program that does.
            let record = &stored.code;
            let same_redirect = exchange
                .redirect_uri
                .as_deref()
                .is_none_or(|uri| uri == record.redirect_uri);
            if record.client_id != registered.id
                || record.resource != connector
                || !same_redirect
                || !answers(&record.challenge, verifier)
            {
                return Ok(invalid_grant());
            }

            let access = OauthSecret::new(OauthKind::Access)?;
            let refresh = OauthSecret::new(OauthKind::Refresh)?;
            let granted = AccessToken {
                id: random_id()?,
                account_id: record.account_id.clone(),
                // The name the program gave itself: the pilot's list
                // shows it as such.
                name: registered.name.clone(),
                scopes: record.scopes.clone(),
                created_at: now,
                last_used_on: None,
                expires_at: Some(now + REFRESH_LIFE),
                client: Some(TokenClient {
                    client_id: registered.id.clone(),
                    resource: connector,
                    access_until: now + ACCESS_LIFE,
                }),
            };
            let grant = NewGrant {
                token: granted.clone(),
                access: access.hash(),
                refresh: refresh.hash(),
            };
            let most = api.limits.tokens_per_account;
            match api
                .store
                .redeem_authorization_code(&code.hash(), &grant, now, most)
                .await?
            {
                Redeemed::Granted => Ok(tokens(&access, &refresh, &granted)),
                Redeemed::Spent => {
                    tracing::warn!(
                        program = %registered.id,
                        "a code of the authorization server was exchanged a second time; \
                         what came of the first exchange is withdrawn"
                    );
                    Ok(invalid_grant())
                }
                Redeemed::Unknown => Ok(invalid_grant()),
                Redeemed::Full => Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "the pilot's account holds as many tokens as it may; the pilot \
                     withdraws one first",
                )),
            }
        }
        Some("refresh_token") => {
            let Some(refresh) = &exchange.refresh_token else {
                return Ok(invalid_request("refresh_token is required"));
            };
            if exchange
                .resource
                .as_deref()
                .is_some_and(|resource| !is_resource(resource, &connector))
            {
                return Ok(invalid_target());
            }
            // Nothing wider than was agreed to: with one scope to give,
            // that is the scope itself or nothing.
            if scopes(exchange.scope.as_deref()).is_err() {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_scope",
                    "a refresh gives what the pilot agreed to and nothing more",
                ));
            }
            let Some(refresh) = OauthSecret::parse(OauthKind::Refresh, refresh) else {
                return Ok(invalid_grant());
            };
            let access = OauthSecret::new(OauthKind::Access)?;
            let next_refresh = OauthSecret::new(OauthKind::Refresh)?;
            let next = Rotation {
                access: access.hash(),
                access_until: now + ACCESS_LIFE,
                refresh: next_refresh.hash(),
                expires_at: now + REFRESH_LIFE,
            };
            match api
                .store
                .refresh_grant(&refresh.hash(), &registered.id, &next, now)
                .await?
            {
                Refreshed::Rotated(rotated) => Ok(tokens(&access, &next_refresh, &rotated.0)),
                Refreshed::Reused => {
                    tracing::warn!(
                        program = %registered.id,
                        "a refresh token was used a second time; the pilot's agreement it \
                         belonged to is withdrawn"
                    );
                    Ok(invalid_grant())
                }
                Refreshed::Unknown => Ok(invalid_grant()),
            }
        }
        _ => Ok(oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "grant_type must be authorization_code or refresh_token",
        )),
    }
}

/// What a program sends to give its tokens up.
#[derive(Deserialize)]
pub(crate) struct Revocation {
    token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

/// `POST /oauth/revoke`: a program gives up the agreement that one of its
/// tokens belongs to (RFC 7009). A token that is not good is answered as
/// one that was: there is nothing left to give up either way.
pub(crate) async fn revoke<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    headers: HeaderMap,
    revocation: Result<Form<Revocation>, FormRejection>,
) -> Result<Response, ApiError> {
    if !api.offers_authorization() {
        return Ok(not_offered());
    }
    if let Err(refused) = api.count_oauth_request(client) {
        return Ok(too_many(&refused));
    }
    if !is_form(&headers) {
        return Ok(invalid_request(
            "send the request as application/x-www-form-urlencoded",
        ));
    }
    let Ok(Form(revocation)) = revocation else {
        return Ok(invalid_request(
            "send the request as application/x-www-form-urlencoded, each parameter once",
        ));
    };
    let registered = api
        .public_client(
            &headers,
            revocation.client_id.as_deref(),
            revocation.client_secret.as_deref(),
        )
        .await?;
    let Some(registered) = registered else {
        return Ok(invalid_client());
    };
    let Some(token) = revocation.token.as_deref() else {
        return Ok(invalid_request("token is required"));
    };
    let secret = OauthSecret::parse(OauthKind::Access, token)
        .or_else(|| OauthSecret::parse(OauthKind::Refresh, token));
    if let Some(secret) = secret
        && api
            .store
            .revoke_grant(&secret.hash(), &registered.id)
            .await?
    {
        tracing::info!(program = %registered.id, "a program gave a pilot's agreement up");
    }
    let mut response = StatusCode::OK.into_response();
    cache::set(&mut response, header::CACHE_CONTROL, "no-store");
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_goes_into_a_query_is_escaped() {
        assert_eq!(percent_encoded("abc-._~XYZ019"), "abc-._~XYZ019");
        assert_eq!(
            percent_encoded("a b&c=d/e?f#g"),
            "a%20b%26c%3Dd%2Fe%3Ff%23g"
        );
        assert_eq!(
            percent_encoded("\r\nSet-Cookie: x"),
            "%0D%0ASet-Cookie%3A%20x"
        );
        assert_eq!(percent_encoded("ü"), "%C3%BC");
    }

    #[test]
    fn the_pages_policy_allows_its_style_one_form_and_no_frame() {
        let policy = share::policy_for(&STYLE);
        assert!(policy.contains("form-action 'none'"));
        assert!(policy.contains("frame-ancestors 'none'"));
        assert!(!policy.contains("unsafe"));
        assert!(!policy.contains("script"));
    }
}
