//! From a flight file to everything the server says about the flight.
//!
//! The file is the truth and is kept as it arrived. What is worked out
//! here, with the same `venti-flight` code the apps run on the device, can
//! be worked out again at any time, and is whenever [`ANALYSIS_VERSION`]
//! goes up.
//!
//! This is a pure function of the file's bytes. It can be slow: on a file
//! built for the purpose the analysis takes minutes (measured 2026-11-19:
//! 20 000 fixes that zigzag across the globe keep one core busy for two and
//! a half minutes). That is why the server never calls it directly on a
//! file a client sent; see [`super::analyser`].

use serde::{Deserialize, Serialize};
use venti_domain::GeoPoint;
use venti_flight::analysis::{self, Moment};
use venti_flight::igc::{self, Fix, IgcError};
use venti_flight::scoring::RouteKind;
use venti_flight::{habits, summarise, track};
use venti_store::{Derived, Place, Score, ScoreKind};

/// The version of this analysis. Raise it when a change here or in
/// `venti-flight` changes what a file gives; the server then works every
/// stored flight out again.
///
/// Version 2 (2026-11-23) keeps how each flight was flown
/// ([`venti_flight::habits`]), for the pilot's own "How you fly".
/// Version 3 (2026-11-23) keeps the country the glider took off in, so
/// that flights can be searched by it.
pub const ANALYSIS_VERSION: u32 = 3;

/// The most fixes a file may hold: more than a day at one fix a second.
/// A file within the size limit can hold only a few more, so this refuses
/// almost nothing; it is here so that the bound does not depend on another
/// one staying as it is.
pub const MAX_FIXES: usize = 100_000;

/// What a file may hold beside its positions without question: headers, a
/// declared task, comments, the recorder's signature. Real files hold a
/// few kilobytes of these.
pub const SPARE_BYTES: usize = 64 * 1024;

/// The most one line with a position counts for. The format's own part of
/// such a line is 35 characters, and what recorders add to it (accuracy,
/// speeds, engine noise) is a few dozen more.
const FIX_LINE_BYTES: usize = 128;

/// The most points of an outline, as in the apps' logbooks.
const OUTLINE_POINTS: usize = 96;

/// The most points of the altitude profile on a share page.
const PROFILE_POINTS: usize = 160;

/// The most characters kept of a glider's name.
const GLIDER_LENGTH: usize = 60;

/// What a flight file gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Derivation {
    /// The flight's identity: the hash of its fixes.
    pub identity: String,
    /// Everything else.
    pub derived: Derived,
}

/// Why a file gives nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Unusable {
    /// It has no date header: it is not a flight file.
    #[error("it has no date header, so it is not an IGC flight file")]
    NoDate,
    /// It has no readable position.
    #[error("it holds no readable position")]
    NoFixes,
    /// It holds more fixes than [`MAX_FIXES`].
    #[error("it holds more than {MAX_FIXES} positions")]
    TooManyFixes,
    /// Most of it is something other than positions.
    #[error(
        "most of it is not positions: a flight file is its positions, with a few kilobytes \
         of headers, comments and signature beside them"
    )]
    MostlyNotFlight,
}

/// Reads a flight file and works out its identity and derived values.
pub fn derive(file: &[u8]) -> Result<Derivation, Unusable> {
    let recording = igc::parse(&text_of(file)).map_err(|error| match error {
        IgcError::NoDate => Unusable::NoDate,
        IgcError::NoFixes => Unusable::NoFixes,
    })?;
    if recording.fixes.len() > MAX_FIXES {
        return Err(Unusable::TooManyFixes);
    }
    if !is_mostly_positions(file, recording.fixes.len()) {
        return Err(Unusable::MostlyNotFlight);
    }

    let analysis = analysis::analyse(&recording.fixes);
    let flown = analysis::flown_part(&recording.fixes);
    let summary = summarise(flown).ok_or(Unusable::NoFixes)?;
    let figures = rounded(habits::habits(flown, &analysis));
    // A flight file gives positions to a thousandth of a minute, under two
    // metres. Six decimals of a degree keep all of that.
    let place = |moment: &Moment| Place {
        time: flown[moment.index].time,
        position: GeoPoint::new(
            round(moment.position.lat(), 1e6),
            round(moment.position.lon(), 1e6),
        )
        .unwrap_or(moment.position),
        altitude_m: moment.altitude_m,
    };

    Ok(Derivation {
        identity: venti_flight::identity(&recording.fixes),
        derived: Derived {
            version: ANALYSIS_VERSION,
            start: summary.start,
            // A recorder whose clock jumped back can end before it began.
            airtime_s: summary.duration_s().max(0),
            takeoff: analysis.takeoff.as_ref().map(place),
            landing: analysis.landing.as_ref().map(place),
            track_length_m: round(summary.track_length_m, 10.0),
            max_altitude_m: summary.max_altitude_m,
            max_climb_ms: round(summary.max_climb_ms, 100.0),
            score: analysis.score.map(|score| Score {
                kind: match score.kind {
                    RouteKind::FreeFlight => ScoreKind::FreeFlight,
                    RouteKind::FreeTriangle => ScoreKind::FreeTriangle,
                    RouteKind::FaiTriangle => ScoreKind::FaiTriangle,
                },
                closed: score.closed,
                distance_m: round(score.distance_m, 10.0),
                points: round(score.points, 100.0),
            }),
            glider: recording.glider_type.as_deref().and_then(tidy),
            outline: outline(flown),
            profile: track::altitude_profile(flown, PROFILE_POINTS)
                .into_iter()
                .map(|point| {
                    let seconds = i32::try_from(point.seconds).unwrap_or(i32::MAX);
                    [seconds, point.altitude_m]
                })
                .collect(),
            // Counts and sums of finite numbers, which JSON always holds.
            habits: serde_json::to_string(&figures).ok(),
            takeoff_country: analysis
                .takeoff
                .as_ref()
                .and_then(|takeoff| super::country::country_at(takeoff.position))
                .map(str::to_owned),
        },
    })
}

/// Whether a file with `fixes` readable positions is mostly those
/// positions.
///
/// The server keeps a file byte for byte and hands it out to whoever may
/// see the flight. The flight file reader skips every line it does not
/// know, so without this a date, one position and four megabytes of
/// anything at all were a flight, and an account a place to keep and hand
/// out files. What is not a position may be as large as the positions,
/// plus [`SPARE_BYTES`]: no real recording comes near that, and one line
/// of random bytes does not get past it.
///
/// A line counts as a position if it begins like one, and all such lines
/// together count for no more than [`FIX_LINE_BYTES`] per position the
/// reader could read: lines that only begin like positions do not add up.
///
/// This does not stop anyone who writes their bytes into lines that read
/// as positions. Nothing in the format can; what an account may keep in
/// total is the bound on that.
fn is_mostly_positions(file: &[u8], fixes: usize) -> bool {
    let begin_like_positions: usize = file
        .split(|byte| *byte == b'\n')
        .filter(|line| line.first() == Some(&b'B'))
        .map(|line| line.len() + 1)
        .sum();
    let positions = begin_like_positions.min(fixes.saturating_mul(FIX_LINE_BYTES));
    let other = file.len().saturating_sub(positions);
    other <= positions.saturating_add(SPARE_BYTES)
}

/// The file as text.
///
/// The format is plain ASCII, but recorders write names in whatever
/// encoding their maker liked. A file that is not UTF-8 is read as Latin-1,
/// in which every byte is a character: nothing is refused for a name, and
/// the lines that hold positions are ASCII in either reading.
pub(super) fn text_of(file: &[u8]) -> std::borrow::Cow<'_, str> {
    match std::str::from_utf8(file) {
        Ok(text) => text.into(),
        Err(_) => file.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// Rounds to a fixed number of decimals, given as the power of ten.
///
/// Sub-millimetre digits mean nothing, and without them the same file
/// gives the same answer on any machine, whatever its maths library does
/// in the last bit.
pub(super) fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

/// The figures of how a flight was flown with their two fractions rounded,
/// for the reason [`round`] gives.
fn rounded(mut figures: habits::FlightHabits) -> habits::FlightHabits {
    figures.best_climb_ms = figures.best_climb_ms.map(|climb| round(climb, 100.0));
    figures.glide_distance_m = round(figures.glide_distance_m, 10.0);
    figures
}

/// The simplified track, rounded to about a metre.
fn outline(flown: &[Fix]) -> Vec<[f64; 2]> {
    track::outline(flown, OUTLINE_POINTS)
        .into_iter()
        .map(|point: GeoPoint| [round(point.lat(), 1e5), round(point.lon(), 1e5)])
        .collect()
}

/// Text from a file's header, made fit to store and show: no control
/// characters, not endless, and `None` if nothing is left.
fn tidy(text: &str) -> Option<String> {
    let tidy: String = text
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(GLIDER_LENGTH)
        .collect();
    (!tidy.is_empty()).then_some(tidy)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_flight::testing::synthetic_flight;

    use super::*;

    fn sample() -> String {
        igc::write(&synthetic_flight())
    }

    /// Bytes that are no flight record of any kind: every one of 256
    /// values, over and over, with a line end now and then.
    fn noise(bytes: usize) -> Vec<u8> {
        (0..bytes).map(|n| (n * 131 % 256) as u8).collect()
    }

    #[test]
    fn a_file_that_is_mostly_something_else_is_not_a_flight() {
        // What the security review of 2026-11-19 uploaded, and got back
        // byte for byte: a date, one position, and four megabytes of
        // anything.
        let mut smuggled = b"HFDTEDATE:040726\r\nB1030004739852N01147802EA0162001620\r\n".to_vec();
        smuggled.extend(noise(4 * 1024 * 1024 - smuggled.len()));
        assert_eq!(derive(&smuggled), Err(Unusable::MostlyNotFlight));

        // The same hidden behind a real flight: the positions do not make
        // room for more than their own size again.
        let mut behind_a_flight = sample().into_bytes();
        let flight = behind_a_flight.len();
        behind_a_flight.extend(noise(flight + SPARE_BYTES + 1));
        assert_eq!(derive(&behind_a_flight), Err(Unusable::MostlyNotFlight));

        // Lines that begin like positions and are none count for nothing.
        let mut dressed_up = sample().into_bytes();
        for _ in 0..40_000 {
            dressed_up.extend(b"Bthis is not a position, it only begins like one\r\n");
        }
        assert_eq!(derive(&dressed_up), Err(Unusable::MostlyNotFlight));
        // Nor does a position line of any length count for more than a
        // real one could be.
        let mut one_long_line = b"HFDTEDATE:040726\r\nB1030004739852N01147802EA0162001620".to_vec();
        one_long_line.extend(std::iter::repeat_n(b'7', 500_000));
        assert_eq!(derive(&one_long_line), Err(Unusable::MostlyNotFlight));
    }

    #[test]
    fn what_real_recorders_write_beside_positions_is_no_reason_to_refuse() {
        let flight = sample();
        let identity = derive(flight.as_bytes()).unwrap().identity;

        // A comment after every position, as long as the position itself,
        // and a long signature at the end.
        let mut chatty = String::new();
        for line in flight.lines() {
            chatty.push_str(line);
            chatty.push_str("\r\n");
            if line.starts_with('B') {
                chatty.push_str("LXCTsat 11 hdop 0.9 baro 1013.2 note\r\n");
            }
        }
        for _ in 0..60 {
            chatty.push_str(&format!("G{}\r\n", "4F5A".repeat(18)));
        }
        assert_eq!(derive(chatty.as_bytes()).unwrap().identity, identity);

        // A recording of a minute with everything a file can carry before
        // its first position: sixty kilobytes of headers and task.
        let mut short = String::from("AXVE000\r\nHFDTEDATE:040726,01\r\n");
        for n in 0..1400 {
            short.push_str(&format!("LXVEnote {n:04} about nothing in particular\r\n"));
        }
        for second in 0..60 {
            short.push_str(&format!("B1030{second:02}4739852N01147802EA0162001620\r\n"));
        }
        assert!(short.len() > 60_000 && short.len() < SPARE_BYTES);
        assert!(derive(short.as_bytes()).is_ok());

        // Positions with every extension a recorder may add: lines of
        // nearly a hundred characters.
        let mut extended = String::from("AXVE000\r\nHFDTEDATE:040726,01\r\n");
        for n in 0..1800 {
            let (minute, second) = (n / 60, n % 60);
            let more = "0".repeat(60);
            extended.push_str(&format!(
                "B10{minute:02}{second:02}4739852N01147802EA0162001620{more}\r\n"
            ));
        }
        assert!(derive(extended.as_bytes()).is_ok());
    }

    #[test]
    fn the_sample_flight_gives_what_the_phone_shows_for_it() {
        let derivation = derive(sample().as_bytes()).unwrap();
        let derived = &derivation.derived;

        // The identity the core gives the bundled sample file.
        assert_eq!(derivation.identity, "6c55247b80a34b03fa0bb3bd38173c59");
        assert_eq!(derived.version, ANALYSIS_VERSION);
        // The numbers the core's own tests expect of this flight.
        assert_eq!(derived.airtime_s, 1744);
        assert_eq!(derived.max_altitude_m, 2117);
        assert_eq!(derived.glider.as_deref(), Some("Sample Wing"));
        assert_eq!(
            derived.start,
            Utc.with_ymd_and_hms(2026, 7, 4, 10, 30, 0).unwrap()
        );
        assert!((derived.max_climb_ms - 2.4).abs() < 0.05);
        assert!((12_000.0..22_000.0).contains(&derived.track_length_m));

        let takeoff = derived.takeoff.unwrap();
        assert_eq!(takeoff.time, derived.start);
        assert_eq!(takeoff.altitude_m, 1620);
        assert!((takeoff.position.lat() - 47.6642).abs() < 1e-4);
        // The sample ends in a spiral above the valley floor, not on it.
        assert_eq!(derived.landing, None);

        let score = derived.score.unwrap();
        assert!(score.distance_m > 5000.0 && score.points > 5.0, "{score:?}");

        assert!((3..=OUTLINE_POINTS).contains(&derived.outline.len()));
        assert_eq!(derived.outline[0], [47.6642, 11.7967]);
        assert!((100..=PROFILE_POINTS).contains(&derived.profile.len()));
        assert_eq!(derived.profile[0], [0, 1620]);
        assert_eq!(derived.profile.last().unwrap()[0], 1744);
    }

    #[test]
    fn numbers_are_rounded_to_what_means_something() {
        let derived = derive(sample().as_bytes()).unwrap().derived;
        let decimals = |value: f64, scale: f64| ((value * scale).round() - value * scale).abs();
        assert!(decimals(derived.track_length_m, 10.0) < 1e-6);
        assert!(decimals(derived.max_climb_ms, 100.0) < 1e-9);
        let score = derived.score.unwrap();
        assert!(decimals(score.distance_m, 10.0) < 1e-6);
        assert!(decimals(score.points, 100.0) < 1e-9);
        for [lat, lon] in &derived.outline {
            assert!(decimals(*lat, 1e5) < 1e-6 && decimals(*lon, 1e5) < 1e-6);
        }
    }

    #[test]
    fn headers_change_nothing_but_what_they_name() {
        let renamed = sample()
            .replace("Sample Pilot", "Somebody Else")
            .replace("Sample Wing", "Another Wing");
        let (a, b) = (
            derive(sample().as_bytes()).unwrap(),
            derive(renamed.as_bytes()).unwrap(),
        );
        assert_eq!(a.identity, b.identity);
        assert_eq!(b.derived.glider.as_deref(), Some("Another Wing"));
        assert_eq!(
            Derived {
                glider: None,
                ..a.derived
            },
            Derived {
                glider: None,
                ..b.derived
            }
        );
    }

    #[test]
    fn a_recording_that_never_flies_is_kept_for_what_it_is() {
        // Ten minutes at walking pace on the landing field.
        let mut text = String::from("AXVE000\r\nHFDTE040726\r\n");
        for second in 0..600 {
            let minutes = 39.852 + f64::from(second) * 0.0005;
            text.push_str(&format!(
                "B10{:02}{:02}47{:05.0}N01147802EA0075000786\r\n",
                second / 60,
                second % 60,
                minutes * 1000.0
            ));
        }

        let derived = derive(text.as_bytes()).unwrap().derived;

        assert_eq!(derived.takeoff, None);
        assert_eq!(derived.landing, None);
        assert_eq!(derived.airtime_s, 599, "the length of the recording");
        assert_eq!(derived.glider, None);
    }

    #[test]
    fn what_is_not_a_flight_file_is_refused() {
        assert_eq!(derive(b""), Err(Unusable::NoDate));
        assert_eq!(derive(b"hello"), Err(Unusable::NoDate));
        assert_eq!(derive(b"\x89PNG\r\n\x1a\n\0\0\0"), Err(Unusable::NoDate));
        assert_eq!(derive(&[0xff; 4096]), Err(Unusable::NoDate));
        assert_eq!(
            derive(b"AXVE000\r\nHFDTE040726\r\n"),
            Err(Unusable::NoFixes)
        );
        assert_eq!(
            derive(b"AXVE000\r\nHFDTE040726\r\nB10300047\r\n"),
            Err(Unusable::NoFixes)
        );
    }

    #[test]
    fn a_file_with_too_many_fixes_is_refused_before_it_is_analysed() {
        // One fix a second from midnight on, for a day and some hours: the
        // reader keeps only fixes whose time moves forward, so the lines
        // must differ.
        let mut text = String::from("AXVE000\r\nHFDTE040726\r\n");
        let line_length = "B0000004739852N01147802EA0075000786\r\n".len();
        text.reserve(line_length * (MAX_FIXES + 1));
        for second in 0..=MAX_FIXES {
            let of_day = second % 86_400;
            text.push_str(&format!(
                "B{:02}{:02}{:02}4739852N01147802EA0075000786\r\n",
                of_day / 3600,
                of_day / 60 % 60,
                of_day % 60
            ));
        }
        let began = std::time::Instant::now();

        assert_eq!(derive(text.as_bytes()), Err(Unusable::TooManyFixes));

        assert!(began.elapsed() < std::time::Duration::from_secs(5));
        // One fewer is a file like any other.
        let within = &text[..text.len() - line_length];
        assert!(derive(within.as_bytes()).is_ok());
    }

    #[test]
    fn a_name_that_is_not_utf8_does_not_cost_the_flight() {
        // "Flügel" as a recorder with a Western European code page writes it.
        let latin1: Vec<u8> = sample()
            .replace("Sample Wing", "Fl\u{00FC}gel")
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap())
            .collect();
        assert!(std::str::from_utf8(&latin1).is_err());

        let derivation = derive(&latin1).unwrap();

        assert_eq!(derivation.derived.glider.as_deref(), Some("Flügel"));
        assert_eq!(derivation.identity, "6c55247b80a34b03fa0bb3bd38173c59");
    }

    #[test]
    fn a_recording_whose_clock_jumps_does_not_take_the_server_down() {
        // Recorders lose their clock now and then. Here the second half of
        // the sample is stamped an hour earlier than the first.
        let text = sample();
        let lines: Vec<&str> = text.lines().collect();
        let half = lines.len() / 2;
        let mut jumped = String::new();
        for (index, line) in lines.iter().enumerate() {
            match line.strip_prefix("B1") {
                Some(rest) if index > half => {
                    jumped.push_str("B0");
                    jumped.push_str(rest);
                }
                _ => jumped.push_str(line),
            }
            jumped.push_str("\r\n");
        }
        assert_ne!(jumped, text);

        // The flight engine keeps only fixes whose time moves forward, so
        // the half stamped earlier is left out and what remains is the
        // first half of the flight. Whatever it makes of such a file, an
        // answer or a panic, must be over quickly; the server analyses
        // uploads in a child process, so a panic would cost that child and
        // nothing else.
        let began = std::time::Instant::now();
        let outcome = std::panic::catch_unwind(|| derive(jumped.as_bytes()));
        assert!(began.elapsed() < std::time::Duration::from_secs(20));
        if let Ok(Ok(derivation)) = outcome {
            assert_ne!(derivation.identity, "6c55247b80a34b03fa0bb3bd38173c59");
            assert!(derivation.derived.airtime_s >= 0, "never negative");
            assert!(derivation.derived.airtime_s < 1744, "half a flight at most");
        }
    }

    #[test]
    fn a_glider_name_is_tidied_before_it_is_kept() {
        assert_eq!(tidy("  Wing\u{7} 2\t "), Some("Wing 2".to_owned()));
        assert_eq!(tidy("\u{1b}[31m"), Some("[31m".to_owned()));
        assert_eq!(tidy(" \u{0} "), None);
        assert_eq!(tidy(&"w".repeat(500)).unwrap().len(), GLIDER_LENGTH);
        // Markup is kept as the text it is; whoever shows it escapes it.
        assert_eq!(
            tidy("<script>alert(1)</script>"),
            Some("<script>alert(1)</script>".to_owned())
        );
    }
}
