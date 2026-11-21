//! Cables as the store keeps them: one whole set per source, as for sites.
//!
//! A source of cables delivers everything it has with every run, and that
//! set replaces the one before. The store never merges the cables of two
//! sources, nor a source's cables with its sites: each stays a layer of
//! its own, under its own licence.

use std::collections::BTreeSet;

use venti_domain::{BoundingBox, Cable, SourceId, Timestamp};

use crate::{RunRecord, SiteDelivery, StoreError};

/// What the store knows about the cables it holds from one source.
#[derive(Debug, Clone, PartialEq)]
pub struct CableSet {
    /// The source.
    pub source: SourceId,
    /// When the run that delivered the set started.
    pub fetched_at: Timestamp,
    /// How many cables the set holds.
    pub cables: u32,
    /// The date the set is of, by the source's own word.
    pub as_of: Timestamp,
    /// Where the set is whole, as the run that delivered it said.
    pub covers: Vec<BoundingBox>,
}

impl CableSet {
    /// Describes the set a run delivered.
    pub(crate) fn of(run: &RunRecord, delivery: &SiteDelivery, cables: &[Cable]) -> Self {
        Self {
            source: run.source.clone(),
            fetched_at: run.started_at,
            cables: u32::try_from(cables.len()).unwrap_or(u32::MAX),
            as_of: delivery.as_of,
            covers: delivery.covers.clone(),
        }
    }
}

/// The cables of one source that a question found, with their set.
#[derive(Debug, Clone, PartialEq)]
pub struct CableLayer {
    /// The whole set the cables are part of.
    pub set: CableSet,
    /// The cables found, sorted by their key.
    pub cables: Vec<Cable>,
}

/// Checks what a cable run hands over. Both stores call this first.
pub(crate) fn check(run: &RunRecord, cables: &[Cable]) -> Result<(), StoreError> {
    let invalid = |problem: String| Err(StoreError::Invalid(problem));
    if !run.healthy {
        return Ok(());
    }
    if cables.is_empty() {
        return invalid(format!(
            "a healthy run of {} delivered no cable; an empty set must not replace a stored one",
            run.source
        ));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for cable in cables {
        if cable.source != run.source {
            return invalid(format!(
                "cable {} does not belong to the run's source {}",
                cable.local, run.source
            ));
        }
        if cable.bounds().is_none() {
            return invalid(format!("cable {} has no line", cable.local));
        }
        if !seen.insert(&cable.local) {
            return invalid(format!("cable {} is listed twice", cable.local));
        }
    }
    Ok(())
}
