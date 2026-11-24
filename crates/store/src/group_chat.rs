//! Groups as communities with a chat (owner decision 26): public and
//! private groups, their owner, admins and members, invites by handle, a
//! home area by which groups are found, and the messages of each group.
//!
//! **Who reads a group.** Its members; for a public group, every account;
//! for a private group, also an account invited into it, which sees the
//! group and not its messages. To every other account a private group is
//! what a group that does not exist is: the two are one value.
//!
//! **Who may do what.** The owner is the account [`Group::keeper`] names.
//! The owner makes and unmakes admins, hands the group on, makes it public
//! or private and deletes it. The owner and the admins invite, change the
//! name, description, picture and home area, remove members who are no
//! admins, and remove messages. A member leaves, posts and removes their
//! own messages.
//!
//! **A message stays when its author leaves the group**, and goes with the
//! author's account. A removed message keeps its place and loses its text,
//! pictures and links.

use venti_domain::{GeoPoint, Timestamp};

use crate::{Group, GroupMembers, GroupPilot, StoreError};

/// Where a group flies: a point, a radius and the place's name.
#[derive(Debug, Clone, PartialEq)]
pub struct HomeArea {
    /// The middle of the area.
    pub center: GeoPoint,
    /// How far around it, kilometres.
    pub radius_km: f64,
    /// The place, in the owner's words: a valley, a mountain, a region.
    pub place: Option<String>,
}

/// What a group says of itself beyond its name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroupProfile {
    /// Whether anyone finds, reads and joins it.
    pub public: bool,
    /// A short description.
    pub description: Option<String>,
    /// The identifier of its picture, a [`GroupPicture`].
    pub picture: Option<String>,
    /// Where it flies. Groups made before decision 26 have none.
    pub home: Option<HomeArea>,
}

/// A change to what a group says of itself. What is `None` stays.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProfileChange {
    /// A new name.
    pub name: Option<String>,
    /// Public or private. The owner's alone.
    pub public: Option<bool>,
    /// A new description, or none.
    pub description: Option<Option<String>>,
    /// A new picture, or none.
    pub picture: Option<Option<String>>,
    /// A new home area, or none.
    pub home: Option<Option<HomeArea>>,
}

/// What an account is in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRole {
    /// The owner, [`Group::keeper`].
    Owner,
    /// An admin.
    Admin,
    /// A member.
    Member,
}

/// How an account stands to a group it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupStanding {
    /// Its role, if it is a member.
    pub role: Option<GroupRole>,
    /// Whether it was invited into the group and is not in it yet.
    pub invited: bool,
    /// Whether the group's messages are not pushed to its phones.
    pub muted: bool,
    /// The number of the last message it read; 0 for none.
    pub read_up_to: i64,
    /// How many messages of others after that, up to the count asked
    /// for.
    pub unread: u32,
}

/// A group as an account reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupSeen {
    /// The group with its members.
    pub group: GroupMembers,
    /// The account ids of its admins, the owner not among them.
    pub admins: Vec<String>,
    /// How the account stands to it.
    pub standing: GroupStanding,
}

/// A group in the list of an account's own groups.
#[derive(Debug, Clone, PartialEq)]
pub struct ListedGroup {
    /// The group.
    pub group: Group,
    /// How the account stands to it.
    pub standing: GroupStanding,
    /// The newest message, if the group has one.
    pub last: Option<Message>,
}

/// A public group found near a place or by name.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundGroup {
    /// The group.
    pub group: Group,
    /// How far its home is from the place asked about, kilometres, when a
    /// place was given and the group has a home.
    pub distance_km: Option<f64>,
    /// Whether the account that asks is in it.
    pub member: bool,
}

/// What groups are looked for.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupSearch {
    /// Near this place: groups whose home's middle is within `within_km`,
    /// nearest first, then those with more members.
    pub near: Option<GeoPoint>,
    /// How far from `near`, kilometres.
    pub within_km: f64,
    /// Text the group's name or home place contains, upper or lower case
    /// alike.
    pub text: Option<String>,
    /// The most groups to give.
    pub limit: usize,
}

/// A pilot invited into a private group.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupInvite {
    /// The group.
    pub group: Group,
    /// Who asked.
    pub invited_by: GroupPilot,
    /// When.
    pub invited_at: Timestamp,
}

/// What became of something only some members may do.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupAct<T> {
    /// It is done.
    Done(T),
    /// The asker is in the group, but may not do this.
    NotAllowed,
    /// The handle named is no member's, or no pilot's.
    NoSuchPilot,
    /// The pilot named is a member already.
    AlreadyIn,
    /// Too many of something; nothing changed.
    Full,
    /// There is no such group, or the asker cannot see it.
    NoSuchGroup,
}

/// What a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Text, pictures and links.
    Text,
    /// A plan of the group, with its card.
    Plan,
}

impl MessageKind {
    /// The word the store and the API know the kind by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Plan => "plan",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "text" => Some(Self::Text),
            "plan" => Some(Self::Plan),
            _ => None,
        }
    }
}

/// What a link in a message names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

impl LinkKind {
    /// The word the store and the API know the kind by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flight => "flight",
            Self::Site => "site",
            Self::OpenPlan => "open_plan",
            Self::Pilot => "pilot",
        }
    }

    /// The kind for a word, if it is one.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "flight" => Some(Self::Flight),
            "site" => Some(Self::Site),
            "open_plan" => Some(Self::OpenPlan),
            "pilot" => Some(Self::Pilot),
            _ => None,
        }
    }
}

/// A link to one of Venti's records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLink {
    /// What it names.
    pub kind: LinkKind,
    /// Its identifier, address or handle.
    pub id: String,
}

impl MessageLink {
    /// As the database keeps it: `kind:identifier`.
    pub(crate) fn kept(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }

    /// From what the database keeps.
    pub(crate) fn from_kept(text: &str) -> Option<Self> {
        let (kind, id) = text.split_once(':')?;
        Some(Self {
            kind: LinkKind::from_name(kind)?,
            id: id.to_owned(),
        })
    }
}

/// Who removed a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovedBy {
    /// Its author.
    Author,
    /// The owner or an admin of the group.
    Admin,
    /// Whoever runs the server.
    Operator,
}

impl RemovedBy {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Author => "author",
            Self::Admin => "admin",
            Self::Operator => "operator",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "author" => Some(Self::Author),
            "admin" => Some(Self::Admin),
            "operator" => Some(Self::Operator),
            _ => None,
        }
    }
}

/// A message of a group.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Its identifier.
    pub id: String,
    /// The group.
    pub group_id: String,
    /// Its place in the chat: larger is later.
    pub number: i64,
    /// New at each change; what "since" compares.
    pub version: i64,
    /// Who wrote it.
    pub author: GroupPilot,
    /// What it is.
    pub kind: MessageKind,
    /// Its text; empty for none and once removed.
    pub text: String,
    /// The identifiers of its pictures.
    pub pictures: Vec<String>,
    /// Its links.
    pub links: Vec<MessageLink>,
    /// The plan a message of kind plan carries, while it is there.
    pub plan_id: Option<String>,
    /// When it was posted.
    pub created_at: Timestamp,
    /// When and by whom it was removed.
    pub removed: Option<(Timestamp, RemovedBy)>,
}

/// A message to post.
#[derive(Debug, Clone, PartialEq)]
pub struct NewMessage {
    /// Its identifier.
    pub id: String,
    /// The group.
    pub group_id: String,
    /// The member who posts it.
    pub author_id: String,
    /// Its text, cleaned.
    pub text: String,
    /// Pictures the author sent to this group and no message has yet.
    pub pictures: Vec<String>,
    /// Its links.
    pub links: Vec<MessageLink>,
    /// When.
    pub created_at: Timestamp,
}

/// What became of a message handed to the store.
#[derive(Debug, Clone, PartialEq)]
pub enum MessagePosting {
    /// It is posted.
    Posted(Box<Message>),
    /// A picture named is not one the author sent to this group, or a
    /// message has it already.
    NoSuchPicture,
    /// There is no such group, or the author is not in it.
    NoSuchGroup,
}

/// Which messages are asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessagePage {
    /// The newest first, before the message with this number, or from
    /// the newest.
    Before(Option<i64>),
    /// Those posted or changed after this version, in the order of their
    /// versions.
    After(i64),
}

/// What became of removing a message.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRemoval {
    /// It shows as removed.
    Removed(Box<Message>),
    /// The asker neither wrote it nor is the owner or an admin.
    NotAllowed,
    /// The group has no such message.
    NoSuchMessage,
    /// There is no such group, or the asker is not in it.
    NoSuchGroup,
}

/// A picture sent to a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPicture {
    /// Its identifier.
    pub id: String,
    /// The group.
    pub group_id: String,
    /// Who sent it.
    pub account_id: String,
    /// The picture as the server made it.
    pub bytes: Vec<u8>,
    /// Its width in pixels.
    pub width: u32,
    /// Its height in pixels.
    pub height: u32,
    /// When it was sent.
    pub created_at: Timestamp,
}

/// A member of a group as a new message is pushed to them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// The account.
    pub account_id: String,
    /// Its handle, for mentions.
    pub handle: String,
    /// Whether it muted the group.
    pub muted: bool,
}

/// How many members and invites groups may have; see
/// [`crate::GroupRoom`] for the members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteRoom {
    /// The most open invites one group may have.
    pub invites_per_group: u32,
}

/// Refuses text no store keeps.
pub(crate) fn check_text(what: &str, text: &str) -> Result<(), StoreError> {
    if text.contains('\0') {
        return Err(StoreError::Invalid(format!(
            "{what} with a zero byte in it"
        )));
    }
    Ok(())
}

/// Refuses a profile no store keeps.
pub(crate) fn check_profile(profile: &GroupProfile) -> Result<(), StoreError> {
    check_text(
        "a group's description",
        profile.description.as_deref().unwrap_or_default(),
    )?;
    check_home(profile.home.as_ref())
}

pub(crate) fn check_home(home: Option<&HomeArea>) -> Result<(), StoreError> {
    if let Some(home) = home {
        if !(home.radius_km.is_finite() && home.radius_km > 0.0) {
            return Err(StoreError::Invalid(
                "a home area without a radius".to_owned(),
            ));
        }
        check_text(
            "a home area's place",
            home.place.as_deref().unwrap_or_default(),
        )?;
    }
    Ok(())
}

/// Refuses a change no store keeps.
pub(crate) fn check_change(change: &ProfileChange) -> Result<(), StoreError> {
    if let Some(name) = &change.name {
        check_text("a group's name", name)?;
    }
    if let Some(Some(description)) = &change.description {
        check_text("a group's description", description)?;
    }
    if let Some(home) = &change.home {
        check_home(home.as_ref())?;
    }
    Ok(())
}

/// Refuses a message no store keeps.
pub(crate) fn check_message(new: &NewMessage) -> Result<(), StoreError> {
    check_text("a message's identifier", &new.id)?;
    check_text("a message", &new.text)?;
    for link in &new.links {
        check_text("a link", &link.id)?;
    }
    for picture in &new.pictures {
        check_text("a picture's identifier", picture)?;
    }
    Ok(())
}

/// How far apart two points are, kilometres, on a sphere: near enough for
/// sorting groups by how far they fly from a pilot.
pub fn distance_km(a: GeoPoint, b: GeoPoint) -> f64 {
    let (lat1, lat2) = (a.lat().to_radians(), b.lat().to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon() - a.lon()).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().min(1.0).asin()
}

/// Whether a group's name or home place contains `text`, upper or lower
/// case alike.
pub(crate) fn names(group: &Group, text: &str) -> bool {
    let text = text.to_lowercase();
    group.name.to_lowercase().contains(&text)
        || group
            .profile
            .home
            .as_ref()
            .and_then(|home| home.place.as_deref())
            .is_some_and(|place| place.to_lowercase().contains(&text))
}

/// Puts found groups in order: nearest first when a place was given,
/// those with more members first among those as near, then by name and
/// identifier.
pub(crate) fn sort_found(found: &mut [FoundGroup]) {
    found.sort_by(|a, b| {
        let distance = |found: &FoundGroup| found.distance_km.unwrap_or(f64::INFINITY);
        distance(a)
            .total_cmp(&distance(b))
            .then(b.group.members.cmp(&a.group.members))
            .then(a.group.name.cmp(&b.group.name))
            .then(a.group.id.cmp(&b.group.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_kept_as_its_kind_and_identifier() {
        let link = MessageLink {
            kind: LinkKind::OpenPlan,
            id: "abc".to_owned(),
        };
        assert_eq!(link.kept(), "open_plan:abc");
        assert_eq!(MessageLink::from_kept("open_plan:abc"), Some(link));
        assert_eq!(MessageLink::from_kept("nothing:abc"), None);
        assert_eq!(MessageLink::from_kept("flight"), None);
    }

    #[test]
    fn munich_is_about_55_km_from_rosenheim() {
        let munich = GeoPoint::new(48.137, 11.575).unwrap();
        let rosenheim = GeoPoint::new(47.856, 12.128).unwrap();
        let km = distance_km(munich, rosenheim);
        assert!((50.0..55.0).contains(&km), "{km}");
    }
}
