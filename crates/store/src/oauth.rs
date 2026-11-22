//! What the authorization server keeps: the programs that registered to
//! ask pilots for access, the requests that wait for a pilot's answer, the
//! codes a pilot's agreement turns into, and what a token that came of
//! such an agreement is bound to.
//!
//! A pilot's agreement is one [`AccessToken`] with a
//! [`TokenClient`]: it is listed among the pilot's tokens and withdrawn as
//! they are. Its secrets change while it lives. The program holds a
//! short-lived access token and a refresh token; using the refresh token
//! replaces both, and a refresh token that is used a second time ends the
//! agreement, since one of its two users is not the program.
//!
//! As everywhere in the store, no secret is held here, only its hash.

use venti_domain::Timestamp;

use crate::{AccessToken, Account, SecretHash};

/// A program that registered itself to ask pilots for access.
///
/// Everything in it is the program's own claim: nobody checked its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OauthClient {
    /// The identifier the server gave it.
    pub id: String,
    /// What it calls itself.
    pub name: String,
    /// The addresses a pilot may be sent back to after agreeing or
    /// refusing: at least one.
    pub redirect_uris: Vec<String>,
    /// When it registered.
    pub created_at: Timestamp,
    /// When a pilot last agreed to it or it last exchanged a code or a
    /// refresh token. `None` for a program no pilot ever agreed to.
    pub last_used_at: Option<Timestamp>,
}

/// What a token that came of a pilot's agreement is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClient {
    /// The program the pilot agreed to.
    pub client_id: String,
    /// The address of what the token may be used at, and nowhere else.
    pub resource: String,
    /// The moment from which the access token in use is no longer good.
    /// The agreement itself lasts until [`AccessToken::expires_at`], and
    /// longer each time the program uses its refresh token.
    pub access_until: Timestamp,
}

/// A request for a pilot's agreement that passed the authorization
/// endpoint's checks and waits for the pilot's answer.
///
/// It is kept under the hash of an identifier nobody can guess, and names
/// no account: whoever answers it does so with a session of their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    /// The program that asks.
    pub client_id: String,
    /// The address the pilot is sent back to with the answer: one the
    /// program registered.
    pub redirect_uri: String,
    /// What the program asks for, as the names of scopes.
    pub scopes: Vec<String>,
    /// The address of what the tokens would be for.
    pub resource: String,
    /// The program's challenge, which goes on into the code.
    pub challenge: String,
    /// The program's own value, handed back with the answer as it came.
    pub state: Option<String>,
    /// When the program sent the pilot's browser.
    pub created_at: Timestamp,
    /// The moment from which the request can no longer be answered.
    pub expires_at: Timestamp,
}

/// What the store holds under the identifier of a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestState {
    /// The request waits for its answer.
    Waiting(AuthorizationRequest),
    /// The request was answered, and its end has not come yet. Nothing of
    /// it is handed out again.
    Answered,
    /// There is no such request, or its end has come.
    Unknown,
}

/// A code a pilot's agreement was turned into, which the program exchanges
/// for tokens, once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationCode {
    /// The program the pilot agreed to.
    pub client_id: String,
    /// The pilot's account.
    pub account_id: String,
    /// The address the pilot was sent back to with the code.
    pub redirect_uri: String,
    /// What the pilot agreed to, as the names of scopes.
    pub scopes: Vec<String>,
    /// The address of what the tokens will be for.
    pub resource: String,
    /// The program's challenge: only whoever knows what hashes to it may
    /// exchange the code.
    pub challenge: String,
    /// When the pilot agreed.
    pub created_at: Timestamp,
    /// The moment from which the code is no longer good.
    pub expires_at: Timestamp,
}

/// A code as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCode {
    /// The code.
    pub code: AuthorizationCode,
    /// Whether it was exchanged already.
    pub used: bool,
}

/// The agreement a code is exchanged for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGrant {
    /// The token as it will be listed. Its [`AccessToken::client`] and its
    /// end are set.
    pub token: AccessToken,
    /// The hash of the first access token.
    pub access: SecretHash,
    /// The hash of the first refresh token.
    pub refresh: SecretHash,
}

/// What became of exchanging a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redeemed {
    /// The agreement is stored and the code is used up. An earlier
    /// agreement of the same account with the same program is replaced.
    Granted,
    /// The code was exchanged before. Whatever came of that exchange is
    /// withdrawn: a code that turns up twice was seen by somebody else.
    Spent,
    /// There is no such code, or its end has come. Nothing changed.
    Unknown,
    /// The account holds as many tokens as it may. Nothing changed.
    Full,
}

/// The secrets a refresh replaces the old ones with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotation {
    /// The hash of the new access token.
    pub access: SecretHash,
    /// The moment from which the new access token is no longer good.
    pub access_until: Timestamp,
    /// The hash of the new refresh token.
    pub refresh: SecretHash,
    /// The moment from which the new refresh token is no longer good, and
    /// with it the agreement.
    pub expires_at: Timestamp,
}

/// What became of using a refresh token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refreshed {
    /// The agreement has new secrets, and the old ones are worth nothing.
    Rotated(Box<(AccessToken, Account)>),
    /// The refresh token was used before. The agreement it belonged to is
    /// withdrawn.
    Reused,
    /// No refresh token of this program has this hash, or its end has
    /// come. Nothing changed.
    Unknown,
}
