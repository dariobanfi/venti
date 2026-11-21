//! The sites pilots keep as their own, in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Transaction;
use venti_domain::Timestamp;

use super::db_error;
use crate::flight::next_change;
use crate::kept_site::kept_from;
use crate::{
    ForgottenSite, KeepingSite, KeptRoom, KeptSite, KeptSiteChanges, NewKeptSite, StoreError,
};

/// Locks an account's kept sites for the rest of the transaction and
/// returns the largest change number they have used. `None` if there is
/// no such account.
///
/// The account's row is held against deletion first, and then the row of
/// the change numbers is made if it is not there and locked: two writes of
/// one account come one after the other.
async fn lock(tx: &Transaction<'_>, account_id: &str) -> Result<Option<i64>, StoreError> {
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR KEY SHARE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(None);
    }
    let row = tx
        .query_one(
            "INSERT INTO kept_site_changes (account_id, latest) VALUES ($1, 0)
             ON CONFLICT (account_id) DO UPDATE SET latest = kept_site_changes.latest
             RETURNING latest",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(Some(row.get(0)))
}

pub(super) async fn keep(
    client: &mut Client,
    account_id: &str,
    new: &NewKeptSite,
    now: Timestamp,
    room: KeptRoom,
) -> Result<KeepingSite, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(latest) = lock(&tx, account_id).await? else {
        return Ok(KeepingSite::NoAccount);
    };
    let have = tx
        .query_opt(
            "SELECT source, kept_at, record, change FROM kept_sites
             WHERE account_id = $1 AND site_id = $2 AND record IS NOT NULL",
            &[&account_id, &new.site_id],
        )
        .await
        .map_err(db_error)?
        .map(|row| KeptSite {
            site_id: new.site_id.clone(),
            source: row.get("source"),
            kept_at: row.get("kept_at"),
            record: row.get("record"),
            change: row.get("change"),
        });
    let Some(kept_at) = kept_from(have.as_ref().map(|have| have.kept_at), new.kept_at, now) else {
        tx.commit().await.map_err(db_error)?;
        // `kept_from` leaves alone only what the account has.
        return Ok(have.map_or(KeepingSite::Full, KeepingSite::Kept));
    };
    let others: i64 = tx
        .query_one(
            "SELECT count(*) FROM kept_sites
             WHERE account_id = $1 AND record IS NOT NULL AND site_id <> $2",
            &[&account_id, &new.site_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if usize::try_from(others).unwrap_or(usize::MAX) >= room.sites {
        return Ok(KeepingSite::Full);
    }

    let change = next_change(latest, now);
    tx.execute(
        "UPDATE kept_site_changes SET latest = $2 WHERE account_id = $1",
        &[&account_id, &change],
    )
    .await
    .map_err(db_error)?;
    tx.execute(
        "INSERT INTO kept_sites (account_id, site_id, source, change, kept_at, record)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (account_id, site_id) DO UPDATE
         SET source = EXCLUDED.source, change = EXCLUDED.change,
             kept_at = EXCLUDED.kept_at, record = EXCLUDED.record",
        &[
            &account_id,
            &new.site_id,
            &new.source,
            &change,
            &kept_at,
            &new.record,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(KeepingSite::Kept(KeptSite {
        site_id: new.site_id.clone(),
        source: new.source.clone(),
        kept_at,
        record: new.record.clone(),
        change,
    }))
}

pub(super) async fn forget(
    client: &mut Client,
    account_id: &str,
    site_id: &str,
    now: Timestamp,
    room: KeptRoom,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(latest) = lock(&tx, account_id).await? else {
        return Ok(false);
    };
    let change = next_change(latest, now);
    let forgotten = tx
        .execute(
            "UPDATE kept_sites SET kept_at = NULL, record = NULL, change = $3
             WHERE account_id = $1 AND site_id = $2 AND record IS NOT NULL",
            &[&account_id, &site_id, &change],
        )
        .await
        .map_err(db_error)?;
    if forgotten == 0 {
        // Nothing was kept under this identifier, and no number is taken.
        tx.commit().await.map_err(db_error)?;
        return Ok(false);
    }

    // Of the forgotten, the newest are remembered.
    let keep = i64::try_from(room.forgotten).unwrap_or(i64::MAX);
    let dropped: Option<i64> = tx
        .query_one(
            "WITH old AS (
                 SELECT site_id, change FROM kept_sites
                 WHERE account_id = $1 AND record IS NULL
                 ORDER BY change DESC OFFSET $2
             ), gone AS (
                 DELETE FROM kept_sites k USING old
                 WHERE k.account_id = $1 AND k.site_id = old.site_id
                 RETURNING old.change
             )
             SELECT max(change) FROM gone",
            &[&account_id, &keep],
        )
        .await
        .map_err(db_error)?
        .get(0);
    tx.execute(
        "UPDATE kept_site_changes SET latest = $2, floor = GREATEST(floor, $3)
         WHERE account_id = $1",
        &[&account_id, &change, &dropped.unwrap_or(0)],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn changes(
    client: &Client,
    account_id: &str,
    since: i64,
    limit: usize,
) -> Result<KeptSiteChanges, StoreError> {
    let more_than_limit = i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1);
    // One statement, and so one moment: the numbers and the changes agree.
    let statement = client
        .prepare_cached(
            "SELECT c.latest, c.floor, k.site_id, k.source, k.kept_at, k.record, k.change
             FROM kept_site_changes c
             LEFT JOIN LATERAL (
                 SELECT site_id, source, kept_at, record, change FROM kept_sites
                 WHERE account_id = c.account_id AND change > $2
                 ORDER BY change LIMIT $3
             ) k ON true
             WHERE c.account_id = $1
             ORDER BY k.change",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id, &since, &more_than_limit])
        .await
        .map_err(db_error)?;

    let mut changes = KeptSiteChanges::default();
    for row in rows.iter().take(limit) {
        changes.latest = row.get("latest");
        changes.floor = row.get("floor");
        // An account whose kept sites did not change has the numbers and
        // no site beside them.
        let Some(site_id) = row.get::<_, Option<String>>("site_id") else {
            continue;
        };
        let change: i64 = row.get("change");
        match row.get::<_, Option<String>>("record") {
            Some(record) => changes.kept.push(KeptSite {
                site_id,
                source: row.get("source"),
                kept_at: row.get("kept_at"),
                record,
                change,
            }),
            None => changes.forgotten.push(ForgottenSite { site_id, change }),
        }
    }
    changes.more = rows.len() > limit;
    Ok(changes)
}
