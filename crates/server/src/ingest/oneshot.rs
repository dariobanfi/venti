//! The `ingest` subcommand: runs the sources of a region once against the
//! live providers, the sources of observations and of airspace alike.
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
use serde::Serialize;
use venti_domain::SourceId;
use venti_regions::RegionPack;
use venti_sources::{
    AirspaceRunOutput, Failure, PlanCtx, RunOptions, RunOutput, Skipped, run_airspace,
    run_observations,
};

use super::{DirArchive, HttpFetch, airspace_summary, summary};
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

/// What `--json` prints: the output of every run, by kind.
#[derive(Debug, Default, Serialize)]
struct Outputs {
    observations: Vec<RunOutput>,
    airspace: Vec<AirspaceRunOutput>,
}

/// Prints the health line of one source, and its first problems below it.
fn report(id: &SourceId, ok: bool, summary: &str, failures: &[Failure], skipped: &[Skipped]) {
    eprintln!("{} {id}: {summary}", if ok { "ok  " } else { "FAIL" });
    for failure in failures.iter().take(5) {
        eprintln!("     failed {}: {}", failure.tag, failure.error);
    }
    for skipped in skipped.iter().take(5) {
        eprintln!("     skipped {:?}: {}", skipped.what, skipped.reason);
    }
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

    let wanted = |id: &SourceId| args.source.as_ref().is_none_or(|wanted| wanted == id);
    let mut all_healthy = true;
    let mut outputs = Outputs::default();
    for id in pack.observations.sources.iter().filter(|id| wanted(id)) {
        let source = registry
            .observations(id)
            .expect("region was checked against the registry");
        let output = run_observations(source, &fetch, &archive, &ctx, &options).await;

        let run = &output.report;
        let ok = run.is_healthy();
        all_healthy &= ok;
        report(id, ok, &summary(run, now), &run.failures, &run.skipped);
        outputs.observations.push(output);
    }
    for id in pack.airspace.sources.iter().filter(|id| wanted(id)) {
        let source = registry
            .airspace(id)
            .expect("region was checked against the registry");
        let output = run_airspace(source, &fetch, &archive, &ctx, &options).await;

        // Healthy means what it means to the server: this set would have
        // replaced the stored one.
        let run = &output.report;
        let ok = run.is_healthy();
        all_healthy &= ok;
        report(id, ok, &airspace_summary(run), &run.failures, &run.skipped);
        for rejected in run.rejected.iter().take(5) {
            eprintln!("     rejected {:?}: {}", rejected.name, rejected.reason);
        }
        outputs.airspace.push(output);
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

    if outputs.observations.is_empty() && outputs.airspace.is_empty() {
        eprintln!("no source ran");
        return ExitCode::from(2);
    }
    if all_healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
