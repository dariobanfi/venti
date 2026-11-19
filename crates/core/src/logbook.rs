//! The pilot's flights on this device.
//!
//! Every imported flight file is kept exactly as it was received, as
//! `flights/<id>.igc` in the core's storage, so nothing a recorder wrote
//! (its signature included) is lost. `logbook-index.json` next to it holds
//! what the logbook list shows for each flight, so opening the app does not
//! mean reading every file again. The index is a cache: it is rebuilt from
//! the files whenever it is missing, out of date or disagrees with them.
//!
//! The logbook reaches its files only through [`Storage`], so it is the
//! same on a phone's disk and in a browser.
//!
//! A flight's identity is a hash of its fixes. The same recording imported
//! twice, or downloaded from two places with different headers, is one
//! flight.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use venti_flight::igc::{self, IgcFile};
use venti_flight::{analysis, summarise, track};

use crate::storage::Storage;

/// The folder the flight files are in.
const FLIGHTS_FOLDER: &str = "flights";

/// The file the index is kept in.
const INDEX_FILE: &str = "logbook-index.json";

/// Bump when [`Summary`] changes shape or meaning; older indexes are rebuilt.
const INDEX_VERSION: u32 = 3;

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
    /// Reading or writing the storage failed.
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
    storage: Arc<dyn Storage>,
    index: Mutex<Index>,
}

impl Logbook {
    /// Opens the logbook kept in `storage` and brings the index in line
    /// with the flight files found there.
    ///
    /// Storage that has not fetched a file yet makes this fail with
    /// [`io::ErrorKind::WouldBlock`]. Whatever could be read by then is
    /// already in the index, so opening again after each fetch gets there
    /// a few files at a time.
    pub fn open(storage: Arc<dyn Storage>) -> Result<Self, LogbookError> {
        let logbook = Self {
            storage,
            index: Mutex::new(Index::default()),
        };

        let mut index = logbook.read_index()?;
        let mut changed = index.version != INDEX_VERSION;
        index.version = INDEX_VERSION;

        let stored: BTreeSet<String> = logbook
            .storage
            .list(FLIGHTS_FOLDER)?
            .iter()
            .filter_map(|name| flight_id(name))
            .map(str::to_owned)
            .collect();

        let before = index.flights.len();
        index.flights.retain(|id, _| stored.contains(id));
        changed |= index.flights.len() != before;

        let mut not_fetched = None;
        for id in stored {
            if index.flights.contains_key(&id) {
                continue;
            }
            match logbook.read_flight(&id).and_then(|text| read(&text)) {
                Ok((_, summary)) => {
                    index.flights.insert(id, summary);
                    changed = true;
                }
                Err(LogbookError::Storage(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                    not_fetched = Some(error);
                }
                // A stored file that no longer reads is left in place and
                // out of the list; it was valid when it was imported.
                Err(_) => {}
            }
        }

        if changed {
            logbook.write_index(&index)?;
        }
        if let Some(error) = not_fetched {
            return Err(error.into());
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
        self.storage.write(&flight_file(&id), igc_text.as_bytes())?;
        index.flights.insert(id.clone(), summary);
        self.write_index(&index)?;
        Ok((id, false))
    }

    /// The text of a stored flight file.
    pub fn flight_text(&self, id: &str) -> Result<String, LogbookError> {
        if !self.lock().flights.contains_key(id) {
            return Err(LogbookError::Unknown);
        }
        self.read_flight(id)
    }

    /// Removes a flight and its file.
    pub fn delete(&self, id: &str) -> Result<(), LogbookError> {
        let mut index = self.lock();
        if index.flights.remove(id).is_none() {
            return Err(LogbookError::Unknown);
        }
        self.storage.remove(&flight_file(id))?;
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

    /// The text of a flight file. A flight whose file has gone is no longer
    /// in the logbook, whatever the index says.
    fn read_flight(&self, id: &str) -> Result<String, LogbookError> {
        let bytes = self
            .storage
            .read(&flight_file(id))?
            .ok_or(LogbookError::Unknown)?;
        String::from_utf8(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error).into())
    }

    /// The stored index, or an empty one if it is missing, unreadable or
    /// from another version.
    fn read_index(&self) -> Result<Index, LogbookError> {
        let bytes = match self.storage.read(INDEX_FILE) {
            // The index is there and only has to be fetched. Rebuilding it
            // from every flight file instead would be a waste.
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Err(error.into()),
            read => read.ok().flatten(),
        };
        Ok(bytes
            .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
            .filter(|index| index.version == INDEX_VERSION)
            .unwrap_or_default())
    }

    fn write_index(&self, index: &Index) -> Result<(), LogbookError> {
        let bytes = serde_json::to_vec(index).map_err(io::Error::other)?;
        self.storage.write(INDEX_FILE, &bytes)?;
        Ok(())
    }
}

/// Parses a flight file and works out its logbook summary. The summary
/// describes the part of the recording that was flown; the file is kept
/// whole.
fn read(igc_text: &str) -> Result<(IgcFile, Summary), LogbookError> {
    let file = igc::parse(igc_text).map_err(LogbookError::Invalid)?;
    let flown = analysis::flown_part(&file.fixes);
    let summary = summarise(flown).ok_or(LogbookError::Invalid(igc::IgcError::NoFixes))?;
    let summary = Summary {
        start_unix_s: summary.start.timestamp(),
        duration_s: summary.duration_s(),
        track_length_m: summary.track_length_m,
        max_altitude_m: summary.max_altitude_m,
        max_climb_ms: summary.max_climb_ms,
        pilot: file.pilot.clone(),
        glider: file.glider_type.clone(),
        outline: track::outline(flown, OUTLINE_POINTS)
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

/// The name a flight's file is stored under.
fn flight_file(id: &str) -> String {
    format!("{FLIGHTS_FOLDER}/{id}.igc")
}

/// The flight a stored file belongs to, if it is a flight file at all.
fn flight_id(name: &str) -> Option<&str> {
    name.strip_prefix(FLIGHTS_FOLDER)?
        .strip_prefix('/')?
        .strip_suffix(".igc")
        .filter(|id| is_id(id))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use venti_flight::testing::synthetic_flight;

    use super::*;
    use crate::storage::testing::{Backing, on_every_storage};
    use crate::storage::{DiskStorage, StagedStorage};
    use crate::testing::TempFolder;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// The sample flight moved to another year, which makes it another flight.
    fn sample_in_2025() -> String {
        sample().replace("040726", "040725")
    }

    fn open(backing: &dyn Backing) -> Logbook {
        Logbook::open(backing.open()).unwrap()
    }

    #[test]
    fn an_imported_flight_is_listed_and_can_be_read_back() {
        on_every_storage(|backing| {
            let logbook = open(backing);

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
        });
    }

    #[test]
    fn the_same_recording_is_stored_once_whatever_its_headers_say() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (id, _) = logbook.import(&sample()).unwrap();

            let renamed = sample().replace("Sample Pilot", "Somebody Else");
            assert_ne!(renamed, sample());
            assert_eq!(logbook.import(&renamed).unwrap(), (id, true));
            assert_eq!(logbook.flights().len(), 1);
        });
    }

    #[test]
    fn flights_are_listed_newest_first() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (older, _) = logbook.import(&sample_in_2025()).unwrap();
            let (newer, _) = logbook.import(&sample()).unwrap();

            let ids: Vec<String> = logbook.flights().into_iter().map(|(id, _)| id).collect();
            assert_eq!(ids, [newer, older]);
        });
    }

    #[test]
    fn the_logbook_survives_closing_and_reopening() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();

            let reopened = open(backing);
            assert_eq!(reopened.flights().len(), 1);
            assert_eq!(reopened.flights()[0].0, id);
            assert_eq!(reopened.flight_text(&id).unwrap(), sample());
        });
    }

    #[test]
    fn a_lost_or_damaged_index_is_rebuilt_from_the_flight_files() {
        on_every_storage(|backing| {
            let before = {
                let logbook = open(backing);
                logbook.import(&sample()).unwrap();
                logbook.import(&sample_in_2025()).unwrap();
                logbook.flights()
            };

            backing.open().remove(INDEX_FILE).unwrap();
            assert_eq!(open(backing).flights(), before);

            backing.open().write(INDEX_FILE, b"not json").unwrap();
            assert_eq!(open(backing).flights(), before);
        });
    }

    #[test]
    fn a_flight_file_removed_behind_the_logbooks_back_leaves_the_list() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();

            backing.open().remove(&flight_file(&id)).unwrap();
            assert!(open(backing).flights().is_empty());
        });
    }

    #[test]
    fn a_flight_whose_file_goes_while_the_logbook_is_open_is_no_longer_in_it() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (id, _) = logbook.import(&sample()).unwrap();

            backing.open().remove(&flight_file(&id)).unwrap();

            assert!(matches!(
                logbook.flight_text(&id),
                Err(LogbookError::Unknown)
            ));
        });
    }

    #[test]
    fn a_stored_file_that_is_not_a_flight_is_left_out_of_the_list() {
        on_every_storage(|backing| {
            let (id, _) = open(backing).import(&sample()).unwrap();
            let storage = backing.open();
            storage
                .write(&flight_file(&"0".repeat(32)), b"not a flight")
                .unwrap();
            storage.write("flights/notes.txt", b"hello").unwrap();
            storage.remove(INDEX_FILE).unwrap();

            let ids: Vec<String> = open(backing)
                .flights()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            assert_eq!(ids, [id]);
        });
    }

    #[test]
    fn a_deleted_flight_is_gone_from_the_list_and_the_storage() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            let (id, _) = logbook.import(&sample()).unwrap();

            logbook.delete(&id).unwrap();

            assert!(logbook.flights().is_empty());
            assert!(matches!(
                logbook.flight_text(&id),
                Err(LogbookError::Unknown)
            ));
            assert!(matches!(logbook.delete(&id), Err(LogbookError::Unknown)));
            let left = backing.open().list(FLIGHTS_FOLDER).unwrap();
            assert!(left.is_empty(), "{left:?}");
            assert!(open(backing).flights().is_empty());
        });
    }

    #[test]
    fn text_that_is_not_a_flight_is_refused_and_nothing_is_stored() {
        on_every_storage(|backing| {
            let logbook = open(backing);

            assert!(matches!(
                logbook.import("hello"),
                Err(LogbookError::Invalid(igc::IgcError::NoDate))
            ));
            assert!(logbook.flights().is_empty());
            assert!(backing.open().list(FLIGHTS_FOLDER).unwrap().is_empty());
        });
    }

    #[test]
    fn an_identifier_that_could_leave_the_folder_is_never_looked_up() {
        on_every_storage(|backing| {
            let logbook = open(backing);
            logbook.import(&sample()).unwrap();

            for id in ["", "../../etc/passwd", "ABCDEF0123456789ABCDEF0123456789"] {
                assert!(!is_id(id));
                assert!(matches!(
                    logbook.flight_text(id),
                    Err(LogbookError::Unknown)
                ));
            }
        });
    }

    /// Phones already hold logbooks in this layout. It must not change
    /// without code that moves them.
    #[test]
    fn on_disk_a_flight_is_one_file_named_by_its_identifier_beside_the_index() {
        let folder = TempFolder::new();
        let storage = Arc::new(DiskStorage::open(folder.path()).unwrap());
        let (id, _) = Logbook::open(storage).unwrap().import(&sample()).unwrap();

        let file = folder.path().join("flights").join(format!("{id}.igc"));
        assert_eq!(fs::read_to_string(file).unwrap(), sample());
        let index = fs::read(folder.path().join("logbook-index.json")).unwrap();
        let index: serde_json::Value = serde_json::from_slice(&index).unwrap();
        assert_eq!(index["version"], INDEX_VERSION);
        assert_eq!(index["flights"][&id]["duration_s"], 1744);

        let mut left: Vec<_> = fs::read_dir(folder.path().join("flights"))
            .unwrap()
            .chain(fs::read_dir(folder.path()).unwrap())
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [format!("{id}.igc"), "flights".into(), INDEX_FILE.into()]
        );
    }

    /// What a browser's store holds after two flights were imported.
    fn two_stored_flights() -> BTreeMap<String, Vec<u8>> {
        let staged = Arc::new(StagedStorage::default());
        let logbook = Logbook::open(Arc::clone(&staged) as Arc<dyn Storage>).unwrap();
        logbook.import(&sample()).unwrap();
        logbook.import(&sample_in_2025()).unwrap();
        staged
            .take_staged()
            .into_iter()
            .map(|file| (file.name, file.contents.expect("nothing was removed")))
            .collect()
    }

    fn would_block<T: std::fmt::Debug>(result: Result<T, LogbookError>) {
        match result {
            Err(LogbookError::Storage(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
            other => panic!("expected a read that would block, got {other:?}"),
        }
    }

    #[test]
    fn a_hosted_logbook_asks_for_its_index_and_then_for_each_flight_it_shows() {
        let store = two_stored_flights();
        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        let storage = || Arc::clone(&staged) as Arc<dyn Storage>;

        would_block(Logbook::open(storage()));
        assert_eq!(staged.take_wanted(), [INDEX_FILE]);

        staged.provide(INDEX_FILE, store.get(INDEX_FILE).cloned());
        let logbook = Logbook::open(storage()).unwrap();
        staged.release();
        // An index that agrees with the files is neither rebuilt nor
        // written again, and no flight file is fetched to open the list.
        assert!(staged.take_wanted().is_empty());
        assert!(staged.take_staged().is_empty());
        assert_eq!(logbook.flights().len(), 2);

        let id = logbook.flights()[0].0.clone();
        would_block(logbook.flight_text(&id));
        assert_eq!(staged.take_wanted(), [flight_file(&id)]);
        staged.provide(&flight_file(&id), store.get(&flight_file(&id)).cloned());
        assert_eq!(logbook.flight_text(&id).unwrap(), sample());
    }

    #[test]
    fn a_hosted_logbook_rebuilds_a_lost_index_a_few_flights_at_a_time() {
        let mut store = two_stored_flights();
        store.remove(INDEX_FILE);
        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        let storage = || Arc::clone(&staged) as Arc<dyn Storage>;

        // Nothing is in memory: both flights are wanted.
        would_block(Logbook::open(storage()));
        let wanted = staged.take_wanted();
        assert_eq!(wanted.len(), 2);

        // With one fetched, that one is indexed and the other still wanted.
        staged.provide(&wanted[0], store.get(&wanted[0]).cloned());
        would_block(Logbook::open(storage()));
        assert_eq!(staged.take_wanted(), [wanted[1].clone()]);
        staged.release();

        staged.provide(&wanted[1], store.get(&wanted[1]).cloned());
        let logbook = Logbook::open(storage()).unwrap();
        assert_eq!(logbook.flights().len(), 2);
        let staged_names: Vec<String> = staged
            .take_staged()
            .into_iter()
            .map(|file| file.name)
            .collect();
        assert_eq!(staged_names, [INDEX_FILE]);
    }
}
