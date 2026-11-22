//! Cables: power lines and ropeways from the Venti server, as a map layer
//! and as the cockpit's warning.
//!
//! The app asks the server for the cables around where the pilot will fly
//! ([`cables_request`]) before the flight, as it asks for airspace, and
//! reads the answer into a [`CableSet`]. The set gives every map its
//! layer ([`CableSet::layer`]) and the cockpit what to watch for
//! ([`crate::FlightSession::use_cables`]). Each source stays a layer of
//! its own with its credit.
//!
//! **No cable on the map, and no warning, is no promise.** The data is
//! OpenStreetMap's and incomplete: many material ropeways and low lines
//! are not mapped, and few cables have a height. The layer's "i" says so
//! ([`CABLES_ARE_INCOMPLETE`]); a cable's card says what the cable is and
//! credits OpenStreetMap, nothing more (owner decision 19).

use std::sync::Arc;

use venti_api_types::v1::{CablesRequest, CablesResponse};
use venti_domain::{BoundingBox, Cable, CableKind as Kind};
use venti_flight::cables::{self, CableAhead, Wire};

use crate::server::answer;
use crate::wind::credit;
use crate::{Coordinate, CoreError, Credit, ServerRequest};

/// Half the height of the area asked for, in degrees: about 33 km to the
/// north and to the south. Smaller than airspace's: cables are many, and
/// an answer carries 5000 at most. *To verify* against the count of a
/// whole region.
const HALF_HEIGHT_DEG: f64 = 0.3;

/// Half its width, in degrees: about 33 km at the latitude of the Alps.
const HALF_WIDTH_DEG: f64 = 0.45;

/// What the cables' layer says behind its "i", never on a cable's card.
pub const CABLES_ARE_INCOMPLETE: &str = "Not every cable is mapped, and few have a height.";

/// What a cable is, for a shell to draw it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub enum CableKind {
    /// A power line on pylons.
    PowerLine,
    /// A low power line on poles.
    MinorPowerLine,
    /// A cable car.
    CableCar,
    /// A gondola lift.
    Gondola,
    /// A chair lift.
    ChairLift,
    /// A drag lift whose rope is mapped above the ground.
    DragLift,
    /// A ropeway for goods, such as a material ropeway to a hut.
    MaterialRopeway,
    /// A zip line.
    ZipLine,
    /// Another cable in the air.
    Other,
}

impl From<Kind> for CableKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::PowerLine => Self::PowerLine,
            Kind::MinorPowerLine => Self::MinorPowerLine,
            Kind::CableCar => Self::CableCar,
            Kind::Gondola => Self::Gondola,
            Kind::ChairLift => Self::ChairLift,
            Kind::DragLift => Self::DragLift,
            Kind::MaterialRopeway => Self::MaterialRopeway,
            Kind::ZipLine => Self::ZipLine,
            Kind::Other => Self::Other,
        }
    }
}

/// The kind's name in a sentence, with a capital.
fn kind_words(kind: Kind) -> &'static str {
    match kind {
        Kind::PowerLine => "Power line",
        Kind::MinorPowerLine => "Minor power line",
        Kind::CableCar => "Cable car",
        Kind::Gondola => "Gondola",
        Kind::ChairLift => "Chair lift",
        Kind::DragLift => "Drag lift",
        Kind::MaterialRopeway => "Material ropeway",
        Kind::ZipLine => "Zip line",
        Kind::Other => "Cable",
    }
}

/// One cable as a map draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CableLine {
    /// The cable's identifier, `source:key`.
    pub id: String,
    /// What it is.
    pub kind: CableKind,
    /// Its course, from pylon to pylon.
    pub points: Vec<Coordinate>,
    /// Whether its source maps its height. A map draws one without dashed.
    pub height_mapped: bool,
    /// What it is in words: "Power line, 110 kV", "Material ropeway,
    /// height not mapped", "Gondola, 35 m above the ground".
    pub words: String,
    /// Its name, if it has one.
    pub name: Option<String>,
}

/// The cables of an area as a map layer, with their credits.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(
    feature = "wasm",
    derive(serde::Serialize, serde::Deserialize, tsify::Tsify)
)]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct CablesMap {
    /// Every cable, source by source.
    pub lines: Vec<CableLine>,
    /// The credit of every source, to show beside the layer and on a
    /// cable's card: its line, linked to its address. The licence's full
    /// wording (the answer's `notice`) is not here: it belongs to the page
    /// of data and licences, not to a card.
    pub credits: Vec<Credit>,
    /// What to say behind the layer's "i": [`CABLES_ARE_INCOMPLETE`]. Not
    /// on a cable's card.
    pub limits: String,
}

/// The cables of an area, read from the server's answer.
#[derive(Debug, uniffi::Object)]
pub struct CableSet {
    cables: Vec<Cable>,
    credits: Vec<Credit>,
}

#[uniffi::export]
impl CableSet {
    /// Reads the server's answer to [`cables_request`].
    #[uniffi::constructor]
    pub fn from_answer(status: u16, body: Vec<u8>) -> Result<Arc<Self>, CoreError> {
        let response: CablesResponse = answer(status, &body)?;
        Ok(Arc::new(Self::of(&response)))
    }

    /// The cables as a map layer.
    pub fn layer(&self) -> CablesMap {
        CablesMap {
            lines: self.cables.iter().map(line).collect(),
            credits: self
                .credits
                .iter()
                .map(|credit| Credit {
                    notice: None,
                    ..credit.clone()
                })
                .collect(),
            limits: CABLES_ARE_INCOMPLETE.to_owned(),
        }
    }

    /// How many cables the set holds.
    pub fn count(&self) -> u32 {
        u32::try_from(self.cables.len()).unwrap_or(u32::MAX)
    }
}

impl CableSet {
    pub(crate) fn of(response: &CablesResponse) -> Self {
        Self {
            cables: response
                .layers
                .iter()
                .flat_map(|layer| layer.cables.iter().cloned())
                .collect(),
            credits: response.attributions.iter().map(credit).collect(),
        }
    }

    /// The kind of the cable at this place in the set.
    pub(crate) fn kind_of(&self, index: usize) -> CableKind {
        self.cables
            .get(index)
            .map_or(CableKind::Other, |cable| cable.kind.into())
    }

    /// The nearest cable ahead of the glider by the engine's rule, with
    /// its identifier and its warning in words.
    pub(crate) fn ahead(&self, glider: &cables::Glider) -> Option<(CableAhead, String, String)> {
        let wires: Vec<Wire<'_>> = self
            .cables
            .iter()
            .map(|cable| Wire {
                line: &cable.line,
                kind: cable.kind,
                height_m: cable.height_m,
            })
            .collect();
        let found = cables::cable_ahead(glider, &wires)?;
        let cable = &self.cables[found.index];
        Some((
            found,
            format!("{}:{}", cable.source, cable.local),
            warning_words(cable, &found),
        ))
    }
}

/// The request for the cables around a place: a box of some 65 by 65 km.
#[uniffi::export]
pub fn cables_request(lat: f64, lon: f64) -> Result<ServerRequest, CoreError> {
    let bbox = BoundingBox::new(
        (lat - HALF_HEIGHT_DEG).clamp(-90.0, 90.0),
        (lon - HALF_WIDTH_DEG).clamp(-180.0, 180.0),
        (lat + HALF_HEIGHT_DEG).clamp(-90.0, 90.0),
        (lon + HALF_WIDTH_DEG).clamp(-180.0, 180.0),
    )
    .map_err(|_| CoreError::Server {
        message: "The app could not ask the server about this place.".to_owned(),
    })?;
    Ok(ServerRequest {
        method: "GET".to_owned(),
        path: CablesRequest { bbox }.path_and_query(),
        body: None,
        content_type: None,
        needs_token: false,
    })
}

fn line(cable: &Cable) -> CableLine {
    let mut words = vec![kind_words(cable.kind).to_owned()];
    if let Some(kv) = cable.voltage_kv {
        words.push(format!("{} kV", kv.round()));
    }
    match cable.height_m {
        Some(height) => words.push(format!("{} m above the ground", height.round())),
        None => words.push("height not mapped".to_owned()),
    }
    CableLine {
        id: format!("{}:{}", cable.source, cable.local),
        kind: cable.kind.into(),
        points: cable
            .line
            .iter()
            .map(|point| Coordinate {
                lat: point.lat(),
                lon: point.lon(),
            })
            .collect(),
        height_mapped: cable.height_m.is_some(),
        words: words.join(", "),
        name: cable.name.clone(),
    }
}

/// "Cable ahead: power line in 400 m, 60 m below the line you fly".
fn warning_words(cable: &Cable, found: &CableAhead) -> String {
    let kind = kind_words(cable.kind).to_lowercase();
    // Rounded to 50 m: the course is walked in steps of 25.
    let metres = ((found.distance_m / 50.0).round() * 50.0) as i64;
    let place = if metres == 0 {
        format!("Cable here: {kind} within {} m", cables::MARGIN_ACROSS_M)
    } else {
        format!("Cable ahead: {kind} in {metres} m")
    };
    let height = match found.above_m {
        None => "your height above the ground is not known".to_owned(),
        Some(above) => {
            let by = above.abs().round() as i64;
            if by < 10 {
                "at the height you fly".to_owned()
            } else if above > 0.0 {
                format!("{by} m below the line you fly")
            } else {
                format!("{by} m above the line you fly")
            }
        }
    };
    let assumed = if found.height_mapped {
        String::new()
    } else {
        format!(
            "; its height is not mapped and taken as {} m",
            cables::assumed_height_m(cable.kind)
        )
    };
    format!("{place}, {height}{assumed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../server/tests/golden/cables.json"
        ))
        .unwrap()
    }

    #[test]
    fn the_layer_says_what_each_cable_is() {
        let set = CableSet::from_answer(200, recorded()).unwrap();
        let map = set.layer();
        assert_eq!(map.lines.len(), 8);
        let words: Vec<&str> = map.lines.iter().map(|line| line.words.as_str()).collect();
        assert!(
            words.contains(&"Power line, 110 kV, height not mapped"),
            "{words:?}"
        );
        assert!(
            words.contains(&"Material ropeway, height not mapped"),
            "{words:?}"
        );
        assert!(map.lines.iter().all(|line| !line.height_mapped));
        // A card credits OpenStreetMap by its line and link alone.
        assert_eq!(map.credits.len(), 1);
        assert_eq!(map.credits[0].text, "© OpenStreetMap contributors");
        assert_eq!(
            map.credits[0].url,
            "https://www.openstreetmap.org/copyright"
        );
        assert_eq!(map.credits[0].notice, None);
        assert_eq!(
            map.limits,
            "Not every cable is mapped, and few have a height."
        );
    }

    #[test]
    fn a_warning_says_the_kind_the_distance_and_the_height() {
        let cable = Cable {
            source: venti_domain::SourceId::new("osm-cables").unwrap(),
            local: "way/1".to_owned(),
            kind: Kind::PowerLine,
            line: Vec::new(),
            height_m: Some(40.0),
            voltage_kv: None,
            name: None,
            operator: None,
            url: None,
        };
        let found = CableAhead {
            index: 0,
            distance_m: 400.0,
            above_m: Some(60.0),
            arrival_above_m: Some(60.0),
            height_mapped: true,
        };
        assert_eq!(
            warning_words(&cable, &found),
            "Cable ahead: power line in 400 m, 60 m below the line you fly"
        );
        let below = CableAhead {
            distance_m: 0.0,
            above_m: Some(-30.0),
            arrival_above_m: Some(-30.0),
            height_mapped: false,
            ..found
        };
        assert_eq!(
            warning_words(&cable, &below),
            "Cable here: power line within 150 m, 30 m above the line you fly; \
             its height is not mapped and taken as 40 m"
        );
    }

    #[test]
    fn the_request_asks_for_a_box_around_the_place() {
        let request = cables_request(47.7, 11.75).unwrap();
        assert_eq!(request.method, "GET");
        assert!(
            request.path.starts_with("/v1/cables?bbox="),
            "{}",
            request.path
        );
        assert!(!request.needs_token);
    }
}
