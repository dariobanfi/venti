//! Pilots among each other: following a pilot, the flights of the pilots
//! one follows, a cheer ("well flown") on a public flight, comments on a
//! public flight, and what a pilot is told of all that.
//!
//! **What is public.** That a pilot follows another, that a pilot cheered
//! a public flight and what a pilot wrote under one are public: each is
//! shown with the handle and the name of the pilot who did it, to
//! everyone, for as long as it stands. The counts are public. The lists
//! of who follows a pilot and whom a pilot follows name only pilots who
//! have a page, which is a pilot with a public flight: an account that
//! shares nothing is counted and not named. A cheer and a comment name
//! their pilot always: both are written onto somebody's flight for
//! everyone to read.
//!
//! **What is private.** The flights of the pilots one follows as one
//! list, and what one was told, are the account's own.
//!
//! **A flight that is not public shows none of it.** A cheer and a
//! comment are on a public flight. Once the flight is private, shared by
//! link only or taken down, every request about its cheers and comments
//! answers that there is no such flight; they are kept while the flight
//! is, and are back when it is public again. They go with the flight.

use serde::{Deserialize, Serialize};
use venti_domain::Timestamp;

use super::flights::{Flight, FlightId, FlightTerms, Pilot};
use super::text;

/// `GET`: the pilots the account follows, all of them.
pub const ACCOUNT_FOLLOWING_PATH: &str = "/v1/account/following";

/// `GET`: the public flights of the pilots the account follows.
pub const ACCOUNT_FEED_PATH: &str = "/v1/account/feed";

/// `GET`: what the account was told.
pub const ACCOUNT_NOTIFICATIONS_PATH: &str = "/v1/account/notifications";

/// The most pilots one account follows.
pub const MAX_FOLLOWS: usize = 1000;

/// The most characters a comment has.
pub const MAX_COMMENT_LENGTH: usize = 500;

/// The most comments one account writes within a day.
pub const COMMENTS_PER_DAY: u32 = 100;

/// The most times one account follows a pilot or cheers a flight within
/// a day, taking either back not counted.
pub const FOLLOWS_AND_CHEERS_PER_DAY: u32 = 1000;

/// For how many days what a pilot was told is kept.
pub const NOTIFICATIONS_KEPT_DAYS: i64 = 30;

/// `PUT` follows the pilot, `DELETE` follows them no more.
pub fn pilot_follow_path(handle: &str) -> String {
    format!("/v1/pilots/{handle}/follow")
}

/// `GET`: whom the pilot follows.
pub fn pilot_following_path(handle: &str) -> String {
    format!("/v1/pilots/{handle}/following")
}

/// `GET`: who follows the pilot.
pub fn pilot_followers_path(handle: &str) -> String {
    format!("/v1/pilots/{handle}/followers")
}

/// `PUT` cheers the flight, `DELETE` takes the cheer back.
pub fn flight_cheer_path(id: &FlightId) -> String {
    format!("/v1/flights/{id}/cheer")
}

/// `GET`: who cheered the flight.
pub fn flight_cheers_path(id: &FlightId) -> String {
    format!("/v1/flights/{id}/cheers")
}

/// `GET` the comments, `POST` one, `PATCH` whether more are taken.
pub fn flight_comments_path(id: &FlightId) -> String {
    format!("/v1/flights/{id}/comments")
}

/// `DELETE`: removes one comment.
pub fn flight_comment_path(id: &FlightId, comment: &str) -> String {
    format!("/v1/flights/{id}/comments/{comment}")
}

/// A page of a list here, behind the cursor of the page before.
fn paged(path: String, cursor_name: &str, cursor: Option<&str>, limit: Option<usize>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(cursor) = cursor {
        parts.push(format!("{cursor_name}={cursor}"));
    }
    if let Some(limit) = limit {
        parts.push(format!("limit={limit}"));
    }
    if parts.is_empty() {
        path
    } else {
        format!("{path}?{}", parts.join("&"))
    }
}

/// A page of a list that goes on with `after`: followers, following,
/// cheers, comments. `path` is one of the paths above; `after` is the
/// `next` of the page before.
pub fn social_page_path(path: String, after: Option<&str>, limit: Option<usize>) -> String {
    paged(path, "after", after, limit)
}

/// A page of the feed: `before` is the `next` of the page before.
pub fn feed_path(before: Option<&str>, limit: Option<usize>) -> String {
    paged(ACCOUNT_FEED_PATH.to_owned(), "before", before, limit)
}

/// What the account was told after `since`, the `cursor` of the answer
/// before.
pub fn notifications_path(since: Option<&str>, limit: Option<usize>) -> String {
    paged(ACCOUNT_NOTIFICATIONS_PATH.to_owned(), "since", since, limit)
}

/// A pilot in a list of followers or of followed pilots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowedPilot {
    /// The pilot.
    pub pilot: Pilot,
    /// Since when the one follows the other.
    pub since: Timestamp,
}

/// The answer of `PUT` and of `DELETE /v1/pilots/{handle}/follow`: how it
/// stands now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowResponse {
    /// Whether the account follows the pilot now.
    pub following: bool,
    /// How many accounts follow the pilot now.
    pub followers: u64,
}

/// The answer of `GET /v1/pilots/{handle}/followers`, of
/// `GET /v1/pilots/{handle}/following` and of `GET /v1/account/following`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowsResponse {
    /// One page of the list, the newest first. A pilot's public lists
    /// name the pilots who have a page; the account's own list names
    /// everyone it follows.
    pub pilots: Vec<FollowedPilot>,
    /// How many accounts follow the pilot the request named (for the
    /// account's own list: the account), those not named included.
    pub followers: u64,
    /// How many accounts that pilot follows, those not named included.
    pub following: u64,
    /// Whether the account that asks follows the pilot the request named.
    /// Left out when nobody is signed in, and in the account's own list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub you_follow: Option<bool>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// One cheer on a flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cheer {
    /// The pilot who cheered.
    pub pilot: Pilot,
    /// When.
    pub at: Timestamp,
}

/// The answer of `PUT` and of `DELETE /v1/flights/{id}/cheer`: how it
/// stands now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheerResponse {
    /// Whether the account cheers the flight now.
    pub cheered: bool,
    /// How many pilots cheer it now.
    pub cheers: u64,
}

/// The answer of `GET /v1/flights/{id}/cheers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheersResponse {
    /// How many pilots cheered the flight.
    pub count: u64,
    /// Whether the account that asks is one of them. Left out when nobody
    /// is signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cheered: Option<bool>,
    /// One page of them, the newest first.
    pub cheers: Vec<Cheer>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// One comment on a flight.
///
/// The text is a pilot's writing: plain text of at most
/// [`MAX_COMMENT_LENGTH`] characters. Show it as text and never as
/// markup; the server makes no link of anything in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// The comment's identifier: 26 characters.
    pub id: String,
    /// The pilot who wrote it.
    pub pilot: Pilot,
    /// What they wrote.
    pub text: String,
    /// When.
    pub at: Timestamp,
}

/// The body of `POST /v1/flights/{id}/comments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentRequest {
    /// The comment: see [`parse_comment`].
    pub text: String,
}

/// The answer of `POST /v1/flights/{id}/comments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentResponse {
    /// The comment as the server keeps it.
    pub comment: Comment,
    /// How many comments the flight has now.
    pub count: u64,
}

/// The answer of `GET /v1/flights/{id}/comments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentsResponse {
    /// How many comments the flight has.
    pub count: u64,
    /// Whether the flight takes more: `false` once its pilot turned
    /// comments off. Those that are there stay and are shown.
    pub open: bool,
    /// One page of them, the oldest first.
    pub comments: Vec<Comment>,
    /// Pass this as `after` to get the next page. Left out on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// The body of `PATCH /v1/flights/{id}/comments`, which only the
/// flight's pilot sends, and its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentsOpen {
    /// Whether the flight takes comments.
    pub open: bool,
}

/// Why a comment is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommentError {
    /// Nothing is left of it.
    #[error("a comment says something")]
    Empty,
    /// It has more than [`MAX_COMMENT_LENGTH`] characters.
    #[error("a comment has at most {MAX_COMMENT_LENGTH} characters")]
    TooLong,
}

/// Makes a comment fit to keep: what the server stores of it, and what a
/// client may show as "this is what will be posted".
///
/// Line breaks stay, as single line feeds. Every other control character
/// is dropped, and so is what would hide text or turn it round on the
/// reader's screen. Leading and trailing white space goes. The length is
/// counted after all that, in characters.
pub fn parse_comment(text: &str) -> Result<String, CommentError> {
    let kept = text::lines(text);
    if kept.is_empty() {
        return Err(CommentError::Empty);
    }
    if kept.chars().count() > MAX_COMMENT_LENGTH {
        return Err(CommentError::TooLong);
    }
    Ok(kept)
}

/// A flight in the feed: the flight as the public list has it, and what
/// pilots made of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedFlight {
    /// The flight, as `GET /v1/flights/public` lists it.
    pub flight: Flight,
    /// How many pilots cheered it.
    pub cheers: u64,
    /// How many comments it has.
    pub comments: u64,
    /// Whether the account that asks cheered it.
    pub cheered: bool,
}

/// The answer of `GET /v1/account/feed`: one page of the public flights
/// of the pilots the account follows, the newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedResponse {
    /// The flights of this page.
    pub flights: Vec<FeedFlight>,
    /// Pass this as `before` to get the next page: the flights that
    /// started before the last of this one. Left out on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// Under what the flights are given.
    pub terms: FlightTerms,
}

/// What a pilot is told of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    /// A pilot follows the account.
    Followed,
    /// A pilot cheered a flight of the account's.
    Cheered,
    /// A pilot commented on a flight of the account's.
    Commented,
    /// A kind this version does not know. A client skips it.
    #[serde(other)]
    Unknown,
}

/// The flight a pilot is told about: one of their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifiedFlight {
    /// The flight's address.
    pub id: FlightId,
    /// When its flown part begins.
    pub start: Timestamp,
    /// The name of the site it took off from, as it stood when the pilot
    /// was told. Left out when no site was known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff_site: Option<String>,
}

/// One thing a pilot was told: who did what, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// What happened.
    pub kind: NotificationKind,
    /// When.
    pub at: Timestamp,
    /// The pilot who did it, as they were called then.
    pub pilot: Pilot,
    /// The flight it was done to. Left out when a pilot follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight: Option<NotifiedFlight>,
    /// For a comment, its identifier. The words are read where the
    /// comments are, and are not there once the comment is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// The answer of `GET /v1/account/notifications`: what the account was
/// told since the device last asked, of the last
/// [`NOTIFICATIONS_KEPT_DAYS`] days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationsResponse {
    /// What the account was told, the oldest first.
    pub notifications: Vec<Notification>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more is waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server did not recognise the request's `since`,
    /// for example after it was restored from a backup, and started again
    /// from the beginning.
    pub reset: bool,
}

/// `social.json` of the archive: what the pilot gave to other pilots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveSocial {
    /// The pilots the account follows, the newest first.
    pub following: Vec<FollowedPilot>,
    /// The flights the account cheered, the oldest first, those among
    /// them that are not public at the moment.
    pub cheers: Vec<ArchiveCheer>,
    /// The comments the account wrote, the oldest first.
    pub comments: Vec<ArchiveComment>,
}

/// One cheer of the pilot's, as their archive has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveCheer {
    /// The address of the flight.
    pub flight: FlightId,
    /// When the pilot cheered it.
    pub at: Timestamp,
}

/// One comment of the pilot's, as their archive has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveComment {
    /// The comment's identifier.
    pub id: String,
    /// The address of the flight it is on.
    pub flight: FlightId,
    /// What the pilot wrote.
    pub text: String,
    /// When.
    pub at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_is_trimmed_bounded_and_keeps_its_lines() {
        assert_eq!(
            parse_comment("  Well flown!\r\nWhat a line.  ").unwrap(),
            "Well flown!\nWhat a line."
        );
        assert_eq!(parse_comment(" \u{200b}\n "), Err(CommentError::Empty));
        let long = "ä".repeat(MAX_COMMENT_LENGTH);
        assert_eq!(parse_comment(&long).unwrap(), long);
        assert_eq!(
            parse_comment(&format!("{long}x")),
            Err(CommentError::TooLong)
        );
        // What turns text round on the reader's screen is dropped.
        assert_eq!(parse_comment("a\u{202e}b").unwrap(), "ab");
    }

    #[test]
    fn the_paths_name_a_page_as_the_server_reads_it() {
        let id: FlightId = "0123456789abcdefghjkmnpqrs".parse().unwrap();
        assert_eq!(pilot_follow_path("lena"), "/v1/pilots/lena/follow");
        assert_eq!(
            social_page_path(pilot_followers_path("lena"), Some("x"), Some(20)),
            "/v1/pilots/lena/followers?after=x&limit=20"
        );
        assert_eq!(
            flight_comment_path(&id, "c"),
            "/v1/flights/0123456789abcdefghjkmnpqrs/comments/c"
        );
        assert_eq!(flight_cheer_path(&id), format!("/v1/flights/{id}/cheer"));
        assert_eq!(feed_path(None, None), "/v1/account/feed");
        assert_eq!(feed_path(Some("k"), None), "/v1/account/feed?before=k");
        assert_eq!(
            notifications_path(Some("7"), Some(50)),
            "/v1/account/notifications?since=7&limit=50"
        );
    }

    #[test]
    fn a_kind_of_a_newer_server_is_read_as_unknown() {
        let read: NotificationKind = serde_json::from_str("\"mentioned\"").unwrap();
        assert_eq!(read, NotificationKind::Unknown);
        assert_eq!(
            serde_json::to_string(&NotificationKind::Cheered).unwrap(),
            "\"cheered\""
        );
    }
}
