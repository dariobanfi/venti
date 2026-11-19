//! The client core for the browser.
//!
//! `wasm-bindgen` turns this crate into a WebAssembly module and a file of
//! TypeScript declarations. The web app in `apps/web` calls it and nothing
//! below it, as the iPhone app calls the same core through UniFFI.
//!
//! [`Core`] has the interface the phones have. The view types it returns
//! are the core's own; their TypeScript declarations are generated from the
//! Rust definitions, so there is nothing to keep in step by hand. Live wind
//! is a handful of plain functions beside it, as it is in the core.
//!
//! The core never talks to a server. For the pilot's account it hands out a
//! [`ServerRequest`] and reads the answer; the page sends the request in
//! between and keeps the session token. Keeping the logbook in step with
//! the server is a run of such requests, [`FlightSync`].
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
//! 3. A logbook can open without flight files its index does not list. They
//!    are in `takeWanted()` after a successful open; the page fetches them
//!    a few at a time and calls `refresh()` after each lot.
//! 4. After a call that changes something, the page takes the changes with
//!    `takeStaged()` and writes them to its store in one transaction. If
//!    that fails, it throws both objects away and starts again from step 1,
//!    because they no longer match the store.
//!
//! The page's side of this is `apps/web/src/lib/core`.

use std::sync::Arc;

use serde::Serialize;
use tsify::{Ts, Tsify};
use venti_core::{
    AccountView, AirspaceFile, AirspaceImport, AirspaceShape, Bounds, CoreError, FlightAnalysis,
    FlightFile, FlightSharing, FlightView, FlightVisibility, ImportedFlight, LogbookStatistics,
    LogbookView, ServerRequest, ServerStatus, SignedIn, StagedStorage, StationHistory, Storage,
    SyncStep, SyncSummary, WindMap,
};
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

    /// Statistics over the whole logbook.
    pub fn statistics(&self) -> Result<Ts<LogbookStatistics>, JsValue> {
        to_js(&self.session.core.statistics())
    }

    /// One flight from the logbook, prepared for display.
    pub fn flight(&self, id: String) -> Result<Ts<FlightView>, JsValue> {
        let flight = self.session.call(|core| core.flight(id)).map_err(thrown)?;
        to_js(&flight)
    }

    /// Analyses one flight from the logbook: take-off and landing, every
    /// thermal and glide, and how it scores as a cross-country flight.
    #[wasm_bindgen(js_name = flightAnalysis)]
    pub fn flight_analysis(&self, id: String) -> Result<Ts<FlightAnalysis>, JsValue> {
        let analysis = self
            .session
            .call(|core| core.flight_analysis(id))
            .map_err(thrown)?;
        to_js(&analysis)
    }

    /// The stored file of one flight, with a name to save it under.
    #[wasm_bindgen(js_name = flightFile)]
    pub fn flight_file(&self, id: String) -> Result<Ts<FlightFile>, JsValue> {
        let file = self
            .session
            .call(|core| core.flight_file(id))
            .map_err(thrown)?;
        to_js(&file)
    }

    /// Removes a flight from the logbook for good.
    #[wasm_bindgen(js_name = deleteFlight)]
    pub fn delete_flight(&self, id: String) -> Result<(), JsValue> {
        self.session
            .call(|core| core.delete_flight(id))
            .map_err(thrown)
    }

    /// Takes into the logbook the stored flight files that its index did
    /// not list and that the page has fetched since. The phones have no
    /// such call: their disk hands over any file at once.
    pub fn refresh(&self) -> Result<(), JsValue> {
        self.session.refresh().map_err(thrown)
    }

    // The pilot's account on the Venti server. The core says what to send
    // and reads the answer; the page sends it and keeps the session token.

    /// The account this browser is signed in to, if any.
    pub fn account(&self) -> Result<Option<Ts<AccountView>>, JsValue> {
        let account = self.session.call(|core| core.account()).map_err(thrown)?;
        account.as_ref().map(to_js).transpose()
    }

    /// The request that signs this browser in. `device` is what to call
    /// it in the account's list of devices.
    #[wasm_bindgen(js_name = signInRequest)]
    pub fn sign_in_request(
        &self,
        handle: String,
        password: String,
        device: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .sign_in_request(handle, password, device)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that creates an account and signs this browser in.
    /// What the server would refuse anyway is refused here, in words.
    #[wasm_bindgen(js_name = createAccountRequest)]
    pub fn create_account_request(
        &self,
        handle: String,
        display_name: String,
        password: String,
        invite: Option<String>,
        device: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .create_account_request(handle, display_name, password, invite, device)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to signing in or creating an account,
    /// and remembers the account. `server` is the address the request was
    /// sent to. The token in the answer is the page's to keep.
    #[wasm_bindgen(js_name = signedIn)]
    pub fn signed_in(
        &self,
        server: String,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Ts<SignedIn>, JsValue> {
        let signed_in = self
            .session
            .call(|core| core.signed_in(server, status, body))
            .map_err(thrown)?;
        to_js(&signed_in)
    }

    /// The request that signs this browser out on the server, if it is
    /// signed in. Whatever the server answers, call `signedOut`.
    #[wasm_bindgen(js_name = signOutRequest)]
    pub fn sign_out_request(&self) -> Result<Option<Ts<ServerRequest>>, JsValue> {
        let request = self
            .session
            .call(|core| core.sign_out_request())
            .map_err(thrown)?;
        request.as_ref().map(to_js).transpose()
    }

    /// Forgets the account in this browser. The flights in the logbook
    /// stay, and so do their copies on the server.
    #[wasm_bindgen(js_name = signedOut)]
    pub fn signed_out(&self) -> Result<(), JsValue> {
        self.session.call(|core| core.signed_out()).map_err(thrown)
    }

    /// The request that deletes the account on the server, with all its
    /// flights there. It needs the password, not only the session.
    #[wasm_bindgen(js_name = deleteAccountRequest)]
    pub fn delete_account_request(&self, password: String) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .delete_account_request(password)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to deleting the account. On success the
    /// account is forgotten here too; the logbook in this browser stays.
    #[wasm_bindgen(js_name = accountDeleted)]
    pub fn account_deleted(&self, status: u16, body: Vec<u8>) -> Result<(), JsValue> {
        self.session
            .call(|core| core.account_deleted(status, body))
            .map_err(thrown)
    }

    /// How a flight of the logbook is shared, if the server has it.
    #[wasm_bindgen(js_name = flightSharing)]
    pub fn flight_sharing(&self, id: String) -> Result<Option<Ts<FlightSharing>>, JsValue> {
        let sharing = self
            .session
            .call(|core| core.flight_sharing(id))
            .map_err(thrown)?;
        sharing.as_ref().map(to_js).transpose()
    }

    /// The request that changes who may see a flight.
    #[wasm_bindgen(js_name = shareRequest)]
    pub fn share_request(
        &self,
        id: String,
        visibility: Ts<FlightVisibility>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let visibility = from_js(&visibility)?;
        let request = self
            .session
            .call(|core| core.share_request(id, visibility))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to `shareRequest` and remembers how the
    /// flight is now shared.
    pub fn shared(
        &self,
        id: String,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Ts<FlightSharing>, JsValue> {
        let sharing = self
            .session
            .call(|core| core.shared(id, status, body))
            .map_err(thrown)?;
        to_js(&sharing)
    }

    /// Starts bringing the logbook in step with the server.
    pub fn sync(&self) -> FlightSync {
        FlightSync {
            sync: Arc::clone(&self.session.core).sync(),
            staged: Arc::clone(&self.session.staged),
        }
    }

    // Airspace, from the OpenAir files the pilot brings in.

    /// Takes in an airspace file in OpenAir format. `file_name` is the
    /// name of the file the pilot chose; `now_unix_s` the current time. A
    /// file of the same name replaces the one brought in before.
    #[wasm_bindgen(js_name = importAirspace)]
    pub fn import_airspace(
        &self,
        file_name: String,
        bytes: Vec<u8>,
        now_unix_s: f64,
    ) -> Result<Ts<AirspaceImport>, JsValue> {
        let imported = self
            .session
            .call(|core| core.import_airspace(file_name, bytes, whole_seconds(now_unix_s)))
            .map_err(thrown)?;
        to_js(&imported)
    }

    /// The airspace files the pilot has brought in, by name.
    #[wasm_bindgen(js_name = airspaceFiles)]
    pub fn airspace_files(&self) -> Result<Vec<Ts<AirspaceFile>>, JsValue> {
        let files = self
            .session
            .call(|core| core.airspace_files())
            .map_err(thrown)?;
        files.iter().map(to_js).collect()
    }

    /// Removes an airspace file. Its airspace is no longer drawn.
    #[wasm_bindgen(js_name = deleteAirspaceFile)]
    pub fn delete_airspace_file(&self, stored_name: String) -> Result<(), JsValue> {
        self.session
            .call(|core| core.delete_airspace_file(stored_name))
            .map_err(thrown)
    }

    /// The airspaces of the pilot's files that touch a box of the map. Ask
    /// when the map has moved, not for every frame.
    #[wasm_bindgen(js_name = airspaceIn)]
    pub fn airspace_in(
        &self,
        south: f64,
        west: f64,
        north: f64,
        east: f64,
    ) -> Result<Vec<Ts<AirspaceShape>>, JsValue> {
        let area = Bounds {
            south,
            west,
            north,
            east,
        };
        // The core reads every file once and keeps the airspace; later
        // calls only look it up.
        let shapes = self
            .session
            .call(|core| Ok(core.airspace()?.in_area(area)))
            .map_err(thrown)?;
        shapes.iter().map(to_js).collect()
    }
}

/// One run of bringing the logbook in step with the server.
///
/// The page sends each request the run hands out and passes the answer
/// back, until the run says that it has finished or why it stopped. Like
/// any other call, a step throws when the core needs a stored file first:
/// the page fetches what `StoredFiles.takeWanted` names and makes the same
/// call again. After every step it stores what `StoredFiles.takeStaged`
/// holds, so a run that is cut off loses nothing.
#[wasm_bindgen]
#[derive(Debug)]
pub struct FlightSync {
    sync: Arc<venti_core::FlightSync>,
    staged: Arc<StagedStorage>,
}

#[wasm_bindgen]
impl FlightSync {
    /// Begins. `now_unix_s` is the current time, kept as the time of the
    /// sync.
    pub fn start(&self, now_unix_s: f64) -> Result<Ts<SyncStep>, JsValue> {
        let step = self.sync.start_when_fetched(whole_seconds(now_unix_s));
        to_js(&settle(&self.staged, step).map_err(thrown)?)
    }

    /// Takes the server's answer to the last request. `retry_after_s` is
    /// the answer's `Retry-After` header in seconds, if it had one.
    pub fn receive(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<Ts<SyncStep>, JsValue> {
        let step = self.sync.receive_when_fetched(status, body, retry_after_s);
        to_js(&settle(&self.staged, step).map_err(thrown)?)
    }
}

/// What a sync did, in a line for the pilot: "3 sent, 1 fetched".
#[wasm_bindgen(js_name = syncSummaryInWords)]
pub fn sync_summary_in_words(summary: Ts<SyncSummary>) -> Result<String, JsValue> {
    Ok(venti_core::sync_summary_in_words(from_js(&summary)?))
}

// Live wind from the Venti server. The core says what to ask the server for
// and reads its answers; the page sends the request in between. None of it
// touches the logbook, so these are plain functions and not calls on `Core`.

/// The path to `GET` to learn how the server is doing.
#[wasm_bindgen(js_name = serverStatusPath)]
pub fn server_status_path() -> String {
    venti_core::server_status_path()
}

/// Reads the server's answer to `serverStatusPath`.
#[wasm_bindgen(js_name = serverStatus)]
pub fn server_status(status: u16, body: Vec<u8>) -> Result<Ts<ServerStatus>, JsValue> {
    to_js(&venti_core::server_status(status, body).map_err(thrown)?)
}

/// The path and query to `GET` for the wind map of a box.
#[wasm_bindgen(js_name = windMapPath)]
pub fn wind_map_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, JsValue> {
    venti_core::wind_map_path(south, west, north, east).map_err(thrown)
}

/// Reads the server's answer to `windMapPath`. `now_unix_s` is the current
/// time by the server's clock, in whole seconds.
#[wasm_bindgen(js_name = windMap)]
pub fn wind_map(status: u16, body: Vec<u8>, now_unix_s: f64) -> Result<Ts<WindMap>, JsValue> {
    to_js(&venti_core::wind_map(status, body, whole_seconds(now_unix_s)).map_err(thrown)?)
}

/// The path and query to `GET` for a station's readings of the last 24
/// hours.
#[wasm_bindgen(js_name = stationHistoryPath)]
pub fn station_history_path(station_id: String) -> Result<String, JsValue> {
    venti_core::station_history_path(station_id).map_err(thrown)
}

/// Reads the server's answer to `stationHistoryPath`.
#[wasm_bindgen(js_name = stationHistory)]
pub fn station_history(
    status: u16,
    body: Vec<u8>,
    now_unix_s: f64,
) -> Result<Ts<StationHistory>, JsValue> {
    to_js(&venti_core::station_history(status, body, whole_seconds(now_unix_s)).map_err(thrown)?)
}

/// A moment as the core counts it. JavaScript has one kind of number, and a
/// 64-bit integer would reach the page as a `BigInt` that nothing else in
/// the app uses.
fn whole_seconds(unix_s: f64) -> i64 {
    // The cast stops at the ends of the range and turns "not a number" into
    // zero; it cannot go wrong.
    unix_s.round() as i64
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
    /// The Venti server gave no usable answer.
    Server,
    /// A file other than a flight is not one the app can read.
    InvalidFile,
}

impl CoreErrorKind {
    fn of(error: &CoreError) -> Self {
        match error {
            CoreError::InvalidFlight { .. } => Self::InvalidFlight,
            CoreError::UnknownFlight { .. } => Self::UnknownFlight,
            CoreError::Storage { .. } => Self::Storage,
            CoreError::Server { .. } => Self::Server,
            CoreError::InvalidFile { .. } => Self::InvalidFile,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::InvalidFlight => "InvalidFlight",
            Self::UnknownFlight => "UnknownFlight",
            Self::Storage => "Storage",
            Self::Server => "Server",
            Self::InvalidFile => "InvalidFile",
        }
    }
}

/// One of the core's view types as the plain JavaScript object its
/// TypeScript declaration describes.
fn to_js<T: Tsify + Serialize>(view: &T) -> Result<Ts<T>, JsValue> {
    Ts::from_rust(view).map_err(|error| js_sys::Error::new(&error.to_string()).into())
}

/// A value the page handed over as one of the core's own types.
fn from_js<T>(value: &Ts<T>) -> Result<T, JsValue>
where
    T: Tsify + serde::de::DeserializeOwned,
    T::JsType: Clone,
{
    value
        .to_rust()
        .map_err(|error| js_sys::Error::new(&error.to_string()).into())
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

    /// Makes a call to the core, and afterwards drops from memory the files
    /// the page fetched for it. They stay only when the call failed for
    /// want of another file, because the page is about to fetch that one
    /// and make the call again.
    fn call<T>(
        &self,
        call: impl FnOnce(&venti_core::Core) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        settle(&self.staged, call(&self.core))
    }

    /// Takes in the flight files fetched since the last time and drops them
    /// from memory. Files still not fetched are wanted again, which is how
    /// the page learns what is left.
    fn refresh(&self) -> Result<(), CoreError> {
        let result = self.core.refresh_logbook();
        self.staged.release();
        result
    }
}

/// After a call to the core: drops from memory the files the page fetched
/// for it. They stay only when the call failed for want of another file,
/// because the page is about to fetch that one and make the call again.
fn settle<T>(staged: &StagedStorage, result: Result<T, CoreError>) -> Result<T, CoreError> {
    if result.is_ok() || !staged.is_waiting() {
        staged.release();
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use venti_flight::igc;
    use venti_flight::testing::synthetic_flight;

    use super::*;

    type Store = BTreeMap<String, Vec<u8>>;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// Applies staged changes the way the page does.
    fn commit(staged: &StagedStorage, store: &mut Store) {
        for file in staged.take_staged() {
            match file.contents {
                Some(contents) => store.insert(file.name, contents),
                None => store.remove(&file.name),
            };
        }
    }

    /// Fetches at most `limit` of the files the core wanted, the way the
    /// page does. Returns how many were wanted.
    fn fetch(staged: &StagedStorage, store: &Store, limit: usize) -> usize {
        let wanted = staged.take_wanted();
        for name in wanted.iter().take(limit) {
            staged.provide(name, store.get(name).cloned());
        }
        wanted.len()
    }

    /// Opens a session the way the page does: fetch what opening asks for,
    /// then take in unlisted flight files one at a time.
    fn open(store: &Store) -> Session {
        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        let session = loop {
            match Session::open(Arc::clone(&staged)) {
                Ok(session) => break session,
                Err(error) => assert!(fetch(&staged, store, usize::MAX) > 0, "{error}"),
            }
        };
        while fetch(&staged, store, 1) > 0 {
            session.refresh().unwrap();
        }
        session
    }

    /// A store that holds the sample flight on `days` different days.
    fn store_with_flights(days: usize) -> Store {
        let mut store = Store::new();
        let session = open(&store);
        for day in 1..=days {
            let flight = sample().replace("040726", &format!("{day:02}0726"));
            session.call(|core| core.import_flight(flight)).unwrap();
        }
        commit(&session.staged, &mut store);
        store
    }

    #[test]
    fn a_page_keeps_a_logbook_across_reloads_by_doing_what_the_core_asks() {
        let mut store = Store::new();

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
        assert_eq!(fetch(&session.staged, &store, usize::MAX), 1);
        let flight = session
            .call(|core| core.flight(imported.id.clone()))
            .unwrap();
        assert_eq!(flight.duration_s, 1744);

        // Once the call has succeeded the file is out of memory again.
        session
            .call(|core| core.flight(imported.id.clone()))
            .unwrap_err();
        assert_eq!(fetch(&session.staged, &store, usize::MAX), 1);

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
        let mut store = Store::new();
        let session = open(&store);
        // A logbook opened for the first time writes its empty index.
        commit(&session.staged, &mut store);
        assert_eq!(store.keys().collect::<Vec<_>>(), ["logbook-index.json"]);

        let error = session
            .call(|core| core.import_flight("hello".to_owned()))
            .unwrap_err();

        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::InvalidFlight);
        assert_eq!(fetch(&session.staged, &store, usize::MAX), 0);
        assert!(session.staged.take_staged().is_empty());
    }

    #[test]
    fn a_lost_index_is_rebuilt_one_fetched_flight_at_a_time() {
        let mut store = store_with_flights(3);
        store.remove("logbook-index.json");

        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        let session = Session::open(Arc::clone(&staged)).unwrap();
        assert!(session.core.logbook().years.is_empty());

        let mut rounds = 0;
        while fetch(&staged, &store, 1) > 0 {
            session.refresh().unwrap();
            rounds += 1;
            assert_eq!(session.core.logbook().totals.flights, rounds);
        }
        assert_eq!(rounds, 3);

        // What is left to store is the rebuilt index and nothing else.
        commit(&staged, &mut store);
        assert_eq!(store.len(), 4, "{:?}", store.keys());
        let reopened = open(&store);
        assert_eq!(reopened.core.logbook().totals.flights, 3);
        assert!(reopened.staged.take_staged().is_empty());
    }

    #[test]
    fn a_stored_file_that_is_not_a_flight_is_asked_for_and_then_left_out() {
        let mut store = store_with_flights(1);
        store.remove("logbook-index.json");
        let junk = format!("flights/{}.igc", "0".repeat(32));
        store.insert(junk.clone(), b"junk".to_vec());

        let staged = Arc::new(StagedStorage::new(store.keys().cloned()));
        let session = Session::open(Arc::clone(&staged)).unwrap();
        assert_eq!(fetch(&staged, &store, usize::MAX), 2);
        session.refresh().unwrap();

        assert_eq!(session.core.logbook().totals.flights, 1);
        assert!(staged.take_wanted().is_empty());
        // The file that does not read would be wanted again by every
        // further refresh. So the page fetches each file once and no more.
        session.refresh().unwrap();
        assert_eq!(staged.take_wanted(), [junk]);
    }

    #[test]
    fn a_flight_another_tab_deleted_is_reported_as_gone() {
        let mut store = store_with_flights(1);
        let session = open(&store);
        let id = session.core.logbook().years[0].flights[0].id.clone();

        // Another tab removes the flight's file from the shared store.
        store.retain(|name, _| !name.starts_with("flights/"));

        session.call(|core| core.flight(id.clone())).unwrap_err();
        assert_eq!(fetch(&session.staged, &store, usize::MAX), 1);
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
            CoreErrorKind::Server,
            CoreErrorKind::InvalidFile,
        ] {
            assert_eq!(kind.name(), format!("{kind:?}"));
        }
    }

    #[test]
    fn a_moment_given_as_a_javascript_number_is_whole_seconds() {
        assert_eq!(whole_seconds(1_795_028_700.4), 1_795_028_700);
        assert_eq!(whole_seconds(1_795_028_700.5), 1_795_028_701);
        assert_eq!(whole_seconds(f64::NAN), 0);
        assert_eq!(whole_seconds(f64::INFINITY), i64::MAX);
    }
}
