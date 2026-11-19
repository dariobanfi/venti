//! Airspace as the store keeps it: one whole set per source.
//!
//! An airspace source delivers everything it has with every run, and that
//! set replaces the one before. The store never holds a mixture of two
//! runs, and never merges the airspace of two sources: each stays a layer
//! of its own, under its own licence.

use std::collections::BTreeSet;

use venti_domain::{Airspace, AirspaceId, SourceId, Timestamp};

use crate::{RunRecord, StoreError};

/// What the store knows about the airspace it holds from one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirspaceSet {
    /// The source.
    pub source: SourceId,
    /// When the run that delivered the set started: the moment the server
    /// fetched it.
    pub fetched_at: Timestamp,
    /// How many airspaces the set holds.
    pub airspaces: u32,
    /// The date of the oldest file any of them was read from. `None` when
    /// no record says.
    pub file_date: Option<Timestamp>,
    /// When the source last changed any of them, if it says.
    pub newest_change: Option<Timestamp>,
}

impl AirspaceSet {
    /// Describes the set a run delivered.
    pub(crate) fn of(run: &RunRecord, airspaces: &[Airspace]) -> Self {
        let dates = |date: fn(&Airspace) -> Option<Timestamp>| airspaces.iter().filter_map(date);
        Self {
            source: run.source.clone(),
            fetched_at: run.started_at,
            airspaces: u32::try_from(airspaces.len()).unwrap_or(u32::MAX),
            file_date: dates(|airspace| airspace.validity.file_date).min(),
            newest_change: dates(|airspace| airspace.validity.updated).max(),
        }
    }
}

/// The airspaces of one source that a question found, with the set they
/// belong to.
#[derive(Debug, Clone, PartialEq)]
pub struct AirspaceLayer {
    /// The set the airspaces are part of. It describes the whole set the
    /// store holds, not only the part that was found.
    pub set: AirspaceSet,
    /// The airspaces found, sorted by identifier.
    pub airspaces: Vec<Airspace>,
}

/// Checks what an airspace run hands over. Both stores call this first, so
/// they refuse the same input in the same way.
pub(crate) fn check(run: &RunRecord, airspaces: &[Airspace]) -> Result<(), StoreError> {
    let invalid = |problem: String| Err(StoreError::Invalid(problem));
    if !run.healthy {
        return Ok(());
    }
    if airspaces.is_empty() {
        return invalid(format!(
            "a healthy run of {} delivered no airspace; an empty set must not replace a stored one",
            run.source
        ));
    }
    let mut seen: BTreeSet<&AirspaceId> = BTreeSet::new();
    for airspace in airspaces {
        if airspace.id.source() != &run.source {
            return invalid(format!(
                "airspace {} does not belong to the run's source {}",
                airspace.id, run.source
            ));
        }
        if !seen.insert(&airspace.id) {
            return invalid(format!("airspace {} is listed twice", airspace.id));
        }
    }
    Ok(())
}
