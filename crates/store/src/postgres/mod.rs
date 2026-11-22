//! The store in PostgreSQL with PostGIS.

mod accounts;
mod airspace;
mod alerts;
mod cables;
mod flights;
mod forecasts;
mod groups;
mod kept_sites;
mod live;
mod migrate;
mod oauth;
mod partitions;
mod pilot_sites;
mod push;
mod reports;
mod sites;
mod social;
mod takeoffs;

use std::time::Duration;

use chrono::NaiveDate;
use deadpool_postgres::{
    Client, Manager, ManagerConfig, Pool, PoolError, RecyclingMethod, Runtime, Transaction,
};
use tokio_postgres::{NoTls, Row};
use venti_domain::{
    Airspace, BoundingBox, Cable, GeoPoint, Observation, Site, SiteId, SourceId, Station,
    StationId, Timestamp,
};

use crate::flight::{Room, Saved};
use crate::{
    AccessToken, Account, AccountCreation, AirspaceLayer, AirspaceSet, CableLayer, CableSet,
    Coverage, Derived, Flight, FlightChanges, FlightKey, ForecastModel, Group, GroupCreation,
    GroupJoining, GroupMembers, GroupRoom, KeeperAct, Listing, LiveBatch, LiveFix, LiveFixAt,
    LiveRemoval, LiveRoom, LiveSaved, LiveTrack, MemberRemoval, Month, NewAccount, NewFlight,
    NewGroup, NewInvite, NewPilotSite, NewPlan, NewReport, NewSiteConfirmation, OutdatedFlight,
    PilotSite, PilotSiteChanges, Plan, PlanChange, PlanDetails, PlanRemoval, PlanRoom, PlanSaving,
    PublicFlightChanges, PublicFlights, PublicOrder, Ranked, RemovedGroup, Report, ReportClosing,
    ReportKey, ReportListing, ReportSaved, RunRecord, SecretHash, Session, SiteConfirmation,
    SiteConfirming, SiteDelivery, SiteLayer, SiteRevising, SiteRevision, SiteSet, SiteWrite,
    SourceStatus, StationLatest, Store, StoreError, Suspension, ThermalPart, Visibility,
    VisibilityChange, batch,
};
use crate::{CandidateRule, FlightsFrom, Flown, FoundPilot, TakeoffCandidate};
use crate::{Given, KeepingSite, KeptRoom, KeptSiteChanges, NewKeptSite};
use crate::{GoogleLink, GoogleLinking, NewGoogleAccount};

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
         skipped, rejected, stations, observations, newest_observation, airspaces)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    ON CONFLICT (source, started_at) DO UPDATE
    SET finished_at = EXCLUDED.finished_at,
        healthy = EXCLUDED.healthy,
        requests = EXCLUDED.requests,
        failed_requests = EXCLUDED.failed_requests,
        skipped = EXCLUDED.skipped,
        rejected = EXCLUDED.rejected,
        stations = EXCLUDED.stations,
        observations = EXCLUDED.observations,
        newest_observation = EXCLUDED.newest_observation,
        airspaces = EXCLUDED.airspaces";

/// Writes the outcome of a run into the run history, as part of the
/// transaction that stores what the run delivered.
async fn upsert_run(tx: &Transaction<'_>, run: &RunRecord) -> Result<(), StoreError> {
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
            &count(run.airspaces),
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

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

        upsert_run(&tx, run).await?;
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

    async fn save_airspace_run(
        &self,
        run: &RunRecord,
        coverage: &Coverage,
        airspaces: &[Airspace],
    ) -> Result<(), StoreError> {
        airspace::save_run(&mut self.client().await?, run, coverage, airspaces).await
    }

    async fn airspaces_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<AirspaceLayer>, StoreError> {
        airspace::in_box(&self.client().await?, sources, area, limit).await
    }

    async fn airspace_sets(&self, sources: &[SourceId]) -> Result<Vec<AirspaceSet>, StoreError> {
        airspace::sets(&self.client().await?, sources).await
    }

    async fn save_site_run(
        &self,
        run: &RunRecord,
        delivery: &SiteDelivery,
        sites: &[Site],
    ) -> Result<(), StoreError> {
        sites::save_run(&mut self.client().await?, run, delivery, sites).await
    }

    async fn sites_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<SiteLayer>, StoreError> {
        sites::in_box(&self.client().await?, sources, area, limit).await
    }

    async fn site_sets(&self, sources: &[SourceId]) -> Result<Vec<SiteSet>, StoreError> {
        sites::sets(&self.client().await?, sources).await
    }

    async fn save_cable_run(
        &self,
        run: &RunRecord,
        delivery: &SiteDelivery,
        cables: &[Cable],
    ) -> Result<(), StoreError> {
        cables::save_run(&mut self.client().await?, run, delivery, cables).await
    }

    async fn cables_in(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<CableLayer>, StoreError> {
        cables::in_box(&self.client().await?, sources, area, limit).await
    }

    async fn cable_sets(&self, sources: &[SourceId]) -> Result<Vec<CableSet>, StoreError> {
        cables::sets(&self.client().await?, sources).await
    }

    async fn set_site(&self, id: &SiteId) -> Result<Option<Site>, StoreError> {
        sites::one(&self.client().await?, id).await
    }

    async fn add_pilot_site(
        &self,
        new: &NewPilotSite,
        apart_m: f64,
    ) -> Result<SiteWrite, StoreError> {
        pilot_sites::add(&mut self.client().await?, new, apart_m).await
    }

    async fn revise_pilot_site(
        &self,
        revising: &SiteRevising,
        apart_m: f64,
    ) -> Result<SiteWrite, StoreError> {
        pilot_sites::revise(&mut self.client().await?, revising, apart_m).await
    }

    async fn pilot_sites_in(
        &self,
        layer: &SourceId,
        area: &BoundingBox,
        limit: usize,
    ) -> Result<Vec<PilotSite>, StoreError> {
        pilot_sites::in_box(&self.client().await?, layer, area, limit).await
    }

    async fn pilot_site(&self, id: &SiteId) -> Result<Option<PilotSite>, StoreError> {
        pilot_sites::one(&self.client().await?, id).await
    }

    async fn pilot_site_revisions(
        &self,
        id: &SiteId,
        after: u32,
        limit: usize,
    ) -> Result<Option<(PilotSite, Vec<SiteRevision>)>, StoreError> {
        pilot_sites::revisions(&self.client().await?, id, after, limit).await
    }

    async fn pilot_sites_revisions(&self, ids: &[SiteId]) -> Result<Vec<SiteRevision>, StoreError> {
        pilot_sites::revisions_of(&self.client().await?, ids).await
    }

    async fn pilot_site_changes(
        &self,
        layer: &SourceId,
        since: i64,
        limit: usize,
    ) -> Result<PilotSiteChanges, StoreError> {
        pilot_sites::changes(&self.client().await?, layer, since, limit).await
    }

    async fn given_by(&self, account_id: &str) -> Result<Given, StoreError> {
        // PostgreSQL takes no zero byte in text, and no account's
        // identifier holds one.
        if account_id.contains('\0') {
            return Ok(Given::default());
        }
        let client = self.client().await?;
        Ok(Given {
            revisions: pilot_sites::revisions_by(&client, account_id).await?,
            confirmations: pilot_sites::confirmations_by(&client, account_id).await?,
            reports: reports::sent_by(&client, account_id).await?,
        })
    }

    async fn confirm_pilot_site(
        &self,
        new: &NewSiteConfirmation,
    ) -> Result<SiteConfirming, StoreError> {
        pilot_sites::confirm(&mut self.client().await?, new).await
    }

    async fn withdraw_site_confirmation(
        &self,
        site: &SiteId,
        account_id: &str,
    ) -> Result<Option<(PilotSite, bool)>, StoreError> {
        pilot_sites::withdraw(&mut self.client().await?, site, account_id).await
    }

    async fn site_confirmations(
        &self,
        site: &SiteId,
        limit: usize,
    ) -> Result<Option<(PilotSite, Vec<SiteConfirmation>)>, StoreError> {
        pilot_sites::confirmations(&self.client().await?, site, limit).await
    }

    async fn save_forecast_check(
        &self,
        run: &RunRecord,
        models: &[ForecastModel],
    ) -> Result<(), StoreError> {
        forecasts::save_check(&mut self.client().await?, run, models).await
    }

    async fn forecast_models(
        &self,
        sources: &[SourceId],
    ) -> Result<Vec<ForecastModel>, StoreError> {
        forecasts::models(&self.client().await?, sources).await
    }

    async fn save_thermal_part(&self, part: &ThermalPart) -> Result<(), StoreError> {
        forecasts::save_thermal_part(&self.client().await?, part).await
    }

    async fn thermal_parts(&self, source: &SourceId) -> Result<Vec<ThermalPart>, StoreError> {
        forecasts::thermal_parts(&self.client().await?, source).await
    }

    async fn create_account(&self, new: &NewAccount) -> Result<AccountCreation, StoreError> {
        accounts::create(&mut self.client().await?, new).await
    }

    async fn account(&self, handle: &str) -> Result<Option<Account>, StoreError> {
        accounts::by_handle(&self.client().await?, handle).await
    }

    async fn password_hash(&self, handle: &str) -> Result<Option<(Account, String)>, StoreError> {
        accounts::password_hash(&self.client().await?, handle).await
    }

    async fn create_google_account(
        &self,
        new: &NewGoogleAccount,
    ) -> Result<AccountCreation, StoreError> {
        accounts::create_google(&mut self.client().await?, new).await
    }

    async fn google_account(&self, subject: &str) -> Result<Option<Account>, StoreError> {
        accounts::by_google(&self.client().await?, subject).await
    }

    async fn google_link(&self, account_id: &str) -> Result<Option<GoogleLink>, StoreError> {
        accounts::google_link(&self.client().await?, account_id).await
    }

    async fn link_google(
        &self,
        account_id: &str,
        google: &GoogleLink,
    ) -> Result<GoogleLinking, StoreError> {
        accounts::link_google(&mut self.client().await?, account_id, google).await
    }

    async fn unlink_google(&self, account_id: &str) -> Result<bool, StoreError> {
        accounts::unlink_google(&self.client().await?, account_id).await
    }

    async fn has_password(&self, account_id: &str) -> Result<bool, StoreError> {
        accounts::has_password(&self.client().await?, account_id).await
    }

    async fn set_password(
        &self,
        account_id: &str,
        password_hash: &str,
        keep_session: &str,
    ) -> Result<(), StoreError> {
        let mut client = self.client().await?;
        accounts::set_password(&mut client, account_id, password_hash, keep_session).await
    }

    async fn delete_account(&self, account_id: &str) -> Result<(), StoreError> {
        accounts::delete(&mut self.client().await?, account_id).await
    }

    async fn suspend_publishing(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Option<Suspension>, StoreError> {
        accounts::suspend_publishing(&mut self.client().await?, account_id, now).await
    }

    async fn resume_publishing(&self, account_id: &str) -> Result<Option<bool>, StoreError> {
        accounts::resume_publishing(&mut self.client().await?, account_id).await
    }

    async fn let_edit_sites(
        &self,
        account_id: &str,
        now: Timestamp,
    ) -> Result<Option<Timestamp>, StoreError> {
        accounts::let_edit_sites(&self.client().await?, account_id, now).await
    }

    async fn stop_editing_sites(&self, account_id: &str) -> Result<Option<bool>, StoreError> {
        accounts::stop_editing_sites(&mut self.client().await?, account_id).await
    }

    async fn site_editors(&self) -> Result<Vec<Account>, StoreError> {
        accounts::site_editors(&self.client().await?).await
    }

    async fn create_invite(&self, invite: &NewInvite) -> Result<(), StoreError> {
        accounts::create_invite(&self.client().await?, invite).await
    }

    async fn create_session(
        &self,
        session: &Session,
        token: &SecretHash,
        keep: usize,
    ) -> Result<(), StoreError> {
        accounts::create_session(&mut self.client().await?, session, token, keep).await
    }

    async fn use_session(
        &self,
        token: &SecretHash,
        now: Timestamp,
        idle_since: Timestamp,
    ) -> Result<Option<(Session, Account)>, StoreError> {
        accounts::use_session(&self.client().await?, token, now, idle_since).await
    }

    async fn sessions(&self, account_id: &str) -> Result<Vec<Session>, StoreError> {
        accounts::sessions(&self.client().await?, account_id).await
    }

    async fn delete_session(&self, account_id: &str, session_id: &str) -> Result<bool, StoreError> {
        accounts::delete_session(&self.client().await?, account_id, session_id).await
    }

    async fn delete_sessions(&self, account_id: &str) -> Result<(), StoreError> {
        accounts::delete_sessions(&self.client().await?, account_id).await
    }

    async fn prune_sessions(&self, idle_since: Timestamp) -> Result<u64, StoreError> {
        accounts::prune_sessions(&self.client().await?, idle_since).await
    }

    async fn create_access_token(
        &self,
        token: &AccessToken,
        secret: &SecretHash,
        most: usize,
    ) -> Result<bool, StoreError> {
        accounts::create_access_token(&mut self.client().await?, token, secret, most).await
    }

    async fn use_access_token(
        &self,
        secret: &SecretHash,
        now: Timestamp,
    ) -> Result<Option<(AccessToken, Account)>, StoreError> {
        accounts::use_access_token(&self.client().await?, secret, now).await
    }

    async fn access_tokens(&self, account_id: &str) -> Result<Vec<AccessToken>, StoreError> {
        accounts::access_tokens(&self.client().await?, account_id).await
    }

    async fn delete_access_token(
        &self,
        account_id: &str,
        token_id: &str,
    ) -> Result<bool, StoreError> {
        accounts::delete_access_token(&self.client().await?, account_id, token_id).await
    }

    async fn create_oauth_client(
        &self,
        client: &crate::OauthClient,
        most: usize,
    ) -> Result<bool, StoreError> {
        oauth::create_oauth_client(&mut self.client().await?, client, most).await
    }

    async fn oauth_client(
        &self,
        client_id: &str,
    ) -> Result<Option<crate::OauthClient>, StoreError> {
        oauth::oauth_client(&self.client().await?, client_id).await
    }

    async fn create_authorization_code(
        &self,
        code: &SecretHash,
        record: &crate::AuthorizationCode,
    ) -> Result<bool, StoreError> {
        oauth::create_authorization_code(&mut self.client().await?, code, record).await
    }

    async fn authorization_code(
        &self,
        code: &SecretHash,
        now: Timestamp,
    ) -> Result<Option<crate::StoredCode>, StoreError> {
        oauth::authorization_code(&self.client().await?, code, now).await
    }

    async fn redeem_authorization_code(
        &self,
        code: &SecretHash,
        grant: &crate::NewGrant,
        now: Timestamp,
        most: usize,
    ) -> Result<crate::Redeemed, StoreError> {
        oauth::redeem_authorization_code(&mut self.client().await?, code, grant, now, most).await
    }

    async fn refresh_grant(
        &self,
        refresh: &SecretHash,
        client_id: &str,
        next: &crate::Rotation,
        now: Timestamp,
    ) -> Result<crate::Refreshed, StoreError> {
        oauth::refresh_grant(&mut self.client().await?, refresh, client_id, next, now).await
    }

    async fn revoke_grant(&self, secret: &SecretHash, client_id: &str) -> Result<bool, StoreError> {
        oauth::revoke_grant(&self.client().await?, secret, client_id).await
    }

    async fn prune_oauth(
        &self,
        now: Timestamp,
        unused_since: Timestamp,
    ) -> Result<u64, StoreError> {
        oauth::prune_oauth(&mut self.client().await?, now, unused_since).await
    }

    async fn keep_site(
        &self,
        account_id: &str,
        new: &NewKeptSite,
        now: Timestamp,
        room: KeptRoom,
    ) -> Result<KeepingSite, StoreError> {
        kept_sites::keep(&mut self.client().await?, account_id, new, now, room).await
    }

    async fn forget_kept_site(
        &self,
        account_id: &str,
        site_id: &str,
        now: Timestamp,
        room: KeptRoom,
    ) -> Result<bool, StoreError> {
        kept_sites::forget(&mut self.client().await?, account_id, site_id, now, room).await
    }

    async fn kept_site_changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<KeptSiteChanges, StoreError> {
        kept_sites::changes(&self.client().await?, account_id, since, limit).await
    }

    async fn alerts(&self, account_id: &str) -> Result<Vec<crate::Alert>, StoreError> {
        alerts::of(&self.client().await?, account_id).await
    }

    async fn add_alert(
        &self,
        account_id: &str,
        new: &crate::NewAlert,
        now: Timestamp,
        most: usize,
    ) -> Result<crate::AddingAlert, StoreError> {
        alerts::add(&mut self.client().await?, account_id, new, now, most).await
    }

    async fn change_alert(
        &self,
        account_id: &str,
        new: &crate::NewAlert,
        now: Timestamp,
    ) -> Result<Option<crate::Alert>, StoreError> {
        alerts::change(&self.client().await?, account_id, new, now).await
    }

    async fn remove_alert(&self, account_id: &str, alert_id: &str) -> Result<bool, StoreError> {
        alerts::remove(&self.client().await?, account_id, alert_id).await
    }

    async fn wind_alerts_at(
        &self,
        station_ids: &[String],
    ) -> Result<Vec<crate::Alert>, StoreError> {
        alerts::at_stations(&self.client().await?, station_ids).await
    }

    async fn alerts_of_kind(&self, kind: &str) -> Result<Vec<crate::Alert>, StoreError> {
        alerts::of_kind(&self.client().await?, kind).await
    }

    async fn note_alert_check(&self, check: &crate::AlertCheck) -> Result<bool, StoreError> {
        alerts::note(&mut self.client().await?, check).await
    }

    async fn fired_alerts(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<crate::FiredAlerts, StoreError> {
        alerts::fired(&self.client().await?, account_id, since, limit).await
    }

    async fn follow(
        &self,
        follower_id: &str,
        followed_id: &str,
        now: Timestamp,
        most: usize,
    ) -> Result<crate::Following, StoreError> {
        social::follow(
            &mut self.client().await?,
            follower_id,
            followed_id,
            now,
            most,
        )
        .await
    }

    async fn unfollow(&self, follower_id: &str, followed_id: &str) -> Result<bool, StoreError> {
        social::unfollow(&self.client().await?, follower_id, followed_id).await
    }

    async fn follow_counts(&self, account_id: &str) -> Result<crate::FollowCounts, StoreError> {
        social::counts(&self.client().await?, account_id).await
    }

    async fn follows_pilot(
        &self,
        follower_id: &str,
        followed_id: &str,
    ) -> Result<bool, StoreError> {
        social::follows_pilot(&self.client().await?, follower_id, followed_id).await
    }

    async fn follows(
        &self,
        account_id: &str,
        side: crate::FollowSide,
        with_a_page: bool,
        after: Option<&crate::SocialKey>,
        limit: usize,
    ) -> Result<Vec<crate::SocialPilot>, StoreError> {
        social::follows(
            &self.client().await?,
            account_id,
            side,
            with_a_page,
            after,
            limit,
        )
        .await
    }

    async fn followed_flights(
        &self,
        account_id: &str,
        after: Option<&crate::FlightKey>,
        limit: usize,
    ) -> Result<Vec<crate::Flight>, StoreError> {
        social::followed_flights(&self.client().await?, account_id, after, limit).await
    }

    async fn cheer(
        &self,
        account_id: &str,
        flight_id: &str,
        now: Timestamp,
    ) -> Result<crate::Cheering, StoreError> {
        social::cheer(&self.client().await?, account_id, flight_id, now).await
    }

    async fn uncheer(&self, account_id: &str, flight_id: &str) -> Result<bool, StoreError> {
        social::uncheer(&self.client().await?, account_id, flight_id).await
    }

    async fn cheers(
        &self,
        flight_id: &str,
        after: Option<&crate::SocialKey>,
        limit: usize,
    ) -> Result<Vec<crate::SocialPilot>, StoreError> {
        social::cheers(&self.client().await?, flight_id, after, limit).await
    }

    async fn flight_social(
        &self,
        flight_ids: &[String],
        viewer: Option<&str>,
    ) -> Result<Vec<crate::FlightSocial>, StoreError> {
        social::of_flights(&self.client().await?, flight_ids, viewer).await
    }

    async fn add_comment(&self, new: &crate::NewComment) -> Result<crate::Commenting, StoreError> {
        social::add_comment(&mut self.client().await?, new).await
    }

    async fn comments(
        &self,
        flight_id: &str,
        after: Option<&crate::SocialKey>,
        limit: usize,
    ) -> Result<Vec<crate::Comment>, StoreError> {
        social::comments(&self.client().await?, flight_id, after, limit).await
    }

    async fn comment(&self, id: &str) -> Result<Option<crate::Comment>, StoreError> {
        social::comment(&self.client().await?, id).await
    }

    async fn remove_comment(&self, id: &str) -> Result<Option<crate::Comment>, StoreError> {
        social::remove_comment(&self.client().await?, id).await
    }

    async fn close_comments(
        &self,
        account_id: &str,
        flight_id: &str,
        closed: bool,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        social::close_comments(&self.client().await?, account_id, flight_id, closed, now).await
    }

    async fn social_given(&self, account_id: &str) -> Result<crate::SocialGiven, StoreError> {
        social::given(&self.client().await?, account_id).await
    }

    async fn add_notice(
        &self,
        account_id: &str,
        actor_id: &str,
        at: Timestamp,
        record: &str,
    ) -> Result<bool, StoreError> {
        social::add_notice(&mut self.client().await?, account_id, actor_id, at, record).await
    }

    async fn notices(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<crate::Notices, StoreError> {
        social::notices(&self.client().await?, account_id, since, limit).await
    }

    async fn prune_notices(&self, before: Timestamp) -> Result<u64, StoreError> {
        social::prune_notices(&self.client().await?, before).await
    }

    async fn prune_fired_alerts(&self, before: Timestamp) -> Result<u64, StoreError> {
        alerts::prune(&self.client().await?, before).await
    }

    async fn add_push_subscription(
        &self,
        subscription: &crate::PushSubscription,
        most: usize,
    ) -> Result<crate::AddingPush, StoreError> {
        push::add(&mut self.client().await?, subscription, most).await
    }

    async fn remove_push_subscription(
        &self,
        account_id: &str,
        endpoint: &str,
    ) -> Result<bool, StoreError> {
        push::remove(&self.client().await?, account_id, endpoint).await
    }

    async fn drop_push_subscription(&self, endpoint: &str) -> Result<bool, StoreError> {
        push::drop(&self.client().await?, endpoint).await
    }

    async fn push_subscriptions(
        &self,
        account_id: &str,
    ) -> Result<Vec<crate::PushSubscription>, StoreError> {
        push::of(&self.client().await?, account_id).await
    }

    async fn push_key(&self) -> Result<Option<String>, StoreError> {
        push::key(&self.client().await?).await
    }

    async fn keep_push_key(&self, key: &str, now: Timestamp) -> Result<String, StoreError> {
        push::keep_key(&self.client().await?, key, now).await
    }

    async fn save_flight(&self, new: &NewFlight, room: &Room) -> Result<Saved, StoreError> {
        flights::save(&mut self.client().await?, new, room).await
    }

    async fn flight(&self, id: &str) -> Result<Option<Flight>, StoreError> {
        flights::get(&self.client().await?, id).await
    }

    async fn flight_file(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        flights::file(&self.client().await?, id).await
    }

    async fn flights_of(
        &self,
        account_id: &str,
        listing: Listing,
        after: Option<&FlightKey>,
        limit: usize,
    ) -> Result<Vec<Flight>, StoreError> {
        flights::of_account(&self.client().await?, account_id, listing, after, limit).await
    }

    async fn public_flights(
        &self,
        which: &PublicFlights,
        order: &PublicOrder,
        limit: usize,
    ) -> Result<Vec<Flight>, StoreError> {
        flights::public(&self.client().await?, which, order, limit).await
    }

    async fn flights_from(
        &self,
        place: GeoPoint,
        within_m: f64,
        best: usize,
    ) -> Result<FlightsFrom, StoreError> {
        takeoffs::began_at(&mut self.client().await?, place, within_m, best).await
    }

    async fn sites_near(
        &self,
        sources: &[SourceId],
        places: &[GeoPoint],
        within_m: f64,
    ) -> Result<Vec<Vec<Site>>, StoreError> {
        takeoffs::sites_near(&self.client().await?, sources, places, within_m).await
    }

    async fn takeoff_candidates(
        &self,
        sources: &[SourceId],
        area: &BoundingBox,
        rule: &CandidateRule,
        limit: usize,
    ) -> Result<Vec<TakeoffCandidate>, StoreError> {
        takeoffs::candidates(&self.client().await?, sources, area, rule, limit).await
    }

    async fn ranking(
        &self,
        which: &PublicFlights,
        counted: usize,
        limit: usize,
    ) -> Result<Vec<Ranked>, StoreError> {
        flights::ranking(&self.client().await?, which, counted, limit).await
    }

    async fn scored_months(&self, which: &PublicFlights) -> Result<Vec<(Month, u32)>, StoreError> {
        flights::scored_months(&self.client().await?, which).await
    }

    async fn pilots_named(&self, text: &str, limit: usize) -> Result<Vec<FoundPilot>, StoreError> {
        flights::pilots_named(&self.client().await?, text, limit).await
    }

    async fn flown_by(&self, account_id: &str) -> Result<Vec<Flown>, StoreError> {
        flights::flown_by(&self.client().await?, account_id).await
    }

    async fn flight_changes(
        &self,
        account_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<FlightChanges, StoreError> {
        flights::changes(&mut self.client().await?, account_id, since, limit).await
    }

    async fn public_flight_changes(
        &self,
        account_id: Option<&str>,
        since: i64,
        limit: usize,
    ) -> Result<PublicFlightChanges, StoreError> {
        flights::public_changes(&mut self.client().await?, account_id, since, limit).await
    }

    async fn set_visibility(
        &self,
        account_id: &str,
        id: &str,
        visibility: Visibility,
        now: Timestamp,
    ) -> Result<VisibilityChange, StoreError> {
        let mut client = self.client().await?;
        flights::set_visibility(&mut client, account_id, id, visibility, now).await
    }

    async fn take_down_flight(
        &self,
        id: &str,
        now: Timestamp,
    ) -> Result<Option<Flight>, StoreError> {
        flights::take_down(&mut self.client().await?, id, now).await
    }

    async fn release_flight(&self, id: &str, now: Timestamp) -> Result<Option<Flight>, StoreError> {
        flights::release(&mut self.client().await?, id, now).await
    }

    async fn delete_flight(
        &self,
        account_id: &str,
        id: &str,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        flights::delete(&mut self.client().await?, account_id, id, now).await
    }

    async fn outdated_flights(
        &self,
        version: u32,
        after: Option<&(u32, String)>,
        limit: usize,
    ) -> Result<Vec<OutdatedFlight>, StoreError> {
        flights::outdated(&self.client().await?, version, after, limit).await
    }

    async fn set_derived(
        &self,
        id: &str,
        derived: &Derived,
        now: Timestamp,
    ) -> Result<bool, StoreError> {
        flights::set_derived(&mut self.client().await?, id, derived, now).await
    }

    async fn save_live_fixes(
        &self,
        batch: &LiveBatch,
        room: &LiveRoom,
    ) -> Result<LiveSaved, StoreError> {
        live::save(&mut self.client().await?, batch, room).await
    }

    async fn live_track(&self, id: &str) -> Result<Option<LiveTrack>, StoreError> {
        live::get(&self.client().await?, id).await
    }

    async fn live_fixes(
        &self,
        id: &str,
        since: i64,
        after: Option<Timestamp>,
        limit: usize,
    ) -> Result<Vec<LiveFixAt>, StoreError> {
        live::fixes(&self.client().await?, id, since, after, limit).await
    }

    async fn live_tail(&self, id: &str, limit: usize) -> Result<Vec<LiveFix>, StoreError> {
        live::tail(&self.client().await?, id, limit).await
    }

    async fn live_tracks_in(
        &self,
        area: &BoundingBox,
        newest_since: Timestamp,
        limit: usize,
    ) -> Result<Vec<LiveTrack>, StoreError> {
        live::in_box(&self.client().await?, area, newest_since, limit).await
    }

    async fn delete_live_track(
        &self,
        account_id: &str,
        id: &str,
        now: Timestamp,
        may_retire: bool,
    ) -> Result<LiveRemoval, StoreError> {
        live::delete(&mut self.client().await?, account_id, id, now, may_retire).await
    }

    async fn remove_live_track(&self, id: &str, now: Timestamp) -> Result<bool, StoreError> {
        live::remove(&mut self.client().await?, id, now).await
    }

    async fn prune_live_tracks(
        &self,
        ended_before: Timestamp,
        now: Timestamp,
    ) -> Result<u64, StoreError> {
        live::prune(&self.client().await?, ended_before, now).await
    }

    async fn create_group(
        &self,
        new: &NewGroup,
        room: &GroupRoom,
    ) -> Result<GroupCreation, StoreError> {
        groups::create(&mut self.client().await?, new, room).await
    }

    async fn join_group(
        &self,
        account_id: &str,
        invite: &SecretHash,
        room: &GroupRoom,
        now: Timestamp,
    ) -> Result<GroupJoining, StoreError> {
        groups::join(&mut self.client().await?, account_id, invite, room, now).await
    }

    async fn groups_of(&self, account_id: &str) -> Result<Vec<Group>, StoreError> {
        groups::of_account(&self.client().await?, account_id).await
    }

    async fn group(&self, id: &str, account_id: &str) -> Result<Option<GroupMembers>, StoreError> {
        groups::get(&mut self.client().await?, id, account_id).await
    }

    async fn rename_group(
        &self,
        id: &str,
        keeper_id: &str,
        name: &str,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        groups::rename(&mut self.client().await?, id, keeper_id, name).await
    }

    async fn renew_group_invite(
        &self,
        id: &str,
        keeper_id: &str,
        invite: &SecretHash,
    ) -> Result<KeeperAct<()>, StoreError> {
        groups::renew_invite(&mut self.client().await?, id, keeper_id, invite).await
    }

    async fn hand_over_group(
        &self,
        id: &str,
        keeper_id: &str,
        member_handle: &str,
    ) -> Result<KeeperAct<GroupMembers>, StoreError> {
        groups::hand_over(&mut self.client().await?, id, keeper_id, member_handle).await
    }

    async fn remove_group_member(
        &self,
        id: &str,
        by_account_id: &str,
        member_handle: &str,
    ) -> Result<MemberRemoval, StoreError> {
        groups::remove_member(&mut self.client().await?, id, by_account_id, member_handle).await
    }

    async fn delete_group(&self, id: &str, keeper_id: &str) -> Result<KeeperAct<()>, StoreError> {
        groups::delete(&mut self.client().await?, id, keeper_id).await
    }

    async fn save_plan(&self, new: &NewPlan, room: &PlanRoom) -> Result<PlanSaving, StoreError> {
        groups::save_plan(&mut self.client().await?, new, room).await
    }

    async fn change_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        details: &PlanDetails,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> Result<PlanChange, StoreError> {
        let mut client = self.client().await?;
        let plan = (group_id, plan_id);
        groups::change_plan(&mut client, plan, by_account_id, details, now, gone_before).await
    }

    async fn delete_plan(
        &self,
        group_id: &str,
        plan_id: &str,
        by_account_id: &str,
        gone_before: NaiveDate,
    ) -> Result<PlanRemoval, StoreError> {
        let mut client = self.client().await?;
        groups::delete_plan(&mut client, group_id, plan_id, by_account_id, gone_before).await
    }

    async fn set_plan_in(
        &self,
        group_id: &str,
        plan_id: &str,
        account_id: &str,
        is_in: bool,
        now: Timestamp,
        gone_before: NaiveDate,
    ) -> Result<PlanChange, StoreError> {
        let mut client = self.client().await?;
        let plan = (group_id, plan_id);
        groups::set_in(&mut client, plan, account_id, is_in, now, gone_before).await
    }

    async fn plans_of_group(
        &self,
        group_id: &str,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
    ) -> Result<Option<Vec<Plan>>, StoreError> {
        groups::plans_of_group(&mut self.client().await?, group_id, account_id, from, to).await
    }

    async fn plans_for(
        &self,
        account_id: &str,
        from: NaiveDate,
        to: Option<NaiveDate>,
        limit: usize,
    ) -> Result<Vec<Plan>, StoreError> {
        groups::plans_for(&mut self.client().await?, account_id, from, to, limit).await
    }

    async fn prune_plans(&self, before: NaiveDate) -> Result<u64, StoreError> {
        groups::prune_plans(&self.client().await?, before).await
    }

    async fn any_group(&self, id: &str) -> Result<Option<GroupMembers>, StoreError> {
        groups::any(&mut self.client().await?, id).await
    }

    async fn plans_of_any_group(
        &self,
        group_id: &str,
        from: NaiveDate,
    ) -> Result<Option<Vec<Plan>>, StoreError> {
        groups::plans_of_any(&mut self.client().await?, group_id, from).await
    }

    async fn any_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<Plan>, StoreError> {
        groups::any_plan(&mut self.client().await?, plan_id, gone_before).await
    }

    async fn rename_any_group(
        &self,
        id: &str,
        name: &str,
    ) -> Result<Option<GroupMembers>, StoreError> {
        groups::rename_any(&mut self.client().await?, id, name).await
    }

    async fn remove_plan(
        &self,
        plan_id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<Plan>, StoreError> {
        groups::remove_plan(&mut self.client().await?, plan_id, gone_before).await
    }

    async fn remove_group(
        &self,
        id: &str,
        gone_before: NaiveDate,
    ) -> Result<Option<RemovedGroup>, StoreError> {
        groups::remove(&mut self.client().await?, id, gone_before).await
    }

    async fn save_report(&self, new: &NewReport) -> Result<ReportSaved, StoreError> {
        reports::save(&self.client().await?, new).await
    }

    async fn reports(
        &self,
        listing: ReportListing,
        after: Option<&ReportKey>,
        limit: usize,
    ) -> Result<Vec<Report>, StoreError> {
        reports::list(&self.client().await?, listing, after, limit).await
    }

    async fn close_report(
        &self,
        id: &str,
        note: &str,
        now: Timestamp,
    ) -> Result<ReportClosing, StoreError> {
        reports::close(&self.client().await?, id, note, now).await
    }

    async fn prune_reports(&self, closed_before: Timestamp) -> Result<u64, StoreError> {
        reports::prune(&self.client().await?, closed_before).await
    }
}
