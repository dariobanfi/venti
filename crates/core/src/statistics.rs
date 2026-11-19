//! The logbook looked at as a whole: how much the pilot flew, on what, and
//! their best flights.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike};

use crate::Core;

/// How many months the monthly figures go back, counting the month of the
/// newest flight.
const MONTHS: i32 = 12;

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
        for (id, summary) in &flights {
            let distance_m = summary.score.map_or(0.0, |score| score.distance_m);

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
        LogbookStatistics {
            months,
            gliders,
            longest,
            farthest,
            most_points,
            highest,
            best_climb,
        }
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
}
