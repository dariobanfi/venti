//! Groups as communities with a chat (owner decision 26): public groups
//! anyone signed in finds, reads and joins; private groups whose owner and
//! admins invite pilots by handle; a home area each, by which the groups
//! near a pilot are found; and each group a chat of text, pictures and
//! links to Venti's records, which the answers turn into cards.
//!
//! **Messages are conversation, not data.** They are read through these
//! endpoints by a pilot signed in to Venti's own apps, and through no
//! public read of the API, no dump and no tool of the connector.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use venti_domain::{GeoPoint, Timestamp};

use super::{FlightId, Group, GroupId, GroupTextError, Handle, Pilot, Plan, text};

/// `GET /v1/groups/find`: public groups near a place and by name.
pub const GROUPS_FIND_PATH: &str = "/v1/groups/find";

/// `GET /v1/groups/invites`: the pilot's invites into private groups.
pub const GROUP_INVITES_PATH: &str = "/v1/groups/invites";

/// Where the pictures sent to groups are served, followed by `/{id}`.
pub const GROUP_PICTURES_PATH: &str = "/v1/groups/pictures";

/// The path of a picture sent to a group.
pub fn group_picture_path(id: &str) -> String {
    format!("{GROUP_PICTURES_PATH}/{id}")
}

/// Path that takes the pilot into a public group, or one they were
/// invited into.
pub fn group_members_path(group: &GroupId) -> String {
    format!("/v1/groups/{group}/members")
}

/// Path that invites a pilot into a group.
pub fn group_invites_path(group: &GroupId) -> String {
    format!("/v1/groups/{group}/invites")
}

/// Path of one invite: taking it back, declining it.
pub fn group_invite_of_path(group: &GroupId, handle: &Handle) -> String {
    format!("/v1/groups/{group}/invites/{handle}")
}

/// Path that makes a member an admin (`PUT`) or a member again (`DELETE`).
pub fn group_admin_path(group: &GroupId, handle: &Handle) -> String {
    format!("/v1/groups/{group}/admins/{handle}")
}

/// Path of a group's own picture.
pub fn group_own_picture_path(group: &GroupId) -> String {
    format!("/v1/groups/{group}/picture")
}

/// Path that sends a picture for a message.
pub fn group_send_picture_path(group: &GroupId) -> String {
    format!("/v1/groups/{group}/pictures")
}

/// Path of a group's messages.
pub fn group_messages_path(group: &GroupId) -> String {
    format!("/v1/groups/{group}/messages")
}

/// Path of one message: removing it.
pub fn group_message_path(group: &GroupId, message: &str) -> String {
    format!("/v1/groups/{group}/messages/{message}")
}

/// Path of how the pilot stands to a group: muting it, marking it read.
pub fn group_me_path(group: &GroupId) -> String {
    format!("/v1/groups/{group}/me")
}

/// The most characters a group's description may have.
pub const MAX_GROUP_DESCRIPTION_LENGTH: usize = 300;

/// The most characters the place of a group's home may have.
pub const MAX_HOME_PLACE_LENGTH: usize = 80;

/// The smallest radius of a group's home, kilometres.
pub const MIN_HOME_RADIUS_KM: f64 = 1.0;

/// The largest radius of a group's home, kilometres.
pub const MAX_HOME_RADIUS_KM: f64 = 100.0;

/// How far from a place the groups near it are looked for, kilometres:
/// the middle of their home within this.
pub const GROUPS_NEAR_KM: f64 = 150.0;

/// How many groups a search gives unless it asks for fewer.
pub const DEFAULT_FOUND_GROUPS: usize = 20;

/// The most groups a search gives.
pub const MAX_FOUND_GROUPS: usize = 50;

/// The most characters a message's text may have.
pub const MAX_MESSAGE_LENGTH: usize = 2000;

/// The most pictures one message may carry.
pub const MAX_MESSAGE_PICTURES: usize = 4;

/// The most links one message may carry, each shown as a card.
pub const MAX_MESSAGE_LINKS: usize = 4;

/// How many messages a page has unless it asks for fewer.
pub const DEFAULT_MESSAGES: usize = 50;

/// The most messages a page has.
pub const MAX_MESSAGES: usize = 100;

/// How often an app asks what is new while a chat is open, seconds. The
/// server pushes nothing to an open screen; "what is new since" is one
/// cheap read.
pub const MESSAGES_POLL_SECONDS: u32 = 5;

/// The longest side of a picture sent with a message, pixels: larger ones
/// are scaled down to it.
pub const MAX_MESSAGE_PICTURE_SIDE: u32 = 1600;

/// The most points of a flight card's track.
pub const MAX_CARD_TRACK_POINTS: usize = 48;

/// The most open invites one group may have.
pub const MAX_GROUP_INVITES: u32 = 200;

/// Makes a group's description fit to keep: line breaks stay, `None` when
/// nothing is left; refused beyond [`MAX_GROUP_DESCRIPTION_LENGTH`]
/// characters.
pub fn parse_group_description(text: &str) -> Result<Option<String>, GroupTextError> {
    let kept = text::lines(text);
    if kept.chars().count() > MAX_GROUP_DESCRIPTION_LENGTH {
        return Err(GroupTextError::TooLong(MAX_GROUP_DESCRIPTION_LENGTH));
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// Makes the place of a home fit to keep: one line, `None` when nothing is
/// left; refused beyond [`MAX_HOME_PLACE_LENGTH`] characters.
pub fn parse_home_place(text: &str) -> Result<Option<String>, GroupTextError> {
    let kept = text::line(text);
    if kept.chars().count() > MAX_HOME_PLACE_LENGTH {
        return Err(GroupTextError::TooLong(MAX_HOME_PLACE_LENGTH));
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// Makes a message's text fit to keep: line breaks stay, empty when
/// nothing is left; refused beyond [`MAX_MESSAGE_LENGTH`] characters.
pub fn parse_message_text(text: &str) -> Result<String, GroupTextError> {
    let kept = text::lines(text);
    if kept.chars().count() > MAX_MESSAGE_LENGTH {
        return Err(GroupTextError::TooLong(MAX_MESSAGE_LENGTH));
    }
    Ok(kept)
}

/// The handles a message's text mentions: `@` and a handle, after a space,
/// a line break or the start, lower case, each once, in order.
pub fn mentions(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut previous = ' ';
    for (at, c) in text.char_indices() {
        if c == '@' && (previous.is_whitespace() || previous == '(') {
            let rest = &text[at + 1..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(rest.len());
            if let Ok(handle) = Handle::new(&rest[..end]) {
                let handle = handle.as_str().to_owned();
                if !found.contains(&handle) {
                    found.push(handle);
                }
            }
        }
        previous = c;
    }
    found
}

/// What a phone is told of a new message: the group's name as the title,
/// and who wrote what as the line, cut to a phone's line.
pub fn group_message_words(
    group: &str,
    author: &str,
    text: &str,
    pictures: usize,
) -> (String, String) {
    const MOST: usize = 140;
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let said = if line.is_empty() {
        match pictures {
            0 => "sent a link".to_owned(),
            1 => "sent a picture".to_owned(),
            n => format!("sent {n} pictures"),
        }
    } else if line.chars().count() > MOST {
        let cut: String = line.chars().take(MOST - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        line
    };
    let body = if text.trim().is_empty() {
        format!("{author} {said}")
    } else {
        format!("{author}: {said}")
    };
    (group.to_owned(), body)
}

/// Where a group flies: the middle of its area, how far around it, and the
/// place's name in the owner's words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeArea {
    /// The middle of the area.
    pub center: GeoPoint,
    /// How far around it, kilometres, [`MIN_HOME_RADIUS_KM`] to
    /// [`MAX_HOME_RADIUS_KM`].
    pub radius_km: f64,
    /// The place: a valley, a mountain, a region. Text a pilot typed. A
    /// search by place finds the group by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
}

/// What an account is in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    /// The owner: the group's `keeper`. Makes and unmakes admins, hands
    /// the group on, makes it public or private and deletes it.
    Owner,
    /// An admin: invites, changes the group's name, description, picture
    /// and home, removes members who are no admins, removes messages.
    Admin,
    /// A member: posts, removes their own messages, leaves.
    Member,
}

/// How the pilot who asks stands to a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMe {
    /// Their role; left out for a pilot who is not in the group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<GroupRole>,
    /// Whether they were invited into it and are not in it yet.
    pub invited: bool,
    /// Whether its messages are not pushed to their phones.
    pub muted: bool,
    /// How many messages of others they have not read, up to
    /// [`UNREAD_COUNTED_UP_TO`](super::UNREAD_COUNTED_UP_TO).
    pub unread: u32,
    /// The cursor of the last message they read; `0` for none.
    pub read_up_to: String,
}

/// One of the pilot's groups in `GET /v1/groups`, beside the group in
/// [`GroupsResponse::groups`](super::GroupsResponse::groups) at the same
/// place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupChat {
    /// The group.
    pub group: GroupId,
    /// How the pilot stands to it.
    pub me: GroupMe,
    /// Its newest message, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<Message>,
}

/// A public group found by `GET /v1/groups/find`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoundGroup {
    /// The group.
    pub group: Group,
    /// How far the middle of its home is from the place asked about,
    /// kilometres. Left out when no place was asked about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    /// Whether the pilot who asks is in it.
    pub member: bool,
}

/// The answer of `GET /v1/groups/find`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupsFoundResponse {
    /// The groups: the nearest first when a place was given, then those
    /// with more members, then by name.
    pub groups: Vec<FoundGroup>,
}

/// The body of `POST /v1/groups/{id}/invites`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteRequest {
    /// The handle of the pilot to invite.
    pub handle: String,
}

/// A pilot's invite into a private group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupInvite {
    /// The group.
    pub group: Group,
    /// Who invited them.
    pub invited_by: Pilot,
    /// When.
    pub invited_at: Timestamp,
}

/// The answer of `POST /v1/groups/{id}/invites`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupInviteAnswer {
    /// The invite, as the invited pilot sees it.
    pub invite: GroupInvite,
}

/// The answer of `GET /v1/groups/invites`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupInvitesResponse {
    /// The pilot's invites, the newest first.
    pub invites: Vec<GroupInvite>,
}

/// A picture sent to a group, ready for a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentPicture {
    /// Its identifier, which a message names.
    pub id: String,
    /// Where it is served: [`group_picture_path`].
    pub path: String,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
}

/// The answer of `POST /v1/groups/{id}/pictures`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentPictureResponse {
    /// The picture.
    pub picture: SentPicture,
}

/// What a link in a message names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// A flight, by its address.
    Flight,
    /// A site, by its identifier.
    Site,
    /// An open plan, by its identifier.
    OpenPlan,
    /// A pilot, by handle.
    Pilot,
}

/// A link to one of Venti's records in a message.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Link {
    /// What it names.
    pub kind: LinkKind,
    /// The flight's address, the site's or open plan's identifier, or the
    /// pilot's handle.
    pub id: String,
}

/// The body of `POST /v1/groups/{id}/messages`. A message has text, a
/// picture or a link, or several of them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRequest {
    /// Its text; see [`parse_message_text`]. `@handle` mentions a member.
    #[serde(default)]
    pub text: String,
    /// Pictures the pilot sent to the group with
    /// `POST /v1/groups/{id}/pictures`, at most [`MAX_MESSAGE_PICTURES`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pictures: Vec<String>,
    /// Links to Venti's records, at most [`MAX_MESSAGE_LINKS`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
}

/// What a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// Text, pictures and links.
    Text,
    /// A plan of the group: [`Message::plan`], with "I'm in" on its card.
    Plan,
}

/// Who removed a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovedBy {
    /// Its author.
    Author,
    /// The owner or an admin of the group.
    Admin,
    /// Whoever runs the server.
    Operator,
}

/// A removed message's mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    /// When.
    pub at: Timestamp,
    /// By whom.
    pub by: RemovedBy,
}

/// A picture of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePicture {
    /// Where it is served: [`group_picture_path`].
    pub path: String,
    /// Its identifier, for a report.
    pub id: String,
}

/// A flight, as a card shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightCard {
    /// Its address.
    pub id: FlightId,
    /// Its pilot.
    pub pilot: Pilot,
    /// The name of the site it took off from, if one stands there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takeoff: Option<String>,
    /// The scored distance, or the distance along the track for a flight
    /// too short to score, kilometres.
    pub distance_km: f64,
    /// The day it was flown, in UTC.
    pub date: NaiveDate,
    /// Its track as at most [`MAX_CARD_TRACK_POINTS`] points, `[lat, lon]`:
    /// enough to draw a small line.
    pub track: Vec<[f64; 2]>,
}

/// A site, as a card shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteCard {
    /// Its identifier.
    pub id: String,
    /// Its name, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What it is for: `takeoff`, `landing`, `towing`, `training`, or
    /// `site` when it says nothing.
    pub site_use: String,
    /// Where it is, for directions.
    pub position: GeoPoint,
    /// Its height, metres above sea level, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_m: Option<f64>,
}

/// An open plan, as a card shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenPlanCard {
    /// Its identifier.
    pub id: String,
    /// The place.
    pub place: String,
    /// The day.
    pub day: NaiveDate,
    /// Where it is.
    pub position: GeoPoint,
    /// Who posted it.
    pub author: Pilot,
    /// How many pilots are in.
    pub pilots_in: u32,
    /// Whether the pilot who reads it is in.
    pub i_am_in: bool,
}

/// What a link in a message is shown as.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Card {
    /// A flight that is public or shared by link.
    Flight(FlightCard),
    /// A site.
    Site(SiteCard),
    /// An open plan.
    OpenPlan(OpenPlanCard),
    /// A pilot with a page.
    Pilot {
        /// The pilot.
        pilot: Pilot,
    },
    /// What the link named is gone, or is not shown to the pilot who
    /// reads it: a flight made private, a plan of a day gone by.
    Gone {
        /// What the link named.
        link: Link,
    },
}

/// A message of a group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Its identifier.
    pub id: String,
    /// The group.
    pub group: GroupId,
    /// Its place in the chat, for `before`: opaque.
    pub cursor: String,
    /// Who wrote it.
    pub author: Pilot,
    /// What it is.
    pub kind: MessageKind,
    /// Its text: show it as text, never as markup. Empty for none and once
    /// removed.
    pub text: String,
    /// Its pictures.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pictures: Vec<MessagePicture>,
    /// Its links, each as a card, in the order they were sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<Card>,
    /// For a message of kind plan: the plan, while it is there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    /// When it was posted.
    pub created_at: Timestamp,
    /// When and by whom it was removed. A removed message shows as such.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<Removed>,
}

/// The answer of `POST /v1/groups/{id}/messages` and of
/// `DELETE /v1/groups/{id}/messages/{message}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageResponse {
    /// The message as it now is.
    pub message: Message,
}

/// The answer of `GET /v1/groups/{id}/messages`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessagesResponse {
    /// The messages: the newest first for a page asked for `before`, or
    /// from the newest; in the order they were posted or changed for
    /// `since`.
    pub messages: Vec<Message>,
    /// `before` for the next older page; left out at the first message.
    /// Only a page asked for without `since` has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older: Option<String>,
    /// `since` for asking what is new after this answer.
    pub since: String,
    /// For `since`: whether more is new than this answer holds. Ask again
    /// with its `since`.
    pub more: bool,
}

/// The body of `PATCH /v1/groups/{id}/me`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMeRequest {
    /// Whether the group's messages are not pushed to the pilot's phones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// The cursor of the last message the pilot was shown. The mark only
    /// moves forward.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_up_to: Option<String>,
}

/// The answer of `PATCH /v1/groups/{id}/me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMeResponse {
    /// How the pilot stands to the group now.
    pub me: GroupMe,
}

/// A message of a group, as a report names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportedMessage {
    /// The group's identifier.
    pub group: String,
    /// The message's identifier, or a picture's.
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_are_handles_after_an_at_sign_once_each() {
        assert_eq!(
            mentions("@mara and @Jonas, see you. mail@x.de (@mara)"),
            ["mara", "jonas"]
        );
        assert!(mentions("no one@here").is_empty());
        assert!(mentions("@ x").is_empty());
    }

    #[test]
    fn a_message_is_bounded_and_keeps_its_lines() {
        assert_eq!(parse_message_text(" Hi\r\nall "), Ok("Hi\nall".to_owned()));
        assert_eq!(parse_message_text("  "), Ok(String::new()));
        let long = "ä".repeat(MAX_MESSAGE_LENGTH + 1);
        assert_eq!(
            parse_message_text(&long),
            Err(GroupTextError::TooLong(MAX_MESSAGE_LENGTH))
        );
        assert_eq!(parse_home_place(" Inntal "), Ok(Some("Inntal".to_owned())));
        assert_eq!(parse_group_description(""), Ok(None));
    }

    #[test]
    fn a_phone_is_told_who_wrote_what_in_one_short_line() {
        let (title, body) =
            group_message_words("Rosenheimer Eagles", "Mara", "Who flies\ntoday?", 0);
        assert_eq!(title, "Rosenheimer Eagles");
        assert_eq!(body, "Mara: Who flies today?");
        let (_, body) = group_message_words("G", "Mara", "", 2);
        assert_eq!(body, "Mara sent 2 pictures");
        let (_, body) = group_message_words("G", "Mara", &"x".repeat(500), 0);
        assert!(body.chars().count() < 160, "{body}");
    }

    #[test]
    fn a_card_says_its_kind() {
        let gone = Card::Gone {
            link: Link {
                kind: LinkKind::OpenPlan,
                id: "x".to_owned(),
            },
        };
        let json = serde_json::to_value(&gone).unwrap();
        assert_eq!(json["kind"], "gone");
        assert_eq!(json["link"]["kind"], "open_plan");
    }
}
