//! Agreeing where the pilot is signed in already: the two calls with which
//! the web app reads a request for a pilot's agreement and answers it.
//!
//! The server's own page ([`super::oauth`]) signs a pilot in with handle
//! and password, and an account that signs in with Google has no
//! password. The web app runs at the server's own address and holds the
//! pilot's session, however the pilot signed in. So the authorization
//! endpoint keeps a request that passed its checks under an identifier
//! nobody can guess, its page links into the web app with that
//! identifier, and the web app:
//!
//! - reads the request (`GET /v1/oauth/requests/{id}`): the program by
//!   the name it gave itself, the host the pilot is sent back to, what may
//!   be read, when the request ends. Whoever holds the identifier may read
//!   it; it names no account.
//! - answers it (`POST /v1/oauth/requests/{id}`), agree or refuse, **with
//!   the pilot's session and with nothing else**. A token that a pilot
//!   gave to a program can never answer, whatever its scopes: a program
//!   must not be able to give itself or another program access. That is
//!   [`OwnSession`], as for making a token by hand. The answer is the
//!   address the web app sends the browser to: the program's own, as it
//!   registered it, with the code or the refusal.
//!
//! **A request is answered once.** An answered one is told "answered
//! already" until its end and hands out nothing; an ended one is unknown.
//!
//! **No second place for a password, no redirect to Google and no secret
//! of Google's**: the session is what the pilot has.
//!
//! **Nothing of a request reaches the log**: the request log leaves the
//! identifier out of the path ([`super::log`]), and no line and no error
//! here holds it, a code, or anything the program sent.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use venti_api_types::v1::{
    AnswerAuthorizationRequest, AuthorizationAnswerResponse, AuthorizationRequestResponse,
    ErrorCode, Scope,
};
use venti_store::{
    AuthorizationCode, AuthorizationRequest, OauthClient, RequestState, SecretHash, Store,
};

use super::auth::{Client, OwnSession};
use super::error::ApiError;
use super::oauth::{MAY_NOT, MAY_READ, lasts};
use super::{Api, body, cache};
use crate::accounts::oauth::{CODE_LIFE, RedirectUri};
use crate::accounts::secret::{OauthKind, OauthSecret};

/// The route of both calls.
pub(crate) const ROUTE: &str = "/v1/oauth/requests/{id}";

fn unknown() -> ApiError {
    // One answer for an identifier nobody was given, a request whose end
    // has come, and one whose program is gone: which it was is nothing
    // its sender needs.
    ApiError::missing(
        ErrorCode::AuthorizationRequestNotFound,
        "no request for agreement has this identifier, or its end has come; start again in \
         the program that asked",
    )
}

fn answered() -> ApiError {
    ApiError::new(
        StatusCode::GONE,
        ErrorCode::AuthorizationRequestAnswered,
        "this request for agreement was answered already; a request is answered once",
    )
}

/// A request that waits for its answer, with the program that asks.
struct Waiting {
    /// What the store keeps it under.
    hash: SecretHash,
    request: AuthorizationRequest,
    program: OauthClient,
}

impl<S: Store> Api<S> {
    /// The request with this identifier, if it waits for its answer and
    /// is one a pilot can still be sent back from. Looking changes
    /// nothing.
    async fn waiting(&self, id: &str, client: std::net::IpAddr) -> Result<Waiting, ApiError> {
        // A server that gives out no tokens has no requests either.
        if !self.offers_authorization() {
            return Err(unknown());
        }
        // Neither call needs more than an identifier to cost a look into
        // the database: they are counted with the page they follow.
        self.count_oauth_request(client).map_err(|refused| {
            ApiError::rate_limited(refused, "requests to the authorization server")
        })?;
        let secret = OauthSecret::parse(OauthKind::Request, id).ok_or_else(unknown)?;
        let hash = secret.hash();
        let request = match self
            .store
            .authorization_request(&hash, self.clock.now())
            .await?
        {
            RequestState::Waiting(request) => request,
            RequestState::Answered => return Err(answered()),
            RequestState::Unknown => return Err(unknown()),
        };
        let program = self
            .store
            .oauth_client(&request.client_id)
            .await?
            .ok_or_else(unknown)?;
        // The address was checked against the registration before the
        // request was kept. This says it a second time, where the pilot
        // is about to be sent there.
        let registered = RedirectUri::parse(&request.redirect_uri).is_ok_and(|redirect| {
            program
                .redirect_uris
                .iter()
                .any(|registered| redirect.is_allowed_by(registered))
        });
        if !registered {
            return Err(unknown());
        }
        Ok(Waiting {
            hash,
            request,
            program,
        })
    }
}

/// `GET /v1/oauth/requests/{id}`: a request for agreement as the pilot
/// must see it before answering. For whoever holds the identifier.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Waiting {
        request, program, ..
    } = api.waiting(&id, client).await?;
    let redirect = RedirectUri::parse(&request.redirect_uri).map_err(|_| unknown())?;
    let shown = AuthorizationRequestResponse {
        program: program.name,
        returns_to: redirect.host().to_owned(),
        on_this_device: redirect.is_loopback(),
        scopes: request
            .scopes
            .iter()
            .filter_map(|name| Scope::from_name(name))
            .collect(),
        may_read: vec![MAY_READ.to_owned()],
        may_not: MAY_NOT.to_owned(),
        lasts: lasts(),
        expires_at: request.expires_at,
    };
    Ok(cache::private_json(StatusCode::OK, &shown))
}

/// `POST /v1/oauth/requests/{id}`: the pilot's answer, agree or refuse.
///
/// **The pilot's own sign-in only** ([`OwnSession`]): a token for a
/// program is answered `403` `session_required`, whatever its scopes, and
/// a token the authorization server gave out is no token at the API at
/// all. The agreement is the account's whose session answers; nothing in
/// the request names an account.
pub(crate) async fn answer<S: Store>(
    State(api): State<Arc<Api<S>>>,
    own: OwnSession,
    Client(client): Client,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let answer: AnswerAuthorizationRequest = body::json(&headers, &body)?;
    let Waiting { hash, program, .. } = api.waiting(&id, client).await?;
    let account = &own.account;

    // What stands against agreeing is said before the request is used
    // up: the pilot sets it right and answers the same request.
    if answer.agree {
        // An account whose publishing whoever runs the server suspended
        // gives nothing new out, to a program as little as to everyone.
        if account.publishing_suspended_at.is_some() {
            return Err(ApiError::forbidden(
                ErrorCode::PublishingSuspended,
                "whoever runs this server has suspended this account's publishing: it gives \
                 no program access; its pilot may refuse the request",
            ));
        }
        // As on the server's own page: an agreement counts among the
        // account's tokens, and agreeing again with a program the account
        // has an agreement with replaces that one.
        let held = api.store.access_tokens(&account.id).await?;
        let others = held
            .iter()
            .filter(|token| {
                token
                    .client
                    .as_ref()
                    .is_none_or(|bound| bound.client_id != program.id)
            })
            .count();
        if others >= api.limits.tokens_per_account {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::TooManyTokens,
                "the account holds as many tokens as it may; withdraw one, then answer again",
            ));
        }
    }

    // From here the request is used up: of two answers, one gets it.
    let now = api.clock.now();
    let request = match api.store.answer_authorization_request(&hash, now).await? {
        RequestState::Waiting(request) => request,
        RequestState::Answered => return Err(answered()),
        RequestState::Unknown => return Err(unknown()),
    };

    if !answer.agree {
        let back = api.way_back(
            &request.redirect_uri,
            request.state.as_deref(),
            &[
                ("error", "access_denied"),
                ("error_description", "the pilot refused"),
            ],
        );
        return Ok(sent_back(back));
    }

    let code = OauthSecret::new(OauthKind::Code)?;
    let record = AuthorizationCode {
        client_id: request.client_id.clone(),
        account_id: account.id.clone(),
        redirect_uri: request.redirect_uri.clone(),
        scopes: request.scopes.clone(),
        resource: request.resource.clone(),
        challenge: request.challenge.clone(),
        created_at: now,
        expires_at: now + CODE_LIFE,
    };
    if !api
        .store
        .create_authorization_code(&code.hash(), &record)
        .await?
    {
        // The program went between the look and the answer.
        return Err(unknown());
    }
    // As on the server's own page: the program's name is its own word and
    // stays out of the log; its identifier is the server's.
    tracing::info!(
        account = %account.handle,
        program = %request.client_id,
        "a pilot agreed that a program may read their flights"
    );
    let back = api.way_back(
        &request.redirect_uri,
        request.state.as_deref(),
        &[("code", code.expose())],
    );
    Ok(sent_back(back))
}

/// The answer that names where the browser goes. Nobody keeps it.
fn sent_back(redirect_to: String) -> Response {
    cache::private_json(StatusCode::OK, &AuthorizationAnswerResponse { redirect_to })
}
