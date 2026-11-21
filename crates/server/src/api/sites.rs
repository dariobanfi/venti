//! Flying sites: the layers inside a box, one site by its address, and
//! the sites that pilots make.
//!
//! **Two kinds of layer, never merged.** A source's sites are its whole
//! set, replaced with every run. The community's own sites are the layer
//! [`pilot_sites::SOURCE`]: records with an author, a history and an
//! identifier of the server's making, under the licence that
//! [`pilot_sites::LICENCE`] names. Every answer says in its `attributions`
//! under which licence its records are given.
//!
//! **Reading is open to anyone**, by box, by identifier, a site's
//! history, and what changed since.
//!
//! **Writing takes an account**, through [`Signed`] and nothing else, so
//! that a token with scopes later changes that one place and no handler
//! here. Whether this server takes sites at all, who may write, and how
//! much, is [`crate::pilot_sites`]'s to say; [`PilotSites::writer`] asks
//! it for every write, in one place. Every write is a revision: nothing
//! here overwrites or deletes, and no handler can take a site down, which
//! only whoever runs the server does ([`crate::moderation`]).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use venti_api_types::v1::{
    AddSiteRequest, ChangeSiteRequest, ErrorCode, Handle, MAX_SITES, RemoveSiteRequest,
    RestoreSiteRequest, SiteChangesResponse, SiteDraft, SiteHistoryResponse, SiteLayer, SitePilot,
    SiteRecord, SiteResponse, SiteRevision, SiteRevisionKind, SiteStanding, SiteVia, SitesResponse,
    parse_bbox, parse_site_comment,
};
use venti_domain::{BoundingBox, Site, SiteId, SiteUse, SourceId, Timestamp};
use venti_sources::Kind;
use venti_store::{NewPilotSite, PilotSite, RevisionKind, SiteAct, SiteRevising, SiteWrite, Store};

use super::auth::Signed;
use super::error::ApiError;
use super::{Api, body, cache};
use crate::accounts::secret;
use crate::accounts::throttle::{Counted, Throttle};
use crate::pilot_sites::{self, APART_M, Settings};

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
}

/// `GET /v1/sites?bbox=west,south,east,north`: the flying sites inside a
/// box, one layer per source the server holds sites of.
pub(crate) async fn sites<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<Params>,
) -> Result<Response, ApiError> {
    let bbox = params.bbox.ok_or_else(|| {
        ApiError::bad_request(
            ErrorCode::InvalidBbox,
            format!("the bbox parameter is missing; {BBOX_HELP}"),
        )
    })?;
    let area = parse_bbox(&bbox).map_err(|error| {
        ApiError::bad_request(ErrorCode::InvalidBbox, format!("{error}; {BBOX_HELP}"))
    })?;

    let answer = api.sites_in(&area).await?;
    Ok(cache::slow_json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The flying sites inside a box, as the API answers them.
    ///
    /// As for airspace, every source the server holds a set of is a layer,
    /// also when it has nothing in the box: the layer says how old its set
    /// is and where it is whole. Only sources this server is configured to
    /// serve sites of are asked for. A box that holds more than one answer
    /// carries is refused whole: see [`MAX_SITES`].
    pub(crate) async fn sites_in(&self, area: &BoundingBox) -> Result<SitesResponse, ApiError> {
        let sources: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|(_, served)| served.kinds.contains(&Kind::Sites))
            .map(|(source, _)| source.clone())
            .collect();
        // One more than an answer carries, to know whether the box holds more.
        let found = self.store.sites_in(&sources, area, MAX_SITES + 1).await?;
        let count: usize = found.iter().map(|layer| layer.sites.len()).sum();
        // The sites that pilots made count towards the same bound.
        let made = self
            .store
            .pilot_sites_in(area, (MAX_SITES + 1).saturating_sub(count))
            .await?;
        let count = count + made.len();
        if count > MAX_SITES {
            return Err(ApiError::bad_request(
                ErrorCode::BoxTooLarge,
                format!(
                    "the box holds more than {MAX_SITES} sites, which is the most one answer \
                     carries; ask for it in several smaller boxes"
                ),
            ));
        }

        let mut layers: Vec<SiteLayer> = found
            .into_iter()
            .map(|layer| SiteLayer {
                source: layer.set.source,
                fetched_at: layer.set.fetched_at,
                as_of: layer.set.as_of,
                covers: layer.set.covers,
                sites: layer.sites,
                standing: Vec::new(),
            })
            .collect();
        let mut attributions = self.attributions(layers.iter().map(|layer| &layer.source));
        // The community's own layer, when it has a site in the box. It is
        // read at this moment, so that is its date. It names no box in
        // which it is whole: it holds what pilots added and is whole
        // nowhere.
        if !made.is_empty() {
            let now = self.clock.now();
            layers.push(SiteLayer {
                source: pilot_sites::source(),
                fetched_at: now,
                as_of: now,
                covers: Vec::new(),
                standing: made.iter().map(standing).collect::<Result<_, _>>()?,
                sites: made.into_iter().map(|site| site.site).collect(),
            });
            attributions.push(pilot_sites::attribution());
            layers.sort_by(|a, b| a.source.cmp(&b.source));
            attributions.sort_by(|a, b| a.source.cmp(&b.source));
        }
        Ok(SitesResponse {
            layers,
            attributions,
        })
    }
}

// --- The sites that pilots make ---------------------------------------------

/// What the handlers keep between requests: the settings, and how much
/// each account has used of what is counted over a day.
#[derive(Debug)]
pub(crate) struct PilotSites {
    settings: Settings,
    /// Sites added, by account.
    added: Throttle,
    /// Revisions written, by account.
    revised: Throttle,
}

/// Which count a write takes from.
#[derive(Debug, Clone, Copy)]
enum Counts {
    Added,
    Revised,
}

impl PilotSites {
    pub(crate) fn new(settings: Settings, watched_keys: usize) -> Self {
        let a_day = chrono::Duration::days(1);
        Self {
            settings,
            added: Throttle::new("accounts that added sites", a_day, watched_keys),
            revised: Throttle::new("accounts that revised sites", a_day, watched_keys),
        }
    }

    /// Lets a write through or says why not: the one place every write of
    /// a site passes. In this order: whether the server takes sites from
    /// pilots at all, whether this account may write, and whether it has
    /// written as much today as an account may.
    fn writer(&self, signed: &Signed, counts: Counts, now: Timestamp) -> Result<(), ApiError> {
        if !self.settings.accept {
            return Err(ApiError::forbidden(
                ErrorCode::SitesClosed,
                "this server takes no sites from pilots",
            ));
        }
        if !pilot_sites::may_write(&signed.account) {
            return Err(ApiError::forbidden(
                ErrorCode::PublishingSuspended,
                "whoever runs this server has suspended this account's publishing: it can \
                 add and change no site",
            ));
        }
        let limits = self.settings.limits;
        let (throttle, limit, what) = match counts {
            Counts::Added => (
                &self.added,
                limits.added_per_day,
                "this account has added as many sites today as one account may",
            ),
            Counts::Revised => (
                &self.revised,
                limits.revisions_per_day,
                "this account has changed sites as often today as one account may",
            ),
        };
        throttle
            .attempt(&[Counted::new(&signed.account.id, limit)], now)
            .map_err(|refused| ApiError::rate_limited(refused, what))
    }
}

fn no_such_site() -> ApiError {
    ApiError::missing(
        ErrorCode::SiteNotFound,
        "there is no site with this identifier",
    )
}

fn gone() -> ApiError {
    ApiError::new(
        StatusCode::GONE,
        ErrorCode::SiteGone,
        "this site was removed; its identifier is not used again",
    )
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidSite, message)
}

fn pilot(pilot: Option<&venti_store::SitePilot>) -> Result<Option<SitePilot>, ApiError> {
    pilot
        .map(|pilot| {
            Ok(SitePilot {
                handle: Handle::new(&pilot.handle).map_err(|_| ApiError::internal())?,
                display_name: pilot.display_name.clone(),
            })
        })
        .transpose()
}

fn standing(site: &PilotSite) -> Result<SiteStanding, ApiError> {
    Ok(SiteStanding {
        id: site.site.id.clone(),
        revision: site.revision,
        created_at: site.created_at,
        created_by: pilot(site.created_by.as_ref())?,
        changed_at: site.changed_at,
        changed_by: pilot(site.changed_by.as_ref())?,
    })
}

/// A site of the community's layer as its address answers it.
fn answer(site: PilotSite) -> Result<SiteResponse, ApiError> {
    Ok(SiteResponse {
        standing: Some(standing(&site)?),
        site: site.site,
        lasting: true,
        attributions: vec![pilot_sites::attribution()],
    })
}

/// What a path under `/v1/sites/` names.
enum Target {
    /// A site of any layer.
    Site(SiteId),
    /// The history of a site that pilots made.
    History(SiteId),
    /// Where a revision that restores an earlier one is sent.
    Revisions(SiteId),
}

/// Whether an identifier has the form of one the server makes for a site
/// of the community's layer.
fn is_made_here(id: &SiteId) -> bool {
    id.source().as_str() == pilot_sites::SOURCE && secret::is_id(id.local())
}

/// Reads the rest of a path. A source's own key may hold a `/`, so the
/// two endings are taken only from an identifier of the community's
/// layer, which holds none.
fn target(rest: &str) -> Result<Target, ApiError> {
    for (ending, make) in [
        ("/history", Target::History as fn(SiteId) -> Target),
        ("/revisions", Target::Revisions),
    ] {
        if let Some(id) = rest.strip_suffix(ending).and_then(|id| id.parse().ok())
            && is_made_here(&id)
        {
            return Ok(make(id));
        }
    }
    rest.parse().map(Target::Site).map_err(|_| no_such_site())
}

/// The site of the community's layer a write is for.
fn made_here(rest: &str) -> Result<SiteId, ApiError> {
    match target(rest)? {
        Target::Site(id) if is_made_here(&id) => Ok(id),
        Target::Site(id) if id.source().as_str() != pilot_sites::SOURCE => {
            Err(ApiError::forbidden(
                ErrorCode::SitesClosed,
                "this site is its source's record and is corrected at its source; the \
                 record's url says where",
            ))
        }
        _ => Err(no_such_site()),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PageParams {
    after: Option<String>,
    since: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/sites/{id}` and `GET /v1/sites/{id}/history`.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Path(rest): Path<String>,
    Query(params): Query<PageParams>,
) -> Result<Response, ApiError> {
    match target(&rest)? {
        Target::Site(id) if id.source().as_str() == pilot_sites::SOURCE => {
            let site = match api.store.pilot_site(&id).await? {
                Some(site) if is_made_here(&id) => site,
                _ => return Err(no_such_site()),
            };
            if !site.stands() {
                return Err(gone());
            }
            Ok(cache::json(&headers, &answer(site)?))
        }
        Target::Site(id) => {
            // A source's site: the record of the set the server holds
            // now, if this server serves sites of that source.
            let served = api
                .sources
                .get(id.source())
                .is_some_and(|served| served.kinds.contains(&Kind::Sites));
            let site = if served {
                api.store.set_site(&id).await?
            } else {
                None
            };
            let site = site.ok_or_else(no_such_site)?;
            let answer = SiteResponse {
                attributions: api.attributions([id.source()]),
                site,
                standing: None,
                lasting: false,
            };
            Ok(cache::slow_json(&headers, &answer))
        }
        Target::History(id) => {
            let limit = body::page_size(params.limit.as_deref())?;
            let after: u32 = match params.after.as_deref() {
                None => 0,
                Some(text) => text.parse().map_err(|_| {
                    ApiError::bad_request(
                        ErrorCode::InvalidCursor,
                        "after must be the number of a revision",
                    )
                })?,
            };
            let (site, revisions) = api
                .store
                .pilot_site_revisions(&id, after, limit)
                .await?
                .ok_or_else(no_such_site)?;
            // A site that was taken down shows nothing of what it said.
            if site.taken_down {
                return Err(gone());
            }
            let revisions = revisions
                .into_iter()
                .map(|revision| {
                    Ok(SiteRevision {
                        number: revision.number,
                        at: revision.at,
                        by: pilot(revision.author.as_ref())?,
                        via: match revision.via {
                            venti_store::SiteVia::Session => SiteVia::Session,
                            venti_store::SiteVia::Operator => SiteVia::Operator,
                        },
                        kind: match revision.kind {
                            RevisionKind::Created => SiteRevisionKind::Created,
                            RevisionKind::Changed => SiteRevisionKind::Changed,
                            RevisionKind::Restored(_) => SiteRevisionKind::Restored,
                            RevisionKind::Removed => SiteRevisionKind::Removed,
                        },
                        restores: revision.kind.restores(),
                        comment: revision.comment,
                        site: revision.record,
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?;
            let more = revisions
                .last()
                .is_some_and(|last| last.number < site.revision);
            let answer = SiteHistoryResponse {
                id,
                revision: site.revision,
                removed: !site.stands(),
                revisions,
                more,
                attributions: vec![pilot_sites::attribution()],
            };
            Ok(cache::json(&headers, &answer))
        }
        Target::Revisions(_) => Err(ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            ErrorCode::MethodNotAllowed,
            "a revision is sent with POST; a site's revisions are read at its history",
        )),
    }
}

/// `GET /v1/sites/changes`: what happened to the sites that pilots made
/// since a program last asked.
pub(crate) async fn changes<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Query(params): Query<PageParams>,
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
    let mut changes = api.store.pilot_site_changes(asked, limit).await?;
    // A cursor from beyond the layer's latest change was not given out by
    // this database as it is now. Start again from the beginning, and say
    // so.
    let reset = asked > changes.latest;
    let since = if reset { 0 } else { asked };
    if reset {
        changes = api.store.pilot_site_changes(0, limit).await?;
    }
    let cursor = changes.sites.last().map_or(since, |site| site.change);
    let mut sites = Vec::new();
    let mut removed = Vec::new();
    for site in changes.sites {
        if site.stands() {
            sites.push(SiteRecord {
                standing: standing(&site)?,
                site: site.site,
            });
        } else {
            removed.push(site.site.id);
        }
    }
    let answer = SiteChangesResponse {
        sites,
        removed,
        cursor: cursor.to_string(),
        more: changes.more,
        reset,
        attributions: vec![pilot_sites::attribution()],
    };
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The record a draft makes under an identifier, if it can be kept:
    /// cleaned, held to the checks every site record passes, with a name
    /// and a use, at a place inside a region this server serves.
    fn site_from(&self, draft: SiteDraft, id: SiteId) -> Result<Site, ApiError> {
        let mut site = draft.into_site(id);
        if site.name.is_none() {
            return Err(invalid("name must not be empty"));
        }
        if site.uses.is_empty() || site.uses.contains(&SiteUse::Other) {
            return Err(invalid(
                "uses must name at least one of takeoff, landing, towing, training",
            ));
        }
        // What the check would remove is what is wrong with the record.
        // Its words name the field and repeat nothing of the text.
        if let Some(rejected) = venti_sources::check_site(&mut site).first() {
            return Err(invalid(format!("{}: {}", rejected.field, rejected.reason)));
        }
        if !self.pilot_sites.settings.serves(site.position) {
            return Err(ApiError::bad_request(
                ErrorCode::SiteOutOfArea,
                "the place is in no region this server serves",
            ));
        }
        Ok(site)
    }

    /// Writes a revision and answers what became of it.
    async fn revise(&self, revising: SiteRevising) -> Result<PilotSite, ApiError> {
        written(self.store.revise_pilot_site(&revising, APART_M).await?)
    }
}

/// What a write came to, as the API says it.
fn written(write: SiteWrite) -> Result<PilotSite, ApiError> {
    match write {
        SiteWrite::Written(site) | SiteWrite::Same(site) => Ok(*site),
        SiteWrite::TooClose(other) => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::SiteTooClose,
            format!(
                "the site {other} stands less than {APART_M} m from this place; if it is the \
                 same place, correct that one"
            ),
        )),
        SiteWrite::Stale { current } => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::SiteChanged,
            format!(
                "the site was changed meanwhile and stands at revision {current}; read it \
                 again and make the change from what it says now"
            ),
        )),
        SiteWrite::Removed => Err(ApiError::new(
            StatusCode::GONE,
            ErrorCode::SiteGone,
            "this site was removed; restoring one of its revisions brings it back",
        )),
        SiteWrite::TakenDown => Err(gone()),
        SiteWrite::NoSuchRevision => Err(ApiError::missing(
            ErrorCode::SiteRevisionNotFound,
            "the site has no revision with this number",
        )),
        SiteWrite::NoSuchSite => Err(no_such_site()),
        SiteWrite::NoSuchAccount => Err(ApiError::unauthorized(
            "the account this session belonged to is gone",
        )),
    }
}

fn comment(comment: Option<&str>) -> Result<Option<String>, ApiError> {
    parse_site_comment(comment)
        .map_err(|most| invalid(format!("comment has at most {most} characters")))
}

/// `POST /v1/sites`: adds a site, as its first revision.
pub(crate) async fn add<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    api.pilot_sites.writer(&signed, Counts::Added, now)?;
    let request: AddSiteRequest = body::json(&headers, &body)?;
    let id = SiteId::new(pilot_sites::source(), secret::random_id()?)
        .map_err(|_| ApiError::internal())?;
    let new = NewPilotSite {
        site: api.site_from(request.site, id)?,
        author_id: signed.account.id.clone(),
        at: now,
        via: venti_store::SiteVia::Session,
        comment: comment(request.comment.as_deref())?,
    };
    let site = written(api.store.add_pilot_site(&new, APART_M).await?)?;
    Ok(cache::private_json(StatusCode::CREATED, &answer(site)?))
}

/// `PUT /v1/sites/{id}`: replaces what a site says, as a new revision.
pub(crate) async fn change<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    Path(rest): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let id = made_here(&rest)?;
    let now = api.clock.now();
    api.pilot_sites.writer(&signed, Counts::Revised, now)?;
    let request: ChangeSiteRequest = body::json(&headers, &body)?;
    let revising = SiteRevising {
        act: SiteAct::Change(Box::new(api.site_from(request.site, id.clone())?)),
        id,
        author_id: Some(signed.account.id.clone()),
        at: now,
        via: venti_store::SiteVia::Session,
        comment: comment(request.comment.as_deref())?,
        expects: Some(request.revision),
    };
    let site = api.revise(revising).await?;
    Ok(cache::private_json(StatusCode::OK, &answer(site)?))
}

/// `POST /v1/sites/{id}/revisions`: puts a site back to what an earlier
/// revision said, as a new revision.
pub(crate) async fn restore<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    Path(rest): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let Target::Revisions(id) = target(&rest)? else {
        return Err(ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            ErrorCode::MethodNotAllowed,
            "a site is changed with PUT; POST goes to its revisions",
        ));
    };
    let now = api.clock.now();
    api.pilot_sites.writer(&signed, Counts::Revised, now)?;
    let request: RestoreSiteRequest = body::json(&headers, &body)?;
    let revising = SiteRevising {
        id,
        author_id: Some(signed.account.id.clone()),
        at: now,
        via: venti_store::SiteVia::Session,
        act: SiteAct::Restore(request.restore),
        comment: comment(request.comment.as_deref())?,
        expects: None,
    };
    let site = api.revise(revising).await?;
    Ok(cache::private_json(StatusCode::OK, &answer(site)?))
}

/// `DELETE /v1/sites/{id}`: removes a site, as a new revision. A pilot's
/// removal is never a take-down: the history stays, and any pilot brings
/// the site back by restoring a revision.
pub(crate) async fn remove<S: Store>(
    State(api): State<Arc<Api<S>>>,
    signed: Signed,
    headers: HeaderMap,
    Path(rest): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let id = made_here(&rest)?;
    let now = api.clock.now();
    api.pilot_sites.writer(&signed, Counts::Revised, now)?;
    // The body may be left out; one that is there says why.
    let request: RemoveSiteRequest = if body.is_empty() {
        RemoveSiteRequest::default()
    } else {
        body::must_say_json(&headers)?;
        body::json(&headers, &body)?
    };
    let revising = SiteRevising {
        id,
        author_id: Some(signed.account.id.clone()),
        at: now,
        via: venti_store::SiteVia::Session,
        act: SiteAct::Remove { take_down: false },
        comment: comment(request.comment.as_deref())?,
        expects: None,
    };
    api.revise(revising).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}
