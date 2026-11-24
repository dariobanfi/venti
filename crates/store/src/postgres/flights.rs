//! Flights in PostgreSQL.
//!
//! Every change to a flight first locks its account's row and takes the
//! account's next change number in the same transaction. Changes to one
//! account therefore happen one after another, and their numbers are in
//! the order in which they become visible, which is what lets a device ask
//! for "everything after number n" without ever missing a change.

use chrono::{Datelike, NaiveDate};
use deadpool_postgres::{Client, GenericClient, Transaction};
use tokio_postgres::types::ToSql;
use tokio_postgres::{IsolationLevel, Row};
use venti_domain::{GeoPoint, Timestamp};

use super::takeoffs::distance;
use super::{corrupt, db_error};
use crate::flight::{
    Kept, PublicEffect, Room, Saved, cut, cut_public, ended_from_start_m, may_be_set, next_change,
    next_public_change, ranked_order, sought, uploaded_visibility,
};
use crate::takeoff::around;
use crate::{
    AllFlightsSet, AllVisibilityChange, DeletedFlight, Derived, Flight, FlightChanges, FlightKey,
    Flown, FoundPilot, GliderClass, Listing, Month, NewFlight, OutdatedFlight, Place,
    PublicFlightChanges, PublicFlights, PublicOrder, Ranked, Score, ScoreKind, StoreError,
    Visibility, VisibilityChange, WithdrawnFlight,
};

/// Everything [`flight_from`] reads: a flight `f` and its account `a`.
pub(super) const FLIGHT_COLUMNS: &str = "
    f.id, f.account_id, a.handle, a.display_name,
    (SELECT p.id FROM pictures p WHERE p.account_id = a.id) AS pilot_picture,
    f.identity, f.visibility, f.uploaded_at,
    f.change, f.file_bytes, f.analysis_version, f.started_at, f.airtime_s,
    f.takeoff_at, f.takeoff_lat, f.takeoff_lon, f.takeoff_altitude_m,
    f.landing_at, f.landing_lat, f.landing_lon, f.landing_altitude_m,
    f.track_length_m, f.max_altitude_m, f.max_climb_ms,
    f.score_kind, f.score_closed, f.score_distance_m, f.score_points,
    f.glider, f.outline, f.profile, f.taken_down_at, f.published_at, f.public_change,
    a.country AS pilot_country, f.glider_class, f.habits, f.takeoff_country";

pub(super) const FLIGHT_FROM: &str = "FROM flights f JOIN accounts a ON a.id = f.account_id";

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

pub(super) fn flight_from(row: &Row) -> Result<Flight, StoreError> {
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
        pilot_country: row.get("pilot_country"),
        pilot_picture: row.get("pilot_picture"),
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
            habits: habits_from(row)?,
            takeoff_country: row.get("takeoff_country"),
        },
        change: row.get("change"),
        taken_down_at: row.get("taken_down_at"),
        published_at: row.get("published_at"),
        public_change: row.get("public_change"),
        glider_class: row
            .get::<_, Option<&str>>("glider_class")
            .map(|name| GliderClass::from_name(name).ok_or_else(|| corrupt("glider class", name)))
            .transpose()?,
    })
}

/// A flight's figures of how it was flown, kept as JSON text.
fn habits_from(row: &Row) -> Result<Option<String>, StoreError> {
    Ok(row.get("habits"))
}

/// The derived values that need converting before they fit their columns.
struct DerivedValues<'a> {
    version: i32,
    takeoff: PlaceValues,
    landing: PlaceValues,
    score_kind: Option<&'static str>,
    outline: Vec<f64>,
    profile: Vec<i32>,
    habits: Option<&'a str>,
}

/// The columns that hold what is worked out from the file.
const DERIVED_COLUMNS: [&str; 22] = [
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
    "habits",
    "takeoff_country",
];

impl<'a> DerivedValues<'a> {
    fn of(derived: &'a Derived) -> Self {
        Self {
            version: i32::try_from(derived.version).unwrap_or(i32::MAX),
            takeoff: PlaceValues::of(derived.takeoff),
            landing: PlaceValues::of(derived.landing),
            score_kind: derived.score.map(|score| score.kind.as_str()),
            outline: derived.outline.iter().flatten().copied().collect(),
            profile: derived.profile.iter().flatten().copied().collect(),
            habits: derived.habits.as_deref(),
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
    /// Whether a flight it uploads is public from the start.
    new_flights_public: bool,
}

/// Locks an account's row for the rest of the transaction and returns what
/// it says. `None` if there is no such account.
async fn lock_account(
    tx: &Transaction<'_>,
    account_id: &str,
) -> Result<Option<Locked>, StoreError> {
    let row = tx
        .query_opt(
            "SELECT flight_change, publishing_suspended_at IS NOT NULL, new_flights_public
             FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| Locked {
        latest: row.get(0),
        publishing_suspended: row.get(1),
        new_flights_public: row.get(2),
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

/// Takes the next `count` numbers of the count of changes to what the
/// public sees of flights, and returns the first of them.
///
/// The count's one row stays locked for the rest of the transaction, so
/// the numbers are committed in the order in which they were taken. Every
/// caller holds its account's row locked already: the two locks are always
/// taken in that order.
///
/// `now` is missing only when an account goes; see
/// [`next_public_change`].
pub(super) async fn take_public_changes(
    tx: &Transaction<'_>,
    count: i64,
    now: Option<Timestamp>,
) -> Result<i64, tokio_postgres::Error> {
    let latest: i64 = tx
        .query_one("SELECT latest FROM flight_public_change FOR UPDATE", &[])
        .await?
        .get(0);
    let first = next_public_change(latest, now);
    let last = first.saturating_add(count.saturating_sub(1));
    tx.execute("UPDATE flight_public_change SET latest = $1", &[&last])
        .await?;
    Ok(first)
}

/// Records that flights are public no more: the flight with the n-th of
/// `ids` takes the number `first` + n − 1. A flight that was withdrawn
/// before has one row, which is brought up to date.
///
/// `account_id` is missing when the flights go with their account.
pub(super) async fn record_withdrawals(
    tx: &Transaction<'_>,
    ids: &[String],
    account_id: Option<&str>,
    first: i64,
    now: Option<Timestamp>,
) -> Result<(), tokio_postgres::Error> {
    tx.execute(
        "INSERT INTO flight_withdrawals (flight_id, account_id, change, withdrawn_at)
         SELECT listed.id, $2, $3 + listed.place - 1, $4
         FROM unnest($1::text[]) WITH ORDINALITY AS listed (id, place)
         ON CONFLICT (flight_id) DO UPDATE
         SET account_id = EXCLUDED.account_id,
             change = EXCLUDED.change,
             withdrawn_at = EXCLUDED.withdrawn_at",
        &[&ids, &account_id, &first, &now],
    )
    .await?;
    Ok(())
}

/// Takes one flight of an account from the public: a number, and the
/// record. The caller changes the flight's row.
async fn withdraw(tx: &Transaction<'_>, flight: &Flight, now: Timestamp) -> Result<(), StoreError> {
    let number = take_public_changes(tx, 1, Some(now))
        .await
        .map_err(db_error)?;
    record_withdrawals(
        tx,
        std::slice::from_ref(&flight.id),
        Some(&flight.account_id),
        number,
        Some(now),
    )
    .await
    .map_err(db_error)
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
    let Some(account) = lock_account(&tx, &new.account_id).await? else {
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

    let change = take_change(&tx, &new.account_id, account.latest, new.uploaded_at).await?;
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
    // Still under the account's lock: neither its setting nor a suspension
    // can change before the flight is in.
    let visibility = uploaded_visibility(
        account.new_flights_public,
        account.publishing_suspended,
        taken_down_at.is_some(),
    );
    // A flight that is public from its upload is published as one that its
    // pilot made public: a moment, a number, and no withdrawal of an
    // earlier flight at its address told any longer.
    let (published_at, public_change) = if visibility == Visibility::Public {
        let number = take_public_changes(&tx, 1, Some(new.uploaded_at))
            .await
            .map_err(db_error)?;
        tx.execute(
            "DELETE FROM flight_withdrawals WHERE flight_id = $1",
            &[&new.id],
        )
        .await
        .map_err(db_error)?;
        (Some(new.uploaded_at), Some(number))
    } else {
        (None, None)
    };

    let derived = &new.derived;
    let values = DerivedValues::of(derived);
    let (takeoff, landing) = (&values.takeoff, &values.landing);
    let file_bytes = i32::try_from(new.file_bytes).unwrap_or(i32::MAX);
    let visibility = visibility.as_str();
    let insert = format!(
        "INSERT INTO flights
             (id, account_id, identity, visibility, uploaded_at, change, file, file_bytes,
              {}, profile, taken_down_at, published_at, public_change, glider_class)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, {}, $31, $32, $33, $34, $35)",
        DERIVED_COLUMNS.join(", "),
        (9..31)
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
            &values.habits,
            &derived.takeoff_country,
            &values.profile,
            &taken_down_at,
            &published_at,
            &public_change,
            &new.glider_class.map(GliderClass::as_str),
        ],
    )
    .await
    .map_err(db_error)?;

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
    client: &mut Client,
    which: &PublicFlights,
    order: &PublicOrder,
    limit: usize,
) -> Result<Vec<Flight>, StoreError> {
    let edges = edges_of(which);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let (query, parameters) = public_query(which, &edges, order, &limit);
    let rows = bounded_query(client, &query, &parameters, Shape::Page).await?;
    rows.iter().map(flight_from).collect()
}

/// The numbers of the places a list of public flights is asked for, and
/// the other values of its filters as the database takes them.
struct Edges {
    /// The area drawn on a map, as well-known text, and the box around it.
    drawn: Option<(String, [f64; 4])>,
    /// The glider class by its name.
    class: Option<&'static str>,
    /// The kind of route by its name.
    kind: Option<&'static str>,
    /// The box: west, south, east, north.
    area: Option<[f64; 4]>,
    /// The box around the place near which flights took off, the place's
    /// latitude and longitude, and how far from it.
    near: Option<[f64; 7]>,
    /// The glider class as the ranking's cells name it: '' for every
    /// class.
    class_or_every: &'static str,
    /// The take-off country as the cells name it: '' for every country.
    country_or_every: String,
}

fn edges_of(which: &PublicFlights) -> Edges {
    Edges {
        drawn: which
            .drawn
            .as_ref()
            .map(|drawn| (drawn.wkt(), drawn.bounds())),
        class: which.class.map(GliderClass::as_str),
        kind: which.kind.map(ScoreKind::as_str),
        area: which
            .area
            .map(|area| [area.west(), area.south(), area.east(), area.north()]),
        near: which.near.map(|near| {
            let (lat, lon) = (near.place.lat(), near.place.lon());
            let [west, south, east, north] = around(lat, lon, near.within_m);
            [west, south, east, north, lat, lon, near.within_m]
        }),
        class_or_every: which.class.map_or("", GliderClass::as_str),
        country_or_every: which.country.clone().unwrap_or_default(),
    }
}

/// The conditions that make a flight `f` one of the public flights `which`
/// describes, with their values put behind those already in `parameters`.
///
/// The condition on visibility is the first of them, as text, whatever
/// `which` says.
fn public_conditions<'a>(
    which: &'a PublicFlights,
    edges: &'a Edges,
    parameters: &mut Vec<&'a (dyn ToSql + Sync)>,
) -> Vec<String> {
    let mut conditions = vec!["f.visibility = 'public'".to_owned()];
    if let Some(pilot) = &which.pilot {
        parameters.push(pilot);
        conditions.push(format!("f.account_id = ${}", parameters.len()));
    }
    if let Some(from) = &which.from {
        parameters.push(from);
        conditions.push(format!("f.started_at >= ${}", parameters.len()));
    }
    if let Some(before) = &which.before {
        parameters.push(before);
        conditions.push(format!("f.started_at < ${}", parameters.len()));
    }
    if let Some([west, south, east, north]) = &edges.area {
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
    if let Some([west, south, east, north, lat, lon, within_m]) = &edges.near {
        let w = parameters.len() + 1;
        let (s, e, n, la, lo, m) = (w + 1, w + 2, w + 3, w + 4, w + 5, w + 6);
        parameters.extend([
            west as &(dyn ToSql + Sync),
            south,
            east,
            north,
            lat,
            lon,
            within_m,
        ]);
        // The box lets the index find the candidates; the distance is the
        // one `Store::flights_from` measures.
        conditions.push(format!(
            "f.takeoff && ST_MakeEnvelope(${w}, ${s}, ${e}, ${n}, 4326)
             AND {} <= ${m}::float8",
            distance(&format!("${la}"), &format!("${lo}"))
        ));
    }
    if let Some((wkt, [west, south, east, north])) = &edges.drawn {
        let w = parameters.len() + 1;
        let (s, e, n, a) = (w + 1, w + 2, w + 3, w + 4);
        parameters.extend([west as &(dyn ToSql + Sync), south, east, north, wkt]);
        // The box lets the index find the candidates, as for a box; the
        // area decides, its edges included.
        conditions.push(format!(
            "f.takeoff && ST_MakeEnvelope(${w}, ${s}, ${e}, ${n}, 4326)
             AND ST_Intersects(f.takeoff, ST_GeomFromText(${a}::text, 4326))"
        ));
    }
    if let Some(class) = &edges.class {
        parameters.push(class);
        conditions.push(format!("f.glider_class = ${}", parameters.len()));
    }
    if let Some(kind) = &edges.kind {
        parameters.push(kind);
        conditions.push(format!("f.score_kind = ${}", parameters.len()));
    }
    if let Some(least) = &which.min_distance_m {
        parameters.push(least);
        conditions.push(format!(
            "f.score_distance_m >= ${}::float8",
            parameters.len()
        ));
    }
    if let Some(country) = &which.country {
        parameters.push(country);
        conditions.push(format!("f.takeoff_country = ${}", parameters.len()));
    }
    if let Some(country) = &which.pilot_country {
        parameters.push(country);
        conditions.push(format!(
            "EXISTS (SELECT 1 FROM accounts pilot WHERE pilot.id = f.account_id \
             AND pilot.country = ${})",
            parameters.len()
        ));
    }
    conditions
}

/// The most a read of everyone's flights may take before the database
/// gives it up: whatever a request asks, it holds no connection and no
/// core longer. The indexes answer every page the API asks for in a few
/// milliseconds over millions of flights (`tests/scale.rs`); one that
/// takes this long is a mistake, and is told as the database being busy.
pub(super) const READ_TIMEOUT: &str = "2s";

/// What a read of everyone's flights does.
#[derive(Clone, Copy)]
enum Shape {
    /// Finds a page along an index.
    Page,
    /// Adds up many flights.
    Sum,
}

/// Runs one read of everyone's flights under [`READ_TIMEOUT`].
async fn bounded_query(
    client: &mut Client,
    query: &str,
    parameters: &[&(dyn ToSql + Sync)],
    shape: Shape,
) -> Result<Vec<Row>, StoreError> {
    let tx = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(db_error)?;
    // Compiling a plan to machine code costs a quarter of a second, more
    // than any of these queries takes.
    let parallel = match shape {
        // Starting helpers costs more than a page of fifty takes, which
        // walks an index until it has them (measured: 40 ms of 45).
        Shape::Page => "SET LOCAL max_parallel_workers_per_gather = 0;",
        Shape::Sum => "",
    };
    tx.batch_execute(&format!(
        "SET LOCAL statement_timeout = '{READ_TIMEOUT}'; SET LOCAL jit = off; {parallel}"
    ))
    .await
    .map_err(db_error)?;
    let rows = tx.query(query, parameters).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(rows)
}

/// The text of one page of everyone's public flights and its values, as
/// [`public`] asks the database: for [`explain_public`] too.
fn public_query<'a>(
    which: &'a PublicFlights,
    edges: &'a Edges,
    order: &'a PublicOrder,
    limit: &'a i64,
) -> (String, Vec<&'a (dyn ToSql + Sync)>) {
    let mut parameters: Vec<&(dyn ToSql + Sync)> = Vec::new();
    let mut conditions = public_conditions(which, edges, &mut parameters);
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
    parameters.push(limit);
    // The page's addresses first, from an index alone: every column a
    // condition reads is in the two indexes that order the list (0034).
    // Then the rows of those flights only.
    let query = format!(
        "SELECT {FLIGHT_COLUMNS}
         FROM (SELECT f.id FROM flights f
               WHERE {}
               ORDER BY {sorted}
               LIMIT ${}) page
         JOIN flights f ON f.id = page.id
         JOIN accounts a ON a.id = f.account_id
         ORDER BY {sorted}",
        conditions.join(" AND "),
        parameters.len()
    );
    (query, parameters)
}

/// What the database does for a page of everyone's public flights:
/// `EXPLAIN (ANALYZE, BUFFERS)` of the very query [`public`] sends, as
/// text. For the tests that hold the plans to the indexes and for
/// measuring them.
pub(super) async fn explain_public(
    client: &mut Client,
    which: &PublicFlights,
    order: &PublicOrder,
    limit: usize,
) -> Result<String, StoreError> {
    let edges = edges_of(which);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let (query, parameters) = public_query(which, &edges, order, &limit);
    explain(client, &query, &parameters, Shape::Page).await
}

/// The same for the ranking.
pub(super) async fn explain_ranking(
    client: &mut Client,
    which: &PublicFlights,
    counted: usize,
    limit: usize,
) -> Result<String, StoreError> {
    let edges = edges_of(which);
    let cells = cells_of(which, counted);
    let counted = i64::try_from(counted).unwrap_or(i64::MAX);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let (query, parameters) = match &cells {
        Some(cells) => kept_ranking_query(cells, &counted, &limit),
        None => ranking_query(which, &edges, &counted, &limit),
    };
    explain(client, &query, &parameters, Shape::Sum).await
}

/// The same for the months that have a ranking.
pub(super) async fn explain_scored_months(
    client: &mut Client,
    which: &PublicFlights,
) -> Result<String, StoreError> {
    let edges = edges_of(which);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = Vec::new();
    let query = scored_months_query(which, &edges, &mut parameters);
    explain(client, &query, &parameters, Shape::Sum).await
}

async fn explain(
    client: &mut Client,
    query: &str,
    parameters: &[&(dyn ToSql + Sync)],
    shape: Shape,
) -> Result<String, StoreError> {
    let rows = bounded_query(
        client,
        &format!("EXPLAIN (ANALYZE, BUFFERS) {query}"),
        parameters,
        shape,
    )
    .await?;
    Ok(rows
        .iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// The pilots of everyone's public flights by the sum of their best.
///
/// Where the kept cells hold the ranking asked for (0036,
/// [`cells_of`]), it is read from them: the best three of each pilot of a
/// period, kept as flights change. Otherwise the database sums the
/// flights themselves.
///
/// As in [`public`], the condition on visibility is in the query as text,
/// and the query is planned anew for every call: how many flights a span
/// and a box hold decides how they are best found.
///
/// The database numbers each pilot's scored flights from the best down,
/// sums those that count and keeps the pilots with the largest sums; the
/// flights that count come back one a row, and the sum that is handed out
/// is made from them here, as the in-memory store makes it.
pub(super) async fn ranking(
    client: &mut Client,
    which: &PublicFlights,
    counted: usize,
    limit: usize,
) -> Result<Vec<Ranked>, StoreError> {
    let edges = edges_of(which);
    let cells = cells_of(which, counted);
    let counted = i64::try_from(counted).unwrap_or(i64::MAX);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let (query, parameters) = match &cells {
        Some(cells) => kept_ranking_query(cells, &counted, &limit),
        None => ranking_query(which, &edges, &counted, &limit),
    };
    let rows = bounded_query(client, &query, &parameters, Shape::Sum).await?;

    // The rows of a pilot follow each other, the best flight first.
    let mut pilots: Vec<(u32, Vec<Flight>)> = Vec::new();
    for row in &rows {
        let flight = flight_from(row)?;
        match pilots.last_mut() {
            Some((_, flights))
                if flights
                    .last()
                    .is_some_and(|last| last.account_id == flight.account_id) =>
            {
                flights.push(flight);
            }
            _ => {
                let scored: i64 = row.get("ranked_scored");
                pilots.push((u32::try_from(scored).unwrap_or(u32::MAX), vec![flight]));
            }
        }
    }
    let mut ranked: Vec<Ranked> = pilots
        .into_iter()
        .filter_map(|(scored, flights)| Ranked::of(flights, scored))
        .collect();
    ranked.sort_by(ranked_order);
    Ok(ranked)
}

/// How many of a pilot's best flights a cell of the ranking keeps (0036).
const KEPT_BEST: usize = 3;

/// The first day of the cells' all time, and the day after the last month
/// they can hold: a span open at one end reaches it.
const CELLS_BEGIN: (i32, u32) = (1970, 1);
const CELLS_END: (i32, u32) = (9999, 1);

/// The kept cells a ranking is read from: of a glider class or of every
/// class (''), of a take-off country or of every country (''), and of a
/// span of whole months.
struct Cells {
    class: &'static str,
    country: String,
    pilot_country: Option<String>,
    span: CellSpan,
}

enum CellSpan {
    /// One period, `m`, `y` or `a`, and its first day: read from the top
    /// along an index.
    One(&'static str, NaiveDate),
    /// The whole years from `years_from` to `years_before`, and the months
    /// from `from` to `years_from` and from `years_before` to `before`.
    Many {
        from: NaiveDate,
        years_from: NaiveDate,
        years_before: NaiveDate,
        before: NaiveDate,
    },
}

fn first_of(year: i32, month: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, 1)
}

/// The first day of a month that a moment is the very first instant of,
/// by UTC; `None` for any other moment.
fn month_start(time: Timestamp) -> Option<NaiveDate> {
    let month = Month::of(time);
    (month.start() == time)
        .then(|| first_of(month.year(), month.month()))
        .flatten()
}

/// The cells that hold the ranking `which` asks for, if they hold it: the
/// best three of each pilot, of whole months by UTC, of every flight or
/// of those of one glider class or of one take-off country, and of pilots
/// of any country or of one. An area, a kind of route, a least distance,
/// one pilot, a class and a country together, or a span that does not
/// begin and end with a month are summed from the flights.
fn cells_of(which: &PublicFlights, counted: usize) -> Option<Cells> {
    let summed = which.pilot.is_some()
        || which.area.is_some()
        || which.near.is_some()
        || which.drawn.is_some()
        || which.kind.is_some()
        || which.min_distance_m.is_some()
        || (which.class.is_some() && which.country.is_some());
    if summed || counted != KEPT_BEST {
        return None;
    }
    let from = match which.from {
        Some(from) => Some(month_start(from)?),
        None => None,
    };
    let before = match which.before {
        Some(before) => Some(month_start(before)?),
        None => None,
    };
    let next = |day: NaiveDate, months: u32| day.checked_add_months(chrono::Months::new(months));
    let span = match (from, before) {
        (None, None) => CellSpan::One("a", first_of(CELLS_BEGIN.0, CELLS_BEGIN.1)?),
        (Some(from), Some(before)) if next(from, 1) == Some(before) => CellSpan::One("m", from),
        (Some(from), Some(before)) if from.month() == 1 && next(from, 12) == Some(before) => {
            CellSpan::One("y", from)
        }
        (from, before) => {
            let from = from.or_else(|| first_of(CELLS_BEGIN.0, CELLS_BEGIN.1))?;
            let before = before.or_else(|| first_of(CELLS_END.0, CELLS_END.1))?;
            let years_from = if from.month() == 1 {
                from
            } else {
                first_of(from.year() + 1, 1)?
            };
            let years_before = first_of(before.year(), 1)?;
            if years_from < years_before {
                CellSpan::Many {
                    from,
                    years_from,
                    years_before,
                    before,
                }
            } else {
                // No whole year: months only.
                CellSpan::Many {
                    from,
                    years_from: before,
                    years_before: before,
                    before,
                }
            }
        }
    };
    Some(Cells {
        class: which.class.map_or("", GliderClass::as_str),
        country: which.country.clone().unwrap_or_default(),
        pilot_country: which.pilot_country.clone(),
        span,
    })
}

/// The text of a ranking read from the kept cells and its values, as
/// [`ranking`] asks the database. The order of pilots and of their
/// flights is that of [`ranking_query`].
fn kept_ranking_query<'a>(
    cells: &'a Cells,
    counted: &'a i64,
    limit: &'a i64,
) -> (String, Vec<&'a (dyn ToSql + Sync)>) {
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&cells.class, &cells.country];
    let mut of_country = String::new();
    if let Some(country) = &cells.pilot_country {
        parameters.push(country);
        of_country = format!("AND owner.country = ${}", parameters.len());
    }
    let query = match &cells.span {
        CellSpan::One(period, starts) => {
            parameters.extend([period as &(dyn ToSql + Sync), starts, limit]);
            let p = parameters.len() - 2;
            let (s, l) = (p + 1, p + 2);
            format!(
                "WITH top AS (
                     SELECT r.account_id, r.points, r.scored::int8 AS scored, r.best_ids, owner.handle
                     FROM ranking_cells r JOIN accounts owner ON owner.id = r.account_id
                     WHERE r.period = ${p} AND r.glider_class = $1 AND r.takeoff_country = $2
                       AND r.starts = ${s} {of_country}
                     ORDER BY r.points DESC, owner.handle COLLATE \"C\"
                     LIMIT ${l}
                 )
                 SELECT top.scored AS ranked_scored, {FLIGHT_COLUMNS}
                 FROM top
                 CROSS JOIN LATERAL unnest(top.best_ids) WITH ORDINALITY AS best (id, place)
                 JOIN flights f ON f.id = best.id AND f.visibility = 'public'
                 JOIN accounts a ON a.id = f.account_id
                 ORDER BY top.points DESC, top.handle COLLATE \"C\", best.place"
            )
        }
        CellSpan::Many {
            from,
            years_from,
            years_before,
            before,
        } => {
            parameters.extend([
                from as &(dyn ToSql + Sync),
                years_from,
                years_before,
                before,
                counted,
                limit,
            ]);
            let f = parameters.len() - 5;
            let (yf, yb, b, c, l) = (f + 1, f + 2, f + 3, f + 4, f + 5);
            // The span's cells, by three ranges of the index that orders
            // them: the whole years, and the months before and after them.
            let span = |columns: &str| {
                format!(
                    "SELECT {columns} FROM ranking_cells r
                     WHERE r.period = 'y' AND r.glider_class = $1 AND r.takeoff_country = $2
                       AND r.starts >= ${yf} AND r.starts < ${yb} AND r.account_id <> ''
                     UNION ALL
                     SELECT {columns} FROM ranking_cells r
                     WHERE r.period = 'm' AND r.glider_class = $1 AND r.takeoff_country = $2
                       AND r.starts >= ${f} AND r.starts < ${yf} AND r.account_id <> ''
                     UNION ALL
                     SELECT {columns} FROM ranking_cells r
                     WHERE r.period = 'm' AND r.glider_class = $1 AND r.takeoff_country = $2
                       AND r.starts >= ${yb} AND r.starts < ${b} AND r.account_id <> ''"
                )
            };
            // Only pilots who can be among the first `limit` are summed,
            // of the country asked for.
            // Each pilot's sum of the span is at least that of their best
            // cell and at most that of all their cells. So the `limit`-th
            // largest best cell of a pilot is a bar no pilot of the top
            // lies below, and a pilot whose cells add up to less cannot
            // reach it. The cells' sums are read from the index alone.
            format!(
                "WITH per AS (
                     SELECT c.account_id, sum(c.points) AS most, max(c.points) AS least,
                            sum(c.scored) AS scored
                     FROM ({}) c
                     JOIN accounts owner ON owner.id = c.account_id
                     WHERE true {of_country}
                     GROUP BY c.account_id
                 ), bar AS (
                     SELECT least FROM per ORDER BY least DESC OFFSET ${l}::int8 - 1 LIMIT 1
                 ), candidates AS (
                     SELECT per.account_id, per.scored FROM per
                     WHERE per.most >= COALESCE((SELECT least FROM bar), 0)
                 ), each AS (
                     SELECT c.account_id, best.id, best.points,
                            row_number() OVER (
                                PARTITION BY c.account_id
                                ORDER BY best.points DESC, best.id COLLATE \"C\" DESC
                            ) AS place
                     FROM ({}) c
                     JOIN candidates USING (account_id)
                     CROSS JOIN LATERAL unnest(c.best_ids, c.best_points) AS best (id, points)
                 ), top AS (
                     SELECT e.account_id, owner.handle, sum(e.points) AS points,
                            max(s.scored) AS scored
                     FROM each e
                     JOIN candidates s ON s.account_id = e.account_id
                     JOIN accounts owner ON owner.id = e.account_id
                     WHERE e.place <= ${c} {of_country}
                     GROUP BY e.account_id, owner.handle
                     ORDER BY points DESC, owner.handle COLLATE \"C\"
                     LIMIT ${l}
                 )
                 SELECT top.scored AS ranked_scored, {FLIGHT_COLUMNS}
                 FROM top
                 JOIN each e ON e.account_id = top.account_id AND e.place <= ${c}
                 JOIN flights f ON f.id = e.id AND f.visibility = 'public'
                 JOIN accounts a ON a.id = f.account_id
                 ORDER BY top.points DESC, top.handle COLLATE \"C\", e.place",
                span("r.account_id, r.points, r.scored"),
                span("r.account_id, r.best_ids, r.best_points"),
            )
        }
    };
    (query, parameters)
}

/// The text of the ranking and its values, as [`ranking`] asks the
/// database.
fn ranking_query<'a>(
    which: &'a PublicFlights,
    edges: &'a Edges,
    counted: &'a i64,
    limit: &'a i64,
) -> (String, Vec<&'a (dyn ToSql + Sync)>) {
    let mut parameters: Vec<&(dyn ToSql + Sync)> = Vec::new();
    let mut conditions = public_conditions(which, edges, &mut parameters);
    conditions.push("f.score_points IS NOT NULL".to_owned());
    parameters.push(counted);
    let c = parameters.len();
    parameters.push(limit);
    let l = parameters.len();
    // Handles are compared byte by byte, as the in-memory store compares
    // them: the database's own order of text depends on how it was set up.
    let query = format!(
        "WITH scored AS (
             SELECT f.id, f.account_id, f.score_points,
                    row_number() OVER (
                        PARTITION BY f.account_id ORDER BY f.score_points DESC, f.id DESC
                    ) AS place,
                    count(*) OVER (PARTITION BY f.account_id) AS scored
             FROM flights f
             WHERE {}
         ), top AS (
             SELECT s.account_id, owner.handle, sum(s.score_points) AS points,
                    max(s.scored) AS scored
             FROM scored s JOIN accounts owner ON owner.id = s.account_id
             WHERE s.place <= ${c}
             GROUP BY s.account_id, owner.handle
             ORDER BY points DESC, owner.handle COLLATE \"C\"
             LIMIT ${l}
         )
         SELECT top.scored AS ranked_scored, {FLIGHT_COLUMNS}
         FROM top
         JOIN scored s ON s.account_id = top.account_id AND s.place <= ${c}
         JOIN flights f ON f.id = s.id
         JOIN accounts a ON a.id = f.account_id
         ORDER BY top.points DESC, top.handle COLLATE \"C\", s.place",
        conditions.join(" AND ")
    );
    (query, parameters)
}

/// The months that have a scored public flight among those asked for.
///
/// Of all of everyone's flights, and of those of one glider class or of
/// one take-off country, they are counted from the kept cells (0036);
/// otherwise from the flights. As in [`public`], the condition on
/// visibility is in the query as text.
pub(super) async fn scored_months(
    client: &mut Client,
    which: &PublicFlights,
) -> Result<Vec<(Month, u32)>, StoreError> {
    let edges = edges_of(which);
    let mut parameters: Vec<&(dyn ToSql + Sync)> = Vec::new();
    let query = scored_months_query(which, &edges, &mut parameters);
    let rows = bounded_query(client, &query, &parameters, Shape::Sum).await?;
    rows.iter()
        .map(|row| {
            let (year, month): (i32, i32) = (row.get("year"), row.get("month"));
            let flights: i64 = row.get("flights");
            let month = u32::try_from(month)
                .ok()
                .and_then(|month| Month::new(year, month))
                .ok_or_else(|| corrupt("month of a flight", month))?;
            Ok((month, u32::try_from(flights).unwrap_or(u32::MAX)))
        })
        .collect()
}

/// The text of [`scored_months`], its values put into `parameters`.
fn scored_months_query<'a>(
    which: &'a PublicFlights,
    edges: &'a Edges,
    parameters: &mut Vec<&'a (dyn ToSql + Sync)>,
) -> String {
    let kept = PublicFlights {
        class: which.class,
        country: which.country.clone(),
        ..PublicFlights::default()
    };
    if *which == kept && (which.class.is_none() || which.country.is_none()) {
        parameters.extend([
            &edges.class_or_every as &(dyn ToSql + Sync),
            &edges.country_or_every,
        ]);
        // Everyone's months, the cells of the account '' (0036).
        return "SELECT date_part('year', r.starts)::int4 AS year,
                       date_part('month', r.starts)::int4 AS month,
                       r.scored::int8 AS flights
                FROM ranking_cells r
                WHERE r.account_id = '' AND r.period = 'm'
                  AND r.glider_class = $1 AND r.takeoff_country = $2 AND r.scored > 0
                ORDER BY r.starts"
            .to_owned();
    }
    let mut conditions = public_conditions(which, edges, parameters);
    conditions.push("f.score_points IS NOT NULL".to_owned());
    format!(
        "SELECT date_part('year', f.started_at AT TIME ZONE 'UTC')::int4 AS year,
                date_part('month', f.started_at AT TIME ZONE 'UTC')::int4 AS month,
                count(*) AS flights
         FROM flights f
         WHERE {}
         GROUP BY 1, 2
         ORDER BY 1, 2",
        conditions.join(" AND ")
    )
}

/// The pilots with a public flight whose handle or name holds a text.
///
/// The database makes the small letters of a handle and a name, by its
/// own rules for letters outside ASCII. Handles are compared byte by
/// byte, as the in-memory store compares them.
pub(super) async fn pilots_named(
    client: &Client,
    text: &str,
    limit: usize,
) -> Result<Vec<FoundPilot>, StoreError> {
    let sought = sought(text);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let statement = client
        .prepare_cached(
            "SELECT a.handle, a.display_name, count(*) AS public_flights,
                    (SELECT p.id FROM pictures p WHERE p.account_id = a.id) AS picture_id,
                    (strpos(lower(a.handle), $1) = 1
                     OR strpos(lower(a.display_name), $1) = 1) AS begins
             FROM accounts a JOIN flights f ON f.account_id = a.id
             WHERE f.visibility = 'public'
               AND (strpos(lower(a.handle), $1) > 0 OR strpos(lower(a.display_name), $1) > 0)
             GROUP BY a.id, a.handle, a.display_name
             ORDER BY begins DESC, public_flights DESC, a.handle COLLATE \"C\"
             LIMIT $2",
        )
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&sought, &limit])
        .await
        .map_err(db_error)?;
    Ok(rows
        .iter()
        .map(|row| FoundPilot {
            handle: row.get("handle"),
            name: row.get("display_name"),
            picture: row.get("picture_id"),
            flights: u32::try_from(row.get::<_, i64>("public_flights")).unwrap_or(u32::MAX),
        })
        .collect())
}

/// An account's flights in the numbers its totals are made of: the public
/// ones, or all of them.
pub(super) async fn flown_by(
    client: &Client,
    account_id: &str,
    listing: Listing,
) -> Result<Vec<Flown>, StoreError> {
    // The condition on visibility is spelled as in the partial index.
    let which = match listing {
        Listing::Public => "AND f.visibility = 'public'",
        Listing::All => "",
    };
    let statement = client
        .prepare_cached(&format!(
            "SELECT f.id, f.started_at, f.airtime_s, f.score_distance_m, f.score_points,
                    f.max_altitude_m, f.takeoff_lat, f.takeoff_lon, f.habits,
                    f.outline[1:2] AS outline_first,
                    f.outline[cardinality(f.outline) - 1:cardinality(f.outline)] AS outline_last
             FROM flights f
             WHERE f.account_id = $1 {which}
             ORDER BY f.started_at DESC, f.id DESC"
        ))
        .await
        .map_err(db_error)?;
    let rows = client
        .query(&statement, &[&account_id])
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            let distance_m: Option<f64> = row.get("score_distance_m");
            let points: Option<f64> = row.get("score_points");
            let lat: Option<f64> = row.get("takeoff_lat");
            let lon: Option<f64> = row.get("takeoff_lon");
            let ends: Vec<[f64; 2]> = [row.get("outline_first"), row.get("outline_last")]
                .into_iter()
                .filter_map(|pair: Vec<f64>| Some([*pair.first()?, *pair.get(1)?]))
                .collect();
            Ok(Flown {
                id: row.get("id"),
                start: row.get("started_at"),
                airtime_s: row.get("airtime_s"),
                score: distance_m.zip(points),
                max_altitude_m: row.get("max_altitude_m"),
                takeoff: lat
                    .zip(lon)
                    .map(|(lat, lon)| GeoPoint::new(lat, lon))
                    .transpose()
                    .map_err(|e| corrupt("flight position", e))?,
                habits: habits_from(row)?,
                ended_from_start_m: ended_from_start_m(&ends),
            })
        })
        .collect()
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

pub(super) async fn public_changes(
    client: &mut Client,
    account_id: Option<&str>,
    since: i64,
    limit: usize,
) -> Result<PublicFlightChanges, StoreError> {
    // Both lists and the latest number from one moment, as in [`changes`].
    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await
        .map_err(db_error)?;
    // One more than the limit of each, to know whether more are waiting.
    let more_than_limit = i64::try_from(limit).map_or(i64::MAX, |limit| limit.saturating_add(1));
    let mut parameters: Vec<&(dyn ToSql + Sync)> = vec![&since, &more_than_limit];
    if let Some(account_id) = &account_id {
        parameters.push(account_id);
    }

    // The condition on visibility is in the query as text, spelled as in
    // the partial indexes on `public_change`.
    let of_account = |column: &str| match account_id {
        Some(_) => format!("AND {column} = $3"),
        None => String::new(),
    };
    let query = format!(
        "SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM}
         WHERE f.visibility = 'public' AND f.public_change > $1 {}
         ORDER BY f.public_change
         LIMIT $2",
        of_account("f.account_id")
    );
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let rows = tx.query(&statement, &parameters).await.map_err(db_error)?;
    let mut flights: Vec<Flight> = rows.iter().map(flight_from).collect::<Result<_, _>>()?;

    let query = format!(
        "SELECT flight_id, change, withdrawn_at FROM flight_withdrawals
         WHERE change > $1 {}
         ORDER BY change
         LIMIT $2",
        of_account("account_id")
    );
    let statement = tx.prepare_cached(&query).await.map_err(db_error)?;
    let rows = tx.query(&statement, &parameters).await.map_err(db_error)?;
    let mut withdrawn: Vec<WithdrawnFlight> = rows
        .iter()
        .map(|row| WithdrawnFlight {
            id: row.get("flight_id"),
            change: row.get("change"),
            withdrawn_at: row.get("withdrawn_at"),
        })
        .collect();

    let latest = tx
        .query_opt("SELECT latest FROM flight_public_change", &[])
        .await
        .map_err(db_error)?
        .map_or(0, |row| row.get(0));
    tx.commit().await.map_err(db_error)?;

    let more = cut_public(&mut flights, &mut withdrawn, limit);
    Ok(PublicFlightChanges {
        flights,
        withdrawn,
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
    // What the public sees of the flight, in the same transaction: a
    // flight that becomes public gets its moment and a number, one that
    // stops being public loses both and is recorded as withdrawn.
    let (published_at, public_change) = match PublicEffect::of(flight.visibility, visibility) {
        PublicEffect::Published => {
            let number = take_public_changes(&tx, 1, Some(now))
                .await
                .map_err(db_error)?;
            tx.execute(
                "DELETE FROM flight_withdrawals WHERE flight_id = $1",
                &[&id],
            )
            .await
            .map_err(db_error)?;
            (Some(now), Some(number))
        }
        PublicEffect::Withdrawn => {
            withdraw(&tx, &flight, now).await?;
            (None, None)
        }
        PublicEffect::Nothing => (None, None),
    };
    tx.execute(
        "UPDATE flights
         SET visibility = $2, change = $3, published_at = $4, public_change = $5
         WHERE id = $1",
        &[
            &id,
            &visibility.as_str(),
            &change,
            &published_at,
            &public_change,
        ],
    )
    .await
    .map_err(db_error)?;
    let flight = one(&tx, owned, &[&id, &account_id]).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(flight.map_or(VisibilityChange::NoSuchFlight, |flight| {
        VisibilityChange::Set(Box::new(flight))
    }))
}

pub(super) async fn set_all_visibility(
    client: &mut Client,
    account_id: &str,
    visibility: Visibility,
    now: Timestamp,
) -> Result<AllVisibilityChange, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(account) = lock_account(&tx, account_id).await? else {
        return Ok(AllVisibilityChange::NoSuchAccount);
    };
    // The account's row is locked: nothing uploads, changes, takes down or
    // deletes a flight of it beside this, and its publishing is not
    // suspended meanwhile.
    if may_be_set(false, account.publishing_suspended, visibility).is_err() {
        return Ok(AllVisibilityChange::PublishingSuspended);
    }
    // By address, byte by byte, as the in-memory store orders them.
    let rows = tx
        .query(
            "SELECT id, visibility, taken_down_at IS NOT NULL FROM flights
             WHERE account_id = $1 ORDER BY id COLLATE \"C\"",
            &[&account_id],
        )
        .await
        .map_err(db_error)?;
    let mut set = AllFlightsSet::default();
    let mut changing: Vec<String> = Vec::new();
    let mut withdrawn: Vec<String> = Vec::new();
    for row in &rows {
        let was: &str = row.get(1);
        let was = Visibility::from_name(was).ok_or_else(|| corrupt("visibility", was))?;
        if set.count(was, row.get(2), visibility) {
            let id: String = row.get(0);
            if PublicEffect::of(was, visibility) == PublicEffect::Withdrawn {
                withdrawn.push(id.clone());
            }
            changing.push(id);
        }
    }
    let Some(more) = i64::try_from(changing.len()).ok().filter(|n| *n > 0) else {
        tx.commit().await.map_err(db_error)?;
        return Ok(AllVisibilityChange::Set(set));
    };

    // Each flight is a change of its own, in the order of the addresses.
    let first = next_change(account.latest, now);
    if visibility == Visibility::Public {
        // Every one of them becomes public, with a number of the public
        // count of its own in the same order.
        let first_public = take_public_changes(&tx, more, Some(now))
            .await
            .map_err(db_error)?;
        tx.execute(
            "DELETE FROM flight_withdrawals WHERE flight_id = ANY($1)",
            &[&changing],
        )
        .await
        .map_err(db_error)?;
        tx.execute(
            "UPDATE flights f
             SET visibility = 'public', change = $2 + listed.place - 1,
                 published_at = $3, public_change = $4 + listed.place - 1
             FROM unnest($1::text[]) WITH ORDINALITY AS listed (id, place)
             WHERE f.id = listed.id",
            &[&changing, &first, &now, &first_public],
        )
        .await
        .map_err(db_error)?;
    } else {
        if let Some(count) = i64::try_from(withdrawn.len()).ok().filter(|n| *n > 0) {
            let first_public = take_public_changes(&tx, count, Some(now))
                .await
                .map_err(db_error)?;
            record_withdrawals(&tx, &withdrawn, Some(account_id), first_public, Some(now))
                .await
                .map_err(db_error)?;
        }
        tx.execute(
            "UPDATE flights f
             SET visibility = $2, change = $3 + listed.place - 1,
                 published_at = NULL, public_change = NULL
             FROM unnest($1::text[]) WITH ORDINALITY AS listed (id, place)
             WHERE f.id = listed.id",
            &[&changing, &visibility.as_str(), &first],
        )
        .await
        .map_err(db_error)?;
    }
    tx.execute(
        "UPDATE accounts SET flight_change = $2 WHERE id = $1",
        &[&account_id, &first.saturating_add(more - 1)],
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(AllVisibilityChange::Set(set))
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
    if flight.visibility == Visibility::Public {
        withdraw(&tx, &flight, now).await?;
    }
    tx.execute(
        "UPDATE flights
         SET visibility = 'private', taken_down_at = $2, change = $3,
             published_at = NULL, public_change = NULL
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
             RETURNING identity, taken_down_at, visibility = 'public'",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    let Some(removed) = removed else {
        return Ok(false);
    };
    // A public flight that is deleted is withdrawn.
    if removed.get::<_, bool>(2) {
        let number = take_public_changes(&tx, 1, Some(now))
            .await
            .map_err(db_error)?;
        record_withdrawals(&tx, &[id.to_owned()], Some(account_id), number, Some(now))
            .await
            .map_err(db_error)?;
    }
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

pub(super) async fn set_glider_class(
    client: &mut Client,
    account_id: &str,
    id: &str,
    class: Option<GliderClass>,
    now: Timestamp,
) -> Result<Option<Flight>, StoreError> {
    let tx = client.transaction().await.map_err(db_error)?;
    let Some(Locked { latest, .. }) = lock_account(&tx, account_id).await? else {
        return Ok(None);
    };
    // Read under the account's lock: what the public sees of a public
    // flight changes with it.
    let found = tx
        .query_opt(
            "SELECT visibility = 'public' FROM flights WHERE id = $1 AND account_id = $2",
            &[&id, &account_id],
        )
        .await
        .map_err(db_error)?;
    let Some(found) = found else {
        return Ok(None);
    };
    let change = take_change(&tx, account_id, latest, now).await?;
    let public_change = if found.get::<_, bool>(0) {
        Some(
            take_public_changes(&tx, 1, Some(now))
                .await
                .map_err(db_error)?,
        )
    } else {
        None
    };
    tx.execute(
        "UPDATE flights SET glider_class = $2, change = $3,
             public_change = COALESCE($4, public_change)
         WHERE id = $1",
        &[
            &id,
            &class.map(GliderClass::as_str),
            &change,
            &public_change,
        ],
    )
    .await
    .map_err(db_error)?;
    let flight = one(&tx, "f.id = $1", &[&id]).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(flight)
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
    // Read under the account's lock: what the public sees of a public
    // flight changes with its values.
    let public = tx
        .query_opt(
            "SELECT visibility = 'public' FROM flights WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(db_error)?
        .is_some_and(|row| row.get(0));
    if public {
        let number = take_public_changes(&tx, 1, Some(now))
            .await
            .map_err(db_error)?;
        tx.execute(
            "UPDATE flights SET public_change = $2 WHERE id = $1",
            &[&id, &number],
        )
        .await
        .map_err(db_error)?;
    }

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
                &values.habits,
                &derived.takeoff_country,
            ],
        )
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(updated > 0)
}
