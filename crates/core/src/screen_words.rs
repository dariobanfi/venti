//! The words of screens that are the same in the three apps and that no
//! reading of the core carries: titles, row names, notes and menu entries.
//! One record per screen, so that a shell writes none of them itself.
//!
//! The words of what a call reads stay with that call; here are only the
//! words around it.

use venti_api_types::v1::LIVE_CLIMB_MINUTES;

use crate::{GliderClassKind, InstrumentView, VarioSourceKind};

// --- The cockpit -------------------------------------------------------------

/// Where the cockpit's climb comes from, for its top bar.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CockpitSourceWords {
    /// In full: "Recorded data", "Phone barometer", "Satellite vario",
    /// "No vario yet", and for an external instrument its name with its
    /// battery, "XC Tracer 80%", or "XC Tracer silent".
    #[uniffi(default = "")]
    pub words: String,
    /// Short, for a narrow phone beside "End": "Recorded", "Barometer",
    /// "Satellite", "No vario", "Vario 80%", "Vario", "Vario silent".
    #[uniffi(default = "")]
    pub short: String,
}

/// Where the cockpit's climb comes from, in words: the external
/// instrument's state ([`crate::CockpitView::instrument`]) first, under
/// `vario_name`, the name the pilot knows it by, or "Vario" without one;
/// in a replay (`is_replay`) the recording, since the pressure is not this
/// phone's; else [`crate::CockpitView::vario_source`].
#[uniffi::export]
pub fn cockpit_source_words(
    vario_source: Option<VarioSourceKind>,
    instrument: Option<InstrumentView>,
    vario_name: Option<String>,
    is_replay: bool,
) -> CockpitSourceWords {
    let both = |words: &str, short: &str| CockpitSourceWords {
        words: words.to_owned(),
        short: short.to_owned(),
    };
    if let Some(instrument) = instrument {
        let name = vario_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Vario".to_owned());
        if !instrument.talking {
            return CockpitSourceWords {
                words: format!("{name} silent"),
                short: "Vario silent".to_owned(),
            };
        }
        return match instrument.battery_percent {
            Some(percent) => CockpitSourceWords {
                words: format!("{name} {percent}%"),
                short: format!("Vario {percent}%"),
            },
            None => CockpitSourceWords {
                words: name,
                short: "Vario".to_owned(),
            },
        };
    }
    if is_replay {
        return both("Recorded data", "Recorded");
    }
    match vario_source {
        Some(VarioSourceKind::Pressure) => both("Phone barometer", "Barometer"),
        Some(VarioSourceKind::Satellite) => both("Satellite vario", "Satellite"),
        Some(VarioSourceKind::Instrument) | None => both("No vario yet", "No vario"),
    }
}

/// The words of the cockpit's details sheet, and the cockpit's own that
/// no reading carries.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CockpitDetailsWords {
    /// The sheet's title: "Details".
    #[uniffi(default = "")]
    pub title: String,
    /// The row of [`crate::CockpitView::airspeed_words`]: "Airspeed".
    #[uniffi(default = "")]
    pub airspeed: String,
    /// The row of [`crate::CockpitView::heading_words`]: "Heading".
    #[uniffi(default = "")]
    pub heading: String,
    /// The row of [`crate::CockpitView::altitude_source_words`]:
    /// "Altitude".
    #[uniffi(default = "")]
    pub altitude: String,
    /// The row of [`crate::CockpitView::qnh_words`]: "QNH".
    #[uniffi(default = "")]
    pub qnh: String,
    /// The row of [`crate::CockpitView::netto_words`]: "Air".
    #[uniffi(default = "")]
    pub air: String,
    /// The row of [`crate::CockpitView::speed_to_fly`]: "Speed to fly".
    #[uniffi(default = "")]
    pub speed_to_fly: String,
    /// In place of the rows while there is none: "Nothing to show yet."
    #[uniffi(default = "")]
    pub none_yet: String,
    /// Under the rows: what the estimates are made from.
    #[uniffi(default = "")]
    pub note: String,
    /// The accessibility label of the reading of
    /// [`crate::CockpitView::nearest_landing`]: "Nearest landing".
    #[uniffi(default = "")]
    pub nearest_landing: String,
}

/// The words of the cockpit's details sheet. A row is shown only when its
/// reading is there, in this order.
#[uniffi::export]
pub fn cockpit_details_words() -> CockpitDetailsWords {
    CockpitDetailsWords {
        title: "Details".to_owned(),
        airspeed: "Airspeed".to_owned(),
        heading: "Heading".to_owned(),
        altitude: "Altitude".to_owned(),
        qnh: "QNH".to_owned(),
        air: "Air".to_owned(),
        speed_to_fly: "Speed to fly".to_owned(),
        none_yet: "Nothing to show yet.".to_owned(),
        note: "Airspeed and heading are estimates: your way over the ground less the wind \
               last measured."
            .to_owned(),
        nearest_landing: "Nearest landing".to_owned(),
    }
}

// --- The glider's settings ---------------------------------------------------

/// The words of the settings' section about the glider.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct GliderWords {
    /// The section's heading: "Glider".
    #[uniffi(default = "")]
    pub title: String,
    /// The row of the class, a menu of [`glider_class_name`]: "Glider
    /// class".
    #[uniffi(default = "")]
    pub class_title: String,
    /// The switch of [`crate::FlightSession::set_never_land`]: "Never land
    /// (hike and fly)".
    #[uniffi(default = "")]
    pub never_land: String,
    /// Under the section: what a class changes and what never landing
    /// does.
    #[uniffi(default = "")]
    pub footer: String,
}

/// The words of the settings' section about the glider.
#[uniffi::export]
pub fn glider_words() -> GliderWords {
    GliderWords {
        title: "Glider".to_owned(),
        class_title: "Glider class".to_owned(),
        never_land: "Never land (hike and fly)".to_owned(),
        footer: "With a glider class the cockpit also says what the air itself does on a glide \
                 and when to speed up, and reckons your arrivals with the class's polar, whose \
                 numbers are typical of the class, not your glider's. With \"Never land\" a \
                 flight is not ended by itself: landing, walking on and taking off again stay \
                 one recording, which you end by hand."
            .to_owned(),
    }
}

/// A glider class as the menu names it: "None" for none, "EN A", "EN B",
/// "EN C", "EN D", "CCC".
#[uniffi::export]
pub fn glider_class_name(class: Option<GliderClassKind>) -> String {
    match class {
        None => "None",
        Some(GliderClassKind::A) => "EN A",
        Some(GliderClassKind::B) => "EN B",
        Some(GliderClassKind::C) => "EN C",
        Some(GliderClassKind::D) => "EN D",
        Some(GliderClassKind::Ccc) => "CCC",
    }
    .to_owned()
}

// --- A site's and a station's sheet ------------------------------------------

/// The words of a site's sheet that no reading carries.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct SiteSheetWords {
    /// The heading over the notes on how the forecast is read: "How this
    /// is read".
    #[uniffi(default = "")]
    pub how_read_title: String,
}

/// The words of a site's sheet that no reading carries.
#[uniffi::export]
pub fn site_sheet_words() -> SiteSheetWords {
    SiteSheetWords {
        how_read_title: "How this is read".to_owned(),
    }
}

/// The words of a station's sheet that no reading carries.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct StationSheetWords {
    /// The heading over what else is known of the air at the station:
    /// "More about the air here".
    #[uniffi(default = "")]
    pub more_air_title: String,
}

/// The words of a station's sheet that no reading carries.
#[uniffi::export]
pub fn station_sheet_words() -> StationSheetWords {
    StationSheetWords {
        more_air_title: "More about the air here".to_owned(),
    }
}

// --- The map's layers --------------------------------------------------------

/// The words of the map's sheet of layers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct MapLayersWords {
    /// The sheet's title: "Layers".
    #[uniffi(default = "")]
    pub title: String,
    /// The layer of [`crate::live_climbs`]: "Climbs now".
    #[uniffi(default = "")]
    pub climbs_now: String,
    /// Its line: "Where gliders circled and climbed in the last 10
    /// minutes."
    #[uniffi(default = "")]
    pub climbs_now_note: String,
    /// The layer of [`crate::flown_thermals`]: "Where thermals have been".
    #[uniffi(default = "")]
    pub flown_thermals: String,
    /// Its line: "From flights their pilots shared, in every month of the
    /// year." The layer asks for no month, so that it is not empty in
    /// winter.
    #[uniffi(default = "")]
    pub flown_thermals_note: String,
    /// When the map shows too wide a part for the climbs: "Zoom in to see
    /// where pilots climb now."
    #[uniffi(default = "")]
    pub climbs_too_wide: String,
    /// When no cell of flown thermals lies in the part shown: "No climbs
    /// from shared flights here yet."
    #[uniffi(default = "")]
    pub flown_empty: String,
    /// The heading over the layers' switches: "On the map".
    #[uniffi(default = "")]
    pub on_the_map: String,
    /// The heading over the rows that open a list or the plans: "Lists and
    /// plans".
    #[uniffi(default = "")]
    pub lists_title: String,
    /// The day alarm's kind beside the alarm for pilots in the air:
    /// "Flying day".
    #[uniffi(default = "")]
    pub alarm_day_kind: String,
    /// The setting that draws others' climbs on the cockpit's map:
    /// "Climbs of others in the cockpit".
    #[uniffi(default = "")]
    pub cockpit_climbs: String,
    /// Its line.
    #[uniffi(default = "")]
    pub cockpit_climbs_note: String,
    /// The heading over the credits of what the map shows: "Credits".
    #[uniffi(default = "")]
    pub credits_title: String,
    /// The layer of the forecast's hours: "Forecast".
    #[uniffi(default = "")]
    pub forecast: String,
    /// The layer of the rain radar: "Radar".
    #[uniffi(default = "")]
    pub radar: String,
    /// The layer of the thermals: "Thermals".
    #[uniffi(default = "")]
    pub thermals: String,
    /// The layer of the wind stations: "Wind".
    #[uniffi(default = "")]
    pub wind: String,
    /// The layer of the airspace: "Airspace".
    #[uniffi(default = "")]
    pub airspace: String,
    /// The layer of the flying sites: "Sites".
    #[uniffi(default = "")]
    pub sites: String,
    /// The layer of the pilots in the air: "Pilots".
    #[uniffi(default = "")]
    pub pilots: String,
    /// The layer of the slopes' steepness: "Slopes".
    #[uniffi(default = "")]
    pub slopes: String,
    /// The layer of the cables: "Cables".
    #[uniffi(default = "")]
    pub cables: String,
    /// The aerial photographs in place of the base map: "Aerial view".
    #[uniffi(default = "")]
    pub aerial_view: String,
    /// The row that opens the list of stations: "Wind stations".
    #[uniffi(default = "")]
    pub wind_stations: String,
    /// The row that opens the list of sites: "Flying sites".
    #[uniffi(default = "")]
    pub flying_sites: String,
    /// The row that opens the sites' filter, with
    /// [`crate::SitesFiltered::filter_words`] as its value: "Filter the
    /// sites".
    #[uniffi(default = "")]
    pub filter_sites: String,
    /// The row that opens the list of who is in the air: "Pilots in the
    /// air".
    #[uniffi(default = "")]
    pub pilots_in_the_air: String,
    /// The row that opens the plans of who flies where: "Flying plans".
    #[uniffi(default = "")]
    pub flying_plans: String,
    /// The row that opens the route planner: "Plan a route". The row that
    /// adds a site is not here: its words depend on the account, and are
    /// [`crate::SiteAdding::entry_words`], shown when not empty.
    #[uniffi(default = "")]
    pub plan_route: String,
    /// The credit of the base map, under "Credits": "Base map: ©
    /// OpenStreetMap contributors. Tiles from OpenFreeMap, ©
    /// OpenMapTiles." The full wording with the licence is the page of
    /// data and licences.
    #[uniffi(default = "")]
    pub base_map_credit: String,
    /// The credit of the terrain beside it: "Terrain: © Mapterhorn."
    #[uniffi(default = "")]
    pub terrain_credit: String,
}

/// The words of the map's sheet of layers.
#[uniffi::export]
pub fn map_layers_words() -> MapLayersWords {
    MapLayersWords {
        title: "Layers".to_owned(),
        climbs_now: "Climbs now".to_owned(),
        climbs_now_note: format!(
            "Where gliders circled and climbed in the last {LIVE_CLIMB_MINUTES} minutes."
        ),
        flown_thermals: "Where thermals have been".to_owned(),
        flown_thermals_note: "From flights their pilots shared, in every month of the year."
            .to_owned(),
        climbs_too_wide: "Zoom in to see where pilots climb now.".to_owned(),
        flown_empty: "No climbs from shared flights here yet.".to_owned(),
        on_the_map: "On the map".to_owned(),
        lists_title: "Lists and plans".to_owned(),
        alarm_day_kind: "Flying day".to_owned(),
        cockpit_climbs: "Climbs of others in the cockpit".to_owned(),
        cockpit_climbs_note: format!(
            "Where others climbed near you in the last {LIVE_CLIMB_MINUTES} minutes, on the \
             cockpit's map: a hint, since a climb may be gone."
        ),
        credits_title: "Credits".to_owned(),
        forecast: "Forecast".to_owned(),
        radar: "Radar".to_owned(),
        thermals: "Thermals".to_owned(),
        wind: "Wind".to_owned(),
        airspace: "Airspace".to_owned(),
        sites: "Sites".to_owned(),
        pilots: "Pilots".to_owned(),
        slopes: "Slopes".to_owned(),
        cables: "Cables".to_owned(),
        aerial_view: "Aerial view".to_owned(),
        wind_stations: "Wind stations".to_owned(),
        flying_sites: "Flying sites".to_owned(),
        filter_sites: "Filter the sites".to_owned(),
        pilots_in_the_air: "Pilots in the air".to_owned(),
        flying_plans: "Flying plans".to_owned(),
        plan_route: "Plan a route".to_owned(),
        base_map_credit:
            "Base map: © OpenStreetMap contributors. Tiles from OpenFreeMap, © OpenMapTiles."
                .to_owned(),
        terrain_credit: "Terrain: © Mapterhorn.".to_owned(),
    }
}

// --- Today -------------------------------------------------------------------

/// The words of the Today tab that no reading carries.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TodayWords {
    /// Under the strip of hours: "Wind in km/h at 10 m. A tall bar is a
    /// flyable hour."
    #[uniffi(default = "")]
    pub hour_strip_note: String,
    /// The same where the strip shows the climb too: "Wind in km/h at 10 m
    /// and the climb in m/s. A tall bar is a flyable hour."
    #[uniffi(default = "")]
    pub hour_strip_note_with_thermals: String,
}

/// The words of the Today tab that no reading carries.
#[uniffi::export]
pub fn today_words() -> TodayWords {
    TodayWords {
        hour_strip_note: "Wind in km/h at 10 m. A tall bar is a flyable hour.".to_owned(),
        hour_strip_note_with_thermals:
            "Wind in km/h at 10 m and the climb in m/s. A tall bar is a flyable hour.".to_owned(),
    }
}

// --- The planner -------------------------------------------------------------

/// The words of the route planner's menu.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PlannerWords {
    /// The entry of [`crate::route_to_gpx`]: "Export GPX".
    #[uniffi(default = "")]
    pub export_gpx: String,
    /// The entry of [`crate::route_from_gpx`]: "Import GPX".
    #[uniffi(default = "")]
    pub import_gpx: String,
    /// Asked before a file's route takes the place of one already drawn:
    /// "Replace the route you have drawn with the file’s?"
    #[uniffi(default = "")]
    pub replace_question: String,
    /// Its yes: "Replace". The no is [`CommonWords::cancel`].
    #[uniffi(default = "")]
    pub replace: String,
}

/// The words of the route planner's menu.
#[uniffi::export]
pub fn planner_words() -> PlannerWords {
    PlannerWords {
        export_gpx: "Export GPX".to_owned(),
        import_gpx: "Import GPX".to_owned(),
        replace_question: "Replace the route you have drawn with the file’s?".to_owned(),
        replace: "Replace".to_owned(),
    }
}

// --- Common controls ---------------------------------------------------------

/// The words of the controls every screen has: the same in the three apps.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CommonWords {
    /// The button that closes a sheet having kept what was chosen:
    /// "Done".
    #[uniffi(default = "")]
    pub done: String,
    /// The way back to the screen before: "Back".
    #[uniffi(default = "")]
    pub back: String,
    /// The menu of further actions: "More".
    #[uniffi(default = "")]
    pub more: String,
    /// The button that takes note of a message: "OK".
    #[uniffi(default = "")]
    pub ok: String,
    /// The button that leaves a question or a form doing nothing:
    /// "Cancel".
    #[uniffi(default = "")]
    pub cancel: String,
    /// The button that asks again for what could not be loaded: "Try
    /// again".
    #[uniffi(default = "")]
    pub try_again: String,
    /// The button that closes a sheet or a message: "Close".
    #[uniffi(default = "")]
    pub close: String,
}

/// The words of the controls every screen has.
#[uniffi::export]
pub fn common_words() -> CommonWords {
    CommonWords {
        done: "Done".to_owned(),
        back: "Back".to_owned(),
        more: "More".to_owned(),
        ok: "OK".to_owned(),
        cancel: "Cancel".to_owned(),
        try_again: "Try again".to_owned(),
        close: "Close".to_owned(),
    }
}

// --- Other pages -------------------------------------------------------------

/// The words of pages that have only a few of their own: everyone's
/// flights, a forecast's notes, the settings and the alarms.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct PagesWords {
    /// The title of the page of everyone's public flights: "Everyone's
    /// flights".
    #[uniffi(default = "")]
    pub everyones_flights: String,
    /// The control over that page's choice of dates: "Filter".
    #[uniffi(default = "")]
    pub filter: String,
    /// Its choice of no date: "Any day".
    #[uniffi(default = "")]
    pub any_day: String,
    /// The heading over what a forecast is made from and how it is read:
    /// "About this forecast".
    #[uniffi(default = "")]
    pub about_forecast: String,
    /// The settings' row that opens the glider's and the cockpit's
    /// settings: "Glider and cockpit".
    #[uniffi(default = "")]
    pub glider_and_cockpit: String,
    /// The accessible name of the choice between the alarm's two kinds:
    /// "Kind".
    #[uniffi(default = "")]
    pub alarm_kind: String,
}

/// The words of pages that have only a few of their own.
#[uniffi::export]
pub fn pages_words() -> PagesWords {
    PagesWords {
        everyones_flights: "Everyone’s flights".to_owned(),
        filter: "Filter".to_owned(),
        any_day: "Any day".to_owned(),
        about_forecast: "About this forecast".to_owned(),
        glider_and_cockpit: "Glider and cockpit".to_owned(),
        alarm_kind: "Kind".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instrument(talking: bool, battery_percent: Option<u8>) -> Option<InstrumentView> {
        Some(InstrumentView {
            talking,
            battery_percent,
            sentences: 10,
            damaged: 0,
        })
    }

    #[test]
    fn the_cockpit_says_where_its_climb_comes_from_in_full_and_short() {
        let said = |source, instrument, name: Option<&str>, replay| {
            let words = cockpit_source_words(source, instrument, name.map(str::to_owned), replay);
            (words.words, words.short)
        };
        let pair = |words: &str, short: &str| (words.to_owned(), short.to_owned());
        assert_eq!(
            said(Some(VarioSourceKind::Pressure), None, None, true),
            pair("Recorded data", "Recorded")
        );
        assert_eq!(
            said(Some(VarioSourceKind::Pressure), None, None, false),
            pair("Phone barometer", "Barometer")
        );
        assert_eq!(
            said(Some(VarioSourceKind::Satellite), None, None, false),
            pair("Satellite vario", "Satellite")
        );
        assert_eq!(
            said(None, None, None, false),
            pair("No vario yet", "No vario")
        );
        assert_eq!(
            said(None, instrument(true, Some(80)), Some("XC Tracer"), true),
            pair("XC Tracer 80%", "Vario 80%")
        );
        assert_eq!(
            said(None, instrument(true, None), None, false),
            pair("Vario", "Vario")
        );
        assert_eq!(
            said(None, instrument(false, Some(80)), Some(" "), false),
            pair("Vario silent", "Vario silent")
        );
    }

    #[test]
    fn the_classes_have_their_names() {
        assert_eq!(glider_class_name(None), "None");
        assert_eq!(glider_class_name(Some(GliderClassKind::A)), "EN A");
        assert_eq!(glider_class_name(Some(GliderClassKind::Ccc)), "CCC");
        assert_eq!(glider_words().footer.matches(". ").count() + 1, 2);
    }

    #[test]
    fn the_layers_name_the_minutes_the_server_lists() {
        assert_eq!(
            map_layers_words().climbs_now_note,
            "Where gliders circled and climbed in the last 10 minutes."
        );
    }

    #[test]
    fn the_layers_sheet_names_its_layers_rows_and_the_map_s_credits() {
        let words = map_layers_words();
        let layers = [
            &words.forecast,
            &words.radar,
            &words.thermals,
            &words.wind,
            &words.airspace,
            &words.sites,
            &words.pilots,
            &words.slopes,
            &words.cables,
            &words.aerial_view,
        ];
        assert_eq!(
            layers.map(String::as_str),
            [
                "Forecast",
                "Radar",
                "Thermals",
                "Wind",
                "Airspace",
                "Sites",
                "Pilots",
                "Slopes",
                "Cables",
                "Aerial view"
            ]
        );
        assert_eq!(words.plan_route, "Plan a route");
        assert_eq!(words.filter_sites, "Filter the sites");
        // The two lines the apps wrote, word for word.
        assert_eq!(
            words.base_map_credit,
            "Base map: © OpenStreetMap contributors. Tiles from OpenFreeMap, © OpenMapTiles."
        );
        assert_eq!(words.terrain_credit, "Terrain: © Mapterhorn.");
    }

    #[test]
    fn the_common_controls_and_the_small_pages_have_their_words() {
        let common = common_words();
        assert_eq!(
            [
                common.done,
                common.back,
                common.more,
                common.ok,
                common.cancel,
                common.try_again,
                common.close
            ],
            ["Done", "Back", "More", "OK", "Cancel", "Try again", "Close"]
        );
        let pages = pages_words();
        assert_eq!(pages.everyones_flights, "Everyone’s flights");
        assert_eq!(
            (pages.filter.as_str(), pages.any_day.as_str()),
            ("Filter", "Any day")
        );
        assert_eq!(pages.alarm_kind, "Kind");
    }
}
