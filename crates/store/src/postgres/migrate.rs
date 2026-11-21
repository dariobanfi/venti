//! The database schema, as an ordered list of migrations compiled into the
//! program.
//!
//! Rules for changing the schema:
//!
//! - never edit a migration that has shipped; add a new file with the next
//!   number and list it in [`MIGRATIONS`];
//! - a migration runs inside one transaction with every other pending
//!   migration, so a failed upgrade leaves the database as it was. A
//!   statement that cannot run in a transaction (`CREATE INDEX
//!   CONCURRENTLY`) needs the runner extended first.

use deadpool_postgres::Client;

use super::{LOCK_KEY, db_error};
use crate::StoreError;

struct Migration {
    version: i32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "stations, observations and run history",
        sql: include_str!("../../migrations/0001_stations_observations_runs.sql"),
    },
    Migration {
        version: 2,
        name: "accounts, sessions, invites and flights",
        sql: include_str!("../../migrations/0002_accounts_sessions_flights.sql"),
    },
    Migration {
        version: 3,
        name: "airspace",
        sql: include_str!("../../migrations/0003_airspace.sql"),
    },
    Migration {
        version: 4,
        name: "where an airspace set is whole",
        sql: include_str!("../../migrations/0004_airspace_coverage.sql"),
    },
    Migration {
        version: 5,
        name: "flying sites",
        sql: include_str!("../../migrations/0005_sites.sql"),
    },
    Migration {
        version: 6,
        name: "the runs of the forecast models",
        sql: include_str!("../../migrations/0006_forecast_models.sql"),
    },
    Migration {
        version: 7,
        name: "live tracks",
        sql: include_str!("../../migrations/0007_live_tracks.sql"),
    },
    Migration {
        version: 8,
        name: "everyone's public flights in one list",
        sql: include_str!("../../migrations/0008_public_flights.sql"),
    },
    Migration {
        version: 9,
        name: "reports, flights taken down and publishing suspended",
        sql: include_str!("../../migrations/0009_reports_and_takedowns.sql"),
    },
    Migration {
        version: 10,
        name: "groups and their plans for a flying day",
        sql: include_str!("../../migrations/0010_groups_and_plans.sql"),
    },
    Migration {
        version: 11,
        name: "the thermal map's judged hours",
        sql: include_str!("../../migrations/0011_thermal_map_parts.sql"),
    },
    Migration {
        version: 12,
        name: "reports about groups and plans",
        sql: include_str!("../../migrations/0012_reports_about_groups.sql"),
    },
    Migration {
        version: 13,
        name: "flying sites that pilots make, with their revisions",
        sql: include_str!("../../migrations/0013_pilot_sites.sql"),
    },
    Migration {
        version: 14,
        name: "tokens a pilot gives to another program",
        sql: include_str!("../../migrations/0014_access_tokens.sql"),
    },
    Migration {
        version: 15,
        name: "reports about sites that pilots made",
        sql: include_str!("../../migrations/0015_reports_about_sites.sql"),
    },
    Migration {
        version: 16,
        name: "the sites a pilot keeps as their own, on the account",
        sql: include_str!("../../migrations/0016_kept_sites.sql"),
    },
];

/// Applies every migration the database has not seen yet.
///
/// Safe to call from several processes at once: a database-wide lock lets
/// one of them work while the others wait and then find nothing to do.
pub(crate) async fn run(client: &mut Client) -> Result<(), StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    tx.execute("SELECT pg_advisory_xact_lock($1)", &[&LOCK_KEY])
        .await
        .map_err(db_error)?;
    tx.batch_execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    integer PRIMARY KEY,
             name       text NOT NULL,
             applied_at timestamptz NOT NULL DEFAULT now()
         )",
    )
    .await
    .map_err(db_error)?;

    let applied: i32 = tx
        .query_one(
            "SELECT COALESCE(max(version), 0) FROM schema_migrations",
            &[],
        )
        .await
        .map_err(db_error)?
        .get(0);
    let newest = MIGRATIONS.last().map_or(0, |migration| migration.version);
    if applied > newest {
        return Err(StoreError::Failure(format!(
            "the database schema is at version {applied}, but this program only knows \
             version {newest}; run the newer server that upgraded it"
        )));
    }

    for migration in MIGRATIONS.iter().filter(|m| m.version > applied) {
        tx.batch_execute(migration.sql).await.map_err(|error| {
            StoreError::Failure(format!(
                "migration {} ({}) failed: {}",
                migration.version,
                migration.name,
                db_error(error)
            ))
        })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES ($1, $2)",
            &[&migration.version, &migration.name],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_numbered_from_one_without_gaps() {
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                usize::try_from(migration.version).unwrap(),
                index + 1,
                "{} is out of order",
                migration.name
            );
            assert!(!migration.sql.trim().is_empty());
        }
    }
}
