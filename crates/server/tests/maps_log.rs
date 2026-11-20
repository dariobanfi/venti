//! What the server writes to its log about its map regions: why one is
//! not handed out, and nothing for a request.
//!
//! Like `log.rs`, this test has a file, and so a process, to itself: it
//! collects the log of its own thread.

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::http::{StatusCode, header};
use sha2::{Digest, Sha256};
use venti_api_types::v1::{MAPS_PATH, map_file_path};
use venti_server::api::{Api, Cors};
use venti_server::maps::{DESCRIPTION, Maps};
use venti_store::MemoryStore;

mod common;

use common::{TestClock, get, get_with, settings};

/// Collects what the server logs while a test holds it.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    /// The lines logged so far that contain `text`.
    fn lines_with(&self, text: &str) -> Vec<String> {
        let log = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
        log.lines()
            .filter(|line| line.contains(text))
            .map(str::to_owned)
            .collect()
    }

    fn lines(&self) -> usize {
        self.lines_with("").len()
    }
}

fn sha256(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const FILE: &str = "basemap.pmtiles";

/// Writes a region with one file of `content`, whose description states
/// the hash of `described`.
fn region(folder: &Path, name: &str, id: &str, content: &[u8], described: &[u8]) {
    let path = folder.join(name);
    std::fs::create_dir_all(&path).unwrap();
    let description = format!(
        r#"id = "{id}"
name = "A valley"
version = "2026-11-20"
covers = {{ south = 47.6, west = 11.64, north = 47.76, east = 11.86 }}

[[files]]
role = "basemap"
name = "{FILE}"
sha256 = "{}"
credits = [{{ text = "© OpenStreetMap contributors" }}]
"#,
        sha256(described)
    );
    std::fs::write(path.join(DESCRIPTION), description).unwrap();
    std::fs::write(path.join(FILE), content).unwrap();
}

#[tokio::test]
async fn the_log_says_why_a_region_is_left_out_and_nothing_about_a_request() {
    let captured = Captured::default();
    let writer = captured.clone();
    // The level a server runs at unless it is told otherwise.
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let folder = std::env::temp_dir().join(format!("venti-maps-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let good: Vec<u8> = (0..5000u32).map(|n| (n % 251) as u8).collect();
    let newer = b"a newer build of the file";
    region(&folder, "good", "good", &good, &good);
    region(&folder, "stale", "stale", newer, &good);
    region(&folder, "shouting", "Shouting Valley", &good, &good);
    region(&folder, "twin-a", "twin", &good, &good);
    region(&folder, "twin-b", "twin", &good, &good);

    let maps = Maps::default();
    let reading = maps.read(&folder);

    assert_eq!(reading.listed, ["good"]);
    assert_eq!(reading.left_out, 4);
    // One line for the region that is handed out.
    let handed_out = captured.lines_with("handing out good");
    assert_eq!(handed_out.len(), 1, "{handed_out:?}");
    assert!(handed_out[0].contains("version 2026-11-20"));
    // A file with another hash: the folder, the file and both hashes.
    let stale = captured.lines_with("stale");
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert!(stale[0].contains("ERROR"), "{}", stale[0]);
    assert!(stale[0].contains("is not handed out"), "{}", stale[0]);
    assert!(stale[0].contains(FILE), "{}", stale[0]);
    assert!(stale[0].contains(&sha256(newer)), "{}", stale[0]);
    assert!(stale[0].contains(&sha256(&good)), "{}", stale[0]);
    // A name that is no name: the folder and the rule.
    let shouting = captured.lines_with("shouting");
    assert_eq!(shouting.len(), 1, "{shouting:?}");
    assert!(shouting[0].contains("the id \"Shouting Valley\""));
    // Two folders with one identifier: both, in one line.
    let twins = captured.lines_with("twin");
    assert_eq!(twins.len(), 1, "{twins:?}");
    assert!(twins[0].contains("twin-a") && twins[0].contains("twin-b"));
    assert!(twins[0].contains("none of them is handed out"));

    // Requests leave no line, whatever they ask for and however often.
    let store = Arc::new(MemoryStore::new());
    let router = Api::new(store, Arc::new(TestClock::new()), [], settings())
        .with_maps(maps)
        .router(&Cors::Any);
    let before = captured.lines();
    let path = map_file_path("good", "2026-11-20", FILE);
    for _ in 0..200 {
        assert_eq!(get(&router, MAPS_PATH).await.status, StatusCode::OK);
        assert_eq!(get(&router, &path).await.status, StatusCode::OK);
        let beyond = get_with(&router, &path, &[(header::RANGE, "bytes=5000-")]).await;
        assert_eq!(beyond.status, StatusCode::RANGE_NOT_SATISFIABLE);
        for unknown in ["stale", "twin", "..%2F..%2Fetc"] {
            let answer = get(&router, &map_file_path(unknown, "2026-11-20", FILE)).await;
            assert_eq!(answer.status, StatusCode::NOT_FOUND);
        }
    }
    assert_eq!(captured.lines(), before);

    // A file that is replaced while the server runs is one line, when the
    // first request finds it, and none after.
    std::fs::write(folder.join("good").join(FILE), newer).unwrap();
    for _ in 0..50 {
        assert_eq!(get(&router, &path).await.status, StatusCode::NOT_FOUND);
    }
    let withdrawn = captured.lines_with("no longer handed out");
    assert_eq!(withdrawn.len(), 1, "{withdrawn:?}");
    assert!(withdrawn[0].contains("Start the server again"));
    assert_eq!(captured.lines(), before + 1);

    std::fs::remove_dir_all(&folder).unwrap();
}
