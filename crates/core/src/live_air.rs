//! What only a server that sees many gliders can say, read from its
//! answers with words: where pilots climb at this moment, how many are in
//! the air at a flying site, and where the public flights' thermals have
//! been.
//!
//! None of it names a pilot. A climb and a cell of the thermals are a
//! place in the air and numbers; a site has two counts. What comes from a
//! network's positions is shown with the network's credit, and never as
//! one record with what comes from Venti's own pilots: the lists here
//! stand side by side.

use venti_api_types::v1::{
    FlownThermalsRequest, FlownThermalsResponse, LiveClimbsRequest, LiveClimbsResponse,
    LiveSitesRequest, LiveSitesResponse, MAX_FLOWN_BOX_DEG, MAX_LIVE_OTHERS_BOX_DEG,
};
use venti_domain::BoundingBox;

use crate::live_others::thousands;
use crate::server::answer;
use crate::wind::{Credit, credit};
use crate::{Coordinate, CoreError};

/// A place where gliders circled and climbed in the last minutes, as a
/// map shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct ClimbView {
    /// The middle of the cell of the grid the climb lies in: the air, not
    /// a glider.
    pub position: Coordinate,
    /// The climb averaged over the circles, m/s.
    #[uniffi(default = 0.0)]
    pub climb_ms: f64,
    /// The lowest height a pilot began to circle at, metres.
    #[uniffi(default = 0.0)]
    pub from_m: f64,
    /// The greatest height a pilot circled to, metres.
    #[uniffi(default = 0.0)]
    pub to_m: f64,
    /// How many pilots circled in it.
    #[uniffi(default = 0)]
    pub pilots: u32,
    /// How long ago the last of them was heard circling in it, seconds,
    /// when the server answered.
    #[uniffi(default = 0.0)]
    pub age_s: f64,
    /// Where the positions came from: a network's source, or `venti`.
    #[uniffi(default = "")]
    pub source: String,
    /// "Climbing now: +2.4 m/s between 1,600 and 2,100 m, 3 pilots, 4 min
    /// ago".
    #[uniffi(default = "")]
    pub line: String,
    /// For the map, short: "+2.4".
    #[uniffi(default = "")]
    pub label: String,
}

/// The climbs of the last minutes in a part of the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveClimbs {
    /// Every layer's climbs, the newest first. A thermal that a network's
    /// gliders and Venti's pilots both work is there twice, once from
    /// each: nothing is merged.
    pub climbs: Vec<ClimbView>,
    /// Credits to show while the layer is on: those of the networks with
    /// a climb in it.
    pub credits: Vec<Credit>,
    /// Whether the part of the map holds more than one answer lists.
    #[uniffi(default = false)]
    pub truncated: bool,
    /// For how many minutes a climb is listed.
    #[uniffi(default = 10)]
    pub listed_minutes: u32,
    /// "3 climbs in the last 10 minutes". Empty when there are none.
    #[uniffi(default = "")]
    pub count_words: String,
}

fn boxed(south: f64, west: f64, north: f64, east: f64, widest: f64) -> Option<BoundingBox> {
    if east - west > widest || north - south > widest {
        return None;
    }
    BoundingBox::new(
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    )
    .ok()
}

/// How long ago, for a climb: "just now", "4 min ago".
fn ago(age_s: f64) -> String {
    if age_s < 60.0 {
        "just now".to_owned()
    } else {
        format!("{} min ago", (age_s / 60.0).round())
    }
}

fn pilots_words(pilots: u32) -> String {
    match pilots {
        1 => "1 pilot".to_owned(),
        n => format!("{n} pilots"),
    }
}

/// A climb rate with its sign: "+2.4".
pub(crate) fn signed(climb_ms: f64) -> String {
    format!("{climb_ms:+.1}")
}

/// The path and query to `GET` for the climbs of the last minutes in a
/// box. `None` for a box wider or taller than the server answers for.
/// Ask again every thirty seconds while the layer is on.
#[uniffi::export]
pub fn live_climbs_path(south: f64, west: f64, north: f64, east: f64) -> Option<String> {
    let bbox = boxed(south, west, north, east, MAX_LIVE_OTHERS_BOX_DEG)?;
    Some(LiveClimbsRequest { bbox }.path_and_query())
}

/// Reads the server's answer to [`live_climbs_path`].
#[uniffi::export]
pub fn live_climbs(status: u16, body: Vec<u8>) -> Result<LiveClimbs, CoreError> {
    let response: LiveClimbsResponse = answer(status, &body)?;
    let mut climbs: Vec<ClimbView> = response
        .layers
        .iter()
        .flat_map(|layer| {
            layer.climbs.iter().map(|climb| {
                let age_s = f64::from(climb.age_s);
                ClimbView {
                    position: Coordinate {
                        lat: climb.position.lat(),
                        lon: climb.position.lon(),
                    },
                    climb_ms: climb.climb_ms,
                    from_m: f64::from(climb.from_m),
                    to_m: f64::from(climb.to_m),
                    pilots: climb.pilots,
                    age_s,
                    source: layer.source.clone(),
                    line: format!(
                        "Climbing now: {} m/s between {} and {} m, {}, {}",
                        signed(climb.climb_ms),
                        thousands(f64::from(climb.from_m)),
                        thousands(f64::from(climb.to_m)),
                        pilots_words(climb.pilots),
                        ago(age_s)
                    ),
                    label: signed(climb.climb_ms),
                }
            })
        })
        .collect();
    climbs.sort_by(|a, b| a.age_s.total_cmp(&b.age_s));
    let credits = response
        .attributions
        .iter()
        .filter(|attribution| {
            response.layers.iter().any(|layer| {
                layer.source == attribution.source.as_str() && !layer.climbs.is_empty()
            })
        })
        .map(credit)
        .collect();
    let listed_minutes = u32::try_from(response.listed_minutes).unwrap_or(10);
    let count_words = match climbs.len() {
        0 => String::new(),
        1 => format!("1 climb in the last {listed_minutes} minutes"),
        n => format!("{n} climbs in the last {listed_minutes} minutes"),
    };
    Ok(LiveClimbs {
        climbs,
        credits,
        truncated: response.layers.iter().any(|layer| layer.truncated),
        listed_minutes,
        count_words,
    })
}

/// What a map says of the layer where it is switched on and nobody
/// climbs: the layer is not broken, the air is quiet or nobody is heard.
#[uniffi::export]
pub fn live_climbs_empty_words() -> String {
    "Nobody was heard circling in a climb here in the last minutes.".to_owned()
}

/// The pilots in the air at one flying site.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteAirView {
    /// The site, as the sites' answers name it.
    pub site_id: String,
    /// Paragliders and hang gliders in the air near it: the count of the
    /// source of live positions that has the most. The sources are not
    /// added up, since a pilot may be in two.
    #[uniffi(default = 0)]
    pub airborne: u32,
    /// How many took off there in the last hour, by the same rule.
    #[uniffi(default = 0)]
    pub took_off: u32,
    /// For a mark on the map, short: "5". Empty when nobody is in the
    /// air.
    #[uniffi(default = "")]
    pub label: String,
}

/// The flying sites of a part of the map with pilots in the air.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesAir {
    /// The sites with somebody in the air or a take-off, the most pilots
    /// first. A site that is not here has nobody.
    pub sites: Vec<SiteAirView>,
    /// Credits to show where a count is shown: those of the networks
    /// that counted.
    pub credits: Vec<Credit>,
}

/// The path and query to `GET` for the sites of a box with pilots in the
/// air. `None` for a box wider or taller than the server answers for.
#[uniffi::export]
pub fn live_sites_path(south: f64, west: f64, north: f64, east: f64) -> Option<String> {
    let bbox = boxed(south, west, north, east, MAX_LIVE_OTHERS_BOX_DEG)?;
    Some(LiveSitesRequest { bbox }.path_and_query())
}

/// Reads the server's answer to [`live_sites_path`].
#[uniffi::export]
pub fn live_sites(status: u16, body: Vec<u8>) -> Result<SitesAir, CoreError> {
    let response: LiveSitesResponse = answer(status, &body)?;
    let mut sites: Vec<SiteAirView> = Vec::new();
    for site in response.layers.iter().flat_map(|layer| &layer.sites) {
        let id = site.site.to_string();
        match sites.iter_mut().find(|held| held.site_id == id) {
            Some(held) => {
                held.airborne = held.airborne.max(site.airborne);
                held.took_off = held.took_off.max(site.took_off);
            }
            None => sites.push(SiteAirView {
                site_id: id,
                airborne: site.airborne,
                took_off: site.took_off,
                label: String::new(),
            }),
        }
    }
    for site in &mut sites {
        if site.airborne > 0 {
            site.label = site.airborne.to_string();
        }
    }
    sites.sort_by(|a, b| {
        (b.airborne, b.took_off, &a.site_id).cmp(&(a.airborne, a.took_off, &b.site_id))
    });
    let credits = response
        .attributions
        .iter()
        .filter(|attribution| {
            response
                .layers
                .iter()
                .any(|layer| layer.source == attribution.source.as_str() && !layer.sites.is_empty())
        })
        .map(credit)
        .collect();
    Ok(SitesAir { sites, credits })
}

/// The line a site's page says of who flies there now: "5 pilots in the
/// air at Wallberg, 2 took off in the last hour". Empty when nobody is in
/// the air and nobody took off: the page then says nothing.
#[uniffi::export]
pub fn site_air_words(air: SitesAir, site_id: String, site_name: String) -> String {
    let Some(site) = air.sites.iter().find(|site| site.site_id == site_id) else {
        return String::new();
    };
    let at = match site_name.trim() {
        "" => String::new(),
        name => format!(" at {name}"),
    };
    let took_off = match site.took_off {
        0 => String::new(),
        n => format!("{n} took off in the last hour"),
    };
    match (site.airborne, took_off.is_empty()) {
        (0, true) => String::new(),
        (0, false) => format!("Nobody in the air{at} now, {took_off}"),
        (n, true) => format!("{} in the air{at}", pilots_words(n)),
        (n, false) => format!("{} in the air{at}, {took_off}", pilots_words(n)),
    }
}

/// A cell in which the public flights of several pilots climbed, as a
/// map shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlownCellView {
    /// The middle of the cell.
    pub position: Coordinate,
    /// How many climbs lie in it.
    #[uniffi(default = 0)]
    pub climbs: u32,
    /// Of how many pilots.
    #[uniffi(default = 0)]
    pub pilots: u32,
    /// The climbs' average, m/s.
    #[uniffi(default = 0.0)]
    pub climb_ms: f64,
    /// The best of them, m/s.
    #[uniffi(default = 0.0)]
    pub best_climb_ms: f64,
    /// The height a climb began at, on average, metres.
    #[uniffi(default = 0.0)]
    pub from_m: f64,
    /// The height a climb ended at, on average, metres.
    #[uniffi(default = 0.0)]
    pub to_m: f64,
    /// How much of the answer's most-flown cell this one is, by pilots,
    /// 0 to 1: for how strongly a map draws it.
    #[uniffi(default = 0.0)]
    pub weight: f64,
    /// "12 climbs of 5 pilots, +1.9 m/s on average, best +3.4, from
    /// 1,450 to 2,100 m".
    #[uniffi(default = "")]
    pub line: String,
}

/// Where thermals have been in a part of the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct FlownThermals {
    /// The cells, the most pilots first.
    pub cells: Vec<FlownCellView>,
    /// Whether the part of the map holds more than one answer lists.
    #[uniffi(default = false)]
    pub more: bool,
    /// The side of a cell, degrees of latitude and of longitude: a map
    /// draws each as a rectangle of this size around its middle.
    #[uniffi(default = 0.005)]
    pub cell_deg: f64,
    /// What the layer is, for its legend: "Where pilots climbed, from the
    /// public flights on Venti. A place shows once 3 pilots climbed
    /// there."
    #[uniffi(default = "")]
    pub note: String,
    /// What the flights it is made from are given under, in the
    /// server's words.
    #[uniffi(default = "")]
    pub terms: String,
    /// A short title for a link that opens `terms`: "Terms for these
    /// flights".
    #[uniffi(default = "")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub terms_title: String,
}

/// The path and query to `GET` for where thermals have been in a box, in
/// a month of the year (1 to 12) and between two hours by the sun, if
/// given. `None` for a box wider or taller than the server answers for,
/// one degree, and for a month or hours that are none.
#[uniffi::export]
pub fn flown_thermals_path(
    south: f64,
    west: f64,
    north: f64,
    east: f64,
    month: Option<u8>,
    from_hour: Option<u8>,
    to_hour: Option<u8>,
) -> Option<String> {
    let bbox = boxed(south, west, north, east, MAX_FLOWN_BOX_DEG)?;
    if month.is_some_and(|month| !(1..=12).contains(&month)) {
        return None;
    }
    let hours = match (from_hour, to_hour) {
        (None, None) => None,
        (from, to) => {
            let (from, to) = (from.unwrap_or(0), to.unwrap_or(24));
            if from >= to || to > 24 {
                return None;
            }
            Some([from, to])
        }
    };
    Some(FlownThermalsRequest { bbox, month, hours }.path_and_query())
}

/// What a map says where it shows too much to ask for the thermals.
#[uniffi::export]
pub fn flown_too_wide_words() -> String {
    "Zoom in to see where pilots climbed.".to_owned()
}

/// Reads the server's answer to [`flown_thermals_path`].
#[uniffi::export]
pub fn flown_thermals(status: u16, body: Vec<u8>) -> Result<FlownThermals, CoreError> {
    let response: FlownThermalsResponse = answer(status, &body)?;
    let most = response
        .cells
        .iter()
        .map(|cell| cell.pilots)
        .max()
        .unwrap_or(1)
        .max(1);
    let cells = response
        .cells
        .iter()
        .map(|cell| FlownCellView {
            position: Coordinate {
                lat: cell.position.lat(),
                lon: cell.position.lon(),
            },
            climbs: cell.climbs,
            pilots: cell.pilots,
            climb_ms: cell.climb_ms,
            best_climb_ms: cell.best_climb_ms,
            from_m: f64::from(cell.from_m),
            to_m: f64::from(cell.to_m),
            weight: f64::from(cell.pilots) / f64::from(most),
            line: format!(
                "{} of {}, {} m/s on average, best {}, from {} to {} m",
                match cell.climbs {
                    1 => "1 climb".to_owned(),
                    n => format!("{n} climbs"),
                },
                pilots_words(cell.pilots),
                signed(cell.climb_ms),
                signed(cell.best_climb_ms),
                thousands(f64::from(cell.from_m)),
                thousands(f64::from(cell.to_m)),
            ),
        })
        .collect();
    Ok(FlownThermals {
        cells,
        more: response.more,
        cell_deg: response.cell_deg,
        note: format!(
            "Where pilots climbed, from the public flights on Venti. A place shows once {} \
             pilots climbed there.",
            response.min_pilots
        ),
        terms: response.terms.text,
        terms_title: crate::takeoffs::TERMS_TITLE.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIMBS: &[u8] = include_bytes!("../../server/tests/golden/live_climbs.json");
    const SITES: &[u8] = include_bytes!("../../server/tests/golden/live_sites.json");
    const FLOWN: &[u8] = include_bytes!("../../server/tests/golden/thermals_flown.json");

    #[test]
    fn climbs_read_with_words_and_the_networks_credit() {
        let climbs = live_climbs(200, CLIMBS.to_vec()).unwrap();
        assert_eq!(climbs.count_words, "2 climbs in the last 10 minutes");
        assert_eq!(climbs.credits.len(), 1);
        assert_eq!(climbs.credits[0].licence, "ODbL 1.0");
        // The newest first.
        assert_eq!(
            climbs.climbs[0].line,
            "Climbing now: +1.3 m/s between 1,350 and 1,550 m, 2 pilots, just now"
        );
        assert_eq!(
            climbs.climbs[1].line,
            "Climbing now: +2.4 m/s between 1,600 and 2,100 m, 3 pilots, 4 min ago"
        );
        assert_eq!(climbs.climbs[1].label, "+2.4");
        assert_eq!(climbs.climbs[1].source, "ogn");
        assert!(!climbs.truncated);
    }

    #[test]
    fn a_site_says_who_flies_there_in_one_line_or_nothing() {
        let air = live_sites(200, SITES.to_vec()).unwrap();
        assert!(!air.sites.is_empty());
        assert_eq!(air.credits.len(), 1);
        let first = air.sites[0].clone();
        assert_eq!(first.label, first.airborne.to_string());
        let line = site_air_words(air.clone(), first.site_id.clone(), "Wallberg".to_owned());
        assert_eq!(
            line,
            format!(
                "{} pilots in the air at Wallberg, {} took off in the last hour",
                first.airborne, first.took_off
            )
        );
        // A site nobody flies at says nothing.
        assert_eq!(
            site_air_words(air, "osm:node/1".to_owned(), "Nowhere".to_owned()),
            ""
        );
        let one = SitesAir {
            sites: vec![SiteAirView {
                site_id: "a:1".to_owned(),
                airborne: 1,
                took_off: 0,
                label: "1".to_owned(),
            }],
            credits: Vec::new(),
        };
        assert_eq!(
            site_air_words(one, "a:1".to_owned(), String::new()),
            "1 pilot in the air"
        );
    }

    #[test]
    fn two_sources_of_one_site_are_not_added_up() {
        let body = br#"{"time":"2026-07-04T11:00:00Z","within_m":5000.0,"took_off_minutes":60,
            "layers":[
              {"source":"ogn","sites":[{"site":"osm:node/1","airborne":4,"took_off":1}],"truncated":false},
              {"source":"venti","sites":[{"site":"osm:node/1","airborne":2,"took_off":2}],"truncated":false}],
            "attributions":[]}"#;
        let air = live_sites(200, body.to_vec()).unwrap();
        assert_eq!(air.sites.len(), 1);
        assert_eq!((air.sites[0].airborne, air.sites[0].took_off), (4, 2));
    }

    #[test]
    fn flown_thermals_read_with_words() {
        let flown = flown_thermals(200, FLOWN.to_vec()).unwrap();
        assert_eq!(flown.cells.len(), 2);
        assert_eq!(
            flown.cells[0].line,
            "5 climbs of 3 pilots, +2.4 m/s on average, best +2.4, from 1,500 to 2,100 m"
        );
        assert_eq!(flown.cells[0].weight, 1.0);
        assert!(flown.note.contains("once 3 pilots climbed there"));
        assert!(flown.terms.starts_with("These flights are their pilots'"));
        assert_eq!(flown.terms_title, "Terms for these flights");
        assert_eq!(flown.cell_deg, 0.005);
    }

    #[test]
    fn the_paths_ask_for_what_the_server_answers() {
        assert_eq!(
            live_climbs_path(47.55, 11.6, 47.85, 12.0).as_deref(),
            Some("/v1/live/climbs?bbox=11.6,47.55,12,47.85")
        );
        assert_eq!(
            live_sites_path(47.55, 11.6, 47.85, 12.0).as_deref(),
            Some("/v1/live/sites?bbox=11.6,47.55,12,47.85")
        );
        assert_eq!(live_climbs_path(40.0, 0.0, 50.0, 20.0), None);
        assert_eq!(
            flown_thermals_path(47.55, 11.6, 47.85, 12.0, Some(7), Some(11), Some(15)).as_deref(),
            Some("/v1/thermals/flown?bbox=11.6,47.55,12,47.85&month=7&hours=11-15")
        );
        assert_eq!(
            flown_thermals_path(47.55, 11.6, 47.85, 12.0, None, Some(12), None).as_deref(),
            Some("/v1/thermals/flown?bbox=11.6,47.55,12,47.85&hours=12-24")
        );
        // Wider than a degree, a month that is none, hours backwards.
        assert_eq!(
            flown_thermals_path(46.0, 10.0, 48.0, 12.0, None, None, None),
            None
        );
        assert_eq!(
            flown_thermals_path(47.55, 11.6, 47.85, 12.0, Some(13), None, None),
            None
        );
        assert_eq!(
            flown_thermals_path(47.55, 11.6, 47.85, 12.0, None, Some(15), Some(11)),
            None
        );
    }
}
