//! The store in PostgreSQL with PostGIS.

mod migrate;
mod partitions;

use std::time::Duration;

use deadpool_postgres::{
    Client, Manager, ManagerConfig, Pool, PoolError, RecyclingMethod, Runtime,
};
use tokio_postgres::{NoTls, Row};
use venti_domain::{BoundingBox, GeoPoint, Observation, SourceId, Station, StationId, Timestamp};

use crate::{Month, RunRecord, SourceStatus, StationLatest, Store, StoreError, batch};

/// Connections kept to the database. The API's queries take milliseconds,
/// so a few connections serve many requests.
const POOL_SIZE: usize = 16;

/// How long a caller waits for a free connection, or for a new one to be
/// made, before the store reports itself unavailable.
const WAIT: Duration = Duration::from_secs(5);

/// The database cancels any statement that runs longer than this, so a
/// query gone wrong cannot hold a connection for ever.
const STATEMENT_TIMEOUT_MS: u32 = 30_000;

/// Key of the database-wide advisory lock that serialises schema changes:
/// migrations and the creation of partitions. The value is arbitrary ("venti"
/// in ASCII) and must never change.
const LOCK_KEY: i64 = 0x76_65_6e_74_69;

/// The store in PostgreSQL with PostGIS.
///
/// The connection is not encrypted: the database runs on the same host as
/// the server, on a private network. A database on another machine needs TLS
/// added here first.
#[derive(Clone)]
pub struct PgStore {
    pool: Pool,
}

impl PgStore {
    /// Connects to the database at `url` (`postgres://user:password@host/db`)
    /// and brings its schema up to date.
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        // The parser's message can quote the URL, password included: keep it
        // out of the error.
        let mut config: tokio_postgres::Config = url.parse().map_err(|_| {
            StoreError::Invalid(
                "the database URL is not of the form postgres://user:password@host:port/database"
                    .to_owned(),
            )
        })?;
        let timeout = format!("-c statement_timeout={STATEMENT_TIMEOUT_MS}");
        let options = match config.get_options() {
            Some(given) => format!("{given} {timeout}"),
            None => timeout,
        };
        config.options(&options);
        if config.get_application_name().is_none() {
            config.application_name("venti-server");
        }

        let manager_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let manager = Manager::from_config(config, NoTls, manager_config);
        let pool = Pool::builder(manager)
            .max_size(POOL_SIZE)
            .wait_timeout(Some(WAIT))
            .create_timeout(Some(WAIT))
            .recycle_timeout(Some(WAIT))
            .runtime(Runtime::Tokio1)
            .build()
            .map_err(|error| StoreError::Failure(error.to_string()))?;

        let store = Self { pool };
        let mut client = store.client().await?;
        migrate::run(&mut client).await?;
        Ok(store)
    }

    async fn client(&self) -> Result<Client, StoreError> {
        self.pool.get().await.map_err(|error| match error {
            PoolError::Backend(error) => db_error(error),
            other => StoreError::Unavailable(other.to_string()),
        })
    }
}

/// Sorts a database error into "try again later" and "something is wrong".
fn db_error(error: tokio_postgres::Error) -> StoreError {
    match error.as_db_error() {
        // No answer from the database at all: the connection failed.
        None => StoreError::Unavailable(error.to_string()),
        Some(db) => {
            let text = format!("{} ({})", db.message(), db.code().code());
            // Classes 08, 53 and 57: connection trouble, out of resources,
            // and shutdown or cancellation by the operator.
            match &db.code().code()[..2] {
                "08" | "53" | "57" => StoreError::Unavailable(text),
                _ => StoreError::Failure(text),
            }
        }
    }
}

fn corrupt(what: &str, problem: impl std::fmt::Display) -> StoreError {
    StoreError::Failure(format!("the database holds an unusable {what}: {problem}"))
}

fn count(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The columns every query selects for a station.
const STATION_COLUMNS: &str = "s.id, s.name, s.elevation_m,
     ST_Y(s.position::geometry) AS lat, ST_X(s.position::geometry) AS lon";

fn station_from(row: &Row) -> Result<Station, StoreError> {
    let id: &str = row.get("id");
    Ok(Station {
        id: id.parse().map_err(|e| corrupt("station identifier", e))?,
        name: row.get("name"),
        position: GeoPoint::new(row.get("lat"), row.get("lon"))
            .map_err(|e| corrupt("station position", e))?,
        elevation_m: row.get("elevation_m"),
    })
}

fn observation_from(station: StationId, time: Timestamp, row: &Row) -> Observation {
    Observation {
        station,
        time,
        wind_speed_ms: row.get("wind_speed_ms"),
        wind_gust_ms: row.get("wind_gust_ms"),
        wind_direction_deg: row.get("wind_direction_deg"),
        gust_direction_deg: row.get("gust_direction_deg"),
        temperature_c: row.get("temperature_c"),
        humidity_pct: row.get("humidity_pct"),
        pressure_hpa: row.get("pressure_hpa"),
    }
}

const UPSERT_STATIONS: &str = "
    INSERT INTO stations (id, source, name, position, elevation_m, first_seen_at, last_seen_at)
    SELECT t.id, t.source, t.name,
           ST_SetSRID(ST_MakePoint(t.lon, t.lat), 4326)::geography,
           t.elevation_m, $7, $7
    FROM unnest($1::text[], $2::text[], $3::text[], $4::float8[], $5::float8[], $6::float8[])
         AS t (id, source, name, lon, lat, elevation_m)
    ON CONFLICT (id) DO UPDATE
    SET name = EXCLUDED.name,
        position = EXCLUDED.position,
        elevation_m = EXCLUDED.elevation_m,
        last_seen_at = GREATEST(stations.last_seen_at, EXCLUDED.last_seen_at)";

/// New measurements win and missing ones leave the stored value alone. The
/// `WHERE` skips rows that would not change: each run delivers the last few
/// hours again, and rewriting identical rows would only make garbage for the
/// database to collect.
const UPSERT_OBSERVATIONS: &str = "
    INSERT INTO observations AS o
        (station_id, time, wind_speed_ms, wind_gust_ms, wind_direction_deg,
         gust_direction_deg, temperature_c, humidity_pct, pressure_hpa)
    SELECT * FROM unnest($1::text[], $2::timestamptz[], $3::float8[], $4::float8[],
                         $5::float8[], $6::float8[], $7::float8[], $8::float8[], $9::float8[])
    ON CONFLICT (station_id, time) DO UPDATE
    SET wind_speed_ms      = COALESCE(EXCLUDED.wind_speed_ms, o.wind_speed_ms),
        wind_gust_ms       = COALESCE(EXCLUDED.wind_gust_ms, o.wind_gust_ms),
        wind_direction_deg = COALESCE(EXCLUDED.wind_direction_deg, o.wind_direction_deg),
        gust_direction_deg = COALESCE(EXCLUDED.gust_direction_deg, o.gust_direction_deg),
        temperature_c      = COALESCE(EXCLUDED.temperature_c, o.temperature_c),
        humidity_pct       = COALESCE(EXCLUDED.humidity_pct, o.humidity_pct),
        pressure_hpa       = COALESCE(EXCLUDED.pressure_hpa, o.pressure_hpa)
    WHERE (COALESCE(EXCLUDED.wind_speed_ms, o.wind_speed_ms),
           COALESCE(EXCLUDED.wind_gust_ms, o.wind_gust_ms),
           COALESCE(EXCLUDED.wind_direction_deg, o.wind_direction_deg),
           COALESCE(EXCLUDED.gust_direction_deg, o.gust_direction_deg),
           COALESCE(EXCLUDED.temperature_c, o.temperature_c),
           COALESCE(EXCLUDED.humidity_pct, o.humidity_pct),
           COALESCE(EXCLUDED.pressure_hpa, o.pressure_hpa))
          IS DISTINCT FROM
          (o.wind_speed_ms, o.wind_gust_ms, o.wind_direction_deg, o.gust_direction_deg,
           o.temperature_c, o.humidity_pct, o.pressure_hpa)";

const UPSERT_RUN: &str = "
    INSERT INTO source_runs
        (source, started_at, finished_at, healthy, requests, failed_requests,
         skipped, rejected, stations, observations, newest_observation)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    ON CONFLICT (source, started_at) DO UPDATE
    SET finished_at = EXCLUDED.finished_at,
        healthy = EXCLUDED.healthy,
        requests = EXCLUDED.requests,
        failed_requests = EXCLUDED.failed_requests,
        skipped = EXCLUDED.skipped,
        rejected = EXCLUDED.rejected,
        stations = EXCLUDED.stations,
        observations = EXCLUDED.observations,
        newest_observation = EXCLUDED.newest_observation";

/// The box is compared as plain longitude and latitude. `&&` lets the index
/// find candidates; the index stores rounded boxes, so the exact comparison
/// follows. The limit is applied before the newest observation of each
/// station is looked up, one index probe per station.
const STATIONS_IN: &str = "
    SELECT s.id, s.name, s.elevation_m, s.lat, s.lon,
           o.time, o.wind_speed_ms, o.wind_gust_ms, o.wind_direction_deg,
           o.gust_direction_deg, o.temperature_c, o.humidity_pct, o.pressure_hpa
    FROM (
        SELECT s.id, s.source, s.name, s.elevation_m,
               ST_Y(s.position::geometry) AS lat, ST_X(s.position::geometry) AS lon
        FROM stations s
        WHERE s.position::geometry && ST_MakeEnvelope($1, $2, $3, $4, 4326)
          AND ST_X(s.position::geometry) BETWEEN $1 AND $3
          AND ST_Y(s.position::geometry) BETWEEN $2 AND $4
        ORDER BY s.source, s.id
        LIMIT $5
    ) s
    LEFT JOIN LATERAL (
        SELECT * FROM observations o
        WHERE o.station_id = s.id
        ORDER BY o.time DESC
        LIMIT 1
    ) o ON true
    ORDER BY s.source, s.id";

/// Each lookup walks the run history of one source backwards from its newest
/// run, on the primary key.
const SOURCE_STATUS: &str = "
    SELECT s.source,
           latest.started_at AS last_started,
           success.finished_at AS last_success,
           COALESCE(success.stations, 0) AS stations,
           failure.finished_at AS last_failure,
           newest.newest_observation
    FROM unnest($1::text[]) WITH ORDINALITY AS s (source, ordinal)
    LEFT JOIN LATERAL (
        SELECT r.started_at FROM source_runs r
        WHERE r.source = s.source
        ORDER BY r.started_at DESC LIMIT 1
    ) latest ON true
    LEFT JOIN LATERAL (
        SELECT r.finished_at, r.stations FROM source_runs r
        WHERE r.source = s.source AND r.healthy
        ORDER BY r.started_at DESC LIMIT 1
    ) success ON true
    LEFT JOIN LATERAL (
        SELECT r.finished_at FROM source_runs r
        WHERE r.source = s.source AND NOT r.healthy
        ORDER BY r.started_at DESC LIMIT 1
    ) failure ON true
    LEFT JOIN LATERAL (
        SELECT r.newest_observation FROM source_runs r
        WHERE r.source = s.source AND r.newest_observation IS NOT NULL
        ORDER BY r.started_at DESC LIMIT 1
    ) newest ON true
    ORDER BY s.ordinal";

impl Store for PgStore {
    async fn save_run(
        &self,
        run: &RunRecord,
        stations: &[Station],
        observations: &[Observation],
    ) -> Result<(), StoreError> {
        let months = batch::check(run, stations, observations)?;
        let mut client = self.client().await?;
        partitions::ensure(&mut client, &months).await?;

        let tx = client.transaction().await.map_err(db_error)?;

        if !stations.is_empty() {
            let ids: Vec<String> = stations.iter().map(|s| s.id.to_string()).collect();
            let sources: Vec<&str> = stations.iter().map(|s| s.id.source().as_str()).collect();
            let names: Vec<&str> = stations.iter().map(|s| s.name.as_str()).collect();
            let lons: Vec<f64> = stations.iter().map(|s| s.position.lon()).collect();
            let lats: Vec<f64> = stations.iter().map(|s| s.position.lat()).collect();
            let elevations: Vec<Option<f64>> = stations.iter().map(|s| s.elevation_m).collect();
            let statement = tx.prepare_cached(UPSERT_STATIONS).await.map_err(db_error)?;
            tx.execute(
                &statement,
                &[
                    &ids,
                    &sources,
                    &names,
                    &lons,
                    &lats,
                    &elevations,
                    &run.started_at,
                ],
            )
            .await
            .map_err(db_error)?;
        }

        if !observations.is_empty() {
            let ids: Vec<String> = observations.iter().map(|o| o.station.to_string()).collect();
            let times: Vec<Timestamp> = observations.iter().map(|o| o.time).collect();
            // One array per measurement, in the order of `Observation::fields`.
            let mut columns: [Vec<Option<f64>>; 7] = Default::default();
            for observation in observations {
                for (column, (_, value)) in columns.iter_mut().zip(observation.fields()) {
                    column.push(value);
                }
            }
            let [
                speed,
                gust,
                direction,
                gust_direction,
                temperature,
                humidity,
                pressure,
            ] = &columns;
            let statement = tx
                .prepare_cached(UPSERT_OBSERVATIONS)
                .await
                .map_err(db_error)?;
            tx.execute(
                &statement,
                &[
                    &ids,
                    &times,
                    speed,
                    gust,
                    direction,
                    gust_direction,
                    temperature,
                    humidity,
                    pressure,
                ],
            )
            .await
            .map_err(db_error)?;
        }

        let statement = tx.prepare_cached(UPSERT_RUN).await.map_err(db_error)?;
        tx.execute(
            &statement,
            &[
                &run.source.as_str(),
                &run.started_at,
                &run.finished_at,
                &run.healthy,
                &count(run.requests),
                &count(run.failed_requests),
                &count(run.skipped),
                &count(run.rejected),
                &count(run.stations),
                &count(run.observations),
                &run.newest_observation,
            ],
        )
        .await
        .map_err(db_error)?;

        tx.commit().await.map_err(db_error)
    }

    async fn stations_in(
        &self,
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<StationLatest>, StoreError> {
        let client = self.client().await?;
        let statement = client.prepare_cached(STATIONS_IN).await.map_err(db_error)?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = client
            .query(
                &statement,
                &[
                    &area.west(),
                    &area.south(),
                    &area.east(),
                    &area.north(),
                    &limit,
                ],
            )
            .await
            .map_err(db_error)?;
        rows.iter()
            .map(|row| {
                let station = station_from(row)?;
                let time: Option<Timestamp> = row.get("time");
                let latest = time.map(|time| observation_from(station.id.clone(), time, row));
                Ok(StationLatest { station, latest })
            })
            .collect()
    }

    async fn station(&self, id: &StationId) -> Result<Option<Station>, StoreError> {
        let client = self.client().await?;
        let query = format!("SELECT {STATION_COLUMNS} FROM stations s WHERE s.id = $1");
        let statement = client.prepare_cached(&query).await.map_err(db_error)?;
        let row = client
            .query_opt(&statement, &[&id.to_string()])
            .await
            .map_err(db_error)?;
        row.as_ref().map(station_from).transpose()
    }

    async fn observations(
        &self,
        station: &StationId,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Vec<Observation>, StoreError> {
        let client = self.client().await?;
        let statement = client
            .prepare_cached(
                "SELECT * FROM observations
                 WHERE station_id = $1 AND time >= $2 AND time <= $3
                 ORDER BY time",
            )
            .await
            .map_err(db_error)?;
        let rows = client
            .query(&statement, &[&station.to_string(), &from, &to])
            .await
            .map_err(db_error)?;
        Ok(rows
            .iter()
            .map(|row| observation_from(station.clone(), row.get("time"), row))
            .collect())
    }

    async fn source_status(&self, sources: &[SourceId]) -> Result<Vec<SourceStatus>, StoreError> {
        let client = self.client().await?;
        let statement = client
            .prepare_cached(SOURCE_STATUS)
            .await
            .map_err(db_error)?;
        let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
        let rows = client
            .query(&statement, &[&names])
            .await
            .map_err(db_error)?;
        Ok(sources
            .iter()
            .zip(&rows)
            .map(|(source, row)| SourceStatus {
                source: source.clone(),
                last_started: row.get("last_started"),
                last_success: row.get("last_success"),
                last_failure: row.get("last_failure"),
                stations: u32::try_from(row.get::<_, i32>("stations")).unwrap_or(0),
                newest_observation: row.get("newest_observation"),
            })
            .collect())
    }

    async fn prune_before(&self, first_kept: Month) -> Result<(), StoreError> {
        let cutoff = first_kept.start();
        let mut client = self.client().await?;
        let tx = client.transaction().await.map_err(db_error)?;
        partitions::drop_before(&tx, first_kept).await?;
        // Observations of a deleted station go with it.
        tx.execute("DELETE FROM stations WHERE last_seen_at < $1", &[&cutoff])
            .await
            .map_err(db_error)?;
        tx.execute("DELETE FROM source_runs WHERE started_at < $1", &[&cutoff])
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)
    }
}
