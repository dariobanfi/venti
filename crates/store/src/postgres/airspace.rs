//! Airspace in PostgreSQL: one whole set per source.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::{Airspace, BoundingBox, SourceId};

use super::{corrupt, count, db_error, upsert_run};
use crate::{AirspaceLayer, AirspaceSet, RunRecord, StoreError, airspace};

/// Makes the set of a run the stored one, unless a newer run's set is
/// stored already. The statement locks the source's row, so of two runs
/// that finish at the same moment the second waits for the first.
const REPLACE_SET: &str = "
    INSERT INTO airspace_sets AS s (source, fetched_at, airspaces, file_date, newest_change)
    VALUES ($1, $2, $3, $4, $5)
    ON CONFLICT (source) DO UPDATE
    SET fetched_at = EXCLUDED.fetched_at,
        airspaces = EXCLUDED.airspaces,
        file_date = EXCLUDED.file_date,
        newest_change = EXCLUDED.newest_change
    WHERE s.fetched_at <= EXCLUDED.fetched_at";

const INSERT_AIRSPACES: &str = "
    INSERT INTO airspaces (id, source, west, south, east, north, record)
    SELECT t.id, $1, t.west, t.south, t.east, t.north, t.record
    FROM unnest($2::text[], $3::float8[], $4::float8[], $5::float8[], $6::float8[], $7::text[])
         AS t (id, west, south, east, north, record)";

/// The columns every query selects for a set, for an `airspace_sets` table
/// named `s`.
const SET_COLUMNS: &str = "s.source, s.fetched_at, s.airspaces, s.file_date, s.newest_change";

/// The boxes are compared as plain longitude and latitude. `&&` lets the
/// index find candidates; the comparison of the four edges that follows is
/// the exact one. One statement reads the airspaces and their sets, so
/// both are of one moment.
const AIRSPACES_IN: &str = "
    SELECT a.record, s.source, s.fetched_at, s.airspaces, s.file_date, s.newest_change
    FROM airspaces a
    JOIN airspace_sets s ON s.source = a.source
    WHERE a.source = ANY($5::text[])
      AND ST_MakeEnvelope(a.west, a.south, a.east, a.north, 4326)
          && ST_MakeEnvelope($1, $2, $3, $4, 4326)
      AND a.west <= $3 AND a.east >= $1
      AND a.south <= $4 AND a.north >= $2
    ORDER BY a.source, a.id
    LIMIT $6";

fn set_from(row: &Row) -> Result<AirspaceSet, StoreError> {
    let source: &str = row.get("source");
    Ok(AirspaceSet {
        source: SourceId::new(source).map_err(|e| corrupt("source identifier", e))?,
        fetched_at: row.get("fetched_at"),
        airspaces: u32::try_from(row.get::<_, i32>("airspaces")).unwrap_or(0),
        file_date: row.get("file_date"),
        newest_change: row.get("newest_change"),
    })
}

pub(super) async fn save_run(
    client: &mut Client,
    run: &RunRecord,
    airspaces: &[Airspace],
) -> Result<(), StoreError> {
    airspace::check(run, airspaces)?;
    // Dropping the transaction without a commit undoes what was done in it.
    let tx = client.transaction().await.map_err(db_error)?;

    upsert_run(&tx, run).await?;

    if run.healthy {
        let set = AirspaceSet::of(run, airspaces);
        let statement = tx.prepare_cached(REPLACE_SET).await.map_err(db_error)?;
        let replaced = tx
            .execute(
                &statement,
                &[
                    &set.source.as_str(),
                    &set.fetched_at,
                    &count(set.airspaces),
                    &set.file_date,
                    &set.newest_change,
                ],
            )
            .await
            .map_err(db_error)?;
        // Nothing replaced: the stored set is from a later run, and stays.
        if replaced == 1 {
            tx.execute(
                "DELETE FROM airspaces WHERE source = $1",
                &[&set.source.as_str()],
            )
            .await
            .map_err(db_error)?;

            let ids: Vec<String> = airspaces.iter().map(|a| a.id.to_string()).collect();
            let boxes: Vec<BoundingBox> = airspaces.iter().map(Airspace::bounds).collect();
            let edge =
                |edge: fn(&BoundingBox) -> f64| -> Vec<f64> { boxes.iter().map(edge).collect() };
            let records = airspaces
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<String>, _>>()
                .map_err(|error| {
                    StoreError::Failure(format!("an airspace cannot be written as JSON: {error}"))
                })?;
            let statement = tx
                .prepare_cached(INSERT_AIRSPACES)
                .await
                .map_err(db_error)?;
            tx.execute(
                &statement,
                &[
                    &set.source.as_str(),
                    &ids,
                    &edge(BoundingBox::west),
                    &edge(BoundingBox::south),
                    &edge(BoundingBox::east),
                    &edge(BoundingBox::north),
                    &records,
                ],
            )
            .await
            .map_err(db_error)?;
        }
    }

    tx.commit().await.map_err(db_error)
}

pub(super) async fn in_box(
    client: &Client,
    sources: &[SourceId],
    area: &BoundingBox,
    limit: usize,
) -> Result<Vec<AirspaceLayer>, StoreError> {
    let statement = client
        .prepare_cached(AIRSPACES_IN)
        .await
        .map_err(db_error)?;
    let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(
            &statement,
            &[
                &area.west(),
                &area.south(),
                &area.east(),
                &area.north(),
                &names,
                &limit,
            ],
        )
        .await
        .map_err(db_error)?;

    // The rows come sorted by source, so each layer is a run of rows.
    let mut layers: Vec<AirspaceLayer> = Vec::new();
    for row in &rows {
        let record: &str = row.get("record");
        let airspace: Airspace =
            serde_json::from_str(record).map_err(|e| corrupt("airspace record", e))?;
        let source: &str = row.get("source");
        match layers.last_mut() {
            Some(layer) if layer.set.source.as_str() == source => layer.airspaces.push(airspace),
            _ => layers.push(AirspaceLayer {
                set: set_from(row)?,
                airspaces: vec![airspace],
            }),
        }
    }
    Ok(layers)
}

pub(super) async fn sets(
    client: &Client,
    sources: &[SourceId],
) -> Result<Vec<AirspaceSet>, StoreError> {
    let query = format!(
        "SELECT {SET_COLUMNS} FROM airspace_sets s
         WHERE s.source = ANY($1::text[]) ORDER BY s.source"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
    let rows = client
        .query(&statement, &[&names])
        .await
        .map_err(db_error)?;
    rows.iter().map(set_from).collect()
}
