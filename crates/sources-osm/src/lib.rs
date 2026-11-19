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
//! tags and, for an element that is an area, the centre Overpass computes
//! for it (`out tags center`). No contributor names are asked for.
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
//! - The wind directions, from `direction` and from the deprecated
//!   `free_flying:site_orientation`; see [`directions`].
//! - `free_flying:paragliding`, `:hanggliding`, `:official`, and what marks
//!   a place as unsafe or a use as discouraged.
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

mod direction;
pub mod fixtures;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use venti_domain::{BoundingBox, GeoPoint, Site, SiteId, SiteUse, SourceId, Timestamp};
use venti_sources::{Manifest, ParsedSites, PlanCtx, Request, SiteSource, Skipped, SourceError};

pub use direction::directions;

/// The interpreter of the main public Overpass instance, which FOSSGIS
/// e.V. runs. A server uses it unless it is told another.
pub const DEFAULT_INTERPRETER: &str = "https://overpass-api.de/api/interpreter";

/// How long the instance may work on the query, in seconds. The instance
/// that recorded the fixtures took 53 seconds for the first region.
const QUERY_TIMEOUT_S: u32 = 90;

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
    fn url(&self, area: &BoundingBox) -> String {
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
        "[out:json][timeout:{QUERY_TIMEOUT_S}][bbox:{},{},{},{}];({selectors});out tags center qt;",
        area.south(),
        area.west(),
        area.north(),
        area.east()
    )
}

/// Writes text as a query parameter: everything but letters, digits and
/// `-._~` is percent-encoded.
fn encode(text: &str) -> String {
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
    /// For a way or a relation: the centre Overpass computes for it.
    center: Option<Centre>,
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

fn time(text: &str) -> Option<Timestamp> {
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
fn elevation(value: &str) -> Option<f64> {
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

    let position = match (element.lat, element.lon, &element.center) {
        (Some(lat), Some(lon), _) => (lat, lon),
        (_, _, Some(centre)) => (centre.lat, centre.lon),
        _ => return Err("the element has no position".to_owned()),
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
                "out tags center qt;"
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
        assert!(url.ends_with("out%20tags%20center%20qt%3B"), "{url}");
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
        // One request a run, and no second run the same day.
        assert_eq!(manifest.requests_per_run(), Some(2));
    }
}
