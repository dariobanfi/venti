//! Flying sites: the layers inside a box, one site by its address, the
//! sites that Venti holds itself, and what pilots confirm of them.
//!
//! **Two kinds of layer, never merged.** A source's sites are its whole
//! set, replaced with every run. Venti's own sites are two layers
//! ([`Layer`]): the partners' (`venti`) and the crowdsourced one
//! (`venti-pilots`), records with an author, a history and an identifier
//! of the server's making, under the licence that [`Layer::licence`]
//! names. Every answer says in its `attributions` under which licence its
//! records are given. The crowdsourced layer is in an answer only when
//! the request names it (`layers=`).
//!
//! **Reading is open to anyone**, by box, by identifier, a site's
//! history, what changed since, and who confirmed a site.
//!
//! **Writing takes an account**: a session, or a token that has the scope
//! `sites:write`, which every handler that writes says in its signature
//! ([`May`]`<`[`WriteSites`]`>`). A revision keeps through which of the
//! two it came in. Who may write to which layer, whether this server
//! takes anything from pilots, and how much, is [`crate::pilot_sites`]'s
//! to say; [`PilotSites::writer`] and [`PilotSites::confirmer`] ask it for
//! every write, in one place. Every write of a site is a revision:
//! nothing here overwrites or deletes, and no handler can take a site
//! down, which only whoever runs the server does ([`crate::moderation`]).
//! A confirmation is no revision: it is a pilot's word about a site, kept
//! beside it.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use venti_api_types::v1::{
    AddSiteRequest, CONFIRMATION_DAYS_BACK, CONFIRMATION_FLIGHT_WITHIN_M, ChangeSiteRequest,
    ConfirmSiteRequest, ErrorCode, ErrorSite, Handle, MAX_SITE_COMMENT_LENGTH,
    MAX_SITE_CONFIRMATIONS, MAX_SITES, RemoveSiteRequest, RestoreSiteRequest, SiteChangesResponse,
    SiteConfirmation, SiteConfirmationsResponse, SiteConfirmed, SiteDraft, SiteHistoryResponse,
    SiteLayer, SitePilot, SiteRecord, SiteResponse, SiteRevision, SiteRevisionKind, SiteStanding,
    SiteVia, SitesResponse, parse_bbox, parse_site_comment,
};
use venti_domain::{BoundingBox, Site, SiteId, SourceId, Timestamp};
use venti_sources::Kind;
use venti_store::{
    NewPilotSite, NewSiteConfirmation, PilotSite, RevisionKind, SiteAct, SiteConfirming,
    SiteRevising, SiteWrite, Store,
};

use super::auth::{May, Signed, WriteSites};
use super::error::ApiError;
use super::{Api, body, cache};
use crate::accounts::secret;
use crate::accounts::throttle::{Counted, Throttle};
use crate::pilot_sites::{self, APART_M, Layer, Refused, Settings};

/// The word in `layers` that stands for the layers an answer holds when
/// the request names none.
const DEFAULT_LAYERS: &str = venti_api_types::v1::DEFAULT_SITE_LAYERS;

const BBOX_HELP: &str =
    "give bbox as west,south,east,north in degrees, for example bbox=11.6,47.55,12.0,47.85";

#[derive(Debug, Deserialize)]
pub(crate) struct Params {
    bbox: Option<String>,
    layers: Option<String>,
}

/// `GET /v1/sites?bbox=west,south,east,north[&layers=a,b]`: the flying
/// sites inside a box, one layer per source the server holds sites of.
/// Without `layers` the answer holds every layer but the crowdsourced
/// one; with it, the layers named and no other.
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

    let answer = api.sites_in(&area, params.layers.as_deref()).await?;
    // A source's set changes once a day at most, and its answer may be
    // reused for an hour. A layer of Venti's own changes whenever a
    // partner or a pilot writes: an answer that was asked for one, whether
    // it holds a site of it yet or not, is reused for a minute, as the
    // other answers that change through the API are.
    Ok(if asks_for_own_layer(params.layers.as_deref()) {
        cache::json(&headers, &answer)
    } else {
        cache::slow_json(&headers, &answer)
    })
}

/// Whether a request's `layers` asks for a layer of Venti's own sites:
/// one that names none does, since the partners' layer is in an answer by
/// default, and so does one that says `default` or names such a layer.
/// What [`Api::sites_in`] refuses is no matter here.
fn asks_for_own_layer(layers: Option<&str>) -> bool {
    layers.is_none_or(|list| {
        list.split(',')
            .map(str::trim)
            .any(|name| name == DEFAULT_LAYERS || Layer::named(name).is_some())
    })
}

impl<S: Store> Api<S> {
    /// The flying sites inside a box, as the API answers them.
    ///
    /// As for airspace, every source the server holds a set of is a layer,
    /// also when it has nothing in the box: the layer says how old its set
    /// is and where it is whole. Only sources this server is configured to
    /// serve sites of are asked for. A box that holds more than one answer
    /// carries is refused whole: see [`MAX_SITES`].
    ///
    /// `layers` is the request's list of the layers it wants, by name and
    /// separated by commas. Without it the answer holds every source's
    /// layer and the partners': what a client from before the
    /// crowdsourced layer expects. A name of no layer of this server is
    /// refused, and the refusal lists the names there are.
    pub(crate) async fn sites_in(
        &self,
        area: &BoundingBox,
        layers: Option<&str>,
    ) -> Result<SitesResponse, ApiError> {
        let served: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|(_, served)| served.kinds.contains(&Kind::Sites))
            .map(|(source, _)| source.clone())
            .collect();
        let (sources, own): (Vec<SourceId>, Vec<Layer>) = match layers {
            None => (
                served,
                Layer::ALL
                    .into_iter()
                    .filter(|layer| layer.by_default())
                    .collect(),
            ),
            Some(list) => {
                let mut sources = Vec::new();
                let mut own = Vec::new();
                for name in list.split(',').map(str::trim) {
                    if name == DEFAULT_LAYERS {
                        // What a request that names none gets, for one
                        // that wants the crowdsourced layer as well and
                        // does not know the sources' names.
                        for source in &served {
                            if !sources.contains(source) {
                                sources.push(source.clone());
                            }
                        }
                        for layer in Layer::ALL.into_iter().filter(|l| l.by_default()) {
                            if !own.contains(&layer) {
                                own.push(layer);
                            }
                        }
                    } else if let Some(layer) = Layer::named(name) {
                        if !own.contains(&layer) {
                            own.push(layer);
                        }
                    } else if let Some(source) = served.iter().find(|s| s.as_str() == name) {
                        if !sources.contains(source) {
                            sources.push(source.clone());
                        }
                    } else {
                        let mut there: Vec<&str> = served.iter().map(SourceId::as_str).collect();
                        there.extend(Layer::ALL.map(Layer::name));
                        there.sort_unstable();
                        return Err(ApiError::bad_request(
                            ErrorCode::InvalidLayers,
                            format!(
                                "layers names the layers of sites wanted, separated by commas; \
                                 this server has {}, and `{DEFAULT_LAYERS}` stands for those \
                                 an answer holds without it",
                                there.join(", ")
                            ),
                        ));
                    }
                }
                (sources, own)
            }
        };
        // One more than an answer carries, to know whether the box holds more.
        let found = self.store.sites_in(&sources, area, MAX_SITES + 1).await?;
        let mut count: usize = found.iter().map(|layer| layer.sites.len()).sum();
        // The sites that Venti holds itself count towards the same bound.
        let mut made = Vec::new();
        for layer in own {
            let room = (MAX_SITES + 1).saturating_sub(count);
            let sites = self
                .store
                .pilot_sites_in(&layer.source(), area, room)
                .await?;
            count += sites.len();
            made.push((layer, sites));
        }
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
        // A layer of Venti's own, when it has a site in the box. It is
        // read at this moment, so that is its date. It names no box in
        // which it is whole: it holds what was added and is whole
        // nowhere.
        let now = self.clock.now();
        for (layer, sites) in made {
            if sites.is_empty() {
                continue;
            }
            layers.push(SiteLayer {
                source: layer.source(),
                fetched_at: now,
                as_of: now,
                covers: Vec::new(),
                standing: sites.iter().map(standing).collect::<Result<_, _>>()?,
                sites: sites.into_iter().map(|site| site.site).collect(),
            });
            attributions.push(layer.attribution());
        }
        layers.sort_by(|a, b| a.source.cmp(&b.source));
        attributions.sort_by(|a, b| a.source.cmp(&b.source));
        Ok(SitesResponse {
            layers,
            attributions,
        })
    }
}

// --- The sites that pilots make ---------------------------------------------

impl<S: Store> Api<S> {
    /// The site that pilots made a report is about, with the number of
    /// the revision it is about: the one named, or the one the site
    /// stands at. Found as the site's own addresses find it: a site of a
    /// source is not reported here, one that was taken down shows nothing
    /// and is gone, and one that pilots removed is still there with its
    /// history.
    pub(crate) async fn reported_site(
        &self,
        id: &str,
        revision: Option<u32>,
    ) -> Result<(SiteId, u32), ApiError> {
        let id: SiteId = id.parse().map_err(|_| no_such_site())?;
        if Layer::of(&id).is_none() {
            return Err(ApiError::bad_request(
                ErrorCode::InvalidReport,
                "a site of a source is its source's record and is corrected there; only a \
                 site that a partner or a pilot added on this server is reported here",
            ));
        }
        if !is_made_here(&id) {
            return Err(no_such_site());
        }
        let site = self.store.pilot_site(&id).await?.ok_or_else(no_such_site)?;
        if site.taken_down {
            return Err(gone());
        }
        let revision = revision.unwrap_or(site.revision);
        if revision == 0 || revision > site.revision {
            return Err(ApiError::missing(
                ErrorCode::SiteRevisionNotFound,
                "the site has no revision with this number",
            ));
        }
        Ok((id, revision))
    }
}

/// What the handlers keep between requests: the settings, and how much
/// each account has used of what is counted over a day.
#[derive(Debug)]
pub(crate) struct PilotSites {
    settings: Settings,
    /// Sites added, by account.
    added: Throttle,
    /// Revisions written, by account.
    revised: Throttle,
    /// Confirmations given and taken back, by account.
    confirmed: Throttle,
}

/// Which count a write takes from.
#[derive(Debug, Clone, Copy)]
enum Counts {
    Added,
    Revised,
    Confirmed,
}

/// Why a write is not let through, as the API says it.
fn refusal(refused: Refused) -> ApiError {
    match refused {
        Refused::Suspended => ApiError::forbidden(
            ErrorCode::PublishingSuspended,
            "whoever runs this server has suspended this account's publishing: it can add \
             and change no site, and confirm none",
        ),
        Refused::ForPartners => ApiError::forbidden(
            ErrorCode::SitesForPartners,
            "this site is one that Venti's partners add and correct, and this account is \
             not one of them; a pilot who finds it wrong reports it (POST /v1/reports)",
        ),
        Refused::Closed => ApiError::forbidden(
            ErrorCode::SitesClosed,
            "this server takes no sites and no confirmations from pilots",
        ),
    }
}

impl PilotSites {
    pub(crate) fn new(settings: Settings, watched_keys: usize) -> Self {
        let a_day = chrono::Duration::days(1);
        Self {
            settings,
            added: Throttle::new("accounts that added sites", a_day, watched_keys),
            revised: Throttle::new("accounts that revised sites", a_day, watched_keys),
            confirmed: Throttle::new("accounts that confirmed sites", a_day, watched_keys),
        }
    }

    /// Whether the server takes sites for the crowdsourced layer, and
    /// confirmations, from pilots: what the health answer says before an
    /// app offers either.
    pub(crate) fn accepts(&self) -> bool {
        self.settings.accept
    }

    /// Lets a write of a site of a layer through or says why not: the one
    /// place every write of a site passes. In this order: whether this
    /// account may write to the layer on this server, and whether it has
    /// written as much today as an account may.
    fn writer(
        &self,
        signed: &Signed,
        layer: Layer,
        counts: Counts,
        now: Timestamp,
    ) -> Result<(), ApiError> {
        pilot_sites::may_write(&signed.account, layer, self.settings.accept).map_err(refusal)?;
        self.counted(signed, counts, now)
    }

    /// Lets a confirmation, or its withdrawal, through or says why not.
    fn confirmer(&self, signed: &Signed, now: Timestamp) -> Result<(), ApiError> {
        pilot_sites::may_confirm(&signed.account, self.settings.accept).map_err(refusal)?;
        self.counted(signed, Counts::Confirmed, now)
    }

    fn counted(&self, signed: &Signed, counts: Counts, now: Timestamp) -> Result<(), ApiError> {
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
            Counts::Confirmed => (
                &self.confirmed,
                limits.confirmations_per_day,
                "this account has confirmed sites as often today as one account may",
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
        confirmed: confirmed(site),
    })
}

fn confirmed(site: &PilotSite) -> SiteConfirmed {
    SiteConfirmed {
        pilots: site.confirmed.pilots,
        latest_day: site.confirmed.latest_day,
    }
}

/// The layer of Venti's own a site is of. An identifier is checked by
/// [`is_made_here`] before a site is read, so one that comes back from
/// the store is of one.
fn layer_of(id: &SiteId) -> Result<Layer, ApiError> {
    Layer::of(id).ok_or_else(ApiError::internal)
}

/// A site of one of Venti's own layers as its address answers it.
fn answer(site: PilotSite) -> Result<SiteResponse, ApiError> {
    Ok(SiteResponse {
        standing: Some(standing(&site)?),
        attributions: vec![layer_of(&site.site.id)?.attribution()],
        site: site.site,
        lasting: true,
    })
}

/// What a path under `/v1/sites/` names.
enum Target {
    /// A site of any layer.
    Site(SiteId),
    /// The history of a site of Venti's own.
    History(SiteId),
    /// Where a revision that restores an earlier one is sent.
    Revisions(SiteId),
    /// The caller's own confirmation of a site of Venti's own.
    Confirmation(SiteId),
    /// What pilots confirm of a site of Venti's own.
    Confirmations(SiteId),
}

/// Whether an identifier has the form of one the server makes for a site
/// of one of its own layers.
fn is_made_here(id: &SiteId) -> bool {
    Layer::of(id).is_some() && secret::is_id(id.local())
}

/// Reads the rest of a path. A source's own key may hold a `/`, so the
/// two endings are taken only from an identifier of the community's
/// layer, which holds none.
fn target(rest: &str) -> Result<Target, ApiError> {
    for (ending, make) in [
        ("/history", Target::History as fn(SiteId) -> Target),
        ("/revisions", Target::Revisions),
        ("/confirmations", Target::Confirmations),
        ("/confirmation", Target::Confirmation),
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
        Target::Site(id) if Layer::of(&id).is_none() => Err(ApiError::forbidden(
            ErrorCode::SitesClosed,
            "this site is its source's record and is corrected at its source; the \
                 record's url says where",
        )),
        _ => Err(no_such_site()),
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PageParams {
    after: Option<String>,
    since: Option<String>,
    limit: Option<String>,
    layer: Option<String>,
}

/// `GET /v1/sites/{id}` and `GET /v1/sites/{id}/history`.
pub(crate) async fn show<S: Store>(
    State(api): State<Arc<Api<S>>>,
    headers: HeaderMap,
    Path(rest): Path<String>,
    Query(params): Query<PageParams>,
) -> Result<Response, ApiError> {
    match target(&rest)? {
        Target::Site(id) if Layer::of(&id).is_some() => {
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
                            // The token's name stays here: it is the
                            // pilot's word for a program of theirs, and
                            // a history is read by everyone.
                            venti_store::SiteVia::Token(_) => SiteVia::Token,
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
                attributions: vec![layer_of(&site.site.id)?.attribution()],
            };
            Ok(cache::json(&headers, &answer))
        }
        Target::Confirmations(id) => {
            let (site, list) = api
                .store
                .site_confirmations(&id, MAX_SITE_CONFIRMATIONS)
                .await?
                .ok_or_else(no_such_site)?;
            // Nobody takes off at a site that is gone, and one that was
            // taken down shows nothing.
            if !site.stands() {
                return Err(gone());
            }
            let confirmations = list
                .into_iter()
                .map(|said| {
                    Ok(SiteConfirmation {
                        by: pilot(Some(&said.pilot))?.ok_or_else(ApiError::internal)?,
                        day: said.day,
                        note: said.note,
                        with_flight: said.with_flight,
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?;
            let answer = SiteConfirmationsResponse {
                id,
                confirmed: confirmed(&site),
                confirmations,
            };
            Ok(cache::json(&headers, &answer))
        }
        Target::Revisions(_) => Err(ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            ErrorCode::MethodNotAllowed,
            "a revision is sent with POST; a site's revisions are read at its history",
        )),
        Target::Confirmation(_) => Err(ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            ErrorCode::MethodNotAllowed,
            "a pilot's confirmation is sent with PUT and taken back with DELETE; what pilots \
             confirm of a site is read at its confirmations",
        )),
    }
}

/// `GET /v1/sites/changes[?layer=…]`: what happened to the sites of one
/// of Venti's own layers since a program last asked. The partners' layer
/// unless `layer` names the crowdsourced one: a mirror follows each with a
/// cursor of its own.
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
    let layer = match params.layer.as_deref() {
        None => Layer::Partners,
        Some(name) => Layer::named(name).ok_or_else(|| {
            ApiError::bad_request(
                ErrorCode::InvalidLayers,
                format!(
                    "layer names one layer of Venti's own sites: {}",
                    Layer::ALL.map(Layer::name).join(" or ")
                ),
            )
        })?,
    };
    let source = layer.source();
    let mut changes = api.store.pilot_site_changes(&source, asked, limit).await?;
    // A cursor from beyond the layer's latest change was not given out by
    // this database as it is now. Start again from the beginning, and say
    // so.
    let reset = asked > changes.latest;
    let since = if reset { 0 } else { asked };
    if reset {
        changes = api.store.pilot_site_changes(&source, 0, limit).await?;
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
        attributions: vec![layer.attribution()],
    };
    Ok(cache::json(&headers, &answer))
}

impl<S: Store> Api<S> {
    /// The record a draft makes under an identifier, if it can be kept:
    /// cleaned, held to the checks every site record passes, with a name
    /// and a use, at a place inside a region this server serves.
    fn site_from(&self, draft: SiteDraft, id: SiteId) -> Result<Site, ApiError> {
        // What a site from a pilot has to be is said where an app reads
        // it too. The words name the field and repeat nothing of the text.
        let site = draft.record(id).map_err(|faults| match faults.first() {
            Some(fault) => invalid(format!("{}: {}", fault.field, fault.reason)),
            None => invalid("the site cannot be kept"),
        })?;
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
        let write = self.store.revise_pilot_site(&revising, APART_M).await?;
        self.written(write).await
    }

    /// What a write came to, as the API says it. A site that stands too
    /// close is named in the error, with the name it has now.
    async fn written(&self, write: SiteWrite) -> Result<PilotSite, ApiError> {
        let SiteWrite::TooClose(other) = &write else {
            return written(write);
        };
        let other = other.clone();
        let name = self
            .store
            .pilot_site(&other)
            .await?
            .and_then(|site| site.site.name);
        written(write).map_err(|error| error.naming(ErrorSite { id: other, name }))
    }
}

/// What a write came to, as the API says it.
fn written(write: SiteWrite) -> Result<PilotSite, ApiError> {
    match write {
        SiteWrite::Written(site) | SiteWrite::Same(site) => Ok(*site),
        // `site` in the error names the other one: `Api::written` adds it.
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
    May(signed, ..): May<WriteSites>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    let now = api.clock.now();
    // Who adds a site decides its layer: a partner's goes to the
    // partners', every other pilot's to the crowdsourced one.
    let layer = pilot_sites::layer_for(&signed.account);
    api.pilot_sites.writer(&signed, layer, Counts::Added, now)?;
    let request: AddSiteRequest = body::json(&headers, &body)?;
    let id = SiteId::new(layer.source(), secret::random_id()?).map_err(|_| ApiError::internal())?;
    let new = NewPilotSite {
        site: api.site_from(request.site, id)?,
        author_id: signed.account.id.clone(),
        at: now,
        via: signed.site_via(),
        comment: comment(request.comment.as_deref())?,
    };
    let write = api.store.add_pilot_site(&new, APART_M).await?;
    let site = api.written(write).await?;
    Ok(cache::private_json(StatusCode::CREATED, &answer(site)?))
}

/// `PUT /v1/sites/{id}`: replaces what a site says, as a new revision.
/// `PUT /v1/sites/{id}/confirmation` comes here too: see [`confirm`].
pub(crate) async fn change<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSites>,
    headers: HeaderMap,
    Path(rest): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    body::must_say_json(&headers)?;
    if let Target::Confirmation(id) = target(&rest)? {
        return confirm(&api, &signed, id, &headers, &body).await;
    }
    let id = made_here(&rest)?;
    let now = api.clock.now();
    api.pilot_sites
        .writer(&signed, layer_of(&id)?, Counts::Revised, now)?;
    let request: ChangeSiteRequest = body::json(&headers, &body)?;
    let revising = SiteRevising {
        act: SiteAct::Change(Box::new(api.site_from(request.site, id.clone())?)),
        id,
        author_id: Some(signed.account.id.clone()),
        at: now,
        via: signed.site_via(),
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
    May(signed, ..): May<WriteSites>,
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
    api.pilot_sites
        .writer(&signed, layer_of(&id)?, Counts::Revised, now)?;
    let request: RestoreSiteRequest = body::json(&headers, &body)?;
    let revising = SiteRevising {
        id,
        author_id: Some(signed.account.id.clone()),
        at: now,
        via: signed.site_via(),
        act: SiteAct::Restore(request.restore),
        comment: comment(request.comment.as_deref())?,
        expects: None,
    };
    let site = api.revise(revising).await?;
    Ok(cache::private_json(StatusCode::OK, &answer(site)?))
}

/// `DELETE /v1/sites/{id}/confirmation` comes here too: see [`withdraw`].
///
/// `DELETE /v1/sites/{id}`: removes a site, as a new revision. A pilot's
/// removal is never a take-down: the history stays, and any pilot brings
/// the site back by restoring a revision.
pub(crate) async fn remove<S: Store>(
    State(api): State<Arc<Api<S>>>,
    May(signed, ..): May<WriteSites>,
    headers: HeaderMap,
    Path(rest): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    if let Target::Confirmation(id) = target(&rest)? {
        return withdraw(&api, &signed, id).await;
    }
    let id = made_here(&rest)?;
    let now = api.clock.now();
    api.pilot_sites
        .writer(&signed, layer_of(&id)?, Counts::Revised, now)?;
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
        via: signed.site_via(),
        act: SiteAct::Remove { take_down: false },
        comment: comment(request.comment.as_deref())?,
        expects: None,
    };
    api.revise(revising).await?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

// --- What pilots confirm of a site -------------------------------------------

fn unconfirmable(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(ErrorCode::InvalidConfirmation, message)
}

/// `PUT /v1/sites/{id}/confirmation`: "I took off here and it was good."
/// One confirmation an account and a site: a second one takes the place
/// of the first. Answers the site as it stands with it.
async fn confirm<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    id: SiteId,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Response, ApiError> {
    let now = api.clock.now();
    api.pilot_sites.confirmer(signed, now)?;
    let request: ConfirmSiteRequest = body::json(headers, body)?;
    // The day is the pilot's own, wherever on earth they are: it may be
    // tomorrow by the server's clock, and no later.
    let today = now.date_naive();
    let ahead = (request.day - today).num_days();
    if !(-CONFIRMATION_DAYS_BACK..=1).contains(&ahead) {
        return Err(unconfirmable(format!(
            "day is today or one of the {CONFIRMATION_DAYS_BACK} days before it"
        )));
    }
    let note = parse_site_comment(request.note.as_deref()).map_err(|_| {
        unconfirmable(format!(
            "note has at most {MAX_SITE_COMMENT_LENGTH} characters"
        ))
    })?;
    let with_flight = match request.flight.as_deref() {
        None => false,
        Some(flight) => {
            api.took_off_at(flight, &signed.account.id, &id).await?;
            true
        }
    };
    let new = NewSiteConfirmation {
        site: id,
        account_id: signed.account.id.clone(),
        day: request.day,
        note,
        with_flight,
        at: now,
    };
    match api.store.confirm_pilot_site(&new).await? {
        SiteConfirming::Confirmed(site) => Ok(cache::private_json(StatusCode::OK, &answer(*site)?)),
        SiteConfirming::Removed => Err(gone()),
        SiteConfirming::NoSuchSite => Err(no_such_site()),
        SiteConfirming::NoSuchAccount => Err(ApiError::unauthorized(
            "the account this session belonged to is gone",
        )),
    }
}

/// `DELETE /v1/sites/{id}/confirmation`: takes the caller's confirmation
/// back. `204` whether there was one or not.
async fn withdraw<S: Store>(
    api: &Api<S>,
    signed: &Signed,
    id: SiteId,
) -> Result<Response, ApiError> {
    api.pilot_sites.confirmer(signed, api.clock.now())?;
    api.store
        .withdraw_site_confirmation(&id, &signed.account.id)
        .await?
        .ok_or_else(no_such_site)?;
    Ok(cache::private(StatusCode::NO_CONTENT.into_response()))
}

impl<S: Store> Api<S> {
    /// Checks the flight a confirmation names: it is this account's, and
    /// it took off within [`CONFIRMATION_FLIGHT_WITHIN_M`] of the site as
    /// the site stands now. A flight of another pilot is answered as one
    /// that is not there: the address of a flight says nothing to whoever
    /// may not see it.
    async fn took_off_at(
        &self,
        flight: &str,
        account_id: &str,
        site: &SiteId,
    ) -> Result<(), ApiError> {
        let not_theirs = || unconfirmable("flight is the address of no flight of this account");
        if !secret::is_id(flight) {
            return Err(not_theirs());
        }
        let flight = self
            .store
            .flight(flight)
            .await?
            .filter(|flight| flight.account_id == account_id)
            .ok_or_else(not_theirs)?;
        let site = self
            .store
            .pilot_site(site)
            .await?
            .ok_or_else(no_such_site)?;
        let near = flight.derived.takeoff.is_some_and(|takeoff| {
            takeoff.position.distance_m(site.site.position) <= CONFIRMATION_FLIGHT_WITHIN_M
        });
        if near {
            Ok(())
        } else {
            Err(unconfirmable(format!(
                "the flight did not take off within {CONFIRMATION_FLIGHT_WITHIN_M} m of this \
                 site; leave it out to confirm the site without it"
            )))
        }
    }
}
