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
use venti_flight::{summarise, track};
use venti_store::{Derived, Place, Score, ScoreKind};

/// The version of this analysis. Raise it when a change here or in
/// `venti-flight` changes what a file gives; the server then works every
/// stored flight out again.
pub const ANALYSIS_VERSION: u32 = 1;

/// The most fixes a file may hold: more than a day at one fix a second.
/// A file within the size limit can hold only a few more, so this refuses
/// almost nothing; it is here so that the bound does not depend on another
/// one staying as it is.
pub const MAX_FIXES: usize = 100_000;

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

    let analysis = analysis::analyse(&recording.fixes);
    let flown = analysis::flown_part(&recording.fixes);
    let summary = summarise(flown).ok_or(Unusable::NoFixes)?;
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
        },
    })
}

/// The file as text.
///
/// The format is plain ASCII, but recorders write names in whatever
/// encoding their maker liked. A file that is not UTF-8 is read as Latin-1,
/// in which every byte is a character: nothing is refused for a name, and
/// the lines that hold positions are ASCII in either reading.
fn text_of(file: &[u8]) -> std::borrow::Cow<'_, str> {
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
fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
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
        let mut text = String::from("AXVE000\r\nHFDTE040726\r\n");
        let line = "B1030004739852N01147802EA0075000786\r\n";
        text.reserve(line.len() * (MAX_FIXES + 1));
        for _ in 0..=MAX_FIXES {
            text.push_str(line);
        }
        let began = std::time::Instant::now();

        assert_eq!(derive(text.as_bytes()), Err(Unusable::TooManyFixes));

        assert!(began.elapsed() < std::time::Duration::from_secs(5));
        // One fewer is a file like any other.
        let within = &text[..text.len() - line.len()];
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

        // The flight engine takes the file as it is: the recording now ends
        // ten hours before it began. Whatever it makes of that, an answer
        // or a panic, must be over quickly; the server analyses uploads in
        // a child process, so a panic would cost that child and nothing
        // else. Today it answers, and the server keeps the nonsense within
        // bounds a client can rely on.
        let began = std::time::Instant::now();
        let outcome = std::panic::catch_unwind(|| derive(jumped.as_bytes()));
        assert!(began.elapsed() < std::time::Duration::from_secs(20));
        if let Ok(Ok(derivation)) = outcome {
            assert_ne!(derivation.identity, "6c55247b80a34b03fa0bb3bd38173c59");
            assert_eq!(derivation.derived.airtime_s, 0, "never negative");
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
