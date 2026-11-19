//! Where the core keeps its files.
//!
//! The core stores whole files under short names such as
//! `flights/<id>.igc`. What holds them differs by platform, so the logbook
//! reaches them through [`Storage`] and never touches a disk itself:
//!
//! - [`DiskStorage`] is a folder the app owns. The phones use it.
//! - [`StagedStorage`] is for a host that can only store things
//!   asynchronously, which is every browser. It keeps the names of the
//!   stored files and hands the reading and writing to the host.
//!
//! Everything built on files (a flight's identity, the index, summaries,
//! totals) is the same code on every platform.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// A place that keeps small files whole.
///
/// Names are relative, with `/` between a folder and the file in it.
pub trait Storage: fmt::Debug + Send + Sync {
    /// The names of the files directly inside `folder`, in no particular
    /// order. A folder that does not exist holds no files.
    fn list(&self, folder: &str) -> io::Result<Vec<String>>;

    /// The contents of a file, or `None` if there is no file of that name.
    ///
    /// [`io::ErrorKind::WouldBlock`] means the file exists but its contents
    /// are not at hand yet: the same call succeeds once the host has
    /// fetched them.
    fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>>;

    /// Stores a file, replacing one of the same name. Afterwards the file
    /// holds the old contents or the new, never half of each.
    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()>;

    /// Removes a file. Removing one that is not there is not an error.
    fn remove(&self, name: &str) -> io::Result<()>;
}

/// Files in a folder on disk.
#[derive(Debug)]
pub struct DiskStorage {
    folder: PathBuf,
}

impl DiskStorage {
    /// Uses `folder`, creating it if it does not exist.
    pub fn open(folder: &Path) -> io::Result<Self> {
        fs::create_dir_all(folder)?;
        Ok(Self {
            folder: folder.to_owned(),
        })
    }

    /// Where a file lives. A name that could lead out of the folder is
    /// refused, whoever asks.
    fn path(&self, name: &str) -> io::Result<PathBuf> {
        let relative = Path::new(name);
        let stays_inside = relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
        if name.is_empty() || !stays_inside {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("\"{name}\" is not a file name"),
            ));
        }
        Ok(self.folder.join(relative))
    }
}

impl Storage for DiskStorage {
    fn list(&self, folder: &str) -> io::Result<Vec<String>> {
        let entries = match fs::read_dir(self.path(folder)?) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut names = Vec::new();
        for entry in entries {
            // A name that is not text was not written by the core.
            if let Some(file) = entry?.file_name().to_str() {
                names.push(format!("{folder}/{file}"));
            }
        }
        Ok(names)
    }

    fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.path(name)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.path(name)?;
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        // Written beside the file and then moved over it, so that a crash
        // leaves either the old contents or the new.
        let partial = path.with_extension("partial");
        fs::write(&partial, bytes)?;
        fs::rename(&partial, path)
    }

    fn remove(&self, name: &str) -> io::Result<()> {
        match fs::remove_file(self.path(name)?) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

/// A file to write or remove in the host's store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    /// The file's name.
    pub name: String,
    /// What to store under that name, or `None` to remove the file.
    pub contents: Option<Vec<u8>>,
}

/// Storage for a host that reads and writes its store itself, and can only
/// do so asynchronously. A browser's storage is like that; the core's
/// interface is not.
///
/// It knows the name of every stored file but holds no contents of its own:
///
/// - A read of a file whose contents are not in memory fails with
///   [`io::ErrorKind::WouldBlock`], and the name is noted as wanted. The
///   host fetches what is wanted, hands it over with
///   [`provide`](Self::provide), and makes the same call to the core again.
/// - Writes and removals are staged. After a call that changes something,
///   the host takes them with [`take_staged`](Self::take_staged) and applies
///   them to its store. If it cannot, this storage no longer matches the
///   store and the host starts again with a new one.
/// - What the host provided is dropped again with
///   [`release`](Self::release), so a logbook of hundreds of flights is
///   never in memory at once.
#[derive(Debug, Default)]
pub struct StagedStorage {
    state: Mutex<Staged>,
}

#[derive(Debug, Default)]
struct Staged {
    /// The name of every file in the host's store.
    stored: BTreeSet<String>,
    /// Contents fetched by the host, until released.
    provided: BTreeMap<String, Vec<u8>>,
    /// Changes the host has not taken yet, in the form of [`StagedFile`].
    changes: BTreeMap<String, Option<Vec<u8>>>,
    /// Files read while their contents were not in memory, in the order
    /// they were first asked for.
    wanted: Vec<String>,
}

impl StagedStorage {
    /// Storage over a host's store that holds the files named.
    pub fn new(stored: impl IntoIterator<Item = String>) -> Self {
        Self {
            state: Mutex::new(Staged {
                stored: stored.into_iter().collect(),
                ..Staged::default()
            }),
        }
    }

    /// The files the core asked for and could not read. Asking empties the
    /// list.
    pub fn take_wanted(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().wanted)
    }

    /// Hands over the contents of a stored file, or says with `None` that
    /// the host's store has no such file after all.
    pub fn provide(&self, name: &str, contents: Option<Vec<u8>>) {
        let mut state = self.lock();
        match contents {
            Some(contents) => {
                state.stored.insert(name.to_owned());
                state.provided.insert(name.to_owned(), contents);
            }
            None => {
                state.stored.remove(name);
                state.provided.remove(name);
            }
        }
    }

    /// Drops the contents the host provided. The files stay stored.
    pub fn release(&self) {
        self.lock().provided.clear();
    }

    /// The changes to apply to the host's store, all together or not at
    /// all. Taking them empties the list and moves their contents out of
    /// memory.
    pub fn take_staged(&self) -> Vec<StagedFile> {
        let mut state = self.lock();
        let changes = std::mem::take(&mut state.changes);
        changes
            .into_iter()
            .map(|(name, contents)| {
                if contents.is_some() {
                    state.stored.insert(name.clone());
                } else {
                    state.stored.remove(&name);
                }
                StagedFile { name, contents }
            })
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Staged> {
        // Every change to the state is a single insert or removal, so a
        // panic while the lock was held cannot have left it half done.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Storage for StagedStorage {
    fn list(&self, folder: &str) -> io::Result<Vec<String>> {
        let state = self.lock();
        let inside = |name: &&String| {
            name.strip_prefix(folder)
                .and_then(|rest| rest.strip_prefix('/'))
                .is_some_and(|file| !file.contains('/'))
        };
        let mut names: BTreeSet<&String> = state.stored.iter().filter(inside).collect();
        for (name, contents) in &state.changes {
            if contents.is_none() {
                names.remove(name);
            } else if inside(&name) {
                names.insert(name);
            }
        }
        Ok(names.into_iter().cloned().collect())
    }

    fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        let mut state = self.lock();
        if let Some(change) = state.changes.get(name) {
            return Ok(change.clone());
        }
        if let Some(contents) = state.provided.get(name) {
            return Ok(Some(contents.clone()));
        }
        if !state.stored.contains(name) {
            return Ok(None);
        }
        if !state.wanted.iter().any(|wanted| wanted == name) {
            state.wanted.push(name.to_owned());
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            format!("\"{name}\" has not been fetched from the store yet"),
        ))
    }

    fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut state = self.lock();
        state.provided.remove(name);
        state.changes.insert(name.to_owned(), Some(bytes.to_vec()));
        Ok(())
    }

    fn remove(&self, name: &str) -> io::Result<()> {
        let mut state = self.lock();
        state.provided.remove(name);
        if state.stored.contains(name) {
            state.changes.insert(name.to_owned(), None);
        } else {
            // Written and removed again before the host took the write:
            // the host's store never needs to hear of it.
            state.changes.remove(name);
        }
        Ok(())
    }
}

/// Stores for tests: one of each kind, and the suite every kind must pass.
#[cfg(test)]
pub(crate) mod testing {
    use std::sync::Arc;

    use super::*;
    use crate::testing::TempFolder;

    /// Something a [`Storage`] can be opened over more than once, the way an
    /// app is closed and started again over the same data.
    pub(crate) trait Backing {
        fn open(&self) -> Arc<dyn Storage>;
    }

    /// A folder on disk.
    pub(crate) struct OnDisk(pub TempFolder);

    impl Backing for OnDisk {
        fn open(&self) -> Arc<dyn Storage> {
            Arc::new(DiskStorage::open(self.0.path()).unwrap())
        }
    }

    /// A stand-in for a browser's store and the shell in front of it.
    #[derive(Default)]
    pub(crate) struct InBrowser {
        store: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    }

    impl Backing for InBrowser {
        fn open(&self) -> Arc<dyn Storage> {
            let names: Vec<String> = self.store.lock().unwrap().keys().cloned().collect();
            Arc::new(Hosted {
                staged: StagedStorage::new(names),
                store: Arc::clone(&self.store),
            })
        }
    }

    /// A [`StagedStorage`] with a host that does its part at once: it
    /// fetches what a read wanted and repeats the read, and applies every
    /// change to its store as soon as it is staged.
    #[derive(Debug)]
    struct Hosted {
        staged: StagedStorage,
        store: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    }

    impl Hosted {
        fn commit(&self) {
            let mut store = self.store.lock().unwrap();
            for change in self.staged.take_staged() {
                match change.contents {
                    Some(contents) => store.insert(change.name, contents),
                    None => store.remove(&change.name),
                };
            }
        }
    }

    impl Storage for Hosted {
        fn list(&self, folder: &str) -> io::Result<Vec<String>> {
            self.staged.list(folder)
        }

        fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
            match self.staged.read(name) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    for wanted in self.staged.take_wanted() {
                        let contents = self.store.lock().unwrap().get(&wanted).cloned();
                        self.staged.provide(&wanted, contents);
                    }
                    let contents = self.staged.read(name);
                    self.staged.release();
                    contents
                }
                contents => contents,
            }
        }

        fn write(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
            self.staged.write(name, bytes)?;
            self.commit();
            Ok(())
        }

        fn remove(&self, name: &str) -> io::Result<()> {
            self.staged.remove(name)?;
            self.commit();
            Ok(())
        }
    }

    /// Runs a test against every kind of storage.
    pub(crate) fn on_every_storage(test: impl Fn(&dyn Backing)) {
        test(&OnDisk(TempFolder::new()));
        test(&InBrowser::default());
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{OnDisk, on_every_storage};
    use super::*;
    use crate::testing::TempFolder;

    fn sorted(mut names: Vec<String>) -> Vec<String> {
        names.sort();
        names
    }

    #[test]
    fn a_written_file_reads_back_and_is_listed_in_its_folder() {
        on_every_storage(|backing| {
            let storage = backing.open();
            assert_eq!(storage.read("flights/a.igc").unwrap(), None);
            assert!(storage.list("flights").unwrap().is_empty());

            storage.write("flights/a.igc", b"one").unwrap();
            storage.write("flights/b.igc", b"two").unwrap();
            storage.write("index.json", b"{}").unwrap();

            assert_eq!(
                storage.read("flights/a.igc").unwrap().as_deref(),
                Some(&b"one"[..])
            );
            assert_eq!(
                storage.read("index.json").unwrap().as_deref(),
                Some(&b"{}"[..])
            );
            assert_eq!(
                sorted(storage.list("flights").unwrap()),
                ["flights/a.igc", "flights/b.igc"]
            );
        });
    }

    #[test]
    fn writing_again_replaces_the_contents() {
        on_every_storage(|backing| {
            let storage = backing.open();
            storage.write("index.json", b"old and long").unwrap();
            storage.write("index.json", b"new").unwrap();

            assert_eq!(
                storage.read("index.json").unwrap().as_deref(),
                Some(&b"new"[..])
            );
        });
    }

    #[test]
    fn a_removed_file_is_gone_and_removing_it_twice_is_fine() {
        on_every_storage(|backing| {
            let storage = backing.open();
            storage.write("flights/a.igc", b"one").unwrap();

            storage.remove("flights/a.igc").unwrap();
            storage.remove("flights/a.igc").unwrap();
            storage.remove("flights/never-there.igc").unwrap();

            assert_eq!(storage.read("flights/a.igc").unwrap(), None);
            assert!(storage.list("flights").unwrap().is_empty());
        });
    }

    #[test]
    fn files_are_still_there_when_the_storage_is_opened_again() {
        on_every_storage(|backing| {
            {
                let storage = backing.open();
                storage.write("flights/a.igc", b"one").unwrap();
                storage.write("flights/b.igc", b"two").unwrap();
                storage.remove("flights/b.igc").unwrap();
            }

            let reopened = backing.open();
            assert_eq!(reopened.list("flights").unwrap(), ["flights/a.igc"]);
            assert_eq!(
                reopened.read("flights/a.igc").unwrap().as_deref(),
                Some(&b"one"[..])
            );
            assert_eq!(reopened.read("flights/b.igc").unwrap(), None);
        });
    }

    #[test]
    fn the_disk_refuses_a_name_that_leads_out_of_its_folder() {
        let backing = OnDisk(TempFolder::new());
        let storage = DiskStorage::open(backing.0.path()).unwrap();

        for name in ["", "../outside", "flights/../../outside", "/etc/passwd"] {
            let error = storage.write(name, b"x").unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name}");
            assert!(storage.read(name).is_err(), "{name}");
        }
    }

    #[test]
    fn the_disk_leaves_no_half_written_file_behind() {
        let backing = OnDisk(TempFolder::new());
        let storage = DiskStorage::open(backing.0.path()).unwrap();

        storage.write("flights/a.igc", b"one").unwrap();

        assert_eq!(storage.list("flights").unwrap(), ["flights/a.igc"]);
    }

    #[test]
    fn staged_storage_asks_for_contents_it_does_not_hold() {
        let storage = StagedStorage::new(["flights/a.igc".to_owned()]);

        let error = storage.read("flights/a.igc").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        // Asking twice wants the file once.
        storage.read("flights/a.igc").unwrap_err();
        assert_eq!(storage.take_wanted(), ["flights/a.igc"]);
        assert!(storage.take_wanted().is_empty());

        storage.provide("flights/a.igc", Some(b"one".to_vec()));
        assert_eq!(
            storage.read("flights/a.igc").unwrap().as_deref(),
            Some(&b"one"[..])
        );

        // Once released the contents are out of memory, and wanted again
        // by the next read.
        storage.release();
        storage.read("flights/a.igc").unwrap_err();
        assert_eq!(storage.take_wanted(), ["flights/a.igc"]);
    }

    #[test]
    fn staged_storage_forgets_a_file_the_store_turns_out_not_to_have() {
        let storage = StagedStorage::new(["flights/a.igc".to_owned()]);
        storage.read("flights/a.igc").unwrap_err();

        storage.provide("flights/a.igc", None);

        assert_eq!(storage.read("flights/a.igc").unwrap(), None);
        assert!(storage.list("flights").unwrap().is_empty());
    }

    #[test]
    fn staged_storage_hands_its_changes_to_the_host_once() {
        let storage = StagedStorage::new(["flights/old.igc".to_owned()]);
        storage.write("flights/new.igc", b"new").unwrap();
        storage.write("index.json", b"first").unwrap();
        storage.write("index.json", b"second").unwrap();
        storage.remove("flights/old.igc").unwrap();
        // Written and removed before the host heard of it: nothing to do.
        storage.write("flights/brief.igc", b"x").unwrap();
        storage.remove("flights/brief.igc").unwrap();

        // Before the host takes them, the changes already read back.
        assert_eq!(storage.list("flights").unwrap(), ["flights/new.igc"]);
        assert_eq!(
            storage.read("index.json").unwrap().as_deref(),
            Some(&b"second"[..])
        );

        assert_eq!(
            storage.take_staged(),
            [
                StagedFile {
                    name: "flights/new.igc".to_owned(),
                    contents: Some(b"new".to_vec()),
                },
                StagedFile {
                    name: "flights/old.igc".to_owned(),
                    contents: None,
                },
                StagedFile {
                    name: "index.json".to_owned(),
                    contents: Some(b"second".to_vec()),
                },
            ]
        );
        assert!(storage.take_staged().is_empty());

        // Taken contents have left memory; the names are still known.
        assert_eq!(storage.list("flights").unwrap(), ["flights/new.igc"]);
        assert_eq!(
            storage.read("index.json").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(storage.read("flights/old.igc").unwrap(), None);
    }
}
