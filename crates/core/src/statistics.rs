//! The logbook looked at as a whole: how much the pilot flew, on what, and
//! their best flights.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike};

use venti_domain::GeoPoint;

use crate::analysis::RouteKind;
use crate::{Coordinate, Core, logbook};

/// How many months the monthly figures go back, counting the month of the
/// newest flight.
const MONTHS: i32 = 12;

/// Flights that began closer together than this began at the same place,
/// metres. The starts of one take-off lie tens of metres apart, the
/// take-offs of one mountain often some hundred: this keeps those apart
/// and a meadow together.
const SAME_PLACE_M: f64 = 300.0;

/// One calendar month of flying (UTC).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct MonthStatistics {
    /// The year.
    pub year: i32,
    /// The month, 1 to 12.
    pub month: u32,
    /// Flights that month.
    pub flights: u32,
    /// Time in the air, seconds.
    pub airtime_s: i64,
    /// Scored cross-country distance, metres.
    pub distance_m: f64,
}

/// The flying done on one glider.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GliderStatistics {
    /// The glider as the flight files name it. Absent for flights whose
    /// files name none.
    pub glider: Option<String>,
    /// Flights on it.
    pub flights: u32,
    /// Time in the air on it, seconds.
    pub airtime_s: i64,
    /// Scored cross-country distance on it, metres.
    pub distance_m: f64,
}

/// One calendar year of flying (UTC).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct YearStatistics {
    /// The year.
    pub year: i32,
    /// Flights that year.
    pub flights: u32,
    /// Time in the air, seconds.
    pub airtime_s: i64,
    /// Scored cross-country distance, metres.
    pub distance_m: f64,
    /// Cross-country points, by this app's own calculation.
    pub points: f64,
    /// The longest flight of the year, seconds.
    pub longest_s: i64,
    /// The longest scored distance of the year, metres.
    pub farthest_m: f64,
    /// The highest point reached that year, metres.
    pub highest_m: i32,
}

/// One month of the calendar over all the years flown: when in the year
/// the pilot flies.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SeasonStatistics {
    /// The month, 1 to 12.
    pub month: u32,
    /// Flights in that month, all years together.
    pub flights: u32,
    /// Time in the air in that month, all years together, seconds.
    pub airtime_s: i64,
    /// Scored cross-country distance in that month, metres.
    pub distance_m: f64,
}

/// The flights that score as one kind of route.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RouteStatistics {
    /// What the flights score as.
    pub kind: RouteKind,
    /// How many flights do.
    pub flights: u32,
    /// How many of them came back to where they began.
    pub closed: u32,
    /// Their scored distance together, metres.
    pub distance_m: f64,
    /// The one with the longest scored distance, in metres.
    pub farthest: Option<BestFlight>,
}

/// What a flight of this pilot is usually like: the middle one of all, so
/// that one long day does not speak for the rest.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TypicalFlight {
    /// The median time in the air, seconds.
    pub airtime_s: i64,
    /// The median scored distance of the flights that have a score, metres.
    pub distance_m: Option<f64>,
    /// The median of the flights' highest points, metres.
    pub highest_m: i32,
    /// The median of the flights' best half-minute climbs, metres per second.
    pub best_climb_ms: f64,
}

/// One of the pilot's bests: which flight, and the figure.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct BestFlight {
    /// The flight's identifier, to open it.
    pub flight_id: String,
    /// When the flight started, seconds since 1970-01-01 UTC.
    pub start_unix_s: i64,
    /// The figure, in the unit of what it measures: seconds, metres,
    /// metres per second or points.
    pub value: f64,
}

/// The flights that began at one place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteStatistics {
    /// What the pilot calls the place: the name of their waypoint there,
    /// a take-off before any other. `None` where they have none; the
    /// place is then known by where it is.
    pub name: Option<String>,
    /// Where the earliest of the flights began, degrees of latitude.
    pub lat: f64,
    /// And of longitude.
    pub lon: f64,
    /// Flights from there.
    pub flights: u32,
    /// Time in the air on them, seconds.
    pub airtime_s: i64,
    /// Scored cross-country distance, metres.
    pub distance_m: f64,
    /// The longest of them, seconds.
    pub longest_s: i64,
    /// The farthest of them by scored distance. `None` when none was
    /// scored.
    pub farthest: Option<BestFlight>,
    /// When the earliest of them started, seconds since 1970-01-01 UTC.
    pub first_unix_s: i64,
    /// When the latest started.
    pub last_unix_s: i64,
}

/// The logbook looked at as a whole.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LogbookStatistics {
    /// The twelve months up to the newest flight, oldest first, months
    /// without flights included.
    pub months: Vec<MonthStatistics>,
    /// The gliders flown, the most flown first.
    pub gliders: Vec<GliderStatistics>,
    /// Every year with a flight, the newest first, for one year beside
    /// another.
    pub years: Vec<YearStatistics>,
    /// January to December over all the years flown: twelve entries once
    /// there is a flight, none before.
    pub seasons: Vec<SeasonStatistics>,
    /// The flights by what they score as, in the order free flight,
    /// triangle, FAI triangle; a kind no flight scores as is left out.
    pub routes: Vec<RouteStatistics>,
    /// What a flight is usually like. Absent for an empty logbook.
    pub typical: Option<TypicalFlight>,
    /// The longest flight, in seconds.
    pub longest: Option<BestFlight>,
    /// The flight with the longest scored distance, in metres.
    pub farthest: Option<BestFlight>,
    /// The flight with the most cross-country points.
    pub most_points: Option<BestFlight>,
    /// The flight that went highest, in metres.
    pub highest: Option<BestFlight>,
    /// The flight with the best half-minute climb, in metres per second.
    pub best_climb: Option<BestFlight>,
}

#[uniffi::export]
impl Core {
    /// Statistics over the whole logbook.
    pub fn statistics(&self) -> LogbookStatistics {
        let flights = self.logbook.flights();

        // Months are counted from year zero so that going back twelve is
        // plain subtraction.
        let month_number = |unix_s: i64| {
            DateTime::from_timestamp(unix_s, 0).map(|time| time.year() * 12 + time.month0() as i32)
        };
        let newest_month = flights
            .iter()
            .filter_map(|(_, summary)| month_number(summary.start_unix_s))
            .max();
        let mut months: Vec<MonthStatistics> = newest_month
            .map(|newest| {
                (newest - MONTHS + 1..=newest)
                    .map(|number| MonthStatistics {
                        year: number.div_euclid(12),
                        month: number.rem_euclid(12) as u32 + 1,
                        flights: 0,
                        airtime_s: 0,
                        distance_m: 0.0,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut gliders: BTreeMap<Option<String>, GliderStatistics> = BTreeMap::new();
        let mut bests: [Option<BestFlight>; 5] = Default::default();
        let mut years: BTreeMap<i32, YearStatistics> = BTreeMap::new();
        let mut seasons: Vec<SeasonStatistics> = if flights.is_empty() {
            Vec::new()
        } else {
            (1..=12)
                .map(|month| SeasonStatistics {
                    month,
                    flights: 0,
                    airtime_s: 0,
                    distance_m: 0.0,
                })
                .collect()
        };
        let mut routes: [Option<RouteStatistics>; 3] = Default::default();
        for (id, summary) in &flights {
            let distance_m = summary.score.map_or(0.0, |score| score.distance_m);

            if let Some(time) = DateTime::from_timestamp(summary.start_unix_s, 0) {
                let year = years.entry(time.year()).or_insert(YearStatistics {
                    year: time.year(),
                    flights: 0,
                    airtime_s: 0,
                    distance_m: 0.0,
                    points: 0.0,
                    longest_s: 0,
                    farthest_m: 0.0,
                    highest_m: summary.max_altitude_m,
                });
                year.flights += 1;
                year.airtime_s += summary.duration_s;
                year.distance_m += distance_m;
                year.points += summary.score.map_or(0.0, |score| score.points);
                year.longest_s = year.longest_s.max(summary.duration_s);
                year.farthest_m = year.farthest_m.max(distance_m);
                year.highest_m = year.highest_m.max(summary.max_altitude_m);
                if let Some(season) = seasons.get_mut(time.month0() as usize) {
                    season.flights += 1;
                    season.airtime_s += summary.duration_s;
                    season.distance_m += distance_m;
                }
            }

            if let Some(score) = summary.score {
                let (place, kind) = match score.kind {
                    logbook::StoredKind::FreeFlight => (0, RouteKind::FreeFlight),
                    logbook::StoredKind::FreeTriangle => (1, RouteKind::FreeTriangle),
                    logbook::StoredKind::FaiTriangle => (2, RouteKind::FaiTriangle),
                };
                let route = routes[place].get_or_insert(RouteStatistics {
                    kind,
                    flights: 0,
                    closed: 0,
                    distance_m: 0.0,
                    farthest: None,
                });
                route.flights += 1;
                route.closed += u32::from(score.closed);
                route.distance_m += score.distance_m;
                if route
                    .farthest
                    .as_ref()
                    .is_none_or(|best| score.distance_m > best.value)
                {
                    route.farthest = Some(BestFlight {
                        flight_id: id.clone(),
                        start_unix_s: summary.start_unix_s,
                        value: score.distance_m,
                    });
                }
            }

            if let (Some(newest), Some(number)) = (newest_month, month_number(summary.start_unix_s))
                && let Some(month) = usize::try_from(number - (newest - MONTHS + 1))
                    .ok()
                    .and_then(|index| months.get_mut(index))
            {
                month.flights += 1;
                month.airtime_s += summary.duration_s;
                month.distance_m += distance_m;
            }

            let glider = summary
                .glider
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned);
            let entry = gliders
                .entry(glider.clone())
                .or_insert_with(|| GliderStatistics {
                    glider,
                    flights: 0,
                    airtime_s: 0,
                    distance_m: 0.0,
                });
            entry.flights += 1;
            entry.airtime_s += summary.duration_s;
            entry.distance_m += distance_m;

            let figures = [
                Some(summary.duration_s as f64),
                summary.score.map(|score| score.distance_m),
                summary.score.map(|score| score.points),
                Some(f64::from(summary.max_altitude_m)),
                Some(summary.max_climb_ms),
            ];
            for (best, figure) in bests.iter_mut().zip(figures) {
                if let Some(value) = figure
                    && best.as_ref().is_none_or(|best| value > best.value)
                {
                    *best = Some(BestFlight {
                        flight_id: id.clone(),
                        start_unix_s: summary.start_unix_s,
                        value,
                    });
                }
            }
        }

        let mut gliders: Vec<GliderStatistics> = gliders.into_values().collect();
        gliders.sort_by(|a, b| (b.airtime_s, &a.glider).cmp(&(a.airtime_s, &b.glider)));
        let [longest, farthest, most_points, highest, best_climb] = bests;

        // The middle one of each figure: of an even number, the lower of
        // the two in the middle, which is a flight that was flown.
        let typical = (!flights.is_empty()).then(|| {
            let mut airtimes: Vec<i64> = flights
                .iter()
                .map(|(_, flight)| flight.duration_s)
                .collect();
            let mut heights: Vec<i32> = flights
                .iter()
                .map(|(_, flight)| flight.max_altitude_m)
                .collect();
            let mut climbs: Vec<f64> = flights
                .iter()
                .map(|(_, flight)| flight.max_climb_ms)
                .collect();
            let mut distances: Vec<f64> = flights
                .iter()
                .filter_map(|(_, flight)| flight.score.map(|score| score.distance_m))
                .collect();
            airtimes.sort_unstable();
            heights.sort_unstable();
            climbs.sort_by(f64::total_cmp);
            distances.sort_by(f64::total_cmp);
            let middle = (flights.len() - 1) / 2;
            TypicalFlight {
                airtime_s: airtimes[middle],
                distance_m: (!distances.is_empty()).then(|| distances[(distances.len() - 1) / 2]),
                highest_m: heights[middle],
                best_climb_ms: climbs[middle],
            }
        });

        LogbookStatistics {
            months,
            gliders,
            years: years.into_values().rev().collect(),
            seasons,
            routes: routes.into_iter().flatten().collect(),
            typical,
            longest,
            farthest,
            most_points,
            highest,
            best_climb,
        }
    }
}

#[uniffi::export]
impl Core {
    /// The logbook by where its flights began: one entry for each place,
    /// the one flown from most often first.
    ///
    /// A place is where flights began within [`SAME_PLACE_M`] of the
    /// earliest of them. Places that carry the same name of the pilot's
    /// waypoints are one: two starts of one mountain that the pilot calls
    /// by one name. A flight whose track has no position is left out.
    pub fn site_statistics(&self) -> Vec<SiteStatistics> {
        // Oldest first, so that a place is where its first flight began.
        let mut flights = self.logbook.flights();
        flights.reverse();

        let mut places: Vec<(GeoPoint, SiteStatistics)> = Vec::new();
        for (id, summary) in &flights {
            let Some(began) = summary
                .outline
                .first()
                .and_then(|&[lat, lon]| GeoPoint::new(lat, lon).ok())
            else {
                continue;
            };
            let nearest = places
                .iter()
                .enumerate()
                .map(|(index, (anchor, _))| (anchor.distance_m(began), index))
                .filter(|(distance, _)| *distance <= SAME_PLACE_M)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, index)| index);
            let index = nearest.unwrap_or_else(|| {
                places.push((
                    began,
                    SiteStatistics {
                        name: None,
                        lat: began.lat(),
                        lon: began.lon(),
                        flights: 0,
                        airtime_s: 0,
                        distance_m: 0.0,
                        longest_s: 0,
                        farthest: None,
                        first_unix_s: summary.start_unix_s,
                        last_unix_s: summary.start_unix_s,
                    },
                ));
                places.len() - 1
            });
            let site = &mut places[index].1;
            site.flights += 1;
            site.airtime_s += summary.duration_s;
            site.longest_s = site.longest_s.max(summary.duration_s);
            site.last_unix_s = site.last_unix_s.max(summary.start_unix_s);
            if let Some(score) = summary.score {
                site.distance_m += score.distance_m;
                if site
                    .farthest
                    .as_ref()
                    .is_none_or(|best| score.distance_m > best.value)
                {
                    site.farthest = Some(BestFlight {
                        flight_id: id.clone(),
                        start_unix_s: summary.start_unix_s,
                        value: score.distance_m,
                    });
                }
            }
        }

        // The pilot's names, and places of one name as one.
        let mut sites: Vec<SiteStatistics> = Vec::new();
        for (_, mut site) in places {
            site.name = self.place_name(Coordinate {
                lat: site.lat,
                lon: site.lon,
            });
            match sites
                .iter_mut()
                .find(|other| site.name.is_some() && other.name == site.name)
            {
                Some(other) => {
                    other.flights += site.flights;
                    other.airtime_s += site.airtime_s;
                    other.distance_m += site.distance_m;
                    other.longest_s = other.longest_s.max(site.longest_s);
                    other.last_unix_s = other.last_unix_s.max(site.last_unix_s);
                    if site.farthest.as_ref().is_some_and(|best| {
                        other
                            .farthest
                            .as_ref()
                            .is_none_or(|have| best.value > have.value)
                    }) {
                        other.farthest = site.farthest;
                    }
                }
                None => sites.push(site),
            }
        }
        // Most flights first, then most airtime, then the place flown
        // from first.
        sites.sort_by(|a, b| {
            (b.flights, b.airtime_s, a.first_unix_s).cmp(&(a.flights, a.airtime_s, b.first_unix_s))
        });
        sites
    }
}

#[cfg(test)]
mod tests {
    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    #[test]
    fn an_empty_logbook_has_no_statistics() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        let statistics = core.statistics();

        assert!(statistics.months.is_empty());
        assert!(statistics.gliders.is_empty());
        assert_eq!(statistics.longest, None);
        assert!(statistics.years.is_empty());
        assert!(statistics.seasons.is_empty());
        assert!(statistics.routes.is_empty());
        assert_eq!(statistics.typical, None);
    }

    #[test]
    fn years_seasons_routes_and_the_typical_flight_are_counted() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        // Two flights in July 2026, one in September and one in June 2025.
        for day in ["040726", "030726", "040925", "040625"] {
            core.import_flight(sample().replace("040726", day)).unwrap();
        }

        let statistics = core.statistics();

        // One year beside another, the newest first.
        let years: Vec<(i32, u32, i64)> = statistics
            .years
            .iter()
            .map(|year| (year.year, year.flights, year.airtime_s))
            .collect();
        assert_eq!(years, [(2026, 2, 2 * 1744), (2025, 2, 2 * 1744)]);
        let year = statistics.years[0];
        assert_eq!(year.longest_s, 1744);
        assert_eq!(year.highest_m, 2117);
        assert!(year.farthest_m > 10_000.0);
        assert!((year.distance_m - 2.0 * year.farthest_m).abs() < 1e-6);
        assert!(year.points > 20.0);

        // When in the year: twelve months, whatever the years.
        assert_eq!(statistics.seasons.len(), 12);
        let flown: Vec<(u32, u32)> = statistics
            .seasons
            .iter()
            .filter(|season| season.flights > 0)
            .map(|season| (season.month, season.flights))
            .collect();
        assert_eq!(flown, [(6, 1), (7, 2), (9, 1)]);
        assert_eq!(statistics.seasons[6].airtime_s, 2 * 1744);

        // The sample scores as a free flight, all four times.
        assert_eq!(statistics.routes.len(), 1);
        let route = &statistics.routes[0];
        assert_eq!(route.kind, RouteKind::FreeFlight);
        assert_eq!((route.flights, route.closed), (4, 0));
        assert!((route.distance_m - 4.0 * year.farthest_m).abs() < 1e-6);
        assert!((route.farthest.as_ref().unwrap().value - year.farthest_m).abs() < 1e-9);

        // Four flights that are the same flight: the middle one is that flight.
        let typical = statistics.typical.unwrap();
        assert_eq!(typical.airtime_s, 1744);
        assert_eq!(typical.highest_m, 2117);
        assert!((typical.distance_m.unwrap() - year.farthest_m).abs() < 1e-9);
        assert!((typical.best_climb_ms - 2.4).abs() < 0.05);
    }

    #[test]
    fn months_gliders_and_bests_are_counted() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        // 4 July 2026, 3 July 2026 on another glider, and 4 September 2025.
        let newest = core.import_flight(sample()).unwrap().id;
        core.import_flight(
            sample()
                .replace("040726", "030726")
                .replace("Sample Wing", "Other Wing"),
        )
        .unwrap();
        core.import_flight(sample().replace("040726", "040925"))
            .unwrap();
        // More than twelve months before the newest: in the totals, not in
        // the months.
        core.import_flight(sample().replace("040726", "040625"))
            .unwrap();

        let statistics = core.statistics();

        assert_eq!(statistics.months.len(), 12);
        let first = statistics.months[0];
        assert_eq!((first.year, first.month), (2025, 8));
        let september = statistics.months[1];
        assert_eq!(
            (september.year, september.month, september.flights),
            (2025, 9, 1)
        );
        let july = statistics.months[11];
        assert_eq!((july.year, july.month, july.flights), (2026, 7, 2));
        assert_eq!(july.airtime_s, 2 * 1744);
        assert!(july.distance_m > 20_000.0);
        assert_eq!(
            statistics
                .months
                .iter()
                .map(|month| month.flights)
                .sum::<u32>(),
            3
        );

        let gliders: Vec<(Option<&str>, u32)> = statistics
            .gliders
            .iter()
            .map(|glider| (glider.glider.as_deref(), glider.flights))
            .collect();
        assert_eq!(gliders, [(Some("Sample Wing"), 3), (Some("Other Wing"), 1)]);

        // All four are the same flight on different days: the first one
        // found holds every record, and the figures are the flight's.
        let longest = statistics.longest.unwrap();
        assert_eq!(longest.value, 1744.0);
        assert_eq!(longest.flight_id, newest);
        assert_eq!(statistics.highest.unwrap().value, 2117.0);
        assert!(statistics.farthest.unwrap().value > 10_000.0);
        assert!(statistics.most_points.unwrap().value > 10.0);
        assert!((statistics.best_climb.unwrap().value - 2.4).abs() < 0.05);
    }

    #[test]
    fn the_logbook_shows_each_flights_score_and_adds_the_distances_up() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        core.import_flight(sample()).unwrap();
        core.import_flight(sample().replace("040726", "030726"))
            .unwrap();

        let logbook = core.logbook();
        let score = logbook.years[0].flights[0].score.unwrap();
        assert!(score.distance_m > 10_000.0);
        assert!((logbook.totals.distance_m - 2.0 * score.distance_m).abs() < 1e-6);
        assert!((logbook.totals.points - 2.0 * score.points).abs() < 1e-6);
    }

    /// The sample flight, begun `north_m` north of where it begins.
    fn sample_from(north_m: f64, day: &str) -> String {
        let mut file = synthetic_flight();
        for fix in &mut file.fixes {
            fix.position =
                GeoPoint::new(fix.position.lat() + north_m / 111_195.0, fix.position.lon())
                    .unwrap();
        }
        igc::write(&file).replace("040726", day)
    }

    #[test]
    fn an_empty_logbook_has_flown_from_nowhere() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        assert!(core.site_statistics().is_empty());
    }

    #[test]
    fn flights_are_counted_by_where_they_began() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        // Two from the take-off and one from 120 m beside it, which is the
        // same place; one from 5 km away, flown first.
        core.import_flight(sample_from(5000.0, "010625")).unwrap();
        core.import_flight(sample_from(0.0, "040726")).unwrap();
        core.import_flight(sample_from(0.0, "030726")).unwrap();
        core.import_flight(sample_from(120.0, "040925")).unwrap();

        let sites = core.site_statistics();

        assert_eq!(sites.len(), 2);
        // The place flown from most often comes first.
        let home = &sites[0];
        assert_eq!(home.flights, 3);
        assert_eq!(home.airtime_s, 3 * 1744);
        assert_eq!(home.longest_s, 1744);
        assert_eq!(home.name, None);
        // It is where the earliest of its flights began: the one beside
        // the take-off, in September 2025.
        assert!((home.lat - (47.6642 + 120.0 / 111_195.0)).abs() < 1e-4);
        assert!(home.first_unix_s < home.last_unix_s);
        let farthest = home.farthest.as_ref().unwrap();
        assert!(farthest.value > 10_000.0);
        // Three flights of the same shape: a flight file keeps positions
        // to a couple of metres, so the three distances differ by that.
        assert!((home.distance_m - 3.0 * farthest.value).abs() < 30.0);

        let away = &sites[1];
        assert_eq!(away.flights, 1);
        assert_eq!(away.first_unix_s, away.last_unix_s);
        assert!((away.lat - home.lat).abs() > 0.04);
    }

    #[test]
    fn a_place_carries_the_pilots_name_for_it_and_places_of_one_name_are_one() {
        let folder = TempFolder::new();
        let core = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        // Two starts of one mountain, 500 m apart: two places by distance.
        core.import_flight(sample_from(0.0, "040726")).unwrap();
        core.import_flight(sample_from(500.0, "030726")).unwrap();
        assert_eq!(core.site_statistics().len(), 2);

        // The pilot has one take-off waypoint for the mountain, within a
        // kilometre of both: one place, by its name.
        core.import_waypoints(
            "name,code,country,lat,lon,elev,style,rwdir,rwlen,rwwidth,freq,desc\n\
             \"Wallberg\",WB,DE,4739.800N,01147.600E,1620m,20,,,,,\n"
                .to_owned(),
        )
        .unwrap();
        let sites = core.site_statistics();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].name.as_deref(), Some("Wallberg"));
        assert_eq!(sites[0].flights, 2);
        assert_eq!(sites[0].airtime_s, 2 * 1744);
    }
}
