//! The pilot's flights on this device.
//!
//! Every imported flight file is kept exactly as it was received, in
//! `flights/<id>.igc` under the core's data folder, so nothing a recorder
//! wrote (its signature included) is lost. `logbook-index.json` next to it
//! holds what the logbook list shows for each flight, so opening the app
//! does not mean reading every file again. The index is a cache: it is
//! rebuilt from the files whenever it is missing, out of date or disagrees
//! with them.
//!
//! A flight's identity is a hash of its fixes. The same recording imported
//! twice, or downloaded from two places with different headers, is one
//! flight.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use venti_flight::igc::{self, IgcFile};
use venti_flight::{summarise, track};

/// Bump when [`Summary`] changes shape or meaning; older indexes are rebuilt.
const INDEX_VERSION: u32 = 1;

/// The most points an outline keeps. Enough for a recognisable thumbnail and
/// an overview map, small enough to hold thousands of flights in memory.
const OUTLINE_POINTS: usize = 96;

/// What can go wrong in the logbook.
#[derive(Debug)]
pub(crate) enum LogbookError {
    /// The text is not a usable flight file.
    Invalid(igc::IgcError),
    /// No flight with this identifier is stored.
    Unknown,
    /// Reading or writing the data folder failed.
    Storage(io::Error),
}

impl From<io::Error> for LogbookError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}

/// What the logbook list shows for one flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Summary {
    pub start_unix_s: i64,
    pub duration_s: i64,
    pub track_length_m: f64,
    pub max_altitude_m: i32,
    pub max_climb_ms: f64,
    pub pilot: Option<String>,
    pub glider: Option<String>,
    /// The simplified track as latitude and longitude pairs.
    pub outline: Vec<[f64; 2]>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    flights: BTreeMap<String, Summary>,
}

/// The stored flights and their index.
#[derive(Debug)]
pub(crate) struct Logbook {
    folder: PathBuf,
    index: Mutex<Index>,
}

impl Logbook {
    /// Opens the logbook in `folder`, creating it if needed, and brings the
    /// index in line with the flight files found there.
    pub fn open(folder: &Path) -> Result<Self, LogbookError> {
        let logbook = Self {
            folder: folder.to_owned(),
            index: Mutex::new(Index::default()),
        };
        fs::create_dir_all(logbook.flights_folder())?;

        let mut index = logbook.read_index();
        let mut changed = index.version != INDEX_VERSION;
        index.version = INDEX_VERSION;

        let mut stored = Vec::new();
        for entry in fs::read_dir(logbook.flights_folder())? {
            let path = entry?.path();
            let id = path.file_stem().and_then(|stem| stem.to_str());
            let extension = path.extension().and_then(|extension| extension.to_str());
            if let (Some(id), Some("igc")) = (id, extension)
                && is_id(id)
            {
                stored.push(id.to_owned());
            }
        }

        let before = index.flights.len();
        index.flights.retain(|id, _| stored.contains(id));
        changed |= index.flights.len() != before;

        for id in stored {
            if index.flights.contains_key(&id) {
                continue;
            }
            // A stored file that no longer reads is left in place and out of
            // the list; it was valid when it was imported.
            if let Ok((_, summary)) = fs::read_to_string(logbook.flight_path(&id))
                .map_err(LogbookError::from)
                .and_then(|text| read(&text))
            {
                index.flights.insert(id, summary);
                changed = true;
            }
        }

        if changed {
            logbook.write_index(&index)?;
        }
        *logbook.lock() = index;
        Ok(logbook)
    }

    /// Stores a flight file. Returns the flight's identifier and whether it
    /// was already in the logbook.
    pub fn import(&self, igc_text: &str) -> Result<(String, bool), LogbookError> {
        let (file, summary) = read(igc_text)?;
        let id = identity(&file);

        let mut index = self.lock();
        if index.flights.contains_key(&id) {
            return Ok((id, true));
        }
        write_atomically(&self.flight_path(&id), igc_text.as_bytes())?;
        index.flights.insert(id.clone(), summary);
        self.write_index(&index)?;
        Ok((id, false))
    }

    /// The text of a stored flight file.
    pub fn flight_text(&self, id: &str) -> Result<String, LogbookError> {
        if !self.lock().flights.contains_key(id) {
            return Err(LogbookError::Unknown);
        }
        Ok(fs::read_to_string(self.flight_path(id))?)
    }

    /// Removes a flight and its file.
    pub fn delete(&self, id: &str) -> Result<(), LogbookError> {
        let mut index = self.lock();
        if index.flights.remove(id).is_none() {
            return Err(LogbookError::Unknown);
        }
        match fs::remove_file(self.flight_path(id)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        self.write_index(&index)?;
        Ok(())
    }

    /// Every flight with its summary, newest first.
    pub fn flights(&self) -> Vec<(String, Summary)> {
        let mut flights: Vec<(String, Summary)> = self
            .lock()
            .flights
            .iter()
            .map(|(id, summary)| (id.clone(), summary.clone()))
            .collect();
        flights.sort_by(|a, b| (b.1.start_unix_s, &b.0).cmp(&(a.1.start_unix_s, &a.0)));
        flights
    }

    fn lock(&self) -> MutexGuard<'_, Index> {
        // A panic while the lock was held cannot leave the index half
        // written: it is only replaced or changed by single inserts and
        // removals. Carry on with what is there.
        self.index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn flights_folder(&self) -> PathBuf {
        self.folder.join("flights")
    }

    fn flight_path(&self, id: &str) -> PathBuf {
        self.flights_folder().join(format!("{id}.igc"))
    }

    fn index_path(&self) -> PathBuf {
        self.folder.join("logbook-index.json")
    }

    /// The index on disk, or an empty one if it is missing, unreadable or
    /// from another version.
    fn read_index(&self) -> Index {
        fs::read(self.index_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
            .filter(|index| index.version == INDEX_VERSION)
            .unwrap_or_default()
    }

    fn write_index(&self, index: &Index) -> Result<(), LogbookError> {
        let bytes = serde_json::to_vec(index).map_err(io::Error::other)?;
        write_atomically(&self.index_path(), &bytes)?;
        Ok(())
    }
}

/// Parses a flight file and works out its logbook summary.
fn read(igc_text: &str) -> Result<(IgcFile, Summary), LogbookError> {
    let file = igc::parse(igc_text).map_err(LogbookError::Invalid)?;
    let summary = summarise(&file.fixes).ok_or(LogbookError::Invalid(igc::IgcError::NoFixes))?;
    let summary = Summary {
        start_unix_s: summary.start.timestamp(),
        duration_s: summary.duration_s(),
        track_length_m: summary.track_length_m,
        max_altitude_m: summary.max_altitude_m,
        max_climb_ms: summary.max_climb_ms,
        pilot: file.pilot.clone(),
        glider: file.glider_type.clone(),
        outline: track::outline(&file.fixes, OUTLINE_POINTS)
            .into_iter()
            .map(|point| [point.lat(), point.lon()])
            .collect(),
    };
    Ok((file, summary))
}

/// A flight's identifier: 32 hexadecimal characters from a hash of its fixes.
fn identity(file: &IgcFile) -> String {
    let mut hash = Sha256::new();
    for fix in &file.fixes {
        hash.update(fix.time.timestamp().to_le_bytes());
        hash.update(fix.position.lat().to_bits().to_le_bytes());
        hash.update(fix.position.lon().to_bits().to_le_bytes());
        hash.update(fix.gps_altitude_m.to_le_bytes());
        hash.update(fix.pressure_altitude_m.to_le_bytes());
    }
    hash.finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether the text has the form of a flight identifier. Identifiers arrive
/// from outside the core and become file names, so nothing else is accepted.
fn is_id(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Writes a file so that a crash leaves either the old contents or the new,
/// never half of each.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let partial = path.with_extension("partial");
    fs::write(&partial, bytes)?;
    fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::testing::TempFolder;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// The sample flight moved to another year, which makes it another flight.
    fn sample_in_2025() -> String {
        sample().replace("040726", "040725")
    }

    #[test]
    fn an_imported_flight_is_listed_and_can_be_read_back() {
        let folder = TempFolder::new();
        let logbook = Logbook::open(folder.path()).unwrap();

        let (id, already) = logbook.import(&sample()).unwrap();

        assert!(is_id(&id), "{id}");
        assert!(!already);
        let flights = logbook.flights();
        assert_eq!(flights.len(), 1);
        assert_eq!(flights[0].0, id);
        assert_eq!(flights[0].1.duration_s, 1744);
        assert_eq!(flights[0].1.max_altitude_m, 2117);
        assert_eq!(flights[0].1.glider.as_deref(), Some("Sample Wing"));
        assert!(flights[0].1.outline.len() <= OUTLINE_POINTS);
        assert_eq!(logbook.flight_text(&id).unwrap(), sample());
    }

    #[test]
    fn the_same_recording_is_stored_once_whatever_its_headers_say() {
        let folder = TempFolder::new();
        let logbook = Logbook::open(folder.path()).unwrap();
        let (id, _) = logbook.import(&sample()).unwrap();

        let renamed = sample().replace("Sample Pilot", "Somebody Else");
        assert_ne!(renamed, sample());
        assert_eq!(logbook.import(&renamed).unwrap(), (id, true));
        assert_eq!(logbook.flights().len(), 1);
    }

    #[test]
    fn flights_are_listed_newest_first() {
        let folder = TempFolder::new();
        let logbook = Logbook::open(folder.path()).unwrap();
        let (older, _) = logbook.import(&sample_in_2025()).unwrap();
        let (newer, _) = logbook.import(&sample()).unwrap();

        let ids: Vec<String> = logbook.flights().into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, [newer, older]);
    }

    #[test]
    fn the_logbook_survives_closing_and_reopening() {
        let folder = TempFolder::new();
        let (id, _) = Logbook::open(folder.path())
            .unwrap()
            .import(&sample())
            .unwrap();

        let reopened = Logbook::open(folder.path()).unwrap();
        assert_eq!(reopened.flights().len(), 1);
        assert_eq!(reopened.flights()[0].0, id);
    }

    #[test]
    fn a_lost_or_damaged_index_is_rebuilt_from_the_flight_files() {
        let folder = TempFolder::new();
        let before = {
            let logbook = Logbook::open(folder.path()).unwrap();
            logbook.import(&sample()).unwrap();
            logbook.import(&sample_in_2025()).unwrap();
            logbook.flights()
        };

        fs::remove_file(folder.path().join("logbook-index.json")).unwrap();
        assert_eq!(Logbook::open(folder.path()).unwrap().flights(), before);

        fs::write(folder.path().join("logbook-index.json"), "not json").unwrap();
        assert_eq!(Logbook::open(folder.path()).unwrap().flights(), before);
    }

    #[test]
    fn a_flight_file_removed_behind_the_logbooks_back_leaves_the_list() {
        let folder = TempFolder::new();
        let (id, _) = Logbook::open(folder.path())
            .unwrap()
            .import(&sample())
            .unwrap();

        fs::remove_file(folder.path().join("flights").join(format!("{id}.igc"))).unwrap();
        assert!(Logbook::open(folder.path()).unwrap().flights().is_empty());
    }

    #[test]
    fn a_deleted_flight_is_gone_from_the_list_and_the_disk() {
        let folder = TempFolder::new();
        let logbook = Logbook::open(folder.path()).unwrap();
        let (id, _) = logbook.import(&sample()).unwrap();

        logbook.delete(&id).unwrap();

        assert!(logbook.flights().is_empty());
        assert!(matches!(
            logbook.flight_text(&id),
            Err(LogbookError::Unknown)
        ));
        assert!(matches!(logbook.delete(&id), Err(LogbookError::Unknown)));
        let left: Vec<_> = fs::read_dir(folder.path().join("flights"))
            .unwrap()
            .collect();
        assert!(left.is_empty(), "{left:?}");
        assert!(Logbook::open(folder.path()).unwrap().flights().is_empty());
    }

    #[test]
    fn text_that_is_not_a_flight_is_refused_and_nothing_is_stored() {
        let folder = TempFolder::new();
        let logbook = Logbook::open(folder.path()).unwrap();

        assert!(matches!(
            logbook.import("hello"),
            Err(LogbookError::Invalid(igc::IgcError::NoDate))
        ));
        assert!(logbook.flights().is_empty());
    }

    #[test]
    fn an_identifier_that_could_leave_the_folder_is_never_looked_up() {
        let folder = TempFolder::new();
        let logbook = Logbook::open(folder.path()).unwrap();
        logbook.import(&sample()).unwrap();

        for id in ["", "../../etc/passwd", "ABCDEF0123456789ABCDEF0123456789"] {
            assert!(!is_id(id));
            assert!(matches!(
                logbook.flight_text(id),
                Err(LogbookError::Unknown)
            ));
        }
    }
}
