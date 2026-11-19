//! Waypoints: places a pilot navigates to, and the files they come in.
//!
//! Two file formats are read:
//!
//! - **SeeYou CUP** (`.cup`), after Naviter's specification, version 1.2.2
//!   of 2026-11-04, read on 2026-11-19 at
//!   <https://github.com/naviter/seeyou_file_formats/blob/main/CUP_file_format.md>.
//!   Only the waypoints are read; the tasks that may follow them are not.
//! - **GEO** (`.wpt` starting with `$FormatGEO`), the format competition
//!   organisers hand out and GpsDump writes. To verify: no specification of
//!   it was found; the reader follows the layout of the files in use, a
//!   name, a latitude and a longitude each as hemisphere, degrees, minutes
//!   and seconds, a height in metres and a description.
//!
//! [`write_cup`] writes waypoints back as a CUP file, which is how the app
//! stores them: a pilot's waypoints stay a file any other flying program
//! can read.
//!
//! A line that cannot be read is skipped and reported with its number and
//! the reason. Nothing here panics on a broken file.

use venti_domain::GeoPoint;

/// What a waypoint is, as far as it matters for flying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaypointKind {
    /// A place to take off from.
    Takeoff,
    /// A place to land.
    Landing,
    /// Anything else: a turnpoint, a summit, a pass.
    Other,
}

/// A place a pilot navigates to.
#[derive(Debug, Clone, PartialEq)]
pub struct Waypoint {
    /// Its name.
    pub name: String,
    /// A short name, as flight instruments list it. May be empty.
    pub code: String,
    /// Where it is.
    pub position: GeoPoint,
    /// Its height above sea level, metres, if the file gives one.
    pub elevation_m: Option<f64>,
    /// What it is.
    pub kind: WaypointKind,
    /// Whatever else the file says about it.
    pub description: String,
}

/// A line of a file that was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedLine {
    /// Its number in the file, counting from 1.
    pub line: usize,
    /// Why it was skipped.
    pub reason: String,
}

/// What a waypoint file held.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaypointFile {
    /// The waypoints, in the file's order.
    pub waypoints: Vec<Waypoint>,
    /// The lines that could not be read.
    pub skipped: Vec<SkippedLine>,
}

/// Why a file could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WaypointError {
    /// The file is in none of the formats this reader knows.
    #[error(
        "This is not a waypoint file the app can read. It reads SeeYou files (.cup) and GEO files (.wpt that start with $FormatGEO)."
    )]
    UnknownFormat,
    /// The file is in a known format but holds no waypoint.
    #[error("This file holds no waypoints.")]
    Empty,
}

/// The most waypoints one file may hold. A national waypoint file has a
/// few thousand; anything far beyond is not a waypoint file.
pub const MAX_WAYPOINTS: usize = 50_000;

/// Reads a waypoint file in either format.
pub fn parse(text: &str) -> Result<WaypointFile, WaypointError> {
    // A byte-order mark is common in files written on Windows.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or(WaypointError::UnknownFormat)?;

    let file = if first.eq_ignore_ascii_case("$FormatGEO") {
        parse_geo(text)
    } else if let Some(columns) = cup_columns(first) {
        parse_cup(text, &columns)
    } else {
        return Err(WaypointError::UnknownFormat);
    };
    if file.waypoints.is_empty() && file.skipped.is_empty() {
        return Err(WaypointError::Empty);
    }
    Ok(file)
}

/// Writes waypoints as a SeeYou CUP file.
pub fn write_cup(waypoints: &[Waypoint]) -> String {
    let mut text =
        String::from("name,code,country,lat,lon,elev,style,rwdir,rwlen,rwwidth,freq,desc\n");
    for waypoint in waypoints {
        let lat = waypoint.position.lat();
        let lon = waypoint.position.lon();
        let style = match waypoint.kind {
            WaypointKind::Takeoff => 20,
            WaypointKind::Landing => 21,
            WaypointKind::Other => 1,
        };
        let elevation = waypoint
            .elevation_m
            .map(|elevation| format!("{elevation:.1}m"))
            .unwrap_or_default();
        text.push_str(&format!(
            "{},{},,{},{},{},{},,,,,{}\n",
            quoted(&waypoint.name),
            quoted(&waypoint.code),
            cup_angle(lat, 2, if lat < 0.0 { 'S' } else { 'N' }),
            cup_angle(lon, 3, if lon < 0.0 { 'W' } else { 'E' }),
            elevation,
            style,
            quoted(&waypoint.description),
        ));
    }
    text
}

// --- CUP ---

/// The columns a CUP file may have, by position in a line.
#[derive(Debug, Default)]
struct CupColumns {
    name: Option<usize>,
    code: Option<usize>,
    lat: Option<usize>,
    lon: Option<usize>,
    elev: Option<usize>,
    style: Option<usize>,
    freq: Option<usize>,
    desc: Option<usize>,
}

/// Reads a CUP header line. The order of the columns is free, and columns
/// after the style may be missing.
fn cup_columns(header: &str) -> Option<CupColumns> {
    let mut columns = CupColumns::default();
    for (index, field) in split_csv(header).iter().enumerate() {
        match field.trim().to_ascii_lowercase().as_str() {
            "name" | "title" => columns.name = Some(index),
            "code" => columns.code = Some(index),
            "lat" | "latitude" => columns.lat = Some(index),
            "lon" | "longitude" => columns.lon = Some(index),
            "elev" | "elevation" => columns.elev = Some(index),
            "style" => columns.style = Some(index),
            "freq" => columns.freq = Some(index),
            "desc" | "description" => columns.desc = Some(index),
            _ => {}
        }
    }
    (columns.name.is_some() && columns.lat.is_some() && columns.lon.is_some()).then_some(columns)
}

fn parse_cup(text: &str, columns: &CupColumns) -> WaypointFile {
    let mut file = WaypointFile::default();
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()));
    // The header.
    lines.find(|(_, line)| !line.is_empty());

    for (number, line) in lines {
        if line.is_empty() {
            continue;
        }
        // What follows this line are tasks, not waypoints.
        if line.starts_with("-----Related Tasks") {
            break;
        }
        if file.waypoints.len() >= MAX_WAYPOINTS {
            file.skipped.push(SkippedLine {
                line: number,
                reason: format!(
                    "the file holds more than {MAX_WAYPOINTS} waypoints; the rest was left out"
                ),
            });
            break;
        }
        let fields = split_csv(line);
        let field = |column: Option<usize>| {
            column
                .and_then(|index| fields.get(index))
                .map(|field| field.trim())
                .unwrap_or("")
        };
        let mut skip = |reason: &str| {
            file.skipped.push(SkippedLine {
                line: number,
                reason: reason.to_owned(),
            });
        };

        let name = field(columns.name);
        if name.is_empty() {
            skip("it has no name");
            continue;
        }
        let Some(lat) = cup_coordinate(field(columns.lat), 2, 'N', 'S') else {
            skip("its latitude could not be read");
            continue;
        };
        let Some(lon) = cup_coordinate(field(columns.lon), 3, 'E', 'W') else {
            skip("its longitude could not be read");
            continue;
        };
        let Ok(position) = GeoPoint::new(lat, lon) else {
            skip("its position is not on the globe");
            continue;
        };
        let kind = match field(columns.style) {
            "20" => WaypointKind::Takeoff,
            // A paragliding landing zone, and what glider pilots mark as a
            // field to land out in.
            "21" | "3" => WaypointKind::Landing,
            _ => WaypointKind::Other,
        };
        // Older files have one column fewer, the runway width, under a
        // header that names it; two of the specification's own examples
        // do. Their description then sits where the radio frequency
        // belongs, and is recognised by not being a number.
        let mut description = field(columns.desc);
        let frequency = field(columns.freq);
        if description.is_empty() && !frequency.is_empty() && frequency.parse::<f64>().is_err() {
            description = frequency;
        }
        file.waypoints.push(Waypoint {
            name: name.to_owned(),
            code: field(columns.code).to_owned(),
            position,
            elevation_m: length_m(field(columns.elev)),
            kind,
            description: description.to_owned(),
        });
    }
    file
}

/// Splits a line of comma-separated fields. A field in double quotes may
/// hold commas, and a doubled quote inside it stands for one quote.
fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut letters = line.chars().peekable();
    while let Some(letter) = letters.next() {
        match letter {
            '"' if quoted && letters.peek() == Some(&'"') => {
                field.push('"');
                letters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            other => field.push(other),
        }
    }
    fields.push(field);
    fields
}

/// A CUP latitude (`5107.830N`) or longitude (`01410.467E`) in degrees.
fn cup_coordinate(text: &str, degree_digits: usize, positive: char, negative: char) -> Option<f64> {
    let hemisphere = text.chars().last()?.to_ascii_uppercase();
    let sign = if hemisphere == positive {
        1.0
    } else if hemisphere == negative {
        -1.0
    } else {
        return None;
    };
    let digits = &text[..text.len() - hemisphere.len_utf8()];
    if !digits.is_ascii() || digits.len() <= degree_digits {
        return None;
    }
    let degrees: f64 = digits[..degree_digits].parse().ok()?;
    let minutes: f64 = digits[degree_digits..].parse().ok()?;
    (minutes.is_finite() && (0.0..60.0).contains(&minutes))
        .then(|| sign * (degrees + minutes / 60.0))
}

/// A CUP length with its unit: metres when none is given, or feet.
fn length_m(text: &str) -> Option<f64> {
    let text = text.trim().to_ascii_lowercase();
    let (number, factor) = if let Some(number) = text.strip_suffix("ft") {
        (number, 0.3048)
    } else if let Some(number) = text.strip_suffix('m') {
        (number, 1.0)
    } else {
        (text.as_str(), 1.0)
    };
    let value: f64 = number.trim().parse().ok()?;
    value.is_finite().then_some(value * factor)
}

/// Degrees as CUP writes them: degrees, then minutes with three decimals,
/// then the hemisphere.
fn cup_angle(degrees: f64, degree_digits: usize, hemisphere: char) -> String {
    // In thousandths of a minute, so that 59.9996 minutes become the next
    // degree and never "60.000".
    let thousandths = (degrees.abs() * 60_000.0).round() as u64;
    let whole = thousandths / 60_000;
    let minutes = (thousandths % 60_000) as f64 / 1000.0;
    format!("{whole:0degree_digits$}{minutes:06.3}{hemisphere}")
}

/// A text field for a CUP line: in quotes, with quotes inside doubled and
/// line breaks removed. Empty stays empty.
fn quoted(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let clean: String = text
        .chars()
        .map(|letter| if letter.is_control() { ' ' } else { letter })
        .collect();
    format!("\"{}\"", clean.replace('"', "\"\""))
}

// --- GEO ---

fn parse_geo(text: &str) -> WaypointFile {
    let mut file = WaypointFile::default();
    for (index, line) in text.lines().enumerate() {
        let (number, line) = (index + 1, line.trim());
        if line.is_empty() || line.starts_with('$') {
            continue;
        }
        if file.waypoints.len() >= MAX_WAYPOINTS {
            file.skipped.push(SkippedLine {
                line: number,
                reason: format!(
                    "the file holds more than {MAX_WAYPOINTS} waypoints; the rest was left out"
                ),
            });
            break;
        }
        match geo_line(line) {
            Ok(waypoint) => file.waypoints.push(waypoint),
            Err(reason) => file.skipped.push(SkippedLine {
                line: number,
                reason: reason.to_owned(),
            }),
        }
    }
    file
}

/// One GEO line: `B01123  N 46 14 18.84  E 013 34 38.16  1230  Kobala`.
fn geo_line(line: &str) -> Result<Waypoint, &'static str> {
    let mut words = line.split_whitespace();
    let code = words.next().ok_or("it is empty")?;
    let lat = geo_angle(&mut words, 'N', 'S').ok_or("its latitude could not be read")?;
    let lon = geo_angle(&mut words, 'E', 'W').ok_or("its longitude could not be read")?;
    let elevation: f64 = words
        .next()
        .and_then(|word| word.parse().ok())
        .filter(|elevation: &f64| elevation.is_finite())
        .ok_or("its height could not be read")?;
    let description = words.collect::<Vec<_>>().join(" ");
    let position = GeoPoint::new(lat, lon).map_err(|_| "its position is not on the globe")?;
    Ok(Waypoint {
        // The long name is the description where there is one.
        name: if description.is_empty() {
            code.to_owned()
        } else {
            description.clone()
        },
        code: code.to_owned(),
        position,
        elevation_m: Some(elevation),
        kind: WaypointKind::Other,
        description: String::new(),
    })
}

/// A hemisphere letter, degrees, minutes and seconds, as four words.
fn geo_angle<'a>(
    words: &mut impl Iterator<Item = &'a str>,
    positive: char,
    negative: char,
) -> Option<f64> {
    let hemisphere = words.next()?;
    let sign = if hemisphere.eq_ignore_ascii_case(&positive.to_string()) {
        1.0
    } else if hemisphere.eq_ignore_ascii_case(&negative.to_string()) {
        -1.0
    } else {
        return None;
    };
    let degrees: f64 = words.next()?.parse().ok()?;
    let minutes: f64 = words.next()?.parse().ok()?;
    let seconds: f64 = words.next()?.parse().ok()?;
    let valid = (0.0..=180.0).contains(&degrees)
        && (0.0..60.0).contains(&minutes)
        && (0.0..60.0).contains(&seconds);
    valid.then(|| sign * (degrees + minutes / 60.0 + seconds / 3600.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The examples of Naviter's specification, and two paragliding sites.
    const CUP: &str = "name,code,country,lat,lon,elev,style,rwdir,rwlen,rwwidth,freq,desc,userdata,pics\r\n\
\"Cross Hands\",\"CSS\",UK,5147.809N,00405.003W,525ft,1,,,,\"Turn Point, A48/A476, Between Cross Hands and Gorslas, 9 NMl ESE of Camarthen.\"\r\n\
\"Lesce\",\"LJBL\",SI,4621.379N,01410.467E,504.0m,5,144,1130.0m,,123.500,\"Home Airfield\"\r\n\
\"Aiton\",\"O23L\",FR,4533.517N,00614.050E,299.9m,3,110,300.0m,,\"Page 222: O23L Large flat area.\"\r\n\
\"Wallberg \"\"Nord\"\"\",WBN,DE,4739.800N,01147.600E,1620m,20,340,,,,\"Start nach Norden\"\r\n\
Landeplatz,LP,DE,4741.000N,01145.500E,,21,,,,,\r\n\
-----Related Tasks-----\r\n\
\"Task\",\"Lesce\",\"Aiton\",\"Lesce\"\r\n";

    #[test]
    fn a_cup_file_is_read_as_its_specification_describes() {
        let file = parse(CUP).unwrap();
        assert!(file.skipped.is_empty(), "{:?}", file.skipped);
        assert_eq!(file.waypoints.len(), 5);

        let cross_hands = &file.waypoints[0];
        assert_eq!(cross_hands.name, "Cross Hands");
        assert_eq!(cross_hands.code, "CSS");
        // 51 degrees 47.809 minutes north, 4 degrees 5.003 minutes west.
        assert!((cross_hands.position.lat() - 51.796_816_7).abs() < 1e-6);
        assert!((cross_hands.position.lon() + 4.083_383_3).abs() < 1e-6);
        assert!((cross_hands.elevation_m.unwrap() - 160.02).abs() < 0.01);
        assert_eq!(cross_hands.kind, WaypointKind::Other);
        // A comma inside quotes does not end the field.
        assert!(cross_hands.description.starts_with("Turn Point, A48/A476"));

        assert_eq!(file.waypoints[1].elevation_m, Some(504.0));
        // A radio frequency is not taken for a description.
        assert_eq!(file.waypoints[1].description, "Home Airfield");
        assert!(file.waypoints[2].description.starts_with("Page 222"));
        assert_eq!(file.waypoints[2].kind, WaypointKind::Landing);
        assert_eq!(file.waypoints[3].name, "Wallberg \"Nord\"");
        assert_eq!(file.waypoints[3].kind, WaypointKind::Takeoff);
        assert_eq!(file.waypoints[4].kind, WaypointKind::Landing);
        assert_eq!(file.waypoints[4].elevation_m, None);
        // The task after the waypoints is not a waypoint.
        assert!(
            file.waypoints
                .iter()
                .all(|waypoint| waypoint.name != "Task")
        );
    }

    #[test]
    fn a_cup_file_may_order_its_columns_freely_and_leave_some_out() {
        let file =
            parse("lat,lon,elev,name,style\n4739.800N,01147.600E,1620,Wallberg,20\n").unwrap();
        assert_eq!(file.waypoints[0].name, "Wallberg");
        assert_eq!(file.waypoints[0].elevation_m, Some(1620.0));
        assert_eq!(file.waypoints[0].code, "");
    }

    #[test]
    fn a_geo_file_is_read() {
        let text = "$FormatGEO\n\
B01123    N 46 14 18.84    E 013 34 38.16  1230  Kobala Start\n\
A02062    S 22 57 06.00    W 043 12 36.00   696\n";
        let file = parse(text).unwrap();
        assert!(file.skipped.is_empty());

        let kobala = &file.waypoints[0];
        assert_eq!(kobala.name, "Kobala Start");
        assert_eq!(kobala.code, "B01123");
        assert!((kobala.position.lat() - 46.238_566_7).abs() < 1e-6);
        assert!((kobala.position.lon() - 13.577_266_7).abs() < 1e-6);
        assert_eq!(kobala.elevation_m, Some(1230.0));

        // South and west are negative; without a description the code is the name.
        let rio = &file.waypoints[1];
        assert_eq!(rio.name, "A02062");
        assert!((rio.position.lat() + 22.951_666_7).abs() < 1e-6);
        assert!((rio.position.lon() + 43.21).abs() < 1e-6);
    }

    #[test]
    fn lines_that_cannot_be_read_are_reported_and_the_rest_is_kept() {
        let text = "name,code,country,lat,lon,elev,style\n\
,X,DE,4739.800N,01147.600E,1620m,1\n\
Nowhere,X,DE,4739.800,01147.600E,1620m,1\n\
Minutes,X,DE,4761.000N,01147.600E,1620m,1\n\
North pole and beyond,X,DE,9100.000N,01147.600E,1620m,1\n\
Good,X,DE,4739.800N,01147.600E,1620m,1\n";
        let file = parse(text).unwrap();

        assert_eq!(file.waypoints.len(), 1);
        assert_eq!(file.waypoints[0].name, "Good");
        let reported: Vec<(usize, &str)> = file
            .skipped
            .iter()
            .map(|skipped| (skipped.line, skipped.reason.as_str()))
            .collect();
        assert_eq!(
            reported,
            [
                (2, "it has no name"),
                (3, "its latitude could not be read"),
                (4, "its latitude could not be read"),
                (5, "its position is not on the globe"),
            ]
        );

        let geo = parse(
            "$FormatGEO\nB01 N 46 14 18.84 E 013 34\nB02 N 46 14 18.84 E 013 34 38.16 high\n",
        )
        .unwrap();
        assert!(geo.waypoints.is_empty());
        assert_eq!(geo.skipped.len(), 2);
    }

    #[test]
    fn other_files_are_refused_in_words() {
        assert_eq!(parse(""), Err(WaypointError::UnknownFormat));
        assert_eq!(
            parse("AXXX001 Venti\nHFDTE040726\n"),
            Err(WaypointError::UnknownFormat)
        );
        assert_eq!(
            parse("name,code,country,lat,lon,elev,style\n"),
            Err(WaypointError::Empty)
        );
        assert_eq!(parse("$FormatGEO\n"), Err(WaypointError::Empty));
        assert!(WaypointError::UnknownFormat.to_string().contains(".cup"));
    }

    #[test]
    fn broken_input_never_panics() {
        // Every prefix of a real file, and the file with its lines cut
        // short in odd places, including inside a multi-byte letter.
        let text = format!("{CUP}\"Höhe\",H,DE,4739.800N,01147.600E,1620m,20\n");
        for end in 0..text.len() {
            if text.is_char_boundary(end) {
                let _ = parse(&text[..end]);
            }
        }
        for junk in [
            "name,lat,lon\n\"\"\"\"\"",
            "name,lat,lon\nX,N,E",
            "name,lat,lon\nX,ééN,ééE",
            "$FormatGEO\n\u{0} N",
            "lat,lon,name\n,,",
        ] {
            let _ = parse(junk);
        }
    }

    #[test]
    fn written_waypoints_read_back_the_same() {
        let original = parse(CUP).unwrap().waypoints;
        let read_back = parse(&write_cup(&original)).unwrap();
        assert!(read_back.skipped.is_empty());
        assert_eq!(read_back.waypoints.len(), original.len());
        for (written, read) in original.iter().zip(&read_back.waypoints) {
            assert_eq!(written.name, read.name);
            assert_eq!(written.code, read.code);
            assert_eq!(written.kind, read.kind);
            assert_eq!(written.description, read.description);
            // A thousandth of a minute is under two metres.
            assert!(written.position.distance_m(read.position) < 2.0);
            match (written.elevation_m, read.elevation_m) {
                (Some(written), Some(read)) => assert!((written - read).abs() < 0.06),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
        }

        // Minutes that round up to sixty carry into the degrees.
        let edge = Waypoint {
            name: "Edge".to_owned(),
            code: String::new(),
            position: GeoPoint::new(46.999_999_9, -11.999_999_9).unwrap(),
            elevation_m: None,
            kind: WaypointKind::Other,
            description: "line\nbreak".to_owned(),
        };
        let text = write_cup(&[edge]);
        assert!(text.contains("4700.000N,01200.000W"), "{text}");
        assert!(text.contains("\"line break\""), "{text}");
    }
}
