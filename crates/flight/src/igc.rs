//! Reading and writing IGC flight files.
//!
//! IGC is the flight-recording format defined by the FAI gliding commission
//! and used by every paragliding league and logger. A file is lines of text,
//! each starting with a letter that says what the line is:
//!
//! - `A`: the recorder's manufacturer code and serial;
//! - `H`: header fields such as the date, pilot and glider;
//! - `B`: one position fix;
//! - others (task, events, extensions, the `G` signature) that this reader
//!   skips for now.
//!
//! A `B` line has fixed columns:
//!
//! ```text
//! B HHMMSS DDMMmmmN DDDMMmmmE V PPPPP GGGGG
//!   time   latitude  longitude  | |     GNSS altitude, metres
//!                               | pressure altitude, metres
//!                               A = 3D fix, V = no valid altitude
//! ```
//!
//! Real files are untidy, so the reader is forgiving: a line it cannot read is
//! skipped and counted, and only a file with no date or no fixes is an error.
//! It has been tested against the format specification and files this crate
//! wrote itself. Files from real recorders are still to be collected as test
//! fixtures.

use std::fmt::Write as _;

use chrono::{Duration, NaiveDate, NaiveTime, Timelike};
use venti_domain::{GeoPoint, Timestamp};

/// One position from a flight file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    /// When the fix was taken.
    pub time: Timestamp,
    /// Where.
    pub position: GeoPoint,
    /// Altitude from the satellite fix, metres.
    pub gps_altitude_m: i32,
    /// Altitude from the pressure sensor on the standard atmosphere, metres.
    /// Recorders without a pressure sensor write zero here.
    pub pressure_altitude_m: i32,
    /// Whether the recorder marked the fix as a full 3D fix.
    pub valid: bool,
}

/// The contents of a flight file.
#[derive(Debug, Clone, PartialEq)]
pub struct IgcFile {
    /// Three-character code of the recorder's maker, for example `XCT`.
    pub manufacturer: String,
    /// The recorder's serial, as written after the maker code.
    pub recorder_id: String,
    /// The UTC date of the first fix.
    pub date: NaiveDate,
    /// The pilot's name, if the file gives one.
    pub pilot: Option<String>,
    /// The glider model, if the file gives one.
    pub glider_type: Option<String>,
    /// The glider's registration or identifier, if the file gives one.
    pub glider_id: Option<String>,
    /// The fixes in the order they were recorded.
    pub fixes: Vec<Fix>,
    /// How many lines could not be read and were skipped.
    pub skipped_lines: usize,
}

impl IgcFile {
    /// Whether any fix carries a pressure altitude.
    pub fn has_pressure_altitude(&self) -> bool {
        self.fixes.iter().any(|fix| fix.pressure_altitude_m != 0)
    }
}

/// Why a flight file could not be used at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IgcError {
    /// The file has no date header, so its times cannot be placed on a day.
    #[error("the file has no date header")]
    NoDate,
    /// The file has no readable position fixes.
    #[error("the file has no readable position fixes")]
    NoFixes,
}

/// Reads a flight file.
pub fn parse(text: &str) -> Result<IgcFile, IgcError> {
    let mut manufacturer = String::new();
    let mut recorder_id = String::new();
    let mut date = None;
    let mut pilot = None;
    let mut glider_type = None;
    let mut glider_id = None;
    let mut raw_fixes = Vec::new();
    let mut skipped_lines = 0;

    for line in text.lines() {
        let line = line.trim_end();
        match line.as_bytes().first() {
            Some(b'A') if manufacturer.is_empty() => {
                // Maker code, then the serial up to any free text after a hyphen or colon.
                manufacturer = line.chars().skip(1).take(3).collect();
                recorder_id = line
                    .chars()
                    .skip(4)
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
            }
            Some(b'H') => match header(line) {
                Some(("DTE", value)) => match header_date(value) {
                    Some(parsed) => date = date.or(Some(parsed)),
                    None => skipped_lines += 1,
                },
                Some(("PLT", value)) => pilot = non_empty(value),
                Some(("GTY", value)) => glider_type = non_empty(value),
                Some(("GID", value)) => glider_id = non_empty(value),
                Some(_) => {}
                None => skipped_lines += 1,
            },
            Some(b'B') => match raw_fix(line) {
                Some(fix) => raw_fixes.push(fix),
                None => skipped_lines += 1,
            },
            _ => {}
        }
    }

    let date = date.ok_or(IgcError::NoDate)?;
    if raw_fixes.is_empty() {
        return Err(IgcError::NoFixes);
    }

    // B lines carry only a time of day. A flight that passes midnight UTC shows
    // as the time jumping back by most of a day, and continues on the next date.
    let mut fixes = Vec::with_capacity(raw_fixes.len());
    let mut day = date;
    let mut previous: Option<NaiveTime> = None;
    for raw in raw_fixes {
        if let Some(previous) = previous {
            let back_by = previous.num_seconds_from_midnight() as i64
                - raw.time.num_seconds_from_midnight() as i64;
            if back_by > 12 * 3600 {
                day += Duration::days(1);
            }
        }
        previous = Some(raw.time);
        fixes.push(Fix {
            time: day.and_time(raw.time).and_utc(),
            position: raw.position,
            gps_altitude_m: raw.gps_altitude_m,
            pressure_altitude_m: raw.pressure_altitude_m,
            valid: raw.valid,
        });
    }

    Ok(IgcFile {
        manufacturer,
        recorder_id,
        date,
        pilot,
        glider_type,
        glider_id,
        fixes,
        skipped_lines,
    })
}

/// Writes a flight file.
///
/// The output has the `A`, `H` and `B` lines a league upload needs, with the
/// line endings the specification requires. It carries no `G` signature yet,
/// so leagues will treat it as unsigned.
pub fn write(file: &IgcFile) -> String {
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push_str("\r\n");
    };

    line(format!("A{}{}", file.manufacturer, file.recorder_id));
    line(format!("HFDTEDATE:{},01", file.date.format("%d%m%y")));
    if let Some(pilot) = &file.pilot {
        line(format!("HFPLTPILOTINCHARGE:{pilot}"));
    }
    if let Some(glider_type) = &file.glider_type {
        line(format!("HFGTYGLIDERTYPE:{glider_type}"));
    }
    if let Some(glider_id) = &file.glider_id {
        line(format!("HFGIDGLIDERID:{glider_id}"));
    }
    line("HFDTMGPSDATUM:WGS84".to_owned());

    for fix in &file.fixes {
        let mut text = String::with_capacity(35);
        let _ = write!(text, "B{}", fix.time.format("%H%M%S"));
        push_angle(&mut text, fix.position.lat(), 2, 'N', 'S');
        push_angle(&mut text, fix.position.lon(), 3, 'E', 'W');
        text.push(if fix.valid { 'A' } else { 'V' });
        push_altitude(&mut text, fix.pressure_altitude_m);
        push_altitude(&mut text, fix.gps_altitude_m);
        line(text);
    }
    out
}

struct RawFix {
    time: NaiveTime,
    position: GeoPoint,
    gps_altitude_m: i32,
    pressure_altitude_m: i32,
    valid: bool,
}

/// Splits an `H` line into its three-letter subject and its value.
///
/// `HFPLTPILOTINCHARGE:Jane Doe` gives `("PLT", "Jane Doe")`. Older files omit
/// the long name and the colon for the date: `HFDTE181126` gives
/// `("DTE", "181126")`.
fn header(line: &str) -> Option<(&str, &str)> {
    let subject = line.get(2..5)?;
    let rest = line.get(5..)?;
    let value = match rest.split_once(':') {
        Some((_, value)) => value,
        None => rest,
    };
    Some((subject, value.trim()))
}

/// Reads the date from a date header value: `181126` or `181126,01`.
fn header_date(value: &str) -> Option<NaiveDate> {
    let digits = value.get(..6)?;
    NaiveDate::parse_from_str(digits, "%d%m%y").ok()
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn raw_fix(line: &str) -> Option<RawFix> {
    if !line.is_ascii() || line.len() < 35 {
        return None;
    }
    let time = NaiveTime::parse_from_str(&line[1..7], "%H%M%S").ok()?;
    let lat = angle(&line[7..9], &line[9..14], &line[14..15], "N", "S")?;
    let lon = angle(&line[15..18], &line[18..23], &line[23..24], "E", "W")?;
    let valid = match &line[24..25] {
        "A" => true,
        "V" => false,
        _ => return None,
    };
    Some(RawFix {
        time,
        position: GeoPoint::new(lat, lon).ok()?,
        pressure_altitude_m: line[25..30].parse().ok()?,
        gps_altitude_m: line[30..35].parse().ok()?,
        valid,
    })
}

/// Reads degrees and thousandths of minutes into signed decimal degrees.
fn angle(
    degrees: &str,
    milli_minutes: &str,
    hemisphere: &str,
    positive: &str,
    negative: &str,
) -> Option<f64> {
    let all_digits = |text: &str| text.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(degrees) || !all_digits(milli_minutes) {
        return None;
    }
    let degrees: f64 = degrees.parse().ok()?;
    let minutes: f64 = milli_minutes.parse::<f64>().ok()? / 1000.0;
    if minutes >= 60.0 {
        return None;
    }
    let value = degrees + minutes / 60.0;
    if hemisphere == positive {
        Some(value)
    } else if hemisphere == negative {
        Some(-value)
    } else {
        None
    }
}

/// Writes an angle as degrees and thousandths of minutes with its hemisphere.
fn push_angle(text: &mut String, value: f64, degree_digits: usize, positive: char, negative: char) {
    let hemisphere = if value < 0.0 { negative } else { positive };
    // Work in whole thousandths of a minute so that 59.9996 minutes rounds
    // into the next degree instead of printing as 60.000.
    let total = (value.abs() * 60_000.0).round() as u64;
    let (degrees, milli_minutes) = (total / 60_000, total % 60_000);
    let _ = write!(
        text,
        "{degrees:0degree_digits$}{milli_minutes:05}{hemisphere}"
    );
}

/// Writes an altitude in five columns; negative values carry a leading minus.
fn push_altitude(text: &mut String, metres: i32) {
    let metres = metres.clamp(-9999, 99_999);
    if metres < 0 {
        let _ = write!(text, "-{:04}", -metres);
    } else {
        let _ = write!(text, "{metres:05}");
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, TimeZone, Utc};

    use super::*;

    /// The example fix from the IGC specification: 16:02:40 UTC,
    /// 54° 07.121' N, 002° 49.342' W, 280 m pressure, 421 m GNSS.
    const SPEC_FIX: &str = "B1602405407121N00249342WA002800042120509950";

    fn file_with(lines: &[&str]) -> String {
        let mut text = String::from("AXCT1234567\r\nHFDTE181126\r\n");
        for line in lines {
            text.push_str(line);
            text.push_str("\r\n");
        }
        text
    }

    #[test]
    fn reads_the_example_fix_from_the_specification() {
        let file = parse(&file_with(&[SPEC_FIX])).unwrap();
        let fix = file.fixes[0];
        assert_eq!(
            fix.time,
            Utc.with_ymd_and_hms(2026, 11, 18, 16, 2, 40).unwrap()
        );
        assert!((fix.position.lat() - (54.0 + 7.121 / 60.0)).abs() < 1e-9);
        assert!((fix.position.lon() + (2.0 + 49.342 / 60.0)).abs() < 1e-9);
        assert_eq!(fix.pressure_altitude_m, 280);
        assert_eq!(fix.gps_altitude_m, 421);
        assert!(fix.valid);
        assert_eq!(file.manufacturer, "XCT");
        assert_eq!(file.recorder_id, "1234567");
    }

    #[test]
    fn reads_both_styles_of_date_header_and_the_named_headers() {
        let text = "AXCT001\r\n\
                    HFDTEDATE:150724,01\r\n\
                    HFPLTPILOTINCHARGE: Jane Doe \r\n\
                    HFGTYGLIDERTYPE:Example Wing 2\r\n\
                    HFGIDGLIDERID:\r\n\
                    B1000004740000N01145000EA0150001520\r\n";
        let file = parse(text).unwrap();
        assert_eq!(file.date, NaiveDate::from_ymd_opt(2024, 7, 15).unwrap());
        assert_eq!(file.pilot.as_deref(), Some("Jane Doe"));
        assert_eq!(file.glider_type.as_deref(), Some("Example Wing 2"));
        assert_eq!(file.glider_id, None);
    }

    #[test]
    fn a_flight_past_midnight_continues_on_the_next_day() {
        let file = parse(&file_with(&[
            "B2359504740000N01145000EA0150001520",
            "B0000104740000N01145000EA0150001520",
        ]))
        .unwrap();
        assert_eq!(file.fixes[0].time.day(), 18);
        assert_eq!(file.fixes[1].time.day(), 19);
        assert_eq!((file.fixes[1].time - file.fixes[0].time).num_seconds(), 20);
    }

    #[test]
    fn unreadable_lines_are_skipped_and_counted() {
        let file = parse(&file_with(&[
            SPEC_FIX,
            "B160241",                             // cut short
            "B1602425407121X00249342WA0028000421", // bad hemisphere
            "B1602435467121N00249342WA0028000421", // 67 minutes
            "B1602445407121N00249342WZ0028000421", // bad validity flag
            "B160245540712ÄN00249342WA0028000421", // not ASCII
            "LXCTsome comment",
            "B1602465407121N00249342WV-001200005",
        ]))
        .unwrap();
        assert_eq!(file.fixes.len(), 2);
        assert_eq!(file.skipped_lines, 5);
        assert_eq!(file.fixes[1].pressure_altitude_m, -12);
        assert!(!file.fixes[1].valid);
    }

    #[test]
    fn a_file_without_a_date_or_without_fixes_is_an_error() {
        assert_eq!(
            parse("AXCT001\r\nB1602405407121N00249342WA0028000421\r\n"),
            Err(IgcError::NoDate)
        );
        assert_eq!(parse("AXCT001\r\nHFDTE181126\r\n"), Err(IgcError::NoFixes));
        assert_eq!(parse(""), Err(IgcError::NoDate));
    }

    #[test]
    fn what_is_written_reads_back_the_same() {
        let original = parse(&file_with(&[
            SPEC_FIX,
            "B1602505407200N00249100WA0029000430",
            "B1603004659999S17959999EV-001200005",
        ]))
        .unwrap();
        let mut expected = original.clone();
        expected.pilot = Some("Jane Doe".to_owned());
        expected.glider_type = Some("Example Wing 2".to_owned());

        let text = write(&expected);
        assert!(text.lines().count() == 8 && text.contains("\r\n"));
        let read_back = parse(&text).unwrap();

        assert_eq!(read_back.fixes.len(), expected.fixes.len());
        for (a, b) in read_back.fixes.iter().zip(&expected.fixes) {
            assert_eq!(a.time, b.time);
            assert!((a.position.lat() - b.position.lat()).abs() < 1e-9);
            assert!((a.position.lon() - b.position.lon()).abs() < 1e-9);
            assert_eq!(
                (a.gps_altitude_m, a.pressure_altitude_m, a.valid),
                (b.gps_altitude_m, b.pressure_altitude_m, b.valid)
            );
        }
        assert_eq!(read_back.pilot, expected.pilot);
        assert_eq!(read_back.glider_type, expected.glider_type);
    }

    #[test]
    fn minutes_that_round_up_to_sixty_carry_into_the_degree() {
        let mut text = String::new();
        push_angle(&mut text, 47.999_999_9, 2, 'N', 'S');
        assert_eq!(text, "4800000N");
        let mut text = String::new();
        push_angle(&mut text, -11.5, 3, 'E', 'W');
        assert_eq!(text, "01130000W");
    }
}
