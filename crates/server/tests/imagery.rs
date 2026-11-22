//! Map imagery through the real router: basemap.at's tiles recorded on
//! 2026-11-20 and IGN's recorded on 2026-11-22, cut along Austria's and
//! France's outlines by the real adapters, fetched from a stand-in for the
//! provider that counts and notes what it is asked, and kept in a folder
//! each test makes and removes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::http::{Method, StatusCode, header};
use futures::future::BoxFuture;
use venti_api_types::v1::ErrorCode;
use venti_server::adapters;
use venti_server::api::{Api, Cors};
use venti_server::dev;
use venti_server::imagery::{Imagery, Limits, TileCache, TileFetch};
use venti_sources::{Cut, FetchError, Fetched, ImagerySource, TileAddress};
use venti_sources_basemapat::BasemapAt;
use venti_sources_basemapat::fixtures::{self, Recorded};
use venti_sources_ign::Ign;
use venti_store::MemoryStore;

mod common;

use common::{Answer, TestClock, WEB_APP, ask, get, settings};

// --- Helpers ------------------------------------------------------------------

/// What the stand-in answers for an address.
#[derive(Clone)]
enum Upstream {
    Body(Vec<u8>),
    Status(u16),
    /// Nothing, for longer than any test waits.
    Silence,
}

/// The provider, as the tests have it: answers from a table, counts what
/// it is asked, and can be made to take its time.
#[derive(Default)]
struct StandIn {
    answers: Mutex<HashMap<String, Upstream>>,
    asked: AtomicUsize,
    /// Every address it was asked for, in order.
    addresses: Mutex<Vec<String>>,
    delay: Mutex<Option<Duration>>,
}

impl StandIn {
    fn answering(self: &Arc<Self>, url: String, answer: Upstream) -> Arc<Self> {
        self.answers.lock().unwrap().insert(url, answer);
        self.clone()
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

impl TileFetch for StandIn {
    fn get(&self, url: String, _max_bytes: usize) -> BoxFuture<'_, Result<Fetched, FetchError>> {
        Box::pin(async move {
            self.asked.fetch_add(1, Ordering::SeqCst);
            self.addresses.lock().unwrap().push(url.clone());
            let delay = *self.delay.lock().unwrap();
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let answer = self.answers.lock().unwrap().get(&url).cloned();
            match answer {
                Some(Upstream::Body(body)) => Ok(Fetched::from(body)),
                Some(Upstream::Status(status)) => Err(FetchError::Status(status)),
                Some(Upstream::Silence) => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    Err(FetchError::Transport("never".to_owned()))
                }
                None => Err(FetchError::Status(404)),
            }
        })
    }
}

/// A folder of kept tiles that goes when the test is over.
struct Folder(PathBuf);

impl Folder {
    fn new(test: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("venti-imagery-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn recorded(name: &str, z: u8) -> Recorded {
    fixtures::recorded()
        .into_iter()
        .find(|recorded| recorded.name == name && recorded.tile.z() == z)
        .unwrap()
}

fn path(tile: TileAddress) -> String {
    format!(
        "/v1/imagery/basemap-at/{}/{}/{}",
        tile.z(),
        tile.x(),
        tile.y()
    )
}

/// A stand-in that has every recorded tile.
fn provider() -> Arc<StandIn> {
    let source = BasemapAt::new();
    let stand_in = Arc::new(StandIn::default());
    for recorded in fixtures::recorded() {
        stand_in.answering(
            source.url(recorded.tile),
            Upstream::Body(recorded.jpeg.to_vec()),
        );
    }
    stand_in
}

fn imagery(fetch: Arc<StandIn>, cache: Option<Arc<TileCache>>, limits: Limits) -> Arc<Imagery> {
    Arc::new(Imagery::new(
        adapters::imagery_sources(),
        fetch,
        cache,
        limits,
    ))
}

fn server_with(imagery: Arc<Imagery>) -> Router {
    let store = Arc::new(MemoryStore::new());
    Api::new(store, Arc::new(TestClock::new()), [], settings())
        .with_imagery(imagery)
        .router(&Cors::Origins(vec![WEB_APP.to_owned()]))
}

/// A server with a stand-in for the provider and a folder for tiles.
fn server(fetch: Arc<StandIn>, folder: &Folder) -> (Router, Arc<TileCache>) {
    let cache = Arc::new(TileCache::new(folder.0.clone(), 1024 * 1024 * 1024).unwrap());
    let router = server_with(imagery(fetch, Some(cache.clone()), Limits::default()));
    (router, cache)
}

fn kept_a_week(answer: &Answer) {
    assert_eq!(
        answer.header(header::CACHE_CONTROL),
        "public, max-age=604800"
    );
}

// --- What a tile is answered with ----------------------------------------------

#[tokio::test]
async fn a_tile_inside_austria_is_the_providers_jpeg_as_it_came() {
    let folder = Folder::new("inside");
    let provider = provider();
    let (router, _) = server(provider.clone(), &folder);
    let innsbruck = recorded("inside_innsbruck", 15);

    let answer = ask(Method::GET, &path(innsbruck.tile))
        .header(header::ORIGIN, WEB_APP)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "image/jpeg");
    assert_eq!(answer.body, innsbruck.jpeg);
    kept_a_week(&answer);
    // The web app's map may draw it.
    assert_eq!(answer.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
    assert_eq!(provider.asked(), 1);
}

#[tokio::test]
async fn a_tile_beyond_the_border_is_nothing_and_the_provider_is_not_asked() {
    let folder = Folder::new("outside");
    let provider = provider();
    let (router, _) = server(provider.clone(), &folder);

    // The white tile over Munich, which the stand-in has; Bolzano at zoom
    // 15; Zurich, outside the service's box; and zooms beyond its range.
    let munich = recorded("outside_munich", 10).tile;
    for tile in [
        munich,
        TileAddress::new(15, 17417, 11591).unwrap(),
        TileAddress::new(12, 2145, 1434).unwrap(),
        TileAddress::new(20, 557_463, 367_655).unwrap(),
        TileAddress::new(24, 0, 0).unwrap(),
    ] {
        let answer = get(&router, &path(tile)).await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT, "{tile}");
        assert!(answer.body.is_empty());
        kept_a_week(&answer);
    }
    assert_eq!(provider.asked(), 0);
}

#[tokio::test]
async fn a_tile_on_the_border_is_cut_and_goes_out_as_a_png() {
    let folder = Folder::new("border");
    let provider = provider();
    let (router, cache) = server(provider.clone(), &folder);
    let brenner = recorded("border_brenner", 15);
    let Cut::Png(cut) = BasemapAt::new()
        .cut(brenner.tile, brenner.jpeg)
        .unwrap()
        .cut
    else {
        panic!("the Brenner's tile is cut");
    };

    let answer = get(&router, &path(brenner.tile)).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "image/png");
    assert_eq!(answer.body, cut);
    kept_a_week(&answer);
    // What is kept is the cut tile, not the provider's.
    let file = cache.file("basemap-at", brenner.tile, "png");
    assert_eq!(std::fs::read(&file).unwrap(), cut);
    assert!(!cache.file("basemap-at", brenner.tile, "jpeg").exists());

    // On the border within the margin, with nothing to remove: the
    // provider's JPEG.
    let finest = recorded("border_achenpass", 19);
    let answer = get(&router, &path(finest.tile)).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "image/jpeg");
    assert_eq!(answer.body, finest.jpeg);
}

#[tokio::test]
async fn a_tile_on_the_border_that_is_all_white_is_nothing() {
    // The provider's white tile as the answer for a tile on the Bavarian
    // side of the Achenpass that the margin reaches into: cut to nothing.
    let folder = Folder::new("white");
    let source = BasemapAt::new();
    let white = recorded("outside_munich", 10).jpeg.to_vec();
    let achenpass = recorded("border_achenpass", 15).tile;
    // The sixteen tiles at zoom 17 that make up the recorded one at 15.
    let tile = (0..4)
        .flat_map(|dy| (0..4).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| TileAddress::new(17, achenpass.x() * 4 + dx, achenpass.y() * 4 + dy))
        .find(|tile| {
            source.classify(*tile) == venti_sources::Coverage::Edge
                && source.cut(*tile, &white).unwrap().cut == Cut::Nothing
        })
        .expect("a tile at zoom 17 beyond the border near the Achenpass");
    let provider = Arc::new(StandIn::default()).answering(source.url(tile), Upstream::Body(white));
    let (router, cache) = server(provider.clone(), &folder);

    let answer = get(&router, &path(tile)).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    kept_a_week(&answer);
    assert!(cache.file("basemap-at", tile, "none").exists());
}

// --- Asking the provider once ---------------------------------------------------

#[tokio::test]
async fn a_tile_is_asked_of_the_provider_once_and_then_read_from_the_disk() {
    let folder = Folder::new("kept");
    let provider = provider();
    let (router, cache) = server(provider.clone(), &folder);
    let tile = recorded("inside_innsbruck", 15).tile;

    let first = get(&router, &path(tile)).await;
    let second = get(&router, &path(tile)).await;
    assert_eq!(provider.asked(), 1);
    assert_eq!(first.body, second.body);
    assert_eq!(second.header(header::CONTENT_TYPE), "image/jpeg");
    assert_eq!(
        cache.file("basemap-at", tile, "jpeg"),
        folder.0.join("basemap-at/15/17420/11489.jpeg")
    );

    // A server started again on the same folder has it too.
    let again = server(provider.clone(), &folder).0;
    let third = get(&again, &path(tile)).await;
    assert_eq!(third.body, first.body);
    assert_eq!(provider.asked(), 1);

    // "Nothing here" from the provider is kept as well.
    let nowhere = TileAddress::new(15, 17420, 11490).unwrap();
    assert_eq!(
        get(&router, &path(nowhere)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get(&router, &path(nowhere)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(provider.asked(), 2);
}

#[tokio::test]
async fn two_requests_for_the_same_tile_at_once_wait_for_one_fetch() {
    let folder = Folder::new("together");
    let provider = provider();
    *provider.delay.lock().unwrap() = Some(Duration::from_millis(200));
    let (router, _) = server(provider.clone(), &folder);
    let tile = recorded("border_achenpass", 15).tile;

    let address = path(tile);
    let (a, b, c) = tokio::join!(
        get(&router, &address),
        get(&router, &address),
        get(&router, &address),
    );
    assert_eq!(provider.asked(), 1);
    for answer in [&a, &b, &c] {
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(answer.header(header::CONTENT_TYPE), "image/png");
    }
    assert_eq!(a.body, b.body);
    assert_eq!(a.body, c.body);
}

#[tokio::test]
async fn without_a_folder_every_request_asks_but_requests_at_once_still_share() {
    // The development mode keeps nothing.
    let provider = provider();
    *provider.delay.lock().unwrap() = Some(Duration::from_millis(100));
    let router = server_with(imagery(provider.clone(), None, Limits::default()));
    let tile = recorded("inside_innsbruck", 15).tile;
    let address = path(tile);
    let (a, b) = tokio::join!(get(&router, &address), get(&router, &address));
    assert_eq!((a.status, b.status), (StatusCode::OK, StatusCode::OK));
    assert_eq!(provider.asked(), 1);
    get(&router, &path(tile)).await;
    assert_eq!(provider.asked(), 2);
}

// --- The folder's size ------------------------------------------------------------

#[tokio::test]
async fn the_folder_is_kept_under_its_size_by_removing_the_oldest_tiles() {
    let folder = Folder::new("sweep");
    let provider = provider();
    // Room for a little more than two of the cut tiles at the Achenpass
    // and the Brenner (30, 98 and 109 kB).
    let cache = Arc::new(TileCache::new(folder.0.clone(), 150 * 1024).unwrap());
    let router = server_with(imagery(
        provider.clone(),
        Some(cache.clone()),
        Limits::default(),
    ));
    let tiles = [
        recorded("border_achenpass", 12).tile,
        recorded("border_achenpass", 15).tile,
        recorded("border_brenner", 15).tile,
    ];
    for tile in tiles {
        assert_eq!(get(&router, &path(tile)).await.status, StatusCode::OK);
    }
    // The sweep the third write set off runs beside the requests; this
    // one is the test's own, after it.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let files = |extension: &str| -> Vec<bool> {
        tiles
            .iter()
            .map(|tile| cache.file("basemap-at", *tile, extension).exists())
            .collect()
    };
    // The two written first are gone, oldest first, until the folder
    // held a tenth under its 150 kB; the newest stays.
    assert_eq!(files("png"), [false, false, true]);
}

#[test]
fn a_sweep_removes_the_files_written_longest_ago_until_a_tenth_under() {
    let folder = Folder::new("sweep-order");
    let cache = TileCache::new(folder.0.clone(), 100 * 1024).unwrap();
    let place = folder.0.join("basemap-at/1/0");
    std::fs::create_dir_all(&place).unwrap();
    for (name, hours_ago) in [("0", 3), ("1", 1), ("2", 2)] {
        let file = place.join(format!("{name}.png"));
        std::fs::write(&file, vec![0; 40 * 1024]).unwrap();
        let file = std::fs::File::options().write(true).open(&file).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(3600 * hours_ago))
            .unwrap();
    }
    // 120 kB over a cap of 100: the oldest goes, and 80 kB is under 90.
    let swept = cache.sweep();
    assert_eq!(swept.found_bytes, 120 * 1024);
    assert_eq!(swept.removed_files, 1);
    assert_eq!(swept.left_bytes, 80 * 1024);
    assert!(!place.join("0.png").exists());
    assert!(place.join("1.png").exists() && place.join("2.png").exists());
    // Under the cap, nothing is removed.
    assert_eq!(cache.sweep().removed_files, 0);
}

// --- Addresses and failures -------------------------------------------------------

#[tokio::test]
async fn what_is_not_the_address_of_a_tile_is_not_found_and_asks_nothing() {
    let folder = Folder::new("addresses");
    let provider = provider();
    let (router, _) = server(provider.clone(), &folder);
    for path in [
        "/v1/imagery/basemap-at/15/17420",
        "/v1/imagery/basemap-at/15/17420/11489.jpeg",
        "/v1/imagery/basemap-at/15/17420/-1",
        "/v1/imagery/basemap-at/15/+17420/11489",
        "/v1/imagery/basemap-at/15/32768/0",
        "/v1/imagery/basemap-at/25/0/0",
        "/v1/imagery/basemap-at/a/b/c",
        "/v1/imagery/basemap-at/15/17420/11489/x",
        "/v1/imagery/basemap-at/15/999999999/11489",
        "/v1/imagery/elsewhere/15/17420/11489",
        "/v1/imagery/..%2F..%2Fetc/15/17420/11489",
        "/v1/imagery/basemap-at/15/..%2F17420/11489",
    ] {
        get(&router, path)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    }
    assert_eq!(provider.asked(), 0);
    assert_eq!(std::fs::read_dir(&folder.0).unwrap().count(), 0);

    // A server that passes no imagery on has no such tiles.
    let store = Arc::new(MemoryStore::new());
    let without = Api::new(store, Arc::new(TestClock::new()), [], settings())
        .router(&Cors::Origins(vec![WEB_APP.to_owned()]));
    get(&without, &path(recorded("inside_innsbruck", 15).tile))
        .await
        .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
}

#[tokio::test]
async fn a_provider_that_fails_is_answered_for_a_minute_and_nothing_is_kept() {
    let folder = Folder::new("failing");
    let source = BasemapAt::new();
    let tile = recorded("inside_innsbruck", 15).tile;
    let provider = Arc::new(StandIn::default()).answering(source.url(tile), Upstream::Status(500));
    let (router, cache) = server(provider.clone(), &folder);

    for _ in 0..2 {
        let answer = get(&router, &path(tile)).await;
        assert_eq!(answer.status, StatusCode::BAD_GATEWAY);
        assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
        assert!(
            answer
                .header(header::CONTENT_TYPE)
                .starts_with("application/json")
        );
    }
    // Asked again each time: a failure is not kept.
    assert_eq!(provider.asked(), 2);
    for extension in ["jpeg", "png", "none"] {
        assert!(!cache.file("basemap-at", tile, extension).exists());
    }

    // What is not a tile is a failure too, and so is a tile that is
    // larger than the adapter allows, whatever the fetcher let through.
    let border = recorded("border_brenner", 15).tile;
    provider.answering(
        source.url(border),
        Upstream::Body(b"<html>busy</html>".to_vec()),
    );
    assert_eq!(
        get(&router, &path(border)).await.status,
        StatusCode::BAD_GATEWAY
    );
    provider.answering(source.url(tile), Upstream::Body(vec![0; 1024 * 1024 + 1]));
    assert_eq!(
        get(&router, &path(tile)).await.status,
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(source.max_tile_bytes(), 1024 * 1024);
}

#[tokio::test]
async fn a_provider_that_does_not_answer_in_time_is_a_timeout() {
    let folder = Folder::new("silent");
    let source = BasemapAt::new();
    let tile = recorded("inside_innsbruck", 15).tile;
    let provider = Arc::new(StandIn::default()).answering(source.url(tile), Upstream::Silence);
    let cache = Arc::new(TileCache::new(folder.0.clone(), 1024 * 1024).unwrap());
    let limits = Limits {
        in_flight: 1,
        timeout: Duration::from_millis(200),
    };
    let router = server_with(imagery(provider.clone(), Some(cache.clone()), limits));

    let answer = get(&router, &path(tile)).await;
    assert_eq!(answer.status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
    assert!(!cache.file("basemap-at", tile, "jpeg").exists());
}

// --- The development mode -------------------------------------------------------

#[tokio::test]
async fn the_development_mode_answers_from_the_recorded_tiles_alone() {
    let (imagery, said) = dev::imagery(false).unwrap();
    assert!(said.contains("recorded tiles only"), "{said}");
    assert!(imagery.cache().is_none());
    let router = server_with(imagery);

    for recorded in fixtures::recorded() {
        let answer = get(&router, &path(recorded.tile)).await;
        let expected = match recorded.name {
            "outside_munich" => StatusCode::NO_CONTENT,
            _ => StatusCode::OK,
        };
        assert_eq!(answer.status, expected, "{}", recorded.name);
    }
    // A tile inside Austria that was not recorded: nothing, and no network.
    let vienna = TileAddress::new(15, 17874, 11362).unwrap();
    assert_eq!(
        get(&router, &path(vienna)).await.status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn the_development_mode_asks_the_provider_only_when_told() {
    let (imagery, said) = dev::imagery(true).unwrap();
    let cache = imagery.cache().expect("online imagery is kept");
    assert!(cache.root().starts_with(std::env::temp_dir()));
    assert!(said.contains("ONLINE"), "{said}");
    assert!(said.contains(&cache.root().display().to_string()), "{said}");
    assert_eq!(
        imagery.sources().collect::<Vec<_>>(),
        ["basemap-at", "ign-ortho"]
    );
    // Nothing is asked here: the tests do not use the network.
}

// --- France's photographs ---------------------------------------------------------

fn french(name: &str, z: u8) -> venti_sources_ign::fixtures::Recorded {
    venti_sources_ign::fixtures::recorded()
        .into_iter()
        .find(|recorded| recorded.name == name && recorded.tile.z() == z)
        .unwrap()
}

fn french_path(tile: TileAddress) -> String {
    format!(
        "/v1/imagery/ign-ortho/{}/{}/{}",
        tile.z(),
        tile.x(),
        tile.y()
    )
}

/// A stand-in that has every tile recorded from IGN.
fn ign() -> Arc<StandIn> {
    let source = Ign::new();
    let stand_in = Arc::new(StandIn::default());
    for recorded in venti_sources_ign::fixtures::recorded() {
        stand_in.answering(
            source.url(recorded.tile),
            Upstream::Body(recorded.jpeg.to_vec()),
        );
    }
    stand_in
}

#[tokio::test]
async fn a_french_tile_is_passed_on_inside_cut_on_the_border_and_nothing_beyond() {
    let folder = Folder::new("france");
    let provider = ign();
    let (router, cache) = server(provider.clone(), &folder);
    let source = Ign::new();

    // Inside: the provider's JPEG as it came.
    let annecy = french("inside_annecy", 15);
    let answer = ask(Method::GET, &french_path(annecy.tile))
        .header(header::ORIGIN, WEB_APP)
        .send(&router)
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header(header::CONTENT_TYPE), "image/jpeg");
    assert_eq!(answer.body, annecy.jpeg);
    kept_a_week(&answer);
    assert_eq!(answer.header(header::ACCESS_CONTROL_ALLOW_ORIGIN), WEB_APP);
    assert_eq!(
        cache.file("ign-ortho", annecy.tile, "jpeg"),
        folder.0.join("ign-ortho/15/16941/11670.jpeg")
    );
    assert!(cache.file("ign-ortho", annecy.tile, "jpeg").exists());

    // Across the border, at the coarsest zoom, a middle one and on Mont
    // Blanc: cut, and the cut tile is what is kept.
    for (name, z) in [
        ("border_veyrier", 13),
        ("border_veyrier", 15),
        ("border_montblanc", 15),
        ("border_menton", 15),
    ] {
        let border = french(name, z);
        let Cut::Png(cut) = source.cut(border.tile, border.jpeg).unwrap().cut else {
            panic!("{name} at zoom {z} is cut");
        };
        let answer = get(&router, &french_path(border.tile)).await;
        assert_eq!(answer.status, StatusCode::OK, "{name}");
        assert_eq!(answer.header(header::CONTENT_TYPE), "image/png");
        assert_eq!(answer.body, cut);
        kept_a_week(&answer);
        assert_eq!(
            std::fs::read(cache.file("ign-ortho", border.tile, "png")).unwrap(),
            cut
        );
        assert!(!cache.file("ign-ortho", border.tile, "jpeg").exists());
    }
    assert_eq!(provider.asked(), 5);

    // Beyond the border: the white tile over Courmayeur, which the
    // stand-in has; Turin; Geneva's centre; Monaco. Nothing, and nothing
    // is asked.
    let courmayeur = french("outside_courmayeur", 15).tile;
    for tile in [
        courmayeur,
        TileAddress::new(15, 17083, 11778).unwrap(),
        TileAddress::new(15, 16942, 11631).unwrap(),
        TileAddress::new(17, 68237, 47794).unwrap(),
    ] {
        let answer = get(&router, &french_path(tile)).await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT, "{tile}");
        assert!(answer.body.is_empty());
        kept_a_week(&answer);
    }
    assert_eq!(provider.asked(), 5);

    // Asked again, a tile comes from the disk.
    get(&router, &french_path(annecy.tile)).await;
    assert_eq!(provider.asked(), 5);
}

#[tokio::test]
async fn a_zoom_the_french_layer_is_not_passed_on_at_is_nothing_and_asks_nothing() {
    let folder = Folder::new("france-zooms");
    let provider = ign();
    let (router, _) = server(provider.clone(), &folder);
    // Annecy at the zooms where the provider's layer shows more than its
    // photographs of France (0 to 12), and beyond its finest (20 to 24).
    for (z, x, y) in [
        (0, 0, 0),
        (8, 132, 91),
        (11, 1058, 729),
        (12, 2117, 1458),
        (20, 542_140, 373_466),
        (24, 8_674_240, 5_975_456),
    ] {
        let tile = TileAddress::new(z, x, y).unwrap();
        let answer = get(&router, &french_path(tile)).await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT, "{tile}");
        kept_a_week(&answer);
    }
    // And a place the box of France does not hold, at a zoom it has.
    let vienna = TileAddress::new(15, 17874, 11362).unwrap();
    assert_eq!(
        get(&router, &french_path(vienna)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(provider.asked(), 0);
    // The first zoom that is passed on is 13.
    let annecy = TileAddress::new(13, 4235, 2917).unwrap();
    assert_eq!(
        get(&router, &french_path(annecy)).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(provider.asked(), 1);
}

#[tokio::test]
async fn a_tile_ign_refuses_is_nothing_if_it_has_none_and_a_failure_otherwise() {
    let folder = Folder::new("france-refused");
    let source = Ign::new();
    // Three tiles inside France, side by side at Annecy.
    let [none, busy, limited] =
        [0, 1, 2].map(|dx| TileAddress::new(15, 16941 + dx, 11671).unwrap());
    let provider = Arc::new(StandIn::default())
        .answering(source.url(none), Upstream::Status(404))
        .answering(source.url(busy), Upstream::Status(500))
        .answering(source.url(limited), Upstream::Status(429));
    let (router, cache) = server(provider.clone(), &folder);

    // "No such tile" is an answer, and it is kept.
    for _ in 0..2 {
        let answer = get(&router, &french_path(none)).await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
        kept_a_week(&answer);
    }
    assert!(cache.file("ign-ortho", none, "none").exists());
    assert_eq!(provider.asked(), 1);

    // A failure, and "too many requests", are failures for a minute, and
    // nothing of them is kept.
    for tile in [busy, limited] {
        let answer = get(&router, &french_path(tile)).await;
        assert_eq!(answer.status, StatusCode::BAD_GATEWAY, "{tile}");
        assert_eq!(answer.header(header::CACHE_CONTROL), "public, max-age=60");
        for extension in ["jpeg", "png", "none"] {
            assert!(!cache.file("ign-ortho", tile, extension).exists());
        }
    }

    // The service's XML in place of a tile on the border is a failure too.
    let border = french("border_veyrier", 15).tile;
    provider.answering(
        source.url(border),
        Upstream::Body(b"<ExceptionReport><Exception/></ExceptionReport>".to_vec()),
    );
    assert_eq!(
        get(&router, &french_path(border)).await.status,
        StatusCode::BAD_GATEWAY
    );
    assert!(!cache.file("ign-ortho", border, "png").exists());
}

#[tokio::test]
async fn the_server_asks_the_providers_for_their_own_layers_and_nothing_else() {
    let folder = Folder::new("relay");
    let basemap_at = BasemapAt::new();
    let source = Ign::new();
    let provider = ign();
    let innsbruck = recorded("inside_innsbruck", 15);
    provider.answering(
        basemap_at.url(innsbruck.tile),
        Upstream::Body(innsbruck.jpeg.to_vec()),
    );
    let (router, _) = server(provider.clone(), &folder);
    let annecy = french("inside_annecy", 15).tile;

    // Another layer, host or address named in place of a source, or
    // slipped into a number: not found, and nothing is asked.
    for path in [
        "/v1/imagery/ORTHOIMAGERY.ORTHOPHOTOS/15/16941/11670",
        "/v1/imagery/ORTHOIMAGERY.ORTHOPHOTOS2024/15/16941/11670",
        "/v1/imagery/GEOGRAPHICALGRIDSYSTEMS.MAPS/15/16941/11670",
        "/v1/imagery/ign/15/16941/11670",
        "/v1/imagery/IGN-ORTHO/15/16941/11670",
        "/v1/imagery/ign-ortho%2F..%2Fbasemap-at/15/16941/11670",
        "/v1/imagery/data.geopf.fr/15/16941/11670",
        "/v1/imagery/https:%2F%2Fexample.org%2Ftiles/15/16941/11670",
        "/v1/imagery/example.org/15/16941/11670",
        "/v1/imagery/ign-ortho/15/16941/11670%26LAYER=GEOGRAPHICALGRIDSYSTEMS.MAPS",
        "/v1/imagery/ign-ortho/15/16941%26TILEROW=1/11670",
        "/v1/imagery/ign-ortho/15%26LAYER=X/16941/11670",
        "/v1/imagery/ign-ortho/15/16941@example.org/11670",
        "/v1/imagery/ign-ortho/15/16941/11670%23",
        "/v1/imagery/ign-ortho/15/16941/11670%3F",
        "/v1/imagery/ign-ortho/15/16941/%2F%2Fexample.org",
        "/v1/imagery/ign-ortho/15/16941/11670.jpeg",
        "/v1/imagery/ign-ortho/15/16941",
        "/v1/imagery/ign-ortho/15/16941/11670/x",
    ] {
        get(&router, path)
            .await
            .error(StatusCode::NOT_FOUND, ErrorCode::NotFound);
    }
    assert_eq!(provider.asked(), 0);
    assert_eq!(std::fs::read_dir(&folder.0).unwrap().count(), 0);

    // What a request adds after the tile changes nothing of what the
    // provider is asked.
    for path in [
        format!(
            "{}?LAYER=GEOGRAPHICALGRIDSYSTEMS.MAPS&url=https://example.org/&FORMAT=image/png",
            french_path(annecy)
        ),
        format!("{}?source=ign-ortho&z=1", path(innsbruck.tile)),
    ] {
        let answer = get(&router, &path).await;
        assert_eq!(answer.status, StatusCode::OK, "{path}");
        assert_eq!(answer.header(header::CONTENT_TYPE), "image/jpeg");
    }
    // Each source was asked for its own layer at its own host, and for
    // the tile that was named.
    assert_eq!(
        *provider.addresses.lock().unwrap(),
        [source.url(annecy), basemap_at.url(innsbruck.tile)]
    );
    assert_eq!(
        source.url(annecy),
        "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&TILEMATRIXSET=PM_0_19&TILEMATRIX=15&TILECOL=16941&TILEROW=11670&FORMAT=image/jpeg"
    );
}

#[tokio::test]
async fn the_development_mode_answers_frances_recorded_tiles_too() {
    let (imagery, _) = dev::imagery(false).unwrap();
    let router = server_with(imagery);
    for recorded in venti_sources_ign::fixtures::recorded() {
        let answer = get(&router, &french_path(recorded.tile)).await;
        let expected = match recorded.name {
            "outside_courmayeur" => StatusCode::NO_CONTENT,
            _ => StatusCode::OK,
        };
        assert_eq!(answer.status, expected, "{}", recorded.name);
    }
    // A tile inside France that was not recorded: nothing, and no network.
    let paris = TileAddress::new(15, 16597, 11273).unwrap();
    assert_eq!(
        get(&router, &french_path(paris)).await.status,
        StatusCode::NO_CONTENT
    );
}
