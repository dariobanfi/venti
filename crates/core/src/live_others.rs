//! Others in the air: the aircraft that another network hears (today the
//! Open Glider Network: FLARM, FANET and others), read from the server's
//! answer, with words, and joined with Venti's own pilots for one layer of
//! the map without merging a record.
//!
//! The network's positions are under the ODbL and Venti's pilots' are
//! their own: the two never become one record. [`live_layer`] puts them
//! side by side, each list with its credit.

use venti_api_types::v1::{
    AircraftKind, LiveOthersRequest, LiveOthersResponse, MAX_LIVE_OTHERS_BOX_DEG, OtherAircraft,
};
use venti_domain::BoundingBox;

use crate::server::answer;
use crate::wind::{Credit, credit};
use crate::{Coordinate, CoreError, LiveGlider, LiveMap, live_age_in_words};

/// What kind of aircraft, as a map tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum OtherAircraftKind {
    /// A paraglider.
    Paraglider,
    /// A hang glider.
    HangGlider,
    /// A glider or motor glider.
    Glider,
    /// A tow plane.
    TowPlane,
    /// A helicopter.
    Helicopter,
    /// A skydiver or parachute.
    Parachute,
    /// Any other aeroplane: a drop plane, one with a piston or a jet
    /// engine.
    Aeroplane,
    /// A balloon or an airship.
    Balloon,
    /// A drone.
    Drone,
    /// An obstacle, or what the network does not name.
    Other,
}

/// An aircraft of another network, as a map shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct OtherAircraftView {
    /// A name for the aircraft that stays the same while the server runs:
    /// the key a map moves its mark by. It says nothing about the aircraft.
    pub id: String,
    /// What kind of aircraft.
    pub kind: OtherAircraftKind,
    /// The kind in words: "Paraglider", "Hang glider", "Glider".
    pub kind_words: String,
    /// Whether it is a paraglider or a hang glider: drawn first.
    pub is_free_flight: bool,
    /// Where it was last heard.
    pub position: Coordinate,
    /// Altitude, metres above sea level.
    pub altitude_m: Option<f64>,
    /// Climb, m/s.
    pub climb_ms: Option<f64>,
    /// Speed over the ground, m/s.
    pub ground_speed_ms: Option<f64>,
    /// Direction it moves in, degrees clockwise from north.
    pub course_deg: Option<f64>,
    /// How old the position was when the server answered, seconds.
    pub age_s: f64,
    /// The age in words: "Now", "40 s ago", "No position for 6 min".
    pub age_words: String,
    /// What the owner lets be shown of who it is: the registration or
    /// the competition number, and the model. `None` when nothing.
    pub name: Option<String>,
    /// "Paraglider at 2,140 m, climbing 1.8 m/s, 40 s ago".
    pub line: String,
    /// For the map, short: "Paraglider · 2140 m".
    pub label: String,
}

/// The aircraft of one network in a part of the map, with its credit.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveOthersNetwork {
    /// The network's source.
    pub source: String,
    /// Whether the server is connected to it now.
    pub connected: bool,
    /// Its aircraft: paragliders and hang gliders first.
    pub aircraft: Vec<OtherAircraftView>,
    /// Whether the part of the map holds more than one answer lists.
    pub truncated: bool,
    /// Whose data it is: shown with the map's credits, with its notice,
    /// while any of its aircraft is drawn.
    pub credit: Option<Credit>,
}

/// The aircraft other networks hear in a part of the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveOthers {
    /// One per network, as the server lists them.
    pub networks: Vec<LiveOthersNetwork>,
    /// For how many minutes after it was heard an aircraft is listed.
    pub listed_minutes: u32,
}

/// The map's layer of who is in the air: Venti's own pilots and the
/// aircraft of other networks, side by side and never one record.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct LiveLayer {
    /// Venti's pilots who show their flight to everyone.
    pub venti: Vec<LiveGlider>,
    /// The aircraft of each other network, each with its credit.
    pub others: Vec<LiveOthersNetwork>,
    /// Credits to show with the map while the layer is on: those of the
    /// networks with an aircraft in it.
    pub credits: Vec<Credit>,
    /// Whether either list holds less than the part of the map has.
    pub truncated: bool,
    /// How many are drawn, in words: "3 pilots of Venti, 5 others in the
    /// air". Empty when there is nobody.
    pub count_words: String,
}

/// The path and query to `GET` for the aircraft other networks hear in a
/// box. `None` for a box wider or taller than the server answers for: the
/// shell then asks for no others and may say
/// [`others_too_wide_words`]. Ask again every ten to fifteen seconds while
/// the map is looked at.
#[uniffi::export]
pub fn live_others_path(south: f64, west: f64, north: f64, east: f64) -> Option<String> {
    if east - west > MAX_LIVE_OTHERS_BOX_DEG || north - south > MAX_LIVE_OTHERS_BOX_DEG {
        return None;
    }
    let bbox = BoundingBox::new(
        south.clamp(-90.0, 90.0),
        west.clamp(-180.0, 180.0),
        north.clamp(-90.0, 90.0),
        east.clamp(-180.0, 180.0),
    )
    .ok()?;
    Some(LiveOthersRequest { bbox }.path_and_query())
}

/// What a map says where it asked for no others because it shows too much.
#[uniffi::export]
pub fn others_too_wide_words() -> String {
    "Zoom in to see the gliders of the Open Glider Network.".to_owned()
}

/// Reads the server's answer to [`live_others_path`].
#[uniffi::export]
pub fn live_others(status: u16, body: Vec<u8>) -> Result<LiveOthers, CoreError> {
    let response: LiveOthersResponse = answer(status, &body)?;
    let networks = response
        .layers
        .iter()
        .map(|layer| LiveOthersNetwork {
            source: layer.source.to_string(),
            connected: layer.connected,
            aircraft: layer.aircraft.iter().map(view).collect(),
            truncated: layer.truncated,
            credit: response
                .attributions
                .iter()
                .find(|attribution| attribution.source == layer.source)
                .map(credit),
        })
        .collect();
    Ok(LiveOthers {
        networks,
        listed_minutes: u32::try_from(response.listed_minutes).unwrap_or(10),
    })
}

/// Puts Venti's own pilots and the other networks' aircraft side by side
/// for one map layer. Nothing is merged: an aircraft of a network that is
/// also a pilot of Venti's is drawn twice, which is rare and true.
#[uniffi::export]
pub fn live_layer(venti: LiveMap, others: LiveOthers) -> LiveLayer {
    let credits = others
        .networks
        .iter()
        .filter(|network| !network.aircraft.is_empty())
        .filter_map(|network| network.credit.clone())
        .collect();
    let truncated = venti.truncated || others.networks.iter().any(|network| network.truncated);
    let pilots = venti.gliders.len();
    let aircraft: usize = others.networks.iter().map(|n| n.aircraft.len()).sum();
    let mut parts = Vec::new();
    match pilots {
        0 => {}
        1 => parts.push("1 pilot of Venti".to_owned()),
        n => parts.push(format!("{n} pilots of Venti")),
    }
    match aircraft {
        0 => {}
        1 => parts.push("1 other in the air".to_owned()),
        n => parts.push(format!("{n} others in the air")),
    }
    LiveLayer {
        venti: venti.gliders,
        others: others.networks,
        credits,
        truncated,
        count_words: parts.join(", "),
    }
}

fn kind(aircraft: AircraftKind) -> OtherAircraftKind {
    match aircraft {
        AircraftKind::Paraglider => OtherAircraftKind::Paraglider,
        AircraftKind::HangGlider => OtherAircraftKind::HangGlider,
        AircraftKind::Glider => OtherAircraftKind::Glider,
        AircraftKind::TowPlane => OtherAircraftKind::TowPlane,
        AircraftKind::Helicopter => OtherAircraftKind::Helicopter,
        AircraftKind::Parachute => OtherAircraftKind::Parachute,
        AircraftKind::DropPlane | AircraftKind::PoweredAircraft | AircraftKind::JetAircraft => {
            OtherAircraftKind::Aeroplane
        }
        AircraftKind::Balloon | AircraftKind::Airship => OtherAircraftKind::Balloon,
        AircraftKind::Drone => OtherAircraftKind::Drone,
        AircraftKind::Obstacle | AircraftKind::Unknown => OtherAircraftKind::Other,
    }
}

/// The kind of aircraft in words.
pub fn kind_words(aircraft: AircraftKind) -> &'static str {
    match aircraft {
        AircraftKind::Paraglider => "Paraglider",
        AircraftKind::HangGlider => "Hang glider",
        AircraftKind::Glider => "Glider",
        AircraftKind::TowPlane => "Tow plane",
        AircraftKind::Helicopter => "Helicopter",
        AircraftKind::Parachute => "Parachute",
        AircraftKind::DropPlane => "Skydivers' plane",
        AircraftKind::PoweredAircraft => "Aeroplane",
        AircraftKind::JetAircraft => "Jet",
        AircraftKind::Balloon => "Balloon",
        AircraftKind::Airship => "Airship",
        AircraftKind::Drone => "Drone",
        AircraftKind::Obstacle => "Obstacle",
        AircraftKind::Unknown => "Aircraft",
    }
}

/// A whole number with a comma between thousands: "2,140".
fn thousands(value: f64) -> String {
    let whole = value.round() as i64;
    let digits = whole.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if whole < 0 { format!("-{out}") } else { out }
}

fn climb_words(climb_ms: f64) -> String {
    if climb_ms >= 0.1 {
        format!("climbing {climb_ms:.1} m/s")
    } else if climb_ms <= -0.1 {
        format!("sinking {:.1} m/s", -climb_ms)
    } else {
        "level".to_owned()
    }
}

fn view(aircraft: &OtherAircraft) -> OtherAircraftView {
    let words = kind_words(aircraft.aircraft);
    let age_s = f64::from(aircraft.age_s);
    let age_words = live_age_in_words(age_s, false);
    let mut line = words.to_owned();
    if let Some(altitude) = aircraft.altitude_m {
        line.push_str(&format!(" at {} m", thousands(altitude)));
    }
    if let Some(climb) = aircraft.climb_ms {
        line.push_str(&format!(", {}", climb_words(climb)));
    }
    line.push_str(&format!(", {}", age_words.to_lowercase()));
    let label = match aircraft.altitude_m {
        Some(altitude) => format!("{words} · {} m", altitude.round()),
        None => words.to_owned(),
    };
    let who = [
        aircraft
            .registration
            .clone()
            .or_else(|| aircraft.competition_id.clone()),
        aircraft.model.clone(),
    ];
    let who: Vec<String> = who.into_iter().flatten().collect();
    OtherAircraftView {
        id: aircraft.id.clone(),
        kind: kind(aircraft.aircraft),
        kind_words: words.to_owned(),
        is_free_flight: aircraft.aircraft.is_free_flight(),
        position: Coordinate {
            lat: aircraft.position.lat(),
            lon: aircraft.position.lon(),
        },
        altitude_m: aircraft.altitude_m,
        climb_ms: aircraft.climb_ms,
        ground_speed_ms: aircraft.ground_speed_ms,
        course_deg: aircraft.course_deg,
        age_s,
        age_words,
        name: (!who.is_empty()).then(|| who.join(", ")),
        line,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &[u8] = include_bytes!("../../server/tests/golden/live_others.json");

    #[test]
    fn the_answer_reads_with_words_and_its_credit() {
        let others = live_others(200, ANSWER.to_vec()).unwrap();
        assert_eq!(others.listed_minutes, 10);
        let network = &others.networks[0];
        assert_eq!(network.source, "ogn");
        let first = &network.aircraft[0];
        assert_eq!(first.kind, OtherAircraftKind::Paraglider);
        assert_eq!(
            first.line,
            "Paraglider at 2,140 m, climbing 1.8 m/s, 20 s ago"
        );
        assert_eq!(first.label, "Paraglider · 2140 m");
        // The owner let it be identified by its competition number.
        assert_eq!(first.name.as_deref(), Some("WB, Paraglider"));
        // Not identified: no name.
        assert_eq!(network.aircraft[1].name, None);
        assert!(network.aircraft[1].line.contains("sinking 1.6 m/s"));
        let glider = network
            .aircraft
            .iter()
            .find(|a| a.kind == OtherAircraftKind::Glider)
            .unwrap();
        assert_eq!(glider.kind_words, "Glider");
        assert_eq!(glider.name.as_deref(), Some("D-0000, ASK 21"));
        let credit = network.credit.as_ref().unwrap();
        assert_eq!(credit.licence, "ODbL 1.0");
        assert!(credit.notice.is_some());
    }

    #[test]
    fn the_layer_keeps_the_two_lists_apart() {
        let others = live_others(200, ANSWER.to_vec()).unwrap();
        let venti = LiveMap {
            gliders: Vec::new(),
            truncated: false,
        };
        let layer = live_layer(venti, others);
        assert!(layer.venti.is_empty());
        assert_eq!(layer.others.len(), 1);
        assert_eq!(layer.credits.len(), 1);
        assert_eq!(layer.count_words, "7 others in the air");
    }

    #[test]
    fn a_box_too_wide_asks_for_no_others() {
        assert_eq!(
            live_others_path(47.55, 11.6, 47.85, 12.0).as_deref(),
            Some("/v1/live/others?bbox=11.6,47.55,12,47.85")
        );
        assert_eq!(live_others_path(40.0, 0.0, 50.0, 20.0), None);
    }

    #[test]
    fn numbers_read_as_people_write_them() {
        assert_eq!(thousands(2140.0), "2,140");
        assert_eq!(thousands(980.4), "980");
        assert_eq!(thousands(12345.0), "12,345");
        assert_eq!(climb_words(0.05), "level");
    }
}
