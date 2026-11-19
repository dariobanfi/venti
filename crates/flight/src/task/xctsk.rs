//! Tasks in the XCTSK format, the open format XCTrack defines and other
//! instruments read: <https://xctrack.org/Competition_Interfaces.html>
//! (read 2026-11-19).
//!
//! A task comes as one of four texts, and [`read`] takes them all:
//!
//! - a file: a JSON object, version 1, with everything spelled out;
//! - the compact form of a QR code: a JSON object, version 2, with short
//!   keys and each turnpoint's place packed into a few letters;
//! - that compact form after `XCTSK:`, as a QR code holds it;
//! - `XCTSKZ:` and then the compact form compressed with zlib and written
//!   in base64, which makes a QR code that scans better in sunlight.
//!
//! A plain route of waypoints is a fifth, shorter kind of the compact
//! form.
//!
//! What the published page leaves open was settled against XCTrack's own
//! task tool, whose output for a task of ours is kept beside the tests:
//! the compact form drops the mark of the take-off, a goal line is 1 and
//! a cylinder 2, and the compressed form is zlib in standard base64.

use std::io::Read as _;

use base64::Engine as _;
use serde_json::{Map, Value, json};
use venti_domain::GeoPoint;

use super::{
    Goal, GoalKind, Role, Start, StartKind, Task, TaskError, TaskKind, TimeOfDay, Turnpoint,
};
use crate::geodesy::Earth;

/// No task text is longer than this, bytes. A task of a hundred
/// turnpoints with long descriptions stays far below it.
const MAX_TEXT: usize = 256 * 1024;

/// The media type of a task file.
pub const MEDIA_TYPE: &str = "application/xctsk";

/// Reads a task from a file's text or a QR code's.
pub fn read(text: &str) -> Result<Task, TaskError> {
    if text.len() > MAX_TEXT {
        return Err(TaskError::TooLong);
    }
    let text = text.trim().trim_start_matches('\u{feff}');
    let unpacked;
    let json = if let Some(packed) = strip_prefix(text, "XCTSKZ:") {
        unpacked = unpack(packed)?;
        unpacked.as_str()
    } else {
        strip_prefix(text, "XCTSK:").unwrap_or(text)
    };
    let value: Value = serde_json::from_str(json.trim()).map_err(|_| TaskError::NotATask)?;
    let object = value.as_object().ok_or(TaskError::NotATask)?;

    // A plain route says so with one letter.
    if object.get("T").and_then(Value::as_str) == Some("W") {
        return read_route(object)?.checked();
    }
    let kind = object
        .get("taskType")
        .and_then(Value::as_str)
        .ok_or(TaskError::NotATask)?;
    if kind != "CLASSIC" {
        return Err(TaskError::Unknown(format!("task type {kind}")));
    }
    match object.get("version").and_then(Value::as_u64) {
        Some(1) => read_file(object)?.checked(),
        Some(2) => read_compact(object)?.checked(),
        Some(other) => Err(TaskError::Unknown(format!("version {other}"))),
        None => Err(TaskError::NotATask),
    }
}

/// The rest of a text after a prefix, whatever the prefix's case.
fn strip_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// Undoes the compression of the `XCTSKZ:` form.
fn unpack(packed: &str) -> Result<String, TaskError> {
    let packed: String = packed.chars().filter(|c| !c.is_whitespace()).collect();
    // XCTrack writes standard base64. Other writers may leave the
    // padding away or use the alphabet for addresses.
    let bytes = [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ]
    .into_iter()
    .find_map(|engine| engine.decode(&packed).ok())
    .ok_or(TaskError::Compressed)?;
    // Read one byte more than a task may have, to tell a task from
    // something made to unpack without end.
    let mut text = String::new();
    flate2::read::ZlibDecoder::new(bytes.as_slice())
        .take(MAX_TEXT as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|_| TaskError::Compressed)?;
    if text.len() > MAX_TEXT {
        return Err(TaskError::TooLong);
    }
    Ok(text)
}

fn turnpoint_problem(number: usize, problem: &str) -> TaskError {
    TaskError::Turnpoint {
        number,
        problem: problem.to_owned(),
    }
}

fn time(value: &Value) -> Result<TimeOfDay, TaskError> {
    let text = value.as_str().ok_or(TaskError::NotATask)?;
    TimeOfDay::parse(text).ok_or_else(|| TaskError::Time(text.to_owned()))
}

fn optional_time(object: &Map<String, Value>, key: &str) -> Result<Option<TimeOfDay>, TaskError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => time(value).map(Some),
    }
}

fn times(value: Option<&Value>) -> Result<Vec<TimeOfDay>, TaskError> {
    let mut gates = match value {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or(TaskError::NotATask)?
            .iter()
            .map(time)
            .collect::<Result<Vec<_>, _>>()?,
    };
    gates.dedup();
    Ok(gates)
}

fn position(number: usize, lat: f64, lon: f64) -> Result<GeoPoint, TaskError> {
    GeoPoint::new(lat, lon).map_err(|_| turnpoint_problem(number, "its place is not on the earth"))
}

/// Version 1: the file.
fn read_file(object: &Map<String, Value>) -> Result<Task, TaskError> {
    let earth = match object.get("earthModel").and_then(Value::as_str) {
        None | Some("WGS84") => Earth::Wgs84,
        Some("FAI_SPHERE") => Earth::FaiSphere,
        Some(other) => return Err(TaskError::Unknown(format!("earth model {other}"))),
    };
    let listed = object
        .get("turnpoints")
        .and_then(Value::as_array)
        .ok_or(TaskError::NotATask)?;
    let mut turnpoints = Vec::with_capacity(listed.len().min(super::MAX_TURNPOINTS));
    for (index, entry) in listed.iter().enumerate() {
        let number = index + 1;
        let waypoint = entry
            .get("waypoint")
            .and_then(Value::as_object)
            .ok_or_else(|| turnpoint_problem(number, "it has no waypoint"))?;
        let coordinate = |key: &str| {
            waypoint
                .get(key)
                .and_then(Value::as_f64)
                .ok_or_else(|| turnpoint_problem(number, "its place is missing"))
        };
        let role = match entry.get("type").and_then(Value::as_str) {
            Some("TAKEOFF") => Some(Role::Takeoff),
            Some("SSS") => Some(Role::Start),
            Some("ESS") => Some(Role::EndOfSpeed),
            _ => None,
        };
        turnpoints.push(Turnpoint {
            name: waypoint
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            description: waypoint
                .get("description")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned),
            position: position(number, coordinate("lat")?, coordinate("lon")?)?,
            altitude_m: waypoint
                .get("altSmoothed")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            radius_m: entry
                .get("radius")
                .and_then(Value::as_f64)
                .ok_or_else(|| turnpoint_problem(number, "it has no radius"))?,
            role,
        });
    }

    let (takeoff_open, takeoff_close) = match object.get("takeoff").and_then(Value::as_object) {
        Some(takeoff) => (
            optional_time(takeoff, "timeOpen")?,
            optional_time(takeoff, "timeClose")?,
        ),
        None => (None, None),
    };
    let start = match object.get("sss").and_then(Value::as_object) {
        Some(sss) => Some(Start {
            kind: match sss.get("type").and_then(Value::as_str) {
                Some("ELAPSED-TIME") => StartKind::ElapsedTime,
                _ => StartKind::Race,
            },
            gates: times(sss.get("timeGates"))?,
            close: optional_time(sss, "timeClose")?,
        }),
        None => None,
    };
    let goal = match object.get("goal").and_then(Value::as_object) {
        Some(goal) => Goal {
            kind: match goal.get("type").and_then(Value::as_str) {
                Some("LINE") => GoalKind::Line,
                _ => GoalKind::Cylinder,
            },
            deadline: optional_time(goal, "deadline")?,
            finish_altitude_m: goal.get("finishAltitude").and_then(Value::as_f64),
        },
        None => Goal::default(),
    };
    Ok(Task {
        kind: TaskKind::Competition,
        earth,
        turnpoints,
        takeoff_open,
        takeoff_close,
        start,
        goal,
    })
}

/// The turnpoints of the compact form. `numbers` is how many numbers a
/// turnpoint's letters hold: four with a radius, three without.
fn compact_turnpoints(
    object: &Map<String, Value>,
    numbers: usize,
) -> Result<Vec<Turnpoint>, TaskError> {
    let listed = object
        .get("t")
        .and_then(Value::as_array)
        .ok_or(TaskError::NotATask)?;
    let mut turnpoints = Vec::with_capacity(listed.len().min(super::MAX_TURNPOINTS));
    for (index, entry) in listed.iter().enumerate() {
        let number = index + 1;
        let packed = entry
            .get("z")
            .and_then(Value::as_str)
            .ok_or_else(|| turnpoint_problem(number, "its place is missing"))?;
        let unpacked = unpack_numbers(packed);
        if unpacked.len() < numbers {
            return Err(turnpoint_problem(number, "its place cannot be read"));
        }
        // Longitude first, then latitude, in hundred-thousandths of a
        // degree; then the height and the radius in metres.
        let (lon, lat) = (unpacked[0] as f64 / 1e5, unpacked[1] as f64 / 1e5);
        turnpoints.push(Turnpoint {
            name: entry
                .get("n")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            description: entry
                .get("d")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned),
            position: position(number, lat, lon)?,
            altitude_m: unpacked[2] as f64,
            radius_m: unpacked
                .get(3)
                .filter(|_| numbers > 3)
                .map_or(0.0, |radius| *radius as f64),
            role: match entry.get("t").and_then(Value::as_u64) {
                Some(2) => Some(Role::Start),
                Some(3) => Some(Role::EndOfSpeed),
                _ => None,
            },
        });
    }
    Ok(turnpoints)
}

/// Version 2: the compact form of a QR code.
fn read_compact(object: &Map<String, Value>) -> Result<Task, TaskError> {
    let start = match object.get("s").and_then(Value::as_object) {
        Some(start) => Some(Start {
            kind: match start.get("t").and_then(Value::as_u64) {
                Some(2) => StartKind::ElapsedTime,
                _ => StartKind::Race,
            },
            gates: times(start.get("g"))?,
            close: None,
        }),
        None => None,
    };
    let goal = match object.get("g").and_then(Value::as_object) {
        Some(goal) => Goal {
            kind: match goal.get("t").and_then(Value::as_u64) {
                Some(1) => GoalKind::Line,
                _ => GoalKind::Cylinder,
            },
            deadline: optional_time(goal, "d")?,
            finish_altitude_m: goal.get("fa").and_then(Value::as_f64),
        },
        None => Goal::default(),
    };
    Ok(Task {
        kind: TaskKind::Competition,
        earth: match object.get("e").and_then(Value::as_u64) {
            Some(1) => Earth::FaiSphere,
            _ => Earth::Wgs84,
        },
        turnpoints: compact_turnpoints(object, 4)?,
        takeoff_open: optional_time(object, "to")?,
        takeoff_close: optional_time(object, "tc")?,
        start,
        goal,
    })
}

/// A plain route of waypoints.
fn read_route(object: &Map<String, Value>) -> Result<Task, TaskError> {
    Ok(Task {
        kind: TaskKind::Route,
        earth: Earth::Wgs84,
        turnpoints: compact_turnpoints(object, 3)?,
        takeoff_open: None,
        takeoff_close: None,
        start: None,
        goal: Goal::default(),
    })
}

/// Packs one whole number the way Google's polyline format does: five
/// bits to a letter, the lowest first, the sign folded into the lowest
/// bit.
fn pack_number(text: &mut String, number: i64) {
    let mut left = if number < 0 {
        !(number << 1)
    } else {
        number << 1
    } as u64;
    while left > 0x1f {
        text.push(char::from(((left & 0x1f) as u8 | 0x20) + 63));
        left >>= 5;
    }
    text.push(char::from(left as u8 + 63));
}

/// Unpacks the numbers of a turnpoint. Letters that cannot be part of a
/// number end it.
fn unpack_numbers(text: &str) -> Vec<i64> {
    let mut numbers = Vec::new();
    let (mut current, mut shift) = (0_u64, 0_u32);
    for letter in text.bytes() {
        let Some(bits) = letter.checked_sub(63).filter(|bits| *bits < 64) else {
            break;
        };
        if shift < 60 {
            current |= u64::from(bits & 0x1f) << shift;
        }
        shift += 5;
        if bits <= 0x1f {
            let half = (current >> 1) as i64;
            numbers.push(if current & 1 == 1 { !half } else { half });
            (current, shift) = (0, 0);
        }
    }
    numbers
}

fn pack_turnpoint(turnpoint: &Turnpoint, with_radius: bool) -> String {
    let mut text = String::new();
    pack_number(&mut text, (turnpoint.position.lon() * 1e5).round() as i64);
    pack_number(&mut text, (turnpoint.position.lat() * 1e5).round() as i64);
    pack_number(&mut text, turnpoint.altitude_m.round() as i64);
    if with_radius {
        pack_number(&mut text, turnpoint.radius_m.round() as i64);
    }
    text
}

/// Whether pilots leave the start cylinder to start or fly into it. The
/// format no longer says and readers ignore it, but asks writers to keep
/// writing something for older instruments: leaving, if the launch lies
/// inside the start cylinder.
fn start_is_exit(task: &Task) -> bool {
    task.start_index()
        .zip(task.turnpoints.first())
        .is_some_and(|(start, launch)| {
            let start = &task.turnpoints[start];
            task.earth.distance_m(start.position, launch.position) < start.radius_m
        })
}

/// Writes a task as a file: version 1, with everything spelled out. A
/// plain route has no such form, since its waypoints have no cylinders;
/// it is written in its own short form, which [`read`] takes as well.
/// Whatever is written reads back as the same task.
pub fn write_file(task: &Task) -> String {
    if task.kind == TaskKind::Route {
        return compact(task);
    }
    let turnpoints: Vec<Value> = task
        .turnpoints
        .iter()
        .map(|turnpoint| {
            let mut waypoint = Map::new();
            waypoint.insert("name".to_owned(), json!(turnpoint.name));
            if let Some(description) = &turnpoint.description {
                waypoint.insert("description".to_owned(), json!(description));
            }
            waypoint.insert("lat".to_owned(), json!(turnpoint.position.lat()));
            waypoint.insert("lon".to_owned(), json!(turnpoint.position.lon()));
            waypoint.insert(
                "altSmoothed".to_owned(),
                json!(turnpoint.altitude_m.round() as i64),
            );
            let mut entry = Map::new();
            match turnpoint.role {
                Some(Role::Takeoff) => entry.insert("type".to_owned(), json!("TAKEOFF")),
                Some(Role::Start) => entry.insert("type".to_owned(), json!("SSS")),
                Some(Role::EndOfSpeed) => entry.insert("type".to_owned(), json!("ESS")),
                None => None,
            };
            entry.insert(
                "radius".to_owned(),
                json!(turnpoint.radius_m.round() as i64),
            );
            entry.insert("waypoint".to_owned(), Value::Object(waypoint));
            Value::Object(entry)
        })
        .collect();

    let mut file = Map::new();
    file.insert("taskType".to_owned(), json!("CLASSIC"));
    file.insert("version".to_owned(), json!(1));
    file.insert(
        "earthModel".to_owned(),
        json!(match task.earth {
            Earth::Wgs84 => "WGS84",
            Earth::FaiSphere => "FAI_SPHERE",
        }),
    );
    file.insert("turnpoints".to_owned(), Value::Array(turnpoints));
    if task.takeoff_open.is_some() || task.takeoff_close.is_some() {
        let mut takeoff = Map::new();
        if let Some(open) = task.takeoff_open {
            takeoff.insert("timeOpen".to_owned(), json!(open.to_string()));
        }
        if let Some(close) = task.takeoff_close {
            takeoff.insert("timeClose".to_owned(), json!(close.to_string()));
        }
        file.insert("takeoff".to_owned(), Value::Object(takeoff));
    }
    if let Some(start) = &task.start {
        let mut sss = Map::new();
        sss.insert(
            "type".to_owned(),
            json!(match start.kind {
                StartKind::Race => "RACE",
                StartKind::ElapsedTime => "ELAPSED-TIME",
            }),
        );
        sss.insert(
            "direction".to_owned(),
            json!(if start_is_exit(task) { "EXIT" } else { "ENTER" }),
        );
        sss.insert(
            "timeGates".to_owned(),
            json!(
                start
                    .gates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            ),
        );
        if let Some(close) = start.close {
            sss.insert("timeClose".to_owned(), json!(close.to_string()));
        }
        file.insert("sss".to_owned(), Value::Object(sss));
    }
    let mut goal = Map::new();
    goal.insert(
        "type".to_owned(),
        json!(match task.goal.kind {
            GoalKind::Cylinder => "CYLINDER",
            GoalKind::Line => "LINE",
        }),
    );
    if let Some(deadline) = task.goal.deadline {
        goal.insert("deadline".to_owned(), json!(deadline.to_string()));
    }
    if let Some(altitude) = task.goal.finish_altitude_m {
        goal.insert("finishAltitude".to_owned(), json!(altitude.round() as i64));
    }
    file.insert("goal".to_owned(), Value::Object(goal));
    Value::Object(file).to_string()
}

/// The compact form, version 2, as a JSON text.
fn compact(task: &Task) -> String {
    let is_route = task.kind == TaskKind::Route;
    let turnpoints: Vec<Value> = task
        .turnpoints
        .iter()
        .map(|turnpoint| {
            let mut entry = Map::new();
            entry.insert("z".to_owned(), json!(pack_turnpoint(turnpoint, !is_route)));
            entry.insert("n".to_owned(), json!(turnpoint.name));
            if !is_route {
                if let Some(description) = &turnpoint.description {
                    entry.insert("d".to_owned(), json!(description));
                }
                // The compact form has no mark for the take-off.
                match turnpoint.role {
                    Some(Role::Start) => entry.insert("t".to_owned(), json!(2)),
                    Some(Role::EndOfSpeed) => entry.insert("t".to_owned(), json!(3)),
                    Some(Role::Takeoff) | None => None,
                };
            }
            Value::Object(entry)
        })
        .collect();

    let mut code = Map::new();
    if is_route {
        code.insert("T".to_owned(), json!("W"));
        code.insert("V".to_owned(), json!(2));
        code.insert("t".to_owned(), Value::Array(turnpoints));
        return Value::Object(code).to_string();
    }
    code.insert("taskType".to_owned(), json!("CLASSIC"));
    code.insert("version".to_owned(), json!(2));
    code.insert("t".to_owned(), Value::Array(turnpoints));
    if let Some(start) = &task.start {
        code.insert(
            "s".to_owned(),
            json!({
                "g": start.gates.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "d": if start_is_exit(task) { 2 } else { 1 },
                "t": match start.kind {
                    StartKind::Race => 1,
                    StartKind::ElapsedTime => 2,
                },
            }),
        );
    }
    let mut goal = Map::new();
    if let Some(deadline) = task.goal.deadline {
        goal.insert("d".to_owned(), json!(deadline.to_string()));
    }
    goal.insert(
        "t".to_owned(),
        json!(match task.goal.kind {
            GoalKind::Line => 1,
            GoalKind::Cylinder => 2,
        }),
    );
    if let Some(altitude) = task.goal.finish_altitude_m {
        goal.insert("fa".to_owned(), json!(altitude.round() as i64));
    }
    code.insert("g".to_owned(), Value::Object(goal));
    if task.earth == Earth::FaiSphere {
        code.insert("e".to_owned(), json!(1));
    }
    if let Some(open) = task.takeoff_open {
        code.insert("to".to_owned(), json!(open.to_string()));
    }
    if let Some(close) = task.takeoff_close {
        code.insert("tc".to_owned(), json!(close.to_string()));
    }
    Value::Object(code).to_string()
}

/// Writes a task as the text of a QR code. Compressed, it makes a code
/// with fewer dots, which the format's authors prefer; uncompressed, it
/// can be read by eye and by the oldest readers.
pub fn write_code(task: &Task, compressed: bool) -> String {
    let compact = compact(task);
    if !compressed {
        return format!("XCTSK:{compact}");
    }
    let mut encoder =
        flate2::read::ZlibEncoder::new(compact.as_bytes(), flate2::Compression::best());
    let mut bytes = Vec::new();
    // Reading from memory into memory cannot fail.
    if encoder.read_to_end(&mut bytes).is_err() {
        return format!("XCTSK:{compact}");
    }
    format!(
        "XCTSKZ:{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::testing::race;

    /// A made-up task written by hand to the published format, and what
    /// XCTrack's own task tool made of it. See the README beside them.
    const FILE: &str = include_str!("../../tests/fixtures/xctsk/task.xctsk");
    const COMPACT: &str = include_str!("../../tests/fixtures/xctsk/task-v2.json");
    const CODE: &str = include_str!("../../tests/fixtures/xctsk/task-qr.txt");
    const CODE_COMPRESSED: &str = include_str!("../../tests/fixtures/xctsk/task-qr-compressed.txt");

    #[test]
    fn a_task_file_is_read_with_all_its_parts() {
        let task = read(FILE).unwrap();
        assert_eq!(task.kind, TaskKind::Competition);
        assert_eq!(task.earth, Earth::Wgs84);
        let names: Vec<&str> = task.turnpoints.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["T01", "B02", "B03", "A04", "A04"]);
        let roles: Vec<Option<Role>> = task.turnpoints.iter().map(|t| t.role).collect();
        assert_eq!(
            roles,
            [
                Some(Role::Takeoff),
                Some(Role::Start),
                None,
                Some(Role::EndOfSpeed),
                None
            ]
        );
        let first = &task.turnpoints[0];
        assert_eq!((first.position.lat(), first.position.lon()), (47.0, 11.0));
        assert_eq!((first.altitude_m, first.radius_m), (1500.0, 400.0));
        assert_eq!(first.description.as_deref(), Some("Launch"));
        assert_eq!(task.turnpoints[1].description, None);

        assert_eq!(task.takeoff_open, TimeOfDay::new(9, 0, 0));
        assert_eq!(task.takeoff_close, TimeOfDay::new(11, 30, 0));
        let start = task.start.as_ref().unwrap();
        assert_eq!(start.kind, StartKind::Race);
        assert_eq!(
            start.gates,
            [
                TimeOfDay::new(10, 30, 0).unwrap(),
                TimeOfDay::new(10, 50, 0).unwrap()
            ]
        );
        assert_eq!(task.goal.kind, GoalKind::Line);
        assert_eq!(task.goal.deadline, TimeOfDay::new(16, 0, 0));
        assert_eq!(task.goal.finish_altitude_m, None);
    }

    #[test]
    fn what_xctrack_makes_of_the_file_reads_as_the_same_task() {
        let from_file = read(FILE).unwrap();
        // The compact form has no mark for the take-off; everything else
        // is the same task.
        let mut expected = from_file.clone();
        expected.turnpoints[0].role = None;
        for (name, text) in [
            ("compact", COMPACT),
            ("code", CODE),
            ("compressed code", CODE_COMPRESSED),
        ] {
            assert_eq!(read(text).unwrap(), expected, "{name}");
        }
    }

    #[test]
    fn a_turnpoint_is_packed_into_the_letters_xctrack_packs_it_into() {
        let task = read(FILE).unwrap();
        let packed: Vec<String> = task
            .turnpoints
            .iter()
            .map(|turnpoint| pack_turnpoint(turnpoint, true))
            .collect();
        assert_eq!(
            packed,
            [
                "_mcbA_uz}Gw|A_X",
                "_~vbAomd~Ggw@ozD",
                "_`~cA_wa_H_jAo}@",
                "_beeA_fn~God@_|B",
                "_beeA_fn~God@_X"
            ]
        );
        assert_eq!(
            unpack_numbers("_mcbA_uz}Gw|A_X"),
            [1_100_000, 4_700_000, 1500, 400]
        );
        // Negative numbers: the southern and western half of the world.
        let mut text = String::new();
        for number in [-1, 0, 1, -17_998_321, i64::from(i32::MAX)] {
            pack_number(&mut text, number);
        }
        assert_eq!(
            unpack_numbers(&text),
            [-1, 0, 1, -17_998_321, i64::from(i32::MAX)]
        );
        // Letters outside the alphabet end the numbers; nothing panics.
        assert_eq!(unpack_numbers("_X \u{e9}"), [400]);
        assert!(unpack_numbers("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~").is_empty());
    }

    #[test]
    fn the_code_we_write_is_the_code_xctrack_writes() {
        let task = read(FILE).unwrap();
        // The same JSON, whatever the order of its keys. The one thing
        // that differs is which way the start is crossed, which readers
        // are told to ignore: the tool passes on what the file said, and
        // the file said "out" of a cylinder its launch lies outside of.
        let mut ours: Value =
            serde_json::from_str(write_code(&task, false).strip_prefix("XCTSK:").unwrap()).unwrap();
        let theirs: Value = serde_json::from_str(COMPACT).unwrap();
        assert_eq!(ours["s"]["d"], 1);
        ours["s"]["d"] = theirs["s"]["d"].clone();
        assert_eq!(ours, theirs);

        let compressed = write_code(&task, true);
        assert!(compressed.starts_with("XCTSKZ:"));
        assert!(compressed.len() < write_code(&task, false).len());
        let mut expected = task.clone();
        expected.turnpoints[0].role = None;
        assert_eq!(read(&compressed).unwrap(), expected);
    }

    #[test]
    fn a_task_survives_being_written_as_a_file_and_read_again() {
        for mut task in [read(FILE).unwrap(), race()] {
            assert_eq!(read(&write_file(&task)).unwrap(), task);
            task.earth = Earth::FaiSphere;
            task.goal.finish_altitude_m = Some(300.0);
            task.start.as_mut().unwrap().kind = StartKind::ElapsedTime;
            assert_eq!(read(&write_file(&task)).unwrap(), task);
            let mut without_takeoff = task.clone();
            without_takeoff.turnpoints[0].role = None;
            assert_eq!(read(&write_code(&task, true)).unwrap(), without_takeoff);
        }
        // The file says which way the start is crossed, for instruments
        // that still want to be told: out of a cylinder around the launch.
        let file: Value = serde_json::from_str(&write_file(&race())).unwrap();
        assert_eq!(file["sss"]["direction"], "EXIT");
        let file: Value = serde_json::from_str(&write_file(&read(FILE).unwrap())).unwrap();
        assert_eq!(file["sss"]["direction"], "ENTER");
    }

    #[test]
    fn a_plain_route_is_read_and_written_in_its_own_short_form() {
        let mut places = String::new();
        let mut entries = Vec::new();
        for (name, lat, lon, altitude) in [
            ("A", 47.0, 11.0, 1500),
            ("B", 47.1, 11.2, 2100),
            ("C", 47.0, 11.4, 700),
        ] {
            places.clear();
            pack_number(&mut places, (lon * 1e5_f64).round() as i64);
            pack_number(&mut places, (lat * 1e5_f64).round() as i64);
            pack_number(&mut places, altitude);
            entries.push(json!({"z": places.clone(), "n": name}));
        }
        let code = format!("XCTSK:{}", json!({"T": "W", "V": 2, "t": entries}));
        let route = read(&code).unwrap();
        assert_eq!(route.kind, TaskKind::Route);
        assert_eq!(route.turnpoints.len(), 3);
        assert_eq!(route.turnpoints[1].name, "B");
        assert_eq!(route.turnpoints[1].radius_m, 0.0);
        assert_eq!(route.turnpoints[1].altitude_m, 2100.0);
        assert!(route.start.is_none());

        let written: Value =
            serde_json::from_str(write_code(&route, false).strip_prefix("XCTSK:").unwrap())
                .unwrap();
        assert_eq!(written, json!({"T": "W", "V": 2, "t": entries}));
        assert_eq!(read(&write_code(&route, true)).unwrap(), route);
        // A route kept as a file is the same route when it is read again.
        assert_eq!(read(&write_file(&route)).unwrap(), route);
    }

    #[test]
    fn what_is_not_a_task_is_refused_with_the_reason() {
        for text in [
            "",
            "hello",
            "[]",
            "{}",
            "XCTSK:",
            "{\"taskType\": \"CLASSIC\"}",
        ] {
            assert_eq!(read(text), Err(TaskError::NotATask), "{text}");
        }
        assert!(matches!(
            read(r#"{"taskType": "CLASSIC", "version": 3, "turnpoints": []}"#),
            Err(TaskError::Unknown(_))
        ));
        assert!(matches!(
            read(r#"{"taskType": "AAT", "version": 1}"#),
            Err(TaskError::Unknown(_))
        ));
        assert_eq!(
            read("XCTSKZ:not base64 at all!"),
            Err(TaskError::Compressed)
        );
        assert_eq!(read("XCTSKZ:aGVsbG8="), Err(TaskError::Compressed));
        assert_eq!(
            read(
                r#"{"taskType": "CLASSIC", "version": 1, "turnpoints": [
                {"radius": 400, "waypoint": {"name": "A", "lat": 47.0, "lon": 11.0, "altSmoothed": 0}}]}"#
            ),
            Err(TaskError::TooFewTurnpoints(1))
        );
        let bad_place = read(
            r#"{"taskType": "CLASSIC", "version": 1, "turnpoints": [
                {"radius": 400, "waypoint": {"name": "A", "lat": 47.0, "lon": 11.0, "altSmoothed": 0}},
                {"radius": 400, "waypoint": {"name": "B", "lat": 97.0, "lon": 11.0, "altSmoothed": 0}}]}"#,
        )
        .unwrap_err();
        assert_eq!(
            bad_place.to_string(),
            "turnpoint 2: its place is not on the earth"
        );
        let bad_time = read(&FILE.replace("16:00:00Z", "late")).unwrap_err();
        assert_eq!(bad_time, TaskError::Time("late".to_owned()));
        assert_eq!(read(&"x".repeat(MAX_TEXT + 1)), Err(TaskError::TooLong));
    }

    #[test]
    fn a_compressed_text_made_to_unpack_without_end_is_refused() {
        let zeros = vec![b' '; 4 * 1024 * 1024];
        let mut encoder =
            flate2::read::ZlibEncoder::new(zeros.as_slice(), flate2::Compression::best());
        let mut bytes = Vec::new();
        encoder.read_to_end(&mut bytes).unwrap();
        let code = format!(
            "XCTSKZ:{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        assert!(code.len() < MAX_TEXT);
        assert_eq!(read(&code), Err(TaskError::TooLong));
    }
}
