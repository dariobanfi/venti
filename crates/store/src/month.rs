use std::fmt;

use chrono::{Datelike, TimeZone, Utc};
use venti_domain::Timestamp;

/// A calendar month in UTC.
///
/// Observations are kept, and removed, by the month: it is the unit of the
/// database's partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month {
    year: i32,
    /// 1 to 12.
    month: u32,
}

impl Month {
    /// A month from its year and its number, 1 to 12.
    pub fn new(year: i32, month: u32) -> Option<Self> {
        (1..=12).contains(&month).then_some(Self { year, month })
    }

    /// The month a moment falls in.
    pub fn of(time: Timestamp) -> Self {
        Self {
            year: time.year(),
            month: time.month(),
        }
    }

    /// The year.
    pub fn year(&self) -> i32 {
        self.year
    }

    /// The month of the year, 1 to 12.
    pub fn month(&self) -> u32 {
        self.month
    }

    /// The first instant of the month.
    ///
    /// # Panics
    ///
    /// Panics for a year chrono cannot represent, which no month made by
    /// [`Month::of`] or [`Month::back`] has.
    pub fn start(&self) -> Timestamp {
        Utc.with_ymd_and_hms(self.year, self.month, 1, 0, 0, 0)
            .single()
            .expect("the first of a month at midnight UTC exists")
    }

    /// The month after this one.
    pub fn next(&self) -> Month {
        if self.month == 12 {
            Month {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Month {
                year: self.year,
                month: self.month + 1,
            }
        }
    }

    /// The month `months` before this one.
    pub fn back(&self, months: u32) -> Month {
        // Count months from year zero, step back, and split again.
        let index = i64::from(self.year) * 12 + i64::from(self.month - 1) - i64::from(months);
        Month {
            year: i32::try_from(index.div_euclid(12)).unwrap_or(i32::MIN),
            month: u32::try_from(index.rem_euclid(12)).unwrap_or(0) + 1,
        }
    }
}

impl fmt::Display for Month {
    /// Written as `2026-11`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i32, month: u32, day: u32, hour: u32) -> Timestamp {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    #[test]
    fn a_month_starts_at_midnight_utc_on_the_first() {
        let november = Month::of(at(2026, 11, 18, 19));
        assert_eq!(november.to_string(), "2026-11");
        assert_eq!(november.start(), at(2026, 11, 1, 0));
        // The last instant before midnight still belongs to the old month.
        let last_hour = at(2026, 11, 30, 23);
        assert_eq!(Month::of(last_hour), november);
    }

    #[test]
    fn stepping_crosses_year_boundaries() {
        let december = Month::of(at(2026, 12, 5, 0));
        assert_eq!(december.next().to_string(), "2027-01");
        assert_eq!(december.back(0), december);
        assert_eq!(december.back(11).to_string(), "2026-01");
        assert_eq!(december.back(12).to_string(), "2025-12");
        assert_eq!(december.back(13).to_string(), "2025-11");
        assert_eq!(Month::of(at(2026, 1, 1, 0)).back(1).to_string(), "2025-12");
    }

    #[test]
    fn a_month_number_outside_the_year_is_refused() {
        assert_eq!(Month::new(2026, 11), Some(Month::of(at(2026, 11, 18, 19))));
        assert_eq!(Month::new(2026, 0), None);
        assert_eq!(Month::new(2026, 13), None);
    }

    #[test]
    fn months_order_by_time() {
        assert!(Month::of(at(2025, 12, 1, 0)) < Month::of(at(2026, 1, 1, 0)));
        assert!(Month::of(at(2026, 1, 1, 0)) < Month::of(at(2026, 2, 1, 0)));
    }
}
