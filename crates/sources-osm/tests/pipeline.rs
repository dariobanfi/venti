//! Runs the real pipeline over an answer recorded from an Overpass instance
//! on 2026-11-21 and compares the result with golden files. One test counts
//! what the first region holds: the numbers `docs/data-sources.md` quotes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use venti_domain::{Site, SiteUse, WindSector};
use venti_sources::testing::{FixtureFetch, MemoryArchive, assert_golden, block_on};
use venti_sources::{PlanCtx, RunOptions, SiteRunOutput, SiteSource, run_sites};
use venti_sources_osm::{OsmFreeFlying, fixtures};

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

fn source() -> OsmFreeFlying {
    OsmFreeFlying::new(fixtures::INTERPRETER).unwrap()
}

/// Shortly after the fixture was recorded, over the first region's area.
fn ctx() -> PlanCtx {
    PlanCtx {
        now: fixtures::recorded_at(),
        area: fixtures::area(),
    }
}

/// The one address a run asks.
fn url() -> String {
    source().plan(&ctx()).remove(0).url
}

fn run(fetch: &FixtureFetch, archive: &MemoryArchive) -> SiteRunOutput {
    block_on(run_sites(
        &source(),
        fetch,
        archive,
        &ctx(),
        &RunOptions::default(),
    ))
}

fn recorded_run() -> SiteRunOutput {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    run(&fetch, &MemoryArchive::default())
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
        format!("{:.4},{:.4}", site.position.lat(), site.position.lon()),
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
            .map(|sector| {
                if sector.from_deg == sector.to_deg {
                    sector.from_deg.to_string()
                } else {
                    format!("{}..{}", sector.from_deg, sector.to_deg)
                }
            })
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
    if let Some(website) = &site.website {
        parts.push(format!("website {website}"));
    }
    if let Some(description) = &site.description {
        parts.push(format!("description {description:?}"));
    }
    if let Some(fee) = &site.fee {
        parts.push(format!("fee {fee:?}"));
    }
    parts.extend(said("rules for guests", site.guest_guidelines));
    if let Some(time) = &site.no_flight_time {
        parts.push(format!("no flying {time:?}"));
    }
    if let Some(outline) = &site.outline {
        // The ring by its points and its length: a point that moved or
        // went shows in the metres.
        let around: f64 = outline
            .points()
            .windows(2)
            .map(|pair| pair[0].distance_m(pair[1]))
            .sum();
        parts.push(format!(
            "outline of {} points, {around:.1} m around",
            outline.points().len()
        ));
    }
    parts.join("; ")
}

/// The run in the form the golden files keep it: each site as one line
/// under its identifier, and the report.
fn summary(output: &SiteRunOutput) -> Value {
    let sites: BTreeMap<String, String> = output
        .sites
        .iter()
        .map(|site| (site.id.to_string(), line(site)))
        .collect();
    json!({ "as_of": output.as_of, "sites": sites, "report": output.report })
}

fn site<'a>(output: &'a SiteRunOutput, id: &str) -> &'a Site {
    output
        .sites
        .iter()
        .find(|site| site.id.to_string() == format!("osm-free-flying:{id}"))
        .unwrap_or_else(|| panic!("no site {id}"))
}

#[test]
fn a_recorded_run_matches_the_golden_file() {
    let fetch = FixtureFetch::new().with_all(fixtures::recorded());
    let archive = MemoryArchive::default();

    let output = run(&fetch, &archive);

    // One request, archived.
    assert_eq!(output.report.requests, 1);
    assert_eq!(archive.stored().len(), 1);
    assert!(output.report.failures.is_empty());
    assert!(
        output.report.rejected.is_empty(),
        "{:?}",
        output.report.rejected
    );
    assert_eq!(output.report.duplicates, 0);
    assert!(output.report.is_healthy());
    // The set is as old as the instance's copy of OpenStreetMap, which the
    // answer names.
    assert_eq!(
        output.as_of,
        Utc.with_ymd_and_hms(2026, 11, 20, 21, 21, 21).unwrap()
    );

    for site in &output.sites {
        assert_eq!(site.id.source().as_str(), "osm-free-flying");
        // Every record says where it can be looked at and corrected.
        let url = site.url.as_deref().unwrap();
        assert_eq!(
            url,
            format!("https://www.openstreetmap.org/{}", site.id.local())
        );
        assert!(fixtures::area().contains(site.position));
    }

    assert_golden(&golden("recorded_run.json"), &summary(&output));
}

/// What OpenStreetMap holds for the first region, by the instance's copy
/// of 2026-11-20. `docs/data-sources.md` quotes these numbers; record the
/// fixture again and they change here first.
#[test]
fn the_first_region_holds_what_was_measured() {
    let output = recorded_run();
    let count = |keep: &dyn Fn(&Site) -> bool| output.sites.iter().filter(|s| keep(s)).count();

    // 517 elements carry the tags; 15 of them are schools, clubs and shops
    // that name no use.
    assert_eq!(output.sites.len(), 502);
    let left_out = |why: &str| {
        output
            .report
            .skipped
            .iter()
            .filter(|skipped| skipped.reason.contains(why))
            .count()
    };
    assert_eq!(left_out("not a take-off or a landing"), 15);

    assert_eq!(count(&|s| s.is(SiteUse::Takeoff)), 252);
    assert_eq!(count(&|s| s.is(SiteUse::Landing)), 142);
    assert_eq!(
        count(&|s| s.is(SiteUse::Takeoff) && s.is(SiteUse::Landing)),
        2
    );
    assert_eq!(count(&|s| s.is(SiteUse::Training)), 9);
    assert_eq!(count(&|s| s.is(SiteUse::Towing)), 0);
    // A fifth of the places carry the tags and do not say what they are.
    assert_eq!(count(&|s| s.uses.is_empty()), 101);

    // The wind directions: tagged on half of the take-offs, and readable
    // for certain on little more than a third.
    let takeoff = |s: &Site| s.is(SiteUse::Takeoff);
    let landing = |s: &Site| s.is(SiteUse::Landing);
    assert_eq!(count(&|s| takeoff(s) && !s.wind_directions.is_empty()), 93);
    assert_eq!(count(&|s| landing(s) && !s.wind_directions.is_empty()), 2);
    assert_eq!(left_out("is left out"), 34);
    assert_eq!(left_out("written with an O"), 19);
    assert_eq!(left_out("which this key does not define"), 7);
    assert_eq!(left_out("other way round"), 6);
    assert_eq!(left_out("not a point of the compass"), 2);

    assert_eq!(count(&|s| takeoff(s) && s.elevation_m.is_some()), 51);
    assert_eq!(count(&|s| landing(s) && s.elevation_m.is_some()), 21);
    assert_eq!(count(&|s| takeoff(s) && s.name.is_some()), 207);
    assert_eq!(count(&|s| landing(s) && s.name.is_some()), 115);
    assert_eq!(count(&|s| s.official == Some(true)), 96);
    assert_eq!(count(&|s| s.official == Some(false)), 15);
    assert_eq!(count(&|s| s.paragliding == Some(false)), 4);
    assert_eq!(count(&|s| s.hang_gliding == Some(false)), 8);
    assert_eq!(count(&|s| s.discouraged), 3);

    // What a site's page gains: a web address on a fifth of the sites, a
    // description on one in twelve, and the rest on fewer.
    assert_eq!(count(&|s| s.website.is_some()), 107);
    assert_eq!(count(&|s| takeoff(s) && s.website.is_some()), 55);
    assert_eq!(count(&|s| landing(s) && s.website.is_some()), 38);
    assert_eq!(count(&|s| s.description.is_some()), 43);
    assert_eq!(count(&|s| s.fee.is_some()), 18);
    assert_eq!(count(&|s| s.fee.as_deref() == Some("yes")), 17);
    assert_eq!(count(&|s| s.guest_guidelines == Some(true)), 13);
    assert_eq!(count(&|s| s.guest_guidelines == Some(false)), 21);
    // 23 sites carry the key for a time without flying; 15 of them say
    // `no`, which is no such time.
    assert_eq!(count(&|s| s.no_flight_time.is_some()), 8);
    assert!(
        output
            .sites
            .iter()
            .all(|s| s.no_flight_time.as_deref() != Some("no"))
    );

    // Every site that is a closed way has its outline: 205 of the 502,
    // among them 82 of the 142 landings and 91 of the 252 take-offs. Two
    // are drawn with fewer points than OpenStreetMap has for them.
    assert_eq!(count(&|s| s.outline.is_some()), 205);
    assert_eq!(count(&|s| landing(s) && s.outline.is_some()), 82);
    assert_eq!(count(&|s| takeoff(s) && s.outline.is_some()), 91);
    assert_eq!(left_out("its outline is drawn with"), 2);
    for site in &output.sites {
        let Some(outline) = &site.outline else {
            assert!(!site.id.local().starts_with("way/") || site.id.local() == "way/40628824");
            continue;
        };
        // A site lies in the box around its outline, at the middle; of
        // the two outlines drawn with fewer points, within a few metres.
        let drawn_with_fewer = ["way/628977468", "way/830382140"].contains(&site.id.local());
        let near = if drawn_with_fewer { 1e-4 } else { 1e-6 };
        let (lats, lons): (Vec<f64>, Vec<f64>) = outline
            .points()
            .iter()
            .map(|point| (point.lat(), point.lon()))
            .unzip();
        let span = |values: &[f64]| {
            (
                values.iter().copied().fold(f64::INFINITY, f64::min),
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        };
        let ((south, north), (west, east)) = (span(&lats), span(&lons));
        assert!((site.position.lat() - f64::midpoint(south, north)).abs() < near);
        assert!((site.position.lon() - f64::midpoint(west, east)).abs() < near);
    }
}

#[test]
fn what_openstreetmap_says_arrives_in_the_record() {
    let output = recorded_run();

    // A take-off with one direction, from the deprecated keys.
    let vogelsang = site(&output, "node/5914647717");
    assert_eq!(vogelsang.name.as_deref(), Some("Vogelsang West"));
    assert_eq!(vogelsang.uses, [SiteUse::Takeoff]);
    assert_eq!(vogelsang.wind_directions, [WindSector::single(270.0)]);
    assert_eq!(vogelsang.official, Some(true));
    assert_eq!(vogelsang.paragliding, Some(true));
    assert_eq!(vogelsang.hang_gliding, None);
    assert!(!vogelsang.discouraged);

    // The ramps on the Wallberg, above the valley of the apps' sample
    // flight: no name, no `sport` tag, found by the scheme's own key.
    let wallberg = site(&output, "node/5964676196");
    assert_eq!(wallberg.name, None);
    assert_eq!(wallberg.uses, [SiteUse::Takeoff]);
    assert!((wallberg.position.lat() - 47.6652).abs() < 1e-4);
    assert!(wallberg.wind_directions.is_empty());

    // An area, placed at its centre, with its height. Its directions are
    // "WNW,W,O": the O is not read, so none of them is.
    let jaegerkamp = site(&output, "way/1080986173");
    assert_eq!(
        jaegerkamp.name.as_deref(),
        Some("Fluggelände Jägerkamp/Nagelspitz")
    );
    assert_eq!(jaegerkamp.elevation_m, Some(1560.0));
    assert_eq!(jaegerkamp.hang_gliding, Some(true));
    assert!(jaegerkamp.wind_directions.is_empty());
    assert!(
        output
            .report
            .skipped
            .iter()
            .any(|skipped| skipped.what.contains("Jägerkamp/Nagelspitz")
                && skipped.reason.contains("WNW,W,O"))
    );

    // A list written with spaces.
    let lenggries = site(&output, "node/389840469");
    assert_eq!(
        lenggries.wind_directions,
        [
            WindSector::single(225.0),
            WindSector::single(180.0),
            WindSector::single(135.0)
        ]
    );

    // A landing with its height, and a place to land at the top. It is an
    // area of four corners, and `website` wins over `contact:website`.
    let blomberg = site(&output, "way/158434276");
    assert_eq!(blomberg.uses, [SiteUse::Landing]);
    assert_eq!(blomberg.elevation_m, Some(700.0));
    assert_eq!(blomberg.outline.as_ref().unwrap().points().len(), 5);
    assert_eq!(
        blomberg.website.as_deref(),
        Some("http://www.dc-isarwinkel.de/")
    );

    // An area lies where it lay while the adapter asked for centres: the
    // position the answer of 2026-11-19 gave for this one.
    assert_eq!(
        (jaegerkamp.position.lat(), jaegerkamp.position.lon()),
        (47.6791413, 11.9051032)
    );
    let field = site(&output, "way/1080988126");
    assert_eq!(
        (field.position.lat(), field.position.lon()),
        (47.6924086, 11.8865478)
    );
    let ring = field.outline.as_ref().unwrap().points();
    assert_eq!(ring.len(), 8);
    assert_eq!(ring.first(), ring.last());
    assert_eq!(field.website.as_deref(), Some("https://www.dhv.de/"));

    // What its mapper wrote, as written: a fee, rules for guests, and a
    // time in which the site may not be used.
    assert_eq!(vogelsang.fee.as_deref(), Some("yes"));
    assert_eq!(vogelsang.guest_guidelines, Some(true));
    assert_eq!(vogelsang.website, None);
    let diedamskopf = site(&output, "node/2484727698");
    assert_eq!(diedamskopf.no_flight_time.as_deref(), Some("01.04-30.09"));
    assert_eq!(diedamskopf.guest_guidelines, Some(false));
    let start = site(&output, "way/1192157262");
    assert_eq!(
        start.description.as_deref(),
        Some("Gleitschirm Startplatz. Bitte Gastflugregeln beachten. Siehe Webseite")
    );
    // A way that is a line has a place and no outline.
    let line = site(&output, "way/40628824");
    assert_eq!(line.outline, None);
    // A point has none either.
    assert_eq!(wallberg.outline, None);

    // The two landing fields with more points than an outline may have.
    let aich = site(&output, "way/628977468");
    assert_eq!(aich.outline.as_ref().unwrap().points().len(), 58);
    let top = output
        .sites
        .iter()
        .find(|s| s.name.as_deref() == Some("Niedere Andelsbuch Toplandeplatz Leo"))
        .unwrap();
    assert_eq!(top.uses, [SiteUse::Landing]);

    // A place that carries the tags and names no use is kept without one.
    // Nothing guesses "landing" from its name.
    let kampenwand = site(&output, "way/223792120");
    assert_eq!(kampenwand.name.as_deref(), Some("Landeplatz Kampenwand"));
    assert!(kampenwand.uses.is_empty());

    // A school is not a site.
    assert!(
        output
            .sites
            .iter()
            .all(|s| s.name.as_deref() != Some("Flugschule Hochries"))
    );
}

#[test]
fn damaged_elements_are_reported_and_the_rest_is_kept() {
    // Eleven elements of the recorded answer. One is sound. Six cannot be
    // read: a node without a position, an area without a centre, a number
    // that is none, a kind OpenStreetMap does not have, tags that are a
    // list, a latitude off the globe. Two are kept without what cannot be
    // read: a height in feet with directions written with an O, and a
    // range the wrong way round beside values the scheme does not have.
    // Two are left out for what they say: every use refused, and a shop.
    let fetch = FixtureFetch::new().with(url(), fixture("overpass_damaged.json"));

    let output = run(&fetch, &MemoryArchive::default());

    let ids: Vec<&str> = output.sites.iter().map(|s| s.id.local()).collect();
    assert_eq!(ids, ["node/5914647717", "way/295964050", "way/353926562"]);
    let nord = site(&output, "way/353926562");
    assert_eq!(nord.elevation_m, None);
    assert!(nord.wind_directions.is_empty());
    assert_eq!(nord.uses, [SiteUse::Takeoff]);
    let nordost = site(&output, "way/295964050");
    assert!(nordost.wind_directions.is_empty());
    assert_eq!(nordost.uses, [SiteUse::Takeoff]);

    // Everything that was left out is in the report, by name.
    assert_eq!(output.report.skipped.len(), 13);
    assert!(output.report.failures.is_empty());
    assert!(output.report.is_healthy());
    assert_golden(&golden("damaged_run.json"), &summary(&output));
}

#[test]
fn an_answer_that_is_not_whole_delivers_nothing() {
    let failed = |body: Vec<u8>| {
        let fetch = FixtureFetch::new().with(url(), body);
        let output = run(&fetch, &MemoryArchive::default());
        assert!(output.sites.is_empty());
        assert!(!output.report.is_healthy());
        assert_eq!(output.report.failures.len(), 1);
        output.report.failures[0].error.clone()
    };

    // The instance ran out of time and sent what it had: one site and a
    // remark. Half an answer must not replace a whole set.
    let error = failed(fixture("overpass_timeout.json"));
    assert!(error.contains("did not finish"), "{error}");
    assert!(error.contains("timed out"), "{error}");

    // The connection broke in the middle of the file.
    let whole = fixture("overpass_alps.json");
    let error = failed(whole[..whole.len() / 2].to_vec());
    assert!(error.contains("not Overpass JSON"), "{error}");

    // A busy instance answers with a page for people.
    let error = failed(b"<html><body>The server is probably too busy</body></html>".to_vec());
    assert!(error.contains("not Overpass JSON"), "{error}");

    // JSON of another shape.
    let error = failed(br#"{"error": true}"#.to_vec());
    assert!(error.contains("not Overpass JSON"), "{error}");

    // An instance that does not answer at all.
    let output = run(&FixtureFetch::new(), &MemoryArchive::default());
    assert!(!output.report.is_healthy());
    assert_eq!(output.report.failures[0].error, "HTTP status 404");
}

#[test]
fn an_area_whose_outline_cannot_be_read_is_kept_as_a_place() {
    // A way with the box around it and a geometry that is damaged, one
    // that is a line, and one with a tag for guests the scheme does not
    // have and a web address that is none.
    let body = br#"{"elements": [
        {"type": "way", "id": 1, "bounds": {"minlat": 47.7, "minlon": 11.8, "maxlat": 47.701, "maxlon": 11.802},
         "geometry": [{"lat": 47.7, "lon": 11.8}, null, {"lat": 47.701, "lon": 11.802}, {"lat": 47.7, "lon": 11.8}],
         "tags": {"free_flying:site": "landing"}},
        {"type": "way", "id": 2, "bounds": {"minlat": 47.7, "minlon": 11.8, "maxlat": 47.701, "maxlon": 11.802},
         "geometry": [{"lat": 47.7, "lon": 11.8}, {"lat": 47.701, "lon": 11.802}],
         "tags": {"free_flying:site": "landing", "free_flying:guest-guidelines": "maybe",
                  "website": "www.example.org", "free_flying:no-flight-time": "no", "fee": " "}},
        {"type": "way", "id": 3, "geometry": [{"lat": 47.7, "lon": 11.8}, {"lat": 47.701, "lon": 11.802}],
         "tags": {"free_flying:site": "landing"}}
    ]}"#;
    let fetch = FixtureFetch::new().with(url(), body.to_vec());
    let output = run(&fetch, &MemoryArchive::default());

    let ids: Vec<&str> = output.sites.iter().map(|s| s.id.local()).collect();
    assert_eq!(ids, ["way/1", "way/2"]);
    for site in &output.sites {
        assert_eq!(site.outline, None);
        assert_eq!(
            (site.position.lat(), site.position.lon()),
            (47.7005, 11.801)
        );
    }
    let said = &output.sites[1];
    assert_eq!(
        (
            &said.guest_guidelines,
            &said.no_flight_time,
            &said.fee,
            &said.website
        ),
        (&None, &None, &None, &None)
    );
    let reasons: Vec<&str> = output
        .report
        .skipped
        .iter()
        .map(|skipped| skipped.reason.as_str())
        .collect();
    assert!(
        reasons[0].starts_with("its outline cannot be read"),
        "{reasons:?}"
    );
    assert_eq!(
        reasons[1],
        "free_flying:guest-guidelines=maybe is not a value of the scheme"
    );
    // Without the box around it, a way has no place.
    assert_eq!(reasons[2], "the element has no position");
    // The address that is none is the pipeline's to refuse.
    assert_eq!(output.report.rejected.len(), 1);
    assert_eq!(output.report.rejected[0].field, "website");
}

#[test]
fn an_answer_without_a_date_is_as_old_as_the_run() {
    let body = br#"{"elements": [{"type": "node", "id": 1, "lat": 47.7, "lon": 11.8,
                    "tags": {"free_flying:site": "takeoff"}}]}"#;
    let fetch = FixtureFetch::new().with(url(), body.to_vec());
    let output = run(&fetch, &MemoryArchive::default());
    assert_eq!(output.sites.len(), 1);
    assert_eq!(output.as_of, fixtures::recorded_at());
    assert_eq!(output.sites[0].uses, [SiteUse::Takeoff]);
}
