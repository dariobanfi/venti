//! What it takes to hand a pilot's flight to a league.
//!
//! Two leagues, two ways:
//!
//! - **XContest** publishes no interface for other apps. A pilot claims a
//!   flight on XContest's own page, signed in there, and uploads the file
//!   by hand. [`xcontest`] knows the page's address; an app hands the pilot
//!   the file and opens the page.
//! - **The DHV-XC** publishes an upload interface for apps in its rules: a
//!   request with the pilot's user name, an upload password the pilot sets
//!   in their profile, and the file. [`dhvxc`] puts that request together
//!   and reads the answer.
//!
//! Both take a flight for their contests for fourteen days after the day
//! it was flown ([`claim_ends`]), and both check the recorder's signature
//! in the file ([`FileFacts`]): a file without one that validates is
//! listed and not scored.
//!
//! Nothing here talks to a league or reads a clock. What is said about
//! the leagues was read on their own pages on 2026-11-20;
//! `docs/data-sources.md` has the addresses and the words.

pub mod dhvxc;
pub mod xcontest;

use chrono::{Days, NaiveTime};
use venti_domain::Timestamp;

/// For how many days after the day of a flight XContest and the DHV-XC
/// take it for their contests.
///
/// XContest's rules, 3.1: "The deadline for uploading a flight is 14 days
/// after day of the flight." The DHV-XC's, 6.5: "spätestens 14 Tage
/// (Flugtag x + 14 Tage / 24:00 Uhr UTC)".
pub const CLAIM_DAYS: u64 = 14;

/// The first moment at which a flight that began at `start` is too late
/// for a league's contest: midnight UTC after the fourteenth day following
/// the day of the flight.
///
/// The DHV-XC names UTC. XContest's rules name no time zone; *to verify*
/// whether it counts the days where the pilot flew.
pub fn claim_ends(start: Timestamp) -> Timestamp {
    let day = start.date_naive();
    day.checked_add_days(Days::new(CLAIM_DAYS + 1))
        .unwrap_or(day)
        .and_time(NaiveTime::MIN)
        .and_utc()
}

/// What a flight file says about itself that a league asks about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    /// Whether the file carries a signature: at least one `G` record with
    /// something in it. Whether the signature is one a league can check is
    /// the league's to say when the file arrives.
    pub signed: bool,
    /// The three characters by which the file names what recorded it, from
    /// its `A` record: "XCT", "FLY". Empty when the file has no such
    /// record.
    pub recorder: String,
}

impl FileFacts {
    /// Reads the facts from the text of an IGC file.
    pub fn of(igc_text: &str) -> Self {
        let mut signed = false;
        let mut recorder = String::new();
        for line in igc_text.lines() {
            match line.as_bytes().first() {
                Some(b'G') if line.trim_end().len() > 1 => signed = true,
                Some(b'A') if recorder.is_empty() => {
                    recorder = line
                        .chars()
                        .skip(1)
                        .take(3)
                        .filter(char::is_ascii_alphanumeric)
                        .collect();
                }
                _ => {}
            }
        }
        Self { signed, recorder }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn a_flight_is_taken_until_the_end_of_the_fourteenth_day_after_its_day() {
        // Flown on 4 July, in the morning or at the day's last second.
        for start in [
            Utc.with_ymd_and_hms(2026, 7, 4, 10, 30, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 4, 23, 59, 59).unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 4, 0, 0, 0).unwrap(),
        ] {
            // The 18th is the last day; the 19th begins too late.
            assert_eq!(
                claim_ends(start),
                Utc.with_ymd_and_hms(2026, 7, 19, 0, 0, 0).unwrap()
            );
        }
        // Across the end of a year.
        assert_eq!(
            claim_ends(Utc.with_ymd_and_hms(2026, 12, 25, 12, 0, 0).unwrap()),
            Utc.with_ymd_and_hms(2027, 1, 9, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn a_file_says_what_recorded_it_and_whether_it_is_signed() {
        let signed = "AXCT4b2f9c3e\r\nHFDTE040726\r\nB1030004739852N01147802EA0162001655\r\n\
                      G5a1b2c3d4e5f\r\nG6a7b8c9d\r\n";
        assert_eq!(
            FileFacts::of(signed),
            FileFacts {
                signed: true,
                recorder: "XCT".to_owned()
            }
        );
        // A file as this project's own recorder writes it: no signature.
        let plain = "AXVE000\r\nHFDTE040726\r\nB1030004739852N01147802EA0162001655\r\n";
        assert_eq!(
            FileFacts::of(plain),
            FileFacts {
                signed: false,
                recorder: "XVE".to_owned()
            }
        );
        // A `G` record with nothing in it signs nothing, and a file without
        // an `A` record names no recorder.
        let empty = "HFDTE040726\nG\nG  \n";
        assert_eq!(
            FileFacts::of(empty),
            FileFacts {
                signed: false,
                recorder: String::new()
            }
        );
        // Whatever stands where the code should is not taken for one.
        assert_eq!(FileFacts::of("A<>\"\n").recorder, "");
    }
}
