//! Keeping the pilot's own sites in step with their account.
//!
//! Signed out, the kept sites are on the device alone. Signed in, they are
//! on the account too, private there, and the files on the device are the
//! copy that works without a network ([`crate::kept_sites`]).
//!
//! [`KeptSitesSync`] is a loop of steps like the logbook's
//! ([`crate::FlightSync`]): the core hands the shell a request, the shell
//! sends it with the session token and hands the answer back, until the
//! core says that it has finished or why it stopped. Sites forgotten here
//! are forgotten there, what changed there is fetched and applied, and
//! sites kept here are sent. Every finished step is saved.
//!
//! **What happens when a device signs in.** What the device holds is
//! offered to the account, once: the list becomes the union of the two.
//! A site only the device has is kept on the account; a site only the
//! account has comes to the device. Of a site both have, the record with
//! the newer note of when it was kept stays, and the server decides that:
//! what a device held before it signed in carries no note, because the
//! core reads no clock, and so never replaces what the account has. A site
//! kept while signed in is noted with the time of the sync that sends it.
//! Keeping wins over forgetting: a site kept on one device and forgotten
//! on another before the two were in step stays kept.
//!
//! **Signing out** leaves the device its copy and forgets only what was
//! noted for the account, so that the next sign-in offers again.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use venti_api_types::v1::{
    KeepSiteRequest, KeptSite, KeptSiteChangesRequest, KeptSiteChangesResponse, KeptSiteResponse,
    kept_site_path,
};
use venti_domain::SiteId;

use crate::account::{Halt, asked_to_wait, failed, fetched, json_request, settled};
use crate::kept_sites::{KeptLayer, KeptRecord};
use crate::server::{answer, code, refusal};
use crate::{Core, CoreError, ServerRequest, SyncStep, SyncSummary};

/// The name of what the device noted for the account. It lies outside the
/// folder of the kept sites, whose every file is a source's.
const FILE: &str = "kept-sites-account.json";

/// How many changes are asked for at a time: the most the server gives.
const CHANGES_AT_A_TIME: usize = 100;

/// What the device knows of the account's side of the kept sites, as
/// stored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct KeptSync {
    /// The account it is of: the server and the handle. What was noted
    /// for another account is not used.
    server: String,
    handle: String,
    /// Where the server's list of changes was read up to.
    #[serde(default)]
    cursor: Option<String>,
    /// Sites kept on this device since, which the account has yet to
    /// learn of: sent with the time of the sync as the note of when.
    #[serde(default)]
    kept_here: BTreeSet<String>,
    /// Sites the device held when it signed in: offered to the account
    /// once, without a note of when.
    #[serde(default)]
    offered: BTreeSet<String>,
    /// Sites forgotten on this device that the account may still keep.
    #[serde(default)]
    forgotten_here: BTreeSet<String>,
}

impl KeptSync {
    /// Notes that a site was kept on this device.
    pub(crate) fn kept_here(&mut self, id: &str) {
        self.forgotten_here.remove(id);
        self.offered.remove(id);
        self.kept_here.insert(id.to_owned());
    }

    /// Notes that a site was forgotten on this device.
    pub(crate) fn forgotten_here(&mut self, id: &str) {
        self.kept_here.remove(id);
        self.offered.remove(id);
        self.forgotten_here.insert(id.to_owned());
    }

    /// Whether the account knows everything this device did.
    pub(crate) fn is_in_step(&self) -> bool {
        self.cursor.is_some()
            && self.kept_here.is_empty()
            && self.offered.is_empty()
            && self.forgotten_here.is_empty()
    }

    /// Whether a change the server tells of is held back, because this
    /// device has something newer to say about the site.
    fn is_waiting(&self, id: &str) -> bool {
        self.kept_here.contains(id) || self.forgotten_here.contains(id)
    }
}

impl Core {
    /// What the device noted for the account it is signed in to. `None`
    /// when signed out: the kept sites are then the device's alone.
    ///
    /// A device that has noted nothing for this account yet offers it
    /// everything it holds.
    pub(crate) fn kept_sync(&self) -> Result<Option<KeptSync>, CoreError> {
        let Some((server, handle, _)) = self.signed_in_to()? else {
            return Ok(None);
        };
        let stored = self
            .storage
            .read(FILE)
            .map_err(storage_error)?
            // The file is the app's own writing. Should it be damaged,
            // everything is offered again, which loses nothing.
            .and_then(|bytes| serde_json::from_slice::<KeptSync>(&bytes).ok())
            .filter(|sync| sync.server == server && sync.handle == handle);
        if let Some(sync) = stored {
            return Ok(Some(sync));
        }
        Ok(Some(KeptSync {
            server,
            handle,
            offered: self
                .kept_layers()?
                .iter()
                .flat_map(|layer| &layer.sites)
                .map(|kept| kept.site.id.to_string())
                .collect(),
            ..KeptSync::default()
        }))
    }

    pub(crate) fn store_kept_sync(&self, sync: &KeptSync) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(sync).map_err(|error| CoreError::Storage {
            message: format!("Your sites could not be saved: {error}."),
        })?;
        self.storage.write(FILE, &bytes).map_err(storage_error)
    }

    /// Forgets what was noted for the account. The kept sites stay.
    pub(crate) fn forget_kept_sync(&self) -> Result<(), CoreError> {
        self.storage.remove(FILE).map_err(storage_error)
    }
}

#[uniffi::export]
impl Core {
    /// Starts bringing the pilot's own sites in step with their account.
    /// Run it after signing in, after the pilot kept or forgot a site, and
    /// whenever the logbook is brought in step.
    pub fn kept_sites_sync(self: Arc<Self>) -> Arc<KeptSitesSync> {
        Arc::new(KeptSitesSync {
            core: self,
            progress: Mutex::new(Progress::default()),
        })
    }
}

/// One run of bringing the pilot's own sites in step with their account.
///
/// Its steps and its end are the logbook's ([`SyncStep`]); in the summary
/// `sent` and `fetched` count sites, `removed_here` and `removed_there`
/// sites that were forgotten, and `refused` sites the server would not
/// take.
#[derive(Debug, uniffi::Object)]
pub struct KeptSitesSync {
    core: Arc<Core>,
    progress: Mutex<Progress>,
}

#[derive(Debug, Default)]
struct Progress {
    now_unix_s: i64,
    waiting_for: Option<Waiting>,
    /// Whether the server's changes have been read to their end.
    pulled: bool,
    /// When the server started its list again from the beginning: the
    /// sites it has named since. What the device holds beyond them is no
    /// longer the account's.
    named_since_reset: Option<BTreeSet<String>>,
    /// Whether the account said that it keeps as many sites as it may.
    full: bool,
    summary: SyncSummary,
}

/// What the sync waits for.
#[derive(Debug)]
enum Waiting {
    Forgetting {
        id: String,
    },
    Changes,
    Keeping {
        id: String,
    },
    /// Not an answer: the last one was taken in, and the step after it
    /// needs a stored file that the storage has yet to fetch.
    Storage,
}

#[uniffi::export]
impl KeptSitesSync {
    /// Begins. `now_unix_s` is the current time: the note of when, for the
    /// sites kept on this device since the last run.
    pub fn start(&self, now_unix_s: i64) -> SyncStep {
        settled(self.try_start(now_unix_s))
    }

    /// Takes the server's answer to the last request. `retry_after_s` is
    /// the answer's `Retry-After` header, if it had one.
    pub fn receive(&self, status: u16, body: Vec<u8>, retry_after_s: Option<u32>) -> SyncStep {
        settled(self.try_receive(status, &body, retry_after_s))
    }
}

impl KeptSitesSync {
    /// [`start`](Self::start) for a host whose storage hands a file over
    /// only after it was asked for it, as [`crate::FlightSync`] has it.
    pub fn start_when_fetched(&self, now_unix_s: i64) -> Result<SyncStep, CoreError> {
        fetched(self.try_start(now_unix_s))
    }

    /// [`receive`](Self::receive) for such a host. The answer is taken in
    /// once, however often the call is made again.
    pub fn receive_when_fetched(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<SyncStep, CoreError> {
        fetched(self.try_receive(status, body, retry_after_s))
    }

    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn try_start(&self, now_unix_s: i64) -> Result<SyncStep, Halt> {
        let mut progress = self.lock();
        *progress = Progress {
            now_unix_s,
            ..Progress::default()
        };
        self.next(&mut progress)
    }

    fn try_receive(
        &self,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<SyncStep, Halt> {
        let mut progress = self.lock();
        if progress.waiting_for.is_none() {
            return Ok(failed("Nothing was asked of the server.", false));
        }
        if !matches!(progress.waiting_for, Some(Waiting::Storage)) {
            self.take_in(&mut progress, status, body, retry_after_s)?;
        }
        progress.waiting_for = None;
        let step = self.next(&mut progress);
        if matches!(step, Err(Halt::Fetch(_))) {
            progress.waiting_for = Some(Waiting::Storage);
        }
        step
    }

    /// What the device noted for the account, and the kept sites. Both are
    /// read before anything is changed: if the storage has yet to fetch
    /// one, the same call goes through once it has.
    fn held(&self) -> Result<(KeptSync, Vec<KeptLayer>), Halt> {
        let Some(sync) = self.core.kept_sync().map_err(Halt::Fetch)? else {
            return Err(failed("This device is not signed in.", true).into());
        };
        let layers = self.core.kept_layers().map_err(Halt::Fetch)?;
        Ok((sync, layers))
    }

    /// Stores the layers that changed, and what is noted for the account.
    fn store(
        &self,
        before: &[KeptLayer],
        layers: &[KeptLayer],
        sync: &KeptSync,
    ) -> Result<(), Halt> {
        let stored = || -> Result<(), CoreError> {
            for layer in layers {
                let was = before.iter().find(|was| was.source == layer.source);
                if was != Some(layer) {
                    self.core.store_kept_layer(layer)?;
                }
            }
            self.core.store_kept_sync(sync)
        };
        stored().map_err(|error| failed(&error.to_string(), false).into())
    }

    /// Applies the server's answer to the request that is waiting for one,
    /// and saves what was learned.
    fn take_in(
        &self,
        progress: &mut Progress,
        status: u16,
        body: &[u8],
        retry_after_s: Option<u32>,
    ) -> Result<(), Halt> {
        let (mut sync, mut layers) = self.held()?;
        let before = layers.clone();
        let Some(waiting) = progress.waiting_for.take() else {
            return Ok(());
        };

        // What holds for every request.
        if status == 401 {
            self.core.session_ended();
            return Err(failed("This device is no longer signed in. Sign in again.", true).into());
        }
        if status == 429 || status == 503 {
            return Err(failed(&asked_to_wait(retry_after_s), false).into());
        }

        match waiting {
            Waiting::Forgetting { id } => {
                // Not kept there is as good as forgotten.
                if !(200..300).contains(&status) && status != 404 {
                    return Err(failed(&refusal(status, body).to_string(), false).into());
                }
                sync.forgotten_here.remove(&id);
                progress.summary.removed_there += 1;
            }
            Waiting::Changes => {
                let changes: KeptSiteChangesResponse = match answer(status, body) {
                    Ok(changes) => changes,
                    Err(error) => return Err(failed(&error.to_string(), false).into()),
                };
                if changes.reset {
                    // What follows is everything the account keeps.
                    progress.named_since_reset = Some(BTreeSet::new());
                }
                for kept in changes.kept {
                    let id = kept.site.id.to_string();
                    if let Some(named) = &mut progress.named_since_reset {
                        named.insert(id.clone());
                    }
                    // What this device has yet to say about the site is
                    // newer than what the server tells.
                    if sync.is_waiting(&id) {
                        continue;
                    }
                    // The account has it: there is nothing to offer.
                    sync.offered.remove(&id);
                    if put(&mut layers, kept) {
                        progress.summary.fetched += 1;
                    }
                }
                for id in changes.forgotten {
                    let id = id.to_string();
                    // Kept here since, or held here when the device signed
                    // in: keeping wins over forgetting.
                    if sync.is_waiting(&id) || sync.offered.contains(&id) {
                        continue;
                    }
                    if take_out(&mut layers, |held| held == id) {
                        progress.summary.removed_here += 1;
                    }
                }
                sync.cursor = Some(changes.cursor);
                progress.pulled = !changes.more;
                if progress.pulled
                    && let Some(named) = progress.named_since_reset.take()
                {
                    // The server could not say what was forgotten: what it
                    // did not name, and this device has nothing to say
                    // about, the account no longer keeps.
                    let gone = |held: &str| {
                        !named.contains(held)
                            && !sync.is_waiting(held)
                            && !sync.offered.contains(held)
                    };
                    let held: usize = layers.iter().map(|layer| layer.sites.len()).sum();
                    take_out(&mut layers, gone);
                    let left: usize = layers.iter().map(|layer| layer.sites.len()).sum();
                    progress.summary.removed_here += u32::try_from(held - left).unwrap_or(u32::MAX);
                }
            }
            Waiting::Keeping { id } => match status {
                200..=299 => {
                    let response: KeptSiteResponse = match answer(status, body) {
                        Ok(response) => response,
                        Err(error) => return Err(failed(&error.to_string(), false).into()),
                    };
                    // The account's record is the device's from now on:
                    // the one sent, or the one the account had, if that
                    // was kept later.
                    // Unless the pilot forgot the site while the request
                    // was under way: that is said to the server next.
                    if !sync.forgotten_here.contains(&id) {
                        put(&mut layers, response.kept);
                    }
                    sync.kept_here.remove(&id);
                    sync.offered.remove(&id);
                    progress.summary.sent += 1;
                }
                // No room for it. It stays on this device and is sent
                // once there is room.
                409 if code(body).as_deref() == Some("too_many_kept_sites") => {
                    progress.full = true;
                }
                // Not a site the server takes: it stays on this device
                // and is not sent again.
                400 | 413 | 422 => {
                    sync.kept_here.remove(&id);
                    sync.offered.remove(&id);
                    progress.summary.refused += 1;
                }
                _ => {
                    return Err(failed(
                        &match code(body) {
                            Some(_) => refusal(status, body).to_string(),
                            None => format!("The server did not take a site (status {status})."),
                        },
                        false,
                    )
                    .into());
                }
            },
            // Not a request: `try_receive` never comes here with it.
            Waiting::Storage => {}
        }

        self.store(&before, &layers, &sync)
    }

    /// The next request, or the end.
    fn next(&self, progress: &mut Progress) -> Result<SyncStep, Halt> {
        match self.core.signed_in_to().map_err(Halt::Fetch)? {
            Some((_, _, true)) => {}
            _ => return Ok(failed("This device is not signed in.", true)),
        }
        let (mut sync, layers) = self.held()?;
        let noted = sync.clone();
        let step = self.step(progress, &mut sync, &layers);
        // What was offered is noted before the first request, so that it
        // is offered once; so is what turned out not to be there to send.
        if sync != noted || matches!(step, SyncStep::Finished { .. }) || !progress.pulled {
            self.store(&layers, &layers, &sync)?;
        }
        Ok(step)
    }

    fn step(&self, progress: &mut Progress, sync: &mut KeptSync, layers: &[KeptLayer]) -> SyncStep {
        let send = |method: &str, id: &SiteId| ServerRequest {
            method: method.to_owned(),
            path: kept_site_path(id),
            body: None,
            content_type: None,
            needs_token: true,
        };

        // 1. What was forgotten here is forgotten there.
        while let Some(id) = sync.forgotten_here.first().cloned() {
            let Ok(site) = id.parse::<SiteId>() else {
                sync.forgotten_here.remove(&id);
                continue;
            };
            progress.waiting_for = Some(Waiting::Forgetting { id });
            return SyncStep::Send {
                request: send("DELETE", &site),
            };
        }

        // 2. What changed there is learned.
        if !progress.pulled {
            progress.waiting_for = Some(Waiting::Changes);
            return SyncStep::Send {
                request: ServerRequest {
                    method: "GET".to_owned(),
                    path: KeptSiteChangesRequest {
                        since: sync.cursor.clone(),
                        limit: Some(CHANGES_AT_A_TIME),
                    }
                    .path_and_query(),
                    body: None,
                    content_type: None,
                    needs_token: true,
                },
            };
        }

        // 3. What was kept here is sent, one site after another: first
        // what the pilot kept since, then what the device held when it
        // signed in. Not to an account that just said it is full.
        while !progress.full {
            let (id, kept_since) = match (sync.kept_here.first(), sync.offered.first()) {
                (Some(id), _) => (id.clone(), true),
                (None, Some(id)) => (id.clone(), false),
                (None, None) => break,
            };
            let held = layers.iter().find_map(|layer| {
                let kept = layer
                    .sites
                    .iter()
                    .find(|kept| kept.site.id.to_string() == id)?;
                Some((layer, kept))
            });
            let request = held.and_then(|(layer, kept)| {
                let body = KeepSiteRequest {
                    site: kept.site.clone(),
                    standing: kept.standing.clone(),
                    attribution: layer.attribution.clone(),
                    as_of: kept
                        .as_of_unix_s
                        .and_then(|at| DateTime::from_timestamp(at, 0)),
                    // Kept since the last run: noted now. Held before
                    // signing in: with the note it has, if it has one.
                    kept_at: if kept_since {
                        DateTime::from_timestamp(progress.now_unix_s, 0)
                    } else {
                        kept.kept_at_unix_s
                            .and_then(|at| DateTime::from_timestamp(at, 0))
                    },
                };
                json_request("PUT", &kept_site_path(&kept.site.id), &body, true).ok()
            });
            let Some(request) = request else {
                // No longer held here, or not a record that can be sent.
                sync.kept_here.remove(&id);
                sync.offered.remove(&id);
                continue;
            };
            progress.waiting_for = Some(Waiting::Keeping { id });
            return SyncStep::Send { request };
        }

        if progress.full {
            // Everything else is done; the pilot is told why sites wait.
            let full = br#"{"error":{"code":"too_many_kept_sites"}}"#;
            return failed(&refusal(409, full).to_string(), false);
        }
        SyncStep::Finished {
            summary: progress.summary,
        }
    }
}

/// Puts a site the account keeps among the device's, in place of the
/// record the device had of it. Returns whether the device had none.
fn put(layers: &mut Vec<KeptLayer>, kept: KeptSite) -> bool {
    let source = kept.site.id.source().clone();
    let index = layers
        .iter()
        .position(|layer| layer.source == source)
        .unwrap_or_else(|| {
            layers.push(KeptLayer {
                source,
                attribution: None,
                sites: Vec::new(),
            });
            layers.len() - 1
        });
    let layer = &mut layers[index];
    if kept.attribution.is_some() {
        layer.attribution = kept.attribution;
    }
    let record = KeptRecord {
        site: kept.site,
        standing: kept.standing,
        as_of_unix_s: kept.as_of.map(|at| at.timestamp()),
        kept_at_unix_s: Some(kept.kept_at.timestamp()),
    };
    match layer
        .sites
        .iter_mut()
        .find(|have| have.site.id == record.site.id)
    {
        Some(have) => {
            *have = record;
            false
        }
        None => {
            layer.sites.push(record);
            true
        }
    }
}

/// Takes the sites out that `gone` names. Returns whether there was one.
fn take_out(layers: &mut [KeptLayer], gone: impl Fn(&str) -> bool) -> bool {
    let mut any = false;
    for layer in layers {
        let before = layer.sites.len();
        layer.sites.retain(|kept| !gone(&kept.site.id.to_string()));
        any |= layer.sites.len() != before;
    }
    any
}

fn storage_error(error: std::io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("Your sites could not be read or written: {error}."),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::sites::{SitesMap, sites_map};
    use crate::testing::TempFolder;
    use crate::{KeptSites, sync_summary_in_words};

    const SERVER: &str = "http://127.0.0.1:8080";
    const NOW: i64 = 1_795_028_700;
    const RAMP: &str = "osm-free-flying:node/5964676196";
    const OTHER_RAMP: &str = "osm-free-flying:node/5964676197";
    const JAEGERKAMP: &str = "osm-free-flying:way/1080986173";

    /// The server's own recorded answers, which its tests keep current.
    fn golden(name: &str) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{path}{name}")).unwrap()
    }

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    fn sign_in(core: &Arc<Core>) {
        core.signed_in(format!("{SERVER}/"), 201, golden("sign_in.json"))
            .unwrap();
    }

    fn recorded() -> SitesMap {
        sites_map(47.55, 11.6, 47.85, 12.0, 200, golden("sites.json")).unwrap()
    }

    fn keep(core: &Core, id: &str) {
        let map = recorded();
        let site = map.sites.iter().find(|site| site.id == id).unwrap().clone();
        core.keep_site(site, map.credits.clone(), map.as_of_unix_s)
            .unwrap();
    }

    fn held(core: &Core) -> Vec<String> {
        let kept: KeptSites = core.kept_sites(None).unwrap();
        let mut ids: Vec<String> = kept.sites.into_iter().map(|kept| kept.site.id).collect();
        ids.sort();
        ids
    }

    fn note(core: &Core) -> String {
        core.kept_sites(None).unwrap().note
    }

    fn request(step: &SyncStep) -> &ServerRequest {
        match step {
            SyncStep::Send { request } => request,
            other => panic!("expected a request, got {other:?}"),
        }
    }

    fn body(step: &SyncStep) -> Value {
        serde_json::from_slice(request(step).body.as_deref().unwrap()).unwrap()
    }

    fn summary(step: &SyncStep) -> String {
        match step {
            SyncStep::Finished { summary } => sync_summary_in_words(*summary),
            other => panic!("expected the end, got {other:?}"),
        }
    }

    /// The server's answer to the changes: these kept, these forgotten.
    fn changes(kept: &[Value], forgotten: &[&str], cursor: &str) -> Vec<u8> {
        json!({"kept": kept, "forgotten": forgotten, "cursor": cursor, "more": false, "reset": false})
            .to_string()
            .into_bytes()
    }

    /// The site the server's recorded answer keeps: the Jägerkamp.
    fn on_the_account() -> Value {
        let answer: Value = serde_json::from_slice(&golden("kept_site.json")).unwrap();
        answer["kept"].clone()
    }

    /// What a server answers to keeping a site: the site as sent, kept
    /// from the moment the request names or from now.
    fn taken(step: &SyncStep) -> Vec<u8> {
        let mut kept = body(step);
        if kept.get("kept_at").is_none() {
            kept["kept_at"] = json!("2026-11-18T19:05:00Z");
        }
        json!({ "kept": kept }).to_string().into_bytes()
    }

    #[test]
    fn signed_out_the_kept_sites_are_the_devices_alone() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, RAMP);
        assert_eq!(held(&core), [RAMP]);
        assert!(
            note(&core)
                .starts_with("Kept on this device only: sign in to have it on your other devices."),
            "{}",
            note(&core)
        );
        // Nothing is noted for an account, and there is nothing to send.
        assert_eq!(core.storage.read(FILE).unwrap(), None);
        let step = Arc::clone(&core).kept_sites_sync().start(NOW);
        assert_eq!(
            step,
            SyncStep::Failed {
                message: "This device is not signed in.".to_owned(),
                needs_sign_in: true
            }
        );
        core.forget_site(RAMP.to_owned()).unwrap();
        assert!(held(&core).is_empty());
    }

    #[test]
    fn signing_in_offers_what_the_device_holds_and_takes_what_the_account_has() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, RAMP);
        keep(&core, JAEGERKAMP);
        sign_in(&core);
        assert!(
            note(&core)
                .starts_with("Kept on this device, and on your Venti account after the next sync."),
            "{}",
            note(&core)
        );

        // The account's whole list is asked for first.
        let sync = Arc::clone(&core).kept_sites_sync();
        let step = sync.start(NOW);
        let asked = request(&step);
        assert_eq!(
            (
                asked.method.as_str(),
                asked.path.as_str(),
                asked.needs_token
            ),
            ("GET", "/v1/account/kept-sites/changes?limit=100", true)
        );
        // The account keeps the Jägerkamp, under another name and with a
        // note of when, and a ramp this device does not have.
        let mut theirs = on_the_account();
        theirs["site"]["name"] = json!("Jägerkamp, as the account has it");
        let mut other = on_the_account();
        other["site"] = json!({"id": OTHER_RAMP, "position": {"lat": 47.6652, "lon": 11.7955},
            "uses": ["takeoff"]});
        let step = sync.receive(200, changes(&[theirs, other], &[], "7"), None);

        // Of what both have, the account's stays: the device's record has
        // no note of when and is not sent. What only the device has is
        // offered, without a note.
        let asked = request(&step);
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            (
                "PUT",
                "/v1/account/kept-sites/osm-free-flying:node/5964676196"
            )
        );
        let sent = body(&step);
        assert_eq!(sent["site"]["id"], RAMP);
        assert_eq!(sent.get("kept_at"), None);
        assert_eq!(sent["attribution"]["source"], "osm-free-flying");
        assert_eq!(sent["as_of"], "2026-11-18T19:05:00Z");
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent, 1 fetched");

        // The union, on the device and without a network.
        assert_eq!(held(&core), [RAMP, OTHER_RAMP, JAEGERKAMP]);
        let kept = core.kept_sites(None).unwrap();
        assert!(
            kept.sites
                .iter()
                .any(|kept| kept.site.name == "Jägerkamp, as the account has it")
        );
        assert_eq!(kept.credits.len(), 1);
        assert!(
            kept.note
                .starts_with("Kept on your Venti account and on this device."),
            "{}",
            kept.note
        );

        // The offer is made once: the next run asks for what changed
        // since, and sends nothing.
        let again = Arc::clone(&core).kept_sites_sync();
        let step = again.start(NOW + 60);
        assert_eq!(
            request(&step).path,
            "/v1/account/kept-sites/changes?since=7&limit=100"
        );
        let step = again.receive(200, changes(&[], &[], "7"), None);
        assert_eq!(summary(&step), "Nothing to send or fetch");
    }

    #[test]
    fn keeping_and_forgetting_while_signed_in_go_to_the_account() {
        let folder = TempFolder::new();
        let core = core(&folder);
        sign_in(&core);
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW);
        let step = sync.receive(200, changes(&[], &[], "3"), None);
        assert_eq!(summary(&step), "Nothing to send or fetch");

        // The same calls as ever; the device has the site at once.
        keep(&core, RAMP);
        keep(&core, JAEGERKAMP);
        assert_eq!(held(&core), [RAMP, JAEGERKAMP]);
        assert!(note(&core).starts_with("Kept on this device, and on your Venti account after"));
        core.forget_site(JAEGERKAMP.to_owned()).unwrap();

        // What was forgotten here is said first, then the changes are
        // read, then what was kept here is sent with the time of the run.
        let sync = Arc::clone(&core).kept_sites_sync();
        let step = sync.start(NOW + 60);
        let asked = request(&step);
        assert_eq!(
            (asked.method.as_str(), asked.path.as_str()),
            (
                "DELETE",
                "/v1/account/kept-sites/osm-free-flying:way/1080986173"
            )
        );
        let step = sync.receive(204, Vec::new(), None);
        assert_eq!(
            request(&step).path,
            "/v1/account/kept-sites/changes?since=3&limit=100"
        );
        let step = sync.receive(200, changes(&[], &[], "3"), None);
        assert_eq!(request(&step).method, "PUT");
        assert_eq!(body(&step)["kept_at"], "2026-11-18T19:06:00Z");
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent, 1 removed on the server");
        assert!(note(&core).starts_with("Kept on your Venti account and on this device."));

        // Forgotten on another device, it goes here too; a site kept here
        // since does not, and neither does one this device never had.
        keep(&core, JAEGERKAMP);
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW + 120);
        let step = sync.receive(
            200,
            changes(&[], &[RAMP, JAEGERKAMP, OTHER_RAMP], "9"),
            None,
        );
        assert_eq!(body(&step)["site"]["id"], JAEGERKAMP);
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent, 1 removed here");
        assert_eq!(held(&core), [JAEGERKAMP]);
    }

    #[test]
    fn signing_out_leaves_the_device_its_copy_and_the_next_sign_in_offers_again() {
        let folder = TempFolder::new();
        let core = core(&folder);
        sign_in(&core);
        keep(&core, RAMP);
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW);
        let step = sync.receive(200, changes(&[], &[], "3"), None);
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent");

        core.signed_out().unwrap();
        assert_eq!(held(&core), [RAMP]);
        assert!(note(&core).starts_with("Kept on this device only"));
        assert_eq!(core.storage.read(FILE).unwrap(), None);
        // Kept while signed out: the device's alone, until the sign-in.
        keep(&core, JAEGERKAMP);

        sign_in(&core);
        let sync = Arc::clone(&core).kept_sites_sync();
        let step = sync.start(NOW + 60);
        assert_eq!(
            request(&step).path,
            "/v1/account/kept-sites/changes?limit=100"
        );
        // The account has the ramp already, with its note: it is not
        // offered again. The other is.
        let mut ramp = on_the_account();
        ramp["site"] = json!({"id": RAMP, "position": {"lat": 47.6652212, "lon": 11.7954245},
            "uses": ["takeoff"]});
        let step = sync.receive(200, changes(&[ramp], &[], "5"), None);
        assert_eq!(body(&step)["site"]["id"], JAEGERKAMP);
        assert_eq!(body(&step).get("kept_at"), None);
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent");
        assert_eq!(held(&core), [RAMP, JAEGERKAMP]);
    }

    #[test]
    fn a_list_the_server_starts_again_says_what_the_account_no_longer_keeps() {
        let folder = TempFolder::new();
        let core = core(&folder);
        sign_in(&core);
        keep(&core, RAMP);
        keep(&core, OTHER_RAMP);
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW);
        let mut step = sync.receive(200, changes(&[], &[], "3"), None);
        while matches!(step, SyncStep::Send { .. }) {
            step = sync.receive(200, taken(&step), None);
        }
        assert_eq!(summary(&step), "2 sent");
        keep(&core, JAEGERKAMP);

        // The server no longer knows the cursor and names everything the
        // account keeps: the other ramp only. The ramp is gone here; what
        // was kept here since stays and is sent.
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW + 60);
        let mut other = on_the_account();
        other["site"] = json!({"id": OTHER_RAMP, "position": {"lat": 47.6652, "lon": 11.7955},
            "uses": ["takeoff"]});
        let all = json!({"kept": [other], "forgotten": [], "cursor": "20", "more": false,
            "reset": true});
        let step = sync.receive(200, all.to_string().into_bytes(), None);
        assert_eq!(body(&step)["site"]["id"], JAEGERKAMP);
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent, 1 removed here");
        assert_eq!(held(&core), [OTHER_RAMP, JAEGERKAMP]);
    }

    #[test]
    fn what_the_server_will_not_take_stays_on_the_device() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, RAMP);
        keep(&core, JAEGERKAMP);
        sign_in(&core);

        // An account that is full: the run stops in words, the sites
        // stay here, and the next run offers them again.
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW);
        let step = sync.receive(200, changes(&[], &[], "3"), None);
        assert_eq!(request(&step).method, "PUT");
        let full = br#"{"error":{"code":"too_many_kept_sites","message":"at most 200"}}"#;
        let step = sync.receive(409, full.to_vec(), None);
        assert!(
            matches!(&step, SyncStep::Failed { message, needs_sign_in: false }
                if message.starts_with("Your Venti account keeps as many sites as it can.")),
            "{step:?}"
        );
        assert_eq!(held(&core), [RAMP, JAEGERKAMP]);

        // A site the server refuses as a record is not sent again.
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW + 60);
        let step = sync.receive(200, changes(&[], &[], "3"), None);
        assert_eq!(request(&step).method, "PUT");
        let invalid = br#"{"error":{"code":"invalid_kept_site","message":"site.name"}}"#;
        let step = sync.receive(400, invalid.to_vec(), None);
        let step = sync.receive(200, taken(&step), None);
        assert_eq!(summary(&step), "1 sent, 1 not taken by the server");
        assert_eq!(held(&core), [RAMP, JAEGERKAMP]);

        // A session that has ended: the pilot signs in again, and nothing
        // of what the device holds is lost.
        keep(&core, OTHER_RAMP);
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW + 120);
        let ended = br#"{"error":{"code":"unauthorized","message":"no"}}"#;
        let step = sync.receive(401, ended.to_vec(), None);
        assert!(matches!(
            step,
            SyncStep::Failed {
                needs_sign_in: true,
                ..
            }
        ));
        assert!(core.account().unwrap().unwrap().needs_sign_in);
        assert_eq!(held(&core).len(), 3);
        let step = Arc::clone(&core).kept_sites_sync().start(NOW + 180);
        assert!(matches!(
            step,
            SyncStep::Failed {
                needs_sign_in: true,
                ..
            }
        ));
    }

    #[test]
    fn the_servers_recorded_answers_are_read() {
        let folder = TempFolder::new();
        let core = core(&folder);
        sign_in(&core);
        let sync = Arc::clone(&core).kept_sites_sync();
        sync.start(NOW);
        let step = sync.receive(200, golden("kept_site_changes.json"), None);
        assert_eq!(summary(&step), "1 fetched");
        let kept = core.kept_sites(None).unwrap();
        assert_eq!(kept.sites.len(), 1);
        assert_eq!(kept.sites[0].site.id, JAEGERKAMP);
        assert_eq!(kept.sites[0].site.name, "Fluggelände Jägerkamp/Nagelspitz");
        assert_eq!(kept.sites[0].as_of_unix_s, Some(1_795_028_700));
        assert_eq!(kept.credits[0].licence, "ODbL 1.0");

        // And what the core sends is what the server's example sends.
        core.forget_site(JAEGERKAMP.to_owned()).unwrap();
        let expected: KeepSiteRequest =
            serde_json::from_slice(&golden("kept_site_keep_request.json")).unwrap();
        let answer: KeptSiteResponse = serde_json::from_slice(&golden("kept_site.json")).unwrap();
        assert_eq!(answer.kept.site, expected.site);
    }
}
