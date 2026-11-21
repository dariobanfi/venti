//! The browsers an account subscribed to Web Push, and the server's own
//! key pair for it.
//!
//! A subscription is a browser's: one endpoint is of one account at a
//! time, the one that subscribed it last, so that a browser a pilot
//! signed out of and another signed in on tells the new one only. The
//! store keeps the keys as the browser gave them, as text, and reads
//! neither them nor the server's key.

use venti_domain::Timestamp;

/// A browser's subscription, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSubscription {
    /// The account it tells.
    pub account_id: String,
    /// The address of the push service the browser was given. It is the
    /// subscription's identity.
    pub endpoint: String,
    /// The browser's P-256 public key, base64url.
    pub p256dh: String,
    /// The browser's authentication secret, base64url.
    pub auth: String,
    /// When it was subscribed, or subscribed again.
    pub created_at: Timestamp,
}

/// What became of adding a subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddingPush {
    /// It was added, or its keys replaced.
    Added,
    /// The account has as many as it may.
    Full,
    /// There is no such account.
    NoAccount,
}
