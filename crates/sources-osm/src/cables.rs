//! Adapter for the cables of OpenStreetMap: power lines and ropeways.
//!
//! A second dataset of the same provider, under the same licence, with a
//! manifest of its own (`osm-cables`). It is never merged with the sites.
//!
//! # The request
//!
//! One GET to the Overpass instance's interpreter, as for the sites, with
//! a query for the box of the run that selects the ways tagged
//! `power=line` or `power=minor_line`, every way tagged `aerialway`, and
//! the ways tagged `man_made=cable`, with their tags and the positions of
//! their points (`out tags geom`).
//!
//! # What is read
//!
//! - The kind, from `power`, `aerialway` and `man_made` ([`kind`]). Lifts
//!   whose rope runs on the ground (`rope_tow`, `magic_carpet`) and the
//!   parts of a lift that are no cable (`station`, `pylon`) are left out.
//!   A drag lift (`drag_lift`, `t-bar`, `j-bar`, `platter`) is kept only
//!   where its height is mapped: its rope hangs a few metres above a ski
//!   slope. A `man_made=cable` is kept only with `location=overhead`.
//!   *To verify* against the wiki's pages for these keys, which were not
//!   read for this adapter.
//! - `height`, in metres above the ground, written as a plain number with
//!   or without `m` behind it. Few cables carry it: OpenStreetMap puts
//!   heights on the pylons (`power=tower`), which this adapter does not
//!   ask for.
//! - `voltage`, in volts, of which the highest is kept, in kilovolts.
//! - `name`, and `operator` for a power line and a lift for people only,
//!   where it is a company. The operator of a material ropeway or a zip
//!   line may be a person, and is never read.

use std::collections::BTreeMap;

use serde::Deserialize;
use venti_domain::{BoundingBox, Cable, CableKind, GeoPoint, SourceId};
use venti_sources::{CableSource, Manifest, ParsedCables, PlanCtx, Request, Skipped, SourceError};

use crate::{QUERY_TIMEOUT_S, elevation, encode, time};

/// The request's tag: there is one request.
const TAG: &str = "cables";

/// The cables of OpenStreetMap.
#[derive(Debug, Clone)]
pub struct OsmCables {
    manifest: Manifest,
    interpreter: String,
}

impl OsmCables {
    /// Creates the adapter for the Overpass instance whose interpreter is
    /// at `interpreter`. The address is checked as for the sites.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new(interpreter: &str) -> Result<Self, String> {
        crate::OsmFreeFlying::new(interpreter)?;
        let manifest = Manifest::from_toml(include_str!("../manifest-cables.toml"))
            .expect("the cables manifest compiled into venti-sources-osm is valid");
        Ok(Self {
            manifest,
            interpreter: interpreter.to_owned(),
        })
    }

    /// The address that asks for the cables inside `area`.
    pub(crate) fn url(&self, area: &BoundingBox) -> String {
        format!("{}?data={}", self.interpreter, encode(&query(area)))
    }
}

/// The query for the cables inside a box, in Overpass QL.
fn query(area: &BoundingBox) -> String {
    format!(
        "[out:json][timeout:{QUERY_TIMEOUT_S}][bbox:{},{},{},{}];\
         (way[\"power\"~\"^(line|minor_line)$\"];way[\"aerialway\"];way[\"man_made\"=\"cable\"];);\
         out tags geom qt;",
        area.south(),
        area.west(),
        area.north(),
        area.east()
    )
}

impl CableSource for OsmCables {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new(TAG, self.url(&ctx.area))]
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        _ctx: &PlanCtx,
    ) -> Result<ParsedCables, SourceError> {
        if request.tag != TAG {
            return Err(SourceError::UnknownRequest(request.tag.clone()));
        }
        let answer: Answer = serde_json::from_slice(body)
            .map_err(|e| SourceError::Malformed(format!("the answer is not Overpass JSON: {e}")))?;
        if let Some(remark) = answer.remark {
            return Err(SourceError::Malformed(format!(
                "the Overpass instance did not finish the query: {remark}"
            )));
        }
        let mut parsed = ParsedCables {
            as_of: answer
                .osm3s
                .and_then(|about| about.timestamp_osm_base)
                .and_then(|text| time(&text)),
            ..ParsedCables::default()
        };
        for element in answer.elements {
            let called = label(&element);
            let element: Way = match serde_json::from_value(element) {
                Ok(element) => element,
                Err(error) => {
                    parsed
                        .skipped
                        .push(Skipped::new(&called, error.to_string()));
                    continue;
                }
            };
            match cable(&self.manifest.id, element) {
                Ok(cable) => parsed.cables.push(cable),
                Err(reason) => parsed.skipped.push(Skipped::new(&called, reason)),
            }
        }
        Ok(parsed)
    }
}

#[derive(Deserialize)]
struct Answer {
    osm3s: Option<About>,
    remark: Option<String>,
    elements: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct About {
    timestamp_osm_base: Option<String>,
}

#[derive(Deserialize)]
struct Way {
    #[serde(rename = "type")]
    kind: String,
    id: u64,
    #[serde(default)]
    geometry: Vec<Vertex>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Vertex {
    lat: f64,
    lon: f64,
}

fn label(element: &serde_json::Value) -> String {
    let kind = element["type"].as_str().unwrap_or("element");
    let id = element["id"]
        .as_u64()
        .map_or_else(|| "without a number".to_owned(), |id| id.to_string());
    match element["tags"]["name"].as_str() {
        Some(name) => format!("{kind} {id} ({name})"),
        None => format!("{kind} {id}"),
    }
}

/// The kind of cable the tags describe, or why the element is none.
fn kind(tags: &BTreeMap<String, String>, has_height: bool) -> Result<CableKind, String> {
    let tag = |key: &str| tags.get(key).map(|value| value.trim());
    match tag("power") {
        Some("line") => return Ok(CableKind::PowerLine),
        Some("minor_line") => return Ok(CableKind::MinorPowerLine),
        _ => {}
    }
    if let Some(lift) = tag("aerialway") {
        return match lift {
            "cable_car" => Ok(CableKind::CableCar),
            "gondola" | "mixed_lift" => Ok(CableKind::Gondola),
            "chair_lift" => Ok(CableKind::ChairLift),
            "goods" => Ok(CableKind::MaterialRopeway),
            "zip_line" => Ok(CableKind::ZipLine),
            "drag_lift" | "t-bar" | "j-bar" | "platter" if has_height => Ok(CableKind::DragLift),
            "drag_lift" | "t-bar" | "j-bar" | "platter" => Err(format!(
                "aerialway={lift} is left out: a drag lift's rope hangs a few metres above the \
                 slope, and this one has no height mapped"
            )),
            other => Err(format!("aerialway={other} is no cable in the air")),
        };
    }
    if tag("man_made") == Some("cable") {
        return if tag("location") == Some("overhead") {
            Ok(CableKind::Other)
        } else {
            Err("man_made=cable is left out: it is not tagged location=overhead".to_owned())
        };
    }
    Err("it is tagged as no cable this adapter reads".to_owned())
}

/// The highest of the voltages a `voltage` tag lists, in kilovolts.
fn voltage_kv(value: &str) -> Option<f64> {
    value
        .split(';')
        .filter_map(|part| part.trim().parse::<f64>().ok())
        .filter(|volts| volts.is_finite() && *volts > 0.0)
        .max_by(f64::total_cmp)
        .map(|volts| volts / 1000.0)
}

fn cable(source: &SourceId, way: Way) -> Result<Cable, String> {
    if way.kind != "way" {
        return Err(format!("a {} is no line", way.kind));
    }
    let tags = &way.tags;
    let text = |key: &str| {
        tags.get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let height_m = tags.get("height").and_then(|value| elevation(value));
    let kind = kind(tags, height_m.is_some())?;
    let line = way
        .geometry
        .iter()
        .map(|vertex| GeoPoint::new(vertex.lat, vertex.lon))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("its line cannot be read: {error}"))?;
    if line.len() < 2 {
        return Err("its line has fewer than two points".to_owned());
    }
    // A company runs a power line or a lift for people. A material
    // ropeway or a zip line may be a farmer's.
    let operator = match kind {
        CableKind::PowerLine
        | CableKind::MinorPowerLine
        | CableKind::CableCar
        | CableKind::Gondola
        | CableKind::ChairLift => text("operator"),
        _ => None,
    };
    let local = format!("way/{}", way.id);
    Ok(Cable {
        source: source.clone(),
        url: Some(format!("https://www.openstreetmap.org/{local}")),
        local,
        kind,
        line,
        height_m,
        voltage_kv: tags.get("voltage").and_then(|value| voltage_kv(value)),
        name: text("name"),
        operator,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn the_query_asks_for_lines_and_ropeways_in_the_box() {
        let area = BoundingBox::new(47.66, 11.70, 47.74, 11.82).unwrap();
        assert_eq!(
            query(&area),
            "[out:json][timeout:90][bbox:47.66,11.7,47.74,11.82];\
             (way[\"power\"~\"^(line|minor_line)$\"];way[\"aerialway\"];way[\"man_made\"=\"cable\"];);\
             out tags geom qt;"
        );
    }

    #[test]
    fn the_kinds_are_read_from_the_tags() {
        assert_eq!(
            kind(&tags(&[("power", "line")]), false),
            Ok(CableKind::PowerLine)
        );
        assert_eq!(
            kind(&tags(&[("power", "minor_line")]), false),
            Ok(CableKind::MinorPowerLine)
        );
        assert_eq!(
            kind(&tags(&[("aerialway", "goods")]), false),
            Ok(CableKind::MaterialRopeway)
        );
        assert_eq!(
            kind(&tags(&[("aerialway", "t-bar")]), true),
            Ok(CableKind::DragLift)
        );
        assert!(kind(&tags(&[("aerialway", "t-bar")]), false).is_err());
        assert!(kind(&tags(&[("aerialway", "station")]), false).is_err());
        assert!(kind(&tags(&[("aerialway", "rope_tow")]), true).is_err());
        assert!(kind(&tags(&[("power", "cable")]), false).is_err());
        assert_eq!(
            kind(
                &tags(&[("man_made", "cable"), ("location", "overhead")]),
                false
            ),
            Ok(CableKind::Other)
        );
        assert!(kind(&tags(&[("man_made", "cable")]), false).is_err());
    }

    #[test]
    fn the_highest_voltage_is_kept_in_kilovolts() {
        assert_eq!(voltage_kv("110000"), Some(110.0));
        assert_eq!(voltage_kv("110000;20000"), Some(110.0));
        assert_eq!(voltage_kv("20000; 380000"), Some(380.0));
        assert_eq!(voltage_kv("high"), None);
        assert_eq!(voltage_kv("0"), None);
    }

    #[test]
    fn the_manifest_is_valid_and_of_the_same_licence_family() {
        let source = OsmCables::new(crate::DEFAULT_INTERPRETER).unwrap();
        let manifest = source.manifest();
        assert_eq!(manifest.id.as_str(), "osm-cables");
        assert_eq!(manifest.kinds, [venti_sources::Kind::Cables]);
        assert_eq!(
            manifest.licence.family(),
            venti_sources::LicenceFamily::ShareAlike
        );
        assert!(OsmCables::new("ftp://x").is_err());
    }
}
