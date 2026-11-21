//! The dumps: each of Venti's own two layers of sites whole, as files
//! that anyone fetches without an account.
//!
//! **What a dump is.** A layer ([`Layer`]) has three files, at addresses
//! that stay: its sites as they stand as GeoJSON and as CSV, and every
//! site it ever had with every revision as JSON Lines. A file holds one
//! layer and nothing else: never a site of the other layer, never a record
//! of another source. The licence is [`Layer::licence`]'s, and every file
//! but the CSV says it in itself, with the layer, the time the file was
//! made and the layer's change number it stands at; the list
//! (`GET /v1/dumps`) says the same of each file, the CSV included, with
//! its size and checksum. A program goes on from a dump with
//! `GET /v1/sites/changes` and the dump's `cursor`.
//!
//! **What is in it is what anyone may read through the API**, and no
//! more. A removed site is in the history file with its revisions and in
//! neither of the other two. Of a site that whoever runs the server took
//! down the history file says that it is removed and when, as
//! `GET /v1/sites/changes` does, and nothing of what it said. A token's
//! name stays out of a revision, as it does in a site's history.
//! Confirmations are in a dump as each site's count and no further: who
//! confirmed, on which day and with which line is a pilot's word that
//! goes with the account, and whether it belongs in a file that is handed
//! out for good is not decided. Flights are in no dump.
//!
//! **Made when asked for, and kept.** There is no schedule and no file on
//! a disk. The first request for a layer's dump makes it from the store
//! and keeps it in memory, compressed; from then on it is handed out as it
//! is. After [`Limits::dump_remake_after`](crate::accounts::Limits) the
//! next request makes the server look whether the layer has changed since
//! the dump's `cursor`, which is one small question to the store, and only
//! if it has is the dump made anew. One dump of a layer is made at a time;
//! a request that arrives meanwhile gets the one before, or waits where
//! there is none yet. So what reading a whole layer costs is bounded by
//! the clock and by nothing a stranger does. A dump that nobody asks for
//! is never made.
//!
//! A schedule that writes files to a folder would have needed a folder,
//! a setting for it and a line in the runbook, and would have made every
//! night what is asked for now and then. It becomes the better way when a
//! layer outgrows what is sensibly held in memory: [`MAX_SITES`] and
//! [`MAX_REVISIONS`] are where this module stops, with an answer that says
//! so, and never with a file that is silently short.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, RwLock};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use venti_api_types::v1::{
    CSV_MEDIA_TYPE, DumpContent, DumpFile, DumpFormat, DumpLayer, DumpStamp, DumpsResponse,
    ErrorCode, GEOJSON_MEDIA_TYPE, JSON_LINES_MEDIA_TYPE, SITES_CSV_COLUMNS, SiteHistoryHead,
    SiteHistoryLine, SitePilot, SiteRecord, SiteRemoval, SiteRevision, sites_dump_path,
};
use venti_domain::{SiteId, Timestamp};
use venti_store::{PilotSite, Store};

use super::cache::{self, Keeper};
use super::error::ApiError;
use super::log::Sparse;
use super::{Api, sites};
use crate::pilot_sites::Layer;

/// The route of a dump's files. Its last step is the layer's name and the
/// ending of the format: `venti.geojson`.
pub(crate) const FILE_ROUTE: &str = "/v1/dumps/sites/{file}";

/// How many sites are asked of the store at a time while a dump is made.
const PAGE: usize = 500;

/// The most sites of one layer, standing and removed, that a dump is made
/// of. Beyond this the files are not made and their addresses say so.
///
/// By the development mode's made-up sites, a standing site is some 650
/// bytes of the GeoJSON file and some 230 of the CSV file, and a revision
/// some 500 of the history file: a layer at both bounds is some 120 MB
/// written out, held for the moment of the making beside what was read,
/// and a tenth of that or less as it is kept, compressed (an estimate:
/// no layer of that size has been made). "Tens of thousands" of sites is
/// what this is for; a layer beyond it wants files on a disk, made on a
/// schedule.
const MAX_SITES: usize = 50_000;

/// The most revisions of one layer that a dump is made of: three for
/// every site of [`MAX_SITES`].
const MAX_REVISIONS: usize = 150_000;

/// The three files of a layer, in the order the list names them.
const FILES: [(DumpContent, DumpFormat, &str); 3] = [
    (DumpContent::Sites, DumpFormat::Geojson, GEOJSON_MEDIA_TYPE),
    (DumpContent::Sites, DumpFormat::Csv, CSV_MEDIA_TYPE),
    (
        DumpContent::History,
        DumpFormat::Jsonl,
        JSON_LINES_MEDIA_TYPE,
    ),
];

/// One file as it is kept.
struct File {
    /// Its size as it is handed out without compression.
    bytes: u64,
    /// The SHA-256 of those bytes.
    sha256: String,
    /// The validator of an answer that carries it.
    tag: String,
    /// The file, compressed with gzip.
    gzip: Bytes,
}

/// The dump of one layer as it was made last.
pub(crate) struct Made {
    stamp: DumpStamp,
    /// The change number the dump stands at: the stamp's `cursor`.
    cursor: i64,
    standing: u32,
    removed: u32,
    revisions: u32,
    /// As [`FILES`] orders them.
    files: [File; 3],
}

/// A made dump, and when the server last looked whether its layer has
/// changed since.
#[derive(Clone)]
struct Held {
    made: Arc<Made>,
    looked_at: Timestamp,
}

/// What is kept of one layer.
#[derive(Default)]
struct Slot {
    held: RwLock<Option<Held>>,
    /// When the layer was last found larger than a dump is made of, if it
    /// was when it was last read.
    outgrown_at: RwLock<Option<Timestamp>>,
    /// Whoever holds this looks and makes; nobody else does meanwhile.
    turn: Arc<tokio::sync::Mutex<()>>,
}

impl Slot {
    fn held(&self) -> Option<Held> {
        self.held
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn hold(&self, made: Arc<Made>, looked_at: Timestamp) {
        *self
            .held
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Held { made, looked_at });
        self.outgrew(None);
    }

    fn outgrown_at(&self) -> Option<Timestamp> {
        *self
            .outgrown_at
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Notes that the layer was found too large at `at`, and keeps no
    /// dump of it from then on: one from before would be handed out for
    /// good and say nothing of what came after. `None` takes the note
    /// back.
    fn outgrew(&self, at: Option<Timestamp>) {
        *self
            .outgrown_at
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = at;
        if at.is_some() {
            *self
                .held
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
    }
}

/// The dumps the server keeps: one of each layer, once it was asked for.
pub(crate) struct Dumps {
    remake_after: chrono::Duration,
    /// [`MAX_SITES`] and [`MAX_REVISIONS`], but in this module's tests.
    max_sites: usize,
    max_revisions: usize,
    layers: [Slot; 2],
}

impl Dumps {
    pub(crate) fn new(remake_after: chrono::Duration) -> Self {
        Self {
            remake_after,
            max_sites: MAX_SITES,
            max_revisions: MAX_REVISIONS,
            layers: [Slot::default(), Slot::default()],
        }
    }

    fn slot(&self, layer: Layer) -> &Slot {
        match layer {
            Layer::Partners => &self.layers[0],
            Layer::Pilots => &self.layers[1],
        }
    }

    /// Whether a dump that was looked at then is handed out now as it is.
    fn is_fresh(&self, looked_at: Timestamp, now: Timestamp) -> bool {
        let age = now - looked_at;
        age >= chrono::Duration::zero() && age < self.remake_after
    }
}

/// Why a dump could not be made.
#[derive(Debug)]
enum Unmade {
    /// The store could not be asked.
    Store(venti_store::StoreError),
    /// The layer is larger than a dump is made of.
    TooLarge,
    /// A record could not be written out.
    Unwritable,
}

impl From<venti_store::StoreError> for Unmade {
    fn from(error: venti_store::StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<Unmade> for ApiError {
    fn from(unmade: Unmade) -> Self {
        match unmade {
            Unmade::Store(error) => error.into(),
            Unmade::TooLarge => {
                static GATE: Sparse = Sparse::new();
                GATE.error(format_args!(
                    "a layer of sites has more than {MAX_SITES} sites or {MAX_REVISIONS} \
                     revisions: its dump is not made"
                ));
                ApiError::busy(
                    "this layer has outgrown the dump this server makes; GET /v1/sites/changes \
                     reads it page by page",
                )
            }
            Unmade::Unwritable => ApiError::internal(),
        }
    }
}

impl<S: Store> Api<S> {
    /// The dump of a layer: the one kept, or one made now where none is
    /// kept or the kept one is due for a look and the layer has changed.
    ///
    /// The look and the making run as a task of their own, so that a
    /// request that gives up does not undo them: the next one finds the
    /// dump made.
    async fn dump(self: &Arc<Self>, layer: Layer) -> Result<Arc<Made>, ApiError> {
        let slot = self.dumps.slot(layer);
        let held = slot.held();
        if let Some(held) = &held
            && self.dumps.is_fresh(held.looked_at, self.clock.now())
        {
            return Ok(held.made.clone());
        }
        // A layer that was too large a moment ago is not read again to
        // find that it still is.
        if slot
            .outgrown_at()
            .is_some_and(|at| self.dumps.is_fresh(at, self.clock.now()))
        {
            return Err(Unmade::TooLarge.into());
        }
        let turn = match slot.turn.clone().try_lock_owned() {
            Ok(turn) => turn,
            // Somebody is at it. The dump before is as good as any other
            // answer of this moment; without one there is the wait.
            Err(_) => match held {
                Some(held) => return Ok(held.made),
                None => slot.turn.clone().lock_owned().await,
            },
        };
        let api = self.clone();
        let work = tokio::spawn(async move {
            let made = api.looked_at(layer).await;
            drop(turn);
            made
        });
        match work.await {
            Ok(Ok(made)) => Ok(made),
            // No dump of a layer that has outgrown it, an older one
            // neither.
            Ok(Err(Unmade::TooLarge)) => Err(Unmade::TooLarge.into()),
            // The dump before is handed out while the store cannot be
            // asked; the failure is in the log.
            Ok(Err(unmade)) => {
                let error = ApiError::from(unmade);
                held.map(|held| held.made).ok_or(error)
            }
            Err(_) => Err(ApiError::internal()),
        }
    }

    /// Looks whether the layer has changed since the dump that is kept,
    /// and makes a new one if it has or if none is kept. Called by whoever
    /// holds the layer's turn.
    async fn looked_at(&self, layer: Layer) -> Result<Arc<Made>, Unmade> {
        let slot = self.dumps.slot(layer);
        let now = self.clock.now();
        if let Some(held) = slot.held() {
            // Whoever had the turn before may just have looked.
            if self.dumps.is_fresh(held.looked_at, now) {
                return Ok(held.made);
            }
            let since = self
                .store
                .pilot_site_changes(&layer.source(), held.made.cursor, 1)
                .await?;
            // A change number below the dump's is a database that is not
            // the one the dump was made from.
            if since.sites.is_empty() && since.latest >= held.made.cursor {
                slot.hold(held.made.clone(), now);
                return Ok(held.made);
            }
        }
        let read = match self.read_layer(layer).await {
            Err(Unmade::TooLarge) => {
                slot.outgrew(Some(now));
                return Err(Unmade::TooLarge);
            }
            read => read?,
        };
        let made = tokio::task::spawn_blocking(move || write_out(layer, &read, now))
            .await
            .map_err(|_| Unmade::Unwritable)??;
        let made = Arc::new(made);
        slot.hold(made.clone(), now);
        Ok(made)
    }

    /// Everything a dump of the layer holds, read from the store.
    ///
    /// The sites are read as `GET /v1/sites/changes` reads them, from the
    /// first change to the last: a site that changes meanwhile takes a
    /// later number and is read again, so every site is in the result as
    /// it was at the change number the result stands at, or later. Of a
    /// revision that was written after its site was read nothing is kept:
    /// the history is the one of the sites as they were read.
    async fn read_layer(&self, layer: Layer) -> Result<LayerRead, Unmade> {
        let source = layer.source();
        let mut sites: BTreeMap<SiteId, PilotSite> = BTreeMap::new();
        let mut cursor = 0_i64;
        loop {
            let page = self.store.pilot_site_changes(&source, cursor, PAGE).await?;
            let Some(last) = page.sites.last() else {
                break;
            };
            cursor = last.change;
            for site in page.sites {
                sites.insert(site.site.id.clone(), site);
            }
            if sites.len() > self.dumps.max_sites {
                return Err(Unmade::TooLarge);
            }
            if !page.more {
                break;
            }
        }

        // A site that was taken down shows nothing of what it said: its
        // revisions are not asked for.
        let shown: Vec<SiteId> = sites
            .values()
            .filter(|site| !site.taken_down)
            .map(|site| site.site.id.clone())
            .collect();
        let mut revisions: BTreeMap<SiteId, Vec<venti_store::SiteRevision>> = BTreeMap::new();
        let mut count = 0_usize;
        for ids in shown.chunks(PAGE) {
            for revision in self.store.pilot_sites_revisions(ids).await? {
                let Some(site) = sites.get(&revision.record.id) else {
                    continue;
                };
                if revision.number > site.revision {
                    continue;
                }
                count += 1;
                revisions
                    .entry(revision.record.id.clone())
                    .or_default()
                    .push(revision);
            }
            if count > self.dumps.max_revisions {
                return Err(Unmade::TooLarge);
            }
        }
        Ok(LayerRead {
            sites,
            revisions,
            cursor,
        })
    }
}

/// A layer as it was read from the store.
struct LayerRead {
    /// Every site the layer ever had, by identifier.
    sites: BTreeMap<SiteId, PilotSite>,
    /// The revisions of every site that shows any, by number.
    revisions: BTreeMap<SiteId, Vec<venti_store::SiteRevision>>,
    /// The change number all of it stands at.
    cursor: i64,
}

/// The collection a GeoJSON file is. The stamp's fields are its "foreign
/// members", which RFC 7946 allows beside its own.
#[derive(Serialize)]
struct Collection<'a> {
    r#type: &'static str,
    #[serde(flatten)]
    stamp: &'a DumpStamp,
    features: Vec<Feature<'a>>,
}

#[derive(Serialize)]
struct Feature<'a> {
    r#type: &'static str,
    id: &'a SiteId,
    geometry: Point,
    properties: &'a SiteRecord,
}

/// A site's position: longitude first, as GeoJSON has it, and no height.
/// GeoJSON's third number is a height above the ellipsoid, and a site's
/// `elevation_m` is above the sea: it stays in the properties.
#[derive(Serialize)]
struct Point {
    r#type: &'static str,
    coordinates: [f64; 2],
}

fn unwritable<T>(_: T) -> Unmade {
    Unmade::Unwritable
}

/// Writes the three files of a layer from what was read of it.
fn write_out(layer: Layer, read: &LayerRead, now: Timestamp) -> Result<Made, Unmade> {
    let licence = layer.licence();
    let stamp = DumpStamp {
        layer: layer.source(),
        provider: licence.provider.to_owned(),
        licence: licence.name.to_owned(),
        licence_url: licence.url.to_owned(),
        notice: licence.notice.to_owned(),
        generated_at: now,
        cursor: read.cursor.to_string(),
    };

    let standing: Vec<SiteRecord> = read
        .sites
        .values()
        .filter(|site| site.stands())
        .map(|site| {
            Ok(SiteRecord {
                standing: sites::standing(site).map_err(unwritable)?,
                site: site.site.clone(),
            })
        })
        .collect::<Result<_, Unmade>>()?;

    let geojson = serde_json::to_vec(&Collection {
        r#type: "FeatureCollection",
        stamp: &stamp,
        features: standing
            .iter()
            .map(|record| Feature {
                r#type: "Feature",
                id: &record.site.id,
                geometry: Point {
                    r#type: "Point",
                    coordinates: [record.site.position.lon(), record.site.position.lat()],
                },
                properties: record,
            })
            .collect(),
    })
    .map_err(unwritable)?;

    let mut csv = csv_line(SITES_CSV_COLUMNS.iter().map(|name| (*name).to_owned()));
    for record in &standing {
        csv.push_str(&csv_line(csv_fields(record)?));
    }

    let mut lines: Vec<SiteHistoryLine> = Vec::with_capacity(read.sites.len());
    let mut revisions = 0_usize;
    for site in read.sites.values() {
        let own: Vec<SiteRevision> = read
            .revisions
            .get(&site.site.id)
            .into_iter()
            .flatten()
            .cloned()
            .map(|revision| sites::revision(revision).map_err(unwritable))
            .collect::<Result<_, Unmade>>()?;
        revisions += own.len();
        let removal = match site.removed_at {
            None => None,
            Some(at) => Some(SiteRemoval {
                id: site.site.id.clone(),
                // As `GET /v1/sites/changes` says it: a site that was
                // taken down shows nothing of what it said, its name
                // included.
                name: site.site.name.clone().filter(|_| !site.taken_down),
                at,
                by: sites::pilot(site.changed_by.as_ref()).map_err(unwritable)?,
            }),
        };
        lines.push(SiteHistoryLine {
            id: site.site.id.clone(),
            revision: site.revision,
            removed: !site.stands(),
            removal,
            revisions: own,
        });
    }
    let count = |number: usize| u32::try_from(number).map_err(unwritable);
    let head = SiteHistoryHead {
        stamp: stamp.clone(),
        sites: count(lines.len())?,
        revisions: count(revisions)?,
    };
    let mut history = serde_json::to_vec(&head).map_err(unwritable)?;
    history.push(b'\n');
    for line in &lines {
        serde_json::to_writer(&mut history, line).map_err(unwritable)?;
        history.push(b'\n');
    }

    Ok(Made {
        cursor: read.cursor,
        standing: count(standing.len())?,
        removed: count(lines.len() - standing.len())?,
        revisions: head.revisions,
        files: [kept(&geojson)?, kept(csv.as_bytes())?, kept(&history)?],
        stamp,
    })
}

/// A file as it is kept: compressed, with its size and checksum.
fn kept(file: &[u8]) -> Result<File, Unmade> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(file).map_err(unwritable)?;
    let gzip = encoder.finish().map_err(unwritable)?;
    let sha256: String = Sha256::digest(file)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(File {
        bytes: file.len() as u64,
        tag: cache::etag(file),
        sha256,
        gzip: gzip.into(),
    })
}

/// A value as JSON writes it, for a number or a truth value.
fn plain(value: impl Serialize) -> Result<String, Unmade> {
    match serde_json::to_value(value).map_err(unwritable)? {
        serde_json::Value::Null => Ok(String::new()),
        serde_json::Value::String(text) => Ok(text),
        other => Ok(other.to_string()),
    }
}

/// The fields of a site's line of the CSV file, as
/// [`SITES_CSV_COLUMNS`] orders them.
fn csv_fields(record: &SiteRecord) -> Result<[String; 25], Unmade> {
    let (site, standing) = (&record.site, &record.standing);
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    let uses = site
        .uses
        .iter()
        .map(plain)
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let wind = site
        .wind_directions
        .iter()
        .map(|sector| {
            Ok(format!(
                "{}-{}",
                plain(sector.from_deg)?,
                plain(sector.to_deg)?
            ))
        })
        .collect::<Result<Vec<_>, Unmade>>()?
        .join(" ");
    let handle = |pilot: &Option<SitePilot>| {
        pilot
            .as_ref()
            .map(|pilot| pilot.handle.to_string())
            .unwrap_or_default()
    };
    Ok([
        site.id.to_string(),
        text(&site.name),
        plain(site.position.lat())?,
        plain(site.position.lon())?,
        plain(site.elevation_m)?,
        uses,
        wind,
        plain(site.paragliding)?,
        plain(site.hang_gliding)?,
        plain(site.official)?,
        plain(site.discouraged)?,
        text(&site.url),
        text(&site.website),
        text(&site.description),
        text(&site.fee),
        plain(site.guest_guidelines)?,
        text(&site.no_flight_time),
        standing.revision.to_string(),
        plain(standing.created_at)?,
        handle(&standing.created_by),
        plain(standing.changed_at)?,
        handle(&standing.changed_by),
        standing.confirmed.pilots.to_string(),
        plain(standing.confirmed.latest_day)?,
        site.id.source().to_string(),
    ])
}

/// One line of a CSV file as RFC 4180 writes it: the fields separated by
/// commas, the line ended by a carriage return and a line feed. A field
/// with a comma, a quotation mark or a line break in it is put between
/// quotation marks, and a quotation mark in it is written twice. A text is
/// otherwise as its author wrote it: nothing is put before a `=` to keep a
/// spreadsheet from taking it for a formula.
fn csv_line(fields: impl IntoIterator<Item = String>) -> String {
    let mut line = String::new();
    for (place, field) in fields.into_iter().enumerate() {
        if place > 0 {
            line.push(',');
        }
        if field.contains([',', '"', '\n', '\r']) {
            line.push('"');
            line.push_str(&field.replace('"', "\"\""));
            line.push('"');
        } else {
            line.push_str(&field);
        }
    }
    line.push_str("\r\n");
    line
}

/// What the list says of a layer's dump.
fn listed(layer: Layer, made: &Made) -> DumpLayer {
    DumpLayer {
        stamp: made.stamp.clone(),
        sites: made.standing,
        removed: made.removed,
        revisions: made.revisions,
        files: FILES
            .iter()
            .zip(&made.files)
            .map(|((content, format, media_type), file)| DumpFile {
                content: *content,
                format: *format,
                path: sites_dump_path(layer.name(), *format),
                media_type: (*media_type).to_owned(),
                bytes: file.bytes,
                sha256: file.sha256.clone(),
            })
            .collect(),
    }
}

/// `GET /v1/dumps`: the dump of each layer, with every file's licence,
/// time, change number, size and checksum.
pub(crate) async fn list<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let mut layers = Vec::new();
    for layer in Layer::ALL {
        layers.push(listed(layer, &*api.dump(layer).await?));
    }
    let answer = DumpsResponse {
        layers,
        attributions: Layer::ALL.map(Layer::attribution).to_vec(),
    };
    // Checked before every reuse, as the files are: a list that was kept
    // for a minute would name the checksum of a file that is no more.
    let tag = cache::etag(&serde_json::to_vec(&answer).map_err(|_| ApiError::internal())?);
    Ok(cache::unchanged(&headers, &tag, Keeper::Anyone)
        .unwrap_or_else(|| cache::tagged_json(&tag, Keeper::Anyone, &answer)))
}

/// The layer and the file of it that the last step of a path names.
fn named(file: &str) -> Option<(Layer, usize)> {
    Layer::ALL.into_iter().find_map(|layer| {
        let ending = file.strip_prefix(layer.name())?;
        FILES
            .iter()
            .position(|(_, format, _)| format.ending() == ending)
            .map(|place| (layer, place))
    })
}

/// Whether a request takes an answer compressed with gzip.
fn takes_gzip(request: &HeaderMap) -> bool {
    request
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|coding| {
            let mut parts = coding.split(';').map(str::trim);
            let name = parts.next().unwrap_or_default();
            let refused = parts.any(|part| {
                part.strip_prefix("q=")
                    .and_then(|q| q.parse::<f32>().ok())
                    .is_some_and(|q| q <= 0.0)
            });
            (name.eq_ignore_ascii_case("gzip") || name == "*") && !refused
        })
}

/// `GET /v1/dumps/sites/{layer}.geojson`, `.csv` and `.history.jsonl`:
/// one file of a layer's dump, as it was made last.
pub(crate) async fn file<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let (layer, place) = named(&name).ok_or_else(|| {
        ApiError::missing(
            ErrorCode::NotFound,
            "there is no dump with this name; GET /v1/dumps lists the files there are",
        )
    })?;
    let made = api.dump(layer).await?;
    let file = &made.files[place];
    if let Some(unchanged) = cache::unchanged(&headers, &file.tag, Keeper::Anyone) {
        return Ok(unchanged);
    }
    let media_type = HeaderValue::from_static(FILES[place].2);
    // The file is kept compressed and goes out as it is kept to whoever
    // takes it so, which the layer that compresses the other answers
    // then leaves alone. For anyone else it is unpacked here.
    let mut response = if takes_gzip(&headers) {
        (
            [
                (header::CONTENT_TYPE, media_type),
                (header::CONTENT_ENCODING, HeaderValue::from_static("gzip")),
            ],
            file.gzip.clone(),
        )
            .into_response()
    } else {
        let mut whole = Vec::with_capacity(usize::try_from(file.bytes).unwrap_or_default());
        GzDecoder::new(&file.gzip[..])
            .read_to_end(&mut whole)
            .map_err(|_| ApiError::internal())?;
        ([(header::CONTENT_TYPE, media_type)], whole).into_response()
    };
    cache::validators(&mut response, &file.tag, Keeper::Anyone);
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("accept-encoding"));
    debug_assert_eq!(response.status(), StatusCode::OK);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: &[&str]) -> String {
        csv_line(fields.iter().map(|field| (*field).to_owned()))
    }

    #[test]
    fn a_csv_line_quotes_what_would_break_it_and_nothing_else() {
        assert_eq!(line(&["a", "", "b c"]), "a,,b c\r\n");
        assert_eq!(line(&["a,b"]), "\"a,b\"\r\n");
        assert_eq!(line(&["say \"hi\""]), "\"say \"\"hi\"\"\"\r\n");
        assert_eq!(line(&["two\nlines", "x"]), "\"two\nlines\",x\r\n");
        assert_eq!(line(&["cr\rhere"]), "\"cr\rhere\"\r\n");
        // Spaces, a semicolon and what a spreadsheet takes for a formula
        // are left as they are.
        assert_eq!(line(&[" a ", "b;c", "=1+1"]), " a ,b;c,=1+1\r\n");
        assert_eq!(line(&[]), "\r\n");
    }

    #[test]
    fn a_file_is_named_by_its_layer_and_its_ending() {
        assert!(matches!(named("venti.geojson"), Some((Layer::Partners, 0))));
        assert!(matches!(named("venti.csv"), Some((Layer::Partners, 1))));
        assert!(matches!(
            named("venti.history.jsonl"),
            Some((Layer::Partners, 2))
        ));
        assert!(matches!(
            named("venti-pilots.geojson"),
            Some((Layer::Pilots, 0))
        ));
        assert!(matches!(
            named("venti-pilots.history.jsonl"),
            Some((Layer::Pilots, 2))
        ));
        for no in [
            "",
            "venti",
            "venti.json",
            "venti.geojson.gz",
            "openstreetmap.geojson",
            "venti-pilot.csv",
            "venti.jsonl",
        ] {
            assert!(named(no).is_none(), "{no}");
        }
    }

    #[test]
    fn gzip_is_taken_where_the_request_says_so() {
        let asks = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static(value));
            takes_gzip(&headers)
        };
        assert!(asks("gzip"));
        assert!(asks("br, GZIP;q=0.5"));
        assert!(asks("*"));
        assert!(!asks("identity"));
        assert!(!asks("br, gzip;q=0"));
        assert!(!takes_gzip(&HeaderMap::new()));
    }

    #[test]
    fn a_kept_file_unpacks_to_what_it_was_and_names_its_checksum() {
        let file = kept(b"abc").unwrap();
        let mut whole = Vec::new();
        GzDecoder::new(&file.gzip[..])
            .read_to_end(&mut whole)
            .unwrap();
        assert_eq!(whole, b"abc");
        assert_eq!(file.bytes, 3);
        // The SHA-256 of "abc", as FIPS 180-2 gives it.
        assert_eq!(
            file.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// The development mode's API with dumps that are looked at after
    /// five minutes and made of at most this much.
    async fn bounded(max_sites: usize, max_revisions: usize) -> Arc<Api<venti_store::MemoryStore>> {
        use crate::accounts::password::Cost;
        use crate::flights::analyser::{Analyser, Where};
        use crate::flights::share::PublicUrl;

        let settings = crate::dev::settings(
            PublicUrl::new("https://venti.example").unwrap(),
            Analyser::new(Where::Thread),
            Cost::TESTS_ONLY,
        );
        let mut api = crate::dev::recorded_api(settings).await.unwrap();
        api.dumps = Dumps {
            max_sites,
            max_revisions,
            ..Dumps::new(chrono::Duration::minutes(5))
        };
        Arc::new(api)
    }

    #[tokio::test]
    async fn a_layer_that_has_outgrown_the_dump_gets_none() {
        // The made-up layers have two sites each; the partners' three
        // revisions and the crowdsourced one's five.
        for (max_sites, max_revisions, refused) in [
            (1, 100, [true, true]),
            (2, 4, [false, true]),
            (2, 2, [true, true]),
            (2, 5, [false, false]),
        ] {
            let api = bounded(max_sites, max_revisions).await;
            for (layer, refused) in Layer::ALL.into_iter().zip(refused) {
                let asked = api.dump(layer).await;
                assert_eq!(asked.is_err(), refused, "{max_sites} {max_revisions}");
                let slot = api.dumps.slot(layer);
                // What was found is kept for as long as a dump is: the
                // layer is not read again to find it out anew.
                assert_eq!(slot.outgrown_at().is_some(), refused);
                assert_eq!(slot.held().is_none(), refused);
                if let Err(error) = asked {
                    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
                    assert_eq!(error.code(), ErrorCode::Unavailable);
                    let again = api.dump(layer).await.err().unwrap();
                    assert_eq!(again.status(), StatusCode::SERVICE_UNAVAILABLE);
                }
            }
        }
    }

    #[test]
    fn a_layer_found_too_large_keeps_no_dump_from_before() {
        let at = crate::adapters::recorded_at();
        let slot = Slot::default();
        let read = LayerRead {
            sites: BTreeMap::new(),
            revisions: BTreeMap::new(),
            cursor: 0,
        };
        let made = Arc::new(write_out(Layer::Pilots, &read, at).unwrap());
        slot.hold(made.clone(), at);
        assert!(slot.held().is_some() && slot.outgrown_at().is_none());
        slot.outgrew(Some(at));
        assert!(slot.held().is_none());
        assert_eq!(slot.outgrown_at(), Some(at));
        // A dump that could be made again takes the note back.
        slot.hold(made, at);
        assert!(slot.held().is_some() && slot.outgrown_at().is_none());
    }

    #[test]
    fn a_dump_is_fresh_for_as_long_as_the_bound_says() {
        let at = crate::adapters::recorded_at();
        let dumps = Dumps::new(chrono::Duration::minutes(5));
        assert!(dumps.is_fresh(at, at));
        assert!(dumps.is_fresh(at, at + chrono::Duration::seconds(299)));
        assert!(!dumps.is_fresh(at, at + chrono::Duration::minutes(5)));
        // A clock that went back is no reason to keep a dump for longer.
        assert!(!dumps.is_fresh(at, at - chrono::Duration::seconds(1)));
        // The development mode's bound: a look with every request.
        assert!(!Dumps::new(chrono::Duration::zero()).is_fresh(at, at));
    }
}
