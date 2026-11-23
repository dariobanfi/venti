//! Phones, the settings for what pilots nearby do, positions and requests
//! for help, in PostgreSQL.
//!
//! **The account's row first.** Registering a phone and asking for help
//! lock the account's row, so that the phones it has and the requests that
//! stand can be counted and two calls cannot share its last place. The
//! database holds the rest: everything goes with its account, a phone with
//! its session (`0032_phones_and_help.sql`).

use deadpool_postgres::{Client, GenericClient};
use tokio_postgres::Row;
use venti_domain::{GeoPoint, Timestamp};

use super::{corrupt, db_error};
use crate::{
    AddingDevice, Circle, GroupPilot, HelpAnswering, HelpCreation, HelpReach, HelpRequest,
    NearbySettings, NewHelpRequest, PhoneDevice, StoreError,
};

/// Kilometres in a degree of latitude, a little less than it is, so that
/// a box drawn with it holds its circle.
const KM_PER_DEGREE: f64 = 110.0;

/// The most rows one look at a box reads before the distance is measured.
const MOST_IN_A_BOX: i64 = 5000;

fn device(row: &Row) -> PhoneDevice {
    PhoneDevice {
        account_id: row.get("account_id"),
        session_id: row.get("session_id"),
        platform: row.get("platform"),
        token: row.get("token"),
        sandbox: row.get("sandbox"),
        created_at: row.get("created_at"),
    }
}

fn point(lat: f64, lon: f64, what: &str) -> Result<GeoPoint, StoreError> {
    GeoPoint::new(lat, lon).map_err(|error| corrupt(what, error))
}

/// The latitudes and longitudes of a box that holds the circle around
/// `middle`.
fn bounds(middle: GeoPoint, radius_km: f64) -> (f64, f64, f64, f64) {
    let lat_span = radius_km / KM_PER_DEGREE;
    let cos = middle.lat().to_radians().cos().max(0.01);
    let lon_span = (radius_km / (KM_PER_DEGREE * cos)).min(180.0);
    (
        middle.lat() - lat_span,
        middle.lat() + lat_span,
        middle.lon() - lon_span,
        middle.lon() + lon_span,
    )
}

pub(super) async fn add_device(
    client: &mut Client,
    new: &PhoneDevice,
    most: usize,
) -> Result<AddingDevice, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&new.account_id],
        )
        .await
        .map_err(db_error)?;
    let session = tx
        .query_opt(
            "SELECT 1 FROM sessions WHERE id = $1 AND account_id = $2",
            &[&new.session_id, &new.account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() || session.is_none() {
        return Ok(AddingDevice::NoSession);
    }
    let others: i64 = tx
        .query_one(
            "SELECT count(*) FROM phone_devices
             WHERE account_id = $1 AND NOT (platform = $2 AND token = $3)",
            &[&new.account_id, &new.platform, &new.token],
        )
        .await
        .map_err(db_error)?
        .get(0);
    if usize::try_from(others).unwrap_or(usize::MAX) >= most {
        return Ok(AddingDevice::Full);
    }
    tx.execute(
        "INSERT INTO phone_devices (platform, token, account_id, session_id, sandbox, created_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (platform, token) DO UPDATE
         SET account_id = EXCLUDED.account_id, session_id = EXCLUDED.session_id,
             sandbox = EXCLUDED.sandbox, created_at = EXCLUDED.created_at",
        &[
            &new.platform,
            &new.token,
            &new.account_id,
            &new.session_id,
            &new.sandbox,
            &new.created_at,
        ],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(AddingDevice::Added)
}

pub(super) async fn remove_device(
    client: &Client,
    account_id: &str,
    platform: &str,
    token: &str,
) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM phone_devices WHERE account_id = $1 AND platform = $2 AND token = $3",
            &[&account_id, &platform, &token],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn drop_device(
    client: &Client,
    platform: &str,
    token: &str,
) -> Result<bool, StoreError> {
    let removed = client
        .execute(
            "DELETE FROM phone_devices WHERE platform = $1 AND token = $2",
            &[&platform, &token],
        )
        .await
        .map_err(db_error)?;
    Ok(removed > 0)
}

pub(super) async fn devices_of(
    client: &Client,
    account_ids: &[String],
) -> Result<Vec<PhoneDevice>, StoreError> {
    let rows = client
        .query(
            "SELECT platform, token, account_id, session_id, sandbox, created_at
             FROM phone_devices WHERE account_id = ANY($1)
             ORDER BY account_id, created_at, platform, token",
            &[&account_ids],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.iter().map(device).collect())
}

fn settings_from(row: &Row) -> Result<NearbySettings, StoreError> {
    let lat: Option<f64> = row.get("plans_lat");
    let lon: Option<f64> = row.get("plans_lon");
    let radius: Option<i32> = row.get("plans_radius_km");
    let open_plans_near = match (lat, lon, radius) {
        (Some(lat), Some(lon), Some(radius)) => Some(Circle {
            position: point(lat, lon, "circle for open plans")?,
            radius_km: u32::try_from(radius).map_err(|error| corrupt("radius", error))?,
        }),
        _ => None,
    };
    Ok(NearbySettings {
        open_plans_near,
        may_be_asked_for_help: row.get("may_be_asked"),
    })
}

pub(super) async fn settings(
    client: &Client,
    account_id: &str,
) -> Result<NearbySettings, StoreError> {
    let row = client
        .query_opt(
            "SELECT plans_lat, plans_lon, plans_radius_km, may_be_asked
             FROM nearby_settings WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    row.as_ref()
        .map_or_else(|| Ok(NearbySettings::default()), settings_from)
}

pub(super) async fn set_settings(
    client: &mut Client,
    account_id: &str,
    settings: &NearbySettings,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(false);
    }
    let circle = settings.open_plans_near;
    let radius = circle.map(|circle| i32::try_from(circle.radius_km).unwrap_or(i32::MAX));
    tx.execute(
        "INSERT INTO nearby_settings
             (account_id, plans_lat, plans_lon, plans_radius_km, may_be_asked, changed_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (account_id) DO UPDATE
         SET plans_lat = EXCLUDED.plans_lat, plans_lon = EXCLUDED.plans_lon,
             plans_radius_km = EXCLUDED.plans_radius_km, may_be_asked = EXCLUDED.may_be_asked,
             changed_at = EXCLUDED.changed_at",
        &[
            &account_id,
            &circle.map(|circle| circle.position.lat()),
            &circle.map(|circle| circle.position.lon()),
            &radius,
            &settings.may_be_asked_for_help,
            &now,
        ],
    )
    .await
    .map_err(db_error)?;
    if !settings.may_be_asked_for_help {
        tx.execute(
            "DELETE FROM help_positions WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn keep_position(
    client: &Client,
    account_id: &str,
    position: GeoPoint,
    now: Timestamp,
) -> Result<bool, StoreError> {
    // Kept only where the setting is on, in one statement: a setting
    // turned off at the same moment wins.
    let kept = client
        .execute(
            "INSERT INTO help_positions (account_id, lat, lon, told_at)
             SELECT account_id, $2, $3, $4 FROM nearby_settings
             WHERE account_id = $1 AND may_be_asked
             ON CONFLICT (account_id) DO UPDATE
             SET lat = EXCLUDED.lat, lon = EXCLUDED.lon, told_at = EXCLUDED.told_at",
            &[&account_id, &position.lat(), &position.lon(), &now],
        )
        .await
        .map_err(db_error)?;
    Ok(kept > 0)
}

pub(super) async fn forget_position(client: &Client, account_id: &str) -> Result<(), StoreError> {
    client
        .execute(
            "DELETE FROM help_positions WHERE account_id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(())
}

pub(super) async fn position_of(
    client: &Client,
    account_id: &str,
    since: Timestamp,
) -> Result<Option<GeoPoint>, StoreError> {
    let row = client
        .query_opt(
            "SELECT lat, lon FROM help_positions WHERE account_id = $1 AND told_at >= $2",
            &[&account_id, &since],
        )
        .await
        .map_err(db_error)?;
    row.map(|row| point(row.get(0), row.get(1), "position"))
        .transpose()
}

pub(super) async fn open_plan_watchers(
    client: &Client,
    position: GeoPoint,
    except: &str,
    most: usize,
) -> Result<Vec<String>, StoreError> {
    // Every circle that can hold the plan has its middle within the
    // largest radius of it.
    let (south, north, _, _) = bounds(position, 100.0);
    let rows = client
        .query(
            "SELECT account_id, plans_lat, plans_lon, plans_radius_km, may_be_asked
             FROM nearby_settings
             WHERE plans_lat BETWEEN $1 AND $2 AND account_id <> $3
             ORDER BY account_id LIMIT $4",
            &[&south, &north, &except, &MOST_IN_A_BOX],
        )
        .await
        .map_err(db_error)?;
    let mut watchers = Vec::new();
    for row in &rows {
        let holds = settings_from(row)?
            .open_plans_near
            .is_some_and(|circle| circle.holds(position));
        if holds {
            watchers.push(row.get("account_id"));
        }
        if watchers.len() == most {
            break;
        }
    }
    Ok(watchers)
}

/// Everything [`request_from`] reads: a request `r` and its pilot's
/// account `a`.
const REQUEST: &str = "SELECT r.id, r.account_id, a.handle, a.display_name, r.lat, r.lon,
        r.note, r.created_at, r.ends_at, r.closed_at
    FROM help_requests r JOIN accounts a ON a.id = r.account_id";

async fn request_from(db: &impl GenericClient, row: &Row) -> Result<HelpRequest, StoreError> {
    let id: String = row.get("id");
    let recipients = db
        .query(
            "SELECT h.account_id, h.can_help_at, a.handle, a.display_name
             FROM help_recipients h JOIN accounts a ON a.id = h.account_id
             WHERE h.request_id = $1 ORDER BY h.rank",
            &[&id],
        )
        .await
        .map_err(db_error)?;
    let asked = recipients.iter().map(|row| row.get(0)).collect();
    let mut helpers: Vec<(Timestamp, GroupPilot)> = recipients
        .iter()
        .filter_map(|row| {
            let since: Option<Timestamp> = row.get(1);
            since.map(|since| {
                (
                    since,
                    GroupPilot {
                        account_id: row.get(0),
                        handle: row.get(2),
                        display_name: row.get(3),
                    },
                )
            })
        })
        .collect();
    helpers.sort_by(|a, b| (a.0, &a.1.account_id).cmp(&(b.0, &b.1.account_id)));
    Ok(HelpRequest {
        pilot: GroupPilot {
            account_id: row.get("account_id"),
            handle: row.get("handle"),
            display_name: row.get("display_name"),
        },
        position: point(row.get("lat"), row.get("lon"), "help request position")?,
        note: row.get("note"),
        created_at: row.get("created_at"),
        ends_at: row.get("ends_at"),
        closed_at: row.get("closed_at"),
        asked,
        helpers: helpers.into_iter().map(|(_, pilot)| pilot).collect(),
        id,
    })
}

async fn read(db: &impl GenericClient, id: &str) -> Result<Option<HelpRequest>, StoreError> {
    let row = db
        .query_opt(&format!("{REQUEST} WHERE r.id = $1"), &[&id])
        .await
        .map_err(db_error)?;
    match row {
        Some(row) => Ok(Some(request_from(db, &row).await?)),
        None => Ok(None),
    }
}

pub(super) async fn create_help_request(
    client: &mut Client,
    new: &NewHelpRequest,
    reach: &HelpReach,
) -> Result<HelpCreation, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let account = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&new.account_id],
        )
        .await
        .map_err(db_error)?;
    if account.is_none() {
        return Ok(HelpCreation::NoAccount);
    }
    let open = tx
        .query_opt(
            "SELECT 1 FROM help_requests
             WHERE account_id = $1 AND closed_at IS NULL AND ends_at > $2 LIMIT 1",
            &[&new.account_id, &new.created_at],
        )
        .await
        .map_err(db_error)?;
    if open.is_some() {
        return Ok(HelpCreation::StillOpen);
    }
    let radius_km = f64::from(reach.radius_km);
    let (south, north, west, east) = bounds(new.position, radius_km);
    let rows = tx
        .query(
            "SELECT p.account_id, p.lat, p.lon FROM help_positions p
             JOIN nearby_settings s ON s.account_id = p.account_id AND s.may_be_asked
             WHERE p.lat BETWEEN $1 AND $2 AND p.lon BETWEEN $3 AND $4
               AND p.told_at >= $5 AND p.account_id <> $6
             ORDER BY p.account_id LIMIT $7",
            &[
                &south,
                &north,
                &west,
                &east,
                &reach.positions_since,
                &new.account_id,
                &MOST_IN_A_BOX,
            ],
        )
        .await
        .map_err(db_error)?;
    let radius_m = radius_km * 1000.0;
    let mut near = Vec::new();
    for row in &rows {
        let at = point(row.get(1), row.get(2), "position")?;
        let distance = at.distance_m(new.position);
        if distance <= radius_m {
            near.push((distance, row.get::<_, String>(0)));
        }
    }
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    near.truncate(reach.most);
    tx.execute(
        "INSERT INTO help_requests (id, account_id, lat, lon, note, created_at, ends_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &new.id,
            &new.account_id,
            &new.position.lat(),
            &new.position.lon(),
            &new.note,
            &new.created_at,
            &new.ends_at,
        ],
    )
    .await
    .map_err(db_error)?;
    for (rank, (_, account)) in near.iter().enumerate() {
        let rank = i32::try_from(rank).unwrap_or(i32::MAX);
        tx.execute(
            "INSERT INTO help_recipients (request_id, account_id, rank) VALUES ($1, $2, $3)",
            &[&new.id, account, &rank],
        )
        .await
        .map_err(db_error)?;
    }
    let request = read(&tx, &new.id)
        .await?
        .ok_or_else(|| corrupt("help request", "not there after it was stored"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(HelpCreation::Created(Box::new(request)))
}

pub(super) async fn help_request(
    client: &Client,
    id: &str,
) -> Result<Option<HelpRequest>, StoreError> {
    read(client, id).await
}

pub(super) async fn answer(
    client: &mut Client,
    id: &str,
    account_id: &str,
    can_help: bool,
    now: Timestamp,
) -> Result<HelpAnswering, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let held = tx
        .query_opt(
            "SELECT h.can_help_at FROM help_recipients h
             JOIN help_requests r ON r.id = h.request_id
             WHERE h.request_id = $1 AND h.account_id = $2
               AND r.closed_at IS NULL AND r.ends_at > $3
             FOR UPDATE OF h",
            &[&id, &account_id, &now],
        )
        .await
        .map_err(db_error)?;
    let Some(held) = held else {
        return Ok(HelpAnswering::NotAsked);
    };
    let had = held.get::<_, Option<Timestamp>>(0).is_some();
    let changed = had != can_help;
    if changed {
        let since = can_help.then_some(now);
        tx.execute(
            "UPDATE help_recipients SET can_help_at = $3
             WHERE request_id = $1 AND account_id = $2",
            &[&id, &account_id, &since],
        )
        .await
        .map_err(db_error)?;
    }
    let request = read(&tx, id)
        .await?
        .ok_or_else(|| corrupt("help request", "gone while answered"))?;
    tx.commit().await.map_err(db_error)?;
    Ok(HelpAnswering::Answered(Box::new(request), changed))
}

pub(super) async fn close(
    client: &Client,
    id: &str,
    account_id: &str,
    now: Timestamp,
) -> Result<Option<HelpRequest>, StoreError> {
    let closed = client
        .execute(
            "UPDATE help_requests SET closed_at = $3
             WHERE id = $1 AND account_id = $2 AND closed_at IS NULL AND ends_at > $3",
            &[&id, &account_id, &now],
        )
        .await
        .map_err(db_error)?;
    if closed == 0 {
        return Ok(None);
    }
    read(client, id).await
}

async fn many(client: &Client, rows: &[Row]) -> Result<Vec<HelpRequest>, StoreError> {
    let mut requests = Vec::with_capacity(rows.len());
    for row in rows {
        requests.push(request_from(client, row).await?);
    }
    Ok(requests)
}

pub(super) async fn requests_of(
    client: &Client,
    account_id: &str,
    now: Timestamp,
) -> Result<Vec<HelpRequest>, StoreError> {
    let rows = client
        .query(
            &format!(
                "{REQUEST} WHERE r.account_id = $1 AND r.closed_at IS NULL AND r.ends_at > $2
                 ORDER BY r.created_at DESC, r.id DESC"
            ),
            &[&account_id, &now],
        )
        .await
        .map_err(db_error)?;
    many(client, &rows).await
}

pub(super) async fn requests_asked(
    client: &Client,
    account_id: &str,
    now: Timestamp,
) -> Result<Vec<HelpRequest>, StoreError> {
    let rows = client
        .query(
            &format!(
                "{REQUEST} JOIN help_recipients h ON h.request_id = r.id AND h.account_id = $1
                 WHERE r.closed_at IS NULL AND r.ends_at > $2
                 ORDER BY r.created_at DESC, r.id DESC"
            ),
            &[&account_id, &now],
        )
        .await
        .map_err(db_error)?;
    many(client, &rows).await
}

pub(super) async fn prune(
    client: &Client,
    positions_before: Timestamp,
    requests_before: Timestamp,
) -> Result<u64, StoreError> {
    let positions = client
        .execute(
            "DELETE FROM help_positions WHERE told_at < $1",
            &[&positions_before],
        )
        .await
        .map_err(db_error)?;
    let requests = client
        .execute(
            "DELETE FROM help_requests WHERE LEAST(COALESCE(closed_at, ends_at), ends_at) < $1",
            &[&requests_before],
        )
        .await
        .map_err(db_error)?;
    Ok(positions + requests)
}
