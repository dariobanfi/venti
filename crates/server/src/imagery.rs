//! Map imagery that the server passes on: a tile when a client's map asks
//! for it, kept on the disk so that the provider is asked once.
//!
//! Some providers' tiles cannot go to a map as they are. basemap.at's are
//! white beyond Austria's border, and a map style cannot cut a raster along
//! a line. So a client asks this server, and the server asks the provider
//! and answers with what the adapter says may be shown
//! ([`venti_sources::ImagerySource`]):
//!
//! - a tile the provider does not have, or that lies wholly outside what
//!   may be shown, is answered with "nothing here" at once, and the
//!   provider is not asked;
//! - a tile wholly inside is the provider's bytes as they came;
//! - a tile on the edge is cut, and goes out as a PNG with transparency,
//!   or as the provider's bytes if the cut removed nothing, or as "nothing
//!   here" if it removed everything.
//!
//! **What a request names is never made into a path.** A tile's place on
//! the disk is made of the source's identifier, as its manifest has it,
//! and three numbers that were read and checked ([`TileAddress`]).
//!
//! **The disk.** Each answer is kept under the folder the configuration
//! names, at `<source>/<z>/<x>/<y>.jpeg`, `.png` or `.none` (an empty file
//! for "nothing here"): the cut tile for a tile on the edge, not the
//! provider's. A tile is written to a file of its own and then renamed,
//! so a half-written tile is never read. The folder is kept under a size
//! ([`TileCache`]): when a write takes it over, the oldest files are
//! removed until it is a tenth under. A tile does not expire otherwise:
//! orthophotos change once in years, and the size keeps the folder to
//! what pilots look at.
//!
//! **The provider is spared.** Only a tile the provider has can cause a
//! request. Two requests for a tile that is not on the disk wait for one
//! fetch. At most [`Limits::in_flight`] requests to providers are under
//! way at once; a tile that cannot get its turn and its answer within
//! [`Limits::timeout`] is answered with "the provider did not answer in
//! time" (504). A provider that fails is answered with 502 for a minute,
//! and nothing of it is kept.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use axum::body::Bytes;
use futures::future::BoxFuture;
use tokio::sync::{OnceCell, Semaphore};
use venti_sources::{
    Coverage, Cut, Fetch, FetchError, Fetched, ImagerySource, Request, TileAddress,
};

/// What fetches a tile from its provider. The server's fetcher, with its
/// bounds, is one; tests have a stand-in.
pub trait TileFetch: Send + Sync {
    /// Fetches `url`, refusing an answer larger than `max_bytes`.
    fn get(&self, url: String, max_bytes: usize) -> BoxFuture<'_, Result<Fetched, FetchError>>;
}

impl<F: Fetch + Send + Sync> TileFetch for F {
    fn get(&self, url: String, max_bytes: usize) -> BoxFuture<'_, Result<Fetched, FetchError>> {
        Box::pin(async move {
            let request = Request::new("tile", url);
            self.get_within(&request, max_bytes).await
        })
    }
}

/// What a tile is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// An image: its media type and its bytes.
    Tile {
        /// `image/jpeg` or `image/png`.
        media_type: &'static str,
        /// The image.
        bytes: Bytes,
    },
    /// Nothing here: no tile, or nothing of it that may be shown.
    Nothing,
    /// The provider failed, or sent what is not a tile. Not kept.
    Failed(String),
    /// The provider did not answer in time. Not kept.
    TimedOut,
}

/// The bounds on asking providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Requests to providers under way at once, all sources together.
    pub in_flight: usize,
    /// How long a tile may wait for its turn and its answer.
    pub timeout: Duration,
}

impl Default for Limits {
    /// Four at once, eight seconds: within the API's own deadline of ten.
    fn default() -> Self {
        Self {
            in_flight: 4,
            timeout: Duration::from_secs(8),
        }
    }
}

/// The size the folder of tiles is kept under unless the configuration
/// says otherwise: 20 GB. A tile is 10 to 100 kB, so that is some
/// hundreds of thousands of tiles, which is far more than the places
/// pilots fly at, and a percent of the server's disk.
pub const DEFAULT_CACHE_MB: u64 = 20 * 1024;

/// The map imagery the server passes on.
pub struct Imagery {
    sources: BTreeMap<String, Arc<dyn ImagerySource>>,
    fetch: Arc<dyn TileFetch>,
    cache: Option<Arc<TileCache>>,
    upstream: Arc<Semaphore>,
    timeout: Duration,
    /// The tiles being fetched now, each with the one answer every request
    /// for it waits for.
    in_flight: Mutex<HashMap<(String, TileAddress), Flight>>,
}

/// One tile being fetched: the answer every request for it waits for.
type Flight = Arc<OnceCell<Answer>>;

impl std::fmt::Debug for Imagery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Imagery")
            .field("sources", &self.sources.keys().collect::<Vec<_>>())
            .field("cache", &self.cache.as_ref().map(|cache| &cache.root))
            .finish_non_exhaustive()
    }
}

impl Imagery {
    /// Imagery of `sources`, fetched with `fetch`, kept in `cache` if there
    /// is one.
    pub fn new(
        sources: Vec<Arc<dyn ImagerySource>>,
        fetch: Arc<dyn TileFetch>,
        cache: Option<Arc<TileCache>>,
        limits: Limits,
    ) -> Self {
        Self {
            sources: sources
                .into_iter()
                .map(|source| (source.manifest().id.as_str().to_owned(), source))
                .collect(),
            fetch,
            cache,
            upstream: Arc::new(Semaphore::new(limits.in_flight.max(1))),
            timeout: limits.timeout,
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// The identifiers of the sources, sorted.
    pub fn sources(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    /// The folder tiles are kept in, if they are kept.
    pub fn cache(&self) -> Option<&Arc<TileCache>> {
        self.cache.as_ref()
    }

    /// The answer for a tile of a source. `None` when there is no source
    /// of that identifier.
    pub async fn answer(self: &Arc<Self>, source: &str, tile: TileAddress) -> Option<Answer> {
        let (id, imagery) = self.sources.get_key_value(source)?;
        if !imagery.covers(tile) || imagery.coverage(tile) == Coverage::Outside {
            return Some(Answer::Nothing);
        }
        if let Some(kept) = self.kept(id, tile).await {
            return Some(kept);
        }

        let key = (id.clone(), tile);
        let cell = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key.clone())
            .or_default()
            .clone();
        let answer = cell
            .get_or_init(|| {
                // The work runs on its own, so that a tile is fetched and
                // kept even when the client that asked has gone.
                let this = self.clone();
                let id = id.clone();
                let work = tokio::spawn(async move { this.fetch_and_keep(&id, tile).await });
                async move {
                    work.await
                        .unwrap_or_else(|_| Answer::Failed("the work on the tile failed".into()))
                }
            })
            .await
            .clone();
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if in_flight
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, &cell))
        {
            in_flight.remove(&key);
        }
        Some(answer)
    }

    /// The answer kept on the disk, if there is one.
    async fn kept(&self, source: &str, tile: TileAddress) -> Option<Answer> {
        let cache = self.cache.as_ref()?;
        cache.read(source, tile).await
    }

    async fn fetch_and_keep(&self, source: &str, tile: TileAddress) -> Answer {
        // Another request may have kept it between the look and the turn.
        if let Some(kept) = self.kept(source, tile).await {
            return kept;
        }
        let Some(imagery) = self.sources.get(source) else {
            return Answer::Nothing;
        };
        let answer = self.fetch_and_cut(imagery.clone(), tile).await;
        if let Some(cache) = &self.cache {
            cache.write(source, tile, &answer).await;
        }
        answer
    }

    async fn fetch_and_cut(&self, imagery: Arc<dyn ImagerySource>, tile: TileAddress) -> Answer {
        let max_bytes = imagery.max_tile_bytes();
        let url = imagery.url(tile);
        let fetched = tokio::time::timeout(self.timeout, async {
            let _turn = self.upstream.acquire().await;
            self.fetch.get(url, max_bytes).await
        })
        .await;
        let body = match fetched {
            Err(_) => return Answer::TimedOut,
            // The provider has no such tile: a place it has no photograph
            // of, beyond its border. That is an answer, and it is kept.
            Ok(Err(FetchError::Status(404))) => return Answer::Nothing,
            Ok(Err(error)) => return Answer::Failed(error.to_string()),
            Ok(Ok(fetched)) if fetched.body.len() > max_bytes => {
                return Answer::Failed(FetchError::TooLarge { limit: max_bytes }.to_string());
            }
            Ok(Ok(fetched)) => fetched.body,
        };
        match imagery.coverage(tile) {
            Coverage::Outside => Answer::Nothing,
            Coverage::Inside => Answer::Tile {
                media_type: imagery.media_type(),
                bytes: Bytes::from(body),
            },
            Coverage::Edge => {
                // Decoding and cutting a tile is some milliseconds of work
                // that waits for nothing: it is done off the threads that
                // answer requests.
                let media_type = imagery.media_type();
                let cutting = tokio::task::spawn_blocking(move || {
                    let cut = imagery.cut(tile, &body);
                    (cut, body)
                })
                .await;
                let Ok((imagery_cut, body)) = cutting else {
                    return Answer::Failed("cutting the tile failed".to_owned());
                };
                match imagery_cut {
                    Ok(cut) => match cut.cut {
                        Cut::Unchanged => Answer::Tile {
                            media_type,
                            bytes: Bytes::from(body),
                        },
                        Cut::Png(png) => Answer::Tile {
                            media_type: "image/png",
                            bytes: Bytes::from(png),
                        },
                        Cut::Nothing => Answer::Nothing,
                    },
                    Err(error) => Answer::Failed(error.to_string()),
                }
            }
        }
    }
}

/// The folder tiles are kept in, under a size.
#[derive(Debug)]
pub struct TileCache {
    root: PathBuf,
    cap_bytes: u64,
    /// What the folder holds, by the last sweep and the writes since.
    used: AtomicU64,
    sweeping: AtomicBool,
}

/// What a sweep found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Swept {
    /// The bytes the folder's files held before.
    pub found_bytes: u64,
    /// The files removed.
    pub removed_files: u64,
    /// The bytes the folder's files hold after.
    pub left_bytes: u64,
}

/// What a kept tile's file is called after, by what it holds.
const KINDS: [(&str, Option<&str>); 3] = [
    ("jpeg", Some("image/jpeg")),
    ("png", Some("image/png")),
    ("none", None),
];

impl TileCache {
    /// Tiles kept in `root`, which is made if it is not there, under
    /// `cap_bytes`.
    pub fn new(root: PathBuf, cap_bytes: u64) -> Result<Self, String> {
        std::fs::create_dir_all(&root)
            .map_err(|e| format!("cannot make the folder of tiles {}: {e}", root.display()))?;
        Ok(Self {
            root,
            cap_bytes,
            used: AtomicU64::new(0),
            sweeping: AtomicBool::new(false),
        })
    }

    /// The folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The most bytes the folder is to hold.
    pub fn cap_bytes(&self) -> u64 {
        self.cap_bytes
    }

    /// The folder of a tile, made of the source's identifier and three
    /// numbers and nothing else.
    fn folder(&self, source: &str, tile: TileAddress) -> PathBuf {
        self.root
            .join(source)
            .join(tile.z().to_string())
            .join(tile.x().to_string())
    }

    /// The file a tile is kept in, for each kind of answer.
    pub fn file(&self, source: &str, tile: TileAddress, extension: &str) -> PathBuf {
        self.folder(source, tile)
            .join(format!("{}.{extension}", tile.y()))
    }

    async fn read(&self, source: &str, tile: TileAddress) -> Option<Answer> {
        for (extension, media_type) in KINDS {
            match tokio::fs::read(self.file(source, tile, extension)).await {
                Ok(bytes) => {
                    return Some(match media_type {
                        Some(media_type) => Answer::Tile {
                            media_type,
                            bytes: Bytes::from(bytes),
                        },
                        None => Answer::Nothing,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!("imagery: a kept tile cannot be read: {error}");
                    return None;
                }
            }
        }
        None
    }

    /// Keeps an answer, if it is one to keep. A failure to write is
    /// written to the log and changes nothing for the client.
    async fn write(self: &Arc<Self>, source: &str, tile: TileAddress, answer: &Answer) {
        let (extension, bytes): (&str, &[u8]) = match answer {
            Answer::Tile { media_type, bytes } => (
                if *media_type == "image/png" {
                    "png"
                } else {
                    "jpeg"
                },
                bytes,
            ),
            Answer::Nothing => ("none", &[]),
            Answer::Failed(_) | Answer::TimedOut => return,
        };
        let folder = self.folder(source, tile);
        let file = self.file(source, tile, extension);
        let part = folder.join(format!("{}.{extension}.part", tile.y()));
        let written = async {
            tokio::fs::create_dir_all(&folder).await?;
            tokio::fs::write(&part, bytes).await?;
            tokio::fs::rename(&part, &file).await
        }
        .await;
        if let Err(error) = written {
            tracing::warn!("imagery: a tile could not be kept: {error}");
            let _ = tokio::fs::remove_file(&part).await;
            return;
        }
        let used = self.used.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
        if used > self.cap_bytes {
            self.sweep_in_background();
        }
    }

    /// Sweeps the folder beside everything else, unless a sweep is under
    /// way. The server does this once at its start, which also learns how
    /// much the folder holds.
    pub fn sweep_in_background(self: &Arc<Self>) {
        if self.sweeping.swap(true, Ordering::AcqRel) {
            return;
        }
        let cache = self.clone();
        tokio::task::spawn_blocking(move || {
            let swept = cache.sweep();
            if swept.removed_files > 0 {
                tracing::info!(
                    "imagery: {} held {} MB, over its {} MB; removed the {} oldest files",
                    cache.root.display(),
                    swept.found_bytes / (1024 * 1024),
                    cache.cap_bytes / (1024 * 1024),
                    swept.removed_files
                );
            }
            cache.sweeping.store(false, Ordering::Release);
        });
    }

    /// Looks at every file in the folder and, if together they hold more
    /// than the cap, removes the oldest by the time they were written
    /// until they hold a tenth less than the cap. Blocks while it reads
    /// the folder.
    pub fn sweep(&self) -> Swept {
        let mut files = Vec::new();
        let mut folders = vec![self.root.clone()];
        while let Some(folder) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if metadata.is_dir() {
                    folders.push(entry.path());
                } else if metadata.is_file() {
                    let written = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    files.push((written, metadata.len(), entry.path()));
                }
            }
        }
        let found_bytes: u64 = files.iter().map(|(_, size, _)| size).sum();
        let mut left_bytes = found_bytes;
        let mut removed_files = 0;
        if found_bytes > self.cap_bytes {
            let goal = self.cap_bytes - self.cap_bytes / 10;
            files.sort();
            for (_, size, path) in files {
                if left_bytes <= goal {
                    break;
                }
                if std::fs::remove_file(&path).is_ok() {
                    left_bytes -= size;
                    removed_files += 1;
                }
            }
        }
        self.used.store(left_bytes, Ordering::Relaxed);
        Swept {
            found_bytes,
            removed_files,
            left_bytes,
        }
    }
}
