//! The dumps of Venti's own two layers of sites: the list, the three
//! files of each layer, what is in them and what is not, and when a file
//! is made anew.

mod common;

use std::collections::BTreeMap;
use std::io::Read;

use axum::Router;
use axum::http::{Method, StatusCode, header};
use chrono::Duration;
use serde_json::Value;
use sha2::{Digest, Sha256};
use venti_api_types::v1::{
    AddSiteRequest, CSV_MEDIA_TYPE, DUMPS_PATH, DumpContent, DumpFormat, DumpLayer, DumpsResponse,
    ErrorCode, GEOJSON_MEDIA_TYPE, JSON_LINES_MEDIA_TYPE, SITES_CSV_COLUMNS, SITES_PATH,
    SiteChangesResponse, SiteHistoryHead, SiteHistoryLine, SiteHistoryResponse, SiteRecord,
    SiteResponse, sites_dump_path,
};
use venti_server::adapters;
use venti_server::dev::{DEMO_SITES, DEMO_TOKEN};
use venti_server::pilot_sites;
use venti_sources::testing::assert_golden;
use venti_store::{SiteAct, SiteRevising, Store};

use common::{
    TestClock, ask, assert_golden_text, get, golden, recorded, recorded_with_its_store, server,
    settings, sign_up,
};

const PARTNERS: &str = "venti";
const PILOTS: &str = "venti-pilots";

/// The made-up sites of the development mode: two of each layer, the
/// fourth removed.
const FIRST: &str = DEMO_SITES[0];
const SECOND: &str = DEMO_SITES[1];
const THIRD: &str = DEMO_SITES[2];
const FOURTH: &str = DEMO_SITES[3];

async fn listed(router: &Router) -> DumpsResponse {
    get(router, DUMPS_PATH).await.ok(StatusCode::OK)
}

fn layer<'a>(list: &'a DumpsResponse, name: &str) -> &'a DumpLayer {
    list.layers
        .iter()
        .find(|layer| layer.stamp.layer.as_str() == name)
        .unwrap_or_else(|| panic!("no dump of {name}"))
}

async fn file(router: &Router, layer: &str, format: DumpFormat) -> String {
    let answer = get(router, &sites_dump_path(layer, format)).await;
    assert_eq!(answer.status, StatusCode::OK, "{layer} {format:?}");
    answer.text().to_owned()
}

/// The lines of a CSV file as RFC 4180 writes them, each as its fields.
fn csv(text: &str) -> Vec<Vec<String>> {
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => line.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {
                chars.next();
                line.push(std::mem::take(&mut field));
                lines.push(std::mem::take(&mut line));
            }
            (false, c) => field.push(c),
        }
    }
    assert!(
        !quoted && field.is_empty() && line.is_empty(),
        "the last line is not ended"
    );
    lines
}

/// A CSV file as one map per site, by column.
fn rows(text: &str) -> Vec<BTreeMap<String, String>> {
    let lines = csv(text);
    let (names, sites) = lines.split_first().expect("the line of column names");
    assert_eq!(names, &SITES_CSV_COLUMNS);
    sites
        .iter()
        .map(|site| {
            assert_eq!(site.len(), names.len());
            names.iter().cloned().zip(site.iter().cloned()).collect()
        })
        .collect()
}

/// A history file: its first line, and every further one.
fn history(text: &str) -> (SiteHistoryHead, Vec<SiteHistoryLine>) {
    assert!(text.ends_with('\n'));
    let mut lines = text.lines();
    let head = serde_json::from_str(lines.next().expect("the first line")).unwrap();
    let sites = lines
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (head, sites)
}

fn ids(features: &Value) -> Vec<&str> {
    features
        .as_array()
        .unwrap()
        .iter()
        .map(|feature| feature["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn the_dumps_are_listed_and_fetched_without_an_account() {
    let router = recorded().await;
    let answer = get(&router, DUMPS_PATH).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, no-cache");
    let list: DumpsResponse = answer.json();
    assert_golden(&golden("dumps.json"), &list);

    let names: Vec<&str> = list
        .layers
        .iter()
        .map(|layer| layer.stamp.layer.as_str())
        .collect();
    assert_eq!(names, [PARTNERS, PILOTS]);
    assert_eq!(list.attributions.len(), 2);
    for layer in &list.layers {
        let name = layer.stamp.layer.as_str();
        assert_eq!(layer.stamp.licence, "CC0 1.0");
        assert_eq!(
            layer.stamp.licence_url,
            "https://creativecommons.org/publicdomain/zero/1.0/"
        );
        assert_eq!(layer.stamp.generated_at, adapters::recorded_at());
        let kinds: Vec<_> = layer
            .files
            .iter()
            .map(|file| (file.content, file.format, file.media_type.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                (DumpContent::Sites, DumpFormat::Geojson, GEOJSON_MEDIA_TYPE),
                (DumpContent::Sites, DumpFormat::Csv, CSV_MEDIA_TYPE),
                (
                    DumpContent::History,
                    DumpFormat::Jsonl,
                    JSON_LINES_MEDIA_TYPE
                ),
            ]
        );
        for listed in &layer.files {
            assert_eq!(listed.path, sites_dump_path(name, listed.format));
            // No token, and the file is the one the list describes.
            let file = get(&router, &listed.path).await;
            assert_eq!(file.status, StatusCode::OK, "{}", listed.path);
            assert_eq!(file.header(header::CONTENT_TYPE), listed.media_type);
            assert_eq!(file.header(header::CACHE_CONTROL), "public, no-cache");
            assert!(file.headers.contains_key(header::ETAG));
            assert_eq!(file.body.len() as u64, listed.bytes);
            let sum: String = Sha256::digest(&file.body)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(sum, listed.sha256, "{}", listed.path);
        }
    }
    // The crowdsourced layer says in every list and file what it is.
    assert!(
        layer(&list, PILOTS)
            .stamp
            .notice
            .contains("not official and not checked by anyone")
    );
    assert_eq!(
        (layer(&list, PARTNERS).sites, layer(&list, PARTNERS).removed),
        (2, 0)
    );
    assert_eq!(
        (layer(&list, PILOTS).sites, layer(&list, PILOTS).removed),
        (1, 1)
    );

    // The files as the documentation shows them.
    assert_golden_text(
        "dumps_venti.geojson",
        &file(&router, PARTNERS, DumpFormat::Geojson).await,
    );
    assert_golden_text(
        "dumps_venti.csv",
        &file(&router, PARTNERS, DumpFormat::Csv).await,
    );
    assert_golden_text(
        "dumps_venti_pilots.history.jsonl",
        &file(&router, PILOTS, DumpFormat::Jsonl).await,
    );
}

#[tokio::test]
async fn a_layers_files_hold_its_sites_and_none_of_the_other() {
    let router = recorded().await;
    let list = listed(&router).await;
    for (name, standing, all) in [
        (PARTNERS, vec![FIRST, THIRD], vec![FIRST, THIRD]),
        (PILOTS, vec![SECOND], vec![SECOND, FOURTH]),
    ] {
        let stamp = &layer(&list, name).stamp;

        // GeoJSON: the licence, the layer and the cursor are members of
        // the collection, and every feature is a site as the API has it.
        let geojson: Value =
            serde_json::from_str(&file(&router, name, DumpFormat::Geojson).await).unwrap();
        assert_eq!(geojson["type"], "FeatureCollection");
        assert_eq!(geojson["layer"], name);
        assert_eq!(geojson["licence"], "CC0 1.0");
        assert_eq!(geojson["licence_url"], stamp.licence_url.as_str());
        assert_eq!(geojson["notice"], stamp.notice.as_str());
        assert_eq!(geojson["cursor"], stamp.cursor.as_str());
        assert_eq!(geojson["generated_at"], "2026-11-18T19:05:00Z");
        assert_eq!(ids(&geojson["features"]), standing);
        for feature in geojson["features"].as_array().unwrap() {
            assert_eq!(feature["type"], "Feature");
            assert_eq!(feature["geometry"]["type"], "Point");
            let record: SiteRecord = serde_json::from_value(feature["properties"].clone()).unwrap();
            assert_eq!(feature["id"], record.site.id.to_string());
            assert_eq!(record.standing.id, record.site.id);
            assert_eq!(
                feature["geometry"]["coordinates"],
                serde_json::json!([record.site.position.lon(), record.site.position.lat()])
            );
            // The record is the one the site's own address answers.
            let asked: SiteResponse = get(&router, &format!("{SITES_PATH}/{}", record.site.id))
                .await
                .ok(StatusCode::OK);
            assert_eq!(asked.site, record.site);
            assert_eq!(asked.standing.as_ref(), Some(&record.standing));
        }

        // CSV: the same sites, a line each, in the stated columns.
        let sites = rows(&file(&router, name, DumpFormat::Csv).await);
        let in_csv: Vec<&str> = sites.iter().map(|site| site["id"].as_str()).collect();
        assert_eq!(in_csv, standing);
        for site in &sites {
            assert_eq!(site["layer"], name);
            assert!(site["id"].starts_with(&format!("{name}:")));
            assert!(!site["name"].is_empty());
            assert!(site["lat"].parse::<f64>().is_ok() && site["lon"].parse::<f64>().is_ok());
        }

        // The history: every site the layer ever had, the removed one
        // too, each with the revisions its own history answers.
        let (head, lines) = history(&file(&router, name, DumpFormat::Jsonl).await);
        assert_eq!(&head.stamp, stamp);
        assert_eq!(head.sites as usize, lines.len());
        let in_history: Vec<String> = lines.iter().map(|line| line.id.to_string()).collect();
        assert_eq!(in_history, all);
        let mut revisions = 0;
        for line in &lines {
            let asked: SiteHistoryResponse =
                get(&router, &format!("{SITES_PATH}/{}/history", line.id))
                    .await
                    .ok(StatusCode::OK);
            assert_eq!(line.revisions, asked.revisions, "{}", line.id);
            assert_eq!(line.revision, asked.revision);
            assert_eq!(line.removed, asked.removed);
            assert_eq!(line.removal.is_some(), line.removed);
            revisions += line.revisions.len();
        }
        assert_eq!(head.revisions as usize, revisions);
        assert_eq!(layer(&list, name).revisions as usize, revisions);
    }

    // The first site of the partners' layer, as its line of the CSV file
    // has it.
    let partners = rows(&file(&router, PARTNERS, DumpFormat::Csv).await);
    let first = &partners[0];
    assert_eq!(first["name"], "Musterkogel West");
    assert_eq!(
        (first["lat"].as_str(), first["lon"].as_str()),
        ("47.3", "10.9")
    );
    assert_eq!(first["elevation_m"], "1862.0");
    assert_eq!(first["uses"], "takeoff");
    assert_eq!(first["wind_directions"], "225.0-315.0");
    assert_eq!(first["paragliding"], "true");
    assert_eq!(first["hang_gliding"], "");
    assert_eq!(first["discouraged"], "false");
    assert_eq!(first["revision"], "2");
    assert_eq!(first["created_at"], "2026-10-29T19:05:00Z");
    assert_eq!(first["created_by"], "demo");
    assert_eq!(first["confirmed_pilots"], "2");
    assert_eq!(first["confirmed_latest_day"], "2026-11-16");

    // The removed site is in the history and in neither of the files of
    // the sites as they stand.
    let (_, lines) = history(&file(&router, PILOTS, DumpFormat::Jsonl).await);
    let removed = lines
        .iter()
        .find(|line| line.id.to_string() == FOURTH)
        .unwrap();
    assert!(removed.removed && !removed.revisions.is_empty());
    assert_eq!(
        removed.removal.as_ref().map(|removal| &removal.id),
        Some(&removed.id)
    );
    for format in [DumpFormat::Geojson, DumpFormat::Csv] {
        assert!(!file(&router, PILOTS, format).await.contains(FOURTH));
    }
    // And no file holds anything of the other layer.
    for format in [DumpFormat::Geojson, DumpFormat::Csv, DumpFormat::Jsonl] {
        let partners = file(&router, PARTNERS, format).await;
        assert!(!partners.contains("venti-pilots"), "{format:?}");
        let pilots = file(&router, PILOTS, format).await;
        assert!(!pilots.contains("venti:") && !pilots.contains("\"venti\""));
    }
}

#[tokio::test]
async fn a_program_goes_on_from_a_dump_with_the_changes_since_its_cursor() {
    let router = recorded().await;
    let before = listed(&router).await;
    let cursor = layer(&before, PARTNERS).stamp.cursor.clone();
    let since = |cursor: &str| format!("/v1/sites/changes?layer={PARTNERS}&since={cursor}");

    // Nothing has happened since the dump.
    let nothing: SiteChangesResponse = get(&router, &since(&cursor)).await.ok(StatusCode::OK);
    assert!(nothing.sites.is_empty() && nothing.removed.is_empty() && !nothing.reset);
    assert_eq!(nothing.cursor, cursor);
    let geojson = sites_dump_path(PARTNERS, DumpFormat::Geojson);
    let old = get(&router, &geojson).await;
    let old_tag = old.header(header::ETAG).to_owned();

    // The demo account is a partner: its site goes to the partners' layer.
    let request: AddSiteRequest =
        serde_json::from_str(&std::fs::read_to_string(golden("site_add_request.json")).unwrap())
            .unwrap();
    let added: SiteResponse = ask(Method::POST, SITES_PATH)
        .token(DEMO_TOKEN)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);

    // What changed since the dump's cursor is that site, and the dump as
    // it is made now holds it and stands at the later cursor.
    let changed: SiteChangesResponse = get(&router, &since(&cursor)).await.ok(StatusCode::OK);
    let new: Vec<_> = changed.sites.iter().map(|record| &record.site.id).collect();
    assert_eq!(new, [&added.site.id]);
    let after = listed(&router).await;
    assert_eq!(layer(&after, PARTNERS).stamp.cursor, changed.cursor);
    assert_eq!(layer(&after, PARTNERS).sites, 3);
    // The other layer's dump is the one it was.
    assert_eq!(layer(&after, PILOTS), layer(&before, PILOTS));

    // Whoever has the older file is sent the new one; whoever has the
    // new one is told that it has not changed.
    let again = ask(Method::GET, &geojson)
        .header(header::IF_NONE_MATCH, &old_tag)
        .send(&router)
        .await;
    assert_eq!(again.status, StatusCode::OK);
    assert!(again.text().contains(&added.site.id.to_string()));
    let new_tag = again.header(header::ETAG).to_owned();
    assert_ne!(new_tag, old_tag);
    let unchanged = ask(Method::GET, &geojson)
        .header(header::IF_NONE_MATCH, &new_tag)
        .send(&router)
        .await;
    assert_eq!(unchanged.status, StatusCode::NOT_MODIFIED);
    assert!(unchanged.body.is_empty());
    assert_eq!(unchanged.header(header::ETAG), new_tag);
}

#[tokio::test]
async fn a_dump_is_made_anew_only_after_its_time_and_only_if_the_layer_changed() {
    let clock = TestClock::new();
    let mut settings = settings();
    settings.limits.dump_remake_after = Duration::minutes(5);
    let (router, _) = server(settings, &clock);

    // A layer nobody has written to has a dump too: no site, cursor 0.
    let empty = listed(&router).await;
    let made_at = layer(&empty, PILOTS).stamp.generated_at;
    assert_eq!(
        (
            layer(&empty, PILOTS).sites,
            layer(&empty, PILOTS).stamp.cursor.as_str()
        ),
        (0, "0")
    );
    let geojson: Value =
        serde_json::from_str(&file(&router, PILOTS, DumpFormat::Geojson).await).unwrap();
    assert_eq!(geojson["features"], serde_json::json!([]));
    assert_eq!(
        csv(&file(&router, PILOTS, DumpFormat::Csv).await).len(),
        1,
        "the line of column names and no other"
    );

    // A pilot adds a site. The dump is the one that was made, for as
    // long as the bound says, however often it is asked for.
    let anna = sign_up(&router, "anna").await;
    let request: AddSiteRequest =
        serde_json::from_str(&std::fs::read_to_string(golden("site_add_request.json")).unwrap())
            .unwrap();
    let added: SiteResponse = ask(Method::POST, SITES_PATH)
        .token(&anna.token)
        .json(&request)
        .send(&router)
        .await
        .ok(StatusCode::CREATED);
    assert!(added.site.id.to_string().starts_with("venti-pilots:"));
    clock.advance(Duration::seconds(299));
    assert_eq!(listed(&router).await, empty);

    // After it, the server looks, finds the layer changed and makes the
    // dump anew; the partners' layer has not changed and keeps its own.
    clock.advance(Duration::seconds(1));
    let later = listed(&router).await;
    assert_eq!(layer(&later, PILOTS).sites, 1);
    assert_eq!(
        layer(&later, PILOTS).stamp.generated_at,
        made_at + Duration::minutes(5)
    );
    assert_eq!(layer(&later, PARTNERS), layer(&empty, PARTNERS));
    assert!(
        file(&router, PILOTS, DumpFormat::Csv)
            .await
            .contains(&added.site.id.to_string())
    );

    // And again after the time, with nothing changed: the same files,
    // made when they were.
    clock.advance(Duration::minutes(30));
    assert_eq!(listed(&router).await, later);
}

#[tokio::test]
async fn a_site_that_was_taken_down_shows_nothing_of_what_it_said() {
    let (router, store) = recorded_with_its_store().await;
    let name = "Beispieljoch";
    let before = file(&router, PILOTS, DumpFormat::Jsonl).await;
    assert!(before.contains(name), "the second site's name");

    let take_down = SiteRevising {
        id: SECOND.parse().unwrap(),
        author_id: None,
        at: adapters::recorded_at(),
        via: venti_store::SiteVia::Operator,
        act: SiteAct::Remove { take_down: true },
        comment: None,
        expects: None,
    };
    store
        .revise_pilot_site(&take_down, pilot_sites::APART_M)
        .await
        .unwrap();

    // The history says that the site is removed and when, as the changes
    // do, and nothing else: no revision, no name.
    let text = file(&router, PILOTS, DumpFormat::Jsonl).await;
    let (head, lines) = history(&text);
    let taken = lines
        .iter()
        .find(|line| line.id.to_string() == SECOND)
        .unwrap();
    assert!(taken.removed && taken.revisions.is_empty());
    let removal = taken.removal.as_ref().unwrap();
    assert_eq!((removal.name.as_ref(), removal.by.as_ref()), (None, None));
    assert!(!text.contains(name));
    assert_eq!(head.sites, 2);
    // It is in neither file of the sites as they stand.
    for format in [DumpFormat::Geojson, DumpFormat::Csv] {
        let standing = file(&router, PILOTS, format).await;
        assert!(!standing.contains(SECOND) && !standing.contains(name));
    }
    assert_eq!(layer(&listed(&router).await, PILOTS).sites, 0);
}

#[tokio::test]
async fn a_file_goes_out_compressed_and_to_a_page_of_any_origin() {
    let router = recorded().await;
    let path = sites_dump_path(PARTNERS, DumpFormat::Jsonl);
    let plain = get(&router, &path).await;
    assert!(!plain.headers.contains_key(header::CONTENT_ENCODING));
    assert!(
        plain
            .header_values(header::VARY)
            .contains("accept-encoding")
    );

    let packed = ask(Method::GET, &path)
        .header(header::ACCEPT_ENCODING, "gzip")
        .header(header::ORIGIN, "https://another-maker.example")
        .send(&router)
        .await;
    assert_eq!(packed.status, StatusCode::OK);
    assert_eq!(packed.header(header::CONTENT_ENCODING), "gzip");
    assert_eq!(packed.header(header::ETAG), plain.header(header::ETAG));
    let mut unpacked = Vec::new();
    flate2::read::GzDecoder::new(&packed.body[..])
        .read_to_end(&mut unpacked)
        .unwrap();
    assert_eq!(unpacked, plain.body);
    // An open read: any page may fetch it and read its validator.
    assert_eq!(packed.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), "*");
    assert!(
        packed
            .header(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .contains("etag")
    );

    // The list is compressed as every other answer is.
    let list = ask(Method::GET, DUMPS_PATH)
        .header(header::ACCEPT_ENCODING, "gzip")
        .send(&router)
        .await;
    assert_eq!(list.header(header::CONTENT_ENCODING), "gzip");

    // `HEAD` answers as `GET` does, without the file.
    let head = ask(Method::HEAD, &path).send(&router).await;
    assert_eq!(head.status, StatusCode::OK);
    assert!(head.body.is_empty());
    assert_eq!(head.header(header::ETAG), plain.header(header::ETAG));
}

#[tokio::test]
async fn only_the_files_there_are_have_an_address() {
    let router = recorded().await;
    for name in [
        "venti.json",
        "venti",
        "venti.geojson.gz",
        "osm-free-flying.geojson",
        "venti-pilots.history.csv",
    ] {
        let message = get(&router, &format!("{DUMPS_PATH}/sites/{name}"))
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
        assert!(message.contains("GET /v1/dumps"), "{name}: {message}");
    }
    // Nothing is written through these addresses.
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let answer = ask(method, &sites_dump_path(PARTNERS, DumpFormat::Csv))
            .token(DEMO_TOKEN)
            .send(&router)
            .await;
        assert_eq!(answer.status, StatusCode::METHOD_NOT_ALLOWED);
    }
}
