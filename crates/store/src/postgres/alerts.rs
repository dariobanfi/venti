//! The alarms pilots set, in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::Timestamp;

use super::db_error;
use crate::flight::next_change;
use crate::{AddingAlert, Alert, AlertCheck, FiredAlert, FiredAlerts, NewAlert, StoreError};

const COLUMNS: &str =
    "id, account_id, name, enabled, kind, station_id, rule, state, created_at, changed_at";

fn alert(row: &Row) -> Alert {
    Alert {
        id: row.get("id"),
        account_id: row.get("account_id"),
        name: row.get("name"),
        enabled: row.get("enabled"),
        kind: row.get("kind"),
        station_id: row.get("station_id"),
        rule: row.get("rule"),
        state: row.get("state"),
        created_at: row.get("created_at"),
        changed_at: row.get("changed_at"),
    }
}

pub(super) async fn of(client: &Client, account_id: &str) -> Result<Vec<Alert>, StoreError> {
    let rows = client
        .query(
            &format!("SELECT {COLUMNS} FROM alerts WHERE account_id = $1 ORDER BY created_at, id"),
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(alert).collect())
}

pub(super) async fn add(
    client: &mut Client,
    account_id: &str,
    new: &NewAlert,
    now: Timestamp,
    most: usize,
) -> Result<AddingAlert, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    // The account's row, held: two additions of one account come one
    // after the other, and the count holds.
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(AddingAlert::NoAccount);
    }
    let have: i64 = tx
        .query_one(
            "SELECT count(*) FROM alerts WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if usize::try_from(have).unwrap_or(usize::MAX) >= most {
        return Ok(AddingAlert::Full);
    }
    let row = tx
        .query_one(
            &format!(
                "INSERT INTO alerts (id, account_id, name, enabled, kind, station_id, rule,
                                     created_at, changed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
                 RETURNING {COLUMNS}"
            ),
            &[
                &new.id,
                &account_id,
                &new.name,
                &new.enabled,
                &new.kind,
                &new.station_id,
                &new.rule,
                &now,
            ],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(AddingAlert::Added(alert(&row)))
}

pub(super) async fn change(
    client: &Client,
    account_id: &str,
    new: &NewAlert,
    now: Timestamp,
) -> Result<Option<Alert>, StoreError> {
    let row = client
        .query_opt(
            &format!(
                "UPDATE alerts SET name = $3, enabled = $4, kind = $5, station_id = $6,
                                   rule = $7, state = '', changed_at = $8
                 WHERE account_id = $1 AND id = $2
                 RETURNING {COLUMNS}"
            ),
            &[
                &account_id,
                &new.id,
                &new.name,
                &new.enabled,
                &new.kind,
                &new.station_id,
                &new.rule,
                &now,
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(row.as_ref().map(alert))
}

pub(super) async fn remove(
    client: &Client,
    account_id: &str,
    alert_id: &str,
) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM alerts WHERE account_id = $1 AND id = $2",
            &[&account_id, &alert_id],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn at_stations(
    client: &Client,
    station_ids: &[String],
) -> Result<Vec<Alert>, StoreError> {
    let rows = client
        .query(
            &format!(
                "SELECT {COLUMNS} FROM alerts
                 WHERE enabled AND station_id IS NOT NULL AND station_id = ANY($1)
                 ORDER BY account_id, id"
            ),
            &[&station_ids],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(alert).collect())
}

pub(super) async fn of_kind(client: &Client, kind: &str) -> Result<Vec<Alert>, StoreError> {
    let rows = client
        .query(
            &format!(
                "SELECT {COLUMNS} FROM alerts WHERE enabled AND kind = $1 ORDER BY account_id, id"
            ),
            &[&kind],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(alert).collect())
}

pub(super) async fn note(client: &mut Client, check: &AlertCheck) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let updated = tx
        .execute(
            "UPDATE alerts SET state = $4
             WHERE account_id = $1 AND id = $2 AND changed_at = $3",
            &[
                &check.account_id,
                &check.alert_id,
                &check.changed_at,
                &check.state,
            ],
        )
        .await
        .map_err(db_error)?;
    if updated == 0 {
        return Ok(false);
    }
    if let Some((fired_at, record)) = &check.fired {
        let latest: i64 = tx
            .query_one(
                "INSERT INTO alerts_fired_numbers (account_id, latest) VALUES ($1, 0)
                 ON CONFLICT (account_id) DO UPDATE SET latest = alerts_fired_numbers.latest
                 RETURNING latest",
                &[&check.account_id],
            )
            .await
            .map_err(db_error)?
            .get(0);
        let number = next_change(latest, *fired_at);
        tx.execute(
            "UPDATE alerts_fired_numbers SET latest = $2 WHERE account_id = $1",
            &[&check.account_id, &number],
        )
        .await
        .map_err(db_error)?;
        tx.execute(
            "INSERT INTO alerts_fired (account_id, number, alert_id, fired_at, record)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &check.account_id,
                &number,
                &check.alert_id,
                fired_at,
                record,
            ],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn fired(
    client: &Client,
    account_id: &str,
    since: i64,
    limit: usize,
) -> Result<FiredAlerts, StoreError> {
    let latest: i64 = client
        .query_opt(
            "SELECT latest FROM alerts_fired_numbers WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .map_or(0, |row| row.get(0));
    let asked = i64::try_from(limit).unwrap_or(i64::MAX - 1) + 1;
    let rows = client
        .query(
            "SELECT number, alert_id, fired_at, record FROM alerts_fired
             WHERE account_id = $1 AND number > $2 ORDER BY number LIMIT $3",
            &[&account_id, &since, &asked],
        )
        .await
        .map_err(db_error)?;
    let mut fired: Vec<FiredAlert> = rows
        .iter()
        .map(|row| FiredAlert {
            number: row.get("number"),
            alert_id: row.get("alert_id"),
            fired_at: row.get("fired_at"),
            record: row.get("record"),
        })
        .collect();
    let more = fired.len() > limit;
    fired.truncate(limit);
    Ok(FiredAlerts {
        fired,
        more,
        latest,
    })
}

pub(super) async fn prune(client: &Client, before: Timestamp) -> Result<u64, StoreError> {
    client
        .execute("DELETE FROM alerts_fired WHERE fired_at < $1", &[&before])
        .await
        .map_err(db_error)
}
