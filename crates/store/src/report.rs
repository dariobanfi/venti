//! Reports: what a pilot tells whoever runs the server about a flight, a
//! pilot's page or a live track that should not be there.
//!
//! A report is kept for the operator and for nobody else. Nothing reads it
//! back to a pilot: not to the one who wrote it, and not to the one it is
//! about, who must never learn from the server who reported them. It names
//! two accounts and goes when either of them goes.

use venti_domain::Timestamp;

use crate::StoreError;

/// Refuses a report no store keeps: one with a zero byte in any of its
/// texts, which the database does not take. The API lets none through.
pub(crate) fn check(new: &NewReport) -> Result<(), StoreError> {
    let texts = [
        new.id.as_str(),
        &new.reporter_id,
        &new.about_account_id,
        new.subject.id(),
        &new.reason,
        new.text.as_deref().unwrap_or_default(),
    ];
    if texts.iter().any(|text| text.contains('\0')) {
        return Err(StoreError::Invalid(
            "a report with a zero byte in it".to_owned(),
        ));
    }
    Ok(())
}

/// Refuses a closing note no store keeps, for the same reason.
pub(crate) fn check_note(note: &str) -> Result<(), StoreError> {
    if note.contains('\0') {
        return Err(StoreError::Invalid(
            "a closing note with a zero byte in it".to_owned(),
        ));
    }
    Ok(())
}

/// What a report is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReportSubject {
    /// A flight, by its address.
    Flight(String),
    /// A pilot's page.
    Pilot,
    /// A live track, by its identifier.
    LiveTrack(String),
}

impl ReportSubject {
    /// The name the database knows the kind of subject by.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Flight(_) => "flight",
            Self::Pilot => "pilot",
            Self::LiveTrack(_) => "live_track",
        }
    }

    /// The address or identifier that names the subject beside its
    /// account; empty for a pilot's page, which the account names.
    pub fn id(&self) -> &str {
        match self {
            Self::Flight(id) | Self::LiveTrack(id) => id,
            Self::Pilot => "",
        }
    }

    /// The subject of this kind and identifier, as the database holds
    /// them. `None` for a kind nobody stored.
    pub fn from_parts(kind: &str, id: &str) -> Option<Self> {
        match kind {
            "flight" => Some(Self::Flight(id.to_owned())),
            "pilot" => Some(Self::Pilot),
            "live_track" => Some(Self::LiveTrack(id.to_owned())),
            _ => None,
        }
    }
}

/// A report to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReport {
    /// The report's identifier.
    pub id: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The account that reports.
    pub reporter_id: String,
    /// The account whose flight, page or live track is reported.
    pub about_account_id: String,
    /// What is reported.
    pub subject: ReportSubject,
    /// Why, as one of the names the API takes. The store keeps the name
    /// as it is: which reasons there are is not its business.
    pub reason: String,
    /// What the reporter wrote beside the reason, if anything.
    pub text: Option<String>,
}

/// What became of a report handed to
/// [`Store::save_report`](crate::Store::save_report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSaved {
    /// It was stored.
    Stored,
    /// The reporter has an open report about the same subject already.
    /// Nothing was stored: the operator reads it once.
    AlreadyOpen,
    /// One of the two accounts it names is not there. Nothing was stored.
    NoSuchAccount,
}

/// A stored report, as whoever runs the server reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The report's identifier.
    pub id: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The handle of the account that reported.
    pub reporter_handle: String,
    /// The identifier of the account that is reported.
    pub about_account_id: String,
    /// That account's handle.
    pub about_handle: String,
    /// That account's display name.
    pub about_name: String,
    /// What is reported.
    pub subject: ReportSubject,
    /// Why.
    pub reason: String,
    /// What the reporter wrote beside the reason, if anything.
    pub text: Option<String>,
    /// When and with what note the report was closed, if it was.
    pub closed: Option<ReportClosure>,
}

/// How a report was closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportClosure {
    /// When.
    pub at: Timestamp,
    /// What the operator noted: what was done, or why nothing was.
    pub note: String,
}

impl Report {
    /// The report's place in a list sorted oldest first: when it was made
    /// and, for reports of the same moment, its identifier.
    pub fn key(&self) -> ReportKey {
        (self.created_at, self.id.clone())
    }
}

/// The place of a report in a list sorted oldest first.
pub type ReportKey = (Timestamp, String);

/// Which reports a list holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportListing {
    /// The ones nobody has closed.
    Open,
    /// All that are kept, closed ones included.
    All,
}

/// What became of a call that closes a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportClosing {
    /// It is closed now.
    Closed,
    /// It was closed before, like this. Nothing was changed.
    AlreadyClosed(ReportClosure),
    /// No report has this identifier.
    NoSuchReport,
}
