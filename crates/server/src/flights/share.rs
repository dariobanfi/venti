//! The page about one flight that a link to it shows.
//!
//! It is made for two readers. A chat app that is given the link fetches
//! the page and reads the `og:` and `twitter:` lines of its head to build a
//! preview: a title, a line of text and a picture. A person who follows the
//! link gets the facts of the flight, its track and altitude profile, and
//! the flight file, with no app and no script.
//!
//! Everything on the page that a pilot or a flight file supplied (a display
//! name, a glider's name) is text someone else typed. It reaches the page
//! only through [`Esc`], which makes it harmless in HTML. The page also
//! forbids itself all scripts ([`CONTENT_SECURITY_POLICY`]), so a mistake
//! here could deface a page but not run code in a visitor's browser.

use std::fmt::{self, Write as _};
use std::sync::LazyLock;

use chrono::{Datelike, Timelike};
use sha2::{Digest, Sha256};
use venti_api_types::v1::{FlightId, flight_file_path, share_image_path, share_path};
use venti_domain::Timestamp;
use venti_store::{Flight, Place, ScoreKind, Visibility};

use super::picture;

/// The colour of a flight's line on a light ground; see `docs/design.md`.
/// The page's style below names it too, with its counterpart for a dark
/// ground.
pub(super) const ACCENT_LIGHT: &str = "#6250D6";

/// Text made safe to put into HTML, as content or inside a quoted
/// attribute.
pub(crate) struct Esc<'a>(pub(crate) &'a str);

impl fmt::Display for Esc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '&' => f.write_str("&amp;")?,
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                '"' => f.write_str("&quot;")?,
                '\'' => f.write_str("&#39;")?,
                other => f.write_char(other)?,
            }
        }
        Ok(())
    }
}

/// A number with its unit, kept apart so that each can be set in its own
/// size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measure {
    /// The number, as text.
    pub value: String,
    /// Its unit.
    pub unit: &'static str,
}

impl Measure {
    fn new(value: String, unit: &'static str) -> Self {
        Self { value, unit }
    }
}

impl fmt::Display for Measure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.value, self.unit)
    }
}

/// What the page and the picture say about a flight, in words.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    /// The headline: the distance and what the flight scored as, or just
    /// "Flight" for a track too short to score.
    pub headline: String,
    /// The scored distance, or the distance along the track without a score.
    pub distance: Measure,
    /// What the distance is: `free triangle`, or `along the track`.
    pub distance_caption: String,
    /// Time in the air.
    pub airtime: Measure,
    /// The points, if the flight scored.
    pub points: Option<Measure>,
    /// The day of the flight, such as `4 July 2026`.
    pub date: String,
}

impl Facts {
    /// The facts of a flight.
    pub fn of(flight: &Flight) -> Self {
        let derived = &flight.derived;
        let (headline, distance, distance_caption) = match &derived.score {
            Some(score) => {
                let distance = kilometres(score.distance_m);
                let kind = match (score.kind, score.closed) {
                    (ScoreKind::FreeFlight, _) => "free flight",
                    (ScoreKind::FreeTriangle, false) => "free triangle",
                    (ScoreKind::FreeTriangle, true) => "closed free triangle",
                    (ScoreKind::FaiTriangle, false) => "FAI triangle",
                    (ScoreKind::FaiTriangle, true) => "closed FAI triangle",
                };
                (format!("{distance} {kind}"), distance, kind.to_owned())
            }
            None => (
                "Flight".to_owned(),
                kilometres(derived.track_length_m),
                "along the track".to_owned(),
            ),
        };
        Self {
            headline,
            distance,
            distance_caption,
            airtime: duration(derived.airtime_s),
            points: derived
                .score
                .map(|score| Measure::new(format!("{:.2}", score.points), "points")),
            date: date(derived.start),
        }
    }

    /// The title of the page and of its preview.
    fn title(&self, flight: &Flight) -> String {
        format!("{} by {}", self.headline, flight.pilot_name)
    }

    /// The line under the title in a preview.
    fn description(&self, flight: &Flight) -> String {
        let mut text = format!("{} · {} in the air", self.date, self.airtime);
        if let Some(points) = &self.points {
            let _ = write!(text, " · {points}");
        }
        if let Some(glider) = &flight.derived.glider {
            let _ = write!(text, " · {glider}");
        }
        text
    }
}

fn kilometres(metres: f64) -> Measure {
    Measure::new(format!("{:.1}", metres / 1000.0), "km")
}

/// `29 min`, or `2:14 h` from an hour on.
fn duration(seconds: i64) -> Measure {
    let minutes = seconds.max(0).saturating_add(30) / 60;
    if minutes < 60 {
        Measure::new(minutes.to_string(), "min")
    } else {
        Measure::new(format!("{}:{:02}", minutes / 60, minutes % 60), "h")
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `4 July 2026`. The day is the day in UTC: the server does not know what
/// time zone a flight was flown in.
pub(crate) fn date(time: Timestamp) -> String {
    format!(
        "{} {} {}",
        time.day(),
        MONTHS[time.month0() as usize],
        time.year()
    )
}

/// `10:30 UTC`.
fn clock(time: Timestamp) -> String {
    format!("{:02}:{:02} UTC", time.hour(), time.minute())
}

/// `47.6642° N, 11.7967° E`.
fn coordinates(place: &Place) -> String {
    let (lat, lon) = (place.position.lat(), place.position.lon());
    format!(
        "{:.4}° {}, {:.4}° {}",
        lat.abs(),
        if lat < 0.0 { 'S' } else { 'N' },
        lon.abs(),
        if lon < 0.0 { 'W' } else { 'E' },
    )
}

/// The page's style. There is no other style on the page: the policy below
/// allows exactly this text and nothing a visitor or a pilot could add.
pub(crate) const STYLE: &str = "
:root { color-scheme: light dark; --accent: #6250D6; --quiet: #6b6b70; --line: #d9d9de; --tint: #f3f1fb; }
@media (prefers-color-scheme: dark) {
  :root { --accent: #9085E9; --quiet: #a1a1a8; --line: #3a3a3f; --tint: #1f1d2b; }
}
* { box-sizing: border-box; }
body { margin: 0; background: Canvas; color: CanvasText;
  font: 1rem/1.5 system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif; }
header, main, footer { max-width: 44rem; margin: 0 auto; padding: 0 1.25rem; }
header { padding-top: 1.25rem; }
.brand { color: var(--accent); font-weight: 700; font-size: 1.125rem; text-decoration: none; }
h1 { font-size: 1.75rem; line-height: 1.2; margin: 1.5rem 0 0.25rem; }
.byline { margin: 0 0 1.5rem; color: var(--quiet); }
.byline b { color: CanvasText; font-weight: 600; }
figure { margin: 0 0 1.5rem; }
svg { display: block; width: 100%; height: auto; border-radius: 1rem; background: var(--tint); }
.line { fill: none; stroke: var(--accent); stroke-width: 4; stroke-linejoin: round; stroke-linecap: round; }
.start { fill: Canvas; stroke: var(--accent); stroke-width: 3; }
.end { fill: var(--accent); }
.area { fill: var(--accent); fill-opacity: 0.18; }
.thin { stroke-width: 2; }
figcaption { margin-top: 0.5rem; color: var(--quiet); font-size: 0.875rem; }
dl { margin: 0 0 1.5rem; }
.numbers { display: grid; grid-template-columns: repeat(auto-fit, minmax(8rem, 1fr)); gap: 1rem 1.5rem; }
.numbers div { display: flex; flex-direction: column-reverse; }
.numbers dt { color: var(--quiet); font-size: 0.875rem; }
.numbers dd { margin: 0; font-size: 2rem; font-weight: 600; line-height: 1.15;
  font-family: ui-rounded, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
  font-variant-numeric: tabular-nums; }
.numbers dd span { color: var(--quiet); font-size: 1rem; font-weight: 400; }
.details { border-top: 1px solid var(--line); }
.details div { display: flex; justify-content: space-between; gap: 1rem; padding: 0.625rem 0;
  border-bottom: 1px solid var(--line); }
.details dt { color: var(--quiet); white-space: nowrap; }
.details dd { margin: 0; text-align: right; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.button { display: inline-block; padding: 0.75rem 1.25rem; border-radius: 0.75rem;
  background: var(--accent); color: #fff; font-weight: 600; text-decoration: none; }
@media (prefers-color-scheme: dark) { .button { color: #14121f; } }
.button:focus-visible, .brand:focus-visible { outline: 3px solid CanvasText; outline-offset: 3px; }
footer { padding-top: 0.5rem; padding-bottom: 2.5rem; color: var(--quiet); font-size: 0.875rem; }
";

/// What the page allows a browser to load and run: nothing but its own
/// style, and pictures from this server. No script of any origin, no
/// frames, no forms.
pub static CONTENT_SECURITY_POLICY: LazyLock<String> = LazyLock::new(|| policy_for(STYLE));

/// The policy of a page whose only style is `style`: that text, pictures
/// from this server, and nothing else.
pub(crate) fn policy_for(style: &str) -> String {
    format!(
        "default-src 'none'; style-src 'sha256-{}'; img-src 'self'; \
         base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        base64(&Sha256::digest(style.as_bytes()))
    )
}

/// Standard base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let bits = u32::from_be_bytes([0, group[0], group[1], group[2]]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                text.push(char::from(ALPHABET[((bits >> shift) & 63) as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// The size of the drawing of the track, in the units of its own
/// coordinates.
const TRACK_BOX: (f64, f64) = (640.0, 360.0);
/// The size of the altitude profile.
const PROFILE_BOX: (f64, f64) = (640.0, 180.0);

/// Where the points of an outline fall inside a box of `width` by `height`
/// with `margin` free on every side: the track as large as fits, centred,
/// north up, with east and north to the same scale.
pub(crate) fn fit(outline: &[[f64; 2]], width: f64, height: f64, margin: f64) -> Vec<(f64, f64)> {
    let Some(first) = outline.first() else {
        return Vec::new();
    };
    // A degree of longitude is shorter than one of latitude by the cosine
    // of the latitude.
    let narrowing = first[0].to_radians().cos().max(0.01);
    let flat: Vec<(f64, f64)> = outline
        .iter()
        .map(|[lat, lon]| ((lon - first[1]) * narrowing, *lat - first[0]))
        .collect();
    let (mut west, mut east, mut south, mut north) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for &(x, y) in &flat {
        west = west.min(x);
        east = east.max(x);
        south = south.min(y);
        north = north.max(y);
    }
    let (room_x, room_y) = (width - 2.0 * margin, height - 2.0 * margin);
    // A track that is a single point has no extent to scale by.
    let scale = (room_x / (east - west).max(1e-9)).min(room_y / (north - south).max(1e-9));
    let scale = if (east - west).max(north - south) < 1e-9 {
        0.0
    } else {
        scale
    };
    let (left, top) = (
        (width - (east - west) * scale) / 2.0,
        (height - (north - south) * scale) / 2.0,
    );
    flat.into_iter()
        .map(|(x, y)| (left + (x - west) * scale, top + (north - y) * scale))
        .collect()
}

pub(crate) fn points_attribute(points: &[(f64, f64)]) -> String {
    let mut text = String::with_capacity(points.len() * 12);
    for (x, y) in points {
        let _ = write!(text, "{x:.1},{y:.1} ");
    }
    text.trim_end().to_owned()
}

fn track_svg(flight: &Flight, facts: &Facts) -> String {
    let (width, height) = TRACK_BOX;
    let points = fit(&flight.derived.outline, width, height, 28.0);
    let label = format!(
        "The shape of the track seen from above, north up: {} {}.",
        facts.distance, facts.distance_caption
    );
    let mut svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{}\">",
        Esc(&label)
    );
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        let _ = write!(
            svg,
            "<polyline class=\"line\" points=\"{}\"/>\
             <circle class=\"start\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"7\"/>\
             <circle class=\"end\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"7\"/>",
            points_attribute(&points),
            first.0,
            first.1,
            last.0,
            last.1
        );
    }
    svg.push_str("</svg>");
    svg
}

/// The altitude profile as a drawing and the sentence that goes under it.
fn profile_svg(flight: &Flight) -> Option<(String, String)> {
    let profile = &flight.derived.profile;
    let (lowest, highest) = (
        profile.iter().map(|point| point[1]).min()?,
        profile.iter().map(|point| point[1]).max()?,
    );
    let seconds = profile.last()?[0].max(1);
    let (width, height) = PROFILE_BOX;
    let (left, right, top, bottom) = (16.0, width - 16.0, 16.0, height - 16.0);
    let span = f64::from((highest - lowest).max(1));
    let points: Vec<(f64, f64)> = profile
        .iter()
        .map(|[second, altitude]| {
            (
                left + (right - left) * f64::from(*second) / f64::from(seconds),
                bottom - (bottom - top) * f64::from(*altitude - lowest) / span,
            )
        })
        .collect();
    let (first, last) = (points.first()?, points.last()?);
    // The numbers are said in words under the drawing, where they are set
    // in the page's own type at a size that can be read.
    let caption = format!(
        "Altitude over the time of the flight: lowest {lowest} m, highest {highest} m above sea level."
    );
    let svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{}\">\
         <polygon class=\"area\" points=\"{:.1},{bottom:.1} {} {:.1},{bottom:.1}\"/>\
         <polyline class=\"line thin\" points=\"{}\"/>\
         </svg>",
        Esc(&caption),
        first.0,
        points_attribute(&points),
        last.0,
        points_attribute(&points),
    );
    Some((svg, caption))
}

/// Where the page's links point: the address the server is reached at from
/// outside, without a slash at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicUrl(String);

impl PublicUrl {
    /// Reads an address such as `https://venti.example`.
    pub fn new(url: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .ok_or_else(|| format!("{url:?} must start with http:// or https://"))?;
        let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']');
        if rest.is_empty() || !rest.chars().all(plain) {
            return Err(format!(
                "{url:?} must be an address such as https://venti.example, without a path"
            ));
        }
        Ok(Self(url.to_owned()))
    }

    /// The address as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether what travels to this address travels in the clear: it is
    /// plain HTTP, and not to this machine itself.
    pub fn is_plain_http_to_elsewhere(&self) -> bool {
        let Some(rest) = self.0.strip_prefix("http://") else {
            return false;
        };
        // The host without its port; an IPv6 address stands in brackets.
        let host = match rest.strip_prefix('[') {
            Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
            None => rest.split(':').next().unwrap_or_default(),
        };
        let this_machine = host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback());
        !this_machine
    }
}

/// The page for a flight that its pilot made public or unlisted.
///
/// # Panics
///
/// Panics if the flight's address is not one the server made, which no
/// stored flight has.
pub fn page(flight: &Flight, public_url: &PublicUrl) -> String {
    let id: FlightId = flight
        .id
        .parse()
        .expect("a stored flight has a well-formed address");
    let facts = Facts::of(flight);
    let derived = &flight.derived;
    let base = public_url.as_str();
    let (title, description) = (facts.title(flight), facts.description(flight));
    let (page_url, image_url) = (
        format!("{base}{}", share_path(&id)),
        format!("{base}{}", share_image_path(&id)),
    );
    let image_alt = format!(
        "The track of the flight drawn as a line, with its numbers: {} {}, {} in the air.",
        facts.distance, facts.distance_caption, facts.airtime
    );

    let mut html = String::with_capacity(16 * 1024);
    let _ = write!(
        html,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title} · Venti</title>\n\
         <meta name=\"description\" content=\"{description}\">\n",
        title = Esc(&title),
        description = Esc(&description),
    );
    // A link that was given to a few people is not for search engines.
    if flight.visibility != Visibility::Public {
        html.push_str("<meta name=\"robots\" content=\"noindex\">\n");
    }
    let _ = write!(
        html,
        "<link rel=\"canonical\" href=\"{page_url}\">\n\
         <meta property=\"og:type\" content=\"website\">\n\
         <meta property=\"og:site_name\" content=\"Venti\">\n\
         <meta property=\"og:title\" content=\"{title}\">\n\
         <meta property=\"og:description\" content=\"{description}\">\n\
         <meta property=\"og:url\" content=\"{page_url}\">\n\
         <meta property=\"og:image\" content=\"{image_url}\">\n\
         <meta property=\"og:image:type\" content=\"image/png\">\n\
         <meta property=\"og:image:width\" content=\"{image_width}\">\n\
         <meta property=\"og:image:height\" content=\"{image_height}\">\n\
         <meta property=\"og:image:alt\" content=\"{image_alt}\">\n\
         <meta name=\"twitter:card\" content=\"summary_large_image\">\n\
         <meta name=\"twitter:title\" content=\"{title}\">\n\
         <meta name=\"twitter:description\" content=\"{description}\">\n\
         <meta name=\"twitter:image\" content=\"{image_url}\">\n\
         <meta name=\"twitter:image:alt\" content=\"{image_alt}\">\n\
         <style>{STYLE}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">Venti</a></header>\n<main>\n\
         <h1>{headline}</h1>\n\
         <p class=\"byline\"><b>{pilot}</b> @{handle} · \
         <time datetime=\"{datetime}\">{date}</time></p>\n\
         <figure>{track}</figure>\n",
        page_url = Esc(&page_url),
        image_url = Esc(&image_url),
        title = Esc(&title),
        description = Esc(&description),
        image_width = picture::WIDTH,
        image_height = picture::HEIGHT,
        image_alt = Esc(&image_alt),
        headline = Esc(&facts.headline),
        pilot = Esc(&flight.pilot_name),
        handle = Esc(&flight.pilot_handle),
        datetime = Esc(&derived.start.format("%Y-%m-%d").to_string()),
        date = Esc(&facts.date),
        track = track_svg(flight, &facts),
    );

    // The headline numbers, large.
    let number = |html: &mut String, name: &str, value: &str, unit: &str| {
        let _ = write!(html, "<div><dt>{}</dt><dd>{}", Esc(name), Esc(value));
        if !unit.is_empty() {
            let _ = write!(html, " <span>{}</span>", Esc(unit));
        }
        html.push_str("</dd></div>\n");
    };
    html.push_str("<dl class=\"numbers\">\n");
    number(
        &mut html,
        "Airtime",
        &facts.airtime.value,
        facts.airtime.unit,
    );
    let distance_name = match &derived.score {
        Some(_) => "Scored distance",
        None => "Distance along the track",
    };
    number(
        &mut html,
        distance_name,
        &facts.distance.value,
        facts.distance.unit,
    );
    if let Some(points) = &facts.points {
        // The name says what the number counts.
        number(&mut html, "Points", &points.value, "");
    }
    number(
        &mut html,
        "Highest point",
        &derived.max_altitude_m.to_string(),
        "m",
    );
    html.push_str("</dl>\n");

    if let Some((profile, caption)) = profile_svg(flight) {
        let _ = writeln!(
            html,
            "<figure>{profile}<figcaption>{}</figcaption></figure>",
            Esc(&caption)
        );
    }

    // Everything else, as a plain list.
    let detail = |html: &mut String, name: &str, value: &str| {
        let _ = writeln!(
            html,
            "<div><dt>{}</dt><dd>{}</dd></div>",
            Esc(name),
            Esc(value)
        );
    };
    html.push_str("<dl class=\"details\">\n");
    if derived.score.is_some() {
        detail(&mut html, "Scored as", &facts.distance_caption);
    }
    match &derived.takeoff {
        Some(takeoff) => detail(
            &mut html,
            "Take-off",
            &format!(
                "{}, {}, {} m",
                clock(takeoff.time),
                coordinates(takeoff),
                takeoff.altitude_m
            ),
        ),
        None => detail(&mut html, "Start of the recording", &clock(derived.start)),
    }
    if let Some(landing) = &derived.landing {
        detail(
            &mut html,
            "Landing",
            &format!(
                "{}, {}, {} m",
                clock(landing.time),
                coordinates(landing),
                landing.altitude_m
            ),
        );
    }
    detail(
        &mut html,
        "Distance along the track",
        &kilometres(derived.track_length_m).to_string(),
    );
    detail(
        &mut html,
        "Best climb",
        &format!("{:.1} m/s", derived.max_climb_ms),
    );
    if let Some(glider) = &derived.glider {
        detail(&mut html, "Glider", glider);
    }
    html.push_str("</dl>\n");

    let _ = write!(
        html,
        "<p><a class=\"button\" href=\"{file}\" download>Download the flight file (IGC)</a></p>\n\
         </main>\n<footer>\n\
         <p>Venti is a free, open-source app for free flight. It worked these numbers out \
         from the flight file. Points follow the World XContest's rules as Venti reads them; \
         they are not the league's result.</p>\n\
         </footer>\n</body>\n</html>\n",
        file = Esc(&flight_file_path(&id)),
    );
    html
}

/// The page shown for a link that leads to nothing anyone may see: no such
/// flight, or one that is private. It is the same page for both.
pub fn not_found_page() -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>No flight here · Venti</title>\n\
         <style>{STYLE}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">Venti</a></header>\n<main>\n\
         <h1>No flight here</h1>\n\
         <p class=\"byline\">This link leads to no flight. It may have been mistyped, or \
         its pilot no longer shares the flight.</p>\n\
         </main>\n</body>\n</html>\n"
    )
}

/// The page shown when the server cannot look a flight up right now.
pub fn trouble_page() -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>Try again in a moment · Venti</title>\n\
         <style>{STYLE}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">Venti</a></header>\n<main>\n\
         <h1>Try again in a moment</h1>\n\
         <p class=\"byline\">Venti cannot show this flight right now. Nothing is wrong \
         with the link.</p>\n\
         </main>\n</body>\n</html>\n"
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use venti_domain::GeoPoint;
    use venti_store::{Derived, Score};

    use super::*;

    /// A public flight with every value present.
    pub(crate) fn flight() -> Flight {
        let start = Utc.with_ymd_and_hms(2026, 7, 4, 10, 30, 0).unwrap();
        Flight {
            id: "0123456789abcdefghjkmnpqrs".to_owned(),
            account_id: "account".to_owned(),
            pilot_handle: "dario_b".to_owned(),
            pilot_name: "Dario B.".to_owned(),
            pilot_picture: None,
            identity: "6c55247b80a34b03fa0bb3bd38173c59".to_owned(),
            visibility: Visibility::Public,
            uploaded_at: start,
            file_bytes: 64_000,
            taken_down_at: None,
            published_at: None,
            public_change: None,
            glider_class: None,
            pilot_country: None,
            derived: Derived {
                version: 1,
                start,
                airtime_s: 8040,
                takeoff: Some(Place {
                    time: start,
                    position: GeoPoint::new(47.6642, 11.7967).unwrap(),
                    altitude_m: 1620,
                }),
                landing: Some(Place {
                    time: start + Duration::seconds(8040),
                    position: GeoPoint::new(47.6831, 11.7604).unwrap(),
                    altitude_m: 742,
                }),
                track_length_m: 61_270.0,
                max_altitude_m: 2817,
                max_climb_ms: 3.42,
                score: Some(Score {
                    kind: ScoreKind::FaiTriangle,
                    closed: true,
                    distance_m: 42_310.0,
                    points: 67.7,
                }),
                glider: Some("Sample Wing".to_owned()),
                outline: vec![
                    [47.6642, 11.7967],
                    [47.76, 11.70],
                    [47.70, 11.95],
                    [47.6831, 11.7604],
                ],
                profile: vec![[0, 1620], [1800, 2817], [6000, 1900], [8040, 742]],
                habits: None,
            },
            change: 1,
        }
    }

    fn url() -> PublicUrl {
        PublicUrl::new("https://venti.example").unwrap()
    }

    #[test]
    fn the_facts_are_worded_for_people() {
        let facts = Facts::of(&flight());
        assert_eq!(facts.headline, "42.3 km closed FAI triangle");
        assert_eq!(facts.distance.to_string(), "42.3 km");
        assert_eq!(facts.airtime.to_string(), "2:14 h");
        assert_eq!(facts.points.as_ref().unwrap().to_string(), "67.70 points");
        assert_eq!(facts.date, "4 July 2026");
        assert_eq!(
            facts.title(&flight()),
            "42.3 km closed FAI triangle by Dario B."
        );
        assert_eq!(
            facts.description(&flight()),
            "4 July 2026 · 2:14 h in the air · 67.70 points · Sample Wing"
        );

        assert_eq!(duration(0).to_string(), "0 min");
        assert_eq!(duration(1744).to_string(), "29 min");
        assert_eq!(duration(3569).to_string(), "59 min");
        assert_eq!(duration(3570).to_string(), "1:00 h");
        assert_eq!(duration(36_000).to_string(), "10:00 h");

        // A track too short to score still has something to say.
        let mut unscored = flight();
        unscored.derived.score = None;
        unscored.derived.glider = None;
        let facts = Facts::of(&unscored);
        assert_eq!(facts.headline, "Flight");
        assert_eq!(facts.distance.to_string(), "61.3 km");
        assert_eq!(facts.points, None);
        assert_eq!(facts.title(&unscored), "Flight by Dario B.");
        assert_eq!(
            facts.description(&unscored),
            "4 July 2026 · 2:14 h in the air"
        );
    }

    #[test]
    fn the_page_carries_what_a_preview_needs() {
        let html = page(&flight(), &url());
        for line in [
            "<title>42.3 km closed FAI triangle by Dario B. · Venti</title>",
            r#"<meta property="og:title" content="42.3 km closed FAI triangle by Dario B.">"#,
            r#"<meta property="og:description" content="4 July 2026 · 2:14 h in the air · 67.70 points · Sample Wing">"#,
            r#"<meta property="og:url" content="https://venti.example/f/0123456789abcdefghjkmnpqrs">"#,
            r#"<meta property="og:image" content="https://venti.example/f/0123456789abcdefghjkmnpqrs/preview.png">"#,
            r#"<meta property="og:image:width" content="1200">"#,
            r#"<meta property="og:image:height" content="630">"#,
            r#"<meta name="twitter:card" content="summary_large_image">"#,
            r#"<meta name="twitter:image" content="https://venti.example/f/0123456789abcdefghjkmnpqrs/preview.png">"#,
            r#"<link rel="canonical" href="https://venti.example/f/0123456789abcdefghjkmnpqrs">"#,
        ] {
            assert!(html.contains(line), "missing: {line}");
        }
        assert!(html.contains("og:image:alt"));
        // A public flight may be indexed; an unlisted one may not.
        assert!(!html.contains("noindex"));
        let mut unlisted = flight();
        unlisted.visibility = Visibility::Unlisted;
        assert!(page(&unlisted, &url()).contains(r#"<meta name="robots" content="noindex">"#));
    }

    #[test]
    fn the_page_is_useful_on_its_own() {
        let html = page(&flight(), &url());
        for part in [
            "<html lang=\"en\">",
            "<h1>42.3 km closed FAI triangle</h1>",
            "<b>Dario B.</b> @dario_b",
            "<time datetime=\"2026-07-04\">4 July 2026</time>",
            "<dt>Airtime</dt><dd>2:14 <span>h</span></dd>",
            "<dt>Scored distance</dt><dd>42.3 <span>km</span></dd>",
            "<dt>Points</dt><dd>67.70</dd>",
            "<dt>Highest point</dt><dd>2817 <span>m</span></dd>",
            "<dt>Take-off</dt><dd>10:30 UTC, 47.6642° N, 11.7967° E, 1620 m</dd>",
            "<dt>Landing</dt><dd>12:44 UTC, 47.6831° N, 11.7604° E, 742 m</dd>",
            "<dt>Best climb</dt><dd>3.4 m/s</dd>",
            "<dt>Glider</dt><dd>Sample Wing</dd>",
            "href=\"/v1/flights/0123456789abcdefghjkmnpqrs/igc\" download",
            "<polyline class=\"line\" points=\"",
            "role=\"img\" aria-label=\"The shape of the track seen from above",
            "aria-label=\"Altitude over the time of the flight: lowest 742 m, highest 2817 m",
            "<figcaption>Altitude over the time of the flight: lowest 742 m, highest 2817 m above sea level.</figcaption>",
            "prefers-color-scheme: dark",
        ] {
            assert!(html.contains(part), "missing: {part}");
        }
        // Self-contained: nothing is fetched to show the page, and nothing
        // runs.
        for absent in [
            "<script",
            "<link rel=\"stylesheet\"",
            "<img",
            "@import",
            "url(",
        ] {
            assert!(!html.contains(absent), "found {absent}");
        }
        // The accent of docs/design.md, light and dark.
        assert!(html.contains(ACCENT_LIGHT) && html.contains("#9085E9"));
    }

    #[test]
    fn nothing_a_pilot_or_a_file_supplies_becomes_markup() {
        let hostile = "<script>alert(1)</script>\"'><img src=x onerror=alert(2)>&amp;";
        let mut flight = flight();
        flight.pilot_name = hostile.to_owned();
        flight.derived.glider = Some(hostile.to_owned());

        let html = page(&flight, &url());

        // The only tags on the page are the ones written in this file.
        assert!(!html.contains("<script"));
        assert!(!html.contains("<img"));
        assert!(!html.contains("onerror=alert(2)>"));
        let escaped = "&lt;script&gt;alert(1)&lt;/script&gt;&quot;&#39;&gt;\
                       &lt;img src=x onerror=alert(2)&gt;&amp;amp;";
        // The name: in the title, in the two preview titles and in the
        // byline. The glider: in the description, in the two preview
        // descriptions and in the list of details.
        assert_eq!(html.matches(escaped).count(), 8, "{html}");
        // No attribute was closed early: every content attribute of the
        // head still ends its line.
        for line in html.lines().filter(|line| line.starts_with("<meta ")) {
            assert!(line.ends_with("\">"), "{line}");
            assert_eq!(line.matches('"').count() % 2, 0, "{line}");
        }
    }

    #[test]
    fn text_is_escaped_for_content_and_for_attributes() {
        assert_eq!(
            Esc("<a href=\"x\" title='y'>&</a>").to_string(),
            "&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;&amp;&lt;/a&gt;"
        );
        assert_eq!(Esc("Jürgen Ö. 山田").to_string(), "Jürgen Ö. 山田");
        assert_eq!(Esc("").to_string(), "");
    }

    #[test]
    fn a_flight_with_little_to_show_still_gets_a_page() {
        let mut bare = flight();
        bare.derived.takeoff = None;
        bare.derived.landing = None;
        bare.derived.score = None;
        bare.derived.glider = None;
        bare.derived.outline = Vec::new();
        bare.derived.profile = Vec::new();

        let html = page(&bare, &url());

        assert!(html.contains("<h1>Flight</h1>"));
        assert!(html.contains("<dt>Start of the recording</dt><dd>10:30 UTC</dd>"));
        assert!(html.contains("<dt>Distance along the track</dt><dd>61.3 <span>km</span></dd>"));
        assert!(!html.contains("<polyline"));
        assert!(!html.contains("Landing"));
        // A single point is a track of no extent.
        bare.derived.outline = vec![[47.0, 11.0]];
        assert!(page(&bare, &url()).contains("<polyline"));
    }

    #[test]
    fn a_track_is_fitted_into_its_box_north_up() {
        // One degree north, then two degrees east at 60° north, where a
        // degree of longitude is half as long: a square on the ground.
        let outline = [[60.0, 10.0], [61.0, 10.0], [61.0, 12.0]];
        let points = fit(&outline, 300.0, 200.0, 20.0);

        let close =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1.5 && (a.1 - b.1).abs() < 1.5;
        // The square is 160 high and wide, centred in the box.
        assert!(close(points[0], (70.0, 180.0)), "{points:?}");
        assert!(close(points[1], (70.0, 20.0)), "{points:?}");
        assert!(close(points[2], (230.0, 20.0)), "{points:?}");

        assert!(fit(&[], 300.0, 200.0, 20.0).is_empty());
        assert_eq!(fit(&[[47.0, 11.0]], 300.0, 200.0, 20.0), [(150.0, 100.0)]);
    }

    #[test]
    fn the_policy_names_the_style_and_allows_no_script() {
        let policy = CONTENT_SECURITY_POLICY.as_str();
        assert!(policy.starts_with("default-src 'none'; style-src 'sha256-"));
        assert!(!policy.contains("unsafe"));
        assert!(!policy.contains("script"));
        assert!(policy.contains("frame-ancestors 'none'"));
        // Test vectors of RFC 4648.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
        // The style in the page is exactly the text the policy names.
        let html = page(&flight(), &url());
        let style = html
            .split_once("<style>")
            .and_then(|(_, rest)| rest.split_once("</style>"))
            .unwrap()
            .0;
        assert_eq!(style, STYLE);
    }

    #[test]
    fn the_public_address_is_an_origin_and_nothing_more() {
        assert!(PublicUrl::new("https://venti.example").is_ok());
        assert!(PublicUrl::new("http://127.0.0.1:8080").is_ok());
        assert!(PublicUrl::new("https://venti.example/").is_err());
        assert!(PublicUrl::new("venti.example").is_err());
        assert!(PublicUrl::new("https://").is_err());
        assert!(PublicUrl::new("https://venti.example\"><script>").is_err());
        assert!(PublicUrl::new("javascript:alert(1)").is_err());
    }

    #[test]
    fn the_page_for_nothing_says_so_without_saying_why() {
        let html = not_found_page();
        assert!(html.contains("<h1>No flight here</h1>"));
        assert!(html.contains("noindex"));
        assert!(!html.contains("private"));
    }
}
