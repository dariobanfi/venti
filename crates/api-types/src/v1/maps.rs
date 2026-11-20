use serde::{Deserialize, Serialize};
use venti_domain::BoundingBox;

/// Path of the map regions a server has for download.
pub const MAPS_PATH: &str = "/v1/maps";

/// The longest identifier, version or file name of a map region.
pub const MAX_MAP_NAME_LENGTH: usize = 64;

/// Path of one file of a map region, in one version of the region.
///
/// The version is part of the path, so what a path names never changes: a
/// download that is taken up again continues the same file, and a copy can
/// be kept for good.
pub fn map_file_path(region: &str, version: &str, name: &str) -> String {
    format!("{MAPS_PATH}/{region}/{version}/{name}")
}

/// Whether text can be a map region's identifier, a version of it or the
/// name of one of its files: lowercase letters, digits, and `-`, `_` and
/// `.` between them, at most [`MAX_MAP_NAME_LENGTH`] characters. Such a
/// text is one step of a path and of a folder, and never leads out of
/// either.
pub fn is_map_name(text: &str) -> bool {
    let inner = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    let bytes = text.as_bytes();
    (1..=MAX_MAP_NAME_LENGTH).contains(&bytes.len())
        && bytes.first().copied().is_some_and(inner)
        && bytes.last().copied().is_some_and(inner)
        && bytes
            .iter()
            .all(|byte| inner(*byte) || matches!(byte, b'-' | b'_' | b'.'))
        && !text.contains("..")
}

/// The answer of `GET /v1/maps`: the regions whose map a client can
/// download to draw it without a network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapsResponse {
    /// The regions, sorted by identifier. Empty on a server that has no
    /// maps to hand out.
    pub regions: Vec<MapRegion>,
}

/// One region's map: a few files that a client downloads whole.
///
/// The files are never merged: each holds data under its own licences and
/// names its own credits. A client draws them as separate layers and shows
/// the credits of every file it draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapRegion {
    /// The region's identifier, see [`is_map_name`]. It stays when the
    /// region's files are renewed.
    pub id: String,
    /// The region's name for people.
    pub name: String,
    /// Which edition of the region's files this is, see [`is_map_name`];
    /// by custom the day they were built, `2026-11-20`. A client that has
    /// another version has an older or a newer map; versions are compared
    /// for equality only.
    pub version: String,
    /// The box the files cover.
    pub covers: BoundingBox,
    /// The files.
    pub files: Vec<MapFile>,
}

/// One file of a region's map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapFile {
    /// What the file is.
    pub role: MapFileRole,
    /// The file's name, see [`is_map_name`].
    pub name: String,
    /// The file's size in bytes.
    pub bytes: u64,
    /// The SHA-256 of the file's bytes, 64 lowercase hexadecimal digits. A
    /// client checks its copy against it before using the copy.
    pub sha256: String,
    /// Where to `GET` the file: [`map_file_path`]. The server answers
    /// requests for a range of its bytes.
    pub path: String,
    /// Whose data the file holds, with what the licences ask to be shown
    /// wherever it is drawn.
    pub credits: Vec<MapCredit>,
}

/// What a file of a region's map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapFileRole {
    /// The vector base map: a PMTiles archive, version 3, of vector tiles
    /// in the OpenMapTiles schema.
    Basemap,
    /// The elevation: a PMTiles archive, version 3, of WebP tiles in the
    /// Terrarium encoding.
    Terrain,
    /// Something a newer server hands out. A client leaves it alone.
    #[serde(other)]
    Other,
}

/// One credit line of a map file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapCredit {
    /// The credit, worded as the provider asks.
    pub text: String,
    /// Where the credit links to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_read_back_and_a_role_of_a_newer_server_is_kept_apart() {
        let text = r#"{"regions": [{
            "id": "alps-de-at", "name": "Alps of Germany and Austria", "version": "2026-11-20",
            "covers": {"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2},
            "files": [
                {"role": "basemap", "name": "basemap.pmtiles", "bytes": 512000000,
                 "sha256": "0f343b0931126a20f133d67c2b018a3b5f1a2b5e0f8f1d3f6f0f4d8a3b1c2d3e",
                 "path": "/v1/maps/alps-de-at/2026-11-20/basemap.pmtiles",
                 "credits": [{"text": "© OpenStreetMap contributors",
                              "url": "https://www.openstreetmap.org/copyright"}]},
                {"role": "contours", "name": "contours.pmtiles", "bytes": 1,
                 "sha256": "0f343b0931126a20f133d67c2b018a3b5f1a2b5e0f8f1d3f6f0f4d8a3b1c2d3e",
                 "path": "/v1/maps/alps-de-at/2026-11-20/contours.pmtiles",
                 "credits": [{"text": "Somebody"}]}
            ]}]}"#;

        let answer: MapsResponse = serde_json::from_str(text).unwrap();

        let region = &answer.regions[0];
        assert_eq!(region.covers.west(), 9.4);
        assert_eq!(region.files[0].role, MapFileRole::Basemap);
        assert_eq!(
            region.files[0].path,
            map_file_path(&region.id, &region.version, &region.files[0].name)
        );
        assert_eq!(region.files[1].role, MapFileRole::Other);
        assert_eq!(region.files[1].credits[0].url, None);
        let written = serde_json::to_value(&answer).unwrap();
        assert!(
            written["regions"][0]["files"][1]["credits"][0]
                .get("url")
                .is_none()
        );
    }

    #[test]
    fn a_name_is_one_step_of_a_path_and_no_more() {
        for name in [
            "alps-de-at",
            "2026-11-20",
            "basemap.pmtiles",
            "a",
            "tile_set.v2",
        ] {
            assert!(is_map_name(name), "{name}");
        }
        for name in [
            "",
            ".",
            "..",
            ".hidden",
            "a/b",
            "a..b",
            "Alps",
            "a b",
            "ende.",
            "-a",
            "ä",
            &"a".repeat(MAX_MAP_NAME_LENGTH + 1),
        ] {
            assert!(!is_map_name(name), "{name}");
        }
    }
}
