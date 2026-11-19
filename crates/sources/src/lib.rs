//! The framework every external data source plugs into.
//!
//! An *adapter* turns one provider's data into the canonical types of
//! `venti-domain`. Adapters do no I/O of their own. They say which URLs they
//! need ([`ObservationSource::plan`]) and turn the bytes that come back into
//! records ([`ObservationSource::parse`]). The pipeline in [`run_observations`]
//! does the fetching, archiving, merging and validation, the same way for
//! every source.
//!
//! That split is what makes an adapter testable from recorded payloads alone,
//! and what guarantees that rate limits, archiving and validation cannot be
//! skipped by one careless adapter.
//!
//! Each kind of data has its own trait and its own run. Observations
//! ([`ObservationSource`], [`run_observations`]) arrive every few minutes in
//! pieces that are merged. Airspace ([`AirspaceSource`], [`run_airspace`])
//! changes with the 28-day publication cycle of aviation data; a run
//! delivers the whole set, which replaces the one before.
//!
//! See `docs/data-sources.md` for the design and the source catalogue.

mod archive;
mod fetch;
mod manifest;
mod registry;
mod run;
mod source;
pub mod testing;
mod validate;

pub use archive::{Archive, ArchiveEntry, ArchiveError};
pub use fetch::{Fetch, FetchError};
pub use manifest::{Auth, Kind, Licence, LicenceFamily, Manifest, ManifestError, RateLimit};
pub use registry::{Registry, RegistryError};
pub use run::{
    AirspaceRejection, AirspaceRunOutput, AirspaceRunReport, Failure, RunOptions, RunOutput,
    RunReport, run_airspace, run_observations,
};
pub use source::{
    AirspaceSource, ObservationSource, Parsed, ParsedAirspace, PlanCtx, Request, Skipped,
    SourceError,
};
pub use validate::Rejection;
