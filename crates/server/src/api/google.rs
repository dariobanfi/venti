//! Signing in with Google: a device signs in with a Google ID token, a
//! first sign-in goes on to choosing a handle, and a signed-in pilot links
//! or unlinks the Google account of their account.
//!
//! These are for the pilot's own sign-in: a token for a program can do
//! none of them. Whether a token is good is asked of
//! [`GoogleSignIn`](crate::accounts::google::GoogleSignIn); without one,
//! every request here is answered `501` `google_not_configured`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use venti_api_types::v1::{
    AccountResponse, CreateGoogleAccountRequest, DevGoogleIdTokenResponse, ErrorCode,
    GoogleSignInRequest, GoogleSignUpResponse, Handle, LinkGoogleRequest, parse_display_name,
};
use venti_store::{Account, AccountCreation, GoogleLink, GoogleLinking, NewGoogleAccount, Store};

use super::account::invalid_invite;
use super::auth::{Client, OwnSession};
use super::error::ApiError;
use super::{Api, body, cache, wire};
use crate::accounts::client::counting_keys;
use crate::accounts::google::{GoogleSignIn, Identity, Refusal, handle_from};
use crate::accounts::secret::{InviteCode, random_id};
use crate::accounts::{Registration, is_reserved};

impl<S: Store> Api<S> {
    /// Signing in with Google, or why the server does not offer it.
    fn google(&self) -> Result<&GoogleSignIn, ApiError> {
        self.google.as_deref().ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                ErrorCode::GoogleNotConfigured,
                "this server does not offer signing in with Google",
            )
        })
    }

    /// Who an ID token says signed in, if it is good for this server.
    pub(super) async fn google_identity(&self, id_token: &str) -> Result<Identity, ApiError> {
        let google = self.google()?;
        google
            .verify(id_token, self.clock.now())
            .await
            .map_err(|refusal| match refusal {
                Refusal::Invalid(why) => ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    ErrorCode::InvalidGoogleToken,
                    format!("the Google ID token is not good for this server: {why}"),
                ),
                Refusal::Unavailable => ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    ErrorCode::GoogleUnavailable,
                    "the server could not fetch Google's keys to check the token; try again later",
                ),
            })
    }

    /// A free handle made from the Google account, if one can be made: the
    /// handle itself, else with a number after it.
    async fn suggested_handle(&self, identity: &Identity) -> Result<Option<String>, ApiError> {
        let Some(base) = handle_from(identity) else {
            return Ok(None);
        };
        for n in 0..20_u32 {
            let candidate = if n == 0 {
                base.clone()
            } else {
                let suffix = (n + 1).to_string();
                let room = Handle::MAX_LENGTH - suffix.len();
                let stem: String = base.chars().take(room).collect();
                format!("{}{suffix}", stem.trim_end_matches('_'))
            };
            let Ok(handle) = Handle::new(&candidate) else {
                continue;
            };
            if !is_reserved(&handle) && self.store.account(handle.as_str()).await?.is_none() {
                return Ok(Some(handle.as_str().to_owned()));
            }
        }
        Ok(None)
    }
}

fn link_of(identity: &Identity, now: venti_domain::Timestamp) -> GoogleLink {
    GoogleLink {
        subject: identity.subject.clone(),
        email: identity.email.clone(),
        linked_at: now,
    }
}

/// `POST /v1/auth/google`: signs this device in with a Google ID token. An
/// account linked to the Google account is signed in to (`201`, as with
/// the password); for a Google account that no account is linked to, the
/// answer (`200`) carries what choosing a handle needs.
pub(crate) async fn sign_in<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let google = api.google()?;
    let request: GoogleSignInRequest = body::json(&headers, &body)?;
    let identity = api.google_identity(&request.id_token).await?;

    if let Some(account) = api.store.google_account(&identity.subject).await? {
        // The email Google gives now is the one the account shows.
        let link = link_of(&identity, api.clock.now());
        api.store.link_google(&account.id, &link).await?;
        tracing::info!(account = %account.handle, "signed in with Google");
        let signed_in = api
            .open_session(&account, request.device.as_deref())
            .await?;
        return Ok(cache::private_json(StatusCode::CREATED, &signed_in));
    }

    let signup = google
        .signup_token(&identity, api.clock.now())
        .map_err(|_| ApiError::internal())?;
    let answer = GoogleSignUpResponse {
        needs_handle: true,
        signup,
        suggested_handle: api.suggested_handle(&identity).await?,
        name: identity
            .name
            .as_deref()
            .and_then(|name| parse_display_name(name).ok()),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `POST /v1/auth/google/account`: creates an account linked to the Google
/// account of a first sign-in with Google, and signs this device in.
///
/// The server's registration setting applies as to `POST /v1/accounts`:
/// `closed` takes nobody, `invite` asks for an invite code, and every
/// attempt is counted against the address as there.
pub(crate) async fn create<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    body::must_say_json(&headers)?;
    let google = api.google()?;
    let address = counting_keys(client);
    let by_address = address.counted(
        api.limits.registrations_per_address,
        api.limits.registrations_per_network,
    );
    api.registrations
        .attempt(&by_address, now)
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "too many accounts were asked for from this address",
            )
        })?;
    if api.registration == Registration::Closed {
        return Err(ApiError::forbidden(
            ErrorCode::RegistrationClosed,
            "this server takes no new accounts",
        ));
    }

    let request: CreateGoogleAccountRequest = body::json(&headers, &body)?;
    let identity = google.read_signup(&request.signup, now).ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::InvalidSignup,
            "the sign-up has expired or is not this server's; sign in with Google again",
        )
    })?;
    let handle = Handle::new(&request.handle)
        .map_err(|e| ApiError::bad_request(ErrorCode::InvalidHandle, e.to_string()))?;
    let display_name = parse_display_name(&request.display_name)
        .map_err(|e| ApiError::bad_request(ErrorCode::InvalidDisplayName, e.to_string()))?;
    let invite = match api.registration {
        Registration::Invite => {
            let code = request.invite.as_deref().and_then(InviteCode::parse);
            Some(code.ok_or_else(invalid_invite)?.hash())
        }
        Registration::Open | Registration::Closed => None,
    };
    let taken = || {
        ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::HandleTaken,
            "this handle is not available",
        )
    };
    if is_reserved(&handle) {
        return Err(taken());
    }

    let account = Account {
        id: random_id()?,
        handle: handle.as_str().to_owned(),
        display_name,
        created_at: now,
        publishing_suspended_at: None,
        edits_sites_since: None,
    };
    let new = NewGoogleAccount {
        account: account.clone(),
        google: link_of(&identity, now),
        invite,
    };
    match api.store.create_google_account(&new).await? {
        AccountCreation::Created => {}
        AccountCreation::InviteRefused => return Err(invalid_invite()),
        AccountCreation::HandleTaken => return Err(taken()),
        AccountCreation::GoogleTaken => return Err(google_taken()),
    }
    tracing::info!(account = %account.handle, "account created with Google");

    let signed_in = api
        .open_session(&account, request.device.as_deref())
        .await?;
    Ok(cache::private_json(StatusCode::CREATED, &signed_in))
}

fn google_taken() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        ErrorCode::GoogleTaken,
        "another Venti account is linked to this Google account; sign in with Google to reach it",
    )
}

/// `POST /v1/account/google`: links the account to the Google account of
/// an ID token, in place of one it was linked to before. The answer is the
/// account, with the Google account's email.
pub(crate) async fn link<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: LinkGoogleRequest = body::json(&headers, &body)?;
    let identity = api.google_identity(&request.id_token).await?;
    let link = link_of(&identity, api.clock.now());
    match api.store.link_google(&signed.account.id, &link).await? {
        GoogleLinking::Linked => {}
        GoogleLinking::TakenByAnother => return Err(google_taken()),
        // Deleted in this very moment.
        GoogleLinking::NoSuchAccount => return Err(ApiError::unauthorized("the account is gone")),
    }
    tracing::info!(account = %signed.account.handle, "Google account linked");
    let google = api.store.google_link(&signed.account.id).await?;
    let account = wire::account(&signed.account, google.as_ref())?;
    Ok(cache::private_json(
        StatusCode::OK,
        &AccountResponse { account },
    ))
}

/// `DELETE /v1/account/google`: unlinks the account from its Google
/// account. Refused for an account that has no password: Google is its one
/// way in.
pub(crate) async fn unlink<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
) -> Result<Response, ApiError> {
    api.google()?;
    if !api.store.has_password(&signed.account.id).await? {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::GoogleOnlySignIn,
            "the account has no password: Google is its only way in, and stays linked",
        ));
    }
    if !api.store.unlink_google(&signed.account.id).await? {
        return Err(ApiError::missing(
            ErrorCode::GoogleNotLinked,
            "the account is not linked to a Google account",
        ));
    }
    tracing::info!(account = %signed.account.handle, "Google account unlinked");
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// What `GET /v1/dev/google-id-token` is asked for.
#[derive(Deserialize)]
pub(crate) struct DevTokenQuery {
    email: Option<String>,
    name: Option<String>,
}

/// `GET /v1/dev/google-id-token?email=…&name=…`: in the development mode,
/// an ID token of its own made-up "Google" for the made-up Google account
/// with this email. Every other server answers as for a path that does not
/// exist.
pub(crate) async fn dev_id_token<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Query(query): Query<DevTokenQuery>,
) -> Result<Response, ApiError> {
    let Some(issuer) = api.google.as_deref().and_then(GoogleSignIn::dev_issuer) else {
        return Err(ApiError::missing(
            ErrorCode::NotFound,
            "this path is the development mode's and nobody else's",
        ));
    };
    let email = query
        .email
        .map(|email| email.trim().to_owned())
        .filter(|email| email.contains('@') && email.len() <= 254)
        .ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidGoogleToken,
                "name the made-up Google account by its email: ?email=anna@example.com",
            )
        })?;
    let answer = DevGoogleIdTokenResponse {
        id_token: issuer.id_token(&email, query.name.as_deref(), api.clock.now()),
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}
