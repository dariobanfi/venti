//! A flight cut into pieces is one flight.
//!
//! When the system ends the app in flight, Android starts the recording
//! service again by itself, and the new cockpit records from the take-off
//! it notices in the air (decision log, 2026-11-22). The logbook then
//! receives two recordings of one flight: the first cut short in the air,
//! the second begun in the air a little later and near where the first
//! ended. [`continues`] says whether a later recording takes over from an
//! earlier one, [`joined`] makes one flight file of the two, and
//! [`pieces`] finds the pieces again in a joined file.
//!
//! The joined file is the first recording as it was, a line that says the
//! recording resumed after a cut and how long the cut was, and the second
//! recording from its first fix on, as it was: the gap stays a gap and
//! nothing is invented in it. Only recordings of Venti's own recorder are
//! joined. Another recorder's file may carry a signature that a join
//! would break, and its maker's rules for a cut are not ours to guess.
//!
//! A recording made the usual way is never joined: it begins with the
//! standing before the run and ends a while after the landing, and a
//! second flight after a landing begins on the ground. The bounds are our
//! own; *to verify* with recordings of real cuts.
//!
//! Pieces come into the logbook in their order on a phone. If a flight
//! was cut twice and its first and last pieces came in before the middle
//! one, with less than [`LONGEST_CUT_S`] between them, the two are joined
//! across the middle, which then stays a flight of its own.

use venti_flight::analysis;
use venti_flight::igc::{self, IgcFile};

/// The longest cut that is joined, seconds. Android restarts the service
/// within seconds; the new cockpit then waits for a position and for the
/// take-off rule, which a cold receiver can stretch to a few minutes.
pub(crate) const LONGEST_CUT_S: i64 = 600;

/// How far the glider may have come during a cut: this much, plus
/// [`CUT_SPEED_MS`] for every second of it. 20 m/s is 72 km/h over the
/// ground, more than a paraglider makes but with a strong wind behind it.
const CUT_DISTANCE_M: f64 = 300.0;
/// See [`CUT_DISTANCE_M`].
const CUT_SPEED_MS: f64 = 20.0;

/// The line a joined file carries where a piece begins, followed by the
/// cut's length: free text of the format's own kind, which every reader
/// passes over.
const RESUMED: &str = "LXVE recording resumed after a cut of";

/// The maker code Venti's own recorder writes.
const OWN_RECORDER: &str = "XVE";

/// Whether `later` takes over from `earlier`: both are Venti's own
/// recordings, `earlier` ended in the air and `later` began in the air, a
/// little later and near where `earlier` ended.
pub(crate) fn continues(earlier: &IgcFile, later: &IgcFile) -> bool {
    let (Some(last), Some(first)) = (earlier.fixes.last(), later.fixes.first()) else {
        return false;
    };
    if earlier.manufacturer != OWN_RECORDER || later.manufacturer != OWN_RECORDER {
        return false;
    }
    let cut_s = (first.time - last.time).num_seconds();
    if !(1..=LONGEST_CUT_S).contains(&cut_s) {
        return false;
    }
    if last.position.distance_m(first.position) > CUT_DISTANCE_M + CUT_SPEED_MS * cut_s as f64 {
        return false;
    }
    // The flight computer is the slow part: last.
    let (_, ended_in_the_air) = analysis::in_the_air_at_its_ends(&earlier.fixes);
    ended_in_the_air && analysis::in_the_air_at_its_ends(&later.fixes).0
}

/// One flight file of two recordings, `later` taking over from `earlier`
/// ([`continues`]): `earlier` whole, the line that marks the cut, and
/// `later` from its first fix on. Its header, and so its pilot and glider,
/// are `earlier`'s.
pub(crate) fn joined(earlier_text: &str, later_text: &str, cut_s: i64) -> String {
    let mut text = earlier_text.trim_end_matches(['\r', '\n']).to_owned();
    text.push_str("\r\n");
    text.push_str(&format!("{RESUMED} {cut_s} s\r\n"));
    let mut offset = 0;
    for line in later_text.split_inclusive('\n') {
        if line.starts_with('B') {
            break;
        }
        offset += line.len();
    }
    text.push_str(&later_text[offset..]);
    text
}

/// The identities of the recordings a joined file was made of, in order,
/// as [`venti_flight::identity`] gives them for each recording by itself.
/// Empty for a file that was never joined.
pub(crate) fn pieces(text: &str) -> Vec<String> {
    if !text.lines().any(|line| line.starts_with(RESUMED)) {
        return Vec::new();
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let header_ends = lines
        .iter()
        .position(|line| line.starts_with('B'))
        .unwrap_or(lines.len());
    let header = lines[..header_ends].concat();
    let identity = |piece: &str| {
        igc::parse(piece)
            .ok()
            .map(|file| venti_flight::identity(&file.fixes))
    };
    let mut pieces = Vec::new();
    let mut piece = header.clone();
    for line in &lines[header_ends..] {
        if line.starts_with(RESUMED) {
            pieces.extend(identity(&piece));
            piece.clone_from(&header);
        } else {
            piece.push_str(line);
        }
    }
    pieces.extend(identity(&piece));
    pieces
}

/// Made-up recordings as the cockpit writes them, for this crate's tests.
#[cfg(test)]
pub(crate) mod testing {
    use chrono::Duration;
    use venti_flight::igc::{self, Fix, IgcFile};
    use venti_flight::testing::synthetic_flight;

    /// The made-up flight as the cockpit records it by itself: half a
    /// minute standing at the take-off first, two minutes at rest where it
    /// came down last.
    pub(crate) fn recording() -> IgcFile {
        let mut flight = synthetic_flight();
        flight.recorder_id = "001".to_owned();
        let first = flight.fixes[0];
        let last = *flight.fixes.last().expect("the flight has fixes");
        let standing = (1..=30).rev().map(|second| Fix {
            time: first.time - Duration::seconds(second),
            ..first
        });
        let resting = (1..=120).map(|second| Fix {
            time: last.time + Duration::seconds(second),
            ..last
        });
        flight.fixes = standing
            .chain(flight.fixes.iter().copied())
            .chain(resting)
            .collect();
        flight
    }

    /// [`recording`] cut into two at the fix `at`, with `cut_s` seconds
    /// of it missing: the first piece as the kill left it, the second as
    /// the cockpit recorded it when it came back.
    pub(crate) fn cut(at: usize, cut_s: usize) -> (String, String) {
        cut_file(&recording(), at, cut_s)
    }

    /// Any recording cut as [`cut`] does.
    pub(crate) fn cut_file(file: &IgcFile, at: usize, cut_s: usize) -> (String, String) {
        let first = IgcFile {
            fixes: file.fixes[..at].to_vec(),
            ..file.clone()
        };
        let second = IgcFile {
            fixes: file.fixes[at + cut_s..].to_vec(),
            ..file.clone()
        };
        (igc::write(&first), igc::write(&second))
    }

    /// A recording's fixes moved by `seconds`.
    pub(crate) fn later_by(file: &IgcFile, seconds: i64) -> IgcFile {
        IgcFile {
            fixes: file
                .fixes
                .iter()
                .map(|fix| Fix {
                    time: fix.time + Duration::seconds(seconds),
                    ..*fix
                })
                .collect(),
            ..file.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{cut, later_by, recording};
    use super::*;

    fn parse(text: &str) -> IgcFile {
        igc::parse(text).unwrap()
    }

    #[test]
    fn a_recording_made_the_usual_way_begins_and_ends_on_the_ground() {
        assert_eq!(
            analysis::in_the_air_at_its_ends(&recording().fixes),
            (false, false)
        );
    }

    #[test]
    fn a_cut_of_ten_seconds_and_one_of_five_minutes_are_taken_over() {
        for cut_s in [10, 300] {
            let (first, second) = cut(700, cut_s);
            let (first, second) = (parse(&first), parse(&second));
            assert_eq!(
                analysis::in_the_air_at_its_ends(&first.fixes),
                (false, true)
            );
            assert_eq!(
                analysis::in_the_air_at_its_ends(&second.fixes),
                (true, false)
            );
            assert!(continues(&first, &second), "a cut of {cut_s} s");
            assert!(!continues(&second, &first), "the wrong way round");
        }
    }

    #[test]
    fn a_second_flight_after_the_landing_is_not_taken_over() {
        let first = recording();
        // Off again a minute after the recording ended, from where it
        // landed: a short hop down the valley, begun on the ground.
        let landed = *first.fixes.last().unwrap();
        let mut second = later_by(&recording(), 0);
        let shift = (landed.time - second.fixes[0].time).num_seconds() + 60;
        second = later_by(&second, shift);
        assert!(!continues(&first, &second));

        // Not even when the second is cut so that it begins in the air.
        let begun_in_the_air = IgcFile {
            fixes: second.fixes[200..].to_vec(),
            ..second.clone()
        };
        assert!(!continues(&first, &begun_in_the_air));
    }

    #[test]
    fn a_cut_too_long_too_far_or_of_another_recorder_is_not_taken_over() {
        let (first, second) = cut(700, 10);
        let (first, second) = (parse(&first), parse(&second));
        assert!(!continues(&first, &later_by(&second, LONGEST_CUT_S)));

        let mut far = second.clone();
        for fix in &mut far.fixes {
            fix.position =
                venti_domain::GeoPoint::new(fix.position.lat() + 0.05, fix.position.lon()).unwrap();
        }
        assert!(!continues(&first, &far));

        let other = IgcFile {
            manufacturer: "XCT".to_owned(),
            ..second
        };
        assert!(!continues(&first, &other));
    }

    #[test]
    fn a_joined_file_is_the_first_whole_the_cut_and_the_second_from_its_first_fix() {
        let (first, second) = cut(700, 10);
        let text = joined(&first, &second, 10);

        assert!(text.starts_with(&first));
        assert!(text.contains("\r\nLXVE recording resumed after a cut of 10 s\r\nB"));
        let file = parse(&text);
        let whole = recording();
        assert_eq!(file.fixes.len(), whole.fixes.len() - 10);
        // In the order of its fixes, with the gap a gap.
        assert!(
            file.fixes
                .windows(2)
                .all(|pair| pair[0].time < pair[1].time)
        );
        assert_eq!(
            (file.fixes[700].time - file.fixes[699].time).num_seconds(),
            11
        );
        assert_eq!(file.pilot.as_deref(), Some("Sample Pilot"));

        assert_eq!(
            pieces(&text),
            [
                venti_flight::identity(&parse(&first).fixes),
                venti_flight::identity(&parse(&second).fixes),
            ]
        );
        assert!(pieces(&first).is_empty());
    }
}
