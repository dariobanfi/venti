//! Who is asking.
//!
//! An account is acted for with a session token in the `Authorization:
//! Bearer` header and in no other way. The server sets no cookie and reads
//! none, so a browser never attaches an account to a request by itself:
//! a page on another site cannot make a visitor's browser act for them,
//! whatever the cross-origin settings are.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header;
use axum::http::request::Parts;
use venti_store::{Account, Session, Store};

use super::Api;
use super::error::ApiError;
use crate::accounts::client::UNKNOWN_PEER;
use crate::accounts::secret::Token;

/// A request that carries a good session token.
#[derive(Debug, Clone)]
pub(crate) struct Signed {
    /// The account the token acts for.
    pub(crate) account: Account,
    /// The session the token belongs to.
    pub(crate) session: Session,
}

/// Whoever is asking: an account, or nobody in particular.
///
/// A request with no token is nobody. A request with a token that is not
/// good is refused, not treated as nobody: its sender believes to be signed
/// in and should hear that it is not.
#[derive(Debug, Clone)]
pub(crate) struct Viewer(pub(crate) Option<Signed>);

impl Viewer {
    /// Whether the viewer is the account with this identifier.
    pub(crate) fn is(&self, account_id: &str) -> bool {
        self.0
            .as_ref()
            .is_some_and(|signed| signed.account.id == account_id)
    }
}

const SIGN_IN: &str = "sign in with POST /v1/sessions and send the token as \
                       'Authorization: Bearer <token>'";

/// The token a request carries. `Ok(None)` when it carries none.
fn bearer(parts: &Parts) -> Result<Option<Token>, ApiError> {
    let mut values = parts.headers.get_all(header::AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    let unusable = || {
        ApiError::unauthorized(format!(
            "the Authorization header does not hold a session token; {SIGN_IN}"
        ))
    };
    if values.next().is_some() {
        return Err(unusable());
    }
    let (scheme, token) = value
        .to_str()
        .ok()
        .and_then(|value| value.split_once(' '))
        .ok_or_else(unusable)?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(unusable());
    }
    Token::parse(token.trim()).map(Some).ok_or_else(unusable)
}

impl<S: Store> FromRequestParts<Arc<Api<S>>> for Viewer {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = bearer(parts)? else {
            return Ok(Self(None));
        };
        let now = api.clock.now();
        let found = api
            .store
            .use_session(&token.hash(), now, now - api.limits.session_idle)
            .await?;
        match found {
            Some((session, account)) => Ok(Self(Some(Signed { account, session }))),
            // Signed out, expired, or never issued: the answer is the same.
            None => Err(ApiError::unauthorized(format!(
                "the session token is no longer valid; {SIGN_IN}"
            ))),
        }
    }
}

impl<S: Store> FromRequestParts<Arc<Api<S>>> for Signed {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let Viewer(signed) = Viewer::from_request_parts(parts, api).await?;
        signed.ok_or_else(|| ApiError::unauthorized(format!("this needs an account; {SIGN_IN}")))
    }
}

/// The address a request came from; see [`crate::accounts::client`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Client(pub(crate) IpAddr);

impl<S: Store> FromRequestParts<Arc<Api<S>>> for Client {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(UNKNOWN_PEER, |ConnectInfo(address)| address.ip());
        let forwarded = parts
            .headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|value| value.to_str().ok());
        Ok(Self(api.proxies.client(peer, forwarded)))
    }
}
