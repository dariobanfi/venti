//! The map regions this server hands out, for apps to draw without a
//! network.
//!
//! Whoever runs the server keeps them in one folder. Every folder inside
//! it that holds a [`DESCRIPTION`] file is one region. That file says what
//! the region is called, which edition of its files this is, what they
//! cover, and which files belong to it, each with its SHA-256 and with the
//! credits its data asks for:
//!
//! ```toml
//! id = "alps-de-at"
//! name = "Alps of Germany and Austria"
//! version = "2026-11-20"
//! covers = { south = 46.3, west = 9.4, north = 48.5, east = 17.2 }
//!
//! [[files]]
//! role = "basemap"
//! name = "basemap.pmtiles"
//! sha256 = "0f343b0931126a20f133d67c2b018a3b5f1a2b5e0f8f1d3f6f0f4d8a3b1c2d3e"
//! credits = [
//!   { text = "© OpenStreetMap contributors", url = "https://www.openstreetmap.org/copyright" },
//! ]
//! ```
//!
//! The files lie beside it. `tools/region-map/build.sh` builds such a
//! folder.
//!
//! **A region is listed only after its files were read.** A client
//! downloads a file whole, often gigabytes of it over a mobile network,
//! and checks its copy against the SHA-256 the server listed. A wrong hash
//! in the list would make every download fail at its very end. So the
//! server reads every file once and lists a region only when each hash is
//! the one its description states ([`Maps::read`]). That takes as long as
//! reading the files takes, and the server does it beside everything else
//! ([`Maps::read_in_background`]): until it is done the region is not
//! listed, and nothing waits for it.
//!
//! **A region that breaks a rule is left out**, and the log says which
//! rule. The server runs on without it.
//!
//! **What a request names is never made into a path.** A request is looked
//! up among the regions that are listed ([`Maps::file`]), and the file
//! that is then opened is the one that was read and checked.
//!
//! The folder is read when the server starts and not again. A file that is
//! found changed afterwards takes its region off the list
//! ([`Maps::withdraw`]) until the server is started again.
//!
//! **Neither reading nor a download keeps the program from stopping.** A
//! region of some gigabytes takes a while to read and far longer to
//! download. Both end when the program is told to stop ([`Maps::new`]): a
//! region that was not read to its end is not listed, and a client whose
//! download broke off takes it up again by its range of bytes.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::SystemTime;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use venti_api_types::v1::{
    MapCredit, MapFile, MapFileRole, MapRegion, MapsResponse, is_map_name, map_file_path,
};
use venti_domain::BoundingBox;

/// The name of the file that describes a region, in the region's folder.
pub const DESCRIPTION: &str = "map.toml";

/// How much of a file is read at a time while its hash is worked out.
const HASHING_CHUNK: usize = 1024 * 1024;

/// The map regions a server hands out.
///
/// It starts out with none. Copies share one list, so the one that reads
/// the folder and the one that answers requests see the same regions.
#[derive(Debug, Clone, Default)]
pub struct Maps {
    listed: Arc<RwLock<BTreeMap<String, Arc<Region>>>>,
    /// Cancelled when the program is told to stop.
    stop: CancellationToken,
}

/// A region that is listed.
#[derive(Debug)]
struct Region {
    /// The region as the API lists it.
    listed: MapRegion,
    /// Its files on disk, in the order of `listed.files`.
    files: Vec<Arc<CheckedFile>>,
}

/// One file of a listed region, as it was when the server read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedFile {
    /// Where the file is, with every link on the way followed.
    pub path: PathBuf,
    /// Its size in bytes.
    pub bytes: u64,
    /// Its SHA-256, 64 lowercase hexadecimal digits.
    pub sha256: String,
    /// When it was last written, where the file system says.
    pub modified: Option<SystemTime>,
}

impl CheckedFile {
    /// Whether a file with this size and this time of writing can still be
    /// the one that was read. A file that was replaced or written to since
    /// differs in one of the two.
    pub fn is_still(&self, bytes: u64, modified: Option<SystemTime>) -> bool {
        self.bytes == bytes && self.modified == modified
    }
}

/// What reading a folder of regions came to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    /// The identifiers of the regions that are now listed.
    pub listed: Vec<String>,
    /// How many regions were left out because they break a rule.
    pub left_out: usize,
    /// Whether the reading was told to stop before it was done.
    pub stopped: bool,
}

impl Maps {
    /// No regions yet, in a program that stops when `stop` is cancelled:
    /// reading the regions ends then, and so does every download.
    ///
    /// [`Maps::default`] is the same for a program that is never told to
    /// stop, such as a test.
    pub fn new(stop: CancellationToken) -> Self {
        Self {
            listed: Arc::default(),
            stop,
        }
    }

    /// Completes when the program is told to stop. A download ends then.
    pub fn stopping(&self) -> impl Future<Output = ()> + Send + 'static {
        self.stop.clone().cancelled_owned()
    }

    /// The regions that are listed, sorted by identifier.
    pub fn list(&self) -> MapsResponse {
        let listed = self.listed.read().unwrap_or_else(PoisonError::into_inner);
        MapsResponse {
            regions: listed
                .values()
                .map(|region| region.listed.clone())
                .collect(),
        }
    }

    /// The file a request names, if a listed region has it in this
    /// version.
    pub fn file(&self, region: &str, version: &str, name: &str) -> Option<Arc<CheckedFile>> {
        let listed = self.listed.read().unwrap_or_else(PoisonError::into_inner);
        let region = listed.get(region)?;
        if region.listed.version != version {
            return None;
        }
        let index = region
            .listed
            .files
            .iter()
            .position(|file| file.name == name)?;
        region.files.get(index).cloned()
    }

    /// Takes a region off the list. Whether it was on it.
    ///
    /// For a region one of whose files is no longer the file that was
    /// read: its listed hash can no longer be vouched for.
    pub fn withdraw(&self, region: &str) -> bool {
        let mut listed = self.listed.write().unwrap_or_else(PoisonError::into_inner);
        listed.remove(region).is_some()
    }

    /// Reads the regions in `folder`, on a thread that may block, beside
    /// whatever else the program does. Each region is listed as soon as
    /// its own files are read.
    pub fn read_in_background(&self, folder: PathBuf) -> JoinHandle<()> {
        let maps = self.clone();
        tokio::task::spawn_blocking(move || {
            maps.read(&folder);
        })
    }

    /// Reads the regions in `folder` and lists those that keep the rules
    /// and whose files have the hashes their descriptions state.
    ///
    /// This reads every file of every region from beginning to end, which
    /// for a region of some gigabytes takes a while: call it where
    /// blocking is allowed. Whatever is left out is written to the log
    /// with the reason. The reading ends early when the program is told
    /// to stop; what was listed by then stays listed.
    pub fn read(&self, folder: &Path) -> Reading {
        let stop = &self.stop;
        let (described, mut left_out) = describe_all(folder);
        let mut reading = Reading::default();
        for region in described {
            if stop.is_cancelled() {
                reading.stopped = true;
                break;
            }
            let id = region.description.id.clone();
            match check(&region, stop) {
                Ok(Some(region)) => {
                    let megabytes = region.files.iter().map(|file| file.bytes).sum::<u64>() as f64
                        / (1024.0 * 1024.0);
                    tracing::info!(
                        "maps: handing out {id}, version {}: {} files, {megabytes:.1} MB",
                        region.listed.version,
                        region.files.len(),
                    );
                    let mut listed = self.listed.write().unwrap_or_else(PoisonError::into_inner);
                    listed.insert(id.clone(), Arc::new(region));
                    reading.listed.push(id);
                }
                Ok(None) => {
                    reading.stopped = true;
                    break;
                }
                Err(reason) => {
                    left_out += 1;
                    tracing::error!(
                        "maps: the region in {} is not handed out: {reason}",
                        region.folder.display()
                    );
                }
            }
        }
        reading.left_out = left_out;
        if reading.stopped {
            tracing::info!("maps: stopped before every region was read");
        } else if reading.listed.is_empty() && left_out == 0 {
            tracing::info!(
                "maps: no region in {}: a region is a folder in it that holds a {DESCRIPTION}",
                folder.display()
            );
        }
        reading
    }
}

/// A region's description, as its file holds it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Description {
    id: String,
    name: String,
    version: String,
    covers: BoundingBox,
    #[serde(default)]
    files: Vec<DescribedFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribedFile {
    role: String,
    name: String,
    sha256: String,
    #[serde(default)]
    credits: Vec<DescribedCredit>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribedCredit {
    text: String,
    url: Option<String>,
}

/// A region whose description keeps the rules. Its files were found and
/// not yet read.
#[derive(Debug)]
struct Described {
    /// The region's folder.
    folder: PathBuf,
    description: Description,
    /// Where each file of the description is, in its order.
    paths: Vec<PathBuf>,
}

/// The regions in `folder` whose descriptions keep the rules, sorted by
/// identifier, and how many were left out. Each one left out is written
/// to the log with the reason.
fn describe_all(folder: &Path) -> (Vec<Described>, usize) {
    let folders = match region_folders(folder) {
        Ok(folders) => folders,
        Err(error) => {
            tracing::error!(
                "maps: the folder {} cannot be read, so no region is handed out: {error}",
                folder.display()
            );
            return (Vec::new(), 0);
        }
    };

    let mut left_out = 0;
    let mut by_id: BTreeMap<String, Vec<Described>> = BTreeMap::new();
    for folder in folders {
        match describe(&folder) {
            Ok(region) => by_id
                .entry(region.description.id.clone())
                .or_default()
                .push(region),
            Err(reason) => {
                left_out += 1;
                tracing::error!(
                    "maps: the region in {} is not handed out: {reason}",
                    folder.display()
                );
            }
        }
    }

    let mut described = Vec::new();
    for (id, mut regions) in by_id {
        if regions.len() == 1 {
            described.extend(regions.pop());
            continue;
        }
        // Which of them is meant cannot be known, so none is.
        left_out += regions.len();
        let folders: Vec<String> = regions
            .iter()
            .map(|region| region.folder.display().to_string())
            .collect();
        tracing::error!(
            "maps: {} folders call their region {id}, and none of them is handed out: {}",
            regions.len(),
            folders.join(", ")
        );
    }
    (described, left_out)
}

/// The folders in `folder` that hold a description, sorted by name.
fn region_folders(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut folders = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.join(DESCRIPTION).is_file() {
            folders.push(path);
        }
    }
    folders.sort();
    Ok(folders)
}

/// Reads the description in a region's folder and holds it to the rules.
/// The error says which rule it breaks.
fn describe(folder: &Path) -> Result<Described, String> {
    let file = folder.join(DESCRIPTION);
    let text =
        fs::read_to_string(&file).map_err(|e| format!("{DESCRIPTION} cannot be read: {e}"))?;
    let description: Description =
        toml::from_str(&text).map_err(|e| format!("{DESCRIPTION} cannot be used: {e}"))?;

    let a_name = "must be lowercase letters and digits, with - _ . between them, \
                  64 characters at most";
    if !is_map_name(&description.id) {
        return Err(format!("the id {:?} {a_name}", description.id));
    }
    if !is_map_name(&description.version) {
        return Err(format!("the version {:?} {a_name}", description.version));
    }
    if !is_a_line(&description.name) {
        return Err("the name is empty, or is more than one line of text".to_owned());
    }
    if description.files.is_empty() {
        return Err("it lists no files".to_owned());
    }

    let home = fs::canonicalize(folder).map_err(|e| format!("its folder cannot be read: {e}"))?;
    let mut paths = Vec::new();
    for (index, file) in description.files.iter().enumerate() {
        let name = &file.name;
        if !is_map_name(name) {
            return Err(format!("the file name {name:?} {a_name}"));
        }
        if description.files[..index]
            .iter()
            .any(|other| other.name == *name)
        {
            return Err(format!("it lists the file {name} twice"));
        }
        if role(&file.role).is_none() {
            return Err(format!(
                "{name}: the role {:?} is not one of basemap, terrain, other",
                file.role
            ));
        }
        if !is_sha256(&file.sha256) {
            return Err(format!(
                "{name}: sha256 is not 64 lowercase hexadecimal digits"
            ));
        }
        if file.credits.is_empty() {
            return Err(format!(
                "{name}: it names no credit, and a file holds somebody's data"
            ));
        }
        for credit in &file.credits {
            if !is_a_line(&credit.text) {
                return Err(format!(
                    "{name}: a credit's text is empty, or is more than one line"
                ));
            }
            if credit.url.as_deref().is_some_and(|url| !is_a_link(url)) {
                return Err(format!(
                    "{name}: a credit's url must start with https:// or http://"
                ));
            }
        }
        paths.push(file_in(&home, name)?);
    }

    Ok(Described {
        folder: folder.to_owned(),
        description,
        paths,
    })
}

/// Where the file called `name` in the folder `home` is, if it is a file
/// that lies in that folder itself. `home` has every link in it followed.
///
/// A name is one step of a path, so it cannot point below the folder or
/// out of it. A link can: one that leads out of the folder is refused,
/// whatever it leads to.
fn file_in(home: &Path, name: &str) -> Result<PathBuf, String> {
    let path = fs::canonicalize(home.join(name))
        .map_err(|e| format!("its file {name} cannot be read: {e}"))?;
    if path.parent() != Some(home) {
        return Err(format!(
            "its file {name} is a link that leads out of the region's folder"
        ));
    }
    let is_file = fs::metadata(&path).is_ok_and(|metadata| metadata.is_file());
    if !is_file {
        return Err(format!("{name} is not a file"));
    }
    Ok(path)
}

/// What a file is, by the word a description uses for it.
///
/// A word this server does not know is refused, not passed on as `other`:
/// a mistyped `basemap` would otherwise be a file that no app draws.
fn role(word: &str) -> Option<MapFileRole> {
    match word {
        "basemap" => Some(MapFileRole::Basemap),
        "terrain" => Some(MapFileRole::Terrain),
        "other" => Some(MapFileRole::Other),
        _ => None,
    }
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether text is one line that says something.
fn is_a_line(text: &str) -> bool {
    !text.trim().is_empty() && !text.chars().any(char::is_control)
}

/// Whether text is a web address an app may open. Anything else in a
/// link's place, such as `javascript:`, has no business there.
fn is_a_link(text: &str) -> bool {
    ["https://", "http://"].iter().any(|scheme| {
        text.strip_prefix(scheme)
            .is_some_and(|rest| !rest.is_empty())
    }) && !text.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Reads every file of a region and makes the region as it is listed, if
/// each file has the hash its description states. `Ok(None)` when `stop`
/// ended the reading. The error says what is wrong.
fn check(region: &Described, stop: &CancellationToken) -> Result<Option<Region>, String> {
    let description = &region.description;
    let mut listed = Vec::new();
    let mut files = Vec::new();
    for (file, path) in description.files.iter().zip(&region.paths) {
        let name = &file.name;
        let Some(read) =
            read_file(path, stop).map_err(|e| format!("{name} cannot be read: {e}"))?
        else {
            return Ok(None);
        };
        if read.sha256 != file.sha256 {
            return Err(format!(
                "{name} has the SHA-256 {}, and {DESCRIPTION} states {}. Whichever is \
                 wrong, a download checked against the stated one would fail",
                read.sha256, file.sha256
            ));
        }
        listed.push(MapFile {
            role: role(&file.role).unwrap_or(MapFileRole::Other),
            name: name.clone(),
            bytes: read.bytes,
            sha256: read.sha256.clone(),
            path: map_file_path(&description.id, &description.version, name),
            credits: file
                .credits
                .iter()
                .map(|credit| MapCredit {
                    text: credit.text.clone(),
                    url: credit.url.clone(),
                })
                .collect(),
        });
        files.push(Arc::new(read));
    }
    Ok(Some(Region {
        listed: MapRegion {
            id: description.id.clone(),
            name: description.name.clone(),
            version: description.version.clone(),
            covers: description.covers,
            files: listed,
        },
        files,
    }))
}

/// Reads a file from beginning to end and says what it is: its size, its
/// SHA-256 and when it was written. `Ok(None)` when `stop` ended the
/// reading.
fn read_file(path: &Path, stop: &CancellationToken) -> std::io::Result<Option<CheckedFile>> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0; HASHING_CHUNK];
    let mut bytes: u64 = 0;
    loop {
        // A region of some gigabytes must not keep the program from
        // stopping.
        if stop.is_cancelled() {
            return Ok(None);
        }
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        hasher.update(&chunk[..count]);
        bytes += count as u64;
    }
    // A file that was written to while it was read has no hash to speak
    // of.
    let after = file.metadata()?;
    let modified = after.modified().ok();
    if after.len() != bytes || before.len() != bytes || before.modified().ok() != modified {
        return Err(std::io::Error::other(
            "the file changed while it was being read",
        ));
    }
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(Some(CheckedFile {
        path: path.to_owned(),
        bytes,
        sha256,
        modified,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of regions that a test fills and that goes when the test
    /// is over.
    struct Folder(PathBuf);

    impl Folder {
        fn new(test: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("venti-maps-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        /// Writes a region's folder: its description and one file of
        /// `content`, called `basemap.pmtiles`.
        fn region(&self, folder: &str, description: &str, content: &[u8]) -> PathBuf {
            let path = self.0.join(folder);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join(DESCRIPTION), description).unwrap();
            fs::write(path.join("basemap.pmtiles"), content).unwrap();
            path
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const CONTENT: &[u8] = b"not really a tile archive";

    fn sha256(content: &[u8]) -> String {
        Sha256::digest(content)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// A description that keeps every rule, for a file of [`CONTENT`].
    fn description(id: &str) -> String {
        format!(
            r#"id = "{id}"
name = "A valley"
version = "2026-11-20"
covers = {{ south = 47.6, west = 11.64, north = 47.76, east = 11.86 }}

[[files]]
role = "basemap"
name = "basemap.pmtiles"
sha256 = "{}"
credits = [
  {{ text = "© OpenStreetMap contributors", url = "https://www.openstreetmap.org/copyright" }},
  {{ text = "Somebody without a page" }},
]
"#,
            sha256(CONTENT)
        )
    }

    fn read(folder: &Folder) -> (Maps, Reading) {
        let maps = Maps::default();
        let reading = maps.read(&folder.0);
        (maps, reading)
    }

    #[test]
    fn a_region_that_keeps_the_rules_is_listed_as_its_description_says() {
        let folder = Folder::new("listed");
        let path = folder.region("Any Folder Name", &description("a-valley"), CONTENT);
        // A folder without a description is no region, and neither is a
        // file.
        fs::create_dir_all(folder.0.join("fonts")).unwrap();
        fs::write(folder.0.join("sample.igc"), b"x").unwrap();

        let (maps, reading) = read(&folder);

        assert_eq!(reading.listed, ["a-valley"]);
        assert_eq!((reading.left_out, reading.stopped), (0, false));
        let regions = maps.list().regions;
        assert_eq!(regions.len(), 1);
        let region = &regions[0];
        assert_eq!(region.id, "a-valley");
        assert_eq!(region.name, "A valley");
        assert_eq!(region.version, "2026-11-20");
        assert_eq!(
            region.covers,
            BoundingBox::new(47.6, 11.64, 47.76, 11.86).unwrap()
        );
        let file = &region.files[0];
        assert_eq!(file.role, MapFileRole::Basemap);
        assert_eq!(file.bytes, CONTENT.len() as u64);
        assert_eq!(file.sha256, sha256(CONTENT));
        assert_eq!(file.path, "/v1/maps/a-valley/2026-11-20/basemap.pmtiles");
        assert_eq!(file.credits.len(), 2);
        assert_eq!(file.credits[1].url, None);

        // The file is found by what the list names, and by nothing else.
        let found = maps
            .file("a-valley", "2026-11-20", "basemap.pmtiles")
            .unwrap();
        assert_eq!(
            found.path,
            fs::canonicalize(path.join("basemap.pmtiles")).unwrap()
        );
        assert_eq!(found.bytes, file.bytes);
        assert!(
            maps.file("a-valley", "2026-11-19", "basemap.pmtiles")
                .is_none()
        );
        assert!(maps.file("a-valley", "2026-11-20", "map.toml").is_none());
        assert!(
            maps.file("another", "2026-11-20", "basemap.pmtiles")
                .is_none()
        );
        assert!(maps.file("a-valley", "2026-11-20", "../map.toml").is_none());
    }

    #[test]
    fn a_region_whose_file_has_another_hash_is_not_listed() {
        let folder = Folder::new("hash");
        folder.region("good", &description("good"), CONTENT);
        let other = folder.region("other", &description("other"), b"something else entirely");

        let (maps, reading) = read(&folder);

        assert_eq!(reading.listed, ["good"]);
        assert_eq!(reading.left_out, 1);
        assert_eq!(maps.list().regions.len(), 1);
        assert!(
            maps.file("other", "2026-11-20", "basemap.pmtiles")
                .is_none()
        );
        // The reason names both hashes.
        let described = describe(&other).unwrap();
        let reason = check(&described, &CancellationToken::new()).unwrap_err();
        assert!(reason.contains(&sha256(CONTENT)), "{reason}");
        assert!(
            reason.contains(&sha256(b"something else entirely")),
            "{reason}"
        );
    }

    #[test]
    fn a_description_that_breaks_a_rule_names_the_rule() {
        let folder = Folder::new("rules");
        let good = description("a-valley");
        let hash = sha256(CONTENT);
        let cases: Vec<(&str, String, &str)> = vec![
            ("id", good.replace("a-valley", "A Valley"), "the id"),
            ("out", good.replace("a-valley", "../out"), "the id"),
            (
                "version",
                good.replace("2026-11-20", "2026/11/20"),
                "the version",
            ),
            ("name", good.replace("A valley", " "), "the name"),
            (
                "covers",
                good.replace("south = 47.6", "south = 97.6"),
                "cannot be used",
            ),
            (
                "turned",
                good.replace("south = 47.6", "south = 48.6"),
                "cannot be used",
            ),
            (
                "role",
                good.replace("\"basemap\"", "\"basmap\""),
                "the role",
            ),
            (
                "below",
                good.replace("\"basemap.pmtiles\"", "\"tiles/basemap.pmtiles\""),
                "the file name",
            ),
            (
                "above",
                good.replace("\"basemap.pmtiles\"", "\"../basemap.pmtiles\""),
                "the file name",
            ),
            (
                "missing",
                good.replace("\"basemap.pmtiles\"", "\"terrain.pmtiles\""),
                "cannot be read",
            ),
            ("short", good.replace(&hash, &hash[..63]), "sha256"),
            ("upper", good.replace(&hash, &hash.to_uppercase()), "sha256"),
            (
                "credit",
                good.replace("Somebody without a page", ""),
                "a credit's text",
            ),
            (
                "link",
                good.replace("https://www.openstreetmap.org", "javascript:alert(1)//"),
                "a credit's url",
            ),
            (
                "typo",
                good.replace("version =", "verison ="),
                "cannot be used",
            ),
            ("toml", "id = ".to_owned(), "cannot be used"),
        ];
        for (name, text, expected) in &cases {
            let path = folder.region(name, text, CONTENT);
            let reason = describe(&path).unwrap_err();
            assert!(reason.contains(expected), "{name}: {reason}");
        }
        // No credit at all, no file at all, and a file listed twice.
        let credits = good.find("credits = [").unwrap();
        let path = folder.region("uncredited", &good[..credits], CONTENT);
        assert!(describe(&path).unwrap_err().contains("names no credit"));
        let files = good.find("[[files]]").unwrap();
        let path = folder.region("empty", &good[..files], CONTENT);
        assert!(describe(&path).unwrap_err().contains("lists no files"));
        let twice = format!("{good}\n{}", &good[files..]);
        let path = folder.region("twice", &twice, CONTENT);
        assert!(describe(&path).unwrap_err().contains("twice"));
        // A folder where a file should be.
        let path = folder.region("folder", &good.replace("basemap.pmtiles", "tiles"), CONTENT);
        fs::create_dir_all(path.join("tiles")).unwrap();
        assert!(describe(&path).unwrap_err().contains("is not a file"));

        // None of them is listed, and the one that is right still is.
        folder.region("right", &good, CONTENT);
        let (maps, reading) = read(&folder);
        assert_eq!(reading.listed, ["a-valley"]);
        assert_eq!(reading.left_out, cases.len() + 4);
        assert_eq!(maps.list().regions.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_regions_folder_is_refused_and_one_inside_it_is_not() {
        use std::os::unix::fs::symlink;

        let folder = Folder::new("links");
        let outside = folder.0.join("outside.bin");
        fs::write(&outside, CONTENT).unwrap();

        // The file is right, hash and all, and lies elsewhere.
        let out = folder.region("out", &description("out"), CONTENT);
        fs::remove_file(out.join("basemap.pmtiles")).unwrap();
        symlink(&outside, out.join("basemap.pmtiles")).unwrap();
        let reason = describe(&out).unwrap_err();
        assert!(
            reason.contains("leads out of the region's folder"),
            "{reason}"
        );

        // A link to a file beside it stays inside.
        let inside = folder.region("inside", &description("inside"), CONTENT);
        fs::rename(inside.join("basemap.pmtiles"), inside.join("real.bin")).unwrap();
        symlink("real.bin", inside.join("basemap.pmtiles")).unwrap();

        let (maps, reading) = read(&folder);
        assert_eq!(reading.listed, ["inside"]);
        assert_eq!(reading.left_out, 1);
        let file = maps
            .file("inside", "2026-11-20", "basemap.pmtiles")
            .unwrap();
        assert!(file.path.ends_with("inside/real.bin"));
    }

    #[test]
    fn two_folders_with_one_identifier_are_both_left_out() {
        let folder = Folder::new("twins");
        folder.region("first", &description("a-valley"), CONTENT);
        folder.region("second", &description("a-valley"), CONTENT);
        folder.region("third", &description("another"), CONTENT);

        let (maps, reading) = read(&folder);

        assert_eq!(reading.listed, ["another"]);
        assert_eq!(reading.left_out, 2);
        assert!(
            maps.file("a-valley", "2026-11-20", "basemap.pmtiles")
                .is_none()
        );
    }

    #[test]
    fn the_list_is_sorted_and_a_withdrawn_region_is_off_it() {
        let folder = Folder::new("sorted");
        for id in ["tegernsee", "alps", "zillertal"] {
            folder.region(&format!("folder-of-{id}"), &description(id), CONTENT);
        }
        let (maps, _) = read(&folder);
        let ids = |maps: &Maps| -> Vec<String> {
            maps.list().regions.into_iter().map(|r| r.id).collect()
        };
        assert_eq!(ids(&maps), ["alps", "tegernsee", "zillertal"]);

        assert!(maps.withdraw("tegernsee"));
        assert!(!maps.withdraw("tegernsee"));
        assert_eq!(ids(&maps), ["alps", "zillertal"]);
        assert!(
            maps.file("tegernsee", "2026-11-20", "basemap.pmtiles")
                .is_none()
        );
    }

    #[test]
    fn a_folder_that_is_not_there_or_holds_nothing_hands_out_nothing() {
        let folder = Folder::new("nothing");
        let (maps, reading) = read(&folder);
        assert_eq!(reading, Reading::default());
        assert!(maps.list().regions.is_empty());

        let maps = Maps::default();
        let missing = folder.0.join("no-such-folder");
        let reading = maps.read(&missing);
        assert_eq!(reading, Reading::default());
        assert!(maps.list().regions.is_empty());
    }

    #[test]
    fn reading_stops_when_the_program_does() {
        let folder = Folder::new("stopped");
        folder.region("a", &description("a-valley"), CONTENT);
        let stop = CancellationToken::new();
        stop.cancel();

        let maps = Maps::new(stop.clone());
        let reading = maps.read(&folder.0);

        assert!(reading.stopped);
        assert!(maps.list().regions.is_empty());
        // Also in the middle of a file.
        let described = describe(&folder.0.join("a")).unwrap();
        assert!(matches!(check(&described, &stop), Ok(None)));
    }

    #[tokio::test]
    async fn the_background_reading_lists_what_the_direct_one_lists() {
        let folder = Folder::new("background");
        folder.region("a", &description("a-valley"), CONTENT);
        let maps = Maps::default();
        assert!(maps.list().regions.is_empty());

        maps.read_in_background(folder.0.clone()).await.unwrap();

        assert_eq!(maps.list().regions[0].id, "a-valley");
    }

    #[test]
    fn a_file_that_was_replaced_is_no_longer_the_one_that_was_read() {
        let folder = Folder::new("replaced");
        let path = folder.region("a", &description("a-valley"), CONTENT);
        let (maps, _) = read(&folder);
        let file = maps
            .file("a-valley", "2026-11-20", "basemap.pmtiles")
            .unwrap();
        let now = || {
            let metadata = fs::metadata(path.join("basemap.pmtiles")).unwrap();
            (metadata.len(), metadata.modified().ok())
        };
        let (bytes, modified) = now();
        assert!(file.is_still(bytes, modified));

        fs::write(path.join("basemap.pmtiles"), b"a newer build of it").unwrap();

        let (bytes, modified) = now();
        assert!(!file.is_still(bytes, modified));
    }

    #[test]
    fn a_hash_a_line_and_a_link_are_told_from_what_is_none() {
        assert!(is_sha256(&"0123456789abcdef".repeat(4)));
        assert!(!is_sha256(&"0123456789ABCDEF".repeat(4)));
        assert!(!is_sha256(&"g".repeat(64)));
        assert!(!is_sha256(""));
        assert!(is_a_line("© OpenStreetMap contributors"));
        assert!(!is_a_line("two\nlines"));
        assert!(!is_a_line("\t"));
        assert!(is_a_link("https://mapterhorn.com/attribution"));
        assert!(is_a_link("http://maps.example"));
        for none in [
            "https://",
            "javascript:alert(1)",
            "mapterhorn.com",
            "https://a b",
            "",
        ] {
            assert!(!is_a_link(none), "{none}");
        }
    }
}
