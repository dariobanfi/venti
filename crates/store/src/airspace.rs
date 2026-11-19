//! Airspace as the store keeps it: one whole set per source.
//!
//! An airspace source delivers everything it has with every run, and that
//! set replaces the one before. The store never holds a mixture of two
//! runs, and never merges the airspace of two sources: each stays a layer
//! of its own, under its own licence.

use std::collections::BTreeSet;

use venti_domain::{Airspace, AirspaceId, BoundingBox, SourceId, Timestamp};

use crate::{RunRecord, StoreError};

/// Where a set of airspace is whole.
///
/// A run collects a source for an area and for the countries its adapter
/// fetches. Inside the area the set holds every airspace the source
/// publishes for those countries; of what lies outside it holds nothing,
/// or only what reaches in. Whoever hands airspace on must hand this on
/// with it: no airspace found is not "no airspace" where the set does not
/// reach.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Coverage {
    /// The boxes the run collected for.
    pub boxes: Vec<BoundingBox>,
    /// The countries whose airspace the run fetched, as ISO 3166-1 alpha-2
    /// codes. A box may reach into other countries; of those the set holds
    /// nothing.
    pub countries: Vec<String>,
}

/// What the store knows about the airspace it holds from one source.
#[derive(Debug, Clone, PartialEq)]
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
    /// Where the set is whole, as the run that delivered it said. Empty
    /// for a set stored before runs said so.
    pub coverage: Coverage,
}

impl AirspaceSet {
    /// Describes the set a run delivered.
    pub(crate) fn of(run: &RunRecord, coverage: &Coverage, airspaces: &[Airspace]) -> Self {
        let dates = |date: fn(&Airspace) -> Option<Timestamp>| airspaces.iter().filter_map(date);
        Self {
            source: run.source.clone(),
            fetched_at: run.started_at,
            airspaces: u32::try_from(airspaces.len()).unwrap_or(u32::MAX),
            file_date: dates(|airspace| airspace.validity.file_date).min(),
            newest_change: dates(|airspace| airspace.validity.updated).max(),
            coverage: coverage.clone(),
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
    /// The airspaces found, sorted by identifier. None, if nothing of the
    /// set is where the question asked.
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
