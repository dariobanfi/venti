//! Cables as the store keeps them: one set per source, as for sites.
//!
//! A source of cables delivers everything it has with every run, and that
//! set replaces the one before. A run that asked for its area in pieces,
//! of which some failed, replaces the stored cables inside the pieces that
//! answered and leaves the others as they were. The store never merges the cables of two
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

/// What a cable run does to the stored set of its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Replacing {
    /// A healthy run: its cables are the whole set.
    Whole,
    /// A run that is not healthy but whose delivery names the boxes that
    /// answered: its cables replace the stored ones inside those boxes.
    Inside,
    /// A run that delivered nothing to store.
    Nothing,
}

impl Replacing {
    pub(crate) fn of(run: &RunRecord, delivery: &SiteDelivery) -> Self {
        if run.healthy {
            Self::Whole
        } else if delivery.covers.is_empty() {
            Self::Nothing
        } else {
            Self::Inside
        }
    }
}

/// Whether a stored cable goes when a run replaces the cables inside
/// `covers`: the box around its line lies wholly inside one of them. Then
/// the answer for that box holds it if it is still there. A line that
/// reaches out of every box stays, and is replaced by its new copy if a
/// box's answer holds one; so a line across the edge of two pieces is never
/// lost when one of them fails, and is never stored twice, since a cable's
/// key is its source's and its own.
pub(crate) fn goes(bounds: &BoundingBox, covers: &[BoundingBox]) -> bool {
    covers.iter().any(|cover| cover.encloses(bounds))
}

/// The set of a source after a run replaced the cables inside some boxes,
/// with `cables` cables in all: what it covers is what it covered before
/// and the run's boxes, the date it is of the older of the two.
pub(crate) fn partly_replaced(
    before: Option<&CableSet>,
    run: &RunRecord,
    delivery: &SiteDelivery,
    cables: usize,
) -> CableSet {
    let (as_of, covers) = match before {
        Some(set) => (
            set.as_of.min(delivery.as_of),
            joined(&set.covers, &delivery.covers),
        ),
        None => (delivery.as_of, joined(&[], &delivery.covers)),
    };
    CableSet {
        source: run.source.clone(),
        fetched_at: run.started_at,
        cables: u32::try_from(cables).unwrap_or(u32::MAX),
        as_of,
        covers,
    }
}

/// The boxes of `old` and of `new`, without a box that lies inside another.
fn joined(old: &[BoundingBox], new: &[BoundingBox]) -> Vec<BoundingBox> {
    let mut boxes: Vec<BoundingBox> = Vec::new();
    for b in old.iter().chain(new) {
        if boxes.iter().any(|kept| kept.encloses(b)) {
            continue;
        }
        boxes.retain(|kept| !b.encloses(kept));
        boxes.push(*b);
    }
    boxes
}

/// Checks what a cable run hands over. Both stores call this first.
pub(crate) fn check(
    run: &RunRecord,
    delivery: &SiteDelivery,
    cables: &[Cable],
) -> Result<(), StoreError> {
    let invalid = |problem: String| Err(StoreError::Invalid(problem));
    match Replacing::of(run, delivery) {
        Replacing::Nothing => return Ok(()),
        Replacing::Whole if cables.is_empty() => {
            return invalid(format!(
                "a healthy run of {} delivered no cable; an empty set must not replace a stored one",
                run.source
            ));
        }
        // A piece of the area may hold no cable: a tile over the sea.
        Replacing::Whole | Replacing::Inside => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    fn b(south: f64, west: f64, north: f64, east: f64) -> BoundingBox {
        BoundingBox::new(south, west, north, east).unwrap()
    }

    #[test]
    fn boxes_inside_others_are_dropped_when_joined() {
        let alps = b(43.5, 5.0, 48.5, 17.2);
        let tile = b(47.0, 11.0, 48.0, 12.0);
        let other = b(47.0, 12.0, 48.0, 13.0);
        assert_eq!(joined(&[alps], &[tile]), [alps]);
        assert_eq!(joined(&[tile], &[alps]), [alps]);
        assert_eq!(joined(&[tile], &[other, tile]), [tile, other]);
        assert_eq!(joined(&[], &[tile, tile]), [tile]);
    }

    #[test]
    fn a_line_goes_only_with_the_box_it_lies_inside() {
        let west = b(47.0, 11.0, 48.0, 12.0);
        let east = b(47.0, 12.0, 48.0, 13.0);
        let inside = b(47.2, 11.2, 47.4, 11.9);
        let across = b(47.2, 11.8, 47.4, 12.2);
        let on_the_edge = b(47.2, 11.5, 47.4, 12.0);
        assert!(goes(&inside, &[west]));
        assert!(!goes(&inside, &[east]));
        assert!(!goes(&across, &[west]));
        assert!(!goes(&across, &[east]));
        assert!(!goes(&across, &[west, east]), "inside neither box alone");
        assert!(goes(&on_the_edge, &[west]));
        assert!(!goes(&on_the_edge, &[east]));
    }
}
