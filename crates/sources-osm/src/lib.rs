//! Adapter for the free-flying sites of OpenStreetMap.
//!
//! OpenStreetMap has a tagging scheme for the places where paragliders and
//! hang gliders take off and land, `sport=free_flying`, described at
//! <https://wiki.openstreetmap.org/wiki/Tag:sport%3Dfree_flying> (read on
//! 2026-11-19). This adapter asks an Overpass API instance for every element
//! in the area that carries those tags and turns each into a canonical
//! [`Site`].
//!
//! # The request
//!
//! One GET to the instance's interpreter, with a query in Overpass QL for
//! the box of the run. It selects nodes, ways and relations that carry
//! `sport=free_flying` or one of the scheme's own keys: a tenth of the
//! tagged places in the first region lack the `sport` tag, among them the
//! two ramps on the Wallberg. The answer is asked for as JSON with the
//! tags and the geometry (`out tags geom`): a point comes with its
//! position, a way with the box around it and the positions of its
//! points, a relation with the box around it. No contributor names are
//! asked for.
//!
//! By the page on Overpass QL
//! (<https://wiki.openstreetmap.org/wiki/Overpass_API/Overpass_QL>, read on
//! 2026-11-21) an `out` statement takes one of `geom`, `bb` and `center`,
//! so an answer with the outlines has no centres: the answer recorded on
//! 2026-11-21 has none. The adapter works the centre out itself, see
//! [`Bounds::middle`].
//!
//! Which instance answers is a setting of whoever runs the server; see
//! [`DEFAULT_INTERPRETER`] for the one used when nothing is set, and the
//! manifest for what its operator asks.
//!
//! # What is read
//!
//! - The use of a place, from the scheme of 2026 (`free_flying:takeoff`,
//!   `:landing`, `:towing`, `:training` with `yes`, `permissive` or
//!   `discouraged`) and from the deprecated one that most elements still
//!   carry (`free_flying:site=takeoff;toplanding`).
//! - `name`, and `ele`, which OpenStreetMap defines as metres above sea
//!   level.
//! - Where it is. A point is where it is. An area is at the middle of the
//!   box around it, which is where Overpass puts its centre, and a way
//!   that is closed gives its outline: the ring of its points, drawn with
//!   fewer where it has more than an outline may have, see [`outline`]. A
//!   relation has no outline: the answer has only the box around it.
//! - The wind directions, from `direction` and from the deprecated
//!   `free_flying:site_orientation`; see [`directions`].
//! - `free_flying:paragliding`, `:hanggliding`, `:official`, and what marks
//!   a place as unsafe or a use as discouraged.
//! - What a page about the site says in its mapper's words: `website`
//!   (and `contact:website` where there is no `website`), `description`,
//!   `fee`, `free_flying:guest-guidelines` and the deprecated
//!   `free_flying:no-flight-time`, a time in which the site may not be
//!   used; its `no`, "the place might be used the whole year", is no such
//!   time and is left out. The text is passed on as written. Nothing is
//!   worked out from it.
//!
//! Never read: `operator`, which the scheme describes as the maintainer's
//! name and which in the first region is a club or a school on most sites
//! and a person on some; `phone`, `email` and the `contact:` keys for them.
//!
//! # What is left out, and said
//!
//! - An element with none of the four uses that is a school, a club or a
//!   shop. It is not a site.
//! - An element that says it is no place to fly: every use it names is
//!   `no`.
//! - Wind directions that cannot be read for certain. People write `O`
//!   for east in German and for west in Italian and French, both of which
//!   are spoken in the first region, and they write ranges both ways
//!   round. A wrong direction on a take-off is worse than none, so the
//!   site is kept and its directions are left out.
//!
//! A place that carries the tags and names no use is kept without one: it
//! is a place for free flying, and OpenStreetMap does not say which kind.
//! Nothing here guesses a use from a name.

mod cables;
mod direction;
pub mod fixtures;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{
    BoundingBox, GeoPoint, Site, SiteId, SiteOutline, SiteUse, SourceId, Timestamp,
};
use venti_sources::{Manifest, ParsedSites, PlanCtx, Request, SiteSource, Skipped, SourceError};

pub use cables::OsmCables;
pub use direction::directions;

/// The interpreter of the main public Overpass instance, which FOSSGIS
/// e.V. runs. A server uses it unless it is told another.
pub const DEFAULT_INTERPRETER: &str = "https://overpass-api.de/api/interpreter";

/// How long the instance may work on the query, in seconds. The instance
/// that recorded the fixtures took 53 seconds for the first region on
/// 2026-11-19 and 22 on 2026-11-21.
pub(crate) const QUERY_TIMEOUT_S: u32 = 90;

/// The keys of the tagging scheme that say what a place is used for, with
/// the use each names.
const USE_KEYS: [(&str, SiteUse); 4] = [
    ("free_flying:takeoff", SiteUse::Takeoff),
    ("free_flying:landing", SiteUse::Landing),
    ("free_flying:towing", SiteUse::Towing),
    ("free_flying:training", SiteUse::Training),
];

/// The key of the deprecated scheme, which names the use as its value.
const SITE_KEY: &str = "free_flying:site";

/// Keys that mark an element as a business or a club house. One that has
/// such a key and names no use is not a site.
const NOT_A_SITE: [&str; 11] = [
    "shop",
    "office",
    "amenity",
    "tourism",
    "craft",
    "building",
    "free_flying:school",
    "free_flying:shop",
    "free_flying:repair",
    "free_flying:club",
    "free_flying:tandem",
];

/// The request's tag: there is one request.
const TAG: &str = "sites";

/// The free-flying sites of OpenStreetMap.
#[derive(Debug, Clone)]
pub struct OsmFreeFlying {
    manifest: Manifest,
    interpreter: String,
}

impl OsmFreeFlying {
    /// Creates the adapter for the Overpass instance whose interpreter is
    /// at `interpreter`, for example [`DEFAULT_INTERPRETER`].
    ///
    /// Returns what is wrong with the address if it cannot be one: it must
    /// start with `https://` or `http://` and carry no query of its own.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new(interpreter: &str) -> Result<Self, String> {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-osm is valid");
        let rest = interpreter
            .strip_prefix("https://")
            .or_else(|| interpreter.strip_prefix("http://"))
            .ok_or_else(|| format!("{interpreter:?} must start with https:// or http://"))?;
        let plain = |c: char| c.is_ascii_graphic() && !matches!(c, '?' | '#' | '"' | '<' | '>');
        if rest.is_empty() || !rest.chars().all(plain) {
            return Err(format!(
                "{interpreter:?} must be the address of an Overpass interpreter, \
                 such as {DEFAULT_INTERPRETER}, without a query"
            ));
        }
        Ok(Self {
            manifest,
            interpreter: interpreter.to_owned(),
        })
    }

    /// The address that asks for the sites inside `area`.
    pub(crate) fn url(&self, area: &BoundingBox) -> String {
        format!("{}?data={}", self.interpreter, encode(&query(area)))
    }
}

/// The query for the sites inside a box, in Overpass QL.
fn query(area: &BoundingBox) -> String {
    let mut selectors = String::from(r#"nwr["sport"~"free_flying"];"#);
    let keys = std::iter::once(SITE_KEY).chain(USE_KEYS.iter().map(|(key, _)| *key));
    for key in keys {
        selectors.push_str(&format!(r#"nwr["{key}"];"#));
    }
    format!(
        "[out:json][timeout:{QUERY_TIMEOUT_S}][bbox:{},{},{},{}];({selectors});out tags geom qt;",
        area.south(),
        area.west(),
        area.north(),
        area.east()
    )
}

/// Writes text as a query parameter: everything but letters, digits and
/// `-._~` is percent-encoded.
pub(crate) fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

impl SiteSource for OsmFreeFlying {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// One request for the whole area.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request> {
        vec![Request::new(TAG, self.url(&ctx.area))]
    }

    fn parse(
        &self,
        request: &Request,
        body: &[u8],
        _ctx: &PlanCtx,
    ) -> Result<ParsedSites, SourceError> {
        if request.tag != TAG {
            return Err(SourceError::UnknownRequest(request.tag.clone()));
        }
        let answer: Answer = serde_json::from_slice(body)
            .map_err(|e| SourceError::Malformed(format!("the answer is not Overpass JSON: {e}")))?;
        // An instance that runs out of time or memory answers with what it
        // had found so far and says so in a remark. Half an answer must not
        // pass for a whole one.
        if let Some(remark) = answer.remark {
            return Err(SourceError::Malformed(format!(
                "the Overpass instance did not finish the query: {remark}"
            )));
        }

        let mut parsed = ParsedSites {
            as_of: answer
                .osm3s
                .and_then(|about| about.timestamp_osm_base)
                .and_then(|text| time(&text)),
            ..ParsedSites::default()
        };
        for element in answer.elements {
            // Each element is read on its own, so that one that cannot be
            // read costs only itself. What it is called is found first, to
            // report it by.
            let called = label(&element);
            let element: Element = match serde_json::from_value(element) {
                Ok(element) => element,
                Err(error) => {
                    parsed
                        .skipped
                        .push(Skipped::new(&called, error.to_string()));
                    continue;
                }
            };
            match site(&self.manifest.id, element, &called, &mut parsed.skipped) {
                Ok(site) => parsed.sites.push(site),
                Err(reason) => parsed.skipped.push(Skipped::new(&called, reason)),
            }
        }
        Ok(parsed)
    }
}

#[derive(Deserialize)]
struct Answer {
    osm3s: Option<About>,
    /// What went wrong, when something did.
    remark: Option<String>,
    elements: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct About {
    /// The moment the instance's copy of OpenStreetMap is of.
    timestamp_osm_base: Option<String>,
}

#[derive(Deserialize)]
struct Element {
    #[serde(rename = "type")]
    kind: Kind,
    id: u64,
    lat: Option<f64>,
    lon: Option<f64>,
    /// For a way or a relation in an answer asked for with `center`: the
    /// centre Overpass computes for it.
    center: Option<Centre>,
    /// For a way or a relation in an answer asked for with `geom`: the box
    /// around it.
    bounds: Option<Bounds>,
    /// For a way in an answer asked for with `geom`: the positions of its
    /// points, in the way's order. Read apart from the element, so that an
    /// outline that cannot be read costs the outline and not the site.
    geometry: Option<serde_json::Value>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Node,
    Way,
    Relation,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Way => "way",
            Self::Relation => "relation",
        }
    }
}

#[derive(Deserialize)]
struct Centre {
    lat: f64,
    lon: f64,
}

/// The box around a way or a relation, as Overpass sends it.
#[derive(Deserialize, Clone, Copy)]
struct Bounds {
    minlat: f64,
    minlon: f64,
    maxlat: f64,
    maxlon: f64,
}

impl Bounds {
    /// The middle of the box, which is what Overpass gives as the centre
    /// of an element asked for with `center`: "the centre of the above
    /// mentioned bounding box", by the page on Overpass QL.
    ///
    /// It is reckoned the way Overpass reckons it, so that an area lies
    /// where it lay while the adapter asked for centres, to the last
    /// digit. Overpass holds a latitude as a whole number of ten-millionths
    /// of a degree counted from 91° south, turns both edges back into
    /// degrees, halves their sum and writes seven decimals. Where the two
    /// edges are an odd number of ten-millionths apart the middle falls
    /// between two such steps, and which of them is written depends on
    /// that arithmetic. Checked against the answer recorded on 2026-11-19,
    /// which was asked for with `center`: the 210 areas of the first
    /// region come out as recorded, in both coordinates. *To verify*
    /// against Overpass's source; the tests keep a sample.
    fn middle(self) -> Option<(f64, f64)> {
        let held = |lat: f64| ((lat + 91.0) * 1e7).round() / 1e7 - 91.0;
        // Seven decimals, rounded as a number is rounded when it is
        // written.
        let written = |degrees: f64| format!("{degrees:.7}").parse::<f64>().ok();
        Some((
            written(f64::midpoint(held(self.minlat), held(self.maxlat)))?,
            written(f64::midpoint(self.minlon, self.maxlon))?,
        ))
    }
}

/// One point of a way's geometry.
#[derive(Deserialize)]
struct Vertex {
    lat: f64,
    lon: f64,
}

/// The outline of a way: its points as the answer has them, if the way is
/// closed. `None` for a way that is a line and for an element without
/// geometry; `Err` with the reason when the geometry cannot be read.
///
/// A way of more points than an outline may have is drawn with fewer, by
/// the method of Ramer, Douglas and Peucker: points are left out that lie
/// within a distance of the line between the points kept, and the
/// distance is doubled from half a metre until the ring fits. Whether
/// points were left out, and how far the farthest lay from the line that
/// replaced it, is the second value.
fn outline(geometry: &serde_json::Value) -> Result<Option<(SiteOutline, Option<f64>)>, String> {
    let vertices: Vec<Vertex> = serde_json::from_value(geometry.clone())
        .map_err(|error| format!("its outline cannot be read: {error}"))?;
    let ring = vertices
        .iter()
        .map(|vertex| GeoPoint::new(vertex.lat, vertex.lon))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("its outline cannot be read: {error}"))?;
    if ring.len() < 4 || ring.first() != ring.last() {
        // A line, not an area.
        return Ok(None);
    }
    let whole = ring.len();
    let mut within_m = None;
    let mut drawn = ring.clone();
    let mut tolerance_m = 0.5;
    while drawn.len() > SiteOutline::MAX_POINTS {
        drawn = simplified(&ring, tolerance_m);
        within_m = Some(tolerance_m);
        tolerance_m *= 2.0;
    }
    SiteOutline::new(drawn)
        .map(|outline| Some((outline, within_m)))
        .map_err(|error| format!("its outline of {whole} points cannot be drawn: {error}"))
}

/// The points of a closed ring that lie farther than `tolerance_m` from
/// the line between their kept neighbours, with the ring's first and last.
fn simplified(ring: &[GeoPoint], tolerance_m: f64) -> Vec<GeoPoint> {
    // Metres east and north of the first point, on a flat sheet: a flying
    // site is a few hundred metres across.
    const METRES_PER_DEGREE: f64 = 111_320.0;
    let origin = ring[0];
    let east = origin.lat().to_radians().cos() * METRES_PER_DEGREE;
    let flat: Vec<(f64, f64)> = ring
        .iter()
        .map(|point| {
            (
                (point.lon() - origin.lon()) * east,
                (point.lat() - origin.lat()) * METRES_PER_DEGREE,
            )
        })
        .collect();
    let away = |point: (f64, f64), from: (f64, f64), to: (f64, f64)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length2 = dx * dx + dy * dy;
        let along = if length2 == 0.0 {
            0.0
        } else {
            (((point.0 - from.0) * dx + (point.1 - from.1) * dy) / length2).clamp(0.0, 1.0)
        };
        (point.0 - from.0 - along * dx).hypot(point.1 - from.1 - along * dy)
    };

    let mut kept = vec![false; ring.len()];
    kept[0] = true;
    kept[ring.len() - 1] = true;
    let mut spans = vec![(0, ring.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        let farthest = (first + 1..last)
            .map(|index| (index, away(flat[index], flat[first], flat[last])))
            .max_by(|one, two| one.1.total_cmp(&two.1));
        if let Some((index, distance_m)) = farthest
            && distance_m > tolerance_m
        {
            kept[index] = true;
            spans.push((first, index));
            spans.push((index, last));
        }
    }
    ring.iter()
        .zip(kept)
        .filter_map(|(point, kept)| kept.then_some(*point))
        .collect()
}

pub(crate) fn time(text: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// What to call an element in a report: its kind and number, and its name
/// if it has one.
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

/// What a tag of the scheme says about one use or one kind of glider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Yes,
    No,
    /// Possible, with problems.
    Discouraged,
}

fn verdict(value: &str) -> Option<Verdict> {
    match value.trim() {
        "yes" | "permissive" => Some(Verdict::Yes),
        "no" => Some(Verdict::No),
        "discouraged" => Some(Verdict::Discouraged),
        _ => None,
    }
}

/// The parts of a value that holds several, as OpenStreetMap writes them
/// with a semicolon and people also write them with a comma.
fn parts(value: &str) -> impl Iterator<Item = &str> {
    value
        .split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

/// Reads `ele`: metres above sea level, as a plain number, with or without
/// the unit written behind it.
pub(crate) fn elevation(value: &str) -> Option<f64> {
    let number = value.trim().trim_end_matches('m').trim_end();
    let plain = !number.is_empty()
        && number
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.' || byte == b'-');
    if !plain {
        return None;
    }
    number
        .parse()
        .ok()
        .filter(|metres: &f64| metres.is_finite())
}

/// Converts one element. `Err` when it is left out; what was left out of a
/// site that is kept goes to `notes`.
fn site(
    source: &SourceId,
    element: Element,
    called: &str,
    notes: &mut Vec<Skipped>,
) -> Result<Site, String> {
    let tags = &element.tags;
    let tag = |key: &str| tags.get(key).map(String::as_str);
    let mut note = |reason: String| notes.push(Skipped::new(called, reason));

    // A point is where it is. An area is at the centre the answer gives
    // for it, if it was asked for with `center`, and else at the middle of
    // the box around it.
    let position = match (element.lat, element.lon, &element.center) {
        (Some(lat), Some(lon), _) => Some((lat, lon)),
        (_, _, Some(centre)) => Some((centre.lat, centre.lon)),
        _ => element.bounds.and_then(Bounds::middle),
    };
    let Some(position) = position else {
        return Err("the element has no position".to_owned());
    };
    let position = GeoPoint::new(position.0, position.1).map_err(|e| e.to_string())?;

    let mut uses = Vec::new();
    let mut discouraged = false;
    let mut refused = false;
    for (key, used_for) in USE_KEYS {
        match tag(key).map(|value| (value, verdict(value))) {
            Some((_, Some(Verdict::Yes))) => uses.push(used_for),
            Some((_, Some(Verdict::Discouraged))) => {
                uses.push(used_for);
                discouraged = true;
            }
            Some((_, Some(Verdict::No))) => refused = true,
            Some((value, None)) => note(format!("{key}={value} is not a value of the scheme")),
            None => {}
        }
    }
    for part in tag(SITE_KEY).into_iter().flat_map(parts) {
        match part {
            "takeoff" => uses.push(SiteUse::Takeoff),
            "landing" | "toplanding" => uses.push(SiteUse::Landing),
            "towing" => uses.push(SiteUse::Towing),
            "training" => uses.push(SiteUse::Training),
            "unsafe" => discouraged = true,
            // A place to fold a glider is no use of its own.
            "packing" => {}
            other => note(format!("{SITE_KEY}={other} is not a value of the scheme")),
        }
    }
    uses.sort_unstable();
    uses.dedup();

    for (key, value) in tags {
        let about_safety = key == "free_flying:unsafe" || key.starts_with("free_flying:unsafe:");
        if about_safety && matches!(verdict(value), Some(Verdict::Yes | Verdict::Discouraged)) {
            discouraged = true;
        }
    }

    if uses.is_empty() {
        if refused {
            return Err("every use the element names is refused".to_owned());
        }
        // A key of the scheme marks a business with `yes`; a key such as
        // `shop` marks one with whatever it says, short of `no`.
        let marks_a_business = |key: &&&str| match tag(key) {
            Some(value) if key.starts_with("free_flying:") => value == "yes",
            Some(value) => value != "no",
            None => false,
        };
        if let Some(key) = NOT_A_SITE.iter().find(marks_a_business) {
            return Err(format!(
                "not a take-off or a landing: it names no use and is tagged {key}"
            ));
        }
    }

    let suits = |key: &str, discouraged: &mut bool| match tag(key).and_then(verdict) {
        Some(Verdict::Yes) => Some(true),
        Some(Verdict::No) => Some(false),
        Some(Verdict::Discouraged) => {
            *discouraged = true;
            Some(true)
        }
        None => None,
    };
    let paragliding = suits("free_flying:paragliding", &mut discouraged);
    let hang_gliding = suits("free_flying:hanggliding", &mut discouraged);
    let official = match tag("free_flying:official").and_then(verdict) {
        Some(Verdict::Yes) => Some(true),
        Some(Verdict::No) => Some(false),
        _ => None,
    };

    let elevation_m = tag("ele").and_then(|value| {
        let read = elevation(value);
        if read.is_none() {
            note(format!("ele={value} cannot be read as metres"));
        }
        read
    });

    // The key of the scheme of 2026 wins over the deprecated one.
    let wind_directions = [("direction", true), ("free_flying:site_orientation", false)]
        .into_iter()
        .find_map(|(key, ranges)| tag(key).map(|value| (key, value, ranges)))
        .map(|(key, value, ranges)| {
            directions(value, ranges).unwrap_or_else(|reason| {
                note(format!("{key}={value} is left out: {reason}"));
                Vec::new()
            })
        })
        .unwrap_or_default();

    let area = match element.geometry.as_ref().map(outline) {
        Some(Ok(Some((area, within_m)))) => {
            if let Some(within_m) = within_m {
                note(format!(
                    "its outline is drawn with {} points: OpenStreetMap has more than {}, and \
                     those left out lie within {within_m} m of the line that stands for them",
                    area.points().len(),
                    SiteOutline::MAX_POINTS
                ));
            }
            Some(area)
        }
        Some(Ok(None)) | None => None,
        Some(Err(reason)) => {
            note(reason);
            None
        }
    };

    // What its mapper wrote, as written, without the blanks around it.
    let text = |key: &str| {
        tag(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let guest_guidelines =
        tag("free_flying:guest-guidelines").and_then(|value| match verdict(value) {
            Some(Verdict::Yes) => Some(true),
            Some(Verdict::No) => Some(false),
            _ => {
                note(format!(
                    "free_flying:guest-guidelines={value} is not a value of the scheme"
                ));
                None
            }
        });

    let local = format!("{}/{}", element.kind.name(), element.id);
    Ok(Site {
        url: Some(format!("https://www.openstreetmap.org/{local}")),
        id: SiteId::new(source.clone(), local).map_err(|e| e.to_string())?,
        name: tag("name")
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        position,
        elevation_m,
        uses,
        wind_directions,
        paragliding,
        hang_gliding,
        official,
        discouraged,
        website: text("website").or_else(|| text("contact:website")),
        description: text("description"),
        fee: text("fee"),
        guest_guidelines,
        // The scheme's `no` says that the place may be used the whole
        // year: no time to pass on.
        no_flight_time: text("free_flying:no-flight-time").filter(|time| time != "no"),
        outline: area,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> BoundingBox {
        BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap()
    }

    #[test]
    fn the_query_asks_for_the_tags_of_the_scheme_inside_the_box() {
        assert_eq!(
            query(&area()),
            concat!(
                "[out:json][timeout:90][bbox:46.3,9.4,48.5,17.2];",
                r#"(nwr["sport"~"free_flying"];nwr["free_flying:site"];"#,
                r#"nwr["free_flying:takeoff"];nwr["free_flying:landing"];"#,
                r#"nwr["free_flying:towing"];nwr["free_flying:training"];);"#,
                "out tags geom qt;"
            )
        );
    }

    #[test]
    fn the_request_is_one_get_to_the_interpreter_that_was_named() {
        let source = OsmFreeFlying::new("https://overpass.example/api/interpreter").unwrap();
        let ctx = PlanCtx {
            now: fixtures::recorded_at(),
            area: area(),
        };

        let requests = source.plan(&ctx);

        assert_eq!(requests.len(), 1);
        let url = &requests[0].url;
        assert!(
            url.starts_with(
                "https://overpass.example/api/interpreter?data=%5Bout%3Ajson%5D%5Btimeout%3A90%5D\
                 %5Bbbox%3A46.3%2C9.4%2C48.5%2C17.2%5D%3B%28nwr%5B%22sport%22~%22free_flying%22%5D"
            ),
            "{url}"
        );
        assert!(url.ends_with("out%20tags%20geom%20qt%3B"), "{url}");
        // Nothing in it needs encoding a second time.
        assert!(
            url.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":/?=%-._~".contains(&b))
        );
    }

    #[test]
    fn an_address_that_cannot_be_an_interpreter_is_refused() {
        assert!(OsmFreeFlying::new(DEFAULT_INTERPRETER).is_ok());
        assert!(OsmFreeFlying::new("http://overpass:12345/api/interpreter").is_ok());
        for bad in [
            "",
            "overpass-api.de/api/interpreter",
            "ftp://overpass-api.de/api/interpreter",
            "https://",
            "https://overpass-api.de/api/interpreter?data=",
            "https://overpass-api.de/api/inter preter",
        ] {
            assert!(OsmFreeFlying::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_area_lies_where_overpass_puts_its_centre() {
        let middle = |minlat, minlon, maxlat, maxlon| {
            Bounds {
                minlat,
                minlon,
                maxlat,
                maxlon,
            }
            .middle()
            .unwrap()
        };
        // Three areas of the first region, with the boxes of the answer
        // recorded on 2026-11-21 and the centres of the one recorded on
        // 2026-11-19, which was asked for with `center`. Each has an edge
        // an odd number of ten-millionths of a degree from the other, so
        // that the middle falls between two steps.
        assert_eq!(
            middle(47.6788445, 11.9046533, 47.6794382, 11.9055532),
            (47.6791413, 11.9051032)
        );
        assert_eq!(
            middle(47.6914638, 11.8850491, 47.6933534, 11.8880464),
            (47.6924086, 11.8865478)
        );
        // One of the 37 whose latitude comes out a step off when the
        // edges are halved as they are written.
        assert_eq!(
            middle(46.4681908, 9.927252, 46.4712569, 9.9301115),
            (46.4697238, 9.9286817)
        );
        // A box that is a point is that point.
        assert_eq!(middle(47.5, 11.5, 47.5, 11.5), (47.5, 11.5));
    }

    #[test]
    fn an_outline_of_many_points_is_drawn_with_fewer() {
        // A circle of 200 points, 100 m across, closed.
        let circle: Vec<serde_json::Value> = (0..=200)
            .map(|step| {
                let angle = f64::from(step % 200) / 200.0 * std::f64::consts::TAU;
                serde_json::json!({
                    "lat": 47.7 + 0.00045 * angle.sin(),
                    "lon": 11.8 + 0.00067 * angle.cos(),
                })
            })
            .collect();
        let (drawn, within_m) = outline(&serde_json::Value::Array(circle.clone()))
            .unwrap()
            .unwrap();
        assert!(drawn.points().len() <= SiteOutline::MAX_POINTS);
        assert!(drawn.points().len() >= 16, "{}", drawn.points().len());
        assert_eq!(drawn.points().first(), drawn.points().last());
        assert_eq!(within_m, Some(0.5));

        // A ring that fits is as it was sent.
        let few = serde_json::Value::Array(
            [0, 50, 100, 150, 200]
                .map(|index| circle[index].clone())
                .to_vec(),
        );
        let (drawn, within_m) = outline(&few).unwrap().unwrap();
        assert_eq!(drawn.points().len(), 5);
        assert_eq!(within_m, None);
        // A line is no area, and neither is what is no list of points.
        let line = serde_json::Value::Array(circle[..5].to_vec());
        assert_eq!(outline(&line), Ok(None));
        assert!(outline(&serde_json::json!([{"lat": 47.7}])).is_err());
        assert!(
            outline(&serde_json::json!([
                {"lat": 147.7, "lon": 11.8}, {"lat": 47.7, "lon": 11.9},
                {"lat": 47.8, "lon": 11.9}, {"lat": 147.7, "lon": 11.8}
            ]))
            .is_err()
        );
    }

    #[test]
    fn elevations_are_plain_metres() {
        assert_eq!(elevation("1420"), Some(1420.0));
        assert_eq!(elevation(" 1420.5 "), Some(1420.5));
        assert_eq!(elevation("1420 m"), Some(1420.0));
        assert_eq!(elevation("1420m"), Some(1420.0));
        // Feet, a thousands separator, a range: not read.
        for unread in [
            "4659 ft",
            "1,420",
            "1 420",
            "1400-1500",
            "hoch",
            "",
            "m",
            "1e3",
        ] {
            assert_eq!(elevation(unread), None, "{unread:?}");
        }
    }

    #[test]
    fn the_manifest_is_valid_and_says_what_the_licence_asks() {
        let source = OsmFreeFlying::new(DEFAULT_INTERPRETER).unwrap();
        let manifest = source.manifest();
        assert_eq!(manifest.id.as_str(), "osm-free-flying");
        assert_eq!(manifest.kinds, [venti_sources::Kind::Sites]);
        assert_eq!(
            manifest.licence.family(),
            venti_sources::LicenceFamily::ShareAlike
        );
        assert!(manifest.licence.attribution.contains("OpenStreetMap"));
        assert!(
            manifest
                .licence
                .notice
                .as_deref()
                .unwrap()
                .contains("Open Database License")
        );
        // One request a run, and two more at most after the instance asked
        // to come back later, two minutes apart: never more than three
        // queries a day, where the main public instance asks for fewer
        // than 100.
        assert_eq!(manifest.requests_per_run(), Some(3));
        assert_eq!(
            manifest.retry_after(),
            Some(std::time::Duration::from_secs(120))
        );
    }
}
