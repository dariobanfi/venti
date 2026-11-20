//! Maps for flying without a network.
//!
//! A Venti server hands out the map of a region as two files, the base
//! map and the elevation, each an archive that the map engine draws from
//! and that [`crate::Terrain`] reads heights from. The shell downloads
//! them and keeps them; the core reads the server's list, checks a
//! download against what the list said, and says which kept region to
//! draw a place from.
//!
//! The two files are never merged: each holds data under its own
//! licences, and a region names the credits of both.

use std::fs::File;
use std::io::Read;

use sha2::{Digest, Sha256};
use venti_api_types::v1::{
    MAPS_PATH, MapCredit, MapFile, MapFileRole, MapRegion, MapsResponse, is_map_name, map_file_path,
};
use venti_domain::BoundingBox;

use crate::server::answer;
use crate::{Bounds, CoreError};

/// One region's map: as a server offers it, or as the device keeps it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MapRegionView {
    /// The region's identifier: also the name of its folder on the device.
    pub id: String,
    /// The region's name.
    pub name: String,
    /// Which edition of the region's files this is, by custom the day
    /// they were built.
    pub version: String,
    /// The box the map covers.
    pub bounds: Bounds,
    /// The vector base map.
    pub basemap: MapFileView,
    /// The elevation.
    pub terrain: MapFileView,
    /// The size of both files together, in bytes.
    pub bytes: u64,
    /// The same in words: "412 MB".
    pub size: String,
    /// Whose data the files hold, the base map's first, each once: to
    /// show wherever the region is drawn.
    pub credits: Vec<MapCreditView>,
}

/// One file of a region's map.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MapFileView {
    /// The file's name, on the server and in the region's folder.
    pub name: String,
    /// The file's size in bytes.
    pub bytes: u64,
    /// The SHA-256 of its bytes, 64 lowercase hexadecimal digits.
    pub sha256: String,
    /// The path to `GET` it from on the server. The server answers
    /// requests for a range of the file, so a download that broke off is
    /// taken up where it stopped.
    pub path: String,
    /// Whose data the file holds.
    pub credits: Vec<MapCreditView>,
}

/// One credit line of a map file.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MapCreditView {
    /// The credit, worded as the provider asks.
    pub text: String,
    /// Where it links to.
    pub url: Option<String>,
}

/// How a region stands between the server and the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MapRegionState {
    /// The server offers it and the device does not have it.
    Offered,
    /// The device has it, and the server offers nothing else: the same
    /// version, or no answer about this region.
    Kept,
    /// The device has it, and the server offers another version.
    KeptWithNewer,
}

/// One line of the list of regions.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MapRegionRow {
    /// The region: what the device has, or else what the server offers.
    pub region: MapRegionView,
    /// How it stands.
    pub state: MapRegionState,
    /// What the server offers in its place, with [`MapRegionState::KeptWithNewer`].
    pub newer: Option<MapRegionView>,
    /// The line under the region's name: "412 MB", "On this device, 412
    /// MB", "On this device, 412 MB. A newer map can be downloaded, 430
    /// MB".
    pub words: String,
}

/// The path to `GET` from the server for the regions it has maps of.
#[uniffi::export]
pub fn maps_path() -> String {
    MAPS_PATH.to_owned()
}

/// Reads the server's answer to [`maps_path`]: the regions this app can
/// draw, sorted by name. A region without a base map or without
/// elevation, or whose names could lead out of its folder, is left out.
///
/// # Errors
///
/// When the server refused or its answer cannot be read.
#[uniffi::export]
pub fn map_regions(status: u16, body: Vec<u8>) -> Result<Vec<MapRegionView>, CoreError> {
    let response: MapsResponse = answer(status, &body)?;
    let mut regions: Vec<MapRegionView> = response.regions.iter().filter_map(view).collect();
    regions.sort_by(|one, other| one.name.cmp(&other.name).then(one.id.cmp(&other.id)));
    regions.dedup_by(|later, earlier| later.id == earlier.id);
    Ok(regions)
}

/// What to keep in a region's folder beside its files, so that the app
/// knows what they are when there is no server to ask: the region as the
/// server described it, in the server's own format.
#[uniffi::export]
pub fn map_region_record(region: MapRegionView) -> Vec<u8> {
    serde_json::to_vec(&wire(&region)).unwrap_or_default()
}

/// Reads what [`map_region_record`] wrote.
///
/// # Errors
///
/// When the record is not one, or describes a region this app cannot
/// draw.
#[uniffi::export]
pub fn map_region_from_record(record: Vec<u8>) -> Result<MapRegionView, CoreError> {
    serde_json::from_slice::<MapRegion>(&record)
        .ok()
        .as_ref()
        .and_then(view)
        .ok_or_else(|| CoreError::InvalidFile {
            message: "The record of this downloaded map cannot be read.".to_owned(),
        })
}

/// Checks a downloaded file against what the server said about it: its
/// size and its SHA-256. A file that passes is the server's, whole.
///
/// This reads the whole file: seconds for a large one. Not for the thread
/// that draws.
///
/// # Errors
///
/// When the file cannot be read, or is not the file the server described.
/// The message says which, for the pilot.
#[uniffi::export]
pub fn check_map_file(path: String, file: MapFileView) -> Result<(), CoreError> {
    let unreadable = |_| CoreError::Storage {
        message: "The downloaded map could not be read back from this device.".to_owned(),
    };
    let mut opened = File::open(&path).map_err(unreadable)?;
    let size = opened.metadata().map_err(unreadable)?.len();
    let broken = |what: &str| CoreError::InvalidFile {
        message: format!(
            "The download of {} is not what the server described: {what}. It was thrown away; download the map again.",
            file.name
        ),
    };
    if size != file.bytes {
        return Err(broken(&format!(
            "it has {size} bytes and should have {}",
            file.bytes
        )));
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let read = opened.read(&mut buffer).map_err(unreadable)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != file.sha256 {
        return Err(broken("its contents differ"));
    }
    Ok(())
}

/// The list of regions for the pilot: those on the device first, then
/// what else the server offers, each sorted by name.
#[uniffi::export]
pub fn map_rows(offered: Vec<MapRegionView>, kept: Vec<MapRegionView>) -> Vec<MapRegionRow> {
    let mut rows: Vec<MapRegionRow> = kept
        .iter()
        .map(|region| {
            let newer = offered
                .iter()
                .find(|offer| offer.id == region.id && offer.version != region.version)
                .cloned();
            let mut words = format!("On this device, {}", region.size);
            if let Some(newer) = &newer {
                words.push_str(&format!(". A newer map can be downloaded, {}", newer.size));
            }
            MapRegionRow {
                region: region.clone(),
                state: if newer.is_some() {
                    MapRegionState::KeptWithNewer
                } else {
                    MapRegionState::Kept
                },
                newer,
                words,
            }
        })
        .collect();
    rows.sort_by(|one, other| one.region.name.cmp(&other.region.name));
    let mut others: Vec<MapRegionRow> = offered
        .into_iter()
        .filter(|offer| kept.iter().all(|region| region.id != offer.id))
        .map(|region| MapRegionRow {
            words: region.size.clone(),
            region,
            state: MapRegionState::Offered,
            newer: None,
        })
        .collect();
    others.sort_by(|one, other| one.region.name.cmp(&other.region.name));
    rows.extend(others);
    rows
}

/// The kept region to draw the map of a place from: the one that covers
/// the place; of several, the smallest, which is the one made for it.
/// `None` where no kept region reaches.
#[uniffi::export]
pub fn map_region_covering(kept: Vec<MapRegionView>, lat: f64, lon: f64) -> Option<MapRegionView> {
    let area = |bounds: &Bounds| (bounds.north - bounds.south) * (bounds.east - bounds.west);
    kept.into_iter()
        .filter(|region| {
            let bounds = &region.bounds;
            (bounds.south..=bounds.north).contains(&lat)
                && (bounds.west..=bounds.east).contains(&lon)
        })
        .min_by(|one, other| area(&one.bounds).total_cmp(&area(&other.bounds)))
}

/// How far a download is, in words: "123 MB of 412 MB".
#[uniffi::export]
pub fn map_download_words(received: u64, total: u64) -> String {
    format!(
        "{} of {}",
        size_in_words(received.min(total)),
        size_in_words(total)
    )
}

/// A size as a person reads it, in the decimal units phones show:
/// "412 MB", "1.3 GB", "7.5 MB".
pub(crate) fn size_in_words(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let megabytes = bytes as f64 / 1_000_000.0;
    if megabytes >= 1000.0 {
        format!("{:.1} GB", megabytes / 1000.0)
    } else if megabytes >= 10.0 {
        format!("{megabytes:.0} MB")
    } else if megabytes >= 0.1 {
        format!("{megabytes:.1} MB")
    } else {
        "less than 0.1 MB".to_owned()
    }
}

/// A region as the app uses it, or `None` for one it cannot draw.
fn view(region: &MapRegion) -> Option<MapRegionView> {
    if !is_map_name(&region.id) || !is_map_name(&region.version) {
        return None;
    }
    let file = |role: MapFileRole| {
        let mut of_role = region.files.iter().filter(|file| file.role == role);
        let file = of_role.next()?;
        // Two files in one role: the app would not know which to draw.
        if of_role.next().is_some() {
            return None;
        }
        let is_hash = file.sha256.len() == 64
            && file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        (is_map_name(&file.name) && is_hash && file.bytes > 0).then(|| MapFileView {
            name: file.name.clone(),
            bytes: file.bytes,
            sha256: file.sha256.clone(),
            // Worked out here and not taken from the answer: the app
            // fetches a region's files from the region's place and
            // nowhere else.
            path: map_file_path(&region.id, &region.version, &file.name),
            credits: file
                .credits
                .iter()
                .filter(|credit| !credit.text.trim().is_empty())
                .map(|credit| MapCreditView {
                    text: credit.text.trim().to_owned(),
                    url: credit.url.clone().filter(|url| url.starts_with("https://")),
                })
                .collect(),
        })
    };
    let (basemap, terrain) = (file(MapFileRole::Basemap)?, file(MapFileRole::Terrain)?);
    if basemap.name == terrain.name {
        return None;
    }
    let mut credits: Vec<MapCreditView> = Vec::new();
    for credit in basemap.credits.iter().chain(&terrain.credits) {
        if !credits.contains(credit) {
            credits.push(credit.clone());
        }
    }
    let bytes = basemap.bytes.saturating_add(terrain.bytes);
    let name = region.name.trim();
    Some(MapRegionView {
        id: region.id.clone(),
        name: if name.is_empty() {
            region.id.clone()
        } else {
            name.to_owned()
        },
        version: region.version.clone(),
        bounds: Bounds {
            south: region.covers.south(),
            west: region.covers.west(),
            north: region.covers.north(),
            east: region.covers.east(),
        },
        basemap,
        terrain,
        bytes,
        size: size_in_words(bytes),
        credits,
    })
}

/// A region in the server's format, to keep.
fn wire(region: &MapRegionView) -> Option<MapRegion> {
    let file = |role: MapFileRole, file: &MapFileView| MapFile {
        role,
        name: file.name.clone(),
        bytes: file.bytes,
        sha256: file.sha256.clone(),
        path: file.path.clone(),
        credits: file
            .credits
            .iter()
            .map(|credit| MapCredit {
                text: credit.text.clone(),
                url: credit.url.clone(),
            })
            .collect(),
    };
    let bounds = &region.bounds;
    Some(MapRegion {
        id: region.id.clone(),
        name: region.name.clone(),
        version: region.version.clone(),
        covers: BoundingBox::new(bounds.south, bounds.west, bounds.north, bounds.east).ok()?,
        files: vec![
            file(MapFileRole::Basemap, &region.basemap),
            file(MapFileRole::Terrain, &region.terrain),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0f343b0931126a20f133d67c2b018a3b5f1a2b5e0f8f1d3f6f0f4d8a3b1c2d3e";

    fn file(role: &str, name: &str, bytes: u64, credit: &str) -> String {
        format!(
            r#"{{"role": "{role}", "name": "{name}", "bytes": {bytes}, "sha256": "{HASH}",
                "path": "https://elsewhere.example/{name}",
                "credits": [{{"text": " {credit} ", "url": "https://example.org/{role}"}},
                            {{"text": "Shared by both"}}, {{"text": "  "}}]}}"#
        )
    }

    fn region(id: &str, name: &str, version: &str, files: &[String]) -> String {
        format!(
            r#"{{"id": "{id}", "name": "{name}", "version": "{version}",
                "covers": {{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}},
                "files": [{}]}}"#,
            files.join(",")
        )
    }

    fn answer_with(regions: &[String]) -> Vec<u8> {
        format!(r#"{{"regions": [{}]}}"#, regions.join(",")).into_bytes()
    }

    fn both() -> Vec<String> {
        vec![
            file("basemap", "basemap.pmtiles", 400_000_000, "Map data"),
            file("terrain", "terrain.pmtiles", 12_400_000, "Elevation"),
        ]
    }

    #[test]
    fn the_servers_regions_are_read_with_their_size_and_credits() {
        let body = answer_with(&[
            region(
                "alps-de-at",
                "Alps of Germany and Austria",
                "2026-11-20",
                &both(),
            ),
            // What this app cannot draw is left out: no elevation, two
            // base maps, a name that leads elsewhere, a hash that is none.
            region("no-terrain", "A", "1", &both()[..1]),
            region(
                "two-basemaps",
                "B",
                "1",
                &[both()[0].clone(), both()[0].clone(), both()[1].clone()],
            ),
            region("../elsewhere", "C", "1", &both()),
            region("bad-version", "D", "../1", &both()),
            region(
                "bad-file",
                "E",
                "1",
                &[
                    file("basemap", "../basemap.pmtiles", 5, "x"),
                    both()[1].clone(),
                ],
            ),
            region(
                "bad-hash",
                "F",
                "1",
                &[both()[0].replace(HASH, "abc"), both()[1].clone()],
            ),
            // A file of a kind a newer server hands out is left alone.
            region(
                "with-contours",
                "Zillertal",
                "3",
                &[
                    both()[0].clone(),
                    both()[1].clone(),
                    file("contours", "c.pmtiles", 9, "x"),
                ],
            ),
        ]);

        let regions = map_regions(200, body).unwrap();

        assert_eq!(
            regions
                .iter()
                .map(|region| region.id.as_str())
                .collect::<Vec<_>>(),
            ["alps-de-at", "with-contours"]
        );
        let alps = &regions[0];
        assert_eq!(alps.name, "Alps of Germany and Austria");
        assert_eq!(alps.bounds.west, 9.4);
        assert_eq!(alps.bytes, 412_400_000);
        assert_eq!(alps.size, "412 MB");
        // The path is the region's own place, whatever the answer said.
        assert_eq!(
            alps.terrain.path,
            "/v1/maps/alps-de-at/2026-11-20/terrain.pmtiles"
        );
        assert_eq!(alps.basemap.sha256, HASH);
        // Each file's credits stay with the file; the region lists every
        // one once, the base map's first.
        assert_eq!(
            alps.credits
                .iter()
                .map(|credit| credit.text.as_str())
                .collect::<Vec<_>>(),
            ["Map data", "Shared by both", "Elevation"]
        );
        assert_eq!(
            alps.credits[0].url.as_deref(),
            Some("https://example.org/basemap")
        );
        assert_eq!(alps.terrain.credits.len(), 2);

        // A server without maps, and one that refuses.
        assert!(
            map_regions(200, br#"{"regions": []}"#.to_vec())
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            map_regions(503, Vec::new()),
            Err(CoreError::Server { .. })
        ));
    }

    #[test]
    fn a_region_is_kept_and_read_back_as_it_was() {
        let body = answer_with(&[region("alps-de-at", "Alps", "2026-11-20", &both())]);
        let region = map_regions(200, body).unwrap().remove(0);

        let record = map_region_record(region.clone());

        assert_eq!(map_region_from_record(record).unwrap(), region);
        assert!(matches!(
            map_region_from_record(b"{}".to_vec()),
            Err(CoreError::InvalidFile { .. })
        ));
    }

    #[test]
    fn a_download_is_checked_against_what_the_server_said() {
        let path = std::env::temp_dir().join(format!("venti-map-check-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let name = path.to_str().unwrap().to_owned();
        let described = |bytes: u64, sha256: &str| MapFileView {
            name: "basemap.pmtiles".to_owned(),
            bytes,
            sha256: sha256.to_owned(),
            path: String::new(),
            credits: Vec::new(),
        };
        // The SHA-256 of "abc".
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

        assert_eq!(check_map_file(name.clone(), described(3, abc)), Ok(()));
        let short = check_map_file(name.clone(), described(4, abc)).unwrap_err();
        assert!(
            short.to_string().contains("3 bytes and should have 4"),
            "{short}"
        );
        let other = check_map_file(name.clone(), described(3, HASH)).unwrap_err();
        assert!(other.to_string().contains("its contents differ"), "{other}");
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            check_map_file(name, described(3, abc)),
            Err(CoreError::Storage { .. })
        ));
    }

    #[test]
    fn the_list_puts_what_the_device_has_first_and_says_when_there_is_a_newer_map() {
        let regions = |version: &str| {
            map_regions(
                200,
                answer_with(&[
                    region("alps-de-at", "Alps", version, &both()),
                    region("tegernsee-demo", "Tegernsee valley", "1", &both()),
                    region("pyrenees", "Pyrenees", "1", &both()),
                ]),
            )
            .unwrap()
        };
        let kept = vec![regions("2026-11-20").remove(0), regions("x").remove(2)];

        // The server has the same Alps: nothing to do.
        let same = map_rows(regions("2026-11-20"), kept.clone());
        assert_eq!(
            same.iter()
                .map(|row| (row.region.id.as_str(), row.state))
                .collect::<Vec<_>>(),
            [
                ("alps-de-at", MapRegionState::Kept),
                ("tegernsee-demo", MapRegionState::Kept),
                ("pyrenees", MapRegionState::Offered),
            ]
        );
        assert_eq!(same[0].words, "On this device, 412 MB");
        assert_eq!(same[2].words, "412 MB");

        // The server has newer Alps: the row is still what the device
        // has, with the newer beside it.
        let newer = map_rows(regions("2026-12-01"), kept.clone());
        assert_eq!(newer[0].state, MapRegionState::KeptWithNewer);
        assert_eq!(newer[0].region.version, "2026-11-20");
        assert_eq!(newer[0].newer.as_ref().unwrap().version, "2026-12-01");
        assert_eq!(
            newer[0].words,
            "On this device, 412 MB. A newer map can be downloaded, 412 MB"
        );

        // No server to ask: what the device has is all there is.
        let alone = map_rows(Vec::new(), kept);
        assert_eq!(alone.len(), 2);
        assert!(alone.iter().all(|row| row.state == MapRegionState::Kept));
    }

    #[test]
    fn a_place_is_drawn_from_the_smallest_kept_region_that_covers_it() {
        let mut alps = map_regions(
            200,
            answer_with(&[region("alps-de-at", "Alps", "1", &both())]),
        )
        .unwrap()
        .remove(0);
        let mut valley = alps.clone();
        valley.id = "tegernsee-demo".to_owned();
        valley.bounds = Bounds {
            south: 47.60,
            west: 11.64,
            north: 47.76,
            east: 11.86,
        };
        let kept = vec![alps.clone(), valley];

        let at = |lat, lon| map_region_covering(kept.clone(), lat, lon).map(|region| region.id);
        assert_eq!(at(47.68, 11.75).as_deref(), Some("tegernsee-demo"));
        assert_eq!(at(47.0, 13.0).as_deref(), Some("alps-de-at"));
        assert_eq!(at(43.0, 0.5), None);
        alps.bounds.north = 46.0;
        assert_eq!(map_region_covering(vec![alps], 47.0, 13.0), None);
    }

    #[test]
    fn sizes_are_said_as_phones_say_them() {
        assert_eq!(size_in_words(1_340_000_000), "1.3 GB");
        assert_eq!(size_in_words(412_400_000), "412 MB");
        assert_eq!(size_in_words(7_476_437), "7.5 MB");
        assert_eq!(size_in_words(120_000), "0.1 MB");
        assert_eq!(size_in_words(12), "less than 0.1 MB");
        assert_eq!(
            map_download_words(123_000_000, 412_400_000),
            "123 MB of 412 MB"
        );
        // More than the whole is the whole.
        assert_eq!(
            map_download_words(9_000_000_000, 7_476_437),
            "7.5 MB of 7.5 MB"
        );
    }
}
