//! Pilots among each other: who follows whom, the cheers and the comments
//! on public flights, and what a pilot was told of them.
//!
//! A follow, a cheer and a comment are their pilot's and go with the
//! account. A cheer and a comment are on a flight and go with it. Both
//! are kept while the flight is, whoever may see it: whether a flight
//! shows them is the caller's to ask, and the stores take a cheer or a
//! comment only for a flight that is public.

use venti_domain::Timestamp;

/// A pilot in a list: of followers, of followed pilots, of those who
/// cheered a flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialPilot {
    /// The account.
    pub account_id: String,
    /// Its handle, as the account is now.
    pub handle: String,
    /// Its display name, as the account is now.
    pub name: String,
    /// Since when they follow or are followed, or when they cheered.
    pub at: Timestamp,
}

impl SocialPilot {
    /// The pilot's place in a list sorted by time.
    pub fn key(&self) -> SocialKey {
        (self.at, self.account_id.clone())
    }
}

/// The place of an entry in a list here: its moment and, for entries of
/// the same moment, the identifier of its account or of the comment.
pub type SocialKey = (Timestamp, String);

/// Which side of following a list is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowSide {
    /// The accounts that follow the account.
    Followers,
    /// The accounts the account follows.
    Following,
}

/// What became of a call that follows a pilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Following {
    /// The one follows the other now; `new` when this call made it so.
    Follows {
        /// Whether this call made it so.
        new: bool,
    },
    /// The follower follows as many accounts as it may.
    Full,
    /// One of the two accounts is not there, or they are one account.
    NoAccount,
}

/// How many follow an account, and how many it follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FollowCounts {
    /// Accounts that follow it.
    pub followers: u64,
    /// Accounts it follows.
    pub following: u64,
}

/// What became of a call that cheers a flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cheering {
    /// The account cheers the flight now; `new` when this call made it
    /// so.
    Cheers {
        /// Whether this call made it so.
        new: bool,
    },
    /// There is no such public flight, or no such account.
    NoSuchFlight,
}

/// What pilots made of a flight.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlightSocial {
    /// The flight's address.
    pub flight_id: String,
    /// How many accounts cheer it.
    pub cheers: u64,
    /// How many comments it has.
    pub comments: u64,
    /// Whether the account that was asked for cheers it.
    pub cheered: bool,
    /// Whether its pilot takes no more comments.
    pub comments_closed: bool,
}

/// A comment on a flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The comment's identifier.
    pub id: String,
    /// The flight it is on.
    pub flight_id: String,
    /// The account that wrote it.
    pub author_id: String,
    /// That account's handle, as the account is now.
    pub author_handle: String,
    /// That account's display name, as the account is now.
    pub author_name: String,
    /// What it says.
    pub text: String,
    /// When it was written.
    pub created_at: Timestamp,
}

impl Comment {
    /// The comment's place in a list sorted oldest first.
    pub fn key(&self) -> SocialKey {
        (self.created_at, self.id.clone())
    }
}

/// A comment to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComment {
    /// The comment's identifier.
    pub id: String,
    /// The flight it is on.
    pub flight_id: String,
    /// The account that writes it.
    pub author_id: String,
    /// What it says.
    pub text: String,
    /// When.
    pub created_at: Timestamp,
}

/// What became of a call that adds a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Commenting {
    /// It was stored.
    Added(Comment),
    /// The flight's pilot takes no comments on it.
    Closed,
    /// There is no such public flight, or no such account.
    NoSuchFlight,
}

/// What an account gave to others' flights, for its archive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SocialGiven {
    /// The flights it cheers and since when, oldest first.
    pub cheers: Vec<(String, Timestamp)>,
    /// The comments it wrote, oldest first.
    pub comments: Vec<Comment>,
}

/// One thing an account was told, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Its number: larger than every number the account's notices used
    /// before.
    pub number: i64,
    /// When.
    pub at: Timestamp,
    /// What, as JSON text.
    pub record: String,
}

/// What an account was told after some number.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Notices {
    /// The notices, by number.
    pub notices: Vec<Notice>,
    /// Whether more is waiting beyond the limit of the request.
    pub more: bool,
    /// The largest number the account's notices have used, zero if none.
    pub latest: i64,
}
