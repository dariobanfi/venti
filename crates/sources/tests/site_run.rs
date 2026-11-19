//! Runs the site pipeline over a stand-in adapter.
//!
//! The adapter's "provider" answers one request per half of the area with a
//! JSON list of canonical records, so the tests are about what the pipeline
//! does with what an adapter hands it: the area, duplicates, the date of
//! the set, and when a run counts as complete.

use chrono::{TimeZone, Utc};
use serde_json::json;
use venti_domain::{BoundingBox, Site, SiteUse, Timestamp};
use venti_sources::testing::{FixtureFetch, MemoryArchive, block_on};
use venti_sources::{
    Kind, Manifest, ParsedSites, PlanCtx, Registry, RegistryError, Request, RunOptions,
    SiteRunOutput, SiteSource, Skipped, SourceError, run_sites,
};

const WEST: &str = "https://sites.example/west.json";
const EAST: &str = "https://sites.example/east.json";

struct Club(Manifest);

impl Club {
    fn new() -> Self {
        let text = r#"
            id = "club"
            provider = "Example Flying Club"
            kinds = ["sites"]
            countries = ["DE", "AT"]
            cadence_seconds = 86400
            auth = "none"

            [licence]
            name = "ODbL 1.0"
            commercial = true
            redistribution = true
            share_alike = true
            attribution = "Example Flying Club"
            link = "https://sites.example"
            verified_on = "2026-11-19"
            verified_from = ["https://sites.example/licence"]
        "#;
        Self(Manifest::from_toml(text).unwrap())
    }
}

/// What the stand-in provider sends: the date its data is of, if it says,
/// and its records.
#[derive(serde::Deserialize)]
struct Answer {
    as_of: Option<Timestamp>,
    sites: Vec<serde_json::Value>,
}

impl SiteSource for Club {
    fn manifest(&self) -> &Manifest {
        &self.0
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("west", WEST), Request::new("east", EAST)]
    }

    fn parse(
        &self,
        _request: &Request,
        body: &[u8],
        _ctx: &PlanCtx,
    ) -> Result<ParsedSites, SourceError> {
        let answer: Answer =
            serde_json::from_slice(body).map_err(|e| SourceError::Malformed(e.to_string()))?;
        let mut parsed = ParsedSites {
            as_of: answer.as_of,
            ..ParsedSites::default()
        };
        for record in answer.sites {
            let id = record["id"].as_str().unwrap_or_default().to_owned();
            match serde_json::from_value::<Site>(record) {
                Ok(site) => parsed.sites.push(site),
                Err(error) => parsed.skipped.push(Skipped::new(&id, error.to_string())),
            }
        }
        Ok(parsed)
    }
}

fn record(id: &str, lat: f64, lon: f64) -> serde_json::Value {
    json!({
        "id": format!("club:{id}"),
        "name": id.to_uppercase(),
        "position": {"lat": lat, "lon": lon},
        "elevation_m": 1500.0,
        "uses": ["takeoff"],
    })
}

fn at(hour: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 11, 19, hour, 0, 0).unwrap()
}

fn ctx() -> PlanCtx {
    PlanCtx {
        now: at(12),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

fn answer(as_of: Option<Timestamp>, sites: &[serde_json::Value]) -> Vec<u8> {
    serde_json::to_vec(&json!({"as_of": as_of, "sites": sites})).unwrap()
}

fn run(fetch: &FixtureFetch) -> SiteRunOutput {
    let archive = MemoryArchive::default();
    let output = block_on(run_sites(
        &Club::new(),
        fetch,
        &archive,
        &ctx(),
        &RunOptions::default(),
    ));
    // Whatever arrived was archived, usable or not.
    assert!(archive.stored().len() <= output.report.requests);
    output
}

#[test]
fn a_run_delivers_the_whole_set_sorted_and_within_the_area() {
    let fetch = FixtureFetch::new()
        .with(
            WEST,
            answer(
                Some(at(3)),
                &[
                    record("wallberg", 47.66, 11.80),
                    // Far north of the area.
                    record("rammelsberg", 51.89, 10.42),
                    record("brauneck", 47.66, 11.52),
                ],
            ),
        )
        .with(
            EAST,
            answer(Some(at(5)), &[record("gerlitzen", 46.69, 13.91)]),
        );

    let output = run(&fetch);

    let ids: Vec<String> = output.sites.iter().map(|s| s.id.to_string()).collect();
    assert_eq!(ids, ["club:brauneck", "club:gerlitzen", "club:wallberg"]);
    assert_eq!(output.report.sites, 3);
    assert_eq!(output.report.requests, 2);
    assert!(output.report.rejected.is_empty());
    assert!(output.report.is_healthy());
    // The set is as old as the oldest answer it was read from.
    assert_eq!(output.as_of, at(3));
    assert_eq!(output.sites[0].uses, [SiteUse::Takeoff]);
}

#[test]
fn an_answer_without_a_date_is_dated_by_its_server_or_by_the_run() {
    let sites = [record("wallberg", 47.66, 11.80)];
    let both = |fetch: FixtureFetch| fetch.with(EAST, answer(None, &sites[..0]));

    // The provider's server put a date on the answer.
    let dated = both(FixtureFetch::new().dated(WEST, answer(None, &sites), at(7)));
    let output = run(&dated);
    assert_eq!(output.as_of, at(7));

    // Nobody says: the time of the run stands in.
    let undated = both(FixtureFetch::new().with(WEST, answer(None, &sites)));
    assert_eq!(run(&undated).as_of, ctx().now);

    // A provider whose clock runs ahead: data is never newer than the run.
    let ahead = both(FixtureFetch::new().with(WEST, answer(Some(at(20)), &sites)));
    assert_eq!(run(&ahead).as_of, ctx().now);
}

#[test]
fn a_run_that_could_not_fetch_everything_must_not_replace_a_set() {
    // The eastern half does not answer.
    let fetch = FixtureFetch::new().with(
        WEST,
        answer(Some(at(3)), &[record("wallberg", 47.66, 11.80)]),
    );

    let output = run(&fetch);

    assert_eq!(output.sites.len(), 1);
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "east");
    assert!(!output.report.is_healthy());

    // An answer that is not what the adapter expects is a failure too.
    let damaged = fetch.with(EAST, b"<html>busy</html>".to_vec());
    let output = run(&damaged);
    assert!(output.report.failures[0].error.contains("malformed"));
    assert!(!output.report.is_healthy());

    // And so is a run that delivers nothing at all.
    let empty = FixtureFetch::new()
        .with(WEST, answer(None, &[]))
        .with(EAST, answer(None, &[]));
    let output = run(&empty);
    assert!(output.report.failures.is_empty());
    assert!(!output.report.is_healthy());
}

#[test]
fn strangers_and_doubles_are_removed_and_faulty_values_cost_only_themselves() {
    let mut high = record("zugspitze", 47.42, 10.98);
    high["elevation_m"] = json!(29_620.0);
    let mut stranger = record("kampenwand", 47.75, 12.35);
    stranger["id"] = json!("other:kampenwand");
    let mut unreadable = record("hochries", 47.75, 12.25);
    unreadable["position"] = json!({"lat": 147.75, "lon": 12.25});
    let fetch = FixtureFetch::new()
        .with(
            WEST,
            answer(
                None,
                &[high, stranger, unreadable, record("wallberg", 47.66, 11.80)],
            ),
        )
        .with(EAST, answer(None, &[record("wallberg", 47.67, 11.81)]));

    let output = run(&fetch);

    let ids: Vec<String> = output.sites.iter().map(|s| s.id.to_string()).collect();
    assert_eq!(ids, ["club:wallberg", "club:zugspitze"]);
    // The first of two records with one identifier is the one that stays.
    assert_eq!(output.sites[0].position.lat(), 47.67);
    assert_eq!(output.report.duplicates, 1);
    assert_eq!(output.sites[1].elevation_m, None);

    let rejected: Vec<(&str, &str)> = output
        .report
        .rejected
        .iter()
        .map(|r| (r.site.as_str(), r.field.as_str()))
        .collect();
    assert_eq!(
        rejected,
        [
            ("club:zugspitze", "elevation_m"),
            ("other:kampenwand", "record")
        ]
    );
    assert_eq!(output.report.skipped.len(), 1);
    assert_eq!(output.report.skipped[0].what, "club:hochries");
    // None of this makes the run unfit: one bad record must not freeze the set.
    assert!(output.report.is_healthy());
}

#[test]
fn a_source_registers_for_the_kind_its_manifest_lists() {
    let mut registry = Registry::new();
    registry.register_sites(Box::new(Club::new())).unwrap();
    let id = Club::new().0.id;

    assert!(registry.sites(&id).is_some());
    assert!(registry.forecasts(&id).is_none());
    assert_eq!(
        registry.manifest(&id, Kind::Sites).unwrap().provider,
        "Example Flying Club"
    );
    assert!(registry.manifest(&id, Kind::Airspace).is_none());
    assert_eq!(registry.manifests().count(), 1);
    assert_eq!(
        registry.register_sites(Box::new(Club::new())),
        Err(RegistryError::Duplicate(id))
    );
}
