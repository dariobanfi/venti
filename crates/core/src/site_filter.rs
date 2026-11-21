//! A filter of the flying sites, for a map and for a list.
//!
//! A pilot asks three things of the sites: what a site is used for, which
//! wind a take-off works in, and how high it lies. The choices, the rules
//! and every word about them are here, so that the three apps offer the
//! same filter and say the same of what it leaves.
//!
//! The rules:
//!
//! - **By use.** All sites, the places to take off from, or the places to
//!   land. A site that is both is among each.
//! - **By wind.** A take-off fits a wind that comes from a direction its
//!   source names, by the rule the forecast is judged with
//!   ([`wind_fits`]): within its spans, or within half a compass point of
//!   one. The direction is one the pilot chose, or the wind now as the
//!   shell has it from a forecast or a station.
//! - **By height.** A take-off at the height asked for, or higher, by the
//!   height its source gives.
//!
//! The wind and the height are asked of take-offs only. A landing has no
//! direction it works in, and nobody looks for a landing by its height:
//! landings and every other site are shown or not by their use alone, and
//! under "Landings" the two are not asked at all.
//!
//! **What a source does not say.** Most take-offs give no wind directions
//! and fewer still a height (data-sources.md has the count for the first
//! region). Such a take-off says nothing: it is neither a fit nor left
//! out. It comes back apart from the ones that fit, with words for the
//! difference, so that a screen can show the two apart. Hiding it would
//! hide most take-offs behind any filter, and showing it among the fits
//! would say what nobody said.
//!
//! As with everything about sites, a fit is a fact about a direction or a
//! height. It says nothing about the wind's strength, the gusts or the
//! day, and a site on the map is not permission to fly there.

use crate::sites::{SiteKind, SiteView, compass, wind_fits};

/// The heights the filter offers for "take-offs from", metres above mean
/// sea level. Round figures between the lowest and the highest take-offs
/// of the first region that give a height (857 to 2580 m). Ours, and the
/// owner's to decide.
const HEIGHT_STEPS_M: [f64; 3] = [1000.0, 1500.0, 2000.0];

/// Which sites a filter shows, by what they are used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum SitesShown {
    /// Every site, whatever it is used for.
    All,
    /// The places to take off from on foot.
    Takeoffs,
    /// The places to land.
    Landings,
}

/// Where the wind direction of a filter comes from. It changes the words
/// and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum SitesWindIs {
    /// A direction the pilot chose.
    Chosen,
    /// The direction a forecast has for now, which the shell read.
    ForecastNow,
    /// The direction a wind station measured last, which the shell read.
    MeasuredNow,
}

/// What a pilot asks of the sites. With `shown` as [`SitesShown::All`] and
/// neither a wind nor a height it asks nothing, and every site is shown.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesFilter {
    /// Which sites, by what they are used for.
    pub shown: SitesShown,
    /// The wind a take-off is to work in: where it comes from, degrees
    /// clockwise from north. `None` asks about no wind. One of
    /// [`SitesFilterChoices::winds`], or the direction of the wind now.
    pub wind_from_deg: Option<f64>,
    /// Where `wind_from_deg` comes from, for the words.
    pub wind_is: SitesWindIs,
    /// Take-offs from this height up, metres above mean sea level. `None`
    /// asks about no height.
    pub takeoffs_from_m: Option<f64>,
}

/// One choice of which sites to show.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesShownChoice {
    /// What to put in [`SitesFilter::shown`].
    pub shown: SitesShown,
    /// Its name: "All sites", "Take-offs", "Landings".
    pub words: String,
    /// Whether the wind and the height are asked under this choice. Under
    /// "Landings" they are not, and a screen offers neither there.
    pub asks_takeoffs: bool,
}

/// One wind direction to choose.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesWindChoice {
    /// What to put in [`SitesFilter::wind_from_deg`], with
    /// [`SitesWindIs::Chosen`].
    pub from_deg: f64,
    /// The direction as a compass point: "NW".
    pub words: String,
}

/// A choice of the wind now. The shell offers it when it has a direction
/// to give: it puts that direction in [`SitesFilter::wind_from_deg`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesWindNowChoice {
    /// What to put in [`SitesFilter::wind_is`].
    pub wind_is: SitesWindIs,
    /// Its name: "Forecast now", "Measured now".
    pub words: String,
}

/// One height to choose for "take-offs from".
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesHeightChoice {
    /// What to put in [`SitesFilter::takeoffs_from_m`]. `None` for the
    /// choice that asks about no height.
    pub from_m: Option<f64>,
    /// Its name: "Any", "1500 m".
    pub words: String,
}

/// The choices the filter of sites offers, with their words.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesFilterChoices {
    /// What stands over the choice of sites: "Show".
    pub shown_title: String,
    /// Which sites to show: all first.
    pub shown: Vec<SitesShownChoice>,
    /// What stands over the choice of a wind: "Take-offs for a wind from".
    pub wind_title: String,
    /// The name of the choice that asks about no wind: "Any".
    pub any_wind: String,
    /// The wind now, from a forecast and from a station.
    pub winds_now: Vec<SitesWindNowChoice>,
    /// The sixteen points of the compass, from north clockwise: sites are
    /// described that finely, and a take-off that works in NNW alone fits
    /// neither N nor NW.
    pub winds: Vec<SitesWindChoice>,
    /// What stands over the choice of a height: "Take-offs from".
    pub height_title: String,
    /// The heights: any first, then from the lowest up.
    pub heights: Vec<SitesHeightChoice>,
    /// What the filter does and what it does not say, to show where it is
    /// chosen.
    pub note: String,
}

/// The sites a filter leaves, and what to say of them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SitesFiltered {
    /// The sites that are what was asked for, in the order they came in.
    pub fitting: Vec<SiteView>,
    /// Take-offs whose source does not say what was asked about: no wind
    /// directions, or no height. To be shown apart from `fitting`, and not
    /// left out. Empty when neither a wind nor a height is asked.
    pub unsaid: Vec<SiteView>,
    /// What the filter leaves, in a line: "12 of 48 take-offs fit a wind
    /// from NW", "9 of 48 take-offs lie at 1500 m or higher", "48
    /// take-offs", "No landings here". The take-offs it counts are those
    /// among the sites handed in.
    pub words: String,
    /// What the take-offs in `unsaid` are, as a heading for them: "31
    /// take-offs give no wind directions". Empty when there are none.
    pub unsaid_title: String,
    /// What that means, to show under the heading. Empty when there are
    /// none.
    pub unsaid_note: String,
    /// The filter itself in a line, for the button that opens it:
    /// "Take-offs for a wind from NW, from 1500 m up". "All sites" when it
    /// asks nothing.
    pub filter_words: String,
    /// Whether the filter asks anything: whether some site could be left
    /// out or shown apart by it.
    pub is_on: bool,
}

/// The choices of the filter of sites, and what to say with them.
#[uniffi::export]
pub fn sites_filter_choices() -> SitesFilterChoices {
    let shown = [
        (SitesShown::All, "All sites"),
        (SitesShown::Takeoffs, "Take-offs"),
        (SitesShown::Landings, "Landings"),
    ];
    let winds_now = [
        (SitesWindIs::ForecastNow, "Forecast now"),
        (SitesWindIs::MeasuredNow, "Measured now"),
    ];
    let any_height = SitesHeightChoice {
        from_m: None,
        words: "Any".to_owned(),
    };
    SitesFilterChoices {
        shown_title: "Show".to_owned(),
        shown: shown
            .into_iter()
            .map(|(shown, words)| SitesShownChoice {
                shown,
                words: words.to_owned(),
                asks_takeoffs: asks_takeoffs(shown),
            })
            .collect(),
        wind_title: "Take-offs for a wind from".to_owned(),
        any_wind: "Any".to_owned(),
        winds_now: winds_now
            .into_iter()
            .map(|(wind_is, words)| SitesWindNowChoice {
                wind_is,
                words: words.to_owned(),
            })
            .collect(),
        winds: (0..16)
            .map(|point| {
                let from_deg = f64::from(point) * 22.5;
                SitesWindChoice {
                    from_deg,
                    words: compass(from_deg).to_owned(),
                }
            })
            .collect(),
        height_title: "Take-offs from".to_owned(),
        heights: std::iter::once(any_height)
            .chain(HEIGHT_STEPS_M.iter().map(|&from_m| SitesHeightChoice {
                from_m: Some(from_m),
                words: metres(from_m),
            }))
            .collect(),
        note: "The wind and the height are asked of take-offs only: landings and other sites \
               stay as they are. A take-off fits a wind from a direction its source names, or \
               within half a compass point of one. That says nothing about the wind's strength, \
               the gusts or the day. \"Forecast now\" and \"Measured now\" take one direction \
               for every take-off, the wind at one place, which is not the wind at each of them. \
               Many take-offs give no wind directions and no height: those are shown apart, \
               neither as a fit nor left out. A site on the map is not permission to fly there."
            .to_owned(),
    }
}

/// The sites a filter leaves of those handed in, with what to say of them.
///
/// A site that is not what the filter shows is left out. Of a take-off the
/// wind and the height are asked, where the filter asks them: one that its
/// source says does not fit is left out, one whose source does not say is
/// in `unsaid`, and the rest fit. A wind or a height that is no number
/// asks nothing.
#[uniffi::export]
pub fn filter_sites(sites: Vec<SiteView>, filter: SitesFilter) -> SitesFiltered {
    let asked = Asked::of(&filter);
    let mut fitting = Vec::new();
    let mut unsaid = Vec::new();
    // The take-offs the wind and the height were asked of, and how many
    // of them fit.
    let (mut takeoffs, mut fits) = (0_usize, 0_usize);
    for site in sites {
        let is_takeoff = site.uses.contains(&SiteKind::Takeoff);
        let shown = match filter.shown {
            SitesShown::All => true,
            SitesShown::Takeoffs => is_takeoff,
            SitesShown::Landings => site.uses.contains(&SiteKind::Landing),
        };
        if !shown {
            continue;
        }
        if !(asked.of_takeoffs() && is_takeoff) {
            fitting.push(site);
            continue;
        }
        takeoffs += 1;
        // What the site's source says to each thing asked: yes, no, or
        // nothing.
        let said = [
            asked
                .wind
                .map(|from_deg| wind_fits(site.wind.clone(), from_deg)),
            asked
                .height
                .map(|from_m| site.elevation_m.map(|height| height >= from_m)),
        ];
        if said.contains(&Some(Some(false))) {
            continue;
        }
        if said.contains(&Some(None)) {
            unsaid.push(site);
        } else {
            fits += 1;
            fitting.push(site);
        }
    }

    let words = if asked.of_takeoffs() {
        asked.fits_in_words(fits, takeoffs)
    } else {
        let (one, many) = match filter.shown {
            SitesShown::All => ("site", "sites"),
            SitesShown::Takeoffs => ("take-off", "take-offs"),
            SitesShown::Landings => ("landing", "landings"),
        };
        match fitting.len() {
            0 => format!("No {many} here"),
            1 => format!("1 {one}"),
            count => format!("{count} {many}"),
        }
    };
    let (unsaid_title, unsaid_note) = if unsaid.is_empty() {
        (String::new(), String::new())
    } else {
        (
            asked.unsaid_in_words(unsaid.len()),
            "Their source does not say, which says nothing about them: they may fit or not. \
             They are shown apart, not left out."
                .to_owned(),
        )
    };
    SitesFiltered {
        fitting,
        unsaid,
        words,
        unsaid_title,
        unsaid_note,
        filter_words: asked.in_words(filter.shown),
        is_on: filter.shown != SitesShown::All || asked.of_takeoffs(),
    }
}

/// What a filter asks of take-offs.
struct Asked {
    /// The wind to work in, where one is asked: where it comes from.
    wind: Option<f64>,
    wind_is: SitesWindIs,
    /// The height to lie at or above, where one is asked.
    height: Option<f64>,
}

impl Asked {
    fn of(filter: &SitesFilter) -> Self {
        let of_takeoffs = asks_takeoffs(filter.shown);
        Self {
            wind: filter
                .wind_from_deg
                .filter(|from_deg| of_takeoffs && from_deg.is_finite()),
            wind_is: filter.wind_is,
            height: filter
                .takeoffs_from_m
                .filter(|from_m| of_takeoffs && from_m.is_finite()),
        }
    }

    /// Whether anything is asked of take-offs.
    fn of_takeoffs(&self) -> bool {
        self.wind.is_some() || self.height.is_some()
    }

    /// The wind as what a take-off fits: "a wind from NW", "the NW wind
    /// forecast now".
    fn wind_in_words(&self, from_deg: f64) -> String {
        let from = compass(from_deg);
        match self.wind_is {
            SitesWindIs::Chosen => format!("a wind from {from}"),
            SitesWindIs::ForecastNow => format!("the {from} wind forecast now"),
            SitesWindIs::MeasuredNow => format!("the {from} wind measured now"),
        }
    }

    /// "12 of 48 take-offs fit a wind from NW", "1 of 48 take-offs lies at
    /// 1500 m or higher".
    fn fits_in_words(&self, fits: usize, takeoffs: usize) -> String {
        if takeoffs == 0 {
            return "No take-offs here".to_owned();
        }
        let one = fits == 1;
        let wind = self.wind.map(|from_deg| {
            format!(
                "{} {}",
                if one { "fits" } else { "fit" },
                self.wind_in_words(from_deg)
            )
        });
        let height = self.height.map(|from_m| {
            format!(
                "{} at {} or higher",
                if one { "lies" } else { "lie" },
                metres(from_m)
            )
        });
        let does: Vec<String> = wind.into_iter().chain(height).collect();
        format!(
            "{fits} of {takeoffs} {} {}",
            if takeoffs == 1 {
                "take-off"
            } else {
                "take-offs"
            },
            does.join(" and ")
        )
    }

    /// "31 take-offs give no wind directions", "1 take-off gives no
    /// height".
    fn unsaid_in_words(&self, count: usize) -> String {
        let what = match (self.wind, self.height) {
            (Some(_), Some(_)) => "no wind directions or no height",
            (Some(_), None) => "no wind directions",
            _ => "no height",
        };
        if count == 1 {
            format!("1 take-off gives {what}")
        } else {
            format!("{count} take-offs give {what}")
        }
    }

    /// The filter in a line: "Take-offs for a wind from NW, from 1500 m
    /// up", "All sites, take-offs from 1500 m up", "Landings".
    fn in_words(&self, shown: SitesShown) -> String {
        let asked: Vec<String> = self
            .wind
            .map(|from_deg| format!("for {}", self.wind_in_words(from_deg)))
            .into_iter()
            .chain(
                self.height
                    .map(|from_m| format!("from {} up", metres(from_m))),
            )
            .collect();
        let asked = asked.join(", ");
        match (shown, asked.is_empty()) {
            (SitesShown::All, true) => "All sites".to_owned(),
            (SitesShown::All, false) => format!("All sites, take-offs {asked}"),
            (SitesShown::Takeoffs, true) => "Take-offs".to_owned(),
            (SitesShown::Takeoffs, false) => format!("Take-offs {asked}"),
            (SitesShown::Landings, _) => "Landings".to_owned(),
        }
    }
}

/// Whether the wind and the height are asked under a choice of sites. Under
/// "Landings" neither is: see the module's heading.
fn asks_takeoffs(shown: SitesShown) -> bool {
    shown != SitesShown::Landings
}

/// A height in whole metres: "1500 m".
fn metres(height_m: f64) -> String {
    format!("{height_m:.0} m")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sites::sites_map;

    /// Nine sites as a server would answer them, made up for the test: no
    /// recorded answer of one valley has take-offs with and without
    /// directions and heights side by side.
    const ANSWER: &str = r#"{
      "layers": [{
        "source": "osm-free-flying",
        "fetched_at": "2026-11-18T19:05:00Z",
        "as_of": "2026-11-18T19:05:00Z",
        "covers": [],
        "sites": [
          {"id": "osm-free-flying:node/1", "name": "West high", "position": {"lat": 47.1, "lon": 11.1},
           "elevation_m": 1800.0, "uses": ["takeoff"],
           "wind_directions": [{"from_deg": 270.0, "to_deg": 315.0}]},
          {"id": "osm-free-flying:node/2", "name": "West low", "position": {"lat": 47.2, "lon": 11.2},
           "elevation_m": 900.0, "uses": ["takeoff"],
           "wind_directions": [{"from_deg": 315.0, "to_deg": 315.0}]},
          {"id": "osm-free-flying:node/3", "name": "South", "position": {"lat": 47.3, "lon": 11.3},
           "elevation_m": 1500.0, "uses": ["takeoff"],
           "wind_directions": [{"from_deg": 180.0, "to_deg": 180.0}]},
          {"id": "osm-free-flying:node/4", "name": "West, no height", "position": {"lat": 47.4, "lon": 11.4},
           "uses": ["takeoff"], "wind_directions": [{"from_deg": 292.5, "to_deg": 337.5}]},
          {"id": "osm-free-flying:node/5", "name": "High, no wind", "position": {"lat": 47.5, "lon": 11.5},
           "elevation_m": 2100.0, "uses": ["takeoff"]},
          {"id": "osm-free-flying:node/6", "position": {"lat": 47.6, "lon": 11.6}, "uses": ["takeoff"]},
          {"id": "osm-free-flying:node/7", "name": "Top and bottom", "position": {"lat": 47.7, "lon": 11.7},
           "elevation_m": 700.0, "uses": ["takeoff", "landing"],
           "wind_directions": [{"from_deg": 90.0, "to_deg": 90.0}]},
          {"id": "osm-free-flying:node/8", "name": "Meadow", "position": {"lat": 47.8, "lon": 11.8},
           "uses": ["landing"]},
          {"id": "osm-free-flying:node/9", "name": "Slope", "position": {"lat": 47.9, "lon": 11.9},
           "uses": ["training"], "wind_directions": [{"from_deg": 0.0, "to_deg": 0.0}]}
        ]
      }],
      "attributions": []
    }"#;

    fn sites() -> Vec<SiteView> {
        sites_map(47.0, 11.0, 48.0, 12.0, 200, ANSWER.as_bytes().to_vec())
            .unwrap()
            .sites
    }

    fn nothing() -> SitesFilter {
        SitesFilter {
            shown: SitesShown::All,
            wind_from_deg: None,
            wind_is: SitesWindIs::Chosen,
            takeoffs_from_m: None,
        }
    }

    /// The numbers the sites' identifiers end in.
    fn numbers(sites: &[SiteView]) -> Vec<u32> {
        sites
            .iter()
            .map(|site| site.id.rsplit('/').next().unwrap().parse().unwrap())
            .collect()
    }

    #[test]
    fn a_filter_that_asks_nothing_shows_every_site() {
        let left = filter_sites(sites(), nothing());
        assert_eq!(numbers(&left.fitting), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(left.unsaid.is_empty());
        assert_eq!(left.words, "9 sites");
        assert_eq!(
            (left.unsaid_title.as_str(), left.unsaid_note.as_str()),
            ("", "")
        );
        assert_eq!(left.filter_words, "All sites");
        assert!(!left.is_on);
    }

    #[test]
    fn sites_are_shown_by_what_they_are_used_for() {
        let takeoffs = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                ..nothing()
            },
        );
        // The site that is a take-off and a landing is among both.
        assert_eq!(numbers(&takeoffs.fitting), [1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(takeoffs.words, "7 take-offs");
        assert_eq!(takeoffs.filter_words, "Take-offs");
        assert!(takeoffs.is_on);

        let landings = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Landings,
                ..nothing()
            },
        );
        assert_eq!(numbers(&landings.fitting), [7, 8]);
        assert_eq!(landings.words, "2 landings");
        assert_eq!(landings.filter_words, "Landings");

        // One, and none.
        let one = filter_sites(
            sites().into_iter().skip(7).collect(),
            SitesFilter {
                shown: SitesShown::Landings,
                ..nothing()
            },
        );
        assert_eq!(one.words, "1 landing");
        let none = filter_sites(
            sites().into_iter().skip(7).collect(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                ..nothing()
            },
        );
        assert!(none.fitting.is_empty());
        assert_eq!(none.words, "No take-offs here");
        assert_eq!(filter_sites(Vec::new(), nothing()).words, "No sites here");
    }

    #[test]
    fn take_offs_are_asked_whether_they_work_in_a_wind() {
        let north_west = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                wind_from_deg: Some(315.0),
                ..nothing()
            },
        );
        // West to north-west, north-west alone, and a span around it;
        // south and east do not fit.
        assert_eq!(numbers(&north_west.fitting), [1, 2, 4]);
        // Two name no direction: neither a fit nor left out.
        assert_eq!(numbers(&north_west.unsaid), [5, 6]);
        assert_eq!(north_west.words, "3 of 7 take-offs fit a wind from NW");
        assert_eq!(
            north_west.unsaid_title,
            "2 take-offs give no wind directions"
        );
        assert!(
            north_west.unsaid_note.contains("shown apart, not left out"),
            "{}",
            north_west.unsaid_note
        );
        assert_eq!(north_west.filter_words, "Take-offs for a wind from NW");
        assert!(north_west.is_on);

        // The rule is the one the forecast is judged by: half a compass
        // point to either side of what the source names, and no more.
        let fits = |from_deg: f64| {
            numbers(
                &filter_sites(
                    sites(),
                    SitesFilter {
                        shown: SitesShown::Takeoffs,
                        wind_from_deg: Some(from_deg),
                        ..nothing()
                    },
                )
                .fitting,
            )
        };
        assert_eq!(fits(337.5), [4]);
        assert_eq!(fits(325.0), [1, 2, 4]);
        assert_eq!(fits(180.0), [3]);
        assert!(fits(45.0).is_empty());
    }

    #[test]
    fn the_wind_now_is_worded_as_what_it_is() {
        let ask = |wind_is| {
            filter_sites(
                sites(),
                SitesFilter {
                    shown: SitesShown::Takeoffs,
                    wind_from_deg: Some(184.0),
                    wind_is,
                    takeoffs_from_m: None,
                },
            )
        };
        let forecast = ask(SitesWindIs::ForecastNow);
        assert_eq!(numbers(&forecast.fitting), [3]);
        assert_eq!(
            forecast.words,
            "1 of 7 take-offs fits the S wind forecast now"
        );
        assert_eq!(
            forecast.filter_words,
            "Take-offs for the S wind forecast now"
        );
        let measured = ask(SitesWindIs::MeasuredNow);
        assert_eq!(
            measured.words,
            "1 of 7 take-offs fits the S wind measured now"
        );
        assert_eq!(
            ask(SitesWindIs::Chosen).words,
            "1 of 7 take-offs fits a wind from S"
        );
    }

    #[test]
    fn take_offs_are_asked_how_high_they_lie() {
        let high = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                takeoffs_from_m: Some(1500.0),
                ..nothing()
            },
        );
        // At the height counts as from it up.
        assert_eq!(numbers(&high.fitting), [1, 3, 5]);
        assert_eq!(numbers(&high.unsaid), [4, 6]);
        assert_eq!(high.words, "3 of 7 take-offs lie at 1500 m or higher");
        assert_eq!(high.unsaid_title, "2 take-offs give no height");
        assert_eq!(high.filter_words, "Take-offs from 1500 m up");

        let highest = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                takeoffs_from_m: Some(2000.0),
                ..nothing()
            },
        );
        assert_eq!(highest.words, "1 of 7 take-offs lies at 2000 m or higher");
    }

    #[test]
    fn a_wind_and_a_height_are_asked_together() {
        let both = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                wind_from_deg: Some(315.0),
                wind_is: SitesWindIs::Chosen,
                takeoffs_from_m: Some(1200.0),
            },
        );
        // Only the first is said to fit the wind and to lie high enough.
        assert_eq!(numbers(&both.fitting), [1]);
        // The second is too low and the third faces south: a "no" to one
        // question leaves a take-off out whatever it says to the other.
        // The fourth fits the wind and gives no height, the fifth is high
        // and gives no directions, the sixth gives neither.
        assert_eq!(numbers(&both.unsaid), [4, 5, 6]);
        assert_eq!(
            both.words,
            "1 of 7 take-offs fits a wind from NW and lies at 1200 m or higher"
        );
        assert_eq!(
            both.unsaid_title,
            "3 take-offs give no wind directions or no height"
        );
        assert_eq!(
            both.filter_words,
            "Take-offs for a wind from NW, from 1200 m up"
        );

        let one_left = filter_sites(
            sites().into_iter().skip(5).take(1).collect(),
            SitesFilter {
                shown: SitesShown::All,
                wind_from_deg: Some(315.0),
                wind_is: SitesWindIs::Chosen,
                takeoffs_from_m: None,
            },
        );
        assert_eq!(one_left.words, "0 of 1 take-off fit a wind from NW");
        assert_eq!(one_left.unsaid_title, "1 take-off gives no wind directions");
    }

    #[test]
    fn the_wind_and_the_height_are_asked_of_take_offs_only() {
        let all = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::All,
                wind_from_deg: Some(315.0),
                wind_is: SitesWindIs::Chosen,
                takeoffs_from_m: Some(1200.0),
            },
        );
        // The landing and the training slope stay, though the slope names
        // north alone; the take-off that is also a landing faces east and
        // is left out.
        assert_eq!(numbers(&all.fitting), [1, 8, 9]);
        assert_eq!(numbers(&all.unsaid), [4, 5, 6]);
        assert_eq!(
            all.words,
            "1 of 7 take-offs fits a wind from NW and lies at 1200 m or higher"
        );
        assert_eq!(
            all.filter_words,
            "All sites, take-offs for a wind from NW, from 1200 m up"
        );
        assert!(all.is_on);

        // Under "Landings" they are not asked at all: the take-off that is
        // also a landing stays, facing east and lying low.
        let landings = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Landings,
                wind_from_deg: Some(315.0),
                wind_is: SitesWindIs::Chosen,
                takeoffs_from_m: Some(1200.0),
            },
        );
        assert_eq!(numbers(&landings.fitting), [7, 8]);
        assert!(landings.unsaid.is_empty());
        assert_eq!(landings.words, "2 landings");
        assert_eq!(landings.filter_words, "Landings");
    }

    #[test]
    fn a_wind_or_a_height_that_is_no_number_asks_nothing() {
        let left = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::All,
                wind_from_deg: Some(f64::NAN),
                wind_is: SitesWindIs::ForecastNow,
                takeoffs_from_m: Some(f64::INFINITY),
            },
        );
        assert_eq!(left.fitting.len(), 9);
        assert_eq!(left.words, "9 sites");
        assert!(!left.is_on);
    }

    #[test]
    fn the_choices_are_offered_with_their_words() {
        let choices = sites_filter_choices();
        let shown: Vec<(SitesShown, &str, bool)> = choices
            .shown
            .iter()
            .map(|choice| (choice.shown, choice.words.as_str(), choice.asks_takeoffs))
            .collect();
        // Under "Landings" neither a wind nor a height is asked.
        assert_eq!(
            shown,
            [
                (SitesShown::All, "All sites", true),
                (SitesShown::Takeoffs, "Take-offs", true),
                (SitesShown::Landings, "Landings", false),
            ]
        );
        let winds: Vec<&str> = choices
            .winds
            .iter()
            .map(|wind| wind.words.as_str())
            .collect();
        assert_eq!(
            winds,
            [
                "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W",
                "WNW", "NW", "NNW"
            ]
        );
        assert_eq!(choices.winds[14].from_deg, 315.0);
        let now: Vec<(SitesWindIs, &str)> = choices
            .winds_now
            .iter()
            .map(|choice| (choice.wind_is, choice.words.as_str()))
            .collect();
        assert_eq!(
            now,
            [
                (SitesWindIs::ForecastNow, "Forecast now"),
                (SitesWindIs::MeasuredNow, "Measured now"),
            ]
        );
        let heights: Vec<(Option<f64>, &str)> = choices
            .heights
            .iter()
            .map(|height| (height.from_m, height.words.as_str()))
            .collect();
        assert_eq!(
            heights,
            [
                (None, "Any"),
                (Some(1000.0), "1000 m"),
                (Some(1500.0), "1500 m"),
                (Some(2000.0), "2000 m"),
            ]
        );
        assert_eq!(
            (
                choices.shown_title.as_str(),
                choices.wind_title.as_str(),
                choices.any_wind.as_str(),
                choices.height_title.as_str()
            ),
            ("Show", "Take-offs for a wind from", "Any", "Take-offs from")
        );
        for said in [
            "take-offs only",
            "says nothing about the wind's strength",
            "shown apart",
            "not permission to fly",
        ] {
            assert!(choices.note.contains(said), "{said}: {}", choices.note);
        }

        // Every direction offered finds the take-offs that name it.
        let west = choices.winds.iter().find(|wind| wind.words == "W").unwrap();
        let found = filter_sites(
            sites(),
            SitesFilter {
                shown: SitesShown::Takeoffs,
                wind_from_deg: Some(west.from_deg),
                ..nothing()
            },
        );
        assert_eq!(numbers(&found.fitting), [1]);
    }
}
