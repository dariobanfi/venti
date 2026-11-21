//! What public flights say about where pilots take off: the flights that
//! began at a place, and the places where several pilots took off and no
//! site stands.
//!
//! Both are made from public flights and from nothing else. A flight that
//! is private or shared by link is in no count, no month and no list
//! here, and neither is one that was public and is not any more: nothing
//! is kept between two questions, so what a flight added is gone with the
//! next one.
//!
//! **Both stores answer alike** because what decides is here. A day, a
//! cell and a sum of positions are worked out from a flight's own numbers
//! in steps that the database repeats exactly ([`solar_day`], [`cell_of`],
//! [`e7`]); which cell is a candidate, where it is said to be and in which
//! order candidates come is [`candidates`], which both stores call. The
//! one thing each store measures for itself is how far a flight began
//! from a place ([`Store::flights_from`](crate::Store::flights_from)): the
//! same formula, written once in Rust and once in SQL.

use std::collections::BTreeSet;

use chrono::{Datelike, Duration, NaiveDate};
use venti_domain::{BoundingBox, GeoPoint, Timestamp};

use crate::Flight;
use crate::flight::Place;

/// How many cells of the grid a degree holds, each way: a cell is 0.0025
/// degrees of latitude by 0.0025 of longitude, which is 278 m from north
/// to south everywhere and 190 m from east to west at 47 degrees north.
///
/// A take-off is a slope some tens of metres across, and the fixes at
/// which recordings show their gliders flying lie along it. A cell this
/// size holds one take-off whole unless an edge of the grid runs through
/// it, and seldom two.
pub const CELLS_PER_DEGREE: f64 = 400.0;

/// How many steps a degree has in a candidate's position: it is rounded
/// to 0.0001 degrees, which is 11 m from north to south.
pub const CANDIDATE_STEPS_PER_DEGREE: f64 = 10_000.0;

/// What a candidate's height is rounded to, metres.
pub const CANDIDATE_ALTITUDE_STEP_M: i32 = 10;

/// What the public flights that began at a place say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightsFrom {
    /// How many public flights began there.
    pub flights: u32,
    /// How many pilots flew them.
    pub pilots: u32,
    /// The day of the newest of them, at the place ([`solar_day`]).
    /// `None` without a flight.
    pub latest_day: Option<NaiveDate>,
    /// How many of them began in each month of the year, January first,
    /// all years together.
    pub by_month: [u32; 12],
    /// The best of them by points, the best first, as the list of
    /// everyone's public flights orders them. A flight too short to score
    /// is not among them.
    pub best: Vec<Flight>,
}

/// What makes a place where flights began a candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateRule {
    /// The least number of different pilots whose public flights began in
    /// one cell.
    pub min_pilots: u32,
    /// How far from the middle of those take-offs no site may stand,
    /// metres. A site exactly this far away is in the way.
    pub clear_m: f64,
}

/// A place where the public flights of several pilots began and no site
/// stands. It names no pilot and no flight.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffCandidate {
    /// The middle of the take-offs, rounded to a step of
    /// [`CANDIDATE_STEPS_PER_DEGREE`].
    pub position: GeoPoint,
    /// The mean of the heights the flights took off at, satellite
    /// altitude in metres, rounded to [`CANDIDATE_ALTITUDE_STEP_M`].
    pub altitude_m: i32,
    /// How many public flights began there.
    pub flights: u32,
    /// How many pilots flew them.
    pub pilots: u32,
    /// The day of the newest of them, at the place.
    pub latest_day: NaiveDate,
    /// How many of them began in each month of the year, January first.
    pub by_month: [u32; 12],
}

/// The day at a place when a flight started there: the day by the sun,
/// which is the time in UTC moved by four minutes for every degree of
/// longitude.
///
/// A flight is flown in daylight, and by this clock the night's middle is
/// midnight everywhere. So the day is the one the pilot had on the
/// calendar, in the Alps and in Australia, without a table of time zones;
/// the day in UTC is the day before for a morning's flight in the far
/// east.
pub fn solar_day(start: Timestamp, lon: f64) -> NaiveDate {
    // Whole seconds, rounded down: the database takes the same step.
    #[allow(clippy::cast_possible_truncation)]
    let seconds = (lon * 240.0).floor() as i64;
    (start + Duration::seconds(seconds)).date_naive()
}

/// A coordinate in ten-millionths of a degree, a whole number: 11 mm.
/// Whole numbers add up to the same sum in whatever order they are added,
/// which a sum of the degrees themselves does not.
pub(crate) fn e7(degrees: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let whole = (degrees * 10_000_000.0 + 0.5).floor() as i64;
    whole
}

fn cell_index(degrees: f64) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let index = (degrees * CELLS_PER_DEGREE).floor() as i32;
    index
}

/// The cell of the grid a point lies in: its number from the equator
/// north and from Greenwich east.
pub(crate) fn cell_of(position: GeoPoint) -> (i32, i32) {
    (cell_index(position.lat()), cell_index(position.lon()))
}

/// The cells a box touches: the first and the last from south to north,
/// and from west to east.
///
/// A cell is asked for whole or not at all. One that an edge of the box
/// runs through is counted with all of its flights, those outside the box
/// too, so that a place is the same candidate whichever box asks for it.
pub(crate) fn cells_of(area: &BoundingBox) -> [i32; 4] {
    [
        cell_index(area.south()),
        cell_index(area.north()),
        cell_index(area.west()),
        cell_index(area.east()),
    ]
}

/// West, south, east and north of a box that holds every point of the
/// cells given ([`cells_of`]), with a hair to spare for the rounding of
/// their edges.
pub(crate) fn box_of_cells([south, north, west, east]: [i32; 4]) -> [f64; 4] {
    const HAIR: f64 = 1e-9;
    let edge = |cell: i32| f64::from(cell) / CELLS_PER_DEGREE;
    [
        (edge(west) - HAIR).max(-180.0),
        (edge(south) - HAIR).max(-90.0),
        (edge(east + 1) + HAIR).min(180.0),
        (edge(north + 1) + HAIR).min(90.0),
    ]
}

/// West, south, east and north of a box that holds every place nearer to
/// a point than `within_m` metres, drawn generously: a degree of latitude
/// is taken as 100 km, which is less than it is anywhere, and a degree of
/// longitude as what it is at the edge of the box that is nearest a pole.
/// Near a pole, and across the line where east meets west, the box is the
/// whole band around the globe.
pub(crate) fn around(lat: f64, lon: f64, within_m: f64) -> [f64; 4] {
    let step = within_m / 100_000.0;
    let south = (lat - step).max(-90.0);
    let north = (lat + step).min(90.0);
    let narrowest = south.abs().max(north.abs()).to_radians().cos();
    let wide = if narrowest > 1e-6 {
        step / narrowest
    } else {
        f64::INFINITY
    };
    if wide.is_finite() && lon - wide >= -180.0 && lon + wide <= 180.0 {
        [lon - wide, south, lon + wide, north]
    } else {
        [-180.0, south, 180.0, north]
    }
}

/// The public flights that began in one cell of the grid, added up.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cell {
    /// The cell: [`cell_of`].
    pub(crate) at: (i32, i32),
    pub(crate) flights: u32,
    pub(crate) pilots: u32,
    /// The sum of the take-offs' latitudes and longitudes, each as
    /// [`e7`] has it.
    pub(crate) lat_e7: i64,
    pub(crate) lon_e7: i64,
    /// The sum of the heights the flights took off at.
    pub(crate) altitude_m: i64,
    pub(crate) latest_day: NaiveDate,
    pub(crate) by_month: [u32; 12],
}

impl Cell {
    /// The middle of the take-offs, as exactly as it is known: what the
    /// distance to a site is measured from. Never handed out.
    fn middle(&self) -> Option<GeoPoint> {
        let flights = f64::from(self.flights);
        #[allow(clippy::cast_precision_loss)]
        let mean = |sum: i64| sum as f64 / flights / 10_000_000.0;
        GeoPoint::new(mean(self.lat_e7), mean(self.lon_e7)).ok()
    }
}

/// Public flights as they are added up, one after another: what the
/// in-memory store counts with. The database counts the same in SQL.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    flights: u32,
    pilots: BTreeSet<String>,
    lat_e7: i64,
    lon_e7: i64,
    altitude_m: i64,
    latest_day: Option<NaiveDate>,
    by_month: [u32; 12],
}

impl Tally {
    /// Counts a flight of an account that started at `start` and took off
    /// at `takeoff`.
    pub(crate) fn add(&mut self, account_id: &str, start: Timestamp, takeoff: &Place) {
        let day = solar_day(start, takeoff.position.lon());
        self.flights += 1;
        self.pilots.insert(account_id.to_owned());
        self.lat_e7 += e7(takeoff.position.lat());
        self.lon_e7 += e7(takeoff.position.lon());
        self.altitude_m += i64::from(takeoff.altitude_m);
        self.latest_day = self.latest_day.max(Some(day));
        self.by_month[day.month0() as usize] += 1;
    }

    fn pilots(&self) -> u32 {
        u32::try_from(self.pilots.len()).unwrap_or(u32::MAX)
    }

    /// What the flights counted say about the place they began at, with
    /// the best of them.
    pub(crate) fn at_a_place(self, best: Vec<Flight>) -> FlightsFrom {
        FlightsFrom {
            flights: self.flights,
            pilots: self.pilots(),
            latest_day: self.latest_day,
            by_month: self.by_month,
            best,
        }
    }

    /// The flights counted as those of one cell; `None` without a flight.
    pub(crate) fn of_cell(self, at: (i32, i32)) -> Option<Cell> {
        Some(Cell {
            at,
            flights: self.flights,
            pilots: self.pilots(),
            lat_e7: self.lat_e7,
            lon_e7: self.lon_e7,
            altitude_m: self.altitude_m,
            latest_day: self.latest_day?,
            by_month: self.by_month,
        })
    }
}

/// West, south, east and north of a box that holds every site that can
/// stand within `clear_m` of the middle of one of `cells`; `None` without
/// a cell. What a store reads its sites in before it asks [`candidates`].
pub(crate) fn reach_of(cells: &[Cell], clear_m: f64) -> Option<[f64; 4]> {
    let mut all: Option<[f64; 4]> = None;
    for middle in cells.iter().filter_map(Cell::middle) {
        let [west, south, east, north] = around(middle.lat(), middle.lon(), clear_m.max(0.0));
        all = Some(match all {
            None => [west, south, east, north],
            Some([w, s, e, n]) => [w.min(west), s.min(south), e.max(east), n.max(north)],
        });
    }
    all
}

fn rounded(degrees: f64) -> f64 {
    (degrees * CANDIDATE_STEPS_PER_DEGREE).round() / CANDIDATE_STEPS_PER_DEGREE
}

/// The candidates among cells of public flights: those in which at least
/// `rule.min_pilots` pilots took off, and from whose middle no site of
/// `sites` stands within `rule.clear_m`. The place with the most pilots
/// comes first, then the one with the most flights, then by cell; at most
/// `limit` are returned.
///
/// **At a cell's edge.** The grid is fixed, and an edge of it may run
/// through a take-off. Its flights are then counted in two cells, or
/// four, each by itself: the take-off is offered twice, some hundred
/// metres apart, or not at all where no part reaches the number of pilots
/// alone. The grid never makes a candidate that is none: every one has
/// that many pilots within a cell.
pub(crate) fn candidates(
    mut cells: Vec<Cell>,
    sites: &[GeoPoint],
    rule: &CandidateRule,
    limit: usize,
) -> Vec<TakeoffCandidate> {
    cells.sort_by(|a, b| {
        b.pilots
            .cmp(&a.pilots)
            .then_with(|| b.flights.cmp(&a.flights))
            .then_with(|| a.at.cmp(&b.at))
    });
    // A degree of latitude is more than 100 km everywhere: a site further
    // north or south than this is not within the distance.
    let band = rule.clear_m.max(0.0) / 100_000.0;
    cells
        .into_iter()
        .filter(|cell| cell.flights > 0 && cell.pilots >= rule.min_pilots)
        .filter_map(|cell| {
            let middle = cell.middle()?;
            let in_the_way = sites.iter().any(|site| {
                (site.lat() - middle.lat()).abs() <= band
                    && middle.distance_m(*site) <= rule.clear_m
            });
            if in_the_way {
                return None;
            }
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let altitude_m = {
                let mean = cell.altitude_m as f64 / f64::from(cell.flights);
                let step = f64::from(CANDIDATE_ALTITUDE_STEP_M);
                (mean / step).round() as i32 * CANDIDATE_ALTITUDE_STEP_M
            };
            Some(TakeoffCandidate {
                position: GeoPoint::new(rounded(middle.lat()), rounded(middle.lon())).ok()?,
                altitude_m,
                flights: cell.flights,
                pilots: cell.pilots,
                latest_day: cell.latest_day,
                by_month: cell.by_month,
            })
        })
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn cell(at: (i32, i32), pilots: u32, flights: u32, lat: f64, lon: f64) -> Cell {
        Cell {
            at,
            flights,
            pilots,
            lat_e7: e7(lat) * i64::from(flights),
            lon_e7: e7(lon) * i64::from(flights),
            altitude_m: 1234 * i64::from(flights),
            latest_day: NaiveDate::from_ymd_opt(2026, 7, 4).unwrap(),
            by_month: [0; 12],
        }
    }

    const RULE: CandidateRule = CandidateRule {
        min_pilots: 3,
        clear_m: 500.0,
    };

    #[test]
    fn the_day_of_a_flight_is_the_day_at_the_place() {
        // Nine in the morning in eastern Australia is the evening before
        // in UTC.
        let start = Utc.with_ymd_and_hms(2026, 1, 31, 23, 0, 0).unwrap();
        assert_eq!(
            solar_day(start, 150.0),
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()
        );
        // And an afternoon in California is the next day in UTC.
        let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 30, 0).unwrap();
        assert_eq!(
            solar_day(start, -120.0),
            NaiveDate::from_ymd_opt(2026, 6, 30).unwrap()
        );
        // In the Alps the two are the same day.
        let start = Utc.with_ymd_and_hms(2026, 7, 4, 9, 30, 0).unwrap();
        assert_eq!(solar_day(start, 11.8), start.date_naive());
    }

    #[test]
    fn a_cell_is_a_four_hundredth_of_a_degree_each_way() {
        let at = |lat, lon| cell_of(GeoPoint::new(lat, lon).unwrap());
        assert_eq!(at(47.6642, 11.7967), (19065, 4718));
        assert_eq!(at(47.6649, 11.7974), (19065, 4718));
        assert_eq!(at(47.6651, 11.7976), (19066, 4719));
        // South of the equator and west of Greenwich the numbers go on
        // downwards, with no cell twice as large around zero.
        assert_eq!(at(-0.001, -0.001), (-1, -1));
        assert_eq!(at(0.001, 0.001), (0, 0));

        let area = BoundingBox::new(47.6642, 11.7967, 47.6651, 11.7976).unwrap();
        assert_eq!(cells_of(&area), [19065, 19066, 4718, 4719]);
        let [west, south, east, north] = box_of_cells(cells_of(&area));
        assert!(west < 11.795 + 1e-12 && east > 11.8 - 1e-12);
        assert!(south < 47.6625 + 1e-12 && north > 47.6675 - 1e-12);
    }

    #[test]
    fn a_candidate_has_enough_pilots_and_no_site_near() {
        let here = cell((19065, 4718), 3, 4, 47.664_23, 11.796_71);
        let offered = candidates(vec![here.clone()], &[], &RULE, 10);
        assert_eq!(offered.len(), 1);
        // Rounded: the place, and no fix of anyone's.
        assert_eq!(
            offered[0].position,
            GeoPoint::new(47.6642, 11.7967).unwrap()
        );
        assert_eq!(offered[0].altitude_m, 1230);
        assert_eq!((offered[0].flights, offered[0].pilots), (4, 3));

        // Two pilots are not enough, however often they flew.
        let two = cell((19065, 4718), 2, 40, 47.6642, 11.7967);
        assert!(candidates(vec![two], &[], &RULE, 10).is_empty());

        // A site 400 m to the north is in the way, one 600 m away is not.
        let near = GeoPoint::new(47.6678, 11.7967).unwrap();
        let far = GeoPoint::new(47.6696, 11.7967).unwrap();
        assert!(candidates(vec![here.clone()], &[far, near], &RULE, 10).is_empty());
        assert_eq!(candidates(vec![here], &[far], &RULE, 10).len(), 1);
    }

    #[test]
    fn candidates_come_by_pilots_then_flights_then_cell() {
        let cells = vec![
            cell((1, 1), 3, 9, 0.003, 0.003),
            cell((40, 40), 5, 5, 0.101, 0.101),
            cell((80, 80), 3, 9, 0.201, 0.201),
            cell((120, 120), 3, 12, 0.301, 0.301),
        ];
        let order: Vec<u32> = candidates(cells.clone(), &[], &RULE, 10)
            .iter()
            .map(|candidate| candidate.flights)
            .collect();
        assert_eq!(order, [5, 12, 9, 9]);
        let first = candidates(cells, &[], &RULE, 3);
        assert_eq!(first.len(), 3);
        assert_eq!(first[2].position, GeoPoint::new(0.003, 0.003).unwrap());
    }

    #[test]
    fn sites_are_read_where_one_could_be_in_the_way() {
        assert_eq!(reach_of(&[], 500.0), None);
        let cells = [
            cell((19065, 4718), 3, 3, 47.6642, 11.7967),
            cell((19100, 4800), 3, 3, 47.7512, 12.0012),
        ];
        let [west, south, east, north] = reach_of(&cells, 500.0).unwrap();
        // 500 m is 0.0045 degrees of latitude and 0.0067 of longitude
        // here; the box is drawn wider than that.
        assert!(south < 47.6642 - 0.0045 && north > 47.7512 + 0.0045);
        assert!(west < 11.7967 - 0.0067 && east > 12.0012 + 0.0067);
        assert!(south > 47.65 && north < 47.76 && west > 11.78 && east < 12.02);
    }
}
