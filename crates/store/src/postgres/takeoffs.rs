//! What public flights say about where pilots take off, in PostgreSQL.
//!
//! Every query here spells the condition on visibility as text, as the
//! partial index on where public flights took off does
//! (`flights_public_takeoff_idx`, `0008`): that index finds the flights,
//! and nothing a caller passes can take the condition out.
//!
//! What a day, a cell and a sum of positions are is said in
//! [`crate::takeoff`], and the SQL repeats those steps exactly; which
//! cell is a candidate is decided there, not here.

use deadpool_postgres::Client;
use tokio_postgres::IsolationLevel;
use tokio_postgres::types::ToSql;
use venti_domain::{BoundingBox, GeoPoint, Site, SourceId};

use super::flights::{FLIGHT_COLUMNS, FLIGHT_FROM, flight_from};
use super::{corrupt, db_error};
use crate::takeoff::{self, Cell, around, box_of_cells, cells_of, reach_of};
use crate::{
    CELLS_PER_DEGREE, CandidateRule, FlightsFrom, NearPilot, StoreError, TakeoffCandidate,
};

/// The day at the place when a flight `f` started: [`takeoff::solar_day`]
/// in SQL.
const DAY: &str = "((f.started_at AT TIME ZONE 'UTC')
    + make_interval(secs => floor(f.takeoff_lon * 240::float8)))::date";

/// How far a flight `f` took off from a place, in metres: the distance on the FAI sphere
/// as `GeoPoint::distance_m` works it out, step for step. The two are the
/// same formula on the same numbers; they could differ in the last digit
/// where the database's sine is not the one Rust was built with, which
/// decides nothing but for a flight that began at the stated distance to
/// a hundred-millionth of a millimetre.
///
/// The place is in the parameters `lat` and `lon`, as `$5` and `$6`.
pub(super) fn distance(lat: &str, lon: &str) -> String {
    format!(
        "2 * 6371000::float8 * asin(least(1::float8, sqrt(
    power(sin((radians(f.takeoff_lat) - radians({lat}::float8)) / 2), 2)
    + cos(radians({lat}::float8)) * cos(radians(f.takeoff_lat))
      * power(sin(radians(f.takeoff_lon - {lon}::float8) / 2), 2))))"
    )
}

/// The public flights that took off inside the box `$1` to `$4` and
/// within `$7` metres of the place.
fn near() -> String {
    format!(
        "f.visibility = 'public'
         AND f.takeoff && ST_MakeEnvelope($1, $2, $3, $4, 4326)
         AND {} <= $7",
        distance("$5", "$6")
    )
}

fn count(value: i64, what: &str) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|e| corrupt(what, e))
}

pub(super) async fn began_at(
    client: &mut Client,
    place: GeoPoint,
    within_m: f64,
    best: usize,
) -> Result<FlightsFrom, StoreError> {
    let nothing = FlightsFrom {
        flights: 0,
        pilots: 0,
        latest_day: None,
        by_month: [0; 12],
        best: Vec::new(),
    };
    // Nothing began nearer than nothing.
    if within_m.is_nan() || within_m < 0.0 {
        return Ok(nothing);
    }
    let [west, south, east, north] = around(place.lat(), place.lon(), within_m);
    let (lat, lon) = (place.lat(), place.lon());
    let limit = i64::try_from(best).unwrap_or(i64::MAX);
    // The counts and the best flights from one moment.
    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await
        .map_err(db_error)?;

    let counted = format!(
        "WITH near AS (
             SELECT f.account_id, {DAY} AS day
             FROM flights f
             WHERE {}
         )
         SELECT extract(month FROM n.day)::int AS month, count(*) AS flights,
                max(n.day) AS latest_day,
                (SELECT count(DISTINCT account_id) FROM near) AS pilots
         FROM near n
         GROUP BY 1",
        near()
    );
    let place_and_distance: [&(dyn ToSql + Sync); 7] =
        [&west, &south, &east, &north, &lat, &lon, &within_m];
    let rows = tx
        .query(counted.as_str(), &place_and_distance)
        .await
        .map_err(db_error)?;
    let mut answer = nothing;
    for row in &rows {
        let month = usize::try_from(row.get::<_, i32>("month") - 1)
            .ok()
            .filter(|month| *month < 12)
            .ok_or_else(|| corrupt("month of a flight", "not one of the twelve"))?;
        let flights = count(row.get("flights"), "count of flights")?;
        answer.by_month[month] = flights;
        answer.flights += flights;
        answer.pilots = count(row.get("pilots"), "count of pilots")?;
        answer.latest_day = answer.latest_day.max(row.get("latest_day"));
    }

    let best = format!(
        "SELECT {FLIGHT_COLUMNS} {FLIGHT_FROM}
         WHERE {} AND f.score_points IS NOT NULL
         ORDER BY f.score_points DESC, f.id DESC
         LIMIT $8",
        near()
    );
    let rows = tx
        .query(
            best.as_str(),
            &[&west, &south, &east, &north, &lat, &lon, &within_m, &limit],
        )
        .await
        .map_err(db_error)?;
    answer.best = rows.iter().map(flight_from).collect::<Result<_, _>>()?;
    Ok(answer)
}

/// The pilots whose public flights took off near a place since a moment.
pub(super) async fn pilots_near(
    client: &Client,
    place: GeoPoint,
    within_m: f64,
    since: venti_domain::Timestamp,
    limit: usize,
) -> Result<Vec<NearPilot>, StoreError> {
    if within_m.is_nan() || within_m < 0.0 {
        return Ok(Vec::new());
    }
    let [west, south, east, north] = around(place.lat(), place.lon(), within_m);
    let (lat, lon) = (place.lat(), place.lon());
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let query = format!(
        "SELECT a.id AS account_id, a.handle, a.display_name,
                (SELECT p.id FROM pictures p WHERE p.account_id = a.id) AS picture_id,
                count(*) AS flights
         FROM flights f JOIN accounts a ON a.id = f.account_id
         WHERE {} AND f.started_at >= $8
           AND a.shown_near_places AND a.publishing_suspended_at IS NULL
         GROUP BY a.id
         ORDER BY flights DESC, a.handle COLLATE \"C\"
         LIMIT $9",
        near()
    );
    let rows = client
        .query(
            query.as_str(),
            &[
                &west, &south, &east, &north, &lat, &lon, &within_m, &since, &limit,
            ],
        )
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            Ok(NearPilot {
                account_id: row.get("account_id"),
                handle: row.get("handle"),
                name: row.get("display_name"),
                picture: row.get("picture_id"),
                flights: count(row.get("flights"), "count of flights")?,
            })
        })
        .collect()
}

/// The public flights that began in the cells a box touches, added up
/// cell by cell: the cells in which at least `min_pilots` pilots took
/// off, which is all that [`takeoff::candidates`] looks at.
async fn cells(
    client: &Client,
    area: &BoundingBox,
    min_pilots: u32,
) -> Result<Vec<Cell>, StoreError> {
    let asked = cells_of(area);
    let [west, south, east, north] = box_of_cells(asked);
    let [first_lat, last_lat, first_lon, last_lon] = asked;
    let min_pilots = i64::from(min_pilots);
    let months: String = (1..=12)
        .map(|month| {
            format!(", count(*) FILTER (WHERE extract(month FROM c.day) = {month}) AS m{month}")
        })
        .collect();
    // The cell and the sums are whole numbers made from each flight's own
    // coordinates, as `takeoff::cell_of` and `takeoff::e7` make them.
    let query = format!(
        "SELECT c.cell_lat, c.cell_lon, count(*) AS flights,
                count(DISTINCT c.account_id) AS pilots,
                sum(c.lat_e7)::bigint AS lat_e7, sum(c.lon_e7)::bigint AS lon_e7,
                sum(c.altitude_m)::bigint AS altitude_m, max(c.day) AS latest_day
                {months}
         FROM (
             SELECT f.account_id,
                    floor(f.takeoff_lat * $5::float8)::int AS cell_lat,
                    floor(f.takeoff_lon * $5::float8)::int AS cell_lon,
                    floor(f.takeoff_lat * 10000000::float8 + 0.5::float8)::bigint AS lat_e7,
                    floor(f.takeoff_lon * 10000000::float8 + 0.5::float8)::bigint AS lon_e7,
                    f.takeoff_altitude_m::bigint AS altitude_m,
                    {DAY} AS day
             FROM flights f
             WHERE f.visibility = 'public'
               AND f.takeoff && ST_MakeEnvelope($1, $2, $3, $4, 4326)
         ) c
         WHERE c.cell_lat BETWEEN $6 AND $7 AND c.cell_lon BETWEEN $8 AND $9
         GROUP BY c.cell_lat, c.cell_lon
         HAVING count(DISTINCT c.account_id) >= $10"
    );
    // Planned anew for every call, as the list of public flights is: how
    // many flights a box holds decides how they are best found.
    let rows = client
        .query(
            query.as_str(),
            &[
                &west,
                &south,
                &east,
                &north,
                &CELLS_PER_DEGREE,
                &first_lat,
                &last_lat,
                &first_lon,
                &last_lon,
                &min_pilots,
            ],
        )
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            let mut by_month = [0; 12];
            for (place, month) in by_month.iter_mut().enumerate() {
                let name = format!("m{}", place + 1);
                *month = count(row.get(name.as_str()), "count of flights")?;
            }
            Ok(Cell {
                at: (row.get("cell_lat"), row.get("cell_lon")),
                flights: count(row.get("flights"), "count of flights")?,
                pilots: count(row.get("pilots"), "count of pilots")?,
                lat_e7: row.get("lat_e7"),
                lon_e7: row.get("lon_e7"),
                altitude_m: row.get("altitude_m"),
                latest_day: row.get("latest_day"),
                by_month,
            })
        })
        .collect()
}

/// Where the standing sites inside a box are: those of the sets of
/// `sources`, and those of both layers of the server's own. `&&` lets the
/// two indexes find them; it compares rounded boxes and may find a few
/// more, which is what a box drawn generously wants.
async fn sites_in(
    client: &Client,
    sources: &[SourceId],
    [west, south, east, north]: [f64; 4],
) -> Result<Vec<GeoPoint>, StoreError> {
    let statement = client
        .prepare_cached(
            "SELECT t.lat, t.lon
             FROM sites t
             WHERE t.source = ANY($5::text[])
               AND ST_SetSRID(ST_MakePoint(t.lon, t.lat), 4326)
                   && ST_MakeEnvelope($1, $2, $3, $4, 4326)
             UNION ALL
             SELECT s.lat, s.lon
             FROM pilot_sites s
             WHERE s.removed_at IS NULL
               AND ST_SetSRID(ST_MakePoint(s.lon, s.lat), 4326)
                   && ST_MakeEnvelope($1, $2, $3, $4, 4326)",
        )
        .await
        .map_err(db_error)?;
    let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
    let rows = client
        .query(&statement, &[&west, &south, &east, &north, &names])
        .await
        .map_err(db_error)?;
    rows.iter()
        .map(|row| {
            GeoPoint::new(row.get("lat"), row.get("lon")).map_err(|e| corrupt("site position", e))
        })
        .collect()
}

pub(super) async fn sites_near(
    client: &Client,
    sources: &[SourceId],
    places: &[GeoPoint],
    within_m: f64,
) -> Result<Vec<Vec<Site>>, StoreError> {
    let mut found = vec![Vec::new(); places.len()];
    // Nothing is nearer than nothing.
    if places.is_empty() || within_m.is_nan() || within_m < 0.0 {
        return Ok(found);
    }
    let mut edges: [Vec<f64>; 4] = Default::default();
    for place in places {
        for (edge, value) in edges
            .iter_mut()
            .zip(around(place.lat(), place.lon(), within_m))
        {
            edge.push(value);
        }
    }
    // A box around each place, and the standing sites inside it, which
    // the two indexes find; the exact distance is measured below.
    let statement = client
        .prepare_cached(
            "SELECT p.place, t.record
             FROM unnest($1::float8[], $2::float8[], $3::float8[], $4::float8[])
                  WITH ORDINALITY AS p (west, south, east, north, place)
             JOIN sites t
               ON t.source = ANY($5::text[])
              AND ST_SetSRID(ST_MakePoint(t.lon, t.lat), 4326)
                  && ST_MakeEnvelope(p.west, p.south, p.east, p.north, 4326)
             UNION ALL
             SELECT p.place, s.record
             FROM unnest($1::float8[], $2::float8[], $3::float8[], $4::float8[])
                  WITH ORDINALITY AS p (west, south, east, north, place)
             JOIN pilot_sites s
               ON s.removed_at IS NULL
              AND ST_SetSRID(ST_MakePoint(s.lon, s.lat), 4326)
                  && ST_MakeEnvelope(p.west, p.south, p.east, p.north, 4326)",
        )
        .await
        .map_err(db_error)?;
    let names: Vec<&str> = sources.iter().map(SourceId::as_str).collect();
    let [west, south, east, north] = &edges;
    let rows = client
        .query(&statement, &[west, south, east, north, &names])
        .await
        .map_err(db_error)?;
    for row in &rows {
        let place: i64 = row.get("place");
        let at = usize::try_from(place - 1)
            .ok()
            .filter(|at| *at < places.len())
            .ok_or_else(|| corrupt("place of a site", "not one that was asked about"))?;
        let site: Site =
            serde_json::from_str(row.get("record")).map_err(|e| corrupt("site record", e))?;
        if site.position.distance_m(places[at]) <= within_m {
            found[at].push(site);
        }
    }
    Ok(found)
}

pub(super) async fn candidates(
    client: &Client,
    sources: &[SourceId],
    area: &BoundingBox,
    rule: &CandidateRule,
    limit: usize,
) -> Result<Vec<TakeoffCandidate>, StoreError> {
    let cells = cells(client, area, rule.min_pilots).await?;
    // Most boxes hold no cell with enough pilots, and then no site is
    // read. The flights and the sites are not read at one moment: a site
    // added between the two takes its candidate with it one question
    // early, and nothing else can come of it.
    let Some(reach) = reach_of(&cells, rule.clear_m) else {
        return Ok(Vec::new());
    };
    let sites = sites_in(client, sources, reach).await?;
    Ok(takeoff::candidates(cells, &sites, rule, limit))
}
