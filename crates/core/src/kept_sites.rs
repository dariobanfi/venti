//! The pilot's own sites, kept on the device, and the take-offs of Today.
//!
//! A pilot marks a flying site as theirs: the take-offs they fly from. The
//! core keeps the site's record in its storage, so that the list is there
//! without a network, with names and places, and Today can ask for the
//! forecast of a take-off that lies far from where the pilot is.
//!
//! **Where they are kept.** In the core's storage, as everything else the
//! pilot keeps: one file per source under `sites-kept/`, each with the
//! source's credit. Sites of different sources are never put into one
//! file, as the server never puts them into one layer; the licences ask
//! for that, and for the credit wherever a site is shown. A record is the
//! canonical one the server sent, so that what a later version of the app
//! reads from a site, it reads from the kept ones too.
//!
//! **With an account, the list is on the account too.** Which sites a
//! pilot keeps is the pilot's own and private. Signed out, it is on this
//! device and nowhere else. Signed in, it is kept in step with the
//! pilot's account, where nobody else reads it, and follows the pilot to
//! their other devices; the files on the device are then the copy that
//! works without a network. There is one way of keeping and forgetting,
//! [`Core::keep_site`] and [`Core::forget_site`]: they change the device's
//! copy and note what the account has yet to learn, and
//! [`crate::KeptSitesSync`] carries it there and brings back what the
//! other devices did.
//!
//! **How old a kept record is.** It is as its source had it when the app
//! last saw the site in an answer, and carries that date. Today refreshes
//! the kept sites it finds in the answer it is handed. A kept site that
//! lies outside that answer stays as it was, and one its source has since
//! removed stays kept: nothing here notices the removal.
//!
//! **Today.** Which take-offs Today lists, and in which order, is decided
//! here and not in the apps: the pilot's own first, however far away, then
//! the take-offs around the place. [`today_order`] puts them in order for
//! a day, the pilot's own first again.

use std::cmp::Ordering;
use std::io;

use serde::{Deserialize, Serialize};
use venti_api_types::v1::{Attribution, SiteStanding};
use venti_domain::{GeoPoint, Site, SiteId, SiteOutline, SiteUse, SourceId, WindSector};
use venti_flight::live::navigation::bearing_deg;

use crate::sites::{SiteKind, SiteView, SitesMap, site_credit, site_view, standing_of};
use crate::today::{Flyability, Thermals};
use crate::{Bounds, Coordinate, Core, CoreError, Credit, TargetPlace, WaypointKind};

/// The folder the kept sites are stored in, one file for each source.
const FOLDER: &str = "sites-kept";

/// How far from the place asked about a take-off may lie to be on Today,
/// metres. The pilot's own are listed however far away they are.
const TODAY_REACH_M: f64 = 30_000.0;

/// How many take-offs around the place Today lists.
const TODAY_NEAR: usize = 6;

/// How many of the pilot's own take-offs Today lists: each is a forecast
/// the server is asked for. Ours, and the owner's to decide.
const TODAY_KEPT: usize = 6;

/// The kept sites of one source, as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct KeptLayer {
    /// The source.
    pub(crate) source: SourceId,
    /// The source's credit, with what its licence asks to be shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attribution: Option<Attribution>,
    /// The sites, in the order the pilot kept them.
    pub(crate) sites: Vec<KeptRecord>,
}

/// One kept site, as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct KeptRecord {
    /// The canonical record, as the server sent it.
    pub(crate) site: Site,
    /// For a site that pilots made on Venti: who added it and who changed
    /// it last, as the server said when the record was last seen. A
    /// source's site has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) standing: Option<SiteStanding>,
    /// The date the source's data was of when the record was last seen in
    /// an answer, seconds since 1970-01-01 UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) as_of_unix_s: Option<i64>,
    /// When the pilot kept it, as the account noted it, seconds since
    /// 1970-01-01 UTC. Absent for a site the account has not taken yet:
    /// the core reads no clock, so the note is made when the site is sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) kept_at_unix_s: Option<i64>,
}

/// One of the pilot's own sites, for a list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct KeptSite {
    /// The site, as its source had it when the app last saw it.
    pub site: SiteView,
    /// The date the source's data was of then, seconds since 1970-01-01
    /// UTC: how old the record is.
    pub as_of_unix_s: Option<i64>,
    /// How far it is from the place asked from, metres. Absent when the
    /// list was asked for without a place.
    pub distance_m: Option<f64>,
    /// The direction to it from that place, degrees clockwise from north.
    pub bearing_deg: Option<f64>,
}

/// The pilot's own sites.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct KeptSites {
    /// The sites: with a place given the nearest first, without by name.
    pub sites: Vec<KeptSite>,
    /// Whose data they are, with what the licences ask to be shown: one
    /// credit for each source that has a site in the list.
    pub credits: Vec<Credit>,
    /// What to say with the list: where it is kept, and what a kept
    /// record is. Signed out: "Kept on this device only: sign in to have
    /// it on your other devices. …"; signed in and in step: "Kept on your
    /// Venti account and on this device. …"; signed in with something the
    /// account has yet to learn: "Kept on this device, and on your Venti
    /// account after the next sync. …".
    pub note: String,
}

/// One take-off of Today's list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TodayTakeoff {
    /// The take-off. Its forecast is asked for at its `position` and read
    /// with its `wind` and `elevation_m` ([`crate::site_outlook`]).
    pub site: SiteView,
    /// Whether it is one of the pilot's own.
    pub is_kept: bool,
    /// How far it is from the place asked about, metres. Absent when no
    /// place was given.
    pub distance_m: Option<f64>,
}

/// The take-offs Today lists.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TodayTakeoffs {
    /// The take-offs: the pilot's own first, then those around the place.
    /// [`today_order`] puts them in order for a day.
    pub takeoffs: Vec<TodayTakeoff>,
    /// What to call the pilot's own, where a screen names the two groups:
    /// "Your take-offs".
    pub kept_title: String,
    /// What to call the others: "Within 30 km".
    pub near_title: String,
    /// Said when the pilot has more take-offs of their own than Today
    /// lists: "2 more of your take-offs are not listed: Today takes the 6
    /// nearest."; without a place, "the first 6 by name". Empty otherwise.
    pub kept_more: String,
    /// Whose data the take-offs are, with what the licences ask to be
    /// shown: one credit for each source that has a take-off in the list.
    pub credits: Vec<Credit>,
}

/// One take-off of Today as its place in the list is decided: what the
/// forecast says of it on the day shown.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TodayStanding {
    /// The take-off's identifier.
    pub id: String,
    /// Whether it is one of the pilot's own, as [`TodayTakeoff`] says.
    pub is_kept: bool,
    /// How far it is from the place asked about, as [`TodayTakeoff`] says.
    pub distance_m: Option<f64>,
    /// The verdict of the day shown ([`crate::OutlookDay::verdict`]).
    /// `None` while the server has not answered for the take-off, and
    /// when its forecast does not reach the day.
    pub verdict: Option<Flyability>,
    /// What the day's thermals make ([`crate::OutlookDay::thermals`]).
    pub thermals: Option<Thermals>,
}

/// Today's take-offs in order for a day.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TodayOrder {
    /// The take-offs' identifiers in the order to list them.
    pub ids: Vec<String>,
    /// The take-off that looks best on the day, whose line stands at the
    /// head of the screen. `None` until one has a forecast for the day.
    pub best: Option<String>,
}

#[uniffi::export]
impl Core {
    /// Marks a site as one of the pilot's own and keeps its record on the
    /// device. Keeping a site that is kept already takes its record anew.
    ///
    /// `credits` are the credits of the answer the site came from
    /// ([`SitesMap::credits`], or the one of [`crate::SiteAtPlace`]); the
    /// credit of the site's source is kept with it, because a kept site is
    /// shown without a network and the licence asks for the credit
    /// wherever it is shown. `as_of_unix_s` is the date of the source's
    /// data, as the same answer gives it.
    pub fn keep_site(
        &self,
        site: SiteView,
        credits: Vec<Credit>,
        as_of_unix_s: Option<i64>,
    ) -> Result<(), CoreError> {
        let record = site_of(&site).ok_or_else(|| CoreError::Storage {
            message: "This site cannot be kept: its record is not one the app can read.".to_owned(),
        })?;
        let source = record.id.source().clone();
        // Read before anything is written: what the account has yet to
        // learn is noted with the change, or the change is not made.
        let mut sync = self.kept_sync()?;
        let mut layer = self
            .kept_layer(&file_of(&source))?
            .unwrap_or_else(|| KeptLayer {
                source: source.clone(),
                attribution: None,
                sites: Vec::new(),
            });
        if let Some(attribution) = attribution_of(&credits, &source) {
            layer.attribution = Some(attribution);
        }
        let kept = KeptRecord {
            site: record,
            standing: standing_of(&site),
            as_of_unix_s,
            kept_at_unix_s: None,
        };
        let id = kept.site.id.to_string();
        match layer
            .sites
            .iter_mut()
            .find(|have| have.site.id == kept.site.id)
        {
            Some(have) => *have = kept,
            None => layer.sites.push(kept),
        }
        self.store_kept_layer(&layer)?;
        if let Some(sync) = &mut sync {
            sync.kept_here(&id);
            self.store_kept_sync(sync)?;
        }
        Ok(())
    }

    /// Unmarks a site: it is no longer one of the pilot's own. Forgetting
    /// one that is not kept is not an error.
    pub fn forget_site(&self, id: String) -> Result<(), CoreError> {
        let Ok(id) = id.parse::<SiteId>() else {
            return Ok(());
        };
        let mut sync = self.kept_sync()?;
        let Some(mut layer) = self.kept_layer(&file_of(id.source()))? else {
            return Ok(());
        };
        let before = layer.sites.len();
        layer.sites.retain(|kept| kept.site.id != id);
        if layer.sites.len() != before {
            self.store_kept_layer(&layer)?;
            if let Some(sync) = &mut sync {
                sync.forgotten_here(&id.to_string());
                self.store_kept_sync(sync)?;
            }
        }
        Ok(())
    }

    /// Whether a site is one of the pilot's own, for the button on its
    /// page.
    pub fn is_site_kept(&self, id: String) -> Result<bool, CoreError> {
        let Ok(id) = id.parse::<SiteId>() else {
            return Ok(false);
        };
        Ok(self
            .kept_layer(&file_of(id.source()))?
            .is_some_and(|layer| layer.sites.iter().any(|kept| kept.site.id == id)))
    }

    /// The pilot's own sites, from the device: no network is needed.
    /// With a place given they are listed nearest first, each with its
    /// distance and direction from there; without, by name.
    pub fn kept_sites(&self, from: Option<Coordinate>) -> Result<KeptSites, CoreError> {
        let from = from.and_then(|from| GeoPoint::new(from.lat, from.lon).ok());
        let layers = self.kept_layers()?;
        let mut sites: Vec<KeptSite> = layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .map(|kept| KeptSite {
                site: site_view(&kept.site, kept.standing.as_ref()),
                as_of_unix_s: kept.as_of_unix_s,
                distance_m: from.map(|from| from.distance_m(kept.site.position)),
                bearing_deg: from.map(|from| bearing_deg(from, kept.site.position)),
            })
            .collect();
        match from {
            Some(_) => sites.sort_by(|a, b| nearer(a.distance_m, b.distance_m)),
            None => sites.sort_by_cached_key(|kept| by_name(&kept.site)),
        }
        Ok(KeptSites {
            sites,
            credits: layers
                .iter()
                .filter(|layer| !layer.sites.is_empty())
                .filter_map(|layer| layer.attribution.as_ref().map(site_credit))
                .collect(),
            note: format!(
                "{} A kept site is as its source had it on the day shown with it.",
                match self.kept_sync()? {
                    None => {
                        "Kept on this device only: sign in to have it on your other devices."
                    }
                    Some(sync) if sync.is_in_step() => {
                        "Kept on your Venti account and on this device."
                    }
                    Some(_) => {
                        "Kept on this device, and on your Venti account after the next sync."
                    }
                }
            ),
        })
    }

    /// The take-offs Today lists: the pilot's own first, however far away,
    /// then the take-offs around the place.
    ///
    /// `place` is where the pilot is, or the place the screen stands for.
    /// `near` is the server's answer for [`today_area`] around it, as
    /// [`crate::sites_map`] read it; `None` when there is no answer, and
    /// then the list is the pilot's own take-offs, which need no network.
    /// Without a place they are listed by name and no others are.
    ///
    /// Of the pilot's own, the sites that are places to take off from are
    /// listed, the six nearest. Of the others, the take-offs within 30 km
    /// that their source does not mark as discouraged and that are not of
    /// the crowdsourced layer (an unofficial place is not put forward as
    /// one to fly from today; the pilot keeps it to have it here): those
    /// with a name
    /// of their own first ("Take-off" tells a pilot nothing), then the
    /// nearest, six at most.
    ///
    /// A kept site that the answer holds is refreshed from it: its record
    /// on the device becomes the answer's.
    pub fn today_takeoffs(
        &self,
        place: Option<Coordinate>,
        near: Option<SitesMap>,
    ) -> Result<TodayTakeoffs, CoreError> {
        let here = place.and_then(|place| GeoPoint::new(place.lat, place.lon).ok());
        let mut layers = self.kept_layers()?;
        if let Some(seen) = &near {
            self.refresh_kept(&mut layers, seen);
        }
        let away = |position: GeoPoint| here.map(|here| here.distance_m(position));

        let mut own: Vec<TodayTakeoff> = layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .filter(|kept| kept.site.is(SiteUse::Takeoff))
            .map(|kept| TodayTakeoff {
                site: site_view(&kept.site, kept.standing.as_ref()),
                is_kept: true,
                distance_m: away(kept.site.position),
            })
            .collect();
        match here {
            Some(_) => own.sort_by(|a, b| nearer(a.distance_m, b.distance_m)),
            None => own.sort_by_cached_key(|takeoff| by_name(&takeoff.site)),
        }
        let more = own.len().saturating_sub(TODAY_KEPT);
        own.truncate(TODAY_KEPT);
        // Which of them Today takes, for the line that says some are not
        // listed.
        let taken = match here {
            Some(_) => format!("the {TODAY_KEPT} nearest"),
            None => format!("the first {TODAY_KEPT} by name"),
        };

        let is_kept = |id: &str| {
            layers
                .iter()
                .flat_map(|layer| &layer.sites)
                .any(|kept| kept.site.id.to_string() == id)
        };
        let mut around: Vec<TodayTakeoff> = near
            .iter()
            .flat_map(|seen| &seen.sites)
            .filter(|site| site.uses.contains(&SiteKind::Takeoff) && !site.discouraged)
            // A crowdsourced place is not official and checked by nobody:
            // Today, which reads as "where to fly today", does not put it
            // forward, whether the answer holds that layer or not. A pilot
            // who kept one sees it among their own.
            .filter(|site| !site.by_pilots.as_ref().is_some_and(|by| by.from_pilots))
            .filter(|site| !is_kept(&site.id))
            .filter_map(|site| {
                let position = GeoPoint::new(site.position.lat, site.position.lon).ok()?;
                let distance_m = away(position).filter(|distance| *distance <= TODAY_REACH_M)?;
                Some(TodayTakeoff {
                    site: site.clone(),
                    is_kept: false,
                    distance_m: Some(distance_m),
                })
            })
            .collect();
        around.sort_by(|a, b| {
            b.site
                .is_named
                .cmp(&a.site.is_named)
                .then_with(|| nearer(a.distance_m, b.distance_m))
        });
        around.truncate(TODAY_NEAR);

        // One credit for each source in the list: the answer's, which is
        // the newer, else the one kept with the pilot's own.
        let mut credits: Vec<Credit> = Vec::new();
        for takeoff in own.iter().chain(&around) {
            let source = &takeoff.site.source;
            if credits.iter().any(|credit| &credit.source == source) {
                continue;
            }
            let answered = near
                .iter()
                .flat_map(|seen| &seen.credits)
                .find(|credit| &credit.source == source)
                .cloned();
            let kept = || {
                layers
                    .iter()
                    .find(|layer| layer.source.as_str() == source)
                    .and_then(|layer| layer.attribution.as_ref())
                    .map(site_credit)
            };
            credits.extend(answered.or_else(kept));
        }

        Ok(TodayTakeoffs {
            takeoffs: own.into_iter().chain(around).collect(),
            kept_title: "Your take-offs".to_owned(),
            near_title: format!("Within {:.0} km", TODAY_REACH_M / 1000.0),
            kept_more: match more {
                0 => String::new(),
                1 => format!("1 more of your take-offs is not listed: Today takes {taken}."),
                more => {
                    format!("{more} more of your take-offs are not listed: Today takes {taken}.")
                }
            },
            credits,
        })
    }
}

/// The most take-offs the cockpit's map is given.
const COCKPIT_TAKEOFFS: usize = 50;

/// A take-off on the cockpit's map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CockpitTakeoff {
    /// The site, as the answer or the pilot's own record has it.
    pub site: SiteView,
    /// Whether it is one of the pilot's own.
    pub is_kept: bool,
    /// How far it is from the glider, metres.
    pub distance_m: f64,
    /// The direction to it, degrees clockwise from north.
    pub bearing_deg: f64,
}

/// The take-offs around the glider, for the cockpit's map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CockpitTakeoffs {
    /// Nearest first, at most fifty.
    pub takeoffs: Vec<CockpitTakeoff>,
    /// Credits to show with the map while they are drawn: one for each
    /// source with a take-off in the list.
    pub credits: Vec<Credit>,
}

#[uniffi::export]
impl Core {
    /// The take-offs to draw on the cockpit's map: those the app holds
    /// around the glider, nearest first, at most fifty. They are the
    /// pilot's own take-offs, which need no network, and those of `near`:
    /// the server's answer for [`today_area`] around the glider, as
    /// [`crate::sites_map`] read it, which the shell asks for again when
    /// the glider has flown out of the area it asked for. A site that is
    /// both is drawn once, as the answer has it.
    ///
    /// Every take-off is drawn, those the source marks as discouraged and
    /// those from pilots among them, each with its record saying what it
    /// is: on a map in the air a place is a landmark, not advice to fly
    /// from it.
    pub fn cockpit_takeoffs(
        &self,
        glider: Coordinate,
        near: Option<SitesMap>,
    ) -> Result<CockpitTakeoffs, CoreError> {
        let Ok(here) = GeoPoint::new(glider.lat, glider.lon) else {
            return Ok(CockpitTakeoffs {
                takeoffs: Vec::new(),
                credits: Vec::new(),
            });
        };
        let layers = self.kept_layers()?;
        let is_kept = |id: &str| {
            layers
                .iter()
                .flat_map(|layer| &layer.sites)
                .any(|kept| kept.site.id.to_string() == id)
        };
        let answered: Vec<&SiteView> = near
            .iter()
            .flat_map(|seen| &seen.sites)
            .filter(|site| site.uses.contains(&SiteKind::Takeoff))
            .collect();
        let own = layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .filter(|kept| kept.site.is(SiteUse::Takeoff))
            .filter(|kept| {
                !answered
                    .iter()
                    .any(|site| site.id == kept.site.id.to_string())
            })
            .map(|kept| (site_view(&kept.site, kept.standing.as_ref()), true));
        let around = answered
            .iter()
            .map(|site| ((*site).clone(), is_kept(&site.id)));
        let mut takeoffs: Vec<CockpitTakeoff> = own
            .chain(around)
            .filter_map(|(site, is_kept)| {
                let position = GeoPoint::new(site.position.lat, site.position.lon).ok()?;
                Some(CockpitTakeoff {
                    distance_m: here.distance_m(position),
                    bearing_deg: bearing_deg(here, position),
                    site,
                    is_kept,
                })
            })
            .collect();
        takeoffs.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        takeoffs.truncate(COCKPIT_TAKEOFFS);

        let mut credits: Vec<Credit> = Vec::new();
        for takeoff in &takeoffs {
            let source = &takeoff.site.source;
            if credits.iter().any(|credit| &credit.source == source) {
                continue;
            }
            let from_answer = near
                .iter()
                .flat_map(|seen| &seen.credits)
                .find(|credit| &credit.source == source)
                .cloned();
            let from_kept = || {
                layers
                    .iter()
                    .find(|layer| layer.source.as_str() == source)
                    .and_then(|layer| layer.attribution.as_ref())
                    .map(site_credit)
            };
            credits.extend(from_answer.or_else(from_kept));
        }
        Ok(CockpitTakeoffs { takeoffs, credits })
    }

    /// The landing fields to hand to [`crate::FlightSession::use_landings`],
    /// nearest to the glider first, at most fifty: the landings among the
    /// sites of `near` (the same answer [`Core::cockpit_takeoffs`] is
    /// given), the pilot's own kept sites that are landings, and the
    /// pilot's waypoints of the kind landing. All but `near` need no
    /// network. A site that is both kept and in the answer is listed
    /// once, as the answer has it; a waypoint of the same name within
    /// 100 m of a site already listed is left out.
    ///
    /// The list changes only when what it is made from changes, so a
    /// shell compares it with the one it handed over last and hands it
    /// over again only when it differs: the session begins its reading of
    /// the nearest landing anew with each.
    pub fn cockpit_landings(
        &self,
        glider: Coordinate,
        near: Option<SitesMap>,
    ) -> Result<Vec<TargetPlace>, CoreError> {
        let Ok(here) = GeoPoint::new(glider.lat, glider.lon) else {
            return Ok(Vec::new());
        };
        let layers = self.kept_layers()?;
        let answered: Vec<&SiteView> = near
            .iter()
            .flat_map(|seen| &seen.sites)
            .filter(|site| site.uses.contains(&SiteKind::Landing))
            .collect();
        let own = layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .filter(|kept| kept.site.is(SiteUse::Landing))
            .filter(|kept| {
                !answered
                    .iter()
                    .any(|site| site.id == kept.site.id.to_string())
            })
            .map(|kept| site_view(&kept.site, kept.standing.as_ref()));
        let mut places: Vec<TargetPlace> = answered
            .iter()
            .map(|site| (*site).clone())
            .chain(own)
            .map(|site| TargetPlace {
                name: site.name,
                position: site.position,
                elevation_m: site.elevation_m,
            })
            .collect();
        let waypoints = self.waypoints(None)?;
        for waypoint in waypoints
            .into_iter()
            .filter(|waypoint| waypoint.kind == WaypointKind::Landing)
        {
            let Ok(at) = GeoPoint::new(waypoint.position.lat, waypoint.position.lon) else {
                continue;
            };
            let same = places.iter().any(|place| {
                place.name.eq_ignore_ascii_case(&waypoint.name)
                    && GeoPoint::new(place.position.lat, place.position.lon)
                        .is_ok_and(|other| other.distance_m(at) < SAME_PLACE_M)
            });
            if !same {
                places.push(TargetPlace {
                    name: waypoint.name,
                    position: waypoint.position,
                    elevation_m: waypoint.elevation_m,
                });
            }
        }
        let distance = |place: &TargetPlace| {
            GeoPoint::new(place.position.lat, place.position.lon)
                .map_or(f64::INFINITY, |at| here.distance_m(at))
        };
        places.retain(|place| distance(place).is_finite());
        places.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        places.truncate(COCKPIT_LANDINGS);
        Ok(places)
    }
}

/// The most landings the cockpit is handed.
const COCKPIT_LANDINGS: usize = 50;

/// A waypoint this near a listed landing of its name is that landing,
/// metres.
const SAME_PLACE_M: f64 = 100.0;

impl Core {
    /// Every source's kept sites. A file that storage has yet to fetch is
    /// an error and not "nothing kept": the host fetches it and makes its
    /// call again.
    pub(crate) fn kept_layers(&self) -> Result<Vec<KeptLayer>, CoreError> {
        let mut names = self.storage.list(FOLDER).map_err(storage_error)?;
        names.sort();
        let mut layers = Vec::new();
        for name in names.iter().filter(|name| name.ends_with(".json")) {
            layers.extend(self.kept_layer(name)?);
        }
        Ok(layers)
    }

    /// The kept sites of one source, from its file.
    fn kept_layer(&self, name: &str) -> Result<Option<KeptLayer>, CoreError> {
        let Some(bytes) = self.storage.read(name).map_err(storage_error)? else {
            return Ok(None);
        };
        // The file is the app's own writing. Should it be damaged, nothing
        // of that source is kept.
        let Ok(mut layer) = serde_json::from_slice::<KeptLayer>(&bytes) else {
            return Ok(None);
        };
        // Only what the file's source filed: the source is what the
        // credit is found by.
        layer
            .sites
            .retain(|kept| kept.site.id.source() == &layer.source);
        Ok(Some(layer))
    }

    /// Stores a source's kept sites; a source with none left has no file.
    pub(crate) fn store_kept_layer(&self, layer: &KeptLayer) -> Result<(), CoreError> {
        let name = file_of(&layer.source);
        if layer.sites.is_empty() {
            return self.storage.remove(&name).map_err(storage_error);
        }
        let bytes = serde_json::to_vec(layer).map_err(|error| CoreError::Storage {
            message: format!("Your sites could not be saved: {error}."),
        })?;
        self.storage.write(&name, &bytes).map_err(storage_error)
    }

    /// Takes the records of kept sites anew from an answer that holds
    /// them, with the answer's date and credit. What cannot be stored now
    /// is refreshed by the next answer: the list of the moment has the
    /// newer records either way.
    fn refresh_kept(&self, layers: &mut [KeptLayer], seen: &SitesMap) {
        for layer in layers {
            let before = layer.clone();
            let mut found = false;
            for kept in &mut layer.sites {
                let id = kept.site.id.to_string();
                let Some((site, standing)) = seen
                    .sites
                    .iter()
                    .find(|site| site.id == id)
                    .and_then(|view| Some((site_of(view)?, standing_of(view))))
                else {
                    continue;
                };
                found = true;
                kept.site = site;
                kept.standing = standing;
                kept.as_of_unix_s = seen.as_of_unix_s.or(kept.as_of_unix_s);
            }
            if found && let Some(attribution) = attribution_of(&seen.credits, &layer.source) {
                layer.attribution = Some(attribution);
            }
            if *layer != before {
                let _ = self.store_kept_layer(layer);
            }
        }
    }
}

/// The part of the map Today asks the server's sites for: a box that holds
/// every place within 30 km of the pilot's. The shell asks for it with
/// [`crate::sites_path`], reads the answer with [`crate::sites_map`] and
/// hands it to [`Core::today_takeoffs`].
#[uniffi::export]
pub fn today_area(lat: f64, lon: f64) -> Result<Bounds, CoreError> {
    // A little more than the reach, so that the box holds the circle
    // whatever length is taken for a degree.
    crate::area_around(lat, lon, TODAY_REACH_M * 1.01).map_err(|_| CoreError::Server {
        message: "The app could not ask the server about the take-offs around this place."
            .to_owned(),
    })
}

/// Today's take-offs in the order to list them on a day.
///
/// The pilot's own come first, then the others. Within each group the
/// best day first: a flyable day by its thermals (a big day, a
/// cross-country day, local thermals, then weak or none), then a marginal
/// day, a day that is not flyable, and last a take-off without a forecast
/// for the day; among equals the nearer first.
///
/// The take-off that looks best is the best by the same rule among all of
/// them, the pilot's own or not; of two that stand alike the pilot's own,
/// then the nearer.
#[uniffi::export]
pub fn today_order(takeoffs: Vec<TodayStanding>) -> TodayOrder {
    let by_day = |a: &&TodayStanding, b: &&TodayStanding| {
        rank(a).cmp(&rank(b)).then_with(|| {
            b.is_kept
                .cmp(&a.is_kept)
                .then_with(|| nearer(a.distance_m, b.distance_m))
        })
    };
    let best = takeoffs
        .iter()
        .filter(|takeoff| takeoff.verdict.is_some())
        .min_by(by_day)
        .map(|takeoff| takeoff.id.clone());
    let mut listed: Vec<&TodayStanding> = takeoffs.iter().collect();
    listed.sort_by(|a, b| b.is_kept.cmp(&a.is_kept).then_with(|| by_day(a, b)));
    TodayOrder {
        ids: listed
            .into_iter()
            .map(|takeoff| takeoff.id.clone())
            .collect(),
        best,
    }
}

/// Where a take-off stands on a day, the best first.
fn rank(takeoff: &TodayStanding) -> u8 {
    match (takeoff.verdict, takeoff.thermals) {
        (Some(Flyability::Flyable), Some(Thermals::Big)) => 0,
        (Some(Flyability::Flyable), Some(Thermals::CrossCountry)) => 1,
        (Some(Flyability::Flyable), Some(Thermals::Local)) => 2,
        (Some(Flyability::Flyable), Some(Thermals::Weak) | None) => 3,
        (Some(Flyability::Marginal), _) => 4,
        (Some(Flyability::NotFlyable), _) => 5,
        (None, _) => 6,
    }
}

/// The nearer of two first; a distance that is not known last.
fn nearer(a: Option<f64>, b: Option<f64>) -> Ordering {
    a.unwrap_or(f64::MAX).total_cmp(&b.unwrap_or(f64::MAX))
}

/// What a list by name goes by.
fn by_name(site: &SiteView) -> (String, String) {
    (site.name.to_lowercase(), site.id.clone())
}

/// The name a source's kept sites are stored under.
fn file_of(source: &SourceId) -> String {
    format!("{FOLDER}/{source}.json")
}

fn storage_error(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("Your sites could not be read or written: {error}."),
    }
}

/// A source's credit as it is stored, from the credits of an answer.
fn attribution_of(credits: &[Credit], source: &SourceId) -> Option<Attribution> {
    let credit = credits
        .iter()
        .find(|credit| credit.source == source.as_str())?;
    Some(Attribution {
        source: source.clone(),
        provider: credit.provider.clone(),
        short_name: None,
        licence: credit.licence.clone(),
        text: credit.text.clone(),
        url: credit.url.clone(),
        notice: credit.notice.clone(),
    })
}

/// The canonical record a view was made from: what its source says, without
/// the words. `None` for a view that was not made from one.
fn site_of(view: &SiteView) -> Option<Site> {
    Some(Site {
        id: view.id.parse().ok()?,
        name: view
            .is_named
            .then(|| view.name.trim().to_owned())
            .filter(|name| !name.is_empty()),
        position: GeoPoint::new(view.position.lat, view.position.lon).ok()?,
        elevation_m: view.elevation_m.filter(|height| height.is_finite()),
        uses: view
            .uses
            .iter()
            .map(|kind| match kind {
                SiteKind::Takeoff => SiteUse::Takeoff,
                SiteKind::Landing => SiteUse::Landing,
                SiteKind::Towing => SiteUse::Towing,
                SiteKind::Training => SiteUse::Training,
                SiteKind::Other => SiteUse::Other,
            })
            .collect(),
        wind_directions: view
            .wind
            .iter()
            .filter(|span| span.from_deg.is_finite() && span.to_deg.is_finite())
            .map(|span| WindSector {
                from_deg: span.from_deg,
                to_deg: span.to_deg,
            })
            .collect(),
        paragliding: view.paragliding,
        hang_gliding: view.hang_gliding,
        official: view.official,
        discouraged: view.discouraged,
        url: view.url.clone(),
        website: view.website.clone(),
        description: view.description.clone(),
        fee: view.fee.clone(),
        guest_guidelines: view.guest_guidelines,
        no_flight_time: view.no_flight_time.clone(),
        // An outline that is no closed ring of a size a site has is left
        // out, and the site is kept as a place.
        outline: view
            .outline
            .iter()
            .map(|point| GeoPoint::new(point.lat, point.lon))
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .and_then(|ring| SiteOutline::new(ring).ok()),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::sites::sites_map;
    use crate::storage::testing::on_every_storage;

    /// The answer the server's own tests pin: five sites around the
    /// Tegernsee, as OpenStreetMap has them. Two unnamed ramps on the
    /// Wallberg, the take-off and the landing of the Jägerkamp, and one
    /// more landing.
    fn recorded() -> SitesMap {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../server/tests/golden/sites.json"
        );
        sites_map(47.55, 11.6, 47.85, 12.0, 200, std::fs::read(path).unwrap()).unwrap()
    }

    const RAMP: &str = "osm-free-flying:node/5964676196";
    const OTHER_RAMP: &str = "osm-free-flying:node/5964676197";
    const JAEGERKAMP: &str = "osm-free-flying:way/1080986173";
    const LANDING: &str = "osm-free-flying:way/1080988126";

    /// The Wallberg's summit, beside the two ramps.
    const WALLBERG: Coordinate = Coordinate {
        lat: 47.6642,
        lon: 11.7967,
    };

    fn site(map: &SitesMap, id: &str) -> SiteView {
        map.sites.iter().find(|site| site.id == id).unwrap().clone()
    }

    fn keep(core: &Core, map: &SitesMap, id: &str) {
        core.keep_site(site(map, id), map.credits.clone(), map.as_of_unix_s)
            .unwrap();
    }

    /// Sites as a server would answer them, made up for the tests: no
    /// recorded answer holds enough take-offs around one place.
    fn made_up(sites: &str) -> SitesMap {
        let answer = format!(
            r#"{{"layers": [{{"source": "osm-free-flying", "fetched_at": "2026-11-20T06:00:00Z",
                 "as_of": "2026-11-20T05:00:00Z", "covers": [], "sites": [{sites}]}}],
               "attributions": [{{"source": "osm-free-flying", "provider": "OpenStreetMap contributors",
                 "licence": "ODbL 1.0", "text": "© OpenStreetMap contributors",
                 "url": "https://www.openstreetmap.org/copyright", "notice": "A newer notice."}}]}}"#
        );
        sites_map(40.0, 5.0, 50.0, 18.0, 200, answer.into_bytes()).unwrap()
    }

    /// A take-off of a made-up answer, `north_km` north of the Wallberg.
    fn takeoff(number: u32, name: Option<&str>, north_km: f64, more: &str) -> String {
        let name = name.map_or_else(String::new, |name| format!(r#""name": "{name}","#));
        format!(
            r#"{{"id": "osm-free-flying:node/{number}", {name}
                "position": {{"lat": {}, "lon": 11.7967}}, "uses": ["takeoff"]{more}}}"#,
            47.6642 + north_km / 111.195
        )
    }

    fn ids(takeoffs: &TodayTakeoffs) -> Vec<(&str, bool)> {
        takeoffs
            .takeoffs
            .iter()
            .map(|takeoff| (takeoff.site.id.rsplit('/').next().unwrap(), takeoff.is_kept))
            .collect()
    }

    #[test]
    fn the_cockpit_map_has_the_take_offs_around_the_glider_nearest_first() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            // Nothing held: nothing to draw, and no credit.
            let none = core.cockpit_takeoffs(WALLBERG, None).unwrap();
            assert!(none.takeoffs.is_empty() && none.credits.is_empty());

            // A kept take-off far away, held without a network.
            let far = made_up(&takeoff(1, Some("Far"), 80.0, ""));
            keep(&core, &far, "osm-free-flying:node/1");
            let kept = core.cockpit_takeoffs(WALLBERG, None).unwrap();
            assert_eq!(kept.takeoffs.len(), 1);
            assert!(kept.takeoffs[0].is_kept);
            assert!((kept.takeoffs[0].distance_m - 80_000.0).abs() < 200.0);
            assert_eq!(kept.credits.len(), 1);

            // Sixty take-offs around, a discouraged one among them, and a
            // landing: the fifty nearest take-offs, the kept one too far
            // to be among them, the landing never.
            let mut sites: Vec<String> = (10..70)
                .map(|number| {
                    let more = if number == 10 {
                        r#", "discouraged": true"#
                    } else {
                        ""
                    };
                    takeoff(number, None, f64::from(number - 9) * 0.5, more)
                })
                .collect();
            sites.push(
                r#"{"id": "osm-free-flying:node/99", "position": {"lat": 47.6643, "lon": 11.7967}, "uses": ["landing"]}"#
                    .to_owned(),
            );
            let near = made_up(&sites.join(","));
            let drawn = core.cockpit_takeoffs(WALLBERG, Some(near.clone())).unwrap();
            assert_eq!(drawn.takeoffs.len(), 50);
            assert!(
                drawn
                    .takeoffs
                    .windows(2)
                    .all(|pair| pair[0].distance_m <= pair[1].distance_m)
            );
            assert_eq!(drawn.takeoffs[0].site.id, "osm-free-flying:node/10");
            assert!(drawn.takeoffs[0].site.discouraged);
            assert!((drawn.takeoffs[0].bearing_deg).abs() < 1.0);
            assert!(drawn.takeoffs.iter().all(|takeoff| !takeoff.is_kept));
            assert!(
                drawn
                    .takeoffs
                    .iter()
                    .all(|takeoff| takeoff.site.id != "osm-free-flying:node/99")
            );
            assert_eq!(drawn.credits, near.credits);

            // A kept take-off that the answer holds is drawn once, as kept.
            keep(&core, &near, "osm-free-flying:node/11");
            let again = core.cockpit_takeoffs(WALLBERG, Some(near)).unwrap();
            let elevens: Vec<&CockpitTakeoff> = again
                .takeoffs
                .iter()
                .filter(|takeoff| takeoff.site.id == "osm-free-flying:node/11")
                .collect();
            assert_eq!(elevens.len(), 1);
            assert!(elevens[0].is_kept);
        });
    }

    #[test]
    fn the_cockpit_is_handed_the_landings_around_the_glider_nearest_first() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            assert!(core.cockpit_landings(WALLBERG, None).unwrap().is_empty());

            // The answer's landings, not its take-offs; a kept one among
            // them listed once.
            let near = recorded();
            keep(&core, &near, LANDING);
            keep(&core, &near, RAMP);
            let landings = core.cockpit_landings(WALLBERG, Some(near.clone())).unwrap();
            let landing_sites: Vec<&SiteView> = near
                .sites
                .iter()
                .filter(|site| site.uses.contains(&SiteKind::Landing))
                .collect();
            assert!(!landing_sites.is_empty());
            assert_eq!(landings.len(), landing_sites.len());
            assert!(landing_sites.iter().all(|site| {
                landings
                    .iter()
                    .filter(|place| place.position == site.position)
                    .count()
                    == 1
            }));
            let from = GeoPoint::new(WALLBERG.lat, WALLBERG.lon).unwrap();
            let distance = |place: &TargetPlace| {
                from.distance_m(GeoPoint::new(place.position.lat, place.position.lon).unwrap())
            };
            assert!(
                landings
                    .windows(2)
                    .all(|pair| distance(&pair[0]) <= distance(&pair[1]))
            );

            // Without a network: the kept landing, and the waypoint of the
            // kind landing; not the take-offs among the waypoints.
            core.import_waypoints(
                "name,code,country,lat,lon,elev,style,rwdir,rwlen,rwwidth,freq,desc\n\
                 \"Wallberg\",WB,DE,4739.800N,01147.600E,1620m,20,,,,,\n\
                 \"Landeplatz Rottach\",LR,DE,4741.000N,01145.500E,740m,21,,,,,\n"
                    .to_owned(),
            )
            .unwrap();
            let offline = core.cockpit_landings(WALLBERG, None).unwrap();
            let names: Vec<&str> = offline.iter().map(|place| place.name.as_str()).collect();
            assert_eq!(names.len(), 2, "{names:?}");
            let rottach = offline
                .iter()
                .find(|place| place.name == "Landeplatz Rottach")
                .unwrap();
            assert_eq!(rottach.elevation_m, Some(740.0));
            assert!(!names.contains(&"Wallberg"));
        });
    }

    #[test]
    fn a_site_kept_by_an_earlier_version_is_still_read() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            // The file as the app wrote it before a record had a web
            // page, a description or an outline.
            let earlier = r#"{"source":"osm-free-flying","sites":[{"site":{
                "id":"osm-free-flying:way/1080988126","name":"Landeplatz Jägerkamp/Nagelspitz",
                "position":{"lat":47.6924086,"lon":11.8865478},"uses":["landing"],
                "url":"https://www.openstreetmap.org/way/1080988126"},"as_of_unix_s":1763500862}]}"#;
            core.storage
                .write(
                    &file_of(&SourceId::new("osm-free-flying").unwrap()),
                    earlier.as_bytes(),
                )
                .unwrap();

            let kept = core.kept_sites(None).unwrap();
            assert_eq!(kept.sites.len(), 1);
            let field = &kept.sites[0].site;
            assert_eq!(field.name, "Landeplatz Jägerkamp/Nagelspitz");
            assert_eq!(kept.sites[0].as_of_unix_s, Some(1_763_500_862));
            assert!(field.outline.is_empty());
            assert_eq!((&field.website, &field.description), (&None, &None));
            assert!(core.is_site_kept(LANDING.to_owned()).unwrap());

            // Kept anew from an answer of today's server, it has what the
            // record has gained, and has it still after a restart.
            let map = recorded();
            keep(&core, &map, LANDING);
            let again = Core::with_storage(backing.open()).unwrap();
            let kept = again.kept_sites(None).unwrap();
            assert_eq!(kept.sites.len(), 1);
            assert_eq!(kept.sites[0].site, site(&map, LANDING));
            assert_eq!(kept.sites[0].site.outline.len(), 8);
            assert_eq!(
                kept.sites[0].site.website.as_deref(),
                Some("https://www.dhv.de/")
            );
        });
    }

    #[test]
    fn a_kept_site_is_given_back_without_a_network_and_forgotten_again() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            assert!(core.kept_sites(None).unwrap().sites.is_empty());
            assert!(core.kept_sites(None).unwrap().credits.is_empty());

            let map = recorded();
            keep(&core, &map, JAEGERKAMP);
            keep(&core, &map, RAMP);
            keep(&core, &map, LANDING);

            // Still there when the app is started again, and nothing but
            // the device was asked: each as its source had it, with the
            // credit and the date of the answer it came from.
            let again = Core::with_storage(backing.open()).unwrap();
            let kept = again.kept_sites(None).unwrap();
            let names: Vec<&str> = kept
                .sites
                .iter()
                .map(|kept| kept.site.name.as_str())
                .collect();
            assert_eq!(
                names,
                [
                    "Fluggelände Jägerkamp/Nagelspitz",
                    "Landeplatz Jägerkamp/Nagelspitz",
                    "Take-off"
                ]
            );
            for kept in &kept.sites {
                assert_eq!(kept.site, site(&map, &kept.site.id));
                assert_eq!(kept.as_of_unix_s, map.as_of_unix_s);
                assert_eq!((kept.distance_m, kept.bearing_deg), (None, None));
            }
            assert_eq!(kept.credits, map.credits);
            assert!(kept.note.contains("on this device only"), "{}", kept.note);

            // From the Wallberg: the ramp beside it, then the Jägerkamp
            // eight kilometres to the east, then its landing.
            let near = again.kept_sites(Some(WALLBERG)).unwrap();
            let order: Vec<&str> = near
                .sites
                .iter()
                .map(|kept| kept.site.id.as_str())
                .collect();
            assert_eq!(order, [RAMP, LANDING, JAEGERKAMP]);
            assert!(near.sites[0].distance_m.unwrap() < 200.0);
            let far = &near.sites[2];
            assert!(
                (far.distance_m.unwrap() - 8_300.0).abs() < 300.0,
                "{:?}",
                far.distance_m
            );
            assert!(
                (far.bearing_deg.unwrap() - 78.0).abs() < 5.0,
                "{:?}",
                far.bearing_deg
            );

            // A site's page asks whether its site is kept.
            assert!(again.is_site_kept(RAMP.to_owned()).unwrap());
            assert!(!again.is_site_kept(OTHER_RAMP.to_owned()).unwrap());
            assert!(!again.is_site_kept("no site".to_owned()).unwrap());

            // Forgotten, one by one; forgetting twice, or what was never
            // kept or is no site, is fine.
            again.forget_site(RAMP.to_owned()).unwrap();
            assert!(!again.is_site_kept(RAMP.to_owned()).unwrap());
            again.forget_site(RAMP.to_owned()).unwrap();
            again.forget_site(OTHER_RAMP.to_owned()).unwrap();
            again.forget_site("no site".to_owned()).unwrap();
            again
                .forget_site("another-source:node/1".to_owned())
                .unwrap();
            assert_eq!(again.kept_sites(None).unwrap().sites.len(), 2);
            again.forget_site(JAEGERKAMP.to_owned()).unwrap();
            again.forget_site(LANDING.to_owned()).unwrap();
            let none = Core::with_storage(backing.open())
                .unwrap()
                .kept_sites(None)
                .unwrap();
            assert!(none.sites.is_empty() && none.credits.is_empty());
        });
    }

    #[test]
    fn keeping_a_site_again_takes_its_record_anew() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let map = recorded();
            keep(&core, &map, RAMP);

            // Somebody gave the ramp a name and its directions.
            let mut newer = site(&map, RAMP);
            newer.name = "Wallberg Rampe".to_owned();
            newer.is_named = true;
            newer.wind = vec![crate::WindSpan {
                from_deg: 315.0,
                to_deg: 45.0,
            }];
            core.keep_site(newer, Vec::new(), Some(1_800_000_000))
                .unwrap();

            let kept = core.kept_sites(None).unwrap();
            assert_eq!(kept.sites.len(), 1);
            assert_eq!(kept.sites[0].site.name, "Wallberg Rampe");
            assert_eq!(kept.sites[0].site.wind_in_words, "NW to NE");
            assert_eq!(kept.sites[0].as_of_unix_s, Some(1_800_000_000));
            // An answer without the credit takes none away.
            assert_eq!(kept.credits, map.credits);
        });
    }

    /// OpenStreetMap's layer and Venti's own in one answer, made up: a
    /// take-off of each.
    fn with_pilots(revision: u32, changed_by: &str) -> SitesMap {
        let answer = format!(
            r#"{{"layers": [
                 {{"source": "osm-free-flying", "fetched_at": "2026-11-20T06:00:00Z",
                   "as_of": "2026-11-20T05:00:00Z", "covers": [],
                   "sites": [{{"id": "osm-free-flying:node/1",
                              "position": {{"lat": 47.66, "lon": 11.79}}, "uses": ["takeoff"]}}]}},
                 {{"source": "venti", "fetched_at": "2026-11-21T09:00:00Z",
                   "as_of": "2026-11-21T09:00:00Z", "covers": [],
                   "sites": [{{"id": "venti:00000000000000000000000041", "name": "Musterberg Ost",
                              "position": {{"lat": 47.6642, "lon": 11.7967}}, "uses": ["takeoff"]}}],
                   "standing": [{{"id": "venti:00000000000000000000000041", "revision": {revision},
                     "created_at": "2026-11-03T10:00:00Z",
                     "created_by": {{"handle": "demo", "display_name": "Max Mustermann"}},
                     "changed_at": "2026-11-12T17:30:00Z",
                     "changed_by": {{"handle": "{changed_by}", "display_name": "Mara Beispiel"}}}}]}}],
               "attributions": [
                 {{"source": "osm-free-flying", "provider": "OpenStreetMap contributors",
                   "licence": "ODbL 1.0", "text": "© OpenStreetMap contributors",
                   "url": "https://www.openstreetmap.org/copyright"}},
                 {{"source": "venti", "provider": "Venti contributors", "licence": "CC0 1.0",
                   "text": "Venti contributors",
                   "url": "https://creativecommons.org/publicdomain/zero/1.0/"}}]}}"#
        );
        sites_map(40.0, 5.0, 50.0, 18.0, 200, answer.into_bytes()).unwrap()
    }

    #[test]
    fn a_kept_site_that_pilots_made_keeps_who_made_it_in_a_file_of_its_own_layer() {
        const OWN: &str = "venti:00000000000000000000000041";
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let map = with_pilots(2, "mara");
            keep(&core, &map, OWN);
            keep(&core, &map, "osm-free-flying:node/1");

            // Read back without a network it is what the answer said, who
            // added it and that nobody verified it included.
            let kept = core.kept_sites(None).unwrap();
            let own = kept
                .sites
                .iter()
                .find(|kept| kept.site.id == OWN)
                .map(|kept| &kept.site);
            assert_eq!(own, Some(&site(&map, OWN)));
            let by = own.and_then(|site| site.by_pilots.as_ref()).unwrap();
            assert_eq!(by.added_in_words, "Added by Max Mustermann (@demo)");
            assert_eq!(by.changed_in_words, "Last changed by Mara Beispiel (@mara)");
            assert!(by.note.contains("not checked"), "{}", by.note);
            // The two layers are two files, each with its own credit.
            let licences: Vec<&str> = kept
                .credits
                .iter()
                .map(|credit| credit.licence.as_str())
                .collect();
            assert_eq!(licences, ["ODbL 1.0", "CC0 1.0"]);
            let mut files = backing.open().list(FOLDER).unwrap();
            files.sort();
            assert_eq!(files.len(), 2, "{files:?}");
            // OpenStreetMap's has no such part, kept or not.
            assert!(
                kept.sites
                    .iter()
                    .any(|kept| kept.site.source == "osm-free-flying"
                        && kept.site.by_pilots.is_none())
            );

            // An answer that holds the site anew brings its newer standing.
            let newer = with_pilots(3, "jonas");
            let today = core
                .today_takeoffs(Some(WALLBERG), Some(newer.clone()))
                .unwrap();
            let listed = today
                .takeoffs
                .iter()
                .find(|takeoff| takeoff.site.id == OWN)
                .unwrap();
            assert!(listed.is_kept);
            let kept = core.kept_sites(None).unwrap();
            let own = kept.sites.iter().find(|kept| kept.site.id == OWN).unwrap();
            let by = own.site.by_pilots.as_ref().unwrap();
            assert_eq!(by.revision, 3);
            assert_eq!(by.changed_by.as_ref().unwrap().handle, "jonas");
        });
    }

    #[test]
    fn a_site_that_is_no_record_is_refused_in_words() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let map = recorded();
            for spoil in [
                (|site: &mut SiteView| site.id = "no identifier".to_owned()) as fn(&mut SiteView),
                |site| site.position.lat = 120.0,
                |site| site.position.lon = f64::NAN,
            ] {
                let mut spoilt = site(&map, RAMP);
                spoil(&mut spoilt);
                let error = core.keep_site(spoilt, Vec::new(), None).unwrap_err();
                assert!(matches!(error, CoreError::Storage { .. }), "{error:?}");
                assert!(error.to_string().contains("cannot be kept"), "{error}");
            }
            assert!(core.kept_sites(None).unwrap().sites.is_empty());

            // What is no number in a record that can be read is left out
            // of it, and the site is kept.
            let mut odd = site(&map, JAEGERKAMP);
            odd.elevation_m = Some(f64::INFINITY);
            odd.wind = vec![crate::WindSpan {
                from_deg: f64::NAN,
                to_deg: 90.0,
            }];
            core.keep_site(odd, Vec::new(), None).unwrap();
            let kept = core.kept_sites(None).unwrap();
            assert_eq!(kept.sites[0].site.elevation_m, None);
            assert!(kept.sites[0].site.wind.is_empty());
        });
    }

    #[test]
    fn the_sites_of_each_source_are_kept_in_a_file_of_their_own() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let files = || {
                let mut files = core.storage.list(FOLDER).unwrap();
                files.sort();
                files
            };
            let map = recorded();
            keep(&core, &map, RAMP);
            // A site of another source, as a second source's layer would
            // have it, with that source's credit.
            let mut elsewhere = site(&map, JAEGERKAMP);
            elsewhere.id = "another-source:42".to_owned();
            elsewhere.source = "another-source".to_owned();
            let other_credit = Credit {
                source: "another-source".to_owned(),
                provider: "Another provider".to_owned(),
                notice: None,
                text: "© Another provider".to_owned(),
                licence: "CC BY-SA 3.0".to_owned(),
                url: "https://example.org/licence".to_owned(),
            };
            // Handed every credit of an answer, the core keeps the one of
            // the site's source with it.
            let both = vec![map.credits[0].clone(), other_credit.clone()];
            core.keep_site(elsewhere, both, None).unwrap();

            assert_eq!(
                files(),
                [
                    "sites-kept/another-source.json",
                    "sites-kept/osm-free-flying.json"
                ]
            );
            let kept = core.kept_sites(None).unwrap();
            assert_eq!(kept.sites.len(), 2);
            // One credit for each source, and each site names its own.
            assert_eq!(kept.credits, [other_credit, map.credits[0].clone()]);
            for site in &kept.sites {
                assert!(
                    kept.credits
                        .iter()
                        .any(|credit| credit.source == site.site.source)
                );
            }

            // With its last site gone, a source's file and its credit go
            // too.
            core.forget_site("another-source:42".to_owned()).unwrap();
            assert_eq!(files(), ["sites-kept/osm-free-flying.json"]);
            assert_eq!(core.kept_sites(None).unwrap().credits, map.credits);
        });
    }

    #[test]
    fn storage_that_fetches_is_asked_for_the_kept_sites_before_anything_is_said_of_them() {
        // A browser's storage hands a file over only after it was asked
        // for it. Kept there and stored, nothing of the sites is in memory.
        let staged = Arc::new(crate::StagedStorage::new([]));
        let core = Core::with_storage(Arc::clone(&staged) as Arc<dyn crate::Storage>).unwrap();
        keep(&core, &recorded(), JAEGERKAMP);
        let store: std::collections::BTreeMap<String, Vec<u8>> = staged
            .take_staged()
            .into_iter()
            .filter_map(|file| Some((file.name, file.contents?)))
            .collect();
        staged.release();

        // "No sites kept" is not said of a file that was not looked at:
        // on the list, on a site's page and on Today alike.
        for call in [
            (|core: &Core| core.kept_sites(None).map(|kept| kept.sites.len()))
                as fn(&Core) -> Result<usize, CoreError>,
            |core| core.is_site_kept(JAEGERKAMP.to_owned()).map(usize::from),
            |core| {
                core.today_takeoffs(None, None)
                    .map(|today| today.takeoffs.len())
            },
        ] {
            let error = call(&core).unwrap_err();
            assert!(matches!(error, CoreError::Storage { .. }), "{error:?}");
            let wanted = staged.take_wanted();
            assert_eq!(wanted, ["sites-kept/osm-free-flying.json"]);
            staged.provide(&wanted[0], store.get(&wanted[0]).cloned());
            assert_eq!(call(&core).unwrap(), 1);
            staged.release();
        }
    }

    #[test]
    fn today_lists_the_pilots_own_take_offs_first_however_far_away() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let around = made_up(
                &[
                    takeoff(1, Some("Near"), 2.0, ""),
                    takeoff(2, None, 1.0, ""),
                    takeoff(3, Some("Far"), 29.0, ""),
                    takeoff(4, Some("Too far"), 31.0, ""),
                    takeoff(5, Some("Marked unsafe"), 3.0, r#", "discouraged": true"#),
                    takeoff(6, Some("Mine, near"), 5.0, ""),
                    r#"{"id": "osm-free-flying:node/7", "name": "Meadow",
                        "position": {"lat": 47.67, "lon": 11.79}, "uses": ["landing"]}"#
                        .to_owned(),
                ]
                .join(","),
            );

            // Without sites of the pilot's own it is the rule the apps
            // had: take-offs within 30 km that are not marked as
            // discouraged, those with a name first, then the nearest.
            let plain = core
                .today_takeoffs(Some(WALLBERG), Some(around.clone()))
                .unwrap();
            assert_eq!(
                ids(&plain),
                [("1", false), ("6", false), ("3", false), ("2", false)]
            );
            assert_eq!(plain.near_title, "Within 30 km");
            assert_eq!(plain.kept_title, "Your take-offs");
            assert_eq!(plain.kept_more, "");
            assert_eq!(plain.credits, around.credits);
            assert!((plain.takeoffs[0].distance_m.unwrap() - 2_000.0).abs() < 5.0);

            // The pilot keeps one of them, one two hundred kilometres
            // away, one that is marked as discouraged, and a landing.
            let elsewhere = made_up(&takeoff(8, Some("Mine, far"), 200.0, ""));
            keep(&core, &around, "osm-free-flying:node/6");
            keep(&core, &elsewhere, "osm-free-flying:node/8");
            keep(&core, &around, "osm-free-flying:node/5");
            keep(&core, &around, "osm-free-flying:node/7");

            let today = core
                .today_takeoffs(Some(WALLBERG), Some(around.clone()))
                .unwrap();
            // The pilot's own first, the nearest first, also the one far
            // beyond 30 km and the one the source discourages: the pilot
            // chose it, and its page says what the source says. The
            // landing is no take-off. None is listed twice.
            assert_eq!(
                ids(&today),
                [
                    ("5", true),
                    ("6", true),
                    ("8", true),
                    ("1", false),
                    ("3", false),
                    ("2", false)
                ]
            );
            assert!((today.takeoffs[2].distance_m.unwrap() - 200_000.0).abs() < 50.0);
            assert_eq!(today.credits.len(), 1);

            // Without an answer, the pilot's own alone: no network is
            // needed for them, and they carry the credit kept with them.
            let offline = core.today_takeoffs(Some(WALLBERG), None).unwrap();
            assert_eq!(ids(&offline), [("5", true), ("6", true), ("8", true)]);
            assert_eq!(offline.credits[0].text, "© OpenStreetMap contributors");

            // Without a place, by name, and nobody's distance is known:
            // then none is "around".
            let nowhere = core.today_takeoffs(None, Some(around)).unwrap();
            let names: Vec<&str> = nowhere
                .takeoffs
                .iter()
                .map(|takeoff| takeoff.site.name.as_str())
                .collect();
            assert_eq!(names, ["Marked unsafe", "Mine, far", "Mine, near"]);
            assert!(
                nowhere
                    .takeoffs
                    .iter()
                    .all(|takeoff| takeoff.distance_m.is_none())
            );
        });
    }

    #[test]
    fn today_puts_no_place_from_pilots_forward_unless_the_pilot_kept_it() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            // An answer that was asked for the crowdsourced layer: a
            // source's take-off and two that pilots added.
            let pilots = |number: u32, name: &str, north_km: f64| {
                format!(
                    r#"{{"id": "venti-pilots:000000000000000000000000{number}", "name": "{name}",
                        "position": {{"lat": {}, "lon": 11.7967}}, "uses": ["takeoff"]}}"#,
                    47.6642 + north_km / 111.195
                )
            };
            let standing = |number: u32| {
                format!(
                    r#"{{"id": "venti-pilots:000000000000000000000000{number}", "revision": 1,
                        "created_at": "2026-11-06T19:05:00Z", "changed_at": "2026-11-06T19:05:00Z"}}"#
                )
            };
            let answer = format!(
                r#"{{"layers": [
                     {{"source": "osm-free-flying", "fetched_at": "2026-11-20T06:00:00Z",
                       "as_of": "2026-11-20T05:00:00Z", "covers": [], "sites": [{}]}},
                     {{"source": "venti-pilots", "fetched_at": "2026-11-20T06:00:00Z",
                       "as_of": "2026-11-20T06:00:00Z", "covers": [], "sites": [{}, {}],
                       "standing": [{}, {}]}}],
                   "attributions": []}}"#,
                takeoff(1, Some("Official enough"), 4.0, ""),
                pilots(21, "Pilots' nearest", 1.0),
                pilots(22, "Pilots' other", 2.0),
                standing(21),
                standing(22),
            );
            let around = sites_map(40.0, 5.0, 50.0, 18.0, 200, answer.into_bytes()).unwrap();
            assert_eq!(around.sites.len(), 3);

            // Nearer than the source's take-off, and still not listed.
            let plain = core
                .today_takeoffs(Some(WALLBERG), Some(around.clone()))
                .unwrap();
            assert_eq!(ids(&plain), [("1", false)]);

            // Kept as the pilot's own, one is listed, among the pilot's
            // own, and says what it is; the other stays out.
            let mine = around
                .sites
                .iter()
                .find(|site| site.name == "Pilots' other")
                .unwrap()
                .clone();
            core.keep_site(mine, around.credits.clone(), around.as_of_unix_s)
                .unwrap();
            let kept = core
                .today_takeoffs(Some(WALLBERG), Some(around.clone()))
                .unwrap();
            let listed: Vec<(&str, bool)> = kept
                .takeoffs
                .iter()
                .map(|takeoff| (takeoff.site.name.as_str(), takeoff.is_kept))
                .collect();
            assert_eq!(
                listed,
                [("Pilots' other", true), ("Official enough", false)]
            );
            let by = kept.takeoffs[0].site.by_pilots.as_ref().unwrap();
            assert!(by.from_pilots);
            assert_eq!(by.label, "From pilots");
            // And without an answer, from the device alone.
            let offline = core.today_takeoffs(Some(WALLBERG), None).unwrap();
            assert_eq!(offline.takeoffs.len(), 1);
            assert!(
                offline.takeoffs[0]
                    .site
                    .by_pilots
                    .as_ref()
                    .unwrap()
                    .from_pilots
            );
        });
    }

    #[test]
    fn today_takes_six_of_each_and_says_when_more_are_kept() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let sites: Vec<String> = (1..=16)
                .map(|number| {
                    takeoff(
                        number,
                        Some(&format!("Take-off {number:02}")),
                        f64::from(number),
                        "",
                    )
                })
                .collect();
            let around = made_up(&sites.join(","));
            // The eight farthest are the pilot's own.
            for number in 9..=16 {
                keep(&core, &around, &format!("osm-free-flying:node/{number}"));
            }

            let today = core
                .today_takeoffs(Some(WALLBERG), Some(around.clone()))
                .unwrap();
            let listed: Vec<String> = ids(&today)
                .into_iter()
                .map(|(number, _)| number.to_owned())
                .collect();
            assert_eq!(
                listed,
                [
                    "9", "10", "11", "12", "13", "14", "1", "2", "3", "4", "5", "6"
                ]
            );
            assert_eq!(
                today.kept_more,
                "2 more of your take-offs are not listed: Today takes the 6 nearest."
            );

            core.forget_site("osm-free-flying:node/16".to_owned())
                .unwrap();
            let today = core.today_takeoffs(Some(WALLBERG), Some(around)).unwrap();
            assert_eq!(
                today.kept_more,
                "1 more of your take-offs is not listed: Today takes the 6 nearest."
            );
            // Without a place nothing is near or far.
            let nowhere = core.today_takeoffs(None, None).unwrap();
            assert_eq!(nowhere.takeoffs.len(), 6);
            assert_eq!(nowhere.takeoffs[0].site.name, "Take-off 09");
            assert_eq!(
                nowhere.kept_more,
                "1 more of your take-offs is not listed: Today takes the first 6 by name."
            );
        });
    }

    #[test]
    fn today_refreshes_the_kept_sites_its_answer_holds() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            let old = made_up(
                &[
                    takeoff(1, None, 2.0, ""),
                    takeoff(2, Some("Far"), 200.0, ""),
                ]
                .join(","),
            );
            core.keep_site(
                site(&old, "osm-free-flying:node/1"),
                Vec::new(),
                Some(1_700_000_000),
            )
            .unwrap();
            core.keep_site(
                site(&old, "osm-free-flying:node/2"),
                Vec::new(),
                Some(1_700_000_000),
            )
            .unwrap();
            assert!(core.kept_sites(None).unwrap().credits.is_empty());

            // Since then somebody named the near one and gave it its
            // directions and its height. The far one is not in the answer.
            let newer = made_up(&takeoff(
                1,
                Some("Named since"),
                2.0,
                r#", "elevation_m": 1620.0, "wind_directions": [{"from_deg": 0.0, "to_deg": 45.0}]"#,
            ));
            let today = core
                .today_takeoffs(Some(WALLBERG), Some(newer.clone()))
                .unwrap();
            assert_eq!(today.takeoffs[0].site.name, "Named since");
            assert_eq!(today.takeoffs[0].site.elevation_m, Some(1620.0));
            assert_eq!(today.takeoffs[1].site.name, "Far");

            // The device has the newer record from then on, with the
            // answer's date and its credit; the far one is as it was.
            let again = Core::with_storage(backing.open()).unwrap();
            let kept = again.kept_sites(Some(WALLBERG)).unwrap();
            assert_eq!(kept.sites[0].site.name, "Named since");
            assert_eq!(kept.sites[0].site.wind_in_words, "N to NE");
            assert_eq!(kept.sites[0].as_of_unix_s, newer.as_of_unix_s);
            assert_eq!(kept.sites[1].site.name, "Far");
            assert_eq!(kept.sites[1].as_of_unix_s, Some(1_700_000_000));
            assert_eq!(kept.credits, newer.credits);
            assert_eq!(kept.credits[0].notice.as_deref(), Some("A newer notice."));
        });
    }

    #[test]
    fn the_box_today_asks_for_holds_everything_within_reach() {
        let area = today_area(WALLBERG.lat, WALLBERG.lon).unwrap();
        let here = GeoPoint::new(WALLBERG.lat, WALLBERG.lon).unwrap();
        // The middle of each edge is the point of it nearest to the place.
        for (lat, lon) in [
            (area.north, WALLBERG.lon),
            (area.south, WALLBERG.lon),
            (WALLBERG.lat, area.east),
            (WALLBERG.lat, area.west),
        ] {
            let edge = here.distance_m(GeoPoint::new(lat, lon).unwrap());
            assert!(
                (TODAY_REACH_M..TODAY_REACH_M * 1.02).contains(&edge),
                "{edge}"
            );
        }
        assert!(crate::sites_path(area.south, area.west, area.north, area.east).is_ok());
        let error = today_area(f64::NAN, 11.0).unwrap_err();
        assert!(error.to_string().contains("take-offs around"), "{error}");
    }

    #[test]
    fn todays_order_has_the_pilots_own_first_and_each_group_by_the_day() {
        let standing = |id: &str,
                        is_kept: bool,
                        km: f64,
                        verdict: Option<Flyability>,
                        thermals: Option<Thermals>| TodayStanding {
            id: id.to_owned(),
            is_kept,
            distance_m: Some(km * 1000.0),
            verdict,
            thermals,
        };
        use Flyability::{Flyable, Marginal, NotFlyable};
        let takeoffs = vec![
            standing("waiting", false, 1.0, None, None),
            standing("weak", false, 2.0, Some(Flyable), Some(Thermals::Weak)),
            standing("big", false, 20.0, Some(Flyable), Some(Thermals::Big)),
            standing(
                "marginal",
                false,
                3.0,
                Some(Marginal),
                Some(Thermals::Local),
            ),
            standing("no estimate", false, 4.0, Some(Flyable), None),
            standing("local", false, 9.0, Some(Flyable), Some(Thermals::Local)),
            standing(
                "cross-country",
                false,
                8.0,
                Some(Flyable),
                Some(Thermals::CrossCountry),
            ),
            standing("not flyable", false, 0.5, Some(NotFlyable), None),
            standing("mine, marginal", true, 150.0, Some(Marginal), None),
            standing(
                "mine, local",
                true,
                300.0,
                Some(Flyable),
                Some(Thermals::Local),
            ),
            standing("mine, waiting", true, 5.0, None, None),
        ];

        let order = today_order(takeoffs.clone());
        assert_eq!(
            order.ids,
            [
                // The pilot's own, by the day.
                "mine, local",
                "mine, marginal",
                "mine, waiting",
                // The others: a flyable day by its thermals, weak or
                // unknown ones by distance, then marginal, not flyable,
                // and the one still waiting for its forecast.
                "big",
                "cross-country",
                "local",
                "weak",
                "no estimate",
                "marginal",
                "not flyable",
                "waiting",
            ]
        );
        // The best is the best of all, the pilot's own or not.
        assert_eq!(order.best.as_deref(), Some("big"));

        // Of two that stand alike, the pilot's own is the best, though
        // it lies farther away.
        let alike = today_order(
            takeoffs
                .iter()
                .filter(|takeoff| takeoff.id.contains("local"))
                .cloned()
                .collect(),
        );
        assert_eq!(alike.ids, ["mine, local", "local"]);
        assert_eq!(alike.best.as_deref(), Some("mine, local"));

        // Until a forecast has arrived there is no best, and the order is
        // the pilot's own, then the nearer.
        let waiting = today_order(vec![
            standing("far", false, 9.0, None, None),
            standing("near", false, 1.0, None, None),
            standing("mine", true, 90.0, None, None),
        ]);
        assert_eq!(waiting.ids, ["mine", "near", "far"]);
        assert_eq!(waiting.best, None);
        assert_eq!(
            today_order(Vec::new()),
            TodayOrder {
                ids: Vec::new(),
                best: None
            }
        );
    }
}
