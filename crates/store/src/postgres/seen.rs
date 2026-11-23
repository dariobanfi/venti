//! Up to where the pilot saw the notices and the fired alarms, in
//! PostgreSQL (`0033_seen_marks.sql`).

use deadpool_postgres::Client;
use venti_domain::Timestamp;

use super::db_error;
use crate::{StoreError, Unread};

pub(super) async fn unread(
    client: &Client,
    account_id: &str,
    notices_from: Timestamp,
    fired_from: Timestamp,
    most: u32,
) -> Result<Unread, StoreError> {
    let most = i64::from(most);
    // Each count reads at most `most` rows of the primary key's index.
    let row = client
        .query_one(
            "WITH mark AS (
                 SELECT COALESCE((SELECT notices_seen FROM seen_marks WHERE account_id = $1), 0)
                            AS notices,
                        COALESCE((SELECT fired_seen FROM seen_marks WHERE account_id = $1), 0)
                            AS fired
             )
             SELECT
                 (SELECT count(*) FROM (
                      SELECT 1 FROM notices, mark
                      WHERE account_id = $1 AND number > mark.notices AND at >= $2
                      LIMIT $4) AS n),
                 (SELECT count(*) FROM (
                      SELECT 1 FROM alerts_fired, mark
                      WHERE account_id = $1 AND number > mark.fired AND fired_at >= $3
                      LIMIT $4) AS f)",
            &[&account_id, &notices_from, &fired_from, &most],
        )
        .await
        .map_err(db_error)?;
    let count = |index: usize| u32::try_from(row.get::<_, i64>(index)).unwrap_or(u32::MAX);
    Ok(Unread {
        notices: count(0),
        fired: count(1),
    })
}

pub(super) async fn mark_seen(
    client: &Client,
    account_id: &str,
    notices: Option<i64>,
    fired: Option<i64>,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let written = client
        .execute(
            "INSERT INTO seen_marks (account_id, notices_seen, fired_seen, seen_at)
             SELECT accounts.id,
                    LEAST(COALESCE($2::bigint, n.latest, 0), COALESCE(n.latest, 0)),
                    LEAST(COALESCE($3::bigint, f.latest, 0), COALESCE(f.latest, 0)),
                    $4
             FROM accounts
             LEFT JOIN notice_numbers AS n ON n.account_id = accounts.id
             LEFT JOIN alerts_fired_numbers AS f ON f.account_id = accounts.id
             WHERE accounts.id = $1
             ON CONFLICT (account_id) DO UPDATE SET
                 notices_seen = GREATEST(seen_marks.notices_seen, EXCLUDED.notices_seen),
                 fired_seen = GREATEST(seen_marks.fired_seen, EXCLUDED.fired_seen),
                 seen_at = EXCLUDED.seen_at",
            &[&account_id, &notices, &fired, &now],
        )
        .await
        .map_err(db_error)?;
    Ok(written > 0)
}
