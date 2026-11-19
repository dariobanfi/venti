//! The client core for the browser.
//!
//! `wasm-bindgen` turns this crate into a WebAssembly module and a file of
//! TypeScript declarations. The web app in `apps/web` calls it and nothing
//! below it, as the iPhone app calls the same core through UniFFI.
//!
//! [`Core`] has the interface the phones have. The view types it returns
//! are the core's own; their TypeScript declarations are generated from the
//! Rust definitions, so there is nothing to keep in step by hand.
//!
//! What differs in a browser is storage. A page cannot read or write
//! anything without waiting, and the core's interface does not wait. So the
//! core works on [`StoredFiles`], which knows what the browser holds and
//! leaves the reading and writing to the page:
//!
//! 1. The page lists the names in its store and creates `StoredFiles` and
//!    then `Core` from them.
//! 2. A call that needs a file that is not in memory throws. The page asks
//!    `takeWanted()` which files those were, fetches them, hands them over
//!    with `provide()` and makes the same call again.
//! 3. After a call that changes something, the page takes the changes with
//!    `takeStaged()` and writes them to its store in one transaction. If
//!    that fails, it throws both objects away and starts again from step 1,
//!    because they no longer match the store.
//!
//! The page's side of this is `apps/web/src/lib/core`.

use std::sync::Arc;

use serde::Serialize;
use tsify::{Ts, Tsify};
use venti_core::{CoreError, FlightView, ImportedFlight, LogbookView, StagedStorage, Storage};
use wasm_bindgen::prelude::*;

/// What the browser has stored for the core, and what is to change in it.
#[wasm_bindgen]
#[derive(Debug)]
pub struct StoredFiles {
    staged: Arc<StagedStorage>,
}

#[wasm_bindgen]
impl StoredFiles {
    /// `names` are the names of every file in the page's store.
    #[wasm_bindgen(constructor)]
    pub fn new(names: Vec<String>) -> Self {
        Self {
            staged: Arc::new(StagedStorage::new(names)),
        }
    }

    /// The files the core asked for and could not read, in the order it
    /// asked. Asking empties the list.
    #[wasm_bindgen(js_name = takeWanted)]
    pub fn take_wanted(&self) -> Vec<String> {
        self.staged.take_wanted()
    }

    /// Hands over the contents of a stored file. Leaving `contents` out
    /// says that the store has no such file after all.
    pub fn provide(&self, name: &str, contents: Option<Vec<u8>>) {
        self.staged.provide(name, contents);
    }

    /// The changes to make to the page's store, all together or not at
    /// all: contents to store under a name, or no contents to remove the
    /// file of that name. Taking them empties the list.
    #[wasm_bindgen(
        js_name = takeStaged,
        unchecked_return_type = "[name: string, contents: Uint8Array | undefined][]"
    )]
    pub fn take_staged(&self) -> js_sys::Array {
        self.staged
            .take_staged()
            .into_iter()
            .map(|file| {
                let contents = file.contents.map_or(JsValue::UNDEFINED, |bytes| {
                    js_sys::Uint8Array::from(bytes.as_slice()).into()
                });
                js_sys::Array::of2(&file.name.into(), &contents)
            })
            .collect()
    }
}

/// The application core. A page creates one and keeps it until its store
/// changes behind it.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Core {
    session: Session,
}

#[wasm_bindgen]
impl Core {
    /// Creates the core over what the browser has stored.
    #[wasm_bindgen(constructor)]
    pub fn new(files: &StoredFiles) -> Result<Core, JsValue> {
        let session = Session::open(Arc::clone(&files.staged)).map_err(thrown)?;
        Ok(Self { session })
    }

    /// The version of the core, for the about screen and bug reports.
    pub fn version(&self) -> String {
        self.session.core.version()
    }

    /// Adds a flight file to the logbook. Importing a flight that is
    /// already there changes nothing and says so.
    #[wasm_bindgen(js_name = importFlight)]
    pub fn import_flight(&self, igc_text: String) -> Result<Ts<ImportedFlight>, JsValue> {
        let imported = self
            .session
            .call(|core| core.import_flight(igc_text))
            .map_err(thrown)?;
        to_js(&imported)
    }

    /// The logbook: every flight grouped by year, with totals.
    pub fn logbook(&self) -> Result<Ts<LogbookView>, JsValue> {
        to_js(&self.session.core.logbook())
    }

    /// One flight from the logbook, prepared for display.
    pub fn flight(&self, id: String) -> Result<Ts<FlightView>, JsValue> {
        let flight = self.session.call(|core| core.flight(id)).map_err(thrown)?;
        to_js(&flight)
    }

    /// Removes a flight from the logbook for good.
    #[wasm_bindgen(js_name = deleteFlight)]
    pub fn delete_flight(&self, id: String) -> Result<(), JsValue> {
        self.session
            .call(|core| core.delete_flight(id))
            .map_err(thrown)
    }
}

/// The `name` of the `Error` a failed call throws. Its `message` is fit to
/// show to the pilot as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Tsify)]
pub enum CoreErrorKind {
    /// The text is not a usable flight file.
    InvalidFlight,
    /// The logbook holds no flight with the identifier that was asked for.
    UnknownFlight,
    /// The store could not be read or written, or a file has to be fetched
    /// first: see `StoredFiles.takeWanted`.
    Storage,
}

impl CoreErrorKind {
    fn of(error: &CoreError) -> Self {
        match error {
            CoreError::InvalidFlight { .. } => Self::InvalidFlight,
            CoreError::UnknownFlight { .. } => Self::UnknownFlight,
            CoreError::Storage { .. } => Self::Storage,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::InvalidFlight => "InvalidFlight",
            Self::UnknownFlight => "UnknownFlight",
            Self::Storage => "Storage",
        }
    }
}

/// One of the core's view types as the plain JavaScript object its
/// TypeScript declaration describes.
fn to_js<T: Tsify + Serialize>(view: &T) -> Result<Ts<T>, JsValue> {
    Ts::from_rust(view).map_err(|error| js_sys::Error::new(&error.to_string()).into())
}

/// The core's error as the JavaScript `Error` a call throws.
fn thrown(error: CoreError) -> JsValue {
    let thrown = js_sys::Error::new(&error.to_string());
    thrown.set_name(CoreErrorKind::of(&error).name());
    thrown.into()
}

/// The core and the storage it was created over. Everything above this is
/// conversion to and from JavaScript, which only runs in a browser; this
/// part is tested on the build machine.
#[derive(Debug)]
struct Session {
    core: Arc<venti_core::Core>,
    staged: Arc<StagedStorage>,
}

impl Session {
    fn open(staged: Arc<StagedStorage>) -> Result<Self, CoreError> {
        let core = venti_core::Core::with_storage(Arc::clone(&staged) as Arc<dyn Storage>)?;
        // Opening read the index; nothing needs it in memory any more.
        staged.release();
        Ok(Self { core, staged })
    }

    /// Makes a call to the core. Once it succeeds, the files the page
    /// fetched for it are dropped from memory. After a failure they stay,
    /// because the page is about to fetch the rest and call again.
    fn call<T>(
        &self,
        call: impl FnOnce(&venti_core::Core) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let result = call(&self.core)?;
        self.staged.release();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// Applies staged changes the way the page does.
    fn commit(staged: &StagedStorage, store: &mut BTreeMap<String, Vec<u8>>) {
        for file in staged.take_staged() {
            match file.contents {
                Some(contents) => store.insert(file.name, contents),
                None => store.remove(&file.name),
            };
        }
    }

    /// Fetches what the core wanted the way the page does. Returns whether
    /// anything was wanted.
    fn fetch(staged: &StagedStorage, store: &BTreeMap<String, Vec<u8>>) -> bool {
        let wanted = staged.take_wanted();
        for name in &wanted {
            staged.provide(name, store.get(name).cloned());
        }
        !wanted.is_empty()
    }

    fn open(store: &BTreeMap<String, Vec<u8>>) -> Session {
        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        loop {
            match Session::open(Arc::clone(&staged)) {
                Ok(session) => return session,
                Err(error) => assert!(fetch(&staged, store), "{error}"),
            }
        }
    }

    #[test]
    fn a_page_keeps_a_logbook_across_reloads_by_doing_what_the_core_asks() {
        let mut store = BTreeMap::new();

        let session = open(&store);
        let imported = session.call(|core| core.import_flight(sample())).unwrap();
        commit(&session.staged, &mut store);
        assert_eq!(store.len(), 2, "{:?}", store.keys());

        // A reload: nothing is left in memory, only the store.
        let session = open(&store);
        assert_eq!(session.core.logbook().totals.flights, 1);
        // Opening changed nothing, so there is nothing to write.
        assert!(session.staged.take_staged().is_empty());

        // The flight's file is fetched when the flight is opened.
        let error = session
            .call(|core| core.flight(imported.id.clone()))
            .unwrap_err();
        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::Storage);
        assert!(fetch(&session.staged, &store));
        let flight = session
            .call(|core| core.flight(imported.id.clone()))
            .unwrap();
        assert_eq!(flight.duration_s, 1744);

        // Once the call has succeeded the file is out of memory again.
        session
            .call(|core| core.flight(imported.id.clone()))
            .unwrap_err();
        assert!(fetch(&session.staged, &store));

        session
            .call(|core| core.delete_flight(imported.id.clone()))
            .unwrap();
        commit(&session.staged, &mut store);
        assert_eq!(
            store.keys().collect::<Vec<_>>(),
            ["logbook-index.json"],
            "the flight's file is removed from the store"
        );
        assert!(open(&store).core.logbook().years.is_empty());
    }

    #[test]
    fn a_failed_call_that_wanted_no_file_is_a_real_failure() {
        let mut store = BTreeMap::new();
        let session = open(&store);
        // A logbook opened for the first time writes its empty index.
        commit(&session.staged, &mut store);
        assert_eq!(store.keys().collect::<Vec<_>>(), ["logbook-index.json"]);

        let error = session
            .call(|core| core.import_flight("hello".to_owned()))
            .unwrap_err();

        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::InvalidFlight);
        assert!(!fetch(&session.staged, &store));
        assert!(session.staged.take_staged().is_empty());
    }

    #[test]
    fn a_flight_another_tab_deleted_is_reported_as_gone() {
        let mut store = BTreeMap::new();
        let session = open(&store);
        let id = session
            .call(|core| core.import_flight(sample()))
            .unwrap()
            .id;
        commit(&session.staged, &mut store);

        // Another tab removes the flight's file from the shared store.
        store.retain(|name, _| !name.starts_with("flights/"));

        session.call(|core| core.flight(id.clone())).unwrap_err();
        assert!(fetch(&session.staged, &store));
        let error = session.call(|core| core.flight(id)).unwrap_err();
        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::UnknownFlight);
    }

    #[test]
    fn every_kind_of_error_has_the_name_typescript_is_told_about() {
        // The TypeScript union is generated from the variant names; the
        // names thrown must be the same words.
        for kind in [
            CoreErrorKind::InvalidFlight,
            CoreErrorKind::UnknownFlight,
            CoreErrorKind::Storage,
        ] {
            assert_eq!(kind.name(), format!("{kind:?}"));
        }
    }
}
