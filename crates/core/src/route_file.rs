//! A route of the planner as a GPX 1.1 file, out and in.
//!
//! **Out.** [`route_to_gpx`] writes the planner's places as one `<rte>`
//! whose `<rtept>`s are named "Start", "Turnpoint 1" and so on, and
//! "Goal": a file any GPS, flying app or map reads. It names Venti as its
//! creator and holds nothing else: no time, no pilot, no device.
//!
//! **In.** [`route_from_gpx`] reads the first `<rte>` that has places. A
//! file without one gives its `<wpt>`s in the order they stand, and a file
//! with neither its track: every `<trkpt>` of every `<trk>`, in order,
//! thinned. More places than the planner takes
//! ([`ROUTE_POINTS_AT_MOST`], the most the weather along a route is asked
//! for) are thinned by one rule: the first and the last place stay, and
//! then, one at a time, the place that lies farthest from the line through
//! the places kept so far, until there are as many as the planner takes.
//! What is left is the shape of the route, its corners first. A place
//! whose `lat` or `lon` cannot be read, or lies off the map, is left out.
//! A file that holds no place, or a single one, is refused in words.
//!
//! **The reader's limits.** No XML library is among the workspace's
//! dependencies, so the reader is written here for these elements alone.
//! It reads tags with their attributes in single or double quotes, element
//! names with a namespace prefix (`gpx:rtept`), comments, `<?…?>` and
//! `<!DOCTYPE …>` (skipped), CDATA, and the five named entities and
//! numbered ones in text and attributes. It does not check that the file
//! is well-formed, nor its schema, nor its `version`: a GPX 1.0 file reads
//! as well. It expands no entity a DOCTYPE defines, and a DOCTYPE with
//! declarations of its own inside `[…]` is not read past. A file larger than
//! [`GPX_BYTES_AT_MOST`] is refused before it is read.

use venti_api_types::v1::ROUTE_POINTS_AT_MOST;
use venti_domain::GeoPoint;

use crate::live::one_line;
use crate::{Coordinate, CoreError};

/// The largest file read as a route: a track of a long day recorded every
/// second is some ten megabytes of GPX.
pub const GPX_BYTES_AT_MOST: usize = 32 * 1024 * 1024;

/// A route read from a file, for the planner.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct RouteFromFile {
    /// The places, in the order they are flown: for
    /// [`plan_route`](crate::plan_route). Two at least, and at most
    /// [`ROUTE_POINTS_AT_MOST`].
    pub points: Vec<Coordinate>,
    /// The name the file gives the route or the track, on one line. Empty
    /// when it gives none.
    pub name: String,
    /// What was read, in a line: "A route of 5 places.", "8 waypoints,
    /// in their order.", "A track of 3,402 points, thinned to 25 places."
    pub words: String,
}

/// What the route is called in a file the planner writes.
const ROUTE_NAME: &str = "Route planned in Venti";

/// The planner's places as a GPX 1.1 file: one `<rte>` with a named
/// `<rtept>` for each place. Places that are not on the map are left out.
#[uniffi::export]
pub fn route_to_gpx(points: Vec<Coordinate>) -> String {
    let places: Vec<GeoPoint> = points
        .iter()
        .filter_map(|point| GeoPoint::new(point.lat, point.lon).ok())
        .collect();
    let last = places.len().saturating_sub(1);
    let mut gpx = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <gpx version=\"1.1\" creator=\"Venti\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n  \
         <rte>\n",
    );
    gpx.push_str(&format!("    <name>{ROUTE_NAME}</name>\n"));
    for (index, place) in places.iter().enumerate() {
        let name = match index {
            0 => "Start".to_owned(),
            at if at == last => "Goal".to_owned(),
            at => format!("Turnpoint {at}"),
        };
        gpx.push_str(&format!(
            "    <rtept lat=\"{:.6}\" lon=\"{:.6}\"><name>{name}</name></rtept>\n",
            place.lat(),
            place.lon()
        ));
    }
    gpx.push_str("  </rte>\n</gpx>\n");
    gpx
}

/// Reads a route for the planner from a GPX file: its route, else its
/// waypoints, else its track, as the module says. A file that is no GPX
/// or holds no route is refused in words.
#[uniffi::export]
pub fn route_from_gpx(text: String) -> Result<RouteFromFile, CoreError> {
    let refused = |message: &str| CoreError::InvalidFile {
        message: message.to_owned(),
    };
    if text.len() > GPX_BYTES_AT_MOST {
        return Err(refused("This file is too large to be a route."));
    }
    let read = read_gpx(&text);
    if !read.is_gpx {
        return Err(refused(
            "This is not a GPX file. A route comes in a file that ends in .gpx.",
        ));
    }
    let (places, name, what) = if let Some(route) = read.routes.into_iter().find(|r| !r.is_empty())
    {
        (route, read.route_name, Source::Route)
    } else if !read.waypoints.is_empty() {
        (read.waypoints, String::new(), Source::Waypoints)
    } else {
        (read.track, read.track_name, Source::Track)
    };
    match places.len() {
        0 => return Err(refused("This file holds no route.")),
        1 => {
            return Err(refused(
                "This file holds a single place, and a route needs two at least.",
            ));
        }
        _ => {}
    }
    let all = places.len();
    let kept = thinned(&places, ROUTE_POINTS_AT_MOST);
    let thinned_words = if kept.len() < all {
        format!(", thinned to {} places", kept.len())
    } else {
        String::new()
    };
    let words = match what {
        Source::Route => format!(
            "A route of {}{thinned_words}.",
            number(all, "place", "places")
        ),
        Source::Waypoints => format!(
            "{}, in their order{thinned_words}.",
            capitalised(&number(all, "waypoint", "waypoints"))
        ),
        Source::Track => format!(
            "A track of {}{thinned_words}.",
            number(all, "point", "points")
        ),
    };
    Ok(RouteFromFile {
        points: kept
            .iter()
            .map(|place| Coordinate {
                lat: place.lat(),
                lon: place.lon(),
            })
            .collect(),
        name: one_line(&name, 60),
        words,
    })
}

/// Which part of a file a route was read from.
enum Source {
    Route,
    Waypoints,
    Track,
}

/// "1 place", "3,402 points".
fn number(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        return format!("1 {one}");
    }
    let digits = count.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {many}")
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// At most `most` of some places, the first and the last among them: the
/// rule in the module's head.
fn thinned(places: &[GeoPoint], most: usize) -> Vec<GeoPoint> {
    if places.len() <= most || most < 2 {
        return places.to_vec();
    }
    // Distances on a plane around the places' middle: close enough to
    // choose which place stands out most.
    let mid_lat = places.iter().map(GeoPoint::lat).sum::<f64>() / places.len() as f64;
    let squeeze = mid_lat.to_radians().cos();
    let flat: Vec<(f64, f64)> = places
        .iter()
        .map(|place| (place.lon() * squeeze, place.lat()))
        .collect();
    let off_line = |at: usize, from: usize, to: usize| {
        let (px, py) = flat[at];
        let ((ax, ay), (bx, by)) = (flat[from], flat[to]);
        let (dx, dy) = (bx - ax, by - ay);
        let length = dx * dx + dy * dy;
        let along = if length > 0.0 {
            (((px - ax) * dx + (py - ay) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (cx, cy) = (ax + along * dx, ay + along * dy);
        (px - cx).powi(2) + (py - cy).powi(2)
    };
    let mut kept = vec![0, places.len() - 1];
    while kept.len() < most {
        // The place farthest off the line between the two kept places it
        // lies between; of two as far, the earlier.
        let mut farthest: Option<(f64, usize)> = None;
        for pair in kept.windows(2) {
            for at in pair[0] + 1..pair[1] {
                let off = off_line(at, pair[0], pair[1]);
                if farthest.is_none_or(|(most_off, _)| off > most_off) {
                    farthest = Some((off, at));
                }
            }
        }
        let Some((_, at)) = farthest else { break };
        let slot = kept.partition_point(|&kept_at| kept_at < at);
        kept.insert(slot, at);
    }
    kept.into_iter().map(|at| places[at]).collect()
}

/// What the reader found in a file.
#[derive(Default)]
struct ReadGpx {
    /// Whether there is a `<gpx>` element at all.
    is_gpx: bool,
    /// Each `<rte>`'s places, in order.
    routes: Vec<Vec<GeoPoint>>,
    /// The name of the first `<rte>` that has one.
    route_name: String,
    /// The `<wpt>`s, in order.
    waypoints: Vec<GeoPoint>,
    /// Every `<trkpt>`, in order.
    track: Vec<GeoPoint>,
    /// The name of the first `<trk>` that has one.
    track_name: String,
}

/// One step through a file's text.
enum Token<'a> {
    Open {
        name: &'a str,
        attributes: &'a str,
        empty: bool,
    },
    Close {
        name: &'a str,
    },
    Text(String),
}

/// The name without its namespace prefix: `gpx:rtept` is `rtept`.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

/// The tokens of a file: tags and text, without comments, processing
/// instructions and declarations.
fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    std::iter::from_fn(move || {
        loop {
            if rest.is_empty() {
                return None;
            }
            let Some(after) = rest.strip_prefix('<') else {
                let end = rest.find('<').unwrap_or(rest.len());
                let (text, after) = rest.split_at(end);
                rest = after;
                return Some(Token::Text(decoded(text)));
            };
            let skip_past = |rest: &mut &str, end: &str| {
                *rest = rest.find(end).map_or("", |at| &rest[at + end.len()..]);
            };
            if after.starts_with("!--") {
                skip_past(&mut rest, "-->");
            } else if let Some(data) = after.strip_prefix("![CDATA[") {
                let end = data.find("]]>").unwrap_or(data.len());
                let text = data[..end].to_owned();
                rest = data.get(end + 3..).unwrap_or("");
                return Some(Token::Text(text));
            } else if after.starts_with('?') {
                skip_past(&mut rest, "?>");
            } else if after.starts_with('!') {
                skip_past(&mut rest, ">");
            } else {
                // A tag ends at the first '>' outside quotes.
                let mut quote: Option<char> = None;
                let mut end = None;
                for (at, char) in after.char_indices() {
                    match (quote, char) {
                        (Some(open), _) if char == open => quote = None,
                        (Some(_), _) => {}
                        (None, '"' | '\'') => quote = Some(char),
                        (None, '>') => {
                            end = Some(at);
                            break;
                        }
                        (None, _) => {}
                    }
                }
                let Some(end) = end else {
                    rest = "";
                    return None;
                };
                let tag = &after[..end];
                rest = &after[end + 1..];
                if let Some(name) = tag.strip_prefix('/') {
                    return Some(Token::Close {
                        name: local(name.trim()),
                    });
                }
                let (tag, empty) = match tag.strip_suffix('/') {
                    Some(tag) => (tag, true),
                    None => (tag, false),
                };
                let split = tag
                    .find(|char: char| char.is_whitespace())
                    .unwrap_or(tag.len());
                return Some(Token::Open {
                    name: local(&tag[..split]),
                    attributes: &tag[split..],
                    empty,
                });
            }
        }
    })
}

/// The value of an attribute of a tag, decoded.
fn attribute(attributes: &str, wanted: &str) -> Option<String> {
    let mut rest = attributes;
    loop {
        rest = rest.trim_start();
        let equals = rest.find('=')?;
        let name = rest[..equals].trim();
        let after = rest[equals + 1..].trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value_and_rest = &after[1..];
        let end = value_and_rest.find(quote)?;
        if local(name) == wanted {
            return Some(decoded(&value_and_rest[..end]));
        }
        rest = &value_and_rest[end + 1..];
    }
}

/// Text with its entities replaced: the five named ones and numbered
/// ones. An entity that is none of these stays as it is.
fn decoded(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let entity = rest[1..].find(';').map(|end| &rest[1..=end]);
        let replaced = entity.and_then(|entity| {
            Some(match entity {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let number = entity.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            })
        });
        match (replaced, entity) {
            (Some(char), Some(entity)) => {
                out.push(char);
                rest = &rest[entity.len() + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A place from a point's `lat` and `lon`, if both read and lie on the map.
fn place(attributes: &str) -> Option<GeoPoint> {
    let read = |name: &str| attribute(attributes, name)?.trim().parse::<f64>().ok();
    GeoPoint::new(read("lat")?, read("lon")?).ok()
}

/// Walks a file's tokens and keeps what a route is read from.
fn read_gpx(text: &str) -> ReadGpx {
    let mut read = ReadGpx::default();
    // The open elements, by their local names.
    let mut open: Vec<&str> = Vec::new();
    let mut name_text = String::new();
    for token in tokens(text) {
        match token {
            Token::Open {
                name,
                attributes,
                empty,
            } => {
                match name {
                    "gpx" => read.is_gpx = true,
                    "rte" if read.is_gpx => read.routes.push(Vec::new()),
                    "rtept" if open.last() == Some(&"rte") => {
                        if let (Some(route), Some(at)) = (read.routes.last_mut(), place(attributes))
                        {
                            route.push(at);
                        }
                    }
                    "wpt" if read.is_gpx => read.waypoints.extend(place(attributes)),
                    "trkpt" if read.is_gpx => read.track.extend(place(attributes)),
                    "name" => name_text.clear(),
                    _ => {}
                }
                if !empty {
                    open.push(name);
                }
            }
            Token::Close { name } => {
                if name == "name" {
                    let parent = open.len().checked_sub(2).and_then(|at| open.get(at));
                    let named = name_text.trim();
                    match parent {
                        Some(&"rte") if read.route_name.is_empty() => {
                            read.route_name = named.to_owned();
                        }
                        Some(&"trk") if read.track_name.is_empty() => {
                            read.track_name = named.to_owned();
                        }
                        _ => {}
                    }
                }
                // Up to the element that closes; a stray closing tag
                // closes nothing.
                if let Some(at) = open.iter().rposition(|open| *open == name) {
                    open.truncate(at);
                }
            }
            Token::Text(text) => {
                if open.last() == Some(&"name") {
                    name_text.push_str(&text);
                }
            }
        }
    }
    read
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALLBERG: Coordinate = Coordinate {
        lat: 47.6642,
        lon: 11.7967,
    };
    const BRAUNECK: Coordinate = Coordinate {
        lat: 47.6625,
        lon: 11.5203,
    };
    const SPITZING: Coordinate = Coordinate {
        lat: 47.6656,
        lon: 11.8869,
    };

    fn words(result: Result<RouteFromFile, CoreError>) -> String {
        match result {
            Err(CoreError::InvalidFile { message }) => message,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_route_goes_out_as_named_route_points_and_comes_back_the_same() {
        let gpx = route_to_gpx(vec![WALLBERG, BRAUNECK, SPITZING]);
        assert!(gpx.starts_with(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"Venti\""
        ));
        assert!(
            gpx.contains("<rtept lat=\"47.664200\" lon=\"11.796700\"><name>Start</name></rtept>")
        );
        assert!(gpx.contains("<name>Turnpoint 1</name>"));
        assert!(gpx.contains("<name>Goal</name>"));
        // Nothing of the pilot's: no time, no author, no device.
        for personal in ["<time>", "<author>", "<desc>", "<metadata>"] {
            assert!(!gpx.contains(personal), "{personal}");
        }

        let back = route_from_gpx(gpx).unwrap();
        assert_eq!(back.points, [WALLBERG, BRAUNECK, SPITZING]);
        assert_eq!(back.name, "Route planned in Venti");
        assert_eq!(back.words, "A route of 3 places.");

        // A place off the map is left out.
        let gpx = route_to_gpx(vec![
            WALLBERG,
            Coordinate {
                lat: 91.0,
                lon: 0.0,
            },
            SPITZING,
        ]);
        assert_eq!(gpx.matches("<rtept").count(), 2);
    }

    #[test]
    fn a_file_without_a_route_gives_its_waypoints_or_its_track() {
        let waypoints = r#"<?xml version="1.0"?>
            <!-- made up for a test -->
            <gpx:gpx xmlns:gpx="http://www.topografix.com/GPX/1/1" version='1.1' creator="Test">
              <gpx:wpt lat='47.6642' lon="11.7967"><gpx:name>Musterkogel &amp; West</gpx:name></gpx:wpt>
              <gpx:wpt lat="x" lon="11.7"/>
              <gpx:wpt lat="47.6625" lon="11.5203"/>
            </gpx:gpx>"#;
        let read = route_from_gpx(waypoints.to_owned()).unwrap();
        assert_eq!(read.points, [WALLBERG, BRAUNECK]);
        assert_eq!(read.words, "2 waypoints, in their order.");
        assert_eq!(read.name, "");

        // A route wins over waypoints, and its name is read, CDATA too.
        let both = r#"<gpx version="1.1"><wpt lat="1" lon="1"/><wpt lat="2" lon="2"/>
            <rte><name><![CDATA[Tegernsee <round>]]></name>
              <rtept lat="47.6642" lon="11.7967"></rtept><rtept lat="47.6656" lon="11.8869"/></rte></gpx>"#;
        let read = route_from_gpx(both.to_owned()).unwrap();
        assert_eq!(read.points, [WALLBERG, SPITZING]);
        assert_eq!(read.name, "Tegernsee <round>");

        // A track of many points is thinned to the planner's limit, its
        // corners kept: out east along a line, then north, then back.
        let mut points = String::new();
        let mut count = 0;
        for step in 0..=1000 {
            let lon = 11.0 + f64::from(step) * 0.001;
            points.push_str(&format!("<trkpt lat=\"47.0\" lon=\"{lon}\"/>"));
            count += 1;
        }
        for step in 1..=500 {
            let lat = 47.0 + f64::from(step) * 0.001;
            points.push_str(&format!("<trkpt lat=\"{lat}\" lon=\"12.0\"/>"));
            count += 1;
        }
        let track = format!(
            "<gpx version=\"1.1\"><trk><name>Long day</name><trkseg>{points}</trkseg></trk></gpx>"
        );
        let read = route_from_gpx(track).unwrap();
        assert_eq!(read.points.len(), ROUTE_POINTS_AT_MOST);
        assert_eq!(count, 1501);
        assert_eq!(read.words, "A track of 1,501 points, thinned to 25 places.");
        assert_eq!(read.name, "Long day");
        let first = read.points.first().unwrap();
        let last = read.points.last().unwrap();
        assert_eq!((first.lat, first.lon), (47.0, 11.0));
        assert_eq!((last.lat, last.lon), (47.5, 12.0));
        // The corner is among the places kept.
        assert!(
            read.points
                .iter()
                .any(|point| point.lat == 47.0 && point.lon == 12.0)
        );
    }

    #[test]
    fn what_holds_no_route_is_refused_in_words() {
        assert_eq!(
            words(route_from_gpx("<gpx version=\"1.1\"></gpx>".to_owned())),
            "This file holds no route."
        );
        assert_eq!(
            words(route_from_gpx(
                "<gpx><wpt lat=\"47.6\" lon=\"11.8\"/></gpx>".to_owned()
            )),
            "This file holds a single place, and a route needs two at least."
        );
        assert_eq!(
            words(route_from_gpx("HFDTE010726\nB1000004740000N".to_owned())),
            "This is not a GPX file. A route comes in a file that ends in .gpx."
        );
        assert_eq!(
            words(route_from_gpx(
                "<kml><Placemark><coordinates>11,47</coordinates></Placemark></kml>".to_owned()
            )),
            "This is not a GPX file. A route comes in a file that ends in .gpx."
        );
        assert_eq!(
            words(route_from_gpx(" ".repeat(GPX_BYTES_AT_MOST + 1))),
            "This file is too large to be a route."
        );
        // A file cut off in the middle of a tag reads what came before.
        let cut = "<gpx><rte><rtept lat=\"47.6642\" lon=\"11.7967\"/><rtept lat=\"47.6656\" lon=\"11.8869\"/><rtept lat=\"4";
        assert_eq!(route_from_gpx(cut.to_owned()).unwrap().points.len(), 2);
    }

    #[test]
    fn entities_and_numbers_read_as_text() {
        assert_eq!(
            decoded("a &lt;b&gt; &#65;&#x42; &unknown; &"),
            "a <b> AB &unknown; &"
        );
        assert_eq!(number(3402, "point", "points"), "3,402 points");
        assert_eq!(number(1, "point", "points"), "1 point");
        assert_eq!(number(999, "point", "points"), "999 points");
        assert_eq!(number(1_000_000, "point", "points"), "1,000,000 points");
    }
}
