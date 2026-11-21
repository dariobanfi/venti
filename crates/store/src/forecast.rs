//! What the store keeps about forecasts: which run each weather model of a
//! source was at when the server last looked, and what the server's
//! thermal map judged of each model's cells.
//!
//! The forecasts themselves are not stored. They are fetched from the
//! forecast service when a pilot asks, and that service is their store.
//! What the server writes down is the outcome of its regular look at the
//! service, so that the health report can say how fresh the models are
//! also after a restart, and from a process that does not do the looking.

use std::collections::BTreeSet;

use venti_domain::{ModelId, SourceId, Timestamp};

use crate::{RunRecord, StoreError};

/// The run one weather model of one source was at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForecastModel {
    /// The source that serves the model.
    pub source: SourceId,
    /// The model.
    pub model: ModelId,
    /// When the newest run the source held began.
    pub run: Timestamp,
    /// When the source had that run ready, if it said.
    pub available_at: Option<Timestamp>,
    /// When the server looked: the start of the run that found this out.
    pub checked_at: Timestamp,
}

/// What the server's thermal map judged of the cells of one model of a
/// source, as the server wrote it down when it last made that model's
/// part whole: so that a restarted server has the hours of today that the
/// model's newest run no longer holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThermalPart {
    /// The source the model's values were fetched from.
    pub source: SourceId,
    /// The model.
    pub model: ModelId,
    /// The run of the model the part was last made whole from.
    pub made_from: Timestamp,
    /// When the server wrote the part down.
    pub written_at: Timestamp,
    /// The judged hours of every cell, in the server's own encoding, which
    /// the store keeps as it is and does not read.
    pub cells: Vec<u8>,
}

/// Checks what a look at a forecast source hands over. Both stores call
/// this first, so they refuse the same input in the same way.
pub(crate) fn check(run: &RunRecord, models: &[ForecastModel]) -> Result<(), StoreError> {
    let invalid = |problem: String| Err(StoreError::Invalid(problem));
    if !run.healthy {
        return Ok(());
    }
    if models.is_empty() {
        return invalid(format!(
            "a healthy look at {} found no model; nothing must not replace what is stored",
            run.source
        ));
    }
    let mut seen: BTreeSet<&ModelId> = BTreeSet::new();
    for model in models {
        if model.source != run.source {
            return invalid(format!(
                "model {} of {} does not belong to the run's source {}",
                model.model, model.source, run.source
            ));
        }
        if model.checked_at != run.started_at {
            return invalid(format!(
                "model {} was looked at at {}, not when the run started",
                model.model, model.checked_at
            ));
        }
        if !seen.insert(&model.model) {
            return invalid(format!("model {} is listed twice", model.model));
        }
    }
    Ok(())
}
