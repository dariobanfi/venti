//! Cables in PostgreSQL: one set per source, as for sites, replaced whole
//! or inside the boxes of a run's pieces that answered.

use deadpool_postgres::{Client, Transaction};
use tokio_postgres::Row;
use venti_domain::{BoundingBox, Cable, SourceId};

use super::airspace::{boxes_from, edges_of};
use super::{corrupt, count, db_error, upsert_run};
use crate::cable::Replacing;
use crate::{CableLayer, CableSet, RunRecord, SiteDelivery, StoreError, cable};

const REPLACE_SET: &str = "
    INSERT INTO cable_sets AS s (source, fetched_at, cables, as_of, covers)
    VALUES ($1, $2, $3, $4, $5)
    ON CONFLICT (source) DO UPDATE
    SET fetched_at = EXCLUDED.fetched_at,
        cables = EXCLUDED.cables,
        as_of = EXCLUDED.as_of,
        covers = EXCLUDED.covers
    WHERE s.fetched_at <= EXCLUDED.fetched_at";

/// A cable already stored under its key, which a run that replaced only
/// some boxes kept because its line reaches out of them, takes its new
/// copy's place.
const INSERT_CABLES: &str = "
    INSERT INTO cables (source, local, west, south, east, north, record)
    SELECT $1, t.local, t.west, t.south, t.east, t.north, t.record
    FROM unnest($2::text[], $3::float8[], $4::float8[], $5::float8[], $6::float8[], $7::text[])
        AS t (local, west, south, east, north, record)
    ON CONFLICT (source, local) DO UPDATE
    SET west = EXCLUDED.west, south = EXCLUDED.south,
        east = EXCLUDED.east, north = EXCLUDED.north,
        record = EXCLUDED.record";

/// The set a run that replaces some boxes starts from, locked until it is
/// done.
const SET_BEFORE: &str = "
    SELECT source, fetched_at, cables, as_of, covers
    FROM cable_sets WHERE source = $1 FOR UPDATE";

/// The stored cables whose line's box lies wholly inside one of the boxes,
/// given as their west, south, east and north edges: the rule of
/// `cable::goes`.
const DELETE_INSIDE: &str = "
    DELETE FROM cables c
    WHERE c.source = $1
      AND EXISTS (
        SELECT 1
        FROM unnest($2::float8[], $3::float8[], $4::float8[], $5::float8[])
            AS b (west, south, east, north)
        WHERE c.west >= b.west AND c.east <= b.east
          AND c.south >= b.south AND c.north <= b.north
      )";

const COUNT_SET: &str = "
    UPDATE cable_sets
    SET cables = (SELECT count(*) FROM cables WHERE source = $1)
    WHERE source = $1";

/// One statement reads the cables and their sets, so both are of one
/// moment. Every set is in the answer: one with nothing found is a single
/// row without a record.
const CABLES_IN: &str = "
    WITH found AS (
        SELECT t.source, t.local, t.record
        FROM cables t
        WHERE t.source = ANY($5::text[])
          AND ST_MakeEnvelope(t.west, t.south, t.east, t.north, 4326)
              && ST_MakeEnvelope($1, $2, $3, $4, 4326)
          AND t.west <= $3 AND t.east >= $1
          AND t.south <= $4 AND t.north >= $2
        ORDER BY t.source, t.local
        LIMIT $6
    )
    SELECT f.record, s.source, s.fetched_at, s.cables, s.as_of, s.covers
    FROM cable_sets s
    LEFT JOIN found f ON f.source = s.source
    WHERE s.source = ANY($5::text[])
    ORDER BY s.source, f.local";

fn set_from(row: &Row) -> Result<CableSet, StoreError> {
    let source: &str = row.get("source");
    Ok(CableSet {
        source: SourceId::new(source).map_err(|e| corrupt("source identifier", e))?,
        fetched_at: row.get("fetched_at"),
        cables: u32::try_from(row.get::<_, i32>("cables")).unwrap_or(0),
        as_of: row.get("as_of"),
        covers: boxes_from(&row.get::<_, Vec<f64>>("covers"))?,
    })
}

pub(super) async fn save_run(
    client: &mut Client,
    run: &RunRecord,
    delivery: &SiteDelivery,
    cables: &[Cable],
) -> Result<(), StoreError> {
    cable::check(run, delivery, cables)?;
    let tx = client.transaction().await.map_err(db_error)?;
    upsert_run(&tx, run).await?;

    match Replacing::of(run, delivery) {
        Replacing::Whole => {
            let set = CableSet::of(run, delivery, cables);
            if write_set(&tx, &set).await? {
                tx.execute(
                    "DELETE FROM cables WHERE source = $1",
                    &[&set.source.as_str()],
                )
                .await
                .map_err(db_error)?;
                insert(&tx, &set.source, cables).await?;
            }
        }
        Replacing::Inside => {
            let statement = tx.prepare_cached(SET_BEFORE).await.map_err(db_error)?;
            let before = match tx
                .query_opt(&statement, &[&run.source.as_str()])
                .await
                .map_err(db_error)?
            {
                Some(row) => Some(set_from(&row)?),
                None => None,
            };
            if before
                .as_ref()
                .is_none_or(|set| set.fetched_at <= run.started_at)
            {
                // The count is set once the cables are in.
                let set = cable::partly_replaced(before.as_ref(), run, delivery, 0);
                write_set(&tx, &set).await?;
                let edge = |of: fn(&BoundingBox) -> f64| {
                    delivery.covers.iter().map(of).collect::<Vec<f64>>()
                };
                let statement = tx.prepare_cached(DELETE_INSIDE).await.map_err(db_error)?;
                tx.execute(
                    &statement,
                    &[
                        &set.source.as_str(),
                        &edge(BoundingBox::west),
                        &edge(BoundingBox::south),
                        &edge(BoundingBox::east),
                        &edge(BoundingBox::north),
                    ],
                )
                .await
                .map_err(db_error)?;
                insert(&tx, &set.source, cables).await?;
                let statement = tx.prepare_cached(COUNT_SET).await.map_err(db_error)?;
                tx.execute(&statement, &[&set.source.as_str()])
                    .await
                    .map_err(db_error)?;
            }
        }
        Replacing::Nothing => {}
    }
    tx.commit().await.map_err(db_error)
}

/// Writes what the store knows about a set, unless a newer one is stored.
/// Whether it was written.
async fn write_set(tx: &Transaction<'_>, set: &CableSet) -> Result<bool, StoreError> {
    let statement = tx.prepare_cached(REPLACE_SET).await.map_err(db_error)?;
    let replaced = tx
        .execute(
            &statement,
            &[
                &set.source.as_str(),
                &set.fetched_at,
                &count(set.cables),
                &set.as_of,
                &edges_of(&set.covers),
            ],
        )
        .await
        .map_err(db_error)?;
    Ok(replaced == 1)
}

/// Inserts cables of one source, each in place of a stored one with its key.
async fn insert(
    tx: &Transaction<'_>,
    source: &SourceId,
    cables: &[Cable],
) -> Result<(), StoreError> {
    // `check` made sure that every cable has a box.
    let boxes: Vec<BoundingBox> = cables.iter().filter_map(Cable::bounds).collect();
    let locals: Vec<&str> = cables.iter().map(|cable| cable.local.as_str()).collect();
    let edge = |of: fn(&BoundingBox) -> f64| boxes.iter().map(of).collect::<Vec<f64>>();
    let records = cables
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<String>, _>>()
        .map_err(|error| {
            StoreError::Failure(format!("a cable cannot be written as JSON: {error}"))
        })?;
    let statement = tx.prepare_cached(INSERT_CABLES).await.map_err(db_error)?;
    tx.execute(
        &statement,
        &[
            &source.as_str(),
            &locals,
            &edge(BoundingBox::west),
            &edge(BoundingBox::south),
            &edge(BoundingBox::east),
            &edge(BoundingBox::north),
            &records,
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

pub(super) async fn in_box(
    client: &Client,
    sources: &[SourceId],
    area: &BoundingBox,
    limit: usize,
) -> Result<Vec<CableLayer>, StoreError> {
    let statement = client.prepare_cached(CABLES_IN).await.map_err(db_error)?;
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
    let mut layers: Vec<CableLayer> = Vec::new();
    for row in &rows {
        let source: &str = row.get("source");
        if layers
            .last()
            .is_none_or(|layer| layer.set.source.as_str() != source)
        {
            layers.push(CableLayer {
                set: set_from(row)?,
                cables: Vec::new(),
            });
        }
        if let Some(record) = row.get::<_, Option<&str>>("record") {
            let cable: Cable =
                serde_json::from_str(record).map_err(|e| corrupt("cable record", e))?;
            if let Some(layer) = layers.last_mut() {
                layer.cables.push(cable);
            }
        }
    }
    Ok(layers)
}

pub(super) async fn sets(
    client: &Client,
    sources: &[SourceId],
) -> Result<Vec<CableSet>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT s.source, s.fetched_at, s.cables, s.as_of, s.covers
             FROM cable_sets s
             WHERE s.source = ANY($1::text[]) ORDER BY s.source",
        )
        .await
        .map_err(db_error)?;
    let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
    let rows = client
        .query(&statement, &[&names])
        .await
        .map_err(db_error)?;
    rows.iter().map(set_from).collect()
}
