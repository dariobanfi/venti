//! Push to the phone apps, and what pilots nearby do: the phones an
//! account registered, the account's settings for open plans near a place
//! and for being asked for help, the coarse positions of pilots who may
//! be asked, and the requests for help after landing with who was asked
//! and who can help.
//!
//! - **A phone is of a session.** A token is registered with the session
//!   of the sign-in it was registered by, and goes with that session: a
//!   pilot who signs out of a phone is told nothing on it any more. One
//!   token is of one account at a time, the one that registered it last.
//! - **A position is coarse and short-lived.** It is kept only while the
//!   account may be asked for help, as the API took it to its grid, and
//!   for an hour; nobody reads it but the store, to choose whom to ask.
//! - **A request for help is seen by few.** Its pilot, and the pilots it
//!   was sent to; its position only by those who said they can help (the
//!   API decides from what the store says). It goes with its pilot's
//!   account, and an answer with the answering pilot's.

use venti_domain::{GeoPoint, Timestamp};

use crate::GroupPilot;

/// A phone registered for push, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneDevice {
    /// The account it tells.
    pub account_id: String,
    /// The session of the sign-in that registered it; it goes with it.
    pub session_id: String,
    /// Whose push service: `apns` or `fcm`.
    pub platform: String,
    /// The token the platform gave the app.
    pub token: String,
    /// An iPhone app built for development, whose token is of Apple's
    /// sandbox.
    pub sandbox: bool,
    /// When it was registered, or registered again.
    pub created_at: Timestamp,
}

/// What became of registering a phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddingDevice {
    /// It was registered, or registered again.
    Added,
    /// The account has as many as it may.
    Full,
    /// The session is gone, and with it the account perhaps.
    NoSession,
}

/// A circle on the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    /// Its middle.
    pub position: GeoPoint,
    /// How far around, in kilometres.
    pub radius_km: u32,
}

impl Circle {
    /// Whether `position` lies in it.
    #[must_use]
    pub fn holds(&self, position: GeoPoint) -> bool {
        self.position.distance_m(position) <= f64::from(self.radius_km) * 1000.0
    }
}

/// An account's settings for what pilots nearby do. Both off for an
/// account that never set them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NearbySettings {
    /// Where the pilot wants to hear of a new open plan.
    pub open_plans_near: Option<Circle>,
    /// Whether pilots nearby may ask the pilot for help.
    pub may_be_asked_for_help: bool,
}

/// A request for help to store, and whom to ask.
#[derive(Debug, Clone, PartialEq)]
pub struct NewHelpRequest {
    /// Its identifier.
    pub id: String,
    /// The account that asks.
    pub account_id: String,
    /// Where its pilot is.
    pub position: GeoPoint,
    /// What they need, if they said.
    pub note: Option<String>,
    /// When.
    pub created_at: Timestamp,
    /// When it ends by itself.
    pub ends_at: Timestamp,
}

/// Whom a new request is sent to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HelpReach {
    /// How far around its pilot, in kilometres.
    pub radius_km: u32,
    /// Positions told before this are not looked at.
    pub positions_since: Timestamp,
    /// The most pilots asked: the nearest.
    pub most: usize,
}

/// A stored request for help.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpRequest {
    /// Its identifier.
    pub id: String,
    /// The pilot who asks.
    pub pilot: GroupPilot,
    /// Where they are.
    pub position: GeoPoint,
    /// What they need, if they said.
    pub note: Option<String>,
    /// When they asked.
    pub created_at: Timestamp,
    /// When it ends by itself.
    pub ends_at: Timestamp,
    /// When they closed it, if they did.
    pub closed_at: Option<Timestamp>,
    /// The accounts it was sent to, nearest first.
    pub asked: Vec<String>,
    /// Who said they can help, the first first.
    pub helpers: Vec<GroupPilot>,
}

impl HelpRequest {
    /// Whether it still stands at `now`.
    #[must_use]
    pub fn stands(&self, now: Timestamp) -> bool {
        self.closed_at.is_none() && self.ends_at > now
    }

    /// Whether `account_id` was asked.
    #[must_use]
    pub fn was_asked(&self, account_id: &str) -> bool {
        self.asked.iter().any(|asked| asked == account_id)
    }

    /// Whether `account_id` said they can help.
    #[must_use]
    pub fn can_help(&self, account_id: &str) -> bool {
        self.helpers
            .iter()
            .any(|helper| helper.account_id == account_id)
    }
}

/// What became of asking for help.
#[derive(Debug, Clone, PartialEq)]
pub enum HelpCreation {
    /// It is stored, sent to the pilots it names.
    Created(Box<HelpRequest>),
    /// The account has a request that still stands.
    StillOpen,
    /// There is no such account.
    NoAccount,
}

/// What became of an answer.
#[derive(Debug, Clone, PartialEq)]
pub enum HelpAnswering {
    /// It is kept; the request as it stands now, and whether the answer
    /// changed anything.
    Answered(Box<HelpRequest>, bool),
    /// No such request stands, or the account was not asked.
    NotAsked,
}
