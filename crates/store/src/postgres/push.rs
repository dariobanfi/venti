//! Web Push subscriptions and the server's key, in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::Timestamp;

use super::db_error;
use crate::{AddingPush, PushSubscription, StoreError};

const COLUMNS: &str = "endpoint, account_id, p256dh, auth, created_at";

fn subscription(row: &Row) -> PushSubscription {
    PushSubscription {
        account_id: row.get("account_id"),
        endpoint: row.get("endpoint"),
        p256dh: row.get("p256dh"),
        auth: row.get("auth"),
        created_at: row.get("created_at"),
    }
}

pub(super) async fn add(
    client: &mut Client,
    new: &PushSubscription,
    most: usize,
) -> Result<AddingPush, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    // The account's row, held: two additions of one account come one
    // after the other, and the count holds.
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&new.account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(AddingPush::NoAccount);
    }
    let others: i64 = tx
        .query_one(
            "SELECT count(*) FROM push_subscriptions WHERE account_id = $1 AND endpoint <> $2",
            &[&new.account_id, &new.endpoint],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if usize::try_from(others).unwrap_or(usize::MAX) >= most {
        return Ok(AddingPush::Full);
    }
    tx.execute(
        "INSERT INTO push_subscriptions (endpoint, account_id, p256dh, auth, created_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (endpoint) DO UPDATE
         SET account_id = EXCLUDED.account_id, p256dh = EXCLUDED.p256dh,
             auth = EXCLUDED.auth, created_at = EXCLUDED.created_at",
        &[
            &new.endpoint,
            &new.account_id,
            &new.p256dh,
            &new.auth,
            &new.created_at,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(AddingPush::Added)
}

pub(super) async fn remove(
    client: &Client,
    account_id: &str,
    endpoint: &str,
) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM push_subscriptions WHERE account_id = $1 AND endpoint = $2",
            &[&account_id, &endpoint],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn drop(client: &Client, endpoint: &str) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM push_subscriptions WHERE endpoint = $1",
            &[&endpoint],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn of(
    client: &Client,
    account_id: &str,
) -> Result<Vec<PushSubscription>, StoreError> {
    let rows = client
        .query(
            &format!(
                "SELECT {COLUMNS} FROM push_subscriptions WHERE account_id = $1
                 ORDER BY created_at, endpoint"
            ),
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(subscription).collect())
}

pub(super) async fn key(client: &Client) -> Result<Option<String>, StoreError> {
    let row = client
        .query_opt("SELECT private_key FROM push_key", &[])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

pub(super) async fn keep_key(
    client: &Client,
    key: &str,
    now: Timestamp,
) -> Result<String, StoreError> {
    client
        .execute(
            "INSERT INTO push_key (private_key, created_at) VALUES ($1, $2)
             ON CONFLICT (one) DO NOTHING",
            &[&key, &now],
        )
        .await
        .map_err(db_error)?;
    let row = client
        .query_one("SELECT private_key FROM push_key", &[])
        .await
        .map_err(db_error)?;
    Ok(row.get(0))
}
