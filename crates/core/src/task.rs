//! The pilot's task: the competition task of the day, or a route planned
//! for a cross-country flight.
//!
//! One task is kept on the device at a time, as a file in the XCTSK format
//! that XCTrack and other instruments read, so that it can be handed on as
//! it came in. A task arrives as the text of a file or of a QR code; what
//! reads it, measures it and follows a flight through it is the flight
//! engine (`venti_flight::task`). This module keeps the task, shows it to
//! screens, and carries it through a flight in the cockpit.

use std::collections::VecDeque;
use std::io;

use venti_domain::GeoPoint;
use venti_flight::geodesy::Earth;
use venti_flight::live::navigation::Target;
use venti_flight::task::progress::{Fix, Phase, TaskEvent, TaskFlight, TaskStatus};
use venti_flight::task::route::Zone;
use venti_flight::task::{GoalKind, Role, StartKind, Task, TaskError, TaskKind, xctsk};

use crate::{Coordinate, Core, CoreError};

/// The name the task is stored under.
const FILE: &str = "task.xctsk";

/// What a turnpoint is in its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
pub enum TaskRole {
    /// The launch. Distances are measured from it; it is not a place to
    /// reach.
    Launch,
    /// The start of the speed section.
    Start,
    /// A turnpoint to round.
    Turnpoint,
    /// The end of the speed section, where it is not the goal.
    EndOfSpeed,
    /// The goal.
    Goal,
}

/// One turnpoint of a task as a screen shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskTurnpointView {
    /// Its place in the task, counted from 1.
    pub number: u32,
    /// Its name.
    pub name: String,
    /// What the competition's waypoint file says about it. May be empty.
    pub description: String,
    /// Its centre.
    pub position: Coordinate,
    /// The height of the ground there, metres above sea level.
    pub altitude_m: f64,
    /// The radius of its cylinder, metres; for a goal line half the
    /// line's length. Zero for a waypoint of a plain route.
    pub radius_m: f64,
    /// What it is in the task.
    pub role: TaskRole,
    /// The same in words: "Launch", "Start", "Goal line".
    pub role_in_words: String,
    /// Where the shortest way through the task touches it.
    pub touch: Coordinate,
    /// The length of the shortest way from the turnpoint before to this
    /// one, metres. Zero for the launch.
    pub leg_m: f64,
    /// How far along the shortest way it lies, metres from the launch.
    pub along_m: f64,
    /// Its outline for a map, as a closed line: the edge of its cylinder,
    /// or the goal line with the half circle behind it. Empty for a
    /// waypoint of a plain route.
    pub outline: Vec<Coordinate>,
}

/// How a task's clock starts.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskStartView {
    /// Whether it is a race, where everyone's clock starts with a gate.
    /// Otherwise each pilot's clock starts when the pilot crosses.
    pub is_race: bool,
    /// The same in words: "Race to goal", "Elapsed time".
    pub in_words: String,
    /// When the gates open, the earliest first, seconds since 1970-01-01
    /// UTC. Empty for a start that is always open.
    pub gates_unix_s: Vec<i64>,
    /// When the start closes.
    pub close_unix_s: Option<i64>,
}

/// A task as a screen shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskView {
    /// Whether it is a plain route of waypoints, without cylinders, start
    /// or goal.
    pub is_route: bool,
    /// The launch first, the goal last.
    pub turnpoints: Vec<TaskTurnpointView>,
    /// The length of the task along its shortest way, metres.
    pub length_m: f64,
    /// The length of the speed section, metres, if the task has one.
    pub speed_section_m: Option<f64>,
    /// The shortest way from the launch, for a map.
    pub way: Vec<Coordinate>,
    /// How the clock starts, if the task says.
    pub start: Option<TaskStartView>,
    /// When the launch opens, seconds since 1970-01-01 UTC.
    pub takeoff_open_unix_s: Option<i64>,
    /// When the launch closes.
    pub takeoff_close_unix_s: Option<i64>,
    /// When the task ends.
    pub deadline_unix_s: Option<i64>,
    /// A height above the goal's ground to arrive over, metres.
    pub finish_altitude_m: Option<f64>,
    /// What its distances are measured on: "WGS84 ellipsoid" or "FAI
    /// sphere".
    pub measured_on: String,
    /// The box that holds every cylinder, for showing the whole task on a
    /// map: south, west, north and east, degrees.
    pub south: f64,
    /// See `south`.
    pub west: f64,
    /// See `south`.
    pub north: f64,
    /// See `south`.
    pub east: f64,
}

/// A task as a file for other programs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskFile {
    /// A name for the file.
    pub name: String,
    /// Its text, in the XCTSK format.
    pub text: String,
}

/// How far a flight has come in its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
pub enum TaskPhase {
    /// The task has a start, and it has not been taken.
    BeforeStart,
    /// On the way round the turnpoints.
    OnTask,
    /// The speed section is flown and the goal still ahead.
    SpeedSectionDone,
    /// In goal.
    InGoal,
}

/// The turnpoint to fly to.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskNextView {
    /// Its place in the task, counted from 1.
    pub number: u32,
    /// Its name.
    pub name: String,
    /// What it is in the task, in words.
    pub role_in_words: String,
    /// The point to fly to: where the shortest way through the rest of
    /// the task touches it.
    pub aim: Coordinate,
    /// How far that point is, metres.
    pub distance_m: f64,
    /// The direction to it, degrees clockwise from north.
    pub bearing_deg: f64,
    /// How far the turnpoint's centre is, metres.
    pub centre_distance_m: f64,
    /// The radius of its cylinder, metres.
    pub radius_m: f64,
    /// Whether the glider is inside the cylinder. Then its edge is
    /// reached by flying out of it.
    pub inside: bool,
}

/// The start gate, while the start can still be taken.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskGateView {
    /// The gate that matters now: the first while none is open, then the
    /// last that opened. Absent for a start that is always open.
    pub gate_unix_s: Option<i64>,
    /// Seconds until it opens; negative once it is open.
    pub opens_in_s: Option<i64>,
    /// The next gate after it, in a task with several.
    pub next_gate_unix_s: Option<i64>,
    /// How far the edge of the start cylinder is along the shortest way,
    /// metres.
    pub distance_m: f64,
    /// The speed over the ground that brings the glider to the edge just
    /// as the gate opens, metres per second. Absent once it is open.
    pub speed_to_gate_ms: Option<f64>,
    /// How long the glider takes to the edge at the speed it flies now,
    /// seconds. Absent while it hardly moves.
    pub arrives_in_s: Option<f64>,
}

/// The task as the cockpit shows it during a flight.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskStatusView {
    /// How far the flight has come.
    pub phase: TaskPhase,
    /// The turnpoint to fly to. Absent in goal.
    pub next: Option<TaskNextView>,
    /// The shortest way still to fly, from the glider through the edge of
    /// every remaining turnpoint, for the map.
    pub way: Vec<Coordinate>,
    /// Its length to the goal, metres.
    pub to_goal_m: f64,
    /// Its length to the end of the speed section, metres, while that is
    /// still ahead.
    pub to_end_of_speed_m: Option<f64>,
    /// How much of the speed section is flown, from 0 to 1.
    pub speed_section_part: Option<f64>,
    /// The start gate, until the turnpoint after the start is reached.
    pub gate: Option<TaskGateView>,
    /// When the pilot's clock started, seconds since 1970-01-01 UTC.
    pub clock_from_unix_s: Option<i64>,
    /// How long the clock has run, seconds: until now, or until the end
    /// of the speed section once that is reached.
    pub clock_s: Option<i64>,
    /// When the goal was reached.
    pub goal_unix_s: Option<i64>,
    /// Seconds until the task ends; negative after its deadline.
    pub deadline_in_s: Option<i64>,
    /// How far the glider is above the height to arrive at in goal,
    /// metres: the goal's ground, plus the finish altitude if the task
    /// sets one. Negative when below it.
    pub above_goal_m: Option<f64>,
    /// The glide ratio over the ground needed along the rest of the task
    /// to arrive at that height. Absent when the glider is not above it.
    pub glide_to_goal: Option<f64>,
    /// Which turnpoints are reached, in the task's order.
    pub reached: Vec<bool>,
}

/// What kind of thing happened in the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
pub enum TaskEventKind {
    /// The start was taken, or taken again.
    Started,
    /// A turnpoint was reached.
    Reached,
    /// The speed section is flown.
    SpeedSectionDone,
    /// The goal is reached.
    Goal,
}

/// Something that happened in the task, for the cockpit to say.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, tsify::Tsify))]
#[cfg_attr(feature = "wasm", serde(rename_all = "camelCase"))]
pub struct TaskEventView {
    /// What happened.
    pub kind: TaskEventKind,
    /// The same in words a voice can say: "Start taken", "WEST reached",
    /// "Speed section done", "Goal".
    pub words: String,
}

fn coordinate(point: GeoPoint) -> Coordinate {
    Coordinate {
        lat: point.lat(),
        lon: point.lon(),
    }
}

fn invalid(error: &TaskError) -> CoreError {
    let mut message = error.to_string();
    if let Some(first) = message.get(..1) {
        message = first.to_uppercase() + &message[1..];
    }
    message.push('.');
    CoreError::InvalidFile { message }
}

fn storage_error(error: io::Error) -> CoreError {
    CoreError::Storage {
        message: format!("The task could not be read or written: {error}."),
    }
}

fn role(task: &Task, index: usize) -> TaskRole {
    let turnpoint = &task.turnpoints[index];
    if index == task.goal_index() {
        TaskRole::Goal
    } else if turnpoint.role == Some(Role::Start) {
        TaskRole::Start
    } else if turnpoint.role == Some(Role::EndOfSpeed) {
        TaskRole::EndOfSpeed
    } else if index == 0 {
        TaskRole::Launch
    } else {
        TaskRole::Turnpoint
    }
}

fn role_in_words(task: &Task, index: usize) -> String {
    match role(task, index) {
        TaskRole::Launch => "Launch",
        TaskRole::Start => "Start",
        TaskRole::Turnpoint if task.kind == TaskKind::Route => "Waypoint",
        TaskRole::Turnpoint => "Turnpoint",
        TaskRole::EndOfSpeed => "End of speed section",
        TaskRole::Goal if task.kind == TaskKind::Route => "Last waypoint",
        TaskRole::Goal if task.goal.kind == GoalKind::Line => "Goal line",
        TaskRole::Goal => "Goal",
    }
    .to_owned()
}

/// The outline of an area as a closed line.
fn outline(earth: Earth, zone: &Zone) -> Vec<Coordinate> {
    match *zone {
        Zone::Point { .. } => Vec::new(),
        Zone::Cylinder { centre, radius_m } => (0..=72)
            .map(|step| coordinate(earth.destination(centre, f64::from(step) * 5.0, radius_m)))
            .collect(),
        Zone::Line {
            centre,
            half_length_m,
            inbound_deg,
        } => {
            // From one end of the line round behind it to the other end,
            // and back along the line.
            let mut ring: Vec<Coordinate> = (0..=18)
                .map(|step| {
                    let bearing = inbound_deg - 90.0 + f64::from(step) * 10.0;
                    coordinate(earth.destination(centre, bearing, half_length_m))
                })
                .collect();
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            ring
        }
    }
}

fn view(task: &Task, now_unix_s: i64) -> TaskView {
    let measured = task.measure();
    let mut turnpoints = Vec::with_capacity(task.turnpoints.len());
    let (mut south, mut west, mut north, mut east) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut stretch = |point: Coordinate| {
        south = south.min(point.lat);
        north = north.max(point.lat);
        west = west.min(point.lon);
        east = east.max(point.lon);
    };
    for (index, turnpoint) in task.turnpoints.iter().enumerate() {
        let ring = task
            .zone(index)
            .map(|zone| outline(task.earth, &zone))
            .unwrap_or_default();
        stretch(coordinate(turnpoint.position));
        // The launch's own cylinder is not part of the task; the others
        // have to fit on the map.
        if index > 0 {
            ring.iter().copied().for_each(&mut stretch);
        }
        let along_m = measured.along_m.get(index).copied().unwrap_or(0.0);
        let before = index
            .checked_sub(1)
            .and_then(|before| measured.along_m.get(before))
            .copied()
            .unwrap_or(0.0);
        turnpoints.push(TaskTurnpointView {
            number: index as u32 + 1,
            name: turnpoint.name.clone(),
            description: turnpoint.description.clone().unwrap_or_default(),
            position: coordinate(turnpoint.position),
            altitude_m: turnpoint.altitude_m,
            radius_m: turnpoint.radius_m,
            role: role(task, index),
            role_in_words: role_in_words(task, index),
            touch: coordinate(
                measured
                    .points
                    .get(index)
                    .copied()
                    .unwrap_or(turnpoint.position),
            ),
            leg_m: along_m - before,
            along_m,
            outline: if index == 0 && task.first_to_reach() == 1 {
                Vec::new()
            } else {
                ring
            },
        });
    }
    TaskView {
        is_route: task.kind == TaskKind::Route,
        turnpoints,
        length_m: measured.length_m,
        speed_section_m: measured.speed_section_m,
        way: measured.points.iter().copied().map(coordinate).collect(),
        start: task.start.as_ref().map(|start| TaskStartView {
            is_race: start.kind == StartKind::Race,
            in_words: match start.kind {
                StartKind::Race => "Race to goal",
                StartKind::ElapsedTime => "Elapsed time",
            }
            .to_owned(),
            gates_unix_s: start
                .gates
                .iter()
                .map(|gate| gate.near(now_unix_s))
                .collect(),
            close_unix_s: start.close.map(|close| close.near(now_unix_s)),
        }),
        takeoff_open_unix_s: task.takeoff_open.map(|time| time.near(now_unix_s)),
        takeoff_close_unix_s: task.takeoff_close.map(|time| time.near(now_unix_s)),
        deadline_unix_s: task.goal.deadline.map(|time| time.near(now_unix_s)),
        finish_altitude_m: task.goal.finish_altitude_m,
        measured_on: match task.earth {
            Earth::Wgs84 => "WGS84 ellipsoid",
            Earth::FaiSphere => "FAI sphere",
        }
        .to_owned(),
        south,
        west,
        north,
        east,
    }
}

/// Reads a task from the text of a file or of a QR code, without keeping
/// it: to show the pilot what came in before it replaces the task of the
/// day. `now_unix_s` says which day the task's times of day mean.
#[uniffi::export]
pub fn read_task(text: String, now_unix_s: i64) -> Result<TaskView, CoreError> {
    let task = xctsk::read(&text).map_err(|error| invalid(&error))?;
    Ok(view(&task, now_unix_s))
}

#[uniffi::export]
impl Core {
    /// Keeps a task, from the text of a file or of a QR code. It replaces
    /// the task kept before.
    pub fn keep_task(&self, text: String) -> Result<(), CoreError> {
        let task = xctsk::read(&text).map_err(|error| invalid(&error))?;
        // Kept as a file whatever form it came in: the file is the one
        // form that says everything, the mark of the take-off included.
        self.storage
            .write(FILE, xctsk::write_file(&task).as_bytes())
            .map_err(storage_error)
    }

    /// The task that is kept, if there is one.
    pub fn kept_task(&self, now_unix_s: i64) -> Result<Option<TaskView>, CoreError> {
        Ok(self.stored_task()?.map(|task| view(&task, now_unix_s)))
    }

    /// The kept task as a file: for the cockpit to fly, and for other
    /// programs.
    pub fn kept_task_file(&self) -> Result<Option<TaskFile>, CoreError> {
        Ok(self.stored_task()?.map(|task| TaskFile {
            name: "venti-task.xctsk".to_owned(),
            text: xctsk::write_file(&task),
        }))
    }

    /// The kept task as the text of a QR code, for another pilot's
    /// instrument to scan. Compressed, the code has fewer dots and scans
    /// better in the sun; some older readers want it uncompressed.
    pub fn kept_task_code(&self, compressed: bool) -> Result<Option<String>, CoreError> {
        Ok(self
            .stored_task()?
            .map(|task| xctsk::write_code(&task, compressed)))
    }

    /// Forgets the kept task. Forgetting when there is none is not an
    /// error.
    pub fn forget_task(&self) -> Result<(), CoreError> {
        self.storage.remove(FILE).map_err(storage_error)
    }
}

impl Core {
    fn stored_task(&self) -> Result<Option<Task>, CoreError> {
        let Some(bytes) = self.storage.read(FILE).map_err(storage_error)? else {
            return Ok(None);
        };
        // The file is the app's own writing. Should it be damaged, there
        // is no task.
        Ok(xctsk::read(&String::from_utf8_lossy(&bytes)).ok())
    }
}

/// A task being flown in the cockpit.
#[derive(Debug)]
pub(crate) struct TaskFlown {
    flight: TaskFlight,
    /// The state at the last position, and that position.
    status: Option<(Fix, TaskStatus)>,
    events: VecDeque<TaskEventView>,
}

/// No more events than this wait to be said. A cockpit that is not
/// listening must not fill the memory.
const EVENTS_KEPT: usize = 16;

impl TaskFlown {
    /// Begins a task from its file's text. `now_unix_s` is a moment of
    /// the flight's day.
    pub(crate) fn begin(text: &str, now_unix_s: i64) -> Result<Self, CoreError> {
        let task = xctsk::read(text).map_err(|error| invalid(&error))?;
        Ok(Self {
            flight: TaskFlight::new(task, now_unix_s),
            status: None,
            events: VecDeque::new(),
        })
    }

    /// Takes in a position.
    pub(crate) fn feed(&mut self, time_unix_s: f64, position: GeoPoint) {
        let fix = Fix {
            time_unix_s: time_unix_s as i64,
            position,
        };
        for event in self.flight.update(fix) {
            let said = self.say(event);
            if self.events.len() == EVENTS_KEPT {
                self.events.pop_front();
            }
            self.events.push_back(said);
        }
        self.status = Some((fix, self.flight.status(fix)));
    }

    fn say(&self, event: TaskEvent) -> TaskEventView {
        match event {
            TaskEvent::Started { .. } => TaskEventView {
                kind: TaskEventKind::Started,
                words: "Start taken".to_owned(),
            },
            TaskEvent::Reached { index } => TaskEventView {
                kind: TaskEventKind::Reached,
                words: match self.flight.task().turnpoints.get(index) {
                    Some(turnpoint) if !turnpoint.name.is_empty() => {
                        format!("{} reached", turnpoint.name)
                    }
                    _ => format!("Turnpoint {} reached", index + 1),
                },
            },
            TaskEvent::SpeedSectionDone { .. } => TaskEventView {
                kind: TaskEventKind::SpeedSectionDone,
                words: "Speed section done".to_owned(),
            },
            TaskEvent::Goal => TaskEventView {
                kind: TaskEventKind::Goal,
                words: "Goal".to_owned(),
            },
        }
    }

    /// The next thing that happened and was not yet said.
    pub(crate) fn take_event(&mut self) -> Option<TaskEventView> {
        self.events.pop_front()
    }

    /// Moves on to the next turnpoint, or back to the one before, when
    /// the pilot says so.
    pub(crate) fn step(&mut self, forward: bool) {
        if forward {
            self.flight.skip();
        } else {
            self.flight.step_back();
        }
        if let Some((fix, _)) = self.status {
            self.status = Some((fix, self.flight.status(fix)));
        }
    }

    /// The point to fly to, as a place for the cockpit's navigation.
    pub(crate) fn aim(&self) -> Option<Target> {
        let next = self.status.as_ref()?.1.next.as_ref()?;
        Some(Target {
            name: next.name.clone(),
            position: next.aim,
            elevation_m: Some(next.altitude_m),
        })
    }

    /// The task as the cockpit shows it. `None` until there is a
    /// position.
    pub(crate) fn view(
        &self,
        now_unix_s: f64,
        altitude_m: Option<f64>,
        ground_speed_ms: Option<f64>,
    ) -> Option<TaskStatusView> {
        let (_, status) = self.status.as_ref()?;
        let task = self.flight.task();
        let now = now_unix_s as i64;
        let goal = task.turnpoints.last()?;
        let above_goal_m = altitude_m.map(|altitude| {
            altitude - goal.altitude_m - task.goal.finish_altitude_m.unwrap_or(0.0)
        });
        let end_reached = task
            .end_of_speed_index()
            .and_then(|end| status.reached_unix_s.get(end).copied().flatten());
        Some(TaskStatusView {
            phase: match status.phase {
                Phase::BeforeStart => TaskPhase::BeforeStart,
                Phase::OnTask => TaskPhase::OnTask,
                Phase::SpeedSectionDone => TaskPhase::SpeedSectionDone,
                Phase::InGoal => TaskPhase::InGoal,
            },
            next: status.next.as_ref().map(|next| TaskNextView {
                number: next.index as u32 + 1,
                name: next.name.clone(),
                role_in_words: role_in_words(task, next.index),
                aim: coordinate(next.aim),
                distance_m: next.distance_m,
                bearing_deg: next.bearing_deg,
                centre_distance_m: next.centre_distance_m,
                radius_m: next.radius_m,
                inside: next.inside,
            }),
            way: status.way.iter().copied().map(coordinate).collect(),
            to_goal_m: status.to_goal_m,
            to_end_of_speed_m: status.to_end_of_speed_m,
            speed_section_part: status.speed_section_part,
            gate: status.start.map(|start| {
                // The countdown runs with the clock, also between two
                // positions.
                let opens_in_s = start.gate_unix_s.map(|gate| gate - now);
                TaskGateView {
                    gate_unix_s: start.gate_unix_s,
                    opens_in_s,
                    next_gate_unix_s: start.next_gate_unix_s,
                    distance_m: start.distance_m,
                    speed_to_gate_ms: opens_in_s
                        .filter(|seconds| *seconds > 0)
                        .map(|seconds| start.distance_m / seconds as f64),
                    arrives_in_s: ground_speed_ms
                        .filter(|speed| *speed > 1.0)
                        .map(|speed| start.distance_m / speed),
                }
            }),
            clock_from_unix_s: status.clock_from_unix_s,
            clock_s: status
                .clock_from_unix_s
                .map(|from| end_reached.unwrap_or(now) - from),
            goal_unix_s: status.goal_unix_s,
            deadline_in_s: status.deadline_unix_s.map(|deadline| deadline - now),
            above_goal_m,
            glide_to_goal: above_goal_m
                .filter(|above| *above > 0.0 && status.phase != Phase::InGoal)
                .map(|above| status.to_goal_m / above),
            reached: status.reached_unix_s.iter().map(Option::is_some).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::testing::on_every_storage;

    /// A made-up task in the form a file has it: a launch, a start
    /// cylinder to leave, a turnpoint, and a goal line with the end of the
    /// speed section around it.
    const FILE_TEXT: &str = r#"{"taskType":"CLASSIC","version":1,"earthModel":"WGS84","turnpoints":[
        {"type":"TAKEOFF","radius":400,"waypoint":{"name":"LAUNCH","description":"Up the hill","lat":47.30,"lon":11.40,"altSmoothed":1900}},
        {"type":"SSS","radius":5000,"waypoint":{"name":"LAUNCH","lat":47.30,"lon":11.40,"altSmoothed":1900}},
        {"radius":2000,"waypoint":{"name":"WEST","lat":47.28,"lon":11.10,"altSmoothed":1200}},
        {"type":"ESS","radius":2000,"waypoint":{"name":"FIELD","lat":47.26,"lon":11.45,"altSmoothed":600}},
        {"radius":200,"waypoint":{"name":"FIELD","lat":47.26,"lon":11.45,"altSmoothed":600}}],
        "takeoff":{"timeOpen":"09:00:00Z","timeClose":"12:00:00Z"},
        "sss":{"type":"RACE","direction":"EXIT","timeGates":["11:00:00Z","11:20:00Z"]},
        "goal":{"type":"LINE","deadline":"16:00:00Z","finishAltitude":100}}"#;

    /// 4 July 2026, 00:00 UTC.
    const MIDNIGHT: i64 = 1_783_123_200;

    fn at(hour: i64, minute: i64) -> i64 {
        MIDNIGHT + hour * 3600 + minute * 60
    }

    #[test]
    fn a_task_is_shown_with_its_roles_lengths_and_outlines() {
        let task = read_task(FILE_TEXT.to_owned(), at(10, 0)).unwrap();
        assert!(!task.is_route);
        let roles: Vec<&str> = task
            .turnpoints
            .iter()
            .map(|turnpoint| turnpoint.role_in_words.as_str())
            .collect();
        assert_eq!(
            roles,
            [
                "Launch",
                "Start",
                "Turnpoint",
                "End of speed section",
                "Goal line"
            ]
        );
        assert_eq!(task.turnpoints[0].description, "Up the hill");
        assert_eq!(task.turnpoints[2].number, 3);

        // The legs add up to the task, and the speed section is the part
        // between the start's edge and the cylinder around the goal.
        let legs: f64 = task
            .turnpoints
            .iter()
            .map(|turnpoint| turnpoint.leg_m)
            .sum();
        assert!((legs - task.length_m).abs() < 1e-6);
        assert_eq!(task.turnpoints[0].leg_m, 0.0);
        assert!((task.turnpoints[1].leg_m - 5_000.0).abs() < 1.0);
        let speed = task.speed_section_m.unwrap();
        assert!((speed - (task.turnpoints[3].along_m - task.turnpoints[1].along_m)).abs() < 1e-6);
        // Out to within 2 km of WEST, 23 km away, and back past the
        // launch to the field: about 45 km.
        assert!(
            task.length_m > 44_000.0 && task.length_m < 47_000.0,
            "{}",
            task.length_m
        );
        assert_eq!(task.way.len(), 5);

        // The launch has no outline; a cylinder has a closed one; the
        // goal line has its half circle.
        assert!(task.turnpoints[0].outline.is_empty());
        let ring = &task.turnpoints[2].outline;
        assert_eq!(ring.len(), 73);
        assert_eq!(ring.first(), ring.last());
        let line = &task.turnpoints[4].outline;
        assert_eq!(line.len(), 20);
        assert_eq!(line.first(), line.last());

        // Times are those of the day asked about.
        let start = task.start.unwrap();
        assert!(start.is_race);
        assert_eq!(start.in_words, "Race to goal");
        assert_eq!(start.gates_unix_s, [at(11, 0), at(11, 20)]);
        assert_eq!(task.takeoff_open_unix_s, Some(at(9, 0)));
        assert_eq!(task.deadline_unix_s, Some(at(16, 0)));
        assert_eq!(task.finish_altitude_m, Some(100.0));
        assert_eq!(task.measured_on, "WGS84 ellipsoid");

        // The box holds the start cylinder, which reaches furthest north.
        assert!(task.north > 47.34 && task.north < 47.35, "{}", task.north);
        assert!(task.west < 11.08 && task.east > 11.46);
    }

    #[test]
    fn what_is_not_a_task_is_refused_in_a_sentence() {
        let refused = read_task("hello".to_owned(), 0).unwrap_err();
        assert_eq!(
            refused.to_string(),
            "This is not a task in the XCTSK format."
        );
        assert!(matches!(refused, CoreError::InvalidFile { .. }));
    }

    #[test]
    fn a_task_is_kept_handed_on_and_forgotten() {
        on_every_storage(|backing| {
            let core = Core::with_storage(backing.open()).unwrap();
            assert_eq!(core.kept_task(at(10, 0)).unwrap(), None);
            assert_eq!(core.kept_task_file().unwrap(), None);
            assert_eq!(core.kept_task_code(true).unwrap(), None);

            core.keep_task(FILE_TEXT.to_owned()).unwrap();
            let kept = core.kept_task(at(10, 0)).unwrap().unwrap();
            assert_eq!(kept, read_task(FILE_TEXT.to_owned(), at(10, 0)).unwrap());

            // It is still there when the app is started again.
            let again = Core::with_storage(backing.open()).unwrap();
            assert_eq!(again.kept_task(at(10, 0)).unwrap().as_ref(), Some(&kept));

            // As a file it is the same task again, and so it is as a code.
            let file = core.kept_task_file().unwrap().unwrap();
            assert_eq!(file.name, "venti-task.xctsk");
            assert_eq!(read_task(file.text, at(10, 0)).unwrap(), kept);
            let code = core.kept_task_code(true).unwrap().unwrap();
            assert!(code.starts_with("XCTSKZ:"));
            let plain = core.kept_task_code(false).unwrap().unwrap();
            assert!(plain.starts_with("XCTSK:"));
            // A code has no mark for the take-off, which changes nothing
            // a screen shows.
            assert_eq!(read_task(code, at(10, 0)).unwrap(), kept);

            // A task that came in as a code is kept as well.
            core.keep_task(plain).unwrap();
            assert_eq!(core.kept_task(at(10, 0)).unwrap().unwrap(), kept);

            // What is not a task replaces nothing.
            assert!(core.keep_task("nothing".to_owned()).is_err());
            assert!(core.kept_task(at(10, 0)).unwrap().is_some());

            core.forget_task().unwrap();
            assert_eq!(core.kept_task(at(10, 0)).unwrap(), None);
            core.forget_task().unwrap();
        });
    }

    #[test]
    fn a_task_is_followed_through_a_flight_for_the_cockpit() {
        let mut flown = TaskFlown::begin(FILE_TEXT, at(9, 30)).unwrap();
        assert!(flown.view(at(10, 50) as f64, Some(2_000.0), None).is_none());
        assert!(flown.aim().is_none());

        let earth = Earth::Wgs84;
        let launch = GeoPoint::new(47.30, 11.40).unwrap();
        // Waiting two kilometres west of the launch, inside the start.
        flown.feed(at(10, 50) as f64, earth.destination(launch, 270.0, 2_000.0));
        let waiting = flown
            .view(at(10, 50) as f64 + 30.0, Some(2_400.0), Some(10.0))
            .unwrap();
        assert_eq!(waiting.phase, TaskPhase::BeforeStart);
        let gate = waiting.gate.clone().unwrap();
        assert_eq!(gate.gate_unix_s, Some(at(11, 0)));
        // Half a minute after the position came in, the countdown has
        // moved on.
        assert_eq!(gate.opens_in_s, Some(570));
        assert!(
            (gate.distance_m - 3_000.0).abs() < 50.0,
            "{}",
            gate.distance_m
        );
        assert!((gate.speed_to_gate_ms.unwrap() - gate.distance_m / 570.0).abs() < 1e-9);
        assert!((gate.arrives_in_s.unwrap() - gate.distance_m / 10.0).abs() < 1e-9);
        let next = waiting.next.clone().unwrap();
        assert_eq!((next.number, next.role_in_words.as_str()), (2, "Start"));
        assert!(next.inside);
        // 2,400 m, over a field at 600 m to be crossed 100 m up.
        assert_eq!(waiting.above_goal_m, Some(1_700.0));
        assert!((waiting.glide_to_goal.unwrap() - waiting.to_goal_m / 1_700.0).abs() < 1e-9);
        assert_eq!(waiting.clock_s, None);
        assert_eq!(waiting.deadline_in_s, Some(at(16, 0) - at(10, 50) - 30));
        assert_eq!(waiting.reached, [false; 5]);
        // The cockpit navigates to the edge of the start, on the way west.
        let aim = flown.aim().unwrap();
        assert_eq!(aim.name, "LAUNCH");
        assert_eq!(aim.elevation_m, Some(1_900.0));
        assert!((earth.distance_m(launch, aim.position) - 5_000.0).abs() < 0.01);

        // Out of the start after the second gate: the clock runs from it.
        flown.feed(at(11, 25) as f64, earth.destination(launch, 270.0, 4_990.0));
        assert!(flown.take_event().is_none());
        flown.feed(
            at(11, 25) as f64 + 1.0,
            earth.destination(launch, 270.0, 5_010.0),
        );
        let said = flown.take_event().unwrap();
        assert_eq!(
            (said.kind, said.words.as_str()),
            (TaskEventKind::Started, "Start taken")
        );
        assert!(flown.take_event().is_none());
        let racing = flown
            .view(at(11, 30) as f64, Some(2_400.0), Some(10.0))
            .unwrap();
        assert_eq!(racing.phase, TaskPhase::OnTask);
        assert_eq!(racing.clock_from_unix_s, Some(at(11, 20)));
        assert_eq!(racing.clock_s, Some(600));
        assert_eq!(racing.next.unwrap().name, "WEST");
        assert_eq!(racing.reached, [false, true, false, false, false]);
        assert_eq!(flown.aim().unwrap().name, "WEST");

        // The pilot steps on by hand, and back.
        flown.step(true);
        assert_eq!(flown.aim().unwrap().name, "FIELD");
        flown.step(false);
        assert_eq!(flown.aim().unwrap().name, "WEST");

        // Into the cylinder of WEST.
        let west = GeoPoint::new(47.28, 11.10).unwrap();
        flown.feed(at(12, 0) as f64, earth.destination(west, 90.0, 2_010.0));
        flown.feed(
            at(12, 0) as f64 + 1.0,
            earth.destination(west, 90.0, 1_990.0),
        );
        assert_eq!(flown.take_event().unwrap().words, "WEST reached");
    }
}
