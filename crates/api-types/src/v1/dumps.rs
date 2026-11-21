//! The dumps of the sites that Venti holds itself: each of its two layers
//! whole, as files that anyone fetches without an account.
//!
//! A layer has three files, and no file holds a site of the other layer or
//! a record of any other source:
//!
//! - the sites as they stand, as GeoJSON: a `FeatureCollection` whose
//!   members beyond GeoJSON's own are the fields of [`DumpStamp`], with
//!   one `Feature` per standing site whose `properties` are the site's
//!   [`SiteRecord`](super::SiteRecord);
//! - the same sites as CSV, in the columns of [`SITES_CSV_COLUMNS`];
//! - every site with its history, standing or removed, as JSON Lines: the
//!   first line a [`SiteHistoryHead`], every further line a
//!   [`SiteHistoryLine`].
//!
//! [`DumpsResponse`], the answer of [`DUMPS_PATH`], names every file with
//! its licence, when it was made, the layer's change number it stands at,
//! its size and its checksum: that is where a reader of the CSV file, which
//! holds nothing but its lines, finds the licence.

use serde::{Deserialize, Serialize};
use venti_domain::{SiteId, SourceId, Timestamp};

use super::{Attribution, SiteRemoval, SiteRevision};

/// Path of the list of dumps.
pub const DUMPS_PATH: &str = "/v1/dumps";

/// The media type of the sites as they stand as GeoJSON (RFC 7946).
pub const GEOJSON_MEDIA_TYPE: &str = "application/geo+json";

/// The media type of the sites as they stand as CSV (RFC 4180): UTF-8,
/// with the line of column names.
pub const CSV_MEDIA_TYPE: &str = "text/csv; charset=utf-8; header=present";

/// The media type of the sites with their history: JSON Lines, one JSON
/// value on every line.
pub const JSON_LINES_MEDIA_TYPE: &str = "application/x-ndjson";

/// The columns of the CSV file, in their order. The first line of the file
/// names them. A column that is added later is added at the end.
///
/// What a column holds follows the site's record: a text as it is, a
/// number or `true`/`false` as JSON writes it, nothing where the record
/// has no value. `uses` are the uses separated by a space;
/// `wind_directions` are the sectors as `from-to` in degrees, separated by
/// a space; `created_by` and `changed_by` are handles. The outline of a
/// site is not in the CSV file: the GeoJSON file has it.
pub const SITES_CSV_COLUMNS: [&str; 25] = [
    "id",
    "name",
    "lat",
    "lon",
    "elevation_m",
    "uses",
    "wind_directions",
    "paragliding",
    "hang_gliding",
    "official",
    "discouraged",
    "url",
    "website",
    "description",
    "fee",
    "guest_guidelines",
    "no_flight_time",
    "revision",
    "created_at",
    "created_by",
    "changed_at",
    "changed_by",
    "confirmed_pilots",
    "confirmed_latest_day",
    "layer",
];

/// What a file of a dump holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DumpContent {
    /// The sites of the layer that stand, each as it stands.
    Sites,
    /// Every site the layer ever had with every revision of it.
    History,
    /// Something a newer server hands out.
    #[serde(other)]
    Other,
}

/// The format of a file of a dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DumpFormat {
    /// GeoJSON.
    Geojson,
    /// CSV.
    Csv,
    /// JSON Lines.
    Jsonl,
    /// Something a newer server hands out.
    #[serde(other)]
    Other,
}

impl DumpFormat {
    /// What the name of a file of this format ends with.
    pub fn ending(self) -> &'static str {
        match self {
            Self::Geojson => ".geojson",
            Self::Csv => ".csv",
            Self::Jsonl => ".history.jsonl",
            Self::Other => "",
        }
    }
}

/// Path of one file of a layer's dump. The address stays; what it answers
/// is the file as it was made last.
pub fn sites_dump_path(layer: &str, format: DumpFormat) -> String {
    format!("{DUMPS_PATH}/sites/{layer}{}", format.ending())
}

/// What every file of a dump says of itself: whose records it holds,
/// under which licence, when it was made, and where the layer stood.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpStamp {
    /// The layer: the source of every record in the file.
    pub layer: SourceId,
    /// Who the records are from.
    pub provider: String,
    /// The licence every record in the file is given under, by its name.
    pub licence: String,
    /// The address of the licence's text.
    pub licence_url: String,
    /// What is to be said wherever a site of the layer is shown.
    pub notice: String,
    /// When the file was made.
    pub generated_at: Timestamp,
    /// The layer's change number the file stands at: everything that
    /// happened to the layer up to it is in the file. A program goes on
    /// from the file with `GET /v1/sites/changes?layer=…&since=` this.
    pub cursor: String,
}

/// One file of a dump, as the list names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpFile {
    /// What it holds.
    pub content: DumpContent,
    /// Its format.
    pub format: DumpFormat,
    /// Where it is fetched, from the server's root.
    pub path: String,
    /// Its media type.
    pub media_type: String,
    /// Its size in bytes, before any compression on the way.
    pub bytes: u64,
    /// The SHA-256 of those bytes, in lowercase hexadecimal.
    pub sha256: String,
}

/// The dump of one layer, as the list names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpLayer {
    /// The licence, the time and the change number, the same in each of
    /// the layer's files.
    #[serde(flatten)]
    pub stamp: DumpStamp,
    /// How many sites of the layer stand.
    pub sites: u32,
    /// How many are removed.
    pub removed: u32,
    /// How many revisions the history file holds.
    pub revisions: u32,
    /// The files, the sites as they stand first.
    pub files: Vec<DumpFile>,
}

/// The answer of `GET /v1/dumps`: the dump of each layer of Venti's own
/// sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpsResponse {
    /// The layers, by name.
    pub layers: Vec<DumpLayer>,
    /// The credit of every layer listed.
    pub attributions: Vec<Attribution>,
}

/// The first line of a history file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteHistoryHead {
    /// The licence, the time and the change number.
    #[serde(flatten)]
    pub stamp: DumpStamp,
    /// How many lines follow: one for every site the layer ever had.
    pub sites: u32,
    /// How many revisions those lines hold.
    pub revisions: u32,
}

/// Every further line of a history file: one site with all its revisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteHistoryLine {
    /// The site.
    pub id: SiteId,
    /// The number of its latest revision.
    pub revision: u32,
    /// Whether it is removed.
    pub removed: bool,
    /// For a removed site: when it was removed and by whom, as
    /// `GET /v1/sites/changes` says it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removal: Option<SiteRemoval>,
    /// Every revision, by number. Empty for a site that whoever runs the
    /// server took down: such a site shows nothing of what it said.
    pub revisions: Vec<SiteRevision>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_has_its_address_by_layer_and_format() {
        assert_eq!(
            sites_dump_path("venti", DumpFormat::Geojson),
            "/v1/dumps/sites/venti.geojson"
        );
        assert_eq!(
            sites_dump_path("venti-pilots", DumpFormat::Csv),
            "/v1/dumps/sites/venti-pilots.csv"
        );
        assert_eq!(
            sites_dump_path("venti", DumpFormat::Jsonl),
            "/v1/dumps/sites/venti.history.jsonl"
        );
    }

    #[test]
    fn a_kind_of_file_from_a_newer_server_is_read() {
        let file: DumpFile = serde_json::from_str(
            r#"{"content":"confirmations","format":"parquet","path":"/v1/dumps/x",
                "media_type":"application/x","bytes":1,"sha256":"00"}"#,
        )
        .unwrap();
        assert_eq!(file.content, DumpContent::Other);
        assert_eq!(file.format, DumpFormat::Other);
    }
}
