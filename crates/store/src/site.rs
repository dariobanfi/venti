//! Flying sites as the store keeps them: one whole set per source.
//!
//! A source of sites delivers everything it has with every run, and that
//! set replaces the one before, as for airspace. The store never holds a
//! mixture of two runs, and never merges the sites of two sources: each
//! stays a layer of its own, under its own licence.

use std::collections::BTreeSet;

use venti_domain::{BoundingBox, Site, SiteId, SourceId, Timestamp};

use crate::{RunRecord, StoreError};

/// What a run says about the set of sites it delivers.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteDelivery {
    /// The date the set is of: when the source's data was taken.
    pub as_of: Timestamp,
    /// The boxes the run collected for. Inside them the set holds every
    /// site the source has; outside them it holds nothing.
    pub covers: Vec<BoundingBox>,
}

/// What the store knows about the sites it holds from one source.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteSet {
    /// The source.
    pub source: SourceId,
    /// When the run that delivered the set started: the moment the server
    /// fetched it.
    pub fetched_at: Timestamp,
    /// How many sites the set holds.
    pub sites: u32,
    /// The date the set is of, by the source's own word.
    pub as_of: Timestamp,
    /// Where the set is whole, as the run that delivered it said.
    pub covers: Vec<BoundingBox>,
}

impl SiteSet {
    /// Describes the set a run delivered.
    pub(crate) fn of(run: &RunRecord, delivery: &SiteDelivery, sites: &[Site]) -> Self {
        Self {
            source: run.source.clone(),
            fetched_at: run.started_at,
            sites: u32::try_from(sites.len()).unwrap_or(u32::MAX),
            as_of: delivery.as_of,
            covers: delivery.covers.clone(),
        }
    }
}

/// The sites of one source that a question found, with the set they belong
/// to.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteLayer {
    /// The set the sites are part of. It describes the whole set the store
    /// holds, not only the part that was found.
    pub set: SiteSet,
    /// The sites found, sorted by identifier. None, if nothing of the set
    /// is where the question asked.
    pub sites: Vec<Site>,
}

/// Checks what a site run hands over. Both stores call this first, so they
/// refuse the same input in the same way.
pub(crate) fn check(run: &RunRecord, sites: &[Site]) -> Result<(), StoreError> {
    let invalid = |problem: String| Err(StoreError::Invalid(problem));
    if !run.healthy {
        return Ok(());
    }
    if sites.is_empty() {
        return invalid(format!(
            "a healthy run of {} delivered no site; an empty set must not replace a stored one",
            run.source
        ));
    }
    let mut seen: BTreeSet<&SiteId> = BTreeSet::new();
    for site in sites {
        if site.id.source() != &run.source {
            return invalid(format!(
                "site {} does not belong to the run's source {}",
                site.id, run.source
            ));
        }
        if !seen.insert(&site.id) {
            return invalid(format!("site {} is listed twice", site.id));
        }
    }
    Ok(())
}
