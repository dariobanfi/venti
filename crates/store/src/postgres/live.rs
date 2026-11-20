//! Live tracks in PostgreSQL.
//!
//! Whatever changes a track first locks the track's row, so the batches of
//! one track are taken in one after another. Two things can happen to an
//! identifier nothing is known of yet: a batch begins a track under it, or
//! a deletion takes it out of use. They must not both happen, and there is
//! no row to lock, so both first take a lock on the identifier itself (an
//! advisory lock on its hash) for the length of their transaction.

use std::collections::BTreeSet;

use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::Row;
use venti_domain::{BoundingBox, GeoPoint, Timestamp};

use super::{corrupt, db_error};
use crate::live::{Held, check, merge};
use crate::{
    LiveBatch, LiveFix, LiveFixAt, LiveKept, LiveRemoval, LiveRoom, LiveSaved, LiveTrack,
    LiveVisibility, StoreError,
};

/// The first half of the key of the lock on an identifier ("live" in
/// ASCII); the second is the identifier's hash. Two halves keep these
/// locks apart from the one that serialises schema changes. The value must
/// never change.
const LOCK_CLASS: i32 = 0x6c_69_76_65;

/// Everything [`stored_from`] reads: a track `t` and its account `a`.
const TRACK_COLUMNS: &str = "
    t.key, t.id, t.account_id, a.handle, a.display_name, t.visibility, t.landed,
    t.started_at, t.fixes, t.last_seq, t.last_at, t.last_lat, t.last_lon,
    t.last_altitude_m, t.last_ground_speed_ms, t.last_climb_ms, t.last_course_deg";

const TRACK_FROM: &str = "FROM live_tracks t JOIN accounts a ON a.id = t.account_id";

/// The columns of a fix `f`, as [`fix_from`] reads them.
const FIX_COLUMNS: &str =
    "f.seq, f.time, f.lat, f.lon, f.altitude_m, f.ground_speed_ms, f.climb_ms, f.course_deg";

/// A track as its row holds it.
struct Stored {
    key: i64,
    id: String,
    account_id: String,
    handle: String,
    name: String,
    held: Held,
}

impl Stored {
    fn track(&self) -> LiveTrack {
        self.held
            .track(&self.id, &self.account_id, &self.handle, &self.name)
    }
}

fn position(lat: f64, lon: f64) -> Result<GeoPoint, StoreError> {
    GeoPoint::new(lat, lon).map_err(|e| corrupt("live position", e))
}

fn stored_from(row: &Row) -> Result<Stored, StoreError> {
    let visibility: &str = row.get("visibility");
    let latest = LiveFix {
        time: row.get("last_at"),
        position: position(row.get("last_lat"), row.get("last_lon"))?,
        altitude_m: row.get("last_altitude_m"),
        ground_speed_ms: row.get("last_ground_speed_ms"),
        climb_ms: row.get("last_climb_ms"),
        course_deg: row.get("last_course_deg"),
    };
    Ok(Stored {
        key: row.get("key"),
        id: row.get("id"),
        account_id: row.get("account_id"),
        handle: row.get("handle"),
        name: row.get("display_name"),
        held: Held {
            visibility: LiveVisibility::from_name(visibility)
                .ok_or_else(|| corrupt("live visibility", visibility))?,
            landed: row.get("landed"),
            started_at: row.get("started_at"),
            latest,
            fixes: u32::try_from(row.get::<_, i32>("fixes"))
                .map_err(|_| corrupt("number of live fixes", "a negative number"))?,
            last_seq: row.get("last_seq"),
        },
    })
}

fn fix_from(row: &Row) -> Result<LiveFixAt, StoreError> {
    Ok(LiveFixAt {
        seq: row.get("seq"),
        fix: LiveFix {
            time: row.get("time"),
            position: position(row.get("lat"), row.get("lon"))?,
            altitude_m: row.get("altitude_m"),
            ground_speed_ms: row.get("ground_speed_ms"),
            climb_ms: row.get("climb_ms"),
            course_deg: row.get("course_deg"),
        },
    })
}

fn count(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn stored_count(fixes: &[LiveFixAt]) -> u32 {
    u32::try_from(fixes.len()).unwrap_or(u32::MAX)
}

/// The track with this identifier, its row locked until the transaction
/// ends.
async fn locked(tx: &Transaction<'_>, id: &str) -> Result<Option<Stored>, StoreError> {
    let query = format!("SELECT {TRACK_COLUMNS} {TRACK_FROM} WHERE t.id = $1 FOR UPDATE OF t");
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let row = tx.query_opt(&statement, &[&id]).await.map_err(db_error)?;
    row.as_ref().map(stored_from).transpose()
}

/// Locks an identifier until the transaction ends, whether or not anything
/// is stored under it.
async fn lock_identifier(tx: &Transaction<'_>, id: &str) -> Result<(), StoreError> {
    tx.execute(
        "SELECT pg_advisory_xact_lock($1, hashtext($2))",
        &[&LOCK_CLASS, &id],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

/// The account an identifier was taken out of use for, if it was.
async fn retired_by(tx: &Transaction<'_>, id: &str) -> Result<Option<String>, StoreError> {
    let row = tx
        .query_opt("SELECT account_id FROM live_retired WHERE id = $1", &[&id])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

/// Removes a track with its fixes and takes its identifier out of use.
async fn retire(tx: &Transaction<'_>, key: i64, now: Timestamp) -> Result<(), StoreError> {
    tx.execute(
        "WITH gone AS (DELETE FROM live_tracks WHERE key = $1 RETURNING id, account_id)
         INSERT INTO live_retired (id, account_id, retired_at)
         SELECT id, account_id, $2 FROM gone",
        &[&key, &now],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn insert_fixes(
    tx: &Transaction<'_>,
    key: i64,
    fixes: &[LiveFixAt],
) -> Result<(), StoreError> {
    if fixes.is_empty() {
        return Ok(());
    }
    let column = |value: fn(&LiveFix) -> Option<f64>| -> Vec<Option<f64>> {
        fixes.iter().map(|at| value(&at.fix)).collect()
    };
    let times: Vec<Timestamp> = fixes.iter().map(|at| at.fix.time).collect();
    let seqs: Vec<i64> = fixes.iter().map(|at| at.seq).collect();
    let lats: Vec<f64> = fixes.iter().map(|at| at.fix.position.lat()).collect();
    let lons: Vec<f64> = fixes.iter().map(|at| at.fix.position.lon()).collect();
    let altitudes: Vec<i32> = fixes.iter().map(|at| at.fix.altitude_m).collect();
    let statement = tx
        .prepare_cached(
            "INSERT INTO live_fixes
                 (track, time, seq, lat, lon, altitude_m, ground_speed_ms, climb_ms, course_deg)
             SELECT $1, t.* FROM unnest($2::timestamptz[], $3::int8[], $4::float8[],
                                        $5::float8[], $6::int4[], $7::float8[], $8::float8[],
                                        $9::float8[]) AS t",
        )
        .await
        .map_err(db_error)?;
    tx.execute(
        &statement,
        &[
            &key,
            &times,
            &seqs,
            &lats,
            &lons,
            &altitudes,
            &column(|fix| fix.ground_speed_ms),
            &column(|fix| fix.climb_ms),
            &column(|fix| fix.course_deg),
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

pub(super) async fn save(
    client: &mut Client,
    batch: &LiveBatch,
    room: &LiveRoom,
) -> Result<LiveSaved, StoreError> {
    check(batch, room)?;
    let tx = client.transaction().await.map_err(db_error)?;
    let mut existing = locked(&tx, &batch.id).await?;
    if existing.is_none() {
        // Nothing to lock yet. With the identifier itself locked, look
        // again: another call may have begun the track in the meantime.
        lock_identifier(&tx, &batch.id).await?;
        existing = locked(&tx, &batch.id).await?;
    }

    let Some(stored) = existing else {
        if retired_by(&tx, &batch.id).await?.is_some() {
            return Ok(LiveSaved::Gone);
        }
        if !room.may_create {
            return Ok(LiveSaved::NotCreated);
        }
        // The account's row is held against a change until this track is
        // stored. Whoever suspends the account's publishing changes the
        // row and narrows its public tracks after that: with the row held,
        // the suspension waits and finds this track, and with the
        // suspension first, this waits and reads that it is suspended.
        let account = tx
            .query_opt(
                "SELECT handle, display_name, publishing_suspended_at IS NOT NULL
                 FROM accounts WHERE id = $1 FOR KEY SHARE",
                &[&batch.account_id],
            )
            .await
            .map_err(db_error)?;
        let Some(account) = account else {
            return Err(StoreError::Invalid(
                "live fixes for an account that does not exist".to_owned(),
            ));
        };
        let (handle, name): (&str, &str) = (account.get(0), account.get(1));
        // An account that may not publish shares by link, whatever it asks.
        let narrowed;
        let batch = if account.get::<_, bool>(2) {
            narrowed = batch.for_those_with_the_link();
            &narrowed
        } else {
            batch
        };

        let merged = merge(None, batch, &BTreeSet::new(), room);
        let held = &merged.held;
        let latest = &held.latest;
        let row = tx
            .query_one(
                "INSERT INTO live_tracks
                     (id, account_id, visibility, landed, started_at, fixes, last_seq,
                      last_at, last_lat, last_lon, last_altitude_m, last_ground_speed_ms,
                      last_climb_ms, last_course_deg)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                 RETURNING key",
                &[
                    &batch.id,
                    &batch.account_id,
                    &held.visibility.as_str(),
                    &held.landed,
                    &held.started_at,
                    &count(held.fixes),
                    &held.last_seq,
                    &latest.time,
                    &latest.position.lat(),
                    &latest.position.lon(),
                    &latest.altitude_m,
                    &latest.ground_speed_ms,
                    &latest.climb_ms,
                    &latest.course_deg,
                ],
            )
            .await
            .map_err(db_error)?;
        insert_fixes(&tx, row.get(0), &merged.stored).await?;
        tx.commit().await.map_err(db_error)?;
        return Ok(LiveSaved::Kept(Box::new(LiveKept {
            track: held.track(&batch.id, &batch.account_id, handle, name),
            created: true,
            stored: stored_count(&merged.stored),
            known: merged.known,
            full: merged.full,
        })));
    };

    // A track that ended too long ago is gone, whether or not the cleanup
    // has come by.
    if stored.held.latest.time < room.expired_before {
        retire(&tx, stored.key, room.now).await?;
        tx.commit().await.map_err(db_error)?;
        return Ok(LiveSaved::Gone);
    }
    if stored.account_id != batch.account_id {
        return Ok(LiveSaved::Taken);
    }
    // Asked now, with the track's row locked, and not together with the
    // track: a suspension that narrowed this track a moment ago has
    // finished, and this reads what it left. One that comes after waits for
    // the row and narrows what this batch writes.
    let suspended: bool = tx
        .query_one(
            "SELECT publishing_suspended_at IS NOT NULL FROM accounts WHERE id = $1",
            &[&batch.account_id],
        )
        .await
        .map_err(db_error)?
        .get(0);
    let narrowed;
    let batch = if suspended {
        narrowed = batch.for_those_with_the_link();
        &narrowed
    } else {
        batch
    };

    let times: Vec<Timestamp> = batch.fixes.iter().map(|fix| fix.time).collect();
    let statement = tx
        .prepare_cached("SELECT time FROM live_fixes WHERE track = $1 AND time = ANY($2)")
        .await
        .map_err(db_error)?;
    let rows = tx
        .query(&statement, &[&stored.key, &times])
        .await
        .map_err(db_error)?;
    let known: BTreeSet<Timestamp> = rows.iter().map(|row| row.get(0)).collect();

    let merged = merge(Some(&stored.held), batch, &known, room);
    // A batch that changes nothing writes nothing.
    if merged.changed {
        insert_fixes(&tx, stored.key, &merged.stored).await?;
        let held = &merged.held;
        let latest = &held.latest;
        let statement = tx
            .prepare_cached(
                "UPDATE live_tracks
                 SET visibility = $2, landed = $3, started_at = $4, fixes = $5, last_seq = $6,
                     last_at = $7, last_lat = $8, last_lon = $9, last_altitude_m = $10,
                     last_ground_speed_ms = $11, last_climb_ms = $12, last_course_deg = $13
                 WHERE key = $1",
            )
            .await
            .map_err(db_error)?;
        tx.execute(
            &statement,
            &[
                &stored.key,
                &held.visibility.as_str(),
                &held.landed,
                &held.started_at,
                &count(held.fixes),
                &held.last_seq,
                &latest.time,
                &latest.position.lat(),
                &latest.position.lon(),
                &latest.altitude_m,
                &latest.ground_speed_ms,
                &latest.climb_ms,
                &latest.course_deg,
            ],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(LiveSaved::Kept(Box::new(LiveKept {
        track: Stored {
            held: merged.held,
            ..stored
        }
        .track(),
        created: false,
        stored: stored_count(&merged.stored),
        known: merged.known,
        full: merged.full,
    })))
}

pub(super) async fn get(client: &Client, id: &str) -> Result<Option<LiveTrack>, StoreError> {
    // PostgreSQL takes no zero byte in text. No identifier holds one, so
    // the answer is known.
    if id.contains('\0') {
        return Ok(None);
    }
    let query = format!("SELECT {TRACK_COLUMNS} {TRACK_FROM} WHERE t.id = $1");
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&id])
        .await
        .map_err(db_error)?;
    Ok(row
        .as_ref()
        .map(stored_from)
        .transpose()?
        .map(|stored| stored.track()))
}

pub(super) async fn fixes(
    client: &Client,
    id: &str,
    since: i64,
    after: Option<Timestamp>,
    limit: usize,
) -> Result<Vec<LiveFixAt>, StoreError> {
    if id.contains('\0') {
        return Ok(Vec::new());
    }
    // One walk along the index on a track's numbers, from the number
    // asked for, that stops at the limit. The track is found first, by
    // itself: joined to it, the fixes would all be read and then sorted,
    // because the planner does not see that one track's fixes come out of
    // the index in order.
    let query = format!(
        "SELECT {FIX_COLUMNS}
         FROM live_fixes f
         WHERE f.track = (SELECT key FROM live_tracks WHERE id = $1)
           AND f.seq > $2 AND ($3::timestamptz IS NULL OR f.time > $3)
         ORDER BY f.seq
         LIMIT $4"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(&statement, &[&id, &since, &after, &limit])
        .await
        .map_err(db_error)?;
    rows.iter().map(fix_from).collect()
}

pub(super) async fn tail(
    client: &Client,
    id: &str,
    limit: usize,
) -> Result<Vec<LiveFix>, StoreError> {
    if id.contains('\0') {
        return Ok(Vec::new());
    }
    // Backwards along the track's fixes by time, as far as the limit.
    let query = format!(
        "SELECT {FIX_COLUMNS}
         FROM live_fixes f
         WHERE f.track = (SELECT key FROM live_tracks WHERE id = $1)
         ORDER BY f.time DESC
         LIMIT $2"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(&statement, &[&id, &limit])
        .await
        .map_err(db_error)?;
    let mut fixes: Vec<LiveFix> = rows
        .iter()
        .map(|row| fix_from(row).map(|at| at.fix))
        .collect::<Result<_, _>>()?;
    fixes.reverse();
    Ok(fixes)
}

pub(super) async fn in_box(
    client: &Client,
    area: &BoundingBox,
    newest_since: Timestamp,
    limit: usize,
) -> Result<Vec<LiveTrack>, StoreError> {
    // The condition on visibility is spelled as in the partial index, which
    // holds the public tracks by the time of their newest fix.
    let query = format!(
        "SELECT {TRACK_COLUMNS} {TRACK_FROM}
         WHERE t.visibility = 'public' AND t.last_at >= $1
           AND t.last_lon BETWEEN $2 AND $4
           AND t.last_lat BETWEEN $3 AND $5
         ORDER BY t.id
         LIMIT $6"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(
            &statement,
            &[
                &newest_since,
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
        .map(|row| stored_from(row).map(|stored| stored.track()))
        .collect()
}

pub(super) async fn delete(
    client: &mut Client,
    account_id: &str,
    id: &str,
    now: Timestamp,
    may_retire: bool,
) -> Result<LiveRemoval, StoreError> {
    if id.contains('\0') || account_id.contains('\0') {
        return Err(StoreError::Invalid(
            "a live track's identifier that cannot be one".to_owned(),
        ));
    }
    let tx = client.transaction().await.map_err(db_error)?;
    lock_identifier(&tx, id).await?;
    let track = tx
        .query_opt(
            "SELECT key, account_id FROM live_tracks WHERE id = $1 FOR UPDATE",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    if let Some(track) = track {
        if track.get::<_, &str>("account_id") != account_id {
            return Ok(LiveRemoval::NotYours);
        }
        retire(&tx, track.get("key"), now).await?;
        tx.commit().await.map_err(db_error)?;
        return Ok(LiveRemoval::Removed);
    }
    if let Some(used_by) = retired_by(&tx, id).await? {
        return Ok(if used_by == account_id {
            LiveRemoval::AlreadyRemoved
        } else {
            LiveRemoval::NotYours
        });
    }
    if !may_retire {
        return Ok(LiveRemoval::NotRetired);
    }
    let retired = tx
        .execute(
            "INSERT INTO live_retired (id, account_id, retired_at)
             SELECT $1, a.id, $3 FROM accounts a WHERE a.id = $2",
            &[&id, &account_id, &now],
        )
        .await
        .map_err(db_error)?;
    if retired == 0 {
        return Err(StoreError::Invalid(
            "a live track's identifier taken out of use for no account".to_owned(),
        ));
    }
    tx.commit().await.map_err(db_error)?;
    Ok(LiveRemoval::Retired)
}

pub(super) async fn remove(
    client: &mut Client,
    id: &str,
    now: Timestamp,
) -> Result<bool, StoreError> {
    // PostgreSQL takes no zero byte in text. No identifier holds one, so
    // the answer is known.
    if id.contains('\0') {
        return Ok(false);
    }
    let tx = client.transaction().await.map_err(db_error)?;
    lock_identifier(&tx, id).await?;
    let track = tx
        .query_opt(
            "SELECT key FROM live_tracks WHERE id = $1 FOR UPDATE",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    let Some(track) = track else {
        return Ok(false);
    };
    retire(&tx, track.get(0), now).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn prune(
    client: &Client,
    ended_before: Timestamp,
    now: Timestamp,
) -> Result<u64, StoreError> {
    // A track that takes in a newer fix at this moment holds its row
    // locked; the deletion waits, looks at the row again and leaves it.
    client
        .execute(
            "WITH gone AS (
                 DELETE FROM live_tracks WHERE last_at < $1 RETURNING id, account_id
             )
             INSERT INTO live_retired (id, account_id, retired_at)
             SELECT id, account_id, $2 FROM gone
             ON CONFLICT (id) DO NOTHING",
            &[&ended_before, &now],
        )
        .await
        .map_err(db_error)
}
