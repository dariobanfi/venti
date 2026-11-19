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

use venti_api_types::v1::{SitesRequest, SitesResponse};
use venti_domain::{BoundingBox, Site, SiteUse, WindSector};

use crate::server::answer;
use crate::wind::credit;
use crate::{Coordinate, CoreError, Credit};

/// What a site is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
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
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct WindSpan {
    /// Where the span starts: the direction the wind comes from, degrees
    /// clockwise from north.
    pub from_deg: f64,
    /// Where it ends, going clockwise. Equal to `from_deg` for a single
    /// direction.
    pub to_deg: f64,
}

/// One flying site as a map and a sheet show it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
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
}

/// The sites in a part of the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
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
    Ok(SitesMap {
        sites: response
            .layers
            .iter()
            .flat_map(|layer| &layer.sites)
            .map(site_view)
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

fn site_view(site: &Site) -> SiteView {
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

/// One of sixteen compass points: sites are tagged that finely.
fn compass(degrees: f64) -> &'static str {
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

        // Asked for more than the server's set is whole for, the answer is
        // not the whole truth about the box.
        let wider = sites_map(45.0, 9.0, 49.0, 18.0, 200, recorded()).unwrap();
        assert!(!wider.is_whole);
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
