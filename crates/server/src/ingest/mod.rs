//! Collecting data from the providers.
//!
//! - [`HttpFetch`] and [`DirArchive`] are the pipeline's two contacts with
//!   the outside world: the network and the disk.
//! - [`Collector`] runs one source through the pipeline and stores the
//!   result.
//! - [`scheduler`] calls the collector at each source's cadence.
//! - [`oneshot`] is the `ingest` subcommand: one live run, printed, stored
//!   nowhere. CI uses it as the daily canary.

mod archive;
mod collect;
mod http;
pub mod oneshot;
mod schedule;
pub mod scheduler;

use venti_domain::Timestamp;
use venti_sources::RunReport;

pub use archive::DirArchive;
pub use collect::Collector;
pub use http::HttpFetch;

/// What a run delivered, in one line for a person.
fn summary(report: &RunReport, now: Timestamp) -> String {
    let age = report.newest_observation.map_or_else(
        || "none".to_owned(),
        |newest| format!("{} min old", (now - newest).num_minutes()),
    );
    format!(
        "{} stations, {} observations (newest {age}), {} requests, \
         {} failed, {} skipped, {} rejected",
        report.stations,
        report.observations,
        report.requests,
        report.failures.len(),
        report.skipped.len(),
        report.rejected.len(),
    )
}
