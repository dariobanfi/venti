//! The sites a pilot keeps as their own, on the account.
//!
//! The list is private to the account and goes with it. A kept site is a
//! copy of a site's record as it was when the pilot kept it; the store
//! holds the record as text and does not read it.

use venti_domain::Timestamp;

/// A site an account keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptSite {
    /// The site's identifier, as its source gives it.
    pub site_id: String,
    /// The source the record is of.
    pub source: String,
    /// When the pilot kept it.
    pub kept_at: Timestamp,
    /// The record as it was when kept, with its source's credit, as JSON
    /// text.
    pub record: String,
    /// The number of the change that kept it.
    pub change: i64,
}

/// A site to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewKeptSite {
    /// The site's identifier.
    pub site_id: String,
    /// The source the record is of.
    pub source: String,
    /// When the pilot kept it, if the device noted that.
    pub kept_at: Option<Timestamp>,
    /// The record, as JSON text.
    pub record: String,
}

/// A site an account kept and has forgotten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgottenSite {
    /// The site's identifier.
    pub site_id: String,
    /// The number of the change that forgot it.
    pub change: i64,
}

/// How much an account's kept sites may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeptRoom {
    /// The most sites an account keeps.
    pub sites: usize,
    /// Of how many forgotten sites the store remembers that they were
    /// forgotten: the newest. Older ones are dropped, and
    /// [`KeptSiteChanges::floor`] says up to where.
    pub forgotten: usize,
}

/// What became of keeping a site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepingSite {
    /// The site as the account keeps it now: the new record, or the one
    /// the account had, if that was not kept earlier than the new one.
    Kept(KeptSite),
    /// The account keeps as many sites as it may.
    Full,
    /// There is no such account.
    NoAccount,
}

/// What changed among an account's kept sites after some change number.
///
/// Every keeping and forgetting gets a number that is larger than every
/// number the account's kept sites have used before, as a flight's
/// changes do ([`crate::FlightChanges`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeptSiteChanges {
    /// Sites that were kept or kept anew, by change number.
    pub kept: Vec<KeptSite>,
    /// Sites that were forgotten, by change number.
    pub forgotten: Vec<ForgottenSite>,
    /// Whether changes beyond the limit of the request are waiting.
    pub more: bool,
    /// The largest change number the account's kept sites have used, zero
    /// if they have used none.
    pub latest: i64,
    /// The largest change number of a forgotten site the store no longer
    /// remembers. Whoever read up to a smaller number, and not from the
    /// beginning, may have missed that a site was forgotten.
    pub floor: i64,
}

impl KeptSiteChanges {
    /// The largest change number among the changes listed here.
    pub fn last(&self) -> Option<i64> {
        let kept = self.kept.iter().map(|kept| kept.change);
        let forgotten = self.forgotten.iter().map(|forgotten| forgotten.change);
        kept.chain(forgotten).max()
    }
}

/// When a site that is offered for keeping is kept from, or `None` when
/// what the account has stays as it is. Both stores ask here, so that
/// they decide alike.
///
/// A site the account does not keep is kept: with the moment the device
/// noted, or with `now` if it noted none. A site the account keeps is
/// replaced only by a record that was kept later. A moment in the future
/// counts as `now`: a device whose clock runs ahead does not win for good.
pub(crate) fn kept_from(
    have: Option<Timestamp>,
    offered: Option<Timestamp>,
    now: Timestamp,
) -> Option<Timestamp> {
    let offered = offered.map(|at| at.min(now));
    match (have, offered) {
        (None, at) => Some(at.unwrap_or(now)),
        (Some(have), Some(at)) if at > have => Some(at),
        (Some(_), _) => None,
    }
}
