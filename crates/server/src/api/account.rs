//! Accounts and sessions: creating an account, signing in and out,
//! changing the password, deleting the account, and the tokens a pilot
//! gives to other programs.
//!
//! Everything here but reading the account is for the pilot's own sign-in
//! ([`OwnSession`]): a token for a program can read the account if it has
//! the scope, and can do nothing else in this file.

use std::net::IpAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use venti_api_types::v1::{
    AccessTokensResponse, AccountResponse, ChangePasswordRequest, CreateAccessTokenRequest,
    CreateAccessTokenResponse, CreateAccountRequest, DeleteAccountRequest, ErrorCode, Handle,
    Password, Scope, SessionsResponse, SignInRequest, SignInResponse, parse_display_name,
    parse_token_name,
};
use venti_store::{AccessToken, Account, AccountCreation, NewAccount, Session, Store};

use super::auth::{Client, May, OwnSession, ReadAccount};
use super::error::ApiError;
use super::{Api, body, cache, wire};
use crate::accounts::client::counting_keys;
use crate::accounts::secret::{AccessSecret, InviteCode, Token, is_id, random_id};
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
    /// and that account if it is. This is signing in: the asker is known
    /// by nothing but the handle they name and the address they come from.
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

        match self.is_password_of(handle, password).await {
            Ok(Checked::Right(account)) => {
                give_back(reserved);
                Ok(Some(account))
            }
            // The place stays taken: this is a wrong password.
            Ok(Checked::Wrong(account)) => {
                // The handle is logged only if it is one: text typed into
                // the wrong field can be a password.
                match account {
                    Some(account) => {
                        tracing::warn!(client = %client, account = %account.handle, "wrong password");
                    }
                    None => {
                        tracing::warn!(client = %client, "sign-in attempt for a handle nobody has");
                    }
                }
                Ok(None)
            }
            // The server could not look: busy, or its database away. That
            // is no guess.
            Err(error) => {
                give_back(reserved);
                Err(error)
            }
        }
    }

    /// Whether `password` is the password of the account a request is
    /// signed in to: what changing the password, deleting the account and
    /// making a token for a program ask for beside the session's token.
    ///
    /// Wrong passwords are counted for the session, in the order
    /// [`check_password`](Self::check_password) keeps: a place is taken
    /// before the password is looked at and given back when it is right.
    /// They are not counted for the handle or the address. A stranger can
    /// use up a handle's wrong passwords from outside at any time; that
    /// closes signing in to the handle, and must not close the account to
    /// the pilot who is signed in to it. And whoever picked up a token has
    /// one session, and with it the session's few guesses.
    async fn confirm_password(
        &self,
        signed: &OwnSession,
        password: &Password,
        client: IpAddr,
    ) -> Result<(), ApiError> {
        let now = self.clock.now();
        let by_session = [Counted::new(
            &signed.session.id,
            self.limits.wrong_passwords_per_session,
        )];
        let reserved = self
            .wrong_by_session
            .reserve(&by_session, now)
            .map_err(|refused| {
                ApiError::rate_limited(refused, "too many wrong passwords from this device")
            })?;

        let handle = Handle::new(&signed.account.handle).ok();
        match self.is_password_of(handle.as_ref(), password).await {
            // The handle's account is the one signed in to, unless the
            // account was deleted and its handle taken in this very moment.
            Ok(Checked::Right(account)) if account.id == signed.account.id => {
                self.wrong_by_session.give_back(reserved);
                Ok(())
            }
            Ok(_) => {
                tracing::warn!(
                    client = %client,
                    account = %signed.account.handle,
                    "wrong password from a signed-in device"
                );
                Err(wrong_password())
            }
            Err(error) => {
                self.wrong_by_session.give_back(reserved);
                Err(error)
            }
        }
    }

    /// Looks a password up and checks it, counting nothing. An account
    /// that does not exist takes as long as one that does.
    async fn is_password_of(
        &self,
        handle: Option<&Handle>,
        password: &Password,
    ) -> Result<Checked, ApiError> {
        let stored = match handle {
            Some(handle) => self.store.password_hash(handle.as_str()).await?,
            None => None,
        };
        let hash = stored.as_ref().map(|(_, hash)| hash.as_str());
        let right = self.hasher.verify(password, hash).await?;
        let account = stored.map(|(account, _)| account);
        Ok(match account {
            Some(account) if right => Checked::Right(account),
            account => Checked::Wrong(account),
        })
    }
}

/// What looking at a password found.
enum Checked {
    /// It is the password of this account.
    Right(Account),
    /// It is not the password of the account with the handle, if there is
    /// one.
    Wrong(Option<Account>),
}

fn wrong_password() -> ApiError {
    ApiError::forbidden(ErrorCode::WrongPassword, "the password is not correct")
}

/// `POST /v1/accounts`: creates an account and signs this device in.
pub(crate) async fn create<S: Store>(
    State(api): State<Arc<Api<S>>>,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    // Not an attempt at all, and not counted as one: a body that does not
    // say it is JSON, which is what a page elsewhere can make a visitor's
    // browser send.
    body::must_say_json(&headers)?;
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

    let request: CreateAccountRequest = body::json(&headers, &body)?;
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
        publishing_suspended_at: None,
        edits_sites_since: None,
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
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: SignInRequest = body::json(&headers, &body)?;
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
    signed: OwnSession,
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
    signed: OwnSession,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    // Text of another form is the identifier of no session, and the
    // database is not asked about it: a path can hold what a database
    // refuses to compare with anything, a zero byte for one.
    let deleted = is_id(&id) && api.store.delete_session(&signed.account.id, &id).await?;
    if !deleted {
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
    signed: OwnSession,
) -> Result<Response, ApiError> {
    api.store.delete_sessions(&signed.account.id).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `GET /v1/account`: the account the request is signed in to.
pub(crate) async fn show(May(signed, ..): May<ReadAccount>) -> Result<Response, ApiError> {
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
    signed: OwnSession,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: ChangePasswordRequest = body::json(&headers, &body)?;
    let handle = Handle::new(&signed.account.handle).ok();
    api.confirm_password(&signed, &request.current_password, client)
        .await?;
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
    signed: OwnSession,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: DeleteAccountRequest = body::json(&headers, &body)?;
    // A token alone is not enough: whoever picks up an unlocked phone can
    // do much, but not this.
    api.confirm_password(&signed, &request.password, client)
        .await?;

    api.store.delete_account(&signed.account.id).await?;
    tracing::info!(account = %signed.account.handle, "account deleted");
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

/// `POST /v1/account/tokens`: makes a token for another program and shows
/// its secret, this once.
pub(crate) async fn create_token<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: CreateAccessTokenRequest = body::json(&headers, &body)?;
    let now = api.clock.now();
    let name = parse_token_name(&request.name)
        .map_err(|e| ApiError::bad_request(ErrorCode::InvalidTokenName, e.to_string()))?;
    let scopes = token_scopes(&request.scopes)?;
    if request.expires_at.is_some_and(|end| end <= now) {
        return Err(ApiError::bad_request(
            ErrorCode::InvalidExpiry,
            "expires_at must be a moment that is still to come; leave it out for a \
             token that never ends",
        ));
    }
    // A session alone is not enough: whoever picks up an unlocked phone
    // could otherwise leave with a token that outlives signing the phone
    // out. It is checked last, so that a request with a mistake in it
    // costs no hashing and no guess.
    api.confirm_password(&signed, &request.password, client)
        .await?;

    let secret = AccessSecret::new()?;
    let token = AccessToken {
        id: random_id()?,
        account_id: signed.account.id.clone(),
        name,
        scopes: scopes
            .iter()
            .map(|scope| scope.as_str().to_owned())
            .collect(),
        created_at: now,
        last_used_on: None,
        expires_at: request.expires_at,
    };
    let most = api.limits.tokens_per_account;
    let stored = api
        .store
        .create_access_token(&token, &secret.hash(), most)
        .await?;
    if !stored {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::TooManyTokens,
            format!("an account holds at most {most} tokens; withdraw one first"),
        ));
    }
    // The name is the pilot's own word and stays out of the log.
    tracing::info!(account = %signed.account.handle, "token for a program made");
    let answer = CreateAccessTokenResponse {
        token: wire::access_token(&token),
        secret: secret.expose().to_owned(),
    };
    Ok(cache::private_json(StatusCode::CREATED, &answer))
}

/// The scopes a request names: at least one, each once, in the order the
/// documents list them.
fn token_scopes(names: &[String]) -> Result<Vec<Scope>, ApiError> {
    let invalid = |message: String| ApiError::bad_request(ErrorCode::InvalidScopes, message);
    let known = || {
        let names: Vec<&str> = Scope::ALL.iter().map(|scope| scope.as_str()).collect();
        names.join(", ")
    };
    if names.is_empty() {
        return Err(invalid(format!(
            "a token has at least one scope; the scopes are {}",
            known()
        )));
    }
    // More names than there are scopes hold one twice or one too many;
    // which, is not looked for in a list of any length.
    if names.len() > Scope::ALL.len() {
        return Err(invalid(format!(
            "a token names each scope once; the scopes are {}",
            known()
        )));
    }
    let mut scopes = Vec::with_capacity(names.len());
    for name in names {
        let scope = Scope::from_name(name).ok_or_else(|| {
            invalid(format!(
                "there is no such scope; the scopes are {}",
                known()
            ))
        })?;
        if scopes.contains(&scope) {
            return Err(invalid(format!("the scope '{scope}' is named twice")));
        }
        scopes.push(scope);
    }
    scopes.sort();
    Ok(scopes)
}

/// `GET /v1/account/tokens`: the tokens the account gave to other
/// programs.
pub(crate) async fn tokens<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
) -> Result<Response, ApiError> {
    let tokens = api.store.access_tokens(&signed.account.id).await?;
    let tokens = tokens.iter().map(wire::access_token).collect();
    Ok(cache::private_json(
        StatusCode::OK,
        &AccessTokensResponse { tokens },
    ))
}

/// `DELETE /v1/account/tokens/{id}`: withdraws one token. Its next
/// request is answered as one with a token nobody was given.
pub(crate) async fn withdraw_token<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: OwnSession,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    // As for a session: text of another form is the identifier of none.
    let deleted = is_id(&id)
        && api
            .store
            .delete_access_token(&signed.account.id, &id)
            .await?;
    if !deleted {
        return Err(ApiError::missing(
            ErrorCode::TokenNotFound,
            "the account has no token with this identifier",
        ));
    }
    tracing::info!(account = %signed.account.handle, "token for a program withdrawn");
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
