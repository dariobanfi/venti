//! Flights in PostgreSQL.
//!
//! Every change to a flight first locks its account's row and takes the
//! account's next change number in the same transaction. Changes to one
//! account therefore happen one after another, and their numbers are in
//! the order in which they become visible, which is what lets a device ask
//! for "everything after number n" without ever missing a change.

use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::types::ToSql;
use tokio_postgres::{IsolationLevel, Row};
use venti_domain::{GeoPoint, Timestamp};

use super::{corrupt, db_error};
use crate::flight::{Kept, Room, Saved, cut, may_be_set, next_change};
use crate::{
    DeletedFlight, Derived, Flight, FlightChanges, FlightKey, Listing, NewFlight, OutdatedFlight,
    Place, PublicFlights, PublicOrder, Score, ScoreKind, StoreError, Visibility, VisibilityChange,
};

/// Everything [`flight_from`] reads: a flight `f` and its account `a`.
const FLIGHT_COLUMNS: &str = "
    f.id, f.account_id, a.handle, a.display_name, f.identity, f.visibility, f.uploaded_at,
    f.change, f.file_bytes, f.analysis_version, f.started_at, f.airtime_s,
    f.takeoff_at, f.takeoff_lat, f.takeoff_lon, f.takeoff_altitude_m,
    f.landing_at, f.landing_lat, f.landing_lon, f.landing_altitude_m,
    f.track_length_m, f.max_altitude_m, f.max_climb_ms,
    f.score_kind, f.score_closed, f.score_distance_m, f.score_points,
    f.glider, f.outline, f.profile, f.taken_down_at";

const FLIGHT_FROM: &str = "FROM flights f JOIN accounts a ON a.id = f.account_id";

fn place_from(row: &Row, end: &str) -> Result<Option<Place>, StoreError> {
    let column = |name: &str| format!("{end}_{name}");
    let Some(time) = row.get::<_, Option<Timestamp>>(column("at").as_str()) else {
        return Ok(None);
    };
    let missing = || corrupt("flight", format!("its {end} has a time and no place"));
    let lat: f64 = row
        .get::<_, Option<f64>>(column("lat").as_str())
        .ok_or_else(missing)?;
    let lon: f64 = row
        .get::<_, Option<f64>>(column("lon").as_str())
        .ok_or_else(missing)?;
    let altitude_m: i32 = row
        .get::<_, Option<i32>>(column("altitude_m").as_str())
        .ok_or_else(missing)?;
    Ok(Some(Place {
        time,
        position: GeoPoint::new(lat, lon).map_err(|e| corrupt("flight position", e))?,
        altitude_m,
    }))
}

fn score_from(row: &Row) -> Result<Option<Score>, StoreError> {
    let Some(kind) = row.get::<_, Option<&str>>("score_kind") else {
        return Ok(None);
    };
    let missing = || corrupt("flight", "its score has a kind and no numbers");
    Ok(Some(Score {
        kind: ScoreKind::from_name(kind).ok_or_else(|| corrupt("score kind", kind))?,
        closed: row
            .get::<_, Option<bool>>("score_closed")
            .ok_or_else(missing)?,
        distance_m: row
            .get::<_, Option<f64>>("score_distance_m")
            .ok_or_else(missing)?,
        points: row
            .get::<_, Option<f64>>("score_points")
            .ok_or_else(missing)?,
    }))
}

/// Splits a flat list into pairs.
fn pairs<T: Copy>(flat: &[T], what: &str) -> Result<Vec<[T; 2]>, StoreError> {
    if !flat.len().is_multiple_of(2) {
        return Err(corrupt(what, "an odd number of values"));
    }
    Ok(flat
        .chunks_exact(2)
        .map(|pair| [pair[0], pair[1]])
        .collect())
}

fn flight_from(row: &Row) -> Result<Flight, StoreError> {
    let visibility: &str = row.get("visibility");
    let number = |name: &str| {
        u32::try_from(row.get::<_, i32>(name)).map_err(|_| corrupt(name, "a negative number"))
    };
    let outline: Vec<f64> = row.get("outline");
    let profile: Vec<i32> = row.get("profile");
    Ok(Flight {
        id: row.get("id"),
        account_id: row.get("account_id"),
        pilot_handle: row.get("handle"),
        pilot_name: row.get("display_name"),
        identity: row.get("identity"),
        visibility: Visibility::from_name(visibility)
            .ok_or_else(|| corrupt("visibility", visibility))?,
        uploaded_at: row.get("uploaded_at"),
        file_bytes: number("file_bytes")?,
        derived: Derived {
            version: number("analysis_version")?,
            start: row.get("started_at"),
            airtime_s: row.get("airtime_s"),
            takeoff: place_from(row, "takeoff")?,
            landing: place_from(row, "landing")?,
            track_length_m: row.get("track_length_m"),
            max_altitude_m: row.get("max_altitude_m"),
            max_climb_ms: row.get("max_climb_ms"),
            score: score_from(row)?,
            glider: row.get("glider"),
            outline: pairs(&outline, "flight outline")?,
            profile: pairs(&profile, "flight profile")?,
        },
        change: row.get("change"),
        taken_down_at: row.get("taken_down_at"),
    })
}

/// The derived values that need converting before they fit their columns.
struct DerivedValues {
    version: i32,
    takeoff: PlaceValues,
    landing: PlaceValues,
    score_kind: Option<&'static str>,
    outline: Vec<f64>,
    profile: Vec<i32>,
}

/// The columns that hold what is worked out from the file.
const DERIVED_COLUMNS: [&str; 20] = [
    "analysis_version",
    "started_at",
    "airtime_s",
    "takeoff_at",
    "takeoff_lat",
    "takeoff_lon",
    "takeoff_altitude_m",
    "landing_at",
    "landing_lat",
    "landing_lon",
    "landing_altitude_m",
    "track_length_m",
    "max_altitude_m",
    "max_climb_ms",
    "score_kind",
    "score_closed",
    "score_distance_m",
    "score_points",
    "glider",
    "outline",
];

impl DerivedValues {
    fn of(derived: &Derived) -> Self {
        Self {
            version: i32::try_from(derived.version).unwrap_or(i32::MAX),
            takeoff: PlaceValues::of(derived.takeoff),
            landing: PlaceValues::of(derived.landing),
            score_kind: derived.score.map(|score| score.kind.as_str()),
            outline: derived.outline.iter().flatten().copied().collect(),
            profile: derived.profile.iter().flatten().copied().collect(),
        }
    }
}

/// What a place puts in its four columns.
struct PlaceValues {
    at: Option<Timestamp>,
    lat: Option<f64>,
    lon: Option<f64>,
    altitude_m: Option<i32>,
}

impl PlaceValues {
    fn of(place: Option<Place>) -> Self {
        Self {
            at: place.map(|place| place.time),
            lat: place.map(|place| place.position.lat()),
            lon: place.map(|place| place.position.lon()),
            altitude_m: place.map(|place| place.altitude_m),
        }
    }
}

/// What an account's locked row says.
struct Locked {
    /// The latest change number of its flights.
    latest: i64,
    /// Whether its publishing is suspended.
    publishing_suspended: bool,
}

/// Locks an account's row for the rest of the transaction and returns what
/// it says. `None` if there is no such account.
async fn lock_account(
    tx: &Transaction<'_>,
    account_id: &str,
) -> Result<Option<Locked>, StoreError> {
    let row = tx
        .query_opt(
            "SELECT flight_change, publishing_suspended_at IS NOT NULL
             FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| Locked {
        latest: row.get(0),
        publishing_suspended: row.get(1),
    }))
}

/// Takes the next change number of an account whose row is locked.
async fn take_change(
    tx: &Transaction<'_>,
    account_id: &str,
    latest: i64,
    now: Timestamp,
) -> Result<i64, StoreError> {
    let change = next_change(latest, now);
    tx.execute(
        "UPDATE accounts SET flight_change = $2 WHERE id = $1",
        &[&account_id, &change],
    )
    .await
    .map_err(db_error)?;
    Ok(change)
}

async fn one(
    client: &impl GenericClient,
    condition: &str,
    parameters: &[&(dyn ToSql + Sync)],
) -> Result<Option<Flight>, StoreError> {
    let query = format!("SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM} WHERE {condition}");
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let row = client
        .query_opt(&statement, parameters)
        .await
        .map_err(db_error)?;
    row.as_ref().map(flight_from).transpose()
}

pub(super) async fn save(
    client: &mut Client,
    new: &NewFlight,
    room: &Room,
) -> Result<Saved, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(Locked { latest, .. }) = lock_account(&tx, &new.account_id).await? else {
        return Err(StoreError::Invalid(
            "a flight for an account that does not exist".to_owned(),
        ));
    };
    // With the account locked, no other upload of the same flight can be
    // under way: either it is there now or this call creates it.
    let existing = one(
        &tx,
        "f.account_id = $1 AND f.identity = $2",
        &[&new.account_id, &new.identity],
    )
    .await?;
    if let Some(flight) = existing {
        return Ok(Saved::AlreadyThere(flight));
    }
    // Still under the account's lock: what it keeps cannot grow between
    // this count and the insert below. The count reads the account's own
    // rows through an index and no file.
    let row = tx
        .query_one(
            "SELECT count(*), coalesce(sum(file_bytes), 0)::bigint
             FROM flights WHERE account_id = $1",
            &[&new.account_id],
        )
        .await
        .map_err(db_error)?;
    let kept = Kept {
        flights: u64::try_from(row.get::<_, i64>(0)).unwrap_or(0),
        file_bytes: u64::try_from(row.get::<_, i64>(1)).unwrap_or(0),
    };
    if !room.has_room_for(&kept, new.file_bytes) {
        return Ok(Saved::NoRoom(kept));
    }

    let change = take_change(&tx, &new.account_id, latest, new.uploaded_at).await?;
    let derived = &new.derived;
    let values = DerivedValues::of(derived);
    let (takeoff, landing) = (&values.takeoff, &values.landing);
    let file_bytes = i32::try_from(new.file_bytes).unwrap_or(i32::MAX);
    let visibility = Visibility::Private.as_str();
    let insert = format!(
        "INSERT INTO flights
             (id, account_id, identity, visibility, uploaded_at, change, file, file_bytes,
              {}, profile)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, {}, $29)",
        DERIVED_COLUMNS.join(", "),
        (9..29)
            .map(|n| format!("${n}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let statement = tx.prepare_cached(&insert).await.map_err(db_error)?;
    tx.execute(
        &statement,
        &[
            &new.id,
            &new.account_id,
            &new.identity,
            &visibility,
            &new.uploaded_at,
            &change,
            &new.file,
            &file_bytes,
            &values.version,
            &derived.start,
            &derived.airtime_s,
            &takeoff.at,
            &takeoff.lat,
            &takeoff.lon,
            &takeoff.altitude_m,
            &landing.at,
            &landing.lat,
            &landing.lon,
            &landing.altitude_m,
            &derived.track_length_m,
            &derived.max_altitude_m,
            &derived.max_climb_ms,
            &values.score_kind,
            &derived.score.map(|score| score.closed),
            &derived.score.map(|score| score.distance_m),
            &derived.score.map(|score| score.points),
            &derived.glider,
            &values.outline,
            &values.profile,
        ],
    )
    .await
    .map_err(db_error)?;
    let deletion = tx
        .query_opt(
            "DELETE FROM flight_deletions WHERE account_id = $1 AND flight_id = $2
             RETURNING taken_down_at",
            &[&new.account_id, &new.id],
        )
        .await
        .map_err(db_error)?;
    // Deleting a flight and uploading it again does not undo that it was
    // taken down.
    let taken_down_at: Option<Timestamp> = deletion.and_then(|row| row.get(0));
    if taken_down_at.is_some() {
        tx.execute(
            "UPDATE flights SET taken_down_at = $2 WHERE id = $1",
            &[&new.id, &taken_down_at],
        )
        .await
        .map_err(db_error)?;
    }

    let flight = one(&tx, "f.id = $1", &[&new.id])
        .await?
        .ok_or_else(|| StoreError::Failure("a flight vanished while it was stored".to_owned()))?;
    tx.commit().await.map_err(db_error)?;
    Ok(Saved::Created(flight))
}

pub(super) async fn get(client: &Client, id: &str) -> Result<Option<Flight>, StoreError> {
    one(client, "f.id = $1", &[&id]).await
}

pub(super) async fn file(client: &Client, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let statement = client
        .prepare_cached("SELECT file FROM flights WHERE id = $1")
        .await
        .map_err(db_error)?;
    let row = client
        .query_opt(&statement, &[&id])
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| row.get(0)))
}

pub(super) async fn of_account(
    client: &Client,
    account_id: &str,
    listing: Listing,
    after: Option<&FlightKey>,
    limit: usize,
) -> Result<Vec<Flight>, StoreError> {
    // The condition on visibility is spelled as in the partial index, so
    // the planner can use that index for the public list.
    let only = match listing {
        Listing::All => "",
        Listing::Public => "AND f.visibility = 'public'",
    };
    // Without a flight to continue after, the page starts at the newest.
    let page = match after {
        Some(_) => "AND (f.started_at, f.id) < ($3, $4)",
        None => "",
    };
    let query = format!(
        "SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM}
         WHERE f.account_id = $1 {only} {page}
         ORDER BY f.started_at DESC, f.id DESC
         LIMIT $2"
    );
    let statement = client.prepare_cached(&query).await.map_err(db_error)?;
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = match after {
        Some((start, id)) => {
            client
                .query(&statement, &[&account_id, &limit, start, id])
                .await
        }
        None => client.query(&statement, &[&account_id, &limit]).await,
    }
    .map_err(db_error)?;
    rows.iter().map(flight_from).collect()
}

/// One page of everyone's public flights.
///
/// The condition on visibility is in every form of the query, as text and
/// spelled as in the three partial indexes that hold the public flights:
/// by start, by points, and by where they took off. Nothing a caller
/// passes can take it out.
///
/// Which of the indexes answers best depends on what is asked: a box
/// around one take-off holds a few flights that are quickly sorted, a box
/// around the Alps holds most, and then walking the list in its order and
/// leaving out what lies elsewhere is quicker. The database can only tell
/// with the box and the times in hand. So this query is planned anew for
/// every call, and not kept prepared like the others: a kept plan is made
/// for values nobody has named yet.
pub(super) async fn public(
    client: &Client,
    which: &PublicFlights,
    order: &PublicOrder,
    limit: usize,
) -> Result<Vec<Flight>, StoreError> {
    let edges = which
        .area
        .map(|area| [area.west(), area.south(), area.east(), area.north()]);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = Vec::new();
    let mut conditions = vec!["f.visibility = 'public'".to_owned()];

    if let Some(from) = &which.from {
        parameters.push(from);
        conditions.push(format!("f.started_at >= ${}", parameters.len()));
    }
    if let Some(before) = &which.before {
        parameters.push(before);
        conditions.push(format!("f.started_at < ${}", parameters.len()));
    }
    if let Some([west, south, east, north]) = &edges {
        let w = parameters.len() + 1;
        let (s, e, n) = (w + 1, w + 2, w + 3);
        parameters.extend([west as &(dyn ToSql + Sync), south, east, north]);
        // `&&` lets the index find the candidates; the exact comparison
        // follows, edges included.
        conditions.push(format!(
            "f.takeoff && ST_MakeEnvelope(${w}, ${s}, ${e}, ${n}, 4326)
             AND f.takeoff_lon BETWEEN ${w} AND ${e}
             AND f.takeoff_lat BETWEEN ${s} AND ${n}"
        ));
    }
    let sorted = match order {
        PublicOrder::Newest(after) => {
            if let Some((start, id)) = after {
                parameters.extend([start as &(dyn ToSql + Sync), id]);
                let last = parameters.len();
                conditions.push(format!("(f.started_at, f.id) < (${}, ${last})", last - 1));
            }
            "f.started_at DESC, f.id DESC"
        }
        PublicOrder::Best(after) => {
            conditions.push("f.score_points IS NOT NULL".to_owned());
            if let Some((points, id)) = after {
                parameters.extend([points as &(dyn ToSql + Sync), id]);
                let last = parameters.len();
                conditions.push(format!("(f.score_points, f.id) < (${}, ${last})", last - 1));
            }
            "f.score_points DESC, f.id DESC"
        }
    };
    parameters.push(&limit);
    let query = format!(
        "SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM}
         WHERE {}
         ORDER BY {sorted}
         LIMIT ${}",
        conditions.join(" AND "),
        parameters.len()
    );
    let rows = client
        .query(query.as_str(), &parameters)
        .await
        .map_err(db_error)?;
    rows.iter().map(flight_from).collect()
}

pub(super) async fn changes(
    client: &mut Client,
    account_id: &str,
    since: i64,
    limit: usize,
) -> Result<FlightChanges, StoreError> {
    // Both lists and the latest number must come from one moment. Read at
    // two moments, a deletion could be seen while a change before it was
    // missed, and a device that then asked for "everything after the
    // deletion" would never hear of that change.
    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await
        .map_err(db_error)?;
    // One more than the limit of each, to know whether more are waiting.
    let more_than_limit = i64::try_from(limit).map_or(i64::MAX, |limit| limit.saturating_add(1));

    let query = format!(
        "SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM}
         WHERE f.account_id = $1 AND f.change > $2
         ORDER BY f.change
         LIMIT $3"
    );
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let rows = tx
        .query(&statement, &[&account_id, &since, &more_than_limit])
        .await
        .map_err(db_error)?;
    let mut flights: Vec<Flight> = rows.iter().map(flight_from).collect::<Result<_, _>>()?;

    let statement = tx
        .prepare_cached(
            "SELECT flight_id, identity, change FROM flight_deletions
             WHERE account_id = $1 AND change > $2
             ORDER BY change
             LIMIT $3",
        )
        .await
        .map_err(db_error)?;
    let rows = tx
        .query(&statement, &[&account_id, &since, &more_than_limit])
        .await
        .map_err(db_error)?;
    let mut deleted: Vec<DeletedFlight> = rows
        .iter()
        .map(|row| DeletedFlight {
            id: row.get("flight_id"),
            identity: row.get("identity"),
            change: row.get("change"),
        })
        .collect();

    let latest = tx
        .query_opt(
            "SELECT flight_change FROM accounts WHERE id = $1",
            &[&account_id],
        )
        .await
        .map_err(db_error)?
        .map_or(0, |row| row.get(0));
    tx.commit().await.map_err(db_error)?;

    let more = cut(&mut flights, &mut deleted, limit);
    Ok(FlightChanges {
        flights,
        deleted,
        more,
        latest,
    })
}

pub(super) async fn set_visibility(
    client: &mut Client,
    account_id: &str,
    id: &str,
    visibility: Visibility,
    now: Timestamp,
) -> Result<VisibilityChange, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(account) = lock_account(&tx, account_id).await? else {
        return Ok(VisibilityChange::NoSuchFlight);
    };
    let owned = "f.id = $1 AND f.account_id = $2";
    let Some(flight) = one(&tx, owned, &[&id, &account_id]).await? else {
        return Ok(VisibilityChange::NoSuchFlight);
    };
    // The account's row is locked. Whoever takes this flight down or
    // suspends the account locks it too, so neither can slip in between
    // this look and the change below.
    let taken_down = flight.taken_down_at.is_some();
    if let Err(refused) = may_be_set(taken_down, account.publishing_suspended, visibility) {
        return Ok(refused);
    }
    if flight.visibility == visibility {
        return Ok(VisibilityChange::Set(Box::new(flight)));
    }

    let change = take_change(&tx, account_id, account.latest, now).await?;
    tx.execute(
        "UPDATE flights SET visibility = $2, change = $3 WHERE id = $1",
        &[&id, &visibility.as_str(), &change],
    )
    .await
    .map_err(db_error)?;
    let flight = one(&tx, owned, &[&id, &account_id]).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(flight.map_or(VisibilityChange::NoSuchFlight, |flight| {
        VisibilityChange::Set(Box::new(flight))
    }))
}

/// The flight with this address, with its account's row locked for the
/// rest of the transaction; `None` if there is no such flight.
async fn locked_with_its_account(
    tx: &Transaction<'_>,
    id: &str,
) -> Result<Option<(Flight, i64)>, StoreError> {
    // PostgreSQL takes no zero byte in text. No address holds one, so the
    // answer is known.
    if id.contains('\0') {
        return Ok(None);
    }
    let owner = tx
        .query_opt("SELECT account_id FROM flights WHERE id = $1", &[&id])
        .await
        .map_err(db_error)?;
    let Some(owner) = owner else {
        return Ok(None);
    };
    let Some(account) = lock_account(tx, owner.get(0)).await? else {
        return Ok(None);
    };
    // Read under the lock: the flight as nothing can change it now.
    let flight = one(tx, "f.id = $1", &[&id]).await?;
    Ok(flight.map(|flight| (flight, account.latest)))
}

pub(super) async fn take_down(
    client: &mut Client,
    id: &str,
    now: Timestamp,
) -> Result<Option<Flight>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some((flight, latest)) = locked_with_its_account(&tx, id).await? else {
        return Ok(None);
    };
    if flight.taken_down_at.is_some() {
        return Ok(Some(flight));
    }
    let change = take_change(&tx, &flight.account_id, latest, now).await?;
    tx.execute(
        "UPDATE flights SET visibility = 'private', taken_down_at = $2, change = $3
         WHERE id = $1",
        &[&id, &now, &change],
    )
    .await
    .map_err(db_error)?;
    let flight = one(&tx, "f.id = $1", &[&id]).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(flight)
}

pub(super) async fn release(
    client: &mut Client,
    id: &str,
    now: Timestamp,
) -> Result<Option<Flight>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some((flight, latest)) = locked_with_its_account(&tx, id).await? else {
        return Ok(None);
    };
    if flight.taken_down_at.is_none() {
        return Ok(Some(flight));
    }
    let change = take_change(&tx, &flight.account_id, latest, now).await?;
    tx.execute(
        "UPDATE flights SET taken_down_at = NULL, change = $2 WHERE id = $1",
        &[&id, &change],
    )
    .await
    .map_err(db_error)?;
    let flight = one(&tx, "f.id = $1", &[&id]).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(flight)
}

pub(super) async fn delete(
    client: &mut Client,
    account_id: &str,
    id: &str,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(Locked { latest, .. }) = lock_account(&tx, account_id).await? else {
        return Ok(false);
    };
    let removed = tx
        .query_opt(
            "DELETE FROM flights WHERE id = $1 AND account_id = $2
             RETURNING identity, taken_down_at",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    let Some(removed) = removed else {
        return Ok(false);
    };
    let identity: &str = removed.get(0);
    // Kept with the deletion, so that the flight comes back taken down if
    // its file is uploaded again.
    let taken_down_at: Option<Timestamp> = removed.get(1);

    let change = take_change(&tx, account_id, latest, now).await?;
    tx.execute(
        "INSERT INTO flight_deletions
             (account_id, flight_id, identity, change, deleted_at, taken_down_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (account_id, flight_id) DO UPDATE
         SET identity = EXCLUDED.identity,
             change = EXCLUDED.change,
             deleted_at = EXCLUDED.deleted_at,
             taken_down_at = EXCLUDED.taken_down_at",
        &[&account_id, &id, &identity, &change, &now, &taken_down_at],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(true)
}

pub(super) async fn outdated(
    client: &Client,
    version: u32,
    after: Option<&(u32, String)>,
    limit: usize,
) -> Result<Vec<OutdatedFlight>, StoreError> {
    // One walk along the index on (analysis_version, id), whether a few
    // flights are behind or all of them.
    let statement = client
        .prepare_cached(
            "SELECT id, analysis_version, file FROM flights
             WHERE analysis_version < $1 AND (analysis_version, id) > ($2, $3)
             ORDER BY analysis_version, id
             LIMIT $4",
        )
        .await
        .map_err(db_error)?;
    let number = |version: u32| i32::try_from(version).unwrap_or(i32::MAX);
    // Without a flight to continue after, start before every flight.
    let (after_version, after_id) = match after {
        Some((version, id)) => (number(*version), id.as_str()),
        None => (i32::MIN, ""),
    };
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let rows = client
        .query(
            &statement,
            &[&number(version), &after_version, &after_id, &limit],
        )
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok(OutdatedFlight {
                id: row.get("id"),
                version: u32::try_from(row.get::<_, i32>("analysis_version"))
                    .map_err(|_| corrupt("analysis version", "a negative number"))?,
                file: row.get("file"),
            })
        })
        .collect()
}

pub(super) async fn set_derived(
    client: &mut Client,
    id: &str,
    derived: &Derived,
    now: Timestamp,
) -> Result<bool, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let owner = tx
        .query_opt("SELECT account_id FROM flights WHERE id = $1", &[&id])
        .await
        .map_err(db_error)?;
    let Some(owner) = owner else {
        return Ok(false);
    };
    let account_id: &str = owner.get(0);
    let Some(Locked { latest, .. }) = lock_account(&tx, account_id).await? else {
        return Ok(false);
    };
    let change = take_change(&tx, account_id, latest, now).await?;

    let values = DerivedValues::of(derived);
    let (takeoff, landing) = (&values.takeoff, &values.landing);
    let assignments: Vec<String> = DERIVED_COLUMNS
        .iter()
        .enumerate()
        .map(|(index, column)| format!("{column} = ${}", index + 4))
        .collect();
    let update = format!(
        "UPDATE flights SET change = $2, profile = $3, {} WHERE id = $1",
        assignments.join(", ")
    );
    let statement = tx.prepare_cached(&update).await.map_err(db_error)?;
    let updated = tx
        .execute(
            &statement,
            &[
                &id,
                &change,
                &values.profile,
                &values.version,
                &derived.start,
                &derived.airtime_s,
                &takeoff.at,
                &takeoff.lat,
                &takeoff.lon,
                &takeoff.altitude_m,
                &landing.at,
                &landing.lat,
                &landing.lon,
                &landing.altitude_m,
                &derived.track_length_m,
                &derived.max_altitude_m,
                &derived.max_climb_ms,
                &values.score_kind,
                &derived.score.map(|score| score.closed),
                &derived.score.map(|score| score.distance_m),
                &derived.score.map(|score| score.points),
                &derived.glider,
                &values.outline,
            ],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(updated > 0)
}
