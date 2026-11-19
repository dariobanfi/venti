//! What is known about the forecast models, in PostgreSQL.

use deadpool_postgres::Client;
use tokio_postgres::Row;
use venti_domain::{ModelId, SourceId, Timestamp};

use super::{corrupt, db_error, upsert_run};
use crate::{ForecastModel, RunRecord, StoreError, forecast};

const INSERT_MODELS: &str = "
    INSERT INTO forecast_models (source, model, run, available_at, checked_at)
    SELECT $1, t.model, t.run, t.available_at, $2
    FROM unnest($3::text[], $4::timestamptz[], $5::timestamptz[]) AS t (model, run, available_at)";

fn model_from(row: &Row) -> Result<ForecastModel, StoreError> {
    let source: &str = row.get("source");
    let model: &str = row.get("model");
    Ok(ForecastModel {
        source: SourceId::new(source).map_err(|e| corrupt("source identifier", e))?,
        model: ModelId::new(model).map_err(|e| corrupt("model name", e))?,
        run: row.get("run"),
        available_at: row.get("available_at"),
        checked_at: row.get("checked_at"),
    })
}

pub(super) async fn save_check(
    client: &mut Client,
    run: &RunRecord,
    models: &[ForecastModel],
) -> Result<(), StoreError> {
    forecast::check(run, models)?;
    // Dropping the transaction without a commit undoes what was done in it.
    let tx = client.transaction().await.map_err(db_error)?;

    upsert_run(&tx, run).await?;

    if run.healthy {
        let source = run.source.as_str();
        // One look at a time per source: of two that finish at the same
        // moment the second waits here and then sees what the first stored.
        tx.execute(
            "SELECT pg_advisory_xact_lock(hashtext('forecast_models:' || $1))",
            &[&source],
        )
        .await
        .map_err(db_error)?;
        let stored: Option<Timestamp> = tx
            .query_one(
                "SELECT max(checked_at) FROM forecast_models WHERE source = $1",
                &[&source],
            )
            .await
            .map_err(db_error)?
            .get(0);
        // What is stored from a later look stays.
        if stored.is_none_or(|stored| stored <= run.started_at) {
            tx.execute("DELETE FROM forecast_models WHERE source = $1", &[&source])
                .await
                .map_err(db_error)?;
            let names: Vec<&str> = models.iter().map(|model| model.model.as_str()).collect();
            let runs: Vec<Timestamp> = models.iter().map(|model| model.run).collect();
            let available: Vec<Option<Timestamp>> =
                models.iter().map(|model| model.available_at).collect();
            let statement = tx.prepare_cached(INSERT_MODELS).await.map_err(db_error)?;
            tx.execute(
                &statement,
                &[&source, &run.started_at, &names, &runs, &available],
            )
            .await
            .map_err(db_error)?;
        }
    }

    tx.commit().await.map_err(db_error)
}

pub(super) async fn models(
    client: &Client,
    sources: &[SourceId],
) -> Result<Vec<ForecastModel>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT source, model, run, available_at, checked_at
             FROM forecast_models
             WHERE source = ANY($1::text[]) ORDER BY source, model",
        )
        .await
        .map_err(db_error)?;
    let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
    let rows = client
        .query(&statement, &[&names])
        .await
        .map_err(db_error)?;
    rows.iter().map(model_from).collect()
}
