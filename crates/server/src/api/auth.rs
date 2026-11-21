//! Who is asking, and what they may do.
//!
//! An account is acted for with a token in the `Authorization: Bearer`
//! header and in no other way. The server sets no cookie and reads none,
//! so a browser never attaches an account to a request by itself: a page
//! on another site cannot make a visitor's browser act for them, whatever
//! the cross-origin settings are.
//!
//! There are two kinds of token. A **session's** is what Venti's own apps
//! get by signing in with the password: it may do everything the pilot
//! may. A **token for a program** is made by the pilot for a logger, a
//! script or another maker's app: it may do what its scopes say, the pilot
//! withdraws it at any time, and it can never make or withdraw tokens,
//! see or end sessions, change the password or delete the account.
//!
//! **This is the one place that checks.** A handler cannot take a
//! [`Signed`] as an argument: only [`May`], [`Viewing`] and [`OwnSession`]
//! are read from a request, and each makes the handler say in its own
//! signature what it asks of the caller:
//!
//! - `May(signed, ..): May<WriteFlights>` takes a session, or a token with
//!   the scope `flights:write`;
//! - `Viewing(viewer, ..): Viewing<ReadFlights>` takes anybody, and counts the
//!   caller as the owner of what is their own only with a session or with
//!   a token that has `flights:read`;
//! - `own: OwnSession` takes a session and no token.
//!
//! A handler that forgets does not compile, and a new scope is one more
//! name in the list at the end of this file.

use std::marker::PhantomData;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header;
use axum::http::request::Parts;
use venti_api_types::v1::{ErrorCode, Scope};
use venti_store::{AccessToken, Account, Session, SiteVia, Store};

use super::Api;
use super::error::ApiError;
use crate::accounts::client::UNKNOWN_PEER;
use crate::accounts::secret::{AccessSecret, Token};

/// Whoever a request acts for, and through what.
#[derive(Debug, Clone)]
pub(crate) struct Signed {
    /// The account the token acts for.
    pub(crate) account: Account,
    /// Through what the request came in.
    pub(crate) by: By,
}

/// Through what a request acts for an account.
#[derive(Debug, Clone)]
pub(crate) enum By {
    /// A session of the pilot's own sign-in. It may do everything.
    Session(Session),
    /// A token the pilot gave to another program. It may do what its
    /// scopes say.
    Token(AccessToken),
}

impl Signed {
    /// Whether the caller may do what `scope` opens.
    fn may(&self, scope: Scope) -> bool {
        match &self.by {
            By::Session(_) => true,
            // The store keeps the scopes as words. One this server does
            // not know is equal to none of its scopes and opens nothing.
            // `Other` is what a client reads an unknown scope as; it is no
            // scope of the server's and opens nothing either.
            By::Token(token) => {
                scope != Scope::Other && token.scopes.iter().any(|name| name == scope.as_str())
            }
        }
    }

    /// What a record that keeps its origin says of a write by this caller.
    pub(crate) fn site_via(&self) -> SiteVia {
        match &self.by {
            By::Session(_) => SiteVia::Session,
            By::Token(token) => SiteVia::Token(token.name.clone()),
        }
    }
}

/// A request that acts for an account and may do what `N` names: one with
/// a session, or with a token for a program that has that scope.
///
/// A token that lacks the scope is answered `403` `insufficient_scope`,
/// and the answer names the scope.
#[derive(Debug, Clone)]
pub(crate) struct May<N>(pub(crate) Signed, pub(crate) PhantomData<N>);

/// A request that needs no account: whoever is asking, or nobody in
/// particular, with what `N` names as the scope a token needs to count as
/// the owner of what is its account's own.
#[derive(Debug, Clone)]
pub(crate) struct Viewing<N>(pub(crate) Viewer, pub(crate) PhantomData<N>);

/// A request made with a session of the pilot's own sign-in. A token for
/// a program is answered `403` `session_required`: what asks for this is
/// the account's own to do.
#[derive(Debug, Clone)]
pub(crate) struct OwnSession {
    /// The account the session is signed in to.
    pub(crate) account: Account,
    /// The session the request was made with.
    pub(crate) session: Session,
}

/// Whoever is asking: an account, or nobody in particular.
///
/// A request with no token is nobody. A request with a token that is not
/// good is refused, not treated as nobody: its sender believes to be signed
/// in and should hear that it is not.
#[derive(Debug, Clone)]
pub(crate) struct Viewer {
    signed: Option<Signed>,
    /// The scope a token needs to count as the owner of what is its
    /// account's own. `None` where that was checked already.
    opens: Option<Scope>,
}

impl Viewer {
    /// Nobody in particular.
    pub(crate) fn nobody() -> Self {
        Self {
            signed: None,
            opens: None,
        }
    }

    /// A caller that has passed its own check, looking at what is theirs
    /// as part of what that check allowed.
    pub(crate) fn allowed(signed: &Signed) -> Self {
        Self {
            signed: Some(signed.clone()),
            opens: None,
        }
    }

    /// Whether the request carries a good token, of either kind.
    pub(crate) fn is_signed(&self) -> bool {
        self.signed.is_some()
    }

    /// Whether the viewer is the account with this identifier and may see
    /// what is that account's own.
    ///
    /// A token of that very account which lacks the scope is told so: it
    /// acts for the owner, and "not found" would send its maker looking
    /// for a mistake that is not there.
    pub(crate) fn owns(&self, account_id: &str) -> Result<bool, ApiError> {
        match (&self.signed, self.opens) {
            (Some(signed), _) if signed.account.id != account_id => Ok(false),
            (Some(signed), Some(scope)) if !signed.may(scope) => {
                Err(ApiError::insufficient_scope(scope))
            }
            (Some(_), _) => Ok(true),
            (None, _) => Ok(false),
        }
    }
}

const SIGN_IN: &str = "sign in with POST /v1/sessions and send the token as \
                       'Authorization: Bearer <token>'";

/// What a request carries to act for an account.
enum Bearer {
    Session(Token),
    Program(AccessSecret),
}

/// The token a request carries. `Ok(None)` when it carries none.
fn bearer(parts: &Parts) -> Result<Option<Bearer>, ApiError> {
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
    let token = token.trim();
    // The longer prefix first: it begins with the shorter one.
    AccessSecret::parse(token)
        .map(Bearer::Program)
        .or_else(|| Token::parse(token).map(Bearer::Session))
        .map(Some)
        .ok_or_else(unusable)
}

/// Whoever the request acts for. `Ok(None)` when it carries no token.
async fn signed<S: Store>(parts: &Parts, api: &Api<S>) -> Result<Option<Signed>, ApiError> {
    let Some(bearer) = bearer(parts)? else {
        return Ok(None);
    };
    let now = api.clock.now();
    match bearer {
        Bearer::Session(token) => api
            .store
            .use_session(&token.hash(), now, now - api.limits.session_idle)
            .await?
            .map(|(session, account)| Signed {
                account,
                by: By::Session(session),
            })
            // Signed out, expired, or never issued: the answer is the same.
            .ok_or_else(|| {
                ApiError::unauthorized(format!("the session token is no longer valid; {SIGN_IN}"))
            }),
        Bearer::Program(secret) => api
            .store
            .use_access_token(&secret.hash(), now)
            .await?
            .map(|(token, account)| Signed {
                account,
                by: By::Token(token),
            })
            // Withdrawn, ended, or never given out: the answer is the
            // same, and the same as a session's.
            .ok_or_else(|| {
                ApiError::unauthorized(
                    "the token is not valid: it was withdrawn, has ended or was never \
                     given out; its pilot makes a new one in their account",
                )
            }),
    }
    .map(Some)
}

fn needs_account() -> ApiError {
    ApiError::unauthorized(format!("this needs an account; {SIGN_IN}"))
}

impl<S: Store, N: Needs> FromRequestParts<Arc<Api<S>>> for May<N> {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let signed = signed(parts, api).await?.ok_or_else(needs_account)?;
        if !signed.may(N::SCOPE) {
            return Err(ApiError::insufficient_scope(N::SCOPE));
        }
        Ok(Self(signed, PhantomData))
    }
}

impl<S: Store, N: Needs> FromRequestParts<Arc<Api<S>>> for Viewing<N> {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let viewer = Viewer {
            signed: signed(parts, api).await?,
            opens: Some(N::SCOPE),
        };
        Ok(Self(viewer, PhantomData))
    }
}

impl<S: Store> FromRequestParts<Arc<Api<S>>> for OwnSession {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        api: &Arc<Api<S>>,
    ) -> Result<Self, Self::Rejection> {
        let signed = signed(parts, api).await?.ok_or_else(needs_account)?;
        match signed.by {
            By::Session(session) => Ok(Self {
                account: signed.account,
                session,
            }),
            By::Token(_) => Err(ApiError::forbidden(
                ErrorCode::SessionRequired,
                "this is for the pilot's own sign-in; a token that a pilot gave to a \
                 program cannot do it, whatever its scopes",
            )),
        }
    }
}

/// What a handler asks of a token for a program: one scope.
pub(crate) trait Needs: Send + Sync + 'static {
    /// The scope a token must have.
    const SCOPE: Scope;
}

/// Declares the names handlers ask by, one per scope.
macro_rules! needs {
    ($($(#[$doc:meta])* $name:ident => $scope:ident,)*) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Copy)]
            pub(crate) struct $name;

            impl Needs for $name {
                const SCOPE: Scope = Scope::$scope;
            }
        )*

        /// Every scope a handler can ask for.
        #[cfg(test)]
        const ASKED_FOR: &[Scope] = &[$(Scope::$scope),*];
    };
}

needs! {
    /// `account:read`.
    ReadAccount => AccountRead,
    /// `flights:read`.
    ReadFlights => FlightsRead,
    /// `flights:write`.
    WriteFlights => FlightsWrite,
    /// `live:write`.
    WriteLive => LiveWrite,
    /// `sites:write`.
    WriteSites => SitesWrite,
    /// `groups:read`.
    ReadGroups => GroupsRead,
    /// `groups:write`.
    WriteGroups => GroupsWrite,
    /// `reports:write`.
    WriteReports => ReportsWrite,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> Account {
        Account {
            id: "0000000000000000000000dario".to_owned(),
            handle: "dario".to_owned(),
            display_name: "Dario".to_owned(),
            created_at: crate::adapters::recorded_at(),
            publishing_suspended_at: None,
        }
    }

    fn with_token(scopes: &[&str]) -> Signed {
        Signed {
            account: account(),
            by: By::Token(AccessToken {
                id: "000000000000000000000logger".to_owned(),
                account_id: account().id,
                name: "Logger".to_owned(),
                scopes: scopes.iter().map(|&scope| scope.to_owned()).collect(),
                created_at: crate::adapters::recorded_at(),
                last_used_on: None,
                expires_at: None,
            }),
        }
    }

    #[test]
    fn every_scope_can_be_asked_for_and_none_twice() {
        let mut asked: Vec<Scope> = ASKED_FOR.to_vec();
        asked.sort();
        asked.dedup();
        assert_eq!(asked.len(), ASKED_FOR.len());
        let mut all = Scope::ALL.to_vec();
        all.sort();
        assert_eq!(asked, all);
    }

    #[test]
    fn a_token_may_do_what_its_scopes_say_and_nothing_else() {
        let token = with_token(&["flights:read", "sites:write", "photos:write", "other"]);
        for scope in Scope::ALL {
            let named = matches!(scope, Scope::FlightsRead | Scope::SitesWrite);
            assert_eq!(token.may(scope), named, "{scope}");
        }
        // A word a newer or older server wrote opens nothing, and neither
        // does the word a client reads an unknown scope as.
        assert!(!token.may(Scope::Other));
        assert_eq!(token.site_via(), SiteVia::Token("Logger".to_owned()));
    }

    #[test]
    fn a_viewer_owns_what_is_theirs_only_with_the_scope() {
        let own = account().id;
        let with = Viewer {
            signed: Some(with_token(&["flights:read"])),
            opens: Some(Scope::FlightsRead),
        };
        assert_eq!(with.owns(&own).ok(), Some(true));
        assert_eq!(with.owns("somebody else").ok(), Some(false));

        let without = Viewer {
            signed: Some(with_token(&["flights:write"])),
            opens: Some(Scope::FlightsRead),
        };
        let refused = without.owns(&own).unwrap_err();
        assert_eq!(refused.code(), ErrorCode::InsufficientScope);
        // What is another's is not theirs, with or without the scope.
        assert_eq!(without.owns("somebody else").ok(), Some(false));
        assert_eq!(Viewer::nobody().owns(&own).ok(), Some(false));
        // A caller whose own check has passed owns what is theirs.
        let allowed = Viewer::allowed(&with_token(&["reports:write"]));
        assert_eq!(allowed.owns(&own).ok(), Some(true));
    }
}
