//! Airspace from the pilot's Venti server, kept on the device.
//!
//! A pilot with a server need not hunt for airspace files. The server
//! collects airspace from its sources once a day; the app asks it for the
//! airspace around where the pilot is and keeps the answer. From then on
//! the maps and the cockpit use it without a network, beside the pilot's
//! own files.
//!
//! The answer comes in layers, one per source, each under its own licence.
//! They are kept one file per source and never merged, and every screen
//! that shows them shows the source's credit and its notice.
//!
//! What is kept is one answer: the airspace around one place. Asking again
//! somewhere else replaces it. Where it ends, the app knows nothing, and
//! says so: see [`crate::AirspaceCheck::OutsideFiles`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use venti_api_types::v1::{AirspaceLayer, AirspaceRequest, AirspaceResponse, Attribution};
use venti_domain::{Airspace, BoundingBox, GeoPoint, SourceId};

use crate::server::answer;
use crate::wind::credit;
use crate::{Bounds, Core, CoreError, Credit};

/// The folder the kept airspace is stored in.
const FOLDER: &str = "airspace-server";

/// The file that says what is kept. The airspaces of each source are in a
/// file of their own beside it, which this file names.
const INDEX: &str = "airspace-server/kept.json";

/// Half the height of the area asked for, in degrees: about 220 km to the
/// north and to the south. A day's flight does not leave it.
const HALF_HEIGHT_DEG: f64 = 2.0;

/// Half its width, in degrees: about 220 km to each side at the latitude
/// of the Alps.
const HALF_WIDTH_DEG: f64 = 3.0;

/// Closer than this to the edge of the kept area, in degrees of latitude,
/// the airspace is asked for again around the new place: about 80 km, so
/// that a flight starting here stays inside what is kept.
const EDGE_MARGIN_DEG: f64 = 0.75;

/// Kept airspace older than this is asked for again. The sources publish
/// once a day at most.
const FRESH_FOR_S: i64 = 24 * 3600;

/// What is kept: one answer of one server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Kept {
    /// The server it came from, as the shell names it.
    server: String,
    /// When it was fetched from the server, by the device's clock, seconds
    /// since 1970-01-01 UTC.
    kept_unix_s: i64,
    /// The area that was asked for.
    asked: BoundingBox,
    /// One entry for each source.
    pub(crate) layers: Vec<KeptLayer>,
}

/// The airspace of one source, as kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct KeptLayer {
    /// The file that holds its airspaces, inside the folder.
    file: String,
    /// The source.
    pub(crate) source: SourceId,
    /// How many airspaces the file holds.
    airspaces: u32,
    /// When the server fetched the source's set.
    fetched_at: DateTime<Utc>,
    /// The date of the oldest file the set was read from, by the source's
    /// own word.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) file_date: Option<DateTime<Utc>>,
    /// The parts of the area asked for in which this layer holds every
    /// airspace its source publishes. Empty when the server did not say:
    /// then the layer speaks for no place, only for the airspaces in it.
    #[serde(default)]
    pub(crate) complete_within: Vec<BoundingBox>,
    /// The countries whose airspace the layer holds, as ISO 3166-1
    /// alpha-2 codes. Where the area reaches into another country, the
    /// layer holds nothing of it.
    #[serde(default)]
    countries: Vec<String>,
    /// The source's credit, with what its licence asks to be shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attribution: Option<Attribution>,
}

impl KeptLayer {
    /// Who publishes the layer, for a heading and for a warning's origin.
    pub(crate) fn provider(&self) -> String {
        self.attribution
            .as_ref()
            .map_or_else(|| self.source.to_string(), |credit| credit.provider.clone())
    }
}

/// The airspace of one source as kept from the server, for a list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct KeptAirspace {
    /// The source's identifier.
    pub source: String,
    /// Who publishes it.
    pub provider: String,
    /// How many airspaces are kept.
    pub airspaces: u32,
    /// When the server fetched them from the provider, seconds since
    /// 1970-01-01 UTC.
    pub fetched_unix_s: i64,
    /// When the provider wrote the oldest of its files, if it says.
    pub written_unix_s: Option<i64>,
    /// When the app fetched them from the server, by the device's clock.
    pub kept_unix_s: i64,
    /// The area that was asked for. Outside it nothing is kept.
    pub area: Bounds,
    /// Whether the server said for which area the airspace is complete.
    /// Without that the cockpit cannot tell "no airspace near" from "no
    /// data here", and says that it is not checking.
    pub says_what_it_covers: bool,
    /// The countries whose airspace is kept, as ISO 3166-1 alpha-2 codes;
    /// empty when the server does not say. The kept area reaches across
    /// their borders, and of the countries beyond nothing is kept: a
    /// screen names the countries beside the airspace.
    pub countries: Vec<String>,
    /// The credit and the notice to show wherever the airspace is shown.
    pub credit: Option<Credit>,
}

/// What asking the server for airspace did.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct AirspaceFetch {
    /// Whether the answer was kept. `false` when the server has nothing to
    /// say about the place, neither airspace nor that there is none: what
    /// was kept before then stays.
    pub replaced: bool,
    /// What is kept now.
    pub kept: Vec<KeptAirspace>,
}

/// The path and query to `GET` from the server for the airspace around a
/// place, to keep on the device.
#[uniffi::export]
pub fn server_airspace_path(lat: f64, lon: f64) -> Result<String, CoreError> {
    Ok(AirspaceRequest {
        bbox: area_around(lat, lon)?,
    }
    .path_and_query())
}

/// The area asked for around a place.
fn area_around(lat: f64, lon: f64) -> Result<BoundingBox, CoreError> {
    BoundingBox::new(
        (lat - HALF_HEIGHT_DEG).clamp(-90.0, 90.0),
        (lon - HALF_WIDTH_DEG).clamp(-180.0, 180.0),
        (lat + HALF_HEIGHT_DEG).clamp(-90.0, 90.0),
        (lon + HALF_WIDTH_DEG).clamp(-180.0, 180.0),
    )
    .map_err(|_| CoreError::Server {
        message: "The app could not ask the server about this place.".to_owned(),
    })
}

#[uniffi::export]
impl Core {
    /// Reads the server's answer to [`server_airspace_path`] for the same
    /// place and keeps it in place of what was kept before. `server` is
    /// the server's address as the shell names it; `now_unix_s` the
    /// current time.
    ///
    /// An answer that says nothing about the place changes nothing: one
    /// without airspace from a server that does not hold the place's
    /// airspace either. What was kept for another place is still true
    /// there. An answer without airspace for a place the server's set is
    /// whole for is kept: there, "none" is knowledge.
    pub fn keep_server_airspace(
        &self,
        server: String,
        lat: f64,
        lon: f64,
        status: u16,
        body: Vec<u8>,
        now_unix_s: i64,
    ) -> Result<AirspaceFetch, CoreError> {
        let asked = area_around(lat, lon)?;
        let response: AirspaceResponse = answer(status, &body)?;
        let layers: Vec<&AirspaceLayer> = response
            .layers
            .iter()
            // A layer filed under a name the pilot's own files use would
            // be taken for one of them; no server of ours sends one.
            .filter(|layer| !is_file_source(&layer.source))
            .filter(|layer| {
                !layer.airspaces.is_empty() || !complete_within(layer, &asked).is_empty()
            })
            .collect();
        if layers.is_empty() {
            return Ok(AirspaceFetch {
                replaced: false,
                kept: self.kept_airspace()?,
            });
        }

        // The new files are written beside the old under names of their
        // own, and the index last: a write that fails half-way leaves what
        // was kept before whole.
        let mut kept = Kept {
            server,
            kept_unix_s: now_unix_s,
            asked,
            layers: Vec::new(),
        };
        for layer in layers {
            let file = format!("{now_unix_s}-{}.json", layer.source);
            let records = serde_json::to_vec(&layer.airspaces).map_err(|_| unwritable())?;
            self.storage
                .write(&format!("{FOLDER}/{file}"), &records)
                .map_err(storage_error)?;
            kept.layers.push(KeptLayer {
                file,
                source: layer.source.clone(),
                airspaces: u32::try_from(layer.airspaces.len()).unwrap_or(u32::MAX),
                fetched_at: layer.fetched_at,
                file_date: layer.file_date,
                complete_within: complete_within(layer, &asked),
                countries: layer.countries.clone(),
                attribution: response
                    .attributions
                    .iter()
                    .find(|credit| credit.source == layer.source)
                    .cloned(),
            });
        }
        let index = serde_json::to_vec(&kept).map_err(|_| unwritable())?;
        self.storage.write(INDEX, &index).map_err(storage_error)?;
        self.sweep_kept_airspace(&kept);
        self.forget_airspace();
        Ok(AirspaceFetch {
            replaced: true,
            kept: views(&kept),
        })
    }

    /// The airspace kept from the server, one entry for each source.
    pub fn kept_airspace(&self) -> Result<Vec<KeptAirspace>, CoreError> {
        let kept = self.kept_when_fetched().map_err(storage_error)?;
        Ok(kept.as_ref().map(views).unwrap_or_default())
    }

    /// Removes the airspace kept from the server. It no longer warns.
    pub fn delete_kept_airspace(&self) -> Result<(), CoreError> {
        for name in self.storage.list(FOLDER).map_err(storage_error)? {
            self.storage.remove(&name).map_err(storage_error)?;
        }
        self.forget_airspace();
        Ok(())
    }

    /// Whether the app should ask the server for airspace now: nothing is
    /// kept from this server, what is kept is more than a day old, or the
    /// place is outside the kept area or near its edge.
    pub fn kept_airspace_is_due(
        &self,
        server: String,
        lat: f64,
        lon: f64,
        now_unix_s: i64,
    ) -> bool {
        let Some(kept) = self.kept() else {
            return true;
        };
        if kept.server != server || !(0..FRESH_FOR_S).contains(&(now_unix_s - kept.kept_unix_s)) {
            return true;
        }
        let inside = BoundingBox::new(
            kept.asked.south() + EDGE_MARGIN_DEG,
            kept.asked.west() + EDGE_MARGIN_DEG * 1.5,
            kept.asked.north() - EDGE_MARGIN_DEG,
            kept.asked.east() - EDGE_MARGIN_DEG * 1.5,
        );
        match (inside, GeoPoint::new(lat, lon)) {
            (Ok(inside), Ok(place)) => !inside.contains(place),
            // A place that is none is no reason to ask.
            (_, Err(_)) => false,
            (Err(_), _) => true,
        }
    }
}

impl Core {
    /// What is kept, as the index says. `None` when nothing is, or when
    /// the index cannot be read: then nothing kept is used.
    pub(crate) fn kept(&self) -> Option<Kept> {
        self.kept_when_fetched().ok().flatten()
    }

    /// [`kept`](Self::kept) for a caller that must not take "not fetched
    /// yet" for "nothing kept". Storage that hands a file over only after
    /// it was asked for it fails here with
    /// [`std::io::ErrorKind::WouldBlock`]; the host fetches the file and
    /// makes its call again.
    pub(crate) fn kept_when_fetched(&self) -> std::io::Result<Option<Kept>> {
        match self.storage.read(INDEX) {
            Ok(bytes) => Ok(bytes.and_then(|bytes| serde_json::from_slice(&bytes).ok())),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Err(error),
            Err(_) => Ok(None),
        }
    }

    /// The airspaces of one kept layer. A file that cannot be read gives
    /// none, and must not cost the pilot the airspace of the others. A
    /// file that storage has yet to fetch is another matter: a set built
    /// without it would draw and warn as if its airspace did not exist, so
    /// that is an error, with the kind that says "fetch and call again".
    pub(crate) fn kept_airspaces(&self, layer: &KeptLayer) -> std::io::Result<Vec<Airspace>> {
        let bytes = match self.storage.read(&format!("{FOLDER}/{}", layer.file)) {
            Ok(Some(bytes)) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Err(error),
            _ => return Ok(Vec::new()),
        };
        let airspaces: Vec<Airspace> = serde_json::from_slice(&bytes).unwrap_or_default();
        // Only what the layer's source filed: the source is what the
        // credit and the warning's origin are found by.
        Ok(airspaces
            .into_iter()
            .filter(|airspace| airspace.id.source() == &layer.source)
            .collect())
    }

    /// Removes the files of earlier answers. What cannot be removed now
    /// is removed after the next answer.
    fn sweep_kept_airspace(&self, kept: &Kept) {
        let Ok(names) = self.storage.list(FOLDER) else {
            return;
        };
        for name in names {
            let wanted = name == INDEX
                || kept
                    .layers
                    .iter()
                    .any(|layer| name == format!("{FOLDER}/{}", layer.file));
            if !wanted {
                let _ = self.storage.remove(&name);
            }
        }
    }
}

/// The parts of the area asked for in which a layer is complete.
fn complete_within(layer: &AirspaceLayer, asked: &BoundingBox) -> Vec<BoundingBox> {
    layer
        .covers
        .iter()
        .filter_map(|cover| overlap(cover, asked))
        .collect()
}

/// The part two boxes share, if they share any.
fn overlap(a: &BoundingBox, b: &BoundingBox) -> Option<BoundingBox> {
    BoundingBox::new(
        a.south().max(b.south()),
        a.west().max(b.west()),
        a.north().min(b.north()),
        a.east().min(b.east()),
    )
    .ok()
}

/// Whether a source identifier is one the pilot's own files are filed
/// under (see `source_for` in the airspace module).
pub(crate) fn is_file_source(source: &SourceId) -> bool {
    source.as_str().starts_with("file-")
}

fn views(kept: &Kept) -> Vec<KeptAirspace> {
    kept.layers
        .iter()
        .map(|layer| KeptAirspace {
            source: layer.source.to_string(),
            provider: layer.provider(),
            airspaces: layer.airspaces,
            fetched_unix_s: layer.fetched_at.timestamp(),
            written_unix_s: layer.file_date.map(|date| date.timestamp()),
            kept_unix_s: kept.kept_unix_s,
            area: Bounds {
                south: kept.asked.south(),
                west: kept.asked.west(),
                north: kept.asked.north(),
                east: kept.asked.east(),
            },
            says_what_it_covers: !layer.complete_within.is_empty(),
            countries: layer.countries.clone(),
            credit: layer.attribution.as_ref().map(credit),
        })
        .collect()
}

fn storage_error(error: std::io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The airspace from the server could not be read or written: {error}."),
    }
}

fn unwritable() -> CoreError {
    CoreError::Storage {
        message: "The airspace from the server could not be written.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::airspace::AirspaceWatch;
    use crate::testing::TempFolder;
    use crate::{AirspaceOrigin, Bounds};

    /// 18 November 2026, 19:05 UTC.
    const NOW: i64 = 1_795_028_700;

    const SERVER: &str = "https://venti.example";

    /// Hochfilzen, where the recorded answer's two airspaces lie.
    const PLACE: (f64, f64) = (47.49, 12.66);

    fn core(folder: &TempFolder) -> Arc<Core> {
        Core::new(folder.path().to_str().unwrap().to_owned()).unwrap()
    }

    /// An answer the server's own tests pin.
    fn golden(name: &str) -> Vec<u8> {
        let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/tests/golden/");
        std::fs::read(format!("{folder}{name}")).unwrap()
    }

    /// The server's answer for Hochfilzen: two airspaces, whole for the
    /// first region.
    fn recorded() -> Vec<u8> {
        golden("airspace.json")
    }

    /// The recorded answer, saying that its layer is complete inside these
    /// boxes, each south, west, north, east.
    fn recorded_covering(boxes: &[[f64; 4]]) -> Vec<u8> {
        let mut answer: serde_json::Value = serde_json::from_slice(&recorded()).unwrap();
        answer["layers"][0]["covers"] = boxes
            .iter()
            .map(|[south, west, north, east]| {
                serde_json::json!({"south": south, "west": west, "north": north, "east": east})
            })
            .collect();
        serde_json::to_vec(&answer).unwrap()
    }

    fn keep(core: &Core, body: Vec<u8>, now: i64) -> AirspaceFetch {
        core.keep_server_airspace(SERVER.to_owned(), PLACE.0, PLACE.1, 200, body, now)
            .unwrap()
    }

    fn point(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::new(lat, lon).unwrap()
    }

    #[test]
    fn the_airspace_around_a_place_is_asked_for() {
        assert_eq!(
            server_airspace_path(47.7, 11.8).unwrap(),
            "/v1/airspace?bbox=8.8,45.7,14.8,49.7"
        );
        // Near a pole and the date line the question is still a valid one.
        assert_eq!(
            server_airspace_path(89.0, 179.0).unwrap(),
            "/v1/airspace?bbox=176,87,180,90"
        );
        assert!(server_airspace_path(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn an_answer_is_kept_listed_used_and_removed() {
        let folder = TempFolder::new();
        let core = core(&folder);
        assert_eq!(core.kept_airspace().unwrap(), Vec::new());

        let fetch = keep(&core, recorded(), NOW);
        assert!(fetch.replaced);
        assert_eq!(fetch.kept, core.kept_airspace().unwrap());
        let layer = &fetch.kept[0];
        assert_eq!(
            (
                layer.source.as_str(),
                layer.provider.as_str(),
                layer.airspaces
            ),
            ("openaip", "openAIP", 2)
        );
        assert!(layer.says_what_it_covers);
        // The countries the development server says it holds, whichever
        // they are today: Germany and Austria first.
        assert_eq!(layer.countries[..2], ["DE", "AT"]);
        assert_eq!(layer.fetched_unix_s, NOW);
        assert_eq!(layer.written_unix_s, Some(NOW));
        assert_eq!(layer.kept_unix_s, NOW);
        assert_eq!(
            layer.area,
            Bounds {
                south: 45.49,
                west: 9.66,
                north: 49.49,
                east: 15.66
            }
        );
        // The licence asks for the credit and for the notice.
        let credit = layer.credit.as_ref().unwrap();
        assert_eq!(credit.licence, "CC BY-NC 4.0");
        assert!(credit.text.starts_with("Data used comes from openAIP"));
        assert!(
            credit
                .notice
                .as_ref()
                .unwrap()
                .starts_with("OpenAIP data is not certified")
        );

        // The maps and the cockpit get it with the pilot's own files.
        let set = core.airspace().unwrap();
        assert_eq!(set.count(), 2);
        let shapes = set.in_area(Bounds {
            south: 47.0,
            west: 12.0,
            north: 48.0,
            east: 13.0,
        });
        assert_eq!(shapes.len(), 2);
        assert!(
            shapes
                .iter()
                .any(|shape| shape.name == "LOD22 (HOCHFILZEN)")
        );

        // A new core finds it again: it is on the device.
        drop(set);
        let again = Core::new(folder.path().to_str().unwrap().to_owned()).unwrap();
        assert_eq!(again.airspace().unwrap().count(), 2);

        core.delete_kept_airspace().unwrap();
        assert_eq!(core.kept_airspace().unwrap(), Vec::new());
        assert_eq!(core.airspace().unwrap().count(), 0);
    }

    #[test]
    fn a_warning_says_that_its_airspace_is_from_the_server_and_how_old_it_is() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, recorded(), NOW + 3600);

        let mut watch = AirspaceWatch::new(core.airspace().unwrap());
        watch.feed(
            NOW as f64,
            point(PLACE.0, PLACE.1),
            Some(1500.0),
            None,
            None,
            Some(9.0),
            Some(90.0),
            Some(0.0),
        );
        let warnings = watch.warnings();
        assert!(!warnings.is_empty());
        for warning in warnings {
            assert_eq!(warning.origin, AirspaceOrigin::Server);
            assert_eq!(warning.source, "openAIP");
            // The date the provider wrote its file, not when it was fetched.
            assert_eq!(warning.data_unix_s, Some(NOW));
        }
    }

    #[test]
    fn an_answer_that_says_nothing_about_the_place_changes_nothing() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, recorded(), NOW);

        // A server that holds no airspace at all.
        let empty = br#"{"layers": [], "attributions": []}"#.to_vec();
        let fetch = keep(&core, empty, NOW + 60);
        assert!(!fetch.replaced);
        assert_eq!(fetch.kept[0].kept_unix_s, NOW);

        // A place far outside what the server collects for: its layer is
        // there, without airspace, and whole only somewhere else.
        let fetch = core
            .keep_server_airspace(
                SERVER.to_owned(),
                60.0,
                25.0,
                200,
                golden("airspace_none.json"),
                NOW + 120,
            )
            .unwrap();
        assert!(!fetch.replaced);
        assert_eq!(core.airspace().unwrap().count(), 2);
    }

    #[test]
    fn a_quiet_valley_the_server_is_whole_for_is_kept_as_knowledge() {
        let folder = TempFolder::new();
        let core = core(&folder);

        let fetch = keep(&core, golden("airspace_none.json"), NOW);
        assert!(fetch.replaced);
        assert_eq!(fetch.kept[0].airspaces, 0);
        assert!(fetch.kept[0].says_what_it_covers);
        // The credit comes with the layer although it is empty.
        assert!(fetch.kept[0].credit.is_some());

        // No airspace near, and that is a statement: the cockpit checks.
        let set = core.airspace().unwrap();
        assert_eq!(set.count(), 0);
        let mut watch = AirspaceWatch::new(set);
        watch.feed(
            NOW as f64,
            point(PLACE.0, PLACE.1),
            Some(1500.0),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(watch.check(NOW as f64), crate::AirspaceCheck::Checking);
        assert!(watch.warnings().is_empty());
    }

    #[test]
    fn a_newer_answer_replaces_the_older_and_leaves_none_of_its_files() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, recorded(), NOW);
        // The next day the server has one airspace fewer.
        let mut answer: serde_json::Value = serde_json::from_slice(&recorded()).unwrap();
        answer["layers"][0]["airspaces"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        let fetch = keep(&core, serde_json::to_vec(&answer).unwrap(), NOW + 86_400);

        assert!(fetch.replaced);
        assert_eq!(fetch.kept[0].airspaces, 1);
        assert_eq!(core.airspace().unwrap().count(), 1);
        let mut files = core.storage.list(FOLDER).unwrap();
        files.sort();
        assert_eq!(
            files,
            [
                format!("{FOLDER}/{}-openaip.json", NOW + 86_400),
                INDEX.to_owned()
            ]
        );
    }

    #[test]
    fn a_refusal_and_an_unreadable_answer_keep_what_was_kept() {
        let folder = TempFolder::new();
        let core = core(&folder);
        keep(&core, recorded(), NOW);

        let too_large = br#"{"error": {"code": "box_too_large", "message": "x"}}"#.to_vec();
        let refused = core
            .keep_server_airspace(SERVER.to_owned(), PLACE.0, PLACE.1, 400, too_large, NOW)
            .unwrap_err();
        assert_eq!(
            refused.to_string(),
            "The server holds more airspace around this place than one answer carries."
        );
        assert!(
            core.keep_server_airspace(
                SERVER.to_owned(),
                PLACE.0,
                PLACE.1,
                200,
                b"<html>".to_vec(),
                NOW
            )
            .is_err()
        );
        assert_eq!(core.airspace().unwrap().count(), 2);
    }

    #[test]
    fn storage_that_fetches_is_asked_for_what_is_kept_before_anything_is_said_about_it() {
        // A browser's storage hands a file over only after it was asked for
        // it. Kept there and stored, nothing of the answer is in memory.
        let staged = Arc::new(crate::StagedStorage::new([]));
        let core = Core::with_storage(Arc::clone(&staged) as Arc<dyn crate::Storage>).unwrap();
        keep(&core, recorded(), NOW);
        let store: std::collections::BTreeMap<String, Vec<u8>> = staged
            .take_staged()
            .into_iter()
            .filter_map(|file| Some((file.name, file.contents?)))
            .collect();
        staged.release();
        // Makes a call the way a page does: again, after fetching what it
        // wanted, until it no longer fails for want of a file.
        let fetching = |call: &dyn Fn() -> Result<usize, CoreError>| {
            let mut fetched = Vec::new();
            loop {
                match call() {
                    Ok(count) => return (count, fetched),
                    Err(error) => {
                        assert!(matches!(error, CoreError::Storage { .. }), "{error:?}");
                        let wanted = staged.take_wanted();
                        assert_eq!(wanted.len(), 1, "{error}");
                        staged.provide(&wanted[0], store.get(&wanted[0]).cloned());
                        fetched.extend(wanted);
                    }
                }
            }
        };

        // "Nothing is kept" is not said of an index that was not looked at.
        let (layers, fetched) = fetching(&|| Ok(core.kept_airspace()?.len()));
        assert_eq!(layers, 1);
        assert_eq!(fetched, [INDEX]);
        staged.release();

        // Nor is a set built without the airspace of a layer not fetched
        // yet: a map would show none there and say nothing.
        let (airspaces, fetched) = fetching(&|| Ok(core.airspace()?.count() as usize));
        assert_eq!(airspaces, 2);
        assert_eq!(fetched.len(), 2, "{fetched:?}");
        assert_eq!(fetched[0], INDEX);
        assert!(fetched[1].starts_with("airspace-server/"), "{fetched:?}");

        // With the index at hand, whether to ask again is answered from it.
        assert!(!core.kept_airspace_is_due(SERVER.to_owned(), PLACE.0, PLACE.1, NOW + 60));
    }

    #[test]
    fn asking_again_is_due_after_a_day_near_the_edge_or_for_another_server() {
        let folder = TempFolder::new();
        let core = core(&folder);
        let due = |server: &str, lat, lon, now| {
            core.kept_airspace_is_due(server.to_owned(), lat, lon, now)
        };
        assert!(due(SERVER, PLACE.0, PLACE.1, NOW));

        keep(&core, recorded(), NOW);
        assert!(!due(SERVER, PLACE.0, PLACE.1, NOW + 3600));
        // A hundred kilometres on, still well inside what is kept.
        assert!(!due(SERVER, PLACE.0 + 0.9, PLACE.1, NOW + 3600));
        // Near the northern edge, and beyond it.
        assert!(due(SERVER, PLACE.0 + 1.5, PLACE.1, NOW + 3600));
        assert!(due(SERVER, PLACE.0 + 5.0, PLACE.1, NOW + 3600));
        // A day later, with a clock set back, and from another server.
        assert!(due(SERVER, PLACE.0, PLACE.1, NOW + 86_401));
        assert!(due(SERVER, PLACE.0, PLACE.1, NOW - 60));
        assert!(due("https://other.example", PLACE.0, PLACE.1, NOW + 3600));
    }

    #[test]
    fn the_set_speaks_only_for_where_the_server_says_it_is_complete() {
        let folder = TempFolder::new();
        let core = core(&folder);

        // A server that does not say: its airspace warns, but nowhere is
        // "no airspace near" taken from it.
        let fetch = keep(&core, recorded_covering(&[]), NOW);
        assert!(!fetch.kept[0].says_what_it_covers);
        assert!(!core.airspace().unwrap().covers(point(PLACE.0, PLACE.1)));

        // The first region's box: the Alps up to 48.5 degrees north.
        let fetch = keep(
            &core,
            recorded_covering(&[[46.3, 9.4, 48.5, 17.2]]),
            NOW + 60,
        );
        assert!(fetch.kept[0].says_what_it_covers);
        let set = core.airspace().unwrap();
        assert!(set.covers(point(PLACE.0, PLACE.1)));
        assert!(set.covers(point(48.4, 12.0)));
        // North of the region the server has nothing, and the app knows.
        assert!(!set.covers(point(48.6, 12.0)));
        // East of what was asked for nothing is kept, whatever the server
        // has there.
        assert!(!set.covers(point(47.5, 16.5)));

        // A cockpit north of the region says that it is not checking.
        let mut watch = AirspaceWatch::new(set);
        watch.feed(
            NOW as f64,
            point(48.6, 12.0),
            Some(1500.0),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(watch.check(NOW as f64), crate::AirspaceCheck::OutsideFiles);
    }
}
