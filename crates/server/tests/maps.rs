//! The map regions a server hands out: the list, and the files with their
//! ranges, through the real router.
//!
//! The region the apps bundle (`apps/ios/Venti/Resources/MapDemo`) is the
//! one region that is checked in, and the list of it is the example in
//! `docs/api.md`. Everything about parts of a file runs over a small file
//! that a test writes and knows byte for byte.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use venti_api_types::v1::{ErrorCode, MAPS_PATH, MapFileRole, MapsResponse, map_file_path};
use venti_domain::BoundingBox;
use venti_server::api::{Api, Cors};
use venti_server::maps::{DESCRIPTION, Maps};
use venti_sources::testing::assert_golden;
use venti_store::MemoryStore;

mod common;

use common::{Answer, TestClock, WEB_APP, ask, get, get_with, golden, recorded, send, settings};

// --- Helpers ------------------------------------------------------------------

/// The folder that holds the region the apps bundle, which is what
/// `venti-server dev --maps apps/ios/Venti/Resources` is given.
fn bundled() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/ios/Venti/Resources")
}

/// A server that hands out the regions in `folder`, read as a server reads
/// them at its start.
fn server_of(folder: &Path) -> Router {
    let maps = Maps::default();
    maps.read(folder);
    server_with(maps)
}

fn server_with(maps: Maps) -> Router {
    let store = Arc::new(MemoryStore::new());
    Api::new(store, Arc::new(TestClock::new()), [], settings())
        .with_maps(maps)
        .router(&Cors::Origins(vec![WEB_APP.to_owned()]))
}

fn sha256(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A folder of regions that a test fills and that goes when the test is
/// over.
struct Folder(PathBuf);

impl Folder {
    fn new(test: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("venti-maps-api-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// Writes a region with one file, `basemap.pmtiles`, of `content`.
    /// Its description states the hash of `described`.
    fn region(&self, id: &str, content: &[u8], described: &[u8]) -> PathBuf {
        let path = self.0.join(id);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(DESCRIPTION), description(id, described)).unwrap();
        std::fs::write(path.join(FILE), content).unwrap();
        path
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const FILE: &str = "basemap.pmtiles";
const VERSION: &str = "2026-11-20";

fn description(id: &str, content: &[u8]) -> String {
    format!(
        r#"id = "{id}"
name = "A valley"
version = "{VERSION}"
covers = {{ south = 47.6, west = 11.64, north = 47.76, east = 11.86 }}

[[files]]
role = "basemap"
name = "{FILE}"
sha256 = "{}"
credits = [{{ text = "© OpenStreetMap contributors", url = "https://www.openstreetmap.org/copyright" }}]
"#,
        sha256(content)
    )
}

/// 5,000 bytes that are the same nowhere twice in a row, and that a
/// compressor would still make much smaller.
fn content() -> Vec<u8> {
    (0..5000u32).map(|n| (n % 251) as u8).collect()
}

/// A server with one region, `a-valley`, whose one file is [`content`].
fn a_valley(folder: &Folder) -> (Router, String) {
    folder.region("a-valley", &content(), &content());
    (
        server_of(&folder.0),
        map_file_path("a-valley", VERSION, FILE),
    )
}

async fn range(router: &Router, path: &str, range: &str) -> Answer {
    get_with(router, path, &[(header::RANGE, range)]).await
}

/// Checks that an answer is the bytes `first..=last` of a file of 5,000.
fn is_part(answer: &Answer, first: usize, last: usize) {
    assert_eq!(answer.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        answer.header(header::CONTENT_RANGE),
        format!("bytes {first}-{last}/5000")
    );
    assert_eq!(
        answer.header(header::CONTENT_LENGTH),
        (last - first + 1).to_string()
    );
    assert_eq!(answer.body, &content()[first..=last]);
    assert_eq!(answer.header(header::ACCEPT_RANGES), "bytes");
    assert!(!answer.headers.contains_key(header::CONTENT_ENCODING));
}

// --- The list -------------------------------------------------------------------

#[tokio::test]
async fn the_bundled_region_is_listed_with_its_files_as_they_are_on_the_disk() {
    let router = server_of(&bundled());

    let answer = get(&router, MAPS_PATH).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "application/json");
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert!(answer.header(header::ETAG).starts_with("W/\""));
    let maps: MapsResponse = answer.json();
    assert_eq!(maps.regions.len(), 1);
    let region = &maps.regions[0];
    assert_eq!(region.id, "tegernsee-demo");
    assert_eq!(region.name, "Tegernsee valley (demo)");
    // The box `tools/demo-map/build.sh` builds.
    assert_eq!(
        region.covers,
        BoundingBox::new(47.60, 11.64, 47.76, 11.86).unwrap()
    );
    let named: Vec<(MapFileRole, &str)> = region
        .files
        .iter()
        .map(|file| (file.role, file.name.as_str()))
        .collect();
    assert_eq!(
        named,
        [
            (MapFileRole::Basemap, "basemap.pmtiles"),
            (MapFileRole::Terrain, "terrain.pmtiles")
        ]
    );
    for file in &region.files {
        let on_disk = std::fs::read(bundled().join("MapDemo").join(&file.name)).unwrap();
        assert_eq!(file.bytes, on_disk.len() as u64, "{}", file.name);
        assert_eq!(file.sha256, sha256(&on_disk), "{}", file.name);
        assert_eq!(
            file.path,
            map_file_path(&region.id, &region.version, &file.name)
        );
        assert!(!file.credits.is_empty());
        for credit in &file.credits {
            assert!(credit.text.starts_with('©'), "{}", credit.text);
            assert!(credit.url.as_deref().unwrap().starts_with("https://"));
        }
    }
    // Each file's credits are its own: OpenStreetMap's data is not
    // credited to the elevation, nor the other way round.
    let credited = |index: usize| -> String {
        let texts: Vec<&str> = region.files[index]
            .credits
            .iter()
            .map(|credit| credit.text.as_str())
            .collect();
        texts.join(", ")
    };
    assert_eq!(credited(0), "© OpenStreetMap contributors, © OpenMapTiles");
    assert_eq!(credited(1), "© Mapterhorn and the surveys it combines");
    assert_golden(&golden("maps.json"), &maps);

    // Asked again with its validator, the list is not sent again.
    let tag = answer.header(header::ETAG).to_owned();
    let again = get_with(&router, MAPS_PATH, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn a_server_without_a_maps_folder_lists_no_region() {
    // The development mode as it starts without `--maps`.
    let router = recorded().await;

    let answer = get(&router, MAPS_PATH).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.text(), r#"{"regions":[]}"#);
    let file = get(
        &router,
        &map_file_path("tegernsee-demo", "2026-11-18", FILE),
    )
    .await;
    file.error(StatusCode::NOT_FOUND, ErrorCode::MapNotFound);
}

// --- A file -----------------------------------------------------------------------

#[tokio::test]
async fn a_whole_file_arrives_as_it_is_on_the_disk() {
    let router = server_of(&bundled());
    let maps: MapsResponse = get(&router, MAPS_PATH).await.json();

    for file in &maps.regions[0].files {
        let answer = get(&router, &file.path).await;

        assert_eq!(answer.status, StatusCode::OK, "{}", file.name);
        assert_eq!(
            answer.header(header::CONTENT_TYPE),
            "application/octet-stream"
        );
        assert_eq!(
            answer.header(header::CONTENT_LENGTH),
            file.bytes.to_string()
        );
        assert_eq!(answer.header(header::ACCEPT_RANGES), "bytes");
        assert_eq!(
            answer.header(header::CACHE_CONTROL),
            "public, max-age=31536000, immutable"
        );
        // The validator is the hash the list states, in quotes.
        assert_eq!(answer.header(header::ETAG), format!("\"{}\"", file.sha256));
        assert_eq!(answer.header(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
        // What a client does with its copy: check it against the list.
        assert_eq!(answer.body.len() as u64, file.bytes);
        assert_eq!(sha256(&answer.body), file.sha256, "{}", file.name);
    }
}

#[tokio::test]
async fn a_part_of_a_file_is_answered_from_its_beginning_its_middle_and_its_end() {
    let folder = Folder::new("ranges");
    let (router, path) = a_valley(&folder);

    // The first bytes, bytes from the middle, and one byte.
    is_part(&range(&router, &path, "bytes=0-99").await, 0, 99);
    is_part(&range(&router, &path, "bytes=1000-1999").await, 1000, 1999);
    is_part(&range(&router, &path, "bytes=4999-4999").await, 4999, 4999);
    // From somewhere to the end: how a download goes on.
    is_part(&range(&router, &path, "bytes=4900-").await, 4900, 4999);
    is_part(&range(&router, &path, "bytes=0-").await, 0, 4999);
    // The last bytes.
    is_part(&range(&router, &path, "bytes=-100").await, 4900, 4999);
    // A range that ends after the file ends with the file.
    is_part(&range(&router, &path, "bytes=4000-9999").await, 4000, 4999);
    is_part(&range(&router, &path, "bytes=-9999").await, 0, 4999);

    // A part says what the whole says about the file.
    let part = range(&router, &path, "bytes=0-99").await;
    assert_eq!(
        part.header(header::CONTENT_TYPE),
        "application/octet-stream"
    );
    assert_eq!(
        part.header(header::ETAG),
        format!("\"{}\"", sha256(&content()))
    );
    assert_eq!(
        part.header(header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn a_range_that_begins_after_the_file_is_refused_with_the_files_length() {
    let folder = Folder::new("beyond");
    let (router, path) = a_valley(&folder);

    for beyond in ["bytes=5000-", "bytes=5000-5999", "bytes=9999-", "bytes=-0"] {
        let answer = range(&router, &path, beyond).await;

        let message = answer.error(
            StatusCode::RANGE_NOT_SATISFIABLE,
            ErrorCode::RangeNotSatisfiable,
        );
        assert!(message.contains("5000 bytes"), "{beyond}: {message}");
        assert_eq!(answer.header(header::CONTENT_RANGE), "bytes */5000");
        assert_eq!(answer.header(header::ACCEPT_RANGES), "bytes");
    }
}

#[tokio::test]
async fn what_is_no_single_range_of_bytes_gets_the_whole_file() {
    let folder = Folder::new("no-range");
    let (router, path) = a_valley(&folder);

    for other in ["bytes=0-99,200-299", "bytes=99-0", "items=0-99", "bytes=x"] {
        let answer = range(&router, &path, other).await;

        assert_eq!(answer.status, StatusCode::OK, "{other}");
        assert!(!answer.headers.contains_key(header::CONTENT_RANGE));
        assert_eq!(answer.header(header::CONTENT_LENGTH), "5000");
        assert_eq!(answer.body, content(), "{other}");
    }
}

#[tokio::test]
async fn head_says_what_get_would_send_and_sends_nothing() {
    let folder = Folder::new("head");
    let (router, path) = a_valley(&folder);

    let answer = ask(Method::HEAD, &path).send(&router).await;

    assert_eq!(answer.status, StatusCode::OK);
    assert!(answer.body.is_empty());
    assert_eq!(answer.header(header::CONTENT_LENGTH), "5000");
    assert_eq!(
        answer.header(header::CONTENT_TYPE),
        "application/octet-stream"
    );
    assert_eq!(answer.header(header::ACCEPT_RANGES), "bytes");
    assert_eq!(
        answer.header(header::ETAG),
        format!("\"{}\"", sha256(&content()))
    );
    assert_eq!(
        answer.header(header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );

    // A range belongs to a `GET`: here it changes nothing.
    let ranged = ask(Method::HEAD, &path)
        .header(header::RANGE, "bytes=0-99")
        .send(&router)
        .await;
    assert_eq!(ranged.status, StatusCode::OK);
    assert_eq!(ranged.header(header::CONTENT_LENGTH), "5000");
    assert!(!ranged.headers.contains_key(header::CONTENT_RANGE));

    // And a file that is not there is not there for `HEAD` either.
    let missing = ask(
        Method::HEAD,
        &map_file_path("a-valley", VERSION, "x.pmtiles"),
    )
    .send(&router)
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

/// The first of the two traps. A phone asks for gzip with every request.
/// The API's other answers are compressed then; a compressed answer has no
/// length and no ranges, so a download could neither show its progress nor
/// go on where it broke off.
#[tokio::test]
async fn a_file_is_never_compressed_whatever_the_request_accepts() {
    let folder = Folder::new("identity");
    let (router, path) = a_valley(&folder);
    // The compression is there, and would take this file: the list, a
    // fraction of its size, is compressed.
    let list = get_with(&router, MAPS_PATH, &[(header::ACCEPT_ENCODING, "gzip")]).await;
    assert_eq!(list.header(header::CONTENT_ENCODING), "gzip");

    for accepted in ["gzip", "gzip, deflate, br, zstd", "*"] {
        let whole = get_with(&router, &path, &[(header::ACCEPT_ENCODING, accepted)]).await;

        assert_eq!(whole.status, StatusCode::OK);
        assert!(
            !whole.headers.contains_key(header::CONTENT_ENCODING),
            "{accepted}"
        );
        assert_eq!(whole.header(header::CONTENT_LENGTH), "5000");
        assert_eq!(whole.header(header::ACCEPT_RANGES), "bytes");
        assert_eq!(whole.body, content());

        let part = get_with(
            &router,
            &path,
            &[
                (header::ACCEPT_ENCODING, accepted),
                (header::RANGE, "bytes=100-199"),
            ],
        )
        .await;
        is_part(&part, 100, 199);

        let head = ask(Method::HEAD, &path)
            .header(header::ACCEPT_ENCODING, accepted)
            .send(&router)
            .await;
        assert!(!head.headers.contains_key(header::CONTENT_ENCODING));
        assert_eq!(head.header(header::CONTENT_LENGTH), "5000");
    }
}

#[tokio::test]
async fn a_client_that_has_the_file_is_not_sent_it_again() {
    let folder = Folder::new("validators");
    let (router, path) = a_valley(&folder);
    let tag = format!("\"{}\"", sha256(&content()));

    let same = get_with(&router, &path, &[(header::IF_NONE_MATCH, &tag)]).await;
    assert_eq!(same.status, StatusCode::NOT_MODIFIED);
    assert!(same.body.is_empty());
    assert_eq!(same.header(header::ETAG), tag);
    assert_eq!(
        same.header(header::CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );

    let other = get_with(&router, &path, &[(header::IF_NONE_MATCH, "\"another\"")]).await;
    assert_eq!(other.status, StatusCode::OK);
    assert_eq!(other.body, content());
}

#[tokio::test]
async fn a_download_goes_on_only_with_the_file_it_began_with() {
    let folder = Folder::new("if-range");
    let (router, path) = a_valley(&folder);
    let tag = format!("\"{}\"", sha256(&content()));
    let going_on = |validator: String| {
        let (router, path) = (router.clone(), path.clone());
        async move {
            let asked = [
                (header::RANGE, "bytes=3000-"),
                (header::IF_RANGE, validator.as_str()),
            ];
            get_with(&router, &path, &asked).await
        }
    };

    // The file the client began with: the rest of it.
    is_part(&going_on(tag.clone()).await, 3000, 4999);

    // Another file, or a date, which the server gives no file: the whole
    // file, so that no client glues the end of one to the start of
    // another.
    for validator in ["\"another\"", "Fri, 20 Nov 2026 10:00:00 GMT"] {
        let whole = going_on(validator.to_owned()).await;
        assert_eq!(whole.status, StatusCode::OK, "{validator}");
        assert_eq!(whole.body, content());
    }
}

// --- What is not there ------------------------------------------------------------

#[tokio::test]
async fn an_unknown_region_version_or_name_is_not_found_and_nothing_else_is_opened() {
    let folder = Folder::new("unknown");
    let (router, path) = a_valley(&folder);
    assert_eq!(get(&router, &path).await.status, StatusCode::OK);

    for unknown in [
        map_file_path("another", VERSION, FILE),
        map_file_path("a-valley", "2026-11-19", FILE),
        map_file_path("a-valley", VERSION, "terrain.pmtiles"),
        // The description lies in the region's folder and is no file of
        // the region.
        map_file_path("a-valley", VERSION, DESCRIPTION),
        // The folder's name is not the region's.
        map_file_path("A-VALLEY", VERSION, FILE),
        // A step up, written so that it arrives as one part of the path.
        map_file_path("a-valley", VERSION, "..%2Fa-valley%2Fbasemap.pmtiles"),
        map_file_path("a-valley", "..", FILE),
        map_file_path("a-valley", VERSION, "%2Fetc%2Fhostname"),
    ] {
        let answer = get(&router, &unknown).await;
        let message = answer.error(StatusCode::NOT_FOUND, ErrorCode::MapNotFound);
        assert!(message.contains(MAPS_PATH), "{unknown}: {message}");
    }

    // A path with fewer or more parts is no part of the API.
    for path in ["/v1/maps/a-valley", "/v1/maps/a-valley/2026-11-20"] {
        let answer = get(&router, path).await;
        answer.error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    }
    let deeper = format!("{path}/more");
    get(&router, &deeper)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);

    // And a file is read, never written.
    for method in [Method::POST, Method::PUT, Method::DELETE] {
        let answer = ask(method, &path).send(&router).await;
        answer.error(StatusCode::METHOD_NOT_ALLOWED, ErrorCode::MethodNotAllowed);
    }
}

#[tokio::test]
async fn a_region_whose_hash_is_wrong_is_not_listed_and_its_file_is_not_served() {
    let folder = Folder::new("wrong-hash");
    folder.region("good", &content(), &content());
    // The file is not the one its description was written for.
    folder.region("stale", b"a newer build of the file", &content());

    let router = server_of(&folder.0);

    let maps: MapsResponse = get(&router, MAPS_PATH).await.json();
    let ids: Vec<&str> = maps.regions.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["good"]);
    get(&router, &map_file_path("stale", VERSION, FILE))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::MapNotFound);
    assert_eq!(
        get(&router, &map_file_path("good", VERSION, FILE))
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_region_with_a_bad_name_or_a_file_outside_its_folder_is_not_listed() {
    let folder = Folder::new("bad-names");
    folder.region("good", &content(), &content());
    // An identifier that is no step of a path.
    let shouting = folder.region("shouting", &content(), &content());
    std::fs::write(
        shouting.join(DESCRIPTION),
        description("Shouting Valley", &content()),
    )
    .unwrap();
    // A version that leads elsewhere.
    let versioned = folder.region("versioned", &content(), &content());
    std::fs::write(
        versioned.join(DESCRIPTION),
        description("versioned", &content()).replace(VERSION, "../2026"),
    )
    .unwrap();
    // A file named by a path: the file exists, one folder up.
    std::fs::write(folder.0.join("outside.pmtiles"), content()).unwrap();
    let reaching = folder.region("reaching", &content(), &content());
    std::fs::write(
        reaching.join(DESCRIPTION),
        description("reaching", &content()).replace(FILE, "../outside.pmtiles"),
    )
    .unwrap();
    // A file that is a link to that file. Its name is fine and its hash
    // is right.
    #[cfg(unix)]
    {
        let linked = folder.region("linked", &content(), &content());
        std::fs::remove_file(linked.join(FILE)).unwrap();
        std::os::unix::fs::symlink(folder.0.join("outside.pmtiles"), linked.join(FILE)).unwrap();
    }
    // Two folders that call their region the same.
    folder.region("twin-a", &content(), &content());
    let twin = folder.region("twin-b", &content(), &content());
    std::fs::write(twin.join(DESCRIPTION), description("twin-a", &content())).unwrap();

    let router = server_of(&folder.0);

    let maps: MapsResponse = get(&router, MAPS_PATH).await.json();
    let ids: Vec<&str> = maps.regions.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["good"]);
    for unlisted in ["reaching", "linked", "twin-a", "versioned"] {
        for name in [FILE, "outside.pmtiles", "..%2Foutside.pmtiles"] {
            get(&router, &map_file_path(unlisted, VERSION, name))
                .await
                .error(StatusCode::NOT_FOUND, ErrorCode::MapNotFound);
        }
    }
}

#[tokio::test]
async fn a_file_that_was_replaced_after_the_start_takes_its_region_off_the_list() {
    let folder = Folder::new("replaced");
    folder.region("other", &content(), &content());
    let (router, path) = a_valley(&folder);
    assert_eq!(get(&router, &path).await.status, StatusCode::OK);

    // A new build is copied over the old one while the server runs. The
    // list still states the old hash, which no download would match.
    std::fs::write(folder.0.join("a-valley").join(FILE), b"a newer build").unwrap();

    get(&router, &path)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::MapNotFound);
    let maps: MapsResponse = get(&router, MAPS_PATH).await.json();
    let ids: Vec<&str> = maps.regions.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["other"]);

    // A file that is taken away does the same to its region.
    let other = map_file_path("other", VERSION, FILE);
    assert_eq!(get(&router, &other).await.status, StatusCode::OK);
    std::fs::remove_file(folder.0.join("other").join(FILE)).unwrap();
    get(&router, &other)
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::MapNotFound);
    assert_eq!(get(&router, MAPS_PATH).await.text(), r#"{"regions":[]}"#);
}

// --- Browsers -----------------------------------------------------------------------

#[tokio::test]
async fn a_page_of_the_web_app_may_ask_for_a_part_of_a_file() {
    let folder = Folder::new("cors");
    let (router, path) = a_valley(&folder);

    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri(&path)
        .header(header::ORIGIN, WEB_APP)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "range, if-range")
        .body(Body::empty())
        .unwrap();
    let preflight = send(&router, preflight).await;
    assert_eq!(preflight.status, StatusCode::OK);
    assert_eq!(
        preflight.header(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        WEB_APP
    );
    let allowed = preflight.header(header::ACCESS_CONTROL_ALLOW_HEADERS);
    assert!(
        allowed.contains("range") && allowed.contains("if-range"),
        "{allowed}"
    );

    let part = get_with(
        &router,
        &path,
        &[(header::ORIGIN, WEB_APP), (header::RANGE, "bytes=0-99")],
    )
    .await;
    is_part(&part, 0, 99);
    assert_eq!(part.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
    let exposed = part.header(header::ACCESS_CONTROL_EXPOSE_HEADERS);
    assert!(
        exposed.contains("content-range") && exposed.contains("etag"),
        "{exposed}"
    );
}

// --- A download that takes its time ---------------------------------------------------

/// The second of the two traps. A region is gigabytes, and a phone in a
/// valley takes many minutes over it. Nothing on the way of a file may
/// give up after a time: here every piece of several downloads is taken
/// an hour after the one before, which is far beyond every deadline the
/// other routes have.
#[tokio::test(start_paused = true)]
async fn a_download_that_takes_hours_is_not_cut_short() {
    let folder = Folder::new("slow");
    // Large enough to be read from the disk in several pieces.
    let large: Vec<u8> = (0..1_500_000u32).map(|n| (n % 241) as u8).collect();
    folder.region("large", &large, &large);
    let router = server_of(&folder.0);
    let path = map_file_path("large", VERSION, FILE);
    let hour = std::time::Duration::from_secs(3600);

    // Three downloads at once, one of them of a part.
    let mut downloads = Vec::new();
    for range in [None, None, Some("bytes=500000-")] {
        let mut request = Request::builder().method(Method::GET).uri(&path);
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        let response = router
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(response.status().is_success());
        let from = if range.is_some() { 500_000 } else { 0 };
        downloads.push((response.into_body().into_data_stream(), Vec::new(), from));
    }

    let mut pieces = 0;
    loop {
        tokio::time::advance(hour).await;
        let mut arrived = false;
        for (body, received, _) in &mut downloads {
            if let Some(piece) = body.next().await {
                received.extend_from_slice(&piece.expect("the download broke off"));
                arrived = true;
                pieces += 1;
            }
        }
        if !arrived {
            break;
        }
    }

    assert!(pieces > 6, "the file arrived in {pieces} pieces");
    for (_, received, from) in &downloads {
        assert_eq!(received.len(), large.len() - from);
        assert!(received == &large[*from..]);
    }
}

/// The other side of the same thing: a download that may take hours must
/// not keep the server from stopping. When the server is told to stop,
/// what is under way ends there. The client has a part of the file and
/// takes the download up again by the range it lacks.
#[tokio::test]
async fn a_download_ends_when_the_server_is_told_to_stop() {
    let folder = Folder::new("stopping");
    let large: Vec<u8> = (0..1_500_000u32).map(|n| (n % 241) as u8).collect();
    folder.region("large", &large, &large);
    let stop = CancellationToken::new();
    let maps = Maps::new(stop.clone());
    maps.read(&folder.0);
    let router = server_with(maps);
    let request = Request::builder()
        .uri(map_file_path("large", VERSION, FILE))
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body().into_data_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(!first.is_empty() && first.len() < large.len());
    assert!(first[..] == large[..first.len()]);

    stop.cancel();

    // Not one piece more.
    assert!(body.next().await.is_none());
    // And a region is not read after that either.
    let late = Maps::new(stop);
    assert!(late.read(&folder.0).stopped);
    assert!(late.list().regions.is_empty());
}

// --- What goes over the wire ----------------------------------------------------------

/// One answer as it arrived on a connection.
struct Wire {
    /// The status line and the headers, in lowercase.
    head: String,
    body: Vec<u8>,
}

impl Wire {
    fn header(&self, name: &str) -> Option<&str> {
        self.head
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}: ")))
    }
}

/// Sends one request over a connection of its own and reads the answer as
/// it comes, with nothing in between that would decode or complete it. If
/// `at_most` is given, the connection is dropped once that many bytes of
/// the body have arrived, as when a phone loses its network.
async fn over_the_wire(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    at_most: Option<usize>,
) -> Wire {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: venti.example\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut received = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let end_of_head = loop {
        if let Some(at) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0, "the connection closed before the headers ended");
        received.extend_from_slice(&chunk[..count]);
    };
    loop {
        if at_most.is_some_and(|most| received.len() - end_of_head >= most) {
            break;
        }
        let count = stream.read(&mut chunk).await.unwrap();
        if count == 0 {
            break;
        }
        received.extend_from_slice(&chunk[..count]);
    }
    let mut body = received.split_off(end_of_head);
    if let Some(most) = at_most {
        body.truncate(most);
    }
    Wire {
        head: String::from_utf8(received).unwrap().to_ascii_lowercase(),
        body,
    }
}

/// What a phone sees: the server as it listens, asked over a socket. The
/// tests above ask the router, which is everything but the last step;
/// this is the last step. A length must stand in the headers, the bytes
/// must come as they are, and a download that broke off must be able to
/// go on.
#[tokio::test]
async fn over_a_connection_a_file_has_its_length_and_a_broken_download_goes_on() {
    let folder = Folder::new("wire");
    let large: Vec<u8> = (0..700_000u32).map(|n| (n % 239) as u8).collect();
    folder.region("large", &large, &large);
    let router = server_of(&folder.0);
    let path = map_file_path("large", VERSION, FILE);
    let tag = format!("\"{}\"", sha256(&large));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    let serving = tokio::spawn(async move { axum::serve(listener, service).await });
    let gzip = ("Accept-Encoding", "gzip, deflate, br");

    // How large is it? A `HEAD` says, and sends nothing.
    let head = over_the_wire(address, "HEAD", &path, &[gzip], None).await;
    assert!(head.head.starts_with("http/1.1 200"), "{}", head.head);
    assert_eq!(head.header("content-length"), Some("700000"));
    assert_eq!(head.header("accept-ranges"), Some("bytes"));
    assert!(head.body.is_empty());

    // The download begins, and breaks off after 200,000 bytes.
    let begun = over_the_wire(address, "GET", &path, &[gzip], Some(200_000)).await;
    assert!(begun.head.starts_with("http/1.1 200"), "{}", begun.head);
    assert_eq!(begun.header("content-length"), Some("700000"));
    assert_eq!(begun.header("content-encoding"), None);
    assert_eq!(begun.header("transfer-encoding"), None);
    assert_eq!(begun.header("etag"), Some(tag.as_str()));
    assert_eq!(begun.body.len(), 200_000);

    // It goes on from there, for the file it began with.
    let rest = over_the_wire(
        address,
        "GET",
        &path,
        &[gzip, ("Range", "bytes=200000-"), ("If-Range", &tag)],
        None,
    )
    .await;
    assert!(rest.head.starts_with("http/1.1 206"), "{}", rest.head);
    assert_eq!(
        rest.header("content-range"),
        Some("bytes 200000-699999/700000")
    );
    assert_eq!(rest.header("content-length"), Some("500000"));
    assert_eq!(rest.header("content-encoding"), None);
    assert_eq!(rest.header("transfer-encoding"), None);

    let mut whole = begun.body;
    whole.extend_from_slice(&rest.body);
    assert_eq!(whole.len(), large.len());
    assert!(whole == large);
    assert_eq!(sha256(&whole), sha256(&large));

    // The list, for comparison, is compressed on the same connection
    // rules: the compression is there, and the files are kept from it.
    let list = over_the_wire(address, "GET", MAPS_PATH, &[gzip], None).await;
    assert_eq!(list.header("content-encoding"), Some("gzip"));

    serving.abort();
}
