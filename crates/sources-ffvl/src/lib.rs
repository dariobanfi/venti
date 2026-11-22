//! Adapter for the flying sites of the Fédération Française de Vol Libre.
//!
//! The FFVL publishes its list of the places where its disciplines are
//! practised, "plus de 2500 terrains de pratique", on data.gouv.fr under
//! the Licence Ouverte 2.0, as one file updated every day
//! (<https://www.data.gouv.fr/datasets/terrains-de-pratique-de-la-ffvl-federation-francaise-de-vol-libre>,
//! read on 2026-11-22). The list covers paragliding, hang gliding,
//! speed-riding, kites, kites on water and snow, and boomerangs.
//!
//! # The requests
//!
//! The file's address carries the moment it was written and changes every
//! day (`…/20261122-071002/terrains.json`), and the address data.gouv.fr
//! gives as stable redirects to another host, which the server's fetcher
//! refuses. So a run asks two addresses: the dataset's description from
//! data.gouv.fr's API ([`DATASET`]), which names the newest file and when
//! it was written, and then that file. The file holds every site of
//! France; the pipeline keeps those inside the run's box.
//!
//! # What is read
//!
//! Each entry of the file is one place, `terrain` in the FFVL's word. The
//! fields have no documentation on the dataset's page; what each is taken
//! to mean was read from the values of the file of 2026-11-22 and is *to
//! verify* with the FFVL where it says so.
//!
//! - `suid`, the place's number, which the identifier keeps:
//!   `ffvl-sites:<suid>`.
//! - `toponym`, the name, with the blanks at its ends removed.
//! - `latitude`, `longitude`; `altitude`, read as metres above sea level
//!   (the unit is not stated, to verify; the values, 0 to 3,800, are
//!   metres).
//! - `possible_usages_text`, the disciplines, separated by `;`:
//!   `parapente` and `delta` make the site one for paragliders and for
//!   hang gliders. A place for neither (a kite beach, a speed-riding slope,
//!   a field for boomerangs) is no flying site here and is left out.
//! - `flying_functions_text`, the uses, separated by `;`: `décollage`
//!   (take-off), `atterrissage` (landing), `treuil` (winch), `pente-école`
//!   (training slope). `tyrolienne`, a zip line, is no use of a site. A
//!   place that names none is kept without a use.
//! - `wind_orientations_ok`, the wind directions the place works in, as
//!   French compass points separated by `;` (a few entries use `,`): `N`,
//!   `NE`, `E`, `SE`, `S`, `SO`, `O`, `NO`. In French `O` is west
//!   (*ouest*); the file bears it out, as the eight points stand side by
//!   side in its entries. A direction that is none of these (a range such
//!   as `S-SO`) leaves the place without directions, and the report says
//!   so: a wrong direction on a take-off is worse than none.
//! - `status`: `actif`, `actif_autorisation_systematique_requise`
//!   (open, with leave to be asked each time), `non_praticable_temporaire`
//!   (closed for now), `interdit_sauf_autorisation` (closed unless leave
//!   is given), `interdit` (closed). A closed place is left out, as
//!   OpenStreetMap's places that say every use is `no`; a place closed for
//!   now or unless leave is given is kept and marked `discouraged`.
//! - `convention_type`, whether the FFVL or a club has an agreement with
//!   the landowner: a place under one of the agreements the file names
//!   (`convention_ffvl`, `convention_orale_ffvl`, `convention_usage`,
//!   `convention_non_ffvl`, `propriete_ffvl`, the FFVL's own ground) is
//!   marked `official`; `pas_de_convention` (none), one being negotiated
//!   and one unknown say nothing. *To verify* with the FFVL that an
//!   agreement is what makes a place official in its own sense.
//! - `description`, the place described in its author's words, passed on
//!   as written with its line ends made `\n`. A description longer than a
//!   site record holds, or with a character that does not print, is left
//!   out and the report says so: nothing is cut.
//! - `terrain_polygon`, for some places, the outline as a list of
//!   `[latitude, longitude]` that is not closed: it is closed here.
//!
//! Never read: the names and addresses of the people responsible for a
//! place (`terrain_coordonnes_referent`), the clubs, the access, the
//! emergency services, the warnings and the rules in words. They are the
//! FFVL's to show on its own pages.

pub mod fixtures;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{GeoPoint, Site, SiteId, SiteOutline, SiteUse, SourceId, Timestamp, WindSector};
use venti_sources::{Manifest, ParsedSites, PlanCtx, Request, SiteSource, Skipped, SourceError};

/// The dataset's description in data.gouv.fr's API, which names the
/// newest file.
pub const DATASET: &str = "https://www.data.gouv.fr/api/1/datasets/6ab3b9c6d0cf12014567cad0/";

/// The title of the file of the dataset that the adapter reads. The
/// dataset also has a GeoJSON file with fewer fields and no outlines.
const FILE_TITLE: &str = "terrains.json";

/// The hosts the file may be named on: data.gouv.fr's file host and its
/// own.
const FILE_HOSTS: [&str; 2] = ["https://static.data.gouv.fr/", "https://www.data.gouv.fr/"];

/// The tag of the request for the dataset's description.
const DATASET_TAG: &str = "dataset";

/// The tag of the request for the file.
const FILE_TAG: &str = "terrains";

/// The flying sites of the FFVL.
#[derive(Debug, Clone)]
pub struct Ffvl {
    manifest: Manifest,
}

impl Ffvl {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        Self {
            manifest: Manifest::from_toml(include_str!("../manifest.toml"))
                .expect("the manifest compiled into venti-sources-ffvl is valid"),
        }
    }
}

impl Default for Ffvl {
    fn default() -> Self {
        Self::new()
    }
}

impl SiteSource for Ffvl {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The dataset's description; the file follows from it.
    fn plan(&self, _ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new(DATASET_TAG, DATASET)]
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        _ctx: &PlanCtx,
    ) -> Result<ParsedSites, SourceError> {
        match request.tag.as_str() {
            DATASET_TAG => dataset(body),
            FILE_TAG => terrains(&self.manifest.id, body),
            other => Err(SourceError::UnknownRequest(other.to_owned())),
        }
    }
}

#[derive(Deserialize)]
struct Dataset {
    resources: Vec<Resource>,
}

#[derive(Deserialize)]
struct Resource {
    title: String,
    url: String,
    last_modified: Option<String>,
}

/// Reads the dataset's description: the address of the newest file, as a
/// request to follow, and when it was written, as the date of the data.
fn dataset(body: &[u8]) -> Result<ParsedSites, SourceError> {
    let dataset: Dataset = serde_json::from_slice(body).map_err(|e| {
        SourceError::Malformed(format!("the dataset's description cannot be read: {e}"))
    })?;
    let file = dataset
        .resources
        .into_iter()
        .find(|resource| resource.title == FILE_TITLE)
        .ok_or_else(|| {
            SourceError::Malformed(format!("the dataset names no file called {FILE_TITLE}"))
        })?;
    let plain = |c: char| c.is_ascii_graphic() && !matches!(c, '"' | '<' | '>' | '\\');
    if !FILE_HOSTS.iter().any(|host| file.url.starts_with(host)) || !file.url.chars().all(plain) {
        return Err(SourceError::Malformed(format!(
            "the dataset names its file at {:?}, which is not on data.gouv.fr",
            file.url
        )));
    }
    Ok(ParsedSites {
        as_of: file.last_modified.as_deref().and_then(time),
        follow_up: vec![Request::new(FILE_TAG, file.url)],
        ..ParsedSites::default()
    })
}

/// A moment as data.gouv.fr writes it: RFC 3339, with an offset.
fn time(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[derive(Deserialize)]
struct Terrain {
    suid: u64,
    toponym: Option<String>,
    latitude: f64,
    longitude: f64,
    /// Read apart, so that a height that is no number costs the height.
    altitude: Option<serde_json::Value>,
    status: String,
    possible_usages_text: Option<String>,
    flying_functions_text: Option<String>,
    wind_orientations_ok: Option<String>,
    convention_type: Option<String>,
    description: Option<String>,
    /// Read apart, so that an outline that cannot be read costs the
    /// outline and not the place.
    terrain_polygon: Option<serde_json::Value>,
}

/// Reads the file: one site for every place for paragliders or hang
/// gliders that is not closed.
fn terrains(source: &SourceId, body: &[u8]) -> Result<ParsedSites, SourceError> {
    let entries: Vec<serde_json::Value> = serde_json::from_slice(body)
        .map_err(|e| SourceError::Malformed(format!("the file is not a list of places: {e}")))?;
    let mut parsed = ParsedSites::default();
    for entry in entries {
        // Each entry is read on its own, so that one that cannot be read
        // costs only itself. Its number is found first, to report it by.
        let called = entry.get("suid").map_or_else(
            || "a place without a number".to_owned(),
            |n| format!("suid {n}"),
        );
        let terrain: Terrain = match serde_json::from_value(entry) {
            Ok(terrain) => terrain,
            Err(error) => {
                parsed
                    .skipped
                    .push(Skipped::new(&called, error.to_string()));
                continue;
            }
        };
        match site(source, terrain, &called, &mut parsed.skipped) {
            Ok(Some(site)) => parsed.sites.push(site),
            Ok(None) => {}
            Err(reason) => parsed.skipped.push(Skipped::new(&called, reason)),
        }
    }
    Ok(parsed)
}

/// One place as a site record. `Ok(None)` for a place that is no flying
/// site at all (a kite beach), which is left out without a word, since the
/// file holds hundreds; `Err` for a flying site that is left out, with the
/// reason. What is left out of a record that is kept goes to `skipped`.
fn site(
    source: &SourceId,
    terrain: Terrain,
    called: &str,
    skipped: &mut Vec<Skipped>,
) -> Result<Option<Site>, String> {
    let disciplines: Vec<&str> = list(terrain.possible_usages_text.as_deref());
    let paragliding = disciplines.contains(&"parapente");
    let hang_gliding = disciplines.contains(&"delta");
    if !paragliding && !hang_gliding {
        return Ok(None);
    }
    let discouraged = match terrain.status.as_str() {
        "actif" | "actif_autorisation_systematique_requise" => false,
        "non_praticable_temporaire" | "interdit_sauf_autorisation" => true,
        "interdit" => return Err("the FFVL marks it closed (interdit)".to_owned()),
        other => return Err(format!("a status the adapter does not know: {other:?}")),
    };
    let position = GeoPoint::new(terrain.latitude, terrain.longitude)
        .map_err(|error| format!("no place: {error}"))?;
    let id = SiteId::new(source.clone(), terrain.suid.to_string())
        .map_err(|error| format!("no identifier: {error}"))?;

    let mut uses = Vec::new();
    for function in list(terrain.flying_functions_text.as_deref()) {
        match function {
            "décollage" => uses.push(SiteUse::Takeoff),
            "atterrissage" => uses.push(SiteUse::Landing),
            "treuil" => uses.push(SiteUse::Towing),
            "pente-école" => uses.push(SiteUse::Training),
            // A zip line is no use of a flying site.
            "tyrolienne" => {}
            other => skipped.push(Skipped::new(
                called,
                format!("a use the adapter does not know, left out: {other:?}"),
            )),
        }
    }

    let wind_directions = match directions(terrain.wind_orientations_ok.as_deref()) {
        Ok(sectors) => sectors,
        Err(reason) => {
            skipped.push(Skipped::new(called, reason));
            Vec::new()
        }
    };

    let description = terrain
        .description
        .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"))
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    let description = match description {
        Some(text)
            if text.chars().count() > Site::MAX_DESCRIPTION_CHARS
                || text.chars().any(|c| c.is_control() && c != '\n') =>
        {
            skipped.push(Skipped::new(
                called,
                format!(
                    "its description of {} characters is longer than a site's {} or has a \
                     character that does not print, and is left out",
                    text.chars().count(),
                    Site::MAX_DESCRIPTION_CHARS
                ),
            ));
            None
        }
        other => other,
    };

    let outline = match terrain.terrain_polygon.as_ref().map(outline) {
        Some(Ok(outline)) => outline,
        Some(Err(reason)) => {
            skipped.push(Skipped::new(called, reason));
            None
        }
        None => None,
    };

    let official = match terrain.convention_type.as_deref() {
        Some(
            "convention_ffvl"
            | "convention_orale_ffvl"
            | "convention_usage"
            | "convention_non_ffvl"
            | "propriete_ffvl",
        ) => Some(true),
        _ => None,
    };

    Ok(Some(Site {
        id,
        name: terrain
            .toponym
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty()),
        position,
        elevation_m: terrain
            .altitude
            .as_ref()
            .and_then(serde_json::Value::as_f64),
        uses,
        wind_directions,
        paragliding: Some(paragliding),
        hang_gliding: Some(hang_gliding),
        official,
        discouraged,
        url: None,
        website: None,
        description,
        fee: None,
        guest_guidelines: None,
        no_flight_time: None,
        outline,
    }))
}

/// The words of a list the FFVL writes with `;` between them, a few
/// entries with `,`, without empty ones.
fn list(text: Option<&str>) -> Vec<&str> {
    text.unwrap_or_default()
        .split([';', ','])
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .collect()
}

/// The wind directions of a place: one sector of a single direction for
/// each compass point named, each once, in the order first named. A word
/// that is no French compass point makes the whole list unreadable.
pub fn directions(text: Option<&str>) -> Result<Vec<WindSector>, String> {
    let mut sectors: Vec<WindSector> = Vec::new();
    for word in list(text) {
        let deg = match word {
            "N" => 0.0,
            "NE" => 45.0,
            "E" => 90.0,
            "SE" => 135.0,
            "S" => 180.0,
            "SO" => 225.0,
            "O" => 270.0,
            "NO" => 315.0,
            other => {
                return Err(format!(
                    "wind directions {:?} name {other:?}, which is no compass point the FFVL \
                     writes; the directions are left out",
                    text.unwrap_or_default()
                ));
            }
        };
        let sector = WindSector::single(deg);
        if !sectors.contains(&sector) {
            sectors.push(sector);
        }
    }
    Ok(sectors)
}

/// The outline of a place: its points as `[latitude, longitude]`, closed
/// here. `None` for fewer than three points.
fn outline(polygon: &serde_json::Value) -> Result<Option<SiteOutline>, String> {
    let points: Vec<[f64; 2]> = serde_json::from_value(polygon.clone())
        .map_err(|error| format!("its outline cannot be read: {error}"))?;
    let mut ring = points
        .iter()
        .map(|&[lat, lon]| GeoPoint::new(lat, lon))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("its outline cannot be read: {error}"))?;
    if ring.len() < 3 {
        return Ok(None);
    }
    if ring.first() != ring.last() {
        ring.push(ring[0]);
    }
    SiteOutline::new(ring)
        .map(Some)
        .map_err(|error| format!("its outline cannot be drawn: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_compass_points_are_degrees_and_o_is_west() {
        let sectors = directions(Some("N;NE;E;SE;S;SO;O;NO;")).unwrap();
        let degrees: Vec<f64> = sectors.iter().map(|s| s.from_deg).collect();
        assert_eq!(
            degrees,
            [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]
        );
        assert!(sectors.iter().all(|s| s.from_deg == s.to_deg));
        // A comma, a repeat and a blank change nothing.
        let west = directions(Some("O, O;NO")).unwrap();
        assert_eq!(west, [WindSector::single(270.0), WindSector::single(315.0)]);
        assert!(directions(None).unwrap().is_empty());
        // A range is not read.
        assert!(directions(Some("S-SO")).is_err());
        assert!(directions(Some("W")).is_err());
    }

    #[test]
    fn an_open_polygon_is_closed() {
        let polygon = serde_json::json!([[45.0, 6.0], [45.001, 6.0], [45.001, 6.001]]);
        let ring = outline(&polygon).unwrap().unwrap();
        assert_eq!(ring.points().len(), 4);
        assert_eq!(ring.points().first(), ring.points().last());
        assert_eq!(outline(&serde_json::json!([[45.0, 6.0]])).unwrap(), None);
        assert!(outline(&serde_json::json!([[95.0, 6.0], [45.0, 6.0], [45.0, 6.1]])).is_err());
    }
}
