//! Adapter for the cables of OpenStreetMap: power lines and ropeways.
//!
//! A second dataset of the same provider, under the same licence, with a
//! manifest of its own (`osm-cables`). It is never merged with the sites.
//!
//! # The requests
//!
//! One GET to the Overpass instance's interpreter for each tile of the
//! run's box, with a query for the tile that selects the ways tagged
//! `power=line` or `power=minor_line`, every way tagged `aerialway`, and
//! the ways tagged `man_made=cable`, with their tags and the positions of
//! all their points (`out tags geom`), also of those outside the tile.
//!
//! The tiles are the cells of a fixed grid of whole degrees ([`TILE_DEG`]),
//! cut to the box: the whole Alps, 43.5 to 48.5° N and 5 to 17.2° E, are
//! 78 tiles. One query for all of them is too much: on 2026-11-22 the box
//! held 157,515 ways, which an instance took 144 s only to count, more than
//! the server gives a request, and the first region's 60,752 took 54 s.
//! That is 2,600 ways and 2.4 s to a square degree over the whole box, and
//! 3,500 ways and 3.1 s over the first region. A tile of one degree three
//! times as dense as the first region holds some 10,000 ways and is counted
//! in some 10 s, well within the query's 90 s and the fetcher's 120 s even
//! if sending the points takes as long again. At 870 bytes a way (a box
//! around Innsbruck, 2026-11-22) its answer is some 9 MB, and an average
//! tile's some 2 to 3 MB, against the 32 MB the manifest allows. Tiles of
//! half a degree would be four times the requests for no need, tiles of
//! two degrees four times the work of one, near the 90 s. *To verify* at
//! the first run of the whole Alps; an instance that is busy takes far
//! longer (33 s for a tile of 0.03° by 0.18° on 2026-11-23). A line across
//! the edge of two tiles is in the answers to both; the pipeline keeps it
//! once. A box smaller than a cell that crosses no grid line is one tile,
//! the box itself. Each answer names its tile as the box it is whole for,
//! so that a tile that answered replaces the stored cables inside it while
//! another tile of the run failed: on 2026-11-23 an overloaded instance
//! answered 31 of the 78 tiles of the Alps and refused the others.
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

/// The beginning of every request's tag, which then names its tile.
const TAG: &str = "cables";

/// The size of a tile, in degrees of latitude and of longitude: the grid's
/// lines are at the whole degrees.
const TILE_DEG: f64 = 1.0;

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

/// The tiles of `area`: the cells of the grid of [`TILE_DEG`] that it
/// touches, each cut to the area, from south to north and in each row from
/// west to east. An edge of the area that lies on a grid line makes no
/// tile of no height or width.
pub(crate) fn tiles(area: &BoundingBox) -> Vec<BoundingBox> {
    let lines = |from: f64, to: f64| {
        let mut edges = vec![from];
        let mut line = (from / TILE_DEG).floor() * TILE_DEG + TILE_DEG;
        while line < to {
            edges.push(line);
            line += TILE_DEG;
        }
        edges.push(to);
        edges
    };
    let rows = lines(area.south(), area.north());
    let columns = lines(area.west(), area.east());
    let mut tiles = Vec::new();
    for row in rows.windows(2) {
        for column in columns.windows(2) {
            if let Ok(tile) = BoundingBox::new(row[0], column[0], row[1], column[1]) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

/// The tag of the request for a tile, which names it in a run's report.
fn tag(tile: &BoundingBox) -> String {
    format!(
        "{TAG} {},{},{},{}",
        tile.south(),
        tile.west(),
        tile.north(),
        tile.east()
    )
}

/// The tile a request's tag names: the box its answer holds every cable
/// of. `None` for a tag that is not one of this adapter's.
fn tile_of(tag: &str) -> Option<BoundingBox> {
    let edges = tag.strip_prefix(TAG)?.strip_prefix(' ')?;
    let edges: Vec<f64> = edges
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    match edges[..] {
        [south, west, north, east] => BoundingBox::new(south, west, north, east).ok(),
        _ => None,
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
        tiles(&ctx.area)
            .iter()
            .map(|tile| Request::new(tag(tile), self.url(tile)))
            .collect()
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        _ctx: &PlanCtx,
    ) -> Result<ParsedCables, SourceError> {
        let Some(tile) = tile_of(&request.tag) else {
            return Err(SourceError::UnknownRequest(request.tag.clone()));
        };
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
            covers: Some(tile),
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
    fn a_box_is_asked_for_in_tiles_of_the_whole_degrees() {
        // The whole Alps: rows from 43.5, 44, ... 48 to 48.5, columns from
        // 5, 6, ... 17 to 17.2.
        let alps = BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap();
        let all = tiles(&alps);
        assert_eq!(all.len(), 6 * 13);
        assert_eq!(all[0], BoundingBox::new(43.5, 5.0, 44.0, 6.0).unwrap());
        assert_eq!(all[12], BoundingBox::new(43.5, 17.0, 44.0, 17.2).unwrap());
        assert_eq!(all[13], BoundingBox::new(44.0, 5.0, 45.0, 6.0).unwrap());
        assert_eq!(
            *all.last().unwrap(),
            BoundingBox::new(48.0, 17.0, 48.5, 17.2).unwrap()
        );
        // The tiles cover the box once: their areas add up to its own.
        let size = |b: &BoundingBox| (b.north() - b.south()) * (b.east() - b.west());
        let sum: f64 = all.iter().map(size).sum();
        assert!((sum - size(&alps)).abs() < 1e-9, "{sum}");
        // The first region alone, and the two others.
        let tiles_of = |s, w, n, e| tiles(&BoundingBox::new(s, w, n, e).unwrap()).len();
        assert_eq!(tiles_of(46.3, 9.4, 48.5, 17.2), 3 * 9);
        assert_eq!(tiles_of(43.5, 5.0, 48.5, 9.4), 6 * 5);
        assert_eq!(tiles_of(43.5, 9.4, 46.3, 17.2), 4 * 9);
        // An edge on a grid line makes no empty tile.
        assert_eq!(tiles_of(47.0, 11.0, 48.0, 13.0), 2);
        // A small box that crosses no grid line is asked for as it is.
        let small = BoundingBox::new(47.66, 11.70, 47.74, 11.82).unwrap();
        assert_eq!(tiles(&small), [small]);
        let source = OsmCables::new(crate::DEFAULT_INTERPRETER).unwrap();
        let ctx = PlanCtx {
            now: crate::fixtures::cables_recorded_at(),
            area: small,
        };
        let plan = source.plan(&ctx);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tag, "cables 47.66,11.7,47.74,11.82");
        assert_eq!(plan[0].url, source.url(&small));
        // Each tag names its tile exactly, so that its answer can say which
        // box it is whole for.
        for tile in &all {
            assert_eq!(tile_of(&tag(tile)), Some(*tile));
        }
        assert_eq!(tile_of(&plan[0].tag), Some(small));
        assert_eq!(tile_of("cables"), None);
        assert_eq!(tile_of("cables 1,2,3"), None);
        assert_eq!(tile_of("sites 47,11,48,12"), None);
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

    #[test]
    fn a_run_of_the_whole_alps_fits_its_budget_with_room_to_ask_again() {
        let source = OsmCables::new(crate::DEFAULT_INTERPRETER).unwrap();
        let manifest = source.manifest();
        let alps = BoundingBox::new(43.5, 5.0, 48.5, 17.2).unwrap();
        let tiles = tiles(&alps).len();
        assert_eq!(manifest.requests_per_run(), Some(120));
        assert_eq!(120 - tiles, 42);
        // Half a minute between two requests: never more in an hour than
        // the hourly limit, and the 78 tiles in 39 minutes of pauses alone.
        let pause = manifest.pause_between().unwrap();
        assert_eq!(pause.as_secs(), 30);
        assert_eq!(3600 / pause.as_secs(), 120);
    }
}
