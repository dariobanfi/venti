//! Accounts and sessions: creating an account, signing in and out,
//! changing the password, deleting the account.

use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::{
    AccountResponse, ChangePasswordRequest, CreateAccountRequest, DeleteAccountRequest, ErrorCode,
    Handle, Password, SessionsResponse, SignInRequest, SignInResponse, parse_display_name,
};
use venti_store::{Account, AccountCreation, NewAccount, Session, Store};

use super::auth::{Client, Signed};
use super::error::ApiError;
use super::{Api, body, cache, wire};
use crate::accounts::client::counting_keys;
use crate::accounts::secret::{InviteCode, Token, random_id};
use crate::accounts::throttle::{self, Counted};
use crate::accounts::{Registration, device_label, is_reserved, password};

impl<S: Store> Api<S> {
    /// Signs a device in to an account: makes a session and its token.
    async fn open_session(
        &self,
        account: &Account,
        device: Option<&str>,
    ) -> Result<SignInResponse, ApiError> {
        let now = self.clock.now();
        let token = Token::new()?;
        let session = Session {
            id: random_id()?,
            account_id: account.id.clone(),
            device: device_label(device),
            created_at: now,
            last_used_at: now,
        };
        self.store
            .create_session(&session, &token.hash(), self.limits.sessions_per_account)
            .await?;
        Ok(SignInResponse {
            account: wire::account(account)?,
            session: wire::session(&session, &session.id),
            token: token.expose().to_owned(),
        })
    }

    /// Whether `password` is the password of the account with `handle`,
    /// and that account if it is.
    ///
    /// Wrong passwords are counted for the handle and for the client's
    /// address, and once either has too many, nothing is checked any more
    /// until its window closes. An account that does not exist takes as
    /// long and answers the same as a wrong password.
    ///
    /// The attempt takes its place among the wrong ones *before* the
    /// password is looked at, under one lock for both counts, and gives the
    /// place back when the password turns out right. Checking a password
    /// takes tens of milliseconds of a whole core; counting only afterwards
    /// let a thousand guesses that arrived together all be checked where
    /// ten are allowed.
    async fn check_password(
        &self,
        handle: Option<&Handle>,
        password: &Password,
        client: IpAddr,
    ) -> Result<Option<Account>, ApiError> {
        let now = self.clock.now();
        let address = counting_keys(client);
        let by_address = address.counted(
            self.limits.wrong_passwords_per_address,
            self.limits.wrong_passwords_per_network,
        );
        let by_handle = handle
            .map(|handle| Counted::new(handle.as_str(), self.limits.wrong_passwords_per_handle));
        let reserved = throttle::reserve_both(
            (&self.wrong_by_handle, by_handle.as_slice()),
            (&self.wrong_by_address, &by_address),
            now,
        )
        .map_err(|refused| {
            ApiError::rate_limited(
                refused,
                "too many wrong passwords for this handle or from this address",
            )
        })?;
        let give_back = |(by_handle, by_address)| {
            self.wrong_by_handle.give_back(by_handle);
            self.wrong_by_address.give_back(by_address);
        };

        let looked_up = match handle {
            Some(handle) => self.store.password_hash(handle.as_str()).await,
            None => Ok(None),
        };
        let stored = match looked_up {
            Ok(stored) => stored,
            Err(error) => {
                // Nothing was found out about the password.
                give_back(reserved);
                return Err(error.into());
            }
        };
        let hash = stored.as_ref().map(|(_, hash)| hash.as_str());
        match self.hasher.verify(password, hash).await {
            Ok(true) => {
                give_back(reserved);
                return Ok(stored.map(|(account, _)| account));
            }
            // The place stays taken: this is a wrong password.
            Ok(false) => {}
            Err(error) => {
                // The server was too busy to look. That is no guess.
                give_back(reserved);
                return Err(error.into());
            }
        }

        // The handle is logged only if it is one: text typed into the wrong
        // field can be a password.
        match &stored {
            Some((account, _)) => {
                tracing::warn!(client = %client, account = %account.handle, "wrong password");
            }
            None => tracing::warn!(client = %client, "sign-in attempt for a handle nobody has"),
        }
        Ok(None)
    }
}

fn wrong_password() -> ApiError {
    ApiError::forbidden(ErrorCode::WrongPassword, "the password is not correct")
}

/// `POST /v1/accounts`: creates an account and signs this device in.
pub(crate) async fn create<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    body: Bytes,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    // Every attempt counts, whatever becomes of it, and before anything
    // else is done with it, the password's hashing above all: this is what
    // makes trying out invite codes or handles slow.
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

    let request: CreateAccountRequest = body::json(&body)?;
    let handle = Handle::new(&request.handle)
        .map_err(|e| ApiError::bad_request(ErrorCode::InvalidHandle, e.to_string()))?;
    let display_name = parse_display_name(&request.display_name)
        .map_err(|e| ApiError::bad_request(ErrorCode::InvalidDisplayName, e.to_string()))?;
    password::check_new(&request.password, &handle)
        .map_err(|e| ApiError::bad_request(ErrorCode::InvalidPassword, e.to_string()))?;

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
    // Which handles are kept back is no secret: the list is in the source.
    if is_reserved(&handle) {
        return Err(taken());
    }

    let account = Account {
        id: random_id()?,
        handle: handle.as_str().to_owned(),
        display_name,
        created_at: now,
    };
    let new = NewAccount {
        account: account.clone(),
        password_hash: api.hasher.hash(&request.password).await?,
        invite,
    };
    // With invites, whether a handle is free is said only to someone who
    // holds a good one: the store checks the invite first.
    match api.store.create_account(&new).await? {
        AccountCreation::Created => {}
        AccountCreation::InviteRefused => return Err(invalid_invite()),
        AccountCreation::HandleTaken => return Err(taken()),
    }
    tracing::info!(account = %account.handle, "account created");

    let signed_in = api
        .open_session(&account, request.device.as_deref())
        .await?;
    Ok(cache::private_json(StatusCode::CREATED, &signed_in))
}

fn invalid_invite() -> ApiError {
    ApiError::forbidden(
        ErrorCode::InvalidInvite,
        "this server takes new accounts by invite, and the invite code is missing, \
         mistyped, used or expired",
    )
}

/// `POST /v1/sessions`: signs this device in.
pub(crate) async fn sign_in<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: SignInRequest = body::json(&body)?;
    // Text that cannot be a handle is the handle of no account. It is
    // checked like any other, so that it takes as long.
    let handle = Handle::new(&request.handle).ok();

    let account = api
        .check_password(handle.as_ref(), &request.password, client)
        .await?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                ErrorCode::InvalidCredentials,
                "the handle or the password is not correct",
            )
        })?;

    let signed_in = api
        .open_session(&account, request.device.as_deref())
        .await?;
    Ok(cache::private_json(StatusCode::CREATED, &signed_in))
}

/// `GET /v1/sessions`: the devices signed in to the account.
pub(crate) async fn sessions<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
) -> Result<Response, ApiError> {
    let sessions = api.store.sessions(&signed.account.id).await?;
    // A session that has expired is no device that is signed in, also
    // while the daily cleanup has not removed it yet.
    let expired_before = api.clock.now() - api.limits.session_idle;
    let sessions = sessions
        .iter()
        .filter(|session| session.last_used_at >= expired_before)
        .map(|session| wire::session(session, &signed.session.id))
        .collect();
    Ok(cache::private_json(
        StatusCode::OK,
        &SessionsResponse { sessions },
    ))
}

/// `DELETE /v1/sessions/{id}`: signs one device out.
pub(crate) async fn sign_out<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if !api.store.delete_session(&signed.account.id, &id).await? {
        return Err(ApiError::missing(
            ErrorCode::SessionNotFound,
            "the account has no session with this identifier",
        ));
    }
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `DELETE /v1/sessions`: signs every device out, this one included.
pub(crate) async fn sign_out_everywhere<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
) -> Result<Response, ApiError> {
    api.store.delete_sessions(&signed.account.id).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `GET /v1/account`: the account the request is signed in to.
pub(crate) async fn show(signed: Signed) -> Result<Response, ApiError> {
    let account = wire::account(&signed.account)?;
    Ok(cache::private_json(
        StatusCode::OK,
        &AccountResponse { account },
    ))
}

/// `PUT /v1/account/password`: changes the password and signs every other
/// device out.
pub(crate) async fn change_password<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Client(client): Client,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: ChangePasswordRequest = body::json(&body)?;
    let handle = Handle::new(&signed.account.handle).ok();
    api.check_password(handle.as_ref(), &request.current_password, client)
        .await?
        .ok_or_else(wrong_password)?;
    if let Some(handle) = &handle {
        password::check_new(&request.new_password, handle)
            .map_err(|e| ApiError::bad_request(ErrorCode::InvalidPassword, e.to_string()))?;
    }

    let hash = api.hasher.hash(&request.new_password).await?;
    api.store
        .set_password(&signed.account.id, &hash, &signed.session.id)
        .await?;
    tracing::info!(account = %signed.account.handle, "password changed");
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `DELETE /v1/account`: deletes the account with everything it holds.
pub(crate) async fn delete<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    Client(client): Client,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: DeleteAccountRequest = body::json(&body)?;
    let handle = Handle::new(&signed.account.handle).ok();
    // A token alone is not enough: whoever picks up an unlocked phone can
    // do much, but not this.
    api.check_password(handle.as_ref(), &request.password, client)
        .await?
        .ok_or_else(wrong_password)?;

    api.store.delete_account(&signed.account.id).await?;
    tracing::info!(account = %signed.account.handle, "account deleted");
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

#[cfg(test)]
mod tests {
    use venti_store::MemoryStore;

    use super::*;
    use crate::accounts::Limits;
    use crate::accounts::password::Cost;
    use crate::api::Settings;
    use crate::clock::FixedClock;
    use crate::flights::analyser::{Analyser, Where};
    use crate::flights::share::PublicUrl;

    /// An API with an empty store and the limits a real server has.
    fn limited_api() -> Api<MemoryStore> {
        let settings = Settings {
            limits: Limits::default(),
            ..crate::dev::settings(
                PublicUrl::new("https://venti.example").unwrap(),
                Analyser::new(Where::Thread),
                Cost::TESTS_ONLY,
            )
        };
        let clock = FixedClock(crate::adapters::recorded_at());
        Api::new(Arc::new(MemoryStore::new()), Arc::new(clock), [], settings)
    }

    #[tokio::test]
    async fn a_password_the_server_was_too_busy_to_check_is_not_a_wrong_one() {
        let api = limited_api();
        let handle = Handle::new("dario").unwrap();
        let client: IpAddr = "203.0.113.9".parse().unwrap();
        let guess = Password::new("a wrong guess!!");

        // Every turn at hashing is taken, and the test owns the clock:
        // each of these waits out its patience and is told "busy".
        let held = api.hasher.take_every_turn();
        tokio::time::pause();
        for _ in 0..50 {
            let busy = api
                .check_password(Some(&handle), &guess, client)
                .await
                .unwrap_err();
            assert_eq!(busy.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
        drop(held);
        tokio::time::resume();

        // None of the fifty was a guess. The handle has its ten left, and
        // then no more.
        for _ in 0..10 {
            let wrong = api.check_password(Some(&handle), &guess, client).await;
            assert!(wrong.unwrap().is_none());
        }
        let refused = api
            .check_password(Some(&handle), &guess, client)
            .await
            .unwrap_err();
        assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
