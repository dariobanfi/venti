//! The rules a batch of records must satisfy before either store accepts it.
//!
//! Both implementations call [`check`] first, so they refuse the same input
//! in the same way.

use std::collections::BTreeSet;

use chrono::Datelike;
use venti_domain::{Observation, Station, StationId, Timestamp};

use crate::{Month, RunRecord, StoreError};

/// The most calendar months one batch of observations may touch.
///
/// A run covers a few hours. A batch spread over years is an adapter bug,
/// and each month it touches would become a database partition.
const MAX_MONTHS: usize = 24;

/// Checks a batch and returns the months its observations fall in.
pub(crate) fn check(
    run: &RunRecord,
    stations: &[Station],
    observations: &[Observation],
) -> Result<BTreeSet<Month>, StoreError> {
    let invalid = |problem: String| Err(StoreError::Invalid(problem));

    let mut known: BTreeSet<&StationId> = BTreeSet::new();
    for station in stations {
        if station.id.source() != &run.source {
            return invalid(format!(
                "station {} does not belong to the run's source {}",
                station.id, run.source
            ));
        }
        if !known.insert(&station.id) {
            return invalid(format!("station {} is listed twice", station.id));
        }
    }

    let mut seen: BTreeSet<(&StationId, Timestamp)> = BTreeSet::new();
    let mut months = BTreeSet::new();
    for observation in observations {
        if !known.contains(&observation.station) {
            return invalid(format!(
                "observation for {}, which is not among the run's stations",
                observation.station
            ));
        }
        if !seen.insert((&observation.station, observation.time)) {
            return invalid(format!(
                "two observations for {} at {}",
                observation.station, observation.time
            ));
        }
        if !(1..=9999).contains(&observation.time.year()) {
            return invalid(format!(
                "observation for {} in the year {}",
                observation.station,
                observation.time.year()
            ));
        }
        months.insert(Month::of(observation.time));
    }
    if months.len() > MAX_MONTHS {
        return invalid(format!(
            "observations spread over {} months; at most {MAX_MONTHS} are accepted in one run",
            months.len()
        ));
    }
    Ok(months)
}
