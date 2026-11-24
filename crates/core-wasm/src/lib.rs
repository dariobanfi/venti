//! The client core for the browser.
//!
//! `wasm-bindgen` turns this crate into a WebAssembly module and a file of
//! TypeScript declarations. The web app in `apps/web` calls it and nothing
//! below it, as the iPhone app calls the same core through UniFFI.
//!
//! [`Core`] has the interface the phones have. The view types it returns
//! are the core's own; their TypeScript declarations are generated from the
//! Rust definitions, so there is nothing to keep in step by hand. Live wind,
//! flying sites, forecasts, who is in the air and other pilots' public
//! flights are a handful of plain functions beside it, as they are in the
//! core, [`LiveFollow`] is the core's reader of one live track, and
//! [`Terrain`] its reader of the map's elevation tiles.
//!
//! The core never talks to a server. For the pilot's account, and for the
//! pilot's groups and their plans, it hands out a [`ServerRequest`] and
//! reads the answer; the page sends the request in between and keeps the
//! session token. Keeping the logbook in step with the server is a run of
//! such requests, [`FlightSync`].
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
use venti_core::FoehnView;
use venti_core::{
    AccountView, AirspaceFetch, AirspaceFile, AirspaceHeightFilter, AirspaceImport,
    AirspaceOverPlace, AirspaceShape, Bounds, CableSet, CablesMap, Coordinate, CoreError, Credit,
    FlightAirspace, FlightAnalysis, FlightComparison, FlightFile, FlightSearch, FlightSearchBounds,
    FlightSharing, FlightView, FlightVisibility, ForecastView, GroundUnderFlight, GroupDetail,
    GroupJoined, GroupMade, ImportedFlight, KeptAirspace, KeptSites, LeagueHandover,
    LiveFollowView, LiveLayer, LiveMap, LiveOthers, LogbookStatistics, LogbookView, OpenPlanDraft,
    OpenPlanView, OpenPlans, PilotPage, PlanBoard, PlanDraft, PlanView, PublicFlight,
    PublicFlights, PublicFlightsOrder, RadarView, Ranking, RankingSeason, ReportForm, ReportReason,
    ReportSubject, RouteAlong, RoutePlace, RouteSetting, ServerRequest, ServerStatus, SignedIn,
    SiteAtPlace, SiteDraftFault, SiteDraftView, SiteForm, SiteHistory, SiteInTheWay, SiteOutlook,
    SitePage, SiteStatistics, SiteView, SiteWritten, SitesFilter, SitesFilterChoices,
    SitesFiltered, SitesMap, StagedStorage, StationAir, StationDay, StationDays, StationHistory,
    Storage, SyncStep, SyncSummary, Tendencies, TerrainTile, ThermalHoursLayer, ThermalMap,
    ThermalMapHourSquare, ThermalMapHoursView, ThermalMapShapes, ThermalStripDay, TodayOrder,
    TodayStanding, TodayTakeoffs, TrackPoint, WindMap, WindSpan, WindStation,
};
use venti_core::{AgreementOutcome, AgreementPage, AgreementWords};
use venti_core::{
    AlertDraft, AlertFaultView, AlertFormWords, AlertView, FiredLine, FiredPage, PilotsAlarmNumbers,
};
use venti_core::{AllFlightsSet, FlightVisibilityWords};
use venti_core::{CandidateOffer, SiteCandidates, SiteFlights, SiteFlightsWords};
use venti_core::{
    ChatMessage, ChatPage, CommunityDetail, CommunityDraft, CommunityInvite, CommunityStanding,
    CommunityView, CommunityWords, FoundCommunity, LinkDraft, MessageDraft, SentGroupPicture,
};
use venti_core::{
    CheerList, CheerState, CommentDraft, CommentList, CommentPosted, CommentsSwitch, Feed,
    FollowList, FollowState, Notifications, Unread,
};
use venti_core::{
    CockpitDetailsWords, CockpitSourceWords, CommonWords, GliderClassKind, GliderWords,
    InstrumentView, MapLayersWords, PagesWords, PlannerWords, SiteSheetWords, SocialWords,
    StationSheetWords, TargetPlace, TodayWords, VarioSourceKind,
};
use venti_core::{FlownThermals, LiveClimbs, SitesAir};
use venti_core::{GoogleSignIn, GoogleWords};
use venti_core::{HelpLists, HelpView, NearbyChoice, PhonePlatform};
use venti_core::{OwnFlight, OwnFlights, UploadedFlight};
use venti_core::{PilotStatistics, PilotsFound, PilotsWords, RouteFromFile};
use venti_core::{
    SiteAdding, SiteConfirmFlight, SiteConfirmForm, SiteConfirmations, SiteCorrecting,
    SiteDraftRebased, SiteHistoryOffers, SiteOffers, SiteRevisionView, SiteWords, SitesRemoved,
};
use venti_core::{TokenCreated, TokenForm, TokenScope, TokenView, TokenWithdrawal};
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

    /// The logbook by where its flights began: one entry for each place,
    /// the one flown from most often first.
    #[wasm_bindgen(js_name = siteStatistics)]
    pub fn site_statistics(&self) -> Result<Vec<Ts<SiteStatistics>>, JsValue> {
        self.session
            .core
            .site_statistics()
            .iter()
            .map(to_js)
            .collect()
    }

    /// What the pilot's own tracks say about how they fly, over the whole
    /// logbook. Flights imported before the core kept their figures are
    /// worked out from their files; one whose file is not in memory is left
    /// out and wanted, as after `refresh()`: the page fetches the wanted
    /// files, hands them over and asks again.
    pub fn tendencies(&self) -> Result<Ts<Tendencies>, JsValue> {
        let tendencies = self.session.core.tendencies();
        self.session.staged.release();
        to_js(&tendencies)
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

    /// Checks one flight of the logbook against the airspace the app has
    /// today: the pilot's files and what is kept from the server. Like any
    /// call it throws while a stored file is still to be fetched, the
    /// flight's or any of the airspace's: the answer is never worked out
    /// from the airspace that happens to be in memory.
    #[wasm_bindgen(js_name = flightAirspace)]
    pub fn flight_airspace(&self, id: String) -> Result<Ts<FlightAirspace>, JsValue> {
        let checked = self
            .session
            .call(|core| core.flight_airspace(id))
            .map_err(thrown)?;
        to_js(&checked)
    }

    /// Checks a flight that is not in the logbook against the airspace the
    /// app has, from its file's text: one of the pilot's flights fetched
    /// from the server. Like `flightAirspace` it throws while any of the
    /// airspace is still to be fetched.
    #[wasm_bindgen(js_name = flightAirspaceFromFile)]
    pub fn flight_airspace_from_file(
        &self,
        igc_text: String,
    ) -> Result<Ts<FlightAirspace>, JsValue> {
        let checked = self
            .session
            .call(|core| core.flight_airspace_from_file(igc_text))
            .map_err(thrown)?;
        to_js(&checked)
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

    /// What to tell the pilot before a flight of the logbook goes to a
    /// league, with its file: until when the leagues take it, whether the
    /// file is signed, and the addresses of XContest's claim page and the
    /// leagues' rules. `now_unix_s` is the moment the pilot asks at.
    #[wasm_bindgen(js_name = leagueHandover)]
    pub fn league_handover(
        &self,
        id: String,
        now_unix_s: f64,
    ) -> Result<Ts<LeagueHandover>, JsValue> {
        let handover = self
            .session
            .call(|core| core.league_handover(id, whole_seconds(now_unix_s)))
            .map_err(thrown)?;
        to_js(&handover)
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

    /// The request that signs this browser in with a Google ID token from
    /// Google Identity Services. Its answer goes to `googleSignIn`.
    #[wasm_bindgen(js_name = googleSignInRequest)]
    pub fn google_sign_in_request(
        &self,
        id_token: String,
        device: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .google_sign_in_request(id_token, device)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to `googleSignInRequest`: signed in, and
    /// the account remembered, or the handle step of a first sign-in.
    #[wasm_bindgen(js_name = googleSignIn)]
    pub fn google_sign_in(
        &self,
        server: String,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Ts<GoogleSignIn>, JsValue> {
        let answer = self
            .session
            .call(|core| core.google_sign_in(server, status, body))
            .map_err(thrown)?;
        to_js(&answer)
    }

    /// The request that creates an account after a first sign-in with
    /// Google. Its answer goes to `signedIn`.
    #[wasm_bindgen(js_name = googleAccountRequest)]
    pub fn google_account_request(
        &self,
        signup: String,
        handle: String,
        display_name: String,
        invite: Option<String>,
        device: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .google_account_request(signup, handle, display_name, invite, device)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that links the signed-in account to a Google account.
    /// Its answer goes to `googleLinked`.
    #[wasm_bindgen(js_name = linkGoogleRequest)]
    pub fn link_google_request(&self, id_token: String) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .link_google_request(id_token)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to `linkGoogleRequest`.
    #[wasm_bindgen(js_name = googleLinked)]
    pub fn google_linked(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Option<Ts<AccountView>>, JsValue> {
        let account = self
            .session
            .call(|core| core.google_linked(status, body))
            .map_err(thrown)?;
        account.as_ref().map(to_js).transpose()
    }

    /// The request that unlinks the account from its Google account. Its
    /// answer goes to `googleUnlinked`.
    #[wasm_bindgen(js_name = unlinkGoogleRequest)]
    pub fn unlink_google_request(&self) -> Result<Ts<ServerRequest>, JsValue> {
        to_js(&self.session.core.unlink_google_request())
    }

    /// Reads the server's answer to `unlinkGoogleRequest`.
    #[wasm_bindgen(js_name = googleUnlinked)]
    pub fn google_unlinked(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Option<Ts<AccountView>>, JsValue> {
        let account = self
            .session
            .call(|core| core.google_unlinked(status, body))
            .map_err(thrown)?;
        account.as_ref().map(to_js).transpose()
    }

    /// The request that deletes an account that signs in with Google: a
    /// fresh ID token in place of the password. Its answer goes to
    /// `accountDeleted`.
    #[wasm_bindgen(js_name = deleteAccountWithGoogleRequest)]
    pub fn delete_account_with_google_request(
        &self,
        id_token: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .core
            .delete_account_with_google_request(id_token)
            .map_err(thrown)?;
        to_js(&request)
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

    /// The request that reads the account from the server, if this
    /// browser is signed in: what the server says of it can change while
    /// the browser stays signed in, as whether it is a partner of Venti
    /// for flying sites. The answer goes to `accountRead`.
    #[wasm_bindgen(js_name = accountRequest)]
    pub fn account_request(&self) -> Result<Option<Ts<ServerRequest>>, JsValue> {
        let request = self
            .session
            .call(|core| core.account_request())
            .map_err(thrown)?;
        request.as_ref().map(to_js).transpose()
    }

    /// The request that sets the pilot's country, or clears it without
    /// one: two letters, such as `DE`. Its answer goes to `accountRead`.
    #[wasm_bindgen(js_name = countryRequest)]
    pub fn country_request(&self, country: Option<String>) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .call(|core| core.country_request(country))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that sets the class of the glider the pilot flies now,
    /// or clears it without one. Flights sent from then on take it. Its
    /// answer goes to `accountRead`.
    #[wasm_bindgen(js_name = gliderClassRequest)]
    pub fn glider_class_request(
        &self,
        class: Option<Ts<GliderClassKind>>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let class = class.as_ref().map(from_js).transpose()?;
        let request = self
            .session
            .call(|core| core.glider_class_request(class))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to `accountRequest` and remembers what
    /// it says of the account. The account as it is known now; nothing
    /// when the browser was signed out meanwhile. A `401` is no error:
    /// the account then says that the pilot has to sign in again.
    #[wasm_bindgen(js_name = accountRead)]
    pub fn account_read(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Option<Ts<AccountView>>, JsValue> {
        let account = self
            .session
            .call(|core| core.account_read(status, body))
            .map_err(thrown)?;
        account.as_ref().map(to_js).transpose()
    }

    // What a shell may offer for a flying site, and confirming one. The
    // rules are the core's (`venti_core::site_offers`), which reads the
    // account this browser stores.

    /// What the page may offer for a site to whoever is signed in: to
    /// correct it, remove it, confirm it, report it and read its history,
    /// each with its words, and whether this pilot has confirmed it.
    /// `takes_sites_from_pilots` is the server's status; `confirmations`
    /// is the site's list as `siteConfirmations` read it, when the page
    /// has it.
    #[wasm_bindgen(js_name = siteOffers)]
    pub fn site_offers(
        &self,
        site: Ts<SiteView>,
        takes_sites_from_pilots: bool,
        confirmations: Option<Ts<SiteConfirmations>>,
    ) -> Result<Ts<SiteOffers>, JsValue> {
        let site = from_js(&site)?;
        let confirmations = confirmations.as_ref().map(from_js).transpose()?;
        let offers = self
            .session
            .call(|core| core.site_offers(site, takes_sites_from_pilots, confirmations))
            .map_err(thrown)?;
        to_js(&offers)
    }

    /// Whether the page may offer to add a site, into which tier it
    /// would go, and what the entry and the form say for that tier. The
    /// entry is shown whenever its words are not empty, and opens the
    /// account where it leads to signing in.
    #[wasm_bindgen(js_name = siteAdding)]
    pub fn site_adding(&self, takes_sites_from_pilots: bool) -> Result<Ts<SiteAdding>, JsValue> {
        let adding = self
            .session
            .call(|core| core.site_adding(takes_sites_from_pilots))
            .map_err(thrown)?;
        to_js(&adding)
    }

    /// What the page may offer in a site's history to whoever is signed
    /// in: to put a revision back, to undo the last change, to bring a
    /// removed site back and to report a revision. `history` is a page
    /// of it as `siteHistory` read it.
    #[wasm_bindgen(js_name = siteHistoryOffers)]
    pub fn site_history_offers(
        &self,
        history: Ts<SiteHistory>,
        takes_sites_from_pilots: bool,
    ) -> Result<Ts<SiteHistoryOffers>, JsValue> {
        let history = from_js(&history)?;
        let offers = self
            .session
            .call(|core| core.site_history_offers(history, takes_sites_from_pilots))
            .map_err(thrown)?;
        to_js(&offers)
    }

    /// Reads the server's answer to `sitesRemovedPath`: the removed
    /// sites of a tier, each in a line with who removed it and when, and
    /// whether whoever is signed in may bring it back.
    #[wasm_bindgen(js_name = sitesRemoved)]
    pub fn sites_removed(
        &self,
        status: u16,
        body: Vec<u8>,
        takes_sites_from_pilots: bool,
    ) -> Result<Ts<SitesRemoved>, JsValue> {
        let removed = self
            .session
            .call(|core| core.sites_removed(status, body, takes_sites_from_pilots))
            .map_err(thrown)?;
        to_js(&removed)
    }

    /// The pilot's own flights to name with a confirmation of a site for
    /// a day (`YYYY-MM-DD`): those that began on it within 500 m of the
    /// site and are on the server. `utc_offset_s` is how far the
    /// browser's clock is ahead of UTC on that day, in seconds.
    #[wasm_bindgen(js_name = siteConfirmFlights)]
    pub fn site_confirm_flights(
        &self,
        site: Ts<SiteView>,
        day: String,
        utc_offset_s: i32,
    ) -> Result<Vec<Ts<SiteConfirmFlight>>, JsValue> {
        let site = from_js(&site)?;
        let flights = self
            .session
            .call(|core| core.site_confirm_flights(site, day, utc_offset_s))
            .map_err(thrown)?;
        flights.iter().map(to_js).collect()
    }

    /// The request that says of a site "I took off here and it was
    /// good". `day` and `today` are `YYYY-MM-DD`; `note` may be empty;
    /// `flight` is the identifier of one of the logbook's flights, from
    /// `siteConfirmFlights`.
    #[wasm_bindgen(js_name = siteConfirmRequest)]
    pub fn site_confirm_request(
        &self,
        id: String,
        day: String,
        today: String,
        note: String,
        flight: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.site_confirm_request(id, day, today, note, flight))
    }

    /// The candidates of an answer (`siteCandidates`) that the pilot
    /// signed in here took off at: each with the pilot's newest flight
    /// from there, the entry's words and a draft at the pilot's own
    /// take-off. Empty where the pilot may add no site and for a place set
    /// aside. `utc_offset_s` is how far the browser's clock is ahead of
    /// UTC, in seconds.
    #[wasm_bindgen(js_name = candidateForPilot)]
    pub fn candidate_for_pilot(
        &self,
        candidates: Ts<SiteCandidates>,
        takes_sites_from_pilots: bool,
        utc_offset_s: i32,
    ) -> Result<Vec<Ts<CandidateOffer>>, JsValue> {
        let candidates = from_js(&candidates)?;
        let offers = self
            .session
            .call(|core| {
                core.candidate_for_pilot(candidates, takes_sites_from_pilots, utc_offset_s)
            })
            .map_err(thrown)?;
        offers.iter().map(to_js).collect()
    }

    /// Says "Not this one" of a candidate in this browser: it and any
    /// candidate within 500 m of it are not offered again.
    #[wasm_bindgen(js_name = candidateSetAside)]
    pub fn candidate_set_aside(&self, position: Ts<Coordinate>) -> Result<(), JsValue> {
        let position = from_js(&position)?;
        self.session
            .call(|core| core.candidate_set_aside(position))
            .map_err(thrown)
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

    /// The request that sets whether a flight sent to the server from now
    /// on is public from the start. Its answer is the account, for
    /// `accountRead`.
    #[wasm_bindgen(js_name = newFlightsPublicRequest)]
    pub fn new_flights_public_request(&self, public: bool) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .call(|core| core.new_flights_public_request(public))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that sets whether the pilot is among the pilots near a
    /// place. Its answer is the account, for `accountRead`.
    #[wasm_bindgen(js_name = shownNearPlacesRequest)]
    pub fn shown_near_places_request(&self, shown: bool) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .call(|core| core.shown_near_places_request(shown))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that sends a picture as the pilot's own: its bytes as
    /// they are and its media type (`image/jpeg`, `image/png` or
    /// `image/webp`). Its answer is the account, for `accountRead`.
    #[wasm_bindgen(js_name = pictureRequest)]
    pub fn picture_request(
        &self,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .call(|core| core.picture_request(picture, media_type))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that removes the pilot's picture. Its answer is the
    /// account, for `accountRead`.
    #[wasm_bindgen(js_name = pictureRemovalRequest)]
    pub fn picture_removal_request(&self) -> Result<Ts<ServerRequest>, JsValue> {
        let request = self
            .session
            .call(venti_core::Core::picture_removal_request)
            .map_err(thrown)?;
        to_js(&request)
    }

    /// The request that makes every flight of the account public, or
    /// every one private. Ask `allFlightsQuestion` first; the answer goes
    /// to `allFlightsSet`.
    #[wasm_bindgen(js_name = allFlightsRequest)]
    pub fn all_flights_request(
        &self,
        visibility: Ts<FlightVisibility>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let visibility = from_js(&visibility)?;
        let request = self
            .session
            .call(|core| core.all_flights_request(visibility))
            .map_err(thrown)?;
        to_js(&request)
    }

    /// Reads the server's answer to `allFlightsRequest`: how the
    /// account's flights stand, with the line to show.
    #[wasm_bindgen(js_name = allFlightsSet)]
    pub fn all_flights_set(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Ts<AllFlightsSet>, JsValue> {
        let set = self
            .session
            .call(|core| core.all_flights_set(status, body))
            .map_err(thrown)?;
        to_js(&set)
    }

    /// Starts bringing the logbook in step with the server.
    pub fn sync(&self) -> FlightSync {
        FlightSync {
            sync: Arc::clone(&self.session.core).sync(),
            staged: Arc::clone(&self.session.staged),
        }
    }

    // Airspace: what the app keeps from the Venti server, and the OpenAir
    // files the pilot brings in. The core keeps both and gives a map one
    // set of them.

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

    /// Reads the server's answer to `serverAirspacePath` for the same
    /// place and keeps it in place of what was kept before. `server` is
    /// the address the request was sent to; `now_unix_s` the current time.
    /// An answer that says nothing about the place changes nothing.
    #[wasm_bindgen(js_name = keepServerAirspace)]
    pub fn keep_server_airspace(
        &self,
        server: String,
        lat: f64,
        lon: f64,
        status: u16,
        body: Vec<u8>,
        now_unix_s: f64,
    ) -> Result<Ts<AirspaceFetch>, JsValue> {
        let fetch = self
            .session
            .call(|core| {
                core.keep_server_airspace(server, lat, lon, status, body, whole_seconds(now_unix_s))
            })
            .map_err(thrown)?;
        to_js(&fetch)
    }

    /// The airspace kept from the server, one entry for each source, with
    /// the credit and the notice to show wherever the airspace is shown.
    #[wasm_bindgen(js_name = keptAirspace)]
    pub fn kept_airspace(&self) -> Result<Vec<Ts<KeptAirspace>>, JsValue> {
        let kept = self
            .session
            .call(|core| core.kept_airspace())
            .map_err(thrown)?;
        kept.iter().map(to_js).collect()
    }

    /// Whether to ask the server for airspace now: nothing is kept from
    /// this server, what is kept is more than a day old, or the place is
    /// outside the kept area or near its edge.
    #[wasm_bindgen(js_name = keptAirspaceIsDue)]
    pub fn kept_airspace_is_due(
        &self,
        server: String,
        lat: f64,
        lon: f64,
        now_unix_s: f64,
    ) -> Result<bool, JsValue> {
        self.session
            .call(|core| {
                // What is kept has to be looked at first. The core's own
                // question takes "not fetched from the store yet" for
                // "nothing kept" and would have the server asked every time.
                core.kept_airspace()?;
                Ok(core.kept_airspace_is_due(server, lat, lon, whole_seconds(now_unix_s)))
            })
            .map_err(thrown)
    }

    /// Removes the airspace kept from the server. It is no longer drawn.
    #[wasm_bindgen(js_name = deleteKeptAirspace)]
    pub fn delete_kept_airspace(&self) -> Result<(), JsValue> {
        self.session
            .call(|core| core.delete_kept_airspace())
            .map_err(thrown)
    }

    /// The airspaces that touch a box of the map, from what is kept from
    /// the server and from the pilot's files. Ask when the map has moved,
    /// not for every frame. `ceiling_m` is the height of the map's filter,
    /// one of `airspaceHeightFilter`'s steps: airspace that certainly
    /// begins above it is left out. Without it, everything.
    #[wasm_bindgen(js_name = airspaceIn)]
    pub fn airspace_in(
        &self,
        south: f64,
        west: f64,
        north: f64,
        east: f64,
        ceiling_m: Option<f64>,
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
            .call(|core| Ok(core.airspace()?.in_area_below(area, ceiling_m)))
            .map_err(thrown)?;
        shapes.iter().map(to_js).collect()
    }

    /// What lies over a place: every airspace whose outline contains it,
    /// from the lowest floor up, with what to say with the list. For the
    /// sheet a click on the map opens.
    #[wasm_bindgen(js_name = airspaceOver)]
    pub fn airspace_over(&self, lat: f64, lon: f64) -> Result<Ts<AirspaceOverPlace>, JsValue> {
        let over = self
            .session
            .call(|core| Ok(core.airspace()?.over_place(Coordinate { lat, lon })))
            .map_err(thrown)?;
        to_js(&over)
    }

    // The pilot's own flying sites, kept in the browser, and the take-offs
    // of Today, which begin with them. Signed out, nothing of it goes to
    // the server; signed in, `keptSitesSync` keeps it in step with the
    // account, where it is private.

    /// Starts bringing the pilot's own sites in step with their account:
    /// after signing in, after the pilot kept or forgot a site, and
    /// whenever the logbook is brought in step.
    #[wasm_bindgen(js_name = keptSitesSync)]
    pub fn kept_sites_sync(&self) -> KeptSitesSync {
        KeptSitesSync {
            sync: Arc::clone(&self.session.core).kept_sites_sync(),
            staged: Arc::clone(&self.session.staged),
        }
    }

    /// Starts fetching the account's alarms and what they fired since the
    /// last run: after signing in, after changing an alarm, and from time
    /// to time while the page is open.
    #[wasm_bindgen(js_name = alertsSync)]
    pub fn alerts_sync(&self) -> AlertsSync {
        AlertsSync {
            sync: Arc::clone(&self.session.core).alerts_sync(),
            staged: Arc::clone(&self.session.staged),
        }
    }

    /// The account's alarms as the browser last fetched them.
    #[wasm_bindgen(js_name = alertsHeld)]
    pub fn alerts_held(&self) -> Result<Vec<Ts<AlertView>>, JsValue> {
        self.session
            .call(venti_core::Core::alerts_held)
            .map_err(thrown)?
            .iter()
            .map(to_js)
            .collect()
    }

    /// What the alarms fired as the browser last fetched it, the newest
    /// first. `utc_offset_s` is how far the clock is ahead of UTC.
    #[wasm_bindgen(js_name = alertsFiredHeld)]
    pub fn alerts_fired_held(
        &self,
        now_unix_s: f64,
        utc_offset_s: i32,
    ) -> Result<Vec<Ts<FiredLine>>, JsValue> {
        self.session
            .call(|core| core.alerts_fired_held(whole_seconds(now_unix_s), utc_offset_s))
            .map_err(thrown)?
            .iter()
            .map(to_js)
            .collect()
    }

    /// Marks a site as one of the pilot's own and keeps its record in the
    /// browser. `credits` are the credits of the answer the site came
    /// from, and `as_of_unix_s` the date of the source's data by the same
    /// answer: the source's credit and the date are kept with the site.
    /// Keeping a site that is kept already takes its record anew.
    #[wasm_bindgen(js_name = keepSite)]
    pub fn keep_site(
        &self,
        site: Ts<SiteView>,
        credits: Vec<Ts<Credit>>,
        as_of_unix_s: Option<f64>,
    ) -> Result<(), JsValue> {
        let site = from_js(&site)?;
        let credits = credits.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
        self.session
            .call(|core| core.keep_site(site, credits, as_of_unix_s.map(whole_seconds)))
            .map_err(thrown)
    }

    /// Unmarks a site: it is no longer one of the pilot's own.
    #[wasm_bindgen(js_name = forgetSite)]
    pub fn forget_site(&self, id: String) -> Result<(), JsValue> {
        self.session
            .call(|core| core.forget_site(id))
            .map_err(thrown)
    }

    /// Whether a site is one of the pilot's own, for the button on its
    /// page.
    #[wasm_bindgen(js_name = isSiteKept)]
    pub fn is_site_kept(&self, id: String) -> Result<bool, JsValue> {
        self.session
            .call(|core| core.is_site_kept(id))
            .map_err(thrown)
    }

    /// The pilot's own sites, from the browser's store alone. With a place
    /// given, the nearest first, each with its distance and direction from
    /// there; without, by name.
    #[wasm_bindgen(js_name = keptSites)]
    pub fn kept_sites(&self, from: Option<Ts<Coordinate>>) -> Result<Ts<KeptSites>, JsValue> {
        let from = from.as_ref().map(from_js).transpose()?;
        let kept = self
            .session
            .call(|core| core.kept_sites(from))
            .map_err(thrown)?;
        to_js(&kept)
    }

    /// The take-offs Today lists: the pilot's own first, however far away,
    /// then those around `place`. `near` is what `sitesMap` read of the
    /// server's answer for `todayArea` around the place; without it the
    /// list is the pilot's own. A kept site the answer holds is refreshed
    /// from it, so there may be changes to store afterwards.
    #[wasm_bindgen(js_name = todayTakeoffs)]
    pub fn today_takeoffs(
        &self,
        place: Option<Ts<Coordinate>>,
        near: Option<Ts<SitesMap>>,
    ) -> Result<Ts<TodayTakeoffs>, JsValue> {
        let place = place.as_ref().map(from_js).transpose()?;
        let near = near.as_ref().map(from_js).transpose()?;
        let today = self
            .session
            .call(|core| core.today_takeoffs(place, near))
            .map_err(thrown)?;
        to_js(&today)
    }

    /// The landing fields for the cockpit's reading of the nearest one
    /// within glide, nearest to `glider` first, at most fifty: the
    /// landings of `near` (what `sitesMap` read of the answer for
    /// `todayArea` around the glider), the pilot's kept landings and the
    /// pilot's waypoints of the kind landing.
    #[wasm_bindgen(js_name = cockpitLandings)]
    pub fn cockpit_landings(
        &self,
        glider: Ts<Coordinate>,
        near: Option<Ts<SitesMap>>,
    ) -> Result<Vec<Ts<TargetPlace>>, JsValue> {
        let glider = from_js(&glider)?;
        let near = near.as_ref().map(from_js).transpose()?;
        self.session
            .call(|core| core.cockpit_landings(glider, near))
            .map_err(thrown)?
            .iter()
            .map(to_js)
            .collect()
    }

    // Groups of pilots and the plans their members make for a flying day,
    // on the Venti server. The core says what to send and reads the answer;
    // the page sends it with the session token. Nothing of it is kept.
    //
    // Every request is refused in words while this browser is signed out.
    // The list of the pilot's groups alone (`groups_request`) is not bound:
    // the board carries the groups, and no page asks for them without it.

    /// The request for the plans of every group the pilot is in, from a
    /// day on and, if given, up to one: both written `2026-11-21`. The
    /// first day is the pilot's own today, which the browser knows and the
    /// server does not.
    #[wasm_bindgen(js_name = planBoardRequest)]
    pub fn plan_board_request(
        &self,
        from_day: String,
        to_day: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.plan_board_request(from_day, to_day))
    }

    /// Reads the server's answer to `planBoardRequest`: the pilot's groups
    /// and their plans, day by day.
    #[wasm_bindgen(js_name = planBoard)]
    pub fn plan_board(&self, status: u16, body: Vec<u8>) -> Result<Ts<PlanBoard>, JsValue> {
        let board = self
            .session
            .call(|core| core.plan_board(status, body))
            .map_err(thrown)?;
        to_js(&board)
    }

    /// The request that makes a group, with the pilot as its keeper.
    #[wasm_bindgen(js_name = createGroupRequest)]
    pub fn create_group_request(&self, name: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.create_group_request(name))
    }

    /// Reads the server's answer to `createGroupRequest`: the group and
    /// its invite code, which the server shows this once.
    #[wasm_bindgen(js_name = groupMade)]
    pub fn group_made(&self, status: u16, body: Vec<u8>) -> Result<Ts<GroupMade>, JsValue> {
        let made = self
            .session
            .call(|core| core.group_made(status, body))
            .map_err(thrown)?;
        to_js(&made)
    }

    /// The request that takes the pilot into the group an invite code
    /// belongs to. Hyphens, spaces and capitals in the code do not matter.
    #[wasm_bindgen(js_name = joinGroupRequest)]
    pub fn join_group_request(&self, code: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.join_group_request(code))
    }

    /// Reads the server's answer to `joinGroupRequest`.
    #[wasm_bindgen(js_name = groupJoined)]
    pub fn group_joined(&self, status: u16, body: Vec<u8>) -> Result<Ts<GroupJoined>, JsValue> {
        let joined = self
            .session
            .call(|core| core.group_joined(status, body))
            .map_err(thrown)?;
        to_js(&joined)
    }

    /// The request for one group with its members.
    #[wasm_bindgen(js_name = groupRequest)]
    pub fn group_request(&self, id: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.group_request(id))
    }

    /// Reads a group with its members: the server's answer to
    /// `groupRequest`, `renameGroupRequest` and `handOverGroupRequest`.
    pub fn group(&self, status: u16, body: Vec<u8>) -> Result<Ts<GroupDetail>, JsValue> {
        let detail = self
            .session
            .call(|core| core.group(status, body))
            .map_err(thrown)?;
        to_js(&detail)
    }

    /// The request that renames a group. Its keeper's to send.
    #[wasm_bindgen(js_name = renameGroupRequest)]
    pub fn rename_group_request(
        &self,
        id: String,
        name: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.rename_group_request(id, name))
    }

    /// The request that hands the keeping of a group to another member.
    /// Its keeper's to send.
    #[wasm_bindgen(js_name = handOverGroupRequest)]
    pub fn hand_over_group_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.hand_over_group_request(id, handle))
    }

    /// The request that makes a new invite code for a group. The code
    /// before it stops working. Its keeper's to send.
    #[wasm_bindgen(js_name = newGroupInviteRequest)]
    pub fn new_group_invite_request(&self, id: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.new_group_invite_request(id))
    }

    /// Reads the server's answer to `newGroupInviteRequest`: the new code,
    /// shown this once.
    #[wasm_bindgen(js_name = groupInvite)]
    pub fn group_invite(&self, status: u16, body: Vec<u8>) -> Result<String, JsValue> {
        self.session.core.group_invite(status, body).map_err(thrown)
    }

    /// The request that takes the pilot out of a group. What the pilot put
    /// into it goes with them: their plans, and their word on the plans of
    /// others.
    #[wasm_bindgen(js_name = leaveGroupRequest)]
    pub fn leave_group_request(&self, id: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.leave_group_request(id))
    }

    /// The request that removes a member from a group. Its keeper's to
    /// send; it also ends the invite code, which the removed member knows.
    #[wasm_bindgen(js_name = removeGroupMemberRequest)]
    pub fn remove_group_member_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.remove_group_member_request(id, handle))
    }

    /// The request that deletes a group with everything in it. Its
    /// keeper's to send.
    #[wasm_bindgen(js_name = deleteGroupRequest)]
    pub fn delete_group_request(&self, id: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.delete_group_request(id))
    }

    /// Reads the server's answer to a request that gives nothing back:
    /// leaving, removing a member, deleting a group or a plan.
    #[wasm_bindgen(js_name = groupDone)]
    pub fn group_done(&self, status: u16, body: Vec<u8>) -> Result<(), JsValue> {
        self.session.core.group_done(status, body).map_err(thrown)
    }

    /// The request that posts a plan to a group. Its author is in from the
    /// start.
    #[wasm_bindgen(js_name = postPlanRequest)]
    pub fn post_plan_request(
        &self,
        group: String,
        draft: Ts<PlanDraft>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.post_plan_request(group, draft))
    }

    /// The request that changes a plan: the whole plan as it is to be. Its
    /// author's to send.
    #[wasm_bindgen(js_name = changePlanRequest)]
    pub fn change_plan_request(
        &self,
        group: String,
        plan: String,
        draft: Ts<PlanDraft>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.change_plan_request(group, plan, draft))
    }

    /// The request that deletes a plan. Its author's and the group's
    /// keeper's to send.
    #[wasm_bindgen(js_name = deletePlanRequest)]
    pub fn delete_plan_request(
        &self,
        group: String,
        plan: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.delete_plan_request(group, plan))
    }

    /// The request that says the pilot is in on a plan, or takes that back.
    #[wasm_bindgen(js_name = planInRequest)]
    pub fn plan_in_request(
        &self,
        group: String,
        plan: String,
        is_in: bool,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.plan_in_request(group, plan, is_in))
    }

    /// Reads one plan: the server's answer to posting it, changing it and
    /// to the pilot's word on it. The group's name is not in such an
    /// answer.
    pub fn plan(&self, status: u16, body: Vec<u8>) -> Result<Ts<PlanView>, JsValue> {
        let plan = self
            .session
            .call(|core| core.plan(status, body))
            .map_err(thrown)?;
        to_js(&plan)
    }

    /// The request for the open plans near a place, from a day on and, if
    /// given, up to one, within `radius_km` (25 when left out). Anyone may
    /// ask, signed in or not.
    #[wasm_bindgen(js_name = openPlansNearRequest)]
    pub fn open_plans_near_request(
        &self,
        place: Ts<Coordinate>,
        radius_km: Option<u32>,
        from_day: String,
        to_day: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let place = from_js(&place)?;
        self.request(|core| core.open_plans_near_request(place, radius_km, from_day, to_day))
    }

    /// The request for the open plans the pilot posted or is in on.
    #[wasm_bindgen(js_name = myOpenPlansRequest)]
    pub fn my_open_plans_request(&self) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.my_open_plans_request())
    }

    /// Reads the server's answer to `openPlansNearRequest` and to
    /// `myOpenPlansRequest`: the plans by day.
    #[wasm_bindgen(js_name = openPlans)]
    pub fn open_plans(&self, status: u16, body: Vec<u8>) -> Result<Ts<OpenPlans>, JsValue> {
        let plans = self
            .session
            .call(|core| core.open_plans(status, body))
            .map_err(thrown)?;
        to_js(&plans)
    }

    /// The request for one open plan, which anyone may read.
    #[wasm_bindgen(js_name = openPlanRequest)]
    pub fn open_plan_request(&self, plan: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.open_plan_request(plan))
    }

    /// The request that posts an open plan, which everyone can read.
    #[wasm_bindgen(js_name = postOpenPlanRequest)]
    pub fn post_open_plan_request(
        &self,
        draft: Ts<OpenPlanDraft>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.post_open_plan_request(draft))
    }

    /// The request that changes an open plan: the whole plan as it is to
    /// be. Its author's to send.
    #[wasm_bindgen(js_name = changeOpenPlanRequest)]
    pub fn change_open_plan_request(
        &self,
        plan: String,
        draft: Ts<OpenPlanDraft>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.change_open_plan_request(plan, draft))
    }

    /// The request that deletes an open plan; `groupDone` reads the answer.
    #[wasm_bindgen(js_name = deleteOpenPlanRequest)]
    pub fn delete_open_plan_request(&self, plan: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.delete_open_plan_request(plan))
    }

    /// The request that says "I'm in" on an open plan, or takes that back.
    #[wasm_bindgen(js_name = openPlanInRequest)]
    pub fn open_plan_in_request(
        &self,
        plan: String,
        is_in: bool,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.open_plan_in_request(plan, is_in))
    }

    /// Reads one open plan: the answer to reading, posting and changing
    /// it, and to the pilot's word on it.
    #[wasm_bindgen(js_name = openPlan)]
    pub fn open_plan(&self, status: u16, body: Vec<u8>) -> Result<Ts<OpenPlanView>, JsValue> {
        let plan = self
            .session
            .call(|core| core.open_plan(status, body))
            .map_err(thrown)?;
        to_js(&plan)
    }
}

// Groups as communities with a chat: the requests and the answers read,
// as `venti_core::group_chat` has them.
#[wasm_bindgen]
impl Core {
    /// The request for public groups near a place (both `lat` and `lon`,
    /// or neither), by name or place, or both.
    #[wasm_bindgen(js_name = findGroupsRequest)]
    pub fn find_groups_request(
        &self,
        lat: Option<f64>,
        lon: Option<f64>,
        text: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let near = lat.zip(lon).map(|(lat, lon)| Coordinate { lat, lon });
        self.request(|core| core.find_groups_request(near, text))
    }

    /// Reads the answer to `findGroupsRequest`.
    #[wasm_bindgen(js_name = foundGroups)]
    pub fn found_groups(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Vec<Ts<FoundCommunity>>, JsValue> {
        self.session
            .call(|core| core.found_groups(status, body))
            .map_err(thrown)?
            .iter()
            .map(to_js)
            .collect()
    }

    /// Reads the answer to `groupsRequest` as the groups' section shows it.
    #[wasm_bindgen(js_name = communities)]
    pub fn communities(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Vec<Ts<CommunityView>>, JsValue> {
        self.session
            .call(|core| core.communities(status, body))
            .map_err(thrown)?
            .iter()
            .map(to_js)
            .collect()
    }

    /// The request that makes a group with the pilot as its owner.
    #[wasm_bindgen(js_name = createCommunityRequest)]
    pub fn create_community_request(
        &self,
        draft: Ts<CommunityDraft>,
        public: bool,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.create_community_request(draft, public))
    }

    /// The request that changes a group's name, description and home.
    #[wasm_bindgen(js_name = changeCommunityRequest)]
    pub fn change_community_request(
        &self,
        id: String,
        draft: Ts<CommunityDraft>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.change_community_request(id, draft))
    }

    /// The request that makes a group public or private.
    #[wasm_bindgen(js_name = groupPublicRequest)]
    pub fn group_public_request(
        &self,
        id: String,
        public: bool,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.group_public_request(id, public))
    }

    /// Reads a group with its members and what the pilot may do in it.
    #[wasm_bindgen(js_name = community)]
    pub fn community(&self, status: u16, body: Vec<u8>) -> Result<Ts<CommunityDetail>, JsValue> {
        let read = self
            .session
            .call(|core| core.community(status, body))
            .map_err(thrown)?;
        to_js(&read)
    }

    /// The request that takes the pilot into a public group, or one they
    /// were invited into. The answer goes to `groupJoined`.
    #[wasm_bindgen(js_name = enterGroupRequest)]
    pub fn enter_group_request(&self, id: String) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.enter_group_request(id))
    }

    /// The request that invites a pilot into a group by handle.
    #[wasm_bindgen(js_name = inviteToGroupRequest)]
    pub fn invite_to_group_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.invite_to_group_request(id, handle))
    }

    /// Reads the answer to `inviteToGroupRequest`: a line to show.
    #[wasm_bindgen(js_name = groupInviteSent)]
    pub fn group_invite_sent(&self, status: u16, body: Vec<u8>) -> Result<String, JsValue> {
        self.session
            .call(|core| core.group_invite_sent(status, body))
            .map_err(thrown)
    }

    /// The request that takes an invite back, or declines it.
    #[wasm_bindgen(js_name = dropGroupInviteRequest)]
    pub fn drop_group_invite_request(
        &self,
        id: String,
        handle: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.drop_group_invite_request(id, handle))
    }

    /// The request for the pilot's invites into private groups.
    #[wasm_bindgen(js_name = groupInvitesRequest)]
    pub fn group_invites_request(&self) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(venti_core::Core::group_invites_request)
    }

    /// Reads the answer to `groupInvitesRequest`.
    #[wasm_bindgen(js_name = groupInvites)]
    pub fn group_invites(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Vec<Ts<CommunityInvite>>, JsValue> {
        self.session
            .call(|core| core.group_invites(status, body))
            .map_err(thrown)?
            .iter()
            .map(to_js)
            .collect()
    }

    /// The request that makes a member an admin, or a member again.
    #[wasm_bindgen(js_name = groupAdminRequest)]
    pub fn group_admin_request(
        &self,
        id: String,
        handle: String,
        admin: bool,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.group_admin_request(id, handle, admin))
    }

    /// The request for the newest messages of a group, or the page before
    /// `before`.
    #[wasm_bindgen(js_name = messagesRequest)]
    pub fn messages_request(
        &self,
        id: String,
        before: Option<String>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.messages_request(id, before))
    }

    /// The request for what is new in a group since a page's `since`:
    /// asked every few seconds while the chat is open.
    #[wasm_bindgen(js_name = messagesSinceRequest)]
    pub fn messages_since_request(
        &self,
        id: String,
        since: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.messages_since_request(id, since))
    }

    /// Reads a page of messages.
    #[wasm_bindgen(js_name = chatPage)]
    pub fn chat_page(&self, status: u16, body: Vec<u8>) -> Result<Ts<ChatPage>, JsValue> {
        let read = self
            .session
            .call(|core| core.chat_page(status, body))
            .map_err(thrown)?;
        to_js(&read)
    }

    /// The request that posts a message, checked as the server checks it.
    #[wasm_bindgen(js_name = postMessageRequest)]
    pub fn post_message_request(
        &self,
        id: String,
        draft: Ts<MessageDraft>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        let draft = from_js(&draft)?;
        self.request(|core| core.post_message_request(id, draft))
    }

    /// Reads one message: the answer to posting and to removing one.
    #[wasm_bindgen(js_name = chatMessage)]
    pub fn chat_message(&self, status: u16, body: Vec<u8>) -> Result<Ts<ChatMessage>, JsValue> {
        let read = self
            .session
            .call(|core| core.chat_message(status, body))
            .map_err(thrown)?;
        to_js(&read)
    }

    /// The request that removes a message.
    #[wasm_bindgen(js_name = removeMessageRequest)]
    pub fn remove_message_request(
        &self,
        id: String,
        message: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.remove_message_request(id, message))
    }

    /// The request that marks a group read up to a message's cursor.
    #[wasm_bindgen(js_name = groupReadRequest)]
    pub fn group_read_request(
        &self,
        id: String,
        cursor: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.group_read_request(id, cursor))
    }

    /// The request that mutes a group or unmutes it.
    #[wasm_bindgen(js_name = groupMuteRequest)]
    pub fn group_mute_request(
        &self,
        id: String,
        muted: bool,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.group_mute_request(id, muted))
    }

    /// Reads the answer to `groupReadRequest` and `groupMuteRequest`.
    #[wasm_bindgen(js_name = groupStanding)]
    pub fn group_standing(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Ts<CommunityStanding>, JsValue> {
        let read = self
            .session
            .call(|core| core.group_standing(status, body))
            .map_err(thrown)?;
        to_js(&read)
    }

    /// The request that sends a picture for a message.
    #[wasm_bindgen(js_name = sendGroupPictureRequest)]
    pub fn send_group_picture_request(
        &self,
        id: String,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.send_group_picture_request(id, picture, media_type))
    }

    /// Reads the answer to `sendGroupPictureRequest`.
    #[wasm_bindgen(js_name = groupPictureSent)]
    pub fn group_picture_sent(
        &self,
        status: u16,
        body: Vec<u8>,
    ) -> Result<Ts<SentGroupPicture>, JsValue> {
        let read = self
            .session
            .call(|core| core.group_picture_sent(status, body))
            .map_err(thrown)?;
        to_js(&read)
    }

    /// The request that gives a group its picture.
    #[wasm_bindgen(js_name = groupOwnPictureRequest)]
    pub fn group_own_picture_request(
        &self,
        id: String,
        picture: Vec<u8>,
        media_type: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.group_own_picture_request(id, picture, media_type))
    }

    /// The request that removes a group's picture.
    #[wasm_bindgen(js_name = removeGroupOwnPictureRequest)]
    pub fn remove_group_own_picture_request(
        &self,
        id: String,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        self.request(|core| core.remove_group_own_picture_request(id))
    }
}

/// The words of the groups' screens.
#[wasm_bindgen(js_name = communityWords)]
pub fn community_words() -> Result<Ts<CommunityWords>, JsValue> {
    to_js(&venti_core::community_words())
}

/// The links to Venti's own records in a message's text.
#[wasm_bindgen(js_name = messageLinks)]
pub fn message_links(text: String) -> Result<Vec<Ts<LinkDraft>>, JsValue> {
    venti_core::message_links(text).iter().map(to_js).collect()
}

/// The form for a report about a message of a group or a picture in one.
#[wasm_bindgen(js_name = messageReportForm)]
pub fn message_report_form() -> Result<Ts<ReportForm>, JsValue> {
    to_js(&venti_core::message_report_form())
}

/// The request that reports a message of a group, or a picture in one.
#[wasm_bindgen(js_name = messageReportRequest)]
pub fn message_report_request(
    group_id: String,
    id: String,
    picture: bool,
    reason: Ts<ReportReason>,
    text: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    let request =
        venti_core::message_report_request(group_id, id, picture, from_js(&reason)?, text)
            .map_err(thrown)?;
    to_js(&request)
}

impl Core {
    /// One of the core's requests about the pilot's groups. Each of them
    /// reads the account first, which may have to be fetched.
    fn request(
        &self,
        make: impl FnOnce(&venti_core::Core) -> Result<ServerRequest, CoreError>,
    ) -> Result<Ts<ServerRequest>, JsValue> {
        to_js(&self.session.call(make).map_err(thrown)?)
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

/// One run of bringing the pilot's own sites in step with their account.
///
/// Its steps are the logbook's: the page sends each request the run hands
/// out, with the session token, and passes the answer back, until the run
/// says that it has finished or why it stopped. In the summary, `sent`
/// and `fetched` count sites.
#[wasm_bindgen]
#[derive(Debug)]
pub struct KeptSitesSync {
    sync: Arc<venti_core::KeptSitesSync>,
    staged: Arc<StagedStorage>,
}

#[wasm_bindgen]
impl KeptSitesSync {
    /// Begins. `now_unix_s` is the current time: the note of when, for
    /// the sites kept in this browser since the last run.
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

/// One run of fetching the account's alarms and what they fired.
#[wasm_bindgen]
pub struct AlertsSync {
    sync: Arc<venti_core::AlertsSync>,
    staged: Arc<StagedStorage>,
}

#[wasm_bindgen]
impl AlertsSync {
    /// Begins.
    pub fn start(&self) -> Result<Ts<SyncStep>, JsValue> {
        let step = self.sync.start_when_fetched();
        to_js(&settle(&self.staged, step).map_err(thrown)?)
    }

    /// Takes the server's answer to the last request.
    pub fn receive(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<Ts<SyncStep>, JsValue> {
        let step = self.sync.receive_when_fetched(status, body, retry_after_s);
        to_js(&settle(&self.staged, step).map_err(thrown)?)
    }

    /// What this run fetched that fired since the run before: what the
    /// browser's notification says while the page is open.
    #[wasm_bindgen(js_name = newLines)]
    pub fn new_lines(&self, utc_offset_s: i32) -> Result<Vec<Ts<FiredLine>>, JsValue> {
        self.sync
            .new_lines(utc_offset_s)
            .iter()
            .map(to_js)
            .collect()
    }
}

// The time zone of a place, to show a time by the clock where it was:
// the flights' own records carry theirs (`timeZone`).

/// The IANA time zone of a place on land, as `Europe/Vienna`; none at sea
/// away from a coast. Format a time in it with `Intl.DateTimeFormat`'s
/// `timeZone`.
#[wasm_bindgen(js_name = timeZoneAt)]
pub fn time_zone_at(lat: f64, lon: f64) -> Option<String> {
    venti_core::time_zone_at(lat, lon)
}

// The pilot's alarms. The core has the choices and the words, checks a
// draft, says what to send and reads the answers; the page sends.

/// The sixteen points of the compass, short, from the north clockwise.
#[wasm_bindgen(js_name = alertPoints)]
pub fn alert_points() -> Vec<String> {
    venti_core::alert_points()
}

/// What a draft waits for, in one line.
#[wasm_bindgen(js_name = alertLine)]
pub fn alert_line(draft: Ts<AlertDraft>) -> Result<String, JsValue> {
    Ok(venti_core::alert_line(from_js(&draft)?))
}

/// Everything that is wrong with a draft. Empty when it may be sent.
#[wasm_bindgen(js_name = alertCheck)]
pub fn alert_check(draft: Ts<AlertDraft>) -> Result<Vec<Ts<AlertFaultView>>, JsValue> {
    venti_core::alert_check(from_js(&draft)?)
        .iter()
        .map(to_js)
        .collect()
}

/// The request for the account's alarms.
#[wasm_bindgen(js_name = alertsRequest)]
pub fn alerts_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::alerts_request())
}

/// The request that sets a new alarm.
#[wasm_bindgen(js_name = alertAddRequest)]
pub fn alert_add_request(draft: Ts<AlertDraft>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::alert_add_request(from_js(&draft)?).map_err(thrown)?)
}

/// The request that changes an alarm.
#[wasm_bindgen(js_name = alertChangeRequest)]
pub fn alert_change_request(
    id: String,
    draft: Ts<AlertDraft>,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::alert_change_request(id, from_js(&draft)?).map_err(thrown)?)
}

/// The request that removes an alarm.
#[wasm_bindgen(js_name = alertRemoveRequest)]
pub fn alert_remove_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::alert_remove_request(id))
}

/// Reads the answer to `alertRemoveRequest`: what became of the alarm. A
/// 404 is an alarm that was gone already, and no error.
#[wasm_bindgen(js_name = alertRemoved)]
pub fn alert_removed(status: u16, body: Vec<u8>) -> Result<String, JsValue> {
    venti_core::alert_removed(status, body).map_err(thrown)
}

/// The words of the alarms' screens: the form, the list and what fired.
#[wasm_bindgen(js_name = alertFormWords)]
pub fn alert_form_words() -> Result<Ts<AlertFormWords>, JsValue> {
    to_js(&venti_core::alert_form_words())
}

/// The numbers of the form of an alarm about pilots in the air: what its
/// two fields start at and the least and most they may be.
#[wasm_bindgen(js_name = pilotsAlarmNumbers)]
pub fn pilots_alarm_numbers() -> Result<Ts<PilotsAlarmNumbers>, JsValue> {
    to_js(&venti_core::pilots_alarm_numbers())
}

/// Reads the answer to `alertsRequest`.
#[wasm_bindgen(js_name = alertsRead)]
pub fn alerts_read(status: u16, body: Vec<u8>) -> Result<Vec<Ts<AlertView>>, JsValue> {
    venti_core::alerts(status, body)
        .map_err(thrown)?
        .iter()
        .map(to_js)
        .collect()
}

/// Reads the answer to `alertAddRequest` or `alertChangeRequest`.
#[wasm_bindgen(js_name = alertSaved)]
pub fn alert_saved(status: u16, body: Vec<u8>) -> Result<Ts<AlertView>, JsValue> {
    to_js(&venti_core::alert_saved(status, body).map_err(thrown)?)
}

/// The request for what the alarms fired since `since`.
#[wasm_bindgen(js_name = alertsFiredRequest)]
pub fn alerts_fired_request(since: Option<String>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::alerts_fired_request(since))
}

/// Reads the answer to `alertsFiredRequest`.
#[wasm_bindgen(js_name = alertsFired)]
pub fn alerts_fired(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
) -> Result<Ts<FiredPage>, JsValue> {
    to_js(&venti_core::alerts_fired(status, body, utc_offset_s).map_err(thrown)?)
}

// Web Push, so that this browser is told when an alarm fires while no
// Venti page is open. The page asks for the permission, subscribes with
// the server's key and sends the subscription; its service worker shows
// what arrives (`docs/api.md`, "Web Push").

/// The line beside the switch that asks for the browser's permission.
#[wasm_bindgen(js_name = pushPermissionLine)]
pub fn push_permission_line() -> String {
    venti_core::push_permission_line()
}

/// The request for the server's public key for Web Push; it needs no
/// token.
#[wasm_bindgen(js_name = pushKeyRequest)]
pub fn push_key_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::push_key_request())
}

/// Reads the answer to `pushKeyRequest`: the `applicationServerKey`,
/// base64url.
#[wasm_bindgen(js_name = pushKey)]
pub fn push_key(status: u16, body: Vec<u8>) -> Result<String, JsValue> {
    venti_core::push_key(status, body).map_err(thrown)
}

/// The request that subscribes this browser, from its subscription's
/// `toJSON()`: the endpoint, `keys.p256dh` and `keys.auth`.
#[wasm_bindgen(js_name = pushSubscribeRequest)]
pub fn push_subscribe_request(
    endpoint: String,
    p256dh: String,
    auth: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::push_subscribe_request(endpoint, p256dh, auth).map_err(thrown)?)
}

/// The request that removes this browser's subscription, by its endpoint.
#[wasm_bindgen(js_name = pushUnsubscribeRequest)]
pub fn push_unsubscribe_request(endpoint: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::push_unsubscribe_request(endpoint).map_err(thrown)?)
}

/// Reads the answer to `pushSubscribeRequest` or `pushUnsubscribeRequest`:
/// throws the server's refusal in words.
#[wasm_bindgen(js_name = pushSaved)]
pub fn push_saved(status: u16, body: Vec<u8>) -> Result<(), JsValue> {
    venti_core::push_saved(status, body).map_err(thrown)
}

// Push to the phone apps, what pilots nearby do, and help after landing
// (`docs/api.md`, "Push to the phone apps" and "Help after landing"). The
// web app has no phone of its own to register; it can show the settings,
// read and answer a call for help, and ask for one.

/// Reads an answer that carries nothing: throws the server's refusal in
/// words.
#[wasm_bindgen(js_name = phoneRequestDone)]
pub fn phone_request_done(status: u16, body: Vec<u8>) -> Result<(), JsValue> {
    venti_core::phone_request_done(status, body).map_err(thrown)
}

/// The request that registers a phone's push token on the account.
#[wasm_bindgen(js_name = deviceRegisterRequest)]
pub fn device_register_request(
    platform: Ts<PhonePlatform>,
    token: String,
    sandbox: bool,
) -> Result<Ts<ServerRequest>, JsValue> {
    let platform = from_js(&platform)?;
    to_js(&venti_core::device_register_request(platform, token, sandbox).map_err(thrown)?)
}

/// The request that removes a phone from the account.
#[wasm_bindgen(js_name = deviceRemoveRequest)]
pub fn device_remove_request(
    platform: Ts<PhonePlatform>,
    token: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    let platform = from_js(&platform)?;
    to_js(&venti_core::device_remove_request(platform, token).map_err(thrown)?)
}

/// The request for the settings for what pilots nearby do.
#[wasm_bindgen(js_name = nearbySettingsRequest)]
pub fn nearby_settings_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::nearby_settings_request())
}

/// The request that sets them, whole.
#[wasm_bindgen(js_name = nearbySettingsChangeRequest)]
pub fn nearby_settings_change_request(
    choice: Ts<NearbyChoice>,
) -> Result<Ts<ServerRequest>, JsValue> {
    let choice = from_js(&choice)?;
    to_js(&venti_core::nearby_settings_change_request(choice).map_err(thrown)?)
}

/// Reads the answer to `nearbySettingsRequest` or
/// `nearbySettingsChangeRequest`.
#[wasm_bindgen(js_name = nearbySettings)]
pub fn nearby_settings(status: u16, body: Vec<u8>) -> Result<Ts<NearbyChoice>, JsValue> {
    to_js(&venti_core::nearby_settings(status, body).map_err(thrown)?)
}

/// The words of the two settings: a title and a line for each.
#[wasm_bindgen(js_name = nearbySettingsWords)]
pub fn nearby_settings_words() -> Vec<String> {
    venti_core::nearby_settings_words()
}

/// The request that tells where the device is, for being asked for help.
#[wasm_bindgen(js_name = positionRequest)]
pub fn position_request(at: Ts<Coordinate>) -> Result<Ts<ServerRequest>, JsValue> {
    let at = from_js(&at)?;
    to_js(&venti_core::position_request(at).map_err(thrown)?)
}

/// The request that has the server forget it.
#[wasm_bindgen(js_name = positionForgetRequest)]
pub fn position_forget_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::position_forget_request())
}

/// The first words of every screen of help after landing.
#[wasm_bindgen(js_name = helpEmergencyWords)]
pub fn help_emergency_words() -> String {
    venti_core::help_emergency_words()
}

/// The request that asks the pilots nearby for help.
#[wasm_bindgen(js_name = helpAskRequest)]
pub fn help_ask_request(at: Ts<Coordinate>, note: String) -> Result<Ts<ServerRequest>, JsValue> {
    let at = from_js(&at)?;
    to_js(&venti_core::help_ask_request(at, note).map_err(thrown)?)
}

/// The request for the requests for help that stand.
#[wasm_bindgen(js_name = helpListRequest)]
pub fn help_list_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::help_list_request())
}

/// The request for one request for help.
#[wasm_bindgen(js_name = helpOneRequest)]
pub fn help_one_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::help_one_request(id))
}

/// The request that closes the pilot's own request.
#[wasm_bindgen(js_name = helpCloseRequest)]
pub fn help_close_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::help_close_request(id))
}

/// The request that says whether the pilot can help.
#[wasm_bindgen(js_name = helpAnswerRequest)]
pub fn help_answer_request(id: String, can_help: bool) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::help_answer_request(id, can_help).map_err(thrown)?)
}

/// Reads the answer to `helpAskRequest`, `helpOneRequest` or
/// `helpAnswerRequest`; `nowMs` is `Date.now()`.
#[wasm_bindgen(js_name = helpRequest)]
pub fn help_request(status: u16, body: Vec<u8>, now_ms: f64) -> Result<Ts<HelpView>, JsValue> {
    to_js(&venti_core::help_request(status, body, millis(now_ms)).map_err(thrown)?)
}

/// Reads the answer to `helpListRequest`; `nowMs` is `Date.now()`.
#[wasm_bindgen(js_name = helpRequests)]
pub fn help_requests(status: u16, body: Vec<u8>, now_ms: f64) -> Result<Ts<HelpLists>, JsValue> {
    to_js(&venti_core::help_requests(status, body, millis(now_ms)).map_err(thrown)?)
}

/// Milliseconds from JavaScript's clock, whole.
fn millis(now_ms: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let whole = now_ms.round() as i64;
    whole
}

// The tokens a pilot gives to other programs, for a screen "Programs with
// access" under the account. The core has the words, says what to send
// and reads the answers; the page sends each request with the session
// token. Nothing of it is kept, and a new token's secret least of all.

/// The things a program can be allowed, each with a title and a line.
#[wasm_bindgen(js_name = tokenScopes)]
pub fn token_scopes() -> Result<Vec<Ts<TokenScope>>, JsValue> {
    venti_core::token_scopes().iter().map(to_js).collect()
}

/// The words of the screen and of the form that makes a token.
#[wasm_bindgen(js_name = tokenForm)]
pub fn token_form() -> Result<Ts<TokenForm>, JsValue> {
    to_js(&venti_core::token_form())
}

/// Checks the name a pilot gives a token before anything is sent, and
/// returns it without the spaces around it.
#[wasm_bindgen(js_name = tokenNameCheck)]
pub fn token_name_check(name: String) -> Result<String, JsValue> {
    venti_core::token_name_check(name).map_err(thrown)
}

/// The request that makes a token: its name, the names of the scopes
/// chosen, the moment it ends if it has an end, and the password.
#[wasm_bindgen(js_name = tokenCreateRequest)]
pub fn token_create_request(
    name: String,
    scopes: Vec<String>,
    ends_unix_s: Option<f64>,
    password: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    let ends_unix_s = ends_unix_s.map(whole_seconds);
    to_js(&venti_core::token_create_request(name, scopes, ends_unix_s, password).map_err(thrown)?)
}

/// Reads the answer to `tokenCreateRequest`: the token, and its secret
/// with the words for the one moment it is shown.
#[wasm_bindgen(js_name = tokenCreated)]
pub fn token_created(
    status: u16,
    body: Vec<u8>,
    now_unix_s: f64,
) -> Result<Ts<TokenCreated>, JsValue> {
    to_js(&venti_core::token_created(status, body, whole_seconds(now_unix_s)).map_err(thrown)?)
}

// Letting a program read the pilot's flights, for the page `/connect/`:
// the server's page links to it with `?request=` and an identifier, the
// page reads the request and answers it with the session token, and the
// answer is the address the browser goes to. The core has every word.
// Nothing of it is kept, the identifier and that address least of all.

/// The words of the page that do not depend on the request: the buttons
/// "Allow" and "Don't allow", "Sign in", and what it says while it waits.
#[wasm_bindgen(js_name = agreementWords)]
pub fn agreement_words() -> Result<Ts<AgreementWords>, JsValue> {
    to_js(&venti_core::agreement_words())
}

/// The page for a request that has ended: for a link with no identifier
/// or one that is none, and when an open page reaches the request's end.
#[wasm_bindgen(js_name = agreementEnded)]
pub fn agreement_ended() -> Result<Ts<AgreementPage>, JsValue> {
    to_js(&venti_core::agreement_ended())
}

/// The request that reads a request for agreement, by the identifier in
/// the page's address. It needs no token. Throws for text that is no
/// identifier: show `agreementEnded` then.
#[wasm_bindgen(js_name = agreementRequest)]
pub fn agreement_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::agreement_request(id).map_err(thrown)?)
}

/// Reads the answer to `agreementRequest` into the page. `handle` is the
/// handle of the account this browser is signed in to, if it is signed in
/// to one. It does not throw for a refusal: that is a page too.
#[wasm_bindgen(js_name = agreementPage)]
pub fn agreement_page(
    status: u16,
    body: Vec<u8>,
    handle: Option<String>,
) -> Result<Ts<AgreementPage>, JsValue> {
    to_js(&venti_core::agreement_page(status, body, handle))
}

/// The request that answers: `agree` is `true` for "Allow" and `false`
/// for "Don't allow". It goes with the session token.
#[wasm_bindgen(js_name = agreementAnswerRequest)]
pub fn agreement_answer_request(id: String, agree: bool) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::agreement_answer_request(id, agree).map_err(thrown)?)
}

/// Reads the answer to `agreementAnswerRequest`: where the browser goes,
/// or what the page shows in its place.
#[wasm_bindgen(js_name = agreementAnswered)]
pub fn agreement_answered(status: u16, body: Vec<u8>) -> Result<Ts<AgreementOutcome>, JsValue> {
    to_js(&venti_core::agreement_answered(status, body))
}

/// The request for the cables around a place: power lines and ropeways
/// from the server, to draw on the map.
#[wasm_bindgen(js_name = cablesRequest)]
pub fn cables_request(lat: f64, lon: f64) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::cables_request(lat, lon).map_err(thrown)?)
}

/// Reads the answer to `cablesRequest` as a map layer: each cable with its
/// kind and words, the credits, and what to say of the data's limits.
#[wasm_bindgen(js_name = cablesLayer)]
pub fn cables_layer(status: u16, body: Vec<u8>) -> Result<Ts<CablesMap>, JsValue> {
    to_js(&CableSet::from_answer(status, body).map_err(thrown)?.layer())
}

/// The request for the account's tokens.
#[wasm_bindgen(js_name = tokensRequest)]
pub fn tokens_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::tokens_request())
}

/// Reads the answer to `tokensRequest`: the tokens, the newest first.
#[wasm_bindgen]
pub fn tokens(status: u16, body: Vec<u8>, now_unix_s: f64) -> Result<Vec<Ts<TokenView>>, JsValue> {
    venti_core::tokens(status, body, whole_seconds(now_unix_s))
        .map_err(thrown)?
        .iter()
        .map(to_js)
        .collect()
}

/// The words that ask the pilot before a token is withdrawn.
#[wasm_bindgen(js_name = tokenWithdrawal)]
pub fn token_withdrawal(name: String) -> Result<Ts<TokenWithdrawal>, JsValue> {
    to_js(&venti_core::token_withdrawal(name))
}

/// The request that withdraws a token, by the identifier the list gives.
#[wasm_bindgen(js_name = tokenWithdrawRequest)]
pub fn token_withdraw_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::token_withdraw_request(id).map_err(thrown)?)
}

/// Reads the answer to `tokenWithdrawRequest`. A token that was withdrawn
/// already is withdrawn.
#[wasm_bindgen(js_name = tokenWithdrawn)]
pub fn token_withdrawn(status: u16, body: Vec<u8>) -> Result<(), JsValue> {
    venti_core::token_withdrawn(status, body).map_err(thrown)
}

/// The words of signing in with Google.
#[wasm_bindgen(js_name = googleWords)]
pub fn google_words() -> Result<Ts<GoogleWords>, JsValue> {
    to_js(&venti_core::google_words())
}

/// The words of who sees a pilot's flights: the switch for new flights,
/// its line, and the two buttons that set all flights at once.
#[wasm_bindgen(js_name = flightVisibilityWords)]
pub fn flight_visibility_words() -> Result<Ts<FlightVisibilityWords>, JsValue> {
    to_js(&venti_core::flight_visibility_words())
}

/// The question asked before every flight is made public or private:
/// "Make all 101 flights public?".
#[wasm_bindgen(js_name = allFlightsQuestion)]
pub fn all_flights_question(
    flights: u32,
    visibility: Ts<FlightVisibility>,
) -> Result<String, JsValue> {
    Ok(venti_core::all_flights_question(
        flights,
        from_js(&visibility)?,
    ))
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

/// What a station's page says under its newest reading: "Measured 25
/// minutes ago.", or "Measured at 14:05." while Venti's clock is not known
/// (`now_unix_s` zero). `utc_offset_s` is how far the pilot's clock is
/// ahead of UTC.
#[wasm_bindgen(js_name = stationMeasuredWords)]
pub fn station_measured_words(
    station: Ts<WindStation>,
    now_unix_s: f64,
    utc_offset_s: i32,
) -> Result<String, JsValue> {
    Ok(venti_core::station_measured_words(
        from_js(&station)?,
        whole_seconds(now_unix_s),
        utc_offset_s,
    ))
}

/// The days of a station's past to offer in a day picker: the first and
/// the last as a date field writes them. `now_unix_s` is the current time
/// by the server's clock and `utc_offset_s` how far the pilot's clock is
/// ahead of UTC.
#[wasm_bindgen(js_name = stationDays)]
pub fn station_days(now_unix_s: f64, utc_offset_s: i32) -> Result<Ts<StationDays>, JsValue> {
    to_js(&venti_core::station_days(
        whole_seconds(now_unix_s),
        utc_offset_s,
    ))
}

/// The day a moment falls on, as `stationDayPath` takes a day.
#[wasm_bindgen(js_name = stationDate)]
pub fn station_date(at_unix_s: f64, utc_offset_s: i32) -> String {
    venti_core::station_date(whole_seconds(at_unix_s), utc_offset_s)
}

/// The path and query to `GET` for one day of a station's readings. `date`
/// is the day as a date field writes it, "2026-11-18"; `utc_offset_s` how
/// far the pilot's clock was ahead of UTC on that day.
#[wasm_bindgen(js_name = stationDayPath)]
pub fn station_day_path(
    station_id: String,
    date: String,
    utc_offset_s: i32,
) -> Result<String, JsValue> {
    venti_core::station_day_path(station_id, date, utc_offset_s).map_err(thrown)
}

/// Reads the server's answer to `stationDayPath`, asked with the same
/// `date` and `utc_offset_s`.
#[wasm_bindgen(js_name = stationDay)]
pub fn station_day(
    status: u16,
    body: Vec<u8>,
    date: String,
    utc_offset_s: i32,
) -> Result<Ts<StationDay>, JsValue> {
    to_js(&venti_core::station_day(status, body, date, utc_offset_s).map_err(thrown)?)
}

/// What a station's newest reading says about the air: a cloud base over
/// it, and the temperature step to a station higher up or lower down among
/// `stations`, the stations of the wind map.
#[wasm_bindgen(js_name = stationAir)]
pub fn station_air(
    station: Ts<WindStation>,
    stations: Vec<Ts<WindStation>>,
) -> Result<Ts<StationAir>, JsValue> {
    let stations = stations
        .iter()
        .map(from_js)
        .collect::<Result<Vec<_>, _>>()?;
    to_js(&venti_core::station_air(from_js(&station)?, stations))
}

/// The path to `GET` from the server for the foehn reading.
#[wasm_bindgen(js_name = foehnPath)]
pub fn foehn_path() -> String {
    venti_core::foehn_path()
}

/// Reads the server's answer to `foehnPath`: the line for Today and the
/// foehn view. `places` are where the pilot's sites are; `now_unix_s` is
/// the current time by the server's clock, zero or less when it is not
/// known.
#[wasm_bindgen(js_name = foehn)]
pub fn foehn(
    status: u16,
    body: Vec<u8>,
    places: Vec<Ts<Coordinate>>,
    now_unix_s: f64,
) -> Result<Ts<FoehnView>, JsValue> {
    let places = places.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
    to_js(&venti_core::foehn(status, body, places, whole_seconds(now_unix_s)).map_err(thrown)?)
}

/// The steps of the map's filter of airspace by height, and what to say
/// with them.
#[wasm_bindgen(js_name = airspaceHeightFilter)]
pub fn airspace_height_filter() -> Result<Ts<AirspaceHeightFilter>, JsValue> {
    to_js(&venti_core::airspace_height_filter())
}

/// The path and query to `GET` from the server for the airspace around a
/// place, to keep in the browser.
#[wasm_bindgen(js_name = serverAirspacePath)]
pub fn server_airspace_path(lat: f64, lon: f64) -> Result<String, JsValue> {
    venti_core::server_airspace_path(lat, lon).map_err(thrown)
}

// Flying sites from the Venti server: take-offs, landings, winch sites and
// training slopes for the map. Like the wind, they are asked for and read.
// Nothing of them is kept among the core's files but the sites the pilot
// marks as their own, through `Core.keepSite`.

/// The path and query to `GET` for the flying sites of a box.
#[wasm_bindgen(js_name = sitesPath)]
pub fn sites_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, JsValue> {
    venti_core::sites_path(south, west, north, east).map_err(thrown)
}

/// The path and query to `GET` for the flying sites of a box.
#[wasm_bindgen(js_name = sitesPathWithPilots)]
pub fn sites_path_with_pilots(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
) -> Result<String, JsValue> {
    venti_core::sites_path_with_pilots(south, west, north, east).map_err(thrown)
}

/// Reads the server's answer to `sitesPath` for the same box.
#[wasm_bindgen(js_name = sitesMap)]
pub fn sites_map(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
    status: u16,
    body: Vec<u8>,
) -> Result<Ts<SitesMap>, JsValue> {
    to_js(&venti_core::sites_map(south, west, north, east, status, body).map_err(thrown)?)
}

/// What the forecast says about flying from a take-off: reads the server's
/// answer to `forecastPath` for the take-off's place. `wind` is where the
/// take-off works, as its source says; `elevation_m` its height, if known;
/// `utc_offset_s` how far the place's clock is ahead of UTC; hours before
/// `now_unix_s`, by Venti's clock, are left out.
#[wasm_bindgen(js_name = siteOutlook)]
pub fn site_outlook(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    wind: Vec<Ts<WindSpan>>,
    elevation_m: Option<f64>,
    now_unix_s: f64,
) -> Result<Ts<SiteOutlook>, JsValue> {
    let wind = wind.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
    to_js(
        &venti_core::site_outlook(
            status,
            body,
            utc_offset_s,
            wind,
            elevation_m,
            whole_seconds(now_unix_s),
        )
        .map_err(thrown)?,
    )
}

/// The path and query to `GET` for the flying sites of a box as a filter
/// asks for them: with the places from pilots where the filter has them
/// on, without them otherwise.
#[wasm_bindgen(js_name = sitesPathFor)]
pub fn sites_path_for(
    filter: Ts<SitesFilter>,
    south: f64,
    west: f64,
    north: f64,
    east: f64,
) -> Result<String, JsValue> {
    venti_core::sites_path_for(from_js(&filter)?, south, west, north, east).map_err(thrown)
}

/// The choices of the filter of sites, and what to say with them.
#[wasm_bindgen(js_name = sitesFilterChoices)]
pub fn sites_filter_choices() -> Result<Ts<SitesFilterChoices>, JsValue> {
    to_js(&venti_core::sites_filter_choices())
}

/// The sites a filter leaves of those handed in: the ones that are what
/// was asked for, the take-offs whose source does not say and that are
/// shown apart, and the words for both.
#[wasm_bindgen(js_name = filterSites)]
pub fn filter_sites(
    sites: Vec<Ts<SiteView>>,
    filter: Ts<SitesFilter>,
) -> Result<Ts<SitesFiltered>, JsValue> {
    let sites = sites.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
    to_js(&venti_core::filter_sites(sites, from_js(&filter)?))
}

// Adding and correcting a site of Venti's own layer. The core holds the
// rules: what a site has to be, what the form says, and what each answer
// of the server means to a pilot.

/// What a form for a site shows: its fields with their titles and helps,
/// the choices, the limits as the server has them, and the words around
/// the act of giving.
#[wasm_bindgen(js_name = siteForm)]
pub fn site_form() -> Result<Ts<SiteForm>, JsValue> {
    to_js(&venti_core::site_form())
}

/// A draft for a new site at a place, with nothing said yet.
#[wasm_bindgen(js_name = siteDraftNew)]
pub fn site_draft_new(place: Ts<Coordinate>) -> Result<Ts<SiteDraftView>, JsValue> {
    to_js(&venti_core::site_draft_new(from_js(&place)?))
}

/// A draft that corrects a site of Venti's own layer: everything the
/// site says, and the revision it stands at. Refuses a source's site.
#[wasm_bindgen(js_name = siteDraftOf)]
pub fn site_draft_of(site: Ts<SiteView>) -> Result<Ts<SiteDraftView>, JsValue> {
    to_js(&venti_core::site_draft_of(from_js(&site)?).map_err(thrown)?)
}

/// The draft with the site at another place. An outline goes with the
/// old place.
#[wasm_bindgen(js_name = siteDraftMoved)]
pub fn site_draft_moved(
    draft: Ts<SiteDraftView>,
    place: Ts<Coordinate>,
) -> Result<Ts<SiteDraftView>, JsValue> {
    to_js(&venti_core::site_draft_moved(
        from_js(&draft)?,
        from_js(&place)?,
    ))
}

/// What is wrong with a draft, field by field, by the server's own check.
/// Empty when it can be sent.
#[wasm_bindgen(js_name = siteDraftCheck)]
pub fn site_draft_check(
    draft: Ts<SiteDraftView>,
    comment: String,
) -> Result<Vec<Ts<SiteDraftFault>>, JsValue> {
    venti_core::site_draft_check(from_js(&draft)?, comment)
        .iter()
        .map(to_js)
        .collect()
}

/// The compass points a draft has ticked, in words: "SW to NW".
#[wasm_bindgen(js_name = siteDraftWindInWords)]
pub fn site_draft_wind_in_words(wind_from_deg: Vec<f64>) -> String {
    venti_core::site_draft_wind_in_words(wind_from_deg)
}

/// The request that adds a site, for a signed-in pilot.
#[wasm_bindgen(js_name = siteAddRequest)]
pub fn site_add_request(
    draft: Ts<SiteDraftView>,
    comment: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::site_add_request(from_js(&draft)?, comment).map_err(thrown)?)
}

/// The request that corrects a site, made from the revision the draft
/// names.
#[wasm_bindgen(js_name = siteChangeRequest)]
pub fn site_change_request(
    draft: Ts<SiteDraftView>,
    comment: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::site_change_request(from_js(&draft)?, comment).map_err(thrown)?)
}

/// The request that removes a site, as a new revision. Its history stays.
#[wasm_bindgen(js_name = siteRemoveRequest)]
pub fn site_remove_request(id: String, comment: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::site_remove_request(id, comment).map_err(thrown)?)
}

/// The request that puts a site back to what one of its revisions said:
/// how a change is undone, and how a removed site comes back.
#[wasm_bindgen(js_name = siteRestoreRequest)]
pub fn site_restore_request(
    id: String,
    revision: u32,
    comment: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::site_restore_request(id, revision, comment).map_err(thrown)?)
}

/// The path to `GET` for one site by its identifier.
#[wasm_bindgen(js_name = sitePath)]
pub fn site_path(id: String) -> Result<String, JsValue> {
    venti_core::site_path(id).map_err(thrown)
}

/// The path to `GET` for a page of a site's history, after revision
/// `after` or from the first.
#[wasm_bindgen(js_name = siteHistoryPath)]
pub fn site_history_path(id: String, after: Option<u32>) -> Result<String, JsValue> {
    venti_core::site_history_path(id, after).map_err(thrown)
}

/// Reads the server's answer to `sitePath`: one site as it stands.
#[wasm_bindgen(js_name = sitePage)]
pub fn site_page(status: u16, body: Vec<u8>) -> Result<Ts<SitePage>, JsValue> {
    to_js(&venti_core::site_page(status, body).map_err(thrown)?)
}

/// Reads the server's answer to `siteAddRequest`, `siteChangeRequest`
/// and `siteRestoreRequest`: the site as it stands now, and what became
/// of the request in a line. A refusal is thrown in words for the pilot.
#[wasm_bindgen(js_name = siteWritten)]
pub fn site_written(status: u16, body: Vec<u8>) -> Result<Ts<SiteWritten>, JsValue> {
    to_js(&venti_core::site_written(status, body).map_err(thrown)?)
}

/// Reads the server's answer to `siteRemoveRequest` and says in a line
/// what became of it.
#[wasm_bindgen(js_name = siteRemoved)]
pub fn site_removed(status: u16, body: Vec<u8>) -> Result<String, JsValue> {
    venti_core::site_removed(status, body).map_err(thrown)
}

/// The site that stands in the way, from the answer to a request that
/// was refused because another site stands at the place. Nothing for
/// every other answer.
#[wasm_bindgen(js_name = siteInTheWay)]
pub fn site_in_the_way(status: u16, body: Vec<u8>) -> Result<Option<Ts<SiteInTheWay>>, JsValue> {
    venti_core::site_in_the_way(status, body)
        .as_ref()
        .map(to_js)
        .transpose()
}

/// Reads the server's answer to `siteHistoryPath`: a page of a site's
/// revisions, each in words, with what it changed.
#[wasm_bindgen(js_name = siteHistory)]
pub fn site_history(status: u16, body: Vec<u8>) -> Result<Ts<SiteHistory>, JsValue> {
    to_js(&venti_core::site_history(status, body).map_err(thrown)?)
}

/// The same for a page after the first: `before` is the last revision of
/// the page before, so that this page's first revision says what it
/// changed. The page reads a history's pages in order.
#[wasm_bindgen(js_name = siteHistoryPage)]
pub fn site_history_page(
    status: u16,
    body: Vec<u8>,
    before: Option<Ts<SiteRevisionView>>,
) -> Result<Ts<SiteHistory>, JsValue> {
    let before = before.as_ref().map(from_js).transpose()?;
    to_js(&venti_core::site_history_page(status, body, before).map_err(thrown)?)
}

/// Whether the server refused a correction because someone else changed
/// the site meanwhile (`site_changed`).
#[wasm_bindgen(js_name = siteWasChanged)]
pub fn site_was_changed(status: u16, body: Vec<u8>) -> bool {
    venti_core::site_was_changed(status, body)
}

/// Carries a draft onto a site that someone else changed while the pilot
/// was editing it: `base` is the site the draft was made from, `edited`
/// the draft as the pilot left it, `now` the site as it stands.
#[wasm_bindgen(js_name = siteDraftRebased)]
pub fn site_draft_rebased(
    base: Ts<SiteView>,
    edited: Ts<SiteDraftView>,
    now: Ts<SiteView>,
) -> Result<Ts<SiteDraftRebased>, JsValue> {
    let rebased =
        venti_core::site_draft_rebased(from_js(&base)?, from_js(&edited)?, from_js(&now)?)
            .map_err(thrown)?;
    to_js(&rebased)
}

/// The words of a site's page, its form and its history that are the
/// same in the three apps.
#[wasm_bindgen(js_name = siteWords)]
pub fn site_words() -> Result<Ts<SiteWords>, JsValue> {
    to_js(&venti_core::site_words())
}

/// The form for a confirmation: the days to choose from and the words.
/// `today` is the browser's date as `YYYY-MM-DD`.
#[wasm_bindgen(js_name = siteConfirmForm)]
pub fn site_confirm_form(today: String) -> Result<Ts<SiteConfirmForm>, JsValue> {
    to_js(&venti_core::site_confirm_form(today).map_err(thrown)?)
}

/// The request that takes the pilot's own confirmation of a site back.
#[wasm_bindgen(js_name = siteWithdrawRequest)]
pub fn site_withdraw_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::site_withdraw_request(id).map_err(thrown)?)
}

/// Reads the server's answer to `Core.siteConfirmRequest`: the site as it
/// stands, and what to say after. `day` is the day that was sent.
#[wasm_bindgen(js_name = siteConfirmed)]
pub fn site_confirmed(status: u16, body: Vec<u8>, day: String) -> Result<Ts<SiteWritten>, JsValue> {
    to_js(&venti_core::site_confirmed(status, body, day).map_err(thrown)?)
}

/// Reads the server's answer to `siteWithdrawRequest` and says what
/// became of it.
#[wasm_bindgen(js_name = siteWithdrawn)]
pub fn site_withdrawn(status: u16, body: Vec<u8>) -> Result<String, JsValue> {
    venti_core::site_withdrawn(status, body).map_err(thrown)
}

/// The path to `GET` for what pilots confirm of a site.
#[wasm_bindgen(js_name = siteConfirmationsPath)]
pub fn site_confirmations_path(id: String) -> Result<String, JsValue> {
    venti_core::site_confirmations_path(id).map_err(thrown)
}

/// Reads the server's answer to `siteConfirmationsPath`: who says they
/// took off at the site, each as a line.
#[wasm_bindgen(js_name = siteConfirmations)]
pub fn site_confirmations(status: u16, body: Vec<u8>) -> Result<Ts<SiteConfirmations>, JsValue> {
    to_js(&venti_core::site_confirmations(status, body).map_err(thrown)?)
}

/// Reads the server's answer to `siteConfirmationsPath` for the site's
/// page: as `siteConfirmations`, and without the line that nobody took
/// off here for a site nobody takes off at.
#[wasm_bindgen(js_name = siteConfirmationsOf)]
pub fn site_confirmations_of(
    site: Ts<SiteView>,
    status: u16,
    body: Vec<u8>,
) -> Result<Ts<SiteConfirmations>, JsValue> {
    to_js(&venti_core::site_confirmations_of(from_js(&site)?, status, body).map_err(thrown)?)
}

/// A route drawn on the map before the flight: each leg's length, the
/// whole, and what it would score if flown as drawn. The points come as
/// one flat list, latitude then longitude of each in turn.
#[wasm_bindgen(js_name = planRoute)]
pub fn plan_route(lat_lon: Vec<f64>) -> Result<Ts<venti_core::RoutePlan>, JsValue> {
    let points = lat_lon
        .chunks_exact(2)
        .map(|pair| Coordinate {
            lat: pair[0],
            lon: pair[1],
        })
        .collect();
    to_js(&venti_core::plan_route(points))
}

/// A drawn route as a GPX 1.1 file: one route with a named point for
/// each place, Venti as its creator and nothing of the pilot's. The
/// points come as one flat list, latitude then longitude of each in turn.
#[wasm_bindgen(js_name = routeToGpx)]
pub fn route_to_gpx(lat_lon: Vec<f64>) -> String {
    let points = lat_lon
        .chunks_exact(2)
        .map(|pair| Coordinate {
            lat: pair[0],
            lon: pair[1],
        })
        .collect();
    venti_core::route_to_gpx(points)
}

/// Reads a route for the planner from a GPX file's text: its route, else
/// its waypoints, else its track thinned to the planner's limit. A file
/// that holds no route is refused in words.
#[wasm_bindgen(js_name = routeFromGpx)]
pub fn route_from_gpx(text: String) -> Result<Ts<RouteFromFile>, JsValue> {
    to_js(&venti_core::route_from_gpx(text).map_err(thrown)?)
}

/// The path to `GET` for what the public flights that began at a site
/// say about it. A site of any layer.
#[wasm_bindgen(js_name = siteFlightsPath)]
pub fn site_flights_path(id: String) -> Result<String, JsValue> {
    venti_core::site_flights_path(id).map_err(thrown)
}

/// Reads the server's answer to `siteFlightsPath`: the count line, the
/// twelve months, the season and the best flights as lines.
#[wasm_bindgen(js_name = siteFlights)]
pub fn site_flights(status: u16, body: Vec<u8>) -> Result<Ts<SiteFlights>, JsValue> {
    to_js(&venti_core::site_flights(status, body).map_err(thrown)?)
}

/// The words around what flights say about a site: titles, and the
/// switch of the map's layer of candidates.
#[wasm_bindgen(js_name = siteFlightsWords)]
pub fn site_flights_words() -> Result<Ts<SiteFlightsWords>, JsValue> {
    to_js(&venti_core::site_flights_words())
}

/// The path and query to `GET` for the places in a part of the map where
/// pilots take off and no site stands. Refused in words for a part wider
/// or taller than 2 degrees.
#[wasm_bindgen(js_name = siteCandidatesPath)]
pub fn site_candidates_path(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
) -> Result<String, JsValue> {
    venti_core::site_candidates_path(south, west, north, east).map_err(thrown)
}

/// Reads the server's answer to `siteCandidatesPath`.
#[wasm_bindgen(js_name = siteCandidates)]
pub fn site_candidates(status: u16, body: Vec<u8>) -> Result<Ts<SiteCandidates>, JsValue> {
    to_js(&venti_core::site_candidates(status, body).map_err(thrown)?)
}

/// What the form says when it corrects a given site, by the site's tier:
/// the title, the sentence of giving, the notice and the button.
#[wasm_bindgen(js_name = siteCorrecting)]
pub fn site_correcting(site: Ts<SiteView>) -> Result<Ts<SiteCorrecting>, JsValue> {
    to_js(&venti_core::site_correcting(from_js(&site)?).map_err(thrown)?)
}

/// The path to `GET` for what happened to the sites of a tier, to find
/// the removed ones: the crowdsourced tier with `from_pilots`, else the
/// partners'. `since` is the cursor of the page before.
#[wasm_bindgen(js_name = sitesRemovedPath)]
pub fn sites_removed_path(from_pilots: bool, since: Option<String>) -> Result<String, JsValue> {
    venti_core::sites_removed_path(from_pilots, since).map_err(thrown)
}

/// What differs between a site as one revision said it and as another
/// did, a line per field.
#[wasm_bindgen(js_name = siteChangesBetween)]
pub fn site_changes_between(
    before: Ts<SiteView>,
    after: Ts<SiteView>,
) -> Result<Vec<String>, JsValue> {
    Ok(venti_core::site_changes_between(
        from_js(&before)?,
        from_js(&after)?,
    ))
}

/// The part of the map Today asks the server's sites for: a box that holds
/// every place within reach of the pilot's. Asked for with `sitesPath`,
/// read with `sitesMap`, handed to `Core.todayTakeoffs`.
#[wasm_bindgen(js_name = todayArea)]
pub fn today_area(lat: f64, lon: f64) -> Result<Ts<Bounds>, JsValue> {
    to_js(&venti_core::today_area(lat, lon).map_err(thrown)?)
}

/// Today's take-offs in the order to list them on a day, the pilot's own
/// first, with the one that looks best. Each take-off is handed in with
/// what its forecast says of the day shown.
#[wasm_bindgen(js_name = todayOrder)]
pub fn today_order(takeoffs: Vec<Ts<TodayStanding>>) -> Result<Ts<TodayOrder>, JsValue> {
    let takeoffs = takeoffs
        .iter()
        .map(from_js)
        .collect::<Result<Vec<_>, _>>()?;
    to_js(&venti_core::today_order(takeoffs))
}

// The thermal map: the kind of day, square by square, over the area the
// server has forecasts for.

/// The path to `GET` for the list of rain radar frames.
#[wasm_bindgen(js_name = radarPath)]
pub fn radar_path() -> String {
    venti_core::radar_path()
}

/// Reads the server's answer to `radarPath`: the radar's layers, one for
/// each source, each with its frames, their times and their ages in
/// words; the key of the steps of rain; and what a radar does not see.
/// `utc_offset_s` is how far the pilot's clock is ahead of UTC and
/// `now_unix_s` the moment by the device's clock, against which the ages
/// are counted.
#[wasm_bindgen(js_name = radar)]
pub fn radar(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: f64,
) -> Result<Ts<RadarView>, JsValue> {
    to_js(
        &venti_core::radar(status, body, utc_offset_s, whole_seconds(now_unix_s))
            .map_err(thrown)?,
    )
}

/// A frame's picture in the page's own colours. `picture` is what the
/// server sent for a frame's `path`, a PNG whose pixels are steps;
/// `rain` are the colours of the six steps of rain, the first step
/// first, and `unseen` the colour of what the radar did not see, each as
/// `0xRRGGBBAA`. The answer is the same picture with those colours, for
/// a `Blob` of type `image/png`.
#[wasm_bindgen(js_name = radarFrameImage)]
pub fn radar_frame_image(
    picture: Vec<u8>,
    rain: Vec<u32>,
    unseen: u32,
) -> Result<Vec<u8>, JsValue> {
    venti_core::radar_frame_image(picture, rain, unseen).map_err(thrown)
}

/// A frame's picture in the page's own colours, for a picture that lies
/// under others at a moment: as `radarFrameImage` colours it, but clear
/// at every pixel that one of the pictures `above` sees, rain or dry.
/// `above` are the pictures of the frames after this one in the moment's
/// `frames`, each as the server sent it; with none, the answer is
/// `radarFrameImage`'s.
#[wasm_bindgen(js_name = radarFrameImageUnder)]
pub fn radar_frame_image_under(
    picture: Vec<u8>,
    above: Vec<js_sys::Uint8Array>,
    rain: Vec<u32>,
    unseen: u32,
) -> Result<Vec<u8>, JsValue> {
    let above = above.iter().map(js_sys::Uint8Array::to_vec).collect();
    venti_core::radar_frame_image_under(picture, above, rain, unseen).map_err(thrown)
}

/// The path to `GET` for the thermal map.
#[wasm_bindgen(js_name = thermalMapPath)]
pub fn thermal_map_path() -> String {
    venti_core::thermal_map_path()
}

/// Reads the server's answer to `thermalMapPath`. `utc_offset_s` is how
/// far the pilot's clock is ahead of UTC and `now_unix_s` the moment by
/// Venti's clock: they decide which day is called today, and days before
/// it are left out.
#[wasm_bindgen(js_name = thermalMap)]
pub fn thermal_map(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: f64,
) -> Result<Ts<ThermalMap>, JsValue> {
    to_js(
        &venti_core::thermal_map(status, body, utc_offset_s, whole_seconds(now_unix_s))
            .map_err(thrown)?,
    )
}

/// A square's days side by side: every day from today on that the map
/// has, with what it says of the square a place lies in, for the sheet of
/// a square the pilot chose. `status` and `body` are the answer
/// `thermalMap` read, which the page keeps. Empty for a place outside the
/// map.
#[wasm_bindgen(js_name = thermalMapStrip)]
pub fn thermal_map_strip(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
    now_unix_s: f64,
    lat: f64,
    lon: f64,
) -> Result<Vec<Ts<ThermalStripDay>>, JsValue> {
    venti_core::thermal_map_strip(
        status,
        body,
        utc_offset_s,
        whole_seconds(now_unix_s),
        lat,
        lon,
    )
    .map_err(thrown)?
    .iter()
    .map(to_js)
    .collect()
}

/// The path and query to `GET` for one layer of the hours of a day of the
/// thermal map. `date` is the day as the map names it, `height_m` the
/// level the wind is wanted at: left out for the wind at the ground, and
/// of no account to the other layers. The wind at the ground and the gusts
/// are one answer, asked for by the same path.
#[wasm_bindgen(js_name = thermalHoursPath)]
pub fn thermal_hours_path(
    date: String,
    layer: Ts<ThermalHoursLayer>,
    height_m: Option<i32>,
) -> Result<String, JsValue> {
    venti_core::thermal_hours_path(date, from_js(&layer)?, height_m).map_err(thrown)
}

/// One layer of the hours of a day of the thermal map: the server's
/// answer, read once and asked hour by hour as the pilot moves along the
/// line of hours. It holds the answer, some hundreds of kilobytes for a
/// region, so the page frees it when it shows another.
#[wasm_bindgen]
#[derive(Debug)]
pub struct ThermalMapHours {
    hours: Arc<venti_core::ThermalMapHours>,
}

#[wasm_bindgen]
impl ThermalMapHours {
    /// Reads the server's answer to `thermalHoursPath` for `layer`.
    /// `utc_offset_s` is how far the pilot's clock is ahead of UTC, which
    /// labels the hours, and `now_unix_s` the moment by Venti's clock,
    /// which decides the hour to show first. Throws, in words for the
    /// pilot, when the answer is a refusal or cannot be read.
    #[wasm_bindgen(constructor)]
    pub fn new(
        status: u16,
        body: Vec<u8>,
        layer: Ts<ThermalHoursLayer>,
        utc_offset_s: i32,
        now_unix_s: f64,
    ) -> Result<ThermalMapHours, JsValue> {
        Ok(Self {
            hours: venti_core::ThermalMapHours::read(
                status,
                body,
                from_js(&layer)?,
                utc_offset_s,
                whole_seconds(now_unix_s),
            )
            .map_err(thrown)?,
        })
    }

    /// What a card says of the hours: the stops of the line of hours, the
    /// hour to show first, the model and its run.
    pub fn view(&self) -> Result<Ts<ThermalMapHoursView>, JsValue> {
        to_js(&self.hours.view())
    }

    /// What the map draws of one hour, by the hour's number in the view's
    /// `hours`: two GeoJSON texts, the squares and the arrows. A map's
    /// source takes an object, so the page parses each.
    pub fn shapes(&self, hour: u32) -> Result<Ts<ThermalMapShapes>, JsValue> {
        to_js(&self.hours.shapes(hour))
    }

    /// The number of the square a place lies in: what a click on the map
    /// is turned into, also where no square is drawn. Nothing outside the
    /// map.
    #[wasm_bindgen(js_name = cellAt)]
    pub fn cell_at(&self, lat: f64, lon: f64) -> Option<u32> {
        self.hours.cell_at(lat, lon)
    }

    /// A square in one hour, in words. Nothing for a square or an hour
    /// there is none of.
    pub fn square(
        &self,
        cell: u32,
        hour: u32,
    ) -> Result<Option<Ts<ThermalMapHourSquare>>, JsValue> {
        self.hours
            .square(cell, hour)
            .as_ref()
            .map(to_js)
            .transpose()
    }
}

// The forecast for a place, from the Venti server: what its weather models
// compute, one beside the next. Asked for and read like the wind.

/// The path and query to `GET` for the forecast of a place.
#[wasm_bindgen(js_name = forecastPath)]
pub fn forecast_path(lat: f64, lon: f64) -> Result<String, JsValue> {
    venti_core::forecast_path(lat, lon).map_err(thrown)
}

/// Reads the server's answer to `forecastPath`. `utc_offset_s` is how far
/// the clock at the place is ahead of UTC, in seconds: the days of the
/// forecast are that clock's days.
#[wasm_bindgen]
pub fn forecast(
    status: u16,
    body: Vec<u8>,
    utc_offset_s: i32,
) -> Result<Ts<ForecastView>, JsValue> {
    to_js(&venti_core::forecast(status, body, utc_offset_s).map_err(thrown)?)
}

// Who is in the air: the gliders whose pilots show their flight to everyone,
// from the Venti server. Asked for and read like the wind, and asked again
// every quarter of a minute while a map shows them.

/// The path and query to `GET` for the gliders in a box.
#[wasm_bindgen(js_name = liveMapPath)]
pub fn live_map_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, JsValue> {
    venti_core::live_map_path(south, west, north, east).map_err(thrown)
}

/// Reads the server's answer to `liveMapPath`. Each glider's age is by the
/// server's own clock at the moment of the answer; the page lets its clock
/// run on from there.
#[wasm_bindgen(js_name = liveMap)]
pub fn live_map(status: u16, body: Vec<u8>) -> Result<Ts<LiveMap>, JsValue> {
    to_js(&venti_core::live_map(status, body).map_err(thrown)?)
}

/// How old a glider's position is, in words: "Now", "40 s ago", "Landed
/// 12 min ago", "No position for 6 min".
#[wasm_bindgen(js_name = liveAgeInWords)]
pub fn live_age_in_words(age_s: f64, landed: bool) -> String {
    venti_core::live_age_in_words(age_s, landed)
}

// Others in the air: the aircraft another network hears (the Open Glider
// Network), never one record with Venti's own pilots.

/// The path and query to `GET` for the aircraft other networks hear in a
/// box, or nothing for a box wider than the server answers for.
#[wasm_bindgen(js_name = liveOthersPath)]
pub fn live_others_path(south: f64, west: f64, north: f64, east: f64) -> Option<String> {
    venti_core::live_others_path(south, west, north, east)
}

/// Reads the server's answer to `liveOthersPath`: each network's aircraft
/// with words, and its credit.
#[wasm_bindgen(js_name = liveOthers)]
pub fn live_others(status: u16, body: Vec<u8>) -> Result<Ts<LiveOthers>, JsValue> {
    to_js(&venti_core::live_others(status, body).map_err(thrown)?)
}

/// The map's layer of who is in the air from the two answers, of
/// `liveMapPath` and of `liveOthersPath`: two lists side by side, each with
/// its credit.
#[wasm_bindgen(js_name = liveLayer)]
pub fn live_layer(
    venti_status: u16,
    venti_body: Vec<u8>,
    others_status: u16,
    others_body: Vec<u8>,
) -> Result<Ts<LiveLayer>, JsValue> {
    let venti = venti_core::live_map(venti_status, venti_body).map_err(thrown)?;
    let others = venti_core::live_others(others_status, others_body).map_err(thrown)?;
    to_js(&venti_core::live_layer(venti, others))
}

// What many gliders say together: where pilots climb now, how many are
// in the air at a site, and where the public flights' thermals have been.
// Places in the air and numbers; no pilot is named.

/// The path and query to `GET` for the climbs of the last minutes in a
/// box, or nothing for a box wider than the server answers for.
#[wasm_bindgen(js_name = liveClimbsPath)]
pub fn live_climbs_path(south: f64, west: f64, north: f64, east: f64) -> Option<String> {
    venti_core::live_climbs_path(south, west, north, east)
}

/// Reads the server's answer to `liveClimbsPath`: each climb with its
/// words, and the credits of the networks it is derived from.
#[wasm_bindgen(js_name = liveClimbs)]
pub fn live_climbs(status: u16, body: Vec<u8>) -> Result<Ts<LiveClimbs>, JsValue> {
    to_js(&venti_core::live_climbs(status, body).map_err(thrown)?)
}

/// What the map says where the climbs are switched on and there are none.
#[wasm_bindgen(js_name = liveClimbsEmptyWords)]
pub fn live_climbs_empty_words() -> String {
    venti_core::live_climbs_empty_words()
}

/// The path and query to `GET` for the sites of a box with pilots in the
/// air, or nothing for a box wider than the server answers for.
#[wasm_bindgen(js_name = liveSitesPath)]
pub fn live_sites_path(south: f64, west: f64, north: f64, east: f64) -> Option<String> {
    venti_core::live_sites_path(south, west, north, east)
}

/// Reads the server's answer to `liveSitesPath`: the pilots in the air
/// at each site, the sources of live positions not added up.
#[wasm_bindgen(js_name = liveSites)]
pub fn live_sites(status: u16, body: Vec<u8>) -> Result<Ts<SitesAir>, JsValue> {
    to_js(&venti_core::live_sites(status, body).map_err(thrown)?)
}

/// The line a site's page says of who flies there now, from the answer
/// to `liveSitesPath`: "5 pilots in the air at Wallberg, 2 took off in
/// the last hour". Empty when nobody does. `siteName` is the site's own
/// name or empty: without one the line says "here".
#[wasm_bindgen(js_name = siteAirWords)]
pub fn site_air_words(
    status: u16,
    body: Vec<u8>,
    site_id: String,
    site_name: String,
) -> Result<String, JsValue> {
    let air = venti_core::live_sites(status, body).map_err(thrown)?;
    Ok(venti_core::site_air_words(air, site_id, site_name))
}

/// The path and query to `GET` for where thermals have been in a box, in
/// a month of the year and between two hours by the sun, if given; or
/// nothing for a box wider than a degree.
#[wasm_bindgen(js_name = flownThermalsPath)]
pub fn flown_thermals_path(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
    month: Option<u8>,
    from_hour: Option<u8>,
    to_hour: Option<u8>,
) -> Option<String> {
    venti_core::flown_thermals_path(south, west, north, east, month, from_hour, to_hour)
}

/// What a map says where it shows too much to ask for the thermals.
#[wasm_bindgen(js_name = flownTooWideWords)]
pub fn flown_too_wide_words() -> String {
    venti_core::flown_too_wide_words()
}

/// Reads the server's answer to `flownThermalsPath`: each cell with its
/// words, and what the layer is.
#[wasm_bindgen(js_name = flownThermals)]
pub fn flown_thermals(status: u16, body: Vec<u8>) -> Result<Ts<FlownThermals>, JsValue> {
    to_js(&venti_core::flown_thermals(status, body).map_err(thrown)?)
}

/// Follows one live track: says what to ask the server for, and puts the
/// answers together. The page asks once, then again every five to ten
/// seconds, and at once while the answer says there is more. It holds every
/// position the server handed out, so the page frees it when it stops
/// following.
#[wasm_bindgen]
#[derive(Debug)]
pub struct LiveFollow {
    follow: Arc<venti_core::LiveFollow>,
}

#[wasm_bindgen]
impl LiveFollow {
    /// Begins to follow the track with this identifier. Throws when the
    /// text is not the identifier of a track.
    #[wasm_bindgen(constructor)]
    pub fn new(id: String) -> Result<LiveFollow, JsValue> {
        Ok(Self {
            follow: venti_core::LiveFollow::new(id).map_err(thrown)?,
        })
    }

    /// The path and query to `GET` now: the whole track the first time,
    /// afterwards what arrived since the last answer.
    pub fn path(&self) -> String {
        self.follow.path()
    }

    /// Reads the server's answer to `path` and returns the flight as far
    /// as it is known. Throws, in words for the pilot, when the track is
    /// gone; `isOver` is true from then on.
    pub fn receive(&self, status: u16, body: Vec<u8>) -> Result<Ts<LiveFollowView>, JsValue> {
        to_js(&self.follow.receive(status, body).map_err(thrown)?)
    }

    /// Whether the track is gone from the server: stop asking.
    #[wasm_bindgen(js_name = isOver)]
    pub fn is_over(&self) -> bool {
        self.follow.is_over()
    }
}

/// The height of the ground, read from the map's own elevation tiles: the
/// ones the map shades its relief from.
///
/// The page asks which tiles cover a flight, fetches them from the map's
/// tile server and hands them over as they came; the core reads the pixels
/// and says how high the ground was under the flight. It holds the heights
/// of every tile it was given, so the page frees it once it has its
/// answer.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct Terrain {
    terrain: Arc<venti_core::Terrain>,
}

#[wasm_bindgen]
impl Terrain {
    /// A terrain that knows no ground yet.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// The tiles to have for the ground under an area, such as the bounds
    /// of a flight, without those that are held already: at the finest
    /// zoom level at which the area needs no more than a few dozen. Empty
    /// when all are there, and for an area that is none.
    #[wasm_bindgen(js_name = tilesCovering)]
    pub fn tiles_covering(
        &self,
        south: f64,
        west: f64,
        north: f64,
        east: f64,
    ) -> Result<Vec<Ts<TerrainTile>>, JsValue> {
        let area = Bounds {
            south,
            west,
            north,
            east,
        };
        self.terrain
            .tiles_covering(area)
            .iter()
            .map(to_js)
            .collect()
    }

    /// Takes one tile as the tile server sends it: a WebP image in the
    /// Terrarium encoding. Throws when the bytes are no such image.
    #[wasm_bindgen(js_name = addTile)]
    pub fn add_tile(&self, z: u8, x: u32, y: u32, image: Vec<u8>) -> Result<(), JsValue> {
        self.terrain.add_tile(z, x, y, image).map_err(thrown)
    }

    /// The ground under a flight's track, at up to `places` moments spread
    /// evenly over it, in order. A moment whose place no tile covers is
    /// left out.
    #[wasm_bindgen(js_name = groundUnder)]
    pub fn ground_under(
        &self,
        track: Vec<Ts<TrackPoint>>,
        places: u32,
    ) -> Result<Vec<Ts<GroundUnderFlight>>, JsValue> {
        let track = track.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
        self.terrain
            .ground_under(track, places)
            .iter()
            .map(to_js)
            .collect()
    }
}

// A planned route with the weather along it: the page asks the server
// for the thermal map's hours at the route's points, reads the answer
// once into a `RouteForecast`, and asks its `along` again whenever the
// pilot changes the start, the speed or the glide; `routeAlong` is the
// same without a forecast.

/// The path and query to `GET` for the forecast along a route: its places
/// and the day, as the thermal map names it. At most 25 places. Throws,
/// in words for the pilot, for a route or a day the server cannot be
/// asked for.
#[wasm_bindgen(js_name = routeWeatherPath)]
pub fn route_weather_path(places: Vec<Ts<RoutePlace>>, date: String) -> Result<String, JsValue> {
    let places = places.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
    venti_core::route_weather_path(places, date).map_err(thrown)
}

/// The server's answer of the forecast along a route, read once.
#[wasm_bindgen]
#[derive(Debug)]
pub struct RouteForecast {
    forecast: Arc<venti_core::RouteForecast>,
}

#[wasm_bindgen]
impl RouteForecast {
    /// Reads the server's answer to `routeWeatherPath`. Throws, in words
    /// for the pilot, when the answer is a refusal or cannot be read.
    #[wasm_bindgen(constructor)]
    pub fn new(status: u16, body: Vec<u8>) -> Result<RouteForecast, JsValue> {
        Ok(Self {
            forecast: venti_core::RouteForecast::read(status, body).map_err(thrown)?,
        })
    }

    /// A route as the pilot means to fly it, with this forecast: when the
    /// pilot is at each place, what the forecast has there then, each
    /// leg's wind, the height to leave each place at over the ground in
    /// `terrain` in that wind, and the whole route in a few sentences.
    /// The forecast and the terrain stay the page's, to ask again with
    /// when the pilot changes the start, the speed or the glide.
    pub fn along(
        &self,
        setting: Ts<RouteSetting>,
        terrain: &Terrain,
        utc_offset_s: i32,
    ) -> Result<Ts<RouteAlong>, JsValue> {
        to_js(&venti_core::route_along(
            from_js(&setting)?,
            Some(self.forecast.clone()),
            Some(terrain.terrain.clone()),
            utc_offset_s,
        ))
    }
}

/// A route as the pilot means to fly it, without a forecast: when the
/// pilot is at each place, and the height to leave each place at over the
/// ground in `terrain` in still air. A `Terrain` that holds no tiles
/// gives no heights, and says so. `RouteForecast.along` is the same with
/// the forecast's wind and weather. `utc_offset_s` is how far the pilot's
/// clock is ahead of UTC.
#[wasm_bindgen(js_name = routeAlong)]
pub fn route_along(
    setting: Ts<RouteSetting>,
    terrain: &Terrain,
    utc_offset_s: i32,
) -> Result<Ts<RouteAlong>, JsValue> {
    to_js(&venti_core::route_along(
        from_js(&setting)?,
        None,
        Some(terrain.terrain.clone()),
        utc_offset_s,
    ))
}

// What a glide reaches from a place on the map, and how steep the ground
// is: the page loads the tiles covering a box around the place into a
// `Terrain` first, and loads a wider box when the answer is `cutShort`.

/// What a straight glide reaches from a place on the map over the ground
/// in `terrain`: the area as a closed ring, each direction's distance and
/// arrival height, and the answer in words. Without a start height the
/// glide starts on the ground there.
#[wasm_bindgen(js_name = glideReach)]
pub fn glide_reach(
    setting: Ts<venti_core::GlideReachSetting>,
    terrain: &Terrain,
) -> Result<Ts<venti_core::GlideReach>, JsValue> {
    to_js(&venti_core::glide_reach(
        from_js(&setting)?,
        terrain.terrain.clone(),
    ))
}

/// How steep the ground in `terrain` is at each place, in the order
/// given, with its class. A place whose ground is not loaded has neither.
#[wasm_bindgen(js_name = slopeClasses)]
pub fn slope_classes(
    points: Vec<Ts<Coordinate>>,
    terrain: &Terrain,
) -> Result<Vec<Ts<venti_core::SlopeAt>>, JsValue> {
    let points = points.iter().map(from_js).collect::<Result<Vec<_>, _>>()?;
    venti_core::slope_classes(points, terrain.terrain.clone())
        .iter()
        .map(to_js)
        .collect()
}

// Other pilots' flights: the flights the pilots of the Venti server share
// with everyone, in one list, a pilot's own page of them, and one of them
// shown from its file like a flight of the logbook. Nothing of it is kept.

/// The path and query to `GET` for one page of everyone's public flights:
/// the newest first, or the best by points. `area` narrows the list to the
/// flights that took off inside a part of the map, as `flownFromHere`
/// names the part around a place. `from_unix_s` and `to_unix_s` narrow it
/// to flights that began at or after the one and before the other; a day
/// is its first second and the first second of the next. `after` is the
/// `next` of the page before, with the rest unchanged.
#[wasm_bindgen(js_name = publicFlightsPath)]
pub fn public_flights_path(
    order: Ts<PublicFlightsOrder>,
    area: Option<Ts<Bounds>>,
    from_unix_s: Option<f64>,
    to_unix_s: Option<f64>,
    after: Option<String>,
) -> Result<String, JsValue> {
    venti_core::public_flights_path(
        from_js(&order)?,
        area.as_ref().map(from_js).transpose()?,
        from_unix_s.map(whole_seconds),
        to_unix_s.map(whole_seconds),
        after,
    )
    .map_err(thrown)
}

/// The path and query to `GET` for one page of everyone's public flights
/// that a search finds: as `publicFlightsPath`, narrowed by `search` (a
/// circle or a drawn area the take-off lies in, the glider class, the kind
/// of route, the least distance, the take-off's country). The answer is
/// read by `publicFlights`.
#[wasm_bindgen(js_name = publicFlightsSearchPath)]
pub fn public_flights_search_path(
    order: Ts<PublicFlightsOrder>,
    area: Option<Ts<Bounds>>,
    from_unix_s: Option<f64>,
    to_unix_s: Option<f64>,
    search: Ts<FlightSearch>,
    after: Option<String>,
) -> Result<String, JsValue> {
    venti_core::public_flights_search_path(
        from_js(&order)?,
        area.as_ref().map(from_js).transpose()?,
        from_unix_s.map(whole_seconds),
        to_unix_s.map(whole_seconds),
        from_js(&search)?,
        after,
    )
    .map_err(thrown)
}

/// The same for one pilot's public flights.
#[wasm_bindgen(js_name = pilotFlightsSearchPath)]
pub fn pilot_flights_search_path(
    handle: String,
    order: Ts<PublicFlightsOrder>,
    from_unix_s: Option<f64>,
    to_unix_s: Option<f64>,
    search: Ts<FlightSearch>,
    after: Option<String>,
) -> Result<String, JsValue> {
    venti_core::pilot_flights_search_path(
        handle,
        from_js(&order)?,
        from_unix_s.map(whole_seconds),
        to_unix_s.map(whole_seconds),
        from_js(&search)?,
        after,
    )
    .map_err(thrown)
}

/// The path and query to `GET` for the ranking of the flights a search
/// finds: as `rankingPath`, narrowed by `search`.
#[wasm_bindgen(js_name = rankingSearchPath)]
pub fn ranking_search_path(
    area: Option<Ts<Bounds>>,
    from_unix_s: Option<f64>,
    to_unix_s: Option<f64>,
    search: Ts<FlightSearch>,
) -> Result<String, JsValue> {
    venti_core::ranking_search_path(
        area.as_ref().map(from_js).transpose()?,
        from_unix_s.map(whole_seconds),
        to_unix_s.map(whole_seconds),
        from_js(&search)?,
    )
    .map_err(thrown)
}

/// The words of the chips that show a search: "Within 10 km",
/// "Drawn area", "EN C", "FAI triangle", "50 km or more", "AT".
#[wasm_bindgen(js_name = flightSearchWords)]
pub fn flight_search_words(search: Ts<FlightSearch>) -> Result<Vec<String>, JsValue> {
    Ok(venti_core::flight_search_words(from_js(&search)?))
}

/// The bounds a search is held to: the radius, the corners of an area,
/// the least distance.
#[wasm_bindgen(js_name = flightSearchBounds)]
pub fn flight_search_bounds() -> Result<Ts<FlightSearchBounds>, JsValue> {
    to_js(&venti_core::flight_search_bounds())
}

/// The name of a kind of route in a choice of them: "Free flight",
/// "Flat triangle", "FAI triangle".
#[wasm_bindgen(js_name = routeKindWords)]
pub fn route_kind_words(kind: Ts<venti_core::RouteKind>) -> Result<String, JsValue> {
    Ok(venti_core::route_kind_words(from_js(&kind)?))
}

/// The path and query to `GET` for one page of the public flights that
/// began at a site, of any layer: those that took off within 500 m of it.
/// The answer is read by `publicFlights`.
#[wasm_bindgen(js_name = publicFlightsAtSitePath)]
pub fn public_flights_at_site_path(
    site_id: String,
    order: Ts<PublicFlightsOrder>,
    after: Option<String>,
) -> Result<String, JsValue> {
    venti_core::public_flights_at_site_path(site_id, from_js(&order)?, after).map_err(thrown)
}

/// The part of the map whose flights count as flown from a place: the
/// square around it, a kilometre and a half to each side.
#[wasm_bindgen(js_name = flownFromHere)]
pub fn flown_from_here(lat: f64, lon: f64) -> Result<Ts<Bounds>, JsValue> {
    to_js(&venti_core::area_around(lat, lon, venti_core::FLOWN_FROM_HERE_M).map_err(thrown)?)
}

/// The path and query to `GET` for the flying sites around a place, to
/// find the site of that place.
#[wasm_bindgen(js_name = siteAtPath)]
pub fn site_at_path(lat: f64, lon: f64) -> Result<String, JsValue> {
    venti_core::site_at_path(lat, lon).map_err(thrown)
}

/// Reads the server's answer to `siteAtPath` for the same place: the site
/// of identifier `id`, or without one the nearest take-off within a
/// kilometre, with whose data it is.
#[wasm_bindgen(js_name = siteAt)]
pub fn site_at(
    lat: f64,
    lon: f64,
    id: Option<String>,
    status: u16,
    body: Vec<u8>,
) -> Result<Ts<SiteAtPlace>, JsValue> {
    to_js(&venti_core::site_at(lat, lon, id, status, body).map_err(thrown)?)
}

/// Reads the server's answer to `publicFlightsPath`.
#[wasm_bindgen(js_name = publicFlights)]
pub fn public_flights(status: u16, body: Vec<u8>) -> Result<Ts<PublicFlights>, JsValue> {
    to_js(&venti_core::public_flights(status, body).map_err(thrown)?)
}

/// The path and query to `GET` for a pilot and one page of the flights
/// that pilot shares with everyone.
#[wasm_bindgen(js_name = pilotPagePath)]
pub fn pilot_page_path(handle: String, after: Option<String>) -> Result<String, JsValue> {
    venti_core::pilot_page_path(handle, after).map_err(thrown)
}

/// The path and query to `GET` for one page of a pilot's public flights
/// of a season: those that began at or after the one moment and before
/// the other, as a season of `pilotStatistics` names them. `after` is
/// the `next` of the page before. Read the answer with `publicFlights`.
#[wasm_bindgen(js_name = pilotFlightsPath)]
pub fn pilot_flights_path(
    handle: String,
    from_unix_s: Option<f64>,
    to_unix_s: Option<f64>,
    after: Option<String>,
) -> Result<String, JsValue> {
    venti_core::pilot_flights_path(
        handle,
        from_unix_s.map(whole_seconds),
        to_unix_s.map(whole_seconds),
        after,
    )
    .map_err(thrown)
}

/// The words around pilots, their totals and the ranking: the search
/// field, the headings of the totals, the season's choice.
#[wasm_bindgen(js_name = pilotsWords)]
pub fn pilots_words() -> Result<Ts<PilotsWords>, JsValue> {
    to_js(&venti_core::pilots_words())
}

/// Where the cockpit's climb comes from, in full and short for a narrow
/// top bar: the external instrument first, under `varioName`; in a
/// replay the recording; else the vario source.
#[wasm_bindgen(js_name = cockpitSourceWords)]
pub fn cockpit_source_words(
    vario_source: Option<Ts<VarioSourceKind>>,
    instrument: Option<Ts<InstrumentView>>,
    vario_name: Option<String>,
    is_replay: bool,
) -> Result<Ts<CockpitSourceWords>, JsValue> {
    let vario_source = vario_source.as_ref().map(from_js).transpose()?;
    let instrument = instrument.as_ref().map(from_js).transpose()?;
    to_js(&venti_core::cockpit_source_words(
        vario_source,
        instrument,
        vario_name,
        is_replay,
    ))
}

/// The words of the cockpit's details sheet: its title, its rows' names,
/// the empty line and the note, and the nearest landing's label.
#[wasm_bindgen(js_name = cockpitDetailsWords)]
pub fn cockpit_details_words() -> Result<Ts<CockpitDetailsWords>, JsValue> {
    to_js(&venti_core::cockpit_details_words())
}

/// The words of the settings' section about the glider.
#[wasm_bindgen(js_name = gliderWords)]
pub fn glider_words() -> Result<Ts<GliderWords>, JsValue> {
    to_js(&venti_core::glider_words())
}

/// A glider class as the menu names it; "None" without one.
#[wasm_bindgen(js_name = gliderClassName)]
pub fn glider_class_name(class: Option<Ts<GliderClassKind>>) -> Result<String, JsValue> {
    let class = class.as_ref().map(from_js).transpose()?;
    Ok(venti_core::glider_class_name(class))
}

/// The words of a site's sheet that no reading carries.
#[wasm_bindgen(js_name = siteSheetWords)]
pub fn site_sheet_words() -> Result<Ts<SiteSheetWords>, JsValue> {
    to_js(&venti_core::site_sheet_words())
}

/// The words of a station's sheet that no reading carries.
#[wasm_bindgen(js_name = stationSheetWords)]
pub fn station_sheet_words() -> Result<Ts<StationSheetWords>, JsValue> {
    to_js(&venti_core::station_sheet_words())
}

/// The words of the map's sheet of layers.
#[wasm_bindgen(js_name = mapLayersWords)]
pub fn map_layers_words() -> Result<Ts<MapLayersWords>, JsValue> {
    to_js(&venti_core::map_layers_words())
}

/// The words of the Today tab that no reading carries: the notes under
/// the strip of hours.
#[wasm_bindgen(js_name = todayWords)]
pub fn today_words() -> Result<Ts<TodayWords>, JsValue> {
    to_js(&venti_core::today_words())
}

/// The words of the route planner's menu.
#[wasm_bindgen(js_name = plannerWords)]
pub fn planner_words() -> Result<Ts<PlannerWords>, JsValue> {
    to_js(&venti_core::planner_words())
}

/// The words of the controls every screen has: "Done", "Back", "More",
/// "OK", "Cancel", "Try again", "Close".
#[wasm_bindgen(js_name = commonWords)]
pub fn common_words() -> Result<Ts<CommonWords>, JsValue> {
    to_js(&venti_core::common_words())
}

/// The words of pages that have only a few of their own: everyone's
/// flights and its filter, a forecast's notes, the settings, the alarms.
#[wasm_bindgen(js_name = pagesWords)]
pub fn pages_words() -> Result<Ts<PagesWords>, JsValue> {
    to_js(&venti_core::pages_words())
}

/// The words around following, cheers, comments and notifications that
/// no answer carries.
#[wasm_bindgen(js_name = socialWords)]
pub fn social_words() -> Result<Ts<SocialWords>, JsValue> {
    to_js(&venti_core::social_words())
}

/// Reads the server's answer to `pilotPagePath`.
#[wasm_bindgen(js_name = pilotPage)]
pub fn pilot_page(status: u16, body: Vec<u8>) -> Result<Ts<PilotPage>, JsValue> {
    to_js(&venti_core::pilot_page(status, body).map_err(thrown)?)
}

/// The seasons a page offers a ranking for. A list alone does not cross to
/// the page as a typed value, so it comes in a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct RankingSeasons {
    /// The newest first, and all years together last.
    pub seasons: Vec<RankingSeason>,
}

/// The seasons a page offers a ranking for at a moment: its year and the
/// three before, and all years together.
#[wasm_bindgen(js_name = rankingSeasons)]
pub fn ranking_seasons(now_unix_s: f64) -> Result<Ts<RankingSeasons>, JsValue> {
    to_js(&RankingSeasons {
        seasons: venti_core::ranking_seasons(whole_seconds(now_unix_s)),
    })
}

/// The path and query to `GET` for the ranking of the server's pilots by
/// their best public flights: of the flights that began at or after the
/// one moment and before the other, as `rankingSeasons` names them, or of
/// all of them; with `area`, of the flights that took off inside it.
#[wasm_bindgen(js_name = rankingPath)]
pub fn ranking_path(
    area: Option<Ts<Bounds>>,
    from_unix_s: Option<f64>,
    to_unix_s: Option<f64>,
) -> Result<String, JsValue> {
    venti_core::ranking_path(
        area.as_ref().map(from_js).transpose()?,
        from_unix_s.map(whole_seconds),
        to_unix_s.map(whole_seconds),
    )
    .map_err(thrown)
}

/// The path and query to `GET` for the ranking of one month, as a month
/// of the ranking's `periods` names it ("2026-08"); with `area`, of the
/// flights that took off inside it.
#[wasm_bindgen(js_name = rankingMonthPath)]
pub fn ranking_month_path(area: Option<Ts<Bounds>>, month: String) -> Result<String, JsValue> {
    venti_core::ranking_month_path(area.as_ref().map(from_js).transpose()?, month).map_err(thrown)
}

/// The path and query to `GET` for the pilots whose handle or name holds
/// a text. Fewer than two characters are refused in words.
#[wasm_bindgen(js_name = pilotsSearchPath)]
pub fn pilots_search_path(text: String) -> Result<String, JsValue> {
    venti_core::pilots_search_path(text).map_err(thrown)
}

/// Reads the server's answer to `pilotsSearchPath`: a line for each
/// pilot, "Lena Probe (@lena), 3 public flights".
#[wasm_bindgen(js_name = pilotsFound)]
pub fn pilots_found(status: u16, body: Vec<u8>) -> Result<Ts<PilotsFound>, JsValue> {
    to_js(&venti_core::pilots_found(status, body).map_err(thrown)?)
}

/// The path and query to `GET` for the pilots who fly near a place,
/// within `radiusKm` (1 to 100; absent, 25).
#[wasm_bindgen(js_name = pilotsNearPath)]
pub fn pilots_near_path(lat: f64, lon: f64, radius_km: Option<u32>) -> Result<String, JsValue> {
    venti_core::pilots_near_path(lat, lon, radius_km).map_err(thrown)
}

/// Reads the server's answer to `pilotsNearPath`: a line for each pilot,
/// with their picture's path, their take-offs near the place and their
/// season.
#[wasm_bindgen(js_name = pilotsNear)]
pub fn pilots_near(status: u16, body: Vec<u8>) -> Result<Ts<venti_core::PilotsNear>, JsValue> {
    to_js(&venti_core::pilots_near(status, body).map_err(thrown)?)
}

/// The words of a pilot's picture and of the pilots near a place.
#[wasm_bindgen(js_name = pictureWords)]
pub fn picture_words() -> Result<Ts<venti_core::PictureWords>, JsValue> {
    to_js(&venti_core::picture_words())
}

/// The path to `GET` for what a pilot's public flights add up to.
#[wasm_bindgen(js_name = pilotStatisticsPath)]
pub fn pilot_statistics_path(handle: String) -> Result<String, JsValue> {
    venti_core::pilot_statistics_path(handle).map_err(thrown)
}

/// Reads the server's answer to `pilotStatisticsPath`: the totals of all
/// years and of each year as labelled rows, the sites flown from most
/// and the twelve months.
#[wasm_bindgen(js_name = pilotStatistics)]
pub fn pilot_statistics(status: u16, body: Vec<u8>) -> Result<Ts<PilotStatistics>, JsValue> {
    to_js(&venti_core::pilot_statistics(status, body).map_err(thrown)?)
}

/// Reads the server's answer to `rankingPath`.
#[wasm_bindgen(js_name = ranking)]
pub fn ranking(status: u16, body: Vec<u8>) -> Result<Ts<Ranking>, JsValue> {
    to_js(&venti_core::ranking(status, body).map_err(thrown)?)
}

/// The path to `GET` for one public flight that is reached by its address
/// alone, as from a link.
#[wasm_bindgen(js_name = publicFlightPath)]
pub fn public_flight_path(id: String) -> Result<String, JsValue> {
    venti_core::public_flight_path(id).map_err(thrown)
}

/// Reads the server's answer to `publicFlightPath`, and also what the
/// server says when it does not hand out the flight's file.
#[wasm_bindgen(js_name = publicFlight)]
pub fn public_flight(status: u16, body: Vec<u8>) -> Result<Ts<PublicFlight>, JsValue> {
    to_js(&venti_core::public_flight(status, body).map_err(thrown)?)
}

/// The reasons a report about a flight, a pilot's page or a live track can
/// give, and what the pilot is told before sending one. For a group or a
/// plan, and for any subject at hand, `reportFormFor` is the one to ask.
#[wasm_bindgen(js_name = reportForm)]
pub fn report_form() -> Result<Ts<ReportForm>, JsValue> {
    to_js(&venti_core::report_form())
}

/// The form for a report about `subject`: the reasons a report about such
/// a thing can give, each with what it means there, and what the pilot is
/// told before sending one. A group, a plan and a site that pilots made
/// have forms of their own.
#[wasm_bindgen(js_name = reportFormFor)]
pub fn report_form_for(subject: Ts<ReportSubject>) -> Result<Ts<ReportForm>, JsValue> {
    to_js(&venti_core::report_form_for(from_js(&subject)?))
}

/// The request that tells whoever runs the server that a flight, a
/// pilot's page, a live track, a group, a plan of a group or a site that
/// pilots made should not be there, or not as it is. `text` is what the pilot wrote, and may be empty except with
/// the reason "other".
#[wasm_bindgen(js_name = reportRequest)]
pub fn report_request(
    subject: Ts<ReportSubject>,
    reason: Ts<ReportReason>,
    text: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    let request =
        venti_core::report_request(from_js(&subject)?, from_js(&reason)?, text).map_err(thrown)?;
    to_js(&request)
}

/// Reads the server's answer to `reportRequest` and says in a line what
/// became of the report.
#[wasm_bindgen(js_name = reportSent)]
pub fn report_sent(status: u16, body: Vec<u8>) -> Result<String, JsValue> {
    venti_core::report_sent(status, body).map_err(thrown)
}

// --- Pilots among each other ------------------------------------------------

/// The form for a report about a comment under a flight: the reasons it
/// can give and what the pilot is told before sending one.
#[wasm_bindgen(js_name = commentReportForm)]
pub fn comment_report_form() -> Result<Ts<ReportForm>, JsValue> {
    to_js(&venti_core::comment_report_form())
}

/// The request that reports a comment under a public flight. The answer
/// is read by `reportSent`.
#[wasm_bindgen(js_name = commentReportRequest)]
pub fn comment_report_request(
    flight_id: String,
    comment_id: String,
    reason: Ts<ReportReason>,
    text: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    let request =
        venti_core::comment_report_request(flight_id, comment_id, from_js(&reason)?, text)
            .map_err(thrown)?;
    to_js(&request)
}

/// What a follow button says: "Following" for a pilot one follows,
/// "Follow" otherwise.
#[wasm_bindgen(js_name = followButton)]
pub fn follow_button(following: bool) -> String {
    venti_core::follow_button(following)
}

/// "12 followers".
#[wasm_bindgen(js_name = followersLine)]
pub fn followers_line(count: u32) -> String {
    venti_core::followers_line(count)
}

/// "Follows 3 pilots".
#[wasm_bindgen(js_name = followingLine)]
pub fn following_line(count: u32) -> String {
    venti_core::following_line(count)
}

/// The request that follows a pilot (`follow`) or follows them no more.
/// The answer is read by `followAnswer`.
#[wasm_bindgen(js_name = followRequest)]
pub fn follow_request(handle: String, follow: bool) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::follow_request(handle, follow).map_err(thrown)?)
}

/// Reads the answer to `followRequest`: how following the pilot stands.
#[wasm_bindgen(js_name = followAnswer)]
pub fn follow_answer(status: u16, body: Vec<u8>) -> Result<Ts<FollowState>, JsValue> {
    to_js(&venti_core::follow_answer(status, body).map_err(thrown)?)
}

/// The request for who follows a pilot. With `signed_in` the session
/// token goes along and the answer says whether the pilot follows them.
/// The answer is read by `followList`.
#[wasm_bindgen(js_name = followersRequest)]
pub fn followers_request(
    handle: String,
    after: Option<String>,
    signed_in: bool,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::followers_request(handle, after, signed_in).map_err(thrown)?)
}

/// The request for whom a pilot follows. The answer is read by
/// `followList`.
#[wasm_bindgen(js_name = followingRequest)]
pub fn following_request(
    handle: String,
    after: Option<String>,
    signed_in: bool,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::following_request(handle, after, signed_in).map_err(thrown)?)
}

/// The request for everyone the signed-in pilot follows. The answer is
/// read by `followList`.
#[wasm_bindgen(js_name = ownFollowingRequest)]
pub fn own_following_request(after: Option<String>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::own_following_request(after))
}

/// Reads a page of followers or of followed pilots, with the counts.
#[wasm_bindgen(js_name = followList)]
pub fn follow_list(status: u16, body: Vec<u8>) -> Result<Ts<FollowList>, JsValue> {
    to_js(&venti_core::follow_list(status, body).map_err(thrown)?)
}

/// The request for the public flights of the pilots the signed-in pilot
/// follows. `before` is the `next` of the page before. The answer is
/// read by `feed`.
#[wasm_bindgen(js_name = feedRequest)]
pub fn feed_request(before: Option<String>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::feed_request(before))
}

/// Reads the answer to `feedRequest`: the rows of the public list, each
/// with its cheers and comments.
#[wasm_bindgen]
pub fn feed(status: u16, body: Vec<u8>) -> Result<Ts<Feed>, JsValue> {
    to_js(&venti_core::feed(status, body).map_err(thrown)?)
}

/// Who cheered, in words: "You and 2 others cheered", "3 pilots cheered".
#[wasm_bindgen(js_name = cheersLine)]
pub fn cheers_line(count: u32, cheered: bool) -> String {
    venti_core::cheers_line(count, cheered)
}

/// What the cheer button says: "Cheered" or "Cheer".
#[wasm_bindgen(js_name = cheerButton)]
pub fn cheer_button(cheered: bool) -> String {
    venti_core::cheer_button(cheered)
}

/// The request that cheers a public flight (`cheer`) or takes the cheer
/// back. The answer is read by `cheerAnswer`.
#[wasm_bindgen(js_name = cheerRequest)]
pub fn cheer_request(flight_id: String, cheer: bool) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::cheer_request(flight_id, cheer).map_err(thrown)?)
}

/// Reads the answer to `cheerRequest`.
#[wasm_bindgen(js_name = cheerAnswer)]
pub fn cheer_answer(status: u16, body: Vec<u8>) -> Result<Ts<CheerState>, JsValue> {
    to_js(&venti_core::cheer_answer(status, body).map_err(thrown)?)
}

/// The request for who cheered a public flight. The answer is read by
/// `cheerList`.
#[wasm_bindgen(js_name = cheersRequest)]
pub fn cheers_request(
    flight_id: String,
    after: Option<String>,
    signed_in: bool,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::cheers_request(flight_id, after, signed_in).map_err(thrown)?)
}

/// Reads the answer to `cheersRequest`.
#[wasm_bindgen(js_name = cheerList)]
pub fn cheer_list(status: u16, body: Vec<u8>) -> Result<Ts<CheerList>, JsValue> {
    to_js(&venti_core::cheer_list(status, body).map_err(thrown)?)
}

/// "2 comments"; empty without one.
#[wasm_bindgen(js_name = commentsLine)]
pub fn comments_line(count: u32) -> String {
    venti_core::comments_line(count)
}

/// The request for the comments under a public flight, the oldest first.
/// The answer is read by `commentList`.
#[wasm_bindgen(js_name = commentsRequest)]
pub fn comments_request(
    flight_id: String,
    after: Option<String>,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::comments_request(flight_id, after).map_err(thrown)?)
}

/// Reads the answer to `commentsRequest`. With the signed-in pilot's
/// handle and the handle of the flight's pilot, each comment says
/// whether the pilot may remove it.
#[wasm_bindgen(js_name = commentList)]
pub fn comment_list(
    status: u16,
    body: Vec<u8>,
    now_unix_s: f64,
    own_handle: Option<String>,
    flight_handle: Option<String>,
) -> Result<Ts<CommentList>, JsValue> {
    let now = whole_seconds(now_unix_s);
    to_js(&venti_core::comment_list(status, body, now, own_handle, flight_handle).map_err(thrown)?)
}

/// Looks at a comment while the pilot writes it: what would be posted,
/// the room left, and what is wrong.
#[wasm_bindgen(js_name = commentDraft)]
pub fn comment_draft(text: String) -> Result<Ts<CommentDraft>, JsValue> {
    to_js(&venti_core::comment_draft(text))
}

/// The request that posts a comment under a public flight. The answer is
/// read by `commentPosted`.
#[wasm_bindgen(js_name = commentRequest)]
pub fn comment_request(flight_id: String, text: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::comment_request(flight_id, text).map_err(thrown)?)
}

/// Reads the answer to `commentRequest`.
#[wasm_bindgen(js_name = commentPosted)]
pub fn comment_posted(
    status: u16,
    body: Vec<u8>,
    now_unix_s: f64,
) -> Result<Ts<CommentPosted>, JsValue> {
    to_js(&venti_core::comment_posted(status, body, whole_seconds(now_unix_s)).map_err(thrown)?)
}

/// The request that removes a comment: the pilot's own, or one under a
/// flight of theirs. The answer is read by `socialDone`.
#[wasm_bindgen(js_name = commentRemoveRequest)]
pub fn comment_remove_request(
    flight_id: String,
    comment_id: String,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::comment_remove_request(flight_id, comment_id).map_err(thrown)?)
}

/// The switch on the pilot's own flight: whether it takes comments, and
/// what that means.
#[wasm_bindgen(js_name = commentsSwitch)]
pub fn comments_switch(open: bool) -> Result<Ts<CommentsSwitch>, JsValue> {
    to_js(&venti_core::comments_switch(open))
}

/// The request by which the pilot says whether a flight of their own
/// takes comments. The answer is read by `commentsOpenAnswer`.
#[wasm_bindgen(js_name = commentsOpenRequest)]
pub fn comments_open_request(flight_id: String, open: bool) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::comments_open_request(flight_id, open).map_err(thrown)?)
}

/// Reads the answer to `commentsOpenRequest`.
#[wasm_bindgen(js_name = commentsOpenAnswer)]
pub fn comments_open_answer(status: u16, body: Vec<u8>) -> Result<Ts<CommentsSwitch>, JsValue> {
    to_js(&venti_core::comments_open_answer(status, body).map_err(thrown)?)
}

/// Reads the answer to a request that carries nothing back
/// (`commentRemoveRequest`): throws the refusal in words when it did not
/// work.
#[wasm_bindgen(js_name = socialDone)]
pub fn social_done(status: u16, body: Vec<u8>) -> Result<(), JsValue> {
    venti_core::social_done(status, body).map_err(thrown)
}

/// The request for what the signed-in pilot was told since `since`, the
/// `cursor` of the answer before. The answer is read by `notifications`.
#[wasm_bindgen(js_name = notificationsRequest)]
pub fn notifications_request(since: Option<String>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::notifications_request(since))
}

/// Reads the answer to `notificationsRequest` into lines: "Lena Probe
/// (@lena) cheered your flight from Wallberg".
#[wasm_bindgen]
pub fn notifications(
    status: u16,
    body: Vec<u8>,
    now_unix_s: f64,
) -> Result<Ts<Notifications>, JsValue> {
    to_js(&venti_core::notifications(status, body, whole_seconds(now_unix_s)).map_err(thrown)?)
}

/// The request for how much is new for the signed-in pilot, the same on
/// every device. The answer is read by `unread`.
#[wasm_bindgen(js_name = unreadRequest)]
pub fn unread_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::unread_request())
}

/// The request that the pilot saw the notifications and the alarms, up to
/// the cursors of the answers they were shown, each left out for up to the
/// newest. The answer is read by `unread`.
#[wasm_bindgen(js_name = unreadSeenRequest)]
pub fn unread_seen_request(
    notifications_cursor: Option<String>,
    alarms_cursor: Option<String>,
) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::unread_seen_request(notifications_cursor, alarms_cursor).map_err(thrown)?)
}

/// Reads the answer to `unreadRequest` and `unreadSeenRequest`: the counts
/// and the badge, "5" or "99+".
#[wasm_bindgen]
pub fn unread(status: u16, body: Vec<u8>) -> Result<Ts<Unread>, JsValue> {
    to_js(&venti_core::unread(status, body).map_err(thrown)?)
}

// The pilot's own flights on the server, for a page that keeps no
// logbook: the requests, with the session's token, and their answers read
// into the rows everyone's flights have, with who may see each one.

/// The request for one page of the pilot's own flights, the private ones
/// too, the newest first. `after` is the `next` of the page before. Read
/// the answer with `ownFlights`.
#[wasm_bindgen(js_name = ownFlightsRequest)]
pub fn own_flights_request(after: Option<String>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::own_flights_request(after))
}

/// Reads the answer to `ownFlightsRequest`. `server` is the address the
/// request went to, for the flights' links.
#[wasm_bindgen(js_name = ownFlights)]
pub fn own_flights(server: String, status: u16, body: Vec<u8>) -> Result<Ts<OwnFlights>, JsValue> {
    to_js(&venti_core::own_flights(server, status, body).map_err(thrown)?)
}

/// The request for one of the pilot's flights by its address. Read the
/// answer with `ownFlight`.
#[wasm_bindgen(js_name = ownFlightRequest)]
pub fn own_flight_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::own_flight_request(id).map_err(thrown)?)
}

/// Reads the answer to `ownFlightRequest`, `flightVisibilityRequest` and
/// `flightGliderClassRequest`: the flight as it now is.
#[wasm_bindgen(js_name = ownFlight)]
pub fn own_flight(server: String, status: u16, body: Vec<u8>) -> Result<Ts<OwnFlight>, JsValue> {
    to_js(&venti_core::own_flight(server, status, body).map_err(thrown)?)
}

/// The request that sends a flight file to the account. Read the answer
/// with `uploadedFlight`.
#[wasm_bindgen(js_name = uploadRequest)]
pub fn upload_request(file: Vec<u8>) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::upload_request(file))
}

/// Reads the answer to `uploadRequest`.
#[wasm_bindgen(js_name = uploadedFlight)]
pub fn uploaded_flight(
    server: String,
    status: u16,
    body: Vec<u8>,
) -> Result<Ts<UploadedFlight>, JsValue> {
    to_js(&venti_core::uploaded_flight(server, status, body).map_err(thrown)?)
}

/// The request that changes who may see one of the pilot's flights. Read
/// the answer with `ownFlight`.
#[wasm_bindgen(js_name = flightVisibilityRequest)]
pub fn flight_visibility_request(
    id: String,
    visibility: Ts<FlightVisibility>,
) -> Result<Ts<ServerRequest>, JsValue> {
    let visibility = from_js(&visibility)?;
    to_js(&venti_core::flight_visibility_request(id, visibility).map_err(thrown)?)
}

/// The request that sets the class of the glider one of the pilot's
/// flights was flown on, or says without one that it is not known. Read
/// the answer with `ownFlight`.
#[wasm_bindgen(js_name = flightGliderClassRequest)]
pub fn flight_glider_class_request(
    id: String,
    class: Option<Ts<GliderClassKind>>,
) -> Result<Ts<ServerRequest>, JsValue> {
    let class = class.as_ref().map(from_js).transpose()?;
    to_js(&venti_core::flight_glider_class_request(id, class).map_err(thrown)?)
}

/// The request that deletes one of the pilot's flights, at once and for
/// good. Read the answer with `flightDeleted`.
#[wasm_bindgen(js_name = deleteFlightRequest)]
pub fn delete_flight_request(id: String) -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::delete_flight_request(id).map_err(thrown)?)
}

/// Reads the answer to `deleteFlightRequest`: throws the refusal in words
/// when the flight is not gone.
#[wasm_bindgen(js_name = flightDeleted)]
pub fn flight_deleted(status: u16, body: Vec<u8>) -> Result<(), JsValue> {
    venti_core::flight_deleted(status, body).map_err(thrown)
}

/// The request for what all the pilot's flights add up to, the private
/// ones too. Read the answer with `ownStatistics`.
#[wasm_bindgen(js_name = ownStatisticsRequest)]
pub fn own_statistics_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::own_statistics_request())
}

/// Reads the answer to `ownStatisticsRequest`: `pilotStatistics`'s shape,
/// with a note that every flight of the pilot's is counted, the private
/// ones too.
#[wasm_bindgen(js_name = ownStatistics)]
pub fn own_statistics(status: u16, body: Vec<u8>) -> Result<Ts<PilotStatistics>, JsValue> {
    to_js(&venti_core::own_statistics(status, body).map_err(thrown)?)
}

/// The request for how each of the pilot's flights on the server was
/// flown, the private ones too. Read the answer with `ownTendencies`.
#[wasm_bindgen(js_name = ownHabitsRequest)]
pub fn own_habits_request() -> Result<Ts<ServerRequest>, JsValue> {
    to_js(&venti_core::own_habits_request())
}

/// Reads the answer to `ownHabitsRequest`: what the pilot's flights on
/// the server say about how they fly, in the words and by the rules of a
/// logbook's `tendencies`.
#[wasm_bindgen(js_name = ownTendencies)]
pub fn own_tendencies(status: u16, body: Vec<u8>) -> Result<Ts<Tendencies>, JsValue> {
    to_js(&venti_core::own_tendencies(status, body).map_err(thrown)?)
}

/// A flight file fetched from the server, with the name the logbook would
/// save it under.
#[wasm_bindgen(js_name = flightFileFromText)]
pub fn flight_file_from_text(igc_text: String) -> Result<Ts<FlightFile>, JsValue> {
    to_js(&venti_core::flight_file_from_text(igc_text).map_err(thrown)?)
}

/// What to tell the pilot before a flight goes to a league, from its
/// file's text fetched from the server. `now_unix_s` is the moment the
/// pilot asks at.
#[wasm_bindgen(js_name = leagueHandoverFromFile)]
pub fn league_handover_from_file(
    igc_text: String,
    now_unix_s: f64,
) -> Result<Ts<LeagueHandover>, JsValue> {
    to_js(
        &venti_core::league_handover_from_file(igc_text, whole_seconds(now_unix_s))
            .map_err(thrown)?,
    )
}

/// A flight that is not in the logbook, prepared for display from its
/// file's text. Its `id` is empty, and nothing is kept.
#[wasm_bindgen(js_name = flightFromFile)]
pub fn flight_from_file(igc_text: String) -> Result<Ts<FlightView>, JsValue> {
    to_js(&venti_core::flight_from_file(igc_text).map_err(thrown)?)
}

/// What happened in a flight that is not in the logbook, from its file's
/// text.
#[wasm_bindgen(js_name = flightAnalysisFromFile)]
pub fn flight_analysis_from_file(igc_text: String) -> Result<Ts<FlightAnalysis>, JsValue> {
    to_js(&venti_core::flight_analysis_from_file(igc_text).map_err(thrown)?)
}

/// A flight laid beside another, and how the two lie to each other.
#[derive(Debug, Clone, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ComparedFlight {
    /// The flight laid beside, prepared for display.
    pub other: FlightView,
    /// How the two lie to each other, and the lines of numbers.
    pub comparison: FlightComparison,
}

/// Compares two flights from their files' text: the first is the one on
/// screen, the other the one laid beside it. Both are read whole, with
/// their thermals and glides where the files allow; nothing is kept.
#[wasm_bindgen(js_name = compareFlightFiles)]
pub fn compare_flight_files(first: String, other: String) -> Result<Ts<ComparedFlight>, JsValue> {
    let first_view = venti_core::flight_from_file(first.clone()).map_err(thrown)?;
    let other_view = venti_core::flight_from_file(other.clone()).map_err(thrown)?;
    let comparison = venti_core::compare_flights(
        first_view,
        other_view.clone(),
        venti_core::flight_analysis_from_file(first).ok(),
        venti_core::flight_analysis_from_file(other).ok(),
    );
    to_js(&ComparedFlight {
        other: other_view,
        comparison,
    })
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

    use venti_core::AirspaceCover;
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
    fn a_page_keeps_the_pilots_sites_across_reloads() {
        // A site as the server answers it, read by the core.
        let answer = br#"{"layers": [{"source": "osm-free-flying",
            "fetched_at": "2026-11-18T19:05:00Z", "as_of": "2026-11-18T19:05:00Z", "covers": [],
            "sites": [{"id": "osm-free-flying:way/1080986173",
                "name": "Fluggel\u00e4nde J\u00e4gerkamp/Nagelspitz",
                "position": {"lat": 47.6791413, "lon": 11.9051032}, "elevation_m": 1560.0,
                "uses": ["takeoff"]}]}],
            "attributions": [{"source": "osm-free-flying",
                "provider": "OpenStreetMap contributors", "licence": "ODbL 1.0",
                "text": "\u00a9 OpenStreetMap contributors",
                "url": "https://www.openstreetmap.org/copyright"}]}"#;
        let map = venti_core::sites_map(47.5, 11.5, 48.0, 12.5, 200, answer.to_vec()).unwrap();
        let site = map.sites[0].clone();

        let mut store = Store::new();
        let session = open(&store);
        session
            .call(|core| core.keep_site(site.clone(), map.credits.clone(), map.as_of_unix_s))
            .unwrap();
        commit(&session.staged, &mut store);
        assert!(store.contains_key("sites-kept/osm-free-flying.json"));

        // After a reload the file is in the store and not in memory: the
        // call fails for want of it, the page fetches it and calls again.
        let reopened = open(&store);
        let error = reopened.call(|core| core.kept_sites(None)).unwrap_err();
        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::Storage);
        assert_eq!(fetch(&reopened.staged, &store, usize::MAX), 1);
        let kept = reopened.call(|core| core.kept_sites(None)).unwrap();
        assert_eq!(kept.sites[0].site, site);
        assert_eq!(kept.credits, map.credits);

        // Today begins with it, also with no answer from the server; the
        // file is out of memory again after each call that succeeded.
        reopened
            .call(|core| core.today_takeoffs(None, None))
            .unwrap_err();
        assert_eq!(fetch(&reopened.staged, &store, usize::MAX), 1);
        let today = reopened
            .call(|core| core.today_takeoffs(None, None))
            .unwrap();
        assert!(today.takeoffs[0].is_kept);
        assert!(reopened.staged.take_staged().is_empty());

        reopened
            .call(|core| core.forget_site(site.id.clone()))
            .unwrap_err();
        assert_eq!(fetch(&reopened.staged, &store, usize::MAX), 1);
        reopened
            .call(|core| core.forget_site(site.id.clone()))
            .unwrap();
        commit(&reopened.staged, &mut store);
        assert!(!store.contains_key("sites-kept/osm-free-flying.json"));
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
    fn a_flight_is_checked_against_airspace_the_page_has_yet_to_fetch() {
        let mut store = Store::new();
        let session = open(&store);
        let id = session
            .call(|core| core.import_flight(sample()))
            .unwrap()
            .id;
        // Ten miles around the valley of the sample flight, from the ground
        // to far above it: the whole flight is inside.
        let zone = "AC D\nAY CTR\nAN CTR VALLEY\nAL GND\nAH 12000ft AMSL\n\
                    V X=47:40:00 N 011:46:00 E\nDC 10\n";
        session
            .call(|core| core.import_airspace("valley.txt".to_owned(), zone.as_bytes().to_vec(), 0))
            .unwrap();
        commit(&session.staged, &mut store);

        // A reload: neither the flight's file nor the airspace file is in
        // memory. A check that took "not fetched yet" for "no airspace"
        // would answer at once that nothing was checked.
        let session = open(&store);
        let mut fetched = Vec::new();
        let checked = loop {
            match session.call(|core| core.flight_airspace(id.clone())) {
                Ok(checked) => break checked,
                Err(error) => {
                    assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::Storage);
                    let wanted = session.staged.take_wanted();
                    assert!(!wanted.is_empty(), "{error}");
                    for name in &wanted {
                        session.staged.provide(name, store.get(name).cloned());
                    }
                    fetched.extend(wanted);
                }
            }
        };
        assert!(
            fetched.iter().any(|name| name.starts_with("flights/"))
                && fetched.iter().any(|name| name.starts_with("airspace/")),
            "{fetched:?}"
        );
        assert_eq!(checked.cover, AirspaceCover::Whole);
        assert_eq!(checked.crossings.len(), 1, "{:?}", checked.crossings);
        assert_eq!(checked.crossings[0].name, "CTR VALLEY");
        assert_eq!(checked.crossings[0].entry.track_index, 0);

        // The airspace is read once and kept; the flight's file is fetched
        // again, like for any other look at the flight.
        session
            .call(|core| core.flight_airspace(id.clone()))
            .unwrap_err();
        assert_eq!(session.staged.take_wanted(), [format!("flights/{id}.igc")]);
    }

    /// One of the server's own recorded answers.
    fn recorded(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../server/tests/golden/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn the_pilot_is_you_on_the_board_also_while_the_account_has_yet_to_be_fetched() {
        let mut store = Store::new();
        let session = open(&store);
        session
            .call(|core| {
                core.signed_in(
                    "https://venti.example".to_owned(),
                    201,
                    recorded("sign_in.json"),
                )
            })
            .unwrap();
        commit(&session.staged, &mut store);

        // A reload: the account is in the store and not in memory.
        let session = open(&store);
        let board = recorded("plan_board.json");

        // Who "you" is, is the handle of the account. The core does not
        // take "not fetched from the store yet" for "signed out", which
        // would call no plan the pilot's own and no group one the pilot
        // keeps: it asks for the account first, as for any stored file.
        let error = session
            .call(|core| core.plan_board(200, board.clone()))
            .unwrap_err();
        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::Storage);
        assert_eq!(fetch(&session.staged, &store, usize::MAX), 1);
        let board = session
            .call(|core| core.plan_board(200, board.clone()))
            .unwrap();
        let plan = &board.days[0].plans[0];
        assert!(plan.is_mine && plan.am_in && plan.can_delete);
        assert_eq!(plan.in_words, "You, Mara Muster and Jonas Beispiel");
        assert!(board.groups[0].is_kept_by_me);
        assert_eq!(board.groups[0].summary, "3 members · kept by you");

        // A request about a group reads the account too, and is refused
        // in words in a browser that is signed out.
        session
            .call(|core| core.plan_board_request("2026-11-18".to_owned(), None))
            .unwrap_err();
        assert_eq!(fetch(&session.staged, &store, usize::MAX), 1);
        let request = session
            .call(|core| core.plan_board_request("2026-11-18".to_owned(), None))
            .unwrap();
        assert_eq!(request.path, "/v1/groups/plans?from=2026-11-18");
        assert!(request.needs_token);

        let signed_out = open(&Store::new());
        let error = signed_out
            .call(|core| core.plan_board_request("2026-11-18".to_owned(), None))
            .unwrap_err();
        assert_eq!(CoreErrorKind::of(&error), CoreErrorKind::Server);
        assert_eq!(error.to_string(), "This device is not signed in.");
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
