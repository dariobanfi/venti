//! The page a link to a live track shows.
//!
//! It is for the people a pilot sends the link to: someone at home, the
//! driver who fetches the pilot from wherever the flight ended. They have
//! no app and no account, and may open the link on any phone. So the page
//! says everything in words, in one HTML document with its style inside,
//! with no script, no font and no request to anywhere else: who, where the
//! glider is, how old that position is, how high, and whether it has
//! landed.
//!
//! A page without a script cannot fetch the next position. It asks the
//! browser to load it again every few seconds instead, and offers a link
//! that stops that, for those whom a page that reloads disturbs.
//!
//! It never says more than it knows. A track that has gone quiet is not
//! "landed", and the age of a position is given as of the moment the page
//! was made, which the page states.
//!
//! As on the page of a shared flight, everything a pilot supplied (the
//! display name) reaches the page only through [`Esc`], and the page
//! forbids itself all scripts.

use std::fmt::Write as _;
use std::sync::LazyLock;

use chrono::Timelike;
use venti_api_types::v1::{LiveTrackId, live_page_path};
use venti_domain::{GeoPoint, Timestamp};
use venti_store::{LiveFix, LiveTrack, LiveVisibility};

use super::Limits;
use crate::flights::share::{Esc, PublicUrl, STYLE, date, fit, points_attribute, policy_for};

/// How many of a track's newest fixes the page draws its line from: half an
/// hour at a fix a second. The page is made anew for every visitor every
/// few seconds, so it reads the end of a track and not all of it.
pub const TRAIL_FIXES: usize = 1800;

/// How many points the drawn line has at most.
const TRAIL_POINTS: usize = 240;

/// After how many seconds the browser loads the page again while positions
/// are arriving, and while they are not.
const RENEW_SECONDS: (u32, u32) = (15, 60);

/// What the live page adds to the style of the page of a shared flight.
const OWN_STYLE: &str = "
.status { margin: 0 0 1.5rem; font-size: 1.125rem; }
.note { margin: 0 0 1.5rem; color: var(--quiet); font-size: 0.875rem; }
.note a { color: var(--accent); }
.note a:focus-visible { outline: 3px solid CanvasText; outline-offset: 3px; }
";

/// The page's whole style: the shared flight page's, so that the two look
/// alike, and a few rules of its own.
static PAGE_STYLE: LazyLock<String> = LazyLock::new(|| format!("{STYLE}{OWN_STYLE}"));

/// What the page allows a browser to load and run: its own style and
/// nothing else. No script, no frames, no forms.
pub static CONTENT_SECURITY_POLICY: LazyLock<String> = LazyLock::new(|| policy_for(&PAGE_STYLE));

/// How a track stands at the moment a page is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Positions are arriving, and no landing was reported.
    UnderWay,
    /// The app has said that the glider landed.
    Landed,
    /// No landing was reported, and the newest position is older than a
    /// public track stays listed. Nothing is known beyond that.
    Quiet,
}

/// What the page says about a track, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    /// How the track stands.
    pub state: State,
    /// The headline.
    pub headline: String,
    /// One or two sentences on what is known.
    pub status: String,
    /// How old the newest position is, for a sentence: `4 seconds`.
    pub age: String,
}

impl Words {
    /// The words for a track at `now`.
    pub fn of(track: &LiveTrack, now: Timestamp, limits: &Limits) -> Self {
        let seconds = (now - track.latest.time).num_seconds().max(0);
        let age = age_in_words(seconds);
        let name = &track.pilot_name;
        let state = if track.landed {
            State::Landed
        } else if seconds > limits.listed.num_seconds() {
            State::Quiet
        } else {
            State::UnderWay
        };
        let (headline, status) = match state {
            State::UnderWay => (
                format!("{name} is under way"),
                format!("No landing has been reported. The last position is {age} old."),
            ),
            State::Landed => (
                format!("{name} has landed"),
                format!(
                    "The pilot's app has reported the landing. The last position is {age} old."
                ),
            ),
            State::Quiet => (
                format!("{name} was last heard of {age} ago"),
                format!(
                    "No landing has been reported, and no position has arrived for {age}. \
                     This is what a phone without a network or with an empty battery looks \
                     like from here. It does not say what happened."
                ),
            ),
        };
        Self {
            state,
            headline,
            status,
            age,
        }
    }
}

/// `1 second`, `42 seconds`, `9 minutes`, `2 hours 14 minutes`.
fn age_in_words(seconds: i64) -> String {
    let plural = |count: i64, unit: &str| {
        if count == 1 {
            format!("1 {unit}")
        } else {
            format!("{count} {unit}s")
        }
    };
    let seconds = seconds.max(0);
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => plural(seconds, "second"),
        (0, minutes) => plural(minutes, "minute"),
        (hours, 0) => plural(hours, "hour"),
        (hours, minutes) => format!("{} {}", plural(hours, "hour"), plural(minutes, "minute")),
    }
}

/// The same age, short, as a number and its unit: `42 s`, `9 min`,
/// `2:14 h`.
fn age_as_number(seconds: i64) -> (String, &'static str) {
    let seconds = seconds.max(0);
    let minutes = seconds / 60;
    if minutes == 0 {
        (seconds.to_string(), "s")
    } else if minutes < 60 {
        (minutes.to_string(), "min")
    } else {
        (format!("{}:{:02}", minutes / 60, minutes % 60), "h")
    }
}

/// A climb rate with its sign, to a tenth: `+1.2`, `−0.8`, and `0.0` for
/// what rounds to nothing. The minus is the real one, as wide as the plus.
fn signed(value: f64) -> String {
    let tenths = (value * 10.0).round() / 10.0;
    if tenths == 0.0 {
        "0.0".to_owned()
    } else {
        format!("{tenths:+.1}").replace('-', "−")
    }
}

/// `19:04:56 UTC`.
fn clock_seconds(time: Timestamp) -> String {
    format!(
        "{:02}:{:02}:{:02} UTC",
        time.hour(),
        time.minute(),
        time.second()
    )
}

/// `47.6642° N, 11.7967° E`.
fn coordinates(position: GeoPoint) -> String {
    let (lat, lon) = (position.lat(), position.lon());
    format!(
        "{:.4}° {}, {:.4}° {}",
        lat.abs(),
        if lat < 0.0 { 'S' } else { 'N' },
        lon.abs(),
        if lon < 0.0 { 'W' } else { 'E' },
    )
}

/// The eighth of the compass a course points into.
fn compass(course_deg: f64) -> &'static str {
    const POINTS: [&str; 8] = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    let eighth = (course_deg.rem_euclid(360.0) / 45.0).round() as usize % 8;
    POINTS[eighth]
}

/// At most [`TRAIL_POINTS`] of the trail's positions, evenly spread, the
/// newest always among them.
fn thinned(trail: &[LiveFix]) -> Vec<[f64; 2]> {
    let point = |fix: &LiveFix| [fix.position.lat(), fix.position.lon()];
    if trail.len() <= TRAIL_POINTS {
        return trail.iter().map(point).collect();
    }
    let last = trail.len() - 1;
    (0..TRAIL_POINTS)
        .map(|n| point(&trail[n * last / (TRAIL_POINTS - 1)]))
        .collect()
}

/// The size of the drawing of the trail, in the units of its own
/// coordinates.
const TRAIL_BOX: (f64, f64) = (640.0, 360.0);

/// The drawing of the end of the track, and the sentence that says what it
/// shows.
fn trail_svg(track: &LiveTrack, trail: &[LiveFix]) -> (String, String) {
    let (width, height) = TRAIL_BOX;
    let points = fit(&thinned(trail), width, height, 28.0);
    let whole = trail.len() as u64 >= u64::from(track.fixes);
    let span = match (trail.first(), trail.last()) {
        (Some(first), Some(last)) => (last.time - first.time).num_seconds(),
        _ => 0,
    };
    let what = if whole {
        "The track so far".to_owned()
    } else {
        format!("The last {} of the track", age_in_words(span))
    };
    let caption = format!(
        "{what}, seen from above with north up. The filled dot is the newest position. \
         There is no map under it."
    );
    let mut svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"{}\">",
        Esc(&caption)
    );
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        let _ = write!(
            svg,
            "<polyline class=\"line\" points=\"{}\"/>\
             <circle class=\"start\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"7\"/>\
             <circle class=\"end\" cx=\"{:.1}\" cy=\"{:.1}\" r=\"9\"/>",
            points_attribute(&points),
            first.0,
            first.1,
            last.0,
            last.1
        );
    }
    svg.push_str("</svg>");
    (svg, caption)
}

/// The page for a live track that may be shown, as it stands at `now`.
///
/// `trail` is the end of the track, oldest first; `renew` says whether the
/// page asks the browser to load it again by itself.
///
/// # Panics
///
/// Panics if the track's identifier is not of the form the API takes,
/// which no stored track's is.
pub fn page(
    track: &LiveTrack,
    trail: &[LiveFix],
    now: Timestamp,
    renew: bool,
    public_url: &PublicUrl,
    limits: &Limits,
) -> String {
    let id: LiveTrackId = track
        .id
        .parse()
        .expect("a stored live track has a well-formed identifier");
    let words = Words::of(track, now, limits);
    let latest = &track.latest;
    let age_s = (now - latest.time).num_seconds().max(0);
    let path = live_page_path(&id);
    let page_url = format!("{}{path}", public_url.as_str());
    // What a chat app shows of the link is kept by the chat for good. It
    // says whose track this is and nothing that will be untrue in a
    // minute: no position, no "in the air".
    let title = format!("Live track of {}", track.pilot_name);
    let description = "Where the glider is, how high, and whether it has landed, as the \
                       pilot's app reports it. Shared with Venti.";
    let every = match words.state {
        State::UnderWay => RENEW_SECONDS.0,
        State::Landed | State::Quiet => RENEW_SECONDS.1,
    };

    let mut html = String::with_capacity(16 * 1024);
    html.push_str(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n",
    );
    if renew {
        let _ = writeln!(html, "<meta http-equiv=\"refresh\" content=\"{every}\">");
    }
    let _ = write!(
        html,
        "<title>{title} · Venti</title>\n\
         <meta name=\"description\" content=\"{description}\">\n\
         <link rel=\"canonical\" href=\"{page_url}\">\n\
         <meta property=\"og:type\" content=\"website\">\n\
         <meta property=\"og:site_name\" content=\"Venti\">\n\
         <meta property=\"og:title\" content=\"{title}\">\n\
         <meta property=\"og:description\" content=\"{description}\">\n\
         <meta property=\"og:url\" content=\"{page_url}\">\n\
         <meta name=\"twitter:card\" content=\"summary\">\n\
         <meta name=\"twitter:title\" content=\"{title}\">\n\
         <meta name=\"twitter:description\" content=\"{description}\">\n\
         <style>{style}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">Venti</a></header>\n<main>\n\
         <h1>{headline}</h1>\n\
         <p class=\"byline\"><b>{pilot}</b> @{handle} · live track</p>\n\
         <p class=\"status\">{status}</p>\n",
        title = Esc(&title),
        description = Esc(description),
        page_url = Esc(&page_url),
        style = PAGE_STYLE.as_str(),
        headline = Esc(&words.headline),
        pilot = Esc(&track.pilot_name),
        handle = Esc(&track.pilot_handle),
        status = Esc(&words.status),
    );

    // The numbers someone looks for first, large.
    let number = |html: &mut String, name: &str, value: &str, unit: &str| {
        let _ = writeln!(
            html,
            "<div><dt>{}</dt><dd>{} <span>{}</span></dd></div>",
            Esc(name),
            Esc(value),
            Esc(unit)
        );
    };
    html.push_str("<dl class=\"numbers\">\n");
    number(&mut html, "Altitude", &latest.altitude_m.to_string(), "m");
    let (age, unit) = age_as_number(age_s);
    number(&mut html, "Age of the position", &age, unit);
    if let Some(speed) = latest.ground_speed_ms {
        number(
            &mut html,
            "Ground speed",
            &format!("{:.0}", speed * 3.6),
            "km/h",
        );
    }
    if let Some(climb) = latest.climb_ms {
        number(&mut html, "Climb", &signed(climb), "m/s");
    }
    html.push_str("</dl>\n");

    let (trail_drawing, caption) = trail_svg(track, trail);
    let _ = writeln!(
        html,
        "<figure>{trail_drawing}<figcaption>{}</figcaption></figure>",
        Esc(&caption)
    );

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
    detail(&mut html, "Position", &coordinates(latest.position));
    detail(
        &mut html,
        "Time of the position",
        &format!("{}, {}", clock_seconds(latest.time), date(latest.time)),
    );
    detail(
        &mut html,
        "Altitude",
        &format!("{} m above sea level", latest.altitude_m),
    );
    if let Some(course) = latest.course_deg {
        detail(
            &mut html,
            "Moving towards",
            &format!("{:.0}°, {}", course.rem_euclid(360.0), compass(course)),
        );
    }
    detail(
        &mut html,
        "Landed",
        match words.state {
            State::Landed => "Yes, as the pilot's app reports",
            State::UnderWay | State::Quiet => "No landing reported",
        },
    );
    detail(
        &mut html,
        "Track began",
        &format!(
            "{}, {}",
            clock_seconds(track.started_at),
            date(track.started_at)
        ),
    );
    detail(&mut html, "Positions received", &track.fixes.to_string());
    detail(
        &mut html,
        "Who can see this",
        match track.visibility {
            LiveVisibility::Link => "Whoever has this link",
            LiveVisibility::Public => "Everyone: the track is on this server's map",
        },
    );
    html.push_str("</dl>\n");

    let made = clock_seconds(now);
    if renew {
        let _ = writeln!(
            html,
            "<p class=\"note\">This page was made at {made} and loads itself again every \
             {every} seconds. <a href=\"?renew=off\">Stop loading it again</a></p>",
        );
    } else {
        let _ = writeln!(
            html,
            "<p class=\"note\">This page was made at {made} and stays as it is. Load it again \
             for the newest position, or <a href=\"{}\">let it load itself again every \
             {every} seconds</a>.</p>",
            Esc(&path),
        );
    }

    let _ = write!(
        html,
        "</main>\n<footer>\n\
         <p>Venti is a free, open-source app for free flight. These positions come from the \
         pilot's own app, and are as right as that phone's reception and satellites. Venti \
         keeps a live track for {kept} after its last position and then removes it. All \
         times are UTC.</p>\n\
         </footer>\n</body>\n</html>\n",
        kept = age_in_words(limits.retention.num_seconds()),
    );
    html
}

/// One of the two short pages that stand in for a track.
fn short_page(title: &str, text: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"robots\" content=\"noindex\">\n\
         <title>{title} · Venti</title>\n\
         <style>{style}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">Venti</a></header>\n<main>\n\
         <h1>{title}</h1>\n\
         <p class=\"byline\">{text}</p>\n\
         </main>\n</body>\n</html>\n",
        style = PAGE_STYLE.as_str(),
    )
}

/// The page shown for a link that leads to no live track anyone may see:
/// none was ever there, its pilot deleted it, or it ended long enough ago
/// to have been removed. It is the same page for all three.
pub fn not_found_page() -> String {
    short_page(
        "No live track here",
        "This link leads to no live track. It may have been mistyped, the pilot may have \
         deleted the track, or the flight ended some days ago: Venti removes a live track \
         a short while after its flight.",
    )
}

/// The page shown when the server cannot look a track up right now.
pub fn trouble_page() -> String {
    short_page(
        "Try again in a moment",
        "Venti cannot show this live track right now. Nothing is wrong with the link.",
    )
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::*;

    fn now() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 18, 19, 5, 0).unwrap()
    }

    fn fix(seconds_ago: i64) -> LiveFix {
        LiveFix {
            time: now() - Duration::seconds(seconds_ago),
            position: GeoPoint::new(47.6831 - seconds_ago as f64 * 1e-5, 11.7604).unwrap(),
            altitude_m: 1620,
            ground_speed_ms: Some(9.5),
            climb_ms: Some(1.24),
            course_deg: Some(320.0),
        }
    }

    /// A public track of twenty minutes whose newest fix is four seconds
    /// old.
    fn track() -> LiveTrack {
        LiveTrack {
            id: "0123456789abcdefghjkmnpqrs".to_owned(),
            account_id: "account".to_owned(),
            pilot_handle: "dario_b".to_owned(),
            pilot_name: "Dario B.".to_owned(),
            visibility: LiveVisibility::Public,
            landed: false,
            started_at: now() - Duration::seconds(1204),
            latest: fix(4),
            fixes: 1201,
            cursor: 1201,
        }
    }

    fn trail() -> Vec<LiveFix> {
        (0..=1200).rev().map(|n| fix(n + 4)).collect()
    }

    fn url() -> PublicUrl {
        PublicUrl::new("https://venti.example").unwrap()
    }

    fn made(track: &LiveTrack, renew: bool) -> String {
        page(track, &trail(), now(), renew, &url(), &Limits::default())
    }

    #[test]
    fn ages_are_worded_for_people() {
        assert_eq!(age_in_words(-5), "0 seconds");
        assert_eq!(age_in_words(1), "1 second");
        assert_eq!(age_in_words(59), "59 seconds");
        assert_eq!(age_in_words(60), "1 minute");
        assert_eq!(age_in_words(9 * 60 + 59), "9 minutes");
        assert_eq!(age_in_words(3600), "1 hour");
        assert_eq!(age_in_words(2 * 3600 + 14 * 60), "2 hours 14 minutes");
        assert_eq!(age_in_words(48 * 3600), "48 hours");

        assert_eq!(age_as_number(-5), ("0".to_owned(), "s"));
        assert_eq!(age_as_number(42), ("42".to_owned(), "s"));
        assert_eq!(age_as_number(9 * 60 + 59), ("9".to_owned(), "min"));
        assert_eq!(age_as_number(2 * 3600 + 14 * 60), ("2:14".to_owned(), "h"));

        assert_eq!(signed(1.24), "+1.2");
        assert_eq!(signed(-0.83), "−0.8");
        assert_eq!(signed(0.04), "0.0");
        assert_eq!(signed(-0.04), "0.0");
        assert_eq!(signed(-12.0), "−12.0");

        assert_eq!(compass(0.0), "north");
        assert_eq!(compass(22.4), "north");
        assert_eq!(compass(22.6), "north-east");
        assert_eq!(compass(320.0), "north-west");
        assert_eq!(compass(359.0), "north");
        assert_eq!(compass(360.0), "north");
        assert_eq!(compass(180.0), "south");
    }

    #[test]
    fn the_words_say_what_is_known_and_no_more() {
        let limits = Limits::default();
        let under_way = Words::of(&track(), now(), &limits);
        assert_eq!(under_way.state, State::UnderWay);
        assert_eq!(under_way.headline, "Dario B. is under way");
        assert_eq!(
            under_way.status,
            "No landing has been reported. The last position is 4 seconds old."
        );

        let mut landed = track();
        landed.landed = true;
        landed.latest = fix(9 * 60);
        let words = Words::of(&landed, now(), &limits);
        assert_eq!(words.state, State::Landed);
        assert_eq!(words.headline, "Dario B. has landed");
        assert!(words.status.contains("The last position is 9 minutes old."));
        // A landing that was reported stays one, however old it is.
        landed.latest = fix(5 * 3600);
        assert_eq!(Words::of(&landed, now(), &limits).state, State::Landed);

        // Half an hour is still "under way"; a second more is not, and is
        // not called a landing either.
        let mut quiet = track();
        quiet.latest = fix(30 * 60);
        assert_eq!(Words::of(&quiet, now(), &limits).state, State::UnderWay);
        quiet.latest = fix(30 * 60 + 1);
        assert_eq!(Words::of(&quiet, now(), &limits).state, State::Quiet);
        quiet.latest = fix(42 * 60);
        let words = Words::of(&quiet, now(), &limits);
        assert_eq!(words.headline, "Dario B. was last heard of 42 minutes ago");
        assert!(
            words
                .status
                .contains("no position has arrived for 42 minutes")
        );
        assert!(words.status.contains("It does not say what happened."));
        assert!(!words.status.contains("landed"));

        // A fix from a phone whose clock runs a minute ahead is not "minus
        // a minute" old.
        let mut ahead = track();
        ahead.latest = fix(-60);
        assert_eq!(Words::of(&ahead, now(), &limits).age, "0 seconds");
    }

    #[test]
    fn the_page_says_in_words_who_where_how_old_how_high_and_whether_landed() {
        let html = made(&track(), true);
        for part in [
            "<html lang=\"en\">",
            "<title>Live track of Dario B. · Venti</title>",
            "<h1>Dario B. is under way</h1>",
            "<b>Dario B.</b> @dario_b · live track",
            "<p class=\"status\">No landing has been reported. The last position is 4 seconds old.</p>",
            "<dt>Altitude</dt><dd>1620 <span>m</span></dd>",
            "<dt>Age of the position</dt><dd>4 <span>s</span></dd>",
            "<dt>Ground speed</dt><dd>34 <span>km/h</span></dd>",
            "<dt>Climb</dt><dd>+1.2 <span>m/s</span></dd>",
            "<dt>Position</dt><dd>47.6831° N, 11.7604° E</dd>",
            "<dt>Time of the position</dt><dd>19:04:56 UTC, 18 November 2026</dd>",
            "<dt>Altitude</dt><dd>1620 m above sea level</dd>",
            "<dt>Moving towards</dt><dd>320°, north-west</dd>",
            "<dt>Landed</dt><dd>No landing reported</dd>",
            "<dt>Track began</dt><dd>18:44:56 UTC, 18 November 2026</dd>",
            "<dt>Positions received</dt><dd>1201</dd>",
            "<dt>Who can see this</dt><dd>Everyone: the track is on this server&#39;s map</dd>",
            "This page was made at 19:05:00 UTC and loads itself again every 15 seconds.",
            "<polyline class=\"line\" points=\"",
            "The track so far, seen from above with north up.",
            "keeps a live track for 48 hours after its last position",
            "prefers-color-scheme: dark",
        ] {
            assert!(html.contains(part), "missing: {part}\n{html}");
        }
        // Self-contained: nothing is fetched to show the page, and nothing
        // runs.
        for absent in [
            "<script",
            "<link rel=\"stylesheet\"",
            "<img",
            "@import",
            "url(",
            "<form",
            "<iframe",
        ] {
            assert!(!html.contains(absent), "found {absent}");
        }
        // Never for search engines, public or not.
        assert!(html.contains("<meta name=\"robots\" content=\"noindex\">"));
    }

    #[test]
    fn what_a_chat_keeps_of_the_link_says_nothing_that_goes_stale() {
        let html = made(&track(), true);
        let head = html.split_once("</head>").unwrap().0;
        for line in [
            r#"<meta property="og:title" content="Live track of Dario B.">"#,
            r#"<meta property="og:url" content="https://venti.example/live/0123456789abcdefghjkmnpqrs">"#,
            r#"<link rel="canonical" href="https://venti.example/live/0123456789abcdefghjkmnpqrs">"#,
            r#"<meta name="twitter:card" content="summary">"#,
        ] {
            assert!(head.contains(line), "missing: {line}");
        }
        // No position, no altitude, no time and no state in the preview.
        let previews: Vec<&str> = head
            .lines()
            .filter(|line| line.contains("og:") || line.contains("twitter:"))
            .chain(
                head.lines()
                    .filter(|line| line.contains("name=\"description\"")),
            )
            .collect();
        assert_eq!(previews.len(), 9);
        for line in previews {
            for stale in ["47.", "11.", "1620", "19:0", "under way", "landed."] {
                assert!(!line.contains(stale), "{stale} in {line}");
            }
        }
        // And no picture: a picture of a position would be kept for good.
        assert!(!head.contains("og:image"));
    }

    #[test]
    fn a_landed_track_and_a_quiet_one_say_so_and_load_again_less_often() {
        let mut landed = track();
        landed.landed = true;
        landed.visibility = LiveVisibility::Link;
        landed.latest = fix(9 * 60);
        let html = made(&landed, true);
        assert!(html.contains("<h1>Dario B. has landed</h1>"));
        assert!(html.contains("<dt>Landed</dt><dd>Yes, as the pilot&#39;s app reports</dd>"));
        assert!(html.contains("<dt>Age of the position</dt><dd>9 <span>min</span></dd>"));
        assert!(html.contains("<dt>Who can see this</dt><dd>Whoever has this link</dd>"));
        assert!(html.contains("<meta http-equiv=\"refresh\" content=\"60\">"));

        let mut quiet = track();
        quiet.latest = fix(42 * 60);
        let html = made(&quiet, true);
        assert!(html.contains("<h1>Dario B. was last heard of 42 minutes ago</h1>"));
        assert!(html.contains("<dt>Landed</dt><dd>No landing reported</dd>"));
        assert!(html.contains("<meta http-equiv=\"refresh\" content=\"60\">"));
    }

    #[test]
    fn the_page_loads_itself_again_only_when_asked_to() {
        let renewing = made(&track(), true);
        assert!(renewing.contains("<meta http-equiv=\"refresh\" content=\"15\">"));
        assert!(renewing.contains("<a href=\"?renew=off\">Stop loading it again</a>"));

        let still = made(&track(), false);
        assert!(!still.contains("http-equiv"));
        assert!(still.contains("and stays as it is"));
        assert!(still.contains(
            "<a href=\"/live/0123456789abcdefghjkmnpqrs\">let it load itself again every 15 seconds</a>"
        ));
    }

    #[test]
    fn a_track_without_speed_climb_or_course_leaves_them_out() {
        let mut bare = track();
        bare.latest.ground_speed_ms = None;
        bare.latest.climb_ms = None;
        bare.latest.course_deg = None;
        bare.fixes = 1;
        let html = page(
            &bare,
            &[bare.latest],
            now(),
            true,
            &url(),
            &Limits::default(),
        );
        for absent in ["Ground speed", "<dt>Climb", "Moving towards"] {
            assert!(!html.contains(absent), "found {absent}");
        }
        // One position is a line of no length with a dot on it.
        assert!(html.contains("<polyline"));
        // And a track the page was handed nothing of still has its words.
        let html = page(&bare, &[], now(), true, &url(), &Limits::default());
        assert!(html.contains("<h1>Dario B. is under way</h1>"));
        assert!(!html.contains("<polyline"));
    }

    #[test]
    fn a_long_track_is_drawn_from_its_end_and_says_so() {
        let mut long = track();
        long.fixes = 20_000;
        let html = page(&long, &trail(), now(), true, &url(), &Limits::default());
        assert!(html.contains("The last 20 minutes of the track, seen from above"));

        // The line has no more points than it should, the newest among
        // them.
        let points = thinned(&trail());
        assert_eq!(points.len(), TRAIL_POINTS);
        let newest = fix(4).position;
        assert_eq!(points.last(), Some(&[newest.lat(), newest.lon()]));
        let oldest = fix(1204).position;
        assert_eq!(points.first(), Some(&[oldest.lat(), oldest.lon()]));
        assert_eq!(thinned(&trail()[..10]).len(), 10);
    }

    #[test]
    fn nothing_a_pilot_supplies_becomes_markup() {
        let hostile = "<script>alert(1)</script>\"'><img src=x onerror=alert(2)>&amp;";
        let mut track = track();
        track.pilot_name = hostile.to_owned();

        let html = made(&track, true);

        assert!(!html.contains("<script"));
        assert!(!html.contains("<img"));
        let escaped = "&lt;script&gt;alert(1)&lt;/script&gt;&quot;&#39;&gt;\
                       &lt;img src=x onerror=alert(2)&gt;&amp;amp;";
        // In the title, the two preview titles, the headline and the
        // byline.
        assert_eq!(html.matches(escaped).count(), 5, "{html}");
        for line in html.lines().filter(|line| line.starts_with("<meta ")) {
            assert!(line.ends_with("\">"), "{line}");
            assert_eq!(line.matches('"').count() % 2, 0, "{line}");
        }
    }

    #[test]
    fn the_policy_names_the_pages_own_style_and_allows_no_script() {
        let policy = CONTENT_SECURITY_POLICY.as_str();
        assert!(policy.starts_with("default-src 'none'; style-src 'sha256-"));
        assert!(!policy.contains("unsafe"));
        assert!(!policy.contains("script"));
        assert!(policy.contains("frame-ancestors 'none'"));
        // It is not the flight page's policy: the style differs.
        assert_ne!(
            policy,
            crate::flights::share::CONTENT_SECURITY_POLICY.as_str()
        );
        // The style in every page is exactly the text the policy names.
        for html in [made(&track(), true), not_found_page(), trouble_page()] {
            let style = html
                .split_once("<style>")
                .and_then(|(_, rest)| rest.split_once("</style>"))
                .unwrap()
                .0;
            assert_eq!(style, PAGE_STYLE.as_str());
            assert_eq!(policy_for(style), policy);
        }
    }

    #[test]
    fn the_page_for_nothing_says_so_without_saying_which() {
        let html = not_found_page();
        assert!(html.contains("<h1>No live track here</h1>"));
        assert!(html.contains("noindex"));
        assert!(!html.contains("http-equiv"));
        assert!(trouble_page().contains("<h1>Try again in a moment</h1>"));
    }
}
