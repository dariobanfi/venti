//! Flying sites from the Venti server: take-offs, landings, winch sites
//! and training slopes, for the map.
//!
//! As with the wind, the core never talks to the network: one function
//! says what to ask the server for, another reads the answer into what a
//! screen shows. The shell does the request in between.
//!
//! The sites are what their source has, and the first source is
//! OpenStreetMap, which anyone can edit: a site on the map is not
//! permission to fly there, a site missing from it may well exist, and
//! most have no name and no wind directions. The views say what the source
//! says and nothing more.
//!
//! Beside the sources' layers the server has one of its own, `venti`: the
//! sites that pilots made on Venti. They are read by the same calls and
//! are the same view, with one thing more: who added the site and who
//! changed it last ([`SiteByPilots`]). They are added by pilots and not
//! verified by anyone, and the view says so.
//!
//! Beside this module: the filter of the sites, by use, wind and height
//! (`site_filter`), and the sites a pilot keeps as their own, with which
//! Today begins (`kept_sites`).

use venti_api_types::v1::{SitePilot, SiteStanding, SitesRequest, SitesResponse};
use venti_domain::{BoundingBox, GeoPoint, Site, SiteId, SiteUse, WindSector};
use venti_flight::live::navigation::bearing_deg;

use crate::server::answer;
use crate::wind::credit;
use crate::{Coordinate, CoreError, Credit, WindStation};

/// What a site is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum SiteKind {
    /// Taking off on foot.
    Takeoff,
    /// Landing.
    Landing,
    /// Taking off on a tow line, behind a winch.
    Towing,
    /// A slope or field to practise on.
    Training,
    /// The source does not say, or says something this version does not
    /// know.
    Other,
}

/// A span of wind directions a site works in, clockwise.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindSpan {
    /// Where the span starts: the direction the wind comes from, degrees
    /// clockwise from north.
    pub from_deg: f64,
    /// Where it ends, going clockwise. Equal to `from_deg` for a single
    /// direction.
    pub to_deg: f64,
}

/// How far to either side of a direction a site names the wind may come
/// from and still count as from that direction, degrees: half of one of
/// the sixteen points of the compass that sites are described with. A
/// site said to work in "W" works in what a pilot calls west wind.
const DIRECTION_TOLERANCE_DEG: f64 = 11.25;

/// Whether a wind comes from a direction a site works in, by the spans
/// the site's source gives. It says nothing about the wind's strength, the
/// gusts or the day: a direction that fits is no word that the site can be
/// flown. `None` when the site gives no directions.
#[uniffi::export]
pub fn wind_fits(spans: Vec<WindSpan>, from_deg: f64) -> Option<bool> {
    if spans.is_empty() || !from_deg.is_finite() {
        return None;
    }
    Some(spans.iter().any(|span| {
        // The span as its source gives it, widened at both ends, and
        // asked the way the domain asks a sector.
        let reach = span.to_deg - span.from_deg;
        let width = if reach >= 360.0 {
            360.0
        } else {
            reach.rem_euclid(360.0)
        };
        WindSector {
            from_deg: span.from_deg - DIRECTION_TOLERANCE_DEG,
            to_deg: span.from_deg + width + DIRECTION_TOLERANCE_DEG,
        }
        .contains(from_deg)
    }))
}

/// A wind station near a place.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationNearby {
    /// The station.
    pub station: WindStation,
    /// How far it is from the place, metres.
    pub distance_m: f64,
    /// The direction from the place to the station, degrees clockwise
    /// from north.
    pub bearing_deg: f64,
    /// How much higher than the place the station stands, metres;
    /// negative when lower. Absent when either height is not known.
    pub higher_by_m: Option<f64>,
    /// Whether its newest wind comes from a direction the place works
    /// in. Absent when the place gives no directions, or the station has
    /// no direction that is fresh enough to stand for the wind now.
    pub fits: Option<bool>,
}

/// The wind stations nearest to a place, the nearest first: for a flying
/// site, the wind measured around it. `wind` is the directions the place
/// works in, if it is a site that gives them.
///
/// A station near a site is not the site: it may stand on a summit, in
/// the valley or behind a ridge, and its wind is its own.
#[uniffi::export]
pub fn stations_near(
    stations: Vec<WindStation>,
    place: Coordinate,
    elevation_m: Option<f64>,
    wind: Vec<WindSpan>,
    within_m: f64,
    at_most: u32,
) -> Vec<StationNearby> {
    let Ok(here) = GeoPoint::new(place.lat, place.lon) else {
        return Vec::new();
    };
    let mut near: Vec<StationNearby> = stations
        .into_iter()
        .filter_map(|station| {
            let there = GeoPoint::new(station.position.lat, station.position.lon).ok()?;
            let distance_m = here.distance_m(there);
            if distance_m > within_m {
                return None;
            }
            let fits = station
                .latest
                .as_ref()
                .filter(|_| !station.is_old)
                .and_then(|reading| reading.direction_deg)
                .and_then(|from_deg| wind_fits(wind.clone(), from_deg));
            Some(StationNearby {
                distance_m,
                bearing_deg: bearing_deg(here, there),
                higher_by_m: station
                    .elevation_m
                    .zip(elevation_m)
                    .map(|(station, place)| station - place),
                fits,
                station,
            })
        })
        .collect();
    near.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
    near.truncate(at_most as usize);
    near
}

/// A pilot as a site that pilots made names them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteAuthor {
    /// The pilot's handle, without an `@`.
    pub handle: String,
    /// The name the pilot is shown by.
    pub name: String,
}

/// What a site of Venti's own layer has that a source's site has not: it
/// was added by a pilot on Venti, anyone signed in may correct it, and
/// every change is kept. Nobody has verified it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteByPilots {
    /// The number of the revision the site stands at; 1 for a site nobody
    /// has changed since it was added. A correction names it.
    pub revision: u32,
    /// When the site was added, seconds since 1970-01-01 UTC.
    pub added_unix_s: i64,
    /// Who added it. Absent once that pilot's account is gone.
    pub added_by: Option<SiteAuthor>,
    /// When it last changed, seconds since 1970-01-01 UTC: when it was
    /// added, for a site at its first revision.
    pub changed_unix_s: i64,
    /// Who changed it last. Absent when whoever runs Venti did, and once
    /// that pilot's account is gone.
    pub changed_by: Option<SiteAuthor>,
    /// What to say wherever the site is shown as more than a mark: "Added
    /// by pilots on Venti and not verified. A site on the map is not
    /// permission to fly there."
    pub note: String,
    /// Who added it, as a line of the site's page: "Added by Max
    /// Mustermann (@demo)". The date beside it is `added_unix_s`.
    pub added_in_words: String,
    /// Who changed it last, as a line of the site's page: "Last changed
    /// by Mara Beispiel (@mara)". The date beside it is `changed_unix_s`.
    /// Empty for a site nobody has changed since it was added.
    pub changed_in_words: String,
}

/// One flying site as a map and a sheet show it. A shell hands it back to
/// have sites filtered ([`crate::filter_sites`]) and to keep one as the
/// pilot's own ([`crate::Core::keep_site`]).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteView {
    /// Its identifier.
    pub id: String,
    /// Its name as the source gives it, or what it is, in words, when the
    /// source has no name for it: "Take-off".
    pub name: String,
    /// Whether `name` is the source's.
    pub is_named: bool,
    /// What it is drawn as: its first use.
    pub kind: SiteKind,
    /// Everything it is used for.
    pub uses: Vec<SiteKind>,
    /// What it is, in words: "Take-off", "Take-off and landing".
    pub kind_in_words: String,
    /// Where it is.
    pub position: Coordinate,
    /// Its height above sea level in metres, if the source gives one.
    pub elevation_m: Option<f64>,
    /// The wind directions it works in, as the source gives them. Empty
    /// when it gives none, which says nothing about the site.
    pub wind: Vec<WindSpan>,
    /// The same in words: "S to SW", "N, NE". Empty when none are given.
    pub wind_in_words: String,
    /// Whether the source calls it official: legal and looked after.
    pub official: Option<bool>,
    /// Whether the source marks it as unsafe or discouraged.
    pub discouraged: bool,
    /// Whether the source says it suits paragliders.
    pub paragliding: Option<bool>,
    /// Whether the source says it suits hang gliders.
    pub hang_gliding: Option<bool>,
    /// The data source it comes from; matches a [`Credit`]'s `source`.
    pub source: String,
    /// Where the source's record can be looked at and corrected.
    pub url: Option<String>,
    /// A web page about the site that its source names: its operator's,
    /// or one that says more. An `http` or `https` address, for a link
    /// that opens in the browser. It is somebody else's page.
    pub website: Option<String>,
    /// What the source says about the site in words, as its author wrote
    /// it and in the author's language; it may have several lines. Shown
    /// as written, as a quotation of the source.
    pub description: Option<String>,
    /// Whether a payment is asked for using the site, as the source
    /// writes it: "yes", "no", or other words of its author's.
    pub fee: Option<String>,
    /// The same as a line of the site's page: "A fee is asked for using
    /// this site.", "No fee is asked for using this site.", or "Fee:" with
    /// the source's words. Empty when the source does not say.
    pub fee_in_words: String,
    /// Whether guests have rules for guests to take notice of.
    pub guest_guidelines: Option<bool>,
    /// The same as a line of the site's page: "Guests have to read this
    /// site's rules for guests before they fly here." Empty when the
    /// source does not say.
    pub guests_in_words: String,
    /// A time in which the site may not be used, as the source writes it:
    /// "01.04-30.09". Never reckoned with: the app does not say from it
    /// that the site is open or closed today.
    pub no_flight_time: Option<String>,
    /// The same as a line of the site's page: "Not to be used in this
    /// time, as its source writes it: 01.04-30.09". Empty when the source
    /// names no such time, which does not say that there is none.
    pub no_flight_in_words: String,
    /// The outline of a site its source draws as an area, closed: the
    /// last point is the first again, 4 to 64 points. Empty for a site
    /// drawn as a point; `position` is then all there is, and with an
    /// outline it is the middle of the box around it.
    pub outline: Vec<Coordinate>,
    /// For a site of Venti's own layer (`source` is `venti`): who added
    /// it and who changed it last, with what to say about such a site.
    /// Absent for a site of a source the server collects, and a record a
    /// shell builds without it is such a site.
    #[uniffi(default = None)]
    #[cfg_attr(feature = "wasm", serde(default), tsify(optional))]
    pub by_pilots: Option<SiteByPilots>,
}

/// The sites in a part of the map. A shell hands it back for Today's
/// take-offs ([`crate::Core::today_takeoffs`]).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesMap {
    /// The sites, from every source, each source's own.
    pub sites: Vec<SiteView>,
    /// Whose data they are, with what the licences ask to be shown.
    pub credits: Vec<Credit>,
    /// The date of the oldest source's data, seconds since 1970-01-01 UTC.
    pub as_of_unix_s: Option<i64>,
    /// Whether the server says that it holds every site its sources have
    /// for the whole area asked for. `false` where it holds none or only
    /// part: then an empty map says nothing.
    pub is_whole: bool,
}

/// The path and query to `GET` from the server for the flying sites of a
/// box.
#[uniffi::export]
pub fn sites_path(south: f64, west: f64, north: f64, east: f64) -> Result<String, CoreError> {
    Ok(SitesRequest {
        bbox: area(south, west, north, east)?,
    }
    .path_and_query())
}

/// Reads the server's answer to [`sites_path`] for the same box.
#[uniffi::export]
pub fn sites_map(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
    status: u16,
    body: Vec<u8>,
) -> Result<SitesMap, CoreError> {
    let asked = area(south, west, north, east)?;
    let response: SitesResponse = answer(status, &body)?;
    Ok(sites_of(&asked, &response))
}

/// What an answer of the server holds for the box that was asked for.
fn sites_of(asked: &BoundingBox, response: &SitesResponse) -> SitesMap {
    SitesMap {
        sites: response
            .layers
            .iter()
            .flat_map(|layer| {
                // Where a layer says where its sites stand, it says so by
                // identifier: a layer of a source's whole set says nothing.
                layer.sites.iter().map(|site| {
                    let standing = layer.standing.iter().find(|one| one.id == site.id);
                    site_view(site, standing)
                })
            })
            .collect(),
        credits: response.attributions.iter().map(credit).collect(),
        as_of_unix_s: response
            .layers
            .iter()
            .map(|layer| layer.as_of.timestamp())
            .min(),
        // Whole when some layer's set is whole for all of the box: its
        // corners lie in one of the boxes the layer names.
        is_whole: response.layers.iter().any(|layer| {
            layer.covers.iter().any(|cover| {
                cover.south() <= asked.south()
                    && cover.west() <= asked.west()
                    && cover.north() >= asked.north()
                    && cover.east() >= asked.east()
            })
        }),
    }
}

/// How far from a place a take-off may lie to be the site of that place,
/// when no site is named: a logbook names a flight after a waypoint that
/// is a take-off as far away.
const TAKEOFF_WITHIN_M: f64 = 1000.0;

/// The flying site at a place, as a page of its own shows it, with whose
/// data it is and of when. It is one answer of the server's, about sites
/// alone: what was flown from the place is asked for apart, and nothing of
/// the one is put into the other.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteAtPlace {
    /// The site, if the server knows one there.
    pub site: Option<SiteView>,
    /// Whose data the site is, with what the licence asks to be shown.
    pub credit: Option<Credit>,
    /// The date of the source's data, seconds since 1970-01-01 UTC.
    pub as_of_unix_s: Option<i64>,
}

/// The box the server is asked for the sites around a place, a kilometre
/// to each side.
fn around_place(lat: f64, lon: f64) -> Result<BoundingBox, CoreError> {
    let unreadable = || CoreError::Server {
        message: "The app could not ask the server about the sites at this place.".to_owned(),
    };
    let around = crate::area_around(lat, lon, TAKEOFF_WITHIN_M).map_err(|_| unreadable())?;
    area(around.south, around.west, around.north, around.east).map_err(|_| unreadable())
}

/// The path and query to `GET` from the server for the sites around a
/// place, to find the site of that place.
#[uniffi::export]
pub fn site_at_path(lat: f64, lon: f64) -> Result<String, CoreError> {
    Ok(SitesRequest {
        bbox: around_place(lat, lon)?,
    }
    .path_and_query())
}

/// Reads the server's answer to [`site_at_path`] for the same place: the
/// site of identifier `id`, or without one the take-off nearest to the
/// place within a kilometre. No site is said as none: most places pilots
/// fly from are not in the source.
#[uniffi::export]
pub fn site_at(
    lat: f64,
    lon: f64,
    id: Option<String>,
    status: u16,
    body: Vec<u8>,
) -> Result<SiteAtPlace, CoreError> {
    let asked = around_place(lat, lon)?;
    let response: SitesResponse = answer(status, &body)?;
    let map = sites_of(&asked, &response);
    let here = GeoPoint::new(lat, lon).map_err(|_| CoreError::Server {
        message: "The app could not ask the server about the sites at this place.".to_owned(),
    })?;
    let away = |site: &SiteView| {
        GeoPoint::new(site.position.lat, site.position.lon)
            .map_or(f64::INFINITY, |there| here.distance_m(there))
    };
    let site = match id.filter(|id| !id.is_empty()) {
        Some(id) => map.sites.into_iter().find(|site| site.id == id),
        None => map
            .sites
            .into_iter()
            .filter(|site| site.uses.contains(&SiteKind::Takeoff) && away(site) <= TAKEOFF_WITHIN_M)
            .min_by(|one, two| away(one).total_cmp(&away(two))),
    };
    let credit = site.as_ref().and_then(|site| {
        map.credits
            .iter()
            .find(|credit| credit.source == site.source)
            .cloned()
    });
    // The date is the one of the site's own layer: another source's data
    // may be older or newer, and says nothing about this site.
    let as_of_unix_s = site.as_ref().and_then(|site| {
        response
            .layers
            .iter()
            .find(|layer| layer.source.as_str() == site.source)
            .map(|layer| layer.as_of.timestamp())
    });
    Ok(SiteAtPlace {
        as_of_unix_s,
        site,
        credit,
    })
}

fn area(south: f64, west: f64, north: f64, east: f64) -> Result<BoundingBox, CoreError> {
    // A map can be dragged past the poles and the date line; ask for what
    // exists.
    BoundingBox::new(
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    )
    .map_err(|_| CoreError::Server {
        message: "The app could not ask the server about this part of the map.".to_owned(),
    })
}

/// What to say wherever a site of Venti's own layer is shown as more than
/// a mark.
const BY_PILOTS_NOTE: &str = "Added by pilots on Venti and not verified. A site on the map is \
                              not permission to fly there.";

fn author(pilot: &SitePilot) -> SiteAuthor {
    SiteAuthor {
        handle: pilot.handle.as_str().to_owned(),
        name: pilot.display_name.clone(),
    }
}

/// "Max Mustermann (@demo)"; the handle alone for a pilot without a name.
fn author_in_words(author: &SiteAuthor) -> String {
    match author.name.trim() {
        "" => format!("@{}", author.handle),
        name => format!("{name} (@{})", author.handle),
    }
}

/// Where a site of Venti's own layer stands, as its page shows it.
fn by_pilots(standing: &SiteStanding) -> SiteByPilots {
    let added_by = standing.created_by.as_ref().map(author);
    let changed_by = standing.changed_by.as_ref().map(author);
    SiteByPilots {
        revision: standing.revision,
        added_unix_s: standing.created_at.timestamp(),
        changed_unix_s: standing.changed_at.timestamp(),
        note: BY_PILOTS_NOTE.to_owned(),
        added_in_words: added_by.as_ref().map_or_else(
            || "Added by a pilot who is no longer on Venti".to_owned(),
            |author| format!("Added by {}", author_in_words(author)),
        ),
        changed_in_words: match (&changed_by, standing.revision) {
            (_, 0 | 1) => String::new(),
            (Some(author), _) => format!("Last changed by {}", author_in_words(author)),
            // Whoever runs Venti changes a site without an account, and a
            // pilot's name goes with the account: the server tells the
            // two apart only in the site's history.
            (None, _) => "Last changed by whoever runs Venti, or by a pilot who is no longer on \
                          Venti"
                .to_owned(),
        },
        added_by,
        changed_by,
    }
}

/// Where a site stands, as the server said it, from what a view says of
/// it: for keeping a site of Venti's own layer with who made it. `None`
/// for a view of a source's site, and for one whose identifier is none.
pub(crate) fn standing_of(view: &SiteView) -> Option<SiteStanding> {
    let by = view.by_pilots.as_ref()?;
    let id: SiteId = view.id.parse().ok()?;
    let pilot = |author: &Option<SiteAuthor>| {
        let author = author.as_ref()?;
        Some(SitePilot {
            handle: venti_api_types::v1::Handle::new(&author.handle).ok()?,
            display_name: author.name.clone(),
        })
    };
    Some(SiteStanding {
        id,
        revision: by.revision,
        created_at: chrono::DateTime::from_timestamp(by.added_unix_s, 0)?,
        created_by: pilot(&by.added_by),
        changed_at: chrono::DateTime::from_timestamp(by.changed_unix_s, 0)?,
        changed_by: pilot(&by.changed_by),
    })
}

/// A site of the canonical record as a map and a sheet show it. `standing`
/// is what the server says beside a site that pilots made; a source's site
/// has none.
pub(crate) fn site_view(site: &Site, standing: Option<&SiteStanding>) -> SiteView {
    let uses: Vec<SiteKind> = site.uses.iter().copied().map(kind).collect();
    let kind_in_words = uses_in_words(&uses);
    let name = site
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    SiteView {
        id: site.id.to_string(),
        name: name.map_or_else(|| kind_in_words.clone(), str::to_owned),
        is_named: name.is_some(),
        kind: uses.first().copied().unwrap_or(SiteKind::Other),
        uses,
        kind_in_words,
        position: Coordinate {
            lat: site.position.lat(),
            lon: site.position.lon(),
        },
        elevation_m: site.elevation_m,
        wind: site
            .wind_directions
            .iter()
            .map(|sector| WindSpan {
                from_deg: sector.from_deg,
                to_deg: sector.to_deg,
            })
            .collect(),
        wind_in_words: wind_in_words(&site.wind_directions),
        official: site.official,
        discouraged: site.discouraged,
        paragliding: site.paragliding,
        hang_gliding: site.hang_gliding,
        source: site.id.source().to_string(),
        url: site.url.clone(),
        website: site.website.clone(),
        description: site.description.clone(),
        fee_in_words: site.fee.as_deref().map_or_else(String::new, fee_in_words),
        fee: site.fee.clone(),
        guest_guidelines: site.guest_guidelines,
        guests_in_words: match site.guest_guidelines {
            Some(true) => "Guests have to read this site's rules for guests before they fly here.",
            Some(false) => "Its source names no rules for guests.",
            None => "",
        }
        .to_owned(),
        no_flight_in_words: site
            .no_flight_time
            .as_deref()
            .map_or_else(String::new, |time| {
                format!("Not to be used in this time, as its source writes it: {time}")
            }),
        no_flight_time: site.no_flight_time.clone(),
        outline: site
            .outline
            .iter()
            .flat_map(|outline| outline.points())
            .map(|point| Coordinate {
                lat: point.lat(),
                lon: point.lon(),
            })
            .collect(),
        by_pilots: standing.map(by_pilots),
    }
}

/// What a source says of a fee, as a line: its `yes` and `no` in a
/// sentence, anything else as its author wrote it.
fn fee_in_words(fee: &str) -> String {
    match fee {
        "yes" => "A fee is asked for using this site.".to_owned(),
        "no" => "No fee is asked for using this site.".to_owned(),
        other => format!("Fee: {other}"),
    }
}

fn kind(used_for: SiteUse) -> SiteKind {
    match used_for {
        SiteUse::Takeoff => SiteKind::Takeoff,
        SiteUse::Landing => SiteKind::Landing,
        SiteUse::Towing => SiteKind::Towing,
        SiteUse::Training => SiteKind::Training,
        SiteUse::Other => SiteKind::Other,
    }
}

/// "Take-off", "Take-off and landing", "Take-off, landing and training
/// slope"; "Flying site" when the source does not say.
fn uses_in_words(uses: &[SiteKind]) -> String {
    let words: Vec<&str> = uses
        .iter()
        .filter_map(|kind| match kind {
            SiteKind::Takeoff => Some("take-off"),
            SiteKind::Landing => Some("landing"),
            SiteKind::Towing => Some("winch site"),
            SiteKind::Training => Some("training slope"),
            SiteKind::Other => None,
        })
        .collect();
    let joined = match words.as_slice() {
        [] => return "Flying site".to_owned(),
        [one] => (*one).to_owned(),
        [most @ .., last] => format!("{} and {last}", most.join(", ")),
    };
    let mut letters = joined.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// "S to SW", "N, NE": each span by its ends, a single direction by its
/// name. Nothing is widened or merged: it is what the source says.
fn wind_in_words(sectors: &[WindSector]) -> String {
    sectors
        .iter()
        .map(|sector| {
            let (from, to) = (compass(sector.from_deg), compass(sector.to_deg));
            if sector.to_deg - sector.from_deg >= 360.0 {
                "any direction".to_owned()
            } else if from == to {
                from.to_owned()
            } else {
                format!("{from} to {to}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The same for the spans a shell hands back: "N to NE".
pub(crate) fn wind_spans_in_words(spans: &[WindSpan]) -> String {
    let sectors: Vec<WindSector> = spans
        .iter()
        .map(|span| WindSector {
            from_deg: span.from_deg,
            to_deg: span.to_deg,
        })
        .collect();
    wind_in_words(&sectors)
}

/// One of sixteen compass points: sites are tagged that finely.
pub(crate) fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    POINTS[((degrees.rem_euclid(360.0) + 11.25) / 22.5) as usize % 16]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answer the server's own tests pin: five sites around the
    /// Tegernsee, as OpenStreetMap has them.
    fn recorded() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../server/tests/golden/sites.json"
        );
        std::fs::read(path).unwrap()
    }

    #[test]
    fn the_sites_of_a_box_are_asked_for() {
        assert_eq!(
            sites_path(47.55, 11.6, 47.85, 12.0).unwrap(),
            "/v1/sites?bbox=11.6,47.55,12,47.85"
        );
        // A map dragged past the edge of the world still asks a valid question.
        assert_eq!(
            sites_path(80.0, 170.0, 95.0, 190.0).unwrap(),
            "/v1/sites?bbox=170,80,180,90"
        );
        assert!(sites_path(48.0, 11.0, 47.0, 12.0).is_err());
    }

    #[test]
    fn the_servers_recorded_sites_are_read_as_the_source_gives_them() {
        let map = sites_map(47.55, 11.6, 47.85, 12.0, 200, recorded()).unwrap();
        assert_eq!(map.sites.len(), 5);
        assert!(map.is_whole);
        assert!(map.as_of_unix_s.is_some());
        let credit = &map.credits[0];
        assert_eq!(credit.text, "© OpenStreetMap contributors");
        assert!(
            credit
                .notice
                .as_deref()
                .unwrap()
                .contains("Anyone can correct it")
        );

        // Most have no name: they are called what they are.
        let unnamed = &map.sites[0];
        assert!(!unnamed.is_named);
        assert_eq!(unnamed.name, "Take-off");
        assert_eq!(unnamed.kind, SiteKind::Takeoff);
        assert_eq!(unnamed.elevation_m, None);
        assert_eq!(unnamed.wind_in_words, "");
        assert_eq!(unnamed.official, None);
        assert_eq!(unnamed.source, "osm-free-flying");
        assert_eq!(
            unnamed.url.as_deref(),
            Some("https://www.openstreetmap.org/node/5964676196")
        );

        let named = map
            .sites
            .iter()
            .find(|site| site.is_named && site.kind == SiteKind::Takeoff)
            .unwrap();
        assert_eq!(named.name, "Fluggelände Jägerkamp/Nagelspitz");
        assert_eq!(named.kind_in_words, "Take-off");
        assert_eq!(named.elevation_m, Some(1560.0));
        assert_eq!(named.official, Some(true));
        assert_eq!(
            (named.paragliding, named.hang_gliding),
            (Some(true), Some(true))
        );
        assert_eq!(
            map.sites
                .iter()
                .filter(|site| site.kind == SiteKind::Landing)
                .count(),
            2
        );

        // A point has no outline. What its mapper wrote about it is
        // passed on as written.
        assert!(unnamed.outline.is_empty());
        assert_eq!(
            unnamed.description.as_deref(),
            Some("Startrampe (vermutlich für Drachenflieger)")
        );
        assert_eq!((&unnamed.website, &unnamed.fee), (&None, &None));
        assert_eq!(named.fee.as_deref(), Some("yes"));
        assert_eq!(named.fee_in_words, "A fee is asked for using this site.");
        assert_eq!(named.outline.len(), 5);
        assert_eq!(
            (
                unnamed.fee_in_words.as_str(),
                unnamed.guests_in_words.as_str(),
                unnamed.no_flight_in_words.as_str()
            ),
            ("", "", "")
        );
        // The landing field of the Jägerkamp is an area: its outline is a
        // closed ring around the place it is drawn at, and its source
        // names a web page.
        let field = map
            .sites
            .iter()
            .find(|site| site.id == "osm-free-flying:way/1080988126")
            .unwrap();
        assert_eq!(field.outline.len(), 8);
        assert_eq!(field.outline.first(), field.outline.last());
        let (south, north) = field
            .outline
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), point| {
                (low.min(point.lat), high.max(point.lat))
            });
        assert!(south < field.position.lat && field.position.lat < north);
        assert_eq!(field.website.as_deref(), Some("https://www.dhv.de/"));

        // Asked for more than the server's set is whole for, the answer is
        // not the whole truth about the box.
        let wider = sites_map(45.0, 9.0, 49.0, 18.0, 200, recorded()).unwrap();
        assert!(!wider.is_whole);
    }

    #[test]
    fn the_site_of_a_place_is_the_one_named_or_the_nearest_take_off() {
        // Where the development mode's flights take off, beside the two
        // ramps on the Wallberg.
        let (lat, lon) = (47.6642, 11.7967);
        let path = site_at_path(lat, lon).unwrap();
        assert!(path.starts_with("/v1/sites?bbox="), "{path}");

        let nearest = site_at(lat, lon, None, 200, recorded()).unwrap();
        let site = nearest.site.unwrap();
        assert_eq!(site.id, "osm-free-flying:node/5964676196");
        assert_eq!(site.kind, SiteKind::Takeoff);
        assert_eq!(nearest.credit.unwrap().text, "© OpenStreetMap contributors");
        assert!(nearest.as_of_unix_s.is_some());

        // Named, it is that one, also where another lies nearer.
        let named = site_at(
            lat,
            lon,
            Some("osm-free-flying:node/5964676197".to_owned()),
            200,
            recorded(),
        )
        .unwrap();
        assert_eq!(named.site.unwrap().id, "osm-free-flying:node/5964676197");

        // A landing is no take-off, and a site farther than a kilometre is
        // not the place's: in the valley there is none.
        let valley = site_at(47.6919, 11.8880, None, 200, recorded()).unwrap();
        assert_eq!(valley.site, None);
        assert_eq!(valley.credit, None);
        assert_eq!(valley.as_of_unix_s, None);
        // A name the answer does not hold is no site either.
        let gone = site_at(
            lat,
            lon,
            Some("osm-free-flying:node/1".to_owned()),
            200,
            recorded(),
        );
        assert_eq!(gone.unwrap().site, None);
        // What is no place is not asked about.
        assert!(site_at_path(f64::NAN, lon).is_err());
        assert!(site_at(lat, lon, None, 503, Vec::new()).is_err());
    }

    #[test]
    fn a_wind_fits_a_site_within_its_spans_and_a_little_to_either_side() {
        let span = |from_deg: f64, to_deg: f64| WindSpan { from_deg, to_deg };
        // West to north-west.
        let site = vec![span(270.0, 315.0)];
        for (from, fits) in [
            (290.0, true),
            (270.0, true),
            // Eight degrees south of west is still west wind.
            (262.0, true),
            (255.0, false),
            (326.0, true),
            (330.0, false),
            (90.0, false),
        ] {
            assert_eq!(wind_fits(site.clone(), from), Some(fits), "{from}");
        }
        // A span across north.
        let north = vec![span(337.5, 22.5)];
        for (from, fits) in [
            (0.0, true),
            (350.0, true),
            (30.0, true),
            (40.0, false),
            (180.0, false),
            (320.0, false),
        ] {
            assert_eq!(wind_fits(north.clone(), from), Some(fits), "{from}");
        }
        // A single direction is that point of the compass.
        let south = vec![span(180.0, 180.0)];
        assert_eq!(wind_fits(south.clone(), 172.0), Some(true));
        assert_eq!(wind_fits(south.clone(), 195.0), Some(false));
        // Any of several spans will do.
        let two = vec![span(180.0, 180.0), span(270.0, 315.0)];
        assert_eq!(wind_fits(two.clone(), 300.0), Some(true));
        assert_eq!(wind_fits(two, 225.0), Some(false));
        // All the way round.
        assert_eq!(wind_fits(vec![span(0.0, 359.0)], 359.5), Some(true));
        assert_eq!(wind_fits(vec![span(0.0, 360.0)], 180.0), Some(true));
        // A site that names no direction is not said to fit or not.
        assert_eq!(wind_fits(Vec::new(), 270.0), None);
        assert_eq!(wind_fits(south, f64::NAN), None);
    }

    #[test]
    fn the_stations_near_a_place_come_nearest_first_with_whether_their_wind_fits() {
        use crate::{WindMark, WindReading, WindSymbol};
        let station = |name: &str,
                       lat: f64,
                       lon: f64,
                       elevation_m: Option<f64>,
                       from: Option<f64>,
                       is_old: bool| {
            WindStation {
                id: name.to_owned(),
                name: name.to_owned(),
                source: "test".to_owned(),
                position: Coordinate { lat, lon },
                elevation_m,
                latest: Some(WindReading {
                    time_unix_s: 0,
                    speed_ms: Some(4.0),
                    gust_ms: None,
                    direction_deg: from,
                    temperature_c: None,
                    humidity_pct: None,
                }),
                age_min: Some(10),
                is_old,
                mark: WindMark {
                    symbol: WindSymbol::Calm,
                    heading_deg: 0.0,
                    label: String::new(),
                },
            }
        };
        let site = Coordinate {
            lat: 47.66,
            lon: 11.80,
        };
        let stations = vec![
            station("far", 47.42, 10.98, Some(2956.0), Some(280.0), false),
            station("summit", 47.67, 11.80, Some(1700.0), Some(280.0), false),
            station("valley", 47.70, 11.76, None, Some(90.0), false),
            station("stale", 47.66, 11.82, Some(800.0), Some(280.0), true),
            station("still", 47.68, 11.83, Some(750.0), None, false),
        ];
        let works_in = vec![WindSpan {
            from_deg: 270.0,
            to_deg: 315.0,
        }];
        let near = stations_near(stations.clone(), site, Some(1600.0), works_in, 15_000.0, 3);
        let names: Vec<&str> = near.iter().map(|near| near.station.name.as_str()).collect();
        // The summit a kilometre north, the stale one a kilometre and a
        // half east, then the one without a direction; the valley is
        // fourth and the far one beyond reach.
        assert_eq!(names, ["summit", "stale", "still"]);
        assert!(
            (near[0].distance_m - 1_112.0).abs() < 5.0,
            "{}",
            near[0].distance_m
        );
        assert!(near[0].bearing_deg.abs() < 0.5);
        assert_eq!(near[0].higher_by_m, Some(100.0));
        assert_eq!(near[0].fits, Some(true));
        // An old reading and a reading without a direction say nothing.
        assert_eq!(near[1].fits, None);
        assert_eq!(near[2].fits, None);
        assert_eq!(near[1].higher_by_m, Some(-800.0));

        // East wind in the valley does not fit; a place without
        // directions is not judged at all.
        let all = stations_near(
            stations.clone(),
            site,
            None,
            vec![WindSpan {
                from_deg: 270.0,
                to_deg: 315.0,
            }],
            15_000.0,
            10,
        );
        assert_eq!(all.len(), 4);
        let valley = all
            .iter()
            .find(|near| near.station.name == "valley")
            .unwrap();
        assert_eq!(valley.fits, Some(false));
        assert_eq!(valley.higher_by_m, None);
        let unjudged = stations_near(stations, site, None, Vec::new(), 15_000.0, 10);
        assert!(unjudged.iter().all(|near| near.fits.is_none()));
    }

    #[test]
    fn uses_and_wind_are_put_in_words() {
        use SiteKind::{Landing, Other, Takeoff, Towing, Training};
        assert_eq!(uses_in_words(&[Takeoff, Landing]), "Take-off and landing");
        assert_eq!(
            uses_in_words(&[Takeoff, Landing, Training]),
            "Take-off, landing and training slope"
        );
        assert_eq!(uses_in_words(&[Towing]), "Winch site");
        assert_eq!(uses_in_words(&[Other]), "Flying site");
        assert_eq!(uses_in_words(&[]), "Flying site");

        let span = |from_deg, to_deg| WindSector { from_deg, to_deg };
        assert_eq!(wind_in_words(&[]), "");
        assert_eq!(wind_in_words(&[WindSector::single(225.0)]), "SW");
        assert_eq!(wind_in_words(&[span(180.0, 225.0)]), "S to SW");
        assert_eq!(
            wind_in_words(&[WindSector::single(0.0), span(292.5, 337.5)]),
            "N, WNW to NNW"
        );
        assert_eq!(wind_in_words(&[span(0.0, 360.0)]), "any direction");
    }

    #[test]
    fn what_a_source_writes_about_a_site_is_put_in_lines() {
        let record = |more: &str| -> Site {
            serde_json::from_str(&format!(
                r#"{{"id":"osm-free-flying:node/1","position":{{"lat":47.5,"lon":9.77}},
                    "uses":["takeoff"]{more}}}"#
            ))
            .unwrap()
        };
        let said = site_view(
            &record(
                r#","fee":"yes","guest_guidelines":true,"no_flight_time":"01.05-30.09",
               "website":"http://example.org/fluggebiet",
               "description":"Nicht im hohen Gras landen.\nWindsack vorhanden.""#,
            ),
            None,
        );
        assert_eq!(said.fee_in_words, "A fee is asked for using this site.");
        assert_eq!(
            said.guests_in_words,
            "Guests have to read this site's rules for guests before they fly here."
        );
        assert_eq!(
            said.no_flight_in_words,
            "Not to be used in this time, as its source writes it: 01.05-30.09"
        );
        assert_eq!(
            said.website.as_deref(),
            Some("http://example.org/fluggebiet")
        );
        // A description is passed on as written, with its lines.
        assert_eq!(
            said.description.as_deref(),
            Some("Nicht im hohen Gras landen.\nWindsack vorhanden.")
        );

        let other = site_view(&record(r#","fee":"no","guest_guidelines":false"#), None);
        assert_eq!(other.fee_in_words, "No fee is asked for using this site.");
        assert_eq!(
            other.guests_in_words,
            "Its source names no rules for guests."
        );
        assert_eq!(other.no_flight_in_words, "");
        // Anything else a mapper wrote about a fee is shown as written.
        let own = site_view(&record(r#","fee":"donation""#), None);
        assert_eq!(own.fee_in_words, "Fee: donation");
        assert_eq!(own.guests_in_words, "");
    }

    /// An answer with OpenStreetMap's layer and Venti's own: a take-off
    /// of each on the same hill, a landing that pilots added and whoever
    /// runs Venti corrected, and a take-off whose author has left. Made
    /// up, in the shape `docs/api.md` gives the answer.
    fn with_the_layer_of_pilots() -> Vec<u8> {
        br#"{
            "layers": [
                {"source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z",
                 "as_of": "2026-11-18T21:21:02Z",
                 "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
                 "sites": [{"id": "osm-free-flying:node/5964676196",
                            "position": {"lat": 47.6652, "lon": 11.7954}, "uses": ["takeoff"],
                            "url": "https://www.openstreetmap.org/node/5964676196"}]},
                {"source": "venti", "fetched_at": "2026-11-21T09:00:00Z",
                 "as_of": "2026-11-21T09:00:00Z",
                 "sites": [
                    {"id": "venti:00000000000000000000000041", "name": "Musterberg Ost",
                     "position": {"lat": 47.6642, "lon": 11.7967}, "elevation_m": 1580.0,
                     "uses": ["takeoff"],
                     "wind_directions": [{"from_deg": 45.0, "to_deg": 135.0}],
                     "description": "Made up for the development mode."},
                    {"id": "venti:00000000000000000000000042", "name": "Musterwiese",
                     "position": {"lat": 47.6700, "lon": 11.7900}, "uses": ["landing"]},
                    {"id": "venti:00000000000000000000000043",
                     "position": {"lat": 47.6600, "lon": 11.7800}, "uses": ["takeoff"]}],
                 "standing": [
                    {"id": "venti:00000000000000000000000041", "revision": 3,
                     "created_at": "2026-11-03T10:00:00Z",
                     "created_by": {"handle": "demo", "display_name": "Max Mustermann"},
                     "changed_at": "2026-11-12T17:30:00Z",
                     "changed_by": {"handle": "mara", "display_name": "Mara Beispiel"}},
                    {"id": "venti:00000000000000000000000042", "revision": 2,
                     "created_at": "2026-11-05T08:00:00Z",
                     "created_by": {"handle": "jonas", "display_name": ""},
                     "changed_at": "2026-11-06T08:00:00Z"},
                    {"id": "venti:00000000000000000000000043", "revision": 1,
                     "created_at": "2026-11-07T08:00:00Z",
                     "changed_at": "2026-11-07T08:00:00Z"}]}
            ],
            "attributions": [
                {"source": "osm-free-flying", "provider": "OpenStreetMap contributors",
                 "licence": "ODbL 1.0", "text": "\u00a9 OpenStreetMap contributors",
                 "url": "https://www.openstreetmap.org/copyright"},
                {"source": "venti", "provider": "Venti contributors", "licence": "CC0 1.0",
                 "text": "Venti contributors",
                 "url": "https://creativecommons.org/publicdomain/zero/1.0/"}
            ]
        }"#
        .to_vec()
    }

    #[test]
    fn a_site_that_pilots_made_says_who_added_it_and_that_nobody_verified_it() {
        let map = sites_map(47.55, 11.6, 47.85, 12.0, 200, with_the_layer_of_pilots()).unwrap();
        // Both layers are on the map, each site its own source's, each
        // source with its own credit: nothing of the two is put together.
        let sources: Vec<&str> = map.sites.iter().map(|site| site.source.as_str()).collect();
        assert_eq!(sources, ["osm-free-flying", "venti", "venti", "venti"]);
        let licences: Vec<&str> = map
            .credits
            .iter()
            .map(|credit| credit.licence.as_str())
            .collect();
        assert_eq!(licences, ["ODbL 1.0", "CC0 1.0"]);
        // A source's site says nothing of the kind.
        assert_eq!(map.sites[0].by_pilots, None);

        let site = &map.sites[1];
        assert_eq!(site.name, "Musterberg Ost");
        assert_eq!(site.wind_in_words, "NE to SE");
        // A pilot's site has no record elsewhere to look at.
        assert_eq!(site.url, None);
        let by = site.by_pilots.as_ref().unwrap();
        assert_eq!(by.revision, 3);
        assert_eq!(
            by.note,
            "Added by pilots on Venti and not verified. A site on the map is not permission to \
             fly there."
        );
        assert_eq!(by.added_in_words, "Added by Max Mustermann (@demo)");
        assert_eq!(by.changed_in_words, "Last changed by Mara Beispiel (@mara)");
        assert_eq!(by.added_unix_s, 1_793_700_000);
        assert_eq!(by.changed_unix_s, 1_794_504_600);
        assert_eq!(by.added_by.as_ref().unwrap().handle, "demo");

        // A pilot without a name is their handle. A change with no pilot
        // to it is whoever runs Venti's, or a pilot's who has left: the
        // answer does not say which.
        let corrected = map.sites[2].by_pilots.as_ref().unwrap();
        assert_eq!(corrected.added_in_words, "Added by @jonas");
        assert_eq!(corrected.changed_by, None);
        assert_eq!(
            corrected.changed_in_words,
            "Last changed by whoever runs Venti, or by a pilot who is no longer on Venti"
        );
        // A site nobody changed has no line about a change, and one whose
        // author has left says so.
        let untouched = map.sites[3].by_pilots.as_ref().unwrap();
        assert_eq!(
            untouched.added_in_words,
            "Added by a pilot who is no longer on Venti"
        );
        assert_eq!(untouched.changed_in_words, "");
        assert_eq!(untouched.changed_unix_s, untouched.added_unix_s);

        // What a view says of where its site stands is what the server
        // said, to the second.
        let response: SitesResponse = serde_json::from_slice(&with_the_layer_of_pilots()).unwrap();
        let said = &response.layers[1].standing;
        assert_eq!(standing_of(&map.sites[1]).as_ref(), Some(&said[0]));
        assert_eq!(standing_of(&map.sites[3]).as_ref(), Some(&said[2]));
        assert_eq!(standing_of(&map.sites[0]), None);
    }

    #[test]
    fn the_site_of_a_place_is_the_nearest_take_off_of_either_layer_with_its_own_credit() {
        // On the hill the pilots' take-off is the nearer one.
        let found = site_at(47.6642, 11.7967, None, 200, with_the_layer_of_pilots()).unwrap();
        let site = found.site.unwrap();
        assert_eq!(site.id, "venti:00000000000000000000000041");
        assert!(site.by_pilots.is_some());
        let credit = found.credit.unwrap();
        assert_eq!(
            (credit.source.as_str(), credit.licence.as_str()),
            ("venti", "CC0 1.0")
        );
        // The date is its own layer's, not the oldest of the answer.
        assert_eq!(found.as_of_unix_s, Some(1_795_251_600));

        // OpenStreetMap's is found by its identifier, with its own date.
        let named = site_at(
            47.6642,
            11.7967,
            Some("osm-free-flying:node/5964676196".to_owned()),
            200,
            with_the_layer_of_pilots(),
        )
        .unwrap();
        assert_eq!(named.site.unwrap().by_pilots, None);
        assert_eq!(named.credit.unwrap().licence, "ODbL 1.0");
        assert_eq!(named.as_of_unix_s, Some(1_795_036_862));
    }

    #[test]
    fn a_refusal_and_a_server_without_sites_are_told_apart() {
        let refused = sites_map(
            47.0,
            11.0,
            48.0,
            12.0,
            404,
            br#"{"error": {"code": "not_found", "message": "x"}}"#.to_vec(),
        )
        .unwrap_err();
        assert_eq!(
            refused.to_string(),
            "The server could not answer (not_found)."
        );
        let none = sites_map(
            47.0,
            11.0,
            48.0,
            12.0,
            200,
            br#"{"layers": [], "attributions": []}"#.to_vec(),
        )
        .unwrap();
        assert!(none.sites.is_empty() && !none.is_whole && none.as_of_unix_s.is_none());
    }
}
