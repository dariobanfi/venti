//! Where thermals have been: the climbs of the public flights the server
//! holds, counted by cell of a grid, month of the year and hour of the
//! day.
//!
//! ```text
//!  the public flights' changes ──► a flight's file ──► its thermals
//!  (a cursor; a few at a time)     (as it was uploaded)  (the analysis' own)
//!                                                           │ climbs that
//!                                                           ▼ gained height
//!  a flight that left the public   ◄── removed ──   climbs by cell, in memory
//! ```
//!
//! **Public flights and nothing else.** The list this follows names a
//! flight when it becomes public and again when it is public no more
//! ([`Store::public_flight_changes`]); a flight that leaves the public is
//! gone from the next answer after the pass that reads its withdrawal, at
//! most [`EVERY`] later.
//!
//! **A cell names air, not a person.** A cell is handed out only when the
//! climbs of at least [`FLOWN_MIN_PILOTS`] different pilots lie in it,
//! counted after the request's month and hours are applied: one pilot's
//! climbs say where that pilot flies. It carries no pilot, no flight and
//! no day, its place is the cell's middle, and its heights are rounded to
//! 50 m. `docs/architecture.md` has the rule in its decision log.
//!
//! **What counts as a climb here**: a thermal as the flight analysis
//! finds it ([`venti_flight::analysis::thermals`]) that gained at least
//! [`MIN_GAIN_M`] at [`MIN_CLIMB_MS`] or more on average. It lies in the
//! cell of the point half-way between where the circling began and where
//! it ended, in the month and the hour it began by the sun at that place
//! (UTC and four minutes for each degree east): no table of time zones,
//! and noon is when the sun is highest everywhere.
//!
//! **What it costs.** Nothing is in the database: a climb is sixteen
//! bytes in memory, a flight has some fifteen, so 100,000 public flights
//! are about 24 MB, and at most [`MAX_CLIMBS`] climbs are held (48 MB);
//! beyond that new flights are left out and the log says so. An answer
//! looks up the cells of its box, at most 40,000, and reads no file. The
//! cost is in the first pass after the server starts: every public flight
//! is read and its thermals found again, some 30 ms each on one core (an
//! estimate; the take-off is found by playing the track through the
//! flight computer), [`BATCH`] at a time beside everything else, so
//! 100,000 flights take most of an hour, during which the map fills.
//! When that is too long, the climbs belong in a table written when a
//! flight is analysed; nothing in the answer would change.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration as StdDuration;

use chrono::{Datelike, Duration, Timelike};
use tokio_util::sync::CancellationToken;
use venti_api_types::v1::{FLOWN_CELL_DEG, FLOWN_MIN_PILOTS, FlownThermalCell, MAX_FLOWN_CELLS};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_flight::{analysis, igc};
use venti_store::{Store, StoreError, Visibility};

use super::derive::text_of;
use super::unpack;

/// The least height a climb gained, metres.
pub const MIN_GAIN_M: i32 = 50;

/// The least average climb, metres per second.
pub const MIN_CLIMB_MS: f64 = 0.5;

/// The most climbs held in memory.
pub const MAX_CLIMBS: usize = 3_000_000;

/// How many changes of public flights one pass reads.
pub const BATCH: usize = 32;

/// How long the server waits between two passes once it has read every
/// change.
pub const EVERY: StdDuration = StdDuration::from_secs(300);

/// One climb of a flight, as it is read from the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlownClimb {
    /// Half-way between where the circling began and ended.
    pub position: GeoPoint,
    /// When it began.
    pub entered: Timestamp,
    /// The average climb, metres per second.
    pub climb_ms: f64,
    /// The height it began at, metres.
    pub from_m: i32,
    /// The height it ended at.
    pub to_m: i32,
}

/// The climbs in a flight file. Empty for a file that is no flight file,
/// which no stored file is.
pub fn climbs_of(file: &[u8]) -> Vec<FlownClimb> {
    let Ok(recording) = igc::parse(&text_of(file)) else {
        return Vec::new();
    };
    analysis::thermals(&recording.fixes)
        .into_iter()
        .filter(|(thermal, _)| thermal.gain_m >= MIN_GAIN_M && thermal.climb_ms >= MIN_CLIMB_MS)
        .filter_map(|(thermal, entered)| {
            let (entry, exit) = (thermal.entry.position, thermal.exit.position);
            Some(FlownClimb {
                position: GeoPoint::new(
                    f64::midpoint(entry.lat(), exit.lat()),
                    f64::midpoint(entry.lon(), exit.lon()),
                )
                .ok()?,
                entered,
                climb_ms: thermal.climb_ms,
                from_m: thermal.entry.altitude_m,
                to_m: thermal.exit.altitude_m,
            })
        })
        .collect()
}

/// A cell of the grid, counted from the equator and from Greenwich.
type Cell = (i32, i32);

fn cell_of(place: GeoPoint) -> Cell {
    // Bounded: a degree has two hundred cells.
    #[allow(clippy::cast_possible_truncation)]
    let index = |degrees: f64| (degrees / FLOWN_CELL_DEG).floor() as i32;
    (index(place.lat()), index(place.lon()))
}

fn middle_of(cell: Cell) -> Option<GeoPoint> {
    let middle = |index: i32| ((f64::from(index) + 0.5) * FLOWN_CELL_DEG * 1e6).round() / 1e6;
    GeoPoint::new(middle(cell.0), middle(cell.1)).ok()
}

/// A climb as it is held: sixteen bytes.
#[derive(Debug, Clone, Copy)]
struct Held {
    flight: u32,
    pilot: u32,
    /// Hundredths of a metre per second.
    climb_cms: i16,
    from_m: i16,
    to_m: i16,
    /// 1 to 12, by the sun.
    month: u8,
    /// 0 to 23, by the sun.
    hour: u8,
}

#[derive(Default)]
struct State {
    /// The public change the next pass asks after.
    cursor: i64,
    /// Each flight read, by its address: its number here, and the cells
    /// its climbs lie in.
    flights: HashMap<String, (u32, Vec<Cell>)>,
    /// A number for each pilot, by account. Neither leaves this module.
    pilots: HashMap<String, u32>,
    cells: HashMap<Cell, Vec<Held>>,
    climbs: usize,
    next_flight: u32,
    full: bool,
}

/// The climbs of the public flights, by cell.
#[derive(Default)]
pub struct FlownThermals {
    state: Mutex<State>,
}

impl std::fmt::Debug for FlownThermals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlownThermals").finish_non_exhaustive()
    }
}

fn clamp_i16(value: f64) -> i16 {
    // Bounded just before.
    #[allow(clippy::cast_possible_truncation)]
    let clamped = value
        .round()
        .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
    clamped
}

impl FlownThermals {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Takes the climbs of a public flight of `account_id`, in place of
    /// what was held of the flight.
    pub fn take(&self, flight_id: &str, account_id: &str, climbs: &[FlownClimb]) {
        let mut state = self.state();
        remove(&mut state, flight_id);
        if state.climbs + climbs.len() > MAX_CLIMBS {
            if !state.full {
                state.full = true;
                tracing::warn!(
                    "the thermals of public flights are full at {MAX_CLIMBS} climbs; flights \
                     made public from now on are left out of them"
                );
            }
            return;
        }
        let next_pilot = u32::try_from(state.pilots.len()).unwrap_or(u32::MAX);
        let pilot = *state
            .pilots
            .entry(account_id.to_owned())
            .or_insert(next_pilot);
        let flight = state.next_flight;
        state.next_flight = state.next_flight.wrapping_add(1);
        let mut cells = Vec::with_capacity(climbs.len());
        for climb in climbs {
            let cell = cell_of(climb.position);
            // By the sun at the place: four minutes for each degree east.
            #[allow(clippy::cast_possible_truncation)]
            let ahead = Duration::seconds((climb.position.lon() * 240.0).floor() as i64);
            let by_the_sun = climb.entered + ahead;
            state.cells.entry(cell).or_default().push(Held {
                flight,
                pilot,
                climb_cms: clamp_i16(climb.climb_ms * 100.0),
                from_m: clamp_i16(f64::from(climb.from_m)),
                to_m: clamp_i16(f64::from(climb.to_m)),
                month: u8::try_from(by_the_sun.month()).unwrap_or(1),
                hour: u8::try_from(by_the_sun.hour()).unwrap_or(0),
            });
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        state.climbs += climbs.len();
        state.flights.insert(flight_id.to_owned(), (flight, cells));
    }

    /// Forgets a flight that is public no more.
    pub fn withdraw(&self, flight_id: &str) {
        remove(&mut self.state(), flight_id);
    }

    /// How many flights have been read.
    pub fn flights(&self) -> usize {
        self.state().flights.len()
    }

    /// The cells of a box in which at least [`FLOWN_MIN_PILOTS`] pilots
    /// climbed in the month and between the hours asked for, the most
    /// pilots first, and whether the box holds more than
    /// [`MAX_FLOWN_CELLS`].
    pub fn cells(
        &self,
        area: &BoundingBox,
        month: Option<u8>,
        hours: Option<[u8; 2]>,
    ) -> (Vec<FlownThermalCell>, bool) {
        let state = self.state();
        // Bounded: a degree has two hundred cells.
        #[allow(clippy::cast_possible_truncation)]
        let index = |degrees: f64| (degrees / FLOWN_CELL_DEG).floor() as i32;
        let (south, west) = (index(area.south()), index(area.west()));
        let (north, east) = (index(area.north()), index(area.east()));
        let mut found: Vec<(Cell, FlownThermalCell)> = Vec::new();
        for lat in south..=north {
            for lon in west..=east {
                let Some(held) = state.cells.get(&(lat, lon)) else {
                    continue;
                };
                let asked: Vec<&Held> = held
                    .iter()
                    .filter(|climb| month.is_none_or(|month| climb.month == month))
                    .filter(|climb| {
                        hours.is_none_or(|[from, to]| climb.hour >= from && climb.hour < to)
                    })
                    .collect();
                let mut pilots: Vec<u32> = asked.iter().map(|climb| climb.pilot).collect();
                pilots.sort_unstable();
                pilots.dedup();
                let pilots = u32::try_from(pilots.len()).unwrap_or(u32::MAX);
                // The guard: fewer pilots than this say where they fly.
                if pilots < FLOWN_MIN_PILOTS {
                    continue;
                }
                let Some(position) = middle_of((lat, lon)).filter(|place| area.contains(*place))
                else {
                    continue;
                };
                let count = asked.len() as f64;
                let mean = |of: fn(&Held) -> i16| {
                    asked.iter().map(|climb| f64::from(of(climb))).sum::<f64>() / count
                };
                let best = asked.iter().map(|climb| climb.climb_cms).max().unwrap_or(0);
                let to_50 = |metres: f64| {
                    // Bounded: it was an i16.
                    #[allow(clippy::cast_possible_truncation)]
                    let rounded = ((metres / 50.0).round() * 50.0) as i32;
                    rounded
                };
                found.push((
                    (lat, lon),
                    FlownThermalCell {
                        position,
                        climbs: u32::try_from(asked.len()).unwrap_or(u32::MAX),
                        pilots,
                        climb_ms: (mean(|climb| climb.climb_cms) / 10.0).round() / 10.0,
                        best_climb_ms: (f64::from(best) / 10.0).round() / 10.0,
                        from_m: to_50(mean(|climb| climb.from_m)),
                        to_m: to_50(mean(|climb| climb.to_m)),
                    },
                ));
            }
        }
        drop(state);
        found.sort_by(|a, b| (b.1.pilots, b.1.climbs, a.0).cmp(&(a.1.pilots, a.1.climbs, b.0)));
        let more = found.len() > MAX_FLOWN_CELLS;
        found.truncate(MAX_FLOWN_CELLS);
        (found.into_iter().map(|(_, cell)| cell).collect(), more)
    }

    /// Reads the next changes of the public flights: at most [`BATCH`].
    /// Whether more are waiting.
    pub async fn update<S: Store>(&self, store: &S) -> Result<bool, StoreError> {
        let since = self.state().cursor;
        let changes = store.public_flight_changes(None, since, BATCH).await?;
        if changes.latest < since {
            // The numbers are not this database's as it is now: start
            // again from nothing.
            *self.state() = State::default();
            return Ok(true);
        }
        for withdrawn in &changes.withdrawn {
            self.withdraw(&withdrawn.id);
        }
        for flight in &changes.flights {
            // The store names public flights only; a mistake there would
            // count a private flight.
            if flight.visibility != Visibility::Public {
                self.withdraw(&flight.id);
                continue;
            }
            let Some(file) = store.flight_file(&flight.id).await? else {
                self.withdraw(&flight.id);
                continue;
            };
            // Reading a file and finding its thermals is work of some
            // tens of milliseconds: off the threads that answer requests.
            let climbs = tokio::task::spawn_blocking(move || {
                unpack(&file).map_or_else(Vec::new, |file| climbs_of(&file))
            })
            .await
            .unwrap_or_default();
            self.take(&flight.id, &flight.account_id, &climbs);
        }
        let read_to = changes
            .last()
            .unwrap_or(if changes.more { since } else { changes.latest });
        self.state().cursor = read_to.max(since);
        Ok(changes.more)
    }

    /// Reads every change there is now: for the development mode and the
    /// tests.
    pub async fn update_all<S: Store>(&self, store: &S) -> Result<(), StoreError> {
        while self.update(store).await? {}
        Ok(())
    }
}

fn remove(state: &mut State, flight_id: &str) {
    let Some((flight, cells)) = state.flights.remove(flight_id) else {
        return;
    };
    for cell in cells {
        let Some(held) = state.cells.get_mut(&cell) else {
            continue;
        };
        let before = held.len();
        held.retain(|climb| climb.flight != flight);
        state.climbs -= before - held.len();
        if held.is_empty() {
            state.cells.remove(&cell);
        }
    }
    state.full = false;
}

/// Follows the public flights until `shutdown`: a pass at a time, the
/// next at once while changes wait and after [`EVERY`] when none do.
pub async fn run<S: Store>(flown: Arc<FlownThermals>, store: Arc<S>, shutdown: CancellationToken) {
    loop {
        let wait = match flown.update(store.as_ref()).await {
            // A breath between two passes, for everything else.
            Ok(true) => StdDuration::from_millis(50),
            Ok(false) => EVERY,
            Err(error) => {
                tracing::warn!("the public flights could not be read for their thermals: {error}");
                EVERY
            }
        };
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(wait) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_flight::testing::synthetic_flight;

    use super::*;

    fn climb(lat: f64, lon: f64, entered: Timestamp, climb_ms: f64) -> FlownClimb {
        FlownClimb {
            position: GeoPoint::new(lat, lon).unwrap(),
            entered,
            climb_ms,
            from_m: 1410,
            to_m: 2080,
        }
    }

    fn area() -> BoundingBox {
        BoundingBox::new(47.5, 11.5, 47.9, 12.1).unwrap()
    }

    #[test]
    fn the_sample_flights_thermals_are_its_climbs() {
        let file = igc::write(&synthetic_flight()).into_bytes();
        let climbs = climbs_of(&file);
        assert!(!climbs.is_empty());
        assert!(climbs.iter().all(|climb| climb.climb_ms >= MIN_CLIMB_MS));
        assert!(
            climbs
                .iter()
                .all(|climb| climb.to_m - climb.from_m >= MIN_GAIN_M)
        );
        assert!(climbs_of(b"not a flight file").is_empty());
    }

    #[test]
    fn a_cell_needs_three_pilots_after_the_month_and_the_hours() {
        let flown = FlownThermals::default();
        // 11:40 UTC at 11.8° east is 12:27 by the sun.
        let july = Utc.with_ymd_and_hms(2026, 7, 4, 11, 40, 0).unwrap();
        let may = Utc.with_ymd_and_hms(2025, 5, 9, 9, 0, 0).unwrap();
        flown.take("f1", "anna", &[climb(47.7012, 11.7989, july, 2.0)]);
        flown.take("f2", "ben", &[climb(47.7030, 11.7960, july, 3.0)]);
        // Two pilots: nothing.
        assert!(flown.cells(&area(), None, None).0.is_empty());
        // The same pilot again is no third.
        flown.take("f3", "ben", &[climb(47.7012, 11.7989, may, 1.0)]);
        assert!(flown.cells(&area(), None, None).0.is_empty());
        flown.take(
            "f4",
            "cara",
            &[
                climb(47.7012, 11.7989, may, 1.0),
                // Alone in its cell.
                climb(47.75, 11.9, may, 4.0),
            ],
        );
        let (cells, more) = flown.cells(&area(), None, None);
        assert!(!more);
        assert_eq!(cells.len(), 1);
        let cell = &cells[0];
        assert_eq!(
            (cell.position.lat(), cell.position.lon()),
            (47.7025, 11.7975)
        );
        assert_eq!((cell.climbs, cell.pilots), (4, 3));
        assert_eq!((cell.climb_ms, cell.best_climb_ms), (1.8, 3.0));
        assert_eq!((cell.from_m, cell.to_m), (1400, 2100));
        // No word of who: the answer is numbers and a place.
        let json = serde_json::to_string(&cells).unwrap();
        for name in ["anna", "ben", "cara", "f1"] {
            assert!(!json.contains(name));
        }

        // In July only two of the three climbed there; in May too.
        assert!(flown.cells(&area(), Some(7), None).0.is_empty());
        assert!(flown.cells(&area(), Some(5), None).0.is_empty());
        // Between 9 and 13 by the sun all four climbs lie; from 12 on two.
        assert_eq!(flown.cells(&area(), None, Some([9, 13])).0.len(), 1);
        assert!(flown.cells(&area(), None, Some([12, 24])).0.is_empty());
        // Another box.
        let elsewhere = BoundingBox::new(46.0, 11.5, 46.5, 12.1).unwrap();
        assert!(flown.cells(&elsewhere, None, None).0.is_empty());

        // A flight that leaves the public takes its climbs along.
        flown.withdraw("f1");
        assert!(flown.cells(&area(), None, None).0.is_empty());
        assert_eq!(flown.flights(), 3);
        // Taken again, it counts once.
        flown.take("f1", "anna", &[climb(47.7012, 11.7989, july, 2.0)]);
        flown.take("f1", "anna", &[climb(47.7012, 11.7989, july, 2.0)]);
        assert_eq!(flown.cells(&area(), None, None).0[0].climbs, 4);
    }
}
