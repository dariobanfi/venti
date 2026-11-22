//! Runs the real pipeline over files recorded from openAIP on 2026-11-19
//! and compares the result with golden files, then flies through the
//! result with the warning engine.

use std::collections::BTreeMap;
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
const SLOVAKIA: &str = "https://storage.openaip.net/openaip-system-exports/sk_asp.geojson";

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

    // One request per country, each archived.
    assert_eq!(output.report.requests, 8);
    assert_eq!(archive.stored().len(), 8);
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
    // Every feature of every file became an airspace.
    let from = |country: &str| {
        let of_country = |a: &&Airspace| a.country.as_deref() == Some(country);
        output.airspaces.iter().filter(of_country).count()
    };
    let per_country = ["DE", "AT", "CH", "IT", "SI", "HR", "HU", "SK"].map(from);
    assert_eq!(per_country, [41, 47, 13, 25, 15, 27, 16, 8]);
    assert_eq!(output.airspaces.len(), 192);
    // The countries of the records are the countries the manifest names,
    // which is what an answer says its airspace is of.
    let manifest = OpenAip::new().manifest().countries.clone();
    assert_eq!(manifest, ["DE", "AT", "CH", "IT", "SI", "HR", "HU", "SK"]);

    for airspace in &output.airspaces {
        assert_eq!(airspace.validate(), Ok(()), "{}", airspace.name);
        assert_eq!(airspace.id.source().as_str(), "openaip");
        // The files carry no date; the time of the fetch stands in.
        assert_eq!(airspace.validity.file_date, Some(fixtures::recorded_at()));
        assert!(airspace.validity.updated.is_some());
        let country = airspace.country.as_ref().unwrap();
        assert!(manifest.contains(country), "{}: {country}", airspace.name);
        // Every one reaches into the first region's box, or the run would
        // not have kept it.
        let bounds = airspace.bounds();
        assert!(bounds.north() >= 46.3 && bounds.south() <= 48.5);
        assert!(bounds.east() >= 9.4 && bounds.west() <= 17.2);
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

/// The recorded files with one country's replaced, or left out.
fn recorded_but(url: &str, body: Option<Vec<u8>>) -> FixtureFetch {
    let others = fixtures::recorded()
        .into_iter()
        .filter(|(recorded, _)| recorded != url)
        .fold(FixtureFetch::new(), |fetch, (recorded, body)| {
            fetch.with(recorded, body)
        });
    match body {
        Some(body) => others.with(url, body),
        None => others,
    }
}

#[test]
fn every_country_is_read_like_the_first_two() {
    let output = recorded_run();

    // Switzerland: the control zone of St. Gallen-Altenrhein, and a
    // ceiling that may be raised.
    let altenrhein = named(&output, "CTR ST. GALLEN (HX)");
    assert_eq!(altenrhein.country.as_deref(), Some("CH"));
    assert_eq!(altenrhein.airspace_type, AirspaceType::Ctr);
    assert_eq!(altenrhein.class, AirspaceClass::D);
    assert_eq!(altenrhein.lower, VerticalLimit::ground());
    assert!(
        output
            .airspaces
            .iter()
            .any(|a| a.country.as_deref() == Some("CH") && a.upper_at_most.is_some())
    );

    // What the warning table makes of each country's sample. It has rules
    // of its own for Germany and Austria only; the others go by type and
    // class. None of the recorded airspaces is of a type the adapter does
    // not know.
    let table = PolicyTable::paraglider();
    let mut counted: BTreeMap<&str, [usize; 3]> = BTreeMap::new();
    for airspace in &output.airspaces {
        assert!(
            !matches!(airspace.airspace_type, AirspaceType::Other(_)),
            "{}",
            airspace.name
        );
        let counts = counted
            .entry(airspace.country.as_deref().unwrap())
            .or_default();
        match table.policy_for(airspace) {
            Policy::Avoid => counts[0] += 1,
            Policy::Caution => counts[1] += 1,
            Policy::Free => counts[2] += 1,
        }
    }
    // Avoid, caution, free.
    let expected = [
        ("AT", [28, 9, 10]),
        ("CH", [11, 0, 2]),
        ("DE", [29, 2, 10]),
        ("HR", [27, 0, 0]),
        ("HU", [12, 4, 0]),
        ("IT", [16, 9, 0]),
        ("SI", [6, 0, 9]),
        ("SK", [8, 0, 0]),
    ];
    assert_eq!(counted, BTreeMap::from(expected));

    // A temporary reserved airspace opens the air in Austria and closes it
    // everywhere else, for all the table knows.
    let reserved = |country: &str| {
        output
            .airspaces
            .iter()
            .find(|a| a.airspace_type == AirspaceType::Tra && a.country.as_deref() == Some(country))
    };
    assert_eq!(table.policy_for(reserved("AT").unwrap()), Policy::Free);
    assert_eq!(table.policy_for(reserved("HR").unwrap()), Policy::Avoid);
    assert_eq!(table.policy_for(reserved("HU").unwrap()), Policy::Avoid);
}

#[test]
fn damaged_airspaces_are_reported_and_the_rest_is_kept() {
    // Eight airspaces of the recorded Austrian file, seven of them damaged:
    // a ring of two corners, a ceiling in an unknown unit, an unknown type,
    // a missing identifier, a shape that is not a polygon, a corner off the
    // globe and a floor above its ceiling.
    let fetch = recorded_but(AUSTRIA, Some(fixture("at_asp_damaged.geojson")));

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
    // Three of Austria's eight, and the other countries whole.
    assert_eq!(output.airspaces.len(), 192 - 47 + 3);
    // Every request worked, so the run may replace the set before it; the
    // report says what is missing.
    assert!(report.is_healthy());

    assert_golden(&golden("damaged_run.json"), &summary(&output));
}

#[test]
fn a_file_that_is_not_geojson_fails_its_country_and_the_run() {
    let broken = b"<html>503 Service Unavailable</html>".to_vec();
    let fetch = recorded_but(AUSTRIA, Some(broken));

    let output = run(&fetch, &MemoryArchive::default());

    assert_eq!(output.report.failures.len(), 1);
    assert_eq!(output.report.failures[0].tag, "AT");
    // The other countries' airspace is delivered, but a set with a country
    // missing must not replace a complete one.
    assert_eq!(output.airspaces.len(), 192 - 47);
    assert!(!output.report.is_healthy());

    // A country that cannot be fetched at all is the same, whichever it is:
    // the answer would go on naming it among its countries.
    for missing in [GERMANY, SLOVAKIA] {
        let output = run(&recorded_but(missing, None), &MemoryArchive::default());
        assert_eq!(output.report.failures.len(), 1);
        assert_eq!(output.report.failures[0].error, "HTTP status 404");
        assert!(!output.report.is_healthy());
    }
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
        qnh_altitude_m: None,
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
        qnh_altitude_m: None,
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

/// Prints what a run delivered, country by country: how many airspaces,
/// what the paraglider's warning table makes of them, and which kinds
/// there are.
fn survey(output: &AirspaceRunOutput) {
    let report = &output.report;
    println!(
        "  {} airspaces, {} skipped, {} rejected, {} duplicates, newest change {:?}",
        report.airspaces,
        report.skipped.len(),
        report.rejected.len(),
        report.duplicates,
        report.newest_change
    );
    for skipped in &report.skipped {
        println!("  skipped {}: {}", skipped.what, skipped.reason);
    }
    for rejected in &report.rejected {
        println!("  rejected {}: {}", rejected.name, rejected.reason);
    }
    let table = PolicyTable::paraglider();
    let mut countries: BTreeMap<&str, Vec<&Airspace>> = BTreeMap::new();
    for airspace in &output.airspaces {
        let country = airspace.country.as_deref().unwrap_or("none");
        countries.entry(country).or_default().push(airspace);
    }
    for (country, airspaces) in &countries {
        let count = |policy: Policy| {
            let with_policy = |a: &&&Airspace| table.policy_for(a) == policy;
            airspaces.iter().filter(with_policy).count()
        };
        let unknown = |a: &&&Airspace| matches!(a.airspace_type, AirspaceType::Other(_));
        let corners: usize = airspaces.iter().map(|a| a.geometry.exterior().len()).sum();
        println!(
            "  {country}: {} airspaces, {corners} corners; {} avoid, {} caution, {} free; \
             {} of a type the adapter does not know",
            airspaces.len(),
            count(Policy::Avoid),
            count(Policy::Caution),
            count(Policy::Free),
            airspaces.iter().filter(unknown).count()
        );
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        for airspace in airspaces {
            let kind = format!(
                "{:?} in class {:?}: {:?}",
                airspace.airspace_type,
                airspace.class,
                table.policy_for(airspace)
            );
            *kinds.entry(kind).or_default() += 1;
        }
        for (kind, count) in kinds {
            println!("      {count:4} {kind}");
        }
    }
}

/// Reads whole country files, when a folder that holds them is named.
///
/// The recorded fixtures are a small part of each country. To check the
/// adapter against everything openAIP publishes for the countries of its
/// manifest, fetch `<country>_asp.geojson` for each of them into a folder
/// and run
///
/// ```text
/// VENTI_OPENAIP_DIR=<folder> cargo test -p venti-sources-openaip --test pipeline whole -- --nocapture
/// ```
///
/// It prints, for every country whole and for what of it reaches into the
/// first region's box, what the warning table makes of the airspace. The
/// numbers of 2026-11-19 are in `docs/airspace.md`. The files are not part
/// of the repository.
#[test]
fn whole_country_files_are_read_when_given() {
    let Some(folder) = std::env::var_os("VENTI_OPENAIP_DIR") else {
        return;
    };
    let folder = PathBuf::from(folder);
    let source = OpenAip::new();
    let fetch = source
        .plan(&ctx())
        .into_iter()
        .fold(FixtureFetch::new(), |fetch, request| {
            let name = request.url.rsplit('/').next().unwrap().to_owned();
            let path = folder.join(&name);
            let body = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            fetch.with(request.url, body)
        });
    // Every country whole, not only the first region's box.
    let everywhere = PlanCtx {
        now: fixtures::recorded_at(),
        area: BoundingBox::new(-90.0, -180.0, 90.0, 180.0).unwrap(),
    };
    let run_over = |ctx: &PlanCtx| {
        let archive = MemoryArchive::default();
        block_on(run_airspace(
            &source,
            &fetch,
            &archive,
            ctx,
            &RunOptions::default(),
        ))
    };
    println!("The first region's box:");
    let in_the_box = run_over(&ctx());
    survey(&in_the_box);
    assert!(in_the_box.report.is_healthy());
    println!("Every country whole:");
    let output = run_over(&everywhere);
    survey(&output);
    let report = &output.report;

    // openAIP publishes the same airspace as OpenAir files. Where the
    // folder holds them too (`de_asp_v2.txt`, `at_asp_v2.txt`), the two
    // readers of this project must agree on every airspace: its class, its
    // type and its two limits, taken by name.
    let describe = |airspace: &Airspace| {
        format!(
            "{} | {:?} {:?} | {} to {}",
            airspace.name,
            airspace.class,
            airspace.airspace_type,
            limit_text(&airspace.lower),
            limit_text(&airspace.upper)
        )
    };
    let mut from_geojson: Vec<String> = output
        .airspaces
        .iter()
        .filter(|a| matches!(a.country.as_deref(), Some("DE" | "AT")))
        .map(describe)
        .collect();
    let mut from_openair = Vec::new();
    for name in ["de_asp_v2.txt", "at_asp_v2.txt"] {
        let Ok(bytes) = std::fs::read(folder.join(name)) else {
            assert!(report.is_healthy());
            return;
        };
        let options = venti_airspace::openair::Options::new(OpenAip::new().manifest().id.clone());
        let file = venti_airspace::openair::parse_bytes(&bytes, &options);
        assert!(file.problems.is_empty(), "{name}: {:?}", file.problems);
        from_openair.extend(file.airspaces.iter().map(describe));
    }
    from_geojson.sort();
    from_openair.sort();
    let differing = from_geojson
        .iter()
        .zip(&from_openair)
        .filter(|(a, b)| a != b)
        .inspect(|(a, b)| println!("  GeoJSON: {a}\n  OpenAir: {b}"))
        .count();
    println!(
        "  {} airspaces as GeoJSON, {} as OpenAir, {differing} described differently",
        from_geojson.len(),
        from_openair.len()
    );
    assert_eq!(from_geojson.len(), from_openair.len());
    assert_eq!(differing, 0);
    assert!(report.is_healthy());
}
