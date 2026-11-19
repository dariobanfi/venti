//! The `ingest` subcommand: runs the observation sources of a region once
//! against the live providers.
//!
//! ```text
//! venti-server ingest <region.toml> [--source ID] [--archive DIR] [--json]
//! ```
//!
//! It prints a health line per source and ends with a failure status when a
//! source delivers nothing or reports problems. It needs no database and
//! stores nothing but, on request, the raw payloads. CI runs it on a schedule
//! as a canary: providers change their formats without notice, and this is
//! how the project finds out. It is also the quickest way to look at what a
//! provider sends today.

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Duration;
use venti_domain::SourceId;
use venti_regions::RegionPack;
use venti_sources::{PlanCtx, RunOptions, run_observations};

use super::{DirArchive, HttpFetch, summary};
use crate::adapters;
use crate::clock::{Clock, SystemClock};

/// What the subcommand was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The region pack whose sources to run.
    pub region: PathBuf,
    /// Run only this source.
    pub source: Option<SourceId>,
    /// Keep the raw payloads under this folder.
    pub archive: Option<PathBuf>,
    /// Print the canonical records and reports as JSON on standard output.
    pub json: bool,
}

/// Runs the sources and reports on standard error. `user_agent` is sent to
/// the providers.
pub async fn run(args: Args, user_agent: &str) -> ExitCode {
    let registry = adapters::registry();
    let pack = match std::fs::read_to_string(&args.region)
        .map_err(|e| format!("cannot read {}: {e}", args.region.display()))
        .and_then(|text| RegionPack::from_toml(&text).map_err(|e| e.to_string()))
        .and_then(|pack| {
            pack.check_against(&registry)
                .map(|()| pack)
                .map_err(|e| e.to_string())
        }) {
        Ok(pack) => pack,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let fetch = match HttpFetch::new(user_agent) {
        Ok(fetch) => fetch,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let archive = args.archive.map(DirArchive::new);

    let now = SystemClock.now();
    let ctx = PlanCtx {
        now,
        area: pack.bounds,
    };
    let options = RunOptions {
        since: Some(now - Duration::hours(2)),
        ..RunOptions::default()
    };

    let mut all_healthy = true;
    let mut outputs = Vec::new();
    for id in &pack.observations.sources {
        if args.source.as_ref().is_some_and(|wanted| wanted != id) {
            continue;
        }
        let source = registry
            .observations(id)
            .expect("region was checked against the registry");
        let output = run_observations(source, &fetch, &archive, &ctx, &options).await;

        let report = &output.report;
        let ok = report.is_healthy();
        all_healthy &= ok;
        eprintln!(
            "{} {id}: {}",
            if ok { "ok  " } else { "FAIL" },
            summary(report, now)
        );
        for failure in report.failures.iter().take(5) {
            eprintln!("     failed {}: {}", failure.tag, failure.error);
        }
        for skipped in report.skipped.iter().take(5) {
            eprintln!("     skipped {:?}: {}", skipped.what, skipped.reason);
        }
        outputs.push(output);
    }

    if args.json {
        match serde_json::to_string_pretty(&outputs) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("cannot write JSON: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    if outputs.is_empty() {
        eprintln!("no source ran");
        return ExitCode::from(2);
    }
    if all_healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
