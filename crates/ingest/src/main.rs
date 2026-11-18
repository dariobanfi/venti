//! Runs the observation sources of a region against the live providers.
//!
//! ```text
//! venti-ingest <region.toml> [--source ID] [--archive DIR] [--json]
//! ```
//!
//! It prints a health line per source and exits with a failure status when a
//! source delivers nothing or reports problems. CI runs it on a schedule as a
//! canary: providers change their formats without notice, and this is how the
//! project finds out.
//!
//! This binary is a development tool. Its pieces move into the server when
//! the server exists.

mod archive;
mod http;

use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Duration, Utc};
use venti_domain::SourceId;
use venti_regions::RegionPack;
use venti_sources::{NoArchive, PlanCtx, Registry, RunOptions, RunOutput, run_observations};
use venti_sources_dwd::Dwd10Min;
use venti_sources_geosphere::Tawes10Min;

/// Every adapter this build contains.
fn registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .register_observations(Box::new(Dwd10Min::new()))
        .expect("dwd-10min registers once");
    registry
        .register_observations(Box::new(Tawes10Min::new()))
        .expect("geosphere-tawes registers once");
    registry
}

struct Args {
    region: PathBuf,
    source: Option<SourceId>,
    archive: Option<PathBuf>,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let usage = "usage: venti-ingest <region.toml> [--source ID] [--archive DIR] [--json]";
    let mut region = None;
    let mut source = None;
    let mut archive = None;
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" => {
                let id = args.next().ok_or(usage)?;
                source = Some(SourceId::new(id).map_err(|e| e.to_string())?);
            }
            "--archive" => archive = Some(PathBuf::from(args.next().ok_or(usage)?)),
            "--json" => json = true,
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{usage}")),
            _ if region.is_none() => region = Some(PathBuf::from(arg)),
            _ => return Err(usage.to_owned()),
        }
    }
    Ok(Args {
        region: region.ok_or(usage)?,
        source,
        archive,
        json,
    })
}

/// Whether a run is healthy enough for the canary to pass.
fn healthy(output: &RunOutput) -> bool {
    let report = &output.report;
    // A few stations are always briefly offline; a provider-wide problem
    // shows as many failures or as nothing delivered at all.
    let tolerated_failures = report.requests / 10;
    report.observations > 0
        && report.failures.len() <= tolerated_failures
        && !report.budget_exhausted
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let registry = registry();
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

    let fetch = match http::HttpFetch::new() {
        Ok(fetch) => fetch,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let now = Utc::now();
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
        let output = match &args.archive {
            Some(root) => {
                let archive = archive::DirArchive::new(root.clone());
                run_observations(source, &fetch, &archive, &ctx, &options).await
            }
            None => run_observations(source, &fetch, &NoArchive, &ctx, &options).await,
        };

        let report = &output.report;
        let ok = healthy(&output);
        all_healthy &= ok;
        let age = report.newest_observation.map_or_else(
            || "none".to_owned(),
            |t| format!("{} min old", (now - t).num_minutes()),
        );
        eprintln!(
            "{} {id}: {} stations, {} observations (newest {age}), {} requests, \
             {} failed, {} skipped, {} rejected",
            if ok { "ok  " } else { "FAIL" },
            report.stations,
            report.observations,
            report.requests,
            report.failures.len(),
            report.skipped.len(),
            report.rejected.len(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_region_pack_is_valid_for_this_build() {
        let regions = concat!(env!("CARGO_MANIFEST_DIR"), "/../../regions");
        let registry = registry();
        let mut checked = 0;
        for entry in std::fs::read_dir(regions).unwrap() {
            let path = entry.unwrap().path().join("region.toml");
            let text = std::fs::read_to_string(&path).unwrap();
            let pack =
                RegionPack::from_toml(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            pack.check_against(&registry)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            checked += 1;
        }
        assert!(checked > 0, "no region packs found");
    }
}
