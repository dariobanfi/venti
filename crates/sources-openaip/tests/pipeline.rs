//! Runs the real pipeline over files recorded from openAIP on 2026-11-19
//! and compares the result with golden files, then flies through the
//! result with the warning engine.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use venti_airspace::{
    AirspaceIndex, Altitudes, Config, Level, Policy, PolicyTable, PressureAltitude, Sample,
    VerticalRelation, WarningEngine,
};
use venti_domain::{
    Airspace, AirspaceClass, AirspaceType, BoundingBox, GeoPoint, HeightUnit, VerticalLimit,
    VerticalReference,
};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{
    AirspaceRunOutput, AirspaceSource, Auth, Kind, LicenceFamily, PlanCtx, RunOptions, run_airspace,
};
use venti_sources_openaip::{OpenAip, fixtures};

const GERMANY: &str = "https://storage.openaip.net/openaip-system-exports/de_asp.geojson";
const AUSTRIA: &str = "https://storage.openaip.net/openaip-system-exports/at_asp.geojson";

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

/// Shortly after the fixtures were recorded, over the first region's area.
fn ctx() -> PlanCtx {
    PlanCtx {
        now: fixtures::recorded_at(),
        area: BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap(),
    }
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive) -> AirspaceRunOutput {
    block_on(run_airspace(
        &OpenAip::new(),
        fetch,
        archive,
        &ctx(),
        &RunOptions::default(),
    ))
}

fn recorded_run() -> AirspaceRunOutput {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    run(&fetch, &MemoryArchive::default())
}

/// A limit as one line: what was published, and the same in metres.
fn limit_text(limit: &VerticalLimit) -> String {
    match limit.reference() {
        VerticalReference::Ground => "ground".to_owned(),
        VerticalReference::Unlimited => "unlimited".to_owned(),
        reference => format!(
            "{} {:?} above {:?} = {:.1} m",
            limit.value(),
            limit.unit(),
            reference,
            limit.metres().unwrap()
        ),
    }
}

/// A number that changes when any corner of a ring changes.
fn fingerprint(ring: &[GeoPoint]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for corner in ring {
        for bytes in [corner.lat().to_bits(), corner.lon().to_bits()] {
            for byte in bytes.to_be_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    format!("{hash:016x}")
}

/// The run in the form the golden files keep it: every field of every
/// airspace, with limits and shapes as one line each. The corners
/// themselves pass through the adapter unchanged, so a box, a count and a
/// fingerprint stand for them.
fn summary(output: &AirspaceRunOutput) -> Value {
    let airspaces: Vec<Value> = output
        .airspaces
        .iter()
        .map(|airspace| {
            let full = serde_json::to_value(airspace).unwrap();
            let bounds = airspace.bounds();
            let ring = airspace.geometry.exterior();
            let mut entry = json!({
                "id": full["id"],
                "name": full["name"],
                "class": full["class"],
                "type": full["airspace_type"],
                "country": full["country"],
                "lower": limit_text(&airspace.lower),
                "upper": limit_text(&airspace.upper),
                "shape": format!(
                    "{} corners, {} holes, {:.4}..{:.4} N, {:.4}..{:.4} E, {}",
                    ring.len(),
                    airspace.geometry.holes().len(),
                    bounds.south(),
                    bounds.north(),
                    bounds.west(),
                    bounds.east(),
                    fingerprint(ring)
                ),
                "activity": full["activity"],
                "updated": full["validity"]["updated"],
            });
            if let Some(limit) = &airspace.lower_at_least {
                entry["lower_at_least"] = limit_text(limit).into();
            }
            if let Some(limit) = &airspace.upper_at_most {
                entry["upper_at_most"] = limit_text(limit).into();
            }
            if !airspace.frequencies.is_empty() {
                entry["frequencies"] = full["frequencies"].clone();
            }
            if airspace.remarks.is_some() {
                entry["remarks"] = full["remarks"].clone();
            }
            entry
        })
        .collect();
    json!({ "airspaces": airspaces, "report": output.report })
}

fn named<'a>(output: &'a AirspaceRunOutput, name: &str) -> &'a Airspace {
    output
        .airspaces
        .iter()
        .find(|airspace| airspace.name == name)
        .unwrap_or_else(|| panic!("no airspace named {name}"))
}

#[test]
fn a_recorded_run_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive);

    // One request per country, both archived.
    assert_eq!(output.report.requests, 2);
    assert_eq!(archive.stored().len(), 2);
    assert!(output.report.failures.is_empty());
    assert!(
        output.report.skipped.is_empty(),
        "{:?}",
        output.report.skipped
    );
    assert!(
        output.report.rejected.is_empty(),
        "{:?}",
        output.report.rejected
    );
    assert_eq!(output.report.duplicates, 0);
    assert!(output.report.is_healthy());
    // Every feature of both files became an airspace.
    assert_eq!(output.airspaces.len(), 41 + 47);

    for airspace in &output.airspaces {
        assert_eq!(airspace.validate(), Ok(()), "{}", airspace.name);
        assert_eq!(airspace.id.source().as_str(), "openaip");
        // The files carry no date; the time of the fetch stands in.
        assert_eq!(airspace.validity.file_date, Some(fixtures::recorded_at()));
        assert!(airspace.validity.updated.is_some());
        assert!(matches!(airspace.country.as_deref(), Some("DE" | "AT")));
    }

    assert_golden(&golden("recorded_run.json"), &summary(&output));
}

#[test]
fn what_openaip_publishes_arrives_in_the_record() {
    let output = recorded_run();
    let feet = |reference, value| VerticalLimit::new(reference, value, HeightUnit::Feet).unwrap();

    // A control zone with its tower frequency.
    let innsbruck = named(&output, "CTR LOWI");
    assert_eq!(innsbruck.id.to_string(), "openaip:677d59be13e31af480aca6d8");
    assert_eq!(innsbruck.class, AirspaceClass::D);
    assert_eq!(innsbruck.airspace_type, AirspaceType::Ctr);
    assert_eq!(innsbruck.country.as_deref(), Some("AT"));
    assert_eq!(innsbruck.lower, VerticalLimit::ground());
    assert_eq!(
        innsbruck.upper,
        feet(VerticalReference::MeanSeaLevel, 7000.0)
    );
    assert_eq!(innsbruck.frequencies.len(), 1);
    assert!((innsbruck.frequencies[0].mhz - 120.1).abs() < 1e-9);
    assert_eq!(
        innsbruck.frequencies[0].name.as_deref(),
        Some("INNSBRUCK TOWER")
    );
    assert!(innsbruck.frequencies[0].primary);
    assert!(innsbruck.activity.hours.is_empty());

    // "11000 ft above sea level, but at least 1500 ft above ground", up to a flight level.
    let tmz = named(&output, "TMZ LOWI E");
    assert_eq!(tmz.airspace_type, AirspaceType::Tmz);
    assert_eq!(tmz.lower, feet(VerticalReference::MeanSeaLevel, 11_000.0));
    assert_eq!(
        tmz.lower_at_least,
        Some(feet(VerticalReference::AboveGround, 1500.0))
    );
    assert_eq!(tmz.upper, VerticalLimit::flight_level(125).unwrap());

    // An area for paragliders above Innsbruck that is opened on request.
    let nordpark = named(&output, "TRA NORDPARK");
    assert_eq!(nordpark.airspace_type, AirspaceType::Tra);
    assert!(nordpark.activity.on_request);
    assert!(
        nordpark
            .activity
            .hours_remarks
            .as_deref()
            .unwrap()
            .contains("https://")
    );

    // A drop zone says what it is for; a national park carries its rule.
    let drop_zone = named(&output, "PARA ALTENSTADT ETHA");
    assert_eq!(drop_zone.airspace_type, AirspaceType::AerialSporting);
    assert_eq!(drop_zone.remarks.as_deref(), Some("Parachuting."));
    let park = named(&output, "NATIONALPARK HOHE TAUERN AUSSENZONE T10");
    assert_eq!(park.airspace_type, AirspaceType::OverflightRestriction);
    assert!(park.remarks.as_deref().unwrap().contains("5000 m"));

    // A glider sector and a floor above ground.
    let sector = named(&output, "MOOSBURG");
    assert_eq!(sector.airspace_type, AirspaceType::GliderSector);
    let munich_e = output
        .airspaces
        .iter()
        .find(|a| a.name == "MUENCHEN" && a.class == AirspaceClass::E)
        .unwrap();
    assert_eq!(munich_e.lower, feet(VerticalReference::AboveGround, 1000.0));
}

#[test]
fn damaged_airspaces_are_reported_and_the_rest_is_kept() {
    // Eight airspaces of the recorded Austrian file, seven of them damaged:
    // a ring of two corners, a ceiling in an unknown unit, an unknown type,
    // a missing identifier, a shape that is not a polygon, a corner off the
    // globe and a floor above its ceiling.
    let fetch = FixtureFetch::new()
        .with(GERMANY, fixture("de_asp.geojson"))
        .with(AUSTRIA, fixture("at_asp_damaged.geojson"));

    let output = run(&fetch, &MemoryArchive::default());
    let report = &output.report;

    assert!(report.failures.is_empty());
    let skipped: Vec<&str> = report.skipped.iter().map(|s| s.what.as_str()).collect();
    assert_eq!(
        skipped,
        [
            "TRA NORDPARK",
            "LOR7 (LIZUM)",
            "CTR LOWS",
            "TRA GAISBERG",
            "LOD21 (LIZUM)"
        ]
    );
    // Four of those were left out. The fifth, the restricted area with the
    // unreadable ceiling, was kept with no upper limit and a note.
    let lizum = named(&output, "LOR7 (LIZUM)");
    assert_eq!(lizum.upper, VerticalLimit::unlimited());
    assert!(
        report.skipped[1]
            .reason
            .contains("no upper limit is assumed")
    );
    // The unknown type was kept as one, and is warned about.
    let odd = named(&output, "TMZ LOWI E");
    assert_eq!(
        odd.airspace_type,
        AirspaceType::Other("openAIP type 99".to_owned())
    );
    assert_eq!(PolicyTable::paraglider().policy_for(odd), Policy::Avoid);
    // The floor above the ceiling was caught by the pipeline's validation.
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].name, "TMA LOWI 1");
    // Germany's 41 and three of Austria's eight.
    assert_eq!(output.airspaces.len(), 41 + 3);
    // Every request worked, so the run may replace the set before it; the
    // report says what is missing.
    assert!(report.is_healthy());

    assert_golden(&golden("damaged_run.json"), &summary(&output));
}

#[test]
fn a_file_that_is_not_geojson_fails_its_country_and_the_run() {
    let fetch = FixtureFetch::new()
        .with(GERMANY, fixture("de_asp.geojson"))
        .with(AUSTRIA, b"<html>503 Service Unavailable</html>".to_vec());

    let output = run(&fetch, &MemoryArchive::default());

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "AT");
    // Germany's airspace is delivered, but a set with a country missing
    // must not replace a complete one.
    assert_eq!(output.airspaces.len(), 41);
    assert!(!output.report.is_healthy());

    // A country that cannot be fetched at all is the same.
    let fetch = FixtureFetch::new().with(GERMANY, fixture("de_asp.geojson"));
    let output = run(&fetch, &MemoryArchive::default());
    assert_eq!(output.report.failures[0].error, "HTTP status 404");
    assert!(!output.report.is_healthy());
}

#[test]
fn the_manifest_declares_a_public_source_that_is_not_for_commercial_use() {
    let source = OpenAip::new();
    let manifest = source.manifest();
    assert_eq!(manifest.id.as_str(), "openaip");
    assert_eq!(manifest.kinds, [Kind::Airspace]);
    assert_eq!(manifest.auth, Auth::None);
    assert_eq!(manifest.cadence_seconds, 86_400);
    assert!(!manifest.licence.commercial && manifest.licence.redistribution);
    // Kept apart from every other licence family.
    assert_eq!(
        manifest.licence.family(),
        LicenceFamily::NonCommercialShareAlike
    );
    assert!(manifest.licence.attribution.contains("openAIP"));
    assert!(
        manifest
            .licence
            .attribution
            .contains("https://www.openaip.net")
    );
}

fn sample(time_s: f64, lat: f64, lon: f64, altitude_m: f64) -> Sample {
    Sample {
        time_s,
        position: GeoPoint::new(lat, lon).unwrap(),
        altitude_m,
        pressure: Some(PressureAltitude::standard(altitude_m)),
        ground_elevation_m: None,
        ground_speed_ms: None,
        course_deg: None,
        climb_ms: None,
    }
}

#[test]
fn the_warning_engine_flies_through_the_recorded_airspace() {
    let index = AirspaceIndex::new(recorded_run().airspaces);
    let names = |warnings: &[venti_airspace::Warning<'_>], level: Level| -> Vec<String> {
        let mut names: Vec<String> = warnings
            .iter()
            .filter(|warning| warning.level == level)
            .map(|warning| warning.airspace.name.clone())
            .collect();
        names.sort();
        names
    };

    // Over Innsbruck airport at 1500 m: in the control zone. The terminal
    // area above it starts at 7000 ft, more than 500 m higher.
    let mut engine = WarningEngine::new(Config::default());
    let warnings = engine
        .update(&index, &sample(0.0, 47.26, 11.344, 1500.0))
        .unwrap();
    assert_eq!(names(&warnings, Level::Inside), ["CTR LOWI"]);
    assert_eq!(warnings.len(), 1);

    // On the Nordkette at 2300 m, above the control zone's 7000 ft: in the
    // terminal area, with the zone 166 m below. The two areas that can be
    // opened for paragliders here contain the pilot too. They do not warn,
    // and they do not silence the terminal area: nobody knows whether they
    // are open.
    let nordkette = GeoPoint::new(47.31, 11.38).unwrap();
    let mut engine = WarningEngine::new(Config::default());
    let warnings = engine
        .update(&index, &sample(0.0, 47.31, 11.38, 2300.0))
        .unwrap();
    assert_eq!(names(&warnings, Level::Inside), ["TMA LOWI 1"]);
    assert_eq!(names(&warnings, Level::Near), ["CTR LOWI"]);
    let altitudes = Altitudes {
        altitude_m: 2300.0,
        pressure: Some(PressureAltitude::standard(2300.0)),
        ground_elevation_m: None,
    };
    let mut around: Vec<&str> = index
        .at(nordkette, &altitudes)
        .iter()
        .filter(|entry| entry.vertical.relation == VerticalRelation::Inside)
        .map(|entry| entry.airspace.name.as_str())
        .collect();
    around.sort_unstable();
    assert_eq!(around, ["TMA LOWI 1", "TRA LOWI C", "TRA NORDPARK"]);

    // Over Munich airport at 800 m: in its control zone.
    let mut engine = WarningEngine::new(Config::default());
    let warnings = engine
        .update(&index, &sample(0.0, 48.354, 11.786, 800.0))
        .unwrap();
    assert_eq!(names(&warnings, Level::Inside), ["CTR MUENCHEN"]);

    // At the Wallberg above Tegernsee at 1700 m: class E and nothing else
    // within reach. The pilot is told nothing, which is right.
    let mut engine = WarningEngine::new(Config::default());
    let warnings = engine
        .update(&index, &sample(0.0, 47.66, 11.80, 1700.0))
        .unwrap();
    assert!(warnings.is_empty(), "{:?}", names(&warnings, Level::Near));

    // In the restricted area on the Reiteralpe, which is active on
    // weekdays: warned whatever the day, because the app does not know.
    let reiteralpe = named(&recorded_run(), "ED-R142 REITERALPE MON-FRI+").bounds();
    let (lat, lon) = (
        (reiteralpe.south() + reiteralpe.north()) / 2.0,
        (reiteralpe.west() + reiteralpe.east()) / 2.0,
    );
    let mut engine = WarningEngine::new(Config::default());
    let warnings = engine
        .update(&index, &sample(0.0, lat, lon, 2000.0))
        .unwrap();
    assert!(
        names(&warnings, Level::Inside).contains(&"ED-R142 REITERALPE MON-FRI+".to_owned()),
        "{:?}",
        names(&warnings, Level::Inside)
    );
}
