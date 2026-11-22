//! Runs the real pipeline over the answers recorded from data.gouv.fr on
//! 2026-11-22 and compares the result with golden files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use venti_domain::{Site, SiteUse, WindSector};
use venti_sources::testing::{
    FixtureFetch, MemoryArchive, assert_golden, block_on, check_adapter_does_no_io,
};
use venti_sources::{PlanCtx, RunOptions, SiteRunOutput, SiteSource, run_sites};
use venti_sources_ffvl::{DATASET, Ffvl, fixtures};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn ctx() -> PlanCtx {
    PlanCtx {
        now: fixtures::recorded_at(),
        area: fixtures::area(),
    }
}

fn run(fetch: &FixtureFetch) -> SiteRunOutput {
    block_on(run_sites(
        &Ffvl::new(),
        fetch,
        &MemoryArchive::default(),
        &ctx(),
        &RunOptions::default(),
    ))
}

/// A site as one line: everything the record holds, in a fixed order.
fn line(site: &Site) -> String {
    let uses: Vec<String> = site
        .uses
        .iter()
        .map(|used_for| {
            serde_json::to_value(used_for)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let mut parts = vec![
        if uses.is_empty() {
            "no use named".to_owned()
        } else {
            uses.join("+")
        },
        format!("{:.5},{:.5}", site.position.lat(), site.position.lon()),
    ];
    if let Some(name) = &site.name {
        parts.push(format!("name {name:?}"));
    }
    if let Some(elevation) = site.elevation_m {
        parts.push(format!("{elevation} m"));
    }
    if !site.wind_directions.is_empty() {
        let sectors: Vec<String> = site
            .wind_directions
            .iter()
            .map(|sector| sector.from_deg.to_string())
            .collect();
        parts.push(format!("wind {}", sectors.join(" ")));
    }
    let said = |what: &str, value: Option<bool>| {
        value.map(|yes| format!("{what} {}", if yes { "yes" } else { "no" }))
    };
    parts.extend(said("paragliding", site.paragliding));
    parts.extend(said("hang gliding", site.hang_gliding));
    parts.extend(said("official", site.official));
    if site.discouraged {
        parts.push("discouraged".to_owned());
    }
    if let Some(description) = &site.description {
        parts.push(format!(
            "description of {} characters",
            description.chars().count()
        ));
    }
    if let Some(outline) = &site.outline {
        parts.push(format!("outline of {} points", outline.points().len()));
    }
    parts.join("; ")
}

fn summary(output: &SiteRunOutput) -> Value {
    let sites: BTreeMap<String, String> = output
        .sites
        .iter()
        .map(|site| (site.id.to_string(), line(site)))
        .collect();
    json!({ "as_of": output.as_of, "sites": sites, "report": output.report })
}

fn site<'a>(output: &'a SiteRunOutput, suid: &str) -> &'a Site {
    output
        .sites
        .iter()
        .find(|site| site.id.local() == suid)
        .unwrap_or_else(|| panic!("no site {suid}"))
}

#[test]
fn a_recorded_run_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let output = run(&fetch);

    // The description, then the file it names.
    assert_eq!(output.report.requests, 2);
    assert!(output.report.failures.is_empty());
    assert!(
        output.report.rejected.is_empty(),
        "{:?}",
        output.report.rejected
    );
    assert!(output.report.is_healthy());
    // The data is as old as the file the description names.
    assert_eq!(
        output.as_of,
        Utc.with_ymd_and_hms(2026, 11, 22, 7, 10, 3).unwrap() + chrono::Duration::milliseconds(384)
    );
    for site in &output.sites {
        assert_eq!(site.id.source().as_str(), "ffvl-sites");
        assert!(fixtures::area().contains(site.position));
        assert!(site.paragliding == Some(true) || site.hang_gliding == Some(true));
    }
    // Albi lies outside the box; the kite place and the closed one are no
    // sites.
    for gone in ["10", "2137", "431"] {
        assert!(output.sites.iter().all(|s| s.id.local() != gone), "{gone}");
    }
    assert_golden(&golden("recorded_run.json"), &summary(&output));
}

#[test]
fn what_the_ffvl_says_arrives_in_the_record() {
    let output = run(&FixtureFetch::new().with_all(fixtures::recorded()));

    // A place with an outline: the ring is closed.
    let echenoz = site(&output, "8");
    assert_eq!(echenoz.uses, [SiteUse::Takeoff]);
    assert_eq!(echenoz.elevation_m, Some(363.0));
    assert_eq!(echenoz.wind_directions, [WindSector::single(90.0)]);
    assert_eq!(echenoz.official, Some(true));
    let outline = echenoz.outline.as_ref().unwrap();
    assert_eq!(outline.points().len(), 5);

    // Closed unless leave is given: kept, and marked.
    assert!(site(&output, "357").discouraged);
    // A range is not read: no directions, and the report says so.
    assert!(site(&output, "5052").wind_directions.is_empty());
    // Descriptions too long for a record are left out, not cut.
    assert_eq!(site(&output, "271").description, None);
    assert_eq!(site(&output, "1026").description, None);
    let said: Vec<&str> = output
        .report
        .skipped
        .iter()
        .map(|skipped| skipped.what.as_str())
        .collect();
    for suid in ["suid 5052", "suid 271", "suid 1026", "suid 431"] {
        assert!(said.contains(&suid), "{suid} not in {said:?}");
    }
}

#[test]
fn damaged_entries_are_reported_and_the_rest_is_kept() {
    let fetch = FixtureFetch::new()
        .with(DATASET, fixture("dataset.json"))
        .with(fixtures::FILE, fixture("terrains_damaged.json"));
    let output = run(&fetch);

    let mut ids: Vec<&str> = output.sites.iter().map(|s| s.id.local()).collect();
    ids.sort_unstable();
    // Kept: the two sound ones, and three that lose what is damaged: the
    // directions, the outline, the height.
    assert_eq!(ids, ["137", "298", "900005", "900006", "900007"]);
    assert!(site(&output, "900005").wind_directions.is_empty());
    assert_eq!(site(&output, "900005").uses, [SiteUse::Takeoff]);
    assert_eq!(site(&output, "900006").outline, None);
    assert_eq!(site(&output, "900007").elevation_m, None);
    assert!(output.report.failures.is_empty());
    assert!(output.report.is_healthy());
    assert_golden(&golden("damaged_run.json"), &summary(&output));
}

#[test]
fn an_answer_that_is_not_the_dataset_delivers_nothing() {
    for body in [
        &b"{\"resources\": []}"[..],
        b"<html>maintenance</html>",
        b"",
    ] {
        let fetch = FixtureFetch::new().with(DATASET, body.to_vec());
        let output = run(&fetch);
        assert!(output.sites.is_empty());
        assert!(!output.report.is_healthy());
    }
    // A file named on another host is not asked.
    let elsewhere = String::from_utf8(fixture("dataset.json"))
        .unwrap()
        .replace("https://static.data.gouv.fr/", "https://example.org/");
    let output = run(&FixtureFetch::new().with(DATASET, elsewhere));
    assert_eq!(output.report.requests, 1);
    assert!(!output.report.is_healthy());
}

#[test]
fn the_plan_asks_https_only_and_the_crate_does_no_io() {
    let requests = Ffvl::new().plan(&ctx());
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.starts_with("https://"));
    check_adapter_does_no_io(Path::new(env!("CARGO_MANIFEST_DIR")));
}
