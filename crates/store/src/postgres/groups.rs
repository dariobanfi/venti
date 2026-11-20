//! Groups and their plans in PostgreSQL.
//!
//! **One change to a group at a time.** Whatever changes who is in a
//! group, who keeps it, which code lets a pilot in or how many plans it
//! holds first locks the group's row. Two pilots who race for the last
//! place get it one after the other, and a pilot who comes in while the
//! code is replaced either held the row first, and is in, or finds the
//! code gone.
//!
//! **The account first, then the group.** Where an account's own row
//! matters too (how many groups it is in, and that it is not being deleted
//! at this moment) it is locked before the group's row, never after, so
//! two calls cannot wait for each other.
//!
//! **A member stays one while a call counts on it.** A call that acts for
//! a member, or names one as the next keeper, holds that member's row
//! against deletion until it is done. An account that is deleted at the
//! same moment waits for the call, or the call finds the member gone.
//!
//! The database itself holds every writer to the rest: a group's keeper is
//! one of its members, and what a member put into a group goes with the
//! member (`0010_groups_and_plans.sql`).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::error::SqlState;
use tokio_postgres::{IsolationLevel, Row};
use venti_domain::{GeoPoint, Timestamp};

use super::{corrupt, db_error};
use crate::group::{can_name, check_details, check_group, check_name, check_plan, days_kept};
use crate::{
    Group, GroupCreation, GroupJoining, GroupMembers, GroupPilot, GroupRoom, KeeperAct,
    MemberRemoval, NewGroup, NewPlan, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom,
    PlanSaving, SecretHash, StoreError,
};

/// Everything [`group_from`] reads: a group `g` and its keeper's account
/// `k`.
const GROUP_COLUMNS: &str = "
    g.id, g.name, g.created_at, g.keeper_id, k.handle AS keeper_handle,
    k.display_name AS keeper_name,
    (SELECT count(*) FROM group_members c WHERE c.group_id = g.id) AS members";

const GROUP_FROM: &str = "FROM groups g JOIN accounts k ON k.id = g.keeper_id";

/// Everything [`plan_from`] reads: a plan `p` and its author's account `a`.
const PLAN_COLUMNS: &str = "
    p.id, p.group_id, p.author_id, a.handle AS author_handle,
    a.display_name AS author_name, p.day, p.place, p.lat, p.lon, p.meet_at, p.note,
    p.created_at, p.changed_at";

const PLAN_FROM: &str = "FROM group_plans p JOIN accounts a ON a.id = p.author_id";

/// The order of [`Plan::key`].
const PLAN_ORDER: &str = "ORDER BY p.day, p.meet_at NULLS LAST, p.created_at, p.id";

fn count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A day to compare the days of plans with, as the database takes it. Its
/// dates end sooner in both directions than a caller's can; a day beyond
/// what a plan can be for compares like the first or the last such day.
fn bound(day: NaiveDate) -> NaiveDate {
    let (first, last) = days_kept();
    day.clamp(first, last)
}

fn group_from(row: &Row) -> Group {
    Group {
        id: row.get("id"),
        name: row.get("name"),
        created_at: row.get("created_at"),
        keeper: GroupPilot {
            account_id: row.get("keeper_id"),
            handle: row.get("keeper_handle"),
            display_name: row.get("keeper_name"),
        },
        members: count(row.get("members")),
    }
}

/// A plan without those who are in; [`with_pilots`] adds them.
fn plan_from(row: &Row) -> Result<Plan, StoreError> {
    let (lat, lon): (Option<f64>, Option<f64>) = (row.get("lat"), row.get("lon"));
    let position = lat
        .zip(lon)
        .map(|(lat, lon)| GeoPoint::new(lat, lon).map_err(|e| corrupt("plan position", e)))
        .transpose()?;
    Ok(Plan {
        id: row.get("id"),
        group_id: row.get("group_id"),
        author: GroupPilot {
            account_id: row.get("author_id"),
            handle: row.get("author_handle"),
            display_name: row.get("author_name"),
        },
        details: PlanDetails {
            day: row.get("day"),
            place: row.get("place"),
            position,
            meet_at: row.get("meet_at"),
            note: row.get("note"),
        },
        created_at: row.get("created_at"),
        changed_at: row.get("changed_at"),
        pilots_in: Vec::new(),
    })
}

fn is(error: &tokio_postgres::Error, state: &SqlState) -> bool {
    error.code() == Some(state)
}

/// Whether a failure is what two calls at the same moment can do to each
/// other, so that the same call made again goes through: a row that was
/// counted on went away before the transaction ended, or two transactions
/// waited for each other and the database ended one.
pub(super) fn is_a_race(error: &tokio_postgres::Error) -> bool {
    [
        SqlState::FOREIGN_KEY_VIOLATION,
        SqlState::T_R_DEADLOCK_DETECTED,
        SqlState::T_R_SERIALIZATION_FAILURE,
    ]
    .iter()
    .any(|state| is(error, state))
}

// --- Reading ------------------------------------------------------------------

async fn read_group(db: &impl GenericClient, id: &str) -> Result<Option<Group>, StoreError> {
    let query = format!("SELECT {GROUP_COLUMNS} {GROUP_FROM} WHERE g.id = $1");
    let statement = db.prepare_cached(&query).await.map_err(db_error)?;
    let row = db.query_opt(&statement, &[&id]).await.map_err(db_error)?;
    Ok(row.as_ref().map(group_from))
}

/// A group that the calling transaction holds or has just made.
async fn held_group(tx: &Transaction<'_>, id: &str) -> Result<Group, StoreError> {
    read_group(tx, id)
        .await?
        .ok_or_else(|| corrupt("group", "it is gone while its row is held"))
}

async fn read_members(db: &impl GenericClient, id: &str) -> Result<Vec<GroupPilot>, StoreError> {
    let statement = db
        .prepare_cached(
            "SELECT m.account_id, a.handle, a.display_name
             FROM group_members m JOIN accounts a ON a.id = m.account_id
             WHERE m.group_id = $1
             ORDER BY m.joined_at, m.account_id",
        )
        .await
        .map_err(db_error)?;
    let rows = db.query(&statement, &[&id]).await.map_err(db_error)?;
    Ok(rows
        .iter()
        .map(|row| GroupPilot {
            account_id: row.get(0),
            handle: row.get(1),
            display_name: row.get(2),
        })
        .collect())
}

/// A group with its members, counted as they are listed.
async fn held_with_members(tx: &Transaction<'_>, id: &str) -> Result<GroupMembers, StoreError> {
    let mut group = held_group(tx, id).await?;
    let members = read_members(tx, id).await?;
    group.members = count(i64::try_from(members.len()).unwrap_or(i64::MAX));
    Ok(GroupMembers { group, members })
}

/// Adds to each plan those who are in.
async fn with_pilots(
    db: &impl GenericClient,
    mut plans: Vec<Plan>,
) -> Result<Vec<Plan>, StoreError> {
    if plans.is_empty() {
        return Ok(plans);
    }
    let ids: Vec<&str> = plans.iter().map(|plan| plan.id.as_str()).collect();
    let statement = db
        .prepare_cached(
            "SELECT i.plan_id, i.account_id, a.handle, a.display_name
             FROM group_plan_pilots i JOIN accounts a ON a.id = i.account_id
             WHERE i.plan_id = ANY($1)
             ORDER BY i.plan_id, i.since, i.account_id",
        )
        .await
        .map_err(db_error)?;
    let rows = db.query(&statement, &[&ids]).await.map_err(db_error)?;
    let mut by_plan: BTreeMap<String, Vec<GroupPilot>> = BTreeMap::new();
    for row in &rows {
        by_plan.entry(row.get(0)).or_default().push(GroupPilot {
            account_id: row.get(1),
            handle: row.get(2),
            display_name: row.get(3),
        });
    }
    for plan in &mut plans {
        plan.pilots_in = by_plan.remove(&plan.id).unwrap_or_default();
    }
    Ok(plans)
}

/// A plan that the calling transaction holds or has just made.
async fn held_plan(tx: &Transaction<'_>, id: &str) -> Result<Plan, StoreError> {
    let query = format!("SELECT {PLAN_COLUMNS} {PLAN_FROM} WHERE p.id = $1");
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let row = tx.query_opt(&statement, &[&id]).await.map_err(db_error)?;
    let plan = row
        .as_ref()
        .map(plan_from)
        .transpose()?
        .ok_or_else(|| corrupt("plan", "it is gone while its row is held"))?;
    let mut plans = with_pilots(tx, vec![plan]).await?;
    plans
        .pop()
        .ok_or_else(|| corrupt("plan", "it is gone while its row is held"))
}

/// A transaction that reads everything as of one moment.
async fn at_one_moment(client: &mut Client) -> Result<Transaction<'_>, StoreError> {
    client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await
        .map_err(db_error)
}

// --- Locks ----------------------------------------------------------------------

/// Locks an account's row for the rest of the transaction. `false` if
/// there is no such account.
async fn lock_account(tx: &Transaction<'_>, id: &str) -> Result<bool, tokio_postgres::Error> {
    let row = tx
        .query_opt("SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE", &[&id])
        .await?;
    Ok(row.is_some())
}

/// Locks a group's row for the rest of the transaction and returns the
/// account that keeps it. `None` if there is no such group.
async fn lock_group(tx: &Transaction<'_>, id: &str) -> Result<Option<String>, StoreError> {
    let statement = tx
        .prepare_cached("SELECT keeper_id FROM groups WHERE id = $1 FOR UPDATE")
        .await
        .map_err(db_error)?;
    let row = tx.query_opt(&statement, &[&id]).await.map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

/// Whether an account is in a group. The member's row is held against
/// deletion for the rest of the transaction.
async fn is_member(tx: &Transaction<'_>, id: &str, account_id: &str) -> Result<bool, StoreError> {
    let statement = tx
        .prepare_cached(
            "SELECT 1 FROM group_members WHERE group_id = $1 AND account_id = $2 FOR KEY SHARE",
        )
        .await
        .map_err(db_error)?;
    let row = tx
        .query_opt(&statement, &[&id, &account_id])
        .await
        .map_err(db_error)?;
    Ok(row.is_some())
}

/// Locks a group's row for one of its members and returns the account that
/// keeps it. `None` if there is no such group or the account is not in it.
async fn lock_as_member(
    tx: &Transaction<'_>,
    id: &str,
    account_id: &str,
) -> Result<Option<String>, StoreError> {
    if !can_name(&[id, account_id]) {
        return Ok(None);
    }
    let Some(keeper) = lock_group(tx, id).await? else {
        return Ok(None);
    };
    Ok(is_member(tx, id, account_id).await?.then_some(keeper))
}

/// Locks a group's row for something only its keeper may do. The inner
/// error is what to answer an account that may not.
async fn lock_as_keeper<T>(
    tx: &Transaction<'_>,
    id: &str,
    keeper_id: &str,
) -> Result<Result<(), KeeperAct<T>>, StoreError> {
    Ok(match lock_as_member(tx, id, keeper_id).await? {
        None => Err(KeeperAct::NoSuchGroup),
        Some(keeper) if keeper != keeper_id => Err(KeeperAct::NotKeeper),
        Some(_) => Ok(()),
    })
}

/// The account of the member of a group with this handle. With `hold`,
/// the member's row is held against deletion for the rest of the
/// transaction.
async fn member_with(
    tx: &Transaction<'_>,
    id: &str,
    handle: &str,
    hold: bool,
) -> Result<Option<String>, StoreError> {
    if !can_name(&[handle]) {
        return Ok(None);
    }
    let lock = if hold { "FOR KEY SHARE OF m" } else { "" };
    let query = format!(
        "SELECT m.account_id FROM group_members m JOIN accounts a ON a.id = m.account_id
         WHERE m.group_id = $1 AND a.handle = $2 {lock}"
    );
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let row = tx
        .query_opt(&statement, &[&id, &handle])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

/// How many groups an account is in.
async fn groups_with(tx: &Transaction<'_>, account_id: &str) -> Result<i64, StoreError> {
    let row = tx
        .query_one(
            "SELECT count(*) FROM group_members WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.get(0))
}

// --- Groups ---------------------------------------------------------------------

pub(super) async fn create(
    client: &mut Client,
    new: &NewGroup,
    room: &GroupRoom,
) -> Result<GroupCreation, StoreError> {
    check_group(new)?;
    let no_account =
        || StoreError::Invalid("a group for an account that does not exist".to_owned());
    let tx = client.transaction().await.map_err(db_error)?;
    if !lock_account(&tx, &new.keeper_id).await.map_err(db_error)? {
        return Err(no_account());
    }
    if groups_with(&tx, &new.keeper_id).await? >= i64::from(room.groups_per_account) {
        return Ok(GroupCreation::TooManyGroups);
    }
    let stored = tx
        .execute(
            "INSERT INTO groups (id, name, created_at, keeper_id, invite_hash)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &new.id,
                &new.name,
                &new.created_at,
                &new.keeper_id,
                &new.invite.0.as_slice(),
            ],
        )
        .await;
    match stored {
        Ok(_) => {}
        Err(error) if is(&error, &SqlState::UNIQUE_VIOLATION) => {
            return Err(StoreError::Invalid(
                "a group under an identifier or an invite that is in use".to_owned(),
            ));
        }
        Err(error) => return Err(db_error(error)),
    }
    tx.execute(
        "INSERT INTO group_members (group_id, account_id, joined_at) VALUES ($1, $2, $3)",
        &[&new.id, &new.keeper_id, &new.created_at],
    )
    .await
    .map_err(db_error)?;
    let group = held_group(&tx, &new.id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(GroupCreation::Created(group))
}

pub(super) async fn join(
    client: &mut Client,
    account_id: &str,
    invite: &SecretHash,
    room: &GroupRoom,
    now: Timestamp,
) -> Result<GroupJoining, StoreError> {
    let no_account =
        || StoreError::Invalid("a place in a group for an account that does not exist".to_owned());
    if !can_name(&[account_id]) {
        return Err(no_account());
    }
    let tx = client.transaction().await.map_err(db_error)?;
    if !lock_account(&tx, account_id).await.map_err(db_error)? {
        return Err(no_account());
    }
    // The row is locked as the row of this code. A call that replaces the
    // code holds the row while it does: this then waits, looks again and
    // finds no group under the code.
    let found = tx
        .query_opt(
            "SELECT id FROM groups WHERE invite_hash = $1 FOR UPDATE",
            &[&invite.0.as_slice()],
        )
        .await
        .map_err(db_error)?;
    let Some(found) = found else {
        return Ok(GroupJoining::NoSuchInvite);
    };
    let id: &str = found.get(0);
    if is_member(&tx, id, account_id).await? {
        return Ok(GroupJoining::AlreadyIn(held_group(&tx, id).await?));
    }
    if groups_with(&tx, account_id).await? >= i64::from(room.groups_per_account) {
        return Ok(GroupJoining::TooManyGroups);
    }
    let members: i64 = tx
        .query_one(
            "SELECT count(*) FROM group_members WHERE group_id = $1",
            &[&id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if members >= i64::from(room.members_per_group) {
        return Ok(GroupJoining::GroupFull);
    }
    tx.execute(
        "INSERT INTO group_members (group_id, account_id, joined_at) VALUES ($1, $2, $3)",
        &[&id, &account_id, &now],
    )
    .await
    .map_err(db_error)?;
    let group = held_group(&tx, id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(GroupJoining::Joined(group))
}

pub(super) async fn of_account(
    client: &Client,
    account_id: &str,
) -> Result<Vec<Group>, StoreError> {
    if !can_name(&[account_id]) {
        return Ok(Vec::new());
    }
    let query = format!(
        "SELECT {GROUP_COLUMNS} {GROUP_FROM}
         JOIN group_members m ON m.group_id = g.id
         WHERE m.account_id = $1
         ORDER BY m.joined_at, g.id"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(group_from).collect())
}

pub(super) async fn get(
    client: &mut Client,
    id: &str,
    account_id: &str,
) -> Result<Option<GroupMembers>, StoreError> {
    if !can_name(&[id, account_id]) {
        return Ok(None);
    }
    let tx = at_one_moment(client).await?;
    let Some(mut group) = read_group(&tx, id).await? else {
        return Ok(None);
    };
    let members = read_members(&tx, id).await?;
    if !members.iter().any(|member| member.account_id == account_id) {
        return Ok(None);
    }
    group.members = count(i64::try_from(members.len()).unwrap_or(i64::MAX));
    Ok(Some(GroupMembers { group, members }))
}

pub(super) async fn rename(
    client: &mut Client,
    id: &str,
    keeper_id: &str,
    name: &str,
) -> Result<KeeperAct<GroupMembers>, StoreError> {
    check_name(name)?;
    let tx = client.transaction().await.map_err(db_error)?;
    if let Err(refused) = lock_as_keeper(&tx, id, keeper_id).await? {
        return Ok(refused);
    }
    tx.execute("UPDATE groups SET name = $2 WHERE id = $1", &[&id, &name])
        .await
        .map_err(db_error)?;
    let group = held_with_members(&tx, id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(KeeperAct::Done(group))
}

pub(super) async fn renew_invite(
    client: &mut Client,
    id: &str,
    keeper_id: &str,
    invite: &SecretHash,
) -> Result<KeeperAct<()>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    if let Err(refused) = lock_as_keeper(&tx, id, keeper_id).await? {
        return Ok(refused);
    }
    let renewed = tx
        .execute(
            "UPDATE groups SET invite_hash = $2 WHERE id = $1",
            &[&id, &invite.0.as_slice()],
        )
        .await;
    match renewed {
        Ok(_) => {}
        Err(error) if is(&error, &SqlState::UNIQUE_VIOLATION) => {
            return Err(StoreError::Invalid(
                "an invite that another group has".to_owned(),
            ));
        }
        Err(error) => return Err(db_error(error)),
    }
    tx.commit().await.map_err(db_error)?;
    Ok(KeeperAct::Done(()))
}

pub(super) async fn hand_over(
    client: &mut Client,
    id: &str,
    keeper_id: &str,
    member_handle: &str,
) -> Result<KeeperAct<GroupMembers>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    if let Err(refused) = lock_as_keeper(&tx, id, keeper_id).await? {
        return Ok(refused);
    }
    // Held until the keeping has passed: an account that is deleted at
    // this moment waits, and then finds that it keeps this group.
    let Some(member) = member_with(&tx, id, member_handle, true).await? else {
        return Ok(KeeperAct::NoSuchMember);
    };
    tx.execute(
        "UPDATE groups SET keeper_id = $2 WHERE id = $1",
        &[&id, &member],
    )
    .await
    .map_err(db_error)?;
    let group = held_with_members(&tx, id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(KeeperAct::Done(group))
}

/// The member who keeps a group when `leaving` no longer does: the one who
/// has been in it longest. That member's row is held against deletion for
/// the rest of the transaction. `None` if nobody else is in the group.
///
/// The caller holds the group's row, so nobody comes in meanwhile.
async fn next_keeper(
    tx: &Transaction<'_>,
    id: &str,
    leaving: &str,
) -> Result<Option<String>, tokio_postgres::Error> {
    loop {
        let found = tx
            .query_opt(
                "SELECT account_id FROM group_members
                 WHERE group_id = $1 AND account_id <> $2
                 ORDER BY joined_at, account_id LIMIT 1
                 FOR KEY SHARE",
                &[&id, &leaving],
            )
            .await?;
        if let Some(row) = found {
            return Ok(Some(row.get(0)));
        }
        // No row can also mean that the member found was deleted while
        // this waited for its row: a deleted account takes its places in
        // groups along without asking. Then the next one is asked for.
        let others: bool = tx
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM group_members
                                WHERE group_id = $1 AND account_id <> $2)",
                &[&id, &leaving],
            )
            .await?
            .get(0);
        if !others {
            return Ok(None);
        }
    }
}

/// Makes a group ready for its keeper to go: passes the keeping on, or
/// removes the group if the keeper is alone in it. Says whether the group
/// is gone. The caller holds the group's row.
async fn release_keeper(
    tx: &Transaction<'_>,
    id: &str,
    keeper_id: &str,
) -> Result<bool, tokio_postgres::Error> {
    match next_keeper(tx, id, keeper_id).await? {
        Some(next) => {
            tx.execute(
                "UPDATE groups SET keeper_id = $2 WHERE id = $1",
                &[&id, &next],
            )
            .await?;
            Ok(false)
        }
        None => {
            // Its members' places and its plans go with it.
            tx.execute("DELETE FROM groups WHERE id = $1", &[&id])
                .await?;
            Ok(true)
        }
    }
}

pub(super) async fn remove_member(
    client: &mut Client,
    id: &str,
    by_account_id: &str,
    member_handle: &str,
) -> Result<MemberRemoval, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(keeper) = lock_as_member(&tx, id, by_account_id).await? else {
        return Ok(MemberRemoval::NoSuchGroup);
    };
    let member = member_with(&tx, id, member_handle, false).await?;
    let leaving = member.as_deref() == Some(by_account_id);
    if !leaving && keeper != by_account_id {
        return Ok(MemberRemoval::NotKeeper);
    }
    let Some(member) = member else {
        return Ok(MemberRemoval::NoSuchMember);
    };
    if !leaving {
        // Whoever is removed knew the code.
        tx.execute("UPDATE groups SET invite_hash = NULL WHERE id = $1", &[&id])
            .await
            .map_err(db_error)?;
    }
    if member == keeper && release_keeper(&tx, id, &member).await.map_err(db_error)? {
        tx.commit().await.map_err(db_error)?;
        return Ok(MemberRemoval::GroupRemoved);
    }
    // The member's plans and their word on the plans of others go with
    // this row.
    tx.execute(
        "DELETE FROM group_members WHERE group_id = $1 AND account_id = $2",
        &[&id, &member],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(MemberRemoval::Removed)
}

pub(super) async fn delete(
    client: &mut Client,
    id: &str,
    keeper_id: &str,
) -> Result<KeeperAct<()>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    if let Err(refused) = lock_as_keeper(&tx, id, keeper_id).await? {
        return Ok(refused);
    }
    tx.execute("DELETE FROM groups WHERE id = $1", &[&id])
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(KeeperAct::Done(()))
}

/// Locks an account's row and takes the account out of the keeping of
/// every group it keeps, as part of deleting it: each such group passes to
/// its next keeper or, with nobody else in it, is removed. `false` if
/// there is no such account.
///
/// The account's places in groups, with what it put into them, go with
/// the account itself.
pub(super) async fn release_account(
    tx: &Transaction<'_>,
    account_id: &str,
) -> Result<bool, tokio_postgres::Error> {
    if !lock_account(tx, account_id).await? {
        return Ok(false);
    }
    // In the order of their identifiers, so that two accounts that are
    // deleted at once take the rows of the groups in the same order.
    let kept = tx
        .query(
            "SELECT id FROM groups WHERE keeper_id = $1 ORDER BY id FOR UPDATE",
            &[&account_id],
        )
        .await?;
    for row in &kept {
        let id: &str = row.get(0);
        release_keeper(tx, id, account_id).await?;
    }
    Ok(true)
}

// --- Plans -----------------------------------------------------------------------

/// Where a plan's place is, as the two columns that hold it.
fn degrees(details: &PlanDetails) -> (Option<f64>, Option<f64>) {
    (
        details.position.map(|position| position.lat()),
        details.position.map(|position| position.lon()),
    )
}

pub(super) async fn save_plan(
    client: &mut Client,
    new: &NewPlan,
    room: &PlanRoom,
) -> Result<PlanSaving, StoreError> {
    check_plan(new)?;
    let tx = client.transaction().await.map_err(db_error)?;
    // The group's row is held so that the plans it holds can be counted.
    if lock_as_member(&tx, &new.group_id, &new.author_id)
        .await?
        .is_none()
    {
        return Ok(PlanSaving::NoSuchGroup);
    }
    let held: i64 = tx
        .query_one(
            "SELECT count(*) FROM group_plans WHERE group_id = $1 AND day >= $2",
            &[&new.group_id, &bound(room.gone_before)],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if held >= i64::from(room.plans_per_group) {
        return Ok(PlanSaving::GroupFull);
    }
    let details = &new.details;
    let (lat, lon) = degrees(details);
    let stored = tx
        .execute(
            "INSERT INTO group_plans
                 (id, group_id, author_id, day, place, lat, lon, meet_at, note,
                  created_at, changed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)",
            &[
                &new.id,
                &new.group_id,
                &new.author_id,
                &details.day,
                &details.place,
                &lat,
                &lon,
                &details.meet_at,
                &details.note,
                &new.created_at,
            ],
        )
        .await;
    match stored {
        Ok(_) => {}
        Err(error) if is(&error, &SqlState::UNIQUE_VIOLATION) => {
            return Err(StoreError::Invalid(
                "a plan under an identifier that is in use".to_owned(),
            ));
        }
        Err(error) => return Err(db_error(error)),
    }
    tx.execute(
        "INSERT INTO group_plan_pilots (plan_id, group_id, account_id, since)
         VALUES ($1, $2, $3, $4)",
        &[&new.id, &new.group_id, &new.author_id, &new.created_at],
    )
    .await
    .map_err(db_error)?;
    let plan = held_plan(&tx, &new.id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(PlanSaving::Stored(Box::new(plan)))
}

/// Whether an account is in a group, for a call that changes one plan. The
/// member's row is held against deletion for the rest of the transaction.
async fn acts_as_member(
    tx: &Transaction<'_>,
    group_id: &str,
    account_id: &str,
) -> Result<bool, StoreError> {
    Ok(can_name(&[group_id, account_id]) && is_member(tx, group_id, account_id).await?)
}

pub(super) async fn change_plan(
    client: &mut Client,
    (group_id, plan_id): (&str, &str),
    by_account_id: &str,
    details: &PlanDetails,
    now: Timestamp,
    gone_before: NaiveDate,
) -> Result<PlanChange, StoreError> {
    check_details(details)?;
    let tx = client.transaction().await.map_err(db_error)?;
    if !acts_as_member(&tx, group_id, by_account_id).await? {
        return Ok(PlanChange::NoSuchGroup);
    }
    if !can_name(&[plan_id]) {
        return Ok(PlanChange::NoSuchPlan);
    }
    let found = tx
        .query_opt(
            "SELECT author_id FROM group_plans
             WHERE id = $1 AND group_id = $2 AND day >= $3 FOR UPDATE",
            &[&plan_id, &group_id, &bound(gone_before)],
        )
        .await
        .map_err(db_error)?;
    let Some(found) = found else {
        return Ok(PlanChange::NoSuchPlan);
    };
    if found.get::<_, &str>(0) != by_account_id {
        return Ok(PlanChange::NotAuthor);
    }
    let (lat, lon) = degrees(details);
    tx.execute(
        "UPDATE group_plans
         SET day = $2, place = $3, lat = $4, lon = $5, meet_at = $6, note = $7, changed_at = $8
         WHERE id = $1",
        &[
            &plan_id,
            &details.day,
            &details.place,
            &lat,
            &lon,
            &details.meet_at,
            &details.note,
            &now,
        ],
    )
    .await
    .map_err(db_error)?;
    let plan = held_plan(&tx, plan_id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(PlanChange::Now(Box::new(plan)))
}

pub(super) async fn delete_plan(
    client: &mut Client,
    group_id: &str,
    plan_id: &str,
    by_account_id: &str,
    gone_before: NaiveDate,
) -> Result<PlanRemoval, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    if !acts_as_member(&tx, group_id, by_account_id).await? {
        return Ok(PlanRemoval::NoSuchGroup);
    }
    if !can_name(&[plan_id]) {
        return Ok(PlanRemoval::NoSuchPlan);
    }
    let found = tx
        .query_opt(
            "SELECT p.author_id, g.keeper_id
             FROM group_plans p JOIN groups g ON g.id = p.group_id
             WHERE p.id = $1 AND p.group_id = $2 AND p.day >= $3 FOR UPDATE OF p",
            &[&plan_id, &group_id, &bound(gone_before)],
        )
        .await
        .map_err(db_error)?;
    let Some(found) = found else {
        return Ok(PlanRemoval::NoSuchPlan);
    };
    let (author, keeper): (&str, &str) = (found.get(0), found.get(1));
    if author != by_account_id && keeper != by_account_id {
        return Ok(PlanRemoval::NotAllowed);
    }
    tx.execute("DELETE FROM group_plans WHERE id = $1", &[&plan_id])
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(PlanRemoval::Removed)
}

pub(super) async fn set_in(
    client: &mut Client,
    (group_id, plan_id): (&str, &str),
    account_id: &str,
    is_in: bool,
    now: Timestamp,
    gone_before: NaiveDate,
) -> Result<PlanChange, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    if !acts_as_member(&tx, group_id, account_id).await? {
        return Ok(PlanChange::NoSuchGroup);
    }
    if !can_name(&[plan_id]) {
        return Ok(PlanChange::NoSuchPlan);
    }
    // Held against deletion until the pilot's word is stored with it.
    let there = tx
        .query_opt(
            "SELECT 1 FROM group_plans
             WHERE id = $1 AND group_id = $2 AND day >= $3 FOR KEY SHARE",
            &[&plan_id, &group_id, &bound(gone_before)],
        )
        .await
        .map_err(db_error)?;
    if there.is_none() {
        return Ok(PlanChange::NoSuchPlan);
    }
    if is_in {
        tx.execute(
            "INSERT INTO group_plan_pilots (plan_id, group_id, account_id, since)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (plan_id, account_id) DO NOTHING",
            &[&plan_id, &group_id, &account_id, &now],
        )
        .await
        .map_err(db_error)?;
    } else {
        tx.execute(
            "DELETE FROM group_plan_pilots WHERE plan_id = $1 AND account_id = $2",
            &[&plan_id, &account_id],
        )
        .await
        .map_err(db_error)?;
    }
    let plan = held_plan(&tx, plan_id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(PlanChange::Now(Box::new(plan)))
}

pub(super) async fn plans_of_group(
    client: &mut Client,
    group_id: &str,
    account_id: &str,
    from: NaiveDate,
    to: Option<NaiveDate>,
) -> Result<Option<Vec<Plan>>, StoreError> {
    if !can_name(&[group_id, account_id]) {
        return Ok(None);
    }
    let tx = at_one_moment(client).await?;
    let member = tx
        .query_opt(
            "SELECT 1 FROM group_members WHERE group_id = $1 AND account_id = $2",
            &[&group_id, &account_id],
        )
        .await
        .map_err(db_error)?;
    if member.is_none() {
        return Ok(None);
    }
    let query = format!(
        "SELECT {PLAN_COLUMNS} {PLAN_FROM}
         WHERE p.group_id = $1 AND p.day >= $2 AND ($3::date IS NULL OR p.day <= $3)
         {PLAN_ORDER}"
    );
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let rows = tx
        .query(&statement, &[&group_id, &bound(from), &to.map(bound)])
        .await
        .map_err(db_error)?;
    let plans = rows.iter().map(plan_from).collect::<Result<_, _>>()?;
    Ok(Some(with_pilots(&tx, plans).await?))
}

pub(super) async fn plans_for(
    client: &mut Client,
    account_id: &str,
    from: NaiveDate,
    to: Option<NaiveDate>,
    limit: usize,
) -> Result<Vec<Plan>, StoreError> {
    if !can_name(&[account_id]) {
        return Ok(Vec::new());
    }
    let tx = at_one_moment(client).await?;
    // Only through the account's own places in groups: a plan of a group
    // it is not in cannot be reached from here.
    let query = format!(
        "SELECT {PLAN_COLUMNS} {PLAN_FROM}
         JOIN group_members m ON m.group_id = p.group_id AND m.account_id = $1
         WHERE p.day >= $2 AND ($3::date IS NULL OR p.day <= $3)
         {PLAN_ORDER}
         LIMIT $4"
    );
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = tx
        .query(
            &statement,
            &[&account_id, &bound(from), &to.map(bound), &limit],
        )
        .await
        .map_err(db_error)?;
    let plans = rows.iter().map(plan_from).collect::<Result<_, _>>()?;
    with_pilots(&tx, plans).await
}

pub(super) async fn prune_plans(client: &Client, before: NaiveDate) -> Result<u64, StoreError> {
    client
        .execute("DELETE FROM group_plans WHERE day < $1", &[&bound(before)])
        .await
        .map_err(db_error)
}
