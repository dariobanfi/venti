//! Runs the airspace pipeline over a stand-in adapter.
//!
//! The adapter's "provider" publishes one file per country, each a JSON list
//! of canonical records, so the tests are about what the pipeline does with
//! what an adapter hands it: the area, validation, duplicates, and above
//! all when a run counts as complete.

use chrono::{TimeZone, Utc};
use serde_json::json;
use venti_domain::{Airspace, BoundingBox, SourceId};
use venti_sources::testing::{FixtureFetch, MemoryArchive, block_on};
use venti_sources::{
    AirspaceRunOutput, AirspaceSource, Kind, Manifest, ObservationSource, Parsed, ParsedAirspace,
    PlanCtx, Registry, RegistryError, Request, RunOptions, Skipped, SourceError, run_airspace,
};

const GERMANY: &str = "https://airspace.example/de.json";
const AUSTRIA: &str = "https://airspace.example/at.json";

struct Charts(Manifest);

impl Charts {
    fn new(kinds: &str) -> Self {
        let text = format!(
            r#"
            id = "charts"
            provider = "Example Charts"
            kinds = {kinds}
            countries = ["DE", "AT"]
            cadence_seconds = 86400
            auth = "none"

            [licence]
            name = "CC BY 4.0"
            commercial = true
            redistribution = true
            share_alike = false
            attribution = "Example Charts"
            link = "https://airspace.example"
            verified_on = "2026-11-19"
            verified_from = ["https://airspace.example/licence"]
            "#
        );
        Self(Manifest::from_toml(&text).unwrap())
    }
}

impl AirspaceSource for Charts {
    fn manifest(&self) -> &Manifest {
        &self.0
    }

    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new("de", GERMANY), Request::new("at", AUSTRIA)]
    }

    fn parse(
        &self,
        _request: &Request,
        body: &[u8],
        _ctx: &PlanCtx,
    ) -> Result<ParsedAirspace, SourceError> {
        let records: Vec<serde_json::Value> =
            serde_json::from_slice(body).map_err(|e| SourceError::Malformed(e.to_string()))?;
        let mut parsed = ParsedAirspace::default();
        for record in records {
            let name = record["name"].as_str().unwrap_or_default().to_owned();
            match serde_json::from_value::<Airspace>(record) {
                Ok(airspace) => parsed.airspaces.push(airspace),
                Err(error) => parsed.skipped.push(Skipped::new(&name, error.to_string())),
            }
        }
        Ok(parsed)
    }
}

impl ObservationSource for Charts {
    fn manifest(&self) -> &Manifest {
        &self.0
    }
    fn plan(&self, _: &PlanCtx) -> Vec<Request> {
        Vec::new()
    }
    fn parse(&self, _: &Request, _: &[u8], _: &PlanCtx) -> Result<Parsed, SourceError> {
        Ok(Parsed::default())
    }
}

/// A square airspace of a tenth of a degree with its south-west corner at
/// the given position.
fn record(id: &str, south: f64, west: f64, lower_ft: f64, upper_ft: f64) -> serde_json::Value {
    let (north, east) = (south + 0.1, west + 0.1);
    json!({
        "id": format!("charts:{id}"),
        "name": id.to_uppercase(),
        "class": "D",
        "airspace_type": "ctr",
        "lower": {"reference": "mean-sea-level", "value": lower_ft, "unit": "feet"},
        "upper": {"reference": "mean-sea-level", "value": upper_ft, "unit": "feet"},
        "geometry": {"exterior": [[west, south], [east, south], [east, north], [west, north], [west, south]]},
        "activity": {"by_notam": false, "on_request": false, "on_demand": false, "special_agreement": false},
        "validity": {"updated": format!("2026-11-{:02}T08:00:00Z", 10 + id.len())},
    })
}

fn ctx() -> PlanCtx {
    PlanCtx {
        now: Utc.with_ymd_and_hms(2026, 11, 19, 12, 0, 0).unwrap(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

fn run(fetch: &FixtureFetch) -> AirspaceRunOutput {
    let source = Charts::new(r#"["airspace"]"#);
    let archive = MemoryArchive::default();
    let output = block_on(run_airspace(
        &source,
        fetch,
        &archive,
        &ctx(),
        &RunOptions::default(),
    ));
    // Whatever arrived was archived, usable or not.
    assert!(archive.stored().len() <= output.report.requests);
    output
}

fn bytes(records: &[serde_json::Value]) -> Vec<u8> {
    serde_json::to_vec(records).unwrap()
}

#[test]
fn a_run_delivers_the_whole_set_sorted_and_within_the_area() {
    let fetch = FixtureFetch::new()
        .with(
            GERMANY,
            bytes(&[
                record("muenchen", 48.3, 11.7, 0.0, 3500.0),
                // Hamburg is far north of the area.
                record("hamburg", 53.6, 9.9, 0.0, 2500.0),
                // Straddles the northern edge of the area: it reaches in, so it stays.
                record("ingolstadt", 48.45, 11.4, 0.0, 3500.0),
            ]),
        )
        .with(
            AUSTRIA,
            bytes(&[record("innsbruck", 47.2, 11.3, 0.0, 5000.0)]),
        );

    let output = run(&fetch);

    let ids: Vec<String> = output.airspaces.iter().map(|a| a.id.to_string()).collect();
    assert_eq!(
        ids,
        ["charts:ingolstadt", "charts:innsbruck", "charts:muenchen"]
    );
    let report = &output.report;
    assert_eq!(report.source.as_str(), "charts");
    assert_eq!((report.requests, report.airspaces), (2, 3));
    assert!(report.failures.is_empty() && report.rejected.is_empty() && report.skipped.is_empty());
    assert_eq!(report.duplicates, 0);
    // The newest change among what was delivered: Ingolstadt's.
    assert_eq!(
        report.newest_change,
        Some(Utc.with_ymd_and_hms(2026, 11, 20, 8, 0, 0).unwrap())
    );
    assert!(report.is_healthy());
}

#[test]
fn a_missing_country_makes_the_run_unfit_to_replace_the_last_one() {
    // Austria's file cannot be fetched. For observations one failed request
    // in two would already be too many; here even one in a hundred is,
    // because what is missing is airspace nobody would be warned about.
    let fetch = FixtureFetch::new().with(
        GERMANY,
        bytes(&[record("muenchen", 48.3, 11.7, 0.0, 3500.0)]),
    );
    let output = run(&fetch);
    assert_eq!(output.airspaces.len(), 1);
    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "at");
    assert!(!output.report.is_healthy());

    // The same when a file arrives and is not what it should be.
    let fetch = fetch.with(AUSTRIA, b"<html>502 Bad Gateway</html>".to_vec());
    let output = run(&fetch);
    assert_eq!(output.report.failures.len(), 1);
    assert!(output.report.failures[0].error.contains("malformed"));
    assert!(!output.report.is_healthy());

    // And a run that delivers nothing is not healthy either.
    let fetch = FixtureFetch::new()
        .with(GERMANY, b"[]".to_vec())
        .with(AUSTRIA, b"[]".to_vec());
    let output = run(&fetch);
    assert!(output.report.failures.is_empty());
    assert!(!output.report.is_healthy());
}

#[test]
fn bad_and_repeated_records_are_removed_and_reported_and_the_rest_is_kept() {
    let mut no_shape = record("no-shape", 47.5, 12.0, 0.0, 3000.0);
    no_shape["geometry"] = json!({"exterior": [[12.0, 47.5], [12.1, 47.5]]});
    let fetch = FixtureFetch::new()
        .with(
            GERMANY,
            bytes(&[
                record("muenchen", 48.3, 11.7, 0.0, 3500.0),
                // The floor above the ceiling: the record parses, validation refuses it.
                record("upside-down", 48.0, 11.0, 5000.0, 2500.0),
                // The adapter cannot even read this one.
                no_shape,
            ]),
        )
        .with(
            AUSTRIA,
            bytes(&[
                record("innsbruck", 47.2, 11.3, 0.0, 5000.0),
                // The same identifier again, from the other file.
                record("muenchen", 48.3, 11.7, 0.0, 9999.0),
            ]),
        );

    let output = run(&fetch);

    let ids: Vec<String> = output.airspaces.iter().map(|a| a.id.to_string()).collect();
    assert_eq!(ids, ["charts:innsbruck", "charts:muenchen"]);
    let report = &output.report;
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].airspace, "charts:upside-down");
    assert_eq!(report.rejected[0].name, "UPSIDE-DOWN");
    assert!(report.rejected[0].reason.contains("lower limit lies above"));
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].what, "NO-SHAPE");
    // Austria's file is read first; its Munich wins and Germany's is the duplicate.
    assert_eq!(report.duplicates, 1);
    assert_eq!(output.airspaces[1].upper.value(), 9999.0);
    // One bad record does not freeze the whole set.
    assert!(report.is_healthy());
}

#[test]
fn an_airspace_is_dated_by_its_file() {
    let written = Utc.with_ymd_and_hms(2026, 11, 19, 1, 57, 33).unwrap();
    let inside = Utc.with_ymd_and_hms(2026, 10, 29, 0, 0, 0).unwrap();
    // The record from Austria names the date of its file itself.
    let mut dated = record("innsbruck", 47.2, 11.3, 0.0, 5000.0);
    dated["validity"]["file_date"] = json!(inside);
    let fetch = FixtureFetch::new()
        // Germany's file store says when it wrote the file.
        .dated(
            GERMANY,
            bytes(&[record("muenchen", 48.3, 11.7, 0.0, 3500.0)]),
            written,
        )
        .dated(
            AUSTRIA,
            bytes(&[dated, record("salzburg", 47.7, 12.9, 0.0, 5000.0)]),
            written,
        );

    let output = run(&fetch);

    let date = |id: &str| {
        let airspace = output.airspaces.iter().find(|a| a.id.local() == id);
        airspace.unwrap().validity.file_date
    };
    assert_eq!(date("muenchen"), Some(written));
    assert_eq!(date("salzburg"), Some(written));
    // What the file says about itself wins over what its server says.
    assert_eq!(date("innsbruck"), Some(inside));

    // A file store that puts no date on a file: the time of the run is the
    // best that is known.
    let undated = FixtureFetch::new()
        .with(
            GERMANY,
            bytes(&[record("muenchen", 48.3, 11.7, 0.0, 3500.0)]),
        )
        .with(
            AUSTRIA,
            bytes(&[record("innsbruck", 47.2, 11.3, 0.0, 5000.0)]),
        );
    let output = run(&undated);
    assert!(
        output
            .airspaces
            .iter()
            .all(|airspace| airspace.validity.file_date == Some(ctx().now))
    );
}

#[test]
fn a_source_registers_for_the_kinds_its_manifest_lists() {
    let mut registry = Registry::new();

    // Listed for observations only: not an airspace source.
    let observations_only = Charts::new(r#"["observations"]"#);
    assert_eq!(
        registry
            .register_airspace(Box::new(observations_only))
            .err(),
        Some(RegistryError::KindNotDeclared(
            SourceId::new("charts").unwrap()
        ))
    );

    registry
        .register_airspace(Box::new(Charts::new(r#"["observations", "airspace"]"#)))
        .unwrap();
    let id = SourceId::new("charts").unwrap();
    assert!(registry.manifest(&id, Kind::Airspace).is_some());
    assert!(registry.manifest(&id, Kind::Observations).is_none());
    assert_eq!(
        registry
            .register_airspace(Box::new(Charts::new(r#"["airspace"]"#)))
            .err(),
        Some(RegistryError::Duplicate(SourceId::new("charts").unwrap()))
    );

    // The same source for its other kind: found under both, listed once.
    registry
        .register_observations(Box::new(Charts::new(r#"["observations", "airspace"]"#)))
        .unwrap();
    assert!(registry.airspace(&id).is_some() && registry.observations(&id).is_some());
    assert!(registry.manifest(&id, Kind::Observations).is_some());
    assert!(
        registry
            .airspace(&SourceId::new("other").unwrap())
            .is_none()
    );
    assert_eq!(registry.manifests().count(), 1);
}
