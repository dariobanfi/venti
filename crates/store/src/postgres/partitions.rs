//! The monthly partitions of the `observations` table.
//!
//! A partition is created when the first observation of its month arrives,
//! not ahead of time by a timer: a timer that did not run would stop
//! ingestion at midnight on the first of a month. Old observations leave by
//! dropping whole partitions, which costs the database almost nothing however
//! many rows they hold.

use std::collections::BTreeSet;

use deadpool_postgres::{Client, Transaction};

use super::{LOCK_KEY, db_error};
use crate::{Month, StoreError};

const PREFIX: &str = "observations_y";

/// The table name of a month's partition, for example `observations_y2026m11`.
fn name(month: Month) -> String {
    format!("{PREFIX}{:04}m{:02}", month.year(), month.month())
}

/// The month a partition holds, read back from its name.
fn month_of(name: &str) -> Option<Month> {
    let (year, month) = name.strip_prefix(PREFIX)?.split_once('m')?;
    Month::new(year.parse().ok()?, month.parse().ok()?)
}

/// The partitions that exist, by the name to use in a statement.
const LIST: &str = "
    SELECT c.relname, c.oid::regclass::text AS qualified
    FROM pg_inherits i
    JOIN pg_class c ON c.oid = i.inhrelid
    WHERE i.inhparent = 'observations'::regclass";

/// Makes sure a partition exists for each of `months`.
///
/// Runs in transactions of its own, before the one that stores the records:
/// creating a partition briefly locks the whole table, and that lock should
/// not be held while a batch is written. An empty partition left behind by a
/// batch that later fails does no harm.
pub(crate) async fn ensure(
    client: &mut Client,
    months: &BTreeSet<Month>,
) -> Result<(), StoreError> {
    if months.is_empty() {
        return Ok(());
    }
    let existing: BTreeSet<Month> = client
        .query(LIST, &[])
        .await
        .map_err(db_error)?
        .iter()
        .filter_map(|row| month_of(row.get("relname")))
        .collect();

    for month in months.difference(&existing) {
        let tx = client.transaction().await.map_err(db_error)?;
        // Two sources may see the first observation of a month at the same
        // moment; the lock makes the second wait and then find the table.
        tx.execute("SELECT pg_advisory_xact_lock($1)", &[&LOCK_KEY])
            .await
            .map_err(db_error)?;
        // The bounds are built from integers, so there is nothing to escape.
        let create = format!(
            "CREATE TABLE IF NOT EXISTS {} PARTITION OF observations
             FOR VALUES FROM ('{}') TO ('{}')",
            name(*month),
            month.start().format("%Y-%m-%d %H:%M:%S+00"),
            month.next().start().format("%Y-%m-%d %H:%M:%S+00"),
        );
        tx.batch_execute(&create).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
    }
    Ok(())
}

/// Drops every partition of a month before `first_kept`.
pub(crate) async fn drop_before(tx: &Transaction<'_>, first_kept: Month) -> Result<(), StoreError> {
    for row in tx.query(LIST, &[]).await.map_err(db_error)? {
        let month = month_of(row.get("relname"));
        if month.is_some_and(|month| month < first_kept) {
            let qualified: &str = row.get("qualified");
            tx.batch_execute(&format!("DROP TABLE {qualified}"))
                .await
                .map_err(db_error)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_names_round_trip() {
        let november = Month::new(2026, 11).unwrap();
        assert_eq!(name(november), "observations_y2026m11");
        assert_eq!(month_of("observations_y2026m11"), Some(november));
        assert_eq!(month_of("observations_y2026m13"), None);
        assert_eq!(month_of("observations_default"), None);
        assert_eq!(month_of("stations"), None);
    }
}
