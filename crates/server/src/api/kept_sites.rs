//! The sites a pilot keeps as their own, on the account: keeping one,
//! forgetting one, and what changed since a device last asked.
//!
//! The list is private. Every request here acts for the account of its
//! token and reads or writes that account's list and no other; nothing in
//! the API hands a kept site to anybody else, and the account's deletion
//! removes the list.
//!
//! A kept site is a copy of a site's record as its pilot's device had it,
//! under its source's licence and with its source's credit. The server
//! checks that it is a site's record and hands it back as it took it; it
//! does not look the site up in its own sets, because a pilot keeps a
//! site also when its source has since dropped it.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use venti_api_types::v1::{
    Attribution, ErrorCode, KeepSiteRequest, KeptSite, KeptSiteChangesResponse, KeptSiteResponse,
    MAX_KEPT_CREDIT_LENGTH, MAX_KEPT_SITE_BODY_BYTES, MAX_KEPT_SITES, SiteStanding,
};
use venti_domain::{Site, SiteId, Timestamp};
use venti_store::{KeepingSite, KeptRoom, NewKeptSite, Store};

use super::auth::{May, ReadKeptSites, WriteKeptSites};
use super::error::ApiError;
use super::{Api, body, cache};

/// How much an account's kept sites hold: [`MAX_KEPT_SITES`] sites, and
/// the memory of as many that were forgotten.
const ROOM: KeptRoom = KeptRoom {
    sites: MAX_KEPT_SITES,
    forgotten: MAX_KEPT_SITES,
};

/// A kept site as the store holds it, beside when it was kept.
#[derive(Debug, Serialize, Deserialize)]
struct Record {
    site: Site,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    standing: Option<SiteStanding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attribution: Option<Attribution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    as_of: Option<Timestamp>,
}

/// A stored kept site as the API hands it out.
fn wire(kept: &venti_store::KeptSite) -> Result<KeptSite, ApiError> {
    // The text is the server's own writing.
    let record: Record = serde_json::from_str(&kept.record).map_err(|_| ApiError::internal())?;
    Ok(KeptSite {
        site: record.site,
        standing: record.standing,
        attribution: record.attribution,
        as_of: record.as_of,
        kept_at: kept.kept_at,
    })
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidKeptSite, message)
}

/// The site a path names. Text that is no site's identifier names none.
fn site_id(rest: &str) -> Result<SiteId, ApiError> {
    rest.parse().map_err(|_| {
        invalid("the path does not end in a site's identifier, such as 'osm-free-flying:node/1'")
    })
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChangesParams {
    since: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/account/kept-sites/changes`: what happened to the account's
/// kept sites since the device last asked.
pub(crate) async fn changes<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<ReadKeptSites>,
    Query(params): Query<ChangesParams>,
) -> Result<Response, ApiError> {
    let limit = body::page_size(params.limit.as_deref())?;
    let asked: i64 = match params.since.as_deref() {
        None => 0,
        Some(text) => text
            .parse()
            .ok()
            .filter(|since| *since >= 0)
            .ok_or_else(|| {
                ApiError::bad_request(
                    ErrorCode::InvalidCursor,
                    "since must be the cursor of an earlier answer",
                )
            })?,
    };

    let account_id = &signed.account.id;
    let mut changes = api
        .store
        .kept_site_changes(account_id, asked, limit)
        .await?;
    // A cursor from beyond the latest change was not given out by this
    // database as it is now. One from before the floor may have missed
    // that a site was forgotten. Either way: start again, and say so.
    let reset = asked > changes.latest || (asked > 0 && asked < changes.floor);
    let since = if reset { 0 } else { asked };
    if reset {
        changes = api.store.kept_site_changes(account_id, 0, limit).await?;
    }

    let answer = KeptSiteChangesResponse {
        cursor: changes.last().unwrap_or(since).to_string(),
        kept: changes.kept.iter().map(wire).collect::<Result<_, _>>()?,
        forgotten: changes
            .forgotten
            .iter()
            .map(|forgotten| forgotten.site_id.parse().map_err(|_| ApiError::internal()))
            .collect::<Result<_, _>>()?,
        more: changes.more,
        reset,
    };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `PUT /v1/account/kept-sites/{id}`: keeps a site as the account's own.
pub(crate) async fn keep<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteKeptSites>,
    Path(rest): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = site_id(&rest)?;
    if body.len() > MAX_KEPT_SITE_BODY_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::PayloadTooLarge,
            format!("a site to keep has at most {MAX_KEPT_SITE_BODY_BYTES} bytes"),
        ));
    }
    let request: KeepSiteRequest = body::json(&headers, &body)?;
    let mut site = request.site;
    if site.id != id {
        return Err(invalid(
            "site.id must be the identifier in the path: a site is kept under its own identifier",
        ));
    }
    // What every site record passes, whichever source it is of.
    if let Some(fault) = site.check().first() {
        return Err(invalid(format!("site.{}: {}", fault.field, fault.reason)));
    }
    if request
        .standing
        .as_ref()
        .is_some_and(|standing| standing.id != id)
    {
        return Err(invalid("standing.id must be the site's identifier"));
    }
    if let Some(credit) = &request.attribution {
        if &credit.source != id.source() {
            return Err(invalid(
                "attribution.source must be the source of the site: a site is kept with \
                 its own source's credit",
            ));
        }
        let texts = [
            ("provider", Some(&credit.provider)),
            ("licence", Some(&credit.licence)),
            ("text", Some(&credit.text)),
            ("url", Some(&credit.url)),
            ("notice", credit.notice.as_ref()),
        ];
        for (field, text) in texts {
            let Some(text) = text else { continue };
            if text.chars().count() > MAX_KEPT_CREDIT_LENGTH {
                return Err(invalid(format!(
                    "attribution.{field} has at most {MAX_KEPT_CREDIT_LENGTH} characters"
                )));
            }
            if text.chars().any(|c| c.is_control() && c != '\n') {
                return Err(invalid(format!(
                    "attribution.{field} holds a character that does not print"
                )));
            }
        }
    }

    let record = Record {
        site,
        standing: request.standing,
        attribution: request.attribution,
        as_of: request.as_of,
    };
    let new = NewKeptSite {
        site_id: id.to_string(),
        source: id.source().to_string(),
        kept_at: request.kept_at,
        record: serde_json::to_string(&record).map_err(|_| ApiError::internal())?,
    };
    let kept = match api
        .store
        .keep_site(&signed.account.id, &new, api.clock.now(), ROOM)
        .await?
    {
        KeepingSite::Kept(kept) => kept,
        KeepingSite::Full => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::TooManyKeptSites,
                format!("an account keeps at most {MAX_KEPT_SITES} sites; forget one first"),
            ));
        }
        // Deleted between the token's check and this.
        KeepingSite::NoAccount => {
            return Err(ApiError::unauthorized("the account is gone"));
        }
    };
    let answer = KeptSiteResponse { kept: wire(&kept)? };
    Ok(cache::private_json(StatusCode::OK, &answer))
}

/// `DELETE /v1/account/kept-sites/{id}`: forgets a site. Forgetting one
/// that is not kept is not an error: what was asked for holds.
pub(crate) async fn forget<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteKeptSites>,
    Path(rest): Path<String>,
) -> Result<Response, ApiError> {
    let id = site_id(&rest)?;
    api.store
        .forget_kept_site(&signed.account.id, &id.to_string(), api.clock.now(), ROOM)
        .await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}
