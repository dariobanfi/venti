//! Accounts, what proves who owns one, and the devices signed in to them.

use std::fmt;

use venti_domain::Timestamp;

/// A session's last use is written down only when the recorded one is this
/// many minutes old, so that a busy app does not make the database write on
/// every request. `last_used_at` is therefore right to within this.
pub const SESSION_TOUCH_MINUTES: i64 = 5;

/// A pilot's account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account's identifier inside the server. It never changes and is
    /// not shown to anyone; people use the handle.
    pub id: String,
    /// The handle, in lower case. No two accounts share one.
    pub handle: String,
    /// The name the pilot is shown by.
    pub display_name: String,
    /// When the account was created.
    pub created_at: Timestamp,
    /// Since when the account may not publish, if whoever runs the server
    /// has suspended that: it can make no flight public, and its live
    /// tracks are shown to those with their link only. Everything else
    /// about the account works. A new account is created with `None`.
    pub publishing_suspended_at: Option<Timestamp>,
}

/// What became of suspending an account's publishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suspension {
    /// Since when the account is suspended: the moment of this call, or of
    /// the earlier one if it was suspended already.
    pub since: Timestamp,
    /// Flights that were public and are now for those with their link.
    pub flights: u64,
    /// Live tracks that were public and are now for those with their link.
    pub live_tracks: u64,
}

/// An account to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    /// The account. Its handle must be in lower case.
    pub account: Account,
    /// The hash of its password, as the hashing function writes it. The
    /// store never sees a password.
    pub password_hash: String,
    /// The invite the account is created with. `None` creates it without
    /// one; whether that is allowed is the caller's decision.
    pub invite: Option<SecretHash>,
}

/// What became of an attempt to create an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountCreation {
    /// The account exists now, and its invite is used up.
    Created,
    /// The invite is unknown, already used or expired. Nothing was stored.
    /// This is checked first, so that without a good invite nothing can be
    /// learned about which handles exist.
    InviteRefused,
    /// Another account has the handle. Nothing was stored and the invite is
    /// still good.
    HandleTaken,
}

/// The SHA-256 hash of a secret the store has to recognise and must not
/// hold: a session token, an invite code.
///
/// These secrets are long random values, so a plain hash is enough: nobody
/// can guess what hashes to it. A password is not, and is never put here.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SecretHash(pub [u8; 32]);

impl fmt::Debug for SecretHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretHash(..)")
    }
}

/// One device signed in to an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The session's identifier. It is not the token and gives no access.
    pub id: String,
    /// The account the session belongs to.
    pub account_id: String,
    /// What the device called itself when it signed in.
    pub device: Option<String>,
    /// When the device signed in.
    pub created_at: Timestamp,
    /// When the session was last used, to within
    /// [`SESSION_TOUCH_MINUTES`].
    pub last_used_at: Timestamp,
}

/// An invite to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInvite {
    /// The hash of the invite code.
    pub code: SecretHash,
    /// When the invite was made.
    pub created_at: Timestamp,
    /// The moment from which it is no longer good. `None` never expires.
    pub expires_at: Option<Timestamp>,
}
