//! The archive of everything a pilot gave: what `GET /v1/account/archive`
//! hands out as one ZIP file, and the records in it that no other answer
//! has.
//!
//! The files of an archive hold the API's own records wherever there is
//! one: `flights.json` is a [`FlightsResponse`](super::FlightsResponse),
//! `groups.json` a [`GroupsResponse`](super::GroupsResponse), and so on.
//! [`ArchiveIndex`] names every file with the type it holds, so a program
//! reads an archive with the types it reads the API with. Three kinds had
//! no list by account and get one here: the revisions of sites, the
//! confirmations, and the reports.

use serde::{Deserialize, Serialize};
use venti_domain::{SiteId, Timestamp};

use super::{FlightId, Handle, ReportRequest, SiteRevision};

/// The address of the archive.
pub const ACCOUNT_ARCHIVE_PATH: &str = "/v1/account/archive";

/// The media type of the archive.
pub const ARCHIVE_MEDIA_TYPE: &str = "application/zip";

/// The format of the archive this version of the code writes. A later
/// format gets a later number; within one, the index only grows.
pub const ARCHIVE_FORMAT: u32 = 1;

/// The name of the index among the archive's files.
pub const ARCHIVE_INDEX_FILE: &str = "index.json";

/// The most bytes of flight files, as they were uploaded, that one archive
/// holds. The files of the newest flights are taken first; a flight
/// beyond the bound is in `flights.json` and listed in
/// [`ArchiveIndex::flights`] without a file, and its file is fetched by
/// itself (`GET /v1/flights/{id}/igc`).
pub const MAX_ARCHIVE_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// `index.json`: what the archive is and what each of its files holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveIndex {
    /// The format: [`ARCHIVE_FORMAT`].
    pub format: u32,
    /// When the archive was made.
    pub made_at: Timestamp,
    /// The handle of the account it is of.
    pub account: Handle,
    /// Every file of the archive but the flight files and this one, in
    /// the order they were written.
    pub files: Vec<ArchiveFile>,
    /// Every flight of the account, newest first, with the name of its
    /// file in the archive.
    pub flights: Vec<ArchiveFlight>,
    /// `false` when a flight is listed without its file because the
    /// archive had no room for it; see [`MAX_ARCHIVE_FILE_BYTES`].
    pub complete: bool,
    /// What the account has on the server that the archive does not
    /// hold, in words.
    pub not_included: Vec<String>,
}

/// One file of an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFile {
    /// The file's name in the archive.
    pub path: String,
    /// What it holds, in words.
    pub holds: String,
    /// The name of the record it holds, as the API's description
    /// (`/v1/openapi.json`) and `docs/api.md` name it.
    pub record: String,
    /// How many entries its list has; for the account, one.
    pub entries: u32,
    /// The licence its records stand under, where the kind has one. Left
    /// out for what is the pilot's own and was given to nobody.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<ArchiveLicence>,
}

/// A licence, by name and address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveLicence {
    /// The licence's name, for example `CC0 1.0`.
    pub name: String,
    /// Where its text is.
    pub url: String,
}

/// One flight as the index lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFlight {
    /// The flight's address, as `flights.json` has it.
    pub id: FlightId,
    /// The name of its file in the archive: the file as it was uploaded.
    /// Left out where the archive had no room for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// `sites.json`: what the pilot gave to the sites that pilots make.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchiveSites {
    /// Every revision the pilot wrote, by site and then by number, as a
    /// site's history has it. A revision names its site in `site.id`; the
    /// one with number 1 made the site.
    pub revisions: Vec<SiteRevision>,
    /// Every confirmation the pilot has, by site.
    pub confirmations: Vec<ArchiveSiteConfirmation>,
}

/// One confirmation of the pilot's: "I took off here and it was good".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveSiteConfirmation {
    /// The site.
    pub site: SiteId,
    /// The day the pilot speaks of.
    pub day: chrono::NaiveDate,
    /// What they added, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Whether the server found a flight of theirs that took off there.
    pub with_flight: bool,
    /// When it was said, or last said again.
    pub at: Timestamp,
}

/// `reports.json`: the reports the pilot sent that the server still keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveReports {
    /// The reports, oldest first.
    pub reports: Vec<ArchiveReport>,
}

/// One report as its sender gets it back: what they sent and when. What
/// whoever runs the server did about it is not part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveReport {
    /// When it was sent.
    pub sent_at: Timestamp,
    /// What was sent: the body of `POST /v1/reports`. A site is named
    /// with the revision the server noted.
    pub report: ReportRequest,
}
