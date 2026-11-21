//! The sites a pilot keeps as their own, on the account.
//!
//! A pilot marks flying sites as theirs: the take-offs they fly from. With
//! an account the list is on the server, so that it follows the pilot to
//! their other devices. It is private: nobody but the account reads it,
//! and nothing of it is handed out to anyone else.
//!
//! A kept site is a copy of the site's record as it was when the pilot
//! kept it, with its source's credit, so that the list can be shown
//! without the source's set of sites. It is no contribution and no site
//! of its own: the record stays its source's, under its source's licence.

use serde::{Deserialize, Serialize};
use venti_domain::{Site, SiteId, Timestamp};

use super::{Attribution, SiteStanding};

/// Path of what changed among the account's kept sites.
pub const KEPT_SITE_CHANGES_PATH: &str = "/v1/account/kept-sites/changes";

/// Path under which each kept site of the account has its address.
pub const KEPT_SITES_PATH: &str = "/v1/account/kept-sites";

/// Path of one kept site, by the site's identifier, written as it is.
pub fn kept_site_path(site: &SiteId) -> String {
    format!("{KEPT_SITES_PATH}/{site}")
}

/// The most sites an account keeps.
pub const MAX_KEPT_SITES: usize = 200;

/// The most bytes the body that keeps a site has: what the server takes as
/// a JSON body anywhere. A record with a landing field's outline and every
/// text at its usual length is far below it.
pub const MAX_KEPT_SITE_BODY_BYTES: usize = 16 * 1024;

/// The most characters each text of a credit kept with a site has.
pub const MAX_KEPT_CREDIT_LENGTH: usize = 500;

/// A request for what changed among the signed-in account's kept sites.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptSiteChangesRequest {
    /// The `cursor` of the answer before. `None` asks for everything.
    pub since: Option<String>,
    /// How many changes to return, at most
    /// [`MAX_PAGE_SIZE`](super::MAX_PAGE_SIZE).
    pub limit: Option<usize>,
}

impl KeptSiteChangesRequest {
    /// The path and query string to GET.
    pub fn path_and_query(&self) -> String {
        // A cursor is what the server gave out: digits. Anything else is
        // left out and asks for everything.
        let since = self
            .since
            .as_deref()
            .filter(|since| !since.is_empty() && since.bytes().all(|byte| byte.is_ascii_digit()))
            .map(|since| format!("since={since}"));
        let limit = self.limit.map(|limit| format!("limit={limit}"));
        let query: Vec<String> = since.into_iter().chain(limit).collect();
        if query.is_empty() {
            KEPT_SITE_CHANGES_PATH.to_owned()
        } else {
            format!("{KEPT_SITE_CHANGES_PATH}?{}", query.join("&"))
        }
    }
}

/// The body of `PUT /v1/account/kept-sites/{id}`: a site to keep, as the
/// device has it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeepSiteRequest {
    /// The site's canonical record, as its source had it when the pilot
    /// kept it. Its identifier is the one in the path.
    pub site: Site,
    /// For a site that pilots made on Venti: who added it and who changed
    /// it last, as the server said then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing: Option<SiteStanding>,
    /// The credit of the record's source, with what its licence asks to
    /// be shown wherever the site is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,
    /// The moment the source's data was of when the record was last seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<Timestamp>,
    /// When the pilot kept it. Left out by a device that did not note it:
    /// such a site is kept if the account does not have it, and never
    /// replaces what the account has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_at: Option<Timestamp>,
}

/// One site an account keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeptSite {
    /// The site's canonical record, as it was when the pilot kept it.
    pub site: Site,
    /// For a site that pilots made on Venti: who added it and who changed
    /// it last, as the server said then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing: Option<SiteStanding>,
    /// The credit of the record's source. A client that shows the site
    /// shows the credit: the licences require it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,
    /// The moment the source's data was of when the record was last seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<Timestamp>,
    /// When the pilot kept it.
    pub kept_at: Timestamp,
}

/// The answer of `PUT /v1/account/kept-sites/{id}`: the site as the
/// account keeps it now. It is the request's, unless the account already
/// kept the site with a `kept_at` that is not older.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeptSiteResponse {
    /// The kept site.
    pub kept: KeptSite,
}

/// The answer of `GET /v1/account/kept-sites/changes`: what happened to
/// the account's kept sites since the device last asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeptSiteChangesResponse {
    /// Sites that were kept, or kept anew, each as it is now.
    pub kept: Vec<KeptSite>,
    /// Sites that are no longer kept, by identifier.
    pub forgotten: Vec<SiteId>,
    /// Pass this as `since` next time.
    pub cursor: String,
    /// Whether more changes are waiting. Ask again with `cursor` at once.
    pub more: bool,
    /// `true` when the server could not answer from the request's `since`
    /// and started again from the beginning: after it was restored from a
    /// backup, or when the cursor is older than what the server remembers
    /// of sites that were forgotten. What follows is then everything the
    /// account keeps, and a site the device holds as the account's that
    /// is not in it is no longer kept.
    pub reset: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_changes_are_asked_for_with_a_cursor_of_digits_or_none() {
        let all = KeptSiteChangesRequest::default();
        assert_eq!(all.path_and_query(), "/v1/account/kept-sites/changes");
        let since = KeptSiteChangesRequest {
            since: Some("1795028700000000".to_owned()),
            limit: Some(100),
        };
        assert_eq!(
            since.path_and_query(),
            "/v1/account/kept-sites/changes?since=1795028700000000&limit=100"
        );
        let odd = KeptSiteChangesRequest {
            since: Some("a&b=c".to_owned()),
            limit: None,
        };
        assert_eq!(odd.path_and_query(), "/v1/account/kept-sites/changes");
    }

    #[test]
    fn a_kept_sites_address_is_its_identifier_as_written() {
        let id: SiteId = "osm-free-flying:node/5964676196".parse().unwrap();
        assert_eq!(
            kept_site_path(&id),
            "/v1/account/kept-sites/osm-free-flying:node/5964676196"
        );
    }
}
