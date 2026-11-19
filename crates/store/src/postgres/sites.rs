//! Flying sites in PostgreSQL: one whole set per source.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::{BoundingBox, Site, SourceId};

use super::airspace::{boxes_from, edges_of};
use super::{corrupt, count, db_error, upsert_run};
use crate::{RunRecord, SiteDelivery, SiteLayer, SiteSet, StoreError, site};

/// Makes the set of a run the stored one, unless a newer run's set is
/// stored already. The statement locks the source's row, so of two runs
/// that finish at the same moment the second waits for the first.
const REPLACE_SET: &str = "
    INSERT INTO site_sets AS s (source, fetched_at, sites, as_of, covers)
    VALUES ($1, $2, $3, $4, $5)
    ON CONFLICT (source) DO UPDATE
    SET fetched_at = EXCLUDED.fetched_at,
        sites = EXCLUDED.sites,
        as_of = EXCLUDED.as_of,
        covers = EXCLUDED.covers
    WHERE s.fetched_at <= EXCLUDED.fetched_at";

const INSERT_SITES: &str = "
    INSERT INTO sites (id, source, lon, lat, record)
    SELECT t.id, $1, t.lon, t.lat, t.record
    FROM unnest($2::text[], $3::float8[], $4::float8[], $5::text[]) AS t (id, lon, lat, record)";

/// The box is compared as plain longitude and latitude. `&&` lets the
/// index find candidates; the comparison of the coordinates that follows
/// is the exact one. One statement reads the sites and their sets, so both
/// are of one moment. Every set is in the answer: one with nothing found
/// is a single row without a record.
const SITES_IN: &str = "
    WITH found AS (
        SELECT t.source, t.id, t.record
        FROM sites t
        WHERE t.source = ANY($5::text[])
          AND ST_SetSRID(ST_MakePoint(t.lon, t.lat), 4326)
              && ST_MakeEnvelope($1, $2, $3, $4, 4326)
          AND t.lon BETWEEN $1 AND $3
          AND t.lat BETWEEN $2 AND $4
        ORDER BY t.source, t.id
        LIMIT $6
    )
    SELECT f.record, s.source, s.fetched_at, s.sites, s.as_of, s.covers
    FROM site_sets s
    LEFT JOIN found f ON f.source = s.source
    WHERE s.source = ANY($5::text[])
    ORDER BY s.source, f.id";

fn set_from(row: &Row) -> Result<SiteSet, StoreError> {
    let source: &str = row.get("source");
    Ok(SiteSet {
        source: SourceId::new(source).map_err(|e| corrupt("source identifier", e))?,
        fetched_at: row.get("fetched_at"),
        sites: u32::try_from(row.get::<_, i32>("sites")).unwrap_or(0),
        as_of: row.get("as_of"),
        covers: boxes_from(&row.get::<_, Vec<f64>>("covers"))?,
    })
}

pub(super) async fn save_run(
    client: &mut Client,
    run: &RunRecord,
    delivery: &SiteDelivery,
    sites: &[Site],
) -> Result<(), StoreError> {
    site::check(run, sites)?;
    // Dropping the transaction without a commit undoes what was done in it.
    let tx = client.transaction().await.map_err(db_error)?;

    upsert_run(&tx, run).await?;

    if run.healthy {
        let set = SiteSet::of(run, delivery, sites);
        let statement = tx.prepare_cached(REPLACE_SET).await.map_err(db_error)?;
        let replaced = tx
            .execute(
                &statement,
                &[
                    &set.source.as_str(),
                    &set.fetched_at,
                    &count(set.sites),
                    &set.as_of,
                    &edges_of(&set.covers),
                ],
            )
            .await
            .map_err(db_error)?;
        // Nothing replaced: the stored set is from a later run, and stays.
        if replaced == 1 {
            tx.execute(
                "DELETE FROM sites WHERE source = $1",
                &[&set.source.as_str()],
            )
            .await
            .map_err(db_error)?;

            let ids: Vec<String> = sites.iter().map(|site| site.id.to_string()).collect();
            let lons: Vec<f64> = sites.iter().map(|site| site.position.lon()).collect();
            let lats: Vec<f64> = sites.iter().map(|site| site.position.lat()).collect();
            let records = sites
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<String>, _>>()
                .map_err(|error| {
                    StoreError::Failure(format!("a site cannot be written as JSON: {error}"))
                })?;
            let statement = tx.prepare_cached(INSERT_SITES).await.map_err(db_error)?;
            tx.execute(
                &statement,
                &[&set.source.as_str(), &ids, &lons, &lats, &records],
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
) -> Result<Vec<SiteLayer>, StoreError> {
    let statement = client.prepare_cached(SITES_IN).await.map_err(db_error)?;
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

    // The rows come sorted by source, so each layer is a run of rows. A
    // set with nothing found is one row without a record.
    let mut layers: Vec<SiteLayer> = Vec::new();
    for row in &rows {
        let source: &str = row.get("source");
        if layers
            .last()
            .is_none_or(|layer| layer.set.source.as_str() != source)
        {
            layers.push(SiteLayer {
                set: set_from(row)?,
                sites: Vec::new(),
            });
        }
        if let Some(record) = row.get::<_, Option<&str>>("record") {
            let site: Site = serde_json::from_str(record).map_err(|e| corrupt("site record", e))?;
            if let Some(layer) = layers.last_mut() {
                layer.sites.push(site);
            }
        }
    }
    Ok(layers)
}

pub(super) async fn sets(
    client: &Client,
    sources: &[SourceId],
) -> Result<Vec<SiteSet>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT s.source, s.fetched_at, s.sites, s.as_of, s.covers
             FROM site_sets s
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
