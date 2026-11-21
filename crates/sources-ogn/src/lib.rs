//! Adapter for the live positions of the Open Glider Network: FLARM,
//! FANET, OGN trackers and others, as the network's receivers hear them.
//!
//! The network hands its positions out over APRS-IS, a connection that
//! stays open: after one line of login the server sends a line for every
//! position its receivers heard inside the client's filter, and a comment
//! line about every twenty seconds (the network's wiki, pages "APRS
//! interaction examples" and "OGN flavoured APRS", read 2026-11-21 at
//! <http://wiki.glidernet.org/>). Beside it the network keeps a database of
//! devices (the DDB), in which the owner of a device says whether it may be
//! tracked and whether it may be identified.
//!
//! An aircraft's line looks like this:
//!
//! ```text
//! FLRE0A5BA>OGFLR,qAS,Wallberg:/190509h4744.13N\01227.47E^342/049/A=005524 !W52! id1EE0A5BA -454fpm +0.1rot 8.8dB gps4x5
//! ```
//!
//! - the sender, `>`, the kind of beacon and the path, `:`;
//! - `/` and the time of day, `hhmmss` and `h`, in UTC;
//! - the latitude in degrees and minutes to a hundredth, a symbol table
//!   character, the longitude, a symbol character;
//! - the course in degrees and the speed in knots, `ccc/sss`, and the
//!   altitude in feet above mean sea level, `/A=dddddd`;
//! - `!Wab!`: the third decimal of the latitude's minutes and of the
//!   longitude's;
//! - `idXXYYYYYY`: one byte of flags and the device's address. The byte
//!   is `STttttaa`, from the most significant bit: the stealth flag, the
//!   no-tracking flag, the aircraft type (FLARM's numbers, 7 a paraglider,
//!   6 a hang glider) and the type of address (ICAO, FLARM, OGN);
//! - the climb in feet a minute, `+123fpm`, and what the receiver says
//!   about the signal, which is not read.
//!
//! A line without the id field is no aircraft: a receiver's own position
//! or status, a FANET weather station, a status line. A line with an id
//! field of another length (Naviter's, of ten hexadecimal digits on
//! 2026-11-21) is not read: its form is not on the wiki. To verify.

pub mod fixtures;

use std::collections::BTreeMap;

use chrono::{Duration, NaiveTime};
use serde::Deserialize;
use venti_domain::{BoundingBox, GeoPoint, Timestamp};
use venti_sources::{
    Aircraft, DeviceChoice, DeviceDirectory, LiveLine, LivePosition, LiveSource, Manifest, Request,
    SourceError,
};

/// The network's APRS servers: one name for several machines.
pub const HOST: &str = "aprs.glidernet.org";

/// The port that takes a filter.
pub const PORT: u16 = 14580;

/// The device database as JSON, with the owners' choices.
pub const DIRECTORY_URL: &str = "https://ddb.glidernet.org/download/?j=1&t=1";

/// The name the server logs in with. A login with passcode -1 may read
/// and not send; the name is the client's own.
const LOGIN_NAME: &str = "VENTI-RO";

const KNOT_MS: f64 = 1852.0 / 3600.0;
const FOOT_M: f64 = 0.3048;

/// The Open Glider Network as a source of live positions.
#[derive(Debug, Clone)]
pub struct Ogn {
    manifest: Manifest,
}

impl Ogn {
    /// Creates the adapter.
    ///
    /// # Panics
    ///
    /// Panics if the manifest compiled into the crate is invalid, which the
    /// crate's tests rule out.
    pub fn new() -> Self {
        let manifest = Manifest::from_toml(include_str!("../manifest.toml"))
            .expect("the manifest compiled into venti-sources-ogn is valid");
        Self { manifest }
    }
}

impl Default for Ogn {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveSource for Ogn {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn server(&self) -> (&str, u16) {
        (HOST, PORT)
    }

    fn login(&self, area: &BoundingBox) -> String {
        login(area)
    }

    fn keepalive(&self) -> String {
        "# venti keepalive\r\n".to_owned()
    }

    fn parse_line(&self, line: &str, received: Timestamp) -> LiveLine {
        parse_line(line, received)
    }

    fn directory(&self) -> Request {
        Request::new("ddb", DIRECTORY_URL)
    }

    fn parse_directory(&self, body: &[u8]) -> Result<DeviceDirectory, SourceError> {
        parse_directory(body)
    }
}

/// The login line for the positions inside `area`: a read-only login
/// (passcode -1) and APRS-IS's area filter, `a/north/west/south/east`.
pub fn login(area: &BoundingBox) -> String {
    format!(
        "user {LOGIN_NAME} pass -1 vers Venti {} filter a/{:.4}/{:.4}/{:.4}/{:.4}\r\n",
        env!("CARGO_PKG_VERSION"),
        area.north(),
        area.west(),
        area.south(),
        area.east(),
    )
}

/// What one line of the feed is.
pub fn parse_line(line: &str, received: Timestamp) -> LiveLine {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.starts_with('#') {
        return LiveLine::Server;
    }
    let Some((_head, body)) = line.split_once(':') else {
        return LiveLine::Other(Some("no ':' between the header and the body".into()));
    };
    // A position with a time: `/` or `@`. Anything else is a status
    // line, a message or a position without a time, none of them an
    // aircraft's.
    if !(body.starts_with('/') || body.starts_with('@')) {
        return LiveLine::Other(None);
    }
    let Some(id) = body.split_whitespace().find(|word| word.starts_with("id")) else {
        return LiveLine::Other(None);
    };
    let hex = &id[2..];
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return LiveLine::Other(Some(format!("id field of another form: {id}")));
    }
    let Ok(flags) = u8::from_str_radix(&hex[..2], 16) else {
        return LiveLine::Other(Some("id field".into()));
    };
    // "Note that no messages with the no-tracking set flag should ever
    // appear on the public APRS network"; the stealth flag "should never
    // be 1". Neither is shown, and nothing else of the line is read.
    if flags & 0xC0 != 0 {
        return LiveLine::Hidden;
    }
    match position(body, &hex[2..], flags, received) {
        Ok(position) => LiveLine::Position(position),
        Err(reason) => LiveLine::Other(Some(reason)),
    }
}

fn aircraft(flags: u8) -> Aircraft {
    match (flags >> 2) & 0x0F {
        0x1 => Aircraft::Glider,
        0x2 => Aircraft::TowPlane,
        0x3 => Aircraft::Helicopter,
        0x4 => Aircraft::Parachute,
        0x5 => Aircraft::DropPlane,
        0x6 => Aircraft::HangGlider,
        0x7 => Aircraft::Paraglider,
        0x8 => Aircraft::PoweredAircraft,
        0x9 => Aircraft::JetAircraft,
        0xB => Aircraft::Balloon,
        0xC => Aircraft::Airship,
        0xD => Aircraft::Drone,
        0xF => Aircraft::Obstacle,
        _ => Aircraft::Unknown,
    }
}

fn digits(text: &str, what: &str) -> Result<f64, String> {
    if text.bytes().all(|b| b.is_ascii_digit() || b == b'.') && !text.is_empty() {
        text.parse::<f64>()
            .map_err(|_| format!("{what} is not a number"))
    } else {
        Err(format!("{what} is not a number: {text:?}"))
    }
}

/// The position in `body`, which begins with `/` or `@`.
fn position(
    body: &str,
    address: &str,
    flags: u8,
    received: Timestamp,
) -> Result<LivePosition, String> {
    let bytes = body.as_bytes();
    // `/hhmmssh` `ddmm.mmN` table `dddmm.mmE` symbol: 27 characters.
    if bytes.len() < 27 || !body.is_ascii() {
        return Err("the position is too short".into());
    }
    if bytes[7] != b'h' {
        return Err("the time is not hhmmss in UTC".into());
    }
    let time = time_of(&body[1..7], received)?;

    let lat_deg = digits(&body[8..10], "the latitude")?;
    let lat_min = digits(&body[10..15], "the latitude")?;
    let north = match bytes[15] {
        b'N' => true,
        b'S' => false,
        _ => return Err("the latitude has no N or S".into()),
    };
    let lon_deg = digits(&body[17..20], "the longitude")?;
    let lon_min = digits(&body[20..25], "the longitude")?;
    let east = match bytes[25] {
        b'E' => true,
        b'W' => false,
        _ => return Err("the longitude has no E or W".into()),
    };
    let symbol = bytes[26];
    if symbol == b'_' {
        return Err("a weather report".into());
    }
    let rest = &body[27..];

    // `ccc/sss` and `/A=dddddd`, each if there.
    let mut course_deg = None;
    let mut ground_speed_ms = None;
    let mut after = rest;
    if rest.len() >= 7
        && rest.as_bytes()[3] == b'/'
        && let (Ok(course), Ok(speed)) = (
            digits(&rest[..3], "the course"),
            digits(&rest[4..7], "the speed"),
        )
    {
        // 0 means "not known" in APRS; north is 360.
        if course > 0.0 && course <= 360.0 {
            course_deg = Some(course % 360.0);
        }
        ground_speed_ms = Some(round2(speed * KNOT_MS));
        after = &rest[7..];
    }
    let mut altitude_m = None;
    if let Some(text) = after.strip_prefix("/A=") {
        let end = text
            .find(|c: char| !(c.is_ascii_digit() || c == '-'))
            .unwrap_or(text.len());
        if let Ok(feet) = text[..end].parse::<f64>() {
            altitude_m = Some((feet * FOOT_M).round());
        }
    }

    let mut lat_min = lat_min;
    let mut lon_min = lon_min;
    let mut climb_ms = None;
    for word in rest.split_whitespace() {
        if let Some(precision) = word.strip_prefix("!W").and_then(|w| w.strip_suffix('!')) {
            let p = precision.as_bytes();
            if p.len() == 2 && p.iter().all(u8::is_ascii_digit) {
                lat_min += f64::from(p[0] - b'0') / 1000.0;
                lon_min += f64::from(p[1] - b'0') / 1000.0;
            }
        } else if let Some(fpm) = word.strip_suffix("fpm")
            && let Ok(fpm) = fpm.parse::<f64>()
        {
            climb_ms = Some(round2(fpm * FOOT_M / 60.0));
        }
    }
    let lat = (lat_deg + lat_min / 60.0) * if north { 1.0 } else { -1.0 };
    let lon = (lon_deg + lon_min / 60.0) * if east { 1.0 } else { -1.0 };
    let position = GeoPoint::new(round6(lat), round6(lon)).map_err(|e| e.to_string())?;

    Ok(LivePosition {
        device: address.to_ascii_uppercase(),
        aircraft: aircraft(flags),
        time,
        position,
        altitude_m,
        course_deg,
        ground_speed_ms,
        climb_ms,
    })
}

/// The time of day `hhmmss` on the day of `received`, or on the day before
/// when that would lie more than an hour after it: a line received just
/// after midnight tells of a position taken just before.
fn time_of(hhmmss: &str, received: Timestamp) -> Result<Timestamp, String> {
    let of_day = NaiveTime::parse_from_str(hhmmss, "%H%M%S")
        .map_err(|_| format!("the time {hhmmss:?} is no time of day"))?;
    let on_the_day = received.date_naive().and_time(of_day).and_utc();
    Ok(if on_the_day - received > Duration::hours(1) {
        on_the_day - Duration::days(1)
    } else {
        on_the_day
    })
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round6(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

#[derive(Deserialize)]
struct DdbFile {
    devices: Vec<DdbDevice>,
}

#[derive(Deserialize)]
struct DdbDevice {
    device_id: String,
    #[serde(default)]
    aircraft_model: String,
    #[serde(default)]
    registration: String,
    #[serde(default)]
    cn: String,
    #[serde(default)]
    tracked: String,
    #[serde(default)]
    identified: String,
}

fn given(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The owners' choices from the device database.
///
/// Only `Y` is a yes. Where the database holds one address twice (for two
/// kinds of address, which it did not on 2026-11-21), the stricter choice
/// wins: the device is tracked and identified only if every entry says so.
pub fn parse_directory(body: &[u8]) -> Result<DeviceDirectory, SourceError> {
    let file: DdbFile = serde_json::from_slice(body)
        .map_err(|e| SourceError::Malformed(format!("the device database is not its JSON: {e}")))?;
    let mut devices: BTreeMap<String, DeviceChoice> = BTreeMap::new();
    for device in file.devices {
        let address = device.device_id.trim().to_ascii_uppercase();
        if address.len() != 6 || !address.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let tracked = device.tracked == "Y";
        let identified = tracked && device.identified == "Y";
        let choice = DeviceChoice {
            tracked,
            identified,
            registration: identified.then(|| given(&device.registration)).flatten(),
            competition_id: identified.then(|| given(&device.cn)).flatten(),
            model: identified.then(|| given(&device.aircraft_model)).flatten(),
        };
        devices
            .entry(address)
            .and_modify(|held| {
                held.tracked &= choice.tracked;
                held.identified &= choice.identified;
                if !held.identified {
                    held.registration = None;
                    held.competition_id = None;
                    held.model = None;
                }
            })
            .or_insert(choice);
    }
    Ok(DeviceDirectory { devices })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_sources::LicenceFamily;

    use super::*;

    fn at(h: u32, m: u32, s: u32) -> Timestamp {
        Utc.with_ymd_and_hms(2026, 11, 21, h, m, s).unwrap()
    }

    #[test]
    fn the_manifest_says_what_the_networks_page_says() {
        let ogn = Ogn::new();
        let manifest = ogn.manifest();
        assert_eq!(manifest.licence.family(), LicenceFamily::ShareAlike);
        assert_eq!(manifest.pass_on_hours, Some(24));
        assert_eq!(manifest.kinds, [venti_sources::Kind::Live]);
    }

    #[test]
    fn the_login_reads_only_and_asks_for_the_box() {
        let area = BoundingBox::new(46.3, 9.4, 48.5, 17.2).unwrap();
        let line = login(&area);
        assert!(line.starts_with("user VENTI-RO pass -1 vers Venti "));
        assert!(line.ends_with(" filter a/48.5000/9.4000/46.3000/17.2000\r\n"));
    }

    #[test]
    fn a_paragliders_line_is_read_in_metres_and_seconds() {
        let line = "FLRE0A5BA>OGFLR,qAS,Wallberg:/190509h4744.13N\\01227.47E^342/049/A=005524 !W52! id1EE0A5BA -454fpm +0.1rot 8.8dB gps4x5";
        let LiveLine::Position(position) = parse_line(line, at(19, 5, 10)) else {
            panic!("not a position");
        };
        assert_eq!(position.device, "E0A5BA");
        assert_eq!(position.aircraft, Aircraft::Paraglider);
        assert_eq!(position.time, at(19, 5, 9));
        // 47° 44.135', 12° 27.472'
        assert!((position.position.lat() - (47.0 + 44.135 / 60.0)).abs() < 1e-6);
        assert!((position.position.lon() - (12.0 + 27.472 / 60.0)).abs() < 1e-6);
        assert_eq!(position.altitude_m, Some(1684.0));
        assert_eq!(position.course_deg, Some(342.0));
        assert_eq!(position.ground_speed_ms, Some(25.21));
        assert_eq!(position.climb_ms, Some(-2.31));
    }

    #[test]
    fn a_line_that_must_not_be_shown_is_hidden() {
        let no_tracking =
            "FLRE0A5BA>OGFLR,qAS,X:/190509h4744.13N/01227.47E'342/049/A=005524 id5EE0A5BA +0fpm";
        assert_eq!(parse_line(no_tracking, at(19, 5, 10)), LiveLine::Hidden);
        let stealth =
            "FLRE0A5BA>OGFLR,qAS,X:/190509h4744.13N/01227.47E'342/049/A=005524 id9EE0A5BA +0fpm";
        assert_eq!(parse_line(stealth, at(19, 5, 10)), LiveLine::Hidden);
    }

    #[test]
    fn what_is_no_aircraft_is_something_else() {
        let now = at(19, 5, 10);
        assert_eq!(parse_line("# aprsc 2.1.0-madeup", now), LiveLine::Server);
        let receiver = "Wallberg>OGNSDR,TCPIP*,qAC,MADEUP1:/190457h4739.40NI01147.85E&/A=005650";
        assert_eq!(parse_line(receiver, now), LiveLine::Other(None));
        let status = "Wallberg>OGNSDR,TCPIP*,qAC,MADEUP1:>190511h v0.3.0 CPU:0.5";
        assert_eq!(parse_line(status, now), LiveLine::Other(None));
        let naviter = "NAVE0000D>OGNAVI,qAS,MADEUP3:/190525h4742.00N/01146.00E'000/000/A=002500 !W34! id0440E0000D +000fpm";
        assert!(matches!(parse_line(naviter, now), LiveLine::Other(Some(_))));
    }

    #[test]
    fn a_position_just_before_midnight_is_of_the_day_before() {
        let received = Utc.with_ymd_and_hms(2026, 11, 22, 0, 0, 3).unwrap();
        let time = time_of("235958", received).unwrap();
        assert_eq!(
            time,
            Utc.with_ymd_and_hms(2026, 11, 21, 23, 59, 58).unwrap()
        );
        // A clock a little behind the network's.
        assert_eq!(time_of("190012", at(19, 0, 5)).unwrap(), at(19, 0, 12));
    }

    #[test]
    fn the_directory_keeps_only_what_owners_allowed() {
        let body = br#"{"devices":[
            {"device_type":"F","device_id":"DD0001","aircraft_model":"Omega","registration":"D-1","cn":"AB","tracked":"Y","identified":"Y","aircraft_type":6},
            {"device_type":"F","device_id":"DD0002","aircraft_model":"","registration":"D-2","cn":"","tracked":"Y","identified":"N","aircraft_type":6},
            {"device_type":"F","device_id":"DD0003","aircraft_model":"","registration":"","cn":"","tracked":"N","identified":"Y","aircraft_type":6},
            {"device_type":"O","device_id":"DD0002","aircraft_model":"","registration":"","cn":"","tracked":"N","identified":"N","aircraft_type":6}
        ]}"#;
        let directory = parse_directory(body).unwrap();
        let one = &directory.devices["DD0001"];
        assert!(one.tracked && one.identified);
        assert_eq!(one.registration.as_deref(), Some("D-1"));
        assert_eq!(one.competition_id.as_deref(), Some("AB"));
        // Not identified: nothing that identifies it is kept. And the
        // second entry of the address says it is not tracked either.
        let two = &directory.devices["DD0002"];
        assert!(!two.tracked && !two.identified && two.registration.is_none());
        // Not tracked is not identified.
        assert!(!directory.devices["DD0003"].identified);
    }
}
