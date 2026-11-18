use venti_domain::{SourceId, Timestamp};

use crate::Request;

/// One raw payload, exactly as the provider sent it.
#[derive(Debug, Clone, Copy)]
pub struct ArchiveEntry<'a> {
    /// The source the payload came from.
    pub source: &'a SourceId,
    /// The request it answers.
    pub request: &'a Request,
    /// The time of the run that fetched it.
    pub fetched_at: Timestamp,
    /// The response body.
    pub body: &'a [u8],
}

/// The archive could not store a payload.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("archive error: {0}")]
pub struct ArchiveError(pub String);

/// Keeps raw payloads so that a parsing bug can be replayed exactly.
pub trait Archive: Sync {
    /// Stores one payload.
    fn store(&self, entry: ArchiveEntry<'_>) -> Result<(), ArchiveError>;
}

/// An archive that keeps nothing, for runs where replay is not needed.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoArchive;

impl Archive for NoArchive {
    fn store(&self, _entry: ArchiveEntry<'_>) -> Result<(), ArchiveError> {
        Ok(())
    }
}
