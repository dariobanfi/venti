//! Open plans in PostgreSQL.
//!
//! **The author's row first, then the plan's.** Posting a plan locks the
//! author's account so that the plans it holds can be counted and two
//! posts cannot share its last place; saying "I am in" locks the plan's
//! row so that two pilots cannot share its last place. Nothing locks in
//! the other order.
//!
//! The database holds the rest: a plan goes with its author's account and
//! a pilot's word with the pilot's (`0031_open_plans.sql`).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::Row;
use tokio_postgres::error::SqlState;
use venti_domain::{GeoPoint, Timestamp};

use super::{corrupt, db_error};
use crate::group::can_name;
use crate::open_plan::{bound, check_open_details};
use crate::{
    GroupPilot, NewOpenPlan, OpenPlan, OpenPlanArea, OpenPlanChange, OpenPlanDetails,
    OpenPlanRemoval, OpenPlanRoom, OpenPlanSaving, StoreError,
};

/// Everything [`plan_from`] reads: a plan `p` and its author's account `a`.
const COLUMNS: &str = "
    p.id, p.author_id, a.handle AS author_handle, a.display_name AS author_name, p.day,
    p.place, p.lat, p.lon, p.meet_at, p.seats, p.note, p.created_at, p.changed_at";

const FROM: &str = "FROM open_plans p JOIN accounts a ON a.id = p.author_id";

/// The order of [`OpenPlan::key`].
const ORDER: &str = "ORDER BY p.day, p.meet_at NULLS LAST, p.created_at, p.id";

/// The most rows one look at an area reads before the distance is
/// measured: far more than an answer carries, so that a crowded box does
/// not hide the plans near its middle in all but an unlikely case.
const MOST_IN_A_BOX: i64 = 2000;

/// A plan without those who are in; [`with_pilots`] adds them.
fn plan_from(row: &Row) -> Result<OpenPlan, StoreError> {
    let position = GeoPoint::new(row.get("lat"), row.get("lon"))
        .map_err(|error| corrupt("open plan position", error))?;
    let seats: Option<i16> = row.get("seats");
    Ok(OpenPlan {
        id: row.get("id"),
        author: GroupPilot {
            account_id: row.get("author_id"),
            handle: row.get("author_handle"),
            display_name: row.get("author_name"),
        },
        details: OpenPlanDetails {
            day: row.get("day"),
            place: row.get("place"),
            position,
            meet_at: row.get("meet_at"),
            seats: seats.and_then(|seats| u8::try_from(seats).ok()),
            note: row.get("note"),
        },
        created_at: row.get("created_at"),
        changed_at: row.get("changed_at"),
        pilots_in: Vec::new(),
    })
}

/// Adds to each plan those who are in.
async fn with_pilots(
    db: &impl GenericClient,
    mut plans: Vec<OpenPlan>,
) -> Result<Vec<OpenPlan>, StoreError> {
    if plans.is_empty() {
        return Ok(plans);
    }
    let ids: Vec<&str> = plans.iter().map(|plan| plan.id.as_str()).collect();
    let statement = db
        .prepare_cached(
            "SELECT i.plan_id, i.account_id, a.handle, a.display_name
             FROM open_plan_pilots i JOIN accounts a ON a.id = i.account_id
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

/// A plan as of now, with who is in. `None` if there is none for a day
/// from `gone_before` on.
async fn read_plan(
    db: &impl GenericClient,
    id: &str,
    gone_before: NaiveDate,
) -> Result<Option<OpenPlan>, StoreError> {
    let query = format!("SELECT {COLUMNS} {FROM} WHERE p.id = $1 AND p.day >= $2");
    let statement = db.prepare_cached(&query).await.map_err(db_error)?;
    let row = db
        .query_opt(&statement, &[&id, &bound(gone_before)])
        .await
        .map_err(db_error)?;
    let Some(plan) = row.as_ref().map(plan_from).transpose()? else {
        return Ok(None);
    };
    Ok(with_pilots(db, vec![plan]).await?.pop())
}

/// A plan that the calling transaction holds or has just made.
async fn held_plan(tx: &Transaction<'_>, id: &str) -> Result<OpenPlan, StoreError> {
    read_plan(tx, id, NaiveDate::MIN)
        .await?
        .ok_or_else(|| corrupt("open plan", "it is gone while its row is held"))
}

/// Locks a plan's row for the rest of the transaction and returns its
/// author. `None` if there is no such plan for a day from `gone_before` on.
async fn lock_plan(
    tx: &Transaction<'_>,
    id: &str,
    gone_before: NaiveDate,
) -> Result<Option<String>, StoreError> {
    let row = tx
        .query_opt(
            "SELECT author_id FROM open_plans WHERE id = $1 AND day >= $2 FOR UPDATE",
            &[&id, &bound(gone_before)],
        )
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

fn seats(details: &OpenPlanDetails) -> Option<i16> {
    details.seats.map(i16::from)
}

pub(super) async fn save(
    client: &mut Client,
    new: &NewOpenPlan,
    room: &OpenPlanRoom,
) -> Result<OpenPlanSaving, StoreError> {
    check_open_details(&new.details)?;
    if !can_name(&[&new.id, &new.author_id]) {
        return Err(StoreError::Invalid(
            "an open plan with a zero byte in an identifier".to_owned(),
        ));
    }
    let tx = client.transaction().await.map_err(db_error)?;
    // The author's row is held so that the plans it holds can be counted.
    let author = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&new.author_id],
        )
        .await
        .map_err(db_error)?;
    if author.is_none() {
        return Err(StoreError::Invalid(
            "an open plan by an account that does not exist".to_owned(),
        ));
    }
    let held: i64 = tx
        .query_one(
            "SELECT count(*) FROM open_plans WHERE author_id = $1 AND day >= $2",
            &[&new.author_id, &bound(room.gone_before)],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if held >= i64::from(room.plans_per_account) {
        return Ok(OpenPlanSaving::TooMany);
    }
    let details = &new.details;
    let stored = tx
        .execute(
            "INSERT INTO open_plans
                 (id, author_id, day, place, lat, lon, meet_at, seats, note, created_at,
                  changed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)",
            &[
                &new.id,
                &new.author_id,
                &details.day,
                &details.place,
                &details.position.lat(),
                &details.position.lon(),
                &details.meet_at,
                &seats(details),
                &details.note,
                &new.created_at,
            ],
        )
        .await;
    match stored {
        Ok(_) => {}
        Err(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
            return Err(StoreError::Invalid(
                "an open plan under an identifier that is in use".to_owned(),
            ));
        }
        Err(error) => return Err(db_error(error)),
    }
    tx.execute(
        "INSERT INTO open_plan_pilots (plan_id, account_id, since) VALUES ($1, $2, $3)",
        &[&new.id, &new.author_id, &new.created_at],
    )
    .await
    .map_err(db_error)?;
    let plan = held_plan(&tx, &new.id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(OpenPlanSaving::Stored(Box::new(plan)))
}

pub(super) async fn change(
    client: &mut Client,
    plan_id: &str,
    by_account_id: &str,
    details: &OpenPlanDetails,
    (now, gone_before): (Timestamp, NaiveDate),
) -> Result<OpenPlanChange, StoreError> {
    check_open_details(details)?;
    if !can_name(&[plan_id, by_account_id]) {
        return Ok(OpenPlanChange::NoSuchPlan);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(author) = lock_plan(&tx, plan_id, gone_before).await? else {
        return Ok(OpenPlanChange::NoSuchPlan);
    };
    if author != by_account_id {
        return Ok(OpenPlanChange::NotAuthor);
    }
    tx.execute(
        "UPDATE open_plans
         SET day = $2, place = $3, lat = $4, lon = $5, meet_at = $6, seats = $7, note = $8,
             changed_at = $9
         WHERE id = $1",
        &[
            &plan_id,
            &details.day,
            &details.place,
            &details.position.lat(),
            &details.position.lon(),
            &details.meet_at,
            &seats(details),
            &details.note,
            &now,
        ],
    )
    .await
    .map_err(db_error)?;
    let plan = held_plan(&tx, plan_id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(OpenPlanChange::Now(Box::new(plan)))
}

pub(super) async fn delete(
    client: &mut Client,
    plan_id: &str,
    by_account_id: &str,
    gone_before: NaiveDate,
) -> Result<OpenPlanRemoval, StoreError> {
    if !can_name(&[plan_id, by_account_id]) {
        return Ok(OpenPlanRemoval::NoSuchPlan);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(author) = lock_plan(&tx, plan_id, gone_before).await? else {
        return Ok(OpenPlanRemoval::NoSuchPlan);
    };
    if author != by_account_id {
        return Ok(OpenPlanRemoval::NotAuthor);
    }
    tx.execute("DELETE FROM open_plans WHERE id = $1", &[&plan_id])
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(OpenPlanRemoval::Removed)
}

pub(super) async fn set_in(
    client: &mut Client,
    plan_id: &str,
    account_id: &str,
    (is_in, now): (bool, Timestamp),
    room: &OpenPlanRoom,
) -> Result<OpenPlanChange, StoreError> {
    if !can_name(&[plan_id, account_id]) {
        return Ok(OpenPlanChange::NoSuchPlan);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    // The plan's row is held so that who is in can be counted.
    if lock_plan(&tx, plan_id, room.gone_before).await?.is_none() {
        return Ok(OpenPlanChange::NoSuchPlan);
    }
    if is_in {
        let there = tx
            .query_opt(
                "SELECT 1 FROM open_plan_pilots WHERE plan_id = $1 AND account_id = $2",
                &[&plan_id, &account_id],
            )
            .await
            .map_err(db_error)?;
        if there.is_none() {
            let held: i64 = tx
                .query_one(
                    "SELECT count(*) FROM open_plan_pilots WHERE plan_id = $1",
                    &[&plan_id],
                )
                .await
                .map_err(db_error)?
                .get(0);
            if held >= i64::from(room.pilots_per_plan) {
                return Ok(OpenPlanChange::Full);
            }
            let stored = tx
                .execute(
                    "INSERT INTO open_plan_pilots (plan_id, account_id, since)
                     VALUES ($1, $2, $3)",
                    &[&plan_id, &account_id, &now],
                )
                .await;
            match stored {
                Ok(_) => {}
                // An account that is not there, or went at this moment.
                Err(error) if error.code() == Some(&SqlState::FOREIGN_KEY_VIOLATION) => {
                    return Ok(OpenPlanChange::NoSuchPlan);
                }
                Err(error) => return Err(db_error(error)),
            }
        }
    } else {
        tx.execute(
            "DELETE FROM open_plan_pilots WHERE plan_id = $1 AND account_id = $2",
            &[&plan_id, &account_id],
        )
        .await
        .map_err(db_error)?;
    }
    let plan = held_plan(&tx, plan_id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(OpenPlanChange::Now(Box::new(plan)))
}

pub(super) async fn get(
    client: &Client,
    plan_id: &str,
    gone_before: NaiveDate,
) -> Result<Option<OpenPlan>, StoreError> {
    if !can_name(&[plan_id]) {
        return Ok(None);
    }
    read_plan(client, plan_id, gone_before).await
}

/// How many degrees of latitude and of longitude `radius_m` spans around
/// a place, a little more rather than less.
fn spans(area: &OpenPlanArea) -> (f64, f64) {
    let lat = area.radius_m / 111_000.0 * 1.01;
    let cos = area
        .around
        .lat()
        .abs()
        .min(89.0)
        .to_radians()
        .cos()
        .max(0.01);
    (lat, (lat / cos).min(360.0))
}

pub(super) async fn near(
    client: &Client,
    area: &OpenPlanArea,
    limit: usize,
) -> Result<Vec<OpenPlan>, StoreError> {
    let (lat_span, lon_span) = spans(area);
    let (lat, lon) = (area.around.lat(), area.around.lon());
    // The box first, in the database; then the distance, here. A box
    // across the antimeridian is asked for as the whole band of latitude.
    let (west, east) = if lon - lon_span < -180.0 || lon + lon_span > 180.0 {
        (-180.0, 180.0)
    } else {
        (lon - lon_span, lon + lon_span)
    };
    let query = format!(
        "SELECT {COLUMNS} {FROM}
         WHERE p.day >= $1 AND p.day <= $2 AND p.lat BETWEEN $3 AND $4
           AND p.lon BETWEEN $5 AND $6
         {ORDER}
         LIMIT $7"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let rows = client
        .query(
            &statement,
            &[
                &bound(area.from),
                &bound(area.to),
                &(lat - lat_span),
                &(lat + lat_span),
                &west,
                &east,
                &MOST_IN_A_BOX,
            ],
        )
        .await
        .map_err(db_error)?;
    let mut plans = Vec::new();
    for row in &rows {
        let plan = plan_from(row)?;
        if plan.is_near(area) {
            plans.push(plan);
        }
        if plans.len() == limit {
            break;
        }
    }
    with_pilots(client, plans).await
}

pub(super) async fn of(
    client: &Client,
    account_id: &str,
    from: NaiveDate,
    limit: usize,
) -> Result<Vec<OpenPlan>, StoreError> {
    if !can_name(&[account_id]) {
        return Ok(Vec::new());
    }
    let query = format!(
        "SELECT {COLUMNS} {FROM}
         WHERE p.day >= $2
           AND (p.author_id = $1 OR EXISTS (
               SELECT 1 FROM open_plan_pilots i WHERE i.plan_id = p.id AND i.account_id = $1))
         {ORDER}
         LIMIT $3"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(&statement, &[&account_id, &bound(from), &limit])
        .await
        .map_err(db_error)?;
    let plans = rows.iter().map(plan_from).collect::<Result<_, _>>()?;
    with_pilots(client, plans).await
}

pub(super) async fn remove(
    client: &mut Client,
    plan_id: &str,
    gone_before: NaiveDate,
) -> Result<Option<OpenPlan>, StoreError> {
    if !can_name(&[plan_id]) {
        return Ok(None);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    if lock_plan(&tx, plan_id, gone_before).await?.is_none() {
        return Ok(None);
    }
    let plan = held_plan(&tx, plan_id).await?;
    tx.execute("DELETE FROM open_plans WHERE id = $1", &[&plan_id])
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Some(plan))
}

pub(super) async fn prune(client: &Client, before: NaiveDate) -> Result<u64, StoreError> {
    client
        .execute("DELETE FROM open_plans WHERE day < $1", &[&bound(before)])
        .await
        .map_err(db_error)
}
