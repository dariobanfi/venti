//! Pilots among each other: following a pilot, the flights of the pilots
//! one follows, cheers and comments on public flights, and what the
//! pilot was told of them.
//!
//! As everywhere, the core does not talk to the server: one function
//! makes the request, another reads the answer into rows and words for a
//! screen, and the shell sends the request in between
//! (`docs/api.md`, "Pilots among each other"). Nothing is kept on the
//! device: all of it is read from the server when a screen shows it.
//!
//! A request that reads what everyone may read takes `signed_in`: with
//! it the shell sends the session token, and the answer also says what
//! is the pilot's own (whether they follow, whether they cheered).
//!
//! What another pilot wrote, a name or a comment, is cut to bounded
//! text before it reaches a screen. A comment is plain text: a shell
//! shows it as text and never as markup, and makes no link of it.

use venti_api_types::v1::{
    self as api, ACCOUNT_FOLLOWING_PATH, CheerResponse, CheersResponse, CommentRequest,
    CommentResponse, CommentsOpen, CommentsResponse, FeedResponse, FlightId, FollowResponse,
    FollowsResponse, Handle, MAX_COMMENT_LENGTH, NotificationKind, NotificationsResponse,
    feed_path, flight_cheer_path, flight_cheers_path, flight_comment_path, flight_comments_path,
    notifications_path, parse_comment, pilot_follow_path, pilot_followers_path,
    pilot_following_path, social_page_path,
};
use venti_domain::Timestamp;

use crate::account::json_request;
use crate::community::row;
use crate::live::one_line;
use crate::server::{answer, refusal};
use crate::site_edit::day_in_words;
use crate::{CoreError, PublicFlight, ServerRequest};

/// The most characters of a name a row shows.
const NAME_AT_MOST: usize = 40;

fn bare(method: &str, path: String, needs_token: bool) -> ServerRequest {
    ServerRequest {
        method: method.to_owned(),
        path,
        body: None,
        content_type: None,
        needs_token,
    }
}

fn handle(text: &str) -> Result<Handle, CoreError> {
    text.trim()
        .trim_start_matches('@')
        .parse()
        .map_err(|_| CoreError::Server {
            message: "This is not the handle of a pilot.".to_owned(),
        })
}

fn flight(text: &str) -> Result<FlightId, CoreError> {
    text.trim().parse().map_err(|_| CoreError::Server {
        message: "This is not the address of a flight.".to_owned(),
    })
}

fn cursor(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.is_empty())
}

/// A count of the server's as a screen holds it.
fn held(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// "1 follower", "12 followers".
fn counted(count: u32, one: &str, several: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {several}")
    }
}

/// How long ago something was, for a line under it: "just now",
/// "5 min ago", "3 h ago", "2 d ago", and from a week on the day,
/// "21 Nov 2026".
///
/// The times are the server's, so "now" is the server's clock as the shell
/// last read it. A shell that has not read it hands in zero, and the line is
/// then the day, which is right by any clock: an age counted by a phone's
/// own clock can be days off.
fn age(at: Timestamp, now_unix_s: i64) -> String {
    if now_unix_s <= 0 {
        return day_in_words(at);
    }
    let seconds = (now_unix_s - at.timestamp()).max(0);
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3_599 => format!("{} min ago", seconds / 60),
        3_600..=86_399 => format!("{} h ago", seconds / 3_600),
        86_400..=604_799 => format!("{} d ago", seconds / 86_400),
        _ => day_in_words(at),
    }
}

// --- Following -------------------------------------------------------------

/// What a follow button says: "Following" for a pilot one follows,
/// "Follow" otherwise. Pressing it sends [`follow_request`] with the
/// opposite of `following`.
#[uniffi::export]
pub fn follow_button(following: bool) -> String {
    if following { "Following" } else { "Follow" }.to_owned()
}

/// "12 followers".
#[uniffi::export]
pub fn followers_line(count: u32) -> String {
    counted(count, "follower", "followers")
}

/// "Follows 3 pilots".
#[uniffi::export]
pub fn following_line(count: u32) -> String {
    format!("Follows {}", counted(count, "pilot", "pilots"))
}

/// The request that follows a pilot (`follow`) or follows them no more.
/// Needs a signed-in pilot.
///
/// # Errors
///
/// When the text is no handle.
#[uniffi::export]
pub fn follow_request(handle_text: String, follow: bool) -> Result<ServerRequest, CoreError> {
    let path = pilot_follow_path(handle(&handle_text)?.as_str());
    Ok(bare(if follow { "PUT" } else { "DELETE" }, path, true))
}

/// How following a pilot stands, for the button and the count beside it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FollowState {
    /// Whether the pilot follows them now.
    pub following: bool,
    /// What the button says now: "Following" or "Follow".
    pub button: String,
    /// How many follow them now.
    pub followers: u32,
    /// "12 followers".
    pub followers_line: String,
}

/// Reads the answer to [`follow_request`].
#[uniffi::export]
pub fn follow_answer(status: u16, body: Vec<u8>) -> Result<FollowState, CoreError> {
    let response: FollowResponse = answer(status, &body)?;
    Ok(FollowState {
        following: response.following,
        button: follow_button(response.following),
        followers: held(response.followers),
        followers_line: followers_line(held(response.followers)),
    })
}

/// The request for who follows a pilot, the newest first. `after` is the
/// `next` of the page before. With `signed_in` the answer says whether
/// the pilot follows them.
///
/// # Errors
///
/// When the text is no handle.
#[uniffi::export]
pub fn followers_request(
    handle_text: String,
    after: Option<String>,
    signed_in: bool,
) -> Result<ServerRequest, CoreError> {
    let path = pilot_followers_path(handle(&handle_text)?.as_str());
    let path = social_page_path(path, cursor(after).as_deref(), None);
    Ok(bare("GET", path, signed_in))
}

/// The request for whom a pilot follows, the newest first.
///
/// # Errors
///
/// When the text is no handle.
#[uniffi::export]
pub fn following_request(
    handle_text: String,
    after: Option<String>,
    signed_in: bool,
) -> Result<ServerRequest, CoreError> {
    let path = pilot_following_path(handle(&handle_text)?.as_str());
    let path = social_page_path(path, cursor(after).as_deref(), None);
    Ok(bare("GET", path, signed_in))
}

/// The request for everyone the signed-in pilot follows, those who share
/// no flight among them.
#[uniffi::export]
pub fn own_following_request(after: Option<String>) -> ServerRequest {
    let path = social_page_path(
        ACCOUNT_FOLLOWING_PATH.to_owned(),
        cursor(after).as_deref(),
        None,
    );
    bare("GET", path, true)
}

/// A pilot in a list of followers or of followed pilots.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FollowRow {
    /// The pilot's handle, for their page.
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    pub name: String,
    /// Since when, seconds since the Unix epoch.
    pub since_unix_s: i64,
    /// The path of the pilot's picture, to fetch with the server's address
    /// in front. `None` without one: show the initials.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub picture_path: Option<String>,
}

/// One page of followers or of followed pilots, with the counts.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FollowList {
    /// The pilots of this page, the newest first. A pilot's public lists
    /// name only pilots who share a flight; the counts count everyone.
    pub pilots: Vec<FollowRow>,
    /// How many follow the pilot.
    pub followers: u32,
    /// "12 followers".
    pub followers_line: String,
    /// How many the pilot follows.
    pub following: u32,
    /// "Follows 3 pilots".
    pub following_line: String,
    /// Whether the signed-in pilot follows them. Absent signed out, and
    /// in the pilot's own list.
    pub you_follow: Option<bool>,
    /// What the follow button says. Absent where `you_follow` is.
    pub button: Option<String>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// Reads the answer to [`followers_request`], [`following_request`] and
/// [`own_following_request`]. A pilot the server does not know is said in
/// words.
#[uniffi::export]
pub fn follow_list(status: u16, body: Vec<u8>) -> Result<FollowList, CoreError> {
    if status == 404 {
        return Err(CoreError::Server {
            message: "The server knows no pilot by this handle.".to_owned(),
        });
    }
    let response: FollowsResponse = answer(status, &body)?;
    Ok(FollowList {
        pilots: response
            .pilots
            .iter()
            .map(|followed| FollowRow {
                handle: followed.pilot.handle.to_string(),
                name: one_line(&followed.pilot.display_name, NAME_AT_MOST),
                since_unix_s: followed.since.timestamp(),
                picture_path: followed.pilot.picture.clone(),
            })
            .collect(),
        followers: held(response.followers),
        followers_line: followers_line(held(response.followers)),
        following: held(response.following),
        following_line: following_line(held(response.following)),
        you_follow: response.you_follow,
        button: response.you_follow.map(follow_button),
        next: response.next,
    })
}

// --- The feed --------------------------------------------------------------

/// The request for the public flights of the pilots the signed-in pilot
/// follows, the newest first. `before` is the `next` of the page before.
#[uniffi::export]
pub fn feed_request(before: Option<String>) -> ServerRequest {
    bare("GET", feed_path(cursor(before).as_deref(), None), true)
}

/// A flight in the feed: the row of the public list, and what pilots
/// made of it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FeedRow {
    /// The flight, as the list of everyone's public flights shows it.
    pub flight: PublicFlight,
    /// How many pilots cheered it.
    pub cheers: u32,
    /// Whether the pilot cheered it.
    pub cheered: bool,
    /// "You and 2 others cheered", "3 pilots cheered"; empty without a
    /// cheer.
    pub cheers_line: String,
    /// How many comments it has.
    pub comments: u32,
    /// "2 comments"; empty without one.
    pub comments_line: String,
}

/// One page of the feed.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Feed {
    /// The flights, the newest first.
    pub rows: Vec<FeedRow>,
    /// What to say in place of an empty first page.
    pub empty_line: String,
    /// What to pass as `before` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// Reads the answer to [`feed_request`].
#[uniffi::export]
pub fn feed(status: u16, body: Vec<u8>) -> Result<Feed, CoreError> {
    let response: FeedResponse = answer(status, &body)?;
    Ok(Feed {
        rows: response
            .flights
            .iter()
            .map(|entry| FeedRow {
                flight: row(&entry.flight),
                cheers: held(entry.cheers),
                cheered: entry.cheered,
                cheers_line: cheers_line(held(entry.cheers), entry.cheered),
                comments: held(entry.comments),
                comments_line: comments_line(held(entry.comments)),
            })
            .collect(),
        empty_line: "The public flights of the pilots you follow show here. Follow a pilot on \
                     their page."
            .to_owned(),
        next: response.next,
    })
}

// --- Cheers ----------------------------------------------------------------

/// Who cheered, in words: "You cheered", "You and 1 other cheered",
/// "You and 2 others cheered", "1 pilot cheered", "3 pilots cheered".
/// Empty without a cheer.
#[uniffi::export]
pub fn cheers_line(count: u32, cheered: bool) -> String {
    match (count, cheered) {
        (0, _) => String::new(),
        (1, true) => "You cheered".to_owned(),
        (2, true) => "You and 1 other cheered".to_owned(),
        (count, true) => format!("You and {} others cheered", count - 1),
        (count, false) => format!("{} cheered", counted(count, "pilot", "pilots")),
    }
}

/// What the cheer button says: "Cheered" once the pilot cheered, "Cheer"
/// otherwise. It means "well flown".
#[uniffi::export]
pub fn cheer_button(cheered: bool) -> String {
    if cheered { "Cheered" } else { "Cheer" }.to_owned()
}

/// The request that cheers a public flight (`cheer`) or takes the cheer
/// back. Needs a signed-in pilot.
///
/// # Errors
///
/// When the text is no flight's address.
#[uniffi::export]
pub fn cheer_request(flight_id: String, cheer: bool) -> Result<ServerRequest, CoreError> {
    let path = flight_cheer_path(&flight(&flight_id)?);
    Ok(bare(if cheer { "PUT" } else { "DELETE" }, path, true))
}

/// How the cheers of a flight stand, for the button and the line.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CheerState {
    /// Whether the pilot cheers it now.
    pub cheered: bool,
    /// What the button says now.
    pub button: String,
    /// How many cheer it now.
    pub cheers: u32,
    /// "You and 2 others cheered".
    pub line: String,
}

/// Reads the answer to [`cheer_request`].
#[uniffi::export]
pub fn cheer_answer(status: u16, body: Vec<u8>) -> Result<CheerState, CoreError> {
    let response: CheerResponse = answer(status, &body)?;
    Ok(CheerState {
        cheered: response.cheered,
        button: cheer_button(response.cheered),
        cheers: held(response.cheers),
        line: cheers_line(held(response.cheers), response.cheered),
    })
}

/// The request for who cheered a public flight, the newest first. With
/// `signed_in` the answer says whether the pilot is one of them.
///
/// # Errors
///
/// When the text is no flight's address.
#[uniffi::export]
pub fn cheers_request(
    flight_id: String,
    after: Option<String>,
    signed_in: bool,
) -> Result<ServerRequest, CoreError> {
    let path = flight_cheers_path(&flight(&flight_id)?);
    let path = social_page_path(path, cursor(after).as_deref(), None);
    Ok(bare("GET", path, signed_in))
}

/// A pilot who cheered.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CheerRow {
    /// The pilot's handle, for their page.
    pub handle: String,
    /// The name the pilot is shown by, on one line.
    pub name: String,
    /// When, seconds since the Unix epoch.
    pub at_unix_s: i64,
    /// The path of the pilot's picture, to fetch with the server's address
    /// in front. `None` without one: show the initials.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub picture_path: Option<String>,
}

/// The cheers of a flight: the count, the line, and one page of who.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CheerList {
    /// How many pilots cheered.
    pub count: u32,
    /// Whether the signed-in pilot is one of them. Absent signed out.
    pub cheered: Option<bool>,
    /// What the cheer button says.
    pub button: String,
    /// "You and 2 others cheered", "3 pilots cheered"; empty without one.
    pub line: String,
    /// The pilots of this page, the newest first.
    pub pilots: Vec<CheerRow>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// Reads the answer to [`cheers_request`].
#[uniffi::export]
pub fn cheer_list(status: u16, body: Vec<u8>) -> Result<CheerList, CoreError> {
    let response: CheersResponse = answer(status, &body)?;
    let cheered = response.cheered.unwrap_or(false);
    Ok(CheerList {
        count: held(response.count),
        cheered: response.cheered,
        button: cheer_button(cheered),
        line: cheers_line(held(response.count), cheered),
        pilots: response
            .cheers
            .iter()
            .map(|cheer| CheerRow {
                handle: cheer.pilot.handle.to_string(),
                name: one_line(&cheer.pilot.display_name, NAME_AT_MOST),
                at_unix_s: cheer.at.timestamp(),
                picture_path: cheer.pilot.picture.clone(),
            })
            .collect(),
        next: response.next,
    })
}

// --- Comments --------------------------------------------------------------

/// "1 comment", "2 comments"; empty without one.
#[uniffi::export]
pub fn comments_line(count: u32) -> String {
    if count == 0 {
        String::new()
    } else {
        counted(count, "comment", "comments")
    }
}

/// The request for the comments under a public flight, the oldest
/// first. The same for everyone: no token goes with it.
///
/// # Errors
///
/// When the text is no flight's address.
#[uniffi::export]
pub fn comments_request(
    flight_id: String,
    after: Option<String>,
) -> Result<ServerRequest, CoreError> {
    let path = flight_comments_path(&flight(&flight_id)?);
    let path = social_page_path(path, cursor(after).as_deref(), None);
    Ok(bare("GET", path, false))
}

/// A comment as a screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommentRow {
    /// The comment's identifier, to remove or report it.
    pub id: String,
    /// The handle of the pilot who wrote it, for their page.
    pub handle: String,
    /// The name that pilot is shown by, on one line.
    pub name: String,
    /// What they wrote: plain text, line breaks kept. Show it as text,
    /// never as markup, and make no link of it.
    pub text: String,
    /// When, seconds since the Unix epoch.
    pub at_unix_s: i64,
    /// How long ago: "5 min ago", "3 h ago", "21 Nov 2026".
    pub age: String,
    /// The line above the text: "Mara Muster · 3 h ago".
    pub byline: String,
    /// Whether the signed-in pilot may remove it: its author, or the
    /// pilot of the flight it is under.
    pub can_remove: bool,
    /// The path of its author's picture, to fetch with the server's
    /// address in front. `None` without one: show the initials.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub picture_path: Option<String>,
}

fn comment_row(
    comment: &api::Comment,
    now_unix_s: i64,
    own_handle: Option<&str>,
    flight_pilot: Option<&str>,
) -> CommentRow {
    let name = one_line(&comment.pilot.display_name, NAME_AT_MOST);
    let age = age(comment.at, now_unix_s);
    let author = comment.pilot.handle.as_str();
    CommentRow {
        id: comment.id.clone(),
        handle: author.to_owned(),
        byline: format!("{name} · {age}"),
        name,
        text: comment.text.clone(),
        at_unix_s: comment.at.timestamp(),
        age,
        can_remove: own_handle.is_some_and(|own| own == author || Some(own) == flight_pilot),
        picture_path: comment.pilot.picture.clone(),
    }
}

/// The comments under a flight: the count and one page.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommentList {
    /// How many comments the flight has.
    pub count: u32,
    /// "2 comments"; empty without one.
    pub count_line: String,
    /// Whether the flight takes more.
    pub open: bool,
    /// What to say in place of the form when it does not: "The pilot has
    /// turned comments off for this flight." Absent while it does.
    pub closed_line: Option<String>,
    /// The comments of this page, the oldest first.
    pub comments: Vec<CommentRow>,
    /// What to pass as `after` for the next page. Absent on the last.
    pub next: Option<String>,
}

/// Reads the answer to [`comments_request`]. `own_handle` is the
/// signed-in pilot's handle and `flight_handle` the handle of the
/// flight's pilot: with them each comment says whether the pilot may
/// remove it.
#[uniffi::export]
pub fn comment_list(
    status: u16,
    body: Vec<u8>,
    now_unix_s: i64,
    own_handle: Option<String>,
    flight_handle: Option<String>,
) -> Result<CommentList, CoreError> {
    let response: CommentsResponse = answer(status, &body)?;
    Ok(CommentList {
        count: held(response.count),
        count_line: comments_line(held(response.count)),
        open: response.open,
        closed_line: (!response.open)
            .then(|| "The pilot has turned comments off for this flight.".to_owned()),
        comments: response
            .comments
            .iter()
            .map(|comment| {
                comment_row(
                    comment,
                    now_unix_s,
                    own_handle.as_deref(),
                    flight_handle.as_deref(),
                )
            })
            .collect(),
        next: response.next,
    })
}

/// A comment as the pilot is writing it: what would be posted, how much
/// room is left and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommentDraft {
    /// The text as it would be kept: trimmed, control characters gone.
    pub text: String,
    /// The most characters a comment has: 500.
    pub at_most: u32,
    /// How many characters are left; negative when it is too long.
    pub left: i32,
    /// "120 of 500".
    pub counter: String,
    /// What is wrong, in words. Absent for a comment that can be sent,
    /// and for an empty one, which is merely not written yet.
    pub fault: Option<String>,
    /// Whether it can be sent.
    pub can_send: bool,
    /// What the pilot is told beside the form.
    pub notice: String,
}

/// Looks at a comment while the pilot writes it.
#[uniffi::export]
pub fn comment_draft(text: String) -> CommentDraft {
    let kept = match parse_comment(&text) {
        Ok(kept) => kept,
        // Too long or empty: what is left of it is shown all the same.
        Err(_) => text.trim().to_owned(),
    };
    let length = i64::try_from(kept.chars().count()).unwrap_or(i64::MAX);
    let at_most = u32::try_from(MAX_COMMENT_LENGTH).unwrap_or(u32::MAX);
    let left = i64::from(at_most) - length;
    let empty = parse_comment(&text) == Err(api::CommentError::Empty);
    let fault = (left < 0).then(|| {
        format!(
            "A comment has room for {MAX_COMMENT_LENGTH} characters. Shorten it by {}.",
            -left
        )
    });
    CommentDraft {
        text: kept,
        at_most,
        left: i32::try_from(left).unwrap_or(i32::MIN),
        counter: format!("{length} of {at_most}"),
        can_send: !empty && fault.is_none(),
        fault,
        notice: "Everyone who can see the flight reads your comment, with your name. You and \
                 the flight's pilot can remove it."
            .to_owned(),
    }
}

/// The request that posts a comment under a public flight. Needs a
/// signed-in pilot.
///
/// # Errors
///
/// When the text is no flight's address, or the comment is empty or too
/// long.
#[uniffi::export]
pub fn comment_request(flight_id: String, text: String) -> Result<ServerRequest, CoreError> {
    let path = flight_comments_path(&flight(&flight_id)?);
    let text = parse_comment(&text).map_err(|error| CoreError::Server {
        message: match error {
            api::CommentError::Empty => "Write something first.".to_owned(),
            api::CommentError::TooLong => {
                format!("A comment has room for {MAX_COMMENT_LENGTH} characters. Shorten it.")
            }
        },
    })?;
    json_request("POST", &path, &CommentRequest { text }, true)
}

/// A comment that was just posted, with how many the flight has now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommentPosted {
    /// The comment, to add under the others. Its author may remove it.
    pub comment: CommentRow,
    /// How many comments the flight has now.
    pub count: u32,
    /// "3 comments".
    pub count_line: String,
}

/// Reads the answer to [`comment_request`].
#[uniffi::export]
pub fn comment_posted(
    status: u16,
    body: Vec<u8>,
    now_unix_s: i64,
) -> Result<CommentPosted, CoreError> {
    let response: CommentResponse = answer(status, &body)?;
    let own = response.comment.pilot.handle.to_string();
    Ok(CommentPosted {
        comment: comment_row(&response.comment, now_unix_s, Some(&own), None),
        count: held(response.count),
        count_line: comments_line(held(response.count)),
    })
}

/// The request that removes a comment: the pilot's own, or one under a
/// flight of theirs. The answer is read by [`social_done`].
///
/// # Errors
///
/// When the text is no flight's address.
#[uniffi::export]
pub fn comment_remove_request(
    flight_id: String,
    comment_id: String,
) -> Result<ServerRequest, CoreError> {
    let path = flight_comment_path(&flight(&flight_id)?, comment_id.trim());
    Ok(bare("DELETE", path, true))
}

/// What the switch on the pilot's own flight says, and the line under
/// it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommentsSwitch {
    /// Whether the flight takes comments.
    pub open: bool,
    /// "Comments".
    pub title: String,
    /// What the setting means as it stands.
    pub line: String,
}

/// The switch for a flight that takes comments (`open`) or does not.
#[uniffi::export]
pub fn comments_switch(open: bool) -> CommentsSwitch {
    CommentsSwitch {
        open,
        title: "Comments".to_owned(),
        line: if open {
            "Other pilots can write under this flight while it is public."
        } else {
            "Nobody can write under this flight. What was written stays."
        }
        .to_owned(),
    }
}

/// The request by which the pilot says whether a flight of their own
/// takes comments.
///
/// # Errors
///
/// When the text is no flight's address.
#[uniffi::export]
pub fn comments_open_request(flight_id: String, open: bool) -> Result<ServerRequest, CoreError> {
    let path = flight_comments_path(&flight(&flight_id)?);
    json_request("PATCH", &path, &CommentsOpen { open }, true)
}

/// Reads the answer to [`comments_open_request`].
#[uniffi::export]
pub fn comments_open_answer(status: u16, body: Vec<u8>) -> Result<CommentsSwitch, CoreError> {
    let response: CommentsOpen = answer(status, &body)?;
    Ok(comments_switch(response.open))
}

/// Reads the answer to a request here that carries nothing back
/// ([`comment_remove_request`]): nothing when it worked, the refusal in
/// words when not.
#[uniffi::export]
pub fn social_done(status: u16, body: Vec<u8>) -> Result<(), CoreError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(refusal(status, &body))
    }
}

// --- What the pilot was told -------------------------------------------------

/// The request for what the signed-in pilot was told since `since`, the
/// `cursor` of the answer before.
#[uniffi::export]
pub fn notifications_request(since: Option<String>) -> ServerRequest {
    bare(
        "GET",
        notifications_path(cursor(since).as_deref(), None),
        true,
    )
}

/// What a line is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum NotificationAbout {
    /// A pilot follows the pilot.
    Followed,
    /// A pilot cheered a flight of the pilot's.
    Cheered,
    /// A pilot commented on a flight of the pilot's.
    Commented,
}

/// One thing the pilot was told, as a line.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct NotificationLine {
    /// What it is about.
    pub about: NotificationAbout,
    /// "Lena Probe (@lena) cheered your flight from Wallberg".
    pub line: String,
    /// The handle of the pilot who did it, for their page.
    pub handle: String,
    /// The flight it is about, for its page. Absent for a follow.
    pub flight_id: Option<String>,
    /// For a comment, its identifier.
    pub comment_id: Option<String>,
    /// When, seconds since the Unix epoch.
    pub at_unix_s: i64,
    /// How long ago: "3 h ago".
    pub age: String,
}

/// What the pilot was told since the device last asked.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Notifications {
    /// The lines, the oldest first. A screen shows the newest on top.
    pub lines: Vec<NotificationLine>,
    /// What to pass as `since` next time.
    pub cursor: String,
    /// Whether more is waiting: ask again at once with `cursor`.
    pub more: bool,
    /// Whether the server started again from the beginning: what was
    /// shown before is replaced by these lines.
    pub reset: bool,
    /// What to say when there never was a line.
    pub empty_line: String,
}

/// "your flight from Wallberg", or with no site known "your flight of
/// 15 Aug 2026".
fn your_flight(flight: Option<&api::NotifiedFlight>) -> String {
    match flight {
        Some(flight) => match flight
            .takeoff_site
            .as_deref()
            .map(|name| one_line(name, 60))
            .filter(|name| !name.is_empty())
        {
            Some(site) => format!("your flight from {site}"),
            None => format!("your flight of {}", day_in_words(flight.start)),
        },
        None => "your flight".to_owned(),
    }
}

/// Reads the answer to [`notifications_request`]. A line of a kind this
/// version does not know is left out.
#[uniffi::export]
pub fn notifications(
    status: u16,
    body: Vec<u8>,
    now_unix_s: i64,
) -> Result<Notifications, CoreError> {
    let response: NotificationsResponse = answer(status, &body)?;
    let lines = response
        .notifications
        .iter()
        .filter_map(|told| {
            let who = format!(
                "{} (@{})",
                one_line(&told.pilot.display_name, NAME_AT_MOST),
                told.pilot.handle
            );
            let flight = your_flight(told.flight.as_ref());
            let (about, line) = match told.kind {
                NotificationKind::Followed => {
                    (NotificationAbout::Followed, format!("{who} follows you"))
                }
                NotificationKind::Cheered => (
                    NotificationAbout::Cheered,
                    format!("{who} cheered {flight}"),
                ),
                NotificationKind::Commented => (
                    NotificationAbout::Commented,
                    format!("{who} commented on {flight}"),
                ),
                NotificationKind::Unknown => return None,
            };
            Some(NotificationLine {
                about,
                line,
                handle: told.pilot.handle.to_string(),
                flight_id: told.flight.as_ref().map(|flight| flight.id.to_string()),
                comment_id: told.comment.clone(),
                at_unix_s: told.at.timestamp(),
                age: age(told.at, now_unix_s),
            })
        })
        .collect();
    Ok(Notifications {
        lines,
        cursor: response.cursor,
        more: response.more,
        reset: response.reset,
        empty_line: "When a pilot follows you, cheers a flight of yours or writes under one, it \
                     shows here for 30 days."
            .to_owned(),
    })
}

// --- What is new -------------------------------------------------------------

/// The request for how much is new for the signed-in pilot, the same on
/// every device: notifications and fired alarms after the pilot's mark.
/// Send it when the app comes to the front; read the answer with
/// [`unread`].
#[uniffi::export]
pub fn unread_request() -> ServerRequest {
    bare("GET", api::ACCOUNT_UNREAD_PATH.to_owned(), true)
}

/// The request that the pilot saw the lists: up to the `cursor` of the
/// last answer of [`notifications_request`] and of the alarms' fetch that
/// the pilot was shown, each `None` for up to the newest. Send it once
/// the pilot opened the notifications; read the answer with [`unread`].
#[uniffi::export]
pub fn unread_seen_request(
    notifications_cursor: Option<String>,
    alarms_cursor: Option<String>,
) -> Result<ServerRequest, CoreError> {
    let request = api::SeenRequest {
        notifications_cursor: cursor(notifications_cursor),
        alarms_cursor: cursor(alarms_cursor),
    };
    json_request("PUT", api::ACCOUNT_UNREAD_PATH, &request, true)
}

/// How much is new for the pilot, for a badge.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct Unread {
    /// New notifications.
    pub notifications: u32,
    /// New lines of what the alarms fired.
    pub alarms: u32,
    /// The two together.
    pub total: u32,
    /// Messages of others in the pilot's groups not read yet: the groups'
    /// marker, beside this one.
    #[uniffi(default = 0)]
    pub groups: u32,
    /// The groups' badge: "3", "99+"; absent for none.
    #[uniffi(default = None)]
    pub groups_badge: Option<String>,
    /// What the badge says: "3", "99+"; absent when nothing is new and
    /// no badge is shown.
    pub badge: Option<String>,
    /// What a screen reader says of it: "3 new"; absent with the badge.
    pub label: Option<String>,
}

/// The badge of a count: none for nothing, the number up to 99, "99+"
/// beyond.
fn badge(count: u32) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".to_owned()),
    }
}

/// Reads the answer to [`unread_request`] and to [`unread_seen_request`].
#[uniffi::export]
pub fn unread(status: u16, body: Vec<u8>) -> Result<Unread, CoreError> {
    let response: api::UnreadResponse = answer(status, &body)?;
    let badge = badge(response.total);
    Ok(Unread {
        notifications: response.notifications,
        alarms: response.alarms,
        total: response.total,
        groups: response.groups,
        groups_badge: self::badge(response.groups),
        label: badge.as_ref().map(|badge| format!("{badge} new")),
        badge,
    })
}

// --- Words of the screens --------------------------------------------------

/// The words around following, cheers, comments and notifications that
/// no answer carries: the same in the three apps.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SocialWords {
    /// The feed's choice of everyone's public flights: "Everyone".
    #[uniffi(default = "")]
    pub feed_everyone: String,
    /// Its choice of those of the pilots one follows ([`feed_request`]):
    /// "Following".
    #[uniffi(default = "")]
    pub feed_following: String,
    /// The row and the title of [`notifications`]: "Notifications".
    #[uniffi(default = "")]
    pub notifications_title: String,
    /// The title of the sheet of comments under a flight: "Comments".
    #[uniffi(default = "")]
    pub comments_title: String,
    /// The comment field's placeholder: "Write a comment".
    #[uniffi(default = "")]
    pub comment_placeholder: String,
    /// The button that posts it ([`comment_request`]): "Send".
    #[uniffi(default = "")]
    pub comment_send: String,
    /// The action on a comment the pilot may remove
    /// ([`CommentRow::can_remove`], [`comment_remove_request`]): "Remove".
    #[uniffi(default = "")]
    pub comment_remove: String,
    /// The action that reports a comment ([`crate::report_form`]):
    /// "Report".
    #[uniffi(default = "")]
    pub comment_report: String,
    /// The title of the sheet of who cheered a flight ([`cheer_list`]):
    /// "Cheers".
    #[uniffi(default = "")]
    pub cheers_title: String,
    /// The title of the sheet of a pilot's followers ([`follow_list`] of
    /// [`followers_request`]): "Followers".
    #[uniffi(default = "")]
    pub followers_title: String,
    /// The title of the sheet of whom a pilot follows ([`follow_list`] of
    /// [`following_request`]): "Following".
    #[uniffi(default = "")]
    pub following_title: String,
    /// The feed's third choice, beside `feed_everyone` and
    /// `feed_following`: "Ranking".
    #[uniffi(default = "")]
    pub feed_ranking: String,
    /// The question before a comment is removed: "Remove this comment?"
    /// Its confirm is `comment_remove`, its cancel
    /// [`crate::CommonWords::cancel`].
    #[uniffi(default = "")]
    pub comment_remove_question: String,
    /// The title of the form that reports a comment: "Report this
    /// comment".
    #[uniffi(default = "")]
    pub comment_report_title: String,
    /// In place of the feed when it could not be loaded: "The flights
    /// could not be loaded." With the error's own words under it, if
    /// any, and [`crate::CommonWords::try_again`].
    #[uniffi(default = "")]
    pub feed_failed: String,
    /// The same for the notifications: "The notifications could not be
    /// loaded."
    #[uniffi(default = "")]
    pub notifications_failed: String,
}

/// The words around following, cheers, comments and notifications that no
/// answer carries.
#[uniffi::export]
pub fn social_words() -> SocialWords {
    SocialWords {
        feed_everyone: "Everyone".to_owned(),
        feed_following: "Following".to_owned(),
        notifications_title: "Notifications".to_owned(),
        comments_title: "Comments".to_owned(),
        comment_placeholder: "Write a comment".to_owned(),
        comment_send: "Send".to_owned(),
        comment_remove: "Remove".to_owned(),
        comment_report: "Report".to_owned(),
        cheers_title: "Cheers".to_owned(),
        followers_title: "Followers".to_owned(),
        following_title: "Following".to_owned(),
        feed_ranking: "Ranking".to_owned(),
        comment_remove_question: "Remove this comment?".to_owned(),
        comment_report_title: "Report this comment".to_owned(),
        feed_failed: "The flights could not be loaded.".to_owned(),
        notifications_failed: "The notifications could not be loaded.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLIGHT: &str = "5t1eshypdtvrq7rxw39a18vsrg";

    /// The server's recorded answers, which its own tests keep true.
    fn recorded(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    fn refused(code: &str) -> Vec<u8> {
        format!(r#"{{"error":{{"code":"{code}","message":"x"}}}}"#).into_bytes()
    }

    fn message(error: CoreError) -> String {
        match error {
            CoreError::Server { message } => message,
            other => panic!("expected words from the server, got {other:?}"),
        }
    }

    /// 18 November 2026, 19:05 UTC: the moment of the recorded data.
    const NOW: i64 = 1_795_028_700;

    #[test]
    fn the_requests_are_the_documented_ones() {
        let follow = follow_request("@timo".to_owned(), true).unwrap();
        assert_eq!(
            (
                follow.method.as_str(),
                follow.path.as_str(),
                follow.needs_token
            ),
            ("PUT", "/v1/pilots/timo/follow", true)
        );
        assert_eq!(follow.body, None);
        let leave = follow_request("timo".to_owned(), false).unwrap();
        assert_eq!(leave.method, "DELETE");
        assert!(follow_request("Not a handle".to_owned(), true).is_err());

        let followers = followers_request("demo".to_owned(), Some("k".to_owned()), false).unwrap();
        assert_eq!(followers.path, "/v1/pilots/demo/followers?after=k");
        assert!(!followers.needs_token);
        let following = following_request("demo".to_owned(), Some(String::new()), true).unwrap();
        assert_eq!(following.path, "/v1/pilots/demo/following");
        assert!(following.needs_token);
        assert_eq!(own_following_request(None).path, "/v1/account/following");

        let page = feed_request(Some("1_x".to_owned()));
        assert_eq!(
            (page.method.as_str(), page.path.as_str(), page.needs_token),
            ("GET", "/v1/account/feed?before=1_x", true)
        );

        let cheer = cheer_request(FLIGHT.to_owned(), true).unwrap();
        assert_eq!(
            (cheer.method.as_str(), cheer.path.as_str()),
            ("PUT", "/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/cheer")
        );
        assert_eq!(
            cheer_request(FLIGHT.to_owned(), false).unwrap().method,
            "DELETE"
        );
        assert!(cheer_request("nothing".to_owned(), true).is_err());
        let cheers = cheers_request(FLIGHT.to_owned(), None, true).unwrap();
        assert_eq!(cheers.path, format!("/v1/flights/{FLIGHT}/cheers"));

        let comments = comments_request(FLIGHT.to_owned(), None).unwrap();
        assert_eq!(comments.path, format!("/v1/flights/{FLIGHT}/comments"));
        assert!(!comments.needs_token);
        let post = comment_request(FLIGHT.to_owned(), "  Well flown!  ".to_owned()).unwrap();
        assert_eq!(post.method, "POST");
        assert_eq!(post.content_type.as_deref(), Some("application/json"));
        assert_eq!(
            post.body.as_deref(),
            Some(&br#"{"text":"Well flown!"}"#[..])
        );
        // The server's recorded request reads as the one the core makes.
        let text = "A made-up comment of a test.".to_owned();
        let same = comment_request(FLIGHT.to_owned(), text).unwrap();
        let theirs: serde_json::Value =
            serde_json::from_slice(&recorded("comment_request.json")).unwrap();
        let ours: serde_json::Value = serde_json::from_slice(&same.body.unwrap()).unwrap();
        assert_eq!(ours, theirs);
        assert_eq!(
            message(comment_request(FLIGHT.to_owned(), " \n ".to_owned()).unwrap_err()),
            "Write something first."
        );
        assert!(comment_request(FLIGHT.to_owned(), "x".repeat(501)).is_err());

        let remove =
            comment_remove_request(FLIGHT.to_owned(), "0000000000000000000000c001".to_owned())
                .unwrap();
        assert_eq!(
            (remove.method.as_str(), remove.path.as_str()),
            (
                "DELETE",
                "/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/comments/0000000000000000000000c001"
            )
        );
        let off = comments_open_request(FLIGHT.to_owned(), false).unwrap();
        assert_eq!(off.method, "PATCH");
        assert_eq!(off.body.as_deref(), Some(&br#"{"open":false}"#[..]));
        let theirs: serde_json::Value =
            serde_json::from_slice(&recorded("comments_open.json")).unwrap();
        let ours: serde_json::Value = serde_json::from_slice(&off.body.unwrap()).unwrap();
        assert_eq!(ours, theirs);

        assert_eq!(
            notifications_request(Some("7".to_owned())).path,
            "/v1/account/notifications?since=7"
        );
    }

    #[test]
    fn following_is_read_with_its_words() {
        assert_eq!(follow_button(false), "Follow");
        assert_eq!(follow_button(true), "Following");
        assert_eq!(followers_line(1), "1 follower");
        assert_eq!(followers_line(12), "12 followers");
        assert_eq!(following_line(1), "Follows 1 pilot");
        assert_eq!(following_line(0), "Follows 0 pilots");

        let state = follow_answer(200, recorded("follow.json")).unwrap();
        assert!(state.following);
        assert_eq!(state.button, "Following");
        assert_eq!(state.followers_line, followers_line(state.followers));

        let list = follow_list(200, recorded("followers.json")).unwrap();
        let handles: Vec<&str> = list
            .pilots
            .iter()
            .map(|pilot| pilot.handle.as_str())
            .collect();
        assert_eq!(handles, ["lena", "jonas", "mara"]);
        assert_eq!(list.pilots[0].name, "Lena Probe");
        assert_eq!(list.followers_line, "3 followers");
        assert_eq!(list.following_line, "Follows 3 pilots");
        // Asked without a session: nothing about the one who asks.
        assert_eq!((list.you_follow, list.button.clone()), (None, None));
        assert_eq!(list.next, None);

        assert_eq!(
            message(follow_list(404, refused("pilot_not_found")).unwrap_err()),
            "The server knows no pilot by this handle."
        );
        assert_eq!(
            message(follow_answer(400, refused("invalid_follow")).unwrap_err()),
            "You cannot follow yourself."
        );
        assert_eq!(
            message(follow_answer(409, refused("too_many_follows")).unwrap_err()),
            "You follow as many pilots as an account can. Stop following one first."
        );
    }

    #[test]
    fn the_feed_is_the_public_row_with_what_pilots_made_of_it() {
        let feed = feed(200, recorded("feed.json")).unwrap();
        assert_eq!(feed.rows.len(), 3);
        assert!(feed.next.is_some());
        let first = &feed.rows[0];
        assert_eq!(first.flight.handle, "jonas");
        assert_eq!(first.flight.pilot, "Jonas Beispiel");
        assert_eq!((first.cheers, first.cheered), (1, true));
        assert_eq!(first.cheers_line, "You cheered");
        assert_eq!(first.comments_line, "");
        let second = &feed.rows[1];
        assert_eq!(second.cheers_line, "You and 1 other cheered");
        assert_eq!(second.comments_line, "1 comment");
        assert_eq!(feed.rows[2].cheers_line, "");
        assert!(!feed.empty_line.is_empty());
    }

    #[test]
    fn cheers_are_said_in_words() {
        assert_eq!(cheers_line(0, false), "");
        assert_eq!(cheers_line(1, false), "1 pilot cheered");
        assert_eq!(cheers_line(3, false), "3 pilots cheered");
        assert_eq!(cheers_line(1, true), "You cheered");
        assert_eq!(cheers_line(3, true), "You and 2 others cheered");
        assert_eq!(cheer_button(false), "Cheer");
        assert_eq!(cheer_button(true), "Cheered");

        let state = cheer_answer(200, recorded("cheer.json")).unwrap();
        assert!(state.cheered);
        assert_eq!(state.button, "Cheered");
        assert_eq!(state.line, cheers_line(state.cheers, true));

        let list = cheer_list(200, recorded("cheers.json")).unwrap();
        assert_eq!((list.count, list.cheered), (2, None));
        assert_eq!(list.line, "2 pilots cheered");
        assert_eq!(list.button, "Cheer");
        assert_eq!(list.pilots[0].name, "Lena Probe");
        assert_eq!(
            message(cheer_answer(400, refused("invalid_cheer")).unwrap_err()),
            "You cannot cheer your own flight."
        );
    }

    #[test]
    fn comments_are_rows_with_their_age_and_who_may_remove_them() {
        let read = |own: Option<&str>, pilot: Option<&str>| {
            comment_list(
                200,
                recorded("comments.json"),
                NOW,
                own.map(str::to_owned),
                pilot.map(str::to_owned),
            )
            .unwrap()
        };
        let list = read(None, Some("demo"));
        assert_eq!((list.count, list.open), (2, true));
        assert_eq!(list.count_line, "2 comments");
        assert_eq!(list.closed_line, None);
        let first = &list.comments[0];
        assert_eq!(first.handle, "mara");
        assert_eq!(first.text, "Well flown. How was the air above the ridge?");
        assert_eq!(first.age, "55 min ago");
        assert_eq!(first.byline, "Mara Muster · 55 min ago");
        assert!(list.comments[1].text.contains('\n'));
        // Signed out nobody removes; the author their own; the flight's
        // pilot every one.
        assert!(list.comments.iter().all(|comment| !comment.can_remove));
        let for_mara = read(Some("mara"), Some("demo"));
        let may: Vec<bool> = for_mara.comments.iter().map(|c| c.can_remove).collect();
        assert_eq!(may, [true, false]);
        let for_demo = read(Some("demo"), Some("demo"));
        assert!(for_demo.comments.iter().all(|comment| comment.can_remove));

        let posted = comment_posted(201, recorded("comment.json"), NOW).unwrap();
        assert_eq!(posted.comment.text, "A made-up comment of a test.");
        assert_eq!(posted.comment.age, "just now");
        assert!(posted.comment.can_remove);
        assert_eq!(posted.count_line, comments_line(posted.count));

        let switch = comments_open_answer(200, recorded("comments_open.json")).unwrap();
        assert!(!switch.open);
        assert_eq!(
            switch.line,
            "Nobody can write under this flight. What was written stays."
        );
        assert!(comments_switch(true).line.contains("while it is public"));

        assert!(social_done(204, Vec::new()).is_ok());
        assert_eq!(
            message(social_done(404, refused("comment_not_found")).unwrap_err()),
            "This comment is gone, or is not yours to remove."
        );
        assert_eq!(
            message(comment_posted(403, refused("comments_off"), NOW).unwrap_err()),
            "The pilot has turned comments off for this flight."
        );
        assert_eq!(
            message(comment_posted(404, refused("flight_not_found"), NOW).unwrap_err()),
            "The server no longer has this flight."
        );
    }

    #[test]
    fn a_comment_is_checked_while_it_is_written() {
        let empty = comment_draft("  \n ".to_owned());
        assert_eq!((empty.left, empty.can_send), (500, false));
        assert_eq!(empty.fault, None);
        assert_eq!(empty.counter, "0 of 500");

        let fine = comment_draft("  Well flown!  ".to_owned());
        assert_eq!(fine.text, "Well flown!");
        assert_eq!((fine.left, fine.can_send), (489, true));
        assert_eq!(fine.counter, "11 of 500");
        assert!(fine.notice.contains("with your name"));

        let long = comment_draft("ä".repeat(503));
        assert_eq!((long.left, long.can_send), (-3, false));
        assert_eq!(
            long.fault.as_deref(),
            Some("A comment has room for 500 characters. Shorten it by 3.")
        );
    }

    #[test]
    fn what_is_new_is_a_badge() {
        let request = unread_request();
        assert_eq!(
            (request.method.as_str(), request.path.as_str()),
            ("GET", "/v1/account/unread")
        );
        assert!(request.needs_token);
        let found = unread(200, recorded("unread.json")).unwrap();
        assert_eq!((found.notifications, found.alarms, found.total), (4, 1, 5));
        assert_eq!(found.badge.as_deref(), Some("5"));
        assert_eq!(found.label.as_deref(), Some("5 new"));

        let none = unread(200, br#"{"notifications":0,"alarms":0,"total":0}"#.to_vec()).unwrap();
        assert_eq!((none.badge, none.label), (None, None));
        let many = unread(
            200,
            br#"{"notifications":100,"alarms":3,"total":100}"#.to_vec(),
        )
        .unwrap();
        assert_eq!(many.badge.as_deref(), Some("99+"));
        assert_eq!(many.label.as_deref(), Some("99+ new"));
        assert!(
            unread(
                401,
                br#"{"error":{"code":"unauthorized","message":"x"}}"#.to_vec()
            )
            .is_err()
        );

        let seen = unread_seen_request(Some("1795026000000000".to_owned()), None).unwrap();
        assert_eq!(
            (seen.method.as_str(), seen.path.as_str(), seen.needs_token),
            ("PUT", "/v1/account/unread", true)
        );
        let theirs: serde_json::Value =
            serde_json::from_slice(&recorded("seen_request.json")).unwrap();
        let ours: serde_json::Value = serde_json::from_slice(&seen.body.unwrap()).unwrap();
        assert_eq!(ours, theirs);
        let all = unread_seen_request(Some(String::new()), None).unwrap();
        assert_eq!(all.body.as_deref(), Some(&b"{}"[..]));
    }

    #[test]
    fn what_the_pilot_was_told_is_said_in_lines() {
        let told = notifications(200, recorded("notifications.json"), NOW).unwrap();
        let lines: Vec<&str> = told.lines.iter().map(|line| line.line.as_str()).collect();
        assert_eq!(
            lines,
            [
                "Lena Probe (@lena) follows you",
                "Mara Muster (@mara) cheered your flight of 15 Aug 2026",
                "Mara Muster (@mara) commented on your flight of 15 Aug 2026",
                "Lena Probe (@lena) cheered your flight of 15 Aug 2026",
            ]
        );
        assert_eq!(told.lines[0].about, NotificationAbout::Followed);
        assert_eq!(told.lines[0].flight_id, None);
        assert_eq!(told.lines[0].age, "1 h ago");
        assert_eq!(told.lines[2].flight_id.as_deref(), Some(FLIGHT));
        assert_eq!(
            told.lines[2].comment_id.as_deref(),
            Some("0000000000000000000000c001")
        );
        assert!(!told.more && !told.reset);
        assert!(!told.cursor.is_empty());

        // With the site the flight took off from, and a kind of a newer
        // server left out.
        let body = br#"{"notifications":[
            {"kind":"cheered","at":"2026-11-10T08:00:00Z",
             "pilot":{"handle":"lena","display_name":"Lena Probe"},
             "flight":{"id":"5t1eshypdtvrq7rxw39a18vsrg","start":"2026-08-15T10:30:00Z",
                       "takeoff_site":"Wallberg"}},
            {"kind":"mentioned","at":"2026-11-10T08:00:00Z",
             "pilot":{"handle":"lena","display_name":"Lena Probe"}}
          ],"cursor":"9","more":true,"reset":false}"#;
        let told = notifications(200, body.to_vec(), NOW).unwrap();
        assert_eq!(told.lines.len(), 1);
        assert_eq!(
            told.lines[0].line,
            "Lena Probe (@lena) cheered your flight from Wallberg"
        );
        assert_eq!(told.lines[0].age, "10 Nov 2026");
        assert!(told.more);
    }

    #[test]
    fn an_age_is_said_as_a_pilot_reads_it() {
        let at = |seconds_ago: i64| chrono::DateTime::from_timestamp(NOW - seconds_ago, 0).unwrap();
        // No server clock read yet: the day, never an age by another clock.
        assert_eq!(age(at(0), 0), day_in_words(at(0)));
        assert_eq!(age(at(0), NOW), "just now");
        assert_eq!(age(at(-30), NOW), "just now");
        assert_eq!(age(at(59), NOW), "just now");
        assert_eq!(age(at(60), NOW), "1 min ago");
        assert_eq!(age(at(3_599), NOW), "59 min ago");
        assert_eq!(age(at(3_600), NOW), "1 h ago");
        assert_eq!(age(at(86_400), NOW), "1 d ago");
        assert_eq!(age(at(6 * 86_400), NOW), "6 d ago");
        assert_eq!(age(at(7 * 86_400), NOW), "11 Nov 2026");
    }

    #[test]
    fn removing_and_reporting_a_comment_and_a_failed_feed_have_their_words() {
        let words = social_words();
        assert_eq!(
            [
                words.feed_everyone.as_str(),
                words.feed_following.as_str(),
                words.feed_ranking.as_str()
            ],
            ["Everyone", "Following", "Ranking"]
        );
        assert_eq!(words.comment_remove_question, "Remove this comment?");
        assert_eq!(words.comment_remove, "Remove");
        assert_eq!(words.comment_report_title, "Report this comment");
        assert!(words.feed_failed.ends_with("could not be loaded."));
        assert!(words.notifications_failed.ends_with("could not be loaded."));
    }
}
