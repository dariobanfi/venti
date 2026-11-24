//! Groups as communities with a chat (owner decision 26), on the pilot's
//! Venti server: finding public groups near a place and by name, coming
//! in, invites by handle, admins, each group's messages with their
//! pictures and the cards their links are shown as, muting, and what is
//! unread.
//!
//! The core puts the requests together and refuses before sending what
//! the server would refuse (a message's length, how many pictures and
//! links, a home's radius), reads the answers into what a screen shows,
//! and has the words. It finds Venti's own links in a message's text, so
//! that a pasted link to a flight becomes a card. Nothing of a chat is
//! kept on the device.
//!
//! While a chat is open, an app asks what is new every
//! [`MESSAGES_POLL_SECONDS`](api::MESSAGES_POLL_SECONDS) seconds with
//! [`Core::messages_since_request`]: one cheap read; the server pushes
//! nothing to an open screen.

use venti_api_types::v1::{
    self as api, Card, FoundGroup, Group, GroupInvite, GroupInvitesResponse, GroupMe,
    GroupMeRequest, GroupMeResponse, GroupResponse, GroupRole, GroupTextError, GroupsFoundResponse,
    GroupsResponse, Handle, HomeArea, InviteRequest, Link, LinkKind, MessageRequest,
    MessageResponse, MessagesResponse, SentPictureResponse, UpdateGroupRequest,
};
use venti_domain::GeoPoint;

use crate::account::json_request;
use crate::groups::{PlanView, bare, group_id, group_name, invalid, plan_view};
use crate::server::answer;
use crate::{Coordinate, Core, CoreError, ServerRequest};

/// What the pilot of this device is in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum CommunityRole {
    /// Not in it.
    None,
    /// Invited into it, and not in it yet.
    Invited,
    /// A member.
    Member,
    /// An admin.
    Admin,
    /// Its owner.
    Owner,
}

/// A group, as the groups' section shows it: a card with its picture, its
/// place, how many members and the last message.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityView {
    /// Its identifier on the server.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its description, if it has one.
    pub description: Option<String>,
    /// The path of its picture on the server, if it has one.
    pub picture_path: Option<String>,
    /// Whether anyone finds and joins it.
    pub is_public: bool,
    /// The place of its home, if it has one.
    pub place: Option<String>,
    /// The middle of its home, if it has one.
    pub center: Option<Coordinate>,
    /// The radius of its home, kilometres.
    pub radius_km: Option<f64>,
    /// How many members it has.
    pub members: u32,
    /// In a line: "12 members · Rosenheim and the Chiemgau".
    pub summary: String,
    /// "Public" or "Private".
    pub kind_words: String,
    /// What the pilot of this device is in it.
    pub my_role: CommunityRole,
    /// Whether its messages are not pushed to the pilot's phones.
    pub muted: bool,
    /// How many messages of others the pilot has not read.
    pub unread: u32,
    /// The badge of that: "3", "99+"; absent for none.
    pub unread_badge: Option<String>,
    /// The last message in a line: "Mara Muster: Who flies today?".
    pub last_message: Option<String>,
    /// When the last message was posted, RFC 3339.
    pub last_message_at: Option<String>,
}

/// A group found near a place or by name.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FoundCommunity {
    /// The group.
    pub group: CommunityView,
    /// How far its home is: "12 km"; absent without a place.
    pub distance: Option<String>,
    /// Whether the pilot is in it.
    pub is_member: bool,
}

/// One member of a group.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityMember {
    /// The member's handle.
    pub handle: String,
    /// The name the member is shown by.
    pub name: String,
    /// What the member is in the group.
    pub role: CommunityRole,
    /// "Owner", "Admin" or empty for a member.
    pub role_words: String,
    /// Whether this is the pilot of this device.
    pub is_me: bool,
}

/// A group with its members, and what the pilot may do in it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityDetail {
    /// The group.
    pub group: CommunityView,
    /// Its members, the one in it longest first.
    pub members: Vec<CommunityMember>,
    /// Whether the pilot may come in: a public group, or one they were
    /// invited into, that they are not in.
    pub can_join: bool,
    /// Whether the pilot reads its messages.
    pub can_read: bool,
    /// Whether the pilot posts in it.
    pub can_post: bool,
    /// Whether the pilot invites, changes the group and removes members
    /// and messages: its owner and admins.
    pub can_moderate: bool,
}

/// A link in a message that the core found or the pilot chose.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LinkDraft {
    /// `flight`, `site`, `open_plan` or `pilot`.
    pub kind: String,
    /// The flight's address, the site's or the open plan's identifier, or
    /// the pilot's handle.
    pub id: String,
}

/// A message as the pilot writes it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct MessageDraft {
    /// Its text. Venti's own links in it become cards by themselves.
    pub text: String,
    /// The identifiers of pictures sent with
    /// [`Core::send_group_picture_request`].
    pub pictures: Vec<String>,
    /// Links the pilot chose beside those in the text.
    pub links: Vec<LinkDraft>,
}

/// What a link in a message is shown as. `kind` says which of the fields
/// are there: `flight`, `site`, `open_plan`, `pilot`, or `gone` for what
/// is gone or not shown to the pilot.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CardView {
    /// What it is.
    pub kind: String,
    /// The identifier to open it by: a flight's address, a site's or an
    /// open plan's identifier, a pilot's handle.
    pub id: String,
    /// The first line: the pilot of a flight, a site's name, a plan's
    /// place, a pilot's name, or what is gone.
    pub title: String,
    /// The second line: "12.0 km · 16 Aug 2026 · from Wallberg", a site's
    /// use, a plan's day and who is in.
    pub subtitle: String,
    /// A flight's track as a small line.
    pub track: Vec<Coordinate>,
    /// Where a site or an open plan is, for directions.
    pub position: Option<Coordinate>,
    /// A pilot's picture's path, if they have one.
    pub picture_path: Option<String>,
    /// For an open plan: whether the pilot is in.
    pub am_in: bool,
}

/// A message, as a chat shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ChatMessage {
    /// Its identifier.
    pub id: String,
    /// Its place in the chat, for [`Core::group_read_request`].
    pub cursor: String,
    /// The author's handle.
    pub author_handle: String,
    /// The author's name.
    pub author_name: String,
    /// Whether the pilot of this device wrote it.
    pub is_mine: bool,
    /// Its text: show it as text, never as markup.
    pub text: String,
    /// The paths of its pictures.
    pub pictures: Vec<String>,
    /// The identifiers of its pictures, for a report.
    pub picture_ids: Vec<String>,
    /// Its cards, in the order of its links.
    pub cards: Vec<CardView>,
    /// For a plan of the group: the plan, with "I'm in".
    pub plan: Option<PlanView>,
    /// When it was posted, RFC 3339.
    pub posted_at: String,
    /// For a removed message: "Removed by its author" or "Removed by an
    /// admin"; absent otherwise.
    pub removed_words: Option<String>,
}

/// A page of a chat.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ChatPage {
    /// The messages: the newest first for a page, in the order they came
    /// or changed for what is new. A message already shown is shown as
    /// it is here.
    pub messages: Vec<ChatMessage>,
    /// What to hand [`Core::messages_request`] for the page before;
    /// absent at the first message.
    pub older: Option<String>,
    /// What to hand [`Core::messages_since_request`] next.
    pub since: String,
    /// Whether more is new: ask again at once.
    pub more: bool,
}

/// A pilot's invite into a private group.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityInvite {
    /// The group.
    pub group: CommunityView,
    /// In a line: "Mara Muster invites you to Rosenheimer Eagles".
    pub words: String,
}

/// How the pilot stands to a group after muting it or marking it read.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityStanding {
    /// Whether its messages are not pushed to the pilot's phones.
    pub muted: bool,
    /// How many messages of others the pilot has not read.
    pub unread: u32,
}

/// A picture sent to a group, ready for a message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SentGroupPicture {
    /// Its identifier, for [`MessageDraft::pictures`].
    pub id: String,
    /// Its path on the server.
    pub path: String,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
}

/// A group's home as the owner sets it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct HomeDraft {
    /// The middle of the area.
    pub center: Coordinate,
    /// How far around it, kilometres: 1 to 100.
    pub radius_km: f64,
    /// The place in a few words: "Inn valley". May be empty.
    pub place: String,
}

/// A group as its owner makes it or an admin changes it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityDraft {
    /// Its name.
    pub name: String,
    /// A short description. May be empty.
    pub description: String,
    /// Where it flies.
    pub home: Option<HomeDraft>,
}

/// The words of the groups' screens that no answer carries.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommunityWords {
    /// The section's title: "Groups".
    pub title: String,
    /// The list near the pilot: "Near you".
    pub near_you: String,
    /// The pilot's own: "Your groups".
    pub yours: String,
    /// The search field: "Search by name or place".
    pub search: String,
    /// Nothing found: "No group here yet. Make one?".
    pub none_found: String,
    /// "Join".
    pub join: String,
    /// "Leave".
    pub leave: String,
    /// "Invite".
    pub invite: String,
    /// "Mute" and "Unmute".
    pub mute: String,
    /// "Unmute".
    pub unmute: String,
    /// The field of a message: "Message".
    pub message_placeholder: String,
    /// "Send".
    pub send: String,
    /// An empty chat: "No messages yet. Say hello.".
    pub empty_chat: String,
    /// "Public": anyone finds and joins it.
    pub public: String,
    /// "Private": by invite.
    pub private: String,
    /// "I'm in".
    pub im_in: String,
}

/// The words of the groups' screens.
#[uniffi::export]
pub fn community_words() -> CommunityWords {
    CommunityWords {
        title: "Groups".to_owned(),
        near_you: "Near you".to_owned(),
        yours: "Your groups".to_owned(),
        search: "Search by name or place".to_owned(),
        none_found: "No group here yet. Make one?".to_owned(),
        join: "Join".to_owned(),
        leave: "Leave".to_owned(),
        invite: "Invite".to_owned(),
        mute: "Mute".to_owned(),
        unmute: "Unmute".to_owned(),
        message_placeholder: "Message".to_owned(),
        send: "Send".to_owned(),
        empty_chat: "No messages yet. Say hello.".to_owned(),
        public: "Public".to_owned(),
        private: "Private".to_owned(),
        im_in: "I'm in".to_owned(),
    }
}

/// The links to Venti's own records in a message's text, in the order
/// they stand, each once: an address of a Venti page, on any host and
/// under any folder, with or without `https://`. The pages are those of
/// the API's records (`/flights/{id}`, `/sites/{id}`, `/open-plans/{id}`,
/// `/pilots/{handle}`) and the website's own: a site's place
/// (`/places/{lat},{lon}/?site={id}`, and the older
/// `/logbook/everyone/place/?lat=…&lon=…&site={id}`), an open plan
/// (`/plans/open/?id={id}`), and the older addresses of a flight and a
/// pilot (`/logbook/everyone/flight/?id=…`, `/logbook/everyone/pilot/?handle=…`).
#[uniffi::export]
pub fn message_links(text: String) -> Vec<LinkDraft> {
    let mut found: Vec<LinkDraft> = Vec::new();
    for word in text.split_whitespace() {
        let word =
            word.trim_matches(|c: char| matches!(c, '(' | ')' | '<' | '>' | ',' | '.' | '!' | '?'));
        let address = word
            .strip_prefix("https://")
            .or_else(|| word.strip_prefix("http://"))
            .and_then(|rest| rest.find('/').map(|at| &rest[at..]));
        let Some(link) = address.and_then(venti_link) else {
            continue;
        };
        if !found.contains(&link) {
            found.push(link);
        }
    }
    found
}

/// The record an address's path and query name, if it is one of Venti's
/// pages: see [`message_links`].
fn venti_link(address: &str) -> Option<LinkDraft> {
    let address = address.split('#').next().unwrap_or(address);
    let (path, query) = address.split_once('?').unwrap_or((address, ""));
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let asked = |name: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| decoded(value))
    };
    let (kind, id) = match parts.as_slice() {
        [.., "places", _] | [.., "logbook", "everyone", "place"] => ("site", asked("site")?),
        [.., "plans", "open"] => ("open_plan", asked("id")?),
        [.., "logbook", "everyone", "flight"] => ("flight", asked("id")?),
        [.., "logbook", "everyone", "pilot"] => ("pilot", asked("handle")?),
        [.., "flights" | "f", id] => ("flight", (*id).to_owned()),
        [.., "sites", id] => ("site", (*id).to_owned()),
        [.., "open-plans", id] => ("open_plan", (*id).to_owned()),
        [.., "pilots", id] => ("pilot", (*id).to_owned()),
        _ => return None,
    };
    let known = match kind {
        "flight" => id.parse::<api::FlightId>().is_ok(),
        "site" => id.parse::<venti_domain::SiteId>().is_ok(),
        "open_plan" => id.parse::<api::PlanId>().is_ok(),
        _ => Handle::new(&id).is_ok(),
    };
    known.then(|| LinkDraft {
        kind: kind.to_owned(),
        id,
    })
}

/// A value of a query string as it was before it was written into one:
/// `%3A` a colon, `+` a space. `None` for one that is not text.
fn decoded(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => {
                let hex = value.get(at + 1..at + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                at += 3;
            }
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    let text = String::from_utf8(out).ok()?;
    (!text.is_empty()).then_some(text)
}

#[uniffi::export]
impl Core {
    /// The request for the public groups near a place (nearest first,
    /// then those with more members), by name or place, or both. Without
    /// a place and without text: the largest public groups.
    pub fn find_groups_request(
        &self,
        near: Option<Coordinate>,
        text: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let mut query = Vec::new();
        if let Some(near) = near {
            GeoPoint::new(near.lat, near.lon)
                .map_err(|_| invalid("The place could not be read."))?;
            query.push(format!("lat={:.4}&lon={:.4}", near.lat, near.lon));
        }
        let text = text.trim();
        if !text.is_empty() {
            let text = api::parse_group_name(text).map_err(|_| {
                invalid(&format!(
                    "A search has room for {} characters.",
                    api::MAX_GROUP_NAME_LENGTH
                ))
            })?;
            query.push(format!("q={}", encode(&text)));
        }
        let path = if query.is_empty() {
            api::GROUPS_FIND_PATH.to_owned()
        } else {
            format!("{}?{}", api::GROUPS_FIND_PATH, query.join("&"))
        };
        Ok(bare("GET", path))
    }

    /// Reads the server's answer to [`Core::find_groups_request`].
    pub fn found_groups(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Vec<FoundCommunity>, CoreError> {
        let response: GroupsFoundResponse = answer(status, &body)?;
        Ok(response
            .groups
            .iter()
            .map(|found: &FoundGroup| FoundCommunity {
                group: community(&found.group, None, None),
                distance: found.distance_km.map(distance_words),
                is_member: found.member,
            })
            .collect())
    }

    /// Reads the server's answer to [`Core::groups_request`] as the
    /// groups' section shows it: each with its unread messages and its
    /// last message, the pilot's own as "You: …".
    pub fn communities(&self, status: u16, body: Vec<u8>) -> Result<Vec<CommunityView>, CoreError> {
        let response: GroupsResponse = answer(status, &body)?;
        let me = self.my_handle()?.unwrap_or_default();
        Ok(response
            .groups
            .iter()
            .map(|group| {
                let chat = response.chats.iter().find(|chat| chat.group == group.id);
                let last = chat
                    .and_then(|chat| chat.last_message.as_ref())
                    .map(|message| (message, me.as_str()));
                community(group, chat.map(|chat| &chat.me), last)
            })
            .collect())
    }

    /// The request that makes a group with the pilot as its owner.
    pub fn create_community_request(
        &self,
        draft: CommunityDraft,
        public: bool,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let request = api::CreateGroupRequest {
            name: group_name(&draft.name)?,
            public,
            description: description(&draft.description)?,
            home: draft.home.as_ref().map(home).transpose()?,
        };
        json_request("POST", api::GROUPS_PATH, &request, true)
    }

    /// Reads the server's answer to [`Core::create_community_request`]:
    /// the group as it was made, with the pilot as its owner and only
    /// member.
    pub fn community_made(&self, status: u16, body: Vec<u8>) -> Result<CommunityDetail, CoreError> {
        let response: api::CreateGroupResponse = answer(status, &body)?;
        let group = response.group;
        let made = GroupResponse {
            members: vec![group.keeper.clone()],
            admins: Vec::new(),
            me: Some(GroupMe {
                role: Some(GroupRole::Owner),
                invited: false,
                muted: false,
                unread: 0,
                read_up_to: "0".to_owned(),
            }),
            group,
        };
        self.community_detail(&made)
    }

    /// The request that changes a group's name, description and home. Its
    /// owner's and admins' to send; the answer goes to
    /// [`Core::community`].
    pub fn change_community_request(
        &self,
        id: String,
        draft: CommunityDraft,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let request = UpdateGroupRequest {
            name: Some(group_name(&draft.name)?),
            public: None,
            description: Some(description(&draft.description)?),
            home: Some(draft.home.as_ref().map(home).transpose()?),
        };
        json_request("PATCH", &api::group_path(&group_id(&id)?), &request, true)
    }

    /// The request that makes a group public or private. Its owner's to
    /// send.
    pub fn group_public_request(
        &self,
        id: String,
        public: bool,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let request = UpdateGroupRequest {
            public: Some(public),
            ..UpdateGroupRequest::default()
        };
        json_request("PATCH", &api::group_path(&group_id(&id)?), &request, true)
    }

    /// Reads a group with its members: the answer to
    /// [`Core::group_request`], to the changes of a group and of its
    /// picture.
    pub fn community(&self, status: u16, body: Vec<u8>) -> Result<CommunityDetail, CoreError> {
        let response: GroupResponse = answer(status, &body)?;
        self.community_detail(&response)
    }

    /// The request that takes the pilot into a public group, or one they
    /// were invited into. The answer goes to [`Core::group_joined`].
    pub fn enter_group_request(&self, id: String) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        Ok(bare("POST", api::group_members_path(&group_id(&id)?)))
    }

    /// The request that invites a pilot into a group by handle. The
    /// owner's and the admins' to send; the answer goes to
    /// [`Core::group_invite_sent`].
    pub fn invite_to_group_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let handle = pilot_handle(&handle)?;
        json_request(
            "POST",
            &api::group_invites_path(&group_id(&id)?),
            &InviteRequest {
                handle: handle.to_string(),
            },
            true,
        )
    }

    /// Reads the answer to [`Core::invite_to_group_request`]: a line to
    /// show.
    pub fn group_invite_sent(&self, status: u16, body: Vec<u8>) -> Result<String, CoreError> {
        let response: api::GroupInviteAnswer = answer(status, &body)?;
        Ok(format!("Invited to {}.", response.invite.group.name))
    }

    /// The request that takes an invite back (the owner, an admin) or
    /// declines it (the invited pilot, by their own handle). Its answer
    /// goes to [`Core::group_done`].
    pub fn drop_group_invite_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let handle = pilot_handle(&handle)?;
        Ok(bare(
            "DELETE",
            api::group_invite_of_path(&group_id(&id)?, &handle),
        ))
    }

    /// The request for the pilot's invites into private groups.
    pub fn group_invites_request(&self) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        Ok(bare("GET", api::GROUP_INVITES_PATH.to_owned()))
    }

    /// Reads the answer to [`Core::group_invites_request`].
    pub fn group_invites(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Vec<CommunityInvite>, CoreError> {
        let response: GroupInvitesResponse = answer(status, &body)?;
        Ok(response
            .invites
            .iter()
            .map(|invite: &GroupInvite| CommunityInvite {
                words: format!(
                    "{} invites you to {}",
                    invite.invited_by.display_name, invite.group.name
                ),
                group: community(&invite.group, None, None),
            })
            .collect())
    }

    /// The request that makes a member an admin (`admin`) or a member
    /// again. The owner's to send; the answer goes to
    /// [`Core::group_done`].
    pub fn group_admin_request(
        &self,
        id: String,
        handle: String,
        admin: bool,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let handle = pilot_handle(&handle)?;
        let method = if admin { "PUT" } else { "DELETE" };
        Ok(bare(
            method,
            api::group_admin_path(&group_id(&id)?, &handle),
        ))
    }

    /// The request for the newest messages of a group, or with `before`
    /// (a page's [`ChatPage::older`]) the page before.
    pub fn messages_request(
        &self,
        id: String,
        before: Option<String>,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let path = api::group_messages_path(&group_id(&id)?);
        Ok(bare(
            "GET",
            match before {
                Some(before) => format!("{path}?before={}", cursor(&before)?),
                None => path,
            },
        ))
    }

    /// The request for what is new in a group since a page's
    /// [`ChatPage::since`]: asked every few seconds while the chat is
    /// open.
    pub fn messages_since_request(
        &self,
        id: String,
        since: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let path = api::group_messages_path(&group_id(&id)?);
        Ok(bare("GET", format!("{path}?since={}", cursor(&since)?)))
    }

    /// Reads a page of messages: the answer to [`Core::messages_request`]
    /// and to [`Core::messages_since_request`].
    pub fn chat_page(&self, status: u16, body: Vec<u8>) -> Result<ChatPage, CoreError> {
        let response: MessagesResponse = answer(status, &body)?;
        let me = self.my_handle()?.unwrap_or_default();
        Ok(ChatPage {
            messages: response
                .messages
                .iter()
                .map(|message| chat_message(message, &me))
                .collect(),
            older: response.older,
            since: response.since,
            more: response.more,
        })
    }

    /// The request that posts a message. What the server would refuse is
    /// refused here first: an empty message, too long a text, more than
    /// four pictures or links. Venti's links in the text become cards.
    pub fn post_message_request(
        &self,
        id: String,
        draft: MessageDraft,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let request = message_request(&draft)?;
        json_request(
            "POST",
            &api::group_messages_path(&group_id(&id)?),
            &request,
            true,
        )
    }

    /// Reads one message: the answer to [`Core::post_message_request`]
    /// and to [`Core::remove_message_request`].
    pub fn chat_message(&self, status: u16, body: Vec<u8>) -> Result<ChatMessage, CoreError> {
        let response: MessageResponse = answer(status, &body)?;
        let me = self.my_handle()?.unwrap_or_default();
        Ok(chat_message(&response.message, &me))
    }

    /// The request that removes a message: the pilot's own, or as the
    /// owner or an admin anyone's.
    pub fn remove_message_request(
        &self,
        id: String,
        message: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let message = message.trim();
        if message.is_empty() || message.len() > 64 {
            return Err(invalid("This is not a message the app knows."));
        }
        Ok(bare(
            "DELETE",
            api::group_message_path(&group_id(&id)?, message),
        ))
    }

    /// The request that marks a group read up to a message's cursor.
    pub fn group_read_request(
        &self,
        id: String,
        cursor_of: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let request = GroupMeRequest {
            muted: None,
            read_up_to: Some(cursor(&cursor_of)?),
        };
        json_request(
            "PATCH",
            &api::group_me_path(&group_id(&id)?),
            &request,
            true,
        )
    }

    /// The request that mutes a group (`muted`) or unmutes it.
    pub fn group_mute_request(&self, id: String, muted: bool) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let request = GroupMeRequest {
            muted: Some(muted),
            read_up_to: None,
        };
        json_request(
            "PATCH",
            &api::group_me_path(&group_id(&id)?),
            &request,
            true,
        )
    }

    /// Reads the answer to [`Core::group_read_request`] and to
    /// [`Core::group_mute_request`].
    pub fn group_standing(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<CommunityStanding, CoreError> {
        let response: GroupMeResponse = answer(status, &body)?;
        Ok(CommunityStanding {
            muted: response.me.muted,
            unread: response.me.unread,
        })
    }

    /// The request that sends a picture for a message: a JPEG, a PNG or a
    /// WebP of at most 8 MB. The server makes its own copy, at most 1600
    /// pixels on its longer side, without the photo's metadata.
    pub fn send_group_picture_request(
        &self,
        id: String,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let mut request = crate::picture::picture_upload_request(picture, &media_type)?;
        request.method = "POST".to_owned();
        request.path = api::group_send_picture_path(&group_id(&id)?);
        Ok(request)
    }

    /// Reads the answer to [`Core::send_group_picture_request`].
    pub fn group_picture_sent(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<SentGroupPicture, CoreError> {
        let response: SentPictureResponse = answer(status, &body)?;
        Ok(SentGroupPicture {
            id: response.picture.id,
            path: response.picture.path,
            width: response.picture.width,
            height: response.picture.height,
        })
    }

    /// The request that gives a group its picture, cut square. The
    /// owner's and the admins' to send; the answer goes to
    /// [`Core::community`].
    pub fn group_own_picture_request(
        &self,
        id: String,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        let mut request = crate::picture::picture_upload_request(picture, &media_type)?;
        request.path = api::group_own_picture_path(&group_id(&id)?);
        Ok(request)
    }

    /// The request that removes a group's picture.
    pub fn remove_group_own_picture_request(&self, id: String) -> Result<ServerRequest, CoreError> {
        self.chat_me()?;
        Ok(bare("DELETE", api::group_own_picture_path(&group_id(&id)?)))
    }
}

impl Core {
    /// A group with its members and what the pilot may do in it, by the
    /// handle of the account: the pilot's own row is "me".
    fn community_detail(&self, response: &GroupResponse) -> Result<CommunityDetail, CoreError> {
        let me = self.my_handle()?.unwrap_or_default();
        let view = community(&response.group, response.me.as_ref(), None);
        let role = view.my_role;
        let members = response
            .members
            .iter()
            .map(|member| {
                let role = if member.handle == response.group.keeper.handle {
                    CommunityRole::Owner
                } else if response.admins.contains(&member.handle) {
                    CommunityRole::Admin
                } else {
                    CommunityRole::Member
                };
                CommunityMember {
                    handle: member.handle.to_string(),
                    name: member.display_name.clone(),
                    role_words: role_words(role).to_owned(),
                    role,
                    is_me: member.handle.as_str() == me,
                }
            })
            .collect();
        let is_in = matches!(
            role,
            CommunityRole::Member | CommunityRole::Admin | CommunityRole::Owner
        );
        Ok(CommunityDetail {
            can_join: !is_in && (view.is_public || role == CommunityRole::Invited),
            can_read: is_in || view.is_public,
            can_post: is_in,
            can_moderate: matches!(role, CommunityRole::Admin | CommunityRole::Owner),
            group: view,
            members,
        })
    }

    fn chat_me(&self) -> Result<String, CoreError> {
        self.my_handle()?.ok_or_else(crate::account::not_signed_in)
    }
}

/// Text for a query string: what is not a letter or digit as `%XX`.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn cursor(text: &str) -> Result<String, CoreError> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) || text.len() > 20 {
        return Err(invalid("This is not a place in a chat the app knows."));
    }
    Ok(text.to_owned())
}

fn pilot_handle(text: &str) -> Result<Handle, CoreError> {
    Handle::new(text.trim()).map_err(|_| invalid("Type the pilot's handle, such as @mara."))
}

fn description(text: &str) -> Result<Option<String>, CoreError> {
    api::parse_group_description(text).map_err(|error| match error {
        GroupTextError::Empty => invalid("The description could not be read."),
        GroupTextError::TooLong(most) => {
            invalid(&format!("A description has room for {most} characters."))
        }
    })
}

fn home(draft: &HomeDraft) -> Result<HomeArea, CoreError> {
    let center = GeoPoint::new(draft.center.lat, draft.center.lon)
        .map_err(|_| invalid("The group's place on the map could not be read."))?;
    if !(api::MIN_HOME_RADIUS_KM..=api::MAX_HOME_RADIUS_KM).contains(&draft.radius_km) {
        return Err(invalid(&format!(
            "A group's area reaches {} to {} km around its place.",
            api::MIN_HOME_RADIUS_KM,
            api::MAX_HOME_RADIUS_KM
        )));
    }
    let place = api::parse_home_place(&draft.place).map_err(|_| {
        invalid(&format!(
            "The place has room for {} characters.",
            api::MAX_HOME_PLACE_LENGTH
        ))
    })?;
    Ok(HomeArea {
        center,
        radius_km: draft.radius_km,
        place,
    })
}

/// A draft as the server takes it, with the links the text has.
fn message_request(draft: &MessageDraft) -> Result<MessageRequest, CoreError> {
    let text = api::parse_message_text(&draft.text).map_err(|_| {
        invalid(&format!(
            "A message has room for {} characters.",
            api::MAX_MESSAGE_LENGTH
        ))
    })?;
    if draft.pictures.len() > api::MAX_MESSAGE_PICTURES {
        return Err(invalid(&format!(
            "A message takes {} pictures at most.",
            api::MAX_MESSAGE_PICTURES
        )));
    }
    let mut links: Vec<LinkDraft> = draft.links.clone();
    for found in message_links(text.clone()) {
        if !links.contains(&found) {
            links.push(found);
        }
    }
    if links.len() > api::MAX_MESSAGE_LINKS {
        return Err(invalid(&format!(
            "A message shows {} links at most.",
            api::MAX_MESSAGE_LINKS
        )));
    }
    let links = links
        .iter()
        .map(|link| {
            let kind = match link.kind.as_str() {
                "flight" => LinkKind::Flight,
                "site" => LinkKind::Site,
                "open_plan" => LinkKind::OpenPlan,
                "pilot" => LinkKind::Pilot,
                _ => return Err(invalid("This link is to nothing the app knows.")),
            };
            Ok(Link {
                kind,
                id: link.id.trim().to_owned(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if text.is_empty() && draft.pictures.is_empty() && links.is_empty() {
        return Err(invalid("Write something, or add a picture."));
    }
    Ok(MessageRequest {
        text,
        pictures: draft.pictures.clone(),
        links,
    })
}

fn badge(count: u32) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".to_owned()),
    }
}

fn distance_words(km: f64) -> String {
    if km < 1.0 {
        "Here".to_owned()
    } else if km < 10.0 {
        format!("{km:.1} km")
    } else {
        format!("{km:.0} km")
    }
}

fn role_of(me: Option<&GroupMe>) -> CommunityRole {
    match me {
        None => CommunityRole::None,
        Some(me) => match me.role {
            Some(GroupRole::Owner) => CommunityRole::Owner,
            Some(GroupRole::Admin) => CommunityRole::Admin,
            Some(GroupRole::Member) => CommunityRole::Member,
            None if me.invited => CommunityRole::Invited,
            None => CommunityRole::None,
        },
    }
}

fn role_words(role: CommunityRole) -> &'static str {
    match role {
        CommunityRole::Owner => "Owner",
        CommunityRole::Admin => "Admin",
        CommunityRole::Invited => "Invited",
        CommunityRole::Member | CommunityRole::None => "",
    }
}

/// A group as a card shows it. `last` is its newest message with the
/// handle of the pilot of this device, whose own message reads "You: …".
fn community(
    group: &Group,
    me: Option<&GroupMe>,
    last: Option<(&api::Message, &str)>,
) -> CommunityView {
    let members = match group.member_count {
        1 => "1 member".to_owned(),
        count => format!("{count} members"),
    };
    let place = group.home.as_ref().and_then(|home| home.place.clone());
    let summary = match &place {
        Some(place) => format!("{members} · {place}"),
        None => members,
    };
    let unread = me.map_or(0, |me| me.unread);
    CommunityView {
        id: group.id.to_string(),
        name: group.name.clone(),
        description: group.description.clone(),
        picture_path: group.picture.clone(),
        is_public: group.public,
        center: group.home.as_ref().map(|home| Coordinate {
            lat: home.center.lat(),
            lon: home.center.lon(),
        }),
        radius_km: group.home.as_ref().map(|home| home.radius_km),
        place,
        members: group.member_count,
        summary,
        kind_words: if group.public { "Public" } else { "Private" }.to_owned(),
        my_role: role_of(me),
        muted: me.is_some_and(|me| me.muted),
        unread,
        unread_badge: badge(unread),
        last_message: last.map(|(message, me)| last_line(message, me)),
        last_message_at: last.map(|(message, _)| message.created_at.to_rfc3339()),
    }
}

/// A message in one line, for the list of groups: the pilot's own, by
/// the handle `me`, as "You: …".
fn last_line(message: &api::Message, me: &str) -> String {
    let said = if message.removed.is_some() {
        "a message was removed".to_owned()
    } else if message.kind == api::MessageKind::Plan {
        match &message.plan {
            Some(plan) => format!("a plan for {}", plan.place),
            None => "a plan".to_owned(),
        }
    } else if !message.text.trim().is_empty() {
        message
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else if !message.pictures.is_empty() {
        "a picture".to_owned()
    } else {
        "a link".to_owned()
    };
    if message.author.handle.as_str() == me {
        format!("You: {said}")
    } else {
        format!("{}: {said}", message.author.display_name)
    }
}

fn chat_message(message: &api::Message, me: &str) -> ChatMessage {
    ChatMessage {
        id: message.id.clone(),
        cursor: message.cursor.clone(),
        author_handle: message.author.handle.to_string(),
        author_name: message.author.display_name.clone(),
        is_mine: message.author.handle.as_str() == me,
        text: message.text.clone(),
        pictures: message
            .pictures
            .iter()
            .map(|picture| picture.path.clone())
            .collect(),
        picture_ids: message
            .pictures
            .iter()
            .map(|picture| picture.id.clone())
            .collect(),
        cards: message
            .cards
            .iter()
            .map(|card| card_view(card, me))
            .collect(),
        plan: message.plan.as_ref().map(|plan| plan_view(plan, None, me)),
        posted_at: message.created_at.to_rfc3339(),
        removed_words: message.removed.map(|removed| {
            match removed.by {
                api::RemovedBy::Author => "Removed by its author",
                api::RemovedBy::Admin => "Removed by an admin",
                api::RemovedBy::Operator => "Removed by Venti",
            }
            .to_owned()
        }),
    }
}

fn coordinate(point: GeoPoint) -> Coordinate {
    Coordinate {
        lat: point.lat(),
        lon: point.lon(),
    }
}

fn card_view(card: &Card, me: &str) -> CardView {
    let empty = |kind: &str, id: &str, title: String, subtitle: String| CardView {
        kind: kind.to_owned(),
        id: id.to_owned(),
        title,
        subtitle,
        track: Vec::new(),
        position: None,
        picture_path: None,
        am_in: false,
    };
    match card {
        Card::Flight(flight) => {
            let mut subtitle = format!(
                "{:.1} km · {}",
                flight.distance_km,
                flight.date.format("%-d %b %Y")
            );
            if let Some(takeoff) = &flight.takeoff {
                subtitle.push_str(&format!(" · from {takeoff}"));
            }
            CardView {
                track: flight
                    .track
                    .iter()
                    .map(|[lat, lon]| Coordinate {
                        lat: *lat,
                        lon: *lon,
                    })
                    .collect(),
                picture_path: flight.pilot.picture.clone(),
                ..empty(
                    "flight",
                    flight.id.as_str(),
                    format!("Flight by {}", flight.pilot.display_name),
                    subtitle,
                )
            }
        }
        Card::Site(site) => {
            let used = match site.site_use.as_str() {
                "takeoff" => "Take-off",
                "landing" => "Landing",
                "towing" => "Winch",
                "training" => "Training slope",
                _ => "Site",
            };
            let subtitle = match site.elevation_m {
                Some(metres) => format!("{used} · {metres:.0} m"),
                None => used.to_owned(),
            };
            CardView {
                position: Some(coordinate(site.position)),
                ..empty(
                    "site",
                    &site.id,
                    site.name.clone().unwrap_or_else(|| used.to_owned()),
                    subtitle,
                )
            }
        }
        Card::OpenPlan(plan) => {
            let pilots = match plan.pilots_in {
                1 => "1 pilot in".to_owned(),
                n => format!("{n} pilots in"),
            };
            CardView {
                position: Some(coordinate(plan.position)),
                am_in: plan.i_am_in,
                ..empty(
                    "open_plan",
                    &plan.id,
                    plan.place.clone(),
                    format!(
                        "{} {} · {pilots}",
                        api::weekday_name(plan.day),
                        plan.day.format("%-d %B")
                    ),
                )
            }
        }
        Card::Pilot { pilot } => CardView {
            picture_path: pilot.picture.clone(),
            ..empty(
                "pilot",
                pilot.handle.as_str(),
                pilot.display_name.clone(),
                if pilot.handle.as_str() == me {
                    "You".to_owned()
                } else {
                    format!("@{}", pilot.handle)
                },
            )
        },
        Card::Gone { link } => {
            let what = match link.kind {
                LinkKind::Flight => "This flight is not shared any more.",
                LinkKind::Site => "This site is gone.",
                LinkKind::OpenPlan => "This plan is over or gone.",
                LinkKind::Pilot => "This pilot has no page.",
            };
            empty("gone", &link.id, what.to_owned(), String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::TempFolder;

    fn golden(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    /// A core signed in as the development mode's demo pilot.
    fn signed_in(folder: &TempFolder) -> Arc<Core> {
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        core.signed_in(
            "https://venti.example".to_owned(),
            201,
            golden("sign_in.json"),
        )
        .unwrap();
        core
    }

    #[test]
    fn the_groups_near_munich_read_as_cards_with_their_place_and_distance() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let request = core
            .find_groups_request(
                Some(Coordinate {
                    lat: 48.14,
                    lon: 11.58,
                }),
                "Inn valley".to_owned(),
            )
            .unwrap();
        assert_eq!(
            request.path,
            "/v1/groups/find?lat=48.1400&lon=11.5800&q=Inn%20valley"
        );
        assert!(request.needs_token);
        let found = core.found_groups(200, golden("groups_found.json")).unwrap();
        assert_eq!(found[0].group.name, "Munich International Paragliders");
        assert_eq!(found[0].group.summary, "3 members · Munich");
        assert_eq!(found[0].group.kind_words, "Public");
        assert_eq!(found[0].distance.as_deref(), Some("Here"));
        assert!(!found[0].is_member);
        assert_eq!(found[0].group.my_role, CommunityRole::None);
    }

    #[test]
    fn a_public_group_says_what_the_pilot_may_do_in_it() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let detail = core.community(200, golden("group_public.json")).unwrap();
        assert!(detail.can_join && detail.can_read);
        assert!(!detail.can_post && !detail.can_moderate);
        let roles: Vec<&str> = detail
            .members
            .iter()
            .map(|member| member.role_words.as_str())
            .collect();
        assert_eq!(roles, ["Owner", "Admin", ""]);
    }

    #[test]
    fn a_chat_reads_with_its_flight_card_and_its_plan_posts() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let page = core.chat_page(200, golden("group_messages.json")).unwrap();
        assert_eq!(page.messages.len(), 2);
        let card = &page.messages[1].cards[0];
        assert_eq!(card.kind, "flight");
        assert_eq!(card.title, "Flight by Mara Muster");
        assert!(
            card.subtitle.starts_with("12.0 km · 16 Aug 2026"),
            "{}",
            card.subtitle
        );
        assert!(card.track.len() >= 3);
        assert!(!page.messages[1].is_mine);
        let request = core
            .messages_since_request("0000000000000000000000g002".to_owned(), page.since.clone())
            .unwrap();
        assert_eq!(
            request.path,
            format!(
                "/v1/groups/0000000000000000000000g002/messages?since={}",
                page.since
            )
        );

        let posts = core
            .chat_page(200, golden("group_plan_posts.json"))
            .unwrap();
        let plan = posts
            .messages
            .iter()
            .find_map(|message| message.plan.as_ref())
            .unwrap();
        assert!(!plan.place.is_empty());
    }

    #[test]
    fn a_message_request_is_what_the_server_takes() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let request = core
            .post_message_request(
                "0000000000000000000000g002".to_owned(),
                MessageDraft {
                    text: "Hi https://venti.fly/pilots/mara".to_owned(),
                    pictures: Vec::new(),
                    links: Vec::new(),
                },
            )
            .unwrap();
        let sent: MessageRequest = serde_json::from_slice(&request.body.unwrap()).unwrap();
        assert_eq!(
            sent.links,
            [Link {
                kind: LinkKind::Pilot,
                id: "mara".to_owned()
            }]
        );
        // The golden request reads as one the core would send.
        let golden: MessageRequest =
            serde_json::from_slice(&golden("group_message_request.json")).unwrap();
        assert_eq!(golden.links.len(), 3);
        assert!(
            core.post_message_request(
                "x".to_owned(),
                MessageDraft {
                    text: "hi".to_owned(),
                    pictures: Vec::new(),
                    links: Vec::new(),
                }
            )
            .is_err()
        );
    }

    #[test]
    fn venti_links_in_a_text_are_found_once_each() {
        let found = message_links(
            "Look: https://venti.fly/flights/0123456789abcdefghjkmnpqrs, and \
             (https://venti.example/pilots/mara) again https://venti.fly/flights/0123456789abcdefghjkmnpqrs \
             but not https://example.com/flights/x or venti.fly/pilots/mara"
                .to_owned(),
        );
        assert_eq!(
            found,
            [
                LinkDraft {
                    kind: "flight".to_owned(),
                    id: "0123456789abcdefghjkmnpqrs".to_owned()
                },
                LinkDraft {
                    kind: "pilot".to_owned(),
                    id: "mara".to_owned()
                },
            ]
        );
    }

    #[test]
    fn the_websites_own_addresses_of_a_site_and_an_open_plan_are_links_too() {
        let plan = "0123456789abcdefghjkmnpqrs";
        let found = message_links(format!(
            "Today at https://venti.fly/places/47.70100,12.01200/?site=dhv%3A1234, \
             or http://localhost:5173/logbook/everyone/place/?lat=47.7&lon=12.0&site=dhv:99#top \
             with https://venti.fly/plans/open/?id={plan}. \
             Old ones: https://venti.fly/logbook/everyone/flight/?id={plan} and \
             https://venti.fly/app/logbook/everyone/pilot/?handle=mara, \
             not https://venti.fly/places/47.7,12.0/ nor https://venti.fly/plans/open/?id=x \
             nor https://venti.fly/places/47.7,12.0/?site=nocolon"
        ));
        let link = |kind: &str, id: &str| LinkDraft {
            kind: kind.to_owned(),
            id: id.to_owned(),
        };
        assert_eq!(
            found,
            [
                link("site", "dhv:1234"),
                link("site", "dhv:99"),
                link("open_plan", plan),
                link("flight", plan),
                link("pilot", "mara"),
            ]
        );
        // What it read before, also under a folder of a site.
        assert_eq!(
            message_links(format!(
                "https://venti.fly/sites/dhv:7 https://venti.fly/open-plans/{plan} \
                 https://example.org/venti/flights/{plan}/"
            )),
            [
                link("site", "dhv:7"),
                link("open_plan", plan),
                link("flight", plan)
            ]
        );
        assert_eq!(decoded("a%3Ab+c").as_deref(), Some("a:b c"));
        assert_eq!(decoded("%3"), None);
        assert_eq!(decoded(""), None);
    }

    #[test]
    fn a_group_just_made_reads_with_the_pilot_as_its_owner() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let made = core
            .community_made(201, golden("group_created.json"))
            .unwrap();
        assert_eq!(made.group.name, "Zweite Mustergruppe");
        assert_eq!(made.group.my_role, CommunityRole::Owner);
        assert_eq!(made.group.summary, "1 member");
        assert_eq!(made.members.len(), 1);
        assert!(made.members[0].is_me);
        assert_eq!(made.members[0].role_words, "Owner");
        assert!(made.can_post && made.can_moderate && made.can_read && !made.can_join);
        // The answer is not a group with members, which `community` reads.
        assert!(core.community(201, golden("group_created.json")).is_err());
    }

    #[test]
    fn the_pilots_own_last_message_reads_you() {
        let folder = TempFolder::new();
        let core = signed_in(&folder);
        let theirs = core.communities(200, golden("groups.json")).unwrap();
        assert_eq!(
            theirs[0].last_message.as_deref(),
            Some("Jonas Beispiel: a plan")
        );
        let mine = String::from_utf8(golden("groups.json"))
            .unwrap()
            .replace(r#""handle": "jonas""#, r#""handle": "demo""#);
        let mine = core.communities(200, mine.into_bytes()).unwrap();
        assert_eq!(mine[0].last_message.as_deref(), Some("You: a plan"));
    }

    #[test]
    fn a_message_is_checked_before_it_is_sent() {
        let draft = |text: &str, pictures: usize| MessageDraft {
            text: text.to_owned(),
            pictures: vec!["0123456789abcdefghjkmnpqrs".to_owned(); pictures],
            links: Vec::new(),
        };
        assert!(message_request(&draft("  ", 0)).is_err());
        assert!(message_request(&draft(&"x".repeat(api::MAX_MESSAGE_LENGTH + 1), 0)).is_err());
        assert!(message_request(&draft("five", 5)).is_err());
        let request = message_request(&draft("See https://venti.fly/pilots/mara", 1)).unwrap();
        assert_eq!(request.links[0].kind, LinkKind::Pilot);
        assert_eq!(request.pictures.len(), 1);
    }

    #[test]
    fn a_home_is_checked_before_it_is_sent() {
        let draft = |radius_km: f64| HomeDraft {
            center: Coordinate {
                lat: 47.86,
                lon: 12.13,
            },
            radius_km,
            place: " Chiemgau ".to_owned(),
        };
        assert_eq!(
            home(&draft(30.0)).unwrap().place.as_deref(),
            Some("Chiemgau")
        );
        assert!(home(&draft(0.5)).is_err());
        assert!(home(&draft(101.0)).is_err());
    }

    #[test]
    fn distances_and_badges_read_as_a_person_says_them() {
        assert_eq!(distance_words(0.4), "Here");
        assert_eq!(distance_words(4.26), "4.3 km");
        assert_eq!(distance_words(52.4), "52 km");
        assert_eq!(badge(0), None);
        assert_eq!(badge(120).as_deref(), Some("99+"));
        assert_eq!(encode("Inn valley"), "Inn%20valley");
    }
}
