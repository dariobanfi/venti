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
    /// Its line: "From flights their pilots shared, this month." The layer
    /// asks for the month it is.
    #[uniffi(default = "")]
    pub flown_thermals_note: String,
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
        flown_thermals_note: "From flights their pilots shared, this month.".to_owned(),
        cockpit_climbs: "Climbs of others in the cockpit".to_owned(),
        cockpit_climbs_note: format!(
            "Where others climbed near you in the last {LIVE_CLIMB_MINUTES} minutes, on the \
             cockpit's map: a hint, since a climb may be gone."
        ),
        credits_title: "Credits".to_owned(),
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
}

/// The words of the route planner's menu.
#[uniffi::export]
pub fn planner_words() -> PlannerWords {
    PlannerWords {
        export_gpx: "Export GPX".to_owned(),
        import_gpx: "Import GPX".to_owned(),
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
}
