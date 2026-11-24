//! Groups as communities with a chat, in PostgreSQL
//! (`0035_group_chat.sql`). The locks are those of [`super::groups`]:
//! whatever changes who is in a group, who may do what in it or what it
//! holds first locks the group's row, the account's row before it where
//! that matters.

use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::Row;
use tokio_postgres::error::SqlState;
use venti_domain::Timestamp;

use super::groups::{
    GROUP_COLUMNS, GROUP_FROM, group_from, groups_with, held_group, is, is_member, lock_account,
    lock_as_member, lock_group, member_with, of_account, read_group, read_members,
};
use super::{corrupt, db_error};
use crate::group::can_name;
use crate::group_chat::{check_change, check_message, names, sort_found};
use crate::{
    FoundGroup, GroupAct, GroupInvite, GroupJoining, GroupMembers, GroupPicture, GroupPilot,
    GroupRole, GroupRoom, GroupSearch, GroupSeen, GroupStanding, InviteRoom, ListedGroup, Listener,
    Message, MessageKind, MessageLink, MessagePage, MessagePosting, MessageRemoval, NewMessage,
    ProfileChange, RemovedBy, StoreError, distance_km,
};

/// Everything [`message_from`] reads: a message `m` and its author `a`.
const MESSAGE_COLUMNS: &str = "
    m.id, m.group_id, m.number, m.version, m.author_id, a.handle AS author_handle,
    a.display_name AS author_name, m.kind, m.body, m.pictures, m.links, m.plan_id,
    m.created_at, m.removed_at, m.removed_by";

const MESSAGE_FROM: &str = "FROM group_messages m JOIN accounts a ON a.id = m.author_id";

/// The most public groups one search reads before it sorts them.
const SEARCH_READS: i64 = 2000;

fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn message_from(row: &Row) -> Result<Message, StoreError> {
    let kind: &str = row.get("kind");
    let removed_at: Option<Timestamp> = row.get("removed_at");
    let removed_by: Option<&str> = row.get("removed_by");
    let removed = match (removed_at, removed_by) {
        (Some(at), Some(by)) => Some((
            at,
            RemovedBy::from_name(by).ok_or_else(|| corrupt("message", "who removed it"))?,
        )),
        _ => None,
    };
    let links: Vec<String> = row.get("links");
    Ok(Message {
        id: row.get("id"),
        group_id: row.get("group_id"),
        number: row.get("number"),
        version: row.get("version"),
        author: GroupPilot {
            account_id: row.get("author_id"),
            handle: row.get("author_handle"),
            display_name: row.get("author_name"),
        },
        kind: MessageKind::from_name(kind).ok_or_else(|| corrupt("message", "its kind"))?,
        text: row.get("body"),
        pictures: row.get("pictures"),
        links: links
            .iter()
            .map(|link| MessageLink::from_kept(link).ok_or_else(|| corrupt("message", "a link")))
            .collect::<Result<_, _>>()?,
        plan_id: row.get("plan_id"),
        created_at: row.get("created_at"),
        removed,
    })
}

/// A plan's card changed: the messages that carry it are new since now.
pub(super) async fn plan_changed(tx: &Transaction<'_>, plan_id: &str) -> Result<(), StoreError> {
    tx.execute(
        "UPDATE group_messages SET version = nextval('group_message_numbers')
         WHERE plan_id = $1",
        &[&plan_id],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

/// How an account stands to a group whose owner is `keeper_id`.
async fn standing(
    db: &impl GenericClient,
    id: &str,
    account_id: &str,
    keeper_id: &str,
    most: u32,
) -> Result<GroupStanding, StoreError> {
    let row = db
        .query_one(
            "SELECT m.admin, m.muted, m.read_up_to,
                    EXISTS (SELECT 1 FROM group_invites i
                            WHERE i.group_id = $1 AND i.account_id = $2) AS invited
             FROM (SELECT 1) AS one
             LEFT JOIN group_members m ON m.group_id = $1 AND m.account_id = $2",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    let admin: Option<bool> = row.get(0);
    let role = match admin {
        None => None,
        Some(_) if account_id == keeper_id => Some(GroupRole::Owner),
        Some(true) => Some(GroupRole::Admin),
        Some(false) => Some(GroupRole::Member),
    };
    let read_up_to: i64 = row.get::<_, Option<i64>>(2).unwrap_or(0);
    let unread = if role.is_some() {
        let found = db
            .query_one(
                "SELECT count(*) FROM (
                     SELECT 1 FROM group_messages
                     WHERE group_id = $1 AND number > $3 AND author_id <> $2
                       AND removed_at IS NULL
                     LIMIT $4) AS u",
                &[&id, &account_id, &read_up_to, &i64::from(most)],
            )
            .await
            .map_err(db_error)?;
        count(found.get(0))
    } else {
        0
    };
    Ok(GroupStanding {
        role,
        invited: role.is_none() && row.get::<_, bool>(3),
        muted: row.get::<_, Option<bool>>(1).unwrap_or(false),
        read_up_to,
        unread,
    })
}

/// A pilot by account.
async fn pilot(db: &impl GenericClient, account_id: &str) -> Result<GroupPilot, StoreError> {
    let row = db
        .query_one(
            "SELECT handle, display_name FROM accounts WHERE id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(GroupPilot {
        account_id: account_id.to_owned(),
        handle: row.get(0),
        display_name: row.get(1),
    })
}

/// The account with a handle.
async fn account_with(db: &impl GenericClient, handle: &str) -> Result<Option<String>, StoreError> {
    if !can_name(&[handle]) {
        return Ok(None);
    }
    let row = db
        .query_opt("SELECT id FROM accounts WHERE handle = $1", &[&handle])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

/// Whether an account is an admin of a group it is in.
async fn is_admin(db: &impl GenericClient, id: &str, account_id: &str) -> Result<bool, StoreError> {
    let row = db
        .query_opt(
            "SELECT admin FROM group_members WHERE group_id = $1 AND account_id = $2",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.is_some_and(|row| row.get(0)))
}

/// Whether an account reads a group's messages: a member, or anyone for a
/// public group.
async fn reads(db: &impl GenericClient, id: &str, account_id: &str) -> Result<bool, StoreError> {
    if !can_name(&[id, account_id]) {
        return Ok(false);
    }
    let row = db
        .query_opt(
            "SELECT g.public OR EXISTS (SELECT 1 FROM group_members m
                                        WHERE m.group_id = g.id AND m.account_id = $2)
             FROM groups g WHERE g.id = $1",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.is_some_and(|row| row.get(0)))
}

async fn read_message(db: &impl GenericClient, id: &str) -> Result<Option<Message>, StoreError> {
    let query = format!("SELECT {MESSAGE_COLUMNS} {MESSAGE_FROM} WHERE m.id = $1");
    let row = db.query_opt(&query, &[&id]).await.map_err(db_error)?;
    row.as_ref().map(message_from).transpose()
}

pub(super) async fn find(
    client: &Client,
    account_id: &str,
    search: &GroupSearch,
) -> Result<Vec<FoundGroup>, StoreError> {
    let text = search.text.as_deref().filter(|text| can_name(&[text]));
    if search.text.is_some() && text.is_none() {
        return Ok(Vec::new());
    }
    // A box around the place, which the distance then narrows.
    let (lat, lon, dlat, dlon) = match search.near {
        Some(near) => {
            let dlat = search.within_km / 111.0;
            let dlon = search.within_km / (111.0 * near.lat().to_radians().cos().max(0.01));
            (Some(near.lat()), Some(near.lon()), dlat, dlon.min(360.0))
        }
        None => (None, None, 0.0, 0.0),
    };
    let query = format!(
        "SELECT {GROUP_COLUMNS},
                EXISTS (SELECT 1 FROM group_members m
                        WHERE m.group_id = g.id AND m.account_id = $1) AS member
         {GROUP_FROM}
         WHERE g.public
           AND ($2::text IS NULL
                OR strpos(lower(g.name), lower($2)) > 0
                OR strpos(lower(COALESCE(g.home_place, '')), lower($2)) > 0)
           AND ($3::float8 IS NULL
                OR (g.home_lat BETWEEN $3::float8 - $5::float8 AND $3::float8 + $5::float8
                    AND g.home_lon BETWEEN $4::float8 - $6::float8
                                       AND $4::float8 + $6::float8))
         ORDER BY g.id
         LIMIT $7"
    );
    let rows = client
        .query(
            &query,
            &[&account_id, &text, &lat, &lon, &dlat, &dlon, &SEARCH_READS],
        )
        .await
        .map_err(db_error)?;
    let mut found: Vec<FoundGroup> = rows
        .iter()
        .filter_map(|row| {
            let group = group_from(row);
            // The database's lower case and Rust's differ for a few
            // letters; what is shown holds to the store's own rule.
            if text.is_some_and(|text| !names(&group, text)) {
                return None;
            }
            let distance_km = search
                .near
                .zip(group.profile.home.as_ref())
                .map(|(near, home)| distance_km(near, home.center));
            if search.near.is_some() && distance_km.is_none_or(|km| km > search.within_km) {
                return None;
            }
            Some(FoundGroup {
                member: row.get("member"),
                group,
                distance_km,
            })
        })
        .collect();
    sort_found(&mut found);
    found.truncate(search.limit);
    Ok(found)
}

pub(super) async fn seen(
    client: &mut Client,
    id: &str,
    account_id: &str,
    most: u32,
) -> Result<Option<GroupSeen>, StoreError> {
    if !can_name(&[id, account_id]) {
        return Ok(None);
    }
    let tx = super::groups::at_one_moment(client).await?;
    let Some(group) = read_group(&tx, id).await? else {
        return Ok(None);
    };
    let standing = standing(&tx, id, account_id, &group.keeper.account_id, most).await?;
    if !group.profile.public && standing.role.is_none() && !standing.invited {
        return Ok(None);
    }
    let members = read_members(&tx, id).await?;
    let admins = tx
        .query(
            "SELECT account_id FROM group_members WHERE group_id = $1 AND admin
             ORDER BY joined_at, account_id",
            &[&id],
        )
        .await
        .map_err(db_error)?
        .iter()
        .map(|row| row.get(0))
        .collect();
    tx.commit().await.map_err(db_error)?;
    let mut group = group;
    group.members = u32::try_from(members.len()).unwrap_or(u32::MAX);
    Ok(Some(GroupSeen {
        group: GroupMembers { group, members },
        admins,
        standing,
    }))
}

pub(super) async fn list(
    client: &Client,
    account_id: &str,
    most: u32,
) -> Result<Vec<ListedGroup>, StoreError> {
    let groups = of_account(client, account_id).await?;
    let query = format!(
        "SELECT {MESSAGE_COLUMNS} {MESSAGE_FROM}
         WHERE m.group_id = $1 ORDER BY m.number DESC LIMIT 1"
    );
    let mut listed = Vec::with_capacity(groups.len());
    for group in groups {
        let standing = standing(
            client,
            &group.id,
            account_id,
            &group.keeper.account_id,
            most,
        )
        .await?;
        let last = client
            .query_opt(&query, &[&group.id])
            .await
            .map_err(db_error)?
            .as_ref()
            .map(message_from)
            .transpose()?;
        listed.push(ListedGroup {
            group,
            standing,
            last,
        });
    }
    Ok(listed)
}

pub(super) async fn change(
    client: &mut Client,
    id: &str,
    by: &str,
    change: &ProfileChange,
) -> Result<GroupAct<GroupMembers>, StoreError> {
    check_change(change)?;
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(keeper) = lock_as_member(&tx, id, by).await? else {
        return Ok(GroupAct::NoSuchGroup);
    };
    let owner = keeper == by;
    if !(owner || is_admin(&tx, id, by).await?) || (change.public.is_some() && !owner) {
        return Ok(GroupAct::NotAllowed);
    }
    if let Some(Some(picture)) = &change.picture {
        let ours = tx
            .query_opt(
                "SELECT 1 FROM group_pictures WHERE id = $1 AND group_id = $2",
                &[picture, &id],
            )
            .await
            .map_err(db_error)?
            .is_some();
        if !ours {
            return Err(StoreError::Invalid(
                "a group's picture that was not sent to it".to_owned(),
            ));
        }
    }
    let mut group = held_group(&tx, id).await?;
    if let Some(name) = &change.name {
        name.clone_into(&mut group.name);
    }
    let profile = &mut group.profile;
    if let Some(public) = change.public {
        profile.public = public;
    }
    if let Some(description) = &change.description {
        profile.description.clone_from(description);
    }
    if let Some(picture) = &change.picture {
        profile.picture.clone_from(picture);
    }
    if let Some(home) = &change.home {
        profile.home.clone_from(home);
    }
    let home = profile.home.as_ref();
    tx.execute(
        "UPDATE groups SET name = $2, public = $3, description = $4, picture_id = $5,
                home_lat = $6, home_lon = $7, home_radius_km = $8, home_place = $9
         WHERE id = $1",
        &[
            &id,
            &group.name,
            &profile.public,
            &profile.description,
            &profile.picture,
            &home.map(|home| home.center.lat()),
            &home.map(|home| home.center.lon()),
            &home.map(|home| home.radius_km),
            &home.and_then(|home| home.place.as_deref()),
        ],
    )
    .await
    .map_err(db_error)?;
    let members = read_members(&tx, id).await?;
    let group = held_group(&tx, id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(GroupAct::Done(GroupMembers { group, members }))
}

pub(super) async fn set_admin(
    client: &mut Client,
    id: &str,
    owner_id: &str,
    handle: &str,
    admin: bool,
) -> Result<GroupAct<()>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(keeper) = lock_as_member(&tx, id, owner_id).await? else {
        return Ok(GroupAct::NoSuchGroup);
    };
    if keeper != owner_id {
        return Ok(GroupAct::NotAllowed);
    }
    let Some(member) = member_with(&tx, id, handle, true).await? else {
        return Ok(GroupAct::NoSuchPilot);
    };
    if member != keeper {
        tx.execute(
            "UPDATE group_members SET admin = $3 WHERE group_id = $1 AND account_id = $2",
            &[&id, &member, &admin],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(GroupAct::Done(()))
}

pub(super) async fn invite(
    client: &mut Client,
    (id, by): (&str, &str),
    handle: &str,
    now: Timestamp,
    room: &InviteRoom,
) -> Result<GroupAct<GroupInvite>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(keeper) = lock_as_member(&tx, id, by).await? else {
        return Ok(GroupAct::NoSuchGroup);
    };
    if keeper != by && !is_admin(&tx, id, by).await? {
        return Ok(GroupAct::NotAllowed);
    }
    let Some(invited) = account_with(&tx, handle).await? else {
        return Ok(GroupAct::NoSuchPilot);
    };
    if is_member(&tx, id, &invited).await? {
        return Ok(GroupAct::AlreadyIn);
    }
    let group = held_group(&tx, id).await?;
    let open = tx
        .query_opt(
            "SELECT invited_by, invited_at FROM group_invites
             WHERE group_id = $1 AND account_id = $2",
            &[&id, &invited],
        )
        .await
        .map_err(db_error)?;
    if let Some(open) = open {
        let asker: String = open.get(0);
        let invite = GroupInvite {
            group,
            invited_by: pilot(&tx, &asker).await?,
            invited_at: open.get(1),
        };
        tx.commit().await.map_err(db_error)?;
        return Ok(GroupAct::Done(invite));
    }
    let held: i64 = tx
        .query_one(
            "SELECT count(*) FROM group_invites WHERE group_id = $1",
            &[&id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if held >= i64::from(room.invites_per_group) {
        return Ok(GroupAct::Full);
    }
    tx.execute(
        "INSERT INTO group_invites (group_id, account_id, invited_by, invited_at)
         VALUES ($1, $2, $3, $4)",
        &[&id, &invited, &by, &now],
    )
    .await
    .map_err(db_error)?;
    let invite = GroupInvite {
        group,
        invited_by: pilot(&tx, by).await?,
        invited_at: now,
    };
    tx.commit().await.map_err(db_error)?;
    Ok(GroupAct::Done(invite))
}

pub(super) async fn drop_invite(
    client: &mut Client,
    id: &str,
    by: &str,
    handle: &str,
) -> Result<GroupAct<()>, StoreError> {
    if !can_name(&[id, by]) {
        return Ok(GroupAct::NoSuchGroup);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    let invited = account_with(&tx, handle).await?;
    if invited.as_deref() == Some(by) {
        let declined = tx
            .execute(
                "DELETE FROM group_invites WHERE group_id = $1 AND account_id = $2",
                &[&id, &by],
            )
            .await
            .map_err(db_error)?;
        if declined > 0 {
            tx.commit().await.map_err(db_error)?;
            return Ok(GroupAct::Done(()));
        }
    }
    let Some(keeper) = lock_as_member(&tx, id, by).await? else {
        return Ok(GroupAct::NoSuchGroup);
    };
    if keeper != by && !is_admin(&tx, id, by).await? {
        return Ok(GroupAct::NotAllowed);
    }
    let Some(invited) = invited else {
        return Ok(GroupAct::NoSuchPilot);
    };
    let dropped = tx
        .execute(
            "DELETE FROM group_invites WHERE group_id = $1 AND account_id = $2",
            &[&id, &invited],
        )
        .await
        .map_err(db_error)?;
    if dropped == 0 {
        return Ok(GroupAct::NoSuchPilot);
    }
    tx.commit().await.map_err(db_error)?;
    Ok(GroupAct::Done(()))
}

pub(super) async fn invites_of(
    client: &Client,
    account_id: &str,
) -> Result<Vec<GroupInvite>, StoreError> {
    if !can_name(&[account_id]) {
        return Ok(Vec::new());
    }
    let query = format!(
        "SELECT {GROUP_COLUMNS}, i.invited_at, i.invited_by, b.handle AS by_handle,
                b.display_name AS by_name
         {GROUP_FROM}
         JOIN group_invites i ON i.group_id = g.id
         JOIN accounts b ON b.id = i.invited_by
         WHERE i.account_id = $1
         ORDER BY i.invited_at DESC, g.id"
    );
    let rows = client
        .query(&query, &[&account_id])
        .await
        .map_err(db_error)?;
    Ok(rows
        .iter()
        .map(|row| GroupInvite {
            group: group_from(row),
            invited_by: GroupPilot {
                account_id: row.get("invited_by"),
                handle: row.get("by_handle"),
                display_name: row.get("by_name"),
            },
            invited_at: row.get("invited_at"),
        })
        .collect())
}

pub(super) async fn enter(
    client: &mut Client,
    id: &str,
    account_id: &str,
    room: &GroupRoom,
    now: Timestamp,
) -> Result<GroupJoining, StoreError> {
    let no_account =
        || StoreError::Invalid("a place in a group for an account that does not exist".to_owned());
    if !can_name(&[account_id]) {
        return Err(no_account());
    }
    if !can_name(&[id]) {
        return Ok(GroupJoining::NoSuchInvite);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    if !lock_account(&tx, account_id).await.map_err(db_error)? {
        return Err(no_account());
    }
    if lock_group(&tx, id).await?.is_none() {
        return Ok(GroupJoining::NoSuchInvite);
    }
    if is_member(&tx, id, account_id).await? {
        return Ok(GroupJoining::AlreadyIn(held_group(&tx, id).await?));
    }
    let row = tx
        .query_one(
            "SELECT g.public,
                    EXISTS (SELECT 1 FROM group_invites i
                            WHERE i.group_id = g.id AND i.account_id = $2),
                    (SELECT count(*) FROM group_members m WHERE m.group_id = g.id)
             FROM groups g WHERE g.id = $1",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    let (public, invited, members): (bool, bool, i64) = (row.get(0), row.get(1), row.get(2));
    if !public && !invited {
        return Ok(GroupJoining::NoSuchInvite);
    }
    if groups_with(&tx, account_id).await? >= i64::from(room.groups_per_account) {
        return Ok(GroupJoining::TooManyGroups);
    }
    if members >= i64::from(room.members_per_group) {
        return Ok(GroupJoining::GroupFull);
    }
    // A new member starts with everything before them read.
    tx.execute(
        "INSERT INTO group_members (group_id, account_id, joined_at, read_up_to)
         VALUES ($1, $2, $3, COALESCE((SELECT max(number) FROM group_messages
                                       WHERE group_id = $1), 0))",
        &[&id, &account_id, &now],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "DELETE FROM group_invites WHERE group_id = $1 AND account_id = $2",
        &[&id, &account_id],
    )
    .await
    .map_err(db_error)?;
    let group = held_group(&tx, id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(GroupJoining::Joined(group))
}

pub(super) async fn post(
    client: &mut Client,
    new: &NewMessage,
) -> Result<MessagePosting, StoreError> {
    check_message(new)?;
    let tx = client.transaction().await.map_err(db_error)?;
    if lock_as_member(&tx, &new.group_id, &new.author_id)
        .await?
        .is_none()
    {
        return Ok(MessagePosting::NoSuchGroup);
    }
    let mut distinct = new.pictures.clone();
    distinct.sort();
    distinct.dedup();
    if distinct.len() != new.pictures.len() {
        return Ok(MessagePosting::NoSuchPicture);
    }
    if !new.pictures.is_empty() {
        let fit: i64 = tx
            .query_one(
                "SELECT count(*) FROM group_pictures p
                 WHERE p.id = ANY($1) AND p.group_id = $2 AND p.account_id = $3
                   AND NOT EXISTS (SELECT 1 FROM group_messages m
                                   WHERE m.group_id = $2 AND p.id = ANY(m.pictures))",
                &[&new.pictures, &new.group_id, &new.author_id],
            )
            .await
            .map_err(db_error)?
            .get(0);
        if usize::try_from(fit).ok() != Some(new.pictures.len()) {
            return Ok(MessagePosting::NoSuchPicture);
        }
    }
    let links: Vec<String> = new.links.iter().map(MessageLink::kept).collect();
    let stored = tx
        .query_one(
            "WITH n AS (SELECT nextval('group_message_numbers') AS n)
             INSERT INTO group_messages (id, group_id, number, version, author_id, kind, body,
                                         pictures, links, created_at)
             SELECT $1, $2, n.n, n.n, $3, 'text', $4, $5, $6, $7 FROM n
             RETURNING number",
            &[
                &new.id,
                &new.group_id,
                &new.author_id,
                &new.text,
                &new.pictures,
                &links,
                &new.created_at,
            ],
        )
        .await;
    let number: i64 = match stored {
        Ok(row) => row.get(0),
        Err(error) if is(&error, &SqlState::UNIQUE_VIOLATION) => {
            return Err(StoreError::Invalid(
                "a message under an identifier that is in use".to_owned(),
            ));
        }
        Err(error) => return Err(db_error(error)),
    };
    // The author has read what they wrote.
    tx.execute(
        "UPDATE group_members SET read_up_to = GREATEST(read_up_to, $3)
         WHERE group_id = $1 AND account_id = $2",
        &[&new.group_id, &new.author_id, &number],
    )
    .await
    .map_err(db_error)?;
    let message = read_message(&tx, &new.id)
        .await?
        .ok_or_else(|| corrupt("message", "it is gone while it is posted"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(MessagePosting::Posted(Box::new(message)))
}

pub(super) async fn page(
    client: &Client,
    group_id: &str,
    account_id: &str,
    page: MessagePage,
    limit: usize,
) -> Result<Option<Vec<Message>>, StoreError> {
    if !reads(client, group_id, account_id).await? {
        return Ok(None);
    }
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = match page {
        MessagePage::Before(before) => {
            let query = format!(
                "SELECT {MESSAGE_COLUMNS} {MESSAGE_FROM}
                 WHERE m.group_id = $1 AND ($2::bigint IS NULL OR m.number < $2)
                 ORDER BY m.number DESC LIMIT $3"
            );
            client.query(&query, &[&group_id, &before, &limit]).await
        }
        MessagePage::After(after) => {
            let query = format!(
                "SELECT {MESSAGE_COLUMNS} {MESSAGE_FROM}
                 WHERE m.group_id = $1 AND m.version > $2
                 ORDER BY m.version LIMIT $3"
            );
            client.query(&query, &[&group_id, &after, &limit]).await
        }
    }
    .map_err(db_error)?;
    rows.iter()
        .map(message_from)
        .collect::<Result<_, _>>()
        .map(Some)
}

pub(super) async fn message(
    client: &Client,
    group_id: &str,
    message_id: &str,
    account_id: &str,
) -> Result<Option<Message>, StoreError> {
    if !can_name(&[message_id]) || !reads(client, group_id, account_id).await? {
        return Ok(None);
    }
    Ok(read_message(client, message_id)
        .await?
        .filter(|message| message.group_id == group_id))
}

pub(super) async fn remove_message(
    client: &mut Client,
    (group_id, message_id): (&str, &str),
    by: &str,
    now: Timestamp,
) -> Result<MessageRemoval, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(keeper) = lock_as_member(&tx, group_id, by).await? else {
        return Ok(MessageRemoval::NoSuchGroup);
    };
    if !can_name(&[message_id]) {
        return Ok(MessageRemoval::NoSuchMessage);
    }
    let found = tx
        .query_opt(
            "SELECT author_id, pictures, removed_at IS NOT NULL FROM group_messages
             WHERE id = $1 AND group_id = $2 FOR UPDATE",
            &[&message_id, &group_id],
        )
        .await
        .map_err(db_error)?;
    let Some(found) = found else {
        return Ok(MessageRemoval::NoSuchMessage);
    };
    let (author, pictures, gone): (String, Vec<String>, bool) =
        (found.get(0), found.get(1), found.get(2));
    let by_author = author == by;
    if !by_author && keeper != by && !is_admin(&tx, group_id, by).await? {
        return Ok(MessageRemoval::NotAllowed);
    }
    if !gone {
        let who = if by_author {
            RemovedBy::Author
        } else {
            RemovedBy::Admin
        };
        tx.execute(
            "UPDATE group_messages
             SET body = '', pictures = '{}', links = '{}',
                 version = nextval('group_message_numbers'),
                 removed_at = $2, removed_by = $3
             WHERE id = $1",
            &[&message_id, &now, &who.as_str()],
        )
        .await
        .map_err(db_error)?;
        tx.execute(
            "DELETE FROM group_pictures WHERE id = ANY($1)",
            &[&pictures],
        )
        .await
        .map_err(db_error)?;
    }
    let message = read_message(&tx, message_id)
        .await?
        .ok_or_else(|| corrupt("message", "it is gone while its row is held"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(MessageRemoval::Removed(Box::new(message)))
}

pub(super) async fn set_standing(
    client: &Client,
    id: &str,
    account_id: &str,
    (muted, read_up_to): (Option<bool>, Option<i64>),
    most: u32,
) -> Result<Option<GroupStanding>, StoreError> {
    if !can_name(&[id, account_id]) {
        return Ok(None);
    }
    let changed = client
        .execute(
            "UPDATE group_members
             SET muted = COALESCE($3, muted),
                 read_up_to = GREATEST(read_up_to, LEAST(
                     COALESCE($4, read_up_to),
                     COALESCE((SELECT max(number) FROM group_messages WHERE group_id = $1), 0)))
             WHERE group_id = $1 AND account_id = $2",
            &[&id, &account_id, &muted, &read_up_to],
        )
        .await
        .map_err(db_error)?;
    if changed == 0 {
        return Ok(None);
    }
    let Some(keeper) = client
        .query_opt("SELECT keeper_id FROM groups WHERE id = $1", &[&id])
        .await
        .map_err(db_error)?
    else {
        return Ok(None);
    };
    let keeper: String = keeper.get(0);
    standing(client, id, account_id, &keeper, most)
        .await
        .map(Some)
}

pub(super) async fn listeners(
    client: &Client,
    group_id: &str,
) -> Result<Vec<Listener>, StoreError> {
    if !can_name(&[group_id]) {
        return Ok(Vec::new());
    }
    let rows = client
        .query(
            "SELECT m.account_id, a.handle, m.muted
             FROM group_members m JOIN accounts a ON a.id = m.account_id
             WHERE m.group_id = $1
             ORDER BY m.joined_at, m.account_id",
            &[&group_id],
        )
        .await
        .map_err(db_error)?;
    Ok(rows
        .iter()
        .map(|row| Listener {
            account_id: row.get(0),
            handle: row.get(1),
            muted: row.get(2),
        })
        .collect())
}

pub(super) async fn save_picture(
    client: &mut Client,
    picture: &GroupPicture,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    if lock_as_member(&tx, &picture.group_id, &picture.account_id)
        .await?
        .is_none()
    {
        return Ok(false);
    }
    let side = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    let stored = tx
        .execute(
            "INSERT INTO group_pictures (id, group_id, account_id, bytes, width, height, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &picture.id,
                &picture.group_id,
                &picture.account_id,
                &picture.bytes,
                &side(picture.width),
                &side(picture.height),
                &picture.created_at,
            ],
        )
        .await;
    match stored {
        Ok(_) => {}
        Err(error) if is(&error, &SqlState::UNIQUE_VIOLATION) => {
            return Err(StoreError::Invalid(
                "a picture under an identifier that is in use".to_owned(),
            ));
        }
        Err(error) => return Err(db_error(error)),
    }
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn picture(client: &Client, id: &str) -> Result<Option<GroupPicture>, StoreError> {
    if !can_name(&[id]) {
        return Ok(None);
    }
    let row = client
        .query_opt(
            "SELECT id, group_id, account_id, bytes, width, height, created_at
             FROM group_pictures WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| {
        let side = |value: i32| u32::try_from(value).unwrap_or(0);
        GroupPicture {
            id: row.get(0),
            group_id: row.get(1),
            account_id: row.get(2),
            bytes: row.get(3),
            width: side(row.get(4)),
            height: side(row.get(5)),
            created_at: row.get(6),
        }
    }))
}

pub(super) async fn unread(
    client: &Client,
    account_id: &str,
    most: u32,
) -> Result<u32, StoreError> {
    if !can_name(&[account_id]) {
        return Ok(0);
    }
    let row = client
        .query_one(
            "SELECT count(*) FROM (
                 SELECT 1 FROM group_members gm
                 JOIN group_messages m ON m.group_id = gm.group_id AND m.number > gm.read_up_to
                 WHERE gm.account_id = $1 AND m.author_id <> $1 AND m.removed_at IS NULL
                 LIMIT $2) AS u",
            &[&account_id, &i64::from(most)],
        )
        .await
        .map_err(db_error)?;
    Ok(count(row.get(0)))
}
