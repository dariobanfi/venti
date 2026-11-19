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
use venti_domain::{BoundingBox, GeoPoint, Site, SiteUse, WindSector};
use venti_flight::live::navigation::bearing_deg;

use crate::server::answer;
use crate::wind::credit;
use crate::{Coordinate, CoreError, Credit, WindStation};

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
